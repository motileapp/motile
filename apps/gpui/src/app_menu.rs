//! The menu bar and the app's shortcuts. The menus follow the store: their titles say what an
//! item would do now, and what can't be done is greyed.

use gpui_kit::component::input;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::panel::state::PanelTab;
use crate::store::{PanelPage, Store};

actions!(
    motile,
    [
        Quit,
        Hide,
        HideOthers,
        ShowAll,
        OpenSettings,
        CheckForUpdates,
        NewThread,
        NewThreadInProject,
        GoToThread,
        ShowCommands,
        Close,
        ToggleSidebar,
        ToggleSidePanel,
        ToggleSidePanelMaximized,
        ShowChanges,
        ShowFiles,
        ShowAgents,
        ShowPullRequest,
        ShowAllPullRequests,
        NewTab,
        ShowNextTab,
        ShowPreviousTab,
        ToggleFullScreen,
        ToggleDone,
        Stop,
        AddProject,
        AddServer,
        Minimize,
        Zoom,
    ]
);

pub fn install(store: &Entity<Store>, cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-n", NewThread, None),
        KeyBinding::new("shift-cmd-n", NewThreadInProject, None),
        KeyBinding::new("cmd-p", GoToThread, None),
        KeyBinding::new("cmd-k", ShowCommands, None),
        KeyBinding::new("cmd-w", Close, None),
        KeyBinding::new("ctrl-cmd-s", ToggleSidebar, None),
        KeyBinding::new("alt-cmd-b", ToggleSidePanel, None),
        KeyBinding::new("shift-alt-cmd-b", ToggleSidePanelMaximized, None),
        KeyBinding::new("cmd-d", ShowChanges, None),
        KeyBinding::new("shift-cmd-e", ShowFiles, None),
        KeyBinding::new("shift-cmd-a", ShowAgents, None),
        KeyBinding::new("shift-cmd-r", ShowPullRequest, None),
        KeyBinding::new("shift-alt-cmd-r", ShowAllPullRequests, None),
        KeyBinding::new("cmd-t", NewTab, None),
        KeyBinding::new("ctrl-tab", ShowNextTab, None),
        KeyBinding::new("ctrl-shift-tab", ShowPreviousTab, None),
        KeyBinding::new("shift-cmd-]", ShowNextTab, None),
        KeyBinding::new("shift-cmd-[", ShowPreviousTab, None),
        KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, None),
        KeyBinding::new("shift-cmd-d", ToggleDone, None),
        KeyBinding::new("cmd-.", Stop, None),
        KeyBinding::new("cmd-m", Minimize, None),
    ]);

    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    on_window(cx, |_: &Minimize, window, _| window.minimize_window());
    on_window(cx, |_: &Zoom, window, _| window.zoom_window());
    on_window(cx, |_: &ToggleFullScreen, window, _| window.toggle_fullscreen());

    let settings = store.clone();
    cx.on_action(move |_: &OpenSettings, cx| {
        settings.update(cx, |store, cx| {
            store.open_settings(Default::default(), None);
            cx.notify();
        })
    });
    on_store(store, cx, |_: &CheckForUpdates, store, cx| store.updater.check(true, cx));
    on_store(store, cx, |_: &NewThread, store, cx| store.new_thread(cx));
    on_store(store, cx, |_: &NewThreadInProject, store, cx| {
        let Some(project) = store.composer_project() else { return };
        store.start_new_thread(Some(project.id), cx);
    });
    on_store(store, cx, |_: &GoToThread, store, _| store.open_panel(PanelPage::Threads));
    on_store(store, cx, |_: &ShowCommands, store, _| store.open_panel(PanelPage::Commands));
    let closing = store.clone();
    cx.on_action(move |_: &Close, cx| {
        let Some(window) = cx.active_window() else { return };
        let closing = closing.clone();
        // The window is busy with the key until the action has run.
        cx.defer(move |cx| {
            if !crate::settings::is_main_window(window, cx) {
                let _ = window.update(cx, |_, window, _| window.remove_window());
                return;
            }
            // The last window closing ends the app, as on the Mac.
            let closed_settings = closing.update(cx, |store, cx| {
                let open = store.settings.section.is_some();
                store.close_settings();
                cx.notify();
                open
            });
            if closed_settings {
                return;
            }
            let closed_tab = closing.update(cx, |store, cx| {
                cx.notify();
                store.close_active_tab()
            });
            if !closed_tab {
                cx.quit();
            }
        });
    });
    on_store(store, cx, |_: &ToggleSidebar, store, _| {
        let hidden = !store.prefs.bool("sidebar.hidden");
        store.prefs.set("sidebar.hidden", hidden);
    });
    on_store(store, cx, |_: &ToggleSidePanel, store, _| {
        let open = !store.side_panel.is_open;
        store.set_panel_open(open);
    });
    on_store(store, cx, |_: &ToggleSidePanelMaximized, store, _| store.toggle_panel_maximized());
    on_store(store, cx, |_: &ShowChanges, store, _| {
        if store.panel_unavailable().is_none() && store.panel_target().is_some_and(|target| target.repository) {
            store.show_diff(None, None);
        }
    });
    on_store(store, cx, |_: &ShowFiles, store, _| {
        if store.panel_unavailable().is_none() {
            store.open_tab(PanelTab::Files);
        }
    });
    on_store(store, cx, |_: &ShowAgents, store, _| {
        if store.panel_unavailable().is_none() {
            store.open_tab(PanelTab::Agents);
        }
    });
    on_store(store, cx, |_: &ShowPullRequest, store, _| {
        if store.panel_unavailable().is_none() && store.pull_requests_unavailable().is_none() {
            store.open_tab(PanelTab::PullRequest);
        }
    });
    on_store(store, cx, |_: &ShowAllPullRequests, store, _| {
        if store.panel_unavailable().is_none()
            && store.pull_requests_unavailable().is_none()
            && store.pull_requests_extended()
        {
            store.open_tab(PanelTab::PullRequests);
        }
    });
    on_store(store, cx, |_: &NewTab, store, _| {
        if store.panel_unavailable().is_none() {
            store.open_blank_tab();
        }
    });
    on_store(store, cx, |_: &ShowNextTab, store, _| {
        if store.side_panel.is_open {
            store.activate_tab_offset(1);
        }
    });
    on_store(store, cx, |_: &ShowPreviousTab, store, _| {
        if store.side_panel.is_open {
            store.activate_tab_offset(-1);
        }
    });
    on_store(store, cx, |_: &ToggleDone, store, cx| store.toggle_done(cx));
    on_store(store, cx, |_: &Stop, store, _| {
        if store.activity.busy() {
            store.stop_thread();
        }
    });
    on_store(store, cx, |_: &AddProject, store, _| {
        if !store.servers.is_empty() {
            store.add_project();
        }
    });
    on_store(store, cx, |_: &AddServer, store, _| {
        if store.account.signed_in {
            store.shows_add_server = true;
        }
    });

    let mut shown = None;
    follow(store, &mut shown, cx);
    cx.observe(store, move |store, cx| follow(&store, &mut shown, cx)).detach();
}

/// Runs a menu's action on the store, when a window is there to show what it did.
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

/// What the menus say and allow, which they are made again from when it changes.
#[derive(Clone, PartialEq)]
struct State {
    sidebar_hidden: bool,
    panel_open: bool,
    panel_maximized: bool,
    shows_changes: bool,
    panel_available: bool,
    shows_pull_request: bool,
    shows_pull_requests: bool,
    switchable_tabs: bool,
    project: Option<String>,
    thread: Option<bool>,
    busy: bool,
    has_servers: bool,
    signed_in: bool,
}

fn follow(store: &Entity<Store>, shown: &mut Option<State>, cx: &mut App) {
    let store = store.read(cx);
    let available = store.panel_unavailable().is_none();
    let state = State {
        sidebar_hidden: store.prefs.bool("sidebar.hidden"),
        panel_open: store.side_panel.is_open,
        panel_maximized: store.panel_maximized(),
        shows_changes: available && store.panel_target().is_some_and(|target| target.repository),
        panel_available: available,
        shows_pull_request: available && store.pull_requests_unavailable().is_none(),
        shows_pull_requests: available && store.pull_requests_unavailable().is_none() && store.pull_requests_extended(),
        switchable_tabs: store.side_panel.is_open && store.panel_tabs().tabs.len() > 1,
        project: store.composer_project().map(|project| project.name),
        thread: store.selected_thread().map(|thread| thread.is_done()),
        busy: store.activity.busy(),
        has_servers: !store.servers.is_empty(),
        signed_in: store.account.signed_in,
    };
    if shown.as_ref() == Some(&state) {
        return;
    }
    cx.set_menus(menus(&state));
    *shown = Some(state);
}

fn menus(state: &State) -> Vec<Menu> {
    vec![
        Menu::new("Motile").items([
            MenuItem::action("Check for Updates…", CheckForUpdates),
            MenuItem::separator(),
            MenuItem::action("Settings…", OpenSettings),
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
            MenuItem::action("New Thread…", NewThread),
            MenuItem::action(
                state
                    .project
                    .as_ref()
                    .map_or("New Thread in This Project".into(), |name| format!("New Thread in “{name}”")),
                NewThreadInProject,
            )
            .disabled(state.project.is_none()),
            MenuItem::action("Go to Thread…", GoToThread),
            MenuItem::action("Commands…", ShowCommands),
            MenuItem::separator(),
            MenuItem::action("Close", Close),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", input::Undo, OsAction::Undo),
            MenuItem::os_action("Redo", input::Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", input::Cut, OsAction::Cut),
            MenuItem::os_action("Copy", input::Copy, OsAction::Copy),
            MenuItem::os_action("Paste", input::Paste, OsAction::Paste),
            MenuItem::os_action("Select All", input::SelectAll, OsAction::SelectAll),
        ]),
        Menu::new("View").items([
            MenuItem::action(if state.sidebar_hidden { "Show Sidebar" } else { "Hide Sidebar" }, ToggleSidebar),
            MenuItem::action(if state.panel_open { "Hide Side Panel" } else { "Show Side Panel" }, ToggleSidePanel),
            MenuItem::action(
                if state.panel_maximized { "Restore Side Panel" } else { "Maximize Side Panel" },
                ToggleSidePanelMaximized,
            )
            .disabled(!state.panel_open),
            MenuItem::action("Show Changes", ShowChanges).disabled(!state.shows_changes),
            MenuItem::action("Show Files", ShowFiles).disabled(!state.panel_available),
            MenuItem::action("Show Agents", ShowAgents).disabled(!state.panel_available),
            MenuItem::action("Show Pull Request", ShowPullRequest).disabled(!state.shows_pull_request),
            MenuItem::action("Show All Pull Requests", ShowAllPullRequests).disabled(!state.shows_pull_requests),
            MenuItem::action("New Tab", NewTab).disabled(!state.panel_available),
            MenuItem::action("Show Next Tab", ShowNextTab).disabled(!state.switchable_tabs),
            MenuItem::action("Show Previous Tab", ShowPreviousTab).disabled(!state.switchable_tabs),
            MenuItem::separator(),
            MenuItem::action("Enter Full Screen", ToggleFullScreen),
        ]),
        Menu::new("Thread").items([
            MenuItem::action(if state.thread == Some(true) { "Mark Undone" } else { "Mark Done" }, ToggleDone)
                .disabled(state.thread.is_none()),
            MenuItem::action("Stop", Stop).disabled(!state.busy),
            MenuItem::separator(),
            MenuItem::action("Add a Project…", AddProject).disabled(!state.has_servers),
            MenuItem::action("Add a Server…", AddServer).disabled(!state.signed_in),
        ]),
        Menu::new("Window").items([MenuItem::action("Minimize", Minimize), MenuItem::action("Zoom", Zoom)]),
    ]
}
