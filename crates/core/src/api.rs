//! What a client says to the core and what the core tells it, as JSON.

use std::collections::HashMap;
use std::path::PathBuf;

use motile_protocol::auth_api::User;
use motile_protocol::wire::{
    Activity, DiffScope, GitAction, GitStage, MergeMethod, NewThread, Project, PullRequestAction, PullRequestEdit,
    PullRequestSettings, PullRequestState, Request, ServerInfo, Thread,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::connection::PathKind;
use crate::git::Control;
use crate::link::State;
use crate::render::agents::AgentView;
use crate::render::highlight::Spans;
use crate::render::rows::{Row, Waiting};

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    /// Where the device key and the cache are kept.
    pub data_dir: PathBuf,
    pub auth_url: String,
    /// How this device appears in the account.
    pub device_name: String,
    pub platform: String,
    /// No relays and no address lookup; every server is dialed at `direct_addr`. For tests and demos.
    #[serde(default)]
    pub local_only: bool,
    #[serde(default)]
    pub direct_addr: Option<String>,
    /// How many bytes the fetched images and videos may take on this device. Two gigabytes
    /// without it.
    #[serde(default)]
    pub media_limit: Option<u64>,
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
    /// The client is in front again: what waits to dial again dials now, and if the system had
    /// the client paused in the background, every server is dialed again.
    Foreground,
    /// The device changed networks: what waits to dial again dials now.
    NetworkChanged,
    /// Asks the auth server about the account every couple of seconds, while waiting for a server.
    WatchServers {
        on: bool,
    },
    /// Answers with `token`, `command` and `expires_at`.
    CreateEnrollToken,
    RemoveServer {
        server_id: String,
    },
    /// Sends the thread's rows from the cache, then keeps them current.
    OpenThread {
        server_id: String,
        thread_id: String,
    },
    CloseThread {
        thread_id: String,
    },
    MarkSeen {
        thread_id: String,
    },
    /// Any request to a server; answers with the server's message.
    Request {
        server_id: String,
        request: Request,
    },
    /// The folders on the server under the path typed in `query`, which starts at `/` or `~/`:
    /// those of its directory whose names start with what follows the last slash. Answers with
    /// a `browse::Listing`.
    Browse {
        server_id: String,
        query: String,
    },
    /// Sends the message with `attachments`, the paths `upload` answered with. Answers with
    /// `thread_id`. With `now`, a turn that runs takes the message at once instead of the next
    /// turn starting with it.
    Send {
        server_id: String,
        thread_id: Option<String>,
        new_thread: Option<NewThread>,
        text: String,
        #[serde(default)]
        attachments: Vec<String>,
        #[serde(default)]
        now: bool,
    },
    /// Sends a file of this device to the server, for a message to be sent with. Answers with
    /// its `path` there, and for an image or a video with `media`, the name the `media` command
    /// finds it under; `upload_progress` events with `key` say how far it is. With
    /// `poster_of`, the path of a video on the server, the file is the image that stands for it.
    Upload {
        server_id: String,
        key: String,
        file: String,
        #[serde(default)]
        poster_of: Option<String>,
    },
    /// Stops the upload, which then answers with an error.
    CancelUpload {
        key: String,
    },
    /// Has the server install the latest release and restart. `server_update` events say how far the
    /// download is; the answer comes when the server is about to restart.
    UpdateServer {
        server_id: String,
    },
    /// Commits, pushes, opens a pull request or pulls in the project's folder, or in the worktree
    /// of the thread; the server writes
    /// the commit message that isn't given and the pull request. `git_progress` events say which
    /// stage runs. Answers with `title`, `description`, the pull request's `url` and the `next`
    /// action, where there is one.
    GitRun {
        server_id: String,
        project_id: String,
        action: GitAction,
        #[serde(default)]
        thread_id: Option<String>,
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        paths: Vec<String>,
        #[serde(default)]
        new_branch: bool,
    },
    /// The changes in the folder the thread works in, or in the project's folder, read for
    /// drawing. Answers with the `files`, each a `render::diff::FileDiff`, and `truncated` when
    /// the server cut them short. A `code_spans` event follows for each file's highlighting.
    /// With a `path`, only that file's changes.
    Diff {
        server_id: String,
        project_id: String,
        #[serde(default)]
        thread_id: Option<String>,
        scope: DiffScope,
        #[serde(default)]
        path: Option<String>,
    },
    /// The pull request with that number, read in the folder the thread works in or the
    /// project's, as the tab shows it: a `pull_request::View`, merging with `method` when the
    /// repository allows it.
    PullRequest {
        server_id: String,
        project_id: String,
        #[serde(default)]
        thread_id: Option<String>,
        number: u64,
        #[serde(default)]
        method: Option<MergeMethod>,
    },
    /// Does `action` to the pull request. Answers with the `title` of what it did, the `url` of a
    /// pull request it opened, and the pull request's `view` afterwards.
    PullRequestAction {
        server_id: String,
        project_id: String,
        #[serde(default)]
        thread_id: Option<String>,
        number: u64,
        action: PullRequestAction,
        #[serde(default)]
        method: Option<MergeMethod>,
        #[serde(default)]
        text: Option<String>,
    },
    /// Changes the pull request as `edit` says. Answers with the `title` of what it did and the
    /// pull request's `view` afterwards.
    PullRequestEdit {
        server_id: String,
        project_id: String,
        #[serde(default)]
        thread_id: Option<String>,
        number: u64,
        edit: PullRequestEdit,
        #[serde(default)]
        method: Option<MergeMethod>,
    },
    /// The repository's pull requests in that state, as the list shows them: `rows`, each a
    /// `pull_request::Row`.
    PullRequests {
        server_id: String,
        project_id: String,
        #[serde(default)]
        thread_id: Option<String>,
        state: PullRequestState,
    },
    /// The issues of a Linear workspace the server is connected to, as the list shows them:
    /// `groups`, each a `linear::Group`. They are of one team or of all, assigned to the user or
    /// to anyone, and with `closed` also the completed and cancelled ones, or with `search` the
    /// ones that have the words.
    LinearIssues {
        server_id: String,
        workspace: String,
        #[serde(default)]
        team: Option<String>,
        mine: bool,
        closed: bool,
        #[serde(default)]
        search: Option<String>,
    },
    /// One issue as its tab shows it: `page`, a `linear::Page`. With `comment` that is said on
    /// the issue first.
    LinearIssue {
        server_id: String,
        workspace: String,
        issue: String,
        #[serde(default)]
        comment: Option<String>,
    },
    /// What the agents spent on every connected server in the last `buckets` spans of
    /// `bucket_secs`, on a clock `utc_offset_secs` ahead of UTC: a `usage::View`.
    Usage {
        bucket_secs: u32,
        buckets: u32,
        utc_offset_secs: i32,
    },
    /// Markdown set for drawing, as `blocks`: what a description looks like before it is saved.
    Markdown {
        text: String,
    },
    /// The prompt that hands a line of a pull request and the user's note on it to the agent.
    /// Answers with `prompt`.
    LinePrompt {
        number: u64,
        url: String,
        head: String,
        path: String,
        line: u32,
        code: String,
        #[serde(default)]
        note: String,
    },
    /// The file at `path` in that folder. Answers with its `kind` and `size`, and for a text
    /// with its `lines` and `truncated` when they are only its start, for an image with the
    /// `file` it is in on this device. A `code_spans` event follows with a text's highlighting.
    File {
        server_id: String,
        project_id: String,
        #[serde(default)]
        thread_id: Option<String>,
        path: String,
    },
    /// Picks the model that writes titles, commit messages and pull requests on the server.
    /// Without `model` the lightest model of the thread's agent writes.
    SetTextModel {
        server_id: String,
        #[serde(default)]
        model: Option<String>,
    },
    /// What the server does with pull requests by itself: marking threads done and removing
    /// worktrees once their pull requests merge.
    SetPullRequestSettings {
        server_id: String,
        settings: PullRequestSettings,
    },
    /// Says how the server's writer names the branches it makes. Without `instructions` the
    /// server goes back to its own.
    SetBranchInstructions {
        server_id: String,
        #[serde(default)]
        instructions: Option<String>,
    },
    /// Makes the image at `path` on the server the project's icon. Without `path` the project
    /// goes back to the icon found in its folder.
    SetProjectIcon {
        server_id: String,
        project_id: String,
        #[serde(default)]
        path: Option<String>,
    },
    /// Answers with the `path` of an image or a video on this device, once it is here: one that
    /// isn't is fetched from the server, and `media_progress` events say how far that is.
    Media {
        server_id: String,
        media_id: String,
    },
    /// Answers with `media_bytes`, what the fetched images and videos take on this device, and
    /// `media_limit`, what they may take.
    Storage,
    /// Removes the fetched images and videos. The servers still have them.
    ClearMedia,
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
    /// Sends the rows of what the agent did that the tool call `agent_id` started, as
    /// `agent_rows` events, then keeps them current. One agent of a thread is open at a time.
    OpenAgent {
        thread_id: String,
        agent_id: String,
    },
    CloseAgent {
        thread_id: String,
    },
    /// Asks for the turns before the first row, when the rows said there are some.
    LoadEarlier {
        thread_id: String,
    },
    /// Lets go of the turns before the one that has the first of the last `keep_rows` rows.
    /// The client asks while it shows the end of a thread; the turns come back with `LoadEarlier`.
    TrimEarlier {
        thread_id: String,
        keep_rows: usize,
    },
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Account {
        account: AccountView,
    },
    /// Everything known from last time has been sent: the account, servers, projects and threads.
    Restored,
    Servers {
        servers: Vec<ServerView>,
    },
    /// All of a server's threads, replacing what the client had for it.
    Threads {
        server_id: String,
        threads: Vec<ThreadView>,
    },
    ThreadUpsert {
        thread: ThreadView,
    },
    ThreadDeleted {
        thread_id: String,
    },
    Projects {
        server_id: String,
        projects: Vec<ProjectView>,
    },
    /// How far a server's download of its update is. `total` is missing when it isn't known.
    ServerUpdate {
        server_id: String,
        received: u64,
        total: Option<u64>,
    },
    /// A stage of a `git_run` has started, in the project's folder or in the worktree of the thread.
    GitProgress {
        project_id: String,
        thread_id: Option<String>,
        stage: GitStage,
    },
    /// How much of an image or a video has arrived from its server.
    MediaProgress {
        id: String,
        received: u64,
        size: u64,
    },
    /// How much of a file has been sent to its server.
    UploadProgress {
        key: String,
        sent: u64,
        size: u64,
    },
    /// Replace `remove` rows at `start` with `rows`. With `reset` the client's rows are dropped first.
    /// `earlier` says that the thread has turns before the first row.
    Rows {
        thread_id: String,
        reset: bool,
        start: usize,
        remove: usize,
        rows: Vec<Row>,
        earlier: bool,
    },
    /// Whether the thread has caught up with its server, so that what arrives from now on is new.
    Live {
        thread_id: String,
        live: bool,
    },
    /// The agents the thread's agent has started, in the order it started them.
    Agents {
        thread_id: String,
        agents: Vec<AgentView>,
    },
    /// The same as `rows`, for the transcript of the agent that is open.
    AgentRows {
        thread_id: String,
        agent_id: String,
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
    /// The highlighting of what the command `id` answered with: the spans of each line of its
    /// file number `file`, counted from the line's start.
    CodeSpans {
        id: u64,
        file: usize,
        lines: Vec<Vec<u32>>,
    },
    Activity {
        thread_id: String,
        activity: Activity,
        /// The tool calls the turn waits with, worded for a person.
        waiting: Vec<Waiting>,
    },
    /// The thread couldn't be opened on its server.
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
pub struct ServerView {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub state: State,
    pub error: Option<String>,
    pub path: Option<PathKind>,
    pub rtt_ms: Option<u64>,
    /// From the server itself; from the cache until it has connected.
    pub info: Option<ServerInfo>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ProjectView {
    #[serde(flatten)]
    pub project: Project,
    /// The project's icon as a file on this device, once it has been fetched from the server.
    pub icon_path: Option<String>,
    /// The git button for its repository: what it does and the menu behind it.
    pub git_control: Option<Control>,
    /// The same for each of its worktrees that git has been read in, by the worktree's path.
    pub worktree_controls: HashMap<String, Control>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ThreadView {
    #[serde(flatten)]
    pub thread: Thread,
    pub server_id: String,
    /// A turn ended since the user last looked at the thread on this device.
    pub unread: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_flat_json_objects_tagged_by_type() {
        let json = r#"{"id": 7, "type": "request", "server_id": "h",
            "request": {"type": "update", "thread_id": "t", "change": {"done": true}}}"#;
        let envelope: Envelope = serde_json::from_str(json).unwrap();

        assert_eq!(envelope.id, 7);
        let Command::Request { server_id, request: Request::Update { thread_id, change } } = envelope.command else {
            panic!("expected an update request");
        };
        assert_eq!((server_id.as_str(), thread_id.as_str(), change.done, change.title), ("h", "t", Some(true), None));

        let send: Envelope = serde_json::from_str(
            r#"{"id": 8, "type": "send", "server_id": "h", "thread_id": "t", "new_thread": null, "text": "hi"}"#,
        )
        .unwrap();
        assert!(matches!(send.command, Command::Send { attachments, .. } if attachments.is_empty()));

        let media: Envelope =
            serde_json::from_str(r#"{"id": 9, "type": "media", "server_id": "h", "media_id": "m.png"}"#).unwrap();
        assert!(matches!(media.command, Command::Media { media_id, .. } if media_id == "m.png"));
    }

    #[test]
    fn a_thread_is_sent_flat_with_its_server() {
        let thread: Thread = serde_json::from_value(serde_json::json!({
            "id": "t", "title": "T", "project_id": "p", "cwd": "/srv", "agent": "claude", "model": null,
            "effort": null, "access": "accept_edits", "plan": false, "created_at": 1.0, "updated_at": 2.0,
            "done_at": null, "undone_at": null, "running": true, "needs_approval": false, "turn_ended_at": null, "rev": 3,
        }))
        .unwrap();
        let event = Event::ThreadUpsert { thread: ThreadView { thread, server_id: "h".into(), unread: false } };
        let json = serde_json::to_value(event).unwrap();

        assert_eq!(json["type"], "thread_upsert");
        assert_eq!(json["thread"]["server_id"], "h");
        assert_eq!(json["thread"]["access"], "accept_edits");
        assert_eq!(json["thread"]["running"], true);
    }
}
