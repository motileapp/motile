//! The live state of every thread: whether a turn is running, what it has produced so far, and
//! who is watching. A turn is one run of the agent's CLI; its events are applied here, saved, and
//! sent to everyone with the thread open. Claude Code's process is talked to while it runs: it
//! asks before a tool call that needs approval and is told the thread's changed settings. It
//! outlives the turn while it monitors something: it then takes the next prompts itself, and
//! starts turns of its own. A message sent while a turn runs is queued, and the agent is given
//! it after its next tool call, so it carries on in the same turn. Codex's process is asked for
//! the thread and its turn, and answered what it asks, in the same way.

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
    Activity, Agent, Item, ItemKind, Message, NewThread, Project, Queued, ServerInfo, Thread, ThreadChange, ToolCall,
    ToolStatus, TurnSummary,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, broadcast, mpsc};

use crate::agents::environment::Environment;
use crate::agents::{self, AgentEvent, Background, PLAN_TOOL, Parser, Turn, claude, executable_name};
use crate::media::MediaStore;
use crate::store::{Store, StoredProject, StoredThread, TitleSource};
use crate::{git, icons, pacing, title};

const UPDATES_BUFFER: usize = 4096;
const ROOT_BYPASS_REFUSAL: &str = "cannot be used with root/sudo privileges";
/// Streamed text is written to disk at most this often, and when its block ends.
const FLUSH_EVERY: Duration = Duration::from_secs(1);

pub struct Hub {
    store: Store,
    pub media: MediaStore,
    environment: Environment,
    threads: Mutex<HashMap<String, Live>>,
    /// Locked after `threads` when both are needed.
    projects: Mutex<Vec<StoredProject>>,
    list_updates: broadcast::Sender<Message>,
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
    run: Option<Run>,
    /// Messages sent while the agent was working, until it takes them.
    queued: Vec<Queued>,
    title_needs_refinement: bool,
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
    /// `media_folder` is where the images and videos that threads show are kept.
    pub fn new(store: Store, media_folder: PathBuf, environment: Environment) -> anyhow::Result<Arc<Self>> {
        let home = environment.variables.get("HOME").map(String::as_str).unwrap_or_default();
        let media = MediaStore::new(media_folder, home);
        let threads = store.load_threads()?;
        let live = |stored: StoredThread| (stored.thread.id.clone(), Live::new(stored, media.clone()));
        let threads = threads.into_iter().map(live).collect();
        let mut projects = store.load_projects()?;
        for project in &mut projects {
            refresh_icon(&store, project);
        }
        let projects = Mutex::new(projects);
        let (list_updates, _) = broadcast::channel(UPDATES_BUFFER);
        Ok(Arc::new(Self { store, media, environment, threads: Mutex::new(threads), projects, list_updates }))
    }

    pub fn server_info(&self) -> ServerInfo {
        ServerInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol: motile_protocol::PROTOCOL_VERSION,
            hostname: Environment::hostname(),
            home: self.environment.variables.get("HOME").cloned().unwrap_or_default(),
            agents: self.environment.agents(),
            models: self.environment.models().to_vec(),
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
                projects: projects(&self.projects.lock().await),
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
        let mut threads = self.threads.lock().await;
        let (thread_id, is_new) = match (thread_id, new_thread) {
            (Some(thread_id), _) => (thread_id, false),
            (None, Some(new_thread)) => {
                let stored = self.new_thread(new_thread, &text, &attachments).await?;
                self.store.save_thread(&stored)?;
                let thread_id = stored.thread.id.clone();
                threads.insert(thread_id.clone(), Live::new(stored, self.media.clone()));
                (thread_id, true)
            }
            (None, None) => bail!("No thread was given."),
        };
        let live = threads.get_mut(&thread_id).context("That thread no longer exists.")?;
        let prompt = prompt(&text, &attachments);
        if let Some(run) = &live.run {
            // An agent that is only monitoring takes the message right away.
            let idle = run.received_result;
            if !idle || !live.write_prompt(&prompt, &new_id()) {
                live.queued.push(Queued { id: new_id(), text, attachments, held: false, sending: false });
                live.send_activity();
                return Ok(thread_id);
            }
            live.append(&self.store, ItemKind::User { text, attachments })?;
            self.resume(live)?;
            return Ok(thread_id);
        }
        live.append(&self.store, ItemKind::User { text: text.clone(), attachments })?;
        self.start_turn(live, prompt)?;
        if is_new {
            tokio::spawn(self.clone().title_from_first_message(thread_id.clone(), text));
        }
        Ok(thread_id)
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
        let thread = Thread {
            id: new_id(),
            title: title::placeholder(text, attachments),
            project_id: project.id.clone(),
            cwd: project.path.clone(),
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
            turn_ended_at: None,
            rev: 0,
        };
        Ok(StoredThread { thread, session_id: None, title_source: TitleSource::Placeholder, next_seq: 0 })
    }

    async fn title_from_first_message(self: Arc<Self>, thread_id: String, text: String) {
        let Some(agent) = self.agent_of(&thread_id).await else { return };
        match title::from_first_message(&self.environment, agent, &text).await {
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
        let Some(generated) = title::from_transcript(&self.environment, agent, &previous_title, &items).await else {
            return;
        };
        self.set_generated_title(&thread_id, generated.title).await;
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

    /// Gives the agent a queued message without waiting for its next tool call.
    pub async fn send_queued(self: &Arc<Self>, thread_id: &str, message_id: &str) -> anyhow::Result<()> {
        let mut threads = self.threads.lock().await;
        let live = threads.get_mut(thread_id).context("That thread no longer exists.")?;
        let index = live.queued_index(message_id)?;
        if live.queued[index].sending {
            return Ok(());
        }
        let Some(run) = &live.run else {
            let queued = live.queued.remove(index);
            return self.start_next_turn(live, queued);
        };
        let idle = run.received_result;
        if !live.give(index) {
            // The agent's process takes no more; the message starts the next turn.
            live.queued[index].held = false;
            live.send_activity();
            return Ok(());
        }
        if idle {
            self.resume(live)?;
        }
        Ok(())
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
        let shown = self.store.media_of(thread_id)?;
        self.store.delete_thread(thread_id)?;
        for media_id in shown {
            if !self.store.shows_media(&media_id)? {
                self.media.remove(&media_id);
            }
        }
        let _ = self.list_updates.send(Message::ThreadDeleted { thread_id: thread_id.to_string() });
        Ok(())
    }

    pub async fn any_running(&self) -> bool {
        self.threads.lock().await.values().any(|live| live.run.is_some())
    }

    pub async fn add_project(&self, path: &str) -> anyhow::Result<()> {
        let path = if path.len() > 1 { path.trim_end_matches('/') } else { path };
        if !Path::new(path).is_absolute() || !Path::new(path).is_dir() {
            bail!("{path} isn't a folder on the server.");
        }
        let mut projects = self.projects.lock().await;
        if projects.iter().any(|project| project.path == path) {
            return Ok(());
        }
        let project = StoredProject {
            id: new_id(),
            path: path.to_string(),
            created_at: now(),
            icon: icons::find(Path::new(path)),
            icon_chosen: false,
        };
        self.store.add_project(&project)?;
        projects.push(project);
        self.announce_projects(&projects);
        Ok(())
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

    /// The branches of the project's repository. The project is announced again too, since the
    /// branch may have been switched in a terminal.
    pub async fn branches(&self, project_id: &str) -> anyhow::Result<Message> {
        let path = self.project_path(project_id).await?;
        let branches = git::branches(&path, &self.environment).await?;
        self.announce_projects(&self.projects.lock().await);
        Ok(Message::Branches { branches })
    }

    /// Checks a branch out in the project's folder, which every thread of the project works in.
    pub async fn switch_branch(&self, project_id: &str, branch: &str, create: bool) -> anyhow::Result<()> {
        let path = self.project_path(project_id).await?;
        let threads = self.threads.lock().await;
        let working = threads.values().any(|live| live.stored.thread.project_id == project_id && live.activity.running);
        drop(threads);
        if working {
            bail!("An agent is working in this project. Switch branches when it has finished.");
        }
        git::switch(&path, &self.environment, branch, create).await?;
        self.announce_projects(&self.projects.lock().await);
        Ok(())
    }

    async fn project_path(&self, project_id: &str) -> anyhow::Result<String> {
        let projects = self.projects.lock().await;
        let project = projects.iter().find(|project| project.id == project_id);
        Ok(project.context("That project is no longer on the server.")?.path.clone())
    }

    fn announce_projects(&self, stored: &[StoredProject]) {
        let _ = self.list_updates.send(Message::Projects { projects: projects(stored) });
    }

    fn announce(&self, thread: &Thread) {
        let _ = self.list_updates.send(Message::ThreadUpsert { thread: thread.clone() });
    }

    fn start_turn(self: &Arc<Self>, live: &mut Live, prompt: String) -> anyhow::Result<()> {
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
            Err(error) => {
                live.stored.thread.updated_at = now();
                live.append(&self.store, ItemKind::Error { message: error.to_string() })?;
                self.store.save_thread(&live.stored)?;
                self.announce(&live.stored.thread);
                return Ok(());
            }
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
        live.activity = Activity { running: true, started_at: Some(now()), ..Activity::default() };
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
        let Some(run) = &live.run else { return Ok(()) };
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
            AgentEvent::ToolInput { id, name, input } => {
                let mut call = live.tool_call(&id).unwrap_or_else(|| new_tool_call(id.clone(), name));
                call.input = input;
                live.upsert(store, id, ItemKind::Tool { call })?;
            }
            AgentEvent::ToolResult { id, output, is_error } => {
                let Some(mut call) = live.tool_call(&id) else { return Ok(()) };
                call.output = Some(output);
                call.status = if is_error { ToolStatus::Failed } else { ToolStatus::Succeeded };
                live.upsert(store, id, ItemKind::Tool { call })?;
                live.hand_over();
            }
            AgentEvent::Tool { call } => {
                live.set_thinking(false);
                let finished = call.status != ToolStatus::Running;
                live.upsert(store, call.id.clone(), ItemKind::Tool { call })?;
                if finished {
                    live.hand_over();
                }
            }
            AgentEvent::Completed { mut summary, result_text } => {
                live.set_thinking(false);
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
                live.append(store, ItemKind::TurnEnd { summary })?;
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
                live.append(store, ItemKind::User { text: queued.text, attachments: queued.attachments })?;
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

    async fn finish_turn(self: &Arc<Self>, thread_id: &str, exit_code: Option<i32>, stderr: &str, interrupted: bool) {
        let mut threads = self.threads.lock().await;
        let Some(live) = threads.get_mut(thread_id) else { return };
        let run = live.run.take();
        if let Err(error) = live.settle(&self.store, run.as_ref(), exit_code, stderr, interrupted) {
            tracing::error!(thread_id, "couldn't save the end of a turn: {error:#}");
        }
        let thread = &mut live.stored.thread;
        // A thread that was monitoring has told of its turn's end already.
        if !thread.monitoring {
            thread.turn_ended_at = Some(now());
        }
        thread.running = false;
        thread.monitoring = false;
        thread.needs_approval = false;
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
        live.append(&self.store, ItemKind::User { text: queued.text, attachments: queued.attachments })?;
        self.start_turn(live, prompt)
    }

    async fn after_turn(self: &Arc<Self>, live: &mut Live) {
        if std::mem::take(&mut live.title_needs_refinement) && live.stored.title_source == TitleSource::Placeholder {
            tokio::spawn(self.clone().title_from_transcript(live.stored.thread.id.clone()));
        }
        // The turn may have switched branches, or made the project an icon.
        let mut projects = self.projects.lock().await;
        if let Some(project) = projects.iter_mut().find(|project| project.id == live.stored.thread.project_id) {
            refresh_icon(&self.store, project);
        }
        self.announce_projects(&projects);
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
            run: None,
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

    /// Gives the agent the first message that waits for its turn, unless it still has one to
    /// take or waits for an answer itself. `false` when nothing was given.
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

    /// Writes the queued message to the agent's process. `false` when it takes no more.
    fn give(&mut self, index: usize) -> bool {
        let queued = &self.queued[index];
        if !self.write_prompt(&prompt(&queued.text, &queued.attachments), &queued.id) {
            return false;
        }
        self.queued[index].sending = true;
        self.queued[index].held = false;
        self.send_activity();
        true
    }

    /// Writes a line to the process's stdin. `false` when it takes no more.
    fn write(&self, line: String) -> bool {
        let input = self.run.as_ref().and_then(|run| run.input.as_ref());
        input.is_some_and(|input| input.send(line).is_ok())
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

    fn new_item(&mut self, id: String, kind: ItemKind) -> Item {
        let seq = self.stored.next_seq;
        self.stored.next_seq += 1;
        Item { id, seq, rev: self.next_rev(), created_at: now(), media: Vec::new(), kind }
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

        // Tools that never reported back were cut off.
        let cut_off: Vec<(String, ToolCall)> = self
            .open
            .values()
            .filter_map(|item| match &item.kind {
                ItemKind::Tool { call } if call.status == ToolStatus::Running => Some((item.id.clone(), call.clone())),
                _ => None,
            })
            .collect();
        for (id, mut call) in cut_off {
            call.status = ToolStatus::Failed;
            if interrupted {
                call.output.get_or_insert_with(|| "Interrupted".to_string());
            }
            self.upsert(store, id, ItemKind::Tool { call })?;
        }

        let received_result = run.is_some_and(|run| run.received_result);
        if interrupted {
            if received_result {
                return Ok(());
            }
            let duration_ms = run.map(|run| run.started.elapsed().as_millis() as u64);
            let summary = TurnSummary { duration_ms, stopped: true, ..TurnSummary::default() };
            return self.append(store, ItemKind::TurnEnd { summary });
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

fn projects(stored: &[StoredProject]) -> Vec<Project> {
    let project = |stored: &StoredProject| Project {
        id: stored.id.clone(),
        path: stored.path.clone(),
        name: file_name(&stored.path).to_string(),
        branch: git::current_branch(&stored.path),
        icon: stored.icon.as_deref().and_then(icons::version),
        created_at: stored.created_at,
    };
    stored.iter().map(project).collect()
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

fn new_tool_call(id: String, name: String) -> ToolCall {
    ToolCall { id, name, input: "{}".to_string(), output: None, status: ToolStatus::Running }
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
