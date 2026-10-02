//! What an app says to the core and what the core tells it, as JSON.

use std::path::PathBuf;

use motile_protocol::auth_api::User;
use motile_protocol::wire::{Activity, HostInfo, NewThread, Project, Request, Thread};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::connection::PathKind;
use crate::link::State;
use crate::render::highlight::Spans;
use crate::render::rows::Row;

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    /// Where the device key and the cache are kept.
    pub data_dir: PathBuf,
    pub auth_url: String,
    /// How this device appears in the account.
    pub device_name: String,
    pub platform: String,
    /// No relays and no address lookup; every host is dialed at `direct_addr`. For tests and demos.
    #[serde(default)]
    pub local_only: bool,
    #[serde(default)]
    pub direct_addr: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct Envelope {
    /// Echoed in the `reply` event.
    pub id: u64,
    #[serde(flatten)]
    pub command: Command,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    /// Answers with the `url` to open in a browser.
    BeginSignIn,
    /// Given the `motile://auth?…` address the browser was sent back to.
    CompleteSignIn {
        url: String,
    },
    /// Signs in without Google, on an auth server that allows it.
    DevSignIn {
        email: String,
    },
    SignOut,
    RefreshAccount,
    /// Asks the auth server about the account every couple of seconds, while waiting for a host.
    WatchHosts {
        on: bool,
    },
    /// Answers with `token`, `command` and `expires_at`.
    CreateEnrollToken,
    RemoveHost {
        host_id: String,
    },
    /// Sends the thread's rows from the cache, then keeps them current.
    OpenThread {
        host_id: String,
        thread_id: String,
    },
    CloseThread {
        thread_id: String,
    },
    MarkSeen {
        thread_id: String,
    },
    /// Any request to a host; answers with the host's message.
    Request {
        host_id: String,
        request: Request,
    },
    /// Uploads `files` from this device, then sends the message. Answers with `thread_id`.
    Send {
        host_id: String,
        thread_id: Option<String>,
        new_thread: Option<NewThread>,
        text: String,
        #[serde(default)]
        files: Vec<String>,
    },
    /// Has the host install the latest release and restart. `host_update` events say how far the
    /// download is; the answer comes when the host is about to restart.
    UpdateHost {
        host_id: String,
    },
    /// Makes the image at `path` on the host the project's icon. Without `path` the project
    /// goes back to the icon found in its folder.
    SetProjectIcon {
        host_id: String,
        project_id: String,
        #[serde(default)]
        path: Option<String>,
    },
    /// Asks for the highlighting of the code in rows that came without it.
    Highlight {
        thread_id: String,
        row_ids: Vec<String>,
    },
    /// Opens or closes a group of tool calls or a turn's fold.
    ToggleRow {
        thread_id: String,
        row_id: String,
    },
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Account {
        account: AccountView,
    },
    /// Everything known from last time has been sent: the account, hosts, projects and threads.
    Restored,
    Hosts {
        hosts: Vec<HostView>,
    },
    /// All of a host's threads, replacing what the app had for it.
    Threads {
        host_id: String,
        threads: Vec<ThreadView>,
    },
    ThreadUpsert {
        thread: ThreadView,
    },
    ThreadDeleted {
        thread_id: String,
    },
    Projects {
        host_id: String,
        projects: Vec<ProjectView>,
    },
    /// How far a host's download of its update is. `total` is missing when it isn't known.
    HostUpdate {
        host_id: String,
        received: u64,
        total: Option<u64>,
    },
    /// Replace `remove` rows at `start` with `rows`. With `reset` the app's rows are dropped first.
    Rows {
        thread_id: String,
        reset: bool,
        start: usize,
        remove: usize,
        rows: Vec<Row>,
    },
    Spans {
        thread_id: String,
        row_id: String,
        spans: Spans,
    },
    Activity {
        thread_id: String,
        activity: Activity,
    },
    /// The thread couldn't be opened on its host.
    ThreadError {
        thread_id: String,
        message: String,
    },
    Reply {
        id: u64,
        ok: bool,
        /// The answer, or `{"error": …}`.
        value: Value,
    },
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct AccountView {
    pub signed_in: bool,
    pub user: Option<User>,
    pub device_key: String,
    pub auth_url: String,
    /// Set when the auth server couldn't be reached and this is the last known state.
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct HostView {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub state: State,
    pub error: Option<String>,
    pub path: Option<PathKind>,
    pub rtt_ms: Option<u64>,
    /// From the host itself; from the cache until it has connected.
    pub info: Option<HostInfo>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ProjectView {
    #[serde(flatten)]
    pub project: Project,
    /// The project's icon as a file on this device, once it has been fetched from the host.
    pub icon_path: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ThreadView {
    #[serde(flatten)]
    pub thread: Thread,
    pub host_id: String,
    /// A turn ended since the user last looked at the thread on this device.
    pub unread: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_flat_json_objects_tagged_by_type() {
        let json = r#"{"id": 7, "type": "request", "host_id": "h",
            "request": {"type": "update", "thread_id": "t", "change": {"done": true}}}"#;
        let envelope: Envelope = serde_json::from_str(json).unwrap();

        assert_eq!(envelope.id, 7);
        let Command::Request { host_id, request: Request::Update { thread_id, change } } = envelope.command else {
            panic!("expected an update request");
        };
        assert_eq!((host_id.as_str(), thread_id.as_str(), change.done, change.title), ("h", "t", Some(true), None));

        let send: Envelope = serde_json::from_str(
            r#"{"id": 8, "type": "send", "host_id": "h", "thread_id": "t", "new_thread": null, "text": "hi"}"#,
        )
        .unwrap();
        assert!(matches!(send.command, Command::Send { files, .. } if files.is_empty()));
    }

    #[test]
    fn a_thread_is_sent_flat_with_its_host() {
        let thread: Thread = serde_json::from_value(serde_json::json!({
            "id": "t", "title": "T", "project_id": "p", "cwd": "/srv", "agent": "claude", "model": null,
            "effort": null, "access": "accept_edits", "plan": false, "created_at": 1.0, "updated_at": 2.0,
            "done_at": null, "undone_at": null, "running": true, "needs_approval": false, "turn_ended_at": null, "rev": 3,
        }))
        .unwrap();
        let event = Event::ThreadUpsert { thread: ThreadView { thread, host_id: "h".into(), unread: false } };
        let json = serde_json::to_value(event).unwrap();

        assert_eq!(json["type"], "thread_upsert");
        assert_eq!(json["thread"]["host_id"], "h");
        assert_eq!(json["thread"]["access"], "accept_edits");
        assert_eq!(json["thread"]["running"], true);
    }
}
