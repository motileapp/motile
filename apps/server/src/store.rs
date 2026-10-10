//! Threads, their transcripts and the projects, in SQLite.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use motile_protocol::wire::{Access, Agent, Item, Queued, Thread, Tokens, UsageBucket};
use rusqlite::{Connection, OptionalExtension, params};

use crate::agents::ModelUsage;

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_project_icons.sql"),
    include_str!("../migrations/0003_media.sql"),
    include_str!("../migrations/0004_settings.sql"),
    include_str!("../migrations/0005_worktrees.sql"),
    include_str!("../migrations/0006_attachments.sql"),
    include_str!("../migrations/0007_thread_pull_requests.sql"),
    include_str!("../migrations/0008_queued.sql"),
    include_str!("../migrations/0009_watching.sql"),
    include_str!("../migrations/0010_usage.sql"),
    include_str!("../migrations/0011_usage_purpose.sql"),
    include_str!("../migrations/0012_interruptions.sql"),
    include_str!("../migrations/0013_agent_accounts.sql"),
    include_str!("../migrations/0014_thread_positions.sql"),
    include_str!("../migrations/0015_thread_sessions.sql"),
    include_str!("../migrations/0016_session_accounts.sql"),
];

pub struct Store {
    connection: Mutex<Connection>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TitleSource {
    /// Cut from the first message until a better one is generated.
    Placeholder,
    Generated,
    User,
}

impl TitleSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Placeholder => "placeholder",
            Self::Generated => "generated",
            Self::User => "user",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "generated" => Self::Generated,
            "user" => Self::User,
            _ => Self::Placeholder,
        }
    }
}

/// What tokens were spent on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Purpose {
    Turn,
    Title,
    Branch,
    Commit,
    PullRequest,
}

impl Purpose {
    fn as_str(self) -> &'static str {
        match self {
            Self::Turn => "turn",
            Self::Title => "title",
            Self::Branch => "branch",
            Self::Commit => "commit",
            Self::PullRequest => "pull_request",
        }
    }
}

/// A thread with what only the server needs to know about it.
#[derive(Clone, Debug)]
pub struct StoredThread {
    pub thread: Thread,
    /// The session of the thread's continuation to resume, known once its first turn starts.
    pub session_id: Option<String>,
    pub title_source: TitleSource,
    pub next_seq: u64,
    /// Set when the thread works in a worktree of its own, which is its `cwd`.
    pub worktree: Option<StoredWorktree>,
}

/// An agent and the folder it keeps its sessions in. Accounts that share one take over each
/// other's sessions.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Continuation {
    pub agent: Agent,
    pub folder: String,
}

/// What the server knows of a thread's native session in one continuation.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Session {
    pub session_id: Option<String>,
    /// The model it last ran.
    pub model: Option<String>,
    /// The position of the last item it has seen.
    pub seen_through: u64,
    /// Set while it was given what it missed up to this position and hasn't taken it yet.
    pub pending_through: Option<u64>,
    pub context_used: Option<u64>,
    pub context_window: Option<u64>,
    /// When it last took a turn.
    pub last_turn_at: Option<f64>,
    /// The id of the account it last took a turn with.
    pub account: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredWorktree {
    /// The branch made for the thread, which the worktree is made again with when it has gone.
    pub branch: String,
    /// The branch that one started from.
    pub base: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredProject {
    pub id: String,
    pub path: String,
    pub created_at: f64,
    /// The image the project is shown with, a file on this machine.
    pub icon: Option<String>,
    /// The user picked the icon; it isn't looked for in the project's folder again.
    pub icon_chosen: bool,
    /// The shell script that runs in every new worktree.
    pub setup: Option<String>,
}

pub struct ThreadSummary {
    pub title: String,
    pub project: String,
    pub agent: Agent,
    pub model: Option<String>,
}

fn as_text<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value).ok().and_then(|value| value.as_str().map(String::from)).unwrap_or_default()
}

fn from_text<T: serde::de::DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(text.to_string())).ok()
}

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.pragma_update(None, "foreign_keys", true)?;
        migrate(&connection)?;
        Ok(Self { connection: Mutex::new(connection) })
    }

    fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.connection.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn load_threads(&self) -> rusqlite::Result<Vec<StoredThread>> {
        let connection = self.connection();
        let mut statement = connection.prepare(
            "SELECT t.id, t.title, t.title_source, t.project_id, t.cwd, t.agent, t.model, t.effort, t.access, t.plan,
                    t.session_id, t.created_at, t.updated_at, t.done_at, t.needs_approval, t.turn_ended_at, t.position,
                    COALESCE(MAX(i.rev), 0), COALESCE(MAX(i.seq) + 1, 0), t.worktree_branch, t.worktree_base,
                    t.pull_request, t.watching, t.running, t.monitoring, t.interruption, t.agent_account
             FROM threads t LEFT JOIN items i ON i.thread_id = t.id
             GROUP BY t.id",
        )?;
        let threads = statement.query_map([], |row| {
            let rev: i64 = row.get(17)?;
            let next_seq: i64 = row.get(18)?;
            let worktree: (Option<String>, Option<String>) = (row.get(19)?, row.get(20)?);
            let pull_request: Option<String> = row.get(21)?;
            let interruption: Option<String> = row.get(25)?;
            Ok(StoredThread {
                thread: Thread {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    project_id: row.get(3)?,
                    cwd: row.get(4)?,
                    agent: from_text(&row.get::<_, String>(5)?).unwrap_or(Agent::Claude),
                    agent_account: row.get(26)?,
                    model: row.get(6)?,
                    effort: row.get(7)?,
                    access: from_text(&row.get::<_, String>(8)?).unwrap_or(Access::Full),
                    plan: row.get(9)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                    done_at: row.get(13)?,
                    position: row.get(16)?,
                    running: row.get(23)?,
                    monitoring: row.get(24)?,
                    needs_approval: row.get(14)?,
                    agents: 0,
                    turn_ended_at: row.get(15)?,
                    pull_request: pull_request.and_then(|json| serde_json::from_str(&json).ok()),
                    watching: row.get(22)?,
                    git_stage: None,
                    interruption: interruption.and_then(|json| serde_json::from_str(&json).ok()),
                    rev: rev as u64,
                },
                session_id: row.get(10)?,
                title_source: TitleSource::parse(&row.get::<_, String>(2)?),
                next_seq: next_seq as u64,
                worktree: match worktree {
                    (Some(branch), Some(base)) => Some(StoredWorktree { branch, base }),
                    _ => None,
                },
            })
        })?;
        threads.collect()
    }

    pub fn save_thread(&self, stored: &StoredThread) -> rusqlite::Result<()> {
        let thread = &stored.thread;
        self.connection().execute(
            "INSERT INTO threads (id, title, title_source, project_id, cwd, agent, model, effort, access, plan,
                                  created_at, updated_at, done_at, needs_approval, turn_ended_at, position,
                                  worktree_branch, worktree_base, pull_request, watching, running, monitoring,
                                  interruption, agent_account)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22,
                     ?23, ?24)
             ON CONFLICT(id) DO UPDATE SET
                 title = excluded.title,
                 title_source = excluded.title_source,
                 agent = excluded.agent,
                 agent_account = excluded.agent_account,
                 model = excluded.model,
                 effort = excluded.effort,
                 access = excluded.access,
                 plan = excluded.plan,
                 updated_at = excluded.updated_at,
                 done_at = excluded.done_at,
                 needs_approval = excluded.needs_approval,
                 turn_ended_at = excluded.turn_ended_at,
                 position = excluded.position,
                 worktree_branch = excluded.worktree_branch,
                 pull_request = excluded.pull_request,
                 watching = excluded.watching,
                 running = excluded.running,
                 monitoring = excluded.monitoring,
                 interruption = excluded.interruption",
            params![
                thread.id,
                thread.title,
                stored.title_source.as_str(),
                thread.project_id,
                thread.cwd,
                as_text(&thread.agent),
                thread.model,
                thread.effort,
                as_text(&thread.access),
                thread.plan,
                thread.created_at,
                thread.updated_at,
                thread.done_at,
                thread.needs_approval,
                thread.turn_ended_at,
                thread.position,
                stored.worktree.as_ref().map(|worktree| &worktree.branch),
                stored.worktree.as_ref().map(|worktree| &worktree.base),
                thread.pull_request.as_ref().and_then(|found| serde_json::to_string(found).ok()),
                thread.watching,
                thread.running,
                thread.monitoring,
                thread.interruption.as_ref().and_then(|interruption| serde_json::to_string(interruption).ok()),
                thread.agent_account,
            ],
        )?;
        Ok(())
    }

    /// The thread's sessions, in each continuation it has run in.
    pub fn sessions(&self, thread_id: &str) -> rusqlite::Result<Vec<(Continuation, Session)>> {
        let connection = self.connection();
        let mut statement = connection.prepare(
            "SELECT agent, sessions_folder, session_id, model, seen_through_seq, pending_through_seq, context_used,
                    context_window, last_turn_at, account
             FROM thread_sessions WHERE thread_id = ?1",
        )?;
        let count = |row: &rusqlite::Row, index| row.get::<_, Option<i64>>(index).map(|count| count.map(|n| n as u64));
        let sessions = statement.query_map([thread_id], |row| {
            let continuation = Continuation {
                agent: from_text(&row.get::<_, String>(0)?).unwrap_or(Agent::Claude),
                folder: row.get(1)?,
            };
            let session = Session {
                session_id: row.get(2)?,
                model: row.get(3)?,
                seen_through: row.get::<_, i64>(4)? as u64,
                pending_through: count(row, 5)?,
                context_used: count(row, 6)?,
                context_window: count(row, 7)?,
                last_turn_at: row.get(8)?,
                account: row.get(9)?,
            };
            Ok((continuation, session))
        })?;
        sessions.collect()
    }

    pub fn session(&self, thread_id: &str, continuation: &Continuation) -> rusqlite::Result<Option<Session>> {
        let sessions = self.sessions(thread_id)?;
        Ok(sessions.into_iter().find(|(kept, _)| kept == continuation).map(|(_, session)| session))
    }

    pub fn save_session(
        &self,
        thread_id: &str,
        continuation: &Continuation,
        session: &Session,
    ) -> rusqlite::Result<()> {
        self.connection().execute(
            "INSERT OR REPLACE INTO thread_sessions (thread_id, agent, sessions_folder, session_id, model,
                 seen_through_seq, pending_through_seq, context_used, context_window, last_turn_at, account)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                thread_id,
                as_text(&continuation.agent),
                continuation.folder,
                session.session_id,
                session.model,
                session.seen_through as i64,
                session.pending_through.map(|seq| seq as i64),
                session.context_used.map(|count| count as i64),
                session.context_window.map(|count| count as i64),
                session.last_turn_at,
                session.account,
            ],
        )?;
        Ok(())
    }

    /// Moves the session a thread kept from before continuations into the one it works in, as
    /// having seen everything up to `seen_through`.
    pub fn adopt_session(
        &self,
        thread_id: &str,
        continuation: &Continuation,
        session_id: &str,
        seen_through: u64,
    ) -> rusqlite::Result<()> {
        if self.session(thread_id, continuation)?.is_none() {
            let session = Session {
                session_id: Some(session_id.to_string()),
                seen_through,
                last_turn_at: Some(motile_protocol::now()),
                ..Session::default()
            };
            self.save_session(thread_id, continuation, &session)?;
        }
        self.connection().execute("UPDATE threads SET session_id = NULL WHERE id = ?1", [thread_id])?;
        Ok(())
    }

    /// The thread's title, its project's folder, its agent and its model, for an agent that reads it.
    pub fn thread_summary(&self, thread_id: &str) -> rusqlite::Result<Option<ThreadSummary>> {
        let connection = self.connection();
        let summary = connection.query_row(
            "SELECT t.title, COALESCE(p.path, ''), t.agent, t.model
             FROM threads t LEFT JOIN projects p ON p.id = t.project_id WHERE t.id = ?1",
            [thread_id],
            |row| {
                Ok(ThreadSummary {
                    title: row.get(0)?,
                    project: row.get(1)?,
                    agent: from_text(&row.get::<_, String>(2)?).unwrap_or(Agent::Claude),
                    model: row.get(3)?,
                })
            },
        );
        summary.optional()
    }

    pub fn delete_thread(&self, thread_id: &str) -> rusqlite::Result<()> {
        self.connection().execute("DELETE FROM threads WHERE id = ?1", [thread_id])?;
        Ok(())
    }

    pub fn save_media(&self, thread_id: &str, media_id: &str) -> rusqlite::Result<()> {
        self.connection()
            .execute("INSERT OR IGNORE INTO media (thread_id, id) VALUES (?1, ?2)", [thread_id, media_id])?;
        Ok(())
    }

    pub fn media_of(&self, thread_id: &str) -> rusqlite::Result<Vec<String>> {
        let connection = self.connection();
        let mut statement = connection.prepare("SELECT id FROM media WHERE thread_id = ?1")?;
        let ids = statement.query_map([thread_id], |row| row.get(0))?;
        ids.collect()
    }

    /// Whether any thread still shows the file.
    pub fn shows_media(&self, media_id: &str) -> rusqlite::Result<bool> {
        self.connection().query_row("SELECT EXISTS (SELECT 1 FROM media WHERE id = ?1)", [media_id], |row| row.get(0))
    }

    pub fn save_attachment(&self, thread_id: &str, path: &str) -> rusqlite::Result<()> {
        self.connection()
            .execute("INSERT OR IGNORE INTO attachments (thread_id, path) VALUES (?1, ?2)", [thread_id, path])?;
        Ok(())
    }

    /// The files attached to the thread's messages, or to those of every thread.
    pub fn attachments(&self, thread_id: Option<&str>) -> rusqlite::Result<Vec<String>> {
        let connection = self.connection();
        let mut statement = connection.prepare("SELECT path FROM attachments WHERE ?1 IS NULL OR thread_id = ?1")?;
        let paths = statement.query_map([thread_id], |row| row.get(0))?;
        paths.collect()
    }

    pub fn load_projects(&self) -> rusqlite::Result<Vec<StoredProject>> {
        let connection = self.connection();
        let mut statement = connection
            .prepare("SELECT id, path, created_at, icon, icon_chosen, setup FROM projects ORDER BY created_at")?;
        let projects = statement.query_map([], |row| {
            Ok(StoredProject {
                id: row.get(0)?,
                path: row.get(1)?,
                created_at: row.get(2)?,
                icon: row.get(3)?,
                icon_chosen: row.get(4)?,
                setup: row.get(5)?,
            })
        })?;
        projects.collect()
    }

    pub fn add_project(&self, project: &StoredProject) -> rusqlite::Result<()> {
        self.connection().execute(
            "INSERT INTO projects (id, path, created_at, icon, icon_chosen) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![project.id, project.path, project.created_at, project.icon, project.icon_chosen],
        )?;
        Ok(())
    }

    pub fn save_project_icon(&self, project: &StoredProject) -> rusqlite::Result<()> {
        self.connection().execute(
            "UPDATE projects SET icon = ?2, icon_chosen = ?3 WHERE id = ?1",
            params![project.id, project.icon, project.icon_chosen],
        )?;
        Ok(())
    }

    pub fn save_project_setup(&self, project: &StoredProject) -> rusqlite::Result<()> {
        self.connection()
            .execute("UPDATE projects SET setup = ?2 WHERE id = ?1", params![project.id, project.setup])?;
        Ok(())
    }

    pub fn remove_project(&self, project_id: &str) -> rusqlite::Result<()> {
        self.connection().execute("DELETE FROM projects WHERE id = ?1", [project_id])?;
        Ok(())
    }

    pub fn setting(&self, name: &str) -> Option<String> {
        let connection = self.connection();
        connection.query_row("SELECT value FROM settings WHERE name = ?1", [name], |row| row.get(0)).ok()
    }

    /// Keeps the value, or forgets the setting when there is none.
    pub fn set_setting(&self, name: &str, value: Option<&str>) -> rusqlite::Result<()> {
        let connection = self.connection();
        match value {
            Some(value) => connection.execute(
                "INSERT INTO settings (name, value) VALUES (?1, ?2) ON CONFLICT (name) DO UPDATE SET value = ?2",
                params![name, value],
            )?,
            None => connection.execute("DELETE FROM settings WHERE name = ?1", [name])?,
        };
        Ok(())
    }

    /// Keeps what the thread's agent spent and returns what it says that cost. With `total` it is
    /// what `session_id` has spent since it began, and only what was added since it last said
    /// is kept.
    pub fn save_usage(
        &self,
        at: f64,
        thread: &Thread,
        session_id: &str,
        spent: &[ModelUsage],
        total: bool,
    ) -> rusqlite::Result<Option<f64>> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let known: Option<bool> = transaction
            .query_row("SELECT usage_known FROM threads WHERE id = ?1", [&thread.id], |row| row.get(0))
            .optional()?;
        let known = known.unwrap_or(true);
        let mut cost = None;
        for usage in spent {
            let (mut tokens, mut cost_usd) = (usage.tokens, usage.cost_usd);
            if total {
                let last = transaction
                    .query_row(
                        "SELECT input, cache_read, cache_write, output, cost_usd FROM usage_totals
                         WHERE thread_id = ?1 AND session_id = ?2 AND model = ?3",
                        params![thread.id, session_id, usage.model],
                        |row| Ok((tokens_at(row, 0)?, row.get::<_, f64>(4)?)),
                    )
                    .optional()?;
                transaction.execute(
                    "INSERT OR REPLACE INTO usage_totals
                         (thread_id, session_id, model, input, cache_read, cache_write, output, cost_usd)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        thread.id,
                        session_id,
                        usage.model,
                        tokens.input as i64,
                        tokens.cache_read as i64,
                        tokens.cache_write as i64,
                        tokens.output as i64,
                        cost_usd.unwrap_or_default(),
                    ],
                )?;
                if !known {
                    continue;
                }
                if let Some((last, last_cost)) = last.filter(|(last, _)| grew_from(&tokens, last)) {
                    tokens = Tokens {
                        input: tokens.input - last.input,
                        cache_read: tokens.cache_read - last.cache_read,
                        cache_write: tokens.cache_write - last.cache_write,
                        output: tokens.output - last.output,
                    };
                    cost_usd = cost_usd.map(|cost| (cost - last_cost).max(0.0));
                }
            }
            if tokens == Tokens::default() {
                continue;
            }
            let usage = ModelUsage { model: usage.model.clone(), tokens, cost_usd };
            let spender = (thread.agent, thread.agent_account.as_str());
            insert_usage(&transaction, at, &thread.id, &thread.project_id, spender, Purpose::Turn, &usage)?;
            if let Some(cost_usd) = cost_usd {
                cost = Some(cost.unwrap_or(0.0) + cost_usd);
            }
        }
        if total && !known {
            transaction.execute("UPDATE threads SET usage_known = 1 WHERE id = ?1", [&thread.id])?;
        }
        transaction.commit()?;
        Ok(cost)
    }

    /// Keeps what writing something took. `thread_id` is missing for what was written outside
    /// a thread.
    pub fn save_written(
        &self,
        at: f64,
        project_id: &str,
        thread_id: Option<&str>,
        spender: (Agent, &str),
        purpose: Purpose,
        spent: &[ModelUsage],
    ) -> rusqlite::Result<()> {
        let connection = self.connection();
        for usage in spent.iter().filter(|usage| usage.tokens != Tokens::default()) {
            insert_usage(&connection, at, thread_id.unwrap_or_default(), project_id, spender, purpose, usage)?;
        }
        Ok(())
    }

    /// What was spent from `since` until `until`, by account, model and project, in buckets of
    /// `bucket_secs` that start where a clock `utc_offset_secs` ahead of UTC starts them. Each
    /// bucket's `account_name` is the account's id.
    pub fn usage(
        &self,
        since: f64,
        until: f64,
        bucket_secs: u32,
        utc_offset_secs: i32,
    ) -> rusqlite::Result<Vec<UsageBucket>> {
        let connection = self.connection();
        let mut statement = connection.prepare(
            "SELECT CAST((at + ?3) / ?4 AS INTEGER) AS bucket, agent, model, project_id,
                    SUM(input), SUM(cache_read), SUM(cache_write), SUM(output), SUM(cost_usd),
                    purpose != 'turn' AS writing, agent_account
             FROM usage WHERE at >= ?1 AND at < ?2
             GROUP BY bucket, agent, agent_account, model, project_id, writing ORDER BY bucket",
        )?;
        let (size, offset) = (f64::from(bucket_secs.max(60)), f64::from(utc_offset_secs));
        let buckets = statement.query_map(params![since, until, offset, size], |row| {
            Ok(UsageBucket {
                start: row.get::<_, i64>(0)? as f64 * size - offset,
                agent: from_text(&row.get::<_, String>(1)?).unwrap_or(Agent::Claude),
                account_name: row.get(10)?,
                model: row.get(2)?,
                project_id: row.get(3)?,
                tokens: tokens_at(row, 4)?,
                cost_usd: row.get(8)?,
                costs: None,
                cache_savings_usd: 0.0,
                writing: row.get(9)?,
            })
        })?;
        buckets.collect()
    }

    /// The items changed after revision `since`, in transcript order.
    pub fn items_since(&self, thread_id: &str, since: u64) -> rusqlite::Result<Vec<Item>> {
        let connection = self.connection();
        let mut statement =
            connection.prepare("SELECT payload FROM items WHERE thread_id = ?1 AND rev > ?2 ORDER BY seq")?;
        let payloads = statement.query_map(params![thread_id, since as i64], |row| row.get::<_, String>(0))?;
        let mut items = Vec::new();
        for payload in payloads {
            if let Ok(item) = serde_json::from_str(&payload?) {
                items.push(item);
            }
        }
        Ok(items)
    }

    pub fn item(&self, thread_id: &str, item_id: &str) -> rusqlite::Result<Option<Item>> {
        let payload: Option<String> = self
            .connection()
            .query_row("SELECT payload FROM items WHERE thread_id = ?1 AND id = ?2", [thread_id, item_id], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(payload.and_then(|payload| serde_json::from_str(&payload).ok()))
    }

    /// The messages that wait, by thread, in their order.
    pub fn load_queued(&self) -> rusqlite::Result<HashMap<String, Vec<Queued>>> {
        let connection = self.connection();
        let mut statement = connection.prepare("SELECT thread_id, payload FROM queued ORDER BY thread_id, position")?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
        let mut queued: HashMap<String, Vec<Queued>> = HashMap::new();
        for row in rows {
            let (thread_id, payload) = row?;
            let Ok(message) = serde_json::from_str(&payload) else { continue };
            queued.entry(thread_id).or_default().push(message);
        }
        Ok(queued)
    }

    /// Replaces the messages that wait for the thread's agent with these.
    pub fn save_queued(&self, thread_id: &str, queued: &[Queued]) -> rusqlite::Result<()> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM queued WHERE thread_id = ?1", [thread_id])?;
        for (position, message) in queued.iter().enumerate() {
            transaction.execute(
                "INSERT INTO queued (thread_id, id, position, payload) VALUES (?1, ?2, ?3, ?4)",
                params![thread_id, message.id, position as i64, serde_json::to_string(message).unwrap_or_default()],
            )?;
        }
        transaction.commit()
    }

    pub fn save_item(&self, thread_id: &str, item: &Item) -> rusqlite::Result<()> {
        self.connection().execute(
            "INSERT INTO items (thread_id, id, seq, rev, payload) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(thread_id, id) DO UPDATE SET rev = excluded.rev, payload = excluded.payload",
            params![
                thread_id,
                item.id,
                item.seq as i64,
                item.rev as i64,
                serde_json::to_string(item).unwrap_or_default()
            ],
        )?;
        Ok(())
    }
}

fn insert_usage(
    connection: &Connection,
    at: f64,
    thread_id: &str,
    project_id: &str,
    (agent, account): (Agent, &str),
    purpose: Purpose,
    usage: &ModelUsage,
) -> rusqlite::Result<()> {
    let tokens = usage.tokens;
    connection.execute(
        "INSERT INTO usage (at, thread_id, project_id, agent, model, input, cache_read, cache_write, output, cost_usd,
                            purpose, agent_account)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            at,
            thread_id,
            project_id,
            as_text(&agent),
            usage.model,
            tokens.input as i64,
            tokens.cache_read as i64,
            tokens.cache_write as i64,
            tokens.output as i64,
            usage.cost_usd,
            purpose.as_str(),
            account,
        ],
    )?;
    Ok(())
}

fn tokens_at(row: &rusqlite::Row, first: usize) -> rusqlite::Result<Tokens> {
    let count = |index: usize| row.get::<_, i64>(first + index).map(|count| count as u64);
    Ok(Tokens { input: count(0)?, cache_read: count(1)?, cache_write: count(2)?, output: count(3)? })
}

/// A total that is smaller than the last one counts a session that began again.
fn grew_from(total: &Tokens, last: &Tokens) -> bool {
    total.input >= last.input
        && total.cache_read >= last.cache_read
        && total.cache_write >= last.cache_write
        && total.output >= last.output
}

fn migrate(connection: &Connection) -> anyhow::Result<()> {
    let applied: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    for (index, migration) in MIGRATIONS.iter().enumerate().skip(applied as usize) {
        connection.execute_batch(&format!("BEGIN; {migration} PRAGMA user_version = {}; COMMIT;", index + 1))?;
    }
    renumber_agent_accounts(connection)
}

/// Accounts were once named after their names. Each gets a UUID, and its threads, usage and
/// models go with it. The default accounts keep their agents' names.
fn renumber_agent_accounts(connection: &Connection) -> anyhow::Result<()> {
    let setting = |name: &str| -> rusqlite::Result<Option<String>> {
        connection.query_row("SELECT value FROM settings WHERE name = ?1", [name], |row| row.get(0)).optional()
    };
    let Some(kept) = setting("agent_accounts")? else { return Ok(()) };
    let mut accounts: Vec<serde_json::Value> = serde_json::from_str(&kept)?;
    let mut renumbered = Vec::new();
    for account in &mut accounts {
        let Some(id) = account["id"].as_str().map(str::to_string) else { continue };
        let agent: Agent = serde_json::from_value(account["agent"].clone())?;
        if id == crate::agent_accounts::default_id(agent) || uuid::Uuid::parse_str(&id).is_ok() {
            continue;
        }
        let new = uuid::Uuid::new_v4().to_string();
        account["id"] = new.clone().into();
        renumbered.push((id, new));
    }
    if renumbered.is_empty() {
        return Ok(());
    }
    let mut models: Vec<serde_json::Value> =
        setting("models")?.map(|models| serde_json::from_str(&models)).transpose()?.unwrap_or_default();
    for model in &mut models {
        let Some((_, new)) = renumbered.iter().find(|(old, _)| model["account"].as_str() == Some(old)) else {
            continue;
        };
        model["account"] = new.clone().into();
    }
    let transaction = connection.unchecked_transaction()?;
    for (old, new) in &renumbered {
        transaction.execute("UPDATE threads SET agent_account = ?2 WHERE agent_account = ?1", params![old, new])?;
        transaction.execute("UPDATE usage SET agent_account = ?2 WHERE agent_account = ?1", params![old, new])?;
    }
    let keep = "INSERT INTO settings (name, value) VALUES (?1, ?2) ON CONFLICT (name) DO UPDATE SET value = ?2";
    transaction.execute(keep, params!["agent_accounts", serde_json::to_string(&accounts)?])?;
    transaction.execute(keep, params!["models", serde_json::to_string(&models)?])?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threads_and_usage_from_before_accounts_belong_to_their_agents_default_account() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("motile.db");
        {
            let connection = Connection::open(&path).unwrap();
            for (index, migration) in MIGRATIONS.iter().take(12).enumerate() {
                connection.execute_batch(&format!("{migration} PRAGMA user_version = {};", index + 1)).unwrap();
            }
            connection
                .execute_batch(
                    "INSERT INTO threads (id, title, title_source, project_id, cwd, agent, access, created_at, updated_at)
                     VALUES ('t', 'Title', 'user', 'p', '/tmp', 'codex', 'full', 0, 0);
                     INSERT INTO usage (at, thread_id, project_id, agent, model, input, cache_read, cache_write, output)
                     VALUES (0, 't', 'p', 'codex', 'gpt-6', 1, 0, 0, 1);",
                )
                .unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.load_threads().unwrap()[0].thread.agent_account, "codex");
        let buckets = store.usage(0.0, 1.0, 3600, 0).unwrap();
        assert_eq!(buckets[0].account_name, "codex");
    }

    #[test]
    fn accounts_named_after_their_names_get_uuids_with_their_threads_usage_and_models() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("motile.db");
        {
            let store = Store::open(&path).unwrap();
            let accounts = r#"[{"id":"claude","agent":"claude","name":"Default","folder":""},
                {"id":"claude-work","agent":"claude","name":"Work","folder":"~/.claude-work"}]"#;
            let models =
                r#"[{"id":"claude-opus-5-5","name":"Opus","agent":"claude","account":"claude-work","efforts":[]}]"#;
            store.set_setting("agent_accounts", Some(accounts)).unwrap();
            store.set_setting("models", Some(models)).unwrap();
            store.connection().execute_batch(
                "INSERT INTO threads (id, title, title_source, project_id, cwd, agent, access, created_at, updated_at, agent_account)
                 VALUES ('t', 'Title', 'user', 'p', '/tmp', 'claude', 'full', 0, 0, 'claude-work');
                 INSERT INTO usage (at, thread_id, project_id, agent, model, input, cache_read, cache_write, output, agent_account)
                 VALUES (0, 't', 'p', 'claude', 'claude-opus-5-5', 1, 0, 0, 1, 'claude-work');",
            ).unwrap();
        }
        let store = Store::open(&path).unwrap();
        let accounts: Vec<serde_json::Value> = serde_json::from_str(&store.setting("agent_accounts").unwrap()).unwrap();
        assert_eq!(accounts[0]["id"], "claude");
        let work = accounts[1]["id"].as_str().unwrap().to_string();
        assert!(uuid::Uuid::parse_str(&work).is_ok(), "{work}");
        assert_eq!(store.load_threads().unwrap()[0].thread.agent_account, work);
        let models: Vec<serde_json::Value> = serde_json::from_str(&store.setting("models").unwrap()).unwrap();
        assert_eq!(models[0]["account"], work);
        let usage: String =
            store.connection().query_row("SELECT agent_account FROM usage", [], |row| row.get(0)).unwrap();
        assert_eq!(usage, work);
        drop(store);
        let again: Vec<serde_json::Value> =
            serde_json::from_str(&Store::open(&path).unwrap().setting("agent_accounts").unwrap()).unwrap();
        assert_eq!(again[1]["id"], work, "an account keeps its id from then on");
    }
}
