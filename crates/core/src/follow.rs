//! Keeps the cached copy of an active thread current while no one looks at it, so it opens up to
//! date. Nothing is rendered; what arrives is written to the cache a few seconds at a time.

use std::collections::HashMap;

use motile_protocol::wire::{Activity, Item, ItemKind, Message, Thread};

use crate::cache::Cache;

pub struct Followed {
    pub server_id: String,
    /// Whether the catch-up is done and updates now arrive in revision order.
    pub live: bool,
    pub rev: u64,
    /// Whether the agent was working, by the thread's own stream.
    working: bool,
    unsaved: HashMap<String, Item>,
    /// The revision last recorded in the cache.
    saved_rev: u64,
}

/// Whether the thread has an agent at work or waiting on the user, by the server's thread list.
pub fn is_active(thread: &Thread) -> bool {
    thread.running || thread.monitoring || thread.needs_approval || thread.agents > 0
}

pub fn is_working(activity: &Activity) -> bool {
    activity.running || activity.monitoring || activity.agents > 0 || !activity.approvals.is_empty()
}

impl Followed {
    pub fn new(server_id: String, rev: u64, live: bool, working: bool) -> Self {
        Self { server_id, live, rev, working, unsaved: HashMap::new(), saved_rev: rev }
    }

    /// The stream is caught up and says the agent is done.
    pub fn is_done(&self) -> bool {
        self.live && !self.working
    }

    /// Takes in a message of the thread's stream. False when the thread can't be followed.
    pub fn take(&mut self, cache: &Cache, thread_id: &str, message: Message) -> bool {
        match message {
            Message::Opened { reset, activity } => {
                if reset {
                    self.unsaved.clear();
                    self.rev = 0;
                    self.saved_rev = 0;
                    cache.clear_items(thread_id);
                }
                self.save(cache, thread_id);
                self.live = false;
                self.working = is_working(&activity);
                cache.set_activity(thread_id, &activity);
            }
            Message::Items { items } => {
                for item in items {
                    self.rev = if self.live { self.rev.max(item.rev) } else { self.rev };
                    self.unsaved.insert(item.id.clone(), item);
                }
            }
            Message::Synced { rev } => {
                self.live = true;
                self.rev = rev;
            }
            Message::TextDelta { id, text, rev } => {
                if !self.unsaved.contains_key(&id) {
                    let Some(item) = cache.item(thread_id, &id) else { return true };
                    self.unsaved.insert(id.clone(), item);
                }
                let Some(item) = self.unsaved.get_mut(&id) else { return true };
                let ItemKind::Assistant { text: current } = &mut item.kind else { return true };
                current.push_str(&text);
                item.rev = rev;
                self.rev = rev;
            }
            Message::Activity { activity } => {
                self.working = is_working(&activity);
                cache.set_activity(thread_id, &activity);
            }
            Message::Error { .. } => return false,
            other => tracing::debug!("unexpected message on a followed thread: {other:?}"),
        }
        true
    }

    /// Writes what arrived since the last time, in one step.
    pub fn save(&mut self, cache: &Cache, thread_id: &str) {
        let synced = self.live.then_some(self.rev);
        if self.unsaved.is_empty() && synced.is_none_or(|rev| rev == self.saved_rev) {
            return;
        }
        let items: Vec<Item> = self.unsaved.drain().map(|(_, item)| item).collect();
        cache.save_items(thread_id, &items.iter().collect::<Vec<_>>(), synced);
        self.saved_rev = synced.unwrap_or(self.saved_rev);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(id: &str, seq: u64, rev: u64, text: &str) -> Item {
        let kind = ItemKind::Assistant { text: text.into() };
        Item { id: id.into(), seq, rev, created_at: 0.0, media: Vec::new(), parent: None, kind }
    }

    fn working() -> Activity {
        Activity { running: true, ..Activity::default() }
    }

    fn cached(cache: &Cache, thread_id: &str) -> Vec<(String, String)> {
        let text = |item: Item| match item.kind {
            ItemKind::Assistant { text } => (item.id, text),
            _ => (item.id, String::new()),
        };
        cache.page(thread_id, None).items.into_iter().map(text).collect()
    }

    fn thread_cache() -> (tempfile::TempDir, Cache) {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(&dir.path().join("cache.sqlite")).unwrap();
        let thread: Thread = serde_json::from_str(
            r#"{"id": "t", "title": "T", "project_id": "p", "cwd": "/srv", "agent": "claude", "model": null,
            "effort": null, "access": "full", "plan": false, "created_at": 1.0, "updated_at": 2.0, "done_at": null,
            "undone_at": null, "running": true, "needs_approval": false, "turn_ended_at": null, "rev": 3}"#,
        )
        .unwrap();
        cache.set_threads("s", &[thread]);
        (dir, cache)
    }

    #[test]
    fn a_followed_thread_reaches_the_cache_with_its_streamed_text_once_saved() {
        let (_dir, cache) = thread_cache();
        let mut followed = Followed::new("s".into(), 0, false, true);
        followed.take(&cache, "t", Message::Opened { reset: false, activity: working() });
        followed.take(&cache, "t", Message::Items { items: vec![reply("a", 0, 4, "Hel")] });
        followed.take(&cache, "t", Message::Synced { rev: 4 });
        followed.take(&cache, "t", Message::TextDelta { id: "a".into(), text: "lo".into(), rev: 5 });
        assert!(cached(&cache, "t").is_empty());

        followed.save(&cache, "t");
        assert_eq!(cached(&cache, "t"), [("a".to_string(), "Hello".to_string())]);
        assert_eq!(cache.synced_rev("t"), 5);

        followed.take(&cache, "t", Message::TextDelta { id: "a".into(), text: "!".into(), rev: 6 });
        followed.save(&cache, "t");
        assert_eq!(cached(&cache, "t"), [("a".to_string(), "Hello!".to_string())]);
        assert_eq!(cache.synced_rev("t"), 6);
    }

    #[test]
    fn a_catch_up_cut_short_records_no_revision() {
        let (_dir, cache) = thread_cache();
        let mut followed = Followed::new("s".into(), 0, false, true);
        followed.take(&cache, "t", Message::Opened { reset: false, activity: working() });
        followed.take(&cache, "t", Message::Items { items: vec![reply("a", 0, 9, "Hi")] });
        followed.save(&cache, "t");

        assert_eq!(cached(&cache, "t"), [("a".to_string(), "Hi".to_string())]);
        assert_eq!(cache.synced_rev("t"), 0);
    }

    #[test]
    fn a_thread_is_done_once_caught_up_with_its_agent_idle() {
        let (_dir, cache) = thread_cache();
        let mut followed = Followed::new("s".into(), 0, false, true);
        followed.take(&cache, "t", Message::Opened { reset: false, activity: Activity::default() });
        assert!(!followed.is_done());

        followed.take(&cache, "t", Message::Synced { rev: 2 });
        assert!(followed.is_done());

        followed.take(&cache, "t", Message::Activity { activity: working() });
        assert!(!followed.is_done());
    }
}
