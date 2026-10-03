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
use motile_protocol::now;
use motile_protocol::wire::{
    Activity, Agent, BranchInstructions, ChangedFile, DiffScope, FileKind, GitAction, GitHubState, GitStage, GitStatus,
    Item, ItemKind, Media, Message, NewThread, Project, Queued, ServerInfo, Subagent, Thread, ThreadChange, ToolCall,
    ToolStatus, TurnChanges, TurnSummary, Worktree,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, broadcast, mpsc};
use tokio::task::AbortHandle;

use crate::agents::environment::Environment;
use crate::agents::{self, AgentEvent, Background, PLAN_TOOL, Parser, Turn, claude, executable_name};
use crate::generate::Writer;
use crate::media::MediaStore;
use crate::store::{Store, StoredProject, StoredThread, StoredWorktree, TitleSource};
use crate::{drafts, files, git, github, icons, pacing, title};

const UPDATES_BUFFER: usize = 4096;
const ROOT_BYPASS_REFUSAL: &str = "cannot be used with root/sudo privileges";
/// Streamed text is written to disk at most this often, and when its block ends.
const FLUSH_EVERY: Duration = Duration::from_secs(1);
/// How long a branch's pull request is taken as known before GitHub is asked again.
const PULL_REQUEST_FRESH: Duration = Duration::from_secs(60);
const TEXT_MODEL: &str = "text_model";
const BRANCH_INSTRUCTIONS: &str = "branch_instructions";
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

/// What an app asked git to do in a project's folder.
pub struct GitRun {
    pub action: GitAction,
    /// The thread the work was done in.
    pub thread_id: Option<String>,
    pub message: Option<String>,
    pub paths: Vec<String>,
    pub new_branch: bool,
}

pub struct Hub {
    store: Store,
    pub media: MediaStore,
    environment: Environment,
    threads: Mutex<HashMap<String, Live>>,
    /// Locked after `threads` when both are needed.
    projects: Mutex<Vec<StoredProject>>,
    /// What git last said about each folder threads work in: the projects' and the worktrees'.
    git: std::sync::Mutex<HashMap<String, GitRead>>,
    /// Where the files that apps upload are.
    attachments_folder: PathBuf,
    /// Where the worktrees are made, each in a folder named after its project.
    worktrees_folder: PathBuf,
    /// The worktrees of the threads that work in one of their own, by thread.
    worktrees: std::sync::Mutex<HashMap<String, ThreadWorktree>>,
    /// The model the user picked to write titles, commit messages and pull requests.
    text_model: std::sync::Mutex<Option<String>>,
    /// How the user wants branches named.
    branch_instructions: std::sync::Mutex<Option<String>>,
    /// Held while a folder is kept as it is, so a thread's snapshots follow one another.
    snapshotting: Mutex<()>,
    list_updates: broadcast::Sender<Message>,
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
    /// Messages sent while the agent was working, until it takes them.
    queued: Vec<Queued>,
    title_needs_refinement: bool,
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
    /// `attachments_folder` where the files that apps upload are, and `worktrees_folder` where
    /// the worktrees of threads are made.
    pub fn new(
        store: Store,
        media_folder: PathBuf,
        attachments_folder: PathBuf,
        worktrees_folder: PathBuf,
        environment: Environment,
    ) -> anyhow::Result<Arc<Self>> {
        let home = environment.variables.get("HOME").map(String::as_str).unwrap_or_default();
        let media = MediaStore::new(media_folder, home);
        let threads = store.load_threads()?;
        let worktrees = threads.iter().filter_map(thread_worktree).collect();
        let live = |stored: StoredThread| (stored.thread.id.clone(), Live::new(stored, media.clone()));
        let threads = threads.into_iter().map(live).collect();
        let mut projects = store.load_projects()?;
        for project in &mut projects {
            refresh_icon(&store, project);
        }
        let projects = Mutex::new(projects);
        let (list_updates, _) = broadcast::channel(UPDATES_BUFFER);
        let git = std::sync::Mutex::default();
        Ok(Arc::new(Self {
            text_model: std::sync::Mutex::new(store.setting(TEXT_MODEL)),
            branch_instructions: std::sync::Mutex::new(store.setting(BRANCH_INSTRUCTIONS)),
            worktrees: std::sync::Mutex::new(worktrees),
            snapshotting: Mutex::default(),
            threads: Mutex::new(threads),
            store,
            media,
            environment,
            projects,
            git,
            attachments_folder,
            worktrees_folder,
            list_updates,
        }))
    }

    pub fn server_info(&self) -> ServerInfo {
        ServerInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol: motile_protocol::PROTOCOL_VERSION,
            hostname: Environment::hostname(),
            home: self.environment.variables.get("HOME").cloned().unwrap_or_default(),
            agents: self.environment.agents(),
            models: self.environment.models().to_vec(),
            text_model: self.writer(Agent::Claude).model,
            branch_instructions: self.branch_instructions(),
        }
    }

    pub async fn subscribe(&self) -> ListSubscription {
        let threads = self.threads.lock().await;
        let mut list: Vec<Thread> = threads.values().map(|live| live.stored.thread.clone()).collect();
        list.sort_by(|a, b| b.updated_at.total_cmp(&a.updated_at));
        ListSubscription {
            first: Message::Welcome {
                server: self.server_info(),
                threads: list,
                projects: self.projects_of(&self.projects.lock().await),
            },
            updates: self.list_updates.subscribe(),
        }
    }

    /// What changed in the thread after revision `since`, and every change from now on.
    pub async fn open(&self, thread_id: &str, since: u64) -> anyhow::Result<ThreadSubscription> {
        let mut threads = self.threads.lock().await;
        let live = threads.get_mut(thread_id).context("That thread no longer exists.")?;
        live.flush(&self.store)?;
        let rev = live.stored.thread.rev;
        // An app ahead of the server has a copy from before the server's data was replaced.
        let reset = since > rev;
        let items = self.store.items_since(thread_id, if reset { 0 } else { since })?;
        Ok(ThreadSubscription { reset, activity: live.activity(), items, rev, updates: live.updates.subscribe() })
    }

    /// Starts a turn and returns the thread it runs in.
    pub async fn send(
        self: &Arc<Self>,
        thread_id: Option<String>,
        new_thread: Option<NewThread>,
        text: String,
        attachments: Vec<String>,
    ) -> anyhow::Result<String> {
        let text = text.trim().to_string();
        if text.is_empty() && attachments.is_empty() {
            bail!("There is nothing to send.");
        }
        if let Some(gone) = attachments.iter().find(|path| !Path::new(path).is_file()) {
            bail!("{} is no longer on the server. Attach it again.", file_name(gone));
        }
        let mut threads = self.threads.lock().await;
        let (thread_id, is_new) = match (thread_id, new_thread) {
            (Some(thread_id), _) => (thread_id, false),
            (None, Some(new_thread)) => {
                let stored = self.new_thread(new_thread, &text, &attachments).await?;
                self.store.save_thread(&stored)?;
                let thread_id = stored.thread.id.clone();
                if let Some((thread_id, worktree)) = thread_worktree(&stored) {
                    self.lock_worktrees().insert(thread_id, worktree);
                }
                threads.insert(thread_id.clone(), Live::new(stored, self.media.clone()));
                (thread_id, true)
            }
            (None, None) => bail!("No thread was given."),
        };
        let live = threads.get_mut(&thread_id).context("That thread no longer exists.")?;
        let media = self.keep_attached(&thread_id, &attachments)?;
        let prompt = prompt(&text, &attachments);
        if live.preparing.is_some() {
            live.queued.push(Queued { id: new_id(), text, attachments, media, held: false, sending: false });
            live.send_activity();
            return Ok(thread_id);
        }
        if let Some(run) = &live.run {
            // An agent that is only monitoring takes the message right away.
            let idle = run.received_result;
            if !idle || !live.write_prompt(&prompt, &new_id()) {
                live.queued.push(Queued { id: new_id(), text, attachments, media, held: false, sending: false });
                live.send_activity();
                return Ok(thread_id);
            }
            live.append_message(&self.store, text, attachments, media)?;
            self.resume(live)?;
            return Ok(thread_id);
        }
        live.append_message(&self.store, text.clone(), attachments, media)?;
        self.start_turn(live, prompt)?;
        if is_new {
            tokio::spawn(self.clone().title_from_first_message(thread_id.clone(), text));
        }
        Ok(thread_id)
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

    async fn new_thread(
        &self,
        new_thread: NewThread,
        text: &str,
        attachments: &[String],
    ) -> anyhow::Result<StoredThread> {
        let projects = self.projects.lock().await;
        let project = projects
            .iter()
            .find(|project| project.id == new_thread.project_id)
            .context("That project is no longer on the server.")?;
        let created_at = now();
        let worktree = match new_thread.worktree {
            Some(new) if new.base.is_empty() || new.base.starts_with('-') => {
                bail!("{} isn't a branch to start from.", new.base)
            }
            Some(new) => {
                let name: String = uuid::Uuid::new_v4().simple().to_string().chars().take(8).collect();
                let folder = self.worktrees_folder.join(file_name(&project.path)).join(&name);
                let branch = temporary_branch(&folder.to_string_lossy());
                Some((folder.to_string_lossy().into_owned(), StoredWorktree { branch, base: new.base }))
            }
            None => None,
        };
        let (cwd, worktree) = match worktree {
            Some((folder, worktree)) => (folder, Some(worktree)),
            None => (project.path.clone(), None),
        };
        let thread = Thread {
            id: new_id(),
            title: title::placeholder(text, attachments),
            project_id: project.id.clone(),
            cwd,
            agent: new_thread.agent,
            model: checked("model", new_thread.model)?,
            effort: checked("effort", new_thread.effort)?,
            access: new_thread.access,
            plan: new_thread.plan,
            created_at,
            updated_at: created_at,
            done_at: None,
            undone_at: None,
            running: false,
            monitoring: false,
            needs_approval: false,
            agents: 0,
            turn_ended_at: None,
            rev: 0,
        };
        Ok(StoredThread { thread, session_id: None, title_source: TitleSource::Placeholder, next_seq: 0, worktree })
    }

    async fn title_from_first_message(self: Arc<Self>, thread_id: String, text: String) {
        let Some(agent) = self.agent_of(&thread_id).await else { return };
        match title::from_first_message(&self.environment, &self.writer(agent), &text).await {
            Some(generated) if !generated.needs_refinement => {
                self.set_generated_title(&thread_id, generated.title).await
            }
            _ => self.refine_title_later(&thread_id).await,
        }
    }

    /// The first message didn't say enough; the transcript will, once the first turn has ended.
    async fn refine_title_later(self: &Arc<Self>, thread_id: &str) {
        let mut threads = self.threads.lock().await;
        let Some(live) = threads.get_mut(thread_id) else { return };
        if live.stored.thread.running {
            live.title_needs_refinement = true;
            return;
        }
        tokio::spawn(self.clone().title_from_transcript(thread_id.to_string()));
    }

    async fn title_from_transcript(self: Arc<Self>, thread_id: String) {
        let Some(agent) = self.agent_of(&thread_id).await else { return };
        let (previous_title, items) = {
            let threads = self.threads.lock().await;
            let Some(live) = threads.get(&thread_id) else { return };
            let Ok(items) = self.store.items_since(&thread_id, 0) else { return };
            (live.stored.thread.title.clone(), items)
        };
        let writer = self.writer(agent);
        let Some(generated) = title::from_transcript(&self.environment, &writer, &previous_title, &items).await else {
            return;
        };
        self.set_generated_title(&thread_id, generated.title).await;
    }

    /// Who writes titles, commit messages and pull requests: the model the user picked, or the
    /// lightest one of `agent`.
    fn writer(&self, agent: Agent) -> Writer {
        let picked = self.text_model.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        let model = picked.and_then(|id| self.environment.models().iter().find(|model| model.id == id));
        match model {
            Some(model) => Writer { agent: model.agent, model: Some(model.id.clone()) },
            None => Writer { agent, model: None },
        }
    }

    pub fn set_text_model(&self, model: Option<String>) -> anyhow::Result<()> {
        if let Some(id) = &model
            && !self.environment.models().iter().any(|model| &model.id == id)
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

    async fn agent_of(&self, thread_id: &str) -> Option<Agent> {
        self.threads.lock().await.get(thread_id).map(|live| live.stored.thread.agent)
    }

    async fn set_generated_title(&self, thread_id: &str, title: String) {
        let mut threads = self.threads.lock().await;
        let Some(live) = threads.get_mut(thread_id) else { return };
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
    ) -> anyhow::Result<()> {
        let mut threads = self.threads.lock().await;
        let live = threads.get_mut(thread_id).context("That thread no longer exists.")?;
        let waiting = live.activity.approvals.iter().position(|approval| approval.id == approval_id);
        let approval = live.activity.approvals.remove(waiting.context("The agent no longer waits for that answer.")?);
        let thread = &live.stored.thread;
        let line = agents::answer(thread.agent, &approval, allow, &answers, thread.access);
        if line.is_some_and(|line| !live.write(line)) {
            bail!("The agent is no longer running.");
        }
        if allow && approval.tool_name == PLAN_TOOL {
            live.stored.thread.plan = false;
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
        let mut threads = self.threads.lock().await;
        let live = threads.get_mut(thread_id).context("That thread no longer exists.")?;
        let index = live.queued_index(message_id)?;
        if live.queued[index].sending {
            return Ok(());
        }
        if live.preparing.is_some() {
            bail!("The agent can't take it yet.");
        }
        let Some(run) = &live.run else {
            let queued = live.queued.remove(index);
            return self.start_next_turn(live, queued);
        };
        if !run.received_result {
            if !live.steer(index) {
                bail!("The agent can't take it yet.");
            }
            return Ok(());
        }
        if !live.give(index) {
            // The agent's process takes no more; the message starts the next turn.
            live.queued[index].held = false;
            live.send_activity();
            return Ok(());
        }
        self.resume(live)
    }

    /// Takes a queued message back.
    pub async fn cancel_queued(&self, thread_id: &str, message_id: &str) -> anyhow::Result<()> {
        let mut threads = self.threads.lock().await;
        let live = threads.get_mut(thread_id).context("That thread no longer exists.")?;
        let index = live.queued_index(message_id)?;
        if live.queued[index].sending {
            bail!("The agent has already been given that message.");
        }
        live.queued.remove(index);
        live.send_activity();
        Ok(())
    }

    fn announce_approvals(&self, live: &mut Live) -> anyhow::Result<()> {
        live.stored.thread.needs_approval = !live.activity.approvals.is_empty();
        self.store.save_thread(&live.stored)?;
        live.send_activity();
        self.announce(&live.stored.thread);
        Ok(())
    }

    pub async fn stop(self: &Arc<Self>, thread_id: &str) {
        let mut threads = self.threads.lock().await;
        let Some(live) = threads.get_mut(thread_id) else { return };
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
        let Some(run) = &live.run else { return };
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
                let threads = hub.threads.lock().await;
                let still_running = threads.get(&thread_id).and_then(|live| live.run.as_ref());
                if still_running.is_none_or(|run| run.process_id != process_id) {
                    return;
                }
                signal(process_id, signal_number);
            }
        });
    }

    pub async fn update(&self, thread_id: &str, change: ThreadChange) -> anyhow::Result<()> {
        let mut threads = self.threads.lock().await;
        let live = threads.get_mut(thread_id).context("That thread no longer exists.")?;
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
        let mode_changed = change.access.is_some() || change.plan.is_some();
        thread.access = change.access.unwrap_or(thread.access);
        thread.plan = change.plan.unwrap_or(thread.plan);
        if mode_changed {
            told.push(claude::access_line(thread.plan, thread.access));
        }
        match change.done {
            Some(true) if thread.running => bail!("A thread can't be marked done while it is working."),
            Some(true) if thread.monitoring => bail!("A thread can't be marked done while it is monitoring."),
            Some(true) => thread.done_at = thread.done_at.or_else(|| Some(now())),
            Some(false) if thread.done_at.is_some() => {
                thread.done_at = None;
                thread.undone_at = Some(now());
            }
            _ => {}
        }
        // Codex takes its settings when its next process starts.
        if thread.agent == Agent::Claude {
            for line in told {
                live.write(line);
            }
        }
        self.store.save_thread(&live.stored)?;
        self.announce(&live.stored.thread);
        Ok(())
    }

    pub async fn delete(&self, thread_id: &str) -> anyhow::Result<()> {
        let mut threads = self.threads.lock().await;
        let Some(live) = threads.remove(thread_id) else { return Ok(()) };
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
        self.threads.lock().await.values().any(|live| live.run.is_some() || live.preparing.is_some())
    }

    /// Answers with the project's id, also when the folder was a project already.
    pub async fn add_project(&self, path: &str) -> anyhow::Result<String> {
        let path = if path.len() > 1 { path.trim_end_matches('/') } else { path };
        if !Path::new(path).is_absolute() || !Path::new(path).is_dir() {
            bail!("{path} isn't a folder on the server.");
        }
        let mut projects = self.projects.lock().await;
        if let Some(project) = projects.iter().find(|project| project.path == path) {
            return Ok(project.id.clone());
        }
        let project = StoredProject {
            id: new_id(),
            path: path.to_string(),
            created_at: now(),
            icon: icons::find(Path::new(path)),
            icon_chosen: false,
            setup: None,
        };
        let id = project.id.clone();
        self.store.add_project(&project)?;
        projects.push(project);
        self.announce_projects(&projects);
        Ok(id)
    }

    /// Where new projects and clones go: `projects` in the home folder.
    fn projects_root(&self) -> anyhow::Result<PathBuf> {
        let home = self.environment.variables.get("HOME").context("The server doesn't know its home folder.")?;
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
                bail!("{} is already there. Pick another name, or add it as a local folder.", path.display());
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
        let mut projects = self.projects.lock().await;
        let project = projects
            .iter_mut()
            .find(|project| project.id == project_id)
            .context("That project is no longer on the server.")?;
        match path {
            Some(path) => {
                if icons::version(&path).is_none() {
                    bail!("{path} isn't an image of at most 1 MB.");
                }
                project.icon = Some(path);
                project.icon_chosen = true;
            }
            None => {
                project.icon = icons::find(Path::new(&project.path));
                project.icon_chosen = false;
            }
        }
        self.store.save_project_icon(project)?;
        self.announce_projects(&projects);
        Ok(())
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
            .context("That project is no longer on the server.")?;
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

    /// Checks a branch out in the project's folder, where its threads without a worktree work.
    pub async fn switch_branch(&self, project_id: &str, branch: &str, create: bool) -> anyhow::Result<()> {
        let path = self.project_path(project_id).await?;
        self.refuse_while_working(&path).await?;
        git::switch(&path, &self.environment, branch, create).await?;
        self.read_git(&path, false).await;
        self.announce_projects(&self.projects.lock().await);
        Ok(())
    }

    async fn refuse_while_working(&self, folder: &str) -> anyhow::Result<()> {
        let threads = self.threads.lock().await;
        let working = threads.values().any(|live| live.stored.thread.cwd == folder && live.activity.running);
        if working {
            bail!("An agent is working there. Switch branches when it has finished.");
        }
        Ok(())
    }

    /// What git says now about the project's folder, or about the thread's worktree, which every
    /// app is told when it has changed.
    pub async fn git_status(&self, project_id: &str, thread_id: Option<&str>, fetch: bool) -> anyhow::Result<Message> {
        let path = self.git_folder(project_id, thread_id).await?;
        if fetch {
            git::fetch(&path, &self.environment).await;
        }
        let (status, files) = self.read_git(&path, fetch).await;
        Ok(Message::GitStatus { status, files })
    }

    /// The changes in the folder the thread works in, or in the project's folder, as a patch.
    pub async fn diff(&self, project_id: &str, thread_id: Option<&str>, scope: DiffScope) -> anyhow::Result<Message> {
        let folder = self.git_folder(project_id, thread_id).await?;
        let environment = &self.environment;
        let (from, to) = match scope {
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
                        let threads = self.threads.lock().await;
                        threads
                            .get(thread_id)
                            .and_then(|live| live.stored.worktree.as_ref())
                            .map(|own| own.base.clone())
                    }
                    None => None,
                };
                git::since_branching(&folder, environment, base.as_deref()).await?
            }
        };
        let (patch, truncated) = git::patch_between(&folder, environment, &from, &to).await?;
        Ok(Message::Diff { patch, truncated })
    }

    /// What is in a folder inside the one the thread works in, or inside the project's.
    pub async fn list_files(&self, project_id: &str, thread_id: Option<&str>, path: &str) -> anyhow::Result<Message> {
        let folder = self.git_folder(project_id, thread_id).await?;
        files::list_files(&folder, path, &self.environment).await
    }

    /// A file inside the folder the thread works in, or inside the project's: what kind it is,
    /// its size and how many of its bytes to send.
    pub async fn open_file(
        &self,
        project_id: &str,
        thread_id: Option<&str>,
        path: &str,
    ) -> anyhow::Result<(tokio::fs::File, FileKind, u64, u64)> {
        let folder = self.git_folder(project_id, thread_id).await?;
        files::open_file(&folder, path).await
    }

    /// Where the thread works: in its worktree, or in the project's folder.
    async fn git_folder(&self, project_id: &str, thread_id: Option<&str>) -> anyhow::Result<String> {
        let worktree = thread_id.and_then(|thread_id| self.lock_worktrees().get(thread_id).map(|own| own.path.clone()));
        match worktree {
            Some(path) => Ok(path),
            None => self.project_path(project_id).await,
        }
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
                // On the default branch a merged one is another branch's history.
                (found.filter(|found| !found.merged || !status.default), Instant::now())
            }
            _ => (None, Instant::now()),
        };
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
        let threads = self.threads.lock().await;
        let Some(live) = thread_id.and_then(|thread_id| threads.get(thread_id)) else {
            let installed =
                [Agent::Claude, Agent::Codex].into_iter().find(|agent| self.environment.executable(*agent).is_some());
            let agent = installed.context("No agent is installed on your server to write this.")?;
            return Ok((self.writer(agent), drafts::Thread::default()));
        };
        let thread = &live.stored.thread;
        let items = self.store.items_since(&thread.id, 0).unwrap_or_default();
        let said = items.iter().filter_map(|item| match &item.kind {
            ItemKind::User { text, .. } if !text.is_empty() => Some(format!("USER:\n{text}")),
            _ => None,
        });
        let messages = said.collect::<Vec<_>>().join("\n\n");
        Ok((self.writer(thread.agent), drafts::Thread { title: thread.title.clone(), messages }))
    }

    /// Carries the action out in the project's folder, or in the worktree of the run's thread,
    /// telling `started` each stage as it starts, and answers with what it did.
    pub async fn git_run(&self, project_id: &str, run: GitRun, started: impl Fn(GitStage)) -> anyhow::Result<Message> {
        let path = self.git_folder(project_id, run.thread_id.as_deref()).await?;
        let done = self.git_stages(&path, &run, started).await;
        self.read_git(&path, true).await;
        done
    }

    async fn git_stages(&self, path: &str, run: &GitRun, started: impl Fn(GitStage)) -> anyhow::Result<Message> {
        let environment = &self.environment;
        let done = |title: String, description: Option<String>, url: Option<String>, next: Option<GitAction>| {
            Ok(Message::GitDone { title, description: description.filter(|text| !text.is_empty()), url, next })
        };
        if run.action == GitAction::Pull {
            started(GitStage::Pull);
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
                    started(if run.new_branch { GitStage::Branch } else { GitStage::Message });
                    let naming = self.branch_instructions().text;
                    Some(drafts::commit_message(path, environment, &writer, &thread, &run.paths, &naming).await?)
                }
            };
            if run.new_branch {
                let suggested = draft.as_ref().and_then(|draft| draft.branch.clone());
                self.branch_off(path, suggested).await?;
            }
            started(GitStage::Commit);
            let message = message.map(str::to_string).or(draft.map(|draft| draft.message())).unwrap_or_default();
            git::commit(path, environment, &message, &run.paths).await?;
        } else if run.action == GitAction::Commit {
            bail!("There is nothing to commit.");
        } else if run.new_branch {
            started(GitStage::Branch);
            let subject = git::head(path, environment).await?.1;
            self.branch_off(path, drafts::branch_name(&subject)).await?;
        }

        let (commit, subject) = git::head(path, environment).await?;
        let mut pushed = None;
        let status = git::status(path, environment).await.map(|read| read.0).unwrap_or(status);
        if pushes && (!status.upstream || status.ahead > 0) {
            started(GitStage::Push);
            git::push(path, environment).await?;
            pushed = git::upstream(path, environment).await;
        }
        if opens {
            if let Some(open) = git::pull_request(path, environment).await.filter(|found| !found.merged) {
                return done(format!("PR #{} is already open", open.number), Some(open.title), Some(open.url), None);
            }
            started(GitStage::PullRequestText);
            let (title, body) = drafts::pull_request(path, environment, &writer, &thread).await?;
            started(GitStage::PullRequest);
            let url = git::open_pull_request(path, environment, &title, &body).await?;
            let number = url.rsplit('/').next().unwrap_or_default();
            return done(format!("Created PR #{number}"), Some(title), Some(url), None);
        }
        if let Some(upstream) = pushed {
            let opened = self
                .lock_git()
                .get(path)
                .and_then(|read| read.status.pull_request.as_ref().map(|found| !found.merged))
                .unwrap_or(false);
            let next = (!status.default && status.pull_requests && !opened).then_some(GitAction::CreatePr);
            return done(format!("Pushed {commit} to {upstream}"), Some(subject), None, next);
        }
        if committed {
            return done(format!("Committed {commit}"), Some(subject), None, status.remote.then_some(GitAction::Push));
        }
        done("Already up to date".to_string(), None, None, None)
    }

    /// Makes a branch for the work from what is checked out and switches to it.
    async fn branch_off(&self, path: &str, suggested: Option<String>) -> anyhow::Result<()> {
        self.refuse_while_working(path).await?;
        let name = suggested.unwrap_or_else(|| "feature".to_string());
        let name = git::free_branch_name(path, &self.environment, &name).await;
        git::switch(path, &self.environment, &name, true).await
    }

    async fn project_path(&self, project_id: &str) -> anyhow::Result<String> {
        let projects = self.projects.lock().await;
        let project = projects.iter().find(|project| project.id == project_id);
        Ok(project.context("That project is no longer on the server.")?.path.clone())
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
            name: file_name(&stored.path).to_string(),
            branch: git::current_branch(&stored.path),
            git: status(&stored.path),
            icon: stored.icon.as_deref().and_then(icons::version),
            worktrees: worktrees_of(&stored.id),
            setup: stored.setup.clone(),
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
        let mut threads = self.threads.lock().await;
        let Some(live) = threads.get_mut(&thread_id) else { return };
        live.preparing = None;
        let started = match made {
            Ok(()) => self.start_agent(live, prompt),
            Err(error) => self.end_without_agent(live, Some(format!("{error:#}"))),
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
        let mut threads = self.threads.lock().await;
        let Some(live) = threads.get_mut(&thread_id) else { return };
        if let Err(error) = live.set_changes(&self.store, &item_id, TurnChanges { snapshot, files }) {
            tracing::error!(thread_id, "couldn't save a turn's changes: {error:#}");
        }
    }

    /// Makes the worktree on its branch, has the writer name a branch that is new, and runs the
    /// project's setup script there.
    async fn make_worktree(self: &Arc<Self>, thread_id: &str, message: &str) -> anyhow::Result<()> {
        let (project_id, path, worktree) = {
            let threads = self.threads.lock().await;
            let stored = &threads.get(thread_id).context("That thread no longer exists.")?.stored;
            let worktree = stored.worktree.clone().context("That thread has no worktree.")?;
            (stored.thread.project_id.clone(), stored.thread.cwd.clone(), worktree)
        };
        let (repository, setup) = {
            let projects = self.projects.lock().await;
            let project = projects.iter().find(|project| project.id == project_id);
            let project = project.context("That project is no longer on the server.")?;
            (project.path.clone(), project.setup.clone())
        };
        if let Some(parent) = Path::new(&path).parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("{} can't be made.", parent.display()))?;
        }
        git::add_worktree(&repository, &self.environment, &path, &worktree.branch, &worktree.base).await?;
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

    async fn show_setup(&self, thread_id: &str, call: &ToolCall) {
        let mut threads = self.threads.lock().await;
        let Some(live) = threads.get_mut(thread_id) else { return };
        if let Err(error) = live.upsert(&self.store, call.id.clone(), ItemKind::Tool { call: call.clone() }) {
            tracing::error!(thread_id, "couldn't save a thread update: {error:#}");
        }
    }

    /// Renames the branch made for the thread's worktree to what the writer calls the work.
    async fn name_branch(self: Arc<Self>, thread_id: String, message: String) {
        let Some(agent) = self.agent_of(&thread_id).await else { return };
        let instructions = self.branch_instructions().text;
        let name = match drafts::branch_for(&self.environment, &self.writer(agent), &instructions, &message).await {
            Ok(name) => name,
            Err(error) => return tracing::warn!(thread_id, "couldn't name a branch: {error:#}"),
        };
        let mut threads = self.threads.lock().await;
        let Some(live) = threads.get_mut(&thread_id) else { return };
        let Some(worktree) = live.stored.worktree.clone() else { return };
        let path = live.stored.thread.cwd.clone();
        // The agent may have checked out another branch by now.
        if git::current_branch(&path).as_deref() != Some(worktree.branch.as_str()) {
            return;
        }
        let Ok(repository) = self.project_path(&live.stored.thread.project_id).await else { return };
        let name = git::free_branch_name(&repository, &self.environment, &name).await;
        if let Err(error) = git::rename_branch(&repository, &self.environment, &worktree.branch, &name).await {
            return tracing::warn!(thread_id, "couldn't rename a branch: {error:#}");
        }
        if let Some(own) = self.lock_worktrees().get_mut(&thread_id) {
            own.branch = name.clone();
        }
        live.stored.worktree = Some(StoredWorktree { branch: name, ..worktree });
        if let Err(error) = self.store.save_thread(&live.stored) {
            tracing::error!(thread_id, "couldn't save a thread: {error:#}");
        }
        drop(threads);
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
        live.open.clear();
        for queued in &mut live.queued {
            queued.held = true;
        }
        self.store.save_thread(&live.stored)?;
        live.send_activity();
        self.announce(&live.stored.thread);
        Ok(())
    }

    fn announce(&self, thread: &Thread) {
        let _ = self.list_updates.send(Message::ThreadUpsert { thread: thread.clone() });
    }

    fn start_turn(self: &Arc<Self>, live: &mut Live, prompt: String) -> anyhow::Result<()> {
        let thread = &live.stored.thread;
        let makes_worktree = live.stored.worktree.is_some() && !Path::new(&thread.cwd).is_dir();
        if !makes_worktree && !git::in_repository(&thread.cwd) {
            return self.start_agent(live, prompt);
        }
        let preparing = self.clone().prepare_and_start(thread.id.clone(), thread.cwd.clone(), prompt, makes_worktree);
        self.announce_working(live)?;
        live.preparing = Some(Preparing { task: tokio::spawn(preparing).abort_handle(), makes_worktree });
        Ok(())
    }

    fn start_agent(self: &Arc<Self>, live: &mut Live, prompt: String) -> anyhow::Result<()> {
        let thread = &live.stored.thread;
        let agent = thread.agent;
        let turn = Turn {
            agent,
            model: thread.model.as_deref(),
            effort: thread.effort.as_deref(),
            access: thread.access,
            plan: thread.plan,
            session_id: live.stored.session_id.as_deref(),
        };
        let child = match self.spawn(&turn, &thread.cwd) {
            Ok(child) => child,
            Err(error) => return self.end_without_agent(live, Some(error.to_string())),
        };

        let interrupted = Arc::new(AtomicBool::new(false));
        let (input, lines) = mpsc::unbounded_channel();
        let prompt_id = new_id();
        let _ = input.send(agents::opening(agent, &prompt, &prompt_id));
        let parser = Parser::new(&turn, &thread.cwd, &prompt, &prompt_id);
        live.run = Some(Run {
            process_id: child.id().unwrap_or_default(),
            started: Instant::now(),
            interrupted: interrupted.clone(),
            received_result: false,
            input: Some(input),
            background: Background::default(),
            turn_id: None,
        });
        self.announce_working(live)?;

        tokio::spawn(self.clone().drive(live.stored.thread.id.clone(), child, lines, parser, interrupted));
        Ok(())
    }

    fn announce_working(&self, live: &mut Live) -> anyhow::Result<()> {
        let thread = &mut live.stored.thread;
        thread.running = true;
        thread.monitoring = false;
        thread.needs_approval = false;
        thread.updated_at = now();
        // New activity brings a done thread back.
        if thread.done_at.take().is_some() {
            thread.undone_at = Some(now());
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
        run.started = Instant::now();
        self.announce_working(live)
    }

    /// What the process does once its turn has ended: take the next message that waits, exit,
    /// keep working in the background, or monitor.
    fn rest(&self, live: &mut Live) -> anyhow::Result<()> {
        let Some(run) = live.run.as_ref().filter(|run| run.received_result) else { return Ok(()) };
        // What the agent asks once its turn has ended, it stays to hear the answer to.
        if !live.activity.approvals.is_empty() {
            return Ok(());
        }
        let interrupted = run.interrupted.load(Ordering::Relaxed);
        // A message the agent was given too late for this turn starts its next one.
        let given = live.queued.iter().any(|queued| queued.sending);
        if !interrupted && (given || live.hand_over()) {
            return self.resume(live);
        }
        let Some(run) = &mut live.run else { return Ok(()) };
        if run.background.is_empty() || interrupted {
            run.input = None;
            return Ok(());
        }
        if run.background.agents > 0 {
            return Ok(());
        }
        let thread = &mut live.stored.thread;
        thread.running = false;
        thread.monitoring = true;
        thread.updated_at = now();
        thread.turn_ended_at = Some(now());
        live.activity = Activity { monitoring: true, ..Activity::default() };
        live.flush(&self.store)?;
        self.store.save_thread(&live.stored)?;
        live.send_activity();
        self.announce(&live.stored.thread);
        Ok(())
    }

    fn spawn(&self, turn: &Turn, cwd: &str) -> anyhow::Result<Child> {
        let name = executable_name(turn.agent);
        let executable = self
            .environment
            .executable(turn.agent)
            .with_context(|| format!("{name} isn't installed on this server."))?;
        if !Path::new(cwd).is_dir() {
            bail!("The folder {cwd} no longer exists.");
        }
        let mut command = Command::new(executable);
        command
            .args(turn.arguments())
            .current_dir(cwd)
            .env_clear()
            .envs(&self.environment.variables)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Its own group, so stopping the turn also stops whatever the agent started.
            .process_group(0)
            .kill_on_drop(true);
        if unsafe { libc::getuid() } == 0 {
            command.env(crate::service::SANDBOX_VARIABLE, "1");
        }
        command.spawn().with_context(|| format!("{name} couldn't be started."))
    }

    async fn drive(
        self: Arc<Self>,
        thread_id: String,
        mut child: Child,
        mut lines: mpsc::UnboundedReceiver<String>,
        mut parser: Parser,
        interrupted: Arc<AtomicBool>,
    ) {
        if let Some(mut stdin) = child.stdin.take() {
            tokio::spawn(async move {
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
                let events = parser.parse(&line);
                if !events.is_empty() {
                    self.apply(&thread_id, events).await;
                }
            }
        }
        let exit_code = child.wait().await.ok().and_then(|status| status.code());
        let stderr = stderr.await.unwrap_or_default();
        self.finish_turn(&thread_id, exit_code, &stderr, interrupted.load(Ordering::Relaxed)).await;
    }

    async fn apply(self: &Arc<Self>, thread_id: &str, events: Vec<AgentEvent>) {
        let mut threads = self.threads.lock().await;
        let Some(live) = threads.get_mut(thread_id) else { return };
        let was_running = live.stored.thread.running;
        let ended = events.iter().any(|event| matches!(event, AgentEvent::Completed { .. }));
        for event in events {
            if let Err(error) = self.apply_event(live, event) {
                tracing::error!(thread_id, "couldn't save a thread update: {error:#}");
            }
        }
        // Decided once everything the agent said with the turn's end is known.
        if ended && let Err(error) = self.rest(live) {
            tracing::error!(thread_id, "couldn't save a thread update: {error:#}");
        }
        self.read_changes(live);
        if was_running && live.stored.thread.monitoring {
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
        );
        if !more_text {
            live.release_held(store)?;
        }
        match event {
            AgentEvent::Session { id } => {
                if live.stored.session_id.as_deref() != Some(&id) {
                    live.stored.session_id = Some(id);
                    store.save_thread(&live.stored)?;
                }
            }
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
                if !live.open.contains_key(&id) {
                    live.upsert(store, id.clone(), ItemKind::Tool { call: new_tool_call(id, name) })?;
                }
            }
            AgentEvent::ToolInput { id, name, input } => live.tool_input(store, id, name, input)?,
            AgentEvent::ToolResult { id, output, is_error } => live.tool_result(store, id, output, is_error)?,
            AgentEvent::Tool { call } => {
                live.set_thinking(false);
                live.upsert(store, call.id.clone(), ItemKind::Tool { call })?;
            }
            AgentEvent::Sub { parent, event } => {
                live.parent = Some(parent);
                let added = live.add_from_subagent(store, *event);
                live.parent = None;
                added?;
            }
            AgentEvent::Task { tool_id, agent } => {
                live.update_subagent(store, &tool_id, agent)?;
                self.count_agents(live);
            }
            AgentEvent::Compacting { active } => {
                if live.activity.compacting != active {
                    live.activity.compacting = active;
                    live.send_activity();
                }
            }
            AgentEvent::Completed { mut summary, result_text, preempted } => {
                live.set_thinking(false);
                // A turn stopped for the message it was given goes on with it.
                if preempted && live.steering() {
                    return Ok(());
                }
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
                    let message = result_text.filter(|text| !text.is_empty());
                    let message = message.unwrap_or_else(|| "The agent reported an error.".to_string());
                    live.append(store, ItemKind::Error { message })?;
                }
                live.activity.approvals.clear();
                live.stored.thread.needs_approval = false;
                live.end_turn(store, summary)?;
            }
            AgentEvent::Approval(approval) => {
                live.set_thinking(false);
                live.activity.approvals.push(approval);
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
            AgentEvent::Taken { id } => {
                let Some(index) = live.queued.iter().position(|queued| queued.id == id) else { return Ok(()) };
                let queued = live.queued.remove(index);
                live.append_message(store, queued.text, queued.attachments, queued.media)?;
                live.send_activity();
            }
            AgentEvent::Turn { id } => {
                if let Some(run) = &mut live.run {
                    run.turn_id = Some(id);
                }
            }
            AgentEvent::Write(lines) => {
                live.write(lines);
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

    /// Tells the apps how many of the agents the thread's agent started still work.
    fn count_agents(&self, live: &mut Live) {
        let working = |item: &&Item| matches!(&item.kind, ItemKind::Tool { call } if call.agent.as_ref().is_some_and(|agent| agent.status == ToolStatus::Running));
        let agents = live.open.values().filter(working).count() as u32;
        if live.stored.thread.agents == agents {
            return;
        }
        live.stored.thread.agents = agents;
        live.activity.agents = agents;
        live.send_activity();
        self.announce(&live.stored.thread);
    }

    async fn finish_turn(self: &Arc<Self>, thread_id: &str, exit_code: Option<i32>, stderr: &str, interrupted: bool) {
        let mut threads = self.threads.lock().await;
        let Some(live) = threads.get_mut(thread_id) else { return };
        let run = live.run.take();
        if let Err(error) = live.settle(&self.store, run.as_ref(), exit_code, stderr, interrupted) {
            tracing::error!(thread_id, "couldn't save the end of a turn: {error:#}");
        }
        self.read_changes(live);
        let thread = &mut live.stored.thread;
        // A thread that was monitoring has told of its turn's end already.
        if !thread.monitoring {
            thread.turn_ended_at = Some(now());
        }
        thread.running = false;
        thread.monitoring = false;
        thread.needs_approval = false;
        thread.agents = 0;
        thread.updated_at = now();
        live.activity = Activity::default();
        live.open.clear();

        // What the user stopped doesn't go on by itself: the messages that waited stay until
        // they are sent.
        for queued in &mut live.queued {
            queued.sending = false;
            queued.held |= interrupted;
        }
        if let Some(index) = live.queued.iter().position(|queued| !queued.held) {
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
        let prompt = prompt(&queued.text, &queued.attachments);
        live.append_message(&self.store, queued.text, queued.attachments, queued.media)?;
        self.start_turn(live, prompt)
    }

    async fn after_turn(self: &Arc<Self>, live: &mut Live) {
        if std::mem::take(&mut live.title_needs_refinement) && live.stored.title_source == TitleSource::Placeholder {
            tokio::spawn(self.clone().title_from_transcript(live.stored.thread.id.clone()));
        }
        // The turn may have switched branches, or made the project an icon.
        let mut projects = self.projects.lock().await;
        let Some(project) = projects.iter_mut().find(|project| project.id == live.stored.thread.project_id) else {
            return;
        };
        refresh_icon(&self.store, project);
        let (hub, path) = (self.clone(), live.stored.thread.cwd.clone());
        self.announce_projects(&projects);
        tokio::spawn(async move { hub.read_git(&path, false).await });
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
            queued: Vec::new(),
            title_needs_refinement: false,
        }
    }

    fn next_rev(&mut self) -> u64 {
        self.stored.thread.rev += 1;
        self.stored.thread.rev
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

    fn queued_index(&self, message_id: &str) -> anyhow::Result<usize> {
        let index = self.queued.iter().position(|queued| queued.id == message_id);
        index.context("That message is no longer waiting.")
    }

    /// A message was sent now to the turn that runs, and the agent hasn't taken it yet.
    fn steering(&self) -> bool {
        let stopped = self.run.as_ref().is_some_and(|run| run.interrupted.load(Ordering::Relaxed));
        !stopped && self.queued.iter().any(|queued| queued.sending)
    }

    /// Gives the idle process the first message that waits, unless it still has one to take or
    /// waits for an answer itself. `false` when nothing was given.
    fn hand_over(&mut self) -> bool {
        if self.queued.iter().any(|queued| queued.sending) || !self.activity.approvals.is_empty() {
            return false;
        }
        let Some(index) = self.queued.iter().position(|queued| !queued.held) else { return false };
        self.give(index)
    }

    /// Writes a prompt to the agent's process. `false` when it takes none.
    fn write_prompt(&self, prompt: &str, id: &str) -> bool {
        let line = agents::input(self.stored.thread.agent, self.stored.session_id.as_deref(), prompt, id);
        line.is_some_and(|line| self.write(line))
    }

    /// Writes the queued message to the idle process, which starts its next turn with it.
    /// `false` when it takes no more.
    fn give(&mut self, index: usize) -> bool {
        let queued = &self.queued[index];
        if !self.write_prompt(&prompt(&queued.text, &queued.attachments), &queued.id) {
            return false;
        }
        self.note_given(index);
        true
    }

    /// Writes the queued message to the turn that runs, which takes it at once. `false` when the
    /// agent can't take it.
    fn steer(&mut self, index: usize) -> bool {
        let queued = &self.queued[index];
        let thread = &self.stored.thread;
        let turn_id = self.run.as_ref().and_then(|run| run.turn_id.as_deref());
        let session_id = self.stored.session_id.as_deref();
        let line =
            agents::steer(thread.agent, session_id, turn_id, &prompt(&queued.text, &queued.attachments), &queued.id);
        if !line.is_some_and(|line| self.write(line)) {
            return false;
        }
        self.note_given(index);
        true
    }

    fn note_given(&mut self, index: usize) {
        self.queued[index].sending = true;
        self.queued[index].held = false;
        self.send_activity();
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

    /// Adds what an agent the thread's agent started said or did to that agent's transcript.
    fn add_from_subagent(&mut self, store: &Store, event: AgentEvent) -> anyhow::Result<()> {
        match event {
            AgentEvent::Text { id, text } if !text.is_empty() => self.upsert(store, id, ItemKind::Assistant { text }),
            AgentEvent::ThinkingText { id, text } => self.upsert(store, id, ItemKind::Thinking { text }),
            AgentEvent::ToolInput { id, name, input } => self.tool_input(store, id, name, input),
            AgentEvent::ToolResult { id, output, is_error } => self.tool_result(store, id, output, is_error),
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
        store.save_item(&self.stored.thread.id, &item)?;
        let _ = self.updates.send(Message::Items { items: vec![item] });
        Ok(())
    }

    /// Adds the item that ends a turn. What the turn changed joins it once that has been read.
    fn end_turn(&mut self, store: &Store, summary: TurnSummary) -> anyhow::Result<()> {
        let item = self.new_item(new_id(), ItemKind::TurnEnd { summary });
        store.save_item(&self.stored.thread.id, &item)?;
        self.ended = Some(item.id.clone());
        let _ = self.updates.send(Message::Items { items: vec![item] });
        Ok(())
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

    /// Adds a message of the user with the images and videos attached to it.
    fn append_message(
        &mut self,
        store: &Store,
        text: String,
        attachments: Vec<String>,
        media: Vec<Media>,
    ) -> anyhow::Result<()> {
        let mut item = self.new_item(new_id(), ItemKind::User { text, attachments });
        item.media = media;
        store.save_item(&self.stored.thread.id, &item)?;
        let _ = self.updates.send(Message::Items { items: vec![item] });
        Ok(())
    }

    fn new_item(&mut self, id: String, kind: ItemKind) -> Item {
        let seq = self.stored.next_seq;
        self.stored.next_seq += 1;
        Item { id, seq, rev: self.next_rev(), created_at: now(), media: Vec::new(), parent: self.parent.clone(), kind }
    }

    /// Adds the item, or replaces what the running turn said about it before.
    fn upsert(&mut self, store: &Store, id: String, kind: ItemKind) -> anyhow::Result<()> {
        let rev = self.next_rev();
        let mut item = match self.open.get_mut(&id) {
            Some(item) => {
                item.kind = kind;
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

    /// Copies the images and videos a reply shows that its item doesn't hold yet. `true` when
    /// the item holds more now.
    fn keep_media(&mut self, store: &Store, id: &str) -> anyhow::Result<bool> {
        let Some(item) = self.open.get_mut(id) else { return Ok(false) };
        let ItemKind::Assistant { text } = &item.kind else { return Ok(false) };
        let kept = self.media.capture_new(text, &self.stored.thread.cwd, &item.media);
        if kept.is_empty() {
            return Ok(false);
        }
        for media in &kept {
            store.save_media(&self.stored.thread.id, &media.id)?;
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

        // Tools and agents that never reported back were cut off.
        let working = |call: &ToolCall| {
            let agent_works = call.agent.as_ref().is_some_and(|agent| agent.status == ToolStatus::Running);
            call.status == ToolStatus::Running || agent_works
        };
        let cut_off: Vec<Item> = self
            .open
            .values()
            .filter(|item| matches!(&item.kind, ItemKind::Tool { call } if working(call)))
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
            if let Some(agent) = &mut call.agent {
                agent.status = ToolStatus::Failed;
            }
            self.upsert(store, item.id, ItemKind::Tool { call })?;
        }

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
        self.append(store, ItemKind::Error { message })
    }
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

/// Looks for the project's icon in its folder again, unless the user picked one that is still
/// there.
fn refresh_icon(store: &Store, project: &mut StoredProject) {
    let chosen_exists = project.icon_chosen && project.icon.as_deref().and_then(icons::version).is_some();
    if chosen_exists {
        return;
    }
    let found = icons::find(Path::new(&project.path));
    if !project.icon_chosen && found == project.icon {
        return;
    }
    project.icon = found;
    project.icon_chosen = false;
    if let Err(error) = store.save_project_icon(project) {
        tracing::error!(project_id = project.id, "couldn't save a project's icon: {error:#}");
    }
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

fn new_tool_call(id: String, name: String) -> ToolCall {
    ToolCall { id, name, input: "{}".to_string(), output: None, status: ToolStatus::Running, agent: None }
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

fn prompt(text: &str, attachments: &[String]) -> String {
    if attachments.is_empty() {
        return text.to_string();
    }
    let list: Vec<String> = attachments.iter().map(|path| format!("- {path}")).collect();
    let files = format!("Attached files:\n{}", list.join("\n"));
    if text.is_empty() { files } else { format!("{text}\n\n{files}") }
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
        assert_eq!(prompt("Look", &["/a".to_string(), "/b".to_string()]), "Look\n\nAttached files:\n- /a\n- /b");
        assert_eq!(prompt("", &["/a".to_string()]), "Attached files:\n- /a");
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
