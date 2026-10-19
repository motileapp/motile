//! What the app keeps of what the core sends: its views of servers, projects and threads, with
//! the lookups the screens need, and what is read out of the answers to commands.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use motile_core::api::{Children, ProjectView, ServerView, ThreadView};
use motile_core::connection::PathKind;
use motile_core::git::Control;
use motile_core::link::State;
use motile_core::render::agents::AgentView;
use motile_core::render::rows::Waiting;
use motile_protocol::wire::{
    Access, Agent, AgentAccount, Ask, ContinueSettings, GitStatus, Interruption, ModelInfo, PullRequest,
    PullRequestSettings, Queued, Thread,
};
use serde::Deserialize;

pub fn agent_name(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "Claude Code",
        Agent::Codex => "Codex",
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Server {
    pub id: String,
    pub name: String,
    /// The name cut short, for the places a long name crowds.
    pub short_name: String,
    pub platform: String,
    /// The server was removed from the account: its threads are copies, read from the other
    /// servers, which go on only on another server.
    pub removed: bool,
    pub state: State,
    pub error: Option<String>,
    /// "relay" or "direct".
    pub path: Option<String>,
    pub rtt_ms: Option<u64>,
    pub home: String,
    /// The version of the server's program.
    pub version: String,
    pub protocol_version: u32,
    pub models: Vec<ModelInfo>,
    /// The shorter names of its models, by model id, for the model picker.
    pub short_model_names: HashMap<String, String>,
    /// The model picked to write titles, commit messages and pull requests there.
    pub text_model: Option<String>,
    /// How the writer there is told to name branches, and what that is until it is changed.
    pub branch_instructions: String,
    pub default_branch_instructions: String,
    /// What the server does with pull requests by itself.
    pub pull_request_settings: PullRequestSettings,
    /// What the server goes on with by itself: threads whose usage limit reset, and threads whose
    /// agents worked when it restarted.
    pub continue_settings: ContinueSettings,
    /// The agents installed on the server, with their versions.
    pub agents: HashMap<Agent, String>,
    /// The agents' accounts, the default ones first.
    pub agent_accounts: Vec<AgentAccount>,
    /// The account a new thread is better off starting on, by account id and model id.
    pub roomier_accounts: HashMap<String, HashMap<String, String>>,
    /// Whether the server has ever told us about itself.
    pub known: bool,
    /// The update the server is putting in place, as it last said.
    pub update: Option<ServerUpdate>,
}

impl From<ServerView> for Server {
    fn from(view: ServerView) -> Self {
        let info = view.info;
        let path = view.path.map(|path| match path {
            PathKind::Relay => "relay".to_string(),
            PathKind::Direct => "direct".to_string(),
        });
        let version = info.as_ref().map(|info| info.version.clone()).unwrap_or_default();
        let accounts = info.as_ref().map(|info| info.agent_accounts.clone()).unwrap_or_default();
        Self {
            id: view.id,
            name: view.name,
            short_name: view.short_name,
            platform: view.platform,
            removed: view.removed,
            state: view.state,
            error: view.error,
            path,
            rtt_ms: view.rtt_ms,
            known: info.is_some(),
            home: info.as_ref().map(|info| info.home.clone()).unwrap_or_default(),
            update: info.as_ref().and_then(|info| info.update).map(|update| ServerUpdate::reported(update, &version)),
            version,
            protocol_version: info.as_ref().map_or(0, |info| info.protocol),
            models: info.as_ref().map(|info| info.models.clone()).unwrap_or_default(),
            short_model_names: view.short_model_names,
            roomier_accounts: view.roomier_accounts,
            text_model: info.as_ref().and_then(|info| info.text_model.clone()),
            branch_instructions: info.as_ref().map(|info| info.branch_instructions.text.clone()).unwrap_or_default(),
            default_branch_instructions: info
                .as_ref()
                .map(|info| info.branch_instructions.default.clone())
                .unwrap_or_default(),
            pull_request_settings: info.as_ref().map(|info| info.pull_request_settings).unwrap_or_default(),
            continue_settings: info.as_ref().map(|info| info.continue_settings).unwrap_or_default(),
            agents: info
                .map(|info| info.agents.into_iter().filter_map(|agent| Some((agent.agent, agent.version?))).collect())
                .unwrap_or_default(),
            agent_accounts: if accounts.is_empty() {
                AGENTS.into_iter().map(default_account).collect()
            } else {
                accounts
            },
        }
    }
}

/// Every agent, in the order they are offered.
pub const AGENTS: [Agent; 2] = [Agent::Claude, Agent::Codex];

/// The agent's own name for its default account, which every server has.
pub fn default_account_id(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude",
        Agent::Codex => "codex",
    }
}

/// The account a server from before accounts has for each agent.
fn default_account(agent: Agent) -> AgentAccount {
    AgentAccount {
        id: default_account_id(agent).into(),
        agent,
        name: "Default".into(),
        folder: String::new(),
        shares_sessions: false,
        variables: Vec::new(),
        email: None,
        plan: None,
    }
}

/// A new account of `agent`, until the server keeps it.
pub fn new_account(agent: Agent) -> AgentAccount {
    AgentAccount {
        id: String::new(),
        agent,
        name: String::new(),
        folder: String::new(),
        shares_sessions: false,
        variables: Vec::new(),
        email: None,
        plan: None,
    }
}

/// What the app asks of an agent's account.
pub trait AgentAccountExt {
    /// The account every agent has, which uses the agent's usual folder.
    fn is_default(&self) -> bool;
    /// Who is signed in, or how it signs in, and its folder: what the settings say of it.
    fn settings_description(&self) -> String;
    /// The variable the account's folder is given to its agent as.
    fn folder_variable(&self) -> &'static str;
    /// The folder the name suggests, as `~/.claude-personal`, with a number when another account
    /// keeps that one.
    fn suggested_folder(&self, besides: &[AgentAccount]) -> String;
    /// What signs the account in, run on its server.
    fn sign_in_command(&self) -> String;
}

impl AgentAccountExt for AgentAccount {
    fn is_default(&self) -> bool {
        self.id == default_account_id(self.agent)
    }

    fn settings_description(&self) -> String {
        let signed_in = self.email.as_ref().map(|email| match &self.plan {
            Some(plan) => format!("{email} · {plan}"),
            None => email.clone(),
        });
        let who = signed_in.unwrap_or_else(|| {
            if self.variables.is_empty() { "Not signed in".into() } else { "Signs in with its variables".into() }
        });
        if self.folder.is_empty() { who } else { format!("{who} · {}", self.folder) }
    }

    fn folder_variable(&self) -> &'static str {
        if self.agent == Agent::Claude { "CLAUDE_CONFIG_DIR" } else { "CODEX_HOME" }
    }

    fn suggested_folder(&self, besides: &[AgentAccount]) -> String {
        if self.name.trim().is_empty() {
            return String::new();
        }
        let name = self.name.to_lowercase();
        let words: Vec<&str> =
            name.split(|character: char| !character.is_ascii_alphanumeric()).filter(|word| !word.is_empty()).collect();
        let agent = if self.agent == Agent::Claude { "claude" } else { "codex" };
        let base = format!("~/.{agent}-{}", if words.is_empty() { "account".into() } else { words.join("-") });
        (1..)
            .map(|number| if number == 1 { base.clone() } else { format!("{base}-{number}") })
            .find(|folder| besides.iter().all(|account| &account.folder != folder))
            .unwrap_or(base)
    }

    fn sign_in_command(&self) -> String {
        let folder =
            if self.folder.is_empty() { String::new() } else { format!("{}={} ", self.folder_variable(), self.folder) };
        let login = if self.agent == Agent::Claude { "claude auth login" } else { "codex login" };
        format!("{folder}{login}")
    }
}

/// What the app asks of a model.
pub trait ModelInfoExt {
    /// Whether the account's agent lists it. A model of a server that lists models for an agent
    /// as a whole runs under each of its accounts.
    fn runs_under(&self, account: &AgentAccount) -> bool;
    /// What the effort menu offers: the efforts, with the agent's own default as "" ahead of them
    /// only when the server couldn't say which one that is.
    fn effort_choices(&self) -> Vec<String>;
}

impl ModelInfoExt for ModelInfo {
    fn runs_under(&self, account: &AgentAccount) -> bool {
        self.agent == account.agent && (self.account.is_empty() || self.account == account.id)
    }

    fn effort_choices(&self) -> Vec<String> {
        if self.efforts.is_empty() || self.default_effort.is_some() {
            return self.efforts.clone();
        }
        std::iter::once(String::new()).chain(self.efforts.iter().cloned()).collect()
    }
}

impl Server {
    pub fn connected(&self) -> bool {
        self.state == State::Connected
    }

    /// The colour of the server's state while it isn't connected.
    pub fn state_tint(&self, c: &crate::theme::Colors) -> Option<gpui_kit::Hsla> {
        match self.state {
            State::Connected => None,
            State::Connecting => Some(c.pending),
            State::Disconnected | State::Refused => Some(c.destructive),
        }
    }

    /// Whether the server can wait for its agents before it restarts, or stop them and go on with
    /// them after.
    pub fn can_choose_restart(&self) -> bool {
        self.protocol_version >= 14
    }

    /// Whether threads there can move between accounts and agents.
    pub fn switches_accounts(&self) -> bool {
        self.protocol_version >= 19
    }

    /// Whether it signs its agents' accounts in for the client.
    pub fn signs_in_accounts(&self) -> bool {
        self.protocol_version >= 31
    }

    /// Whether the server keeps where the user moves its threads in the sidebar.
    pub fn moves_threads(&self) -> bool {
        self.protocol_version >= 21
    }

    /// Whether the server asks its agents for their models when asked to.
    pub fn refreshes_models(&self) -> bool {
        self.protocol_version >= 23
    }

    /// Whether the server can keep its projects' folders pulled on their default branch.
    pub fn pulls_default_branch(&self) -> bool {
        self.protocol_version >= 25
    }

    /// The model's name without its maker, for the model picker.
    pub fn short_name<'a>(&'a self, model: &'a ModelInfo) -> &'a str {
        self.short_model_names.get(&model.id).map_or(model.name.as_str(), String::as_str)
    }

    pub fn accounts(&self, agent: Agent) -> Vec<&AgentAccount> {
        self.agent_accounts.iter().filter(|account| account.agent == agent).collect()
    }

    /// Each model once, however many accounts run it, for picking one by its id.
    pub fn distinct_models(&self) -> Vec<&ModelInfo> {
        let mut seen = std::collections::HashSet::new();
        self.models.iter().filter(|model| seen.insert(model.id.as_str())).collect()
    }

    /// The account with that id, or the agent's default one.
    pub fn account(&self, id: Option<&str>, agent: Agent) -> Option<&AgentAccount> {
        let accounts = self.accounts(agent);
        let named = accounts.iter().find(|account| Some(account.id.as_str()) == id);
        named.or_else(|| accounts.iter().find(|account| account.is_default())).or(accounts.first()).copied()
    }

    /// The model with that id as the account lists it, or else the first one offered.
    pub fn model<'a>(id: Option<&str>, account: Option<&str>, offered: &'a [ModelInfo]) -> Option<&'a ModelInfo> {
        let listed_by_account =
            |model: &&ModelInfo| model.account.is_empty() || Some(model.account.as_str()) == account;
        let named: Vec<&ModelInfo> = offered.iter().filter(|model| Some(model.id.as_str()) == id).collect();
        named
            .iter()
            .find(|model| listed_by_account(model))
            .or(named.first())
            .copied()
            .or_else(|| offered.iter().find(listed_by_account))
            .or(offered.first())
    }

    /// The model a new thread with these choices would start with, and the account of its agent
    /// that has used clearly less of its limits and runs it, when there is one.
    pub fn roomier_account(&self, model: Option<&str>, account: Option<&str>) -> Option<(&ModelInfo, &str)> {
        let model = Server::model(model, account, &self.models)?;
        let account = self.account(account, model.agent)?;
        let roomier = self.roomier_accounts.get(&account.id)?.get(&model.id)?;
        Some((model, roomier.as_str()))
    }

    /// The model's name, with the account's when its agent has more than one.
    pub fn label(&self, model: &ModelInfo, account: Option<&str>) -> String {
        let short = self.short_name(model);
        if self.accounts(model.agent).len() < 2 {
            return short.to_string();
        }
        let Some(account) = self.account(account, model.agent) else { return short.to_string() };
        format!("{short} · {}", account.name)
    }
}

/// A server that is installing a new version of itself.
#[derive(Clone, PartialEq, Debug)]
pub struct ServerUpdate {
    /// The version it had when the update began.
    pub from: String,
    /// How much of the download has arrived, when the server knows how much there is.
    pub fraction: Option<f64>,
    /// The new version is installed, and the server starts it once its agents have finished.
    pub waiting: bool,
    /// The new version is installed and the server is starting it.
    pub restarting: bool,
}

impl ServerUpdate {
    pub fn new(from: &str) -> Self {
        Self { from: from.to_string(), fraction: None, waiting: false, restarting: false }
    }

    /// The update as the server itself reports it.
    fn reported(update: motile_protocol::wire::ServerUpdate, from: &str) -> Self {
        use motile_protocol::wire::ServerUpdate as Reported;
        let mut shown = Self::new(from);
        match update {
            Reported::Installing { percent } => shown.fraction = percent.map(|percent| f64::from(percent) / 100.),
            Reported::Waiting => shown.waiting = true,
            Reported::Restarting => shown.restarting = true,
        }
        shown
    }
}

/// A git worktree of a project, made for the thread whose `cwd` it is.
#[derive(Clone, PartialEq, Debug)]
pub struct Worktree {
    pub path: String,
    pub branch: Option<String>,
    pub git: Option<GitStatus>,
    pub git_control: Option<Control>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Project {
    pub id: String,
    pub server_id: String,
    pub path: String,
    pub name: String,
    pub branch: Option<String>,
    /// What its server last read from git there.
    pub git: Option<GitStatus>,
    /// The git button for its repository.
    pub git_control: Option<Control>,
    /// The worktree of the thread the project is seen from, when it works in one.
    pub worktree: Option<Worktree>,
    pub worktrees: Vec<Worktree>,
    /// The shell script that runs in every new worktree.
    pub setup: Option<String>,
    /// The icon as a file on this device, once the core has fetched it.
    pub icon_path: Option<String>,
    /// The project of its server that threads start in without one, each in a folder of its own.
    pub no_project: bool,
    pub created_at: f64,
}

impl Project {
    pub fn new(view: ProjectView, server_id: &str) -> Self {
        let mut controls = view.worktree_controls;
        let project = view.project;
        Self {
            worktrees: project
                .worktrees
                .into_iter()
                .map(|worktree| Worktree {
                    git_control: controls.remove(&worktree.path),
                    path: worktree.path,
                    branch: worktree.branch,
                    git: worktree.git,
                })
                .collect(),
            id: project.id,
            server_id: server_id.to_string(),
            path: project.path,
            name: project.name,
            branch: project.branch,
            git: project.git,
            git_control: view.git_control,
            worktree: None,
            setup: project.setup,
            icon_path: view.icon_path,
            no_project: project.no_project,
            created_at: project.created_at,
        }
    }

    /// How a new thread's headline reads with the project picked: "Let’s build in motile", or
    /// "Let’s build without a project".
    pub fn headline(project: Option<&Project>) -> (&'static str, String) {
        match project {
            None => ("Let’s build in", "a project".into()),
            Some(project) if project.no_project => ("Let’s build", "without a project".into()),
            Some(project) => ("Let’s build in", project.name.clone()),
        }
    }

    /// The symbol a menu shows for it when it has no icon.
    pub fn symbol(&self) -> &'static str {
        if self.no_project { "message-square-dashed" } else { "folder" }
    }

    /// The symbol of where a thread works in it: a detached head, a worktree or a branch.
    pub fn checkout_symbol(&self) -> &'static str {
        if self.git.as_ref().is_some_and(|git| git.branch.is_none()) {
            return "git-commit-horizontal";
        }
        if self.worktree.is_some() { "folder-git-2" } else { "git-branch" }
    }

    /// A thread's pull request: its own, or the one of the worktree it works in.
    pub fn pull_request(&self, thread: &ThreadInfo) -> Option<PullRequest> {
        if let Some(pull_request) = &thread.pull_request {
            return Some(pull_request.clone());
        }
        self.worktrees.iter().find(|worktree| worktree.path == thread.cwd)?.git.as_ref()?.pull_request.clone()
    }

    /// Names the checkout git runs in from here: the project's folder or the thread's worktree.
    pub fn checkout_id(&self) -> String {
        format!("{}:{}", self.id, self.worktree.as_ref().map(|worktree| worktree.path.as_str()).unwrap_or(&self.path))
    }

    /// The project as the thread works in it: with the branch and the git state of its worktree,
    /// when it has one.
    pub fn seen(&self, thread: &ThreadInfo) -> Project {
        let Some(worktree) = self.worktrees.iter().find(|worktree| worktree.path == thread.cwd) else {
            return self.clone();
        };
        let mut seen = self.clone();
        seen.branch = worktree.branch.clone();
        seen.git = worktree.git.clone();
        seen.git_control = worktree.git_control.clone();
        seen.worktree = Some(worktree.clone());
        seen
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct ThreadInfo {
    pub thread: Thread,
    pub server_id: String,
    pub unread: bool,
    /// How the agents its agent started are doing, theirs too.
    pub children: Option<Children>,
}

impl From<ThreadView> for ThreadInfo {
    fn from(view: ThreadView) -> Self {
        Self { thread: view.thread, server_id: view.server_id, unread: view.unread, children: view.children }
    }
}

impl std::ops::Deref for ThreadInfo {
    type Target = Thread;

    fn deref(&self) -> &Thread {
        &self.thread
    }
}

impl ThreadInfo {
    pub fn is_done(&self) -> bool {
        self.done_at.is_some()
    }

    /// The agent's process is still there, working or monitoring.
    pub fn busy(&self) -> bool {
        self.running || self.monitoring
    }

    /// The thread whose agent started this one. The sidebar lists only threads without one.
    pub fn parent_id(&self) -> Option<&str> {
        self.parent.as_ref().map(|parent| parent.thread_id.as_str())
    }

    /// It works, or an agent it started does.
    pub fn working(&self) -> bool {
        self.running || self.children.as_ref().is_some_and(|children| children.running > 0)
    }

    /// Its agent ended its turn, and the agents it started work on.
    pub fn waits_on_agents(&self) -> bool {
        !self.running && self.children.as_ref().is_some_and(|children| children.running > 0)
    }

    /// Its agent's process is still there or the agents it started work, so it can't be done yet.
    pub fn unfinished(&self) -> bool {
        self.busy() || self.waits_on_agents()
    }

    /// What it, or else an agent it started, waits for the user to answer.
    pub fn waits_for(&self) -> Option<Ask> {
        self.asking().or_else(|| self.children.as_ref()?.waiting.first().map(|child| child.ask))
    }

    /// Where it is listed among the active threads, the highest first: when it was created or
    /// last came back from done, until the user moves it. A server from before positions lists
    /// its threads by when they were created.
    pub fn position(&self) -> f64 {
        if self.position > 0. { self.position } else { self.created_at }
    }

    /// What its list is in order of, the highest first: when it was marked done, or else its
    /// position. Active threads keep their place when something happens in them.
    pub fn listed_at(&self) -> f64 {
        if self.is_done() { self.done_at.unwrap_or(0.) } else { self.position() }
    }

    /// The order of the sidebar's lists: the highest `listed_at` first, then the highest id.
    pub fn listed_order(one: &ThreadInfo, other: &ThreadInfo) -> std::cmp::Ordering {
        other.listed_at().total_cmp(&one.listed_at()).then_with(|| other.id.cmp(&one.id))
    }

    /// What the tool call that waits for the user asks of them. A server from before `asking`
    /// only says that something waits.
    pub fn asking(&self) -> Option<Ask> {
        self.needs_approval.then(|| self.thread.asking.unwrap_or(Ask::Approval))
    }

    pub fn monitoring_since(&self) -> f64 {
        self.turn_ended_at.unwrap_or(self.updated_at)
    }
}

/// What a thread's status says while it waits for the user: its word and its symbol.
pub fn ask_label(ask: Ask) -> (&'static str, &'static str) {
    match ask {
        Ask::Approval => ("Approval", "shield"),
        Ask::Question => ("Question", "message-circle-question-mark"),
        Ask::Plan => ("Plan", "clipboard-list"),
    }
}

/// What a thread is said to do while it waits for the user.
pub fn ask_phrase(ask: Ask) -> &'static str {
    match ask {
        Ask::Approval => "needs approval",
        Ask::Question => "has a question",
        Ask::Plan => "has a plan",
    }
}

/// What a thread's status says of why its agent stopped: its word and its symbol.
pub fn interruption_label(interruption: Interruption) -> (&'static str, &'static str) {
    match interruption {
        Interruption::Limit { .. } => ("Limited", "clock"),
        Interruption::Restart => ("Interrupted", "refresh-cw"),
    }
}

/// What the strip above the composer calls why the agent stopped.
pub fn interruption_title(interruption: Interruption) -> &'static str {
    match interruption {
        Interruption::Limit { .. } => "Usage limit reached",
        Interruption::Restart => "Interrupted",
    }
}

/// When the thread goes on, or what it waits for to be continued.
pub fn interruption_detail(interruption: Interruption, at: f64) -> String {
    match interruption {
        Interruption::Restart => "Your server restarted before the agent finished".into(),
        Interruption::Limit { resets_at: Some(resets_at), continues } if resets_at > at => {
            let stamp = crate::transcript::rows::stamp(resets_at);
            if continues { format!("Continues at {stamp}") } else { format!("Resets at {stamp}") }
        }
        Interruption::Limit { resets_at: Some(_), continues: true } => "Continues in a moment".into(),
        Interruption::Limit { resets_at: Some(_), continues: false } => "The limit has reset".into(),
        Interruption::Limit { resets_at: None, .. } => "Reset time unknown".into(),
    }
}

/// The path with its server's home as `~`.
pub fn shortened(path: &str, home: &str) -> String {
    if home.is_empty() {
        return path.to_string();
    }
    let Some(rest) = path.strip_prefix(home) else { return path.to_string() };
    if !rest.is_empty() && !rest.starts_with('/') {
        return path.to_string();
    }
    format!("~{rest}")
}

pub fn access_label(access: Access) -> &'static str {
    match access {
        Access::Supervised => "Supervised",
        Access::AcceptEdits => "Auto-accept edits",
        Access::Auto => "Auto",
        Access::Full => "Full access",
    }
}

pub fn access_detail(access: Access) -> &'static str {
    match access {
        Access::Supervised => "Ask before commands and file changes.",
        Access::AcceptEdits => "Auto-approve edits, ask before other actions.",
        Access::Auto => "The agent approves routine actions itself.",
        Access::Full => "Allow commands and edits without prompts.",
    }
}

pub const ACCESSES: [Access; 4] = [Access::Supervised, Access::AcceptEdits, Access::Auto, Access::Full];

/// What the open thread's agent is doing, and what waits for the user.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Activity {
    pub running: bool,
    pub monitoring: bool,
    pub thinking: bool,
    /// The agent is making its conversation shorter to go on with it.
    pub compacting: bool,
    /// How many agents it has started that still work.
    pub agents: u32,
    pub started_at: Option<f64>,
    /// The tool calls the running turn waits with until they are allowed or refused.
    pub approvals: Vec<Waiting>,
    /// The messages that wait for the agent to take them.
    pub queued: Vec<Queued>,
}

impl Activity {
    pub fn new(activity: motile_protocol::wire::Activity, waiting: Vec<Waiting>) -> Self {
        Self {
            running: activity.running,
            monitoring: activity.monitoring,
            thinking: activity.thinking,
            compacting: activity.compacting,
            agents: activity.agents,
            started_at: activity.started_at,
            approvals: waiting,
            // What the agent is told of the agents it started isn't the user's to send or take back.
            queued: activity.queued.into_iter().filter(|queued| queued.notice.is_none()).collect(),
        }
    }

    pub fn busy(&self) -> bool {
        self.running || self.monitoring
    }

    /// What is shown between sending a first message and the server saying the turn runs.
    pub fn starting() -> Self {
        Self { running: true, started_at: Some(now()), ..Self::default() }
    }
}

pub trait AgentViewExt {
    fn working(&self) -> bool;
    fn usage(&self) -> Option<String>;
    fn facts(&self) -> Option<String>;
}

impl AgentViewExt for AgentView {
    fn working(&self) -> bool {
        self.status == motile_protocol::wire::ToolStatus::Running
    }

    /// "3 tools · 21.4K tokens", as far as either is known.
    fn usage(&self) -> Option<String> {
        let tools = self.tool_uses.map(|count| format!("{count} {}", if count == 1 { "tool" } else { "tools" }));
        let parts: Vec<String> = [tools, self.tokens.map(tokens)].into_iter().flatten().collect();
        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    /// "6.1-Sol · work · 3 tools · 21.4K tokens", as far as each is known.
    fn facts(&self) -> Option<String> {
        let parts: Vec<String> =
            [self.model.clone(), self.account.clone(), self.usage()].into_iter().flatten().collect();
        (!parts.is_empty()).then(|| parts.join(" · "))
    }
}

/// "820 tokens", "21.4K tokens", "4.57M tokens".
pub fn tokens(count: u64) -> String {
    format!("{} tokens", crate::format::abbreviated(count))
}

/// A file attached to a message, as the transcript shows it. An image or a video names the file
/// the core has it under, and a video the image that stands for it until it plays.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct AttachedFile {
    pub name: String,
    pub media: Option<String>,
    pub video: bool,
    pub poster: Option<String>,
}

impl AttachedFile {
    /// The image a tile of it shows.
    pub fn picture(&self) -> Option<&String> {
        if self.video { self.poster.as_ref() } else { self.media.as_ref() }
    }
}

impl From<&motile_core::render::rows::Attached> for AttachedFile {
    fn from(attached: &motile_core::render::rows::Attached) -> Self {
        Self {
            name: attached.name.clone(),
            media: attached.media.clone(),
            video: attached.video,
            poster: attached.poster.clone(),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum UploadState {
    Uploading(f64),
    Ready,
    Failed(String),
}

/// A file in the composer: on its way to the server, or there and ready to be sent.
#[derive(Clone, PartialEq, Debug)]
pub struct Attachment {
    pub id: String,
    /// Where it is on this device. A file that came back from a queued message is only on its
    /// server.
    pub file: Option<PathBuf>,
    pub name: String,
    pub bytes: Option<u64>,
    pub video: bool,
    /// Shown as a tile with its picture, not by its name.
    pub pictured: bool,
    pub server_id: String,
    pub state: UploadState,
    /// Where it is on the server, once it is there.
    pub path: Option<String>,
    pub media: Option<String>,
    pub poster: Option<String>,
}

pub fn is_video(path: &Path) -> bool {
    let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or_default().to_lowercase();
    matches!(extension.as_str(), "mov" | "mp4" | "m4v" | "avi" | "mkv" | "webm" | "mpg" | "mpeg" | "3gp")
}

pub fn is_image(path: &Path) -> bool {
    let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or_default().to_lowercase();
    matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" | "bmp" | "tiff" | "tif" | "ico" | "svg"
    )
}

impl Attachment {
    pub fn from_file(file: PathBuf, server_id: &str) -> Self {
        let video = is_video(&file);
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: file.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default(),
            bytes: std::fs::metadata(&file).ok().filter(|metadata| metadata.is_file()).map(|metadata| metadata.len()),
            video,
            pictured: video || is_image(&file),
            file: Some(file),
            server_id: server_id.to_string(),
            state: UploadState::Uploading(0.),
            path: None,
            media: None,
            poster: None,
        }
    }

    pub fn from_server(path: &str, shown: Option<&AttachedFile>, server_id: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            file: None,
            name: Path::new(path).file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default(),
            bytes: None,
            video: shown.is_some_and(|shown| shown.video),
            pictured: shown.is_some(),
            server_id: server_id.to_string(),
            state: UploadState::Ready,
            path: Some(path.to_string()),
            media: shown.and_then(|shown| shown.media.clone()),
            poster: shown.and_then(|shown| shown.poster.clone()),
        }
    }

    pub fn attached(&self) -> AttachedFile {
        AttachedFile {
            name: self.name.clone(),
            media: self.media.clone(),
            video: self.media.is_some() && self.video,
            poster: self.poster.clone(),
        }
    }

    pub fn viewed(&self) -> Option<ViewedMedia> {
        if let Some(file) = &self.file {
            return Some(ViewedMedia {
                name: self.name.clone(),
                video: self.video,
                source: MediaSource::File(file.clone()),
            });
        }
        self.media.as_ref().map(|media| ViewedMedia {
            name: self.name.clone(),
            video: self.video,
            source: MediaSource::Media(media.clone()),
        })
    }
}

/// An image or a video the viewer shows: a file on this device, or one the core has or fetches.
#[derive(Clone, PartialEq, Debug)]
pub enum MediaSource {
    File(PathBuf),
    Media(String),
}

#[derive(Clone, PartialEq, Debug)]
pub struct ViewedMedia {
    pub name: String,
    pub video: bool,
    pub source: MediaSource,
}

/// What the viewer has open: the images and videos of one message or of the composer, and
/// which of them is shown.
#[derive(Clone, PartialEq, Debug)]
pub struct Viewing {
    pub items: Vec<ViewedMedia>,
    pub index: usize,
}

impl Viewing {
    pub fn item(&self) -> &ViewedMedia {
        &self.items[self.index]
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct EnrollToken {
    pub command: String,
    pub expires_at: f64,
    /// The command's colours, as the core's `[start, length, palette index]` spans.
    #[serde(default)]
    pub spans: Vec<u32>,
}

impl EnrollToken {
    pub fn is_expired(&self, at: f64) -> bool {
        self.expires_at <= at
    }

    /// How long the command still works, as "14:59".
    pub fn time_left(&self, at: f64) -> String {
        let seconds = (self.expires_at - at).ceil().max(0.) as u64;
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

/// The folders of a server under the path typed in the panel.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct FolderListing {
    pub path: String,
    pub typed: String,
    pub parent: Option<String>,
    pub folders: Vec<ListedFolder>,
    /// The images that can be a project's icon, when one is being chosen.
    #[serde(default)]
    pub images: Vec<ListedImage>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ListedImage {
    pub name: String,
    pub path: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ListedFolder {
    pub name: String,
    pub path: String,
    /// What to type to look inside it.
    pub typed: String,
}

pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        .unwrap_or(0.)
}

/// "now", "5m", "3h", "2d": how long ago, as short as a sidebar needs.
pub fn ago(timestamp: f64) -> String {
    let seconds = (now() - timestamp).max(0.);
    if seconds < 60. {
        return "now".into();
    }
    if seconds < 3600. {
        return format!("{}m", (seconds / 60.) as u64);
    }
    if seconds < 86400. {
        return format!("{}h", (seconds / 3600.) as u64);
    }
    format!("{}d", (seconds / 86400.) as u64)
}

/// "850ms", "12s", "3m 05s", "1h 02m".
pub fn duration(milliseconds: u64) -> String {
    if milliseconds < 1000 {
        return format!("{milliseconds}ms");
    }
    let seconds = milliseconds / 1000;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    minutes(seconds)
}

/// "3m 05s", "1h 02m": the second unit always takes two digits, so a timer doesn't jump.
fn minutes(seconds: u64) -> String {
    if seconds < 3600 {
        return format!("{}m {:02}s", seconds / 60, seconds % 60);
    }
    format!("{}h {:02}m", seconds / 3600, seconds % 3600 / 60)
}

/// A running timer: "5s", "12m 03s", "1h 03m".
pub fn elapsed(since: f64) -> String {
    let seconds = (now() - since).max(0.) as u64;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    minutes(seconds)
}

/// Whether `version` comes before `other`, comparing their numbers from the left.
pub fn is_older(version: &str, other: Option<&str>) -> bool {
    let Some(other) = other else { return false };
    if version.is_empty() || other.is_empty() {
        return false;
    }
    let numbers = |text: &str| text.split('.').map(|part| part.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (ours, theirs) = (numbers(version), numbers(other));
    for index in 0..ours.len().max(theirs.len()) {
        let left = ours.get(index).copied().unwrap_or(0);
        let right = theirs.get(index).copied().unwrap_or(0);
        if left != right {
            return left < right;
        }
    }
    false
}

/// How what the user reads names the computer the client runs on.
pub const THIS_DEVICE: &str = if cfg!(target_os = "macos") { "this Mac" } else { "this computer" };

/// The file manager's command that shows a file where it is.
pub const REVEAL_IN_FOLDER: &str = if cfg!(target_os = "macos") {
    "Show in Finder"
} else if cfg!(target_os = "windows") {
    "Show in Explorer"
} else {
    "Show in Folder"
};

pub fn last_component(path: &str) -> String {
    Path::new(path).file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_account_suggests_a_folder_from_its_name() {
        let mut account = new_account(Agent::Claude);
        account.name = "Work Stuff!".into();
        assert_eq!(account.suggested_folder(&[]), "~/.claude-work-stuff");
        let mut taken = new_account(Agent::Claude);
        taken.folder = "~/.claude-work-stuff".into();
        assert_eq!(account.suggested_folder(&[taken]), "~/.claude-work-stuff-2");
        account.name = "Été".into();
        assert_eq!(account.suggested_folder(&[]), "~/.claude-t");
        account.name = "  ".into();
        assert_eq!(account.suggested_folder(&[]), "");
        account.agent = Agent::Codex;
        account.folder = "~/.codex-work".into();
        assert_eq!(account.sign_in_command(), "CODEX_HOME=~/.codex-work codex login");
        assert_eq!(new_account(Agent::Claude).sign_in_command(), "claude auth login");
    }

    #[test]
    fn an_account_says_who_is_signed_in() {
        let mut account = new_account(Agent::Claude);
        assert_eq!(account.settings_description(), "Not signed in");
        account.email = Some("a@b.c".into());
        account.plan = Some("Max".into());
        account.folder = "~/.claude-work".into();
        assert_eq!(account.settings_description(), "a@b.c · Max · ~/.claude-work");
    }

    #[test]
    fn versions_compare_by_their_numbers() {
        assert!(is_older("0.1.9", Some("0.1.12")));
        assert!(!is_older("0.1.12", Some("0.1.12")));
        assert!(!is_older("0.2", Some("0.1.12")));
        assert!(is_older("0.1", Some("0.1.1")));
        assert!(!is_older("", Some("0.1.1")));
        assert!(!is_older("0.1", None));
    }

    #[test]
    fn an_enroll_token_counts_down_to_its_expiry() {
        let token = EnrollToken { command: String::new(), expires_at: 1000., spans: Vec::new() };
        assert_eq!(token.time_left(100.5), "15:00");
        assert_eq!(token.time_left(941.), "0:59");
        assert_eq!(token.time_left(1200.), "0:00");
        assert!(!token.is_expired(999.));
        assert!(token.is_expired(1000.));
    }

    fn model(id: &str, account: &str) -> ModelInfo {
        ModelInfo {
            id: id.into(),
            name: id.into(),
            agent: Agent::Claude,
            account: account.into(),
            efforts: Vec::new(),
            default_effort: None,
        }
    }

    #[test]
    fn a_model_is_found_as_its_account_lists_it() {
        let offered = [model("opus", "work"), model("opus", "claude"), model("sonnet", "")];
        assert_eq!(Server::model(Some("opus"), Some("claude"), &offered), Some(&offered[1]));
        assert_eq!(Server::model(Some("opus"), Some("home"), &offered), Some(&offered[0]));
        assert_eq!(Server::model(Some("haiku"), Some("home"), &offered), Some(&offered[2]));
        assert_eq!(Server::model(None, None, &[]), None);
    }

    #[test]
    fn a_new_thread_moves_to_the_roomier_account_that_runs_its_model() {
        let account = |id: &str| AgentAccount { id: id.into(), name: id.into(), ..new_account(Agent::Claude) };
        let roomier =
            HashMap::from([("claude".to_string(), HashMap::from([("opus".to_string(), "work".to_string())]))]);
        let server = Server::from(ServerView {
            id: "key".into(),
            name: "studio".into(),
            short_name: "studio".into(),
            platform: "macos".into(),
            removed: false,
            state: State::Connected,
            error: None,
            path: None,
            rtt_ms: None,
            info: None,
            short_model_names: HashMap::new(),
            roomier_accounts: roomier,
        });
        let server = Server {
            models: vec![model("opus", ""), model("sonnet", "")],
            agent_accounts: vec![account("claude"), account("work")],
            ..server
        };
        let found = |model, account| server.roomier_account(model, account).map(|(model, to)| (model.id.as_str(), to));
        assert_eq!(found(Some("opus"), Some("claude")), Some(("opus", "work")));
        assert_eq!(found(None, None), Some(("opus", "work")));
        assert_eq!(found(Some("opus"), Some("gone")), Some(("opus", "work")));
        assert_eq!(found(Some("sonnet"), Some("claude")), None);
        assert_eq!(found(Some("opus"), Some("work")), None);
    }

    #[test]
    fn a_folder_under_the_home_reads_from_tilde() {
        assert_eq!(shortened("/home/me/code/api", "/home/me"), "~/code/api");
        assert_eq!(shortened("/home/me", "/home/me"), "~");
        assert_eq!(shortened("/home/meg/api", "/home/me"), "/home/meg/api");
        assert_eq!(shortened("/srv/api", ""), "/srv/api");
    }

    #[test]
    fn durations_read_as_short_as_they_can() {
        assert_eq!(duration(850), "850ms");
        assert_eq!(duration(12_000), "12s");
        assert_eq!(duration(185_000), "3m 05s");
        assert_eq!(duration(750_000), "12m 30s");
        assert_eq!(duration(3_720_000), "1h 02m");
    }

    #[test]
    fn a_removed_server_keeps_its_last_name_and_is_not_connected() {
        let view = ServerView {
            id: "key".into(),
            name: "build-box".into(),
            short_name: "build-box".into(),
            platform: "linux".into(),
            removed: true,
            state: State::Disconnected,
            error: None,
            path: None,
            rtt_ms: None,
            info: None,
            short_model_names: HashMap::new(),
            roomier_accounts: HashMap::new(),
        };
        let server = Server::from(view);
        assert!(server.removed);
        assert!(!server.connected());
        assert!(!server.known);
        assert_eq!(server.name, "build-box");
        assert_eq!(server.accounts(Agent::Claude).len(), 1);
    }
}
