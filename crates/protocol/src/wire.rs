//! Messages between a client and a server. Every stream starts with one `Request` from the client; the
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
    /// The id of the agent's account on the server that the thread works with.
    #[serde(default)]
    pub agent_account: String,
    /// `None` uses the agent's own default.
    pub model: Option<String>,
    /// Reasoning effort; `None` uses the model's default.
    pub effort: Option<String>,
    pub access: Access,
    /// The agent only reads and proposes; it changes nothing.
    pub plan: bool,
    pub created_at: f64,
    /// When something last happened in the thread.
    pub updated_at: f64,
    /// Set while the thread is marked done.
    pub done_at: Option<f64>,
    /// Where the sidebar lists the thread among the active ones, the highest first: when it was
    /// created or last came back from done, until the user moves it between two others.
    #[serde(default)]
    pub position: f64,
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
    /// The pull request that was opened for the thread or linked to it, whatever became of it.
    #[serde(default)]
    pub pull_request: Option<PullRequest>,
    /// The thread's agent is told when its pull request's checks finish, someone comments on it
    /// or it starts to conflict.
    #[serde(default)]
    pub watching: bool,
    /// What a commit, a push or the like that was started from the thread is at.
    #[serde(default)]
    pub git_stage: Option<GitStage>,
    /// Why the agent stopped before it finished, until it works again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interruption: Option<Interruption>,
    /// The transcript's revision; a client whose copy is older has catching up to do.
    pub rev: u64,
}

/// Why a thread's agent stopped before it finished.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Interruption {
    /// The agent reached its usage limit. `resets_at` is when the limit resets, when the agent
    /// said; with `continues` the thread goes on by itself then.
    Limit { resets_at: Option<f64>, continues: bool },
    /// The server restarted while the agent worked.
    Restart,
}

/// What the server tells an agent to have it go on with what it was doing.
pub const CONTINUE_PROMPT: &str = "Continue where you left off.";

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
    /// The image that stands for a video until it plays, which the client that attached it made.
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
    /// The id of one of the agent's accounts; `None` is the agent's default account.
    #[serde(default)]
    pub agent_account: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub access: Access,
    pub plan: bool,
    /// The thread works in a git worktree of its own, on a branch of its own. Without it, it
    /// works in the project's folder, or in a folder of its own in the server's "No project".
    #[serde(default)]
    pub worktree: Option<NewWorktree>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct NewWorktree {
    /// The branch the worktree's branch starts from: as the remote has it, unless the local one
    /// is ahead.
    pub base: String,
    /// What to call the worktree's branch, where the writer isn't to name it.
    #[serde(default)]
    pub branch: Option<String>,
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
    /// Where the sidebar lists the thread among the active ones, as the user moved it.
    #[serde(default)]
    pub position: Option<f64>,
    /// Whether a thread that waits for its usage limit continues once the limit resets.
    #[serde(default)]
    pub continues: Option<bool>,
    /// Moves the thread to another account, of its agent or of the other one, with `model` one
    /// of that agent's. One that keeps its sessions elsewhere starts a new session that is told
    /// the conversation so far.
    #[serde(default)]
    pub agent_account: Option<String>,
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
    /// that is only monitoring gets it right away. With `now`, the turn that runs takes it at once
    /// if the agent can.
    Send {
        thread_id: Option<String>,
        new_thread: Option<NewThread>,
        text: String,
        attachments: Vec<String>,
        #[serde(default)]
        now: bool,
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
    /// The Linear workspaces the server is connected to. `Linear` answers.
    LinearStatus,
    /// Starts connecting the server to a Linear workspace. `LinearAuthorize` answers with the page
    /// where the user picks the workspace and approves, which ends at `LINEAR_REDIRECT` with a
    /// `code` and a `state`.
    LinearConnect,
    /// Finishes connecting with what Linear sent the browser back with. `Linear` answers.
    LinearFinish {
        code: String,
        state: String,
    },
    /// Takes back what Linear granted for the workspace and forgets it. `Linear` answers.
    LinearDisconnect {
        workspace: String,
    },
    /// The workspace's teams with their statuses, and its users. `LinearTeams` answers.
    LinearTeams {
        workspace: String,
    },
    /// The workspace's issues, the last updated first: of one team or of all, assigned to the
    /// user or to anyone, and in a status of one of the kinds in `states`. Without `states`,
    /// which servers before 0.1.268 don't read, `closed` says whether the completed and
    /// cancelled ones are among them. With `search` they are the team's issues that have the
    /// words, whoever has them and whatever their status. `LinearIssues` answers.
    LinearIssues {
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
    /// One issue with its description and comments. `LinearIssueDetail` answers.
    LinearIssue {
        workspace: String,
        issue: String,
    },
    /// Changes the issue's status, assignee or priority. `LinearIssue` answers.
    LinearUpdate {
        workspace: String,
        issue: String,
        change: LinearChange,
    },
    /// Comments on the issue. `LinearIssueDetail` answers.
    LinearComment {
        workspace: String,
        issue: String,
        body: String,
    },
    /// Files a new issue. `LinearIssue` answers.
    LinearCreate {
        workspace: String,
        issue: NewLinearIssue,
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
    /// Makes the project's folder a git repository.
    InitRepository {
        project_id: String,
    },
    /// What a new worktree on a branch made from `base` would start at. With `fetch` the remote
    /// is asked for `base` first. `WorktreeStart` answers.
    WorktreeStart {
        project_id: String,
        base: String,
        #[serde(default)]
        fetch: bool,
    },
    /// Fast-forwards the local `base` to the remote's, for a new worktree to start from it.
    /// `WorktreeStart` answers.
    UpdateBase {
        project_id: String,
        base: String,
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
    /// What GitHub says of the pull request, from the folder the thread works in or the
    /// project's folder. `PullRequest` answers.
    PullRequest {
        project_id: String,
        #[serde(default)]
        thread_id: Option<String>,
        number: u64,
    },
    /// Does something to the pull request. `method` is how to merge, or with `UpdateBranch`
    /// whether to rebase; `text` is what a comment, a review or a close says. `PullRequestDone`
    /// answers.
    PullRequestAction {
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
    /// Changes the pull request's title, text, labels, reviewers, reactions, review comments or
    /// viewed files, or reviews it with comments on lines. `PullRequestDone` answers.
    PullRequestEdit {
        project_id: String,
        #[serde(default)]
        thread_id: Option<String>,
        number: u64,
        edit: PullRequestEdit,
    },
    /// The repository's pull requests, the last updated first. `PullRequests` answers.
    PullRequests {
        project_id: String,
        #[serde(default)]
        thread_id: Option<String>,
        state: PullRequestState,
    },
    /// Makes the pull request with that number the thread's own, read in the folder it works in;
    /// `None` takes the one it has away.
    LinkPullRequest {
        thread_id: String,
        number: Option<u64>,
    },
    /// Has the server tell the thread's agent what happens on its pull request, or stop.
    WatchPullRequest {
        thread_id: String,
        watch: bool,
    },
    /// What the server does with pull requests by itself; `None` leaves a setting as it is.
    SetPullRequestSettings {
        #[serde(default)]
        done_on_merge: Option<bool>,
        #[serde(default)]
        remove_merged_worktrees: Option<bool>,
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
    /// The file at `path` in that folder, or with `blob` the one git keeps under that name, as a
    /// diff calls it. `File` answers, and the bytes follow on the same stream.
    ReadFile {
        project_id: String,
        #[serde(default)]
        thread_id: Option<String>,
        path: String,
        #[serde(default)]
        blob: Option<String>,
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
    /// with `Updating` while it downloads, `UpdateWaiting` while its agents work, then `Ok` just
    /// before it restarts. Without `when`, as older clients ask, it refuses while agents work.
    #[serde(alias = "update_host")]
    UpdateServer {
        #[serde(default)]
        when: Option<RestartWhen>,
    },
    /// Has the thread's agent go on with what it was doing.
    Continue {
        thread_id: String,
    },
    /// What the server goes on with by itself; `None` leaves a setting as it is.
    SetContinueSettings {
        #[serde(default)]
        after_limits: Option<bool>,
        #[serde(default)]
        after_restarts: Option<bool>,
    },
    /// What the agents spent between `since` and `until`, in buckets of `bucket_secs` that start
    /// where the hours and days of a clock `utc_offset_secs` ahead of UTC do. `Usage` answers.
    Usage {
        since: f64,
        until: f64,
        bucket_secs: u32,
        utc_offset_secs: i32,
    },
    /// How much of their plans the agents' accounts have used. What was read in the last minutes
    /// is answered again unless `refresh`. `Limits` answers.
    Limits {
        #[serde(default)]
        refresh: bool,
    },
    /// Adds an account of an agent, or changes the one with the same id. A new account's id is
    /// empty; the server picks it.
    SaveAgentAccount {
        account: AgentAccount,
    },
    /// Removes an account of an agent. Its threads go on with the agent's default account.
    RemoveAgentAccount {
        id: String,
    },
}

/// One of an agent's accounts on a server: the folder its CLI keeps the sign-in in.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct AgentAccount {
    /// The agent's own name for its default account, which every server has.
    pub id: String,
    pub agent: Agent,
    pub name: String,
    /// `CLAUDE_CONFIG_DIR` or `CODEX_HOME`. Empty for the default account, which uses the
    /// agent's usual folder.
    pub folder: String,
    /// A Codex account that keeps only its sign-in in `folder` and shares the rest of the default
    /// account's folder, its sessions too, so that a thread can move between them.
    #[serde(default)]
    pub shares_sessions: bool,
    /// Set for the agent besides the server's environment, for an API key or a router.
    #[serde(default)]
    pub variables: Vec<Variable>,
    /// Who is signed in, as the agent's CLI last said.
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Variable {
    pub name: String,
    /// Empty for a sensitive one when the server sends it: its value never leaves the server. A
    /// sensitive one saved with an empty value keeps the value it had.
    pub value: String,
    #[serde(default)]
    pub sensitive: bool,
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
    /// The server's "No project": each of its threads works in a folder of its own inside it.
    #[serde(default)]
    pub no_project: bool,
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

/// A Linear workspace a server is connected to, and the user who connected it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct LinearConnection {
    /// Names the workspace in requests.
    pub id: String,
    pub workspace: String,
    pub user: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct LinearTeam {
    pub id: String,
    /// What its issues' identifiers start with: `ENG`.
    pub key: String,
    pub name: String,
    pub states: Vec<LinearState>,
}

/// A status a team's issues can have.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct LinearState {
    pub id: String,
    pub name: String,
    pub kind: LinearStateKind,
    /// Hex without the `#`.
    pub color: String,
    /// Where it stands among the team's statuses of its kind.
    pub position: f64,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum LinearStateKind {
    Triage,
    Backlog,
    Unstarted,
    Started,
    Completed,
    Canceled,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct LinearIssue {
    pub id: String,
    /// `ENG-123`.
    pub identifier: String,
    pub title: String,
    pub url: String,
    /// 0 is none, 1 urgent, 2 high, 3 medium and 4 low.
    pub priority: u8,
    pub state: LinearState,
    /// Its team's id.
    pub team: String,
    pub assignee: Option<LinearUser>,
    pub labels: Vec<Label>,
    /// What Linear would call a branch for it.
    pub branch_name: String,
    pub updated_at: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct LinearUser {
    pub id: String,
    pub name: String,
    /// The one who connected the workspace.
    #[serde(default)]
    pub me: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct LinearIssueDetail {
    pub issue: LinearIssue,
    /// Markdown.
    pub description: String,
    /// The oldest first.
    pub comments: Vec<LinearComment>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct LinearComment {
    pub id: String,
    pub author: String,
    /// Markdown.
    pub body: String,
    pub created_at: f64,
}

/// What to change of an issue; `None` leaves it as it is.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct LinearChange {
    #[serde(default)]
    pub state: Option<String>,
    /// A user's id, or nothing in it for nobody.
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub priority: Option<u8>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct NewLinearIssue {
    pub team: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub priority: u8,
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

/// All GitHub says of a pull request: where it stands, what stops it from merging and what was
/// said on it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct PullRequestDetail {
    #[serde(flatten)]
    pub pull_request: PullRequest,
    pub body: String,
    pub author: String,
    /// The branch it merges into, and the one it merges.
    pub base: String,
    pub head: String,
    pub additions: u32,
    pub deletions: u32,
    pub changed_files: u32,
    pub commits: u32,
    pub created_at: f64,
    pub merged_at: Option<f64>,
    pub merged_by: Option<String>,
    pub closed_at: Option<f64>,
    pub mergeable: Mergeable,
    /// Commits the base has that the branch hasn't, when GitHub says.
    pub behind_by: Option<u32>,
    pub review: Option<ReviewDecision>,
    pub checks: Vec<Check>,
    /// How it merges by itself once it may, when that was asked for.
    pub auto_merge: Option<MergeMethod>,
    /// The ways the repository lets it merge, the one it prefers first.
    pub merge_methods: Vec<MergeMethod>,
    pub auto_merge_allowed: bool,
    pub viewer: Viewer,
    /// Its commits, comments and reviews, the oldest first.
    pub activity: Vec<PullRequestEvent>,
    /// Names it in GitHub's API.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub default_branch: Option<String>,
    #[serde(default)]
    pub labels: Vec<Label>,
    /// The labels the repository has, to add from.
    #[serde(default)]
    pub repository_labels: Vec<Label>,
    /// Who was asked to review it and who did, with their latest verdict.
    #[serde(default)]
    pub reviewers: Vec<Reviewer>,
    /// Who can be asked to review it.
    #[serde(default)]
    pub assignable: Vec<String>,
    /// Its files, and which of them the user has marked as viewed.
    #[serde(default)]
    pub files: Vec<FileViewed>,
    /// The comments on its lines, by the line they were written against.
    #[serde(default)]
    pub threads: Vec<ReviewThread>,
    /// The stack GitHub keeps it in, bottom first.
    #[serde(default)]
    pub stack: Option<Stack>,
    /// The pull request of the branch it merges into, when that isn't the default branch.
    #[serde(default)]
    pub stacked_on: Option<PullRequest>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Label {
    pub name: String,
    /// Hex without the `#`.
    pub color: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Reviewer {
    /// A login, or a team's name.
    pub name: String,
    /// Asked and hasn't answered yet.
    pub requested: bool,
    pub verdict: Option<Verdict>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct FileViewed {
    pub path: String,
    pub viewed: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ReviewThread {
    pub id: String,
    pub path: String,
    /// The line in the file as it is now, missing when the thread is outdated.
    pub line: Option<u32>,
    pub side: Side,
    pub resolved: bool,
    pub outdated: bool,
    pub comments: Vec<ThreadComment>,
}

/// Which version of a file a line comment is on: as it was, or as the pull request makes it.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ThreadComment {
    pub id: String,
    pub author: String,
    pub body: String,
    pub at: f64,
    pub url: Option<String>,
    /// The lines of the diff it was written under.
    pub hunk: Option<String>,
    pub reactions: Vec<Reaction>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ReactionKind {
    ThumbsUp,
    ThumbsDown,
    Laugh,
    Hooray,
    Confused,
    Heart,
    Rocket,
    Eyes,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Reaction {
    pub kind: ReactionKind,
    pub count: u32,
    /// The user on the server reacted so.
    pub mine: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Stack {
    pub number: u64,
    pub url: String,
    pub base: String,
    pub layers: Vec<StackLayer>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct StackLayer {
    pub number: u64,
    pub title: String,
    pub head: String,
    pub draft: bool,
    pub merged: bool,
    pub closed: bool,
}

/// A pull request as the repository's list shows it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct PullRequestSummary {
    #[serde(flatten)]
    pub pull_request: PullRequest,
    pub author: String,
    pub head: String,
    pub base: String,
    pub updated_at: f64,
    pub review: Option<ReviewDecision>,
    /// How its checks went, all of them together; missing without checks.
    pub checks: Option<CheckStatus>,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestState {
    Open,
    Closed,
    Merged,
    All,
}

/// A change to a pull request that says more than an action does.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PullRequestEdit {
    Title {
        title: String,
    },
    Body {
        body: String,
    },
    Labels {
        add: Vec<String>,
        remove: Vec<String>,
    },
    Reviewers {
        add: Vec<String>,
        remove: Vec<String>,
    },
    /// Adds the reaction to a comment, a review or a review comment, or takes it back.
    React {
        subject: String,
        reaction: ReactionKind,
        on: bool,
    },
    Reply {
        thread: String,
        body: String,
    },
    Resolve {
        thread: String,
        resolved: bool,
    },
    Viewed {
        path: String,
        viewed: bool,
    },
    /// A review with comments on lines.
    Review {
        verdict: ReviewVerdict,
        body: String,
        comments: Vec<LineComment>,
    },
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Comment,
    Approve,
    RequestChanges,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct LineComment {
    pub path: String,
    pub line: u32,
    pub side: Side,
    pub body: String,
}

/// When an updated server restarts while its agents work.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RestartWhen {
    /// Once no agent works.
    Idle,
    /// At once: the agents are stopped and their threads continue after the restart.
    Now,
}

/// What the server goes on with by itself after its agents were stopped.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ContinueSettings {
    /// A thread that reached its agent's usage limit continues once the limit resets.
    pub after_limits: bool,
    /// A thread whose agent worked when the server restarted continues once it is back.
    pub after_restarts: bool,
}

/// What the server does with pull requests by itself.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PullRequestSettings {
    /// A thread is marked done when its pull request merges or closes.
    pub done_on_merge: bool,
    /// A thread's worktree is removed once its pull request merges, when nothing in it is lost.
    pub remove_merged_worktrees: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Mergeable {
    Mergeable,
    Conflicting,
    /// GitHub hasn't worked it out yet.
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
    ReviewRequired,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

/// A check run or a commit status on the pull request's last commit.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Check {
    pub name: String,
    pub workflow: Option<String>,
    pub status: CheckStatus,
    pub description: Option<String>,
    pub url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pending,
    /// Waits for someone, like a workflow that needs approving.
    ActionRequired,
    Success,
    Failure,
    Cancelled,
    Skipped,
    Neutral,
}

/// What the user on the server may do to the pull request.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
pub struct Viewer {
    /// Can push to the repository, so merge.
    pub can_write: bool,
    /// Can close, reopen and mark it ready or a draft.
    pub can_update: bool,
    pub can_update_branch: bool,
    /// Opened it, so can't approve it.
    pub authored: bool,
    /// Can change its labels.
    #[serde(default)]
    pub can_triage: bool,
    #[serde(default)]
    pub login: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct PullRequestEvent {
    pub at: f64,
    pub author: String,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    Commit {
        oid: String,
        headline: String,
        /// The whole name, for asking for its changes.
        #[serde(default)]
        sha: String,
    },
    Comment {
        body: String,
        url: Option<String>,
        #[serde(default)]
        id: String,
        #[serde(default)]
        reactions: Vec<Reaction>,
    },
    Review {
        verdict: Verdict,
        body: String,
        url: Option<String>,
        #[serde(default)]
        id: String,
        #[serde(default)]
        reactions: Vec<Reaction>,
    },
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
}

/// What can be done to a pull request.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestAction {
    Merge,
    /// Has GitHub merge it once its checks and reviews let it.
    EnableAutoMerge,
    DisableAutoMerge,
    Ready,
    Draft,
    Close,
    Reopen,
    /// Brings the base's commits into the branch, with a merge commit or by rebasing.
    UpdateBranch,
    /// Opens a pull request that undoes a merged one.
    Revert,
    Comment,
    Approve,
    RequestChanges,
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
    /// What the pull request changes, as GitHub has it.
    PullRequest { number: u64 },
    /// What one commit changed, as GitHub has it.
    Commit { sha: String },
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
    Video,
    /// None of them: its bytes aren't sent.
    Binary,
}

impl FileKind {
    /// What a file is shown as by its name, when that is an image or a video.
    pub fn shown(path: &str) -> Option<FileKind> {
        const IMAGES: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "heic", "bmp", "tiff", "ico"];
        let extension = std::path::Path::new(path).extension()?.to_str()?.to_lowercase();
        match extension.as_str() {
            image if IMAGES.contains(&image) => Some(FileKind::Image),
            video if crate::media::VIDEOS.contains(&video) => Some(FileKind::Video),
            _ => None,
        }
    }
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
    /// The commit message is written, and with it the name of a branch the run makes.
    Message,
    Commit,
    Push,
    PullRequestText,
    PullRequest,
    Pull,
    Merge,
    /// Turns GitHub's auto-merge on or off.
    AutoMerge,
    UpdateBranch,
    Close,
    Reopen,
    Revert,
    /// A stage of a newer server.
    #[serde(other)]
    Unknown,
}

/// A model an agent's account on the server can run, and the choices it offers.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub agent: Agent,
    /// The account whose CLI lists it.
    #[serde(default)]
    pub account: String,
    /// Reasoning efforts it accepts, weakest first. Empty when it has no such setting.
    pub efforts: Vec<String>,
    /// Missing when the agent applies its own default, as Claude Code does.
    pub default_effort: Option<String>,
}

/// Tokens by what each is billed as. `input` is what was neither read from the cache nor
/// written to it, and `output` includes the reasoning.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub struct Tokens {
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
}

/// What `Tokens` cost at the API's prices, in dollars.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub struct TokenCosts {
    pub input: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub output: f64,
}

/// What one model spent in one project during the time that starts at `start`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct UsageBucket {
    pub start: f64,
    pub agent: Agent,
    /// The name of the agent's account that spent it.
    #[serde(default)]
    pub account_name: String,
    pub model: String,
    pub project_id: String,
    pub tokens: Tokens,
    /// What the API would have charged. Missing when the model's prices aren't known.
    pub cost_usd: Option<f64>,
    /// That cost by kind of token, when the model's prices are known.
    pub costs: Option<TokenCosts>,
    /// What reading from the cache saved over sending the same tokens again.
    pub cache_savings_usd: f64,
    /// Spent on writing a title, a branch's name, a commit message or a pull request, not on a
    /// thread's turn.
    #[serde(default)]
    pub writing: bool,
}

/// What an agent's CLI says its login has used of its plan. No windows and no error: the login
/// has no plan limits, as with an API key.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct AgentLimits {
    pub agent: Agent,
    /// The name of the agent's account on the server.
    #[serde(default)]
    pub account_name: String,
    /// Who is signed in, which tells two servers with the same login apart from two logins.
    pub account: Option<String>,
    pub plan: Option<String>,
    pub windows: Vec<LimitWindow>,
    /// Resets the login may use to start its windows over.
    #[serde(default)]
    pub reset_credits: u32,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct LimitWindow {
    /// "Session", "Weekly", "Weekly · Fable".
    pub label: String,
    pub used_percent: f64,
    pub resets_at: Option<f64>,
    pub window_secs: Option<u64>,
    /// The agent warns that the window is running out.
    #[serde(default)]
    pub warning: bool,
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
    /// The agents' accounts, the default ones first.
    #[serde(default)]
    pub agent_accounts: Vec<AgentAccount>,
    /// The model that writes titles, commit messages and pull requests, when one was picked.
    #[serde(default)]
    pub text_model: Option<String>,
    #[serde(default)]
    pub branch_instructions: BranchInstructions,
    #[serde(default)]
    pub pull_request_settings: PullRequestSettings,
    #[serde(default)]
    pub continue_settings: ContinueSettings,
    /// The update the server is putting in place, until it restarts.
    #[serde(default)]
    pub update: Option<ServerUpdate>,
}

/// Where an update of the server is.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ServerUpdate {
    /// Downloading the latest release. `percent` is missing when its size isn't known.
    Installing { percent: Option<u8> },
    /// Installed: the server restarts once its agents have finished.
    Waiting,
    /// Starting the new version.
    Restarting,
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

/// A message sent while the agent was working. It joins the transcript when the agent is given it.
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
    /// Always false: what the agent is given leaves the queue. Older clients still read it.
    #[serde(default)]
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
    /// The server's details, whenever its update moves on.
    Server {
        server: ServerInfo,
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

    /// First answer to `Open`. With `reset` the client's copy can't be caught up and is thrown
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
    Linear {
        connections: Vec<LinearConnection>,
    },
    LinearAuthorize {
        url: String,
    },
    LinearTeams {
        teams: Vec<LinearTeam>,
        users: Vec<LinearUser>,
    },
    LinearIssues {
        issues: Vec<LinearIssue>,
    },
    LinearIssue {
        issue: LinearIssue,
    },
    LinearIssueDetail {
        detail: Box<LinearIssueDetail>,
    },
    /// Local branches first, then the ones only on the remote.
    Branches {
        branches: Vec<Branch>,
    },
    /// `start` is the base, or the remote's base, like `origin/main`, when the remote has commits
    /// the local one lacks. `problem` is what git said when the remote couldn't be asked.
    WorktreeStart {
        start: String,
        problem: Option<String>,
    },
    /// `status` is missing when the folder is no repository. `problem` is what git said when the
    /// remote couldn't be fetched from.
    GitStatus {
        status: Option<GitStatus>,
        files: Vec<ChangedFile>,
        #[serde(default)]
        problem: Option<String>,
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
    PullRequest {
        pull_request: Box<PullRequestDetail>,
    },
    PullRequests {
        pull_requests: Vec<PullRequestSummary>,
    },
    /// What a `PullRequestAction` did, in words to show: "Merged PR #7". `url` is the pull
    /// request it opened, if it opened one.
    PullRequestDone {
        title: String,
        url: Option<String>,
        pull_request: Box<PullRequestDetail>,
    },
    Uploaded {
        path: String,
    },
    /// How far the download of the server's update is. `total` is missing when it isn't known.
    Updating {
        received: u64,
        total: Option<u64>,
    },
    /// The update is installed, and the server restarts once its agents have finished.
    UpdateWaiting,
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
    /// `sent` bytes of a file of `size` bytes follow: all of an image or a video, and the start
    /// of a long text.
    File {
        kind: FileKind,
        size: u64,
        sent: u64,
    },
    /// A project's icon: the file's bytes in base64.
    Icon {
        data: String,
    },
    Usage {
        buckets: Vec<UsageBucket>,
    },
    Limits {
        agents: Vec<AgentLimits>,
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

        assert_eq!(update, Request::UpdateServer { when: None });
        assert!(matches!(welcome, Message::Welcome { server, .. } if server.version == "0.1.6"));
        assert_eq!(kind, crate::auth_api::DeviceKind::Server);
    }

    #[test]
    fn a_stage_of_a_newer_server_is_read_as_unknown() {
        let stage: GitStage = serde_json::from_value(serde_json::json!("squashing")).unwrap();

        assert_eq!(stage, GitStage::Unknown);
    }
}
