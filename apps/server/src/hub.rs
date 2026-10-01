//! The live state of every thread: whether a turn is running, what it has produced so far, and
//! who is watching. A turn is one run of the agent's CLI; its events are applied here, saved, and
//! sent to everyone with the thread open.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use motile_protocol::now;
use motile_protocol::wire::{
    Activity, Agent, Denial, HostInfo, Item, ItemKind, Message, NewThread, Project, Thread, ThreadChange, ToolCall,
    ToolStatus, TurnSummary,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, broadcast};

use crate::agents::environment::Environment;
use crate::agents::{AgentEvent, Parser, Turn, claude, executable_name};
use crate::store::{Store, StoredProject, StoredThread, TitleSource};
use crate::title;

const UPDATES_BUFFER: usize = 4096;
const ROOT_BYPASS_REFUSAL: &str = "cannot be used with root/sudo privileges";
/// Streamed text is written to disk at most this often, and when its block ends.
const FLUSH_EVERY: Duration = Duration::from_secs(1);

pub struct Hub {
    store: Store,
    environment: Environment,
    threads: Mutex<HashMap<String, Live>>,
    /// Locked after `threads` when both are needed.
    projects: Mutex<Vec<StoredProject>>,
    list_updates: broadcast::Sender<Message>,
}

struct Live {
    stored: StoredThread,
    /// Items the running turn may still change, by id. Everything else is only on disk.
    open: HashMap<String, Item>,
    /// Open items whose latest text isn't on disk yet.
    unsaved: HashSet<String>,
    last_flush: Instant,
    activity: Activity,
    updates: broadcast::Sender<Message>,
    run: Option<Run>,
    /// Prompts sent while a turn was running; they start the next one.
    queued: Vec<String>,
    title_needs_refinement: bool,
}

struct Run {
    process_id: u32,
    started: Instant,
    interrupted: Arc<AtomicBool>,
    received_result: bool,
    allowed_tools: Vec<String>,
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
    pub fn new(store: Store, environment: Environment) -> anyhow::Result<Arc<Self>> {
        let threads = store.load_threads()?;
        let threads = threads.into_iter().map(|stored| (stored.thread.id.clone(), Live::new(stored))).collect();
        let projects = Mutex::new(store.load_projects()?);
        let (list_updates, _) = broadcast::channel(UPDATES_BUFFER);
        Ok(Arc::new(Self { store, environment, threads: Mutex::new(threads), projects, list_updates }))
    }

    pub fn host_info(&self) -> HostInfo {
        HostInfo {
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
                host: self.host_info(),
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
        // An app ahead of the host has a copy from before the host's data was replaced.
        let reset = since > rev;
        let items = self.store.items_since(thread_id, if reset { 0 } else { since })?;
        Ok(ThreadSubscription { reset, activity: live.activity, items, rev, updates: live.updates.subscribe() })
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
                threads.insert(thread_id.clone(), Live::new(stored));
                (thread_id, true)
            }
            (None, None) => bail!("No thread was given."),
        };
        let live = threads.get_mut(&thread_id).context("That thread no longer exists.")?;
        let prompt = prompt(&text, &attachments);
        live.append(&self.store, ItemKind::User { text: text.clone(), attachments })?;
        if live.stored.thread.running {
            live.queued.push(prompt);
            return Ok(thread_id);
        }
        self.start_turn(live, prompt, Vec::new())?;
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
            .context("That project is no longer on the host.")?;
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

    pub async fn allow(self: &Arc<Self>, thread_id: &str, denials: Vec<Denial>) -> anyhow::Result<()> {
        if denials.is_empty() {
            bail!("There is nothing to allow.");
        }
        let mut threads = self.threads.lock().await;
        let live = threads.get_mut(thread_id).context("That thread no longer exists.")?;
        if live.stored.thread.running {
            bail!("This thread is already running a turn.");
        }
        let mut rules: Vec<String> = denials.iter().map(claude::allow_rule).collect();
        rules.sort();
        rules.dedup();
        let mut names: Vec<&str> = denials.iter().map(|denial| denial.tool_name.as_str()).collect();
        names.sort();
        names.dedup();
        let text = format!("I've allowed {}. Please continue.", names.join(", "));
        live.append(&self.store, ItemKind::User { text: text.clone(), attachments: Vec::new() })?;
        self.start_turn(live, text, rules)
    }

    pub async fn stop(self: &Arc<Self>, thread_id: &str) {
        let threads = self.threads.lock().await;
        let Some(run) = threads.get(thread_id).and_then(|live| live.run.as_ref()) else { return };
        run.interrupted.store(true, Ordering::Relaxed);
        let process_id = run.process_id;
        signal(process_id, libc::SIGINT);

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
        if let Some(model) = change.model {
            thread.model = checked("model", Some(model))?;
        }
        if let Some(effort) = change.effort {
            thread.effort = checked("effort", Some(effort))?;
        }
        if let Some(access) = change.access {
            thread.access = access;
        }
        if let Some(plan) = change.plan {
            thread.plan = plan;
        }
        match change.done {
            Some(true) if thread.running => bail!("A thread can't be marked done while it is working."),
            Some(true) => thread.done_at = thread.done_at.or_else(|| Some(now())),
            Some(false) if thread.done_at.is_some() => {
                thread.done_at = None;
                thread.undone_at = Some(now());
            }
            _ => {}
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
        self.store.delete_thread(thread_id)?;
        let _ = self.list_updates.send(Message::ThreadDeleted { thread_id: thread_id.to_string() });
        Ok(())
    }

    pub async fn add_project(&self, path: &str) -> anyhow::Result<()> {
        let path = if path.len() > 1 { path.trim_end_matches('/') } else { path };
        if !Path::new(path).is_absolute() || !Path::new(path).is_dir() {
            bail!("{path} isn't a folder on the host.");
        }
        let mut projects = self.projects.lock().await;
        if projects.iter().any(|project| project.path == path) {
            return Ok(());
        }
        let project = StoredProject { id: new_id(), path: path.to_string(), created_at: now() };
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

    fn announce_projects(&self, stored: &[StoredProject]) {
        let _ = self.list_updates.send(Message::Projects { projects: projects(stored) });
    }

    fn announce(&self, thread: &Thread) {
        let _ = self.list_updates.send(Message::ThreadUpsert { thread: thread.clone() });
    }

    fn start_turn(self: &Arc<Self>, live: &mut Live, prompt: String, allowed_tools: Vec<String>) -> anyhow::Result<()> {
        let thread = &live.stored.thread;
        let turn = Turn {
            agent: thread.agent,
            model: thread.model.as_deref(),
            effort: thread.effort.as_deref(),
            access: thread.access,
            plan: thread.plan,
            session_id: live.stored.session_id.as_deref(),
            allowed_tools: &allowed_tools,
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
        live.run = Some(Run {
            process_id: child.id().unwrap_or_default(),
            started: Instant::now(),
            interrupted: interrupted.clone(),
            received_result: false,
            allowed_tools,
        });
        let thread = &mut live.stored.thread;
        thread.running = true;
        thread.needs_approval = false;
        thread.updated_at = now();
        // New activity brings a done thread back.
        if thread.done_at.take().is_some() {
            thread.undone_at = Some(now());
        }
        live.activity = Activity { running: true, thinking: false, started_at: Some(now()) };
        self.store.save_thread(&live.stored)?;
        live.send_activity();
        self.announce(&live.stored.thread);

        let parser = Parser::new(live.stored.thread.agent);
        tokio::spawn(self.clone().drive(live.stored.thread.id.clone(), child, prompt, parser, interrupted));
        Ok(())
    }

    fn spawn(&self, turn: &Turn, cwd: &str) -> anyhow::Result<Child> {
        let name = executable_name(turn.agent);
        let executable =
            self.environment.executable(turn.agent).with_context(|| format!("{name} isn't installed on this host."))?;
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
            command.env(crate::setup::SANDBOX_VARIABLE, "1");
        }
        command.spawn().with_context(|| format!("{name} couldn't be started."))
    }

    async fn drive(
        self: Arc<Self>,
        thread_id: String,
        mut child: Child,
        prompt: String,
        mut parser: Parser,
        interrupted: Arc<AtomicBool>,
    ) {
        if let Some(mut stdin) = child.stdin.take() {
            tokio::spawn(async move {
                let _ = stdin.write_all(prompt.as_bytes()).await;
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

    async fn apply(&self, thread_id: &str, events: Vec<AgentEvent>) {
        let mut threads = self.threads.lock().await;
        let Some(live) = threads.get_mut(thread_id) else { return };
        for event in events {
            if let Err(error) = self.apply_event(live, event) {
                tracing::error!(thread_id, "couldn't save a thread update: {error:#}");
            }
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
        match event {
            AgentEvent::Session { id } => {
                if live.stored.session_id.as_deref() != Some(&id) {
                    live.stored.session_id = Some(id);
                    store.save_thread(&live.stored)?;
                }
            }
            AgentEvent::TextStarted { .. } => live.set_thinking(false),
            AgentEvent::TextDelta { id, text } => live.append_text(store, id, text)?,
            AgentEvent::Text { id, text } => {
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
            }
            AgentEvent::Tool { call } => {
                live.set_thinking(false);
                live.upsert(store, call.id.clone(), ItemKind::Tool { call })?;
            }
            AgentEvent::Completed { mut summary, result_text } => {
                live.set_thinking(false);
                if let Some(run) = &mut live.run {
                    run.received_result = true;
                    summary.duration_ms = Some(run.started.elapsed().as_millis() as u64);
                }
                if summary.is_error {
                    let message = result_text.filter(|text| !text.is_empty());
                    let message = message.unwrap_or_else(|| "The agent reported an error.".to_string());
                    live.append(store, ItemKind::Error { message })?;
                }
                live.stored.thread.needs_approval = !summary.denials.is_empty();
                live.append(store, ItemKind::TurnEnd { summary })?;
            }
            AgentEvent::Failed { message } => {
                if let Some(run) = &mut live.run {
                    run.received_result = true;
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
        thread.running = false;
        thread.updated_at = now();
        thread.turn_ended_at = Some(now());
        live.activity = Activity::default();
        live.open.clear();

        if !live.queued.is_empty() {
            let prompt = std::mem::take(&mut live.queued).join("\n\n");
            let allowed_tools = run.map(|run| run.allowed_tools).unwrap_or_default();
            if let Err(error) = self.start_turn(live, prompt, allowed_tools) {
                tracing::error!(thread_id, "couldn't start the next turn: {error:#}");
            }
            return;
        }
        if let Err(error) = self.store.save_thread(&live.stored) {
            tracing::error!(thread_id, "couldn't save a thread: {error:#}");
        }
        live.send_activity();
        self.announce(&live.stored.thread);
        if std::mem::take(&mut live.title_needs_refinement) && live.stored.title_source == TitleSource::Placeholder {
            tokio::spawn(self.clone().title_from_transcript(thread_id.to_string()));
        }
        // The turn may have switched branches.
        self.announce_projects(&self.projects.lock().await);
    }
}

impl Live {
    fn new(stored: StoredThread) -> Self {
        let (updates, _) = broadcast::channel(UPDATES_BUFFER);
        Self {
            stored,
            open: HashMap::new(),
            unsaved: HashSet::new(),
            last_flush: Instant::now(),
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

    fn send_activity(&self) {
        let _ = self.updates.send(Message::Activity { activity: self.activity });
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
        Item { id, seq, rev: self.next_rev(), created_at: now(), kind }
    }

    /// Adds the item, or replaces what the running turn said about it before.
    fn upsert(&mut self, store: &Store, id: String, kind: ItemKind) -> anyhow::Result<()> {
        let rev = self.next_rev();
        let item = match self.open.get_mut(&id) {
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
        self.unsaved.remove(&id);
        store.save_item(&self.stored.thread.id, &item)?;
        let _ = self.updates.send(Message::Items { items: vec![item] });
        Ok(())
    }

    fn append_text(&mut self, store: &Store, id: String, text: String) -> anyhow::Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        self.set_thinking(false);
        let Some(Item { kind: ItemKind::Assistant { text: current }, .. }) = self.open.get_mut(&id) else {
            return self.upsert(store, id, ItemKind::Assistant { text });
        };
        current.push_str(&text);
        let rev = self.next_rev();
        if let Some(item) = self.open.get_mut(&id) {
            item.rev = rev;
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
                "Claude Code refuses full access when the host runs as root. Run `motile setup` again on the host \
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
        branch: git_branch(&stored.path),
        created_at: stored.created_at,
    };
    stored.iter().map(project).collect()
}

/// The branch checked out in the folder, read from git's own files.
fn git_branch(path: &str) -> Option<String> {
    let dot_git = Path::new(path).join(".git");
    // In a worktree `.git` is a file that points at the real folder.
    let git_dir = match std::fs::read_to_string(&dot_git) {
        Ok(pointer) => Path::new(path).join(pointer.trim().strip_prefix("gitdir:")?.trim()),
        Err(_) => dot_git,
    };
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref: refs/heads/") {
        Some(branch) => Some(branch.to_string()),
        None => Some(head.chars().take(7).collect()),
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
