//! The sidebar's search through the threads' messages, by their index (`item_search` and
//! `copied_item_search`): the newest messages with every word long enough for it, narrowed by the
//! shorter words, then graded and ranked as `motile_protocol::search` grades them.

use std::cmp::Ordering;
use std::collections::HashMap;

use motile_protocol::search::{self, Graded, Matcher};
use motile_protocol::wire::{Speaker, ThreadMatch};
use rusqlite::{Connection, Params};

use crate::store::Store;

/// The newest messages read for a search, so that a common word costs no more than a rare one.
const HIT_LIMIT: u32 = 2000;
/// How many of the newest messages the index finds the filters may pass over to fill that.
const CANDIDATE_LIMIT: u32 = 10_000;
/// A thread's last activity lifts it less by half every two weeks.
const RECENCY_HALF_LIFE: f64 = 14.0 * 24.0 * 3600.0;

const OWN_HITS: &str = "
    SELECT '', items.thread_id, json_extract(items.payload, '$.type'), json_extract(items.payload, '$.text'),
           json_extract(items.payload, '$.created_at'), threads.updated_at, items.id
    FROM item_search
    JOIN items ON items.rowid = item_search.rowid
    JOIN threads ON threads.id = items.thread_id
    WHERE item_search MATCH ?1
      AND item_search.rowid >= coalesce(
          (SELECT rowid FROM item_search WHERE item_search MATCH ?1 ORDER BY rowid DESC LIMIT 1 OFFSET ?3), 0)
      AND threads.parent_thread IS NULL
      AND NOT EXISTS (SELECT 1 FROM json_each(?2) AS token
                      WHERE instr(lower(json_extract(items.payload, '$.text')), token.value) = 0)
    ORDER BY item_search.rowid DESC
    LIMIT ?4";

const COPIED_HITS: &str = "
    SELECT copied_items.origin, copied_items.thread_id, json_extract(copied_items.payload, '$.type'),
           json_extract(copied_items.payload, '$.text'), json_extract(copied_items.payload, '$.created_at'),
           json_extract(copied_threads.payload, '$.updated_at'), copied_items.id
    FROM copied_item_search
    JOIN copied_items ON copied_items.rowid = copied_item_search.rowid
    JOIN copied_threads ON copied_threads.origin = copied_items.origin AND copied_threads.id = copied_items.thread_id
    WHERE copied_item_search MATCH ?1
      AND copied_item_search.rowid >= coalesce(
          (SELECT rowid FROM copied_item_search WHERE copied_item_search MATCH ?1 ORDER BY rowid DESC LIMIT 1 OFFSET ?3), 0)
      AND copied_items.origin IN (SELECT value FROM json_each(?5))
      AND json_extract(copied_threads.payload, '$.parent') IS NULL
      AND NOT EXISTS (SELECT 1 FROM json_each(?2) AS token
                      WHERE instr(lower(json_extract(copied_items.payload, '$.text')), token.value) = 0)
    ORDER BY copied_item_search.rowid DESC
    LIMIT ?4";

/// A message with the words in it.
struct Hit {
    origin: String,
    thread_id: String,
    item_id: String,
    user: bool,
    text: String,
    created_at: f64,
    /// When its thread last changed.
    updated_at: f64,
}

/// A thread's best message, and how many of its messages match as well.
struct Best {
    hit: Hit,
    graded: Graded,
    hits: u32,
}

impl Store {
    pub fn search_threads(&self, query: &str, limit: u32) -> rusqlite::Result<Vec<ThreadMatch>> {
        self.search_threads_with(query, limit, &[])
    }

    /// The threads with every word of `query` in a message of the user's or of the agent's, best
    /// first, and through the copies of the `removed` servers' threads too. Not the threads an
    /// agent started, nor what the agents an agent started inside a thread said. Each comes with
    /// its best message.
    pub fn search_threads_with(
        &self,
        query: &str,
        limit: u32,
        removed: &[String],
    ) -> rusqlite::Result<Vec<ThreadMatch>> {
        let tokens = search::tokens(query);
        if tokens.long.is_empty() {
            return Ok(Vec::new());
        }
        let phrases: Vec<String> =
            tokens.long.iter().map(|token| format!("\"{}\"", token.replace('"', "\"\""))).collect();
        let (phrases, short) = (phrases.join(" "), serde_json::to_string(&tokens.short).unwrap_or_default());
        let connection = self.reader();
        let mut found = hits(&connection, OWN_HITS, rusqlite::params![phrases, short, CANDIDATE_LIMIT - 1, HIT_LIMIT])?;
        if !removed.is_empty() {
            let removed = serde_json::to_string(removed).unwrap_or_default();
            let params = rusqlite::params![phrases, short, CANDIDATE_LIMIT - 1, HIT_LIMIT, removed];
            found.extend(hits(&connection, COPIED_HITS, params)?);
        }
        drop(connection);
        Ok(rank(found, &Matcher::new(query), limit as usize, motile_protocol::now()))
    }
}

fn hits(connection: &Connection, sql: &str, params: impl Params) -> rusqlite::Result<Vec<Hit>> {
    let mut statement = connection.prepare_cached(sql)?;
    let rows = statement.query_map(params, |row| {
        Ok(Hit {
            origin: row.get(0)?,
            thread_id: row.get(1)?,
            user: row.get::<_, String>(2)? == "user",
            text: row.get(3)?,
            created_at: row.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
            updated_at: row.get::<_, Option<f64>>(5)?.unwrap_or(0.0),
            item_id: row.get(6)?,
        })
    })?;
    rows.collect()
}

/// Each thread once, by its best message: the best graded, then the user's, then the newest.
/// Threads where every word is a word come first, then the better graded, then those with more
/// such messages and lately changed, then the newest message.
fn rank(hits: Vec<Hit>, matcher: &Matcher, limit: usize, now: f64) -> Vec<ThreadMatch> {
    let mut threads: HashMap<(String, String), Best> = HashMap::new();
    for mut hit in hits {
        hit.text = search::link_text(&hit.text);
        let Some(graded) = matcher.grade(&hit.text) else { continue };
        let key = (hit.origin.clone(), hit.thread_id.clone());
        let Some(best) = threads.get_mut(&key) else {
            threads.insert(key, Best { hit, graded, hits: 1 });
            continue;
        };
        match graded.quality.total_cmp(&best.graded.quality) {
            Ordering::Greater => best.hits = 1,
            Ordering::Equal => best.hits += 1,
            Ordering::Less => {}
        }
        let better = graded
            .quality
            .total_cmp(&best.graded.quality)
            .then(hit.user.cmp(&best.hit.user))
            .then(hit.created_at.total_cmp(&best.hit.created_at));
        if better == Ordering::Greater {
            *best = Best { hit, graded, hits: best.hits };
        }
    }
    let score = |best: &Best| {
        let recency = 2f64.powf(-(now - best.hit.updated_at).max(0.0) / RECENCY_HALF_LIFE);
        (1.0 + (1.0 + f64::from(best.hits)).log2()) * (1.0 + recency)
    };
    let mut ranked: Vec<Best> = threads.into_values().collect();
    ranked.sort_by(|a, b| {
        let words = |best: &Best| best.graded.quality >= search::PREFIX;
        words(b)
            .cmp(&words(a))
            .then(b.graded.quality.total_cmp(&a.graded.quality))
            .then(score(b).total_cmp(&score(a)))
            .then(b.hit.created_at.total_cmp(&a.hit.created_at))
            .then(a.hit.thread_id.cmp(&b.hit.thread_id))
    });
    ranked
        .into_iter()
        .take(limit)
        .map(|Best { hit, graded, .. }| ThreadMatch {
            speaker: if hit.user { Speaker::User } else { Speaker::Agent },
            excerpt: search::excerpt(&hit.text, graded.offset),
            origin: Some(hit.origin).filter(|origin| !origin.is_empty()),
            thread_id: hit.thread_id,
            item_id: hit.item_id,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use motile_protocol::wire::Item;

    use super::*;
    use crate::store::MIGRATIONS;

    fn add_threads(store: &Store) {
        store
            .connection()
            .execute_batch(
                "INSERT INTO threads (id, title, title_source, project_id, cwd, agent, access, created_at, updated_at)
                 VALUES ('old', 'Old', 'user', 'p', '/tmp', 'claude', 'full', 0, 1),
                        ('new', 'New', 'user', 'p', '/tmp', 'claude', 'full', 0, 2);",
            )
            .unwrap();
    }

    fn save(store: &Store, thread_id: &str, seq: u64, payload: serde_json::Value) {
        let mut payload = payload;
        payload["id"] = format!("{thread_id}-{seq}").into();
        payload["seq"] = seq.into();
        payload["rev"] = seq.into();
        payload["created_at"] = (seq as f64).into();
        let item: Item = serde_json::from_value(payload).unwrap();
        store.save_item(thread_id, &item).unwrap();
    }

    fn store_with_threads(items: &[(&str, u64, serde_json::Value)]) -> (tempfile::TempDir, Store) {
        let folder = tempfile::tempdir().unwrap();
        let store = Store::open(&folder.path().join("motile.db")).unwrap();
        add_threads(&store);
        for (thread_id, seq, payload) in items {
            save(&store, thread_id, *seq, payload.clone());
        }
        (folder, store)
    }

    fn found(store: &Store, query: &str) -> Vec<(String, String)> {
        let found = store.search_threads(query, 50).unwrap();
        found.into_iter().map(|found| (found.thread_id, found.excerpt)).collect()
    }

    fn pair(thread_id: &str, excerpt: &str) -> (String, String) {
        (thread_id.into(), excerpt.into())
    }

    #[test]
    fn a_search_finds_each_thread_once_by_its_best_message_the_users_and_the_newest_first() {
        let (_folder, store) = store_with_threads(&[
            ("old", 1, serde_json::json!({"type": "assistant", "text": "The DEPLOY failed"})),
            ("new", 1, serde_json::json!({"type": "user", "text": "Watch the deploy", "attachments": []})),
            ("new", 2, serde_json::json!({"type": "assistant", "text": "The deploy is green"})),
            ("new", 3, serde_json::json!({"type": "user", "text": "Deploy again", "attachments": []})),
            ("new", 4, serde_json::json!({"type": "thinking", "text": "deploy"})),
        ]);

        let found = store.search_threads("deploy", 50).unwrap();

        assert_eq!(
            found,
            [
                ThreadMatch {
                    thread_id: "new".into(),
                    speaker: Speaker::User,
                    excerpt: "Deploy again".into(),
                    item_id: "new-3".into(),
                    origin: None
                },
                ThreadMatch {
                    thread_id: "old".into(),
                    speaker: Speaker::Agent,
                    excerpt: "The DEPLOY failed".into(),
                    item_id: "old-1".into(),
                    origin: None
                },
            ]
        );
        assert_eq!(store.search_threads("deploy", 1).unwrap().len(), 1);
    }

    #[test]
    fn words_match_in_any_order_and_whole_words_rank_above_parts_of_words() {
        let (_folder, store) = store_with_threads(&[
            ("old", 1, serde_json::json!({"type": "assistant", "text": "The deploy is green"})),
            ("new", 1, serde_json::json!({"type": "assistant", "text": "Redeployed, all green"})),
            ("new", 2, serde_json::json!({"type": "assistant", "text": "Green across the board"})),
        ]);

        assert_eq!(
            found(&store, "green deploy"),
            [pair("old", "The deploy is green"), pair("new", "Redeployed, all green")]
        );
        assert_eq!(found(&store, "go deploy"), []);
        assert_eq!(found(&store, "de"), []);
    }

    #[test]
    fn a_search_passes_over_the_agents_an_agent_started_and_takes_wildcards_as_they_are() {
        let (_folder, store) = store_with_threads(&[
            ("old", 1, serde_json::json!({"type": "assistant", "text": "100% done", "parent": "tool"})),
            ("new", 1, serde_json::json!({"type": "user", "text": "Say \"100_ok\"", "attachments": []})),
        ]);

        assert_eq!(found(&store, "100%"), []);
        assert_eq!(found(&store, " 100 ").len(), 1);
        assert_eq!(found(&store, "\"100_ok\""), [pair("new", "Say \"100_ok\"")]);
        assert_eq!(found(&store, "100_o!"), []);

        store.connection().execute("UPDATE threads SET parent_thread = 'old' WHERE id = 'new'", []).unwrap();
        assert_eq!(found(&store, "100_ok"), []);
    }

    #[test]
    fn a_message_is_found_as_it_reads_now() {
        let (_folder, store) = store_with_threads(&[(
            "new",
            1,
            serde_json::json!({"type": "assistant", "text": "Read [the guide](https://example.com/setup-guide)"}),
        )]);
        assert_eq!(found(&store, "the guide"), [pair("new", "Read the guide")]);
        assert_eq!(found(&store, "example"), []);

        save(&store, "new", 1, serde_json::json!({"type": "assistant", "text": "Rewritten"}));
        assert_eq!(found(&store, "guide"), []);
        assert_eq!(found(&store, "rewritten"), [pair("new", "Rewritten")]);

        store.connection().execute("DELETE FROM threads WHERE id = 'new'", []).unwrap();
        assert_eq!(found(&store, "rewritten"), []);
    }

    #[test]
    fn the_messages_from_before_the_index_are_found() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("motile.db");
        {
            let connection = Connection::open(&path).unwrap();
            for (index, migration) in MIGRATIONS.iter().take(MIGRATIONS.len() - 1).enumerate() {
                connection.execute_batch(&format!("{migration} PRAGMA user_version = {};", index + 1)).unwrap();
            }
            connection
                .execute_batch(
                    "INSERT INTO threads (id, title, title_source, project_id, cwd, agent, access, created_at, updated_at)
                     VALUES ('t', 'Title', 'user', 'p', '/tmp', 'codex', 'full', 0, 0);
                     INSERT INTO items (thread_id, id, seq, rev, payload) VALUES
                        ('t', 'a', 0, 1, '{\"id\":\"a\",\"seq\":0,\"rev\":1,\"created_at\":0,\"type\":\"user\",\"text\":\"Fix the café’s menu\",\"attachments\":[]}'),
                        ('t', 'b', 1, 2, '{\"id\":\"b\",\"seq\":1,\"rev\":2,\"created_at\":1,\"type\":\"tool\",\"text\":\"menu\"}');",
                )
                .unwrap();
        }

        let store = Store::open(&path).unwrap();

        assert_eq!(found(&store, "cafe's"), [pair("t", "Fix the café’s menu")]);
    }
}
