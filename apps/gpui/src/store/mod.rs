//! Everything the views show. It mirrors the core: commands go to it, and its events change the
//! state here. All of it is used on the main thread.

mod account;
mod attachments;
mod git;
mod projects;
mod threads;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use futures::StreamExt;
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::{AccountView, Command, Config, Event};
use motile_core::render::agents::AgentView;
use motile_protocol::wire::{Access, Branch, ChangedFile, GitHubState, GitStage, Repo, Request};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use self::git::{GitNotice, PendingGit, stage_label};
use crate::bridge::{Bridge, Incoming, Read, error_of};
use crate::models::*;
use crate::panel::state::{PanelTabs, PanelTarget, SidePanel};
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
    pub effort: Option<String>,
    pub access: Access,
    pub plan: bool,
    /// The thread starts in a new worktree, on a branch that starts from `base`.
    pub worktree: Option<bool>,
    pub base: Option<String>,
}

impl ThreadDraft {
    fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string().to_uppercase(),
            project_id: None,
            model: None,
            effort: None,
            access: Access::Full,
            plan: false,
            worktree: None,
            base: None,
        }
    }
}

/// A draft as the sidebar lists it.
#[derive(Clone, Debug)]
pub struct ListedDraft {
    pub draft: ThreadDraft,
    pub preview: String,
}

/// Where the command panel opens.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum PanelPage {
    /// Everything that can be done from here.
    Commands,
    /// The projects, to start a thread in one.
    Projects,
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
}

/// A project that a server is making or cloning.
#[derive(Clone, PartialEq, Debug)]
pub struct AddingProject {
    pub server_id: String,
    pub name: String,
}

/// A server that is installing a new version of itself.
#[derive(Clone, PartialEq, Debug)]
pub struct ServerUpdate {
    /// The version it had when the update began.
    pub from: String,
    /// How much of the download has arrived, when the server knows how much there is.
    pub fraction: Option<f64>,
    /// The new version is installed and the server is starting it.
    pub restarting: bool,
}

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

pub type Reply = Result<Value, String>;
type Answer = Box<dyn FnOnce(&mut Store, Reply, &mut Context<Store>)>;
type ReadAnswer = Box<dyn FnOnce(&mut Store, Result<Read, String>, &mut Context<Store>)>;

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
    pub shows_add_server: bool,
    add_server_count: usize,
    /// The project an icon is being chosen for.
    pub icon_project: Option<Project>,
    /// The browser sheet of a sign-in that is open.
    pub sign_in_session: Option<crate::sign_in::Session>,
    /// Files are being dragged over the window.
    pub drop_targeted: bool,
    /// The branch picker under the composer is open, on the branches it was opened with.
    pub shows_branches: bool,
    pub listed_branches: Result<Vec<Branch>, String>,
    /// The project whose changes the commit sheet is open on, with the files it was opened with.
    pub committing_project: Option<Project>,
    pub git_files: Vec<ChangedFile>,
    /// An action that pushes from the default branch, until the user says where it should happen.
    pub pending_git: Option<PendingGit>,
    /// The stage a project's git action is at, by project.
    pub git_stages: HashMap<String, GitStage>,
    pub git_notice: Option<GitNotice>,
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
    awaited_project_id: Option<String>,
    /// Counts up when the composer should take the keyboard back.
    pub composer_focus: u64,
    /// Counts up when the composer should let go of the keyboard.
    pub blur_composer: u64,

    // What the servers hold
    pub servers: Vec<Server>,
    pub server_updates: HashMap<String, ServerUpdate>,
    pub projects: Vec<Project>,
    pub threads: HashMap<String, ThreadInfo>,

    // The open thread
    /// Always a draft or a thread; `load_preferences` opens the first draft.
    pub selection: Selection,
    pub activity: Activity,
    pub transcript_is_empty: bool,
    /// The agents the open thread's agent has started, in the order it started them.
    pub agents: Vec<AgentView>,
    /// The drafts whose first message is on its way to the server.
    pub sending_draft_ids: HashSet<String>,
    pub error_message: Option<String>,
    pub thread_drafts: Vec<ThreadDraft>,
    /// What the open draft said when it was opened, if it said anything. Its row in the sidebar
    /// shows this, so the sidebar doesn't change while the draft is being written.
    pub opened_draft_preview: Option<String>,
    pub undo: Option<UndoNotice>,
    undo_task: Option<Task<()>>,
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

    pub updater: Updater,
    pub side_panel: SidePanel,
    pub transcript: Entity<Transcript>,
    /// What the agent did that the side panel shows.
    pub agent_transcript: Entity<Transcript>,
    open_thread_id: Option<String>,
    /// The thread that was open when the app was last closed, until it has been opened again.
    last_selection: Option<String>,
    /// Whether the app is in front.
    pub app_active: bool,
    _events: Option<Task<()>>,
}

impl Store {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let data_dir = data_dir();
        let prefs_file =
            std::env::var("MOTILE_PREFS").map(PathBuf::from).unwrap_or_else(|_| data_dir.join("preferences.json"));
        let prefs = Prefs::load(prefs_file);
        let side_panel = SidePanel::new(
            prefs.bool("panel.open"),
            prefs.get::<BTreeMap<String, PanelTabs>>("panel.tabs").unwrap_or_default(),
        );
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
            shows_add_server: false,
            add_server_count: 0,
            icon_project: None,
            sign_in_session: None,
            drop_targeted: false,
            shows_branches: false,
            listed_branches: Ok(Vec::new()),
            committing_project: None,
            git_files: Vec::new(),
            pending_git: None,
            git_stages: HashMap::new(),
            git_notice: None,
            panel: None,
            panel_notice: None,
            github: HashMap::new(),
            repos: HashMap::new(),
            repo_errors: HashMap::new(),
            adding_project: None,
            projects_added: 0,
            awaited_project_id: None,
            composer_focus: 0,
            blur_composer: 0,
            servers: Vec::new(),
            server_updates: HashMap::new(),
            projects: Vec::new(),
            threads: HashMap::new(),
            selection: Selection::Draft(String::new()),
            activity: Activity::default(),
            transcript_is_empty: true,
            agents: Vec::new(),
            sending_draft_ids: HashSet::new(),
            error_message: None,
            thread_drafts: Vec::new(),
            opened_draft_preview: None,
            undo: None,
            undo_task: None,
            media_storage: None,
            drafts: HashMap::new(),
            attachments_by_key: HashMap::new(),
            viewing: None,
            media_progress: HashMap::new(),
            workspace_version: 0,
            draft_version: 0,
            updater: Updater::new(),
            side_panel,
            transcript: cx.new(|_| Transcript::new()),
            agent_transcript: cx.new(|_| Transcript::new()),
            open_thread_id: None,
            last_selection: None,
            app_active: true,
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
        if active {
            self.mark_open_thread_seen();
            self.read_git(true, None::<fn(&mut Store, Vec<ChangedFile>, &mut Context<Store>)>, cx);
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
        then: impl FnOnce(&mut Store, Result<T, String>, &mut Context<Store>) + 'static,
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
                store.error_message = Some(error);
            }
        });
    }

    // Events

    fn receive(&mut self, incoming: Incoming, cx: &mut Context<Self>) {
        match incoming {
            Incoming::Event(event) => self.apply_event(event, cx),
            Incoming::Read { id, result } => {
                if let Some(answer) = self.read_answers.remove(&id) {
                    answer(self, result, cx);
                }
            }
            Incoming::Rows(prepared) => self.apply_rows(prepared, cx),
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
                if self.side_panel.turns != turns {
                    self.side_panel.turns = turns;
                }
            }
            Prepared::AgentRows { thread_id, agent_id, reset, start, remove, rows } => {
                if self.transcript.read(cx).thread_id.as_deref() != Some(&thread_id)
                    || self.side_panel.shown_agent.as_deref() != Some(&agent_id)
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
            Event::Servers { servers } => self.apply_servers(servers.into_iter().map(Server::from).collect()),
            Event::Threads { server_id, threads } => {
                self.apply_threads(threads.into_iter().map(ThreadInfo::from).collect(), &server_id, cx)
            }
            Event::ThreadUpsert { thread } => self.upsert(thread.into()),
            Event::ThreadDeleted { thread_id } => self.remove_thread(&thread_id, cx),
            Event::Projects { server_id, projects } => {
                let projects = projects.into_iter().map(|view| Project::new(view, &server_id)).collect();
                self.apply_projects(projects, &server_id, cx);
            }
            Event::ServerUpdate { server_id, received, total } => {
                if let Some(update) = self.server_updates.get_mut(&server_id) {
                    update.fraction = total.filter(|total| *total > 0).map(|total| received as f64 / total as f64);
                }
            }
            Event::GitProgress { project_id, stage } => {
                if let Some(current) = self.git_stages.get_mut(&project_id) {
                    *current = stage;
                }
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
                let result = if ok { Ok(value) } else { Err(error_of(&value)) };
                answer(self, result, cx);
            }
            Event::Rows { .. } | Event::AgentRows { .. } => {}
        }
    }

    // Lookups

    pub fn active_threads(&self) -> Vec<&ThreadInfo> {
        let mut active: Vec<&ThreadInfo> = self.threads.values().filter(|thread| !thread.is_done()).collect();
        active.sort_by(|a, b| b.active_order().total_cmp(&a.active_order()).then_with(|| b.id.cmp(&a.id)));
        active
    }

    pub fn done_threads(&self) -> Vec<&ThreadInfo> {
        let mut done: Vec<&ThreadInfo> = self.threads.values().filter(|thread| thread.is_done()).collect();
        done.sort_by(|a, b| b.done_at.unwrap_or(0.).total_cmp(&a.done_at.unwrap_or(0.)).then_with(|| b.id.cmp(&a.id)));
        done
    }

    pub fn selected_thread(&self) -> Option<&ThreadInfo> {
        let Selection::Thread(id) = &self.selection else { return None };
        self.threads.get(id)
    }

    pub fn selected_draft(&self) -> Option<&ThreadDraft> {
        let Selection::Draft(id) = &self.selection else { return None };
        self.thread_drafts.iter().find(|draft| &draft.id == id)
    }

    /// Every draft with something written in it that isn't being sent, newest first. The open
    /// one is listed as it was when it was opened.
    pub fn listed_drafts(&self) -> Vec<ListedDraft> {
        self.thread_drafts
            .iter()
            .rev()
            .filter(|draft| !self.sending_draft_ids.contains(&draft.id))
            .filter_map(|draft| {
                let written = if self.selection == Selection::Draft(draft.id.clone()) {
                    self.opened_draft_preview.clone()
                } else {
                    self.preview(draft)
                };
                written.map(|preview| ListedDraft { draft: draft.clone(), preview })
            })
            .collect()
    }

    /// The first line of what was written in a draft, or what is attached to it. Nothing for a
    /// draft that is still empty.
    fn preview(&self, draft: &ThreadDraft) -> Option<String> {
        let text = self.drafts.get(&draft.id).map(|text| text.trim()).unwrap_or_default();
        if let Some(line) = text.split('\n').find(|line| !line.is_empty()) {
            return Some(line.to_string());
        }
        let files = self.attachments_by_key.get(&draft.id)?;
        Some(if files.len() == 1 { "1 attachment".into() } else { format!("{} attachments", files.len()) })
    }

    pub fn project(&self, id: Option<&str>) -> Option<&Project> {
        let id = id?;
        self.projects.iter().find(|project| project.id == id)
    }

    /// The projects, the one a thread was last started in first. A project without threads
    /// counts from when it was added.
    pub fn recent_projects(&self) -> Vec<&Project> {
        let mut last_used: HashMap<&str, f64> = HashMap::new();
        for thread in self.threads.values() {
            let used = last_used.entry(thread.project_id.as_str()).or_insert(0.);
            *used = used.max(thread.created_at);
        }
        let mut projects: Vec<&Project> = self.projects.iter().collect();
        let used = |project: &Project| last_used.get(project.id.as_str()).copied().unwrap_or(project.created_at);
        projects.sort_by(|a, b| used(b).total_cmp(&used(a)).then_with(|| b.id.cmp(&a.id)));
        projects
    }

    pub fn server(&self, id: Option<&str>) -> Option<&Server> {
        let id = id?;
        self.servers.iter().find(|server| server.id == id)
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

    /// The folder the side panel looks into: the one the open thread works in, or the project's
    /// when the open draft's thread would start there.
    pub fn panel_target(&self) -> Option<PanelTarget> {
        let project = self.thread_project().or_else(|| self.draft_folder_project())?;
        let folder =
            project.worktree.as_ref().map(|worktree| worktree.path.clone()).unwrap_or_else(|| project.path.clone());
        Some(PanelTarget {
            key: self.draft_key(),
            server_id: project.server_id.clone(),
            project_id: project.id.clone(),
            thread_id: self.selected_thread().map(|thread| thread.id.clone()),
            name: last_component(&folder),
            repository: project.branch.is_some(),
            worktree: project.worktree.is_some(),
        })
    }

    /// Why the side panel has nothing to show here, when it hasn't.
    pub fn panel_unavailable(&self) -> Option<String> {
        let Some(target) = self.panel_target() else {
            return Some(if self.draft_uses_worktree() {
                "The worktree is made when the thread starts.".into()
            } else {
                "Add a project to see its files.".into()
            });
        };
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
        let project = self.project(self.selected_draft().and_then(|draft| draft.project_id.as_deref()));
        self.server(project.map(|project| project.server_id.as_str())).or(self.servers.first())
    }

    /// The models the composer offers: an open thread stays with its agent.
    pub fn composer_models(&self) -> Vec<motile_protocol::wire::ModelInfo> {
        let models = self.composer_server().map(|server| server.models.clone()).unwrap_or_default();
        let Some(thread) = self.selected_thread() else { return models };
        models.into_iter().filter(|model| model.agent == thread.agent).collect()
    }

    pub fn composer_model(&self) -> Option<motile_protocol::wire::ModelInfo> {
        let id = match self.selected_thread() {
            Some(thread) => thread.model.clone(),
            None => self.selected_draft().and_then(|draft| draft.model.clone()),
        };
        let models = self.composer_models();
        models.iter().find(|model| Some(&model.id) == id.as_ref()).or(models.first()).cloned()
    }

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
        model.default_effort.clone().or_else(|| model.efforts.first().cloned())
    }

    pub fn composer_access(&self) -> Access {
        self.selected_thread()
            .map(|thread| thread.access)
            .or_else(|| self.selected_draft().map(|draft| draft.access))
            .unwrap_or(Access::Full)
    }

    pub fn composer_plan(&self) -> bool {
        self.selected_thread()
            .map(|thread| thread.plan)
            .or_else(|| self.selected_draft().map(|draft| draft.plan))
            .unwrap_or(false)
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
        self.thread_drafts = saved.into_iter().filter(|draft| self.preview(draft).is_some()).collect();
        let opened = match self.thread_drafts.iter().find(|draft| Some(&draft.id) == self.last_selection.as_ref()) {
            Some(draft) => draft.clone(),
            None => self.empty_draft(),
        };
        self.selection = Selection::Draft(opened.id.clone());
        self.opened_draft_preview = self.preview(&opened);
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

    fn apply_last_settings(&self, draft: &mut ThreadDraft) {
        draft.model = self.prefs.string("new.model");
        draft.effort = self.prefs.string("new.effort");
        draft.access = self.prefs.get("new.access").unwrap_or(Access::Full);
    }

    fn remember_settings(&mut self, model: Option<String>, effort: Option<String>, access: Access) {
        self.prefs.set("new.model", model);
        self.prefs.set("new.effort", effort);
        self.prefs.set("new.access", access);
    }

    /// A draft with nothing in it: the one that is already there, or a new one.
    fn empty_draft(&mut self) -> ThreadDraft {
        let empty = self
            .thread_drafts
            .iter()
            .rposition(|draft| self.preview(draft).is_none() && !self.sending_draft_ids.contains(&draft.id));
        if let Some(index) = empty {
            let mut draft = self.thread_drafts[index].clone();
            self.apply_last_settings(&mut draft);
            self.thread_drafts[index] = draft.clone();
            self.save_thread_drafts();
            return draft;
        }
        self.add_draft()
    }

    /// Changes the open draft, and has the next draft start with the same choices.
    fn update_draft(&mut self, change: impl FnOnce(&mut ThreadDraft)) {
        let Selection::Draft(id) = &self.selection else { return };
        let Some(index) = self.thread_drafts.iter().position(|draft| &draft.id == id) else { return };
        change(&mut self.thread_drafts[index]);
        let draft = self.thread_drafts[index].clone();
        self.prefs.set("new.project", draft.project_id.clone());
        self.prefs.set("new.worktree", draft.worktree == Some(true));
        self.remember_settings(draft.model, draft.effort, draft.access);
        self.save_thread_drafts();
    }

    fn remove_draft(&mut self, id: &str) {
        self.thread_drafts.retain(|draft| draft.id != id);
        self.attachments_by_key.remove(id);
        self.set_text(String::new(), id);
        self.save_thread_drafts();
    }

    /// Keeps the open draft pointing at a project that exists, once the projects are known.
    fn ensure_draft_project(&mut self) {
        if !self.ready {
            return;
        }
        let Some(draft) = self.selected_draft() else { return };
        if self.project(draft.project_id.as_deref()).is_some() {
            return;
        }
        let Some(first) = self.projects.first().map(|project| project.id.clone()) else { return };
        self.update_draft(|draft| draft.project_id = Some(first));
    }

    /// Opens the thread that was open when the app was last closed, once it is known.
    fn restore_selection(&mut self, cx: &mut Context<Self>) {
        let Some(wanted) = self.last_selection.clone() else { return };
        if !self.threads.contains_key(&wanted) {
            return;
        }
        self.last_selection = None;
        let Some(draft) = self.selected_draft() else { return };
        if self.preview(draft).is_some() {
            return;
        }
        self.select(Selection::Thread(wanted), cx);
    }

    /// Shows an alert that says what went wrong.
    pub fn fail(&mut self, message: impl Into<String>) {
        self.error_message = Some(message.into());
    }
}

pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("MOTILE_DATA_DIR") {
        return PathBuf::from(dir);
    }
    let base = if cfg!(target_os = "macos") {
        std::env::var("HOME").map(|home| PathBuf::from(home).join("Library/Application Support")).ok()
    } else {
        std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .ok()
            .or_else(|| std::env::var("HOME").map(|home| PathBuf::from(home).join(".local/share")).ok())
    };
    // Not the Mac app's folder: both can run on one Mac, each a device of its own.
    base.map(|base| base.join("Motile GPUI")).unwrap_or_else(std::env::temp_dir)
}

fn device_name() -> String {
    #[cfg(target_os = "macos")]
    if let Ok(output) = std::process::Command::new("scutil").arg("--get").arg("ComputerName").output() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }
    std::process::Command::new("hostname")
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Computer".into())
}
