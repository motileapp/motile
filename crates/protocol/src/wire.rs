//! Messages between an app and a host. Every stream starts with one `Request` from the app; the
//! host answers with one `Message`, or with a stream of them for `Subscribe` and `Open`.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Agent {
    Claude,
    Codex,
}

/// How much the agent may do without asking.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// Tools that need approval end the turn until the user allows them.
    Supervised,
    AcceptEdits,
    /// The agent decides which routine actions are safe to run without asking.
    Auto,
    #[default]
    Full,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Thread {
    pub id: String,
    pub title: String,
    pub project_id: String,
    pub cwd: String,
    pub agent: Agent,
    /// `None` uses the agent's own default.
    pub model: Option<String>,
    /// Reasoning effort; `None` uses the model's default.
    pub effort: Option<String>,
    pub access: Access,
    /// The agent only reads and proposes; it changes nothing.
    pub plan: bool,
    pub created_at: f64,
    /// When something last happened in the thread; the sidebar sorts by it.
    pub updated_at: f64,
    /// Set while the thread is marked done.
    pub done_at: Option<f64>,
    /// When it last came back from being done; active threads sort by this and `created_at`.
    pub undone_at: Option<f64>,
    pub running: bool,
    /// The last turn ended asking for permission.
    pub needs_approval: bool,
    /// When the last turn ended, for telling the user about replies they haven't seen.
    pub turn_ended_at: Option<f64>,
    /// The transcript's revision; an app whose copy is older has catching up to do.
    pub rev: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Item {
    pub id: String,
    /// Position in the transcript.
    pub seq: u64,
    /// The transcript revision that last changed the item.
    pub rev: u64,
    pub created_at: f64,
    #[serde(flatten)]
    pub kind: ItemKind,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ItemKind {
    User { text: String, attachments: Vec<String> },
    Assistant { text: String },
    Thinking { text: String },
    Tool { call: ToolCall },
    TurnEnd { summary: TurnSummary },
    Error { message: String },
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Running,
    Succeeded,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// The tool's input as JSON text.
    pub input: String,
    pub output: Option<String>,
    pub status: ToolStatus,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct TurnSummary {
    pub duration_ms: Option<u64>,
    pub cost_usd: Option<f64>,
    pub is_error: bool,
    /// The user stopped the turn.
    #[serde(default)]
    pub stopped: bool,
    pub denials: Vec<Denial>,
}

/// A tool call Claude Code refused because nobody had allowed it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Denial {
    pub tool_name: String,
    pub tool_use_id: String,
    pub input: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct NewThread {
    pub project_id: String,
    pub agent: Agent,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub access: Access,
    pub plan: bool,
}

/// Settings of a thread to change; `None` leaves one as it is.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct ThreadChange {
    pub title: Option<String>,
    /// An empty model means the agent's default.
    pub model: Option<String>,
    /// An empty effort means the model's default.
    pub effort: Option<String>,
    pub access: Option<Access>,
    pub plan: Option<bool>,
    pub done: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Host details, the threads and the projects, then every change to them.
    Subscribe,
    /// The transcript items changed after revision `since`, then every change as it happens.
    Open {
        thread_id: String,
        since: u64,
    },
    /// Starts a turn, in `thread_id` or in a thread created from `new_thread`. While a turn is
    /// running the message waits and starts the next one.
    Send {
        thread_id: Option<String>,
        new_thread: Option<NewThread>,
        text: String,
        attachments: Vec<String>,
    },
    /// Continues after a turn that ended with denials, with exactly those calls allowed.
    Allow {
        thread_id: String,
        denials: Vec<Denial>,
    },
    Stop {
        thread_id: String,
    },
    Update {
        thread_id: String,
        change: ThreadChange,
    },
    Delete {
        thread_id: String,
    },
    AddProject {
        path: String,
    },
    /// Takes the folder off the list. Its threads stay.
    RemoveProject {
        project_id: String,
    },
    /// The bytes of the project's icon.
    ProjectIcon {
        project_id: String,
    },
    /// Makes the image at `path` on the host the project's icon. `None` goes back to the one
    /// found in the project's folder.
    SetProjectIcon {
        project_id: String,
        path: Option<String>,
    },
    /// Folders inside `path`, or inside the home folder.
    ListDir {
        path: Option<String>,
    },
    /// The file's bytes follow on the same stream.
    Upload {
        name: String,
        size: u64,
    },
    /// Replaces the host's program with the latest release and starts it again. The host answers
    /// with `Updating` while it downloads, then `Ok` just before it restarts.
    UpdateHost,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct AgentInfo {
    pub agent: Agent,
    /// `None` when the agent's CLI isn't installed on the host.
    pub version: Option<String>,
}

/// A folder on the host that threads work in.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Project {
    pub id: String,
    pub path: String,
    pub name: String,
    /// The git branch checked out there, if it is a repository.
    pub branch: Option<String>,
    /// Names the icon's contents: it changes when the icon does, and ends in the file's
    /// extension. `None` when the project has no icon.
    #[serde(default)]
    pub icon: Option<String>,
    pub created_at: f64,
}

/// A model an agent on the host can run, and the choices it offers.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub agent: Agent,
    /// Reasoning efforts it accepts, weakest first. Empty when it has no such setting.
    pub efforts: Vec<String>,
    pub default_effort: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct HostInfo {
    pub version: String,
    pub protocol: u32,
    pub hostname: String,
    pub home: String,
    pub agents: Vec<AgentInfo>,
    /// The models of the installed agents, in the order to offer them.
    pub models: Vec<ModelInfo>,
}

/// What a thread's agent is doing right now.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub struct Activity {
    pub running: bool,
    pub thinking: bool,
    pub started_at: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Welcome {
        host: HostInfo,
        threads: Vec<Thread>,
        projects: Vec<Project>,
    },
    /// The projects, whenever one is added or removed or a branch changes.
    Projects {
        projects: Vec<Project>,
    },
    ThreadUpsert {
        thread: Thread,
    },
    ThreadDeleted {
        thread_id: String,
    },

    /// First answer to `Open`. With `reset` the app's copy can't be caught up and is thrown
    /// away; every item follows.
    Opened {
        reset: bool,
        activity: Activity,
    },
    /// Adds the items, or replaces the ones with the same ids.
    Items {
        items: Vec<Item>,
    },
    /// Everything up to `rev` has been sent; what follows is live.
    Synced {
        rev: u64,
    },
    /// Appends to an assistant item's text while it streams.
    TextDelta {
        id: String,
        text: String,
        rev: u64,
    },
    Activity {
        activity: Activity,
    },

    Ok,
    Sent {
        thread_id: String,
    },
    Dir {
        path: String,
        parent: Option<String>,
        folders: Vec<String>,
    },
    Uploaded {
        path: String,
    },
    /// How far the download of the host's update is. `total` is missing when it isn't known.
    Updating {
        received: u64,
        total: Option<u64>,
    },
    /// A project's icon: the file's bytes in base64.
    Icon {
        data: String,
    },
    Error {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_are_flat_tagged_objects() {
        let item =
            Item { id: "a".into(), seq: 3, rev: 7, created_at: 1.0, kind: ItemKind::Assistant { text: "hi".into() } };
        let json = serde_json::to_value(&item).unwrap();

        assert_eq!(
            json,
            serde_json::json!({"id": "a", "seq": 3, "rev": 7, "created_at": 1.0, "type": "assistant", "text": "hi"})
        );
        assert_eq!(serde_json::from_value::<Item>(json).unwrap(), item);
    }
}
