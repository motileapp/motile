//! Everything the views show. It mirrors the core: commands go to it, and its events change the
//! state here. All of it is used on the main thread.

mod account;
mod attachments;
mod commands;
mod git;
pub mod integrations;
pub mod linear;
mod projects;
pub mod pull_requests;
pub mod ricrac;
pub mod settings;
pub mod shortcuts;
mod sidebar;
pub mod skills;
mod threads;
pub mod usage;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use futures::StreamExt;
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::{AccountView, Command, Config, Event};
use motile_core::render::agents::AgentView;
use motile_protocol::wire::{
    Access, AgentAccount, Branch, ChangedFile, GitHubState, GitStage, ModelInfo, Repo, Request,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use self::git::{Notice, PendingGit, stage_label};
pub use self::projects::RepoListing;
pub use self::pull_requests::PullRequests;
pub use self::settings::SettingsState;
pub use self::shortcuts::Shortcuts;
pub use self::sidebar::{SearchResult, SidebarState};
pub use crate::bridge::CoreError;
use crate::bridge::{Bridge, Incoming, Read};
use crate::models::*;
use crate::panel::state::{PanelTabs, PanelTarget, RightPanel};
use crate::prefs::Prefs;
use crate::transcript::model::{Prepared, Transcript};
use crate::updater::Updater;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Selection {
    Draft(String),
    Thread(String),
}

/// A thread that hasn't been sent yet: where it will start and with what. Its text is in `drafts`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ThreadDraft {
    pub id: String,
    pub project_id: Option<String>,
    pub model: Option<String>,
    /// The id of the account of the model's agent the thread starts with.
    #[serde(default)]
    pub agent_account: Option<String>,
    pub effort: Option<String>,
    pub access: Access,
    /// The thread starts in a new worktree, on a branch that starts from `base`.
    pub worktree: Option<bool>,
    pub base: Option<String>,
    /// The thread of a removed server the new thread goes on from.
    #[serde(default)]
    pub from: Option<Continuation>,
}

/// A thread of a removed server, which a new thread on another server goes on from.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Continuation {
    pub origin: String,
    pub thread_id: String,
    pub title: String,
}

impl ThreadDraft {
    fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string().to_uppercase(),
            project_id: None,
            model: None,
            agent_account: None,
            effort: None,
            access: Access::Full,
            worktree: None,
            base: None,
            from: None,
        }
    }
}

/// A draft as the sidebar lists it.
#[derive(Clone, Debug)]
pub struct ListedDraft {
    pub draft: ThreadDraft,
    pub preview: String,
    /// Its first message is on its way to the server.
    pub sending: bool,
}

/// The first message of a draft, on its way to the server.
#[derive(Clone, Debug)]
pub struct SendingDraft {
    pub text: String,
    pub attachments: Vec<AttachedFile>,
    pub activity: Activity,
    /// The client has gone elsewhere since, so the sidebar lists the draft.
    pub left: bool,
}

impl SendingDraft {
    fn preview(&self) -> String {
        preview_of(&self.text, self.attachments.len()).unwrap_or_default()
    }
}

/// The first line of what was written, or what is attached. Nothing for a draft that is empty.
fn preview_of(text: &str, attachments: usize) -> Option<String> {
    if let Some(line) = text.trim().split('\n').find(|line| !line.is_empty()) {
        return Some(line.to_string());
    }
    match attachments {
        0 => None,
        1 => Some("1 attachment".into()),
        count => Some(format!("{count} attachments")),
    }
}

/// Where the command panel opens.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum PanelPage {
    /// Everything that can be done from here.
    Commands,
    /// The projects, to start a thread in one.
    Projects,
    /// The projects, to choose the one the open draft starts in.
    DraftProject,
    /// The threads, to open one.
    Threads,
    /// The servers, to add a project on one.
    Servers,
    /// The ways to add a project on the server.
    Sources(String),
    NewProject(String),
    /// The repositories of the server's GitHub login, to clone one.
    Github(String),
    /// What GitHub needs on the server before its repositories can be listed.
    GithubSetup(String),
    /// The server's folders, to add one.
    Folder(String),
    /// The folders and images on the project's server, to choose its icon.
    Icon(String),
}

/// A project that a server is making or cloning.
#[derive(Clone, PartialEq, Debug)]
pub struct AddingProject {
    pub server_id: String,
    pub name: String,
}

/// What to do with a project that was just made or cloned, once its server has told about it.
type AwaitedProject = Box<dyn FnOnce(&mut Store, Project, &mut Context<Store>)>;

/// What the images and videos fetched from the servers take on this device, and what they may take.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MediaStorage {
    pub used: u64,
    pub limit: u64,
}

#[derive(Clone, PartialEq, Debug)]
pub struct UndoNotice {
    pub thread_ids: Vec<String>,
    pub text: String,
}

pub type Reply = Result<Value, CoreError>;
type Answer = Box<dyn FnOnce(&mut Store, Reply, &mut Context<Store>)>;
type ReadAnswer = Box<dyn FnOnce(&mut Store, Result<Read, CoreError>, &mut Context<Store>)>;

pub struct Store {
    bridge: Option<Bridge>,
    answers: HashMap<u64, Answer>,
    read_answers: HashMap<u64, ReadAnswer>,
    pub prefs: Prefs,
    pub data_dir: PathBuf,

    // Account
    pub account: AccountView,
    /// Whether the core has sent what it remembers from last time. Until then the window is empty.
    pub ready: bool,
    pub signing_in: bool,
    pub sign_in_error: Option<String>,
    pub enroll_token: Option<EnrollToken>,
    /// A new install command has been asked for.
    pub enroll_token_pending: bool,
    pub shows_add_server: bool,
    add_server_count: usize,
    /// The project the user asked to remove, until they say whether to.
    pub removing_project: Option<Project>,
    /// The browser sheet of a sign-in that is open.
    pub sign_in_session: Option<crate::sign_in::Session>,
    /// The branch picker under the composer is open. Its branches are `None` until the server
    /// has listed them.
    pub shows_branches: bool,
    pub listed_branches: Option<Result<Vec<Branch>, String>>,
    branches_project_id: Option<String>,
    /// Each project's branches as last listed, which the picker opens on while it lists them again.
    known_branches: HashMap<String, Vec<Branch>>,
    /// The project whose changes the commit sheet is open on, with the files it was opened with.
    pub committing_project: Option<Project>,
    pub git_files: Vec<ChangedFile>,
    /// An action that pushes from the default branch, until the user says where it should happen.
    pub pending_git: Option<PendingGit>,
    /// The stage a project's git action is at, by project.
    pub git_stages: HashMap<String, GitStage>,
    pub notice: Option<Notice>,
    /// The projects whose folders their servers are making git repositories.
    pub initializing_git: HashSet<String>,
    /// The `draft_worktree_key`s whose base is being pulled.
    pulling_bases: HashSet<String>,
    /// What a new worktree starts at, like `origin/main`, by `draft_worktree_key`.
    worktree_starts: HashMap<String, String>,
    pub panel: Option<PanelPage>,
    /// What a server refused while the panel was adding a project.
    pub panel_notice: Option<String>,
    /// Whether GitHub can be used on each server, as last heard.
    pub github: HashMap<String, GitHubState>,
    /// The GitHub repositories each server last listed, and why one couldn't.
    pub repos: HashMap<String, Vec<Repo>>,
    pub repo_errors: HashMap<String, String>,
    pub adding_project: Option<AddingProject>,
    /// Counts up when a project has been made or cloned.
    pub projects_added: u64,
    /// The project just made or cloned, and what to do with it once the server has told about it.
    awaited_project: Option<(String, AwaitedProject)>,
    /// A folder added as a project, by its server and path, until the server tells about it.
    awaited_folder: Option<(String, String)>,
    /// Counts up when the composer should take the keyboard back.
    pub composer_focus: u64,
    /// Counts up when the composer should let go of the keyboard.
    pub blur_composer: u64,

    // What the servers hold
    pub servers: Vec<Server>,
    server_updates: HashMap<String, ServerUpdate>,
    /// The folders the user added. "No project" of each server is in `no_projects`.
    pub projects: Vec<Project>,
    /// The project of each server that threads start in without one, in the servers' order.
    pub no_projects: Vec<Project>,
    pub threads: HashMap<String, ThreadInfo>,
    /// The active threads and the done ones, each in the order the sidebar lists them.
    active: Vec<ThreadInfo>,
    done: Vec<ThreadInfo>,

    // The open thread
    /// Always a draft or a thread; `load_preferences` opens the first draft.
    pub selection: Selection,
    pub activity: Activity,
    pub transcript_is_empty: bool,
    /// The agents the open thread's agent has started, in the order it started them.
    pub agents: Vec<AgentView>,
    /// The drafts whose first message is on its way to the server, by draft.
    pub sending_drafts: HashMap<String, SendingDraft>,
    pub error_message: Option<String>,
    pub thread_drafts: Vec<ThreadDraft>,
    /// What the open draft said when it was opened, if it said anything. Its row in the sidebar
    /// shows this, so the sidebar doesn't change while the draft is being written.
    pub opened_draft_preview: Option<String>,
    pub undo: Option<UndoNotice>,
    undo_task: Option<Task<()>>,
    /// The account's picture as a file on this device, once it has been fetched.
    pub account_picture: Option<PathBuf>,
    /// Unknown until Settings asks for it.
    pub media_storage: Option<MediaStorage>,
    drafts: HashMap<String, String>,
    attachments_by_key: HashMap<String, Vec<Attachment>>,
    /// The images and videos the viewer has open over the window.
    pub viewing: Option<Viewing>,
    /// How far each image or video that is being fetched has arrived.
    pub media_progress: HashMap<String, f64>,

    /// Counts up when the folder the open thread works in may have changed: a turn ended there,
    /// or git did something.
    pub workspace_version: u64,
    /// Counts up when the text of what is being written changed outside the composer.
    pub draft_version: u64,
    /// The skills the composer offers for the word typed before its cursor.
    pub skills: skills::Skills,

    pub updater: Updater,
    pub right_panel: RightPanel,
    pub settings: SettingsState,
    pub linear: linear::Linear,
    pub ricrac: ricrac::Ricrac,
    pub pull_requests: PullRequests,
    pub repo_listing: RepoListing,
    pub sidebar: SidebarState,
    pub transcript: Entity<Transcript>,
    /// What the agent did that the right panel shows.
    pub agent_transcript: Entity<Transcript>,
    open_thread_id: Option<String>,
    /// The thread that was open when the app was last closed, until it has been opened again.
    last_selection: Option<String>,
    /// The draft whose server wasn't known yet when it was given its server's last choices.
    draft_awaiting_server: Option<String>,
    /// Whether the app is in front.
    pub app_active: bool,
    /// Whether the window fills the screen, which the menu bar says how to leave.
    pub full_screen: bool,
    pub shortcuts: Shortcuts,
    /// The shortcut field that records a key, so every key is that.
    pub recording_shortcut: Option<SharedString>,
    /// The key a field recorded, until the view it is in takes it. Nothing when it was stopped.
    pub recorded_shortcut: Option<(SharedString, Option<String>)>,
    /// The modifiers of the threads' keys are held, so the sidebar shows the keys.
    pub shows_jump_hints: bool,
    jump_hint_task: Option<Task<()>>,
    /// The usage is open over the window, as a route of its own.
    pub shows_usage: bool,
    pub usage: usage::Usage,
    _events: Option<Task<()>>,
}

impl Store {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let data_dir = data_dir();
        let prefs_file =
            std::env::var("MOTILE_PREFS").map(PathBuf::from).unwrap_or_else(|_| data_dir.join("preferences.json"));
        let prefs = Prefs::load(prefs_file);
        let right_panel = RightPanel::new(
            prefs.bool("panel.open"),
            prefs.get::<BTreeMap<String, PanelTabs>>("panel.tabs").unwrap_or_default(),
        );
        let linear = linear::Linear::kept(&prefs);
        let ricrac = ricrac::Ricrac::kept(&prefs);
        let mut store = Self {
            bridge: None,
            answers: HashMap::new(),
            read_answers: HashMap::new(),
            prefs,
            data_dir,
            account: AccountView::default(),
            ready: false,
            signing_in: false,
            sign_in_error: None,
            enroll_token: None,
            enroll_token_pending: false,
            shows_add_server: false,
            add_server_count: 0,
            removing_project: None,
            sign_in_session: None,
            shows_branches: false,
            listed_branches: None,
            branches_project_id: None,
            known_branches: HashMap::new(),
            committing_project: None,
            git_files: Vec::new(),
            pending_git: None,
            git_stages: HashMap::new(),
            notice: None,
            initializing_git: HashSet::new(),
            pulling_bases: HashSet::new(),
            worktree_starts: HashMap::new(),
            panel: None,
            panel_notice: None,
            github: HashMap::new(),
            repos: HashMap::new(),
            repo_errors: HashMap::new(),
            adding_project: None,
            projects_added: 0,
            awaited_project: None,
            awaited_folder: None,
            composer_focus: 0,
            blur_composer: 0,
            servers: Vec::new(),
            server_updates: HashMap::new(),
            projects: Vec::new(),
            no_projects: Vec::new(),
            threads: HashMap::new(),
            active: Vec::new(),
            done: Vec::new(),
            selection: Selection::Draft(String::new()),
            activity: Activity::default(),
            transcript_is_empty: true,
            agents: Vec::new(),
            sending_drafts: HashMap::new(),
            error_message: None,
            thread_drafts: Vec::new(),
            opened_draft_preview: None,
            undo: None,
            undo_task: None,
            account_picture: None,
            media_storage: None,
            drafts: HashMap::new(),
            attachments_by_key: HashMap::new(),
            viewing: None,
            media_progress: HashMap::new(),
            workspace_version: 0,
            draft_version: 0,
            skills: skills::Skills::default(),
            updater: Updater::new(),
            right_panel,
            settings: SettingsState::default(),
            linear,
            ricrac,
            pull_requests: PullRequests::default(),
            repo_listing: RepoListing::default(),
            sidebar: SidebarState::default(),
            transcript: cx.new(|_| Transcript::new()),
            agent_transcript: cx.new(|_| Transcript::new()),
            open_thread_id: None,
            last_selection: None,
            draft_awaiting_server: None,
            app_active: true,
            full_screen: false,
            shortcuts: Shortcuts::default(),
            recording_shortcut: None,
            recorded_shortcut: None,
            shows_jump_hints: false,
            jump_hint_task: None,
            shows_usage: false,
            usage: usage::Usage::default(),
            _events: None,
        };
        store.load_preferences();
        store
    }

    // Starting

    pub fn start(&mut self, cx: &mut Context<Self>) {
        let auth_url = std::env::var("MOTILE_AUTH_URL")
            .ok()
            .or_else(|| option_env!("MOTILE_AUTH_URL").map(String::from))
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| "https://auth.motile.app".to_string());
        let config = Config {
            data_dir: self.data_dir.clone(),
            auth_url,
            device_name: device_name(),
            platform: std::env::consts::OS.to_string(),
            local_only: std::env::var("MOTILE_LOCAL").as_deref() == Ok("1"),
            direct_addr: std::env::var("MOTILE_SERVER_ADDR").ok(),
            direct_addrs: Default::default(),
            media_limit: None,
        };
        match Bridge::start(config) {
            Ok((bridge, mut events)) => {
                self.bridge = Some(bridge);
                self._events = Some(cx.spawn(async move |this, cx| {
                    while let Some(first) = events.next().await {
                        // Whatever else has arrived by now is applied in the same update.
                        let mut batch = vec![first];
                        while let Ok(more) = events.try_recv() {
                            batch.push(more);
                            if batch.len() >= 256 {
                                break;
                            }
                        }
                        let applied = this.update(cx, |store, cx| {
                            for incoming in batch {
                                store.receive(incoming, cx);
                            }
                            cx.notify();
                        });
                        if applied.is_err() {
                            break;
                        }
                    }
                }));
            }
            Err(error) => {
                tracing::error!("the core couldn't start: {error:#}");
                self.error_message = Some("Motile couldn't start. Its data folder may not be writable.".into());
            }
        }
        if std::env::var("MOTILE_DEMO").as_deref() != Ok("1") {
            self.updater.start(cx);
        }
    }

    pub fn stop(&self) {
        if let Some(bridge) = &self.bridge {
            bridge.stop();
        }
    }

    /// Motile has come to the front, or gone to the back.
    pub fn set_app_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.app_active == active {
            return;
        }
        self.app_active = active;
        if !active {
            self.jump_hint_task = None;
            self.shows_jump_hints = false;
        }
        if active {
            // What waits to dial again dials now.
            self.send(Command::Foreground);
            self.send(Command::ReloadKeybindings);
            self.mark_open_thread_seen();
            self.read_git(true, None::<fn(&mut Store, Vec<ChangedFile>, &mut Context<Store>)>, cx);
            self.read_worktree_start(true);
        }
    }

    // Commands

    pub fn send(&mut self, command: Command) -> u64 {
        let Some(bridge) = self.bridge.as_mut() else { return 0 };
        bridge.send(command)
    }

    /// Sends the command; `then` gets its answer.
    pub fn ask(
        &mut self,
        command: Command,
        then: impl FnOnce(&mut Store, Reply, &mut Context<Store>) + 'static,
    ) -> u64 {
        let id = self.send(command);
        if id != 0 {
            self.answers.insert(id, Box::new(then));
        }
        id
    }

    /// Sends a command whose answer is large: `read` turns it into what the app keeps, off the
    /// main thread, and `then` gets that.
    pub fn ask_read<T: Send + 'static>(
        &mut self,
        command: Command,
        read: impl FnOnce(Value) -> T + Send + 'static,
        then: impl FnOnce(&mut Store, Result<T, CoreError>, &mut Context<Store>) + 'static,
    ) -> u64 {
        let Some(bridge) = self.bridge.as_mut() else { return 0 };
        let id = bridge.send_read(command, read);
        self.read_answers.insert(
            id,
            Box::new(move |store, result, cx| {
                let result = result.map(|read| *read.downcast::<T>().expect("the answer is what was read"));
                then(store, result, cx)
            }),
        );
        id
    }

    /// Any request to a server; `then` gets the server's message.
    pub fn request_then(
        &mut self,
        server_id: &str,
        request: Request,
        then: impl FnOnce(&mut Store, Reply, &mut Context<Store>) + 'static,
    ) -> u64 {
        self.ask(Command::Request { server_id: server_id.to_string(), request }, then)
    }

    /// A request to a server whose failure is said in an alert.
    pub fn request(&mut self, server_id: &str, request: Request) {
        self.request_then(server_id, request, |store, result, _| {
            if let Err(error) = result {
                store.fail(error);
            }
        });
    }

    // Events

    fn receive(&mut self, incoming: Incoming, cx: &mut Context<Self>) {
        match incoming {
            Incoming::Event(event) => self.apply_event(*event, cx),
            Incoming::Read { id, result } => {
                if let Some(answer) = self.read_answers.remove(&id) {
                    answer(self, result, cx);
                }
            }
            Incoming::Rows(prepared) => self.apply_rows(*prepared, cx),
        }
    }

    fn apply_rows(&mut self, prepared: Prepared, cx: &mut Context<Self>) {
        match prepared {
            Prepared::Rows { thread_id, reset, start, remove, rows, earlier } => {
                if self.transcript.read(cx).thread_id.as_deref() != Some(&thread_id) {
                    return;
                }
                let (is_empty, turns) = self.transcript.update(cx, |transcript, cx| {
                    transcript.apply(reset, start, remove, rows, earlier);
                    cx.notify();
                    (transcript.is_empty(), transcript.turns())
                });
                self.transcript_is_empty = is_empty;
                if self.right_panel.turns != turns {
                    self.right_panel.turns = turns;
                }
            }
            Prepared::AgentRows { thread_id, agent_id, reset, start, remove, rows } => {
                if self.transcript.read(cx).thread_id.as_deref() != Some(&thread_id)
                    || self.right_panel.shown_agent.as_deref() != Some(&agent_id)
                {
                    return;
                }
                self.agent_transcript.update(cx, |transcript, cx| {
                    transcript.apply(reset, start, remove, rows, false);
                    cx.notify();
                });
            }
        }
    }

    fn apply_event(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Account { account } => self.apply_account(account, cx),
            Event::Restored => {
                self.ready = true;
                self.ensure_draft_project();
            }
            Event::Servers { servers } => self.apply_servers(servers.into_iter().map(Server::from).collect(), cx),
            Event::Threads { server_id, threads } => {
                self.apply_threads(threads.into_iter().map(ThreadInfo::from).collect(), &server_id, cx)
            }
            Event::ThreadUpsert { thread } => self.upsert((*thread).into()),
            Event::ThreadDeleted { thread_id } => self.remove_thread(&thread_id, cx),
            Event::Projects { server_id, projects } => {
                let projects = projects.into_iter().map(|view| Project::new(view, &server_id)).collect();
                self.apply_projects(projects, &server_id, cx);
            }
            Event::ServerUpdate { server_id, received, total, waiting } => {
                let Some(update) = self.server_updates.get_mut(&server_id) else { return };
                if waiting {
                    update.waiting = true;
                } else {
                    update.fraction = total.filter(|total| *total > 0).map(|total| received as f64 / total as f64);
                }
            }
            Event::GitProgress { project_id, thread_id, stage } => {
                self.apply_git_progress(&project_id, thread_id.as_deref(), stage)
            }
            Event::UploadProgress { key, sent, size } => {
                if size > 0 {
                    self.change_attachment(&key, |attachment| {
                        if matches!(attachment.state, UploadState::Uploading(_)) {
                            attachment.state = UploadState::Uploading(sent as f64 / size as f64);
                        }
                    });
                }
            }
            Event::MediaProgress { id, received, size } => {
                if size > 0 {
                    self.media_progress.insert(id, received as f64 / size as f64);
                }
            }
            Event::Agents { thread_id, agents } => {
                if self.transcript.read(cx).thread_id.as_deref() != Some(&thread_id) {
                    return;
                }
                if self.agents != agents {
                    self.agents = agents;
                }
                self.agents_changed(cx);
            }
            Event::CodeSpans { id, file, lines } => self.colour_code(id, file, lines),
            Event::Spans { thread_id, row_id, spans } => {
                if self.transcript.read(cx).thread_id.as_deref() != Some(&thread_id) {
                    return;
                }
                let both = [self.transcript.clone(), self.agent_transcript.clone()];
                for transcript in both {
                    transcript.update(cx, |transcript, cx| {
                        transcript.set_spans(&row_id, spans.clone());
                        cx.notify();
                    });
                }
            }
            Event::Activity { thread_id, activity, waiting } => {
                if self.transcript.read(cx).thread_id.as_deref() != Some(&thread_id) {
                    return;
                }
                let activity = Activity::new(activity, waiting);
                if self.activity != activity {
                    self.activity = activity.clone();
                }
                self.transcript.update(cx, |transcript, cx| {
                    transcript.set_activity(activity);
                    cx.notify();
                });
            }
            Event::ThreadError { message, .. } => self.error_message = Some(message),
            Event::Reply { id, ok, value } => {
                let Some(answer) = self.answers.remove(&id) else { return };
                let result = if ok { Ok(value) } else { Err(CoreError::of(&value)) };
                answer(self, result, cx);
            }
            Event::Live { thread_id, live } => {
                for transcript in [self.transcript.clone(), self.agent_transcript.clone()] {
                    if transcript.read(cx).thread_id.as_deref() == Some(&thread_id) {
                        transcript.update(cx, |transcript, _| transcript.set_live(live));
                    }
                }
            }
            Event::Rows { .. } | Event::AgentRows { .. } => {}
            Event::Keybindings { keybindings } => self.shortcuts = keybindings.into(),
            Event::ThreadSearch { query, searching, matches } => {
                self.apply_thread_search(sidebar::MessageSearch::new(query, searching, matches))
            }
        }
    }

    // Lookups

    pub fn active_threads(&self) -> &[ThreadInfo] {
        &self.active
    }

    pub fn done_threads(&self) -> &[ThreadInfo] {
        &self.done
    }

    fn set_threads(&mut self, threads: HashMap<String, ThreadInfo>) {
        self.threads = threads;
        let mut active: Vec<ThreadInfo> = self.threads.values().filter(|thread| !thread.is_done()).cloned().collect();
        let mut done: Vec<ThreadInfo> = self.threads.values().filter(|thread| thread.is_done()).cloned().collect();
        active.sort_by(ThreadInfo::listed_order);
        done.sort_by(ThreadInfo::listed_order);
        self.active = active;
        self.done = done;
    }

    /// Changes one thread, and moves it in its list only when its place there changed.
    fn set_thread(&mut self, thread: Option<ThreadInfo>, id: &str) {
        let before = match &thread {
            Some(thread) => self.threads.insert(id.to_string(), thread.clone()),
            None => self.threads.remove(id),
        };
        if let (Some(before), Some(thread)) = (&before, &thread)
            && before.is_done() == thread.is_done()
            && before.listed_at() == thread.listed_at()
        {
            let list = self.list_mut(thread.is_done());
            if let Some(index) = index_of(list, before) {
                list[index] = thread.clone();
            }
            return;
        }
        if let Some(before) = before {
            let list = self.list_mut(before.is_done());
            if let Some(index) = index_of(list, &before) {
                list.remove(index);
            }
        }
        if let Some(thread) = thread {
            let list = self.list_mut(thread.is_done());
            let place = place_of(list, &thread);
            list.insert(place, thread);
        }
    }

    fn list_mut(&mut self, done: bool) -> &mut Vec<ThreadInfo> {
        if done { &mut self.done } else { &mut self.active }
    }

    pub fn selected_thread(&self) -> Option<&ThreadInfo> {
        let Selection::Thread(id) = &self.selection else { return None };
        self.threads.get(id)
    }

    pub fn selected_draft(&self) -> Option<&ThreadDraft> {
        let Selection::Draft(id) = &self.selection else { return None };
        self.thread_drafts.iter().find(|draft| &draft.id == id)
    }

    /// Every draft with something written in it, newest first. The open one is listed as it was
    /// when it was opened, and one being sent once it has been left.
    pub fn listed_drafts(&self) -> Vec<ListedDraft> {
        self.thread_drafts
            .iter()
            .rev()
            .filter_map(|draft| {
                if let Some(sending) = self.sending_drafts.get(&draft.id) {
                    let listed = ListedDraft { draft: draft.clone(), preview: sending.preview(), sending: true };
                    return sending.left.then_some(listed);
                }
                let written = if self.selection == Selection::Draft(draft.id.clone()) {
                    self.opened_draft_preview.clone()
                } else {
                    self.preview(draft)
                };
                written.map(|preview| ListedDraft { draft: draft.clone(), preview, sending: false })
            })
            .collect()
    }

    /// The first line of what was written in a draft, or what is attached to it. Nothing for a
    /// draft that is still empty.
    fn preview(&self, draft: &ThreadDraft) -> Option<String> {
        let text = self.drafts.get(&draft.id).map(String::as_str).unwrap_or_default();
        preview_of(text, self.attachments_by_key.get(&draft.id).map_or(0, Vec::len))
    }

    pub fn is_sending(&self, draft_id: &str) -> bool {
        self.sending_drafts.contains_key(draft_id)
    }

    /// The project with that id, a folder the user added or a server's "No project".
    pub fn project(&self, id: Option<&str>) -> Option<&Project> {
        let id = id?;
        self.projects.iter().chain(&self.no_projects).find(|project| project.id == id)
    }

    /// The projects a thread can start in, the one a thread was last started in first. A project
    /// without threads counts from when it was added.
    pub fn recent_projects(&self) -> Vec<&Project> {
        let mut last_used: HashMap<&str, f64> = HashMap::new();
        for thread in self.threads.values() {
            let used = last_used.entry(thread.project_id.as_str()).or_insert(0.);
            *used = used.max(thread.created_at);
        }
        let mut projects: Vec<&Project> = self.projects.iter().filter(|project| self.is_live(project)).collect();
        let used = |project: &Project| last_used.get(project.id.as_str()).copied().unwrap_or(project.created_at);
        projects.sort_by(|a, b| used(b).total_cmp(&used(a)).then_with(|| b.id.cmp(&a.id)));
        projects
    }

    pub fn server(&self, id: Option<&str>) -> Option<&Server> {
        let id = id?;
        self.servers.iter().find(|server| server.id == id)
    }

    /// The servers of the account, which threads start and go on on. A removed one only keeps its
    /// threads readable.
    pub fn live_servers(&self) -> impl Iterator<Item = &Server> {
        self.servers.iter().filter(|server| !server.removed)
    }

    pub fn has_live_servers(&self) -> bool {
        self.live_servers().next().is_some()
    }

    pub fn is_removed(&self, server_id: &str) -> bool {
        self.server(Some(server_id)).is_some_and(|server| server.removed)
    }

    /// Whether the thread is a copy of a removed server's, which can only be read.
    pub fn on_removed_server(&self, thread: &ThreadInfo) -> bool {
        self.is_removed(&thread.server_id)
    }

    /// Whether a thread can start in the project: its server wasn't removed.
    pub fn is_live(&self, project: &Project) -> bool {
        !self.is_removed(&project.server_id)
    }

    /// The "No project" of each server a thread can start on.
    pub fn live_no_projects(&self) -> impl Iterator<Item = &Project> {
        self.no_projects.iter().filter(|project| self.is_live(project))
    }

    /// The first project a thread can start in: a folder, else a server's "No project".
    fn first_live_project(&self) -> Option<&Project> {
        self.projects.iter().find(|project| self.is_live(project)).or_else(|| self.live_no_projects().next())
    }

    /// Whether the open thread's server was removed, so that it can only go on on another one.
    pub fn open_thread_removed(&self) -> bool {
        self.selected_thread().is_some_and(|thread| self.is_removed(&thread.server_id))
    }

    /// The project the composer's thread works in, or the one the open draft would start in.
    pub fn composer_project(&self) -> Option<Project> {
        self.thread_project().or_else(|| self.project(self.selected_draft()?.project_id.as_deref()).cloned())
    }

    /// The open thread's project, as the thread works in it.
    pub fn thread_project(&self) -> Option<Project> {
        let thread = self.selected_thread()?;
        Some(self.project(Some(&thread.project_id))?.seen(thread))
    }

    fn draft_folder_project(&self) -> Option<Project> {
        if self.draft_uses_worktree() {
            return None;
        }
        self.project(self.selected_draft()?.project_id.as_deref()).cloned()
    }

    /// The project git works in from here: the open thread's, or the open draft's when its thread
    /// would start in the project's folder.
    pub fn git_project(&self) -> Option<Project> {
        let project = self.thread_project().or_else(|| self.draft_folder_project())?;
        self.can_use_git(&project).then_some(project)
    }

    /// The folder the right panel looks into: the one the open thread works in, or the project's
    /// when the open draft's thread would start there.
    pub fn panel_target(&self) -> Option<PanelTarget> {
        let thread = self.selected_thread();
        let project =
            self.thread_project().or_else(|| self.project(self.selected_draft()?.project_id.as_deref()).cloned())?;
        // A thread without a project has no folder until it starts.
        if project.no_project && thread.is_none() {
            return None;
        }
        let folder = if project.no_project {
            thread.map(|thread| thread.cwd.clone())
        } else {
            project.worktree.as_ref().map(|worktree| worktree.path.clone())
        };
        let folder = folder.unwrap_or_else(|| project.path.clone());
        let awaits_worktree = self.selected_thread().is_none() && self.draft_uses_worktree();
        let pull_request = (!awaits_worktree)
            .then(|| {
                self.selected_thread()
                    .and_then(|thread| thread.pull_request.as_ref())
                    .or_else(|| project.git.as_ref().and_then(|git| git.pull_request.as_ref()))
                    .map(|pull_request| pull_request.number)
            })
            .flatten();
        Some(PanelTarget {
            key: self.draft_key(),
            server_id: project.server_id.clone(),
            project_id: project.id.clone(),
            thread_id: self.selected_thread().map(|thread| thread.id.clone()),
            name: last_component(&folder),
            repository: project.branch.is_some(),
            worktree: project.worktree.is_some(),
            awaits_worktree,
            pull_request,
        })
    }

    /// What the right panel says in place of what it can't show of an unlinked server.
    pub fn removed_note(&self, server_id: &str) -> Option<String> {
        let server = self.server(Some(server_id)).filter(|server| server.removed)?;
        Some(format!("“{}” is unlinked, so this thread is read-only.", server.name))
    }

    /// Why the right panel has nothing to show here, when it hasn't.
    pub fn panel_unavailable(&self) -> Option<String> {
        let Some(target) = self.panel_target() else { return Some("Add a project to see its files.".into()) };
        if let Some(note) = self.removed_note(&target.server_id) {
            return Some(note);
        }
        let Some(server) = self.server(Some(&target.server_id)).filter(|server| server.connected()) else {
            return Some("Your server isn't connected.".into());
        };
        (server.protocol_version < 7).then(|| format!("Update {} to see files and changes.", server.name))
    }

    /// The server the composer is talking to: the open thread's, or the open draft's project's.
    pub fn composer_server(&self) -> Option<&Server> {
        if let Some(thread) = self.selected_thread() {
            return self.server(Some(&thread.server_id));
        }
        self.selected_draft().and_then(|draft| self.server_of(draft))
    }

    /// The server a draft starts its thread on: its project's, or the first one.
    fn server_of(&self, draft: &ThreadDraft) -> Option<&Server> {
        let project = self.project(draft.project_id.as_deref());
        self.server(project.map(|project| project.server_id.as_str())).or(self.live_servers().next())
    }

    /// The models the composer offers. An open thread on a server from before accounts stays
    /// with its agent.
    pub fn composer_models(&self) -> Vec<ModelInfo> {
        let Some(server) = self.composer_server() else { return Vec::new() };
        let Some(thread) = self.selected_thread().filter(|_| !server.switches_accounts()) else {
            return server.models.clone();
        };
        server.models.iter().filter(|model| model.agent == thread.agent).cloned().collect()
    }

    /// The id of the account the thread works with, or the draft starts with.
    fn composer_account_id(&self) -> Option<&str> {
        let id = match self.selected_thread() {
            Some(thread) => Some(thread.agent_account.as_str()),
            None => self.selected_draft()?.agent_account.as_deref(),
        };
        id.filter(|id| !id.is_empty())
    }

    /// The model of the thread or the draft, as its account lists it, or the first one offered.
    pub fn composer_model(&self) -> Option<ModelInfo> {
        let id = match self.selected_thread() {
            Some(thread) => thread.model.clone(),
            None => self.selected_draft().and_then(|draft| draft.model.clone()),
        };
        let agent = self.selected_thread().map(|thread| thread.agent);
        let offered: Vec<ModelInfo> =
            self.composer_models().into_iter().filter(|model| agent.is_none_or(|agent| model.agent == agent)).collect();
        Server::model(id.as_deref(), self.composer_account_id(), &offered).cloned()
    }

    /// The account of the composer's model's agent the thread works with.
    pub fn composer_account(&self) -> Option<AgentAccount> {
        let model = self.composer_model()?;
        self.composer_server()?.account(self.composer_account_id(), model.agent).cloned()
    }

    /// The model the composer shows, with the account when its agent has more than one.
    pub fn composer_model_label(&self) -> Option<String> {
        let model = self.composer_model()?;
        let label = self.composer_server().map(|server| server.label(&model, self.composer_account_id()));
        Some(label.unwrap_or_else(|| model.name.clone()))
    }

    /// What the model menu lists: each account with its agent's models, titled with the
    /// account's name when its agent has more than one.
    pub fn composer_choices(&self) -> Vec<(String, AgentAccount, Vec<ModelInfo>)> {
        let Some(server) = self.composer_server() else { return Vec::new() };
        let models = self.composer_models();
        AGENTS
            .into_iter()
            .filter(|agent| models.iter().any(|model| model.agent == *agent))
            .flat_map(|agent| server.accounts(agent))
            .map(|account| {
                let several = server.accounts(account.agent).len() > 1;
                let name = agent_name(account.agent);
                let title = if several { format!("{name} · {}", account.name) } else { name.to_string() };
                let offered = models.iter().filter(|model| model.runs_under(account)).cloned().collect();
                (title, account.clone(), offered)
            })
            .collect()
    }

    /// The effort the thread or the draft runs at: "" where the agent applies its own default.
    pub fn composer_effort(&self) -> Option<String> {
        let effort = match self.selected_thread() {
            Some(thread) => thread.effort.clone(),
            None => self.selected_draft().and_then(|draft| draft.effort.clone()),
        };
        let model = self.composer_model()?;
        if model.efforts.is_empty() {
            return None;
        }
        if let Some(effort) = effort.filter(|effort| model.efforts.contains(effort)) {
            return Some(effort);
        }
        Some(model.default_effort.clone().unwrap_or_default())
    }

    pub fn composer_access(&self) -> Access {
        self.selected_thread()
            .map(|thread| thread.access)
            .or_else(|| self.selected_draft().map(|draft| draft.access))
            .unwrap_or(Access::Full)
    }

    /// What the composer's text and attachments are kept under: the open draft or thread.
    pub fn draft_key(&self) -> String {
        match &self.selection {
            Selection::Draft(id) | Selection::Thread(id) => id.clone(),
        }
    }

    pub fn draft(&self) -> String {
        self.drafts.get(&self.draft_key()).cloned().unwrap_or_default()
    }

    /// What the composer says as it is typed in.
    pub fn set_draft(&mut self, text: String) {
        let key = self.draft_key();
        self.set_text(text, &key);
    }

    pub fn attachments(&self) -> Vec<Attachment> {
        self.attachments_by_key.get(&self.draft_key()).cloned().unwrap_or_default()
    }

    fn set_text(&mut self, text: String, key: &str) {
        if text.is_empty() {
            self.drafts.remove(key);
        } else {
            self.drafts.insert(key.to_string(), text);
        }
        let drafts = self.drafts.clone();
        self.prefs.set("drafts", drafts);
    }

    // Preferences

    fn load_preferences(&mut self) {
        self.drafts = self.prefs.get("drafts").unwrap_or_default();
        self.last_selection = self.prefs.string("selection");
        let saved: Vec<ThreadDraft> = self.prefs.get("threadDrafts").unwrap_or_default();
        let last = self.last_selection.clone();
        self.thread_drafts = saved
            .into_iter()
            .filter(|draft| self.preview(draft).is_some() || Some(&draft.id) == last.as_ref())
            .collect();
        let restored = self.thread_drafts.iter().find(|draft| Some(&draft.id) == last.as_ref()).cloned();
        let restored_preview = self.prefs.string("openedDraftPreview");
        let opened = match &restored {
            Some(draft) => draft.clone(),
            None => self.empty_draft(),
        };
        if opened.model.is_none() && opened.agent_account.is_none() && opened.effort.is_none() {
            self.draft_awaiting_server = Some(opened.id.clone());
        }
        self.set_selection(Selection::Draft(opened.id.clone()));
        self.set_opened_draft_preview(restored.and(restored_preview));
    }

    /// Opens the draft or the thread, kept for the next launch.
    fn set_selection(&mut self, selection: Selection) {
        self.selection = selection;
        let key = self.draft_key();
        self.prefs.set("selection", key);
    }

    /// What the open draft said when it was opened, kept for the next launch too.
    fn set_opened_draft_preview(&mut self, preview: Option<String>) {
        self.prefs.set("openedDraftPreview", preview.clone());
        self.opened_draft_preview = preview;
    }

    fn save_thread_drafts(&mut self) {
        let drafts = self.thread_drafts.clone();
        self.prefs.set("threadDrafts", drafts);
    }

    /// A draft that starts with what the last one was set to.
    fn add_draft(&mut self) -> ThreadDraft {
        let mut draft = ThreadDraft::new();
        draft.project_id = self.prefs.string("new.project");
        draft.worktree = Some(self.prefs.bool("new.worktree"));
        self.apply_last_settings(&mut draft);
        self.thread_drafts.push(draft.clone());
        self.save_thread_drafts();
        draft
    }

    /// The model, account and effort are the server's, so each server keeps what was last chosen
    /// on it. A draft whose server isn't known yet keeps what it had.
    fn apply_last_settings(&mut self, draft: &mut ThreadDraft) {
        draft.access = self.prefs.get("new.access").unwrap_or(Access::Full);
        draft.base = None;
        let Some(server) = self.server_of(draft).map(|server| server.id.clone()) else {
            self.draft_awaiting_server = Some(draft.id.clone());
            return;
        };
        if self.draft_awaiting_server.as_ref() == Some(&draft.id) {
            self.draft_awaiting_server = None;
        }
        draft.model = self.prefs.string(&format!("new.model-{server}"));
        draft.agent_account = self.prefs.string(&format!("new.agentAccount-{server}"));
        draft.effort = self.prefs.string(&format!("new.effort-{server}"));
    }

    /// Has the next draft start with what was just chosen, and only that.
    fn remember_choices(&mut self, draft: &ThreadDraft, before: &ThreadDraft) {
        if draft.project_id != before.project_id {
            self.prefs.set("new.project", draft.project_id.clone());
        }
        if draft.worktree != before.worktree {
            self.prefs.set("new.worktree", draft.worktree == Some(true));
        }
        if draft.access != before.access {
            self.prefs.set("new.access", draft.access);
        }
        let Some(server) = self.server_of(draft).map(|server| server.id.clone()) else { return };
        if draft.model != before.model {
            self.prefs.set(&format!("new.model-{server}"), draft.model.clone());
        }
        if draft.agent_account != before.agent_account {
            self.prefs.set(&format!("new.agentAccount-{server}"), draft.agent_account.clone());
        }
        if draft.effort != before.effort {
            self.prefs.set(&format!("new.effort-{server}"), draft.effort.clone());
        }
    }

    /// A draft with nothing in it: the one that is already there, or a new one.
    fn empty_draft(&mut self) -> ThreadDraft {
        let empty =
            self.thread_drafts.iter().rposition(|draft| self.preview(draft).is_none() && !self.is_sending(&draft.id));
        if let Some(index) = empty {
            let mut draft = self.thread_drafts[index].clone();
            draft.from = None;
            self.apply_last_settings(&mut draft);
            self.thread_drafts[index] = draft.clone();
            self.save_thread_drafts();
            return draft;
        }
        self.add_draft()
    }

    /// Changes the open draft, and has the next draft start with what changed.
    fn update_draft(&mut self, change: impl FnOnce(&mut ThreadDraft)) {
        let Selection::Draft(id) = &self.selection else { return };
        let Some(index) = self.thread_drafts.iter().position(|draft| &draft.id == id) else { return };
        let before = self.thread_drafts[index].clone();
        change(&mut self.thread_drafts[index]);
        let draft = self.thread_drafts[index].clone();
        self.remember_choices(&draft, &before);
        self.save_thread_drafts();
    }

    /// Puts the open draft in a project. On another server, it starts with that server's last
    /// choices.
    fn move_draft(&mut self, project_id: Option<String>) {
        let Selection::Draft(id) = &self.selection else { return };
        let Some(index) = self.thread_drafts.iter().position(|draft| &draft.id == id) else { return };
        let before = self.thread_drafts[index].clone();
        let from = self.server_of(&before).map(|server| server.id.clone());
        let mut draft = before.clone();
        draft.project_id = project_id;
        draft.base = None;
        let to = self.server_of(&draft).map(|server| server.id.clone());
        if to != from {
            self.apply_last_settings(&mut draft);
        }
        self.thread_drafts[index] = draft.clone();
        self.remember_choices(&draft, &before);
        self.save_thread_drafts();
        if to != from {
            self.refresh_composer_models();
        }
    }

    /// Has the open draft's server ask its agents what models they run, so that its menus offer
    /// what the thread would start with.
    fn refresh_composer_models(&mut self) {
        let Some(server) = self.composer_server().filter(|server| server.connected() && server.refreshes_models())
        else {
            return;
        };
        let server_id = server.id.clone();
        self.request_then(&server_id, Request::RefreshModels, |_, _, _| {});
    }

    fn remove_draft(&mut self, id: &str) {
        self.thread_drafts.retain(|draft| draft.id != id);
        self.attachments_by_key.remove(id);
        self.set_text(String::new(), id);
        self.save_thread_drafts();
    }

    /// Keeps the open draft pointing at a project that exists, once the projects are known.
    fn ensure_draft_project(&mut self) {
        self.move_to_first_project();
        self.apply_last_settings_once_server_known();
    }

    fn move_to_first_project(&mut self) {
        if !self.ready {
            return;
        }
        let Some(draft) = self.selected_draft() else { return };
        if self.project(draft.project_id.as_deref()).is_some_and(|project| self.is_live(project)) {
            return;
        }
        let first = self.first_live_project().map(|project| project.id.clone());
        let Some(first) = first else { return };
        self.move_draft(Some(first));
    }

    /// Gives the draft opened at launch without choices of its own its server's last ones, once
    /// the cache has said which server that is.
    fn apply_last_settings_once_server_known(&mut self) {
        let Some(awaiting) = self.draft_awaiting_server.clone() else { return };
        if self.selection != Selection::Draft(awaiting.clone()) {
            return;
        }
        let Some(index) = self.thread_drafts.iter().position(|draft| draft.id == awaiting) else { return };
        let mut draft = self.thread_drafts[index].clone();
        let known = match &draft.project_id {
            None => self.has_live_servers(),
            Some(id) => self.project(Some(id)).is_some(),
        };
        if !known {
            return;
        }
        self.apply_last_settings(&mut draft);
        self.thread_drafts[index] = draft;
        self.save_thread_drafts();
    }

    /// Opens the thread that was open when the app was last closed, once it is known.
    fn restore_selection(&mut self, cx: &mut Context<Self>) {
        let Some(wanted) = self.last_selection.clone() else { return };
        if !self.threads.contains_key(&wanted) {
            return;
        }
        self.last_selection = None;
        let Some(draft) = self.selected_draft() else { return };
        if self.preview(draft).is_some() || self.is_sending(&draft.id) {
            return;
        }
        self.select(Selection::Thread(wanted), cx);
    }

    /// Shows an alert that says what went wrong, unless it is that a server couldn't be reached,
    /// which its state in the sidebar already shows.
    pub fn fail(&mut self, error: impl Into<CoreError>) {
        let error = error.into();
        if error.offline {
            return;
        }
        self.error_message = Some(error.message);
    }

    /// The account's name, when its sign-in gave one.
    pub fn account_name(&self) -> Option<&str> {
        self.account.user.as_ref()?.name.as_deref()
    }
}

/// Where the thread is, or would go, in a list in the sidebar's order.
fn place_of(list: &[ThreadInfo], thread: &ThreadInfo) -> usize {
    list.partition_point(|listed| ThreadInfo::listed_order(listed, thread).is_lt())
}

fn index_of(list: &[ThreadInfo], thread: &ThreadInfo) -> Option<usize> {
    let place = place_of(list, thread);
    (list.get(place)?.id == thread.id).then_some(place)
}

pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("MOTILE_DATA_DIR") {
        return PathBuf::from(dir);
    }
    let home = || std::env::var_os("HOME").filter(|home| !home.is_empty()).map(PathBuf::from);
    // Not the Mac app's folder, nor the server's on Linux: each is a device of its own.
    let (base, folder) = if cfg!(target_os = "macos") {
        (home().map(|home| home.join("Library/Application Support")), "Motile GPUI")
    } else if cfg!(windows) {
        (std::env::var_os("LOCALAPPDATA").map(PathBuf::from), "Motile GPUI")
    } else {
        let xdg = std::env::var_os("XDG_DATA_HOME").filter(|dir| !dir.is_empty()).map(PathBuf::from);
        (xdg.or_else(|| home().map(|home| home.join(".local/share"))), "motile-gpui")
    };
    base.map(|base| base.join(folder)).unwrap_or_else(std::env::temp_dir)
}

fn device_name() -> String {
    #[cfg(target_os = "macos")]
    if let Ok(output) = std::process::Command::new("scutil").arg("--get").arg("ComputerName").output() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }
    #[cfg(windows)]
    if let Some(name) = std::env::var("COMPUTERNAME").ok().filter(|name| !name.is_empty()) {
        return name;
    }
    std::process::Command::new("hostname")
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Computer".into())
}

#[cfg(test)]
mod tests {
    use super::{Continuation, ThreadDraft};

    #[test]
    fn a_draft_keeps_the_thread_it_goes_on_from_and_one_kept_before_has_none() {
        let kept = serde_json::json!({
            "id": "D", "project_id": "p", "model": null, "effort": null, "access": "full",
            "worktree": false, "base": null
        });
        let draft: ThreadDraft = serde_json::from_value(kept).unwrap();
        assert_eq!(draft.from, None);

        let from = Continuation { origin: "gone".into(), thread_id: "t".into(), title: "Fix the build".into() };
        let draft = ThreadDraft { from: Some(from.clone()), ..draft };
        let read: ThreadDraft = serde_json::from_value(serde_json::to_value(&draft).unwrap()).unwrap();
        assert_eq!(read.from, Some(from));
    }
}
