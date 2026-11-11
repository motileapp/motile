//! What a client says to the core and what the core tells it, as JSON.

use std::collections::HashMap;
use std::path::PathBuf;

use motile_protocol::auth_api::User;
use motile_protocol::gateway_wire::{AccountChange, Capture, McpChange};
use motile_protocol::wire::{
    Activity, Agent, Bucket, ClientForm, ClientInfo, ClientKind, ContinueSettings, DiffScope, GitAction, GitStage,
    LinearStateKind, MergeMethod, NewThread, Project, PullRequestAction, PullRequestEdit, PullRequestSettings,
    PullRequestState, Request, RestartWhen, RicracStateKind, ServerInfo, Thread,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::connection::PathKind;
use crate::find::FoundRow;
use crate::gateway;
use crate::git::Control;
use crate::keybindings::{Rule, View as KeybindingsView};
use crate::limits;
use crate::link::State;
use crate::outbox::Outgoing;
use crate::render::agents::AgentView;
use crate::render::highlight::Spans;
use crate::render::openui::UiOp;
use crate::render::openui::host::UiEvent;
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
    /// Which client this is, which every server is told once it connects.
    #[serde(default)]
    pub client: Option<ClientDescription>,
}

/// Which client this is and where it runs. The core adds its version.
#[derive(Deserialize, Clone, Debug)]
pub struct ClientDescription {
    pub kind: ClientKind,
    /// `macos`, `ios`, `ipados`, `android`, `linux` or `windows`.
    pub system: String,
    pub form: ClientForm,
}

impl ClientDescription {
    pub fn info(&self) -> ClientInfo {
        ClientInfo {
            kind: self.kind,
            system: self.system.clone(),
            form: self.form,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Where finding in a thread starts: the first row that shows below the client's bars, and how
/// far into its text it shows, in UTF-16 units: the first character of its first whole line there.
/// A long reply is a single row, so its first match can be far above what the client shows.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct FindFrom {
    pub row_id: String,
    #[serde(default)]
    pub offset: u32,
}

/// A place in a transcript: the row at the top of the viewport, and how far the viewport's top is
/// below the row's.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ScrollAt {
    pub row_id: String,
    pub delta: f64,
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
    /// The client went to the background.
    Background,
    /// The client is in front again: what waits to dial again dials now, and if the system had
    /// the client paused, or it was in the background for a while, every server is dialed again.
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
    /// those of its directory whose names start with what follows the last slash. Answers with a
    /// `browse::Listing`.
    Browse {
        server_id: String,
        query: String,
    },
    /// The images in the project's folder that can be its icon and fit `query`, the best first.
    /// Answers with `images`, each a `browse::Image`.
    ProjectImages {
        server_id: String,
        project_id: String,
        query: String,
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
    /// Whether a message of `text` is shown cut, for the copy shown while it travels. Answers with
    /// `collapsible`.
    Collapses {
        text: String,
    },
    /// Sends the message with `attachments`, the paths `upload` answered with. Answers with
    /// `thread_id`. With `now`, a turn that runs takes the message at once instead of the next
    /// turn starting with it.
    /// Keeps the message in the outbox and answers with its `message_id` at once; the core sends
    /// it as soon as its server is connected, and again after a lost connection or a restart, until
    /// its server has it. `sent` and `unsent` say how it went, `outbox` what still waits.
    Send {
        server_id: String,
        thread_id: Option<String>,
        new_thread: Option<NewThread>,
        text: String,
        #[serde(default)]
        attachments: Vec<String>,
        #[serde(default)]
        now: bool,
        /// What the client shows of the message while it travels, kept with it as given.
        #[serde(default)]
        shown: Value,
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
    /// What was done on the connected `servers`, or all of them, in the last `buckets` spans of
    /// `bucket_secs`, on a clock `utc_offset_secs` ahead of UTC: an `activity::View`. `kept`
    /// answers at once with what was last read.
    Activity {
        bucket_secs: u32,
        buckets: u32,
        utc_offset_secs: i32,
        #[serde(default)]
        servers: Option<Vec<String>>,
        #[serde(default)]
        kept: bool,
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
    /// How much of their plans the agents' accounts on the connected `servers`, or all of them,
    /// have used, asked of the servers: `sections`, each a `limits::Section`. `kept` answers at
    /// once with what was last read.
    Limits {
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
    /// What the server does with pull requests by itself: archiving threads and removing
    /// worktrees once their pull requests merge.
    SetPullRequestSettings {
        server_id: String,
        settings: PullRequestSettings,
    },
    /// Backs every server up to the bucket, and keeps its keys on this device to give a server
    /// that has none; `None` stops. The first connected server checks the bucket before it is kept.
    SetBackup {
        bucket: Option<Bucket>,
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
    /// What the user did to the node `node` of the program in the row: its id in the row's tree.
    Ui {
        thread_id: String,
        row_id: String,
        node: String,
        event: UiEvent,
    },
    /// Whether the user allows the command a program asked to run with a `ui_confirm` event.
    UiConfirm {
        thread_id: String,
        row_id: String,
        call_id: u64,
        allow: bool,
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
    /// Where the client left the thread's transcript, or with `agent_id` the agent's: `at` a row,
    /// or without it at the end. The transcript opens there again until the client quits.
    KeepPlace {
        thread_id: String,
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        at: Option<ScrollAt>,
    },
    /// Finds `query` in the thread, as a browser finds in a page, and answers with `found`. The
    /// current match is the one in `item_id` when it has one, or else the first from the current
    /// match, or without one from `from`, where the client shows the thread. Each time the current
    /// match moves, `reveal` brings it into sight.
    Find {
        thread_id: String,
        query: String,
        #[serde(default)]
        from: Option<FindFrom>,
        #[serde(default)]
        item_id: Option<String>,
    },
    /// Moves to the next match, or with `back` the one before.
    FindStep {
        thread_id: String,
        #[serde(default)]
        back: bool,
    },
    FindEnd {
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
    /// Whether a view that shows the account's gateway is open: the Gateway page or the Limits tab.
    /// While one is, the core stays connected to the gateway and `gateway` events follow its state.
    WatchGateway {
        on: bool,
    },
    /// Makes the code a gateway joins the account with. Answers with a `gateway::Enroll`.
    AddGateway,
    /// Removes the gateway from the account. The servers' Gateway accounts go with it.
    RemoveGateway,
    /// Starts signing an account of `agent` in on the gateway. Answers with the `url` to open and, for
    /// Codex, the `user_code` to enter there; the `gateway` event's `sign_in` follows it. `account` is
    /// the one it signs in again, if any.
    GatewaySignIn {
        agent: Agent,
        #[serde(default)]
        account: Option<String>,
    },
    /// Finishes Claude's sign-in with the code its page showed. Answers once the account is added.
    GatewayFinishSignIn {
        code: String,
    },
    /// Forgets the sign-in under way.
    GatewayCancelSignIn,
    /// Renames an account, sets its priority, or turns it on or off.
    GatewayUpdateAccount {
        account: String,
        change: AccountChange,
    },
    GatewayRemoveAccount {
        account: String,
    },
    /// A page of the gateway's request log, newest first, before the entry `before`, or only the
    /// requests that failed. Answers with `rows`, each a `gateway::LogRow`, and `more` when there are
    /// older ones.
    GatewayLog {
        #[serde(default)]
        before: Option<i64>,
        #[serde(default)]
        errors_only: bool,
    },
    /// A captured request in full. Answers with `request`, a `gateway::CapturedView`, or null when the
    /// gateway no longer has it.
    GatewayCaptured {
        entry: i64,
    },
    /// Starts or stops capturing requests in full: every request of one thread, or the next few.
    GatewayCapture {
        capture: Capture,
    },
    /// Adds the MCP at `url` to the gateway, with a key when `key` holds one (`header` empty for
    /// `Authorization`). Answers with `added`, the MCP server's id; or `sign_in`, the `url` to open
    /// and the `state` to finish with; or `needs_key` when it takes a key.
    McpServerAdd {
        url: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        header: String,
        #[serde(default)]
        key: String,
    },
    /// Signs an MCP server in again. Answers as `ConnectorAdd` does.
    McpServerSignIn {
        mcp_server: String,
    },
    /// Finishes a sign-in with the `motile://mcp` address it came back to. Answers with
    /// `added`.
    McpServerFinishSignIn {
        address: String,
    },
    /// Renames an MCP server, changes its slug or key, turns it or one of its tools on or off.
    McpServerUpdate {
        mcp_server: String,
        change: McpChange,
    },
    /// Gives an MCP server a new key.
    McpServerKey {
        mcp_server: String,
        #[serde(default)]
        header: String,
        key: String,
    },
    /// Asks an MCP server for its tools again.
    McpServerRefresh {
        mcp_server: String,
    },
    McpServerRemove {
        mcp_server: String,
    },
    /// A page of the calls agents made, newest first, before the call `before`, or only the ones
    /// that failed. Answers with `rows`, each a `MCP servers::CallRow`, and `more`.
    McpServerCalls {
        #[serde(default)]
        before: Option<i64>,
        #[serde(default)]
        errors_only: bool,
    },
    /// A call with its arguments and result. Answers with `call`, a `MCP servers::CallView`, or null.
    McpServerCall {
        call: i64,
    },
    /// The fields a tool is tried with. Answers with `fields`, each a `MCP servers::Field`.
    McpServerFields {
        mcp_server: String,
        tool: String,
    },
    /// Calls a tool with what its fields hold, by name. Answers with a `MCP servers::Answered`.
    McpServerTry {
        mcp_server: String,
        tool: String,
        #[serde(default)]
        values: serde_json::Map<String, serde_json::Value>,
    },
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
    /// The account's backup, which follows `Servers`.
    Backup {
        backup: BackupView,
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
        /// With `reset`, where the client left the transcript, which shows that in place of the end.
        #[serde(skip_serializing_if = "Option::is_none")]
        at: Option<ScrollAt>,
    },
    /// What finding in the thread found: how many matches, which is the current one, and where
    /// they are in the rows the client has. Sent again whenever those rows change. `ready` once
    /// the whole thread has been searched.
    Found {
        thread_id: String,
        query: String,
        count: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        current: Option<usize>,
        rows: Vec<FoundRow>,
        ready: bool,
    },
    /// Scroll the row into sight, and with `range` that part of its text, `[start, length]` in
    /// UTF-16 units.
    Reveal {
        thread_id: String,
        row_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<[u32; 2]>,
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
        #[serde(skip_serializing_if = "Option::is_none")]
        at: Option<ScrollAt>,
    },
    Spans {
        thread_id: String,
        row_id: String,
        spans: Spans,
    },
    /// Changes the tree of a program row, in the transcript or in the open agent's.
    UiPatch {
        thread_id: String,
        row_id: String,
        ops: Vec<UiOp>,
    },
    /// A program asks to run `command` in the thread's folder: the client asks the user, and
    /// answers with `ui_confirm`.
    UiConfirm {
        thread_id: String,
        row_id: String,
        call_id: u64,
        command: String,
    },
    /// A program opens a web page in the browser.
    OpenUrl {
        url: String,
    },
    /// A program copies text to the clipboard.
    Copy {
        text: String,
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
    /// The account's gateway, as the Gateway page shows it, whenever that changes.
    Gateway {
        gateway: Box<gateway::View>,
    },
    /// The messages on their way to their servers, oldest first, at start and whenever that
    /// changes. One that couldn't be sent yet says why in `error`.
    Outbox {
        messages: Vec<Outgoing>,
    },
    /// A message of the outbox reached its server, in `thread_id`. `draft_id` is the draft the
    /// thread was started from, whose id a server from before thread ids doesn't give it.
    Sent {
        message_id: String,
        thread_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        draft_id: Option<String>,
    },
    /// A message of the outbox won't be sent, for `error`: it goes back to where it was written.
    Unsent {
        message: Box<Outgoing>,
        error: String,
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
    /// The account a new thread is better off starting on, by account id and model id: the one of
    /// the same agent that runs the model and that `motile_protocol::select` ranks first.
    pub roomier_accounts: HashMap<String, HashMap<String, String>>,
    /// What each of its agents' accounts has used of its limits, by account id.
    pub account_limits: HashMap<String, Vec<limits::Row>>,
    /// The name of the gateway's account whose limits a Gateway account's are: the one a new thread
    /// would start on, by account id.
    pub next_accounts: HashMap<String, String>,
    /// Its agents' accounts, each agent's in the order a new thread would choose them.
    pub accounts_by_room: Vec<String>,
    /// How its copies of the account's other servers' threads stand. None for a removed server.
    pub sync: Option<SyncView>,
}

/// The account's S3 bucket, which every server backs up to and syncs through.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct BackupView {
    /// From this device, or from a server that uses it; its secret only when this device keeps it.
    /// None while there is no backup.
    pub bucket: Option<Bucket>,
    /// "motile on 4f1c.r2.cloudflarestorage.com", or what a backup does while there is none.
    pub description: String,
    /// This device keeps the keys and gives them to a server that has none.
    pub saved: bool,
}

/// What the settings say under a server of the copies it keeps of the others' threads.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SyncView {
    pub state: SyncState,
    /// "Synced", "Syncing · 48%", "Couldn't sync from vibe", "Not syncing".
    pub text: String,
    /// Why the last round couldn't copy to it.
    pub error: Option<String>,
    /// When a round last left its copies whole, in seconds since the epoch.
    pub synced_at: Option<f64>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    Synced,
    Copying,
    Waiting,
    Failed,
    Off,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ProjectView {
    #[serde(flatten)]
    pub project: Project,
    /// The same on every server that has the project: its repository's origin, or "no-project".
    pub key: String,
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
    /// It woke from a snooze by itself since the user last looked at it on this device.
    pub woke: bool,
    /// Where the sidebar lists it.
    pub section: Section,
    /// How the agents its agent started are doing, theirs too, when it started any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Children>,
}

/// The part of the sidebar a thread is listed in.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Active,
    /// Its agent, or an agent it started, works or monitors, and nothing waits for the user.
    Working,
    Snoozed,
    Archived,
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
    /// How many work, as the agents strip says it before "working": "1 agent", "3 agents".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running_count: Option<String>,
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
        assert_eq!(
            (server_id.as_str(), thread_id.as_str(), change.archived, change.title),
            ("h", "t", Some(true), None)
        );

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
    fn gateway_commands_name_their_account_and_entry_apart_from_the_commands_id() {
        let json = r#"{"id": 3, "type": "gateway_update_account", "account": "org-1",
            "change": {"type": "priority", "priority": 2}}"#;
        let envelope: Envelope = serde_json::from_str(json).unwrap();
        let Command::GatewayUpdateAccount { account, change: AccountChange::Priority { priority } } = envelope.command
        else {
            panic!("expected an account's priority");
        };
        assert_eq!((envelope.id, account.as_str(), priority), (3, "org-1", 2));

        let capture: Envelope = serde_json::from_str(
            r#"{"id": 4, "type": "gateway_capture", "capture": {"type": "thread", "server": "s", "thread": "t"}}"#,
        )
        .unwrap();
        assert!(matches!(capture.command, Command::GatewayCapture { capture: Capture::Thread { .. } }));

        let captured: Envelope = serde_json::from_str(r#"{"id": 5, "type": "gateway_captured", "entry": 9}"#).unwrap();
        assert!(matches!(captured.command, Command::GatewayCaptured { entry: 9 }));
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
            thread: Box::new(ThreadView {
                thread,
                server_id: "h".into(),
                unread: false,
                woke: false,
                section: Section::Active,
                children: None,
            }),
        };
        let json = serde_json::to_value(event).unwrap();

        assert_eq!(json["type"], "thread_upsert");
        assert_eq!(json["thread"]["server_id"], "h");
        assert_eq!(json["thread"]["access"], "accept_edits");
        assert_eq!(json["thread"]["running"], true);
    }
}
