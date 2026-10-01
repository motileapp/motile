//! The app's copy of what its hosts hold, in SQLite, so the app shows where it left off before
//! it has connected to anything.

use std::path::Path;
use std::sync::Mutex;

use motile_protocol::auth_api::Me;
use motile_protocol::wire::{HostInfo, Item, Project, Thread};
use rusqlite::{Connection, OptionalExtension, params};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS hosts (id TEXT PRIMARY KEY, info TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS projects (
    host_id TEXT NOT NULL,
    id TEXT NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY (host_id, id)
);
CREATE TABLE IF NOT EXISTS threads (
    host_id TEXT NOT NULL,
    id TEXT NOT NULL,
    payload TEXT NOT NULL,
    -- When the user last looked at the thread on this device.
    seen_at REAL,
    -- The transcript revision the items below are complete up to.
    synced_rev INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (host_id, id)
);
CREATE TABLE IF NOT EXISTS items (
    thread_id TEXT NOT NULL,
    id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY (thread_id, id)
);
CREATE INDEX IF NOT EXISTS items_by_seq ON items(thread_id, seq);
";

pub struct Cache {
    connection: Mutex<Connection>,
}

pub struct CachedThread {
    pub thread: Thread,
    pub seen_at: Option<f64>,
}

impl Cache {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let connection = match Self::connect(path) {
            Ok(connection) => connection,
            // The cache only mirrors the hosts, so a damaged one is thrown away.
            Err(_) => {
                let _ = std::fs::remove_file(path);
                Self::connect(path)?
            }
        };
        Ok(Self { connection: Mutex::new(connection) })
    }

    fn connect(path: &Path) -> rusqlite::Result<Connection> {
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.execute_batch(SCHEMA)?;
        Ok(connection)
    }

    fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.connection.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn account(&self) -> Option<Me> {
        let text: Option<String> = self
            .connection()
            .query_row("SELECT value FROM kv WHERE key = 'account'", [], |row| row.get(0))
            .optional()
            .ok()?;
        serde_json::from_str(&text?).ok()
    }

    pub fn set_account(&self, me: &Me) {
        let text = serde_json::to_string(me).unwrap_or_default();
        let _ = self.connection().execute(
            "INSERT INTO kv (key, value) VALUES ('account', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [text],
        );
    }

    /// Forgets everything, as signing out does.
    pub fn clear(&self) {
        let _ = self.connection().execute_batch(
            "DELETE FROM kv; DELETE FROM hosts; DELETE FROM projects; DELETE FROM threads; DELETE FROM items;",
        );
    }

    pub fn host_info(&self, host_id: &str) -> Option<HostInfo> {
        let text: Option<String> = self
            .connection()
            .query_row("SELECT info FROM hosts WHERE id = ?1", [host_id], |row| row.get(0))
            .optional()
            .ok()?;
        serde_json::from_str(&text?).ok()
    }

    pub fn set_host_info(&self, host_id: &str, info: &HostInfo) {
        let text = serde_json::to_string(info).unwrap_or_default();
        let _ = self.connection().execute(
            "INSERT INTO hosts (id, info) VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET info = excluded.info",
            [host_id, &text],
        );
    }

    pub fn remove_host(&self, host_id: &str) {
        let connection = self.connection();
        let _ = connection
            .execute("DELETE FROM items WHERE thread_id IN (SELECT id FROM threads WHERE host_id = ?1)", [host_id]);
        let _ = connection.execute("DELETE FROM threads WHERE host_id = ?1", [host_id]);
        let _ = connection.execute("DELETE FROM projects WHERE host_id = ?1", [host_id]);
        let _ = connection.execute("DELETE FROM hosts WHERE id = ?1", [host_id]);
    }

    pub fn projects(&self, host_id: &str) -> Vec<Project> {
        let connection = self.connection();
        let Ok(mut statement) = connection.prepare("SELECT payload FROM projects WHERE host_id = ?1") else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map([host_id], |row| row.get::<_, String>(0)) else { return Vec::new() };
        let mut projects: Vec<Project> = rows.flatten().filter_map(|text| serde_json::from_str(&text).ok()).collect();
        projects.sort_by(|a, b| a.created_at.total_cmp(&b.created_at));
        projects
    }

    pub fn set_projects(&self, host_id: &str, projects: &[Project]) {
        let mut connection = self.connection();
        let Ok(transaction) = connection.transaction() else { return };
        let _ = transaction.execute("DELETE FROM projects WHERE host_id = ?1", [host_id]);
        for project in projects {
            let _ = transaction.execute(
                "INSERT INTO projects (host_id, id, payload) VALUES (?1, ?2, ?3)",
                params![host_id, project.id, serde_json::to_string(project).unwrap_or_default()],
            );
        }
        let _ = transaction.commit();
    }

    pub fn threads(&self, host_id: &str) -> Vec<CachedThread> {
        let connection = self.connection();
        let Ok(mut statement) = connection.prepare("SELECT payload, seen_at FROM threads WHERE host_id = ?1") else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map([host_id], |row| Ok((row.get::<_, String>(0)?, row.get(1)?))) else {
            return Vec::new();
        };
        let threads = rows.flatten().filter_map(|(text, seen_at)| {
            let mut thread: Thread = serde_json::from_str(&text).ok()?;
            // Whether it is running is only known once the host says so.
            thread.running = false;
            Some(CachedThread { thread, seen_at })
        });
        threads.collect()
    }

    /// Replaces the host's threads with the list it sent. Threads it no longer has are dropped.
    pub fn set_threads(&self, host_id: &str, threads: &[Thread]) {
        let mut connection = self.connection();
        let Ok(transaction) = connection.transaction() else { return };
        let ids =
            serde_json::to_string(&threads.iter().map(|thread| &thread.id).collect::<Vec<_>>()).unwrap_or_default();
        let _ = transaction.execute(
            "DELETE FROM items WHERE thread_id IN
                 (SELECT id FROM threads WHERE host_id = ?1 AND id NOT IN (SELECT value FROM json_each(?2)))",
            [host_id, &ids],
        );
        let _ = transaction.execute(
            "DELETE FROM threads WHERE host_id = ?1 AND id NOT IN (SELECT value FROM json_each(?2))",
            [host_id, &ids],
        );
        for thread in threads {
            let _ = upsert_thread(&transaction, host_id, thread);
        }
        let _ = transaction.commit();
    }

    pub fn upsert_thread(&self, host_id: &str, thread: &Thread) {
        let _ = upsert_thread(&self.connection(), host_id, thread);
    }

    pub fn remove_thread(&self, thread_id: &str) {
        let connection = self.connection();
        let _ = connection.execute("DELETE FROM items WHERE thread_id = ?1", [thread_id]);
        let _ = connection.execute("DELETE FROM threads WHERE id = ?1", [thread_id]);
    }

    pub fn seen_at(&self, thread_id: &str) -> Option<f64> {
        self.connection()
            .query_row("SELECT seen_at FROM threads WHERE id = ?1", [thread_id], |row| row.get(0))
            .optional()
            .ok()
            .flatten()
            .flatten()
    }

    pub fn set_seen(&self, thread_id: &str, at: f64) {
        let _ = self.connection().execute("UPDATE threads SET seen_at = ?2 WHERE id = ?1", params![thread_id, at]);
    }

    pub fn synced_rev(&self, thread_id: &str) -> u64 {
        let rev: Option<i64> = self
            .connection()
            .query_row("SELECT synced_rev FROM threads WHERE id = ?1", [thread_id], |row| row.get(0))
            .optional()
            .ok()
            .flatten();
        rev.unwrap_or(0) as u64
    }

    pub fn items(&self, thread_id: &str) -> Vec<Item> {
        let connection = self.connection();
        let Ok(mut statement) = connection.prepare("SELECT payload FROM items WHERE thread_id = ?1 ORDER BY seq")
        else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map([thread_id], |row| row.get::<_, String>(0)) else { return Vec::new() };
        rows.flatten().filter_map(|text| serde_json::from_str(&text).ok()).collect()
    }

    /// Stores the items and, in the same step, how far the copy is now complete. `synced_rev`
    /// is `None` while a catch-up is still arriving, because its items come in transcript order.
    pub fn save_items(&self, thread_id: &str, items: &[&Item], synced_rev: Option<u64>) {
        let mut connection = self.connection();
        let Ok(transaction) = connection.transaction() else { return };
        for item in items {
            let _ = transaction.execute(
                "INSERT INTO items (thread_id, id, seq, payload) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(thread_id, id) DO UPDATE SET seq = excluded.seq, payload = excluded.payload",
                params![thread_id, item.id, item.seq as i64, serde_json::to_string(item).unwrap_or_default()],
            );
        }
        if let Some(rev) = synced_rev {
            let _ =
                transaction.execute("UPDATE threads SET synced_rev = ?2 WHERE id = ?1", params![thread_id, rev as i64]);
        }
        let _ = transaction.commit();
    }

    pub fn clear_items(&self, thread_id: &str) {
        let connection = self.connection();
        let _ = connection.execute("DELETE FROM items WHERE thread_id = ?1", [thread_id]);
        let _ = connection.execute("UPDATE threads SET synced_rev = 0 WHERE id = ?1", [thread_id]);
    }
}

fn upsert_thread(connection: &Connection, host_id: &str, thread: &Thread) -> rusqlite::Result<usize> {
    connection.execute(
        "INSERT INTO threads (host_id, id, payload) VALUES (?1, ?2, ?3)
         ON CONFLICT(host_id, id) DO UPDATE SET payload = excluded.payload",
        params![host_id, thread.id, serde_json::to_string(thread).unwrap_or_default()],
    )
}
