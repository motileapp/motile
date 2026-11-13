//! The server's change log and its copies of the account's other servers' logs, which the clients
//! carry between servers. A batch of a log holds every row written after one change and up to
//! another; a copy keeps each row with the change its origin gave it, so a copy can be passed on.

use anyhow::{Context, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use motile_protocol::wire::{
    Agent, ChangeBatch, ChangeSet, Item, Logged, LoggedItem, OriginWatermark, Project, Thread, Tombstone,
    TombstoneKind, UsageRecord, Watermark,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params, params_from_iter};

use crate::activity::{
    ACTION_COLUMNS, CLIENT_COLUMNS, ENGAGED_COLUMNS, TURN_COLUMNS, action_record, action_values, client_record,
    client_values, engaged_record, engaged_values, placeholders, turn_record, turn_values,
};

use crate::hub::file_name;
use crate::store::{Store, as_text, from_text, threads_where, tokens_at};

/// How many rows of each kind a batch holds at most.
const BATCH_ROWS: usize = 2000;
/// How much JSON a batch holds, about, before it is compressed.
const BATCH_BYTES: usize = 4 * 1024 * 1024;
const COMPRESSION_LEVEL: i32 = 3;
const TRANSCRIPT_PIECE: usize = 200;

/// The rows of one kind after a change, and whether there were more than a batch holds.
struct Rows<T> {
    rows: Vec<(u64, T)>,
    full: bool,
}

impl<T> Rows<T> {
    fn last(&self) -> Option<u64> {
        self.full.then(|| self.rows.last().map(|(change, _)| *change)).flatten()
    }

    fn up_to(self, cut: u64) -> Vec<(u64, T)> {
        self.rows.into_iter().filter(|(change, _)| *change <= cut).collect()
    }
}

/// Reads rows of `(change, payload)` until a batch is full.
fn read_rows<T, P: rusqlite::Params>(
    connection: &Connection,
    sql: &str,
    parameters: P,
    mut decode: impl FnMut(&rusqlite::Row) -> rusqlite::Result<Option<(T, usize)>>,
) -> rusqlite::Result<Rows<T>> {
    let mut statement = connection.prepare(sql)?;
    let mut found = statement.query(parameters)?;
    let (mut rows, mut bytes) = (Vec::new(), 0);
    while let Some(row) = found.next()? {
        if rows.len() >= BATCH_ROWS || bytes >= BATCH_BYTES {
            return Ok(Rows { rows, full: true });
        }
        let change = row.get::<_, i64>(0)? as u64;
        let Some((value, size)) = decode(row)? else { continue };
        bytes += size;
        rows.push((change, value));
    }
    Ok(Rows { rows, full: false })
}

fn json<T: serde::de::DeserializeOwned>(row: &rusqlite::Row, index: usize) -> rusqlite::Result<Option<(T, usize)>> {
    let payload: String = row.get(index)?;
    Ok(serde_json::from_str(&payload).ok().map(|value| (value, payload.len())))
}

/// A project as a copy shows it: no git and no icon, which only its own server can read, but the
/// repository it was cloned from, so that another server's clone of it is known as the same.
fn copied_project(id: String, path: String, created_at: f64, setup: Option<String>) -> Project {
    Project {
        origin: crate::git::origin_of(&path),
        id,
        name: file_name(&path).to_string(),
        path,
        branch: None,
        git: None,
        icon: None,
        worktrees: Vec::new(),
        setup,
        no_project: false,
        created_at,
    }
}

fn kind_text(kind: TombstoneKind) -> &'static str {
    match kind {
        TombstoneKind::Project => "project",
        TombstoneKind::Thread => "thread",
    }
}

fn kind_of(text: &str) -> TombstoneKind {
    match text {
        "project" => TombstoneKind::Project,
        _ => TombstoneKind::Thread,
    }
}

/// Where the batch ends: before the first row a full kind left out, or at the end of the log.
fn cut(end: u64, lasts: &[Option<u64>]) -> (u64, bool) {
    match lasts.iter().flatten().min() {
        Some(last) => (*last, true),
        None => (end, false),
    }
}

/// A part of a log, before it is compressed into a `ChangeBatch`.
pub struct LogPart {
    pub log_id: String,
    pub after: u64,
    pub up_to: u64,
    pub more: bool,
    pub set: ChangeSet,
}

impl LogPart {
    pub fn encode(&self) -> anyhow::Result<ChangeBatch> {
        Ok(ChangeBatch {
            log_id: self.log_id.clone(),
            after: self.after,
            up_to: self.up_to,
            more: self.more,
            data: STANDARD.encode(self.compress()?),
        })
    }

    /// The set as zstd-compressed JSON.
    pub fn compress(&self) -> anyhow::Result<Vec<u8>> {
        Ok(zstd::encode_all(serde_json::to_vec(&self.set)?.as_slice(), COMPRESSION_LEVEL)?)
    }

    pub fn decompress(log_id: &str, after: u64, up_to: u64, compressed: &[u8]) -> anyhow::Result<Self> {
        let json = zstd::decode_all(compressed).context("The batch can't be decompressed.")?;
        let set = serde_json::from_slice(&json)?;
        Ok(Self { log_id: log_id.to_string(), after, up_to, more: false, set })
    }

    pub fn decode(batch: &ChangeBatch) -> anyhow::Result<Self> {
        let compressed = STANDARD.decode(&batch.data).context("The batch isn't base64.")?;
        Self::decompress(&batch.log_id, batch.after, batch.up_to, &compressed)
    }
}

fn own_watermark(connection: &Connection) -> rusqlite::Result<Watermark> {
    connection.query_row("SELECT log_id, change FROM sync", [], |row| {
        Ok(Watermark { log_id: row.get(0)?, change: row.get::<_, i64>(1)? as u64 })
    })
}

fn origin_watermark(connection: &Connection, origin: &str) -> rusqlite::Result<Option<Watermark>> {
    connection
        .query_row("SELECT log_id, change FROM origins WHERE id = ?1", [origin], |row| {
            Ok(Watermark { log_id: row.get(0)?, change: row.get::<_, i64>(1)? as u64 })
        })
        .optional()
}

fn drop_copies(transaction: &Transaction, origin: &str) -> rusqlite::Result<()> {
    let tables = [
        "copied_projects",
        "copied_threads",
        "copied_items",
        "copied_usage",
        "copied_clients",
        "copied_turns",
        "copied_actions",
        "copied_engaged",
        "copied_tombstones",
        "origins",
    ];
    for table in tables {
        let column = if table == "origins" { "id" } else { "origin" };
        transaction.execute(&format!("DELETE FROM {table} WHERE {column} = ?1"), [origin])?;
    }
    Ok(())
}

impl Store {
    pub fn watermark(&self) -> rusqlite::Result<Watermark> {
        own_watermark(&self.connection())
    }

    pub fn origin_watermark(&self, origin: &str) -> rusqlite::Result<Option<Watermark>> {
        origin_watermark(&self.connection(), origin)
    }

    pub fn origin_watermarks(&self) -> rusqlite::Result<Vec<OriginWatermark>> {
        let connection = self.connection();
        let mut statement = connection.prepare("SELECT id, log_id, change FROM origins ORDER BY id")?;
        let origins = statement.query_map([], |row| {
            Ok(OriginWatermark {
                origin: row.get(0)?,
                watermark: Watermark { log_id: row.get(1)?, change: row.get::<_, i64>(2)? as u64 },
            })
        })?;
        origins.collect()
    }

    /// What changed in this server's own log after `after`.
    pub fn changes(&self, after: u64) -> anyhow::Result<LogPart> {
        let connection = self.connection();
        let own = own_watermark(&connection)?;
        let after = after.min(own.change);
        let at = after as i64;
        let limit = (BATCH_ROWS + 1) as i64;
        let projects = read_rows(
            &connection,
            "SELECT change, id, path, created_at, setup FROM projects WHERE change > ?1 ORDER BY change LIMIT ?2",
            params![at, limit],
            |row| Ok(Some((copied_project(row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?), 200))),
        )?;
        let threads = {
            let found = threads_where(&connection, "t.change > ?1", "ORDER BY t.change LIMIT ?2", params![at, limit])?;
            let full = found.len() > BATCH_ROWS;
            let rows = found.into_iter().take(BATCH_ROWS).map(|(stored, change)| (change, stored.thread)).collect();
            Rows { rows, full }
        };
        let items = read_rows(
            &connection,
            "SELECT change, thread_id, payload FROM items WHERE change > ?1 ORDER BY change LIMIT ?2",
            params![at, limit],
            |row| {
                let thread_id: String = row.get(1)?;
                Ok(json::<Item>(row, 2)?.map(|(item, size)| ((thread_id, item), size)))
            },
        )?;
        let usage = read_rows(
            &connection,
            "SELECT change, at, thread_id, project_id, agent, agent_account, model, input, cache_read, cache_write,
                    output, cost_usd, purpose
             FROM usage WHERE change > ?1 ORDER BY change LIMIT ?2",
            params![at, limit],
            |row| Ok(Some((usage_record(row, 1)?, 200))),
        )?;
        let tombstones = read_rows(
            &connection,
            "SELECT change, kind, id FROM tombstones WHERE change > ?1 ORDER BY change LIMIT ?2",
            params![at, limit],
            |row| Ok(Some(((kind_of(&row.get::<_, String>(1)?), row.get::<_, String>(2)?), 100))),
        )?;
        let own_rows = |table: &str, columns: &str| {
            format!("SELECT change, {columns} FROM {table} WHERE change > ?1 ORDER BY change LIMIT ?2")
        };
        let clients = read_rows(&connection, &own_rows("clients", CLIENT_COLUMNS), params![at, limit], |row| {
            Ok(Some((client_record(row, 1)?, 150)))
        })?;
        let turns = read_rows(&connection, &own_rows("turns", TURN_COLUMNS), params![at, limit], |row| {
            Ok(Some((turn_record(row, 1)?, 400)))
        })?;
        let actions = read_rows(&connection, &own_rows("actions", ACTION_COLUMNS), params![at, limit], |row| {
            Ok(Some((action_record(row, 1)?, 200)))
        })?;
        let engaged = read_rows(&connection, &own_rows("engaged", ENGAGED_COLUMNS), params![at, limit], |row| {
            Ok(Some((engaged_record(row, 1)?, 120)))
        })?;
        let lasts = [
            projects.last(),
            threads.last(),
            items.last(),
            usage.last(),
            tombstones.last(),
            clients.last(),
            turns.last(),
            actions.last(),
            engaged.last(),
        ];
        let (up_to, more) = cut(own.change, &lasts);
        let set = ChangeSet {
            projects: projects.up_to(up_to).into_iter().map(|(change, value)| Logged { change, value }).collect(),
            threads: threads.up_to(up_to).into_iter().map(|(change, value)| Logged { change, value }).collect(),
            items: items
                .up_to(up_to)
                .into_iter()
                .map(|(change, (thread_id, item))| LoggedItem { change, thread_id, item })
                .collect(),
            usage: usage.up_to(up_to).into_iter().map(|(change, value)| Logged { change, value }).collect(),
            tombstones: tombstones
                .up_to(up_to)
                .into_iter()
                .map(|(change, (kind, id))| Tombstone { kind, id, change })
                .collect(),
            clients: logged(clients.up_to(up_to)),
            turns: logged(turns.up_to(up_to)),
            actions: logged(actions.up_to(up_to)),
            engaged: logged(engaged.up_to(up_to)),
        };
        Ok(LogPart { log_id: own.log_id, after, up_to, more, set })
    }

    /// What changed in this server's copy of `origin`'s log after `after`.
    pub fn copied_changes(&self, origin: &str, after: u64) -> anyhow::Result<LogPart> {
        let connection = self.connection();
        let Some(copy) = origin_watermark(&connection, origin)? else {
            bail!("This server has no copy of that server's threads.");
        };
        let after = after.min(copy.change);
        let at = after as i64;
        let limit = (BATCH_ROWS + 1) as i64;
        let payloads = |table: &str| {
            format!("SELECT change, payload FROM {table} WHERE origin = ?1 AND change > ?2 ORDER BY change LIMIT ?3")
        };
        let projects = read_rows(&connection, &payloads("copied_projects"), params![origin, at, limit], |row| {
            json::<Project>(row, 1)
        })?;
        let threads = read_rows(&connection, &payloads("copied_threads"), params![origin, at, limit], |row| {
            json::<Thread>(row, 1)
        })?;
        let items = read_rows(
            &connection,
            "SELECT change, thread_id, payload FROM copied_items
             WHERE origin = ?1 AND change > ?2 ORDER BY change LIMIT ?3",
            params![origin, at, limit],
            |row| {
                let thread_id: String = row.get(1)?;
                Ok(json::<Item>(row, 2)?.map(|(item, size)| ((thread_id, item), size)))
            },
        )?;
        let usage = read_rows(
            &connection,
            "SELECT change, at, thread_id, project_id, agent, agent_account, model, input, cache_read, cache_write,
                    output, cost_usd, purpose
             FROM copied_usage WHERE origin = ?1 AND change > ?2 ORDER BY change LIMIT ?3",
            params![origin, at, limit],
            |row| Ok(Some((usage_record(row, 1)?, 200))),
        )?;
        let tombstones = read_rows(
            &connection,
            "SELECT change, kind, id FROM copied_tombstones WHERE origin = ?1 AND change > ?2 ORDER BY change LIMIT ?3",
            params![origin, at, limit],
            |row| Ok(Some(((kind_of(&row.get::<_, String>(1)?), row.get::<_, String>(2)?), 100))),
        )?;
        let copied_rows = |table: &str, columns: &str| {
            format!("SELECT change, {columns} FROM {table} WHERE origin = ?1 AND change > ?2 ORDER BY change LIMIT ?3")
        };
        let clients = read_rows(
            &connection,
            &copied_rows("copied_clients", CLIENT_COLUMNS),
            params![origin, at, limit],
            |row| Ok(Some((client_record(row, 1)?, 150))),
        )?;
        let turns =
            read_rows(&connection, &copied_rows("copied_turns", TURN_COLUMNS), params![origin, at, limit], |row| {
                Ok(Some((turn_record(row, 1)?, 400)))
            })?;
        let actions = read_rows(
            &connection,
            &copied_rows("copied_actions", ACTION_COLUMNS),
            params![origin, at, limit],
            |row| Ok(Some((action_record(row, 1)?, 200))),
        )?;
        let engaged = read_rows(
            &connection,
            &copied_rows("copied_engaged", ENGAGED_COLUMNS),
            params![origin, at, limit],
            |row| Ok(Some((engaged_record(row, 1)?, 120))),
        )?;
        let lasts = [
            projects.last(),
            threads.last(),
            items.last(),
            usage.last(),
            tombstones.last(),
            clients.last(),
            turns.last(),
            actions.last(),
            engaged.last(),
        ];
        let (up_to, more) = cut(copy.change, &lasts);
        let set = ChangeSet {
            projects: projects.up_to(up_to).into_iter().map(|(change, value)| Logged { change, value }).collect(),
            threads: threads.up_to(up_to).into_iter().map(|(change, value)| Logged { change, value }).collect(),
            items: items
                .up_to(up_to)
                .into_iter()
                .map(|(change, (thread_id, item))| LoggedItem { change, thread_id, item })
                .collect(),
            usage: usage.up_to(up_to).into_iter().map(|(change, value)| Logged { change, value }).collect(),
            tombstones: tombstones
                .up_to(up_to)
                .into_iter()
                .map(|(change, (kind, id))| Tombstone { kind, id, change })
                .collect(),
            clients: logged(clients.up_to(up_to)),
            turns: logged(turns.up_to(up_to)),
            actions: logged(actions.up_to(up_to)),
            engaged: logged(engaged.up_to(up_to)),
        };
        Ok(LogPart { log_id: copy.log_id, after, up_to, more, set })
    }

    /// Keeps a batch of `origin`'s log in this server's copy of it, in one transaction. Only what
    /// the copy doesn't have yet is written, so a batch that comes twice changes nothing. A batch
    /// of another log than the copy's, or with `reset`, replaces the copy, and must start at the
    /// beginning of the log.
    pub fn apply(
        &self,
        origin: &str,
        name: &str,
        platform: &str,
        batch: &ChangeBatch,
        reset: bool,
    ) -> anyhow::Result<()> {
        self.apply_part(origin, name, platform, &LogPart::decode(batch)?, reset)
    }

    pub fn apply_part(
        &self,
        origin: &str,
        name: &str,
        platform: &str,
        batch: &LogPart,
        reset: bool,
    ) -> anyhow::Result<()> {
        let set = &batch.set;
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        if own_watermark(&transaction)?.log_id == batch.log_id {
            bail!("That is this server's own log.");
        }
        let have = match origin_watermark(&transaction, origin)? {
            Some(copy) if copy.log_id == batch.log_id && !reset => copy.change,
            Some(_) | None => {
                if batch.after != 0 {
                    bail!("The copy starts with a batch from the beginning of the log.");
                }
                drop_copies(&transaction, origin)?;
                0
            }
        };
        if batch.after > have {
            bail!("The batch starts after what the copy has.");
        }
        let fresh = |change: u64| change > have;
        for project in set.projects.iter().filter(|logged| fresh(logged.change)) {
            transaction.execute(
                "INSERT INTO copied_projects (origin, id, change, payload) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (origin, id) DO UPDATE SET change = excluded.change, payload = excluded.payload
                 WHERE excluded.change > copied_projects.change",
                params![origin, project.value.id, project.change as i64, serde_json::to_string(&project.value)?],
            )?;
        }
        for thread in set.threads.iter().filter(|logged| fresh(logged.change)) {
            transaction.execute(
                "INSERT INTO copied_threads (origin, id, change, payload) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (origin, id) DO UPDATE SET change = excluded.change, payload = excluded.payload
                 WHERE excluded.change > copied_threads.change",
                params![origin, thread.value.id, thread.change as i64, serde_json::to_string(&thread.value)?],
            )?;
        }
        for logged in set.items.iter().filter(|logged| fresh(logged.change)) {
            transaction.execute(
                "INSERT INTO copied_items (origin, thread_id, id, seq, change, payload) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (origin, thread_id, id) DO UPDATE SET
                     seq = excluded.seq, change = excluded.change, payload = excluded.payload
                 WHERE excluded.change > copied_items.change",
                params![
                    origin,
                    logged.thread_id,
                    logged.item.id,
                    logged.item.seq as i64,
                    logged.change as i64,
                    serde_json::to_string(&logged.item)?
                ],
            )?;
        }
        for logged in set.usage.iter().filter(|logged| fresh(logged.change)) {
            let usage = &logged.value;
            transaction.execute(
                "INSERT OR IGNORE INTO copied_usage (origin, change, at, thread_id, project_id, agent, agent_account,
                     model, input, cache_read, cache_write, output, cost_usd, purpose)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    origin,
                    logged.change as i64,
                    usage.at,
                    usage.thread_id,
                    usage.project_id,
                    as_text(&usage.agent),
                    usage.agent_account,
                    usage.model,
                    usage.tokens.input as i64,
                    usage.tokens.cache_read as i64,
                    usage.tokens.cache_write as i64,
                    usage.tokens.output as i64,
                    usage.cost_usd,
                    usage.purpose,
                ],
            )?;
        }
        for logged in set.clients.iter().filter(|logged| fresh(logged.change)) {
            let mut values: Vec<rusqlite::types::Value> =
                vec![origin.to_string().into(), (logged.change as i64).into()];
            values.extend(client_values(&logged.value));
            transaction.execute(
                &format!(
                    "INSERT INTO copied_clients (origin, change, {CLIENT_COLUMNS}) VALUES ({})
                     ON CONFLICT (origin, device) DO UPDATE SET change = excluded.change, kind = excluded.kind,
                         system = excluded.system, form = excluded.form, version = excluded.version,
                         seen_at = excluded.seen_at
                     WHERE excluded.change > copied_clients.change",
                    placeholders(0, 8)
                ),
                params_from_iter(values),
            )?;
        }
        for logged in set.turns.iter().filter(|logged| fresh(logged.change)) {
            let mut values: Vec<rusqlite::types::Value> =
                vec![origin.to_string().into(), (logged.change as i64).into()];
            values.extend(turn_values(&logged.value));
            transaction.execute(
                &format!(
                    "INSERT INTO copied_turns (origin, change, {TURN_COLUMNS}) VALUES ({})
                     ON CONFLICT (origin, id) DO UPDATE SET change = excluded.change, starter = excluded.starter,
                         device = excluded.device
                     WHERE excluded.change > copied_turns.change",
                    placeholders(0, 24)
                ),
                params_from_iter(values),
            )?;
        }
        for logged in set.actions.iter().filter(|logged| fresh(logged.change)) {
            let mut values: Vec<rusqlite::types::Value> =
                vec![origin.to_string().into(), (logged.change as i64).into()];
            values.extend(action_values(&logged.value));
            transaction.execute(
                &format!(
                    "INSERT OR IGNORE INTO copied_actions (origin, change, {ACTION_COLUMNS}) VALUES ({})",
                    placeholders(0, 11)
                ),
                params_from_iter(values),
            )?;
        }
        for logged in set.engaged.iter().filter(|logged| fresh(logged.change)) {
            let mut values: Vec<rusqlite::types::Value> =
                vec![origin.to_string().into(), (logged.change as i64).into()];
            values.extend(engaged_values(&logged.value));
            transaction.execute(
                &format!(
                    "INSERT OR IGNORE INTO copied_engaged (origin, change, {ENGAGED_COLUMNS}) VALUES ({})",
                    placeholders(0, 6)
                ),
                params_from_iter(values),
            )?;
        }
        for tombstone in set.tombstones.iter().filter(|tombstone| fresh(tombstone.change)) {
            match tombstone.kind {
                TombstoneKind::Project => {
                    transaction.execute(
                        "DELETE FROM copied_projects WHERE origin = ?1 AND id = ?2",
                        [origin, tombstone.id.as_str()],
                    )?;
                }
                TombstoneKind::Thread => {
                    transaction.execute(
                        "DELETE FROM copied_threads WHERE origin = ?1 AND id = ?2",
                        [origin, tombstone.id.as_str()],
                    )?;
                    transaction.execute(
                        "DELETE FROM copied_items WHERE origin = ?1 AND thread_id = ?2",
                        [origin, tombstone.id.as_str()],
                    )?;
                }
            }
            transaction.execute(
                "INSERT OR REPLACE INTO copied_tombstones (origin, kind, id, change) VALUES (?1, ?2, ?3, ?4)",
                params![origin, kind_text(tombstone.kind), tombstone.id, tombstone.change as i64],
            )?;
        }
        transaction.execute(
            "INSERT INTO origins (id, log_id, change, name, platform) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (id) DO UPDATE SET
                 log_id = excluded.log_id, change = MAX(origins.change, excluded.change),
                 name = excluded.name, platform = excluded.platform",
            params![origin, batch.log_id, batch.up_to.max(have) as i64, name, platform],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Drops every copy of these servers' threads.
    pub fn drop_origins(&self, origins: &[String]) -> rusqlite::Result<()> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        for origin in origins {
            drop_copies(&transaction, origin)?;
        }
        transaction.commit()
    }

    /// The projects and threads of this server's copy of `origin`.
    pub fn copies(&self, origin: &str) -> rusqlite::Result<(Vec<Project>, Vec<Thread>)> {
        let connection = self.connection();
        let payloads = |table: &str| -> rusqlite::Result<Vec<String>> {
            let mut statement = connection.prepare(&format!(
                "SELECT payload FROM {table} WHERE origin = ?1 ORDER BY json_extract(payload, '$.created_at')"
            ))?;
            let rows = statement.query_map([origin], |row| row.get(0))?;
            rows.collect()
        };
        let projects = payloads("copied_projects")?.iter().filter_map(|json| serde_json::from_str(json).ok()).collect();
        let threads = payloads("copied_threads")?.iter().filter_map(|json| serde_json::from_str(json).ok()).collect();
        Ok((projects, threads))
    }

    /// The transcript of a thread of `origin`, from this server's copy, in pieces.
    pub fn copied_transcript(&self, origin: &str, thread_id: &str) -> rusqlite::Result<Vec<Vec<Item>>> {
        let items = self.copied_items(origin, thread_id)?;
        Ok(items.chunks(TRANSCRIPT_PIECE).map(<[Item]>::to_vec).collect())
    }

    pub fn copied_items(&self, origin: &str, thread_id: &str) -> rusqlite::Result<Vec<Item>> {
        let connection = self.connection();
        let mut statement =
            connection.prepare("SELECT payload FROM copied_items WHERE origin = ?1 AND thread_id = ?2 ORDER BY seq")?;
        let rows = statement.query_map([origin, thread_id], |row| row.get::<_, String>(0))?;
        let mut items = Vec::new();
        for payload in rows {
            if let Ok(item) = serde_json::from_str(&payload?) {
                items.push(item);
            }
        }
        Ok(items)
    }

    /// Whether the copy of another server holds a thread whose id starts with `prefix`.
    pub fn copies_thread(&self, prefix: &str) -> rusqlite::Result<bool> {
        let query = "SELECT EXISTS(SELECT 1 FROM copied_threads WHERE substr(id, 1, length(?1)) = ?1)";
        self.connection().query_row(query, [prefix], |row| row.get(0))
    }

    pub fn copied_thread(&self, origin: &str, thread_id: &str) -> rusqlite::Result<Option<Thread>> {
        let payload: Option<String> = self
            .connection()
            .query_row("SELECT payload FROM copied_threads WHERE origin = ?1 AND id = ?2", [origin, thread_id], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(payload.and_then(|payload| serde_json::from_str(&payload).ok()))
    }
}

fn logged<T>(rows: Vec<(u64, T)>) -> Vec<Logged<T>> {
    rows.into_iter().map(|(change, value)| Logged { change, value }).collect()
}

fn usage_record(row: &rusqlite::Row, first: usize) -> rusqlite::Result<UsageRecord> {
    Ok(UsageRecord {
        at: row.get(first)?,
        thread_id: row.get(first + 1)?,
        project_id: row.get(first + 2)?,
        agent: from_text(&row.get::<_, String>(first + 3)?).unwrap_or(Agent::Claude),
        agent_account: row.get(first + 4)?,
        model: row.get(first + 5)?,
        tokens: tokens_at(row, first + 6)?,
        cost_usd: row.get(first + 10)?,
        purpose: row.get(first + 11)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let folder = tempfile::tempdir().unwrap();
        let store = Store::open(&folder.path().join("motile.db")).unwrap();
        (folder, store)
    }

    fn add_thread(store: &Store, id: &str, items: usize) {
        let connection = store.connection();
        connection
            .execute(
                "INSERT INTO threads (id, title, title_source, project_id, cwd, agent, access, created_at, updated_at)
                 VALUES (?1, 'Title', 'user', 'p', '/tmp', 'claude', 'full', 0, 1)",
                [id],
            )
            .unwrap();
        drop(connection);
        for seq in 0..items {
            let item = serde_json::json!({
                "id": format!("{id}-{seq}"), "seq": seq, "rev": seq + 1, "created_at": 0.0,
                "type": "assistant", "text": format!("Reply {seq} in {id}")
            });
            store.save_item(id, &serde_json::from_value(item).unwrap()).unwrap();
        }
    }

    fn set(part: &LogPart) -> ChangeSet {
        LogPart::decode(&part.encode().unwrap()).unwrap().set
    }

    /// Carries the whole log of `from` (its own, or its copy of `origin`) to `to`.
    fn carry(from: &Store, origin: &str, own: bool, to: &Store) -> usize {
        let mut batches = 0;
        loop {
            let after = to
                .origin_watermarks()
                .unwrap()
                .iter()
                .find(|copy| copy.origin == origin)
                .map_or(0, |copy| copy.watermark.change);
            let part = if own { from.changes(after).unwrap() } else { from.copied_changes(origin, after).unwrap() };
            to.apply(origin, "build-box", "linux", &part.encode().unwrap(), false).unwrap();
            batches += 1;
            if !part.more {
                return batches;
            }
        }
    }

    #[test]
    fn every_write_takes_the_next_change_and_a_delete_leaves_a_tombstone() {
        let (_folder, store) = store();
        add_thread(&store, "a", 2);
        let first = set(&store.changes(0).unwrap());
        assert_eq!(first.threads.len(), 1);
        assert_eq!(first.items.iter().map(|logged| logged.change).collect::<Vec<_>>(), vec![2, 3]);

        let after = store.watermark().unwrap().change;
        store.connection().execute("UPDATE threads SET title = 'Renamed' WHERE id = 'a'", []).unwrap();
        let renamed = set(&store.changes(after).unwrap());
        assert_eq!(renamed.threads[0].value.title, "Renamed");
        assert!(renamed.items.is_empty());

        let after = store.watermark().unwrap().change;
        store.delete_thread("a").unwrap();
        let deleted = store.changes(after).unwrap();
        assert_eq!(deleted.set.tombstones.len(), 1);
        assert_eq!(deleted.set.tombstones[0].id, "a");
        assert_eq!(deleted.up_to, store.watermark().unwrap().change);
        assert!(!deleted.more);
    }

    #[test]
    fn a_long_log_comes_in_batches_that_leave_nothing_out() {
        let (_folder, from) = store();
        let (_other, to) = store();
        add_thread(&from, "a", BATCH_ROWS + 10);
        add_thread(&from, "b", 3);

        assert!(carry(&from, "origin", true, &to) > 1);

        assert_eq!(to.copied_items("origin", "a").unwrap().len(), BATCH_ROWS + 10);
        assert_eq!(to.copied_items("origin", "b").unwrap().len(), 3);
        let (_, threads) = to.copies("origin").unwrap();
        assert_eq!(threads.len(), 2);
        let copy = &to.origin_watermarks().unwrap()[0];
        assert_eq!(copy.watermark, from.watermark().unwrap());
    }

    #[test]
    fn a_batch_that_comes_twice_changes_nothing_and_one_after_a_gap_is_refused() {
        let (_folder, from) = store();
        let (_other, to) = store();
        add_thread(&from, "a", 2);
        let first = from.changes(0).unwrap().encode().unwrap();
        let middle = from.watermark().unwrap().change;
        from.delete_thread("a").unwrap();
        add_thread(&from, "b", 1);
        let middle_part = from.changes(middle).unwrap().encode().unwrap();

        assert!(to.apply("origin", "box", "linux", &middle_part, false).is_err(), "it starts after what the copy has");
        to.apply("origin", "box", "linux", &first, false).unwrap();
        to.apply("origin", "box", "linux", &middle_part, false).unwrap();
        // The first batch again doesn't bring back the thread that was deleted since.
        to.apply("origin", "box", "linux", &first, false).unwrap();

        let (_, threads) = to.copies("origin").unwrap();
        assert_eq!(threads.iter().map(|thread| thread.id.as_str()).collect::<Vec<_>>(), vec!["b"]);
        assert!(to.copied_items("origin", "a").unwrap().is_empty());
        assert_eq!(to.origin_watermarks().unwrap()[0].watermark.change, from.watermark().unwrap().change);
    }

    #[test]
    fn a_copy_of_another_log_is_made_again_from_its_beginning() {
        let (_folder, from) = store();
        let (_other, to) = store();
        add_thread(&from, "a", 1);
        carry(&from, "origin", true, &to);

        let (_restored, again) = store();
        add_thread(&again, "c", 1);
        let later = again.changes(1).unwrap().encode().unwrap();
        assert!(to.apply("origin", "box", "linux", &later, false).is_err(), "a new log starts at its beginning");
        to.apply("origin", "box", "linux", &again.changes(0).unwrap().encode().unwrap(), false).unwrap();

        let (_, threads) = to.copies("origin").unwrap();
        assert_eq!(threads.iter().map(|thread| thread.id.as_str()).collect::<Vec<_>>(), vec!["c"]);
        assert_eq!(to.origin_watermarks().unwrap()[0].watermark, again.watermark().unwrap());

        let reset = from.changes(0).unwrap().encode().unwrap();
        let (_, threads) = {
            let (_copy, copy) = store();
            copy.apply("origin", "box", "linux", &reset, false).unwrap();
            add_thread(&from, "d", 1);
            copy.apply("origin", "box", "linux", &from.changes(0).unwrap().encode().unwrap(), true).unwrap();
            copy.copies("origin").unwrap()
        };
        assert_eq!(threads.len(), 2);
    }

    #[test]
    fn a_server_refuses_its_own_log() {
        let (_folder, store) = store();
        add_thread(&store, "a", 1);
        let own = store.changes(0).unwrap().encode().unwrap();
        assert!(store.apply("me", "box", "linux", &own, false).is_err());
    }

    #[test]
    fn a_copy_passed_on_keeps_its_origins_changes_and_tombstones() {
        let (_a, a) = store();
        let (_b, b) = store();
        let (_c, c) = store();
        add_thread(&a, "kept", 2);
        add_thread(&a, "gone", 2);
        carry(&a, "a", true, &b);
        a.delete_thread("gone").unwrap();
        carry(&a, "a", true, &b);

        carry(&b, "a", false, &c);

        assert_eq!(c.origin_watermarks().unwrap(), b.origin_watermarks().unwrap());
        let (_, threads) = c.copies("a").unwrap();
        assert_eq!(threads.iter().map(|thread| thread.id.as_str()).collect::<Vec<_>>(), vec!["kept"]);
        assert_eq!(c.copied_items("a", "kept").unwrap().len(), 2);
        assert!(c.copied_items("a", "gone").unwrap().is_empty());
        let tombstones = set(&c.copied_changes("a", 0).unwrap()).tombstones;
        assert_eq!(tombstones.iter().map(|tombstone| tombstone.id.as_str()).collect::<Vec<_>>(), vec!["gone"]);
    }

    #[test]
    fn a_copy_tells_which_thread_ids_are_another_servers() {
        let (_a, a) = store();
        let (_b, b) = store();
        add_thread(&a, "1b949867-aaaa", 1);
        carry(&a, "a", true, &b);

        assert!(b.copies_thread("1b949867").unwrap());
        assert!(b.copies_thread("1b949867-aaaa").unwrap());
        assert!(!b.copies_thread("1b949868").unwrap());
        assert!(!a.copies_thread("1b949867").unwrap(), "a server's own threads aren't copies");
    }

    #[test]
    fn usage_is_copied_and_counted_by_its_origin() {
        let (_a, a) = store();
        let (_b, b) = store();
        a.connection()
            .execute(
                "INSERT INTO usage (at, thread_id, project_id, agent, model, input, cache_read, cache_write, output,
                                    agent_account)
                 VALUES (10, 't', 'p', 'codex', 'gpt-6', 5, 0, 0, 7, 'codex')",
                [],
            )
            .unwrap();
        carry(&a, "a", true, &b);

        assert!(b.usage(0.0, 100.0, 3600, 0).unwrap().is_empty());
        let copied = b.usage_of(Some("a"), 0.0, 100.0, 3600, 0).unwrap();
        assert_eq!((copied[0].tokens.input, copied[0].tokens.output), (5, 7));
        assert!(b.usage_of(Some("other"), 0.0, 100.0, 3600, 0).unwrap().is_empty());
    }

    #[test]
    fn activity_is_copied_counted_by_its_origin_and_passed_on() {
        use motile_protocol::wire::{
            ActionRecord, ActorKind, ClientForm, ClientInfo, ClientKind, ClientRecord, EngagedRecord, TurnRecord,
        };
        let (_a, a) = store();
        let (_b, b) = store();
        let (_c, c) = store();
        let ipad = ClientInfo {
            kind: ClientKind::Ios,
            system: "ipados".into(),
            form: ClientForm::Tablet,
            version: "1".into(),
        };
        a.save_client(&ClientRecord { device: "ipad".into(), client: ipad, seen_at: 5.0, connected: false }).unwrap();
        let turn: TurnRecord = serde_json::from_value(serde_json::json!({
            "id": "end", "thread_id": "t", "project_id": "p", "agent": "codex", "agent_account": "", "model": "gpt-6",
            "effort": "", "access": "full", "starter": "user", "device": "ipad", "started_at": 10.0, "ended_at": 40.0,
            "outcome": "done", "waited_secs": 0.0, "asks": 0, "steers": 1, "tools": 4, "agents": 0, "subagents": 0,
            "output_tokens": 90, "reasoning_tokens": 30, "fast": null
        }))
        .unwrap();
        a.save_turn(&turn).unwrap();
        let action = ActionRecord {
            at: 12.0,
            actor: ActorKind::User,
            device: Some("ipad".into()),
            feature: "thread.archive".into(),
            variant: String::new(),
            thread_id: "t".into(),
            project_id: "p".into(),
            path: None,
            rtt_ms: None,
        };
        a.save_action(&action).unwrap();
        let stretch = EngagedRecord { device: String::new(), project_id: "p".into(), started_at: 20.0, ended_at: 50.0 };
        a.save_engaged("ipad", &[stretch]).unwrap();
        carry(&a, "a", true, &b);
        carry(&b, "a", false, &c);

        for copy in [&b, &c] {
            assert!(copy.activity_of(None, 0.0, 100.0, 3600, 0).unwrap().turns.is_empty());
            let copied = copy.activity_of(Some("a"), 0.0, 100.0, 3600, 0).unwrap();
            let turns: Vec<_> = copied
                .turns
                .iter()
                .map(|bucket| (bucket.turns, bucket.steers, bucket.tools, bucket.reasoning_tokens))
                .collect();
            assert_eq!(turns, [(1, 1, 4, 30)]);
            assert_eq!(copied.actions[0].feature, "thread.archive");
            assert_eq!(copied.clients[0].client.form, ClientForm::Tablet);
            assert_eq!(copied.engaged[0].device, "ipad");
            assert_eq!(copied.engaged[0].spans, [[20, 50]]);
        }
    }

    #[test]
    fn a_search_finds_a_removed_servers_threads_only_when_asked_to() {
        let (_a, a) = store();
        let (_b, b) = store();
        add_thread(&a, "far", 1);
        add_thread(&b, "near", 1);
        carry(&a, "a", true, &b);

        let own = b.search_threads_with("reply 0", 10, &[]).unwrap();
        assert_eq!(own.iter().map(|found| found.thread_id.as_str()).collect::<Vec<_>>(), vec!["near"]);
        let both = b.search_threads_with("reply 0", 10, &["a".to_string()]).unwrap();
        let origins: Vec<_> = both.iter().map(|found| (found.thread_id.as_str(), found.origin.as_deref())).collect();
        assert!(origins.contains(&("far", Some("a"))) && origins.contains(&("near", None)), "{origins:?}");
    }

    #[test]
    fn dropping_an_origin_leaves_no_copy_of_it() {
        let (_a, a) = store();
        let (_b, b) = store();
        add_thread(&a, "t", 2);
        carry(&a, "a", true, &b);

        b.drop_origins(&["a".to_string()]).unwrap();

        assert!(b.origin_watermarks().unwrap().is_empty());
        assert_eq!(b.copies("a").unwrap().1.len(), 0);
        assert!(b.copied_items("a", "t").unwrap().is_empty());
    }

    #[test]
    fn a_copied_project_says_where_its_repository_was_cloned_from() {
        let (folder, store) = store();
        let repository = folder.path().join("motile");
        std::fs::create_dir_all(repository.join(".git")).unwrap();
        std::fs::write(
            repository.join(".git/config"),
            "[remote \"origin\"]\n\turl = git@github.com:owner/motile.git\n",
        )
        .unwrap();
        store
            .connection()
            .execute("INSERT INTO projects (id, path, created_at) VALUES ('p', ?1, 0)", [repository.to_string_lossy()])
            .unwrap();

        let projects = set(&store.changes(0).unwrap()).projects;

        assert_eq!(projects[0].value.origin.as_deref(), Some("github.com/owner/motile"));
    }

    #[test]
    fn a_database_from_before_the_log_has_its_rows_numbered() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("motile.db");
        {
            let connection = Connection::open(&path).unwrap();
            for (index, migration) in crate::store::MIGRATIONS.iter().take(17).enumerate() {
                connection.execute_batch(&format!("{migration} PRAGMA user_version = {};", index + 1)).unwrap();
            }
            connection
                .execute_batch(
                    "INSERT INTO threads (id, title, title_source, project_id, cwd, agent, access, created_at, updated_at)
                     VALUES ('t', 'Title', 'user', 'p', '/tmp', 'claude', 'full', 0, 0);
                     INSERT INTO items (thread_id, id, seq, rev, payload)
                     VALUES ('t', 'i', 0, 1, '{\"id\":\"i\",\"seq\":0,\"rev\":1,\"created_at\":0,\"type\":\"assistant\",\"text\":\"Hi\"}');",
                )
                .unwrap();
        }
        let store = Store::open(&path).unwrap();
        let changes = set(&store.changes(0).unwrap());
        assert_eq!((changes.threads[0].change, changes.items[0].change), (1, 2));
        assert_eq!(store.watermark().unwrap().change, 2);
        add_thread(&store, "u", 0);
        assert_eq!(store.watermark().unwrap().change, 3);
    }
}
