//! The menu bar. Its items run the keyboard shortcuts' commands and show their keys as the user
//! set them; their titles say what an item would do now, and what can't be done is greyed.

use gpui_kit::component::input;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::store::integrations::Integration;
use crate::store::{PanelPage, Store};

actions!(
    motile,
    [
        About,
        Help,
        Quit,
        Hide,
        HideOthers,
        ShowAll,
        CheckForUpdates,
        AddProject,
        AddServer,
        ToggleFullScreen,
        Minimize,
        Zoom
    ]
);

/// Runs a keyboard shortcut's command from the menu.
#[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
#[action(namespace = motile, no_json)]
pub struct Perform(pub &'static str);

/// Opens the command panel on the page of a command that toggles it: the menu only ever opens
/// it, where its keys open and close it.
#[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
#[action(namespace = motile, no_json)]
pub struct OpenPanel(pub &'static str);

/// The page of a command that toggles the command panel.
fn panel_page(command: &str) -> Option<PanelPage> {
    match command {
        "threadPicker.toggle" => Some(PanelPage::Threads),
        "commandPalette.toggle" => Some(PanelPage::Commands),
        _ => None,
    }
}

/// The context of the menus' keys. No view has it, so the keys only label the menus: a key press
/// meets the rules in `keyboard` first.
const MENU_KEYS: &str = "MenuKeys";

pub fn install(store: &Entity<Store>, cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, None),
        KeyBinding::new("cmd-m", Minimize, None),
    ]);

    cx.on_action(|_: &About, cx| show_about(cx));
    cx.on_action(|_: &Help, cx| show_help(cx));
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    on_window(cx, |_: &Minimize, window, _| window.minimize_window());
    on_window(cx, |_: &Zoom, window, _| window.zoom_window());
    on_window(cx, |_: &ToggleFullScreen, window, _| window.toggle_fullscreen());
    on_store(store, cx, |_: &CheckForUpdates, store, cx| store.updater.check(true, cx));
    on_store(store, cx, |action: &OpenPanel, store, _| {
        if let Some(page) = panel_page(action.0) {
            store.open_panel(page);
        }
    });
    on_store(store, cx, |_: &AddProject, store, _| {
        if store.has_live_servers() {
            store.add_project();
        }
    });
    on_store(store, cx, |_: &AddServer, store, _| {
        if store.account.signed_in {
            store.shows_add_server = true;
        }
    });
    let performing = store.clone();
    cx.on_action(move |action: &Perform, cx| {
        let Some(window) = cx.active_window().or_else(|| cx.windows().into_iter().next()) else { return };
        let (store, command) = (performing.clone(), action.0);
        // The window is busy with the menu's key until the action has run.
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| crate::keyboard::perform(&store, command, window, cx));
        });
    });

    let mut shown = None;
    follow(store, &mut shown, cx);
    cx.observe(store, move |store, cx| follow(&store, &mut shown, cx)).detach();
}

/// Runs a menu's action on the store.
fn on_store<A: Action>(
    store: &Entity<Store>,
    cx: &mut App,
    run: impl Fn(&A, &mut Store, &mut Context<Store>) + 'static,
) {
    let store = store.clone();
    cx.on_action(move |action: &A, cx| {
        store.update(cx, |store, cx| {
            run(action, store, cx);
            cx.notify();
        })
    });
}

fn on_window<A: Action + Clone>(cx: &mut App, run: impl Fn(&A, &mut Window, &mut App) + 'static) {
    let run = std::rc::Rc::new(run);
    cx.on_action(move |action: &A, cx| {
        let Some(window) = cx.active_window() else { return };
        let (action, run) = (action.clone(), run.clone());
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| run(&action, window, cx));
        });
    });
}

/// The commands the menus run, whose keys they show.
const COMMANDS: [&str; 27] = [
    "settings.open",
    "usage.open",
    "chat.new",
    "chat.newLocal",
    "chat.newWithoutProject",
    "threadPicker.toggle",
    "commandPalette.toggle",
    "rightPanel.close",
    "sidebar.toggle",
    "rightPanel.toggle",
    "rightPanel.toggleMaximized",
    "rightPanel.diff",
    "rightPanel.files",
    "rightPanel.agents",
    "rightPanel.pullRequest",
    "rightPanel.pullRequests",
    "rightPanel.linear",
    "rightPanel.ricrac",
    "rightPanel.new",
    "rightPanel.nextTab",
    "rightPanel.previousTab",
    "thread.done",
    "thread.stop",
    "thread.previous",
    "thread.next",
    "navigation.back",
    "navigation.forward",
];

/// What the menus say and allow, which they are made again from when it changes.
#[derive(Clone, PartialEq)]
struct State {
    keys: Vec<(&'static str, String)>,
    sidebar_hidden: bool,
    panel_open: bool,
    panel_maximized: bool,
    can_maximize: bool,
    shows_changes: bool,
    panel_available: bool,
    shows_pull_request: bool,
    shows_pull_requests: bool,
    /// Whether Linear's and Ricrac's tabs can be shown, while their integrations are on.
    shows_linear: Option<bool>,
    shows_ricrac: Option<bool>,
    switchable_tabs: bool,
    /// The project of "New Thread in …", and whether it is a server's "No project".
    project: Option<(String, bool)>,
    no_projects: bool,
    thread: Option<bool>,
    /// The open thread's server was unlinked, so nothing can be done to it.
    read_only: bool,
    busy: bool,
    jumps: bool,
    can_go_back: bool,
    can_go_forward: bool,
    has_servers: bool,
    signed_in: bool,
    full_screen: bool,
}

fn follow(store: &Entity<Store>, shown: &mut Option<State>, cx: &mut App) {
    let store = store.read(cx);
    let available = store.panel_unavailable().is_none();
    let state = State {
        keys: COMMANDS
            .into_iter()
            .filter_map(|command| Some((command, store.shortcuts.menu_keystroke(command)?)))
            .collect(),
        sidebar_hidden: store.prefs.bool("sidebar.hidden"),
        panel_open: store.right_panel.is_open,
        panel_maximized: store.panel_maximized(),
        can_maximize: store.can_maximize_panel(),
        shows_changes: available && store.panel_target().is_some_and(|target| target.repository),
        panel_available: available,
        shows_pull_request: available && store.pull_requests_unavailable().is_none(),
        shows_pull_requests: available && store.pull_requests_unavailable().is_none() && store.pull_requests_extended(),
        shows_linear: store.is_on(Integration::Linear).then(|| available && store.linear_unavailable().is_none()),
        shows_ricrac: store.is_on(Integration::Ricrac).then(|| available && store.ricrac_unavailable().is_none()),
        switchable_tabs: store.right_panel.is_open && store.panel_tabs().tabs.len() > 1,
        project: store
            .composer_project()
            .filter(|project| store.is_live(project))
            .map(|project| (project.name, project.no_project)),
        no_projects: store.live_no_projects().next().is_some(),
        thread: store.selected_thread().map(|thread| thread.is_done()),
        read_only: store.open_thread_removed(),
        busy: store.activity.busy(),
        jumps: store.has_jump_threads(),
        can_go_back: store.can_go(true),
        can_go_forward: store.can_go(false),
        has_servers: store.has_live_servers(),
        signed_in: store.account.signed_in,
        full_screen: store.full_screen,
    };
    if shown.as_ref() == Some(&state) {
        return;
    }
    if shown.as_ref().is_none_or(|shown| shown.keys != state.keys) {
        bind_menu_keys(&state.keys, cx);
    }
    cx.set_menus(menus(&state));
    *shown = Some(state);
}

/// Gives the menus' items the keys of their commands, in place of the ones they had.
fn bind_menu_keys(keys: &[(&'static str, String)], cx: &mut App) {
    let keymap = cx.key_bindings();
    let others: Vec<KeyBinding> = keymap
        .borrow()
        .bindings()
        .filter(|binding| {
            let action = binding.action().as_any();
            !action.is::<Perform>() && !action.is::<OpenPanel>()
        })
        .cloned()
        .collect();
    let context = KeyBindingContextPredicate::parse(MENU_KEYS).ok().map(std::rc::Rc::new);
    let ours = keys.iter().filter_map(|(command, keystroke)| {
        let action: Box<dyn Action> =
            if panel_page(command).is_some() { Box::new(OpenPanel(command)) } else { Box::new(Perform(command)) };
        KeyBinding::load(keystroke, action, context.clone(), false, None, &DummyKeyboardMapper).ok()
    });
    let mut keymap = keymap.borrow_mut();
    keymap.clear();
    keymap.add_bindings(others.into_iter().chain(ours));
}

fn item(title: impl Into<SharedString>, command: &'static str) -> MenuItem {
    if panel_page(command).is_some() {
        return MenuItem::action(title, OpenPanel(command));
    }
    MenuItem::action(title, Perform(command))
}

fn menus(state: &State) -> Vec<Menu> {
    let new_in_project = match &state.project {
        Some((_, true)) => "New Thread Without a Project".to_string(),
        Some((name, false)) => format!("New Thread in “{name}”"),
        None => "New Thread in This Project".to_string(),
    };
    vec![
        Menu::new("Motile").items([
            MenuItem::action("About Motile", About),
            MenuItem::action("Check for Updates", CheckForUpdates),
            MenuItem::separator(),
            item("Settings", "settings.open"),
            item("Usage", "usage.open").disabled(!state.signed_in),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Hide Motile", Hide),
            MenuItem::action("Hide Others", HideOthers),
            MenuItem::action("Show All", ShowAll),
            MenuItem::separator(),
            MenuItem::action("Quit Motile", Quit),
        ]),
        Menu::new("File").items([
            item("New Thread", "chat.new"),
            item(new_in_project, "chat.newLocal").disabled(state.project.is_none()),
            item("New Thread Without a Project", "chat.newWithoutProject").disabled(!state.no_projects),
            item("Go to Thread", "threadPicker.toggle"),
            item("Commands", "commandPalette.toggle"),
            MenuItem::separator(),
            item("Close", "rightPanel.close"),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", input::Undo, OsAction::Undo),
            MenuItem::os_action("Redo", input::Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", input::Cut, OsAction::Cut),
            MenuItem::os_action("Copy", input::Copy, OsAction::Copy),
            MenuItem::os_action("Paste", input::Paste, OsAction::Paste),
            // The fields only ever take plain text, so pasting matches their style anyway.
            MenuItem::action("Paste and Match Style", input::Paste),
            MenuItem::action("Delete", input::Delete),
            MenuItem::os_action("Select All", input::SelectAll, OsAction::SelectAll),
        ]),
        Menu::new("View").items(
            [
                item(if state.sidebar_hidden { "Show Sidebar" } else { "Hide Sidebar" }, "sidebar.toggle"),
                item(if state.panel_open { "Hide Right Panel" } else { "Show Right Panel" }, "rightPanel.toggle"),
                item(
                    if state.panel_maximized { "Restore Right Panel" } else { "Maximize Right Panel" },
                    "rightPanel.toggleMaximized",
                )
                .disabled(!state.can_maximize),
                item("Show Changes", "rightPanel.diff").disabled(!state.shows_changes),
                item("Show Files", "rightPanel.files").disabled(!state.panel_available),
                item("Show Agents", "rightPanel.agents").disabled(!state.panel_available),
                item("Show Pull Request", "rightPanel.pullRequest").disabled(!state.shows_pull_request),
                item("Show All Pull Requests", "rightPanel.pullRequests").disabled(!state.shows_pull_requests),
            ]
            .into_iter()
            .chain(state.shows_linear.map(|shows| item("Show Linear", "rightPanel.linear").disabled(!shows)))
            .chain(state.shows_ricrac.map(|shows| item("Show Ricrac", "rightPanel.ricrac").disabled(!shows)))
            .chain([
                item("New Tab", "rightPanel.new").disabled(!state.panel_available),
                item("Show Next Tab", "rightPanel.nextTab").disabled(!state.switchable_tabs),
                item("Show Previous Tab", "rightPanel.previousTab").disabled(!state.switchable_tabs),
                MenuItem::separator(),
                MenuItem::action(
                    if state.full_screen { "Exit Full Screen" } else { "Enter Full Screen" },
                    ToggleFullScreen,
                ),
            ]),
        ),
        Menu::new("Thread").items([
            item(if state.thread == Some(true) { "Mark Undone" } else { "Mark Done" }, "thread.done")
                .disabled(state.thread.is_none() || state.read_only),
            item("Stop", "thread.stop").disabled(!state.busy || state.read_only),
            item("Previous Thread", "thread.previous").disabled(!state.jumps),
            item("Next Thread", "thread.next").disabled(!state.jumps),
            item("Go Back", "navigation.back").disabled(!state.can_go_back),
            item("Go Forward", "navigation.forward").disabled(!state.can_go_forward),
            MenuItem::separator(),
            MenuItem::action("Add a Project", AddProject).disabled(!state.has_servers),
            MenuItem::action("Add a Server", AddServer).disabled(!state.signed_in),
        ]),
        // The system adds its own items to the window menu: filling and tiling, Bring All to
        // Front and the windows.
        Menu::new("Window").items([MenuItem::action("Minimize", Minimize), MenuItem::action("Zoom", Zoom)]),
        // The system adds the search of the menus to the help menu.
        Menu::new("Help").items([MenuItem::action("Motile Help", Help)]),
    ]
}

/// The system's panel with the app's name, icon and version.
#[cfg(target_os = "macos")]
fn show_about(_: &mut App) {
    let Some(main) = objc2::MainThreadMarker::new() else { return };
    let app = objc2_app_kit::NSApplication::sharedApplication(main);
    app.orderFrontStandardAboutPanel(None);
}

/// What the system says of an app without a help book, as it does for the Mac app.
#[cfg(target_os = "macos")]
fn show_help(_: &mut App) {
    let Some(main) = objc2::MainThreadMarker::new() else { return };
    let app = objc2_app_kit::NSApplication::sharedApplication(main);
    unsafe { app.showHelp(None) };
}

/// Motile's site, which says what it is and which version is the newest.
#[cfg(not(target_os = "macos"))]
fn show_about(cx: &mut App) {
    cx.open_url("https://motile.app");
}

/// The repository's README, which says how Motile is set up.
#[cfg(not(target_os = "macos"))]
fn show_help(cx: &mut App) {
    cx.open_url("https://github.com/motileapp/motile#readme");
}
