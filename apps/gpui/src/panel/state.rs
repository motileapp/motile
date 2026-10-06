//! What the side panel shows: whether it is open, the tabs each thread has in it, and what the
//! tabs of the open thread show.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Command;
use motile_protocol::wire::{Change, DiffScope, Request};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::store::Store;
use crate::transcript::model::TurnChange;

pub const WIDTHS: (f32, f32) = (340., 900.);

/// A tab of the panel beside the thread.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "path", rename_all = "snake_case")]
pub enum PanelTab {
    Diff,
    /// The folder's files, to open one.
    Files,
    /// One file, by its path in the folder.
    File(String),
    /// What one turn changed in one file, by the item that ended the turn.
    Change {
        turn: String,
        path: String,
    },
    /// The agents the thread's agent has started, and what one of them did.
    Agents,
    /// The pull request of the branch the thread works on.
    PullRequest,
    /// Another pull request of the repository, by its number.
    PullRequestNumber(u64),
    /// The repository's pull requests.
    PullRequests,
    /// The issues of the Linear workspace the server is connected to, or how to connect it.
    Linear,
    /// One issue of a workspace, by its id there, titled with its identifier.
    LinearIssue {
        workspace: String,
        id: String,
        identifier: String,
    },
    /// A tab that offers what there is to open. A thread can have several, told apart by number.
    Blank(u32),
}

impl PanelTab {
    pub fn id(&self) -> String {
        match self {
            PanelTab::Blank(number) => format!("blank:{number}"),
            PanelTab::Diff => "diff".into(),
            PanelTab::Files => "files".into(),
            PanelTab::Agents => "agents".into(),
            PanelTab::PullRequest => "pull_request".into(),
            PanelTab::PullRequestNumber(number) => format!("pull_request:{number}"),
            PanelTab::PullRequests => "pull_requests".into(),
            PanelTab::Linear => "linear".into(),
            PanelTab::LinearIssue { id, .. } => format!("linear:{id}"),
            PanelTab::File(path) => format!("file:{path}"),
            PanelTab::Change { path, .. } => format!("change:{path}"),
        }
    }

    pub fn blank_number(&self) -> Option<u32> {
        match self {
            PanelTab::Blank(number) => Some(*number),
            _ => None,
        }
    }

    /// The file a tab is about.
    pub fn path(&self) -> Option<&str> {
        match self {
            PanelTab::File(path) | PanelTab::Change { path, .. } => Some(path),
            _ => None,
        }
    }

    pub fn title(&self) -> String {
        match self {
            PanelTab::Diff => "Diff".into(),
            PanelTab::Files => "Files".into(),
            PanelTab::Agents => "Agents".into(),
            PanelTab::PullRequest => "Pull Request".into(),
            PanelTab::PullRequestNumber(number) => format!("PR #{number}"),
            PanelTab::PullRequests => "Pull Requests".into(),
            PanelTab::Linear => "Linear".into(),
            PanelTab::LinearIssue { identifier, .. } => identifier.clone(),
            PanelTab::Blank(_) => "New Tab".into(),
            PanelTab::File(path) | PanelTab::Change { path, .. } => crate::models::last_component(path),
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            PanelTab::Diff | PanelTab::Change { .. } => "diff",
            PanelTab::Files => "folder",
            PanelTab::Agents => "users",
            PanelTab::PullRequest | PanelTab::PullRequestNumber(_) | PanelTab::PullRequests => "git-pull-request",
            PanelTab::Linear | PanelTab::LinearIssue { .. } => "linear",
            PanelTab::Blank(_) => "plus",
            PanelTab::File(path) => file_symbol(path),
        }
    }
}

/// The tabs a thread has open in the panel.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct PanelTabs {
    pub tabs: Vec<PanelTab>,
    pub active: Option<PanelTab>,
    /// What the diff tab shows, once that was chosen.
    pub scope: Option<DiffScope>,
    /// The panel covers the thread.
    pub maximized: Option<bool>,
}

impl PanelTabs {
    /// All there is is a blank tab, as before one was opened.
    pub fn is_blank(&self) -> bool {
        self.tabs.len() == 1 && self.tabs[0].blank_number().is_some()
    }
}

/// The folder the panel looks into: the one the open thread works in, or the project's when a
/// thread is about to start there.
#[derive(Clone, PartialEq, Debug)]
pub struct PanelTarget {
    /// What its tabs are kept under: the thread, or the draft.
    pub key: String,
    pub server_id: String,
    pub project_id: String,
    pub thread_id: Option<String>,
    pub name: String,
    pub repository: bool,
    /// The thread works in a worktree of its own.
    pub worktree: bool,
    /// The thread will start in a worktree that isn't made yet, so the folder is still the project's.
    pub awaits_worktree: bool,
    /// The number of the pull request of the branch it works on, when there is one.
    pub pull_request: Option<u64>,
}

#[derive(Clone, Debug)]
pub enum Loaded<T> {
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Loaded<T> {
    pub fn value(&self) -> Option<&T> {
        match self {
            Loaded::Ready(value) => Some(value),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub folder: bool,
    #[serde(default)]
    pub ignored: bool,
}

/// A row of the files tab: a file or a folder, as deep as the folders above it.
#[derive(Clone, PartialEq, Debug)]
pub struct FileNode {
    pub path: String,
    pub name: String,
    pub folder: bool,
    pub ignored: bool,
    pub depth: usize,
    pub open: bool,
}

/// What a line of a file's diff is, as the core says.
pub const UNCHANGED: u8 = 0;
pub const ADDED: u8 = 1;
pub const REMOVED: u8 = 2;
/// The heading of a hunk, or a note in place of the lines.
pub const NOTE: u8 = 3;

/// A file of a diff, or a whole file, as lines to draw.
#[derive(Clone, Debug)]
pub struct CodeFile {
    pub path: String,
    /// Where a renamed file was.
    pub from: Option<String>,
    pub change: Option<Change>,
    pub added: u32,
    pub removed: u32,
    pub lines: Vec<SharedString>,
    pub kinds: Vec<u8>,
    /// Each line's number in the file as it was and as it is, 0 where it isn't in that one.
    pub old: Vec<u32>,
    pub new: Vec<u32>,
    /// The length of the longest line, in columns.
    pub columns: usize,
    /// The highlighting of each line once it has arrived, as the core's span triples.
    pub spans: Vec<Vec<u32>>,
}

/// A file with more lines than this starts closed in a diff.
pub const OPEN_UP_TO_LINES: usize = 1500;

#[derive(Deserialize)]
struct DiffFile {
    path: String,
    from: Option<String>,
    change: Change,
    added: u32,
    removed: u32,
    binary: bool,
    lines: Vec<String>,
    kinds: Vec<u8>,
    old: Vec<u32>,
    new: Vec<u32>,
}

/// Counts what is wider than a letter as two columns, and a tab as four.
fn widest(lines: &[SharedString]) -> usize {
    lines
        .iter()
        .map(|line| {
            line.chars()
                .map(|character| match character {
                    '\t' => 4,
                    character if (character as u32) < 0x2e80 => 1,
                    _ => 2,
                })
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0)
}

impl CodeFile {
    fn from_diff(file: DiffFile) -> Self {
        if file.lines.is_empty() {
            let note = if file.binary {
                "Binary file"
            } else if file.change == Change::Renamed {
                "Renamed without changes"
            } else {
                "Empty file"
            };
            return Self {
                path: file.path,
                from: file.from,
                change: Some(file.change),
                added: file.added,
                removed: file.removed,
                columns: note.len(),
                lines: vec![note.into()],
                kinds: vec![NOTE],
                old: vec![0],
                new: vec![0],
                spans: Vec::new(),
            };
        }
        let lines: Vec<SharedString> = file.lines.into_iter().map(SharedString::from).collect();
        Self {
            path: file.path,
            from: file.from,
            change: Some(file.change),
            added: file.added,
            removed: file.removed,
            columns: widest(&lines),
            lines,
            kinds: file.kinds,
            old: file.old,
            new: file.new,
            spans: Vec::new(),
        }
    }

    /// A whole file, every line as it is.
    pub fn whole(path: String, lines: Vec<String>) -> Self {
        let count = lines.len();
        let lines: Vec<SharedString> = lines.into_iter().map(SharedString::from).collect();
        Self {
            path,
            from: None,
            change: None,
            added: 0,
            removed: 0,
            columns: widest(&lines),
            lines,
            kinds: vec![UNCHANGED; count],
            old: vec![0; count],
            new: (1..=count as u32).collect(),
            spans: Vec::new(),
        }
    }

    pub fn kind(&self, line: usize) -> u8 {
        self.kinds.get(line).copied().unwrap_or(UNCHANGED)
    }
}

/// Files to draw one under the other: the files of a diff, or one whole file.
#[derive(Clone, Debug)]
pub struct CodeDocument {
    /// Names what it shows. A document with the same name takes the place of the one before it
    /// without the view moving.
    pub id: String,
    pub files: Vec<CodeFile>,
    pub truncated: bool,
    /// Every file is under a heading with its name, as in a diff.
    pub headed: bool,
}

impl CodeDocument {
    pub fn from_diff(value: Value, id: String) -> Self {
        let truncated = value["truncated"].as_bool().unwrap_or(false);
        let files: Vec<DiffFile> = serde_json::from_value(value["files"].clone()).unwrap_or_default();
        Self { id, files: files.into_iter().map(CodeFile::from_diff).collect(), truncated, headed: true }
    }

    pub fn added(&self) -> u32 {
        self.files.iter().map(|file| file.added).sum()
    }

    pub fn removed(&self) -> u32 {
        self.files.iter().map(|file| file.removed).sum()
    }
}

/// What a pull request's diff shows besides its lines: which files the user has viewed, and the
/// lines that can have comments and those that have some.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct CodeMarks {
    /// Each file's heading has a box to mark it viewed.
    pub viewable: bool,
    pub viewed: HashSet<String>,
    /// A line's number can be clicked to comment on it.
    pub commentable: bool,
    /// The lines with conversations or comments waiting, by file, by their index in it.
    pub marked: HashMap<String, HashSet<usize>>,
}

#[derive(Clone, Debug)]
pub enum FileContent {
    Text(CodeDocument, bool),
    /// The image's file on this device.
    Image(String),
    Binary(u64),
}

fn read_file(answer: Value, path: &str, id: String) -> FileContent {
    match answer["kind"].as_str() {
        Some("text") => {
            let lines: Vec<String> = serde_json::from_value(answer["lines"].clone()).unwrap_or_default();
            let document = CodeDocument {
                id,
                files: vec![CodeFile::whole(path.to_string(), lines)],
                truncated: false,
                headed: false,
            };
            FileContent::Text(document, answer["truncated"].as_bool().unwrap_or(false))
        }
        Some("image") => match answer["file"].as_str() {
            Some(file) => FileContent::Image(file.to_string()),
            None => FileContent::Binary(answer["size"].as_u64().unwrap_or(0)),
        },
        _ => FileContent::Binary(answer["size"].as_u64().unwrap_or(0)),
    }
}

/// The panel beside the thread.
pub struct SidePanel {
    pub is_open: bool,
    pub tabs_by_key: BTreeMap<String, PanelTabs>,
    /// The turns of the open thread that changed files, the first one first.
    pub turns: Vec<TurnChange>,
    /// The agent whose transcript the agents tab shows. Without one it lists them.
    pub shown_agent: Option<String>,

    pub diff: Loaded<CodeDocument>,
    /// The files of the diff that are closed, by path.
    pub collapsed: HashSet<String>,
    /// The file of the diff to bring into view, and a count that goes up with every request.
    pub reveal: Option<(String, u64)>,

    /// What each folder that was looked into has in it, by its path.
    pub listings: HashMap<String, Vec<FileEntry>>,
    pub open_folders: HashSet<String>,
    pub files_error: Option<String>,
    /// What the tabs of one file show: the file, or what a turn changed in it.
    pub contents: HashMap<PanelTab, Loaded<FileContent>>,

    /// The folder all of the above is of.
    shown: Option<PanelTarget>,
    shown_scope: Option<DiffScope>,
    diff_request: u64,
    file_requests: HashMap<u64, PanelTab>,
}

impl SidePanel {
    pub fn new(is_open: bool, tabs_by_key: BTreeMap<String, PanelTabs>) -> Self {
        Self {
            is_open,
            tabs_by_key,
            turns: Vec::new(),
            shown_agent: None,
            diff: Loaded::Loading,
            collapsed: HashSet::new(),
            reveal: None,
            listings: HashMap::new(),
            open_folders: HashSet::new(),
            files_error: None,
            contents: HashMap::new(),
            shown: None,
            shown_scope: None,
            diff_request: 0,
            file_requests: HashMap::new(),
        }
    }

    /// The rows of the files tab: every folder that is open with what is in it.
    pub fn nodes(&self) -> Vec<FileNode> {
        let mut nodes = Vec::new();
        self.list_nodes("", 0, &mut nodes);
        nodes
    }

    fn list_nodes(&self, folder: &str, depth: usize, nodes: &mut Vec<FileNode>) {
        for entry in self.listings.get(folder).into_iter().flatten() {
            let path = if folder.is_empty() { entry.name.clone() } else { format!("{folder}/{}", entry.name) };
            let open = entry.folder && self.open_folders.contains(&path);
            nodes.push(FileNode {
                path: path.clone(),
                name: entry.name.clone(),
                folder: entry.folder,
                ignored: entry.ignored,
                depth,
                open,
            });
            if open {
                self.list_nodes(&path, depth + 1, nodes);
            }
        }
    }

    /// Forgets what was shown when the folder is another one than before.
    fn look_into(&mut self, target: &PanelTarget) {
        if self.shown.as_ref() == Some(target) {
            return;
        }
        let same_folder = self.shown.as_ref().is_some_and(|shown| shown.key == target.key);
        self.shown = Some(target.clone());
        if same_folder {
            return;
        }
        self.shown_scope = None;
        self.diff = Loaded::Loading;
        self.collapsed.clear();
        self.listings.clear();
        self.open_folders.clear();
        self.files_error = None;
        self.contents.clear();
        self.file_requests.clear();
    }
}

fn target_request(target: &PanelTarget) -> (String, Option<String>) {
    (target.project_id.clone(), target.thread_id.clone())
}

impl Store {
    fn panel_key(&self) -> Option<String> {
        self.panel_target().map(|target| target.key)
    }

    /// The thread's tabs. There is always one: a blank one when none was opened.
    pub fn panel_tabs(&self) -> PanelTabs {
        let mut tabs =
            self.panel_key().and_then(|key| self.side_panel.tabs_by_key.get(&key).cloned()).unwrap_or_default();
        if tabs.tabs.is_empty() {
            tabs.tabs = vec![PanelTab::Blank(0)];
        }
        if tabs.active.is_none() {
            tabs.active = tabs.tabs.first().cloned();
        }
        tabs
    }

    fn change_tabs(&mut self, change: impl FnOnce(&mut PanelTabs)) {
        let Some(key) = self.panel_key() else { return };
        let mut tabs = self.panel_tabs();
        change(&mut tabs);
        let untouched = tabs.is_blank() && tabs.scope.is_none() && tabs.maximized.is_none();
        if untouched {
            self.side_panel.tabs_by_key.remove(&key);
        } else {
            self.side_panel.tabs_by_key.insert(key, tabs);
        }
        let saved = self.side_panel.tabs_by_key.clone();
        self.prefs.set("panel.tabs", saved);
    }

    pub fn set_panel_open(&mut self, open: bool) {
        self.side_panel.is_open = open;
        self.prefs.set("panel.open", open);
        if !open {
            self.change_tabs(|tabs| tabs.maximized = None);
        }
    }

    /// Shows the tab, opening it and the panel when they aren't. It takes the place of the blank
    /// tab it is opened from.
    pub fn open_tab(&mut self, tab: PanelTab) {
        self.change_tabs(|tabs| {
            let blank =
                tabs.tabs.iter().position(|open| Some(open) == tabs.active.as_ref() && open.blank_number().is_some());
            if tabs.tabs.contains(&tab) {
                if let Some(blank) = blank {
                    tabs.tabs.remove(blank);
                }
            } else if let Some(blank) = blank {
                tabs.tabs[blank] = tab.clone();
            } else {
                tabs.tabs.push(tab.clone());
            }
            tabs.active = Some(tab);
        });
        self.set_panel_open(true);
    }

    /// Adds a blank tab. A hidden panel that only has its blank tab just opens.
    pub fn open_blank_tab(&mut self) {
        if !self.side_panel.is_open && self.panel_tabs().is_blank() {
            self.set_panel_open(true);
            return;
        }
        self.change_tabs(|tabs| {
            let next = tabs.tabs.iter().filter_map(PanelTab::blank_number).max().map_or(0, |number| number + 1);
            let tab = PanelTab::Blank(next);
            tabs.tabs.push(tab.clone());
            tabs.active = Some(tab);
        });
        self.set_panel_open(true);
    }

    pub fn activate_tab(&mut self, tab: PanelTab) {
        self.change_tabs(|tabs| tabs.active = Some(tab));
    }

    /// Shows the tab `offset` places from the active one, wrapping around the ends.
    pub fn activate_tab_offset(&mut self, offset: isize) {
        let tabs = self.panel_tabs();
        if !self.side_panel.is_open || tabs.tabs.len() < 2 {
            return;
        }
        let Some(index) = tabs.active.as_ref().and_then(|active| tabs.tabs.iter().position(|tab| tab == active)) else {
            return;
        };
        let count = tabs.tabs.len() as isize;
        let next = ((index as isize + offset) % count + count) % count;
        self.activate_tab(tabs.tabs[next as usize].clone());
    }

    /// Closes the tab. The one beside it is shown in its place.
    pub fn close_tab(&mut self, tab: &PanelTab) {
        self.change_tabs(|tabs| {
            let Some(index) = tabs.tabs.iter().position(|open| open == tab) else { return };
            tabs.tabs.remove(index);
            if tabs.active.as_ref() != Some(tab) {
                return;
            }
            tabs.active =
                if tabs.tabs.is_empty() { None } else { Some(tabs.tabs[index.min(tabs.tabs.len() - 1)].clone()) };
        });
        self.side_panel.contents.remove(tab);
    }

    pub fn close_other_tabs(&mut self, tab: &PanelTab) {
        self.change_tabs(|tabs| {
            tabs.tabs = vec![tab.clone()];
            tabs.active = Some(tab.clone());
        });
    }

    pub fn close_all_tabs(&mut self) {
        self.change_tabs(|tabs| {
            tabs.tabs.clear();
            tabs.active = None;
        });
    }

    /// What ⌘W does while the panel is open: closes its tab, or hides it when all it has is a
    /// blank one. `false` when the panel is hidden.
    pub fn close_active_tab(&mut self) -> bool {
        if !self.side_panel.is_open {
            return false;
        }
        let tabs = self.panel_tabs();
        let Some(active) = tabs.active.clone().filter(|_| !tabs.is_blank()) else {
            self.set_panel_open(false);
            return true;
        };
        self.close_tab(&active);
        true
    }

    /// The panel covers the thread, so the window shows the sidebar and the panel.
    pub fn panel_maximized(&self) -> bool {
        self.side_panel.is_open && self.panel_tabs().maximized == Some(true)
    }

    /// A panel that has no folder to show stays beside the thread.
    pub fn can_maximize_panel(&self) -> bool {
        self.side_panel.is_open && self.panel_key().is_some()
    }

    pub fn toggle_panel_maximized(&mut self) {
        if !self.can_maximize_panel() {
            return;
        }
        let maximized = !self.panel_maximized();
        self.change_tabs(|tabs| tabs.maximized = maximized.then_some(true));
        if maximized {
            // The composer is behind the panel now, and must not take what is typed.
            self.blur_composer += 1;
        }
    }

    /// The tabs a draft had go to the thread it became.
    pub fn move_tabs(&mut self, draft_key: &str, thread_key: &str) {
        let Some(tabs) = self.side_panel.tabs_by_key.remove(draft_key) else { return };
        self.side_panel.tabs_by_key.insert(thread_key.to_string(), tabs);
        if self.side_panel.shown.as_ref().is_some_and(|shown| shown.key == draft_key) {
            self.side_panel.shown = None;
        }
    }

    pub fn forget_tabs(&mut self, key: &str) {
        self.side_panel.tabs_by_key.remove(key);
    }

    // What the panel can show here

    /// Why no pull request can be shown here, when none can.
    pub fn pull_requests_unavailable(&self) -> Option<String> {
        let target = self.panel_target()?;
        let server = self.server(Some(&target.server_id))?;
        if server.protocol_version < 8 {
            return Some(format!("Update {} to see pull requests here.", server.name));
        }
        (!target.repository).then(|| "This folder isn't a git repository.".to_string())
    }

    /// Why the thread's pull request tab has nothing to show here, when it hasn't.
    pub fn pull_request_unavailable(&self) -> Option<String> {
        let target = self.panel_target()?;
        self.pull_requests_unavailable()
            .or_else(|| target.pull_request.is_none().then(|| "This branch has no pull request yet.".to_string()))
    }

    /// The server lists, links, edits and watches pull requests, and reviews their lines.
    pub fn pull_requests_extended(&self) -> bool {
        let Some(target) = self.panel_target() else { return false };
        self.server(Some(&target.server_id)).is_some_and(|server| server.protocol_version >= 9)
    }

    /// Opens the pull request in a tab: the thread's own tab when it is the thread's.
    pub fn show_pull_request_tab(&mut self, number: u64, target: &PanelTarget) {
        let tab = if Some(number) == target.pull_request {
            PanelTab::PullRequest
        } else {
            PanelTab::PullRequestNumber(number)
        };
        self.open_tab(tab);
    }

    // Agents

    /// Opens the agents tab on what the agent did that the tool call started.
    pub fn show_agent(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(thread_id) = self.transcript.read(cx).thread_id.clone() else { return };
        self.side_panel.shown_agent = Some(id.clone());
        self.agent_transcript.update(cx, |transcript, _| transcript.begin(Some(thread_id.clone())));
        self.agents_changed(cx);
        self.send(Command::OpenAgent { thread_id, agent_id: id });
        self.open_tab(PanelTab::Agents);
    }

    /// Goes back to the list of agents.
    pub fn show_agents(&mut self, cx: &mut Context<Self>) {
        if self.side_panel.shown_agent.take().is_none() {
            return;
        }
        let thread_id = self.agent_transcript.read(cx).thread_id.clone();
        if let Some(thread_id) = thread_id {
            self.send(Command::CloseAgent { thread_id });
        }
        self.agent_transcript.update(cx, |transcript, _| transcript.begin(None));
    }

    /// The shown agent's transcript says that it works for as long as it does.
    pub fn agents_changed(&mut self, cx: &mut Context<Self>) {
        use crate::models::AgentViewExt;
        let Some(agent) = self.agents.iter().find(|agent| Some(&agent.id) == self.side_panel.shown_agent.as_ref())
        else {
            return;
        };
        let activity = crate::models::Activity {
            running: agent.working(),
            started_at: Some(agent.started_at),
            ..Default::default()
        };
        self.agent_transcript.update(cx, |transcript, _| {
            if transcript.activity != activity {
                transcript.set_activity(activity);
            }
        });
    }

    // Diff

    /// What the diff tab shows: what was chosen, or the turn's work as far as it is known.
    pub fn diff_scope(&self, target: &PanelTarget) -> DiffScope {
        if let Some(chosen) = self.panel_tabs().scope.filter(|chosen| self.can_show(chosen, target)) {
            return chosen;
        }
        if target.worktree { DiffScope::Branch } else { DiffScope::Uncommitted }
    }

    /// A turn that is no longer known, or a pull request the folder no longer has, isn't shown.
    fn can_show(&self, scope: &DiffScope, target: &PanelTarget) -> bool {
        match scope {
            DiffScope::Turn { item_id } => self.side_panel.turns.iter().any(|turn| &turn.id == item_id),
            DiffScope::PullRequest { number } => Some(*number) == target.pull_request,
            DiffScope::Commit { .. } | DiffScope::Uncommitted | DiffScope::Branch => true,
        }
    }

    pub fn choose_diff_scope(&mut self, scope: DiffScope) {
        self.change_tabs(|tabs| tabs.scope = Some(scope));
    }

    /// Opens the diff tab on `scope`, with the file at `path` in view.
    pub fn show_diff(&mut self, scope: Option<DiffScope>, revealing: Option<String>) {
        if let Some(scope) = scope {
            self.choose_diff_scope(scope);
        }
        if let Some(path) = revealing {
            let count = self.side_panel.reveal.as_ref().map_or(0, |reveal| reveal.1) + 1;
            self.side_panel.reveal = Some((path, count));
        }
        self.open_tab(PanelTab::Diff);
    }

    /// Opens what the turn changed in the file in a tab of its own. A file has one such tab,
    /// which shows the turn that was asked for last.
    pub fn show_change(&mut self, turn: String, path: String) {
        let tab = PanelTab::Change { turn, path };
        let mut replaced = None;
        self.change_tabs(|tabs| {
            let Some(index) = tabs.tabs.iter().position(|open| open.id() == tab.id()) else { return };
            if tabs.tabs[index] == tab {
                return;
            }
            replaced = Some(tabs.tabs[index].clone());
            tabs.tabs[index] = tab.clone();
        });
        if let Some(replaced) = replaced {
            self.side_panel.contents.remove(&replaced);
        }
        self.open_tab(tab);
    }

    /// What the diff's menu and a change's tab call the turn.
    pub fn turn_name(&self, id: &str) -> String {
        let turns = &self.side_panel.turns;
        let Some(turn) = turns.iter().find(|turn| turn.id == id) else { return "Earlier turn".into() };
        if Some(&turn.id) == turns.last().map(|last| &last.id) {
            return "Latest turn".into();
        }
        format!("Turn at {}", stamp(turn.at))
    }

    /// Asks the server for the diff. What is shown stays until the answer is there, unless it
    /// is of another folder or scope.
    pub fn load_diff(&mut self, target: &PanelTarget, scope: DiffScope) {
        self.side_panel.look_into(target);
        if self.side_panel.shown_scope.as_ref() != Some(&scope) {
            self.side_panel.shown_scope = Some(scope.clone());
            self.side_panel.diff = Loaded::Loading;
            self.side_panel.collapsed.clear();
        }
        let (project_id, thread_id) = target_request(target);
        let id = format!("{}/{scope:?}", target.key);
        let fresh = self.side_panel.diff.value().is_none_or(|document| document.id != id);
        let target = target.clone();
        let command = Command::Diff {
            server_id: target.server_id.clone(),
            project_id,
            thread_id,
            scope: scope.clone(),
            path: None,
        };
        let read_id = id.clone();
        self.side_panel.diff_request = self.ask_read(
            command,
            move |value| CodeDocument::from_diff(value, read_id),
            move |store, result: Result<CodeDocument, String>, _| {
                if store.side_panel.shown.as_ref() != Some(&target)
                    || store.side_panel.shown_scope.as_ref() != Some(&scope)
                {
                    return;
                }
                match result {
                    Ok(document) => {
                        if fresh {
                            store.side_panel.collapsed = document
                                .files
                                .iter()
                                .filter(|file| file.lines.len() > OPEN_UP_TO_LINES)
                                .map(|file| file.path.clone())
                                .collect();
                        }
                        store.side_panel.diff = Loaded::Ready(document);
                    }
                    Err(error) => store.side_panel.diff = Loaded::Failed(error),
                }
            },
        );
    }

    pub fn toggle_collapsed(&mut self, path: &str) {
        if !self.side_panel.collapsed.remove(path) {
            self.side_panel.collapsed.insert(path.to_string());
        }
    }

    pub fn set_all_collapsed(&mut self, closed: bool) {
        self.side_panel.collapsed = if closed {
            self.side_panel
                .diff
                .value()
                .map(|document| document.files.iter().map(|file| file.path.clone()).collect())
                .unwrap_or_default()
        } else {
            HashSet::new()
        };
    }

    // Files

    /// Reads the folders that have been looked into again, the folder itself first.
    pub fn load_files(&mut self, target: &PanelTarget) {
        self.side_panel.look_into(target);
        let mut folders: Vec<String> = self.side_panel.listings.keys().cloned().collect();
        if !folders.iter().any(String::is_empty) {
            folders.push(String::new());
        }
        folders.sort();
        for folder in folders {
            self.list_files_in(folder, target);
        }
    }

    pub fn toggle_folder(&mut self, path: &str) {
        if self.side_panel.open_folders.remove(path) {
            return;
        }
        self.side_panel.open_folders.insert(path.to_string());
        if self.side_panel.listings.contains_key(path) {
            return;
        }
        let Some(shown) = self.side_panel.shown.clone() else { return };
        self.list_files_in(path.to_string(), &shown);
    }

    fn list_files_in(&mut self, folder: String, target: &PanelTarget) {
        let (project_id, thread_id) = target_request(target);
        let request = Request::ListFiles { project_id, thread_id, path: folder.clone() };
        let target = target.clone();
        self.request_then(&target.server_id.clone(), request, move |store, result, _| {
            if store.side_panel.shown.as_ref() != Some(&target) {
                return;
            }
            match result {
                Ok(answer) => {
                    let entries: Vec<FileEntry> = serde_json::from_value(answer["entries"].clone()).unwrap_or_default();
                    if store.side_panel.listings.get(&folder) != Some(&entries) {
                        store.side_panel.listings.insert(folder.clone(), entries);
                    }
                    if folder.is_empty() {
                        store.side_panel.files_error = None;
                    }
                }
                Err(error) => {
                    // A folder that has gone closes; only the folder itself says what went wrong.
                    store.side_panel.listings.remove(&folder);
                    store.side_panel.open_folders.remove(&folder);
                    if folder.is_empty() {
                        store.side_panel.files_error = Some(error);
                    }
                }
            }
        });
    }

    /// Asks the server for the file. What is shown of it stays until the answer is there.
    pub fn load_file(&mut self, path: &str, target: &PanelTarget) {
        let (project_id, thread_id) = target_request(target);
        let id = format!("{}/{path}", target.key);
        let command =
            Command::File { server_id: target.server_id.clone(), project_id, thread_id, path: path.to_string() };
        let read_path = path.to_string();
        self.load_content(PanelTab::File(path.to_string()), command, target, move |value| {
            read_file(value, &read_path, id)
        });
    }

    /// Asks the server for what the turn changed in the file.
    pub fn load_change(&mut self, turn: &str, path: &str, target: &PanelTarget) {
        let (project_id, thread_id) = target_request(target);
        let id = format!("{}/{turn}/{path}", target.key);
        let command = Command::Diff {
            server_id: target.server_id.clone(),
            project_id,
            thread_id,
            scope: DiffScope::Turn { item_id: turn.to_string() },
            path: Some(path.to_string()),
        };
        let tab = PanelTab::Change { turn: turn.to_string(), path: path.to_string() };
        self.load_content(tab, command, target, move |value| {
            let truncated = value["truncated"].as_bool().unwrap_or(false);
            FileContent::Text(CodeDocument::from_diff(value, id), truncated)
        });
    }

    fn load_content(
        &mut self,
        tab: PanelTab,
        command: Command,
        target: &PanelTarget,
        read: impl FnOnce(Value) -> FileContent + Send + 'static,
    ) {
        self.side_panel.look_into(target);
        self.side_panel.contents.entry(tab.clone()).or_insert(Loaded::Loading);
        let target = target.clone();
        let answered = tab.clone();
        let request = self.ask_read(command, read, move |store, result: Result<FileContent, String>, _| {
            if store.side_panel.shown.as_ref() != Some(&target) {
                return;
            }
            let loaded = match result {
                Ok(content) => Loaded::Ready(content),
                Err(error) => Loaded::Failed(error),
            };
            store.side_panel.contents.insert(answered, loaded);
        });
        self.side_panel.file_requests.retain(|_, requested| requested != &tab);
        self.side_panel.file_requests.insert(request, tab);
    }

    /// The highlighting of what a request answered with has arrived.
    pub fn colour_code(&mut self, request: u64, file: usize, lines: Vec<Vec<u32>>) {
        let panel = &mut self.side_panel;
        let document = if request == panel.diff_request {
            match &mut panel.diff {
                Loaded::Ready(document) => Some(document),
                _ => None,
            }
        } else if let Some(tab) = panel.file_requests.get(&request) {
            match panel.contents.get_mut(tab) {
                Some(Loaded::Ready(FileContent::Text(document, _))) => Some(document),
                _ => None,
            }
        } else {
            None
        };
        let Some(document) = document else { return };
        let Some(code) = document.files.get_mut(file) else { return };
        if code.lines.len() != lines.len() {
            return;
        }
        code.spans = lines;
    }
}

/// "14:03", or with the day when it isn't today.
pub fn stamp(at: f64) -> String {
    use chrono::{Local, TimeZone};
    let Some(time) = Local.timestamp_opt(at as i64, 0).single() else { return String::new() };
    if time.date_naive() == Local::now().date_naive() {
        time.format("%H:%M").to_string()
    } else {
        time.format("%-d %b %Y, %H:%M").to_string()
    }
}

/// The symbol a file is shown with, by what its name ends in.
pub fn file_symbol(path: &str) -> &'static str {
    let extension =
        Path::new(path).extension().and_then(|extension| extension.to_str()).unwrap_or_default().to_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "bmp" | "tiff" | "ico" | "svg" => "image",
        "md" | "markdown" | "txt" | "rst" => "file-text",
        "json" | "yaml" | "yml" | "toml" | "xml" | "plist" | "lock" => "braces",
        "sh" | "bash" | "zsh" | "fish" => "terminal",
        "" => "file",
        _ => "code",
    }
}
