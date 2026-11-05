//! The messages on their way to their servers. Each is kept in the cache before it is sent and
//! until its server has it, so that neither a lost connection nor a restart loses it, and carries
//! an id the server knows it by, so that sending it again never sends it twice.

use motile_protocol::wire::{NewThread, Request};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cache::Cache;

const KEY: &str = "outbox";
/// The first protocol whose servers know a message sent again by its id.
const MESSAGE_ID_PROTOCOL: u32 = 37;
/// The first protocol whose servers know a new thread's first message sent again by the thread's id.
const THREAD_ID_PROTOCOL: u32 = 36;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Outgoing {
    pub id: String,
    pub server_id: String,
    /// The thread it goes to. A new thread's first message gives the id of the draft it was
    /// written in, which the thread takes.
    pub thread_id: String,
    pub new_thread: Option<NewThread>,
    pub text: String,
    pub attachments: Vec<String>,
    pub now: bool,
    /// What the client shows of it, as the client gave it.
    pub shown: Value,
    pub sent_at: f64,
    pub tries: u32,
    /// Why it hasn't reached its server yet.
    pub error: Option<String>,
    #[serde(skip)]
    pub on_the_way: bool,
}

impl Outgoing {
    pub fn request(&self) -> Request {
        Request::Send {
            thread_id: self.new_thread.is_none().then(|| self.thread_id.clone()),
            new_thread: self.new_thread.clone(),
            text: self.text.clone(),
            attachments: self.attachments.clone(),
            now: self.now,
            context: None,
            message_id: Some(self.id.clone()),
        }
    }

    /// Whether a server of `protocol` takes it again without starting it twice, for when it may
    /// have reached the server already.
    pub fn can_send_again(&self, protocol: u32) -> bool {
        self.tries == 0
            || protocol >= MESSAGE_ID_PROTOCOL
            || (self.new_thread.is_some() && protocol >= THREAD_ID_PROTOCOL)
    }
}

#[derive(Default)]
pub struct Outbox {
    messages: Vec<Outgoing>,
}

impl Outbox {
    pub fn load(cache: &Cache) -> Self {
        Self { messages: cache.kept(KEY).unwrap_or_default() }
    }

    fn save(&self, cache: &Cache) {
        cache.keep(KEY, &self.messages);
    }

    pub fn messages(&self) -> &[Outgoing] {
        &self.messages
    }

    pub fn add(&mut self, cache: &Cache, message: Outgoing) {
        self.messages.push(message);
        self.save(cache);
    }

    pub fn remove(&mut self, cache: &Cache, id: &str) -> Option<Outgoing> {
        let index = self.messages.iter().position(|message| message.id == id)?;
        let message = self.messages.remove(index);
        self.save(cache);
        Some(message)
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// The servers whose next message can go now: none of theirs is on its way, since a thread's
    /// messages go in the order they were written.
    pub fn waiting_servers(&self) -> Vec<String> {
        let mut servers: Vec<String> = Vec::new();
        for message in &self.messages {
            if !servers.contains(&message.server_id) {
                servers.push(message.server_id.clone());
            }
        }
        servers
            .retain(|server| !self.messages.iter().any(|message| &message.server_id == server && message.on_the_way));
        servers
    }

    pub fn next(&self, server_id: &str) -> Option<&Outgoing> {
        self.messages.iter().find(|message| message.server_id == server_id)
    }

    /// Marks the message on its way, one more try.
    pub fn start(&mut self, cache: &Cache, id: &str) -> Option<Outgoing> {
        let message = self.messages.iter_mut().find(|message| message.id == id)?;
        message.on_the_way = true;
        message.tries += 1;
        let started = message.clone();
        self.save(cache);
        Some(started)
    }

    /// The message didn't reach its server, for `error`; it waits to be sent again.
    pub fn failed(&mut self, cache: &Cache, id: &str, error: String) {
        let Some(message) = self.messages.iter_mut().find(|message| message.id == id) else { return };
        message.on_the_way = false;
        message.error = Some(error);
        self.save(cache);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str, server_id: &str) -> Outgoing {
        Outgoing {
            id: id.to_string(),
            server_id: server_id.to_string(),
            thread_id: "thread".to_string(),
            new_thread: None,
            text: "Hello".to_string(),
            attachments: Vec::new(),
            now: false,
            shown: Value::Null,
            sent_at: 0.0,
            tries: 0,
            error: None,
            on_the_way: false,
        }
    }

    #[test]
    fn messages_survive_a_restart_and_go_one_at_a_time_per_server() {
        let folder = tempfile::tempdir().unwrap();
        let cache = Cache::open(&folder.path().join("cache.db")).unwrap();
        let mut outbox = Outbox::load(&cache);
        outbox.add(&cache, message("a", "studio"));
        outbox.add(&cache, message("b", "studio"));
        outbox.add(&cache, message("c", "laptop"));
        outbox.start(&cache, "a");

        let restarted = Outbox::load(&cache);

        assert_eq!(restarted.messages().iter().map(|message| message.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(restarted.messages()[0].tries, 1);
        assert_eq!(restarted.waiting_servers(), ["studio", "laptop"]);
        assert_eq!(outbox.waiting_servers(), ["laptop"]);
    }

    #[test]
    fn a_message_that_may_have_arrived_goes_again_only_to_a_server_that_knows_it() {
        let mut tried = message("a", "studio");
        tried.tries = 1;
        let mut first = tried.clone();
        first.new_thread = Some(
            serde_json::from_value(
                serde_json::json!({ "project_id": "p", "agent": "claude", "model": "m", "access": "full", "id": "draft" }),
            )
            .unwrap(),
        );

        assert!(message("a", "studio").can_send_again(30));
        assert!(!tried.can_send_again(36));
        assert!(tried.can_send_again(37));
        assert!(first.can_send_again(36));
        assert!(!first.can_send_again(35));
    }
}
