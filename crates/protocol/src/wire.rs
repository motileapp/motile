//! Messages between an app and a server. Every stream starts with one `Request` from the app; the
//! server answers with one `Message`, or with a stream of them for `Subscribe` and `Open`.

use std::collections::HashMap;

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
    /// Tools that need approval wait until the user allows or refuses them.
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
    /// The turn is over, but the agent still watches something it left running.
    #[serde(default)]
    pub monitoring: bool,
    /// A tool call waits for the user to allow or refuse it.
    pub needs_approval: bool,
    /// How many agents the thread's agent has started that still work.
    #[serde(default)]
    pub agents: u32,
    /// When the last turn ended, for telling the user about replies they haven't seen.
    pub turn_ended_at: Option<f64>,
    /// The pull request that was opened for the thread, whatever became of it.
    #[serde(default)]
    pub pull_request: Option<PullRequest>,
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
    /// The images and videos the item shows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<Media>,
    /// The tool call that started the agent which said or did this. Such an item belongs to
    /// that agent's transcript, not to the thread's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(flatten)]
    pub kind: ItemKind,
}

/// An image or a video an agent showed or the user attached. The server keeps the copy it took
/// then, so the thread shows the same thing after the file has changed or gone.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Media {
    /// Names the contents, and ends in the file's extension.
    pub id: String,
    /// The file as the agent wrote it in its reply, or the attachment's path.
    pub src: String,
    pub video: bool,
    pub size: u64,
    /// In pixels, for an image, and for a video that has a poster.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// The image that stands for a video until it plays, which the app that attached it made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
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
    /// Set when the call started an agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<Subagent>,
}

/// An agent the thread's agent started with a tool call, and how far it is. It can go on after
/// the call has returned.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Subagent {
    /// The kind of agent it was started as, in the agent's own words: "Explore".
    pub kind: Option<String>,
    pub status: ToolStatus,
    /// What it is doing now.
    pub progress: Option<String>,
    /// What it reported when it ended.
    pub result: Option<String>,
    pub tokens: Option<u64>,
    pub tool_uses: Option<u64>,
    pub duration_ms: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct TurnSummary {
    pub duration_ms: Option<u64>,
    pub cost_usd: Option<f64>,
    pub is_error: bool,
    /// The user stopped the turn.
    #[serde(default)]
    pub stopped: bool,
    /// What the turn changed in the thread's folder, once the server has compared the folder
    /// with how the turn found it. Missing when the folder is no repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<TurnChanges>,
}

/// The files a turn left different from how it found them.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct TurnChanges {
    /// Names the folder as the turn left it, for asking for the turn's diff.
    pub snapshot: String,
    pub files: Vec<ChangedFile>,
}

/// A tool call that waits for the user to allow or refuse it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Approval {
    pub id: String,
    pub tool_name: String,
    /// The tool's input as JSON text.
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
    /// The thread works in a git worktree of its own, on a branch of its own. Without it, it
    /// works in the project's folder.
    #[serde(default)]
    pub worktree: Option<NewWorktree>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct NewWorktree {
    /// The branch the worktree's branch starts from: as the remote has it, unless the local one
    /// is ahead.
    pub base: String,
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
    /// Server details, the threads and the projects, then every change to them.
    Subscribe,
    /// The transcript items changed after revision `since`, then every change as it happens.
    Open {
        thread_id: String,
        since: u64,
    },
    /// Starts a turn, in `thread_id` or in a thread created from `new_thread`. While a turn is
    /// running the message is queued until the turn ends, when it starts the next one. An agent
    /// that is only monitoring gets it right away.
    Send {
        thread_id: Option<String>,
        new_thread: Option<NewThread>,
        text: String,
        attachments: Vec<String>,
    },
    /// Gives the agent a queued message now.
    SendQueued {
        thread_id: String,
        message_id: String,
    },
    /// Takes a queued message back before the agent has been given it.
    CancelQueued {
        thread_id: String,
        message_id: String,
    },
    /// Allows or refuses a tool call that waits for it; the turn goes on either way. A tool call
    /// that asks the user questions is allowed with what they chose, by question.
    Answer {
        thread_id: String,
        approval_id: String,
        allow: bool,
        #[serde(default)]
        answers: HashMap<String, String>,
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
    /// Makes a folder named after `name` where the server keeps new projects, starts a git
    /// repository in it and adds it as a project. `ProjectAdded` answers.
    NewProject {
        name: String,
    },
    /// Whether GitHub's `gh` can be used on the server. `Github` answers.
    GithubStatus,
    /// The repositories the server's GitHub login reaches, the last pushed first. `Repos`
    /// answers, or `Github` when the login no longer works.
    GithubRepos,
    /// Clones `owner/name` from GitHub into where the server keeps new projects and adds it as a
    /// project. `ProjectAdded` answers.
    CloneRepo {
        repo: String,
    },
    /// Takes the folder off the list. Its threads stay.
    RemoveProject {
        project_id: String,
    },
    /// The bytes of the project's icon.
    ProjectIcon {
        project_id: String,
    },
    /// Makes the image at `path` on the server the project's icon. `None` goes back to the one
    /// found in the project's folder.
    SetProjectIcon {
        project_id: String,
        path: Option<String>,
    },
    /// Folders inside `path`, or inside the home folder. With `icons`, also the files there that
    /// can be a project's icon, and with `hidden` the folders whose names start with a dot.
    ListDir {
        path: Option<String>,
        #[serde(default)]
        icons: bool,
        #[serde(default)]
        hidden: bool,
    },
    /// The branches of the project's repository. `Branches` answers.
    Branches {
        project_id: String,
    },
    /// Checks the branch out in the project's folder, where all the project's threads work. With
    /// `create` the branch is made first, from what is checked out.
    SwitchBranch {
        project_id: String,
        branch: String,
        #[serde(default)]
        create: bool,
    },
    /// What git says about the project's folder, or about the worktree of the thread. With
    /// `fetch` the remote is asked first. `GitStatus` answers.
    GitStatus {
        project_id: String,
        #[serde(default)]
        thread_id: Option<String>,
        #[serde(default)]
        fetch: bool,
    },
    /// Commits, pushes, opens a pull request or pulls in the project's folder, or in the worktree
    /// of the thread. The server writes
    /// the commit message when `message` is missing, and the pull request's title and text. The
    /// server answers with a `GitProgress` as each stage starts, then `GitDone`.
    GitRun {
        project_id: String,
        action: GitAction,
        /// The thread the work was done in, which tells the writer why and where.
        #[serde(default)]
        thread_id: Option<String>,
        #[serde(default)]
        message: Option<String>,
        /// Only the changes at these paths are committed. Empty for all of them.
        #[serde(default)]
        paths: Vec<String>,
        /// Makes a branch for the work first, named by the server, and carries on there.
        #[serde(default)]
        new_branch: bool,
    },
    /// The changes in the folder the thread works in, or in the project's folder, as a patch.
    /// `Diff` answers.
    Diff {
        project_id: String,
        #[serde(default)]
        thread_id: Option<String>,
        scope: DiffScope,
    },
    /// What is in `path`, a folder inside the one the thread works in or inside the project's
    /// folder, and relative to it. Empty for that folder itself. `Files` answers.
    ListFiles {
        project_id: String,
        #[serde(default)]
        thread_id: Option<String>,
        path: String,
    },
    /// The file at `path` in that folder. `File` answers, and the bytes follow on the same
    /// stream.
    ReadFile {
        project_id: String,
        #[serde(default)]
        thread_id: Option<String>,
        path: String,
    },
    /// Picks the model that writes thread titles, commit messages and pull requests on this
    /// server. `None` goes back to the lightest model of the thread's agent.
    SetTextModel {
        model: Option<String>,
    },
    /// How this server's writer is told to name the branches it makes. `None` goes back to the
    /// server's own instructions.
    SetBranchInstructions {
        instructions: Option<String>,
    },
    /// The shell script that runs in every new worktree of the project before the agent starts
    /// there, to install what the work needs. `None` runs nothing.
    SetProjectSetup {
        project_id: String,
        script: Option<String>,
    },
    /// The file's bytes follow on the same stream. With `poster_of`, the path of a video that was
    /// uploaded before, the file is the image that stands for that video.
    Upload {
        name: String,
        size: u64,
        #[serde(default)]
        poster_of: Option<String>,
    },
    /// The copy the server keeps of an image or a video. `Media` answers, and the bytes follow on
    /// the same stream.
    Media {
        id: String,
    },
    /// Replaces the server's program with the latest release and starts it again. The server answers
    /// with `Updating` while it downloads, then `Ok` just before it restarts.
    #[serde(alias = "update_host")]
    UpdateServer,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct AgentInfo {
    pub agent: Agent,
    /// `None` when the agent's CLI isn't installed on the server.
    pub version: Option<String>,
}

/// A folder on the server that threads work in.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Project {
    pub id: String,
    pub path: String,
    pub name: String,
    /// The git branch checked out there, if it is a repository.
    pub branch: Option<String>,
    /// What the server last read from git there. Missing until it has, and when it is no repository.
    #[serde(default)]
    pub git: Option<GitStatus>,
    /// Names the icon's contents: it changes when the icon does, and ends in the file's
    /// extension. `None` when the project has no icon.
    #[serde(default)]
    pub icon: Option<String>,
    /// The worktrees the project's threads work in.
    #[serde(default)]
    pub worktrees: Vec<Worktree>,
    /// The shell script that runs in every new worktree.
    #[serde(default)]
    pub setup: Option<String>,
    pub created_at: f64,
}

/// A git worktree of a project, made for one thread, which has it as its `cwd`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Worktree {
    pub path: String,
    pub branch: Option<String>,
    /// What the server last read from git there.
    pub git: Option<GitStatus>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum GitHubState {
    Ready,
    /// `gh` is installed and nobody is signed in to it.
    SignedOut,
    /// `gh` isn't installed.
    Missing,
}

/// A repository on GitHub.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Repo {
    /// `owner/name`.
    pub name: String,
    pub description: Option<String>,
    pub private: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Branch {
    pub name: String,
    pub current: bool,
    /// The one the remote starts new work from.
    pub default: bool,
    /// Only on the remote so far. Switching to it makes the local branch.
    pub remote: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct GitStatus {
    /// `None` when no branch is checked out.
    pub branch: Option<String>,
    /// The checked-out branch is the one the remote starts new work from.
    pub default: bool,
    /// The branch new work starts from: the remote's, or `main` or `master` without one.
    #[serde(default)]
    pub default_branch: Option<String>,
    pub remote: bool,
    pub upstream: bool,
    /// Commits that aren't on the remote yet.
    pub ahead: u32,
    pub behind: u32,
    pub ahead_of_default: u32,
    /// Files with changes that aren't committed, and the lines added and removed in them.
    pub changed: u32,
    pub added: u32,
    pub removed: u32,
    /// The server can open pull requests for this repository.
    pub pull_requests: bool,
    /// The pull request of the branch: the open one, or the merged or closed one while the branch
    /// has no commit since.
    pub pull_request: Option<PullRequest>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub draft: bool,
    #[serde(default)]
    pub merged: bool,
    /// Closed without being merged.
    #[serde(default)]
    pub closed: bool,
}

impl PullRequest {
    pub fn is_open(&self) -> bool {
        !self.merged && !self.closed
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ChangedFile {
    pub path: String,
    /// Where a renamed file was.
    pub from: Option<String>,
    pub change: Change,
    pub added: u32,
    pub removed: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// Which changes a diff shows.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiffScope {
    /// What the turn that ended with the item changed.
    Turn { item_id: String },
    /// What isn't committed.
    Uncommitted,
    /// Everything since the branch left the one it started from, committed or not.
    Branch,
}

/// A file or a folder in a folder threads work in.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct FileEntry {
    pub name: String,
    pub folder: bool,
    /// Git ignores it.
    pub ignored: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Text,
    Image,
    /// Neither: its bytes aren't sent.
    Binary,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum GitAction {
    Commit,
    Push,
    /// Pushes first when the remote doesn't have the branch's commits.
    CreatePr,
    CommitPush,
    CommitPushPr,
    Pull,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum GitStage {
    Branch,
    Message,
    Commit,
    Push,
    PullRequestText,
    PullRequest,
    Pull,
}

/// A model an agent on the server can run, and the choices it offers.
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
pub struct ServerInfo {
    pub version: String,
    pub protocol: u32,
    pub hostname: String,
    pub home: String,
    pub agents: Vec<AgentInfo>,
    /// The models of the installed agents, in the order to offer them.
    pub models: Vec<ModelInfo>,
    /// The model that writes titles, commit messages and pull requests, when one was picked.
    #[serde(default)]
    pub text_model: Option<String>,
    #[serde(default)]
    pub branch_instructions: BranchInstructions,
}

/// How the writer is told to name the branches it makes.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct BranchInstructions {
    pub text: String,
    /// What `text` is until the user changes it.
    pub default: String,
}

/// What a thread's agent is doing right now.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Activity {
    pub running: bool,
    #[serde(default)]
    pub monitoring: bool,
    pub thinking: bool,
    /// The agent is making its conversation shorter to go on with it.
    #[serde(default)]
    pub compacting: bool,
    /// How many agents it has started that still work.
    #[serde(default)]
    pub agents: u32,
    pub started_at: Option<f64>,
    /// The running turn stands still until these are answered.
    #[serde(default)]
    pub approvals: Vec<Approval>,
    /// The messages that wait for the agent to take them, in the order they were sent.
    #[serde(default)]
    pub queued: Vec<Queued>,
}

/// A message sent while the agent was working. It joins the transcript when the agent takes it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Queued {
    pub id: String,
    pub text: String,
    pub attachments: Vec<String>,
    /// The images and videos among them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<Media>,
    /// The turn it waited for was stopped; it goes when the user sends it.
    pub held: bool,
    /// The agent has been given it and hasn't taken it yet.
    pub sending: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Welcome {
        #[serde(alias = "host")]
        server: ServerInfo,
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
        #[serde(default)]
        files: Vec<String>,
    },
    ProjectAdded {
        project_id: String,
    },
    Github {
        state: GitHubState,
    },
    Repos {
        repos: Vec<Repo>,
    },
    /// Local branches first, then the ones only on the remote.
    Branches {
        branches: Vec<Branch>,
    },
    /// `status` is missing when the folder is no repository.
    GitStatus {
        status: Option<GitStatus>,
        files: Vec<ChangedFile>,
    },
    GitProgress {
        stage: GitStage,
    },
    /// What a `GitRun` did, in words to show: "Committed 1a2b3c4" and the commit's subject.
    GitDone {
        title: String,
        description: Option<String>,
        /// The pull request that was opened, or was open already.
        url: Option<String>,
        /// What to do after it, if anything follows.
        next: Option<GitAction>,
    },
    Uploaded {
        path: String,
    },
    /// How far the download of the server's update is. `total` is missing when it isn't known.
    Updating {
        received: u64,
        total: Option<u64>,
    },
    /// `size` bytes of an image or a video follow.
    Media {
        size: u64,
    },
    /// `truncated` when the patch was cut for being too long.
    Diff {
        patch: String,
        truncated: bool,
    },
    /// Folders first, each kind by name.
    Files {
        entries: Vec<FileEntry>,
    },
    /// `sent` bytes of a file of `size` bytes follow: all of an image, and the start of a long
    /// text.
    File {
        kind: FileKind,
        size: u64,
        sent: u64,
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
        let item = Item {
            id: "a".into(),
            seq: 3,
            rev: 7,
            created_at: 1.0,
            media: Vec::new(),
            parent: None,
            kind: ItemKind::Assistant { text: "hi".into() },
        };
        let json = serde_json::to_value(&item).unwrap();

        assert_eq!(
            json,
            serde_json::json!({"id": "a", "seq": 3, "rev": 7, "created_at": 1.0, "type": "assistant", "text": "hi"})
        );
        assert_eq!(serde_json::from_value::<Item>(json).unwrap(), item);
    }

    #[test]
    fn what_0_1_6_calls_a_host_is_read_as_a_server() {
        let update: Request = serde_json::from_value(serde_json::json!({"type": "update_host"})).unwrap();
        let welcome: Message = serde_json::from_value(serde_json::json!({
            "type": "welcome", "threads": [], "projects": [],
            "host": {"version": "0.1.6", "protocol": 1, "hostname": "box", "home": "/root", "agents": [], "models": []},
        }))
        .unwrap();
        let kind: crate::auth_api::DeviceKind = serde_json::from_value(serde_json::json!("host")).unwrap();

        assert_eq!(update, Request::UpdateServer);
        assert!(matches!(welcome, Message::Welcome { server, .. } if server.version == "0.1.6"));
        assert_eq!(kind, crate::auth_api::DeviceKind::Server);
    }
}
