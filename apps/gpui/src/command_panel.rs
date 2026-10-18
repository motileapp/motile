//! The panel that opens over the window to start a thread, open one, add a project, choose a
//! project's icon or run a command, all from the keyboard. ⌘N and the new-thread button open it on the projects when
//! there is more than one, ⌘P on the threads and ⌘K on the commands.

use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::{GitHubState, Repo, RestartWhen};

use crate::models::{FolderListing, Project, Server, ThreadInfo, ago, ask_phrase, interruption_label, last_component};
use crate::store::settings::SettingsSection;
use crate::store::{AddingProject, PanelPage, Selection, Store};
use crate::theme::{Colors, ControlSize, Layer, Opacity, Radius, ShadowSize, Surface, colors};
use crate::ui::{ActionButton, InputField, InputVariant, KeyCaps, Spinner, icons, logos};

const WIDTH: f32 = 620.;
const RADIUS: f32 = Radius::XL;
const ROW_HEIGHT: f32 = 52.;
const ROW_RADIUS: f32 = Radius::MD;
const SIDE_MARGIN: f32 = 8.;
const TITLE_HEIGHT: f32 = 17.;
const DETAIL_HEIGHT: f32 = 15.;
const NOTICE_HEIGHT: f32 = 36.;
const MAX_HEIGHT: f32 = 420.;
/// The layer the card is, which its rows, bars and key caps lie on.
const SURFACE: Surface = Surface::Popover;

type Action = Rc<dyn Fn(&mut CommandPanel, &mut Window, &mut Context<CommandPanel>)>;

enum ItemIcon {
    Symbol(&'static str),
    /// A logo, drawn in the colour of the symbols.
    GitHub,
    Project(Box<Option<Project>>),
}

struct Item {
    id: String,
    title: String,
    detail: String,
    /// Shown instead of `detail`, each part after its symbol.
    detail_parts: Vec<(&'static str, String)>,
    icon: ItemIcon,
    /// The digit that runs it with ⌘, or Ctrl off the Mac.
    shortcut: Option<usize>,
    /// The keys of the command it runs, as the shortcuts have them.
    keys: Option<String>,
    /// It leads to another page of the panel.
    keeps_open: bool,
    /// Its place among what can be selected, for moving through with the arrow keys.
    index: Option<usize>,
    /// Said at the row's end, as a warning with `warns`.
    note: Option<String>,
    warns: bool,
    /// It is shown and can't be selected.
    off: bool,
    /// What it started is still going on.
    busy: bool,
    /// Stands in for a row that is on its way, with as many lines of text.
    placeholder_lines: usize,
    /// What ⌘↩, or Ctrl+Enter off the Mac, does instead.
    alternate: Option<Action>,
    action: Action,
}

impl Item {
    fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        detail: impl Into<String>,
        icon: ItemIcon,
        action: impl Fn(&mut CommandPanel, &mut Window, &mut Context<CommandPanel>) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            detail: detail.into(),
            detail_parts: Vec::new(),
            icon,
            shortcut: None,
            keys: None,
            keeps_open: false,
            index: None,
            note: None,
            warns: false,
            off: false,
            busy: false,
            placeholder_lines: 0,
            alternate: None,
            action: Rc::new(action),
        }
    }

    fn keeps_open(mut self) -> Self {
        self.keeps_open = true;
        self
    }

    fn keys(mut self, keys: Option<String>) -> Self {
        self.keys = keys;
        self
    }

    fn placeholder(position: usize, lines: usize) -> Self {
        let mut item = Self::new(format!("placeholder-{position}"), "", "", ItemIcon::Symbol(""), |_, _, _| {});
        item.placeholder_lines = lines;
        item.index = Some(position);
        item
    }

    fn selectable(&self) -> bool {
        !self.off && self.placeholder_lines == 0
    }
}

struct Section {
    title: String,
    items: Vec<Item>,
}

/// A row of the list.
enum Entry {
    /// What is said when nothing was found.
    Empty(String),
    Title(usize),
    /// An item, by its section and its place in it.
    Row(usize, usize),
    /// What a server refused.
    Notice(String),
    /// The room under the last row.
    End,
}

pub struct CommandPanel {
    store: Entity<Store>,
    pub start: PanelPage,
    pages: Vec<PanelPage>,
    query: Entity<InputState>,
    highlighted: usize,
    /// The folders under the path typed on the folder page. Missing while they are asked for.
    listing: Option<FolderListing>,
    browse_error: Option<String>,
    browses_asked: u64,
    browses_answered: u64,
    copied: bool,
    /// What is offered, as it was last worked out, and whether it has to be worked out again.
    shown: Rc<Vec<Section>>,
    stale: bool,
    /// The list's rows: only the ones on screen are built.
    entries: Rc<Vec<Entry>>,
    list: ListState,
    entries_signature: u64,
    projects_added: u64,
    _subscriptions: Vec<Subscription>,
}

impl CommandPanel {
    pub fn new(store: Entity<Store>, start: PanelPage, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx));
        query.update(cx, |query, cx| query.focus(window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&query, window, |this, _, event: &InputEvent, window, cx| {
                if let InputEvent::Change = event {
                    this.query_changed(window, cx);
                }
            }),
            cx.observe_in(&store, window, |this, store, window, cx| {
                let added = store.read(cx).projects_added;
                if added != this.projects_added {
                    this.projects_added = added;
                    return this.close(cx);
                }
                this.follow_github(window, cx);
                this.refresh(cx);
            }),
        ];
        let projects_added = store.read(cx).projects_added;
        let mut panel = Self {
            store,
            start: start.clone(),
            pages: vec![start],
            query,
            highlighted: 0,
            listing: None,
            browse_error: None,
            browses_asked: 0,
            browses_answered: 0,
            copied: false,
            shown: Rc::default(),
            stale: true,
            entries: Rc::default(),
            list: ListState::new(0, ListAlignment::Top, px(ROW_HEIGHT * 4.)),
            entries_signature: 0,
            projects_added,
            _subscriptions: subscriptions,
        };
        panel.arrive(window, cx);
        panel
    }

    fn page(&self) -> PanelPage {
        self.pages.last().cloned().unwrap_or(PanelPage::Commands)
    }

    fn query_text(&self, cx: &App) -> String {
        self.query.read(cx).value().to_string()
    }

    /// Types the text, the cursor after it, which a long path is scrolled to.
    fn set_query(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.query.update(cx, |query, cx| {
            query.set_value(text, window, cx);
            query.set_scroll_offset(point(px(-f32::MAX), px(0.)), cx);
        });
        self.query_changed(window, cx);
    }

    /// What is offered starts over from the first, and the folders under a typed path are asked for.
    fn query_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.highlighted = 0;
        self.store.update(cx, |store, _| store.panel_notice = None);
        self.browse(window, cx);
        self.refresh(cx);
    }

    fn server_name(&self, id: &str, cx: &App) -> String {
        self.store.read(cx).server(Some(id)).map_or("your server".into(), |server| server.name.clone())
    }

    /// What is offered has changed: it is worked out again when it is next needed.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.stale = true;
        cx.notify();
    }

    /// What is offered now.
    fn current(&mut self, cx: &App) -> Rc<Vec<Section>> {
        if self.stale {
            self.shown = Rc::new(self.sections(cx));
            self.stale = false;
        }
        self.shown.clone()
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.close_panel();
            cx.notify();
        });
    }

    // What is offered

    fn sections(&self, cx: &App) -> Vec<Section> {
        let query = self.query_text(cx);
        let mut narrows = true;
        let section = |title: &str, items: Vec<Item>| Section { title: title.to_string(), items };
        let sections = match self.page() {
            PanelPage::Projects | PanelPage::DraftProject => {
                let mut items = self.project_items(cx);
                items.push(self.add_project());
                vec![section("Projects", items)]
            }
            PanelPage::Threads => vec![section("Threads", self.thread_items(cx))],
            PanelPage::Commands => {
                let mut sections =
                    vec![section("This thread", self.thread_commands(cx)), section("Commands", self.commands(cx))];
                // Searching from here looks through everything.
                if !query.is_empty() {
                    sections.push(section("Threads", self.thread_items(cx)));
                    sections.push(section("Start a thread in", self.project_items(cx)));
                }
                sections
            }
            PanelPage::Servers => vec![section("Servers", self.server_items(cx))],
            PanelPage::Sources(id) => {
                let title = if self.store.read(cx).live_servers().count() > 1 {
                    format!("Add a project on {}", self.server_name(&id, cx))
                } else {
                    "Add a project".into()
                };
                vec![section(&title, self.source_items(&id, cx))]
            }
            PanelPage::NewProject(id) => {
                narrows = false;
                vec![section("New project", vec![self.new_project(&id, cx)])]
            }
            PanelPage::Github(id) => {
                narrows = false;
                vec![section("Your GitHub", self.repo_items(&id, cx))]
            }
            PanelPage::GithubSetup(id) => {
                let name = self.server_name(&id, cx);
                let title = if self.store.read(cx).github.get(&id) == Some(&GitHubState::Missing) {
                    format!("GitHub's gh isn't installed on {name}")
                } else {
                    format!("GitHub isn't signed in on {name}")
                };
                vec![section(&title, self.setup_items(&id, cx))]
            }
            PanelPage::Folder(id) => {
                narrows = false;
                vec![section(&format!("Folders on {}", self.server_name(&id, cx)), self.folder_items(&id, cx))]
            }
            PanelPage::Icon(id) => {
                narrows = false;
                let name =
                    self.store.read(cx).project(Some(&id)).map_or("the project".into(), |project| project.name.clone());
                vec![section(&format!("Icon for {name}"), self.icon_items(&id, cx))]
            }
        };
        let lower = query.to_lowercase();
        let matches = |item: &Item| {
            query.is_empty()
                || item.placeholder_lines > 0
                || item.title.to_lowercase().contains(&lower)
                || item.detail.to_lowercase().contains(&lower)
        };
        let mut index = 0;
        sections
            .into_iter()
            .filter_map(|section| {
                let mut items: Vec<Item> = section.items.into_iter().filter(|item| !narrows || matches(item)).collect();
                if items.is_empty() {
                    return None;
                }
                for item in items.iter_mut().filter(|item| item.selectable()) {
                    item.index = Some(index);
                    index += 1;
                }
                Some(Section { title: section.title, items })
            })
            .collect()
    }

    /// The projects, the one the open thread works in first, then by when a thread last started,
    /// and "No project" of each server last.
    fn projects(&self, cx: &App) -> Vec<Project> {
        let store = self.store.read(cx);
        let mut projects: Vec<Project> = store.recent_projects().into_iter().cloned().collect();
        if let Some(current) = store.composer_project().filter(|project| !project.no_project && store.is_live(project))
        {
            projects.retain(|project| project.id != current.id);
            projects.insert(0, current);
        }
        projects.extend(store.live_no_projects().cloned());
        projects
    }

    fn project_items(&self, cx: &App) -> Vec<Item> {
        let page = self.page();
        let on_projects = matches!(page, PanelPage::Projects | PanelPage::DraftProject);
        let for_draft = page == PanelPage::DraftProject;
        self.projects(cx)
            .into_iter()
            .enumerate()
            .map(|(position, project)| {
                let server = self
                    .store
                    .read(cx)
                    .server(Some(&project.server_id))
                    .map(|server| server.short_name.clone())
                    .unwrap_or_default();
                let location = if project.no_project {
                    "A folder of its own for each thread".into()
                } else {
                    project.path.clone()
                };
                let parts = if server.is_empty() {
                    vec![("folder", location.clone())]
                } else {
                    vec![("server", server.clone()), ("folder", location.clone())]
                };
                let id = project.id.clone();
                let mut item = Item::new(
                    format!("project-{}", project.id),
                    project.name.clone(),
                    format!("{server} {location}"),
                    ItemIcon::Project(Box::new(Some(project))),
                    move |this, _, cx| {
                        let id = Some(id.clone());
                        this.store.update(cx, |store, cx| {
                            if for_draft {
                                store.set_new_thread_project(id, cx);
                            } else {
                                store.start_new_thread(id, cx);
                            }
                        });
                    },
                );
                item.detail_parts = parts;
                item.shortcut = (position < 9 && on_projects).then_some(position + 1);
                item
            })
            .collect()
    }

    fn add_project(&self) -> Item {
        Item::new(
            "add-project",
            "Add a project",
            "A new one, one of your GitHub's or a folder",
            ItemIcon::Symbol("folder-plus"),
            |this, window, cx| {
                let page = this.store.read(cx).add_project_page();
                this.open(page, window, cx);
            },
        )
        .keeps_open()
    }

    /// The servers a project can be added on: the open thread's first, the offline ones last.
    fn server_items(&self, cx: &App) -> Vec<Item> {
        let store = self.store.read(cx);
        let current = store.composer_server().map(|server| server.id.clone());
        let rank = |server: &Server| {
            if !server.connected() {
                2
            } else if Some(&server.id) == current.as_ref() {
                0
            } else {
                1
            }
        };
        let mut servers: Vec<(usize, &Server)> = store.live_servers().enumerate().collect();
        servers.sort_by_key(|(position, server)| (rank(server), *position));
        servers
            .into_iter()
            .map(|(_, server)| {
                let projects = store.projects.iter().filter(|project| project.server_id == server.id).count();
                let detail = match (server.connected(), projects) {
                    (false, _) => "Offline".to_string(),
                    (true, 1) => "1 project".to_string(),
                    (true, count) => format!("{count} projects"),
                };
                let id = server.id.clone();
                let mut item = Item::new(
                    format!("server-{}", server.id),
                    server.name.clone(),
                    detail,
                    ItemIcon::Symbol("server"),
                    move |this, window, cx| this.open(PanelPage::Sources(id.clone()), window, cx),
                )
                .keeps_open();
                item.off = !server.connected();
                item
            })
            .collect()
    }

    /// GitHub comes last while it still needs setting up on the server.
    fn source_items(&self, id: &str, cx: &App) -> Vec<Item> {
        let store = self.store.read(cx);
        let name = self.server_name(id, cx);
        let folder_id = id.to_string();
        let folder = Item::new(
            "source-folder",
            "Local folder",
            format!("Browse the folders on {name}"),
            ItemIcon::Symbol("folder"),
            move |this, window, cx| this.open(PanelPage::Folder(folder_id.clone()), window, cx),
        )
        .keeps_open();
        if !store.starts_projects(store.server(Some(id))) {
            return vec![folder];
        }
        let new_id = id.to_string();
        let new = Item::new(
            "source-new",
            "New project",
            "Start a new Git repository from a name",
            ItemIcon::Symbol("square-plus"),
            move |this, window, cx| this.open(PanelPage::NewProject(new_id.clone()), window, cx),
        )
        .keeps_open();
        let Some(state) = store.github.get(id).copied() else { return vec![new, Item::placeholder(0, 2), folder] };
        let github_id = id.to_string();
        let mut github =
            Item::new("source-github", "Your GitHub", "Clone one of your repositories", ItemIcon::GitHub, {
                move |this, window, cx| {
                    let page = if state == GitHubState::Ready {
                        PanelPage::Github(github_id.clone())
                    } else {
                        PanelPage::GithubSetup(github_id.clone())
                    };
                    this.open(page, window, cx)
                }
            })
            .keeps_open();
        if state == GitHubState::Ready {
            return vec![new, github, folder];
        }
        github.note = Some("Setup required".into());
        github.warns = true;
        vec![new, folder, github]
    }

    fn new_project(&self, id: &str, cx: &App) -> Item {
        let name = self.query_text(cx).trim().to_string();
        let title = if name.is_empty() { "Name the project".to_string() } else { format!("Create {name}") };
        let detail = format!("A new Git repository in ~/projects on {}", self.server_name(id, cx));
        let server = id.to_string();
        let typed = name.clone();
        let mut item = Item::new("create", title, detail, ItemIcon::Symbol("square-plus"), move |this, _, cx| {
            let (name, server) = (typed.clone(), server.clone());
            this.store.update(cx, |store, _| store.new_project_named(name, &server));
        })
        .keeps_open();
        item.off = name.is_empty();
        item.busy = self.store.read(cx).adding_project == Some(AddingProject { server_id: id.to_string(), name });
        item
    }

    fn repo_items(&self, id: &str, cx: &App) -> Vec<Item> {
        let store = self.store.read(cx);
        let Some(repos) = store.repos.get(id) else {
            if store.repo_errors.contains_key(id) {
                return Vec::new();
            }
            return (0..8).map(|position| Item::placeholder(position, 2)).collect();
        };
        let clone = |name: &str, title: String, detail: String, symbol: &'static str| {
            let (repo, server) = (name.to_string(), id.to_string());
            Item::new(format!("repo-{name}"), title, detail, ItemIcon::Symbol(symbol), move |this, _, cx| {
                let (repo, server) = (repo.clone(), server.clone());
                this.store.update(cx, |store, cx| store.clone_repo(repo, &server, cx));
            })
        };
        let typed = self.query_text(cx).trim().to_string();
        let mut items: Vec<Item> = found(repos, &typed.to_lowercase())
            .into_iter()
            .map(|repo| {
                let mut item =
                    clone(&repo.name, repo.name.clone(), repo.description.clone().unwrap_or_default(), "book-marked");
                item.note = repo.private.then(|| "Private".to_string());
                item
            })
            .collect();
        // A repository that isn't listed is cloned by its name.
        let listed = repos.iter().any(|repo| repo.name.eq_ignore_ascii_case(&typed));
        if !listed && is_repo_name(&typed) {
            items.push(clone(
                &typed,
                format!("Clone {typed}"),
                "A repository that isn't in your list".into(),
                "circle-arrow-down",
            ));
        }
        items
    }

    fn setup_items(&self, id: &str, cx: &App) -> Vec<Item> {
        let name = self.server_name(id, cx);
        let server = id.to_string();
        let check = Item::new(
            "check-github",
            "Check again",
            "Once that is done",
            ItemIcon::Symbol("rotate-cw"),
            move |this, _, cx| {
                let server = server.clone();
                this.store.update(cx, |store, _| store.read_github(&server));
            },
        )
        .keeps_open();
        if self.store.read(cx).github.get(id) == Some(&GitHubState::Missing) {
            let install = Item::new(
                "install-gh",
                "Open cli.github.com",
                format!("Install gh on {name}, then run gh auth login there"),
                ItemIcon::Symbol("square-arrow-out-up-right"),
                |_, _, cx| cx.open_url("https://cli.github.com"),
            )
            .keeps_open();
            return vec![install, check];
        }
        let copy = Item::new(
            "copy-login",
            if self.copied { "Copied" } else { "Copy gh auth login" },
            format!("Run it in a terminal on {name}"),
            ItemIcon::Symbol(if self.copied { "check" } else { "copy" }),
            |this, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string("gh auth login".into()));
                this.copied = true;
                this.refresh(cx);
            },
        )
        .keeps_open();
        vec![copy, check]
    }

    /// The folder that is typed, the one above it, and the folders in it.
    fn folder_items(&self, id: &str, cx: &App) -> Vec<Item> {
        let Some(listing) = &self.listing else {
            if self.browse_error.is_some() {
                return Vec::new();
            }
            return (0..8).map(|position| Item::placeholder(position, 1)).collect();
        };
        let store = self.store.read(cx);
        let projects: Vec<&str> = store
            .projects
            .iter()
            .filter(|project| project.server_id == id)
            .map(|project| project.path.as_str())
            .collect();
        let query = self.query_text(cx);
        let mut items = Vec::new();
        if query.ends_with('/') || query == "~" {
            let (server, path) = (id.to_string(), listing.path.clone());
            items.push(Item::new(
                "add-here",
                "Add this folder",
                listing.typed.clone(),
                ItemIcon::Symbol("folder-plus"),
                move |this, _, cx| {
                    let (server, path) = (server.clone(), path.clone());
                    this.store.update(cx, |store, cx| store.add_project_at(&server, path, cx));
                },
            ));
            if let Some(parent) = listing.parent.clone() {
                items.push(
                    Item::new("parent", "..", "", ItemIcon::Symbol("corner-left-up"), move |this, window, cx| {
                        this.set_query(parent.clone(), window, cx)
                    })
                    .keeps_open(),
                );
            }
        }
        for folder in &listing.folders {
            let typed = folder.typed.clone();
            let mut item = Item::new(
                folder.path.clone(),
                folder.name.clone(),
                "",
                ItemIcon::Symbol("folder"),
                move |this, window, cx| this.set_query(typed.clone(), window, cx),
            )
            .keeps_open();
            item.note = projects.contains(&folder.path.as_str()).then(|| "Project".to_string());
            let (server, path) = (id.to_string(), folder.path.clone());
            item.alternate = Some(Rc::new(move |this, _, cx| {
                let (server, path) = (server.clone(), path.clone());
                this.store.update(cx, |store, cx| store.add_project_at(&server, path, cx));
            }));
            items.push(item);
        }
        items
    }

    /// The icon in the project's folder, the folder above the typed one, and the folders and
    /// images in it.
    fn icon_items(&self, id: &str, cx: &App) -> Vec<Item> {
        let Some(project) = self.store.read(cx).project(Some(id)).cloned() else { return Vec::new() };
        let Some(listing) = &self.listing else {
            if self.browse_error.is_some() {
                return Vec::new();
            }
            return (0..8).map(|position| Item::placeholder(position, 1)).collect();
        };
        let set_icon = |path: Option<String>| {
            let project = project.clone();
            move |this: &mut CommandPanel, _: &mut Window, cx: &mut Context<CommandPanel>| {
                let (project, path) = (project.clone(), path.clone());
                this.store.update(cx, |store, _| store.set_icon(&project, path));
            }
        };
        let query = self.query_text(cx);
        let mut items = Vec::new();
        if query.ends_with('/') || query == "~" {
            items.push(Item::new(
                "folder-icon",
                "Use the icon in its folder",
                project.path.clone(),
                ItemIcon::Symbol("undo-2"),
                set_icon(None),
            ));
            if let Some(parent) = listing.parent.clone() {
                items.push(
                    Item::new("parent", "..", "", ItemIcon::Symbol("corner-left-up"), move |this, window, cx| {
                        this.set_query(parent.clone(), window, cx)
                    })
                    .keeps_open(),
                );
            }
        }
        for folder in &listing.folders {
            let typed = folder.typed.clone();
            items.push(
                Item::new(
                    folder.path.clone(),
                    folder.name.clone(),
                    "",
                    ItemIcon::Symbol("folder"),
                    move |this, window, cx| this.set_query(typed.clone(), window, cx),
                )
                .keeps_open(),
            );
        }
        for image in &listing.images {
            items.push(Item::new(
                image.path.clone(),
                image.name.clone(),
                "",
                ItemIcon::Symbol("image"),
                set_icon(Some(image.path.clone())),
            ));
        }
        items
    }

    /// The threads, then the ones agents started, each saying which thread started it.
    fn thread_items(&self, cx: &App) -> Vec<Item> {
        let store = self.store.read(cx);
        let mut started: Vec<&ThreadInfo> =
            store.threads.values().filter(|thread| thread.parent_id().is_some()).collect();
        started.sort_by(|one, other| other.updated_at.total_cmp(&one.updated_at));
        store
            .active_threads()
            .iter()
            .chain(store.done_threads())
            .chain(started)
            .map(|thread| {
                let project = store.project(Some(&thread.project_id)).cloned();
                let name = project.as_ref().map_or_else(|| last_component(&thread.cwd), |project| project.name.clone());
                let from = thread
                    .parent_id()
                    .and_then(|id| store.threads.get(id))
                    .map(|parent| format!(" · from {}", parent.title))
                    .unwrap_or_default();
                let state = if thread.is_done() {
                    "done".to_string()
                } else if let Some(ask) = thread.waits_for() {
                    ask_phrase(ask).into()
                } else if thread.working() {
                    "working".into()
                } else if thread.monitoring {
                    "monitoring".into()
                } else if let Some(interruption) = thread.interruption {
                    interruption_label(interruption).0.to_lowercase()
                } else {
                    ago(thread.updated_at)
                };
                let id = thread.id.clone();
                Item::new(
                    format!("thread-{}", thread.id),
                    thread.title.clone(),
                    format!("{name}{from} · {state}"),
                    ItemIcon::Project(Box::new(project)),
                    move |this, _, cx| {
                        let id = id.clone();
                        this.store.update(cx, |store, cx| store.select(Selection::Thread(id), cx));
                    },
                )
            })
            .collect()
    }

    /// What can be done with the open thread.
    fn thread_commands(&self, cx: &App) -> Vec<Item> {
        let store = self.store.read(cx);
        let Some(thread) = store.changeable_thread() else { return Vec::new() };
        if thread.busy() {
            let stop = Item::new(
                "stop",
                "Stop the agent",
                thread.title.clone(),
                ItemIcon::Symbol("circle-stop"),
                |this, _, cx| this.store.update(cx, |store, cx| store.stop_thread(cx)),
            );
            return vec![stop.keys(store.shortcuts.label("thread.stop"))];
        }
        let mut items = Vec::new();
        if thread.interruption.is_some() {
            items.push(Item::new(
                "continue",
                "Continue the agent",
                thread.title.clone(),
                ItemIcon::Symbol("play"),
                |this, _, cx| this.store.update(cx, |store, _| store.continue_thread()),
            ));
        }
        let done = thread.is_done();
        if !done && thread.waits_on_agents() {
            return items;
        }
        let mark = Item::new(
            "done",
            if done { "Mark undone" } else { "Mark done" },
            thread.title.clone(),
            ItemIcon::Symbol(if done { "undo-2" } else { "circle-check" }),
            |this, _, cx| this.store.update(cx, |store, cx| store.toggle_done(cx)),
        );
        items.push(mark.keys(store.shortcuts.label("thread.done")));
        items
    }

    /// The servers that run an older version than the newest release. One whose agents work is
    /// updated once they finish, or now, with their threads going on after.
    fn server_updates(&self, cx: &App) -> Vec<Item> {
        let store = self.store.read(cx);
        let latest = store.updater.latest.clone().unwrap_or_default();
        let update = |id: String, title: String, detail: String, server: &Server, when: RestartWhen| {
            let server = server.clone();
            Item::new(id, title, detail, ItemIcon::Symbol("circle-arrow-down"), move |this, _, cx| {
                this.store.update(cx, |store, _| store.update_server(&server, when))
            })
        };
        store
            .live_servers()
            .filter(|server| store.is_outdated(server) && store.server_update(server).is_none())
            .flat_map(|server| {
                let detail = format!("From version {} to {latest}", server.version);
                let (id, name) = (format!("update-{}", server.id), &server.name);
                if !store.is_busy(server) || !server.can_choose_restart() {
                    return vec![update(id, format!("Update {name}"), detail, server, RestartWhen::Idle)];
                }
                vec![
                    update(id, format!("Update {name} when agents finish"), detail, server, RestartWhen::Idle),
                    update(
                        format!("update-now-{}", server.id),
                        format!("Update {name} now"),
                        "Its agents stop and continue once it is back".into(),
                        server,
                        RestartWhen::Now,
                    ),
                ]
            })
            .collect()
    }

    fn commands(&self, cx: &App) -> Vec<Item> {
        let store = self.store.read(cx);
        let keys = |command: &str| store.shortcuts.label(command);
        let mut items = self.server_updates(cx);
        items.push(
            Item::new(
                "new-thread",
                "New thread",
                "Choose a project to start in",
                ItemIcon::Symbol("square-pen"),
                |this, window, cx| this.open(PanelPage::Projects, window, cx),
            )
            .keeps_open()
            .keys(keys("chat.new")),
        );
        items.push(
            Item::new(
                "go-to-thread",
                "Go to thread",
                format!("{} threads", store.threads.len()),
                ItemIcon::Symbol("message-square-text"),
                |this, window, cx| this.open(PanelPage::Threads, window, cx),
            )
            .keeps_open()
            .keys(keys("threadPicker.toggle")),
        );
        items.push(self.add_project());
        items.push(Item::new(
            "add-server",
            "Add a server",
            "A machine that runs your agents",
            ItemIcon::Symbol("server"),
            |this, _, cx| this.store.update(cx, |store, _| store.shows_add_server = true),
        ));
        items.push(Item::new(
            "check-updates",
            "Check for updates",
            format!("Motile {}", store.updater.current),
            ItemIcon::Symbol("refresh-cw"),
            |this, _, cx| this.store.update(cx, |store, cx| store.updater.check(true, cx)),
        ));
        items.push(
            Item::new(
                "settings",
                "Settings",
                "Appearance, servers and projects",
                ItemIcon::Symbol("settings"),
                |_, window, cx| window.dispatch_action(Box::new(crate::app_menu::Perform("settings.open")), cx),
            )
            .keys(keys("settings.open")),
        );
        items.push(Item::new(
            "shortcuts",
            "Keybindings",
            "Change the keys of every command",
            ItemIcon::Symbol("keyboard"),
            |this, _, cx| this.store.update(cx, |store, _| store.open_settings(SettingsSection::Keyboard, None)),
        ));
        items.push(
            Item::new(
                "usage",
                "Usage",
                "Limits, cost and tokens",
                ItemIcon::Symbol("chart-column"),
                |_, window, cx| window.dispatch_action(Box::new(crate::app_menu::Perform("usage.open")), cx),
            )
            .keys(keys("usage.open")),
        );
        items
    }

    // Acting

    fn open(&mut self, page: PanelPage, window: &mut Window, cx: &mut Context<Self>) {
        self.pages.push(page);
        self.arrive(window, cx);
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pages.len() < 2 {
            return;
        }
        self.pages.pop();
        self.arrive(window, cx);
    }

    /// Moves between the repositories and what GitHub still needs as the server's answer changes.
    fn follow_github(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let github = &self.store.read(cx).github;
        let replacement = match self.page() {
            PanelPage::Github(id) if github.get(&id) != Some(&GitHubState::Ready) => PanelPage::GithubSetup(id),
            PanelPage::GithubSetup(id) if github.get(&id) == Some(&GitHubState::Ready) => PanelPage::Github(id),
            _ => return,
        };
        if let Some(last) = self.pages.last_mut() {
            *last = replacement;
        }
        self.arrive(window, cx);
    }

    fn arrive(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.copied = false;
        self.listing = None;
        self.browse_error = None;
        let query = match self.page() {
            PanelPage::Folder(_) => "~/".into(),
            PanelPage::Icon(id) => {
                let path = self.store.read(cx).project(Some(&id)).map_or("~".into(), |project| project.path.clone());
                format!("{path}/")
            }
            _ => String::new(),
        };
        self.set_query(query, window, cx);
        let prompt = self.prompt(cx);
        self.query.update(cx, |query, cx| query.set_placeholder(prompt, window, cx));
        let lists_repos = match self.page() {
            PanelPage::Github(id) => Some(id),
            PanelPage::Sources(id) if self.store.read(cx).github.get(&id) == Some(&GitHubState::Ready) => Some(id),
            _ => None,
        };
        if let Some(id) = lists_repos {
            self.store.update(cx, |store, _| store.load_repos(&id, false));
        }
        self.refresh(cx);
    }

    /// Asks for the folders under the typed path. Placeholders take the place of the folders that
    /// are shown when the answer takes a while.
    fn browse(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some((id, icons)) = self.browsed(cx) else { return };
        self.browses_asked += 1;
        let asked = self.browses_asked;
        let query = self.query_text(cx);
        let panel = cx.entity().downgrade();
        self.store.update(cx, |store, _| {
            store.browse(&id, query, icons, move |_, result, cx| {
                let _ = panel.update(cx, |this, cx| {
                    if asked != this.browses_asked {
                        return;
                    }
                    this.browses_answered = asked;
                    match result {
                        Ok(found) => {
                            this.listing = Some(found);
                            this.browse_error = None;
                        }
                        Err(error) => {
                            this.listing = None;
                            this.browse_error = Some(error);
                        }
                    }
                    this.refresh(cx);
                });
            });
        });
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(150)).await;
            let _ = this.update(cx, |this, cx| {
                if asked == this.browses_asked && this.browses_answered < asked {
                    this.listing = None;
                    this.browse_error = None;
                    this.refresh(cx);
                }
            });
        })
        .detach();
    }

    /// The server whose folders the page lists, and whether with the images that can be an icon.
    fn browsed(&self, cx: &App) -> Option<(String, bool)> {
        match self.page() {
            PanelPage::Folder(id) => Some((id, false)),
            PanelPage::Icon(id) => Some((self.store.read(cx).project(Some(&id))?.server_id.clone(), true)),
            _ => None,
        }
    }

    fn run(&mut self, item: &Item, window: &mut Window, cx: &mut Context<Self>) {
        if !item.selectable() {
            return;
        }
        if !item.keeps_open {
            self.close(cx);
        }
        (item.action)(self, window, cx);
        self.store.update(cx, |_, cx| cx.notify());
    }

    fn run_highlighted(&mut self, alternate: bool, window: &mut Window, cx: &mut Context<Self>) {
        let sections = self.current(cx);
        let Some(item) =
            sections.iter().flat_map(|section| &section.items).find(|item| item.index == Some(self.highlighted))
        else {
            return;
        };
        match item.alternate.clone().filter(|_| alternate) {
            Some(alternate) => {
                self.close(cx);
                alternate(self, window, cx);
            }
            None => self.run(item, window, cx),
        }
    }

    fn move_highlight(&mut self, step: isize, cx: &mut Context<Self>) {
        let sections = self.current(cx);
        let count = sections.iter().flat_map(|section| &section.items).filter(|item| item.selectable()).count();
        if count == 0 {
            return;
        }
        self.highlighted = (self.highlighted as isize + step).clamp(0, count as isize - 1) as usize;
        let highlighted = self.highlighted;
        let entry = self.entries.iter().position(|entry| {
            matches!(entry, Entry::Row(section, item) if sections[*section].items[*item].index == Some(highlighted))
        });
        if let Some(entry) = entry {
            self.list.scroll_to_reveal_item(entry);
        }
        cx.notify();
    }

    /// Lists the repositories again, to find one made since.
    fn refresh_repos(&mut self, cx: &mut Context<Self>) {
        let PanelPage::Github(id) = self.page() else { return };
        self.store.update(cx, |store, cx| {
            store.load_repos(&id, true);
            cx.notify();
        });
    }

    /// Takes the keys the panel is steered with before the search field does.
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let command = event.keystroke.modifiers.secondary();
        let handled = match event.keystroke.key.as_str() {
            "escape" => {
                self.close(cx);
                true
            }
            "down" => {
                self.move_highlight(1, cx);
                true
            }
            "up" => {
                self.move_highlight(-1, cx);
                true
            }
            "enter" => {
                self.run_highlighted(command, window, cx);
                true
            }
            "backspace" if self.query_text(cx).is_empty() && self.pages.len() > 1 => {
                self.back(window, cx);
                true
            }
            "r" if command => {
                if !matches!(self.page(), PanelPage::Github(_)) {
                    return;
                }
                self.refresh_repos(cx);
                true
            }
            digit if command => {
                let Ok(digit) = digit.parse::<usize>() else { return };
                let sections = self.current(cx);
                let Some(item) =
                    sections.iter().flat_map(|section| &section.items).find(|item| item.shortcut == Some(digit))
                else {
                    return;
                };
                self.run(item, window, cx);
                true
            }
            _ => false,
        };
        if handled {
            cx.stop_propagation();
        }
    }

    // Parts

    fn prompt(&self, cx: &App) -> String {
        match self.page() {
            PanelPage::Commands => "Search threads, projects and commands".into(),
            PanelPage::Projects => "Start a thread in".into(),
            PanelPage::DraftProject => "Search projects".into(),
            PanelPage::Threads => "Go to thread".into(),
            PanelPage::Servers => "Add a project on".into(),
            PanelPage::Sources(_) => "Add a project".into(),
            PanelPage::NewProject(_) => "Project name".into(),
            PanelPage::Github(_) => "Search your repositories".into(),
            PanelPage::GithubSetup(_) => "Set up GitHub".into(),
            PanelPage::Folder(id) => format!("Path on {}", self.server_name(&id, cx)),
            PanelPage::Icon(id) => {
                let server = self.store.read(cx).project(Some(&id)).map(|project| project.server_id.clone());
                format!("Path to an image on {}", self.server_name(&server.unwrap_or_default(), cx))
            }
        }
    }

    fn empty_text(&self, cx: &App) -> String {
        match self.page() {
            PanelPage::Folder(_) => self.browse_error.clone().unwrap_or("No folders found".into()),
            PanelPage::Icon(_) => self.browse_error.clone().unwrap_or("No folders or images found".into()),
            PanelPage::Github(id) => {
                self.store.read(cx).repo_errors.get(&id).cloned().unwrap_or("No repository found".into())
            }
            _ => "Nothing found".into(),
        }
    }

    /// What a server refused, or why it couldn't list the repositories again.
    fn notice(&self, cx: &App) -> Option<String> {
        let store = self.store.read(cx);
        let PanelPage::Github(id) = self.page() else { return store.panel_notice.clone() };
        if !store.repos.contains_key(&id) {
            return store.panel_notice.clone();
        }
        store.panel_notice.clone().or_else(|| store.repo_errors.get(&id).cloned())
    }

    /// The pages that fill as the server answers keep one height, so nothing moves when it does.
    fn height(&self, sections: &[Section], rows: usize, notice: bool) -> f32 {
        if matches!(self.page(), PanelPage::Github(_) | PanelPage::Folder(_) | PanelPage::Icon(_)) {
            return MAX_HEIGHT;
        }
        let notice = if notice { NOTICE_HEIGHT } else { 0. };
        (rows as f32 * ROW_HEIGHT + sections.len() as f32 * 32. + 10. + notice).clamp(90., MAX_HEIGHT)
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let leading: AnyElement = if self.pages.len() > 1 {
            ActionButton::icon("panel-back", "arrow-left", "Back")
                .surface(SURFACE)
                .on_click(cx.listener(|this, _, window, cx| this.back(window, cx)))
                .into_any_element()
        } else {
            div()
                .size(px(ControlSize::Regular.height()))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .child(icons::symbol("search", 15.).text_color(c.muted_stronger_foreground))
                .into_any_element()
        };
        let field_size = ControlSize::Large;
        let refresh_help = format!("Refresh ({})", self.store.read(cx).shortcuts.keys_label("mod+r"));
        let refresh = match self.page() {
            PanelPage::Github(id) => Some(self.store.read(cx).repo_listing.listing.contains(&id)),
            _ => None,
        };
        div()
            .h(px(52.))
            .px(px(16.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(leading)
            .child(
                div().flex_1().min_w_0().h_full().mx(px(-(field_size.padding() - 2.))).child(
                    InputField::new(&self.query)
                        .variant(InputVariant::Bare)
                        .size(field_size)
                        .text_size(16.)
                        .surface(SURFACE),
                ),
            )
            .when_some(refresh, |header, listing| {
                header.child(
                    ActionButton::icon("refresh-repos", "rotate-cw", refresh_help)
                        .surface(SURFACE)
                        .pending(listing)
                        .on_click(cx.listener(|this, _, _, cx| this.refresh_repos(cx))),
                )
            })
    }

    /// The list's rows: the sections' titles and items, what is said when nothing was found, and
    /// what a server refused. The list starts over at the top when they change.
    fn follow_entries(&mut self, sections: &[Section], notice: Option<String>, cx: &App) {
        let mut entries = Vec::new();
        if sections.iter().all(|section| section.items.is_empty()) {
            entries.push(Entry::Empty(self.empty_text(cx)));
        }
        for (position, section) in sections.iter().enumerate() {
            entries.push(Entry::Title(position));
            entries.extend((0..section.items.len()).map(|item| Entry::Row(position, item)));
        }
        entries.extend(notice.map(Entry::Notice));
        entries.push(Entry::End);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for entry in &entries {
            match entry {
                Entry::Row(section, item) => sections[*section].items[*item].id.hash(&mut hasher),
                Entry::Title(section) => sections[*section].title.hash(&mut hasher),
                Entry::Empty(text) | Entry::Notice(text) => text.hash(&mut hasher),
                Entry::End => {}
            }
        }
        let signature = hasher.finish();
        if signature != self.entries_signature || entries.len() != self.list.item_count() {
            self.entries_signature = signature;
            self.list.reset(entries.len());
        }
        self.entries = Rc::new(entries);
    }

    fn entry(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let Some(entry) = self.entries.get(index) else { return div().into_any_element() };
        match entry {
            Entry::Empty(text) => div()
                .w_full()
                .px(px(24.))
                .py(px(28.))
                .text_size(px(13.))
                .text_color(c.muted_stronger_foreground)
                .text_center()
                .child(text.clone())
                .into_any_element(),
            Entry::Title(section) => div()
                .px(px(SIDE_MARGIN + 10.))
                .pt(px(10.))
                .pb(px(4.))
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.muted_stronger_foreground)
                .child(self.shown[*section].title.clone())
                .into_any_element(),
            Entry::Row(section, item) => {
                let shown = self.shown.clone();
                self.row(&shown[*section].items[*item], cx)
            }
            Entry::Notice(notice) => div()
                .h(px(NOTICE_HEIGHT))
                .px(px(SIDE_MARGIN + 10.))
                .flex()
                .items_center()
                .text_size(px(12.))
                .text_color(c.destructive)
                .line_clamp(2)
                .child(notice.clone())
                .into_any_element(),
            Entry::End => div().h(px(8.)).into_any_element(),
        }
    }

    fn row(&self, item: &Item, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let light = SURFACE.layer(Layer::Control, c);
        let lit = item.selectable() && item.index == Some(self.highlighted);
        let placeholder = item.placeholder_lines > 0;
        let icon: AnyElement = match &item.icon {
            ItemIcon::Symbol(_) if placeholder => {
                div().size(px(20.)).rounded(px(Radius::XS)).bg(light).into_any_element()
            }
            ItemIcon::Symbol(name) => icons::symbol(name, 15.).text_color(c.muted_foreground).into_any_element(),
            ItemIcon::GitHub => logos::github_mark(16., cx).text_color(c.muted_foreground).into_any_element(),
            ItemIcon::Project(project) => logos::project_icon(project.as_ref().as_ref(), 20., cx),
        };
        let text = if placeholder { placeholder_lines(item, light) } else { lines(item, c) };
        let trailing: Option<AnyElement> = if item.busy {
            Some(div().text_color(c.muted_foreground).child(Spinner::regular().render(cx)).into_any_element())
        } else if let Some(note) = &item.note {
            Some(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if item.warns { c.warning } else { c.muted_stronger_foreground })
                    .when(item.warns, |note| note.px(px(8.)).py(px(3.)).rounded_full().bg(c.wash(c.warning, false)))
                    .child(note.clone())
                    .into_any_element(),
            )
        } else {
            let shortcuts = &self.store.read(cx).shortcuts;
            let digit = |digit: usize| shortcuts.keys_label(&format!("mod+{digit}"));
            item.keys.clone().or_else(|| item.shortcut.map(digit)).map(|keys| {
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c.muted_stronger_foreground)
                    .child(keys)
                    .into_any_element()
            })
        };
        let index = item.index;
        let selectable = item.selectable();
        let id = item.id.clone();
        let row_light = SURFACE.layer(Layer::Row, c);
        // Every row lights under the pointer, as a button does, though only some can be run.
        let row = div()
            .id(SharedString::from(format!("panel-{}", item.id)))
            .group("panel-row")
            .px(px(SIDE_MARGIN))
            .child(
                div()
                    .h(px(ROW_HEIGHT))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .rounded(px(ROW_RADIUS))
                    .when(lit, |row| row.bg(row_light))
                    .when(!lit, |row| row.group_hover("panel-row", move |row| row.bg(row_light)))
                    .when(item.off, |row| row.opacity(c.opacity(Opacity::Disabled)))
                    .child(div().size(px(22.)).flex_shrink_0().flex().items_center().justify_center().child(icon))
                    .child(div().flex_1().min_w_0().child(text))
                    .children(trailing.map(|trailing| div().ml(px(8.)).flex_shrink_0().child(trailing))),
            )
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if let (true, Some(index)) = (*hovered, index)
                    && this.highlighted != index
                {
                    this.highlighted = index;
                    cx.notify();
                }
            }))
            .when(selectable, |row| {
                row.on_click(cx.listener(move |this, _, window, cx| {
                    let sections = this.current(cx);
                    if let Some(item) = sections.iter().flat_map(|section| &section.items).find(|item| item.id == id) {
                        this.run(item, window, cx);
                    }
                }))
            });
        if placeholder {
            let faded = c.opacity(Opacity::Disabled);
            return row
                .with_animation(
                    SharedString::from(format!("placeholder-{}", item.id)),
                    Animation::new(Duration::from_millis(1600)).repeat(),
                    move |row, delta| row.opacity(1. - (1. - faded) * (1. - (delta * 2. - 1.).abs())),
                )
                .into_any_element();
        }
        row.into_any_element()
    }

    fn hints(&self, cx: &App) -> impl IntoElement {
        let c = colors(cx);
        let shortcuts = &self.store.read(cx).shortcuts;
        // Each of `keys` as `keybindings.json` writes it, its caps drawn for the system.
        let hint = |keys: &[&str], text: &str| {
            let caps: Vec<String> = keys.iter().flat_map(|keys| shortcuts.caps_of(keys)).collect();
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .text_color(c.muted_foreground)
                .child(KeyCaps::new(caps).surface(SURFACE))
                .child(div().text_size(px(12.)).child(text.to_string()))
        };
        let folder = matches!(self.page(), PanelPage::Folder(_));
        let icon = matches!(self.page(), PanelPage::Icon(_));
        let github = matches!(self.page(), PanelPage::Github(_));
        div()
            .h(px(40.))
            .px(px(16.))
            .flex()
            .items_center()
            .gap(px(14.))
            .child(hint(&["up", "down"], "Navigate"))
            .when(folder, |hints| hints.child(hint(&["enter"], "Open")).child(hint(&["mod+enter"], "Add")))
            .when(icon, |hints| hints.child(hint(&["enter"], "Open")))
            .when(!folder && !icon, |hints| hints.child(hint(&["enter"], "Select")))
            .when(github, |hints| hints.child(hint(&["mod+r"], "Refresh")))
            .when(self.pages.len() > 1, |hints| hints.child(hint(&["backspace"], "Back")))
            .child(hint(&["escape"], "Close"))
    }
}

/// The title and the detail, or the detail's parts each after its symbol.
fn lines(item: &Item, c: &Colors) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .min_w_0()
        .child(
            div()
                .h(px(TITLE_HEIGHT))
                .text_size(px(14.))
                .line_height(px(TITLE_HEIGHT))
                .truncate()
                .child(item.title.clone()),
        )
        .when(!item.detail_parts.is_empty(), |text| text.child(detail_parts(&item.detail_parts, c)))
        .when(item.detail_parts.is_empty() && !item.detail.is_empty(), |text| {
            text.child(
                crate::ui::middle_truncated(item.detail.clone())
                    .h(px(DETAIL_HEIGHT))
                    .text_size(px(12.))
                    .line_height(px(DETAIL_HEIGHT))
                    .text_color(c.muted_foreground),
            )
        })
}

fn detail_parts(parts: &[(&'static str, String)], c: &Colors) -> Div {
    let mut row = div()
        .h(px(DETAIL_HEIGHT))
        .flex()
        .items_center()
        .gap(px(4.))
        .text_size(px(12.))
        .line_height(px(DETAIL_HEIGHT))
        .text_color(c.muted_foreground);
    for (position, (symbol, text)) in parts.iter().enumerate() {
        if position > 0 {
            row = row.child(div().text_color(c.muted_stronger_foreground).child("·"));
        }
        // Lucide's server fills more of its square than the icons beside it.
        let size = if *symbol == "server" { 11. } else { 12. };
        row = row.child(
            div()
                .flex()
                .items_center()
                .gap(px(3.))
                .min_w_0()
                .when(position == 0, |part| part.flex_shrink_0())
                .child(icons::symbol(symbol, size))
                .child(crate::ui::middle_truncated(text.clone())),
        );
    }
    row
}

/// Bars where the text will be, each in the room its line takes.
fn placeholder_lines(item: &Item, light: Hsla) -> Div {
    const TITLES: [f32; 8] = [150., 210., 120., 180., 240., 140., 200., 160.];
    const DETAILS: [f32; 8] = [280., 190., 320., 230., 150., 300., 210., 260.];
    let position = item.index.unwrap_or(0) % 8;
    let bar = |width: f32, height: f32, room: f32| {
        div()
            .h(px(room))
            .flex()
            .items_center()
            .child(div().w(px(width)).h(px(height)).rounded(px(height / 2.)).bg(light))
    };
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(bar(TITLES[position], 10., TITLE_HEIGHT))
        .when(item.placeholder_lines > 1, |text| text.child(bar(DETAILS[position], 8., DETAIL_HEIGHT)))
}

/// The repositories that answer the search, the best first and the last pushed among equals.
fn found<'a>(repos: &'a [Repo], search: &str) -> Vec<&'a Repo> {
    if search.is_empty() {
        return repos.iter().collect();
    }
    let mut ranked: Vec<(u8, usize, &Repo)> =
        repos.iter().enumerate().filter_map(|(position, repo)| Some((rank(repo, search)?, position, repo))).collect();
    ranked.sort_by_key(|(rank, position, _)| (*rank, *position));
    ranked.into_iter().map(|(_, _, repo)| repo).collect()
}

/// How well a repository answers a lowercased search, the best at 0: the name after the owner
/// starting with it, then the whole name, then the name holding it, then the description.
fn rank(repo: &Repo, search: &str) -> Option<u8> {
    let name = repo.name.to_lowercase();
    let short = name.split_once('/').map_or(name.as_str(), |(_, short)| short);
    if short.starts_with(search) {
        return Some(0);
    }
    if name.starts_with(search) {
        return Some(1);
    }
    if name.contains(search) {
        return Some(2);
    }
    let description = repo.description.as_deref().unwrap_or_default().to_lowercase();
    description.contains(search).then_some(3)
}

/// An `owner/name` that GitHub could have.
fn is_repo_name(text: &str) -> bool {
    let allowed = |part: &str| !part.is_empty() && part.chars().all(|ch| ch.is_alphanumeric() || "_.-".contains(ch));
    matches!(text.split_once('/'), Some((owner, name)) if allowed(owner) && allowed(name))
}

fn divider(c: &Colors) -> Div {
    div().h(px(1.)).w_full().flex_shrink_0().bg(SURFACE.border(c))
}

impl Render for CommandPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let sections = self.current(cx);
        let rows: usize = sections.iter().map(|section| section.items.len()).sum();
        let notice = self.notice(cx);
        let height = self.height(&sections, rows, notice.is_some());
        self.follow_entries(&sections, notice, cx);
        let results =
            list(self.list.clone(), cx.processor(|this, index, _, cx| this.entry(index, cx))).h(px(height)).w_full();
        let size = crate::window_frame::content_size(window);
        // The card is there at once; its scrim is the window's, which stays from page to page.
        let card = div()
            .id("command-panel-card")
            .w(px(WIDTH))
            .h_auto()
            .flex()
            .flex_col()
            .bg(SURFACE.color(c))
            .rounded(px(RADIUS))
            .border_1()
            .border_color(c.border_popover)
            .shadow(c.shadow(ShadowSize::Xl, Opacity::ShadowStrongest))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(self.header(cx))
            .child(divider(c))
            .child(results)
            .child(divider(c))
            .child(self.hints(cx));
        deferred(
            anchored().position(crate::window_frame::content_origin(window)).child(
                div()
                    .id("command-panel")
                    .w(size.width)
                    .h(size.height)
                    .relative()
                    .occlude()
                    .flex()
                    .justify_center()
                    .items_start()
                    .pt(px(crate::theme::TOP_BAR + 70.))
                    .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| this.key(event, window, cx)))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                    .child(card),
            ),
        )
        .with_priority(1)
    }
}
