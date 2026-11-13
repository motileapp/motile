//! The live state of every thread: whether a turn is running, what it has produced so far, and
//! who is watching. A turn is one run of the agent's CLI; its events are applied here, saved, and
//! sent to everyone with the thread open. Claude Code's process is talked to while it runs: it
//! asks before a tool call that needs approval and is told the thread's changed settings. It
//! outlives the turn while it monitors something: it then takes the next prompts itself, and
//! starts turns of its own. A message sent while a turn runs is queued until the turn ends, when
//! it starts the next one; sent now, the agent takes it at once, in the turn that runs. Codex's
//! process is asked for the thread and its turn, and answered what it asks, in the same way.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use motile_protocol::gateway_wire::GatewayAccount;
use motile_protocol::wire::{
    AccountSkills, ActionRecord, Activity, Actor, ActorKind, Agent, AgentAccount, AgentLimits, AgentNotice, Approval,
    BackupState, BranchInstructions, CONTINUE_PROMPT, ChangedFile, ContinueSettings, CopiedThread, DiffScope, FileKind,
    GitAction, GitHubState, GitStage, GitStatus, HandoffEnd, Interruption, Item, ItemKind, Media, MergeMethod, Message,
    ModelInfo, NewThread, NoticeEvent, OriginActivity, OriginUsage, PLAN_TOOL, Project, PullRequest, PullRequestAction,
    QUESTION_TOOL, Queued, Request, RunsOn, ServerInfo, ServerReason, ServerUpdate, SkillMention, Subagent, Thread,
    ThreadChange, ThreadParent, ToolCall, ToolStatus, TurnChanges, TurnOutcome, TurnStarter, TurnSummary, Worktree,
};
use motile_protocol::{error_text, now};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, OwnedMutexGuard, broadcast, mpsc, oneshot, watch};
use tokio::task::AbortHandle;

use crate::activity::{self, Sender, Tally};
use crate::agents::environment::Environment;
use crate::agents::{
    self, AgentEvent, Background, Parser, Settings, ToolImage, Turn, claude, executable_name, models, skills,
};
use crate::files::FileBytes;
use crate::generate::{self, Writer};
use crate::handoff::{self, Range};
use crate::linear::Linear;
use crate::mcp::{self, McpAccess};
use crate::media::MediaStore;
use crate::pricing::{self, Prices};
use crate::ricrac::Ricrac;
use crate::sign_in::SignIns;
use crate::store::{Continuation, Purpose, Session, Store, StoredProject, StoredThread, StoredWorktree, TitleSource};
use crate::ui_lookups::Lookups;
use crate::{agent_accounts, drafts, files, gateway, git, github, icons, limits, pacing, pull_requests, title, ui};

mod children;
mod threads;

pub use children::{LONGEST_WAIT, WAIT};
use motile_protocol::wire::{
    CheckStatus, EventKind, Mergeable, PullRequestDetail, PullRequestEdit, PullRequestSettings, PullRequestState,
};

const UPDATES_BUFFER: usize = 4096;
const ROOT_BYPASS_REFUSAL: &str = "cannot be used with root/sudo privileges";
/// Streamed text is written to disk at most this often, and when its block ends.
const FLUSH_EVERY: Duration = Duration::from_secs(1);
/// How long a branch's pull request is taken as known before GitHub is asked again.
pub const PULL_REQUEST_FRESH: Duration = Duration::from_secs(60);
/// How often a watched pull request is looked at.
pub const PULL_REQUEST_WATCH: Duration = Duration::from_secs(30);
/// How often the threads that wait for their usage limit are looked at.
pub const LIMITS_CHECK: Duration = Duration::from_secs(30);
/// How often the snoozed threads are looked at, for the ones whose time has come.
pub const SNOOZE_CHECK: Duration = Duration::from_secs(5);
/// How long what the agents last said of their models is taken as current when a client asks.
const MODELS_FRESH: Duration = Duration::from_secs(60);
/// How long the skills a project's agents listed are offered before they are asked again.
const SKILLS_FRESH: Duration = Duration::from_secs(30);
const DONE_ON_MERGE: &str = "done_on_merge";
const REMOVE_MERGED_WORKTREES: &str = "remove_merged_worktrees";
const PULL_DEFAULT_BRANCH: &str = "pull_default_branch";
const TEXT_MODEL: &str = "text_model";
const AGENT_ACCOUNTS: &str = "agent_accounts";
/// The models the agents' accounts last listed, served until they are asked again.
const MODELS: &str = "models";
const BRANCH_INSTRUCTIONS: &str = "branch_instructions";
const CONTINUE_AFTER_LIMITS: &str = "continue_after_limits";
const CONTINUE_AFTER_RESTARTS: &str = "continue_after_restarts";
/// Set while the server restarts after its agents were stopped for it, whatever the settings say.
const CONTINUE_AFTER_THIS_RESTART: &str = "continue_after_this_restart";
/// How long a stopped agent has to save its session before it is killed.
const AGENTS_STOP_WITHIN: Duration = Duration::from_secs(2);
/// How long Claude Code's process waits for the thread's next message once its turn has ended, so
/// that the turn starts with its MCP servers connected.
const RESTING_PROCESS_STAYS: Duration = Duration::from_secs(10 * 60);
/// How long a new process of Claude Code holds its first prompt for its MCP servers to connect.
const MCP_WAIT: Duration = Duration::from_secs(5);
const MCP_ASK_EVERY: Duration = Duration::from_millis(250);
const PRICES: &str = "prices";
/// How often the gateway is asked what accounts it has, which Gateway accounts are pinned to.
const GATEWAY_ACCOUNTS_EVERY: Duration = Duration::from_secs(60);
/// How often what the agents' accounts have used is read.
const LIMITS_READ_EVERY: Duration = Duration::from_secs(5 * 60);
/// How recent a read of what the agents' accounts have used answers whoever asks for it, so that
/// clients and agents asking at once read the accounts once.
const LIMITS_ANSWER_WITHIN: Duration = Duration::from_secs(30);
const PRICES_STALE: Duration = Duration::from_secs(24 * 3600);
const PRICES_RETRY: Duration = Duration::from_secs(3600);
const MAX_INSTRUCTIONS_CHARS: usize = 4000;
const MAX_SETUP_CHARS: usize = 20_000;
/// A worktree's setup script installs what the work needs, which can take minutes.
const SETUP_TIMEOUT: Duration = Duration::from_secs(900);
const SETUP_OUTPUT_CHARS: usize = 4000;
/// Starts the names of the branches made for worktrees until the writer has named them.
const BRANCH_PREFIX: &str = "motile";
/// How long an uploaded file waits for the message it is attached to.
const UNSENT_UPLOADS_STAY: Duration = Duration::from_secs(24 * 3600);
const SWEEP_UPLOADS_EVERY: Duration = Duration::from_secs(3600);
const NO_ROOM_FOR_HANDOFF: &str = "There isn't room in this model's context for what was said before. Pick a model \
     with a larger context, or start a new thread.";

/// What a client asked git to do in a project's folder.
pub struct GitRun {
    pub action: GitAction,
    /// The thread the work was done in.
    pub thread_id: Option<String>,
    pub message: Option<String>,
    pub paths: Vec<String>,
    pub new_branch: bool,
}

pub struct Hub {
    store: Arc<Store>,
    pub media: MediaStore,
    pub linear: Linear,
    pub ricrac: Ricrac,
    pub lookups: Lookups,
    environment: Environment,
    /// The server's own threads, each behind a lock of its own. A thread's lock is never waited for
    /// while another's is held, and the map is never held while a lock is waited for.
    threads: std::sync::Mutex<HashMap<String, Slot>>,
    /// Locked after a thread's when both are needed.
    projects: Mutex<Vec<StoredProject>>,
    /// What git last said about each folder threads work in: the projects' and the worktrees'.
    git: std::sync::Mutex<HashMap<String, GitRead>>,
    /// Where the files that clients upload are.
    attachments_folder: PathBuf,
    /// Where the worktrees are made, each in a folder named after its project.
    worktrees_folder: PathBuf,
    /// The folder of the "No project" project, which has a folder for each of its threads.
    no_project_folder: PathBuf,
    /// The worktrees of the threads that work in one of their own, by thread.
    worktrees: std::sync::Mutex<HashMap<String, ThreadWorktree>>,
    /// The agents' accounts, the default ones first.
    agent_accounts: std::sync::Mutex<Vec<AgentAccount>>,
    /// The models the accounts run, as their agents last listed them.
    models: std::sync::Mutex<Vec<ModelInfo>>,
    /// When the agents were last asked for their models, held while they are asked.
    models_asked: Mutex<Option<Instant>>,
    /// The skills each account's agent listed in each project, by project, and when.
    skills: std::sync::Mutex<HashMap<String, (Instant, Vec<AccountSkills>)>>,
    /// The model the user picked to write titles, commit messages and pull requests.
    text_model: std::sync::Mutex<Option<String>>,
    /// How the user wants branches named.
    branch_instructions: std::sync::Mutex<Option<String>>,
    pull_request_settings: std::sync::Mutex<PullRequestSettings>,
    continue_settings: std::sync::Mutex<ContinueSettings>,
    /// The threads whose agents the last restart cut off that go on once the server serves.
    to_continue: std::sync::Mutex<Vec<String>>,
    /// The server is about to stop: its agents are let go, and nothing new starts.
    closing: AtomicBool,
    update: std::sync::Mutex<Option<ServerUpdate>>,
    backup: std::sync::Mutex<Option<BackupState>>,
    /// What the models cost at the API's prices, as last fetched.
    prices: std::sync::Mutex<Prices>,
    /// What the agents' accounts had used of their plans, and when that was read.
    limits: Mutex<Option<(Instant, Vec<AgentLimits>)>>,
    /// What was last seen of each watched thread's pull request, by thread.
    watched: std::sync::Mutex<HashMap<String, Seen>>,
    /// How many connections each client device has open.
    connected: std::sync::Mutex<HashMap<String, usize>>,
    /// Held while a folder is kept as it is, so a thread's snapshots follow one another.
    snapshotting: Mutex<()>,
    list_updates: broadcast::Sender<Message>,
    /// The tokens of the threads whose agents may read threads, and the port they read them on
    /// once that is served.
    mcp_tokens: Arc<mcp::Tokens>,
    mcp_port: std::sync::OnceLock<u16>,
    /// The way the Gateway accounts' agents reach the account's gateway, once that is served.
    gateway: std::sync::OnceLock<GatewayWay>,
    sign_ins: Arc<SignIns>,
    /// The threads agents started whose parents have news to hear, and where that is read.
    children_changed: mpsc::UnboundedSender<String>,
    children_changes: std::sync::Mutex<Option<mpsc::UnboundedReceiver<String>>>,
    /// The threads agents started whose parents were told how they are doing.
    children_told: broadcast::Sender<String>,
    /// How many of an agent's calls wait for each thread it started, by thread.
    waiting_on: std::sync::Mutex<HashMap<String, usize>>,
    /// The threads agents started, by the thread that started each and the id it gave the request.
    started: std::sync::Mutex<HashMap<(String, String), String>>,
}

struct GatewayWay {
    tunnel: Arc<gateway::Tunnel>,
    port: u16,
    /// What the server's own calls through the gateway are made with.
    token: String,
    /// Where the Gateway accounts keep their folders.
    folder: PathBuf,
    /// The gateway's accounts as it last listed them, which Gateway accounts are pinned to.
    pool: std::sync::Mutex<Vec<GatewayAccount>>,
    /// The slugs of the gateway's MCP servers that are on, as it last listed them, which every agent is
    /// given.
    mcp_servers: std::sync::Mutex<Vec<String>>,
}

impl GatewayWay {
    fn pool(&self) -> Vec<GatewayAccount> {
        self.pool.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }
}

/// A thread with the lock its work takes.
#[derive(Clone)]
struct Slot {
    live: Arc<Mutex<Live>>,
    /// The thread that started it, which never changes, so that a thread's family is known without
    /// its lock.
    parent: Option<ThreadParent>,
    /// The thread whose agent started it, which never changes either.
    started_by: Option<String>,
}

impl Slot {
    fn new(live: Live) -> Self {
        let parent = live.stored.thread.parent.clone();
        let started_by = live.stored.started_by.clone();
        Self { live: Arc::new(Mutex::new(live)), parent, started_by }
    }
}

/// What the agent of a thread that watches its pull request was last told about.
#[derive(Clone, Default, PartialEq)]
struct Seen {
    checks_running: bool,
    /// The checks that failed, by name.
    failing: HashSet<String>,
    conflicting: bool,
    /// The comments and reviews that were there, by id.
    said: HashSet<String>,
}

struct ThreadWorktree {
    project_id: String,
    path: String,
    branch: String,
}

struct GitRead {
    status: GitStatus,
    /// The commit that was checked out when the pull request was looked up, and when that was.
    head: String,
    pull_request_read: Instant,
}

struct Live {
    stored: StoredThread,
    media: MediaStore,
    /// Items the running turn may still change, by id. Everything else is only on disk.
    open: HashMap<String, Item>,
    /// Open items whose latest text isn't on disk yet.
    unsaved: HashSet<String>,
    last_flush: Instant,
    /// Streamed text that isn't a finished block yet, by item.
    held: HashMap<String, String>,
    last_delivery: Option<Instant>,
    activity: Activity,
    updates: broadcast::Sender<Message>,
    /// The tool call that started the agent whose items are being added.
    parent: Option<String>,
    run: Option<Run>,
    /// The turn starts when its folder is ready.
    preparing: Option<Preparing>,
    /// The item that ended a turn whose changes haven't been read yet.
    ended: Option<String>,
    /// Who stopped the turn, until it has ended.
    stopped_by: Actor,
    /// Who moved the thread to another account, until its next turn notes the handoff.
    moved_by: Actor,
    /// Messages sent while the agent was working, until they are given to it.
    queued: Vec<Queued>,
    /// Messages given to the agent that it hasn't taken yet. They are in the transcript already.
    given: Vec<Queued>,
    title_needs_refinement: bool,
    /// What the agent says the turn that runs has cost so far.
    cost_usd: Option<f64>,
    /// The position of the user's last message, which starts the next turn.
    last_message_seq: Option<u64>,
    /// The turn the agent's process was started for.
    turn: Option<StartedTurn>,
    /// The agent didn't find the session it was to resume: the turn starts again with this prompt.
    retry: Option<String>,
    /// What the thread's agents read threads with.
    mcp_token: Option<String>,
    /// The model the agent's session runs, as it said.
    session_model: Option<String>,
    /// The handoff that waits to be told the model the new session runs.
    handoff_item: Option<String>,
    /// The skills the thread's messages started, which Claude Code is given as its commands.
    skills: Vec<SkillMention>,
    /// The question the agent that started the thread was told it asks.
    told_ask: Option<String>,
    /// The agent that started the thread was told it waits for its usage limit to reset.
    told_limit: bool,
    /// How many tool calls of its own the transcript has, once they were counted.
    tools: Option<u64>,
    /// The thread was deleted while this waited for its lock.
    deleted: bool,
    /// The approvals the server asks for itself, of calls to the gateway's MCP servers, by id: told
    /// the user's answer, or dropped as a refusal.
    server_asks: HashMap<String, oneshot::Sender<bool>>,
    /// What the turn that runs has done, kept once it ends.
    tally: Option<Tally>,
}

struct StartedTurn {
    prompt_id: String,
    prompt: String,
    message_seq: u64,
    retried: bool,
    accepted: bool,
}

/// What happens before a turn's agent starts: the thread's worktree is made when it isn't
/// there, and the folder is kept as the turn finds it.
struct Preparing {
    task: AbortHandle,
    makes_worktree: bool,
}

struct Run {
    process_id: u32,
    /// When the agent last went to work.
    started: Instant,
    interrupted: Arc<AtomicBool>,
    /// The turn has ended; a process that is still there is idle.
    received_result: bool,
    /// Since when the process has waited for the thread's next message, with nothing it started
    /// still running.
    resting: Option<Instant>,
    /// Writes lines to the process's stdin, which closes when this is dropped.
    input: Option<mpsc::UnboundedSender<String>>,
    background: Background,
    /// The agent's own name for the turn that runs.
    turn_id: Option<String>,
}

pub struct ListSubscription {
    pub first: Message,
    pub updates: broadcast::Receiver<Message>,
}

pub struct ThreadSubscription {
    pub reset: bool,
    pub activity: Activity,
    pub items: Vec<Item>,
    pub rev: u64,
    pub updates: broadcast::Receiver<Message>,
}

impl Hub {
    /// `media_folder` is where the images and videos that threads show are kept,
    /// `attachments_folder` where the files that clients upload are, `worktrees_folder` where
    /// the worktrees of threads are made, and `no_project_folder` where the threads started
    /// without a project work.
    pub fn new(
        store: Store,
        media_folder: PathBuf,
        attachments_folder: PathBuf,
        worktrees_folder: PathBuf,
        no_project_folder: PathBuf,
        environment: Environment,
    ) -> anyhow::Result<Arc<Self>> {
        let environment = stop_git_above(environment, &no_project_folder);
        let home = environment.variables.get("HOME").map(String::as_str).unwrap_or_default();
        let media = MediaStore::new(media_folder, home);
        let threads = store.load_threads()?;
        let worktrees = threads.iter().filter_map(thread_worktree).collect();
        let mut queued = store.load_queued()?;
        let kept_accounts = store.setting(AGENT_ACCOUNTS).and_then(|kept| serde_json::from_str(&kept).ok());
        let accounts = agent_accounts::with_every_agent(kept_accounts.unwrap_or_default());
        let continue_settings = ContinueSettings {
            after_limits: store.setting(CONTINUE_AFTER_LIMITS).is_none_or(|value| value == "true"),
            after_restarts: store.setting(CONTINUE_AFTER_RESTARTS).is_none_or(|value| value == "true"),
        };
        let continues = continue_settings.after_restarts || store.setting(CONTINUE_AFTER_THIS_RESTART).is_some();
        store.set_setting(CONTINUE_AFTER_THIS_RESTART, None)?;
        let mut to_continue = Vec::new();
        let mut live = |mut stored: StoredThread| -> anyhow::Result<(String, Live)> {
            let thread = &stored.thread;
            let continuation = continuation_of(thread, &accounts, &environment);
            if let Some(kept) = stored.session_id.take() {
                store.adopt_session(&thread.id, &continuation, &kept, stored.next_seq.saturating_sub(1))?;
            }
            stored.session_id = store.session(&thread.id, &continuation)?.and_then(|session| session.session_id);
            let mut live = Live::new(stored, media.clone());
            live.queued = queued.remove(&live.stored.thread.id).unwrap_or_default();
            let cut_off = live.recover(&store)?;
            // No turn survives a restart, so what waited for one goes when the user sends it, unless
            // the thread goes on by itself. What waits for the usage limit still follows the thread.
            let goes_on = cut_off && continues && live.stored.session_id.is_some();
            let waits = goes_on || live.limited();
            for queued in &mut live.queued {
                queued.held = !waits;
            }
            if goes_on {
                to_continue.push(live.stored.thread.id.clone());
            }
            Ok((live.stored.thread.id.clone(), live))
        };
        let threads = threads
            .into_iter()
            .map(|stored| live(stored).map(|(id, live)| (id, Slot::new(live))))
            .collect::<anyhow::Result<_>>()?;
        let mut projects = store.load_projects()?;
        ensure_no_project(&store, &mut projects, &no_project_folder)?;
        let projects = Mutex::new(projects);
        let (list_updates, _) = broadcast::channel(UPDATES_BUFFER);
        let git = std::sync::Mutex::default();
        let (children_changed, children_changes) = mpsc::unbounded_channel();
        let (children_told, _) = broadcast::channel(UPDATES_BUFFER);
        Ok(Arc::new(Self {
            children_changed,
            children_told,
            children_changes: std::sync::Mutex::new(Some(children_changes)),
            waiting_on: std::sync::Mutex::default(),
            started: std::sync::Mutex::default(),
            agent_accounts: std::sync::Mutex::new(accounts),
            models: std::sync::Mutex::new(
                store.setting(MODELS).and_then(|models| serde_json::from_str(&models).ok()).unwrap_or_default(),
            ),
            models_asked: Mutex::default(),
            skills: std::sync::Mutex::default(),
            text_model: std::sync::Mutex::new(store.setting(TEXT_MODEL)),
            branch_instructions: std::sync::Mutex::new(store.setting(BRANCH_INSTRUCTIONS)),
            pull_request_settings: std::sync::Mutex::new(PullRequestSettings {
                done_on_merge: store.setting(DONE_ON_MERGE).is_some_and(|value| value == "true"),
                remove_merged_worktrees: store.setting(REMOVE_MERGED_WORKTREES).is_some_and(|value| value == "true"),
                pull_default_branch: store.setting(PULL_DEFAULT_BRANCH).is_none_or(|value| value == "true"),
            }),
            continue_settings: std::sync::Mutex::new(continue_settings),
            to_continue: std::sync::Mutex::new(to_continue),
            closing: AtomicBool::new(false),
            update: std::sync::Mutex::default(),
            backup: std::sync::Mutex::default(),
            prices: std::sync::Mutex::new(
                store.setting(PRICES).and_then(|prices| serde_json::from_str(&prices).ok()).unwrap_or_default(),
            ),
            limits: Mutex::default(),
            watched: std::sync::Mutex::default(),
            connected: std::sync::Mutex::default(),
            worktrees: std::sync::Mutex::new(worktrees),
            snapshotting: Mutex::default(),
            threads: std::sync::Mutex::new(threads),
            store: Arc::new(store),
            media,
            linear: Linear::from_environment(),
            ricrac: Ricrac::from_environment(),
            lookups: Lookups::default(),
            environment,
            projects,
            git,
            attachments_folder,
            worktrees_folder,
            no_project_folder,
            list_updates,
            mcp_tokens: Arc::default(),
            mcp_port: std::sync::OnceLock::new(),
            gateway: std::sync::OnceLock::new(),
            sign_ins: Arc::default(),
        }))
    }

    /// Serves the tools the agents read threads and start agents with. Agents started before it
    /// serves go without.
    pub async fn serve_mcp(self: &Arc<Self>) -> anyhow::Result<()> {
        let port = mcp::serve(Arc::downgrade(self), self.mcp_tokens.clone()).await?;
        let _ = self.mcp_port.set(port);
        Ok(())
    }

    /// Where the thread's agent reads threads and reaches the gateway's MCP servers, with the token of
    /// the thread.
    fn mcp_access(&self, live: &mut Live) -> Option<McpAccess> {
        let port = self.mcp_port.get()?;
        let mcp_servers = match self.gateway.get().filter(|way| way.tunnel.has_gateway()) {
            Some(way) => {
                let slugs = way.mcp_servers.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
                slugs
                    .into_iter()
                    .map(|slug| {
                        let url = gateway::mcp_server_url(way.port, &slug);
                        (slug, url)
                    })
                    .collect()
            }
            None => vec![],
        };
        Some(McpAccess { url: format!("http://127.0.0.1:{port}/mcp"), token: self.thread_token(live), mcp_servers })
    }

    /// What the thread's agent reads threads and reaches the gateway with.
    fn thread_token(&self, live: &mut Live) -> String {
        live.mcp_token.get_or_insert_with(|| self.mcp_tokens.mint(&live.stored.thread.id)).clone()
    }

    /// Serves the way the Gateway accounts' agents reach the account's gateway over `endpoint`, and
    /// offers those accounts, which keep their folders in `folder`, while `key` says the account
    /// has a gateway.
    pub async fn serve_gateway(
        self: &Arc<Self>,
        endpoint: iroh::Endpoint,
        mut key: watch::Receiver<Option<String>>,
        folder: PathBuf,
    ) -> anyhow::Result<()> {
        let tunnel = gateway::Tunnel::new(endpoint, key.clone());
        let token = hex::encode(rand::random::<[u8; 32]>());
        let port = gateway::serve(tunnel.clone(), self.mcp_tokens.clone(), token.clone(), Arc::downgrade(self)).await?;
        std::fs::create_dir_all(folder.join(agent_accounts::usual_id(Agent::Claude)))?;
        gateway::write_codex_config(&folder.join(agent_accounts::usual_id(Agent::Codex)), port)?;
        let way = GatewayWay {
            tunnel,
            port,
            token,
            folder,
            pool: std::sync::Mutex::default(),
            mcp_servers: std::sync::Mutex::default(),
        };
        if self.gateway.set(way).is_err() {
            bail!("The way to the gateway is served already.");
        }
        let has_gateway = key.borrow_and_update().is_some();
        self.offer_gateway(has_gateway).await?;
        let hub = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut every = tokio::time::interval(GATEWAY_ACCOUNTS_EVERY);
            loop {
                tokio::select! {
                    changed = key.changed() => if changed.is_err() { return },
                    _ = every.tick() => {}
                }
                let has_gateway = key.borrow_and_update().is_some();
                let Some(hub) = hub.upgrade() else { return };
                if let Err(error) = hub.offer_gateway(has_gateway).await {
                    tracing::warn!("couldn't change the Gateway accounts: {error:#}");
                }
            }
        });
        Ok(())
    }

    /// Offers the Gateway accounts as the gateway's accounts are now, before an agent picks one.
    pub async fn follow_gateway(self: &Arc<Self>) {
        let Some(way) = self.gateway.get() else { return };
        if let Err(error) = self.offer_gateway(way.tunnel.has_gateway()).await {
            tracing::warn!("couldn't change the Gateway accounts: {error:#}");
        }
    }

    /// Offers the Gateway accounts while the account has a gateway, one pinned to each of its accounts
    /// too, and takes them away when it no longer has one, or no longer has the account, as a removed
    /// account is: their threads go on with their agents' defaults. The gateway's accounts stay as
    /// they were while it can't be asked. What the accounts have used is read again when either
    /// changed, as it is when the server starts: the accounts it keeps are offered already, but the
    /// gateway's weren't listed when it first read them. Otherwise the pinned accounts take what the
    /// gateway's accounts have used from its list, which costs no read.
    async fn offer_gateway(self: &Arc<Self>, has_gateway: bool) -> anyhow::Result<()> {
        let Some(way) = self.gateway.get() else { return Ok(()) };
        let mut listed_others = false;
        if has_gateway {
            match way.tunnel.accounts().await {
                Ok(pool) => {
                    let mut kept = way.pool.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    let ids = |pool: &[GatewayAccount]| pool.iter().map(|pooled| pooled.id.clone()).collect::<Vec<_>>();
                    listed_others = ids(&kept) != ids(&pool);
                    *kept = pool;
                }
                Err(error) => tracing::debug!("couldn't ask the gateway for its accounts: {error:#}"),
            }
        }
        let mcp_servers = match has_gateway {
            true => way.tunnel.mcp_servers().await.unwrap_or_else(|error| {
                tracing::debug!("couldn't ask the gateway for its MCP servers: {error:#}");
                vec![]
            }),
            false => vec![],
        };
        *way.mcp_servers.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = mcp_servers;
        let pool = way.pool();
        let mut threads = self.every_thread().await;
        let accounts = self.agent_accounts();
        let offered =
            agent_accounts::with_gateway(accounts.clone(), has_gateway.then_some((way.folder.as_path(), &pool)));
        if offered == accounts {
            drop(threads);
            match listed_others {
                true => self.refresh_limits(),
                false => self.keep_pinned_limits(&accounts, &pool).await,
            }
            return Ok(());
        }
        let gone: Vec<&AgentAccount> =
            accounts.iter().filter(|account| !offered.iter().any(|kept| kept.id == account.id)).collect();
        for account in &gone {
            let_go_of_account(&mut threads, &account.id);
        }
        self.keep_agent_accounts(offered.clone())?;
        for account in gone {
            let default = agent_accounts::default_of(&offered, account.agent);
            self.reload_sessions(&mut threads, &account.id, Some(&default))?;
        }
        drop(threads);
        self.refresh_limits();
        Ok(())
    }

    pub fn environment(&self) -> &Environment {
        &self.environment
    }

    fn lock_threads(&self) -> std::sync::MutexGuard<'_, HashMap<String, Slot>> {
        self.threads.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn slot(&self, thread_id: &str) -> Option<Slot> {
        self.lock_threads().get(thread_id).cloned()
    }

    fn add_thread(&self, live: Live) {
        self.lock_threads().insert(live.stored.thread.id.clone(), Slot::new(live));
    }

    /// The thread, once nothing else works on it, while it is there.
    async fn live(&self, thread_id: &str) -> Option<OwnedMutexGuard<Live>> {
        let live = self.slot(thread_id)?.live.lock_owned().await;
        (!live.deleted).then_some(live)
    }

    async fn live_or(&self, thread_id: &str, gone: &'static str) -> anyhow::Result<OwnedMutexGuard<Live>> {
        self.live(thread_id).await.context(gone)
    }

    /// What `pick` takes from each thread, one thread at a time.
    async fn each_thread<T>(&self, mut pick: impl FnMut(&mut Live) -> Option<T>) -> Vec<T> {
        let slots: Vec<Slot> = self.lock_threads().values().cloned().collect();
        let mut picked = Vec::new();
        for slot in slots {
            let mut live = slot.live.lock().await;
            if !live.deleted {
                picked.extend(pick(&mut live));
            }
        }
        picked
    }

    /// Every thread at once, each locked in the order of its id, for what has to see them all as
    /// they are.
    async fn every_thread(&self) -> Vec<OwnedMutexGuard<Live>> {
        let mut slots: Vec<(String, Slot)> =
            self.lock_threads().iter().map(|(id, slot)| (id.clone(), slot.clone())).collect();
        slots.sort_by(|a, b| a.0.cmp(&b.0));
        let mut locked = Vec::new();
        for (_, slot) in slots {
            let live = slot.live.lock_owned().await;
            if !live.deleted {
                locked.push(live);
            }
        }
        locked
    }

    /// One of the server's own threads, as it is now.
    pub async fn thread(&self, thread_id: &str) -> Option<Thread> {
        Some(self.live(thread_id).await?.stored.thread.clone())
    }

    /// The agent of a thread, as it acts in another one.
    pub(crate) async fn agent_actor(&self, thread_id: &str) -> Actor {
        let Some(thread) = self.thread(thread_id).await else { return Actor::Unknown };
        let model = thread.model.unwrap_or_default();
        Actor::Agent { thread_id: thread.id, agent: thread.agent, account: thread.agent_account, model }
    }

    /// The projects' names, by id.
    pub async fn project_names(&self) -> HashMap<String, String> {
        let projects = self.projects_of(&self.projects.lock().await);
        projects.into_iter().map(|project| (project.id, project.name)).collect()
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn server_info(&self) -> ServerInfo {
        ServerInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol: motile_protocol::PROTOCOL_VERSION,
            hostname: Environment::hostname(),
            home: self.environment.variables.get("HOME").cloned().unwrap_or_default(),
            agents: self.environment.agents(),
            models: self.models(),
            agent_accounts: self.agent_accounts().into_iter().map(agent_accounts::redacted).collect(),
            text_model: self.text_model(),
            branch_instructions: self.branch_instructions(),
            pull_request_settings: self.pull_request_settings(),
            continue_settings: self.continue_settings(),
            update: *self.update.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
            backup: self.backup.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone(),
        }
    }

    /// Tells every client where the server's update is.
    pub fn set_update(&self, update: Option<ServerUpdate>) {
        *self.update.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = update;
        self.announce_server();
    }

    /// Tells every client how the server's backup stands.
    pub fn set_backup(&self, backup: Option<BackupState>) {
        *self.backup.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = backup;
        self.announce_server();
    }

    fn announce_server(&self) {
        let _ = self.list_updates.send(Message::Server { server: self.server_info() });
    }

    fn lock_agent_accounts(&self) -> std::sync::MutexGuard<'_, Vec<AgentAccount>> {
        self.agent_accounts.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn agent_accounts(&self) -> Vec<AgentAccount> {
        self.lock_agent_accounts().clone()
    }

    fn default_account(&self, agent: Agent) -> AgentAccount {
        agent_accounts::default_of(&self.agent_accounts(), agent)
    }

    /// The account the thread works with, or its agent's default one when that has gone.
    fn account_of(&self, thread: &Thread) -> AgentAccount {
        account_in(&self.agent_accounts(), thread)
    }

    fn continuation(&self, thread: &Thread) -> Continuation {
        continuation_of(thread, &self.agent_accounts(), &self.environment)
    }

    /// The session of the thread's continuation that is there to resume.
    fn saved_session(&self, thread: &Thread) -> rusqlite::Result<Option<String>> {
        let session = self.store.session(&thread.id, &self.continuation(thread))?;
        Ok(session.and_then(|session| session.session_id))
    }

    /// Changes what is known of the session of the thread's continuation.
    fn change_session(&self, thread: &Thread, change: impl FnOnce(&mut Session)) -> rusqlite::Result<()> {
        let continuation = self.continuation(thread);
        let mut session = self.store.session(&thread.id, &continuation)?.unwrap_or_default();
        change(&mut session);
        self.store.save_session(&thread.id, &continuation, &session)
    }

    fn account_environment(&self, account: &AgentAccount) -> Environment {
        let environment = agent_accounts::environment(&self.environment, account);
        match self.gateway.get().filter(|_| account.gateway) {
            Some(way) => environment.with(gateway::variables(account.agent, way.port, &way.token)),
            None => environment,
        }
    }

    fn shares_sessions(&self, one: &AgentAccount, other: &AgentAccount) -> bool {
        one.agent == other.agent
            && agent_accounts::sessions_folder(one, &self.environment)
                == agent_accounts::sessions_folder(other, &self.environment)
    }

    fn keep_agent_accounts(self: &Arc<Self>, accounts: Vec<AgentAccount>) -> anyhow::Result<()> {
        self.store.set_setting(AGENT_ACCOUNTS, Some(&serde_json::to_string(&accounts)?))?;
        *self.lock_agent_accounts() = accounts;
        self.announce_server();
        let hub = self.clone();
        tokio::spawn(async move { hub.refresh_models().await });
        Ok(())
    }

    pub fn models(&self) -> Vec<ModelInfo> {
        self.models.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    /// Asks each account's agent what models it runs, after any asking under way, and tells the
    /// clients when that changed.
    pub async fn refresh_models(&self) {
        let mut asked = self.models_asked.lock().await;
        self.ask_models().await;
        *asked = Some(Instant::now());
    }

    /// Asks the agents what models they run in the background, unless they are being asked or
    /// were within the last minute, so that a client that connects or starts a thread soon
    /// offers what they would.
    pub fn refresh_models_soon(self: &Arc<Self>) {
        let hub = self.clone();
        tokio::spawn(async move {
            let Ok(mut asked) = hub.models_asked.try_lock() else { return };
            if asked.is_some_and(|at| at.elapsed() < MODELS_FRESH) {
                return;
            }
            hub.ask_models().await;
            *asked = Some(Instant::now());
        });
    }

    /// Asks each account's agent what models it runs, the accounts at once. What an account last
    /// listed stays while its agent doesn't answer, and a default effort it can't say now stays
    /// as it was.
    async fn ask_models(&self) {
        let known = self.models();
        let (pinned, asked): (Vec<_>, Vec<_>) =
            self.agent_accounts().into_iter().partition(|account| account.pin.is_some());
        let asks: Vec<_> = asked
            .into_iter()
            .map(|account| {
                let environment = self.account_environment(&account);
                tokio::spawn(async move {
                    let models = match (account.agent, environment.executable(account.agent)) {
                        (_, None) => Some(Vec::new()),
                        (Agent::Claude, Some(_)) => models::claude_models(&environment).await,
                        (Agent::Codex, Some(_)) => models::codex_models(&environment).await,
                    };
                    (account, models)
                })
            })
            .collect();
        let mut listed = Vec::new();
        for ask in asks {
            let Ok((account, models)) = ask.await else { continue };
            let Some(models) = models else {
                tracing::warn!("{:?} didn't list the models of its {} account", account.agent, account.name);
                listed.extend(known.iter().filter(|model| model.account == account.id).cloned());
                continue;
            };
            let models = models.into_iter().map(|model| ModelInfo { account: account.id.clone(), ..model });
            listed.extend(models.map(|model| with_known_default(model, &known)));
        }
        let pinned_models: Vec<ModelInfo> = pinned
            .iter()
            .flat_map(|account| {
                let placing = agent_accounts::gateway_id(account.agent);
                let models = listed.iter().filter(move |model| model.account == placing);
                models.map(|model| ModelInfo { account: account.id.clone(), ..model.clone() })
            })
            .collect();
        listed.extend(pinned_models);
        if listed == known {
            return;
        }
        match serde_json::to_string(&listed) {
            Ok(kept) => {
                if let Err(error) = self.store.set_setting(MODELS, Some(&kept)) {
                    tracing::warn!("couldn't keep the models the agents listed: {error:#}");
                }
            }
            Err(error) => tracing::warn!("couldn't keep the models the agents listed: {error:#}"),
        }
        *self.models.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = listed;
        self.announce_server();
    }

    /// Adds an account of an agent or changes one. Threads whose sessions it no longer has start
    /// new ones, told what was said.
    pub async fn save_agent_account(self: &Arc<Self>, account: AgentAccount) -> anyhow::Result<String> {
        self.keep_agent_account(account, None).await
    }

    /// Saves the account, signed in as `signed_in_as` when that is known already.
    async fn keep_agent_account(
        self: &Arc<Self>,
        account: AgentAccount,
        signed_in_as: Option<(Option<String>, Option<String>)>,
    ) -> anyhow::Result<String> {
        let mut threads = self.every_thread().await;
        let accounts = self.agent_accounts();
        let mut account = agent_accounts::checked(account, &accounts)?;
        agent_accounts::prepare(&account, &self.environment)?;
        let before = accounts.iter().find(|kept| kept.id == account.id);
        // Who is signed in is known again once the agent has said, unless it signs in as before.
        (account.email, account.plan) = match (signed_in_as, before) {
            (Some(signed_in_as), _) => signed_in_as,
            (None, Some(before)) if before.folder == account.folder && before.variables == account.variables => {
                (before.email.clone(), before.plan.clone())
            }
            _ => (None, None),
        };
        let moves_sessions = before.is_some_and(|before| !self.shares_sessions(before, &account));
        if moves_sessions {
            ensure_idle(&threads, &account.id)?;
        }
        let_go_of_account(&mut threads, &account.id);
        let id = account.id.clone();
        let mut accounts = accounts;
        match accounts.iter_mut().find(|kept| kept.id == account.id) {
            Some(kept) => *kept = account,
            None => accounts.push(account),
        }
        self.keep_agent_accounts(accounts)?;
        if moves_sessions {
            self.reload_sessions(&mut threads, &id, None)?;
        }
        drop(threads);
        self.refresh_limits();
        Ok(id)
    }

    /// Starts signing the account in, as the form has it, and answers with the page to sign in on.
    pub async fn start_sign_in(&self, account: AgentAccount) -> anyhow::Result<Message> {
        let account = agent_accounts::signing_in(account, &self.agent_accounts())?;
        let environment = agent_accounts::environment(&self.environment, &account);
        let (id, url) = self.sign_ins.start(environment, account.folder).await?;
        Ok(Message::SignInLink { id, url })
    }

    /// Finishes a sign-in with the code its page showed, saves the account, and answers with who
    /// signed in.
    pub async fn finish_sign_in(
        self: &Arc<Self>,
        id: &str,
        code: &str,
        account: AgentAccount,
    ) -> anyhow::Result<Message> {
        let accounts = self.agent_accounts();
        let folder = agent_accounts::checked(account.clone(), &accounts)?.folder;
        let environment = self.sign_ins.finish(id, code, &folder).await?;
        let signed_in_as = limits::claude_login(&environment)
            .await
            .inspect_err(|error| tracing::warn!("couldn't read who signed in: {error:#}"))
            .ok();
        let account = self.keep_agent_account(account, signed_in_as.clone()).await?;
        let (email, plan) = signed_in_as.unwrap_or_default();
        Ok(Message::SignedIn { account, email, plan })
    }

    pub async fn cancel_sign_in(&self, id: &str) {
        self.sign_ins.cancel(id).await;
    }

    /// Removes an account. Its threads go on with the agent's default account, which is the next
    /// one when it was the default.
    pub async fn remove_agent_account(self: &Arc<Self>, id: &str) -> anyhow::Result<()> {
        let mut threads = self.every_thread().await;
        let mut accounts = self.agent_accounts();
        let account =
            accounts.iter().find(|account| account.id == id).context("That account is no longer on your server.")?;
        if account.gateway {
            bail!("The Gateway account goes when your gateway does.");
        }
        let agent = account.agent;
        if !accounts.iter().any(|other| other.agent == agent && other.id != id) {
            bail!(
                "This is your server's only {} account. Add another before removing it.",
                agent_accounts::agent_name(agent)
            );
        }
        ensure_idle(&threads, id)?;
        let_go_of_account(&mut threads, id);
        accounts.retain(|account| account.id != id);
        let default = agent_accounts::default_of(&accounts, agent);
        self.keep_agent_accounts(accounts)?;
        self.reload_sessions(&mut threads, id, Some(&default))
    }

    /// Makes the account its agent's default, which new threads start with.
    pub fn make_agent_account_default(self: &Arc<Self>, id: &str) -> anyhow::Result<()> {
        let accounts = agent_accounts::made_default(self.agent_accounts(), id)?;
        self.keep_agent_accounts(accounts)
    }

    /// Has the account's threads, or with `moving` the threads it had, which go to that account,
    /// go on with the sessions of the folder the account keeps them in now.
    fn reload_sessions(
        &self,
        threads: &mut [OwnedMutexGuard<Live>],
        id: &str,
        moving: Option<&AgentAccount>,
    ) -> anyhow::Result<()> {
        for live in threads.iter_mut().filter(|live| live.stored.thread.agent_account == id) {
            if let Some(to) = moving {
                live.stored.thread.agent_account = to.id.clone();
            }
            live.stored.session_id = self.saved_session(&live.stored.thread)?;
            self.store.save_thread(&live.stored)?;
            self.announce(&live.stored.thread);
        }
        Ok(())
    }

    /// Reads what the accounts have used in the background, which also says who they are.
    pub fn refresh_limits(self: &Arc<Self>) {
        let hub = self.clone();
        tokio::spawn(async move { hub.read_limits(Duration::ZERO).await });
    }

    /// Reads what the accounts have used now and every few minutes, so that a client never waits
    /// on the agents for it.
    pub fn keep_limits_current(self: &Arc<Self>) {
        let hub = self.clone();
        tokio::spawn(async move {
            loop {
                hub.read_limits(Duration::ZERO).await;
                tokio::time::sleep(LIMITS_READ_EVERY).await;
            }
        });
    }

    /// Notes who is signed in to each account, as its read said, and tells the clients when that
    /// changed. A read that failed before it said who is signed in changes nothing.
    fn note_signed_in(&self, read: &[AgentLimits]) {
        let kept = {
            let mut accounts = self.lock_agent_accounts();
            let mut changed = false;
            for account in accounts.iter_mut().filter(|account| !account.gateway) {
                let found =
                    read.iter().find(|limits| limits.agent == account.agent && limits.account_name == account.name);
                let Some(limits) = found else { continue };
                if limits.error.is_some() && limits.account.is_none() {
                    continue;
                }
                let (email, plan) = (limits.account.clone(), limits.plan.clone());
                if (&account.email, &account.plan) != (&email, &plan) {
                    (account.email, account.plan) = (email, plan);
                    changed = true;
                }
            }
            changed.then(|| serde_json::to_string(&*accounts))
        };
        let Some(Ok(kept)) = kept else { return };
        if let Err(error) = self.store.set_setting(AGENT_ACCOUNTS, Some(&kept)) {
            tracing::warn!("couldn't keep who is signed in to the agents' accounts: {error:#}");
        }
        self.announce_server();
    }

    fn continue_settings(&self) -> ContinueSettings {
        *self.continue_settings.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn set_continue_settings(
        &self,
        after_limits: Option<bool>,
        after_restarts: Option<bool>,
    ) -> anyhow::Result<()> {
        let mut settings = self.continue_settings();
        if let Some(on) = after_limits {
            self.store.set_setting(CONTINUE_AFTER_LIMITS, Some(if on { "true" } else { "false" }))?;
            settings.after_limits = on;
        }
        if let Some(on) = after_restarts {
            self.store.set_setting(CONTINUE_AFTER_RESTARTS, Some(if on { "true" } else { "false" }))?;
            settings.after_restarts = on;
        }
        *self.continue_settings.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = settings;
        Ok(())
    }

    /// Has the agents the last restart cut off go on, where the settings or the update say so.
    pub fn continue_interrupted(self: &Arc<Self>) {
        let threads = std::mem::take(&mut *self.to_continue.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
        let hub = self.clone();
        tokio::spawn(async move {
            for thread_id in threads {
                if let Err(error) = hub.continue_thread(&thread_id, Err(ServerReason::Restart)).await {
                    tracing::warn!(thread_id, "couldn't continue a thread the restart cut off: {error:#}");
                }
            }
        });
    }

    /// Has the thread's agent go on with what it was doing: for the client of `by`, or by itself
    /// for its reason.
    pub async fn continue_thread(
        self: &Arc<Self>,
        thread_id: &str,
        by: Result<Sender, ServerReason>,
    ) -> anyhow::Result<()> {
        let message = match by {
            Ok(sender) => NewMessage::from_client(CONTINUE_PROMPT, sender),
            Err(reason) => NewMessage::text(CONTINUE_PROMPT, Actor::Server { reason }),
        };
        self.send(Some(thread_id.to_string()), None, message).await?;
        Ok(())
    }

    /// Continues the threads whose usage limit has reset, looking every `every`.
    pub fn keep_limits_continued(self: &Arc<Self>, every: Duration) {
        let hub = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(every).await;
                for thread_id in hub.limits_reset().await {
                    if let Err(error) = hub.continue_thread(&thread_id, Err(ServerReason::Limit)).await {
                        tracing::warn!(thread_id, "couldn't continue a thread after its usage limit: {error:#}");
                    }
                }
            }
        });
    }

    /// The threads that wait to continue once their usage limit resets, and whose limit has.
    async fn limits_reset(&self) -> Vec<String> {
        let reset = |live: &Live| match live.stored.thread.interruption {
            Some(Interruption::Limit { resets_at: Some(at), continues: true }) => at <= now(),
            _ => false,
        };
        let open = |live: &Live| live.stored.thread.archived_at.is_none();
        self.each_thread(|live| (!live.busy() && reset(live) && open(live)).then(|| live.stored.thread.id.clone()))
            .await
    }

    /// Wakes the snoozed threads whose time has come, looking every `every`.
    pub fn keep_snoozes_woken(self: &Arc<Self>, every: Duration) {
        let hub = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(every).await;
                hub.each_thread(|live| {
                    let due = live.stored.thread.snoozed_until.is_some_and(|until| until <= now());
                    if due && wake(&mut live.stored.thread, true) {
                        hub.save_and_announce(live);
                    }
                    None::<()>
                })
                .await;
            }
        });
    }

    /// Brings a snoozed thread back when the user writes to it.
    pub async fn wake(&self, thread_id: &str) {
        let Some(mut live) = self.live(thread_id).await else { return };
        if wake(&mut live.stored.thread, false) {
            self.save_and_announce(&live);
        }
    }

    fn save_and_announce(&self, live: &Live) {
        if let Err(error) = self.store.save_thread(&live.stored) {
            tracing::error!("couldn't save a thread: {error:#}");
        }
        self.announce(&live.stored.thread);
    }

    /// Lets the agents go before the server stops: nothing new starts, and what runs is stopped
    /// where it is, as the restart finds it. With `continues` the threads they worked in go on
    /// once the server is back.
    pub async fn close(&self, continues: bool) {
        self.closing.store(true, Ordering::SeqCst);
        if continues && let Err(error) = self.store.set_setting(CONTINUE_AFTER_THIS_RESTART, Some("true")) {
            tracing::error!("couldn't note that the threads continue after the restart: {error:#}");
        }
        let processes: Vec<u32> = self
            .each_thread(|live| {
                if let Some(preparing) = &live.preparing {
                    preparing.task.abort();
                }
                live.run.as_ref().map(|run| run.process_id)
            })
            .await;
        if processes.is_empty() {
            return;
        }
        for process_id in &processes {
            signal(*process_id, libc::SIGTERM);
        }
        tokio::time::sleep(AGENTS_STOP_WITHIN).await;
        for process_id in processes {
            signal(process_id, libc::SIGKILL);
        }
    }

    fn pull_request_settings(&self) -> PullRequestSettings {
        *self.pull_request_settings.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn set_pull_request_settings(
        self: &Arc<Self>,
        done_on_merge: Option<bool>,
        remove_merged_worktrees: Option<bool>,
        pull_default_branch: Option<bool>,
    ) -> anyhow::Result<()> {
        let mut settings = self.pull_request_settings();
        if let Some(on) = done_on_merge {
            self.store.set_setting(DONE_ON_MERGE, Some(if on { "true" } else { "false" }))?;
            settings.done_on_merge = on;
        }
        if let Some(on) = remove_merged_worktrees {
            self.store.set_setting(REMOVE_MERGED_WORKTREES, Some(if on { "true" } else { "false" }))?;
            settings.remove_merged_worktrees = on;
        }
        if let Some(on) = pull_default_branch {
            self.store.set_setting(PULL_DEFAULT_BRANCH, Some(if on { "true" } else { "false" }))?;
            settings.pull_default_branch = on;
        }
        *self.pull_request_settings.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = settings;
        if pull_default_branch == Some(true) {
            self.pull_default_branches();
        }
        Ok(())
    }

    pub async fn subscribe(&self) -> ListSubscription {
        let updates = self.list_updates.subscribe();
        let mut list: Vec<Thread> = self.each_thread(|live| Some(live.stored.thread.clone())).await;
        list.sort_by(|a, b| b.updated_at.total_cmp(&a.updated_at));
        ListSubscription {
            first: Message::Welcome {
                server: self.server_info(),
                threads: list,
                projects: self.projects_of(&self.projects.lock().await),
            },
            updates,
        }
    }

    /// What changed in the thread after revision `since`, and every change from now on.
    pub async fn open(&self, thread_id: &str, since: u64) -> anyhow::Result<ThreadSubscription> {
        let mut live = self.live_or(thread_id, "That thread no longer exists.").await?;
        live.flush(&self.store)?;
        let rev = live.stored.thread.rev;
        // A client ahead of the server has a copy from before the server's data was replaced.
        let reset = since > rev;
        let items = self.store.items_since(thread_id, if reset { 0 } else { since })?;
        Ok(ThreadSubscription { reset, activity: live.activity(), items, rev, updates: live.updates.subscribe() })
    }

    /// Starts a turn and returns the thread it runs in.
    pub async fn send(
        self: &Arc<Self>,
        thread_id: Option<String>,
        new_thread: Option<NewThread>,
        message: NewMessage,
    ) -> anyhow::Result<String> {
        self.send_started_by(thread_id, new_thread, message, None).await
    }

    /// Starts a turn as `send` does; a new thread is one the agent of `started_by` made, when given.
    async fn send_started_by(
        self: &Arc<Self>,
        thread_id: Option<String>,
        new_thread: Option<NewThread>,
        message: NewMessage,
        started_by: Option<String>,
    ) -> anyhow::Result<String> {
        let NewMessage { text, attachments, now, context, message_id, by, sender } = message;
        let uses = activity_uses(&thread_id, &new_thread, now, &context);
        let text = text.trim().to_string();
        if text.is_empty() && attachments.is_empty() && context.is_none() {
            bail!("There is nothing to send.");
        }
        if message_id.as_deref().is_some_and(|id| uuid::Uuid::parse_str(id).is_err()) {
            bail!("The message's id isn't a UUID. Update the client and try again.");
        }
        if let (Some(thread_id), Some(message_id)) = (&thread_id, &message_id)
            && let Some(live) = self.live(thread_id).await
            && self.has_message(&live, message_id)?
        {
            return Ok(thread_id.clone());
        }
        if self.closing.load(Ordering::SeqCst) {
            bail!("Your server is restarting. Send it again once it is back.");
        }
        if let Some(gone) = attachments.iter().find(|path| !Path::new(path).is_file()) {
            bail!("{} is no longer on your server. Attach it again.", file_name(gone));
        }
        if !motile_protocol::skills::mentions(&text).is_empty() {
            self.list_skills_once(thread_id.as_deref(), new_thread.as_ref()).await;
        }
        let (thread_id, is_new) = match (thread_id, new_thread) {
            (Some(thread_id), _) => (thread_id, false),
            (None, Some(mut new_thread)) => {
                // The client sends again what it can't tell reached the server.
                if let Some(id) = &new_thread.id {
                    let id = self.free_thread_id(id)?;
                    if self.thread(&id).await.is_some() {
                        return Ok(id);
                    }
                    new_thread.id = Some(id);
                }
                let from = match &new_thread.from {
                    Some(from) => Some(self.copied_history(from)?),
                    None => None,
                };
                let mut stored = self.new_thread(new_thread, &text, &attachments, None).await?;
                stored.started_by = started_by;
                if let Some((copied, _)) = &from {
                    stored.thread.title = copied.title.clone();
                    stored.title_source = TitleSource::Generated;
                }
                self.store.save_thread(&stored)?;
                let thread_id = stored.thread.id.clone();
                if let Some((thread_id, worktree)) = thread_worktree(&stored) {
                    self.lock_worktrees().insert(thread_id, worktree);
                }
                let mut live = Live::new(stored, self.media.clone());
                if let Some((_, items)) = from {
                    live.take_history(&self.store, items)?;
                }
                self.add_thread(live);
                (thread_id, true)
            }
            (None, None) => bail!("No thread was given."),
        };
        let mut live = self.live_or(&thread_id, "That thread no longer exists.").await?;
        if let Some(message_id) = &message_id
            && self.has_message(&live, message_id)?
        {
            return Ok(thread_id);
        }
        let live = &mut *live;
        // A message goes on with the thread, so it no longer waits to be continued.
        live.stored.thread.interruption = None;
        let message = Queued {
            id: message_id.unwrap_or_else(new_id),
            media: self.keep_attached(&thread_id, &attachments)?,
            skills: self.mentioned_skills(&live.stored.thread, &text),
            programs: ui::states(&self.store.programs_changed(&thread_id)?),
            text: text.clone(),
            attachments,
            held: false,
            sending: false,
            notice: None,
            by,
            context,
        };
        // Continuing is a feature of its own, which its request keeps.
        if let Some(sender) = sender.as_ref().filter(|_| text != CONTINUE_PROMPT) {
            let thread = &live.stored.thread;
            activity::keep_used(&self.store, sender, uses, &thread.id, &thread.project_id);
        }
        if live.preparing.is_some() {
            live.queued.push(message);
            live.save_queued(&self.store)?;
            live.send_activity();
            return Ok(thread_id);
        }
        if let Some(run) = live.run.as_ref().filter(|run| run.resting.is_none()) {
            // An agent that is only monitoring takes the message right away.
            let idle = run.received_result;
            live.note_skills(&message.skills);
            if !idle || !live.write_prompt(&prompt(&message), &message.id) {
                live.queued.push(message);
                if now && !idle && live.steer(&self.store, live.queued.len() - 1)? {
                    return Ok(thread_id);
                }
                live.save_queued(&self.store)?;
                live.send_activity();
                return Ok(thread_id);
            }
            live.append_queued(&self.store, message)?;
            self.resume(live)?;
            return Ok(thread_id);
        }
        self.note_handoff(live)?;
        let prompt = prompt(&message);
        live.append_queued(&self.store, message)?;
        self.start_turn(live, prompt)?;
        if is_new {
            tokio::spawn(self.clone().title_from_first_message(thread_id.clone(), text));
        }
        Ok(thread_id)
    }

    /// Whether the thread has the message already, queued or in its transcript.
    fn has_message(&self, live: &Live, message_id: &str) -> anyhow::Result<bool> {
        if live.queued.iter().any(|queued| queued.id == message_id) {
            return Ok(true);
        }
        Ok(self.store.item(&live.stored.thread.id, message_id)?.is_some())
    }

    /// Keeps the state the user left a program of an assistant item in. Only the server's own
    /// threads keep it: a removed server's copies are as their server left them.
    pub async fn set_ui(&self, thread_id: &str, item_id: &str, block: u32, state: Value) -> anyhow::Result<()> {
        if state.to_string().len() > ui::MAX_STATE {
            bail!("That program's state is too large to keep.");
        }
        let mut live = self.live_or(thread_id, "That thread isn't one of your server's own.").await?;
        live.set_ui(&self.store, item_id, block, state)
    }

    /// Photos of what `query` names, kept as the thread's media.
    pub async fn ui_images(&self, thread_id: &str, query: &str) -> anyhow::Result<Vec<Media>> {
        self.thread(thread_id).await.context("That thread isn't one of your server's own.")?;
        let images = self.lookups.images(&self.media, &self.store, query).await?;
        for image in &images {
            self.store.save_media(thread_id, &image.id)?;
        }
        Ok(images)
    }

    /// Notes that the thread has the files, and copies the images and videos among them.
    fn keep_attached(&self, thread_id: &str, attachments: &[String]) -> anyhow::Result<Vec<Media>> {
        for path in attachments {
            self.store.save_attachment(thread_id, path)?;
        }
        let media = self.media.capture_attached(attachments);
        for id in media.iter().flat_map(|media| std::iter::once(&media.id).chain(&media.poster)) {
            self.store.save_media(thread_id, id)?;
        }
        Ok(media)
    }

    /// Keeps the files attached to messages from before the server kept every attached file, so
    /// that they can be downloaded again while their uploads are still here.
    pub fn keep_earlier_attachments(self: &Arc<Self>) {
        let hub = self.clone();
        tokio::spawn(async move {
            let missing = match hub.store.messages_missing_attached() {
                Ok(missing) => missing,
                Err(error) => return tracing::error!("couldn't read the attached files: {error:#}"),
            };
            for (thread_id, item_id) in missing {
                if let Err(error) = hub.keep_attached_to(&thread_id, &item_id).await {
                    tracing::warn!("couldn't keep the files attached to {item_id}: {error:#}");
                }
            }
        });
    }

    async fn keep_attached_to(&self, thread_id: &str, item_id: &str) -> anyhow::Result<()> {
        let Some(item) = self.store.item(thread_id, item_id)? else { return Ok(()) };
        let ItemKind::User { attachments, .. } = &item.kind else { return Ok(()) };
        let unkept: Vec<String> =
            attachments.iter().filter(|path| !item.media.iter().any(|media| &media.src == *path)).cloned().collect();
        let media = self.media.clone();
        let kept = tokio::task::spawn_blocking(move || media.capture_attached(&unkept)).await?;
        if kept.is_empty() {
            return Ok(());
        }
        let Some(mut live) = self.live(thread_id).await else {
            self.forget_media(&kept);
            return Ok(());
        };
        live.keep_attached_media(&self.store, item_id, kept)
    }

    /// Removes copies no thread came to show.
    fn forget_media(&self, kept: &[Media]) {
        for id in kept.iter().flat_map(|media| std::iter::once(&media.id).chain(&media.poster)) {
            if !self.store.shows_media(id).unwrap_or(true) {
                self.media.remove(id);
            }
        }
    }

    /// Removes the uploads that were never sent.
    pub fn keep_uploads_swept(self: &Arc<Self>) {
        let hub = self.clone();
        tokio::spawn(async move {
            loop {
                match hub.store.attachments(None) {
                    Ok(kept) => files::sweep_uploads(&hub.attachments_folder, &kept, UNSENT_UPLOADS_STAY),
                    Err(error) => tracing::error!("couldn't read the attachments: {error:#}"),
                }
                tokio::time::sleep(SWEEP_UPLOADS_EVERY).await;
            }
        });
    }

    /// A removed server's thread from this server's copy, with its transcript, for a new thread to
    /// go on from.
    fn copied_history(&self, from: &CopiedThread) -> anyhow::Result<(Thread, Vec<Item>)> {
        let thread = self.store.copied_thread(&from.origin, &from.thread_id)?;
        let thread = thread.context("That thread isn't copied to this server yet.")?;
        Ok((thread, self.store.copied_items(&from.origin, &from.thread_id)?))
    }

    /// A new thread from what a client asked for. With `folder` and no worktree it works in that
    /// folder, as a thread an agent started works in the folder of the thread that started it.
    /// The id a thread the client named starts under. A deleted thread's id isn't taken again, so
    /// its message starts a thread under an id made from it, the same each time it comes.
    fn free_thread_id(&self, requested: &str) -> anyhow::Result<String> {
        let Ok(mut uuid) = uuid::Uuid::parse_str(requested) else { bail!("{requested} isn't a thread's id.") };
        let mut id = requested.to_string();
        while self.store.thread_deleted(&id)? {
            uuid = uuid::Uuid::new_v5(&uuid, b"deleted");
            id = uuid.to_string();
        }
        Ok(id)
    }

    async fn new_thread(
        &self,
        new_thread: NewThread,
        text: &str,
        attachments: &[String],
        folder: Option<String>,
    ) -> anyhow::Result<StoredThread> {
        let projects = self.projects.lock().await;
        let project = projects
            .iter()
            .find(|project| project.id == new_thread.project_id)
            .context("That project is no longer on your server.")?;
        let account = match &new_thread.agent_account {
            Some(id) => {
                self.agent_accounts().into_iter().find(|account| &account.id == id && account.agent == new_thread.agent)
            }
            None => Some(self.default_account(new_thread.agent)),
        };
        let account = account.context("That account is no longer on your server.")?;
        let created_at = now();
        let id = new_thread.id.clone().unwrap_or_else(new_id);
        let no_project = self.is_no_project(project);
        let worktree = match new_thread.worktree {
            Some(_) if no_project => bail!("A thread without a project can't have a worktree."),
            // A repository without a commit has nothing for a worktree to start from, so the thread
            // works in the project's folder, as it would have been offered had the client known.
            Some(_) if !git::has_commit(&project.path, &self.environment).await => None,
            Some(new) if new.base.is_empty() || new.base.starts_with('-') => {
                bail!("{} isn't a branch to start from.", new.base)
            }
            Some(new) => {
                let name: String = uuid::Uuid::new_v4().simple().to_string().chars().take(8).collect();
                let folder = self.worktrees_folder.join(file_name(&project.path)).join(&name);
                let branch = match new.branch.filter(|branch| !branch.is_empty() && !branch.starts_with('-')) {
                    Some(wanted) => git::free_branch_name(&project.path, &self.environment, &wanted).await,
                    None => temporary_branch(&folder.to_string_lossy()),
                };
                Some((folder.to_string_lossy().into_owned(), StoredWorktree { branch, base: new.base }))
            }
            None => None,
        };
        let (cwd, worktree) = match (worktree, folder) {
            (Some((folder, worktree)), _) => (folder, Some(worktree)),
            (None, Some(folder)) => (folder, None),
            (None, None) if no_project => (self.make_no_project_folder(&id, text, created_at)?, None),
            (None, None) => (project.path.clone(), None),
        };
        let thread = Thread {
            id,
            title: title::placeholder(text, attachments),
            project_id: project.id.clone(),
            cwd,
            agent: new_thread.agent,
            agent_account: account.id,
            model: checked("model", new_thread.model)?,
            effort: checked("effort", new_thread.effort)?,
            access: new_thread.access,
            created_at,
            updated_at: created_at,
            archived_at: None,
            snoozed_until: None,
            woke_at: None,
            position: created_at,
            running: false,
            monitoring: false,
            needs_approval: false,
            asking: None,
            agents: 0,
            turn_ended_at: None,
            pull_request: None,
            watching: false,
            git_stage: None,
            interruption: None,
            parent: None,
            rev: 0,
        };
        let title_source = TitleSource::Placeholder;
        Ok(StoredThread {
            thread,
            session_id: None,
            title_source,
            next_seq: 0,
            worktree,
            reports: false,
            started_by: None,
        })
    }

    async fn title_from_first_message(self: Arc<Self>, thread_id: String, text: String) {
        let Some(account) = self.account_of_thread(&thread_id).await else { return };
        let writer = self.writer(&account);
        let generated = title::from_first_message(&self.environment, &writer, &text).await;
        self.keep_written_in(&thread_id, &writer, Purpose::Title).await;
        match generated {
            Some(generated) if !generated.needs_refinement => {
                self.set_generated_title(&thread_id, generated.title).await
            }
            _ => self.refine_title_later(&thread_id).await,
        }
    }

    /// The first message didn't say enough; the transcript will, once the first turn has ended.
    async fn refine_title_later(self: &Arc<Self>, thread_id: &str) {
        let Some(mut live) = self.live(thread_id).await else { return };
        if live.stored.thread.running {
            live.title_needs_refinement = true;
            return;
        }
        tokio::spawn(self.clone().title_from_transcript(thread_id.to_string()));
    }

    async fn title_from_transcript(self: Arc<Self>, thread_id: String) {
        let Some(account) = self.account_of_thread(&thread_id).await else { return };
        let Some(thread) = self.thread(&thread_id).await else { return };
        let Ok(items) = self.store.items_since(&thread_id, 0) else { return };
        let previous_title = thread.title;
        let writer = self.writer(&account);
        let generated = title::from_transcript(&writer, &previous_title, &items).await;
        self.keep_written_in(&thread_id, &writer, Purpose::Title).await;
        let Some(generated) = generated else { return };
        self.set_generated_title(&thread_id, generated.title).await;
    }

    /// Keeps what the writer's answers took, with what the tokens of the turns took.
    fn keep_written(&self, writer: &Writer, purpose: Purpose, project_id: &str, thread_id: Option<&str>) {
        let spent = writer.take_spent();
        let spender = (writer.agent, writer.agent_account.as_str());
        if let Err(error) = self.store.save_written(now(), project_id, thread_id, spender, purpose, &spent) {
            tracing::warn!("couldn't keep what writing took: {error:#}");
        }
    }

    async fn keep_written_in(&self, thread_id: &str, writer: &Writer, purpose: Purpose) {
        let Some(thread) = self.thread(thread_id).await else { return };
        self.keep_written(writer, purpose, &thread.project_id, Some(thread_id));
    }

    /// Who writes titles, commit messages and pull requests: the model the user picked, or the
    /// lightest one of the account's agent. The account writes when it is of the writer's agent,
    /// that agent's default account otherwise.
    fn writer(&self, account: &AgentAccount) -> Writer {
        let models = self.models();
        let model = self.text_model();
        let agent = model
            .as_ref()
            .and_then(|id| models.iter().find(|model| &model.id == id))
            .map_or(account.agent, |model| model.agent);
        let account = if agent == account.agent { account.clone() } else { self.default_account(agent) };
        let model = model.or_else(|| generate::small_codex_model(&models, &account));
        Writer::new(&account, model, self.account_environment(&account))
    }

    /// The model the user picked to write, while the server still has it.
    fn text_model(&self) -> Option<String> {
        let picked = self.text_model.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        picked.filter(|id| self.models().iter().any(|model| &model.id == id))
    }

    pub fn set_text_model(&self, model: Option<String>) -> anyhow::Result<()> {
        if let Some(id) = &model
            && !self.models().iter().any(|model| &model.id == id)
        {
            bail!("{id} isn't a model on your server.");
        }
        self.store.set_setting(TEXT_MODEL, model.as_deref())?;
        *self.text_model.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = model;
        Ok(())
    }

    fn branch_instructions(&self) -> BranchInstructions {
        let picked = self.branch_instructions.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        let default = drafts::BRANCH_INSTRUCTIONS.to_string();
        BranchInstructions { text: picked.unwrap_or_else(|| default.clone()), default }
    }

    pub fn set_branch_instructions(&self, instructions: Option<String>) -> anyhow::Result<()> {
        let instructions = instructions.map(|text| text.trim().to_string());
        let instructions = instructions.filter(|text| !text.is_empty() && text != drafts::BRANCH_INSTRUCTIONS);
        if instructions.as_ref().is_some_and(|text| text.chars().count() > MAX_INSTRUCTIONS_CHARS) {
            bail!("The instructions can be at most {MAX_INSTRUCTIONS_CHARS} characters.");
        }
        self.store.set_setting(BRANCH_INSTRUCTIONS, instructions.as_deref())?;
        *self.branch_instructions.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = instructions;
        Ok(())
    }

    async fn account_of_thread(&self, thread_id: &str) -> Option<AgentAccount> {
        Some(self.account_of(&self.thread(thread_id).await?))
    }

    async fn set_generated_title(&self, thread_id: &str, title: String) {
        let Some(mut live) = self.live(thread_id).await else { return };
        if live.stored.title_source != TitleSource::Placeholder {
            return;
        }
        live.stored.thread.title = title;
        live.stored.title_source = TitleSource::Generated;
        if let Err(error) = self.store.save_thread(&live.stored) {
            tracing::error!(thread_id, "couldn't save a title: {error:#}");
        }
        self.announce(&live.stored.thread);
    }

    /// Allows or refuses a tool call the running turn waits with. `answers` is what the user chose
    /// when the call asked them questions.
    pub async fn answer(
        &self,
        thread_id: &str,
        approval_id: &str,
        allow: bool,
        answers: HashMap<String, String>,
        by: Actor,
    ) -> anyhow::Result<()> {
        let mut live = self.live_or(thread_id, "That thread no longer exists.").await?;
        let live = &mut *live;
        let waiting = live.activity.approvals.iter().position(|approval| approval.id == approval_id);
        let approval = live.activity.approvals.remove(waiting.context("The agent no longer waits for that answer.")?);
        if let Some(asked) = live.server_asks.remove(approval_id) {
            let _ = asked.send(allow);
            return self.announce_approvals(live);
        }
        let thread = &live.stored.thread;
        let line = agents::answer(thread.agent, &approval, allow, &answers, thread.access);
        if line.is_some_and(|line| !live.write(line)) {
            bail!("The agent is no longer running.");
        }
        match approval.tool_name == QUESTION_TOOL {
            true => live.keep_answers(&self.store, &approval, allow, &answers, by)?,
            false => live.keep_decision(&self.store, &approval, by)?,
        }
        // Codex presents its plan when the turn has ended: it carries the plan out in a new
        // turn, and has nothing left to do when the plan is refused.
        let ended = live.run.as_ref().is_some_and(|run| run.received_result);
        if ended && allow {
            return self.resume(live);
        }
        if let Some(run) = live.run.as_mut().filter(|_| ended) {
            run.input = None;
        }
        self.announce_approvals(live)
    }

    /// Gives the agent a queued message now: the turn that runs takes it at once, an idle
    /// process starts its next turn with it.
    pub async fn send_queued(self: &Arc<Self>, thread_id: &str, message_id: &str) -> anyhow::Result<()> {
        let mut live = self.live_or(thread_id, "That thread no longer exists.").await?;
        let live = &mut *live;
        let index = live.queued_index(message_id)?;
        if live.preparing.is_some() {
            bail!("The agent isn't ready yet. Send it again in a moment.");
        }
        let Some(run) = live.run.as_ref().filter(|run| run.resting.is_none()) else {
            let queued = live.queued.remove(index);
            return self.start_next_turn(live, queued);
        };
        if !run.received_result {
            if !live.steer(&self.store, index)? {
                bail!("The agent isn't ready yet. Send it again in a moment.");
            }
            return Ok(());
        }
        if !live.give(&self.store, index)? {
            // The agent's process takes no more; the message starts the next turn.
            live.queued[index].held = false;
            live.save_queued(&self.store)?;
            live.send_activity();
            return Ok(());
        }
        self.resume(live)
    }

    /// Takes a queued message back.
    pub async fn cancel_queued(&self, thread_id: &str, message_id: &str) -> anyhow::Result<()> {
        let mut live = self.live_or(thread_id, "That thread no longer exists.").await?;
        let index = live.queued_index(message_id)?;
        live.queued.remove(index);
        live.save_queued(&self.store)?;
        live.send_activity();
        Ok(())
    }

    /// Asks the user whether the thread's agent may make `call` of a tool of the gateway's MCP servers,
    /// and waits for the answer: refused when the thread is gone or its turn ends first.
    pub async fn ask_for_mcp_call(&self, thread_id: &str, tool_name: String, input: String) -> bool {
        let answered = {
            let Some(mut live) = self.live(thread_id).await else { return false };
            let live = &mut *live;
            let id = format!("mcp:{}", hex::encode(rand::random::<[u8; 8]>()));
            let (asked, answered) = oneshot::channel();
            live.server_asks.insert(id.clone(), asked);
            live.set_thinking(false);
            live.activity.approvals.push(Approval { id, tool_name, input });
            if let Some(tally) = &mut live.tally {
                tally.asked();
            }
            if let Err(error) = self.announce_approvals(live) {
                tracing::warn!(thread_id, "couldn't ask for a tool call: {error:#}");
            }
            answered
        };
        answered.await.unwrap_or(false)
    }

    fn announce_approvals(&self, live: &mut Live) -> anyhow::Result<()> {
        let Live { server_asks, activity, .. } = live;
        server_asks.retain(|id, _| activity.approvals.iter().any(|approval| approval.id == *id));
        live.note_waiting();
        let asks = !live.activity.approvals.is_empty();
        if asks && !live.stored.thread.needs_approval {
            wake(&mut live.stored.thread, true);
        }
        live.stored.thread.needs_approval = asks;
        live.stored.thread.asking = live.activity.approvals.first().map(Approval::ask);
        self.store.save_thread(&live.stored)?;
        live.send_activity();
        self.announce(&live.stored.thread);
        Ok(())
    }

    /// Stops the thread's agent and every agent it started, theirs first. What they would have
    /// told it of their ends isn't told: the user stopped it all.
    pub async fn stop(self: &Arc<Self>, thread_id: &str, by: Actor) {
        if let Some(mut live) = self.live(thread_id).await {
            live.queued.retain(|queued| queued.notice.is_none());
        }
        let children = self.descendants(thread_id);
        for id in &children {
            let Some(mut child) = self.live(id).await else { continue };
            child.stored.reports = false;
            if let Err(error) = self.store.save_thread(&child.stored) {
                tracing::error!(thread_id, "couldn't save a thread: {error:#}");
            }
        }
        for child in children {
            self.stop_one(&child, by.clone()).await;
        }
        self.stop_one(thread_id, by).await;
    }

    async fn stop_one(self: &Arc<Self>, thread_id: &str, by: Actor) {
        let Some(mut live) = self.live(thread_id).await else { return };
        let live = &mut *live;
        live.stopped_by = by;
        if let Some(preparing) = live.preparing.take() {
            preparing.task.abort();
            // What was made of the worktree so far may be half of it.
            if preparing.makes_worktree {
                self.discard_worktree(&live.stored.thread).await;
            }
            if let Err(error) = self.end_without_agent(live, None) {
                tracing::error!(thread_id, "couldn't save the end of a turn: {error:#}");
            }
            return;
        }
        let Some(run) = live.run.as_ref().filter(|run| run.resting.is_none()) else {
            if live.limited()
                && let Err(error) = self.stop_waiting_for_limit(live)
            {
                tracing::error!(thread_id, "couldn't stop a thread that waits for its usage limit: {error:#}");
            }
            return;
        };
        run.interrupted.store(true, Ordering::Relaxed);
        let process_id = run.process_id;
        let thread = &live.stored.thread;
        let stop = agents::stop(thread.agent, live.stored.session_id.as_deref(), run.turn_id.as_deref());
        // A process that is asked to stop ends its turn itself, and is only signalled if it doesn't.
        let asked = stop.is_some_and(|line| live.write(line));
        if !asked && let Some(run) = &mut live.run {
            run.input = None;
            signal(process_id, libc::SIGINT);
        }
        self.end_process(thread_id, process_id);
    }

    fn end_process(self: &Arc<Self>, thread_id: &str, process_id: u32) {
        // Escalate if the agent ignores the interrupt.
        let hub = self.clone();
        let thread_id = thread_id.to_string();
        tokio::spawn(async move {
            for (delay, signal_number) in [(1500, libc::SIGTERM), (2500, libc::SIGKILL)] {
                tokio::time::sleep(Duration::from_millis(delay)).await;
                let live = hub.live(&thread_id).await;
                let still_running = live.as_ref().and_then(|live| live.run.as_ref());
                if still_running.is_none_or(|run| run.process_id != process_id) {
                    return;
                }
                signal(process_id, signal_number);
            }
        });
    }

    pub async fn update(&self, thread_id: &str, change: ThreadChange, by: Actor) -> anyhow::Result<()> {
        let mut live = self.live_or(thread_id, "That thread no longer exists.").await?;
        let live = &mut *live;
        if let Some(id) = change.agent_account.as_ref().filter(|id| **id != live.stored.thread.agent_account) {
            self.move_to_account(live, id)?;
            live.moved_by = by;
        }
        let thread = &mut live.stored.thread;
        if let Some(title) = change.title {
            let title = title.trim().to_string();
            if title.is_empty() {
                bail!("A thread needs a title.");
            }
            thread.title = title;
            live.stored.title_source = TitleSource::User;
        }
        // What a process that is still there has to be told; a new one starts with it.
        let mut told = Vec::new();
        if let Some(model) = change.model {
            thread.model = checked("model", Some(model))?;
            told.push(claude::model_line(thread.model.as_deref()));
        }
        if let Some(effort) = change.effort {
            thread.effort = checked("effort", Some(effort))?;
            told.push(claude::effort_line(thread.effort.as_deref()));
        }
        if let Some(access) = change.access {
            thread.access = access;
            told.push(claude::access_line(access));
        }
        if let Some(on) = change.continues {
            let Some(Interruption::Limit { resets_at, continues }) = &mut thread.interruption else {
                bail!("The thread no longer waits for its usage limit.");
            };
            *continues = on && resets_at.is_some_and(|at| at > now());
        }
        match change.archived {
            Some(true) if thread.running => {
                bail!("The agent is still working. Archive the thread when it has finished.")
            }
            Some(true) if thread.monitoring => {
                bail!("The agent is still monitoring. Archive the thread when it has stopped.")
            }
            Some(true) if thread.agents > 0 => {
                bail!("Its agents are still working. Archive the thread when they have finished.")
            }
            Some(true) => {
                thread.archived_at = thread.archived_at.or_else(|| Some(now()));
                thread.snoozed_until = None;
            }
            Some(false) if thread.archived_at.is_some() => {
                thread.archived_at = None;
                thread.position = now();
            }
            _ => {}
        }
        match change.snoozed_until {
            Some(until) if !until.is_finite() => bail!("A snooze needs a time to wake at."),
            Some(until) if until > now() => {
                thread.snoozed_until = Some(until);
                thread.archived_at = None;
            }
            Some(_) => {
                wake(thread, false);
            }
            None => {}
        }
        if let Some(position) = change.position {
            if !position.is_finite() {
                bail!("A thread's position must be a number.");
            }
            thread.position = position;
        }
        // Codex takes its settings with its next turn.
        if thread.agent == Agent::Claude {
            for line in told {
                live.write(line);
            }
        }
        if live.stored.thread.archived_at.is_some() {
            live.let_go();
        }
        self.store.save_thread(&live.stored)?;
        self.announce(&live.stored.thread);
        Ok(())
    }

    /// Has the thread go on with another account, in the session it has where that account keeps
    /// its sessions. Without one, a new one starts and is told what was said.
    fn move_to_account(&self, live: &mut Live, id: &str) -> anyhow::Result<()> {
        if live.busy() {
            bail!("The agent is still working. Switch accounts once it has finished.");
        }
        live.let_go();
        let account = self.agent_accounts().into_iter().find(|account| account.id == id);
        let account = account.context("That account is no longer on your server.")?;
        let thread = &mut live.stored.thread;
        if account.agent != thread.agent {
            (thread.model, thread.effort) = (None, None);
        }
        thread.agent = account.agent;
        thread.agent_account = account.id;
        live.stored.session_id = self.saved_session(&live.stored.thread)?;
        live.session_model = None;
        Ok(())
    }

    /// Deletes the thread and the threads its agent started, theirs first.
    pub async fn delete(&self, thread_id: &str) -> anyhow::Result<()> {
        let children = self.descendants(thread_id);
        for child in children {
            self.delete_one(&child).await?;
        }
        self.delete_one(thread_id).await
    }

    async fn delete_one(&self, thread_id: &str) -> anyhow::Result<()> {
        let Some(mut live) = self.live(thread_id).await else { return Ok(()) };
        live.deleted = true;
        self.lock_threads().remove(thread_id);
        if let Some(run) = &live.run {
            signal(run.process_id, libc::SIGKILL);
        }
        if let Some(preparing) = &live.preparing {
            preparing.task.abort();
        }
        let thread = &live.stored.thread;
        let repository = self.project_path(&thread.project_id).await.unwrap_or_else(|_| thread.cwd.clone());
        if git::in_repository(&repository) {
            let (environment, name) = (self.environment.clone(), thread_id.to_string());
            tokio::spawn(async move { git::forget_snapshots(&repository, &environment, &name).await });
        }
        if self.lock_worktrees().remove(thread_id).is_some() {
            self.discard_worktree(&live.stored.thread).await;
            self.lock_git().remove(&live.stored.thread.cwd);
            self.announce_projects(&self.projects.lock().await);
        }
        self.mcp_tokens.forget(thread_id);
        let shown = self.store.media_of(thread_id)?;
        let attached = self.store.attachments(Some(thread_id))?;
        self.store.delete_thread(thread_id)?;
        let kept = self.store.attachments(None)?;
        for path in attached.iter().filter(|path| !kept.contains(path)) {
            files::remove_upload(&self.attachments_folder, path);
        }
        for media_id in shown {
            if !self.store.shows_media(&media_id)? {
                self.media.remove(&media_id);
            }
        }
        let _ = self.list_updates.send(Message::ThreadDeleted { thread_id: thread_id.to_string() });
        Ok(())
    }

    pub async fn any_running(&self) -> bool {
        !self.each_thread(|live| live.busy().then_some(())).await.is_empty()
    }

    /// Answers with the project's id, also when the folder was a project already.
    pub async fn add_project(&self, path: &str) -> anyhow::Result<String> {
        let path = if path.len() > 1 { path.trim_end_matches('/') } else { path };
        if !Path::new(path).is_absolute() || !Path::new(path).is_dir() {
            bail!("{path} isn't a folder on your server.");
        }
        let icon = icons::find(path, &self.environment).await;
        let mut projects = self.projects.lock().await;
        if let Some(project) = projects.iter().find(|project| project.path == path) {
            return Ok(project.id.clone());
        }
        let project = StoredProject {
            id: new_id(),
            path: path.to_string(),
            created_at: now(),
            icon,
            icon_chosen: false,
            setup: None,
        };
        let id = project.id.clone();
        self.store.add_project(&project)?;
        projects.push(project);
        drop(projects);
        // The project arrives with what git says of it, so a client knows at once whether a
        // worktree can start there.
        self.read_git(path, false).await;
        self.announce_projects(&self.projects.lock().await);
        Ok(id)
    }

    /// Where new projects and clones go: `projects` in the home folder.
    fn projects_root(&self) -> anyhow::Result<PathBuf> {
        let home = self.environment.variables.get("HOME").context("Your server doesn't know its home folder.")?;
        let root = Path::new(home).join("projects");
        std::fs::create_dir_all(&root).with_context(|| format!("{} can't be made.", root.display()))?;
        Ok(root)
    }

    /// Starts a git repository in a new folder named after `name` and adds it as a project.
    pub async fn new_project(&self, name: &str) -> anyhow::Result<String> {
        let folder = folder_name(name).context("A project needs a name with a letter or a digit in it.")?;
        let path = self.projects_root()?.join(folder);
        if let Err(error) = std::fs::create_dir(&path) {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                bail!("{} already exists. Pick another name, or add it as a local folder.", path.display());
            }
            return Err(error).with_context(|| format!("{} can't be made.", path.display()));
        }
        let path = path.to_string_lossy().into_owned();
        if let Err(error) = git::init(&path, &self.environment).await {
            let _ = std::fs::remove_dir(&path);
            return Err(error);
        }
        self.add_project(&path).await
    }

    pub async fn github(&self) -> GitHubState {
        github::state(&self.environment).await
    }

    pub async fn github_repos(&self) -> anyhow::Result<Message> {
        github::repos(&self.environment).await
    }

    /// Clones `owner/name` from GitHub and adds it as a project. A folder that is that
    /// repository already is only added.
    pub async fn clone_repo(&self, repo: &str) -> anyhow::Result<String> {
        let path = self.projects_root()?.join(github::repo_name(repo)?);
        let folder = path.to_string_lossy().into_owned();
        if !path.exists() {
            github::clone(repo, &folder, &self.environment).await?;
            return self.add_project(&folder).await;
        }
        let origin = git::origin(&folder, &self.environment).await.unwrap_or_default().to_lowercase();
        let origin = origin.trim_end_matches('/').trim_end_matches(".git");
        let repo = repo.to_lowercase();
        if !origin.ends_with(&format!("/{repo}")) && !origin.ends_with(&format!(":{repo}")) {
            bail!("{folder} is already there and isn't {repo}.");
        }
        self.add_project(&folder).await
    }

    pub async fn remove_project(&self, project_id: &str) -> anyhow::Result<()> {
        let mut projects = self.projects.lock().await;
        if projects.iter().any(|project| project.id == project_id && self.is_no_project(project)) {
            bail!("No project stays on your server.");
        }
        self.store.remove_project(project_id)?;
        projects.retain(|project| project.id != project_id);
        self.announce_projects(&projects);
        Ok(())
    }

    pub async fn project_icon(&self, project_id: &str) -> anyhow::Result<Message> {
        let projects = self.projects.lock().await;
        let project = projects.iter().find(|project| project.id == project_id);
        let icon = project.and_then(|project| project.icon.as_deref()).context("That project has no icon.")?;
        Ok(Message::Icon { data: BASE64.encode(icons::read(icon)?) })
    }

    /// Makes the image at `path` the project's icon, or goes back to the one in its folder.
    pub async fn set_project_icon(&self, project_id: &str, path: Option<String>) -> anyhow::Result<()> {
        let found = match path {
            Some(_) => None,
            None => icons::find(&self.project_path(project_id).await?, &self.environment).await,
        };
        let mut projects = self.projects.lock().await;
        let project = projects
            .iter_mut()
            .find(|project| project.id == project_id)
            .context("That project is no longer on your server.")?;
        match path {
            Some(path) => {
                if icons::version(&path).is_none() {
                    bail!("{path} isn't an image of at most 1 MB.");
                }
                project.icon = Some(path);
                project.icon_chosen = true;
            }
            None => {
                project.icon = found;
                project.icon_chosen = false;
            }
        }
        self.store.save_project_icon(project)?;
        self.announce_projects(&projects);
        Ok(())
    }

    pub async fn project_images(&self, project_id: &str) -> anyhow::Result<Message> {
        let folder = self.project_path(project_id).await?;
        let paths = icons::all(&folder, &self.environment).await;
        Ok(Message::ProjectImages { folder, paths })
    }

    /// Sets the shell script that runs in every new worktree of the project, or takes it away.
    pub async fn set_project_setup(&self, project_id: &str, script: Option<String>) -> anyhow::Result<()> {
        let script = script.map(|script| script.trim().to_string()).filter(|script| !script.is_empty());
        if script.as_ref().is_some_and(|script| script.chars().count() > MAX_SETUP_CHARS) {
            bail!("The script can be at most {MAX_SETUP_CHARS} characters.");
        }
        let mut projects = self.projects.lock().await;
        let project = projects
            .iter_mut()
            .find(|project| project.id == project_id)
            .context("That project is no longer on your server.")?;
        project.setup = script;
        self.store.save_project_setup(project)?;
        self.announce_projects(&projects);
        Ok(())
    }

    /// The branches of the project's repository. The project is announced again too, since the
    /// branch may have been switched in a terminal.
    pub async fn branches(&self, project_id: &str) -> anyhow::Result<Message> {
        let path = self.project_path(project_id).await?;
        let branches = git::branches(&path, &self.environment).await?;
        self.announce_projects(&self.projects.lock().await);
        Ok(Message::Branches { branches })
    }

    /// What a new worktree on a branch made from `base` would start at.
    pub async fn worktree_start(&self, project_id: &str, base: &str, fetch: bool) -> anyhow::Result<Message> {
        let path = self.project_path(project_id).await?;
        let start = git::start(&path, &self.environment, base, fetch).await;
        if fetch {
            self.pull_default_branch(project_id).await;
        }
        Ok(Message::WorktreeStart { start: start.name().to_string(), problem: start.problem })
    }

    /// The skills each account's agent can run in the project, as they last listed them. Once
    /// that is older than `SKILLS_FRESH` they are asked again behind the answer; only a project
    /// they never listed waits for them.
    pub async fn skills(self: &Arc<Self>, project_id: &str) -> anyhow::Result<Message> {
        let known = self.lock_skills().get(project_id).cloned();
        let Some((at, accounts)) = known else {
            return Ok(Message::Skills { accounts: self.ask_skills(project_id).await? });
        };
        if at.elapsed() >= SKILLS_FRESH {
            self.lock_skills().insert(project_id.to_string(), (Instant::now(), accounts.clone()));
            let (hub, project_id) = (self.clone(), project_id.to_string());
            tokio::spawn(async move { hub.ask_skills(&project_id).await });
        }
        Ok(Message::Skills { accounts })
    }

    /// Asks each account's agent for the skills it can run in the project, the accounts at once.
    /// What an agent last listed stays while it doesn't answer.
    async fn ask_skills(&self, project_id: &str) -> anyhow::Result<Vec<AccountSkills>> {
        let path = PathBuf::from(self.project_path(project_id).await?);
        let known = self.lock_skills().get(project_id).cloned();
        let asks: Vec<_> = self
            .agent_accounts()
            .into_iter()
            .map(|account| {
                let environment = self.account_environment(&account);
                let path = path.clone();
                tokio::spawn(async move {
                    let skills = match (account.agent, environment.executable(account.agent)) {
                        (_, None) => Some(Vec::new()),
                        (Agent::Claude, Some(_)) => skills::claude_skills(&environment, &path).await,
                        (Agent::Codex, Some(_)) => skills::codex_skills(&environment, &path).await,
                    };
                    (account.id, skills)
                })
            })
            .collect();
        let mut accounts = Vec::new();
        for ask in asks {
            let Ok((account, skills)) = ask.await else { continue };
            let last = || {
                let mut listed = known.iter().flat_map(|(_, accounts)| accounts);
                listed.find(|listed| listed.account == account).map(|listed| listed.skills.clone()).unwrap_or_default()
            };
            let skills = skills.unwrap_or_else(last);
            accounts.push(AccountSkills { account, skills });
        }
        self.lock_skills().insert(project_id.to_string(), (Instant::now(), accounts.clone()));
        Ok(accounts)
    }

    /// Asks the agents for the skills of the thread's project unless they were, for a message that
    /// may start one.
    async fn list_skills_once(&self, thread_id: Option<&str>, new_thread: Option<&NewThread>) {
        let project_id = match (thread_id, new_thread) {
            (Some(thread_id), _) => self.thread(thread_id).await.map(|thread| thread.project_id),
            (None, new_thread) => new_thread.map(|new_thread| new_thread.project_id.clone()),
        };
        let Some(project_id) = project_id else { return };
        if !self.lock_skills().contains_key(&project_id) {
            let _ = self.ask_skills(&project_id).await;
        }
    }

    fn lock_skills(&self) -> std::sync::MutexGuard<'_, HashMap<String, (Instant, Vec<AccountSkills>)>> {
        self.skills.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The skills the thread's account listed in its project that `text` starts.
    fn mentioned_skills(&self, thread: &Thread, text: &str) -> Vec<SkillMention> {
        let account = self.account_of(thread).id;
        let listed = self.lock_skills();
        let accounts = listed.get(&thread.project_id).map(|(_, accounts)| accounts.as_slice()).unwrap_or_default();
        let skills = accounts.iter().find(|listed| listed.account == account).map(|listed| listed.skills.as_slice());
        skills::mentioned(text, skills.unwrap_or_default())
    }

    /// Brings the base a new worktree starts from up to the remote's, and says where it starts now.
    pub async fn update_base(&self, project_id: &str, base: &str) -> anyhow::Result<Message> {
        let path = self.project_path(project_id).await?;
        if let Some(problem) = git::start(&path, &self.environment, base, true).await.problem {
            bail!(problem);
        }
        if git::current_branch(&path).as_deref() == Some(base) {
            self.refuse_while_working(&path).await?;
        }
        git::update_base(&path, &self.environment, base).await?;
        self.read_git(&path, false).await;
        let start = git::start(&path, &self.environment, base, false).await;
        Ok(Message::WorktreeStart { start: start.name().to_string(), problem: None })
    }

    /// Checks a branch out in the project's folder, where its threads without a worktree work.
    pub async fn switch_branch(&self, project_id: &str, branch: &str, create: bool) -> anyhow::Result<()> {
        let path = self.project_path(project_id).await?;
        self.refuse_while_working(&path).await?;
        if !create {
            self.take_back_branch(&path, branch).await?;
        }
        git::switch(&path, &self.environment, branch, create).await?;
        self.read_git(&path, false).await;
        self.announce_projects(&self.projects.lock().await);
        Ok(())
    }

    /// A thread's worktree whose agent switched onto `branch` gives it back, so that the project's
    /// folder can check it out. A worktree that isn't a thread's, or whose agent still works, keeps it.
    async fn take_back_branch(&self, path: &str, branch: &str) -> anyhow::Result<()> {
        let Some(holder) = git::worktree_holding(path, &self.environment, branch).await else { return Ok(()) };
        let holders = self
            .each_thread(|live| {
                let holds = same_folder(&live.stored.thread.cwd, &holder);
                let branch = live.stored.worktree.as_ref().map(|worktree| worktree.branch.clone());
                holds.then(|| (live.activity.running, live.stored.thread.title.clone(), branch))
            })
            .await;
        let own = match holders.into_iter().next() {
            None => bail!("{branch} is checked out in the worktree at {holder}."),
            Some((true, title, _)) => {
                bail!("{branch} is checked out in the worktree of “{title}”, which is still working.")
            }
            Some((false, _, own)) => own,
        };
        git::release_branch(&holder, &self.environment, own.as_deref()).await?;
        self.read_git(&holder, false).await;
        Ok(())
    }

    pub async fn init_repository(&self, project_id: &str) -> anyhow::Result<()> {
        let path = self.project_path(project_id).await?;
        if Path::new(&path) == self.no_project_folder {
            bail!("A thread without a project has no repository.");
        }
        if git::in_repository(&path) {
            bail!("This folder is already in a git repository.");
        }
        git::init(&path, &self.environment).await?;
        self.read_git(&path, false).await;
        Ok(())
    }

    async fn refuse_while_working(&self, folder: &str) -> anyhow::Result<()> {
        let working =
            self.each_thread(|live| (live.stored.thread.cwd == folder && live.activity.running).then_some(()));
        if !working.await.is_empty() {
            bail!("An agent is still working. Switch branches when it has finished.");
        }
        Ok(())
    }

    /// What git says now about the project's folder, or about the thread's worktree, which every
    /// client is told when it has changed.
    pub async fn git_status(&self, project_id: &str, thread_id: Option<&str>, fetch: bool) -> anyhow::Result<Message> {
        let path = self.git_folder(project_id, thread_id).await?;
        let problem = match fetch && git::in_repository(&path) {
            true => git::fetch(&path, &self.environment).await,
            false => None,
        };
        if fetch {
            self.pull_default_branch(project_id).await;
        }
        let (status, files) = self.read_git(&path, fetch).await;
        Ok(Message::GitStatus { status, files, problem })
    }

    /// The changes in the folder the thread works in, or in the project's folder, as a patch.
    pub async fn diff(&self, project_id: &str, thread_id: Option<&str>, scope: DiffScope) -> anyhow::Result<Message> {
        let folder = self.git_folder(project_id, thread_id).await?;
        let environment = &self.environment;
        let (from, to) = match scope {
            DiffScope::PullRequest { number } => {
                let folder = self.github_folder(project_id, thread_id).await?;
                let (patch, truncated) = pull_requests::diff(&folder, environment, number).await?;
                return Ok(Message::Diff { patch, truncated });
            }
            DiffScope::Commit { sha } => {
                let folder = self.github_folder(project_id, thread_id).await?;
                let (patch, truncated) = pull_requests::commit_diff(&folder, environment, &sha).await?;
                return Ok(Message::Diff { patch, truncated });
            }
            DiffScope::Turn { item_id } => {
                let thread_id = thread_id.context("A turn's changes are asked for with its thread.")?;
                let changes = match self.store.item(thread_id, &item_id)?.map(|item| item.kind) {
                    Some(ItemKind::TurnEnd { summary }) => summary.changes,
                    _ => None,
                };
                let snapshot = changes.context("What that turn changed is no longer known.")?.snapshot;
                (git::before_snapshot(&snapshot), snapshot)
            }
            DiffScope::Uncommitted => git::uncommitted(&folder, environment).await?,
            DiffScope::Branch => {
                let base = match thread_id {
                    Some(thread_id) => {
                        let live = self.live(thread_id).await;
                        live.and_then(|live| live.stored.worktree.as_ref().map(|own| own.base.clone()))
                    }
                    None => None,
                };
                git::since_branching(&folder, environment, base.as_deref()).await?
            }
        };
        let (patch, truncated) = git::patch_between(&folder, environment, &from, &to).await?;
        Ok(Message::Diff { patch, truncated })
    }

    /// What GitHub says of the pull request, read in the folder the thread works in or the
    /// project's. The threads in that folder that opened it are told what became of it.
    pub async fn pull_request(
        &self,
        project_id: &str,
        thread_id: Option<&str>,
        number: u64,
    ) -> anyhow::Result<Message> {
        let folder = self.github_folder(project_id, thread_id).await?;
        let pull_request = pull_requests::detail(&folder, &self.environment, number).await?;
        self.keep_pull_request_of(&folder, &pull_request.pull_request).await;
        Ok(Message::PullRequest { pull_request: Box::new(pull_request) })
    }

    /// Does something to the pull request, then reads it and the folder's git state again.
    pub async fn pull_request_action(
        &self,
        project_id: &str,
        thread_id: Option<&str>,
        number: u64,
        action: PullRequestAction,
        method: Option<MergeMethod>,
        text: Option<&str>,
    ) -> anyhow::Result<Message> {
        let folder = self.github_folder(project_id, thread_id).await?;
        let stage = pull_requests::stage(action);
        let following = match stage {
            Some(_) => self.threads_following(project_id, number).await,
            None => Vec::new(),
        };
        for thread_id in &following {
            self.set_git_stage(Some(thread_id), stage).await;
        }
        let acted = pull_requests::act(&folder, &self.environment, number, action, method, text).await;
        for thread_id in &following {
            self.set_git_stage(Some(thread_id), None).await;
        }
        let (title, url) = acted?;
        let pull_request = pull_requests::detail(&folder, &self.environment, number).await?;
        self.keep_pull_request_of(&folder, &pull_request.pull_request).await;
        self.read_git(&folder, true).await;
        Ok(Message::PullRequestDone { title, url, pull_request: Box::new(pull_request) })
    }

    /// The project's threads whose pull request has that number.
    async fn threads_following(&self, project_id: &str, number: u64) -> Vec<String> {
        self.each_thread(|live| {
            let thread = &live.stored.thread;
            let follows = thread.project_id == project_id
                && thread.pull_request.as_ref().is_some_and(|found| found.number == number);
            follows.then(|| thread.id.clone())
        })
        .await
    }

    /// Changes the pull request as the edit says, then reads it again.
    pub async fn pull_request_edit(
        &self,
        project_id: &str,
        thread_id: Option<&str>,
        number: u64,
        edit: PullRequestEdit,
    ) -> anyhow::Result<Message> {
        let folder = self.github_folder(project_id, thread_id).await?;
        let title = pull_requests::edit(&folder, &self.environment, number, &edit).await?;
        let pull_request = pull_requests::detail(&folder, &self.environment, number).await?;
        self.keep_pull_request_of(&folder, &pull_request.pull_request).await;
        Ok(Message::PullRequestDone { title, url: None, pull_request: Box::new(pull_request) })
    }

    /// The pull requests of the project's repository.
    pub async fn pull_requests(
        &self,
        project_id: &str,
        thread_id: Option<&str>,
        state: PullRequestState,
    ) -> anyhow::Result<Message> {
        let folder = self.github_folder(project_id, thread_id).await?;
        Ok(Message::PullRequests { pull_requests: pull_requests::list(&folder, &self.environment, state).await? })
    }

    /// Makes the pull request with that number the thread's own, or takes its own away.
    pub async fn link_pull_request(&self, thread_id: &str, number: Option<u64>) -> anyhow::Result<()> {
        let project_id = self.thread(thread_id).await.context("That thread no longer exists.")?.project_id;
        let found = match number {
            Some(number) => {
                let folder = self.github_folder(&project_id, Some(thread_id)).await?;
                let found = git::pull_request_numbered(&folder, &self.environment, number).await;
                Some(found.with_context(|| format!("GitHub has no pull request #{number} in this repository."))?)
            }
            None => None,
        };
        let mut live = self.live_or(thread_id, "That thread no longer exists.").await?;
        live.stored.thread.watching &= found
            .as_ref()
            .is_some_and(|found| live.stored.thread.pull_request.as_ref().map(|own| own.number) == Some(found.number));
        live.stored.thread.pull_request = found;
        self.store.save_thread(&live.stored)?;
        self.announce(&live.stored.thread);
        Ok(())
    }

    /// Has the thread's agent told what happens on its pull request, or stops it.
    pub async fn watch_pull_request(&self, thread_id: &str, watch: bool) -> anyhow::Result<()> {
        let mut live = self.live_or(thread_id, "That thread no longer exists.").await?;
        if watch && !live.stored.thread.pull_request.as_ref().is_some_and(PullRequest::is_open) {
            bail!("This thread has no open pull request to watch.");
        }
        live.stored.thread.watching = watch;
        self.lock_watched().remove(thread_id);
        self.store.save_thread(&live.stored)?;
        self.announce(&live.stored.thread);
        Ok(())
    }

    fn lock_watched(&self) -> std::sync::MutexGuard<'_, HashMap<String, Seen>> {
        self.watched.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Fetches the price list from `url` now and once a day, and keeps it for when it can't.
    pub fn keep_prices_current(self: &Arc<Self>, url: String) {
        let hub = self.clone();
        tokio::spawn(async move {
            loop {
                let fetched = pricing::fetch(&url).await;
                let wait = if fetched.is_ok() { PRICES_STALE } else { PRICES_RETRY };
                match fetched {
                    Ok(prices) => hub.set_prices(prices),
                    Err(error) => tracing::warn!("couldn't fetch the price list: {error:#}"),
                }
                tokio::time::sleep(wait).await;
            }
        });
    }

    pub fn set_prices(&self, prices: Prices) {
        let saved = serde_json::to_string(&prices).ok();
        if let Err(error) = self.store.set_setting(PRICES, saved.as_deref()) {
            tracing::warn!("couldn't keep the price list: {error:#}");
        }
        *self.prices.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = prices;
    }

    /// What the agents spent from `since` until `until` and what the API would have charged, and
    /// what this server's copies say the agents of the `removed` servers spent.
    pub fn usage(
        &self,
        since: f64,
        until: f64,
        bucket_secs: u32,
        utc_offset_secs: i32,
        removed: &[String],
    ) -> anyhow::Result<Message> {
        let mut buckets = self.store.usage(since, until, bucket_secs, utc_offset_secs)?;
        let accounts = self.agent_accounts();
        let prices = self.prices.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for bucket in &mut buckets {
            prices.price(bucket);
            if let Some(account) = accounts.iter().find(|account| account.id == bucket.account_name) {
                bucket.account_name = account.name.clone();
            }
        }
        let mut copies = Vec::new();
        for origin in removed {
            let mut buckets = self.store.usage_of(Some(origin), since, until, bucket_secs, utc_offset_secs)?;
            buckets.iter_mut().for_each(|bucket| prices.price(bucket));
            copies.push(OriginUsage { origin: origin.clone(), buckets });
        }
        Ok(Message::Usage { buckets, copies })
    }

    /// What was done, as `usage` asks it, with the accounts by their names.
    pub fn activity(
        &self,
        since: f64,
        until: f64,
        bucket_secs: u32,
        utc_offset_secs: i32,
        removed: &[String],
    ) -> anyhow::Result<Message> {
        let mut report = self.store.activity_of(None, since, until, bucket_secs, utc_offset_secs)?;
        let connected = self.connected.lock().unwrap();
        for record in &mut report.clients {
            record.connected = connected.contains_key(&record.device);
        }
        drop(connected);
        let accounts = self.agent_accounts();
        for bucket in &mut report.turns {
            if let Some(account) = accounts.iter().find(|account| account.id == bucket.agent_account) {
                bucket.agent_account = account.name.clone();
            }
        }
        let mut copies = Vec::new();
        for origin in removed {
            let report = self.store.activity_of(Some(origin), since, until, bucket_secs, utc_offset_secs)?;
            copies.push(OriginActivity { origin: origin.clone(), report });
        }
        Ok(Message::ActivityReport { report, copies })
    }

    pub fn client_connected(&self, device: &str) {
        *self.connected.lock().unwrap().entry(device.to_string()).or_default() += 1;
    }

    pub fn client_disconnected(&self, device: &str) {
        {
            let mut connected = self.connected.lock().unwrap();
            let Some(count) = connected.get_mut(device) else { return };
            *count -= 1;
            if *count > 0 {
                return;
            }
            connected.remove(device);
        }
        if let Err(error) = self.store.client_left(device, now()) {
            tracing::warn!("couldn't keep when a client left: {error:#}");
        }
    }

    /// What changed in a log after `after`: this server's own, or its copy of `origin`'s. The own
    /// log's projects say which is the "No project", and its usage the names of the accounts.
    pub fn changes(&self, origin: Option<&str>, after: u64) -> anyhow::Result<Message> {
        let Some(origin) = origin else {
            let mut part = self.store.changes(after)?;
            for logged in &mut part.set.projects {
                if Path::new(&logged.value.path) == self.no_project_folder {
                    logged.value.name = "No project".to_string();
                    logged.value.no_project = true;
                }
            }
            let accounts = self.agent_accounts();
            for logged in &mut part.set.usage {
                if let Some(account) = accounts.iter().find(|account| account.id == logged.value.agent_account) {
                    logged.value.agent_account = account.name.clone();
                }
            }
            return Ok(Message::Changes { batch: part.encode()? });
        };
        let part = self.store.copied_changes(origin, after)?;
        Ok(Message::Changes { batch: part.encode()? })
    }

    pub fn watermarks(&self, bucket: Vec<String>) -> anyhow::Result<Message> {
        Ok(Message::Watermarks { own: self.store.watermark()?, copies: self.store.origin_watermarks()?, bucket })
    }

    /// What the agents' accounts have used of their plans now: read again unless they just were.
    pub async fn limits(&self) -> Message {
        Message::Limits { agents: self.read_limits(LIMITS_ANSWER_WITHIN).await }
    }

    /// Reads what the agents' accounts have used of their plans, unless they were read within
    /// `kept_for`. An account that can't be read keeps what it said last time, beside why.
    async fn read_limits(&self, kept_for: Duration) -> Vec<AgentLimits> {
        let mut kept = self.limits.lock().await;
        if let Some((_, agents)) = kept.as_ref().filter(|(at, _)| at.elapsed() < kept_for) {
            return agents.clone();
        }
        let (pooled, own): (Vec<_>, Vec<_>) = self.agent_accounts().into_iter().partition(|account| account.gateway);
        let own = own.into_iter().map(|account| {
            let environment = self.account_environment(&account);
            (account, environment)
        });
        let (mut read, pools) = tokio::join!(limits::read(own.collect()), self.read_pools(pooled));
        read.extend(pools);
        let before = kept.as_ref().map(|(_, agents)| agents.as_slice()).unwrap_or_default();
        let agents = limits::kept(read, before);
        self.note_signed_in(&agents);
        *kept = Some((Instant::now(), agents.clone()));
        agents
    }

    /// What the gateway's accounts that new threads of the Gateway accounts would start on have used,
    /// and for each one pinned to an account, what that account has.
    async fn read_pools(&self, accounts: Vec<AgentAccount>) -> Vec<AgentLimits> {
        let Some(way) = self.gateway.get() else { return vec![] };
        let pool = way.pool();
        let (pinned, placing): (Vec<_>, Vec<_>) = accounts.into_iter().partition(|account| account.pin.is_some());
        let pinned = pinned.into_iter().map(|account| {
            let pooled = pool.iter().find(|pooled| account.pin.as_deref() == Some(pooled.id.as_str()));
            limits::pinned(&account, pooled)
        });
        let reads: Vec<_> = placing
            .into_iter()
            .map(|account| {
                let tunnel = way.tunnel.clone();
                tokio::spawn(async move { limits::pooled(&account, tunnel.pool(account.agent).await) })
            })
            .collect();
        let mut read: Vec<AgentLimits> = pinned.collect();
        for task in reads {
            read.extend(task.await.ok());
        }
        read
    }

    /// Puts what the gateway's accounts have used, as `pool` lists them, in the limits read last,
    /// for the accounts pinned to them.
    async fn keep_pinned_limits(&self, accounts: &[AgentAccount], pool: &[GatewayAccount]) {
        let mut kept = self.limits.lock().await;
        let Some((_, agents)) = kept.as_mut() else { return };
        for account in accounts.iter().filter(|account| account.pin.is_some()) {
            let pooled = pool.iter().find(|pooled| account.pin.as_deref() == Some(pooled.id.as_str()));
            let listed = limits::pinned(account, pooled);
            let same = |read: &&mut AgentLimits| read.agent == account.agent && read.account_name == account.name;
            match agents.iter_mut().find(same) {
                Some(read) => *read = listed,
                None => agents.push(listed),
            }
        }
    }

    /// Looks at the watched pull requests every so often and tells their threads' agents what
    /// changed.
    pub fn watch_pull_requests(self: &Arc<Self>, every: Duration) {
        let hub = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(every).await;
                hub.look_at_watched().await;
            }
        });
    }

    async fn look_at_watched(self: &Arc<Self>) {
        let watched: Vec<(String, String, u64)> = self
            .each_thread(|live| {
                let thread = &live.stored.thread;
                let number = thread.pull_request.as_ref().filter(|_| thread.watching)?.number;
                Some((thread.id.clone(), thread.project_id.clone(), number))
            })
            .await;
        for (thread_id, project_id, number) in watched {
            let Ok(folder) = self.github_folder(&project_id, Some(&thread_id)).await else { continue };
            let Ok(found) = pull_requests::detail(&folder, &self.environment, number).await else { continue };
            self.keep_pull_request_of(&folder, &found.pull_request).await;
            let seen = seen(&found);
            let before = self.lock_watched().insert(thread_id.clone(), seen.clone());
            let Some(before) = before else { continue };
            let Some(text) = news(&found, &before, &seen) else { continue };
            if let Err(error) = self
                .send(
                    Some(thread_id.clone()),
                    None,
                    NewMessage::text(text, Actor::Server { reason: ServerReason::PullRequest }),
                )
                .await
            {
                tracing::warn!(thread_id, "couldn't tell the agent about its pull request: {error:#}");
            }
        }
    }

    /// The folder `gh` is run in for the thread's pull requests: where it works, or the project's
    /// folder while its worktree is gone.
    async fn github_folder(&self, project_id: &str, thread_id: Option<&str>) -> anyhow::Result<String> {
        let folder = self.git_folder(project_id, thread_id).await?;
        if Path::new(&folder).is_dir() {
            return Ok(folder);
        }
        self.project_path(project_id).await
    }

    /// What is in a folder inside the one the thread works in, or inside the project's.
    pub async fn list_files(&self, project_id: &str, thread_id: Option<&str>, path: &str) -> anyhow::Result<Message> {
        let folder = self.git_folder(project_id, thread_id).await?;
        files::list_files(&folder, path, &self.environment).await
    }

    /// A file inside the folder the thread works in, or inside the project's, or the one git
    /// keeps as `blob` there: its bytes, what kind it is, its size and how many of its bytes to
    /// send.
    pub async fn open_file(
        &self,
        project_id: &str,
        thread_id: Option<&str>,
        path: &str,
        blob: Option<&str>,
    ) -> anyhow::Result<(FileBytes, FileKind, u64, u64)> {
        let Some(blob) = blob else {
            let folder = self.git_folder(project_id, thread_id).await?;
            return files::open_file(&folder, path).await;
        };
        let folder = self.github_folder(project_id, thread_id).await?;
        files::open_blob(&folder, &self.environment, path, blob).await
    }

    /// Where the thread works: in its worktree, in its own folder in "No project", or in the
    /// project's folder.
    async fn git_folder(&self, project_id: &str, thread_id: Option<&str>) -> anyhow::Result<String> {
        let worktree = thread_id.and_then(|thread_id| self.lock_worktrees().get(thread_id).map(|own| own.path.clone()));
        if let Some(path) = worktree {
            return Ok(path);
        }
        let path = self.project_path(project_id).await?;
        let Some(thread_id) = thread_id.filter(|_| Path::new(&path) == self.no_project_folder) else { return Ok(path) };
        let thread = self.thread(thread_id).await.context("That thread is no longer on your server.")?;
        Ok(thread.cwd)
    }

    /// Reads the folder's status. GitHub is asked for the branch's pull request when it was last
    /// asked a while ago or for another commit, and always with `ask_again`.
    async fn read_git(&self, path: &str, ask_again: bool) -> (Option<GitStatus>, Vec<ChangedFile>) {
        let Some((mut status, files, head)) = git::status(path, &self.environment).await else {
            if self.lock_git().remove(path).is_some() {
                self.announce_projects(&self.projects.lock().await);
            }
            return (None, Vec::new());
        };
        let known = match self.lock_git().get(path) {
            Some(read) if !ask_again && read.head == head && read.status.branch == status.branch => {
                Some((read.status.pull_request.clone(), read.pull_request_read))
            }
            _ => None,
        };
        let pushed = status.upstream || (status.ahead == 0 && !status.default);
        let (pull_request, pull_request_read) = match known {
            Some(known) if known.1.elapsed() < PULL_REQUEST_FRESH => known,
            _ if status.pull_requests && status.branch.is_some() && pushed => {
                let found = git::pull_request(path, &self.environment).await;
                // On the default branch a merged or closed one is another branch's history.
                (found.filter(|found| found.is_open() || !status.default), Instant::now())
            }
            _ => (None, Instant::now()),
        };
        if let Some(found) = &pull_request {
            self.follow_pull_request_of(path, found).await;
        }
        status.pull_request = pull_request;
        let read = GitRead { status: status.clone(), head, pull_request_read };
        let before = self.lock_git().insert(path.to_string(), read);
        if before.map(|before| before.status).as_ref() != Some(&status) {
            self.announce_projects(&self.projects.lock().await);
        }
        (Some(status), files)
    }

    fn lock_git(&self) -> std::sync::MutexGuard<'_, HashMap<String, GitRead>> {
        self.git.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Who writes for the run and what its thread is about. Without a thread, the first agent
    /// that is installed writes.
    async fn writer_of(&self, thread_id: Option<&str>) -> anyhow::Result<(Writer, drafts::Thread)> {
        let thread = match thread_id {
            Some(thread_id) => self.thread(thread_id).await,
            None => None,
        };
        let Some(thread) = thread else {
            let installed =
                [Agent::Claude, Agent::Codex].into_iter().find(|agent| self.environment.executable(*agent).is_some());
            let agent = installed.context("No agent is installed on your server to write this.")?;
            return Ok((self.writer(&self.default_account(agent)), drafts::Thread::default()));
        };
        let items = self.store.items_since(&thread.id, 0).unwrap_or_default();
        let said = items.iter().filter_map(|item| match &item.kind {
            ItemKind::User { text, .. } if !text.is_empty() => Some(format!("USER:\n{text}")),
            _ => None,
        });
        let messages = said.collect::<Vec<_>>().join("\n\n");
        Ok((self.writer(&self.account_of(&thread)), drafts::Thread { title: thread.title.clone(), messages }))
    }

    /// Makes the pull request the thread's own, or keeps what GitHub now says of it.
    async fn set_pull_request(&self, thread_id: Option<&str>, pull_request: PullRequest) {
        let Some(thread_id) = thread_id else { return };
        let Some(mut live) = self.live(thread_id).await else { return };
        if live.stored.thread.pull_request.as_ref() == Some(&pull_request) {
            return;
        }
        let was_open = live.stored.thread.pull_request.as_ref().is_some_and(PullRequest::is_open);
        let ended = was_open && !pull_request.is_open();
        let merged = ended && pull_request.merged;
        live.stored.thread.pull_request = Some(pull_request);
        let settings = self.pull_request_settings();
        let thread = &mut live.stored.thread;
        if ended {
            thread.watching = false;
            if settings.done_on_merge && thread.archived_at.is_none() && !thread.running && !thread.monitoring {
                thread.archived_at = Some(now());
                thread.snoozed_until = None;
            }
        }
        if let Err(error) = self.store.save_thread(&live.stored) {
            tracing::error!("couldn't save the thread's pull request: {error:#}");
        }
        self.announce(&live.stored.thread);
        let removes =
            merged && settings.remove_merged_worktrees && live.stored.worktree.is_some() && !live.stored.thread.running;
        let (cwd, project_id) = (live.stored.thread.cwd.clone(), live.stored.thread.project_id.clone());
        drop(live);
        if removes {
            self.remove_merged_worktree(&cwd, &project_id).await;
        }
        if merged && settings.pull_default_branch {
            self.fetch_default_branch(&project_id).await;
        }
    }

    /// Fetches every project's folder and pulls its default branch, when the setting says so.
    pub fn pull_default_branches(self: &Arc<Self>) {
        if !self.pull_request_settings().pull_default_branch {
            return;
        }
        let hub = self.clone();
        tokio::spawn(async move {
            let projects: Vec<String> = hub.projects.lock().await.iter().map(|project| project.id.clone()).collect();
            for project_id in projects {
                hub.fetch_default_branch(&project_id).await;
            }
        });
    }

    async fn fetch_default_branch(&self, project_id: &str) {
        let Ok(path) = self.project_path(project_id).await else { return };
        if !git::in_repository(&path) {
            return;
        }
        if let Some(problem) = git::fetch(&path, &self.environment).await {
            tracing::warn!(path, "couldn't fetch to pull the default branch: {problem}");
            return;
        }
        self.pull_default_branch(project_id).await;
    }

    /// Fast-forwards the project's folder to its remote when the setting says so and it is on the
    /// default branch, behind, with no changes or commits of its own and no agent at work there.
    async fn pull_default_branch(&self, project_id: &str) {
        if !self.pull_request_settings().pull_default_branch {
            return;
        }
        let Ok(path) = self.project_path(project_id).await else { return };
        let Some((status, _, _)) = git::status(&path, &self.environment).await else { return };
        let behind = status.default && status.upstream && status.behind > 0;
        if !behind || status.ahead > 0 || status.changed > 0 || self.refuse_while_working(&path).await.is_err() {
            return;
        }
        if let Err(error) = git::fast_forward(&path, &self.environment).await {
            tracing::warn!(path, "couldn't pull the default branch: {error:#}");
            return;
        }
        Box::pin(self.read_git(&path, false)).await;
    }

    /// Removes the worktree of a thread whose pull request merged, when all it has is pushed and
    /// committed. The thread's next turn makes it again.
    async fn remove_merged_worktree(&self, path: &str, project_id: &str) {
        let Some((status, _, _)) = git::status(path, &self.environment).await else { return };
        if status.changed > 0 || !status.upstream || status.ahead > 0 {
            return;
        }
        let Ok(repository) = self.project_path(project_id).await else { return };
        if let Err(error) = git::remove_worktree(&repository, &self.environment, path).await {
            tracing::warn!(path, "couldn't remove the merged worktree: {error:#}");
            return;
        }
        self.lock_git().remove(path);
        self.announce_projects(&self.projects.lock().await);
    }

    /// Keeps what GitHub now says of a pull request on the threads in that folder that opened it.
    async fn keep_pull_request_of(&self, folder: &str, found: &PullRequest) {
        self.set_pull_request_where(folder, found, |own, _| own.number == found.number).await;
    }

    /// The branch's pull request: kept on the threads in that folder that opened it, and given to
    /// those still at work whose own is merged or closed, which went on in the branch.
    async fn follow_pull_request_of(&self, folder: &str, found: &PullRequest) {
        let follows = |own: &PullRequest, thread: &Thread| {
            own.number == found.number || (!own.is_open() && thread.archived_at.is_none())
        };
        self.set_pull_request_where(folder, found, follows).await;
    }

    async fn set_pull_request_where(
        &self,
        folder: &str,
        found: &PullRequest,
        wanted: impl Fn(&PullRequest, &Thread) -> bool,
    ) {
        let chosen: Vec<String> = self
            .each_thread(|live| {
                let thread = &live.stored.thread;
                let own = thread.pull_request.as_ref().filter(|_| thread.cwd == folder)?;
                wanted(own, thread).then(|| thread.id.clone())
            })
            .await;
        for thread_id in chosen {
            self.set_pull_request(Some(&thread_id), found.clone()).await;
        }
    }

    /// Asks GitHub every so often what became of the threads' pull requests: those of the threads
    /// that aren't done unless merged, and those of the done ones while open.
    pub fn keep_pull_requests_current(self: &Arc<Self>, every: Duration) {
        let hub = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(every).await;
                hub.refresh_pull_requests().await;
            }
        });
    }

    async fn refresh_pull_requests(&self) {
        let followed: Vec<(String, String, u64)> = self
            .each_thread(|live| {
                let thread = &live.stored.thread;
                let pull_request = thread.pull_request.as_ref().filter(|found| match thread.archived_at {
                    Some(_) => found.is_open(),
                    None => !found.merged,
                })?;
                Some((thread.id.clone(), thread.project_id.clone(), pull_request.number))
            })
            .await;
        let mut asked: HashMap<(String, u64), Option<PullRequest>> = HashMap::new();
        for (thread_id, project_id, number) in followed {
            let Ok(folder) = self.project_path(&project_id).await else { continue };
            let key = (folder, number);
            if !asked.contains_key(&key) {
                let found = git::pull_request_numbered(&key.0, &self.environment, number).await;
                asked.insert(key.clone(), found);
            }
            let Some(found) = asked.get(&key).cloned().flatten() else { continue };
            self.set_pull_request(Some(&thread_id), found).await;
        }
    }

    /// Carries the action out in the project's folder, or in the worktree of the run's thread,
    /// telling `started` each stage as it starts, and answers with what it did.
    pub async fn git_run(&self, project_id: &str, run: GitRun, started: impl Fn(GitStage)) -> anyhow::Result<Message> {
        let path = self.git_folder(project_id, run.thread_id.as_deref()).await?;
        let done = self.git_stages(project_id, &path, &run, started).await;
        self.set_git_stage(run.thread_id.as_deref(), None).await;
        self.read_git(&path, true).await;
        done
    }

    async fn git_stages(
        &self,
        project_id: &str,
        path: &str,
        run: &GitRun,
        started: impl Fn(GitStage),
    ) -> anyhow::Result<Message> {
        let environment = &self.environment;
        let started = async |stage| {
            self.set_git_stage(run.thread_id.as_deref(), Some(stage)).await;
            started(stage);
        };
        let done = |title: String, description: Option<String>, url: Option<String>, next: Option<GitAction>| {
            Ok(Message::GitDone { title, description: description.filter(|text| !text.is_empty()), url, next })
        };
        if run.action == GitAction::Pull {
            started(GitStage::Pull).await;
            let before = git::head(path, environment).await.ok();
            git::pull(path, environment).await?;
            let pulled = git::head(path, environment).await.ok() != before;
            return done(if pulled { "Pulled" } else { "Already up to date" }.to_string(), None, None, None);
        }

        let read = git::status(path, environment).await.context("This folder isn't a git repository.")?;
        let status = read.0;
        let commits = matches!(run.action, GitAction::Commit | GitAction::CommitPush | GitAction::CommitPushPr);
        let opens = matches!(run.action, GitAction::CreatePr | GitAction::CommitPushPr);
        let pushes = run.action != GitAction::Commit;
        if pushes && status.branch.is_none() {
            bail!("Check out a branch before pushing.");
        }
        let (writer, thread) = self.writer_of(run.thread_id.as_deref()).await?;
        let message = run.message.as_deref().map(str::trim).filter(|message| !message.is_empty());

        let committed = commits && status.changed > 0;
        if committed {
            let draft = match (message, run.new_branch) {
                (Some(_), false) => None,
                _ => {
                    started(GitStage::Message).await;
                    let naming = self.branch_instructions().text;
                    let draft = drafts::commit_message(path, environment, &writer, &thread, &run.paths, &naming).await;
                    self.keep_written(&writer, Purpose::Commit, project_id, run.thread_id.as_deref());
                    Some(draft?)
                }
            };
            if run.new_branch {
                let suggested = draft.as_ref().and_then(|draft| draft.branch.clone());
                self.branch_off(path, suggested).await?;
            }
            started(GitStage::Commit).await;
            let message = message.map(str::to_string).or(draft.map(|draft| draft.message())).unwrap_or_default();
            git::commit(path, environment, &message, &run.paths).await?;
        } else if run.action == GitAction::Commit {
            bail!("There is nothing to commit.");
        } else if run.new_branch {
            let subject = git::head(path, environment).await?.1;
            self.branch_off(path, drafts::branch_name(&subject)).await?;
        }

        let (commit, subject) = git::head(path, environment).await?;
        let mut pushed = None;
        let status = git::status(path, environment).await.map(|read| read.0).unwrap_or(status);
        if pushes && (!status.upstream || status.ahead > 0) {
            started(GitStage::Push).await;
            git::push(path, environment).await?;
            pushed = git::upstream(path, environment).await;
        }
        if opens {
            if let Some(open) = git::pull_request(path, environment).await.filter(PullRequest::is_open) {
                self.set_pull_request(run.thread_id.as_deref(), open.clone()).await;
                return done(format!("PR #{} is already open", open.number), Some(open.title), Some(open.url), None);
            }
            started(GitStage::PullRequestText).await;
            let draft = drafts::pull_request(path, environment, &writer, &thread).await;
            self.keep_written(&writer, Purpose::PullRequest, project_id, run.thread_id.as_deref());
            let (title, body) = draft?;
            started(GitStage::PullRequest).await;
            let url = git::open_pull_request(path, environment, &title, &body).await?;
            let number = url.rsplit('/').next().unwrap_or_default();
            let opened = match (git::pull_request(path, environment).await, number.parse()) {
                (Some(opened), _) => Some(opened),
                (None, Ok(number)) => {
                    let (title, url) = (title.clone(), url.clone());
                    Some(PullRequest { number, title, url, draft: false, merged: false, closed: false })
                }
                (None, Err(_)) => None,
            };
            if let Some(opened) = opened {
                self.set_pull_request(run.thread_id.as_deref(), opened).await;
            }
            return done(format!("Created PR #{number}"), Some(title), Some(url), None);
        }
        if let Some(upstream) = pushed {
            let opened = self
                .lock_git()
                .get(path)
                .and_then(|read| read.status.pull_request.as_ref().map(PullRequest::is_open))
                .unwrap_or(false);
            let next = (!status.default && status.pull_requests && !opened).then_some(GitAction::CreatePr);
            return done(format!("Pushed {commit} to {upstream}"), Some(subject), None, next);
        }
        if committed {
            return done(format!("Committed {commit}"), Some(subject), None, status.remote.then_some(GitAction::Push));
        }
        done("Already up to date".to_string(), None, None, None)
    }

    async fn set_git_stage(&self, thread_id: Option<&str>, stage: Option<GitStage>) {
        let Some(thread_id) = thread_id else { return };
        let Some(mut live) = self.live(thread_id).await else { return };
        if live.stored.thread.git_stage == stage {
            return;
        }
        live.stored.thread.git_stage = stage;
        self.announce(&live.stored.thread);
    }

    /// Makes a branch for the work from what is checked out and switches to it.
    async fn branch_off(&self, path: &str, suggested: Option<String>) -> anyhow::Result<()> {
        self.refuse_while_working(path).await?;
        let name = suggested.unwrap_or_else(|| "feature".to_string());
        let name = git::free_branch_name(path, &self.environment, &name).await;
        git::switch(path, &self.environment, &name, true).await
    }

    fn is_no_project(&self, project: &StoredProject) -> bool {
        Path::new(&project.path) == self.no_project_folder
    }

    /// Makes the folder a thread without a project works in: named after the day and its first
    /// words, so that it can be found among the others.
    fn make_no_project_folder(&self, thread_id: &str, text: &str, created_at: f64) -> anyhow::Result<String> {
        let day = chrono::DateTime::from_timestamp(created_at as i64, 0).unwrap_or_default().format("%Y-%m-%d");
        let words = folder_name(&text.split_whitespace().take(5).collect::<Vec<_>>().join(" "));
        let short_id: String = thread_id.chars().take(8).collect();
        let name = [Some(day.to_string()), words, Some(short_id)].into_iter().flatten().collect::<Vec<_>>().join("-");
        let folder = self.no_project_folder.join(name);
        std::fs::create_dir_all(&folder).with_context(|| format!("{} can't be made.", folder.display()))?;
        Ok(folder.to_string_lossy().into_owned())
    }

    async fn project_path(&self, project_id: &str) -> anyhow::Result<String> {
        let projects = self.projects.lock().await;
        let project = projects.iter().find(|project| project.id == project_id);
        Ok(project.context("That project is no longer on your server.")?.path.clone())
    }

    /// Looks for every project's icon in its folder again.
    pub async fn refresh_icons(&self) {
        let ids: Vec<String> = self.projects.lock().await.iter().map(|project| project.id.clone()).collect();
        let mut changed = false;
        for id in ids {
            changed |= self.refresh_icon(&id).await;
        }
        if changed {
            self.announce_projects(&self.projects.lock().await);
        }
    }

    /// Looks for the project's icon in its folder again, unless the user picked one that is still
    /// there. Says whether it changed.
    async fn refresh_icon(&self, project_id: &str) -> bool {
        let projects = self.projects.lock().await;
        let Some(project) = projects.iter().find(|project| project.id == project_id) else { return false };
        let (path, icon, chosen) = (project.path.clone(), project.icon.clone(), project.icon_chosen);
        drop(projects);
        if chosen && icon.as_deref().and_then(icons::version).is_some() {
            return false;
        }
        let found = icons::find(&path, &self.environment).await;
        if !chosen && found == icon {
            return false;
        }
        let mut projects = self.projects.lock().await;
        let Some(project) = projects.iter_mut().find(|project| project.id == project_id) else { return false };
        if project.icon != icon || project.icon_chosen != chosen {
            return false;
        }
        project.icon = found;
        project.icon_chosen = false;
        if let Err(error) = self.store.save_project_icon(project) {
            tracing::error!(project_id, "couldn't save a project's icon: {error:#}");
        }
        true
    }

    fn announce_projects(&self, stored: &[StoredProject]) {
        let _ = self.list_updates.send(Message::Projects { projects: self.projects_of(stored) });
    }

    fn projects_of(&self, stored: &[StoredProject]) -> Vec<Project> {
        let git = self.lock_git();
        let status = |path: &str| git.get(path).map(|read| read.status.clone());
        let worktrees = self.lock_worktrees();
        let worktrees_of = |project_id: &str| {
            let own = worktrees.values().filter(|own| own.project_id == project_id);
            let mut list: Vec<Worktree> = own
                .map(|own| Worktree {
                    path: own.path.clone(),
                    // The branch it gets is known before the worktree is there.
                    branch: git::current_branch(&own.path).or_else(|| Some(own.branch.clone())),
                    git: status(&own.path),
                })
                .collect();
            list.sort_by(|a, b| a.path.cmp(&b.path));
            list
        };
        let project = |stored: &StoredProject| Project {
            id: stored.id.clone(),
            path: stored.path.clone(),
            name: match self.is_no_project(stored) {
                true => "No project".to_string(),
                false => file_name(&stored.path).to_string(),
            },
            branch: git::current_branch(&stored.path),
            git: status(&stored.path),
            icon: stored.icon.as_deref().and_then(icons::version),
            worktrees: worktrees_of(&stored.id),
            setup: stored.setup.clone(),
            no_project: self.is_no_project(stored),
            origin: git::origin_of(&stored.path),
            created_at: stored.created_at,
        };
        stored.iter().map(project).collect()
    }

    fn lock_worktrees(&self) -> std::sync::MutexGuard<'_, HashMap<String, ThreadWorktree>> {
        self.worktrees.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Makes the thread's worktree when it isn't there, which takes a while, keeps the folder as
    /// the turn finds it, and starts the agent.
    async fn prepare_and_start(
        self: Arc<Self>,
        thread_id: String,
        folder: String,
        prompt: String,
        makes_worktree: bool,
    ) {
        let made = match makes_worktree {
            true => self.make_worktree(&thread_id, &prompt).await,
            false => Ok(()),
        };
        if made.is_ok() {
            self.snapshot(&thread_id, &folder).await;
        }
        let Some(mut live) = self.live(&thread_id).await else { return };
        let live = &mut *live;
        live.preparing = None;
        let started = match made {
            Ok(()) => self.start_agent(live, prompt, false),
            Err(error) => self.end_without_agent(live, Some(error_text(&error))),
        };
        if let Err(error) = started {
            tracing::error!(thread_id, "couldn't start a turn: {error:#}");
        }
    }

    /// Keeps the thread's folder as it is now, when it is in a repository: the snapshot, and the
    /// one before it.
    async fn snapshot(&self, thread_id: &str, folder: &str) -> Option<(String, Option<String>)> {
        if !git::in_repository(folder) {
            return None;
        }
        let _one_at_a_time = self.snapshotting.lock().await;
        match git::snapshot(folder, &self.environment, thread_id).await {
            Ok(snapshot) => Some(snapshot),
            Err(error) => {
                tracing::debug!(thread_id, "couldn't keep a folder as it is: {error:#}");
                None
            }
        }
    }

    /// Has what the turn that just ended changed read, away from the thread's updates.
    fn read_changes(self: &Arc<Self>, live: &mut Live) {
        let Some(item_id) = live.ended.take() else { return };
        let thread = &live.stored.thread;
        tokio::spawn(self.clone().record_changes(thread.id.clone(), thread.cwd.clone(), item_id));
    }

    /// Adds what a turn changed in the thread's folder to the item that ended it.
    async fn record_changes(self: Arc<Self>, thread_id: String, folder: String, item_id: String) {
        let Some((snapshot, Some(before))) = self.snapshot(&thread_id, &folder).await else { return };
        if snapshot == before {
            return;
        }
        let files = match git::changed_between(&folder, &self.environment, &before, &snapshot).await {
            Ok(files) => files,
            Err(error) => return tracing::debug!(thread_id, "couldn't read a turn's changes: {error:#}"),
        };
        let Some(mut live) = self.live(&thread_id).await else { return };
        if let Err(error) = live.set_changes(&self.store, &item_id, TurnChanges { snapshot, files }) {
            tracing::error!(thread_id, "couldn't save a turn's changes: {error:#}");
        }
    }

    /// Makes the worktree on its branch, has the writer name a branch that is new, and runs the
    /// project's setup script there.
    async fn make_worktree(self: &Arc<Self>, thread_id: &str, message: &str) -> anyhow::Result<()> {
        let (project_id, path, worktree) = {
            let live = self.live_or(thread_id, "That thread no longer exists.").await?;
            let stored = &live.stored;
            let worktree = stored.worktree.clone().context("That thread has no worktree.")?;
            (stored.thread.project_id.clone(), stored.thread.cwd.clone(), worktree)
        };
        let (repository, setup) = {
            let projects = self.projects.lock().await;
            let project = projects.iter().find(|project| project.id == project_id);
            let project = project.context("That project is no longer on your server.")?;
            (project.path.clone(), project.setup.clone())
        };
        if let Some(parent) = Path::new(&path).parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("{} can't be made.", parent.display()))?;
        }
        let start = git::start(&repository, &self.environment, &worktree.base, true).await;
        if let Some(problem) = &start.problem {
            self.show_failed_fetch(thread_id, &repository, &worktree.base, problem).await;
        }
        git::add_worktree(&repository, &self.environment, &path, &worktree.branch, &worktree.base, &start).await?;
        if !Path::new(&path).is_dir() {
            bail!("Git didn't make the worktree at {path}.");
        }
        self.announce_projects(&self.projects.lock().await);
        let (hub, folder) = (self.clone(), path.clone());
        tokio::spawn(async move { hub.read_git(&folder, false).await });
        if worktree.branch == temporary_branch(&path) {
            tokio::spawn(self.clone().name_branch(thread_id.to_string(), message.to_string()));
        }
        if let Some(script) = setup {
            self.run_setup(thread_id, &repository, &path, &script).await;
        }
        Ok(())
    }

    /// Runs the project's setup script in the new worktree, as a tool call of the turn. The agent
    /// starts after it, also when it failed.
    async fn run_setup(&self, thread_id: &str, repository: &str, worktree: &str, script: &str) {
        let mut call = new_tool_call(new_id(), "Bash".to_string());
        call.input = serde_json::json!({ "command": script }).to_string();
        self.show_setup(thread_id, &call).await;
        let mut command = Command::new("sh");
        command
            .args(["-c", script])
            .current_dir(worktree)
            .env_clear()
            .envs(&self.environment.variables)
            .env("MOTILE_PROJECT", repository)
            .env("MOTILE_WORKTREE", worktree)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let (succeeded, said) = match tokio::time::timeout(SETUP_TIMEOUT, command.output()).await {
            Ok(Ok(output)) => {
                let said = [output.stdout, output.stderr].concat();
                (output.status.success(), String::from_utf8_lossy(&said).into_owned())
            }
            Ok(Err(error)) => (false, format!("The setup script couldn't be started: {error}")),
            Err(_) => (false, "The setup script took too long and was stopped.".to_string()),
        };
        call.output = Some(tail(said.trim(), SETUP_OUTPUT_CHARS).to_string());
        call.status = if succeeded { ToolStatus::Succeeded } else { ToolStatus::Failed };
        self.show_setup(thread_id, &call).await;
    }

    /// Says in the thread that the remote couldn't be asked for the base, so the worktree starts
    /// from what your server fetched last.
    async fn show_failed_fetch(&self, thread_id: &str, repository: &str, base: &str, problem: &str) {
        let remote = git::remote(repository, &self.environment).await.unwrap_or_default();
        let mut call = new_tool_call(new_id(), "Bash".to_string());
        call.input = serde_json::json!({ "command": format!("git fetch {remote} {base}") }).to_string();
        call.output = Some(problem.to_string());
        call.status = ToolStatus::Failed;
        self.show_setup(thread_id, &call).await;
    }

    async fn show_setup(&self, thread_id: &str, call: &ToolCall) {
        let Some(mut live) = self.live(thread_id).await else { return };
        if let Err(error) = live.upsert(&self.store, call.id.clone(), ItemKind::Tool { call: call.clone() }) {
            tracing::error!(thread_id, "couldn't save a thread update: {error:#}");
        }
    }

    /// Renames the branch made for the thread's worktree to what the writer calls the work.
    async fn name_branch(self: Arc<Self>, thread_id: String, message: String) {
        let Some(account) = self.account_of_thread(&thread_id).await else { return };
        let instructions = self.branch_instructions().text;
        let writer = self.writer(&account);
        let name = drafts::branch_for(&writer, &instructions, &message).await;
        self.keep_written_in(&thread_id, &writer, Purpose::Branch).await;
        let name = match name {
            Ok(name) => name,
            Err(error) => return tracing::warn!(thread_id, "couldn't name a branch: {error:#}"),
        };
        let Some(live) = self.live(&thread_id).await else { return };
        let Some(worktree) = live.stored.worktree.clone() else { return };
        let (path, project_id) = (live.stored.thread.cwd.clone(), live.stored.thread.project_id.clone());
        drop(live);
        // The agent may have checked out another branch by now.
        if git::current_branch(&path).as_deref() != Some(worktree.branch.as_str()) {
            return;
        }
        let Ok(repository) = self.project_path(&project_id).await else { return };
        let name = git::free_branch_name(&repository, &self.environment, &name).await;
        if let Err(error) = git::rename_branch(&repository, &self.environment, &worktree.branch, &name).await {
            return tracing::warn!(thread_id, "couldn't rename a branch: {error:#}");
        }
        if let Some(own) = self.lock_worktrees().get_mut(&thread_id) {
            own.branch = name.clone();
        }
        let Some(mut live) = self.live(&thread_id).await else { return };
        live.stored.worktree = Some(StoredWorktree { branch: name, ..worktree });
        if let Err(error) = self.store.save_thread(&live.stored) {
            tracing::error!(thread_id, "couldn't save a thread: {error:#}");
        }
        drop(live);
        self.read_git(&path, false).await;
        self.announce_projects(&self.projects.lock().await);
    }

    /// Takes the thread's worktree away with everything in it. Its branch stays.
    async fn discard_worktree(&self, thread: &Thread) {
        // Moved first, so nothing starts in a folder that is going.
        let going = format!("{}.removed", thread.cwd);
        if std::fs::rename(&thread.cwd, &going).is_err() {
            return;
        }
        let repository = self.project_path(&thread.project_id).await.ok();
        let environment = self.environment.clone();
        tokio::spawn(async move {
            let _ = tokio::fs::remove_dir_all(&going).await;
            if let Some(repository) = repository {
                git::prune_worktrees(&repository, &environment).await;
            }
        });
    }

    /// Ends a turn that has no agent's process: with what went wrong, or because the user
    /// stopped it. The messages that waited stay until they are sent.
    fn end_without_agent(&self, live: &mut Live, error: Option<String>) -> anyhow::Result<()> {
        match error {
            Some(message) => live.append(&self.store, ItemKind::Error { message })?,
            None => live.settle(&self.store, None, None, "", true)?,
        }
        // No agent ran, so nothing changed.
        live.ended = None;
        let thread = &mut live.stored.thread;
        thread.running = false;
        thread.updated_at = now();
        thread.turn_ended_at = Some(now());
        live.activity = Activity::default();
        live.server_asks.clear();
        live.open.clear();
        for queued in &mut live.queued {
            queued.held = true;
        }
        live.save_queued(&self.store)?;
        self.store.save_thread(&live.stored)?;
        live.send_activity();
        self.announce(&live.stored.thread);
        Ok(())
    }

    /// Ends the turn the usage limit cut off as stopped, so that the thread no longer goes on by
    /// itself once the limit resets. What was queued for it stays, held.
    fn stop_waiting_for_limit(&self, live: &mut Live) -> anyhow::Result<()> {
        live.stored.thread.interruption = None;
        live.settle(&self.store, None, None, "", true)?;
        live.ended = None;
        for queued in &mut live.queued {
            queued.held = true;
        }
        live.save_queued(&self.store)?;
        live.stored.thread.updated_at = now();
        self.store.save_thread(&live.stored)?;
        live.send_activity();
        self.announce(&live.stored.thread);
        Ok(())
    }

    fn announce(&self, thread: &Thread) {
        let _ = self.list_updates.send(Message::ThreadUpsert { thread: thread.clone() });
        if thread.parent.is_some() {
            let _ = self.children_changed.send(thread.id.clone());
        }
    }

    fn start_turn(self: &Arc<Self>, live: &mut Live, prompt: String) -> anyhow::Result<()> {
        live.tally.get_or_insert_with(|| Tally::new(TurnStarter::Server, None, now()));
        if !Path::new(&live.stored.thread.cwd).is_dir() {
            live.let_go();
        }
        let thread = &live.stored.thread;
        let makes_worktree = live.stored.worktree.is_some() && !Path::new(&thread.cwd).is_dir();
        if !makes_worktree && !git::in_repository(&thread.cwd) {
            return self.start_agent(live, prompt, false);
        }
        let preparing = self.clone().prepare_and_start(thread.id.clone(), thread.cwd.clone(), prompt, makes_worktree);
        self.announce_working(live)?;
        live.preparing = Some(Preparing { task: tokio::spawn(preparing).abort_handle(), makes_worktree });
        Ok(())
    }

    /// Starts the agent's process for a turn, in the session of the thread's continuation, told
    /// what the thread said that the session didn't see. `retried` when the session it was to
    /// resume wasn't there.
    fn start_agent(self: &Arc<Self>, live: &mut Live, prompt: String, retried: bool) -> anyhow::Result<()> {
        live.flush(&self.store)?;
        if live.run.as_ref().is_some_and(|run| run.resting.is_some()) {
            let prompt_id = new_id();
            if live.write_prompt(&prompt, &prompt_id) {
                let message_seq = live.last_message_seq.unwrap_or(live.stored.next_seq);
                live.turn = Some(StartedTurn { prompt_id, prompt, message_seq, retried, accepted: false });
                return self.resume(live);
            }
            live.let_go();
        }
        let mcp = self.mcp_access(live);
        let token = self.thread_token(live);
        let thread = &live.stored.thread;
        let continuation = self.continuation(thread);
        let sessions = self.store.sessions(&thread.id)?;
        let others_ran = sessions.iter().any(|(kept, session)| *kept != continuation && session.last_turn_at.is_some());
        let found = sessions.into_iter().find(|(kept, _)| *kept == continuation);
        let mut session = found.map(|(_, session)| session).unwrap_or_default();
        // What it was last given may not have reached it; giving it again could tell it twice.
        if session.pending_through.is_some() {
            session.session_id = None;
        }
        let message_seq = live.last_message_seq.unwrap_or(live.stored.next_seq);
        let range = match (&session.session_id, others_ran) {
            (Some(_), false) => None,
            (Some(_), true) => Some(Range::After(session.seen_through)),
            (None, _) => Some(Range::Full),
        };
        let handoff = match range {
            Some(range) => {
                let items = self.store.items_since(&thread.id, 0)?;
                let budget = self.handoff_budget(thread, &session, &items, message_seq, &prompt);
                let mut built = handoff::build(&thread.id, &items, range, message_seq, budget);
                // A session too full to be told what it missed is left for a new one, told everything.
                if built.is_err() && session.session_id.take().is_some() {
                    let budget = self.handoff_budget(thread, &session, &items, message_seq, &prompt);
                    built = handoff::build(&thread.id, &items, Range::Full, message_seq, budget);
                }
                match built {
                    Ok(handoff) => handoff,
                    Err(handoff::NoRoom) => return self.end_without_agent(live, Some(NO_ROOM_FOR_HANDOFF.to_string())),
                }
            }
            None => None,
        };
        live.stored.session_id = session.session_id.clone();
        let thread = &live.stored.thread;
        let agent = thread.agent;
        let account = self.account_of(thread);
        let turn = Turn {
            agent,
            model: thread.model.as_deref(),
            effort: thread.effort.as_deref(),
            access: thread.access,
            session_id: live.stored.session_id.as_deref(),
            handoff: handoff.as_ref(),
            mcp: mcp.as_ref(),
            gateway_token: account.gateway.then_some(token.as_str()),
        };
        let child = match self.spawn(&turn, &thread.cwd, &account, &token) {
            Ok(child) => child,
            Err(error) => return self.end_without_agent(live, Some(error.to_string())),
        };
        if let Some(handoff) = &handoff {
            session.pending_through = Some(handoff.through);
            self.store.save_session(&thread.id, &continuation, &session)?;
        }

        let interrupted = Arc::new(AtomicBool::new(false));
        let (input, lines) = mpsc::unbounded_channel();
        let prompt_id = new_id();
        // Codex is given what the thread said as messages of its own once its thread is there.
        let opening = match (agent, &handoff) {
            (Agent::Claude, Some(handoff)) => handoff.inline(&prompt),
            _ => prompt.clone(),
        };
        let _ = input.send(agents::opening(agent, &opening, &live.skills, &prompt_id));
        let parser = Parser::new(&turn, &thread.cwd, &prompt, &prompt_id);
        live.turn = Some(StartedTurn { prompt_id, prompt, message_seq, retried, accepted: false });
        live.run = Some(Run {
            process_id: child.id().unwrap_or_default(),
            started: Instant::now(),
            interrupted: interrupted.clone(),
            received_result: false,
            resting: None,
            input: Some(input),
            background: Background::default(),
            turn_id: None,
        });
        self.announce_working(live)?;

        tokio::spawn(self.clone().drive(live.stored.thread.id.clone(), agent, child, lines, parser, interrupted));
        Ok(())
    }

    /// How much a handoff may take of the context of the model the turn runs.
    fn handoff_budget(
        &self,
        thread: &Thread,
        session: &Session,
        items: &[Item],
        message_seq: u64,
        prompt: &str,
    ) -> usize {
        let message = items.iter().find(|item| item.seq == message_seq).map(|item| &item.kind);
        let cost = match message {
            Some(ItemKind::User { text, attachments, context, programs, .. }) => {
                handoff::message_cost(&ui::told(text, context.as_ref(), programs), attachments)
            }
            _ => prompt.len(),
        };
        let model = thread.model.as_deref().or(session.model.as_deref());
        let prices = self.prices.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let window = session.context_window.or_else(|| prices.context_window(model?));
        let used = session.session_id.as_ref().and(session.context_used).unwrap_or(0);
        handoff::budget(window, used, cost)
    }

    fn announce_working(&self, live: &mut Live) -> anyhow::Result<()> {
        let thread = &mut live.stored.thread;
        thread.running = true;
        thread.monitoring = false;
        thread.needs_approval = false;
        thread.asking = None;
        thread.interruption = None;
        thread.updated_at = now();
        // New activity brings an archived thread back, to the top of the list.
        if thread.archived_at.take().is_some() {
            thread.position = now();
        }
        let agents = live.stored.thread.agents;
        live.activity = Activity { running: true, started_at: Some(now()), agents, ..Activity::default() };
        self.store.save_thread(&live.stored)?;
        live.send_activity();
        self.announce(&live.stored.thread);
        Ok(())
    }

    /// The idle process is at work again.
    fn resume(&self, live: &mut Live) -> anyhow::Result<()> {
        let Some(run) = live.run.as_mut().filter(|run| run.received_result) else { return Ok(()) };
        run.received_result = false;
        run.resting = None;
        run.started = Instant::now();
        self.announce_working(live)
    }

    /// What the process does once its turn has ended: take the next message that waits, exit,
    /// keep working in the background, monitor, or rest until the next message.
    fn rest(self: &Arc<Self>, live: &mut Live) -> anyhow::Result<()> {
        let Some(run) = live.run.as_ref().filter(|run| run.received_result) else { return Ok(()) };
        // What the agent asks once its turn has ended, it stays to hear the answer to.
        if !live.activity.approvals.is_empty() {
            return Ok(());
        }
        let interrupted = run.interrupted.load(Ordering::Relaxed);
        // A message the agent was given too late for this turn starts its next one.
        let given = !live.given.is_empty();
        if !interrupted && (given || live.hand_over(&self.store)?) {
            return self.resume(live);
        }
        let Some(run) = &mut live.run else { return Ok(()) };
        let rests = run.background.is_empty() && !interrupted && live.stored.thread.agent == Agent::Claude;
        if !rests && (run.background.is_empty() || interrupted) {
            run.input = None;
            return Ok(());
        }
        let monitoring = run.background.watches > 0;
        let thread = &mut live.stored.thread;
        if !thread.running && thread.monitoring == monitoring && run.resting.is_some() == rests {
            return Ok(());
        }
        let resting = rests.then(Instant::now);
        run.resting = resting;
        if std::mem::replace(&mut thread.running, false) {
            thread.turn_ended_at = Some(now());
        }
        thread.monitoring = monitoring;
        thread.updated_at = now();
        live.activity = Activity { monitoring, agents: thread.agents, ..Activity::default() };
        live.flush(&self.store)?;
        self.store.save_thread(&live.stored)?;
        live.send_activity();
        self.announce(&live.stored.thread);
        if let Some(since) = resting {
            self.let_go_once_rested(live.stored.thread.id.clone(), since);
        }
        Ok(())
    }

    fn let_go_once_rested(self: &Arc<Self>, thread_id: String, since: Instant) {
        let hub = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(RESTING_PROCESS_STAYS).await;
            let Some(mut live) = hub.live(&thread_id).await else { return };
            if live.run.as_ref().is_some_and(|run| run.resting == Some(since)) {
                live.let_go();
            }
        });
    }

    /// Starts the agent of a turn of the thread whose token is `token`.
    fn spawn(&self, turn: &Turn, cwd: &str, account: &AgentAccount, token: &str) -> anyhow::Result<Child> {
        let name = executable_name(turn.agent);
        let executable = self
            .environment
            .executable(turn.agent)
            .with_context(|| format!("{name} isn't installed on this server."))?;
        if !Path::new(cwd).is_dir() {
            bail!("The folder {cwd} no longer exists.");
        }
        agent_accounts::prepare(account, &self.environment)?;
        let mut command = Command::new(executable);
        command
            .args(turn.arguments())
            .current_dir(cwd)
            .env_clear()
            .envs(&self.account_environment(account).variables)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Its own group, so stopping the turn also stops whatever the agent started.
            .process_group(0)
            .kill_on_drop(true);
        if unsafe { libc::getuid() } == 0 {
            command.env(crate::service::SANDBOX_VARIABLE, "1");
        }
        if let Some(mcp) = turn.mcp {
            command.env(mcp::AUTHORIZATION_VARIABLE, format!("Bearer {}", mcp.token));
            command.env("MCP_TOOL_TIMEOUT", (mcp::TOOL_TIMEOUT_SECS * 1000).to_string());
        }
        if account.gateway {
            command.env(gateway::token_variable(account.agent), token);
            if let (Some(way), Some(thread)) = (self.gateway.get(), self.mcp_tokens.thread_of(token)) {
                way.tunnel.pin(&thread, account.pin.as_deref());
            }
        }
        command.spawn().with_context(|| format!("{name} couldn't be started."))
    }

    async fn drive(
        self: Arc<Self>,
        thread_id: String,
        agent: Agent,
        mut child: Child,
        mut lines: mpsc::UnboundedReceiver<String>,
        mut parser: Parser,
        interrupted: Arc<AtomicBool>,
    ) {
        let process_id = child.id().unwrap_or_default();
        let started = Instant::now();
        let (mcp_sender, mcp_ready) = watch::channel(false);
        if let Some(mut stdin) = child.stdin.take() {
            tokio::spawn(async move {
                if agent == Agent::Claude {
                    wait_for_mcp(&mut stdin, mcp_ready).await;
                }
                while let Some(line) = lines.recv().await {
                    if stdin.write_all(line.as_bytes()).await.is_err() {
                        return;
                    }
                }
            });
        }
        let stderr = child.stderr.take();
        let stderr = tokio::spawn(async move {
            let mut text = String::new();
            if let Some(mut stderr) = stderr {
                let _ = stderr.read_to_string(&mut text).await;
            }
            text
        });

        if let Some(stdout) = child.stdout.take() {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(ready) = claude::mcp_ready(&line, started.elapsed()) {
                    mcp_sender.send_replace(ready);
                    continue;
                }
                let events = parser.parse(&line);
                if !events.is_empty() {
                    self.apply(&thread_id, events).await;
                }
            }
        }
        let exit_code = child.wait().await.ok().and_then(|status| status.code());
        let stderr = stderr.await.unwrap_or_default();
        self.finish_turn(&thread_id, process_id, exit_code, &stderr, interrupted.load(Ordering::Relaxed)).await;
    }

    async fn apply(self: &Arc<Self>, thread_id: &str, events: Vec<AgentEvent>) {
        // The restart finds the thread as the agent left it.
        if self.closing.load(Ordering::SeqCst) {
            return;
        }
        let Some(mut live) = self.live(thread_id).await else { return };
        let live = &mut *live;
        let was_running = live.stored.thread.running;
        // What works in the background after the turn can end without waking the agent, when the
        // usage limit stopped it all.
        let settles =
            events.iter().any(|event| matches!(event, AgentEvent::Completed { .. } | AgentEvent::Background(_)));
        for event in events {
            if let Err(error) = self.apply_event(live, event) {
                tracing::error!(thread_id, "couldn't save a thread update: {error:#}");
            }
        }
        // Decided once everything the agent said with the turn's end is known.
        if settles && let Err(error) = self.rest(live) {
            tracing::error!(thread_id, "couldn't save a thread update: {error:#}");
        }
        self.read_changes(live);
        if live.stored.thread.parent.is_some() {
            let _ = self.children_changed.send(thread_id.to_string());
        }
        if was_running && !live.stored.thread.running {
            self.after_turn(live).await;
        }
        if live.last_flush.elapsed() < FLUSH_EVERY {
            return;
        }
        if let Err(error) = live.flush(&self.store) {
            tracing::error!(thread_id, "couldn't save streamed text: {error:#}");
        }
    }

    fn apply_event(&self, live: &mut Live, event: AgentEvent) -> anyhow::Result<()> {
        let store = &self.store;
        let more_text = matches!(
            event,
            AgentEvent::Session { .. }
                | AgentEvent::Background(_)
                | AgentEvent::Sub { .. }
                | AgentEvent::Task { .. }
                | AgentEvent::TextStarted { .. }
                | AgentEvent::TextDelta { .. }
                | AgentEvent::Text { .. }
                | AgentEvent::Usage { .. }
        );
        if !more_text {
            live.release_held(store)?;
        }
        match event {
            AgentEvent::Session { id, model } => {
                let thread = &live.stored.thread;
                self.change_session(thread, |session| {
                    session.session_id = Some(id.clone());
                    session.model = model.clone().or(session.model.take());
                })?;
                live.stored.session_id = Some(id);
                if let Some(model) = model {
                    self.name_handoff(live, &model)?;
                    live.session_model = Some(model);
                }
            }
            AgentEvent::Accepted => self.accept(live)?,
            AgentEvent::Fast(fast) => {
                if let Some(tally) = &mut live.tally {
                    tally.fast(fast);
                }
            }
            AgentEvent::Reasoning(tokens) => {
                if let Some(tally) = &mut live.tally {
                    tally.reasoned(tokens);
                }
            }
            AgentEvent::SessionMissing => self.session_missing(live)?,
            AgentEvent::TextStarted { .. } => live.set_thinking(false),
            AgentEvent::TextDelta { id, text } => live.hold_text(store, id, text)?,
            AgentEvent::Text { id, text } => {
                live.held.remove(&id);
                if text.is_empty() {
                    return Ok(());
                }
                live.set_thinking(false);
                live.upsert(store, id, ItemKind::Assistant { text })?;
            }
            AgentEvent::Thinking { active } => live.set_thinking(active),
            AgentEvent::ThinkingText { id, text } => live.upsert(store, id, ItemKind::Thinking { text })?,
            AgentEvent::ToolStarted { id, name } => {
                live.set_thinking(false);
                self.note_tool(live, &id, &name);
                if !live.open.contains_key(&id) {
                    live.upsert(store, id.clone(), ItemKind::Tool { call: new_tool_call(id, name) })?;
                }
            }
            AgentEvent::ToolInput { id, name, input } => {
                self.note_tool(live, &id, &name);
                live.tool_input(store, id, name, input)?
            }
            AgentEvent::ToolResult { id, output, is_error } => live.tool_result(store, id, output, is_error)?,
            AgentEvent::ToolImages { id, images } => live.tool_images(store, id, images)?,
            AgentEvent::Tool { mut call } => {
                live.set_thinking(false);
                self.note_tool(live, &call.id, &call.name);
                if let Some(agent) = call.agent.as_mut().filter(|agent| agent.runs_on.is_none()) {
                    agent.runs_on = Some(Box::new(self.runs_on(live, &call.input)));
                }
                live.upsert(store, call.id.clone(), ItemKind::Tool { call })?;
            }
            AgentEvent::Sub { parent, event } => {
                live.parent = Some(parent);
                let added = live.add_from_subagent(store, *event);
                live.parent = None;
                added?;
            }
            AgentEvent::Task { tool_id, mut agent } => {
                if let Some(tally) = &mut live.tally {
                    tally.subagent(&tool_id);
                }
                if agent.runs_on.is_none() {
                    let input = live.tool_call(&tool_id).map(|call| call.input).unwrap_or_default();
                    agent.runs_on = Some(Box::new(self.runs_on(live, &input)));
                }
                live.update_subagent(store, &tool_id, agent)?;
                self.count_agents(live);
            }
            AgentEvent::Compacting { active } => {
                if live.activity.compacting != active {
                    live.activity.compacting = active;
                    live.send_activity();
                }
            }
            AgentEvent::Usage { spent, total, context_window, context_used } => {
                let thread = &live.stored.thread;
                let session_id = live.stored.session_id.as_deref().unwrap_or(&thread.id);
                let (cost, output) = store.save_usage(now(), thread, session_id, &spent, total)?;
                if let Some(tally) = &mut live.tally {
                    tally.spent(output);
                }
                live.cost_usd = cost.map(|cost| cost + live.cost_usd.unwrap_or_default()).or(live.cost_usd);
                if context_window.is_some() || context_used.is_some() {
                    self.change_session(thread, |session| {
                        session.context_window = context_window.or(session.context_window);
                        session.context_used = context_used.or(session.context_used);
                    })?;
                }
            }
            AgentEvent::Completed { mut summary, result_text, preempted } => {
                live.set_thinking(false);
                // A turn stopped for the message it was given goes on with it.
                if preempted && live.steering() {
                    return Ok(());
                }
                live.fail_dropped_tools(store)?;
                if let Some(run) = &mut live.run {
                    run.received_result = true;
                    run.turn_id = None;
                    summary.duration_ms = Some(run.started.elapsed().as_millis() as u64);
                    // Claude Code reports a turn the user stopped as one that failed.
                    if run.interrupted.load(Ordering::Relaxed) {
                        summary.stopped = true;
                        summary.is_error = false;
                    }
                }
                if summary.is_error {
                    wake(&mut live.stored.thread, true);
                    let message = result_text.filter(|text| !text.is_empty());
                    let message = message.unwrap_or_else(|| "The agent reported an error.".to_string());
                    live.append(store, ItemKind::Error { message })?;
                }
                live.activity.approvals.clear();
                live.server_asks.clear();
                live.note_waiting();
                live.stored.thread.needs_approval = false;
                live.stored.thread.asking = None;
                summary.cost_usd = live.cost_usd.take();
                live.end_turn(store, summary)?;
                self.note_seen(live)?;
            }
            AgentEvent::Approval(approval) => {
                live.set_thinking(false);
                live.activity.approvals.push(approval);
                if let Some(tally) = &mut live.tally {
                    tally.asked();
                }
                self.announce_approvals(live)?;
            }
            AgentEvent::ApprovalWithdrawn { id } => {
                live.activity.approvals.retain(|approval| approval.id != id);
                self.announce_approvals(live)?;
            }
            AgentEvent::Background(background) => {
                if let Some(run) = &mut live.run {
                    run.background = background;
                }
            }
            AgentEvent::Woke => self.resume(live)?,
            AgentEvent::TooLate { id } => {
                let Some(message) = live.given.iter().find(|given| given.id == id) else { return Ok(()) };
                let prompt = prompt(message);
                live.write_prompt(&prompt, &id);
            }
            AgentEvent::Taken { id } => {
                live.given.retain(|given| given.id != id);
                if live.turn.as_ref().is_some_and(|turn| turn.prompt_id == id) {
                    self.accept(live)?;
                }
            }
            AgentEvent::Turn { id } => {
                if let Some(run) = &mut live.run {
                    run.turn_id = Some(id);
                }
            }
            AgentEvent::Write(lines) => {
                live.write(lines);
            }
            AgentEvent::Limited { resets_at } => {
                let noted = self.gateway.get().and_then(|way| way.tunnel.take_limit_reset(&live.stored.thread.id));
                let resets_at = resets_at.or(noted);
                let continues = self.continue_settings().after_limits && resets_at.is_some_and(|at| at > now());
                live.stored.thread.interruption = Some(Interruption::Limit { resets_at, continues });
                wake(&mut live.stored.thread, true);
                store.save_thread(&live.stored)?;
                self.announce(&live.stored.thread);
            }
            // The process can't go on, and would stay if it weren't let go.
            AgentEvent::Failed { message } => {
                if let Some(run) = &mut live.run {
                    run.received_result = true;
                    run.input = None;
                }
                live.append(store, ItemKind::Error { message })?;
            }
        }
        Ok(())
    }

    /// Counts a tool call of the turn's agent, and keeps its use of a motile tool as a feature's.
    fn note_tool(&self, live: &mut Live, id: &str, name: &str) {
        let Some(tally) = &mut live.tally else { return };
        if !tally.tool(id, name) {
            return;
        }
        let Some(feature) = motile_protocol::activity::agent_tool(name) else { return };
        let thread = &live.stored.thread;
        let action = ActionRecord {
            at: now(),
            actor: ActorKind::Agent,
            device: None,
            feature: feature.to_string(),
            variant: String::new(),
            thread_id: thread.id.clone(),
            project_id: thread.project_id.clone(),
            path: None,
            rtt_ms: None,
        };
        if let Err(error) = self.store.save_action(&action) {
            tracing::warn!(thread_id = thread.id, "couldn't keep an agent's use of a tool: {error:#}");
        }
    }

    /// The agent has taken its turn, and with it what it was told of the thread: its session has
    /// seen everything up to the turn's message.
    fn accept(&self, live: &mut Live) -> anyhow::Result<()> {
        let Some(turn) = live.turn.as_mut().filter(|turn| !turn.accepted) else { return Ok(()) };
        turn.accepted = true;
        let message_seq = turn.message_seq;
        let account = live.stored.thread.agent_account.clone();
        self.change_session(&live.stored.thread, |session| {
            session.seen_through = session.seen_through.max(session.pending_through.unwrap_or(0)).max(message_seq);
            session.pending_through = None;
            session.last_turn_at = Some(now());
            session.account = Some(account);
        })?;
        Ok(())
    }

    /// Everything in the transcript happened in the session of the turn that ended.
    fn note_seen(&self, live: &Live) -> anyhow::Result<()> {
        if !live.turn.as_ref().is_some_and(|turn| turn.accepted) {
            return Ok(());
        }
        let last = live.stored.next_seq.saturating_sub(1);
        self.change_session(&live.stored.thread, |session| session.seen_through = session.seen_through.max(last))?;
        Ok(())
    }

    /// The session the agent was told to resume isn't there. The turn starts again once, in a new
    /// session that is told what was said.
    fn session_missing(&self, live: &mut Live) -> anyhow::Result<()> {
        self.change_session(&live.stored.thread, |session| {
            session.session_id = None;
            session.pending_through = None;
        })?;
        live.stored.session_id = None;
        let again = live.turn.as_ref().filter(|turn| !turn.retried && !turn.accepted);
        live.retry = again.map(|turn| turn.prompt.clone());
        if let Some(run) = &mut live.run {
            run.received_result = true;
            run.input = None;
        }
        if live.retry.is_none() {
            let message = "The agent couldn't find its session. Send the message again to start a new one.";
            live.append(&self.store, ItemKind::Error { message: message.to_string() })?;
        }
        Ok(())
    }

    /// Tells the clients how many of the agents the thread's agent started in its own session
    /// still work. Those started through the server work in threads of their own.
    fn count_agents(&self, live: &mut Live) {
        let working = |item: &&Item| {
            matches!(&item.kind, ItemKind::Tool { call } if !children::starts_agent(call)
                && call.agent.as_ref().is_some_and(|agent| agent.status == ToolStatus::Running))
        };
        let agents = live.open.values().filter(working).count() as u32;
        if live.stored.thread.agents == agents {
            return;
        }
        live.stored.thread.agents = agents;
        live.activity.agents = agents;
        live.send_activity();
        self.announce(&live.stored.thread);
    }

    async fn finish_turn(
        self: &Arc<Self>,
        thread_id: &str,
        process_id: u32,
        exit_code: Option<i32>,
        stderr: &str,
        interrupted: bool,
    ) {
        if self.closing.load(Ordering::SeqCst) {
            return;
        }
        let Some(mut live) = self.live(thread_id).await else { return };
        let live = &mut *live;
        // A process that was let go, or ended as it rested, has told of its turn already.
        let Some(run) = live.run.as_ref().filter(|run| run.process_id == process_id) else { return };
        if run.resting.is_some() {
            live.run = None;
            return;
        }
        let answered = live.run.as_ref().is_some_and(|run| run.received_result);
        if !answered
            && stderr.contains(claude::NO_CONVERSATION)
            && let Err(error) = self.session_missing(live)
        {
            tracing::error!(thread_id, "couldn't forget a session: {error:#}");
        }
        let run = live.run.take();
        if let Some(prompt) = live.retry.take() {
            let started = live.release_held(&self.store).and_then(|_| self.start_agent(live, prompt, true));
            if let Err(error) = started {
                tracing::error!(thread_id, "couldn't start a turn again: {error:#}");
            }
            return;
        }
        let settled = live.settle(&self.store, run.as_ref(), exit_code, stderr, interrupted);
        if let Err(error) = settled.and_then(|_| self.note_seen(live)) {
            tracing::error!(thread_id, "couldn't save the end of a turn: {error:#}");
        }
        live.turn = None;
        self.read_changes(live);
        let thread = &mut live.stored.thread;
        // A thread that was monitoring has told of its turn's end already.
        if !thread.monitoring {
            thread.turn_ended_at = Some(now());
        }
        thread.running = false;
        thread.monitoring = false;
        thread.needs_approval = false;
        thread.asking = None;
        thread.agents = 0;
        thread.updated_at = now();
        if interrupted {
            thread.interruption = None;
        }
        live.activity = Activity::default();
        live.server_asks.clear();
        live.open.clear();

        // What the user stopped doesn't go on by itself, not even once its usage limit resets: the
        // messages that waited stay until they are sent.
        for queued in &mut live.queued {
            queued.held |= interrupted;
        }
        if let Err(error) = live.save_queued(&self.store) {
            tracing::error!(thread_id, "couldn't save the queued messages: {error:#}");
        }
        // What the agent was given and didn't take starts the next turn, already in the transcript.
        let given = std::mem::take(&mut live.given);
        if !interrupted && !given.is_empty() {
            live.tally = Some(Tally::new(starter_of(&given[0]), given[0].by.device_key(), now()));
            let prompts: Vec<String> = given.iter().map(prompt).collect();
            if let Err(error) = self.start_turn(live, prompts.join("\n\n")) {
                tracing::error!(thread_id, "couldn't start the next turn: {error:#}");
            }
            return;
        }
        // What waits for an agent at its usage limit goes once the thread continues.
        if let Some(index) = live.queued.iter().position(|queued| !queued.held).filter(|_| !live.limited()) {
            let queued = live.queued.remove(index);
            if let Err(error) = self.start_next_turn(live, queued) {
                tracing::error!(thread_id, "couldn't start the next turn: {error:#}");
            }
            return;
        }
        if let Err(error) = self.store.save_thread(&live.stored) {
            tracing::error!(thread_id, "couldn't save a thread: {error:#}");
        }
        live.send_activity();
        self.announce(&live.stored.thread);
        self.after_turn(live).await;
    }

    fn start_next_turn(self: &Arc<Self>, live: &mut Live, queued: Queued) -> anyhow::Result<()> {
        let prompt = prompt(&queued);
        live.save_queued(&self.store)?;
        self.note_handoff(live)?;
        live.append_queued(&self.store, queued)?;
        self.start_turn(live, prompt)
    }

    /// Notes in the transcript that the thread goes on in another continuation than its last turn
    /// ran in, with another agent or in another sessions folder, from the turn that starts now.
    fn note_handoff(&self, live: &mut Live) -> anyhow::Result<()> {
        let moved_by = std::mem::take(&mut live.moved_by);
        let thread = &live.stored.thread;
        let continuation = self.continuation(thread);
        let sessions = self.store.sessions(&thread.id)?;
        let ran = sessions.iter().filter_map(|(kept, session)| Some((kept, session, session.last_turn_at?)));
        let Some((last, session, _)) = ran.max_by(|(_, _, one), (_, _, other)| one.total_cmp(other)) else {
            return Ok(());
        };
        if *last == continuation {
            return Ok(());
        }
        let own = sessions.iter().find(|(kept, _)| *kept == continuation).and_then(|(_, own)| own.model.clone());
        let from = self.handoff_end(last.agent, session.model.clone(), session.account.as_deref());
        let to = self.handoff_end(thread.agent, thread.model.clone().or(own), Some(&thread.agent_account));
        let waits_for_model = to.model.is_none();
        let item = live.new_item(new_id(), ItemKind::Handoff { from, to, by: moved_by });
        live.handoff_item = waits_for_model.then(|| item.id.clone());
        live.save(&self.store, item)
    }

    fn handoff_end(&self, agent: Agent, model: Option<String>, account: Option<&str>) -> HandoffEnd {
        let name = model.as_deref().and_then(|model| self.model_name(agent, model));
        let accounts = self.agent_accounts().into_iter().filter(|kept| kept.agent == agent).collect::<Vec<_>>();
        let used = accounts.iter().find(|kept| Some(kept.id.as_str()) == account);
        let gateway = used.is_some_and(|used| used.gateway);
        let named = used.filter(|used| accounts.len() > 1 && (!used.gateway || used.pin.is_some()));
        let pinned = used.is_some_and(|used| used.pin.is_some());
        HandoffEnd { agent, model, name, account: named.map(|kept| kept.name.clone()), gateway, pinned }
    }

    /// The model's name as the picker lists it. An agent names a model it runs with its whole id,
    /// which can end in a date or the size of its context.
    fn model_name(&self, agent: Agent, model: &str) -> Option<String> {
        let models = self.models();
        let listed = |id: &str| models.iter().find(|listed| listed.agent == agent && listed.id == id);
        let base = model.split('[').next().unwrap_or(model);
        let undated = base.rsplit_once('-').filter(|(_, date)| date.len() == 8).map_or(base, |(undated, _)| undated);
        let found = [model, base, undated].into_iter().find_map(listed).map(|listed| listed.name.clone());
        found.or_else(|| (agent == Agent::Claude && model.starts_with("claude-")).then(|| models::claude_name(undated)))
    }

    /// What an agent the thread's agent started on its own runs on: the thread's account, and the
    /// model the tool call that started it names, or else the thread's.
    fn runs_on(&self, live: &Live, input: &str) -> RunsOn {
        let thread = &live.stored.thread;
        let account = self.account_of(thread);
        let input: serde_json::Value = serde_json::from_str(input).unwrap_or_default();
        let named = input["model"].as_str().filter(|model| !model.is_empty() && *model != "inherit");
        let listed = |alias: &str| {
            let alias = alias.to_ascii_lowercase();
            let models = self.models().into_iter().filter(|model| model.account == account.id);
            models
                .into_iter()
                .find(|model| model.id.contains(&alias) || model.name.to_ascii_lowercase().contains(&alias))
        };
        let model = match named {
            Some(alias) => listed(alias).map(|model| model.name).or_else(|| Some(alias.to_string())),
            None => thread.model.clone().or_else(|| live.session_model.clone()),
        };
        let model = model.map(|model| self.model_name(thread.agent, &model).unwrap_or(model));
        RunsOn { model, ..self.shown_account(&thread.id, &account) }
    }

    /// Gives the handoff that waited for it the model the new session runs.
    fn name_handoff(&self, live: &mut Live, model: &str) -> anyhow::Result<()> {
        let Some(item_id) = live.handoff_item.take() else { return Ok(()) };
        let Some(mut item) = self.store.item(&live.stored.thread.id, &item_id)? else { return Ok(()) };
        let ItemKind::Handoff { to, .. } = &mut item.kind else { return Ok(()) };
        let named = self.handoff_end(to.agent, Some(model.to_string()), None);
        *to = HandoffEnd { account: to.account.take(), gateway: to.gateway, pinned: to.pinned, ..named };
        item.rev = live.next_rev();
        live.save(&self.store, item)
    }

    async fn after_turn(self: &Arc<Self>, live: &mut Live) {
        if let Err(error) = self.send_back_programs(live) {
            tracing::error!(thread_id = live.stored.thread.id, "couldn't send programs back to be fixed: {error:#}");
        }
        if std::mem::take(&mut live.title_needs_refinement) && live.stored.title_source == TitleSource::Placeholder {
            tokio::spawn(self.clone().title_from_transcript(live.stored.thread.id.clone()));
        }
        // The turn may have switched branches, or made the project an icon.
        let projects = self.projects.lock().await;
        if !projects.iter().any(|project| project.id == live.stored.thread.project_id) {
            return;
        }
        self.announce_projects(&projects);
        let (hub, path) = (self.clone(), live.stored.thread.cwd.clone());
        tokio::spawn(async move { hub.read_git(&path, false).await });
        let (hub, project_id) = (self.clone(), live.stored.thread.project_id.clone());
        tokio::spawn(async move {
            if hub.refresh_icon(&project_id).await {
                hub.announce_projects(&hub.projects.lock().await);
            }
        });
    }

    /// Asks the agent to fix the programs of the turn that just ended that the clients can't draw
    /// whole, unless the usage limit ended it.
    fn send_back_programs(self: &Arc<Self>, live: &mut Live) -> anyhow::Result<()> {
        if live.limited() {
            return Ok(());
        }
        live.flush(&self.store)?;
        let thread = &live.stored.thread;
        let Some(repair) = ui::repair(&self.store.program_items(&thread.id)?) else { return Ok(()) };
        let notice = AgentNotice {
            thread_id: thread.id.clone(),
            title: "Program errors".to_string(),
            event: NoticeEvent::Failed,
            agent: thread.agent,
            summary: Some(repair.summary),
            repair: true,
        };
        self.tell(live, notice, repair.text)
    }
}

impl Live {
    fn new(stored: StoredThread, media: MediaStore) -> Self {
        let (updates, _) = broadcast::channel(UPDATES_BUFFER);
        Self {
            stored,
            media,
            open: HashMap::new(),
            unsaved: HashSet::new(),
            last_flush: Instant::now(),
            held: HashMap::new(),
            last_delivery: None,
            activity: Activity::default(),
            updates,
            parent: None,
            run: None,
            preparing: None,
            ended: None,
            stopped_by: Actor::Unknown,
            moved_by: Actor::Unknown,
            queued: Vec::new(),
            given: Vec::new(),
            title_needs_refinement: false,
            cost_usd: None,
            last_message_seq: None,
            turn: None,
            retry: None,
            mcp_token: None,
            told_ask: None,
            told_limit: false,
            tools: None,
            deleted: false,
            server_asks: HashMap::new(),
            tally: None,
            session_model: None,
            handoff_item: None,
            skills: Vec::new(),
        }
    }

    fn next_rev(&mut self) -> u64 {
        self.stored.thread.rev += 1;
        self.stored.thread.rev
    }

    /// How many tool calls of its own the transcript has, counted once.
    fn tool_count(&mut self, store: &Store) -> rusqlite::Result<u64> {
        if let Some(tools) = self.tools {
            return Ok(tools);
        }
        let tools = store.tool_count(&self.stored.thread.id)?;
        self.tools = Some(tools);
        Ok(tools)
    }

    /// Keeps the user's answers on the call that asked the questions, so that the transcript shows
    /// them whatever the agent says back. Codex has no item for its questions; they get one here.
    fn keep_answers(
        &mut self,
        store: &Store,
        approval: &Approval,
        allow: bool,
        answers: &HashMap<String, String>,
        by: Actor,
    ) -> anyhow::Result<()> {
        let mut input: Value = serde_json::from_str(&approval.input).unwrap_or_default();
        let waiting: Vec<&ToolCall> = self
            .open
            .values()
            .filter_map(|item| match &item.kind {
                ItemKind::Tool { call } if call.name == QUESTION_TOOL && call.status == ToolStatus::Running => {
                    Some(call)
                }
                _ => None,
            })
            .collect();
        let same = |call: &&&ToolCall| {
            serde_json::from_str::<Value>(&call.input).is_ok_and(|asked| asked["questions"] == input["questions"])
        };
        let asked = waiting.iter().find(same).or(waiting.first()).map(|call| (*call).clone());
        let status = if allow { ToolStatus::Succeeded } else { ToolStatus::Failed };
        let mut call = asked.unwrap_or_else(|| ToolCall {
            id: approval.id.clone(),
            name: QUESTION_TOOL.to_string(),
            input: String::new(),
            output: None,
            status,
            agent: None,
            decided_by: Actor::Unknown,
        });
        if allow {
            input["answers"] = serde_json::json!(answers);
        }
        call.input = input.to_string();
        call.decided_by = by;
        self.upsert(store, call.id.clone(), ItemKind::Tool { call })
    }

    /// Keeps who allowed or refused a tool call, or the plan, on the call the agent asked for: the
    /// latest of the turn by that tool with that input. A call the turn has no item of keeps nothing.
    fn keep_decision(&mut self, store: &Store, approval: &Approval, by: Actor) -> anyhow::Result<()> {
        let asked: Value = serde_json::from_str(&approval.input).unwrap_or_default();
        let same = |call: &ToolCall| {
            let input = serde_json::from_str::<Value>(&call.input).unwrap_or_default();
            call.name == approval.tool_name && (approval.tool_name == PLAN_TOOL || input == asked)
        };
        let mut calls: Vec<&Item> = self.open.values().collect();
        calls.sort_by_key(|item| std::cmp::Reverse(item.seq));
        let found = calls.into_iter().find_map(|item| match &item.kind {
            ItemKind::Tool { call } if same(call) && call.decided_by.is_unknown() => Some(call.clone()),
            _ => None,
        });
        let Some(mut call) = found else { return Ok(()) };
        call.decided_by = by;
        self.upsert(store, call.id.clone(), ItemKind::Tool { call })
    }

    fn tool_call(&self, id: &str) -> Option<ToolCall> {
        match &self.open.get(id)?.kind {
            ItemKind::Tool { call } => Some(call.clone()),
            _ => None,
        }
    }

    fn activity(&self) -> Activity {
        Activity { queued: self.queued.clone(), ..self.activity.clone() }
    }

    fn send_activity(&self) {
        let _ = self.updates.send(Message::Activity { activity: self.activity() });
    }

    fn save_queued(&self, store: &Store) -> rusqlite::Result<()> {
        store.save_queued(&self.stored.thread.id, &self.queued)
    }

    fn queued_index(&self, message_id: &str) -> anyhow::Result<usize> {
        let index = self.queued.iter().position(|queued| queued.id == message_id);
        index.context("That message is no longer waiting.")
    }

    /// The agent reached its usage limit and hasn't worked since.
    fn limited(&self) -> bool {
        matches!(self.stored.thread.interruption, Some(Interruption::Limit { .. }))
    }

    /// Ends what the agent was doing when the server stopped, as the transcript shows it: a turn
    /// it was in the middle of, or a watch. `true` when the restart cut something off.
    fn recover(&mut self, store: &Store) -> anyhow::Result<bool> {
        let thread = &self.stored.thread;
        let (running, monitoring) = (thread.running, thread.monitoring);
        if !running && !monitoring {
            return Ok(false);
        }
        if running {
            let items = store.items_since(&thread.id, 0)?;
            let working = items.iter().filter(|item| matches!(&item.kind, ItemKind::Tool { call } if works(call)));
            self.open = working.map(|item| (item.id.clone(), item.clone())).collect();
            self.cut_off_tools(store, true)?;
            self.open.clear();
            let message = RESTARTED.to_string();
            self.append(store, ItemKind::Error { message })?;
            let started =
                items.iter().rev().find(|item| matches!(item.kind, ItemKind::User { .. } | ItemKind::Notice { .. }));
            let started_at = started.map_or_else(now, |item| item.created_at);
            self.tally = Some(Tally::new(TurnStarter::Other, None, started_at));
            let summary = TurnSummary { is_error: true, ..TurnSummary::default() };
            self.end_turn_as(store, summary, Some(TurnOutcome::Restart))?;
            self.ended = None;
        }
        let thread = &mut self.stored.thread;
        thread.running = false;
        thread.monitoring = false;
        thread.needs_approval = false;
        thread.asking = None;
        thread.interruption = Some(Interruption::Restart);
        if running {
            thread.turn_ended_at = Some(now());
        }
        store.save_thread(&self.stored)?;
        Ok(true)
    }

    /// A message was sent now to the turn that runs, and the agent hasn't taken it yet.
    fn steering(&self) -> bool {
        let stopped = self.run.as_ref().is_some_and(|run| run.interrupted.load(Ordering::Relaxed));
        !stopped && !self.given.is_empty()
    }

    /// Gives the idle process the first message that waits, unless it still has one to take or
    /// waits for an answer itself. `false` when nothing was given.
    fn hand_over(&mut self, store: &Store) -> anyhow::Result<bool> {
        if !self.given.is_empty() || !self.activity.approvals.is_empty() || self.limited() {
            return Ok(false);
        }
        let Some(index) = self.queued.iter().position(|queued| !queued.held) else { return Ok(false) };
        self.give(store, index)
    }

    /// Writes a prompt to the agent's process, with the thread's settings. `false` when it takes
    /// none.
    fn write_prompt(&self, prompt: &str, id: &str) -> bool {
        let thread = &self.stored.thread;
        let model = thread.model.as_deref().or(self.session_model.as_deref());
        let settings = Settings { model, effort: thread.effort.as_deref() };
        if thread.agent == Agent::Claude && !self.write(claude::access_line(thread.access)) {
            return false;
        }
        let line = agents::input(thread.agent, self.stored.session_id.as_deref(), prompt, &self.skills, id, settings);
        line.is_some_and(|line| self.write(line))
    }

    /// Writes the queued message to the idle process, which starts its next turn with it.
    /// `false` when it takes no more.
    fn give(&mut self, store: &Store, index: usize) -> anyhow::Result<bool> {
        self.note_skills(&self.queued[index].skills.clone());
        let queued = &self.queued[index];
        if !self.write_prompt(&prompt(queued), &queued.id) {
            return Ok(false);
        }
        self.note_given(store, index)?;
        Ok(true)
    }

    /// Writes the queued message to the turn that runs, which takes it at once. `false` when the
    /// agent can't take it.
    fn steer(&mut self, store: &Store, index: usize) -> anyhow::Result<bool> {
        self.note_skills(&self.queued[index].skills.clone());
        let queued = &self.queued[index];
        let thread = &self.stored.thread;
        let turn_id = self.run.as_ref().and_then(|run| run.turn_id.as_deref());
        let session_id = self.stored.session_id.as_deref();
        let prompt = prompt(queued);
        let line = agents::steer(thread.agent, session_id, turn_id, &prompt, &self.skills, &queued.id);
        if !line.is_some_and(|line| self.write(line)) {
            return Ok(false);
        }
        self.note_given(store, index)?;
        Ok(true)
    }

    /// Moves a message the agent was given from the queue into the transcript.
    fn note_given(&mut self, store: &Store, index: usize) -> anyhow::Result<()> {
        let queued = self.queued.remove(index);
        self.save_queued(store)?;
        self.append_queued(store, queued.clone())?;
        self.given.push(queued);
        self.send_activity();
        Ok(())
    }

    /// Whether the agent works on the thread, or is about to.
    fn busy(&self) -> bool {
        self.preparing.is_some() || self.run.as_ref().is_some_and(|run| run.resting.is_none())
    }

    /// Lets the resting process go: its stdin closes and it exits. The next turn starts a new one.
    fn let_go(&mut self) {
        if self.run.as_ref().is_some_and(|run| run.resting.is_some()) {
            self.run = None;
        }
    }

    /// Writes a line to the process's stdin. `false` when it takes no more.
    fn write(&self, line: String) -> bool {
        let input = self.run.as_ref().and_then(|run| run.input.as_ref());
        input.is_some_and(|input| input.send(line).is_ok())
    }

    fn tool_input(&mut self, store: &Store, id: String, name: String, input: String) -> anyhow::Result<()> {
        let mut call = self.tool_call(&id).unwrap_or_else(|| new_tool_call(id.clone(), name));
        call.input = input;
        self.upsert(store, id, ItemKind::Tool { call })
    }

    fn tool_result(&mut self, store: &Store, id: String, output: String, is_error: bool) -> anyhow::Result<()> {
        let Some(mut call) = self.tool_call(&id) else { return Ok(()) };
        call.output = Some(output);
        call.status = if is_error { ToolStatus::Failed } else { ToolStatus::Succeeded };
        self.upsert(store, id, ItemKind::Tool { call })
    }

    /// Keeps the images a tool call's result showed the agent, which its row shows.
    fn tool_images(&mut self, store: &Store, id: String, images: Vec<ToolImage>) -> anyhow::Result<()> {
        let Some(call) = self.tool_call(&id) else { return Ok(()) };
        let input: Value = serde_json::from_str(&call.input).unwrap_or_default();
        let read = input["file_path"].as_str().filter(|_| images.len() == 1);
        let mut kept: Vec<Media> = Vec::new();
        for image in &images {
            let media = match image {
                ToolImage::Data(bytes) => self
                    .media
                    .keep_shown(bytes, kept.len() + 1)?
                    .map(|media| Media { src: read.map_or(media.src, String::from), ..media }),
                ToolImage::File(path) => self.media.capture_image(path, &self.stored.thread.cwd),
            };
            kept.extend(media);
        }
        let Some(item) = self.open.get_mut(&id) else { return Ok(()) };
        kept.retain(|media| !item.media.iter().any(|known| known.id == media.id));
        if kept.is_empty() {
            return Ok(());
        }
        for media in &kept {
            store.save_media(&self.stored.thread.id, &media.id)?;
        }
        item.media.extend(kept);
        self.upsert(store, id, ItemKind::Tool { call })
    }

    /// Adds what an agent the thread's agent started said or did to that agent's transcript.
    fn add_from_subagent(&mut self, store: &Store, event: AgentEvent) -> anyhow::Result<()> {
        match event {
            AgentEvent::Text { id, text } if !text.is_empty() => self.upsert(store, id, ItemKind::Assistant { text }),
            AgentEvent::ThinkingText { id, text } => self.upsert(store, id, ItemKind::Thinking { text }),
            AgentEvent::ToolInput { id, name, input } => self.tool_input(store, id, name, input),
            AgentEvent::ToolResult { id, output, is_error } => self.tool_result(store, id, output, is_error),
            AgentEvent::ToolImages { id, images } => self.tool_images(store, id, images),
            AgentEvent::Tool { call } => self.upsert(store, call.id.clone(), ItemKind::Tool { call }),
            _ => Ok(()),
        }
    }

    /// Notes how far the agent is that the tool call started.
    fn update_subagent(&mut self, store: &Store, tool_id: &str, update: Subagent) -> anyhow::Result<()> {
        let Some(mut call) = self.tool_call(tool_id) else { return Ok(()) };
        let agent = match call.agent.take() {
            Some(known) => Subagent {
                kind: update.kind.or(known.kind),
                status: update.status,
                progress: update.progress.or(known.progress),
                result: update.result.or(known.result),
                tokens: update.tokens.or(known.tokens),
                tool_uses: update.tool_uses.or(known.tool_uses),
                duration_ms: update.duration_ms.or(known.duration_ms),
                workflow: update.workflow.or(known.workflow),
                runs_on: update.runs_on.or(known.runs_on),
                waits: update.waits,
                stopped: update.stopped,
                limited: update.limited,
            },
            None => update,
        };
        call.agent = Some(agent);
        self.upsert(store, tool_id.to_string(), ItemKind::Tool { call })
    }

    fn set_thinking(&mut self, thinking: bool) {
        if self.activity.thinking == thinking {
            return;
        }
        self.activity.thinking = thinking;
        self.send_activity();
    }

    /// Adds an item that won't change again.
    fn append(&mut self, store: &Store, kind: ItemKind) -> anyhow::Result<()> {
        let item = self.new_item(new_id(), kind);
        self.save(store, item)
    }

    /// Keeps the item and sends it to everyone with the thread open.
    fn save(&self, store: &Store, item: Item) -> anyhow::Result<()> {
        store.save_item(&self.stored.thread.id, &item)?;
        let _ = self.updates.send(Message::Items { items: vec![item] });
        Ok(())
    }

    /// Adds the item that ends a turn. What the turn changed joins it once that has been read.
    fn end_turn(&mut self, store: &Store, summary: TurnSummary) -> anyhow::Result<()> {
        self.end_turn_as(store, summary, None)
    }

    /// `end_turn`, kept as ending with `outcome` in place of what the summary says.
    fn end_turn_as(
        &mut self,
        store: &Store,
        mut summary: TurnSummary,
        outcome: Option<TurnOutcome>,
    ) -> anyhow::Result<()> {
        let stopped_by = std::mem::take(&mut self.stopped_by);
        if summary.stopped {
            summary.stopped_by = stopped_by;
        }
        let limited = matches!(self.stored.thread.interruption, Some(Interruption::Limit { .. }));
        let outcome = outcome.unwrap_or(match () {
            _ if summary.stopped => TurnOutcome::Stopped,
            _ if limited => TurnOutcome::Limit,
            _ if summary.is_error => TurnOutcome::Failed,
            _ => TurnOutcome::Done,
        });
        let item = self.new_item(new_id(), ItemKind::TurnEnd { summary });
        store.save_item(&self.stored.thread.id, &item)?;
        self.ended = Some(item.id.clone());
        self.keep_turn(store, item.id.clone(), outcome);
        let _ = self.updates.send(Message::Items { items: vec![item] });
        Ok(())
    }

    /// Keeps what the turn that ran did, as ending now with `outcome`.
    fn keep_turn(&mut self, store: &Store, id: String, outcome: TurnOutcome) {
        let started = self.run.as_ref().map(|run| now() - run.started.elapsed().as_secs_f64());
        let tally =
            self.tally.take().unwrap_or_else(|| Tally::new(TurnStarter::Server, None, started.unwrap_or_else(now)));
        let thread = &self.stored.thread;
        let model = self.session_model.as_deref().or(thread.model.as_deref()).unwrap_or_default();
        let turn = tally.record(id, thread, model, outcome, now());
        if let Err(error) = store.save_turn(&turn) {
            tracing::warn!(thread_id = thread.id, "couldn't keep a turn: {error:#}");
        }
    }

    /// Notes whether the turn waits for the user now.
    fn note_waiting(&mut self) {
        let waiting = !self.activity.approvals.is_empty();
        if let Some(tally) = &mut self.tally {
            tally.waits(waiting, now());
        }
    }

    fn set_changes(&mut self, store: &Store, item_id: &str, changes: TurnChanges) -> anyhow::Result<()> {
        let Some(mut item) = store.item(&self.stored.thread.id, item_id)? else { return Ok(()) };
        let ItemKind::TurnEnd { summary } = &mut item.kind else { return Ok(()) };
        summary.changes = Some(changes);
        item.rev = self.next_rev();
        store.save_item(&self.stored.thread.id, &item)?;
        let _ = self.updates.send(Message::Items { items: vec![item] });
        Ok(())
    }

    /// Adds to the message the copies of its attached files that it doesn't hold yet.
    fn keep_attached_media(&mut self, store: &Store, item_id: &str, kept: Vec<Media>) -> anyhow::Result<()> {
        let Some(mut item) = store.item(&self.stored.thread.id, item_id)? else { return Ok(()) };
        let new: Vec<Media> =
            kept.into_iter().filter(|media| !item.media.iter().any(|held| held.src == media.src)).collect();
        if new.is_empty() {
            return Ok(());
        }
        for id in new.iter().flat_map(|media| std::iter::once(&media.id).chain(&media.poster)) {
            store.save_media(&self.stored.thread.id, id)?;
        }
        item.media.extend(new);
        item.rev = self.next_rev();
        self.save(store, item)
    }

    fn set_ui(&mut self, store: &Store, item_id: &str, block: u32, state: Value) -> anyhow::Result<()> {
        let found = match self.open.get(item_id) {
            Some(item) => Some(item.clone()),
            None => store.item(&self.stored.thread.id, item_id)?,
        };
        let mut item = found.context("That reply is no longer in the thread.")?;
        if !matches!(item.kind, ItemKind::Assistant { .. }) {
            bail!("Only an agent's reply has programs.");
        }
        match state {
            Value::Null => item.ui.remove(&block),
            state => item.ui.insert(block, state),
        };
        item.rev = self.next_rev();
        if let Some(open) = self.open.get_mut(item_id) {
            open.clone_from(&item);
            self.unsaved.remove(item_id);
        }
        self.save(store, item)
    }

    /// Adds a message to the transcript, as the user's message with the images and videos attached
    /// to it, or as the notice it is. A queued message keeps its id, so that the client can tell
    /// which waiting row it replaces.
    fn append_queued(&mut self, store: &Store, queued: Queued) -> anyhow::Result<()> {
        match &mut self.tally {
            Some(tally) => tally.steered(),
            None => self.tally = Some(Tally::new(starter_of(&queued), queued.by.device_key(), now())),
        }
        let Queued { id, text, attachments, media, skills, notice, context, programs, by, .. } = queued;
        let item = match notice {
            Some(notice) => self.new_item(id, ItemKind::Notice { notice, text }),
            None => {
                self.note_skills(&skills);
                let kind = ItemKind::User { text, attachments, skills, context, programs, by };
                Item { media, ..self.new_item(id, kind) }
            }
        };
        self.last_message_seq = Some(item.seq);
        self.save(store, item)
    }

    /// Changes a tool call of the transcript, whether the running turn may still change it or not.
    /// `change` says whether it changed anything.
    fn change_tool(
        &mut self,
        store: &Store,
        tool_id: &str,
        change: impl FnOnce(&mut ToolCall) -> bool,
    ) -> anyhow::Result<()> {
        if let Some(mut call) = self.tool_call(tool_id) {
            if !change(&mut call) {
                return Ok(());
            }
            return self.upsert(store, tool_id.to_string(), ItemKind::Tool { call });
        }
        let Some(mut item) = store.item(&self.stored.thread.id, tool_id)? else { return Ok(()) };
        let ItemKind::Tool { call } = &mut item.kind else { return Ok(()) };
        if !change(call) {
            return Ok(());
        }
        item.rev = self.next_rev();
        self.save(store, item)
    }

    /// Makes another thread's transcript the beginning of this one's, which its agent is then told
    /// as the thread's history.
    fn take_history(&mut self, store: &Store, items: Vec<Item>) -> anyhow::Result<()> {
        self.tools = None;
        let mut taken = Vec::with_capacity(items.len());
        for item in items {
            let seq = self.stored.next_seq;
            self.stored.next_seq += 1;
            let item = Item { seq, rev: self.next_rev(), ..item };
            store.save_item(&self.stored.thread.id, &item)?;
            taken.push(item);
        }
        let _ = self.updates.send(Message::Items { items: taken });
        Ok(())
    }

    fn note_skills(&mut self, skills: &[SkillMention]) {
        let new: Vec<SkillMention> = skills.iter().filter(|skill| !self.skills.contains(skill)).cloned().collect();
        self.skills.extend(new);
    }

    fn new_item(&mut self, id: String, kind: ItemKind) -> Item {
        let seq = self.stored.next_seq;
        self.stored.next_seq += 1;
        let parent = self.parent.clone();
        if parent.is_none()
            && matches!(kind, ItemKind::Tool { .. })
            && let Some(tools) = &mut self.tools
        {
            *tools += 1;
        }
        Item {
            id,
            seq,
            rev: self.next_rev(),
            created_at: now(),
            media: Vec::new(),
            parent,
            ui: Default::default(),
            kind,
        }
    }

    /// Adds the item, or replaces what the running turn said about it before.
    fn upsert(&mut self, store: &Store, id: String, kind: ItemKind) -> anyhow::Result<()> {
        let rev = self.next_rev();
        let mut item = match self.open.get_mut(&id) {
            Some(item) => {
                item.kind = with_decision(kind, &item.kind);
                item.rev = rev;
                item.clone()
            }
            None => {
                // `new_item` takes a revision of its own; give back the one taken above.
                self.stored.thread.rev -= 1;
                let item = self.new_item(id.clone(), kind);
                self.open.insert(id.clone(), item.clone());
                item
            }
        };
        if self.keep_media(store, &id)? {
            item = self.open[&id].clone();
        }
        self.unsaved.remove(&id);
        store.save_item(&self.stored.thread.id, &item)?;
        let _ = self.updates.send(Message::Items { items: vec![item] });
        Ok(())
    }

    /// Copies the images, videos and files a reply shows that its item doesn't hold yet. `true`
    /// when the item holds more now.
    fn keep_media(&mut self, store: &Store, id: &str) -> anyhow::Result<bool> {
        let Some(item) = self.open.get_mut(id) else { return Ok(false) };
        let ItemKind::Assistant { text } = &item.kind else { return Ok(false) };
        let kept = self.media.capture_new(text, &self.stored.thread.cwd, &item.media);
        if kept.is_empty() {
            return Ok(false);
        }
        for id in kept.iter().flat_map(|media| std::iter::once(&media.id).chain(&media.poster)) {
            store.save_media(&self.stored.thread.id, id)?;
        }
        item.media.extend(kept);
        Ok(true)
    }

    /// Takes streamed text and passes on the blocks it finishes.
    fn hold_text(&mut self, store: &Store, id: String, text: String) -> anyhow::Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        self.set_thinking(false);
        let held = self.held.entry(id.clone()).or_default();
        held.push_str(&text);
        let too_soon = self.last_delivery.is_some_and(|last| last.elapsed() < pacing::DELIVER_EVERY);
        if too_soon || !text.contains('\n') {
            return Ok(());
        }
        let delivered = match self.open.get(&id) {
            Some(Item { kind: ItemKind::Assistant { text }, .. }) => text.as_str(),
            _ => "",
        };
        let ready = pacing::settled_len(&format!("{delivered}{held}")).saturating_sub(delivered.len());
        if ready == 0 {
            return Ok(());
        }
        let finished: String = held.drain(..ready).collect();
        self.last_delivery = Some(Instant::now());
        self.append_text(store, id, finished)
    }

    /// Passes on the text that was waiting to become a block; nothing more of it is coming.
    fn release_held(&mut self, store: &Store) -> anyhow::Result<()> {
        for (id, text) in std::mem::take(&mut self.held) {
            self.append_text(store, id, text)?;
        }
        Ok(())
    }

    fn append_text(&mut self, store: &Store, id: String, text: String) -> anyhow::Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        let Some(Item { kind: ItemKind::Assistant { text: current }, .. }) = self.open.get_mut(&id) else {
            return self.upsert(store, id, ItemKind::Assistant { text });
        };
        current.push_str(&text);
        let rev = self.next_rev();
        if let Some(item) = self.open.get_mut(&id) {
            item.rev = rev;
        }
        // An item that shows something new is sent whole, with what it shows.
        if self.keep_media(store, &id)? {
            let item = self.open[&id].clone();
            self.unsaved.remove(&id);
            store.save_item(&self.stored.thread.id, &item)?;
            let _ = self.updates.send(Message::Items { items: vec![item] });
            return Ok(());
        }
        self.unsaved.insert(id.clone());
        let _ = self.updates.send(Message::TextDelta { id, text, rev });
        Ok(())
    }

    fn flush(&mut self, store: &Store) -> anyhow::Result<()> {
        self.last_flush = Instant::now();
        for id in std::mem::take(&mut self.unsaved) {
            let Some(item) = self.open.get(&id) else { continue };
            store.save_item(&self.stored.thread.id, item)?;
        }
        Ok(())
    }

    /// Fails the thread's tool calls that a turn ended without: the agent dropped them unrun.
    fn fail_dropped_tools(&mut self, store: &Store) -> anyhow::Result<()> {
        let dropped: Vec<(String, ToolCall)> = self
            .open
            .values()
            .filter(|item| item.parent.is_none())
            .filter_map(|item| match &item.kind {
                ItemKind::Tool { call } if call.status == ToolStatus::Running && call.agent.is_none() => {
                    Some((item.id.clone(), call.clone()))
                }
                _ => None,
            })
            .collect();
        for (id, mut call) in dropped {
            call.status = ToolStatus::Failed;
            self.upsert(store, id, ItemKind::Tool { call })?;
        }
        Ok(())
    }

    /// Fails the tools and agents that never reported back: they were cut off.
    fn cut_off_tools(&mut self, store: &Store, interrupted: bool) -> anyhow::Result<()> {
        let cut_off: Vec<Item> = self
            .open
            .values()
            .filter(|item| matches!(&item.kind, ItemKind::Tool { call } if works(call)))
            .cloned()
            .collect();
        for item in cut_off {
            let ItemKind::Tool { mut call } = item.kind else { continue };
            if call.status == ToolStatus::Running {
                call.status = ToolStatus::Failed;
                if interrupted {
                    call.output.get_or_insert_with(|| "Interrupted".to_string());
                }
            }
            // An agent that works in a thread of its own goes on without the turn that started it.
            let own_thread = |agent: &Subagent| agent.runs_on.as_ref().is_some_and(|on| on.thread_id.is_some());
            if let Some(agent) = call.agent.as_mut().filter(|agent| !own_thread(agent)) {
                agent.status = ToolStatus::Failed;
                agent.stopped = interrupted;
            }
            self.upsert(store, item.id, ItemKind::Tool { call })?;
        }
        Ok(())
    }

    /// Tidies the transcript once the agent's process has exited.
    fn settle(
        &mut self,
        store: &Store,
        run: Option<&Run>,
        exit_code: Option<i32>,
        stderr: &str,
        interrupted: bool,
    ) -> anyhow::Result<()> {
        self.release_held(store)?;
        self.flush(store)?;
        self.cut_off_tools(store, interrupted)?;

        let received_result = run.is_some_and(|run| run.received_result);
        if interrupted {
            if received_result {
                return Ok(());
            }
            let duration_ms = run.map(|run| run.started.elapsed().as_millis() as u64);
            let summary = TurnSummary { duration_ms, stopped: true, ..TurnSummary::default() };
            return self.end_turn(store, summary);
        }
        if received_result {
            return Ok(());
        }
        let details = stderr.trim();
        let agent = agent_name(self.stored.thread.agent);
        let message = match (details.is_empty(), exit_code) {
            (false, _) if details.contains(ROOT_BYPASS_REFUSAL) => {
                "Claude Code refuses full access when the server runs as root. Run `motile setup` again on the server \
                 to fix the service."
                    .to_string()
            }
            (false, _) => tail(details, 2000).to_string(),
            (true, Some(code)) => format!("{agent} exited unexpectedly (status {code})."),
            (true, None) => format!("{agent} exited unexpectedly."),
        };
        self.append(store, ItemKind::Error { message })?;
        self.keep_turn(store, new_id(), TurnOutcome::Failed);
        Ok(())
    }
}

fn account_in(accounts: &[AgentAccount], thread: &Thread) -> AgentAccount {
    let found = accounts.iter().find(|account| account.id == thread.agent_account && account.agent == thread.agent);
    found.cloned().unwrap_or_else(|| agent_accounts::default_of(accounts, thread.agent))
}

/// The agent of the thread's account and the folder it keeps its sessions in.
fn continuation_of(thread: &Thread, accounts: &[AgentAccount], environment: &Environment) -> Continuation {
    let folder = agent_accounts::sessions_folder(&account_in(accounts, thread), environment);
    Continuation { agent: thread.agent, folder: folder.to_string_lossy().into_owned() }
}

fn ensure_idle(threads: &[OwnedMutexGuard<Live>], account_id: &str) -> anyhow::Result<()> {
    if threads.iter().any(|live| live.stored.thread.agent_account == account_id && live.busy()) {
        bail!("An agent still works with this account. Change it once the agent has finished.");
    }
    Ok(())
}

/// Lets the account's resting processes go, so that the next turn starts with what it is now.
fn let_go_of_account(threads: &mut [OwnedMutexGuard<Live>], account_id: &str) {
    for live in threads.iter_mut().filter(|live| live.stored.thread.agent_account == account_id) {
        live.let_go();
    }
}

/// Holds Claude Code's first prompt until its MCP servers are ready, asking it every
/// `MCP_ASK_EVERY` for at most `MCP_WAIT`. It starts a turn without waiting for claude.ai's
/// connectors, which would leave the turn without them.
async fn wait_for_mcp(stdin: &mut ChildStdin, ready: watch::Receiver<bool>) {
    let asking = async {
        while !*ready.borrow() {
            if stdin.write_all(claude::mcp_status_line().as_bytes()).await.is_err() {
                return;
            }
            tokio::time::sleep(MCP_ASK_EVERY).await;
        }
    };
    let _ = tokio::time::timeout(MCP_WAIT, asking).await;
}

/// The model with the default effort it was known to have, where its agent couldn't say it now.
fn with_known_default(model: ModelInfo, known: &[ModelInfo]) -> ModelInfo {
    if model.default_effort.is_some() {
        return model;
    }
    let same = known.iter().find(|before| before.account == model.account && before.id == model.id);
    let default_effort =
        same.and_then(|before| before.default_effort.clone()).filter(|effort| model.efforts.contains(effort));
    ModelInfo { default_effort, ..model }
}

/// Model and effort names end up on the agent's command line and in its config, so they are
/// held to what such names are made of.
fn checked(what: &str, value: Option<String>) -> anyhow::Result<Option<String>> {
    let Some(value) = value.filter(|value| !value.is_empty()) else { return Ok(None) };
    let allowed = |character: char| character.is_ascii_alphanumeric() || "-_.[]".contains(character);
    if value.len() > 100 || !value.chars().all(allowed) {
        bail!("{value} isn't a valid {what}.");
    }
    Ok(Some(value))
}

/// Adds the "No project" project when the server doesn't have it yet, and makes its folder.
fn ensure_no_project(store: &Store, projects: &mut Vec<StoredProject>, folder: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(folder).with_context(|| format!("{} can't be made.", folder.display()))?;
    let path = folder.to_string_lossy().into_owned();
    if projects.iter().any(|project| project.path == path) {
        return Ok(());
    }
    let project = StoredProject { id: new_id(), path, created_at: now(), icon: None, icon_chosen: false, setup: None };
    store.add_project(&project)?;
    projects.push(project);
    Ok(())
}

/// Has git stop looking for a repository at the server's own folder, so that a thread without a
/// project never works in one its folder happens to be inside, like a home folder kept in git.
fn stop_git_above(environment: Environment, no_project_folder: &Path) -> Environment {
    let Some(data_folder) = no_project_folder.parent() else { return environment };
    let data_folder = data_folder.to_string_lossy();
    let ceilings = match environment.variables.get("GIT_CEILING_DIRECTORIES") {
        Some(others) if !others.is_empty() => format!("{others}:{data_folder}"),
        _ => data_folder.into_owned(),
    };
    environment.with([("GIT_CEILING_DIRECTORIES".to_string(), ceilings)])
}

/// Whether the two paths name one folder, however either is spelled.
fn same_folder(a: &str, b: &str) -> bool {
    let real = |path: &str| std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    a == b || real(a) == real(b)
}

fn thread_worktree(stored: &StoredThread) -> Option<(String, ThreadWorktree)> {
    let worktree = stored.worktree.as_ref()?;
    let thread = &stored.thread;
    let own = ThreadWorktree {
        project_id: thread.project_id.clone(),
        path: thread.cwd.clone(),
        branch: worktree.branch.clone(),
    };
    Some((thread.id.clone(), own))
}

/// The branch a worktree is made on, named after its folder until the writer names it.
fn temporary_branch(worktree: &str) -> String {
    format!("{BRANCH_PREFIX}/{}", file_name(worktree))
}

/// A tool call the agent reports again keeps who allowed or refused it.
fn with_decision(mut kind: ItemKind, before: &ItemKind) -> ItemKind {
    if let (ItemKind::Tool { call }, ItemKind::Tool { call: known }) = (&mut kind, before)
        && call.decided_by.is_unknown()
    {
        call.decided_by = known.decided_by.clone();
    }
    kind
}

/// The tool call, or the agent it started, hasn't reported back yet.
fn works(call: &ToolCall) -> bool {
    let agent_works = call.agent.as_ref().is_some_and(|agent| agent.status == ToolStatus::Running);
    call.status == ToolStatus::Running || agent_works
}

fn new_tool_call(id: String, name: String) -> ToolCall {
    ToolCall {
        id,
        name,
        input: "{}".to_string(),
        output: None,
        status: ToolStatus::Running,
        agent: None,
        decided_by: Actor::Unknown,
    }
}

/// What a turn the server's restart cut off ends with.
pub(crate) const RESTARTED: &str = "Your server restarted before the agent finished.";

/// A message for a thread, as `Request::Send` carries it.
#[derive(Default)]
pub struct NewMessage {
    pub text: String,
    pub attachments: Vec<String>,
    /// Given to the turn that runs at once, if the agent can take it.
    pub now: bool,
    pub context: Option<Value>,
    /// The id the client gave it, which a message sent again is known by.
    pub message_id: Option<String>,
    pub by: Actor,
    /// The client that sent it, whose use of the features it keeps.
    pub sender: Option<Sender>,
}

impl NewMessage {
    pub fn text(text: impl Into<String>, by: Actor) -> Self {
        Self { text: text.into(), by, ..Self::default() }
    }

    pub fn from_client(text: impl Into<String>, sender: Sender) -> Self {
        Self { by: Actor::device(&sender.device), sender: Some(sender), ..Self::text(text, Actor::Unknown) }
    }
}

/// Brings a snoozed thread back to the top of the active ones, marked as woken when it woke by
/// itself: its time came, or it needs the user.
fn wake(thread: &mut Thread, by_itself: bool) -> bool {
    if thread.snoozed_until.take().is_none() {
        return false;
    }
    thread.position = now();
    if by_itself {
        thread.woke_at = Some(now());
    }
    true
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn agent_name(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "Claude Code",
        Agent::Codex => "Codex",
    }
}

fn signal(process_id: u32, signal_number: i32) {
    let Ok(group) = i32::try_from(process_id) else { return };
    if group <= 0 {
        return;
    }
    unsafe { libc::kill(-group, signal_number) };
}

fn tail(text: &str, max_chars: usize) -> &str {
    let skip = text.chars().count().saturating_sub(max_chars);
    text.char_indices().nth(skip).map_or("", |(index, _)| &text[index..])
}

/// The folder a project named `name` gets: its letters and digits in lowercase, with dashes
/// for everything between them.
fn folder_name(name: &str) -> Option<String> {
    let words = name.split(|c: char| !c.is_ascii_alphanumeric()).filter(|word| !word.is_empty());
    let folder: String = words.collect::<Vec<_>>().join("-").to_lowercase().chars().take(64).collect();
    let folder = folder.trim_end_matches('-');
    (!folder.is_empty()).then(|| folder.to_string())
}

pub(crate) fn file_name(path: &str) -> &str {
    Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or(path)
}

/// A message with nothing attached to it.
fn plain_message(text: String, by: Actor) -> Queued {
    Queued {
        id: new_id(),
        text,
        attachments: Vec::new(),
        media: Vec::new(),
        skills: Vec::new(),
        held: false,
        sending: false,
        notice: None,
        by,
        context: None,
        programs: Vec::new(),
    }
}

/// What a message that starts a turn says started it.
fn starter_of(message: &Queued) -> TurnStarter {
    match (&message.notice, &message.by) {
        (Some(_), _) => TurnStarter::Notice,
        (None, Actor::User { .. } | Actor::Unknown) => TurnStarter::User,
        (None, Actor::Agent { .. }) => TurnStarter::Agent,
        (None, Actor::Server { .. }) => TurnStarter::Server,
    }
}

/// The features a message uses, as the request that sent it says.
fn activity_uses(
    thread_id: &Option<String>,
    new_thread: &Option<NewThread>,
    now: bool,
    context: &Option<Value>,
) -> Vec<motile_protocol::activity::Use> {
    motile_protocol::activity::uses(&Request::Send {
        thread_id: thread_id.clone(),
        new_thread: new_thread.clone(),
        text: String::new(),
        attachments: Vec::new(),
        now,
        context: context.clone(),
        message_id: None,
    })
}

fn prompt(message: &Queued) -> String {
    let text = ui::told(&message.text, message.context.as_ref(), &message.programs);
    with_attachments(&text, &message.attachments)
}

fn with_attachments(text: &str, attachments: &[String]) -> String {
    if attachments.is_empty() {
        return text.to_string();
    }
    let list: Vec<String> = attachments.iter().map(|path| format!("- {path}")).collect();
    let files = format!("Attached files:\n{}", list.join("\n"));
    if text.is_empty() { files } else { format!("{text}\n\n{files}") }
}

/// The checks that failed or were cancelled, by their names.
fn failing(found: &PullRequestDetail) -> Vec<String> {
    let failed =
        found.checks.iter().filter(|check| matches!(check.status, CheckStatus::Failure | CheckStatus::Cancelled));
    failed
        .map(|check| match &check.workflow {
            Some(workflow) => format!("{workflow} / {}", check.name),
            None => check.name.clone(),
        })
        .collect()
}

/// What a watched pull request shows now.
fn seen(found: &PullRequestDetail) -> Seen {
    let mut said: HashSet<String> = HashSet::new();
    for event in &found.activity {
        match &event.kind {
            EventKind::Comment { id, .. } | EventKind::Review { id, .. } => said.insert(id.clone()),
            EventKind::Commit { .. } => continue,
        };
    }
    for thread in &found.threads {
        said.extend(thread.comments.iter().map(|comment| comment.id.clone()));
    }
    Seen {
        checks_running: found.checks.iter().any(|check| check.status == CheckStatus::Pending),
        failing: failing(found).into_iter().collect(),
        conflicting: found.mergeable == Mergeable::Conflicting,
        said,
    }
}

/// What the agent of a thread that watches its pull request is told has changed, if anything.
fn news(found: &PullRequestDetail, before: &Seen, now: &Seen) -> Option<String> {
    let mut lines = Vec::new();
    let failed = failing(found);
    let newly_failed = failed.iter().any(|name| !before.failing.contains(name));
    if !now.checks_running && (before.checks_running || newly_failed) {
        lines.push(match failed.len() {
            0 => "- Its checks finished, and all of them passed.".to_string(),
            count => format!("- Its checks finished: {count} of {} failed: {}.", found.checks.len(), failed.join(", ")),
        });
    }
    if now.conflicting && !before.conflicting {
        lines.push(format!("- It now conflicts with `{}`.", found.base));
    }
    let login = &found.viewer.login;
    let mut said = |author: &str, did: String, id: &str, body: &str| {
        if before.said.contains(id) || author == login {
            return;
        }
        let quote: String = body.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(500).collect();
        lines.push(if quote.is_empty() {
            format!("- {author} {did}.")
        } else {
            format!("- {author} {did}: \"{quote}\"")
        });
    };
    for event in &found.activity {
        match &event.kind {
            EventKind::Comment { id, body, .. } => said(&event.author, "commented".to_string(), id, body),
            EventKind::Review { id, body, verdict, .. } => {
                let did = match verdict {
                    motile_protocol::wire::Verdict::Approved => "approved it",
                    motile_protocol::wire::Verdict::ChangesRequested => "requested changes",
                    _ => "reviewed it",
                };
                said(&event.author, did.to_string(), id, body)
            }
            EventKind::Commit { .. } => {}
        }
    }
    for thread in &found.threads {
        let place = match thread.line {
            Some(line) => format!("commented on `{}` line {line}", thread.path),
            None => format!("commented on `{}`", thread.path),
        };
        for comment in &thread.comments {
            said(&comment.author, place.clone(), &comment.id, &comment.body);
        }
    }
    if lines.is_empty() {
        return None;
    }
    let number = found.pull_request.number;
    let mut text = vec![format!("PR #{number} ({}) changed on GitHub:", found.pull_request.url)];
    text.extend(lines);
    text.push(motile_protocol::wire::PULL_REQUEST_NEWS_END.to_string());
    Some(text.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_projects_folder_is_named_after_it() {
        assert_eq!(folder_name("My App!").as_deref(), Some("my-app"));
        assert_eq!(folder_name("  api_v2 / web ").as_deref(), Some("api-v2-web"));
        assert_eq!(folder_name("../.."), None);
        assert_eq!(folder_name(&"a".repeat(80)).map(|folder| folder.len()), Some(64));
    }

    #[test]
    fn prompt_lists_attachments_after_the_text() {
        let attachments = ["/a".to_string(), "/b".to_string()];
        assert_eq!(with_attachments("Look", &attachments), "Look\n\nAttached files:\n- /a\n- /b");
        assert_eq!(with_attachments("", &["/a".to_string()]), "Attached files:\n- /a");
    }

    #[test]
    fn a_prompt_tells_what_the_program_sent_before_the_attachments() {
        let message = Queued {
            id: "m".into(),
            text: "Book it".into(),
            attachments: vec!["/a".into()],
            media: Vec::new(),
            skills: Vec::new(),
            held: false,
            sending: false,
            notice: None,
            by: Actor::Unknown,
            context: Some(serde_json::json!(["User clicked: Book it", {"people": 4}])),
            programs: Vec::new(),
        };

        assert_eq!(
            prompt(&message),
            "Book it\ncontext and form values = [\"User clicked: Book it\",{\"people\":4}]\n\nAttached files:\n- /a"
        );
    }

    #[test]
    fn model_names_are_held_to_safe_characters() {
        assert_eq!(
            checked("model", Some("claude-opus-5-5[1m]".into())).unwrap().as_deref(),
            Some("claude-opus-5-5[1m]")
        );
        assert_eq!(checked("model", Some(String::new())).unwrap(), None);
        assert!(checked("model", Some("a; rm -rf /".into())).is_err());
    }
}
