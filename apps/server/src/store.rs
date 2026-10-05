//! Threads, their transcripts and the projects, in SQLite.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use motile_protocol::wire::{Access, Agent, Item, Queued, Thread};
use rusqlite::{Connection, OptionalExtension, params};

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

/// A thread with what only the server needs to know about it.
#[derive(Clone, Debug)]
pub struct StoredThread {
    pub thread: Thread,
    /// The agent's session to resume, known once the first turn starts.
    pub session_id: Option<String>,
    pub title_source: TitleSource,
    pub next_seq: u64,
    /// Set when the thread works in a worktree of its own, which is its `cwd`.
    pub worktree: Option<StoredWorktree>,
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
                    t.session_id, t.created_at, t.updated_at, t.done_at, t.needs_approval, t.turn_ended_at, t.undone_at,
                    COALESCE(MAX(i.rev), 0), COALESCE(MAX(i.seq) + 1, 0), t.worktree_branch, t.worktree_base,
                    t.pull_request, t.watching
             FROM threads t LEFT JOIN items i ON i.thread_id = t.id
             GROUP BY t.id",
        )?;
        let threads = statement.query_map([], |row| {
            let rev: i64 = row.get(17)?;
            let next_seq: i64 = row.get(18)?;
            let worktree: (Option<String>, Option<String>) = (row.get(19)?, row.get(20)?);
            let pull_request: Option<String> = row.get(21)?;
            Ok(StoredThread {
                thread: Thread {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    project_id: row.get(3)?,
                    cwd: row.get(4)?,
                    agent: from_text(&row.get::<_, String>(5)?).unwrap_or(Agent::Claude),
                    model: row.get(6)?,
                    effort: row.get(7)?,
                    access: from_text(&row.get::<_, String>(8)?).unwrap_or(Access::Full),
                    plan: row.get(9)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                    done_at: row.get(13)?,
                    undone_at: row.get(16)?,
                    running: false,
                    monitoring: false,
                    needs_approval: row.get(14)?,
                    agents: 0,
                    turn_ended_at: row.get(15)?,
                    pull_request: pull_request.and_then(|json| serde_json::from_str(&json).ok()),
                    watching: row.get(22)?,
                    git_stage: None,
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
                                  session_id, created_at, updated_at, done_at, needs_approval, turn_ended_at, undone_at,
                                  worktree_branch, worktree_base, pull_request, watching)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
             ON CONFLICT(id) DO UPDATE SET
                 title = excluded.title,
                 title_source = excluded.title_source,
                 model = excluded.model,
                 effort = excluded.effort,
                 access = excluded.access,
                 plan = excluded.plan,
                 session_id = excluded.session_id,
                 updated_at = excluded.updated_at,
                 done_at = excluded.done_at,
                 needs_approval = excluded.needs_approval,
                 turn_ended_at = excluded.turn_ended_at,
                 undone_at = excluded.undone_at,
                 worktree_branch = excluded.worktree_branch,
                 pull_request = excluded.pull_request,
                 watching = excluded.watching",
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
                stored.session_id,
                thread.created_at,
                thread.updated_at,
                thread.done_at,
                thread.needs_approval,
                thread.turn_ended_at,
                thread.undone_at,
                stored.worktree.as_ref().map(|worktree| &worktree.branch),
                stored.worktree.as_ref().map(|worktree| &worktree.base),
                thread.pull_request.as_ref().and_then(|found| serde_json::to_string(found).ok()),
                thread.watching,
            ],
        )?;
        Ok(())
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

fn migrate(connection: &Connection) -> anyhow::Result<()> {
    let applied: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    for (index, migration) in MIGRATIONS.iter().enumerate().skip(applied as usize) {
        connection.execute_batch(&format!("BEGIN; {migration} PRAGMA user_version = {}; COMMIT;", index + 1))?;
    }
    Ok(())
}
