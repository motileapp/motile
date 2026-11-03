//! The gateway's SQLite on its volume: the accounts with their tokens sealed, the settings, the limits
//! read, the request log with the requests captured in full, and the tokens spent.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use motile_protocol::gateway_wire::{
    AccountStatus, CapturedRequest, LogEntry, McpCall, McpCallDetail, McpStatus, UsageRow,
};
use motile_protocol::wire::{Agent, LimitWindow};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::accounts::{Account, Tokens};
use crate::mcp::{Login, McpEntry};
use crate::seal::Seal;
use crate::usage::Spent;

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_mcp_servers.sql"),
    include_str!("../migrations/0003_tool_permissions.sql"),
];
const READINGS_KEPT: f64 = 48.0 * 3600.0;
const LOG_KEPT: f64 = 30.0 * 86400.0;
/// About 200 MB of entries.
const LOG_ROWS: i64 = 400_000;
const CAPTURES_KEPT: f64 = 24.0 * 3600.0;
const CAPTURE_BYTES: i64 = 1024 * 1024 * 1024;

pub struct Store {
    connection: Mutex<Connection>,
}

/// A finished request, as the log keeps it.
pub struct Record {
    pub entry: LogEntry,
    pub agent: Agent,
    pub finished_at: f64,
    pub cost: f64,
    pub spent: Spent,
}

pub struct Captured {
    pub request_headers: Vec<(String, String)>,
    pub request_body: Vec<u8>,
    pub response_headers: Vec<(String, String)>,
    pub response_body: Vec<u8>,
}

/// An MCP server's tool call, as the log keeps it.
pub struct CallRecord {
    pub at: f64,
    pub server: Option<String>,
    pub thread: Option<String>,
    pub mcp_server: String,
    pub tool: String,
    pub latency_ms: u64,
    pub error: Option<String>,
    pub arguments: String,
    pub result: String,
}

/// A request of the log, as much as the router needs to pick its threads up again after a restart.
pub struct Replayed {
    pub server: String,
    pub thread: String,
    pub agent: Agent,
    pub account: Option<String>,
    pub model: Option<String>,
    pub at: f64,
    pub finished_at: f64,
    pub cost: f64,
}

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let connection = Connection::open(path)?;
        let applied: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if applied == 0 {
            connection.pragma_update(None, "auto_vacuum", "INCREMENTAL")?;
        }
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        for (index, migration) in MIGRATIONS.iter().enumerate().skip(applied as usize) {
            connection.execute_batch(&format!("BEGIN; {migration} PRAGMA user_version = {}; COMMIT;", index + 1))?;
        }
        Ok(Self { connection: Mutex::new(connection) })
    }

    fn connection(&self) -> MutexGuard<'_, Connection> {
        self.connection.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Every account. One whose tokens can't be opened has to be signed in again.
    pub fn accounts(&self, seal: &Seal) -> anyhow::Result<Vec<Account>> {
        let connection = self.connection();
        let mut statement = connection.prepare(
            "SELECT id, agent, name, email, plan, priority, enabled, status, tokens, windows, read_at, reset_credits,
                    created_at FROM accounts ORDER BY created_at",
        )?;
        let rows = statement.query_map([], |row| {
            let sealed: Vec<u8> = row.get(8)?;
            Ok((account_row(row)?, sealed))
        })?;
        let mut accounts = Vec::new();
        for row in rows {
            let (mut account, sealed) = row?;
            let tokens = seal.open(&sealed).ok().and_then(|plain| serde_json::from_slice::<Tokens>(&plain).ok());
            match tokens {
                Some(tokens) => account.tokens = tokens,
                None => account.status = AccountStatus::SignInAgain,
            }
            accounts.push(account);
        }
        Ok(accounts)
    }

    pub fn save_account(&self, account: &Account, seal: &Seal) -> anyhow::Result<()> {
        let tokens = seal.seal(&serde_json::to_vec(&account.tokens)?);
        self.connection().execute(
            "INSERT INTO accounts (id, agent, name, email, plan, priority, enabled, status, tokens, windows, read_at,
                                 reset_credits, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT (id) DO UPDATE SET agent = ?2, name = ?3, email = ?4, plan = ?5, priority = ?6, enabled = ?7,
                status = ?8, tokens = ?9, windows = ?10, read_at = ?11, reset_credits = ?12",
            params![
                account.id,
                agent_name(account.agent),
                account.name,
                account.email,
                account.plan,
                account.priority,
                account.enabled,
                serde_json::to_string(&account.status)?,
                tokens,
                serde_json::to_string(&account.windows)?,
                account.read_at,
                account.reset_credits,
                account.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_account(&self, id: &str) -> anyhow::Result<()> {
        let connection = self.connection();
        connection.execute("DELETE FROM accounts WHERE id = ?1", [id])?;
        connection.execute("DELETE FROM readings WHERE account = ?1", [id])?;
        Ok(())
    }

    pub fn setting<T: DeserializeOwned>(&self, name: &str) -> Option<T> {
        let connection = self.connection();
        let text: Option<String> = connection
            .query_row("SELECT value FROM settings WHERE name = ?1", [name], |row| row.get(0))
            .optional()
            .ok()
            .flatten();
        serde_json::from_str(&text?).ok()
    }

    pub fn set_setting<T: Serialize>(&self, name: &str, value: &T) -> anyhow::Result<()> {
        self.connection().execute(
            "INSERT INTO settings (name, value) VALUES (?1, ?2) ON CONFLICT (name) DO UPDATE SET value = ?2",
            params![name, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    pub fn add_reading(&self, account: &str, at: f64, windows: &[LimitWindow]) -> anyhow::Result<()> {
        self.connection().execute(
            "INSERT INTO readings (account, at, windows) VALUES (?1, ?2, ?3)",
            params![account, at, serde_json::to_string(windows)?],
        )?;
        Ok(())
    }

    pub fn readings(&self, account: &str, since: f64) -> anyhow::Result<Vec<(f64, Vec<LimitWindow>)>> {
        let connection = self.connection();
        let mut statement =
            connection.prepare("SELECT at, windows FROM readings WHERE account = ?1 AND at >= ?2 ORDER BY at")?;
        let rows = statement.query_map(params![account, since], |row| Ok((row.get(0)?, row.get::<_, String>(1)?)))?;
        let mut readings = Vec::new();
        for row in rows {
            let (at, windows) = row?;
            readings.push((at, serde_json::from_str(&windows).unwrap_or_default()));
        }
        Ok(readings)
    }

    /// What each request on the account cost, by when it finished.
    pub fn costs(&self, account: &str, since: f64) -> anyhow::Result<Vec<(f64, f64)>> {
        let connection = self.connection();
        let mut statement = connection.prepare(
            "SELECT finished_at, cost FROM log WHERE account = ?1 AND finished_at >= ?2 AND cost > 0 ORDER BY finished_at",
        )?;
        let rows = statement.query_map(params![account, since], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn record(&self, record: &Record, captured: Option<&Captured>) -> anyhow::Result<i64> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let entry = &record.entry;
        transaction.execute(
            "INSERT INTO log (at, finished_at, server, thread, agent, account, model, path, status, latency_ms,
                              input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cost, retries,
                              request_bytes, response_bytes, error, captured)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                entry.at,
                record.finished_at,
                entry.server,
                entry.thread,
                agent_name(record.agent),
                entry.account,
                entry.model,
                entry.path,
                entry.status,
                entry.latency_ms as i64,
                entry.input_tokens as i64,
                entry.output_tokens as i64,
                entry.cache_read_tokens as i64,
                entry.cache_write_tokens as i64,
                record.cost,
                entry.retries,
                entry.request_bytes as i64,
                entry.response_bytes as i64,
                entry.error,
                captured.is_some(),
            ],
        )?;
        let id = transaction.last_insert_rowid();
        if let Some(captured) = captured {
            let size = captured.request_body.len() + captured.response_body.len();
            transaction.execute(
                "INSERT INTO captures (entry, at, size, request_headers, request_body, response_headers, response_body)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    entry.at,
                    size as i64,
                    serde_json::to_string(&captured.request_headers)?,
                    captured.request_body,
                    serde_json::to_string(&captured.response_headers)?,
                    captured.response_body,
                ],
            )?;
        }
        if let (Some(account), Some(model)) = (&entry.account, &record.spent.model) {
            let spent = &record.spent;
            transaction.execute(
                "INSERT INTO usage (hour, account, model, requests, input_tokens, output_tokens, cache_read_tokens,
                                    cache_write_tokens, cost)
                 VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (hour, account, model) DO UPDATE SET requests = requests + 1,
                    input_tokens = input_tokens + ?4, output_tokens = output_tokens + ?5,
                    cache_read_tokens = cache_read_tokens + ?6, cache_write_tokens = cache_write_tokens + ?7,
                    cost = cost + ?8",
                params![
                    (entry.at / 3600.0).floor() * 3600.0,
                    account,
                    model,
                    spent.input as i64,
                    spent.output as i64,
                    spent.cache_read as i64,
                    spent.cache_write as i64,
                    record.cost,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(id)
    }

    pub fn log(&self, before: Option<i64>, errors_only: bool, limit: u32) -> anyhow::Result<Vec<LogEntry>> {
        let connection = self.connection();
        let mut statement = connection.prepare(&format!(
            "SELECT {LOG_COLUMNS} FROM log WHERE id < ?1 {} ORDER BY id DESC LIMIT ?2",
            if errors_only { "AND error IS NOT NULL" } else { "" }
        ))?;
        let rows = statement.query_map(params![before.unwrap_or(i64::MAX), limit.min(1000)], log_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn log_head(&self) -> anyhow::Result<Option<i64>> {
        Ok(self.connection().query_row("SELECT MAX(id) FROM log", [], |row| row.get(0))?)
    }

    pub fn captured(&self, id: i64) -> anyhow::Result<Option<CapturedRequest>> {
        let connection = self.connection();
        let found = connection
            .query_row(
                &format!(
                    "SELECT {LOG_COLUMNS}, captures.request_headers, captures.request_body, captures.response_headers,
                            captures.response_body
                     FROM log JOIN captures ON captures.entry = log.id WHERE log.id = ?1"
                ),
                [id],
                |row| {
                    let text = |index: usize| -> rusqlite::Result<String> {
                        Ok(String::from_utf8_lossy(&row.get::<_, Vec<u8>>(index)?).into_owned())
                    };
                    let headers = |index: usize| -> rusqlite::Result<Vec<(String, String)>> {
                        Ok(serde_json::from_str(&row.get::<_, String>(index)?).unwrap_or_default())
                    };
                    Ok(CapturedRequest {
                        entry: log_row(row)?,
                        request_headers: headers(18)?,
                        request_body: text(19)?,
                        response_headers: headers(20)?,
                        response_body: text(21)?,
                    })
                },
            )
            .optional()?;
        Ok(found)
    }

    pub fn usage(&self, since: f64) -> anyhow::Result<Vec<UsageRow>> {
        let connection = self.connection();
        let mut statement = connection.prepare(
            "SELECT account, model, SUM(requests), SUM(input_tokens), SUM(output_tokens), SUM(cache_read_tokens),
                    SUM(cache_write_tokens)
             FROM usage WHERE hour >= ?1 GROUP BY account, model ORDER BY account, model",
        )?;
        let rows = statement.query_map([(since / 3600.0).floor() * 3600.0], |row| {
            Ok(UsageRow {
                account: row.get(0)?,
                model: row.get(1)?,
                requests: row.get::<_, i64>(2)? as u64,
                input_tokens: row.get::<_, i64>(3)? as u64,
                output_tokens: row.get::<_, i64>(4)? as u64,
                cache_read_tokens: row.get::<_, i64>(5)? as u64,
                cache_write_tokens: row.get::<_, i64>(6)? as u64,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn replay(&self, since: f64) -> anyhow::Result<Vec<Replayed>> {
        let connection = self.connection();
        let mut statement = connection.prepare(
            "SELECT server, thread, agent, account, model, at, finished_at, cost FROM log WHERE at >= ?1 ORDER BY at",
        )?;
        let rows = statement.query_map([since], |row| {
            Ok(Replayed {
                server: row.get(0)?,
                thread: row.get(1)?,
                agent: parse_agent(&row.get::<_, String>(2)?),
                account: row.get(3)?,
                model: row.get(4)?,
                at: row.get(5)?,
                finished_at: row.get(6)?,
                cost: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Drops what is kept longer, or takes more room, than it may.
    pub fn prune(&self, now: f64) -> anyhow::Result<()> {
        let connection = self.connection();
        connection.execute("DELETE FROM readings WHERE at < ?1", [now - READINGS_KEPT])?;
        connection.execute("DELETE FROM log WHERE at < ?1", [now - LOG_KEPT])?;
        connection.execute(
            "DELETE FROM log WHERE id <= (SELECT id FROM log ORDER BY id DESC LIMIT 1 OFFSET ?1)",
            [LOG_ROWS],
        )?;
        connection.execute("DELETE FROM calls WHERE at < ?1", [now - LOG_KEPT])?;
        connection.execute("DELETE FROM captures WHERE at < ?1", [now - CAPTURES_KEPT])?;
        connection.execute(
            "DELETE FROM captures WHERE entry NOT IN (SELECT id FROM log)
                OR entry IN (SELECT entry FROM (SELECT entry, SUM(size) OVER (ORDER BY entry DESC) AS kept
                                                 FROM captures) WHERE kept > ?1)",
            [CAPTURE_BYTES],
        )?;
        connection
            .execute("UPDATE log SET captured = 0 WHERE captured = 1 AND id NOT IN (SELECT entry FROM captures)", [])?;
        connection.execute_batch("PRAGMA incremental_vacuum;")?;
        Ok(())
    }
}

impl Store {
    /// Every MCP server. One whose login can't be opened has to be signed in again.
    pub fn mcp_servers(&self, seal: &Seal) -> anyhow::Result<Vec<McpEntry>> {
        let connection = self.connection();
        let mut statement = connection.prepare(
            "SELECT id, slug, name, url, enabled, login, status, tools, permissions, icon, created_at FROM mcp_servers
             ORDER BY created_at",
        )?;
        let rows = statement.query_map([], |row| {
            let sealed: Vec<u8> = row.get(5)?;
            let status: String = row.get(6)?;
            let tools: String = row.get(7)?;
            let permissions: String = row.get(8)?;
            let mcp_server = McpEntry {
                id: row.get(0)?,
                slug: row.get(1)?,
                name: row.get(2)?,
                url: row.get(3)?,
                enabled: row.get(4)?,
                login: Login::None,
                status: serde_json::from_str(&status).unwrap_or(McpStatus::Ready),
                tools: serde_json::from_str(&tools).unwrap_or_default(),
                permissions: serde_json::from_str(&permissions).unwrap_or_default(),
                icon: row.get(9)?,
                created_at: row.get(10)?,
            };
            Ok((mcp_server, sealed))
        })?;
        let mut mcp_servers = Vec::new();
        for row in rows {
            let (mut mcp_server, sealed) = row?;
            match seal.open(&sealed).ok().and_then(|plain| serde_json::from_slice::<Login>(&plain).ok()) {
                Some(login) => mcp_server.login = login,
                None => mcp_server.status = McpStatus::SignInAgain,
            }
            mcp_servers.push(mcp_server);
        }
        Ok(mcp_servers)
    }

    pub fn save_mcp_server(&self, mcp_server: &McpEntry, seal: &Seal) -> anyhow::Result<()> {
        let login = seal.seal(&serde_json::to_vec(&mcp_server.login)?);
        self.connection().execute(
            "INSERT INTO mcp_servers (id, slug, name, url, enabled, login, status, tools, permissions, icon, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT (id) DO UPDATE SET slug = ?2, name = ?3, url = ?4, enabled = ?5, login = ?6, status = ?7,
                tools = ?8, permissions = ?9, icon = ?10",
            params![
                mcp_server.id,
                mcp_server.slug,
                mcp_server.name,
                mcp_server.url,
                mcp_server.enabled,
                login,
                serde_json::to_string(&mcp_server.status)?,
                serde_json::to_string(&mcp_server.tools)?,
                serde_json::to_string(&mcp_server.permissions)?,
                mcp_server.icon,
                mcp_server.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_mcp_server(&self, id: &str) -> anyhow::Result<()> {
        self.connection().execute("DELETE FROM mcp_servers WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn add_call(&self, call: &CallRecord) -> anyhow::Result<i64> {
        let connection = self.connection();
        connection.execute(
            "INSERT INTO calls (at, server, thread, mcp_server, tool, latency_ms, error, arguments, result)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                call.at,
                call.server,
                call.thread,
                call.mcp_server,
                call.tool,
                call.latency_ms as i64,
                call.error,
                call.arguments,
                call.result,
            ],
        )?;
        Ok(connection.last_insert_rowid())
    }

    /// The calls before `before`, newest first.
    pub fn calls(&self, before: Option<i64>, errors_only: bool, limit: u32) -> anyhow::Result<Vec<McpCall>> {
        let connection = self.connection();
        let errors = if errors_only { "AND error IS NOT NULL" } else { "" };
        let mut statement = connection
            .prepare(&format!("SELECT {CALL_COLUMNS} FROM calls WHERE id < ?1 {errors} ORDER BY id DESC LIMIT ?2"))?;
        let rows = statement.query_map(params![before.unwrap_or(i64::MAX), limit], call_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn call(&self, id: i64) -> anyhow::Result<Option<McpCallDetail>> {
        let connection = self.connection();
        let found = connection
            .query_row(&format!("SELECT {CALL_COLUMNS}, arguments, result FROM calls WHERE id = ?1"), [id], |row| {
                Ok(McpCallDetail { call: call_row(row)?, arguments: row.get(8)?, result: row.get(9)? })
            })
            .optional()?;
        Ok(found)
    }

    pub fn calls_head(&self) -> anyhow::Result<Option<i64>> {
        Ok(self.connection().query_row("SELECT MAX(id) FROM calls", [], |row| row.get(0))?)
    }
}

const CALL_COLUMNS: &str = "id, at, server, thread, mcp_server, tool, latency_ms, error";

fn call_row(row: &Row) -> rusqlite::Result<McpCall> {
    Ok(McpCall {
        id: row.get(0)?,
        at: row.get(1)?,
        server: row.get(2)?,
        thread: row.get(3)?,
        mcp_server: row.get(4)?,
        tool: row.get(5)?,
        latency_ms: row.get::<_, i64>(6)? as u64,
        error: row.get(7)?,
    })
}

const LOG_COLUMNS: &str = "log.id, log.at, log.server, log.thread, log.account, log.model, log.path, log.status,
    log.latency_ms, log.input_tokens, log.output_tokens, log.cache_read_tokens, log.cache_write_tokens, log.retries,
    log.request_bytes, log.response_bytes, log.error, log.captured";

fn log_row(row: &Row) -> rusqlite::Result<LogEntry> {
    Ok(LogEntry {
        id: row.get(0)?,
        at: row.get(1)?,
        server: row.get(2)?,
        thread: row.get(3)?,
        account: row.get(4)?,
        model: row.get(5)?,
        path: row.get(6)?,
        status: row.get(7)?,
        latency_ms: row.get::<_, i64>(8)? as u64,
        input_tokens: row.get::<_, i64>(9)? as u64,
        output_tokens: row.get::<_, i64>(10)? as u64,
        cache_read_tokens: row.get::<_, i64>(11)? as u64,
        cache_write_tokens: row.get::<_, i64>(12)? as u64,
        retries: row.get(13)?,
        request_bytes: row.get::<_, i64>(14)? as u64,
        response_bytes: row.get::<_, i64>(15)? as u64,
        error: row.get(16)?,
        captured: row.get(17)?,
    })
}

fn account_row(row: &Row) -> rusqlite::Result<Account> {
    let json = |index: usize| -> rusqlite::Result<String> { row.get(index) };
    Ok(Account {
        id: row.get(0)?,
        agent: parse_agent(&row.get::<_, String>(1)?),
        name: row.get(2)?,
        email: row.get(3)?,
        plan: row.get(4)?,
        priority: row.get(5)?,
        enabled: row.get(6)?,
        status: serde_json::from_str(&json(7)?).unwrap_or(AccountStatus::Ready),
        tokens: Tokens::default(),
        windows: serde_json::from_str(&json(9)?).unwrap_or_default(),
        read_at: row.get(10)?,
        reset_credits: row.get(11)?,
        created_at: row.get(12)?,
    })
}

pub fn agent_name(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude",
        Agent::Codex => "codex",
    }
}

fn parse_agent(name: &str) -> Agent {
    match name {
        "codex" => Agent::Codex,
        _ => Agent::Claude,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(at: f64, error: Option<&str>) -> Record {
        Record {
            entry: LogEntry {
                id: 0,
                at,
                server: "s".into(),
                thread: "t".into(),
                account: Some("l".into()),
                model: Some("m".into()),
                path: "/v1/messages".into(),
                status: if error.is_some() { 500 } else { 200 },
                latency_ms: 10,
                input_tokens: 5,
                output_tokens: 7,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                retries: 0,
                request_bytes: 1,
                response_bytes: 2,
                error: error.map(String::from),
                captured: false,
            },
            agent: Agent::Claude,
            finished_at: at + 1.0,
            cost: 0.5,
            spent: Spent { model: Some("m".into()), input: 5, output: 7, ..Spent::default() },
        }
    }

    #[test]
    fn the_log_pages_back_from_the_newest_and_keeps_errors_apart() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("gateway.sqlite")).unwrap();
        let first = store.record(&entry(10.0, None), None).unwrap();
        let failed = store.record(&entry(20.0, Some("overloaded")), None).unwrap();
        let last = store.record(&entry(30.0, None), None).unwrap();

        let ids = |entries: Vec<LogEntry>| entries.into_iter().map(|entry| entry.id).collect::<Vec<_>>();
        assert_eq!(ids(store.log(None, false, 10).unwrap()), [last, failed, first]);
        assert_eq!(ids(store.log(Some(last), false, 1).unwrap()), [failed]);
        assert_eq!(ids(store.log(None, true, 10).unwrap()), [failed]);
        let usage = store.usage(0.0).unwrap();
        assert_eq!((usage[0].requests, usage[0].input_tokens, usage[0].output_tokens), (3, 15, 21));
    }

    #[test]
    fn captures_older_than_a_day_are_dropped_with_their_mark() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("gateway.sqlite")).unwrap();
        let captured = Captured {
            request_headers: vec![("a".into(), "b".into())],
            request_body: b"{}".to_vec(),
            response_headers: vec![],
            response_body: b"ok".to_vec(),
        };
        let id = store.record(&entry(0.0, None), Some(&captured)).unwrap();
        assert_eq!(store.captured(id).unwrap().unwrap().response_body, "ok");

        store.prune(CAPTURES_KEPT + 10.0).unwrap();
        assert!(store.captured(id).unwrap().is_none());
        assert!(!store.log(None, false, 1).unwrap()[0].captured);
    }
}
