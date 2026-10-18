//! What a client says to the core and what the core tells it, as JSON.

use std::collections::HashMap;
use std::path::PathBuf;

use motile_protocol::auth_api::User;
use motile_protocol::wire::{
    Activity, ContinueSettings, DiffScope, GitAction, GitStage, LinearStateKind, MergeMethod, NewThread, Project,
    PullRequestAction, PullRequestEdit, PullRequestSettings, PullRequestState, Request, RestartWhen, RicracStateKind,
    ServerInfo, Thread,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::connection::PathKind;
use crate::git::Control;
use crate::keybindings::{Rule, View as KeybindingsView};
use crate::link::State;
use crate::render::agents::AgentView;
use crate::render::highlight::Spans;
use crate::render::rows::{Row, Waiting};
use crate::search::MatchView;

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
    /// The address each server is dialed at, by its key, in place of `direct_addr`.
    #[serde(default)]
    pub direct_addrs: HashMap<String, String>,
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
    /// Removes the server from the account. Its threads stay readable from the copies the
    /// account's other servers keep, and it shows as `removed`.
    RemoveServer {
        server_id: String,
    },
    /// Removes the server if it is still linked and drops every copy of its threads, on every
    /// server and client.
    DeleteServerCompletely {
        server_id: String,
    },
    /// Whether the device is on a network that costs by the byte, where a server's copy of
    /// another isn't started from the beginning.
    SetMetered {
        metered: bool,
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
    /// those of its directory whose names start with what follows the last slash, and with
    /// `icons` the images there that can be a project's icon. Answers with a `browse::Listing`.
    Browse {
        server_id: String,
        query: String,
        #[serde(default)]
        icons: bool,
    },
    /// The skills of the agent's `account` in the project that fit the word that ends
    /// `before_cursor`, the composer's text up to its cursor, when it starts with `$` or `/`.
    /// Answers with a `skills::Menu`, without rows when no skill fits.
    SkillMenu {
        server_id: String,
        project_id: String,
        account: String,
        before_cursor: String,
    },
    /// Has the skills of the project's agents listed ahead of the composer's menu, which then
    /// opens with them at once. Answers once they are.
    PrefetchSkills {
        server_id: String,
        project_id: String,
    },
    /// The `$name`s of `text` that are skills of the agent's `account` in the project. Answers
    /// with them as `chips`, each a `SkillChip`.
    SkillChips {
        server_id: String,
        project_id: String,
        account: String,
        text: String,
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
    /// Has the server install the latest release and restart: `when` its agents work, once they
    /// have finished or at once. `server_update` events say how far the download is, and that the
    /// server waits for its agents; the answer comes when the server is about to restart.
    UpdateServer {
        server_id: String,
        #[serde(default)]
        when: Option<RestartWhen>,
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
    /// to anyone, and in a status of one of the kinds in `states` (without them `closed` says
    /// whether the completed and cancelled ones are among them), or with `search` the ones that
    /// have the words.
    LinearIssues {
        server_id: String,
        workspace: String,
        #[serde(default)]
        team: Option<String>,
        mine: bool,
        closed: bool,
        #[serde(default)]
        states: Vec<LinearStateKind>,
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
    /// The issues of a Ricrac workspace the server is connected to, as the list shows them:
    /// `groups`, each a `ricrac::Group`, chosen as `LinearIssues` chooses them.
    RicracIssues {
        server_id: String,
        workspace: String,
        #[serde(default)]
        team: Option<String>,
        mine: bool,
        closed: bool,
        #[serde(default)]
        states: Vec<RicracStateKind>,
        #[serde(default)]
        search: Option<String>,
    },
    /// One Ricrac issue as its tab shows it: `page`, a `ricrac::Page`. With `comment` that is
    /// said on the issue first.
    RicracIssue {
        server_id: String,
        workspace: String,
        issue: String,
        #[serde(default)]
        comment: Option<String>,
    },
    /// What the agents spent on the connected `servers`, or all of them, in the last `buckets`
    /// spans of `bucket_secs`, on a clock `utc_offset_secs` ahead of UTC: a `usage::View`. `kept`
    /// answers at once with what was last read.
    Usage {
        bucket_secs: u32,
        buckets: u32,
        utc_offset_secs: i32,
        #[serde(default)]
        servers: Option<Vec<String>>,
        #[serde(default)]
        kept: bool,
    },
    /// How much of their plans the agents' logins on the connected `servers`, or all of them,
    /// have used, read anew with `refresh`: `sections`, each a `limits::Section`. `kept` answers
    /// at once with what was last read, `stale` when some of it is old or missing.
    Limits {
        #[serde(default)]
        refresh: bool,
        #[serde(default)]
        kept: bool,
        #[serde(default)]
        servers: Option<Vec<String>>,
    },
    /// Searches the messages of every connected server's threads for `query`, a little after the
    /// last time it changed. `thread_search` events tell what was found; there is no reply.
    SearchThreads {
        query: String,
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
    /// The file at `path` in that folder, or with `blob` the image or the video a diff calls so.
    /// Answers with its `kind` and `size`, and for a text with its `lines` and `truncated` when
    /// they are only its start, for an image or a video with the `file` it is in on this device.
    /// A `code_spans` event follows with a text's highlighting.
    File {
        server_id: String,
        project_id: String,
        #[serde(default)]
        thread_id: Option<String>,
        path: String,
        #[serde(default)]
        blob: Option<String>,
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
    /// What the server goes on with by itself: threads that reached their usage limit, and
    /// threads whose agents worked when it restarted.
    SetContinueSettings {
        server_id: String,
        settings: ContinueSettings,
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
    /// Answers with the `path` of an image, a video or a file an agent sent on this device, once it
    /// is here: one that isn't is fetched from the server, and `media_progress` events say how far
    /// that is.
    Media {
        server_id: String,
        media_id: String,
    },
    /// Stops fetching the image, the video or the file. The `media` commands waiting for it answer
    /// with an error.
    CancelMedia {
        media_id: String,
    },
    /// Answers with `media_bytes`, what the fetched images, videos and files take on this device,
    /// and `media_limit`, what they may take.
    Storage,
    /// Removes the fetched images, videos and files. The servers still have them.
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
    /// Reads `keybindings.json` again, and sends `keybindings` if it changed.
    ReloadKeybindings,
    /// Gives the command the key, in place of `replace` when that is one of its rules.
    SetKeybinding {
        command: String,
        key: String,
        #[serde(default)]
        when: Option<String>,
        #[serde(default)]
        replace: Option<Rule>,
    },
    RemoveKeybinding {
        rule: Rule,
    },
    /// Gives the command its default keys back.
    ResetKeybinding {
        command: String,
    },
    /// Answers with how a key being chosen reads (`caps`), the commands it clashes with
    /// (`conflicts`), the condition's unknown names (`unknown`) and what is wrong with it
    /// (`when_error`). `row` is the rule it takes the place of.
    CheckKeybinding {
        key: String,
        #[serde(default)]
        when: Option<String>,
        #[serde(default)]
        row: Option<Rule>,
    },
    /// Answers with the `path` of `keybindings.json`, made if there was none.
    KeybindingsFile,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// The threads whose messages have `query` in them, as far as the servers have answered:
    /// `searching` while some still may. What was found for a shorter search stays while it fits.
    ThreadSearch {
        query: String,
        searching: bool,
        matches: Vec<MatchView>,
    },
    /// The keyboard shortcuts, at start and whenever they change.
    Keybindings {
        keybindings: KeybindingsView,
    },
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
        thread: Box<ThreadView>,
    },
    ThreadDeleted {
        thread_id: String,
    },
    Projects {
        server_id: String,
        projects: Vec<ProjectView>,
    },
    /// How far a server's download of its update is. `total` is missing when it isn't known. With
    /// `waiting` the update is installed and the server restarts once its agents have finished.
    ServerUpdate {
        server_id: String,
        received: u64,
        total: Option<u64>,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        waiting: bool,
    },
    /// A stage of a `git_run` has started, in the project's folder or in the worktree of the thread.
    GitProgress {
        project_id: String,
        thread_id: Option<String>,
        stage: GitStage,
    },
    /// How much of an image, a video or a file an agent sent has arrived from its server.
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
        /// The answer, or `{"error": …, "offline": …}`, `offline` when the server couldn't be reached.
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
    /// The name cut short, for the places a long name crowds.
    pub short_name: String,
    pub platform: String,
    /// The server was removed from the account: its threads are copies, which can be read and
    /// searched and go on only on another server.
    pub removed: bool,
    pub state: State,
    pub error: Option<String>,
    pub path: Option<PathKind>,
    pub rtt_ms: Option<u64>,
    /// From the server itself; from the cache until it has connected.
    pub info: Option<ServerInfo>,
    /// The names of its models that have a shorter one, by model id, for the model picker.
    pub short_model_names: HashMap<String, String>,
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
    /// How the agents its agent started are doing, theirs too, when it started any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Children>,
}

/// The agents a thread's agent started, and theirs, as the thread shows them.
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct Children {
    /// How many of them work.
    pub running: u32,
    /// The agents of the first few that work, the first started first, whose avatars stand side
    /// by side.
    pub working: Vec<motile_protocol::wire::Agent>,
    /// The ones that wait for the user, the one that has waited longest first.
    pub waiting: Vec<WaitingChild>,
    /// What the thread says of those that wait: "Run greet.py needs your approval",
    /// "2 agents need you".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiting_text: Option<String>,
    /// What the thread says while they work: "1 agent working", "3 agents working".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running_text: Option<String>,
    /// What stopping the thread asks first while they work: "Stop 1 agent?", "Stop 3 agents?".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_question: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct WaitingChild {
    pub thread_id: String,
    pub title: String,
    pub ask: motile_protocol::wire::Ask,
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
            "effort": null, "access": "accept_edits", "created_at": 1.0, "updated_at": 2.0,
            "done_at": null, "undone_at": null, "running": true, "needs_approval": false, "turn_ended_at": null, "rev": 3,
        }))
        .unwrap();
        let event = Event::ThreadUpsert {
            thread: Box::new(ThreadView { thread, server_id: "h".into(), unread: false, children: None }),
        };
        let json = serde_json::to_value(event).unwrap();

        assert_eq!(json["type"], "thread_upsert");
        assert_eq!(json["thread"]["server_id"], "h");
        assert_eq!(json["thread"]["access"], "accept_edits");
        assert_eq!(json["thread"]["running"], true);
    }
}
