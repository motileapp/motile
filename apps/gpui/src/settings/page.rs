//! One section of the settings: its groups of rows, and the way to the group a search picked.

use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::{SyncState, SyncView};
use motile_core::link::State;
use motile_protocol::wire::{Agent, AgentAccount, ContinueSettings, PullRequestSettings};

use super::account_sheet::AccountSheet;
use super::keybindings::KeybindingsPage;
use super::layout::{
    Cut, END_ROOM, GROUP_GAP, INSET, PAGE_PADDING, Targets, column, footed_label, group, label, large_outset, more,
    note, row, switch_row, target,
};
use crate::format;
use crate::models::{self, AgentAccountExt, Project, Server, agent_name, new_account};
use crate::sidebar::{AppUpdateRow, ServerUpdateStatus};
use crate::store::integrations::Integration;
use crate::store::settings::{PULL_DEFAULT_BRANCH_CAPTION, SettingsSection};
use crate::store::{PanelPage, Store};
use crate::theme::{self, Appearance, ControlSize, Layer, Radius, Surface, colors};
use crate::ui::menu::Menu;
use crate::ui::modal::{Answer, Confirmation, confirmation, modal, present_sheet, presented};
use crate::ui::motion::{SPRING, Tween, spring};
use crate::ui::{
    ActionButton, ActionMenu, Spinner, Switch, TextArea, Variant, divider_on, icons, logos, text_area_state, tooltip,
};
use crate::updater::UpdateState;

/// What a row's menu runs, kept for each time the menu is built again.
type Run = Rc<dyn Fn(&mut Window, &mut App)>;

/// How far the page fades in under the top bar.
const FADE: f32 = 20.;
/// The side of an integration's logo tile, and the room between it and the integration's name.
const LOGO_TILE: f32 = 32.;
const LOGO_GAP: f32 = 12.;

/// What the settings take away once the user says so: a server, a server with every copy of its
/// threads, a project or an agent's account.
#[derive(Clone)]
enum Removal {
    Server(Box<Server>),
    ServerCompletely(Box<Server>),
    Project(Box<Project>),
    Account(Box<(AgentAccount, Server)>),
}

impl Removal {
    fn title(&self) -> String {
        match self {
            Removal::Server(server) => format!("Unlink “{}”?", server.name),
            Removal::ServerCompletely(server) => format!("Delete “{}”?", server.name),
            Removal::Project(project) => format!("Remove “{}”?", project.name),
            Removal::Account(pair) => format!("Remove “{} · {}”?", agent_name(pair.0.agent), pair.0.name),
        }
    }

    fn message(&self, store: &Store) -> String {
        match self {
            Removal::Server(server) => format!(
                "Your account lets go of {}. Its threads stay readable, and can go on, from the copies your other servers keep. If it is your only server, nothing is kept. What is on it stays there.",
                server.name
            ),
            Removal::ServerCompletely(server) => format!(
                "{name} leaves your account and every copy of its threads is deleted, on all your servers and clients. What is on {name} itself stays there.",
                name = server.name
            ),
            Removal::Project(project) => {
                let server = store.server(Some(&project.server_id)).map_or("its server", |server| server.name.as_str());
                format!("{} leaves your clients. Its folder stays on {server} with everything in it.", project.name)
            }
            Removal::Account(pair) => format!(
                "Its threads move to the default account on {}. Its folder and its sign-in stay there.",
                pair.1.name
            ),
        }
    }

    fn answer(&self) -> &'static str {
        match self {
            Removal::ServerCompletely(_) => "Delete",
            Removal::Server(_) => "Unlink",
            _ => "Remove",
        }
    }

    fn run(&self, store: &mut Store) {
        match self {
            Removal::Server(server) => store.remove_server(server),
            Removal::ServerCompletely(server) => store.delete_server_completely(server),
            Removal::Project(project) => store.remove_project(project),
            Removal::Account(pair) => store.remove_agent_account(&pair.0, &pair.1),
        }
    }
}

pub struct SettingsPage {
    store: Entity<Store>,
    branch_editors: HashMap<String, Entity<BranchInstructions>>,
    setup: Option<Entity<SetupSheet>>,
    account: Option<Entity<AccountSheet>>,
    removal: Option<Removal>,
    keybindings: Option<Entity<KeybindingsPage>>,
    scroll: ScrollHandle,
    /// Where the page scrolls on its way to what a search picked.
    scrolling: Option<Tween>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsPage {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        store.update(cx, |store, _| store.refresh_media_storage());
        let subscriptions = vec![cx.observe_in(&store, window, |this, _, window, cx| {
            this.follow_servers(window, cx);
            cx.notify();
        })];
        let mut page = Self {
            store,
            branch_editors: HashMap::new(),
            setup: None,
            account: None,
            removal: None,
            keybindings: None,
            scroll: ScrollHandle::new(),
            scrolling: None,
            _subscriptions: subscriptions,
        };
        page.follow_servers(window, cx);
        page
    }

    /// Each server that names branches has its own box for how.
    fn follow_servers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let servers: Vec<Server> =
            self.store.read(cx).servers.iter().filter(|server| names_branches(server)).cloned().collect();
        self.branch_editors.retain(|id, _| servers.iter().any(|server| &server.id == id));
        for server in servers {
            match self.branch_editors.get(&server.id) {
                Some(editor) => editor.update(cx, |editor, cx| editor.follow(&server, window, cx)),
                None => {
                    let store = self.store.clone();
                    let editor = cx.new(|cx| BranchInstructions::new(store, &server, window, cx));
                    self.branch_editors.insert(server.id.clone(), editor);
                }
            }
        }
    }

    /// Asks before the removal, and goes ahead only when the user says so.
    fn ask_removal(page: &WeakEntity<Self>, removal: Removal) -> Run {
        let page = page.clone();
        Rc::new(move |_, cx| {
            let removal = removal.clone();
            let _ = page.update(cx, |page, cx| {
                page.removal = Some(removal);
                cx.notify();
            });
        })
    }

    fn general(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        vec![
            self.account(cx),
            self.updates(cx),
            self.appearance(cx),
            self.messages(cx),
            self.provider_limits(cx),
            self.storage(cx),
        ]
    }

    fn account(&self, cx: &mut Context<Self>) -> AnyElement {
        let account = self.store.read(cx).account.clone();
        let email = account.user.as_ref().map(|user| user.email.clone()).unwrap_or_default();
        let signed_in_as = if account.signed_in { email } else { "Not signed in".into() };
        let mut trailing = Vec::new();
        if account.signed_in {
            let store = self.store.clone();
            trailing.push(
                large("sign-out", "Sign Out")
                    .on_click(move |_, _, cx| {
                        store.update(cx, |store, cx| {
                            store.sign_out(cx);
                            cx.notify();
                        })
                    })
                    .into_any_element(),
            );
        }
        let leading = label("Signed in as", Some(signed_in_as), None, Cut::Middle, cx);
        group("account", "Account", None, true, vec![row(vec![leading], trailing).into_any_element()], cx)
    }

    fn updates(&self, cx: &mut Context<Self>) -> AnyElement {
        let (state, current) = {
            let updater = &self.store.read(cx).updater;
            (updater.state.clone(), updater.current)
        };
        let store = self.store.clone();
        let check = large("check-updates", "Check for Updates")
            .pending(state == UpdateState::Checking)
            .on_click(move |_, _, cx| store.update(cx, |store, cx| store.updater.check(true, cx)));
        let leading = label(format!("Motile {current}"), None, None, Cut::Wraps, cx);
        let mut rows = vec![row(vec![leading], vec![check.into_any_element()]).into_any_element()];
        if state != UpdateState::Idle {
            rows.push(divider_on(Surface::Card, cx).into_any_element());
            let update = AppUpdateRow::new(&self.store)
                .variant(Variant::Outline)
                .size(ControlSize::Large)
                .surface(Surface::Card);
            let vertical = (super::layout::ROW_HEIGHT - ControlSize::Large.height()) / 2.;
            rows.push(
                div().pl(px(INSET)).pr(px(INSET - large_outset())).py(px(vertical)).child(update).into_any_element(),
            );
        }
        group("updates", "Updates", None, true, rows, cx)
    }

    fn appearance(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.store.read(cx).prefs.get::<Appearance>("appearance").unwrap_or_default();
        let store = self.store.clone();
        let menu = ActionMenu::new("appearance", chosen.label(), move |_, _| {
            Appearance::ALL.iter().fold(Menu::new(), |menu, appearance| {
                let (store, appearance) = (store.clone(), *appearance);
                menu.item(appearance.label(), move |_, cx| {
                    store.update(cx, |store, cx| {
                        store.prefs.set("appearance", appearance);
                        cx.notify();
                    })
                })
            })
        })
        .button(outline);
        let leading = label("Theme", None, None, Cut::Wraps, cx);
        let rows = vec![row(vec![leading], vec![menu.into_any_element()]).into_any_element()];
        group("appearance", "Appearance", None, true, rows, cx)
    }

    fn messages(&self, cx: &mut Context<Self>) -> AnyElement {
        let steers = self.store.read(cx).steers();
        let description = if steers {
            "The agent reads it at once, in the turn that runs"
        } else {
            "Waits for the turn to end and starts the next one"
        };
        let store = self.store.clone();
        let picker = ActionMenu::new("steers", if steers { "Steer" } else { "Queue" }, move |_, _| {
            let choose = |steers: bool| {
                let store = store.clone();
                move |_: &mut Window, cx: &mut App| {
                    store.update(cx, |store, cx| {
                        store.prefs.set(Store::STEERS_KEY, steers);
                        cx.notify();
                    })
                }
            };
            Menu::new().item("Queue", choose(false)).item("Steer", choose(true))
        })
        .button(outline);
        let leading = label("Sent while the agent works", Some(description.into()), None, Cut::Wraps, cx);
        let rows = vec![row(vec![leading], vec![picker.into_any_element()]).into_any_element()];
        group("messages", "Messages", None, true, rows, cx)
    }

    /// Whether a new thread starts on the account that has used the least of its limits.
    fn provider_limits(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.store.read(cx).manages_new_threads();
        let store = self.store.clone();
        let switch = Switch::new("manage-new-threads", on).on_change(move |on, _, cx| {
            store.update(cx, |store, cx| {
                store.set_manages_new_threads(on);
                cx.notify();
            })
        });
        let description = "Starts new threads on the account with the most limits left.";
        let leading = label("Manage new threads automatically", Some(description.into()), None, Cut::Wraps, cx);
        let rows = vec![switch_row(vec![leading], switch).into_any_element()];
        group("manage-new-threads", "Provider Limits", None, true, rows, cx)
    }

    /// The servers keep every image and video; the ones kept here only make threads open with them.
    fn storage(&self, cx: &mut Context<Self>) -> AnyElement {
        let storage = self.store.read(cx).media_storage;
        let description = match &storage {
            Some(storage) => {
                format!(
                    "{} of {} on {}. Your servers keep them all.",
                    format::bytes(storage.used),
                    format::bytes(storage.limit),
                    crate::models::THIS_DEVICE
                )
            }
            None => format!("Kept on {} so threads open with them", crate::models::THIS_DEVICE),
        };
        let store = self.store.clone();
        let clear = large("clear-media", "Clear").disabled(storage.map_or(0, |storage| storage.used) == 0).on_click(
            move |_, _, cx| {
                crate::media::MediaFiles::clear(cx);
                store.update(cx, |store, _| store.clear_media());
            },
        );
        let leading = label("Images and videos", Some(description), None, Cut::Wraps, cx);
        let rows = vec![row(vec![leading], vec![clear.into_any_element()]).into_any_element()];
        group("storage", "Storage", None, true, rows, cx)
    }

    fn servers(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (servers, removed): (Vec<Server>, Vec<Server>) =
            self.store.read(cx).servers.iter().cloned().partition(|server| !server.removed);
        let page = cx.weak_entity();
        let mut rows = Vec::new();
        for (index, server) in servers.iter().enumerate() {
            let status = ServerUpdateStatus::new(&self.store, server)
                .variant(Variant::Outline)
                .size(ControlSize::Large)
                .surface(Surface::Card);
            let remove = Self::ask_removal(&page, Removal::Server(Box::new(server.clone())));
            let delete = Self::ask_removal(&page, Removal::ServerCompletely(Box::new(server.clone())));
            let menu = more(("server-menu", index), format!("What to do with {}", server.name), move |_, _| {
                let (remove, delete) = (remove.clone(), delete.clone());
                Menu::new()
                    .danger_symbol_item("Unlink Server", "unlink", move |window, cx| remove(window, cx))
                    .danger_symbol_item("Delete", "trash-2", move |window, cx| delete(window, cx))
            });
            let sync = server.sync.as_ref().map(|sync| sync_line(sync, cx));
            let leading =
                footed_label(server.name.clone(), Some(description(server)), Some("server"), Cut::End, sync, cx);
            rows.push(row(vec![leading], vec![status.into_any_element(), menu.into_any_element()]).into_any_element());
            rows.push(divider_on(Surface::Card, cx).into_any_element());
        }
        let store = self.store.clone();
        let add = large("add-server", "Add a Server").symbol("plus").on_click(move |_, _, cx| {
            store.update(cx, |store, cx| {
                store.shows_add_server = true;
                cx.notify();
            })
        });
        rows.push(
            row(vec![div().ml(px(-large_outset())).child(add).into_any_element()], Vec::new()).into_any_element(),
        );
        let mut groups = vec![group("servers", "Servers", None, true, rows, cx)];
        groups.extend(self.removed_servers(&removed, cx));
        groups.extend(self.continuing(cx));
        groups
    }

    /// The servers removed from the account, whose threads the other servers keep copies of, to
    /// delete those copies too.
    fn removed_servers(&self, removed: &[Server], cx: &mut Context<Self>) -> Option<AnyElement> {
        if removed.is_empty() {
            return None;
        }
        let page = cx.weak_entity();
        let mut rows = Vec::new();
        for (index, server) in removed.iter().enumerate() {
            let delete = Self::ask_removal(&page, Removal::ServerCompletely(Box::new(server.clone())));
            let menu = more(("removed-server-menu", index), format!("What to do with {}", server.name), move |_, _| {
                let delete = delete.clone();
                Menu::new().danger_symbol_item("Delete", "trash-2", move |window, cx| delete(window, cx))
            });
            rows.push(row(vec![server_label(server, cx)], vec![menu.into_any_element()]).into_any_element());
            if index + 1 < removed.len() {
                rows.push(divider_on(Surface::Card, cx).into_any_element());
            }
        }
        Some(group("removed-servers", "Unlinked", None, true, rows, cx))
    }

    /// What each server goes on with by itself: threads whose usage limit reset, and threads whose
    /// agents a restart cut off.
    fn continuing(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let servers: Vec<Server> = self
            .store
            .read(cx)
            .servers
            .iter()
            .filter(|server| server.connected() && server.can_choose_restart())
            .cloned()
            .collect();
        if servers.is_empty() {
            return Vec::new();
        }
        let limits = self.switch_rows(
            "continue-limits",
            &servers,
            |server| server.continue_settings.after_limits,
            |store, server, on| {
                let settings = ContinueSettings { after_limits: on, ..server.continue_settings };
                store.set_continue_settings(settings, server)
            },
            cx,
        );
        let restarts = self.switch_rows(
            "continue-restarts",
            &servers,
            |server| server.continue_settings.after_restarts,
            |store, server, on| {
                let settings = ContinueSettings { after_restarts: on, ..server.continue_settings };
                store.set_continue_settings(settings, server)
            },
            cx,
        );
        vec![
            group(
                "continue-limits",
                "Continue after usage limits",
                Some("A thread whose agent reached its usage limit goes on once the limit resets"),
                true,
                limits,
                cx,
            ),
            group(
                "continue-restarts",
                "Continue after restarts",
                Some("When your server comes back from a restart, agents that were working carry on."),
                true,
                restarts,
                cx,
            ),
        ]
    }

    /// A row with a switch for each server, divided.
    fn switch_rows(
        &self,
        id: &'static str,
        servers: &[Server],
        on: fn(&Server) -> bool,
        change: fn(&mut Store, &Server, bool),
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        for (index, server) in servers.iter().enumerate() {
            let (store, changed) = (self.store.clone(), server.clone());
            let switch = Switch::new((id, index), on(server)).on_change(move |on, _, cx| {
                store.update(cx, |store, cx| {
                    change(store, &changed, on);
                    cx.notify();
                })
            });
            rows.push(switch_row(vec![server_label(server, cx)], switch).into_any_element());
            if index + 1 < servers.len() {
                rows.push(divider_on(Surface::Card, cx).into_any_element());
            }
        }
        rows
    }

    /// The accounts each server's agents work with, a card for each server.
    fn agents(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let servers: Vec<Server> = self.store.read(cx).live_servers().cloned().collect();
        if servers.is_empty() {
            return vec![note(
                "The accounts your agents work with are set on each of your servers, once you have one.",
                cx,
            )];
        }
        let cards = servers.iter().enumerate().map(|(index, server)| {
            crate::ui::card_default(div().flex().flex_col(), cx).children(self.agent_accounts(index, server, cx))
        });
        let cards = vec![div().flex().flex_col().gap(px(24.)).children(cards).into_any_element()];
        let caption =
            "Each account keeps its sign-in in a folder of its own. A thread works with one and can move to another.";
        vec![group("agent-accounts", "Provider Accounts", Some(caption), false, cards, cx)]
    }

    fn agent_accounts(&self, index: usize, server: &Server, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let c = colors(cx);
        let unavailable = accounts_unavailable(server);
        let name = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_color(c.foreground)
            .child(icons::symbol("server", 13.))
            .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(server.name.clone()))
            .into_any_element();
        let reason = unavailable
            .clone()
            .map(|reason| div().text_size(px(12.)).text_color(c.muted_foreground).child(reason).into_any_element());
        let mut rows = vec![row(vec![name], reason.into_iter().collect()).into_any_element()];
        if unavailable.is_some() {
            return rows;
        }
        let installed: Vec<Agent> =
            [Agent::Claude, Agent::Codex].into_iter().filter(|agent| server.agents.contains_key(agent)).collect();
        rows.push(divider_on(Surface::Card, cx).into_any_element());
        let page = cx.weak_entity();
        let accounts = server.agent_accounts.iter().filter(|account| installed.contains(&account.agent));
        for (place, account) in accounts.enumerate() {
            let edit = self.edit_account(server, account.clone(), cx);
            let is_default = server.is_default_account(account);
            let make_default = (!is_default).then(|| self.make_default(server, account));
            let remove = Self::ask_removal(&page, Removal::Account(Box::new((account.clone(), server.clone()))));
            let menu = more(("account-menu", index * 1000 + place), "What to do with the account", move |_, _| {
                let (edit, remove) = (edit.clone(), remove.clone());
                Menu::new()
                    .symbol_item("Edit", "pencil", move |window, cx| edit(window, cx))
                    .when_some(make_default.clone(), |menu, make_default| {
                        menu.symbol_item("Make Default", "circle-check", move |window, cx| make_default(window, cx))
                    })
                    .separator()
                    .danger_symbol_item("Remove Account", "trash-2", move |window, cx| remove(window, cx))
            });
            let title = format!("{} · {}", agent_name(account.agent), account.name);
            let description = account.settings_description();
            let description = if is_default { format!("Default · {description}") } else { description };
            let leading = vec![
                logos::agent_icon(account.agent, 16., cx).into_any_element(),
                label(title, Some(description), None, Cut::Wraps, cx),
            ];
            rows.push(row(leading, vec![menu.into_any_element()]).into_any_element());
            rows.push(divider_on(Surface::Card, cx).into_any_element());
        }
        let first = installed.first().copied().unwrap_or(Agent::Claude);
        let add = self.edit_account(server, new_account(first), cx);
        let button = large(("add-account", index), "Add Account")
            .symbol("plus")
            .disabled(installed.is_empty())
            .on_click(move |_, window, cx| add(window, cx));
        rows.push(
            row(vec![div().ml(px(-large_outset())).child(button).into_any_element()], Vec::new()).into_any_element(),
        );
        rows
    }

    fn make_default(&self, server: &Server, account: &AgentAccount) -> Run {
        let (store, server, account) = (self.store.clone(), server.clone(), account.clone());
        Rc::new(move |_, cx| store.update(cx, |store, _| store.make_agent_account_default(&account, &server)))
    }

    /// Opens the sheet that adds or changes the account on the server.
    fn edit_account(&self, server: &Server, account: AgentAccount, cx: &mut Context<Self>) -> Run {
        let (page, store, server) = (cx.weak_entity(), self.store.clone(), server.clone());
        Rc::new(move |window, cx| {
            let (store, server, account) = (store.clone(), server.clone(), account.clone());
            let _ = page.update(cx, |page, cx| {
                let closing = cx.weak_entity();
                page.account = Some(cx.new(|cx| AccountSheet::new(store, server, account, closing, window, cx)));
                cx.notify();
            });
        })
    }

    pub fn close_account_sheet(&mut self, cx: &mut Context<Self>) {
        self.account = None;
        cx.notify();
    }

    fn projects(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let store = self.store.read(cx);
        let projects: Vec<Project> = store.projects.iter().filter(|project| store.is_live(project)).cloned().collect();
        let no_servers = !store.has_live_servers();
        let page = cx.weak_entity();
        let mut rows = Vec::new();
        for (index, project) in projects.iter().enumerate() {
            let names_setup =
                self.store.read(cx).server(Some(&project.server_id)).map_or(0, |server| server.protocol_version) >= 6;
            let (store, setup_page, chosen) = (self.store.clone(), page.clone(), project.clone());
            let remove = Self::ask_removal(&page, Removal::Project(Box::new(project.clone())));
            let menu = more(("project-menu", index), format!("What to do with {}", project.name), move |_, _| {
                let (store, icon_project) = (store.clone(), chosen.id.clone());
                let (setup_page, setup_project) = (setup_page.clone(), chosen.clone());
                let remove = remove.clone();
                Menu::new()
                    .symbol_item("Choose an Icon", "image", move |_, cx| {
                        store.update(cx, |store, cx| {
                            store.open_panel(PanelPage::Icon(icon_project.clone()));
                            cx.notify();
                        })
                    })
                    .when(names_setup, |menu| {
                        menu.symbol_item("Worktree Setup", "terminal", move |window, cx| {
                            let project = setup_project.clone();
                            let _ = setup_page.update(cx, |page, cx| page.open_setup(project, window, cx));
                        })
                    })
                    .separator()
                    .danger_symbol_item("Remove Project", "trash-2", move |window, cx| remove(window, cx))
            });
            let leading = vec![
                logos::project_icon(Some(project), 26., cx),
                label(project.name.clone(), Some(project.path.clone()), None, Cut::Start, cx),
            ];
            rows.push(row(leading, vec![menu.into_any_element()]).into_any_element());
            rows.push(divider_on(Surface::Card, cx).into_any_element());
        }
        let store = self.store.clone();
        let add =
            large("add-project", "Add a Project").symbol("plus").disabled(no_servers).on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.close_settings();
                    store.add_project();
                    cx.notify();
                })
            });
        rows.push(
            row(vec![div().ml(px(-large_outset())).child(add).into_any_element()], Vec::new()).into_any_element(),
        );
        vec![group("projects", "Projects", None, true, rows, cx)]
    }

    fn open_setup(&mut self, project: Project, window: &mut Window, cx: &mut Context<Self>) {
        let (store, page) = (self.store.clone(), cx.weak_entity());
        self.setup = Some(cx.new(|cx| SetupSheet::new(store, page, project, window, cx)));
        cx.notify();
    }

    /// The model that writes thread titles, branch names, commit messages and pull requests, and
    /// how it names branches, by server.
    fn text_generation(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let servers: Vec<Server> =
            self.store.read(cx).servers.iter().filter(|server| writes_text(server)).cloned().collect();
        if servers.is_empty() {
            let text = "The model that writes thread titles, branch names, commit messages and pull requests is set on each of your servers, once one is connected.";
            return vec![note(text, cx)];
        }
        let mut rows = Vec::new();
        for (index, server) in servers.iter().enumerate() {
            let chosen = server
                .text_model
                .as_ref()
                .and_then(|id| server.models.iter().find(|model| &model.id == id).map(|model| model.name.clone()))
                .unwrap_or("Automatic".into());
            let (store, menu_server) = (self.store.clone(), server.clone());
            let menu = ActionMenu::new(("text-model", index), chosen, move |_, _| {
                let server = menu_server.clone();
                let automatic = {
                    let (store, id) = (store.clone(), server.id.clone());
                    move |_: &mut Window, cx: &mut App| store.update(cx, |store, _| store.set_text_model(None, &id))
                };
                server.distinct_models().into_iter().fold(Menu::new().item("Automatic", automatic), |menu, model| {
                    let (store, id, model_id) = (store.clone(), server.id.clone(), model.id.clone());
                    menu.item(model.name.clone(), move |_, cx| {
                        let model = model_id.clone();
                        store.update(cx, |store, _| store.set_text_model(Some(model), &id))
                    })
                })
            })
            .button(outline);
            rows.push(row(vec![server_label(server, cx)], vec![menu.into_any_element()]).into_any_element());
            if index + 1 < servers.len() {
                rows.push(divider_on(Surface::Card, cx).into_any_element());
            }
        }
        let caption = "The model that writes thread titles, branch names, commit messages and pull requests";
        let mut groups = vec![group("text-model", "Model", Some(caption), true, rows, cx)];
        let naming: Vec<&Server> = servers.iter().filter(|server| names_branches(server)).collect();
        if naming.is_empty() {
            return groups;
        }
        let mut rows = Vec::new();
        for (index, server) in naming.iter().enumerate() {
            if naming.len() > 1 {
                rows.push(row(vec![server_label(server, cx)], Vec::new()).into_any_element());
            }
            if let Some(editor) = self.branch_editors.get(&server.id) {
                rows.push(editor.clone().into_any_element());
            }
            if index + 1 < naming.len() {
                rows.push(divider_on(Surface::Card, cx).into_any_element());
            }
        }
        let caption = "How the writer is told to name the branches it makes";
        groups.push(group("branch-names", "Branch names", Some(caption), true, rows, cx));
        groups
    }

    /// What each server does with pull requests by itself.
    fn pull_requests(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let servers: Vec<Server> = self
            .store
            .read(cx)
            .servers
            .iter()
            .filter(|server| server.connected() && server.protocol_version >= 9)
            .cloned()
            .collect();
        if servers.is_empty() {
            let text = "What your servers do once a thread's pull request merges is set on each of them, once one is connected.";
            return vec![note(text, cx)];
        }
        let merged = self.switch_rows(
            "done-on-merge",
            &servers,
            |server| server.pull_request_settings.done_on_merge,
            |store, server, on| {
                let settings = PullRequestSettings { done_on_merge: on, ..server.pull_request_settings };
                store.set_pull_request_settings(settings, server)
            },
            cx,
        );
        let worktrees = self.switch_rows(
            "remove-merged-worktrees",
            &servers,
            |server| server.pull_request_settings.remove_merged_worktrees,
            |store, server, on| {
                let settings = PullRequestSettings { remove_merged_worktrees: on, ..server.pull_request_settings };
                store.set_pull_request_settings(settings, server)
            },
            cx,
        );
        let mut groups = vec![
            group("merged", "Mark the thread done after merge or close", None, true, merged, cx),
            group("worktrees", "Remove the thread's worktree after merge", None, true, worktrees, cx),
        ];
        let pulling: Vec<Server> = servers.into_iter().filter(Server::pulls_default_branch).collect();
        if !pulling.is_empty() {
            let rows = self.switch_rows(
                "pull-default-branch",
                &pulling,
                |server| server.pull_request_settings.pull_default_branch,
                |store, server, on| {
                    let settings = PullRequestSettings { pull_default_branch: on, ..server.pull_request_settings };
                    store.set_pull_request_settings(settings, server)
                },
                cx,
            );
            groups.push(group("pull", "Automatically pull", Some(PULL_DEFAULT_BRANCH_CAPTION), true, rows, cx));
        }
        groups
    }

    fn keybindings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<KeybindingsPage> {
        let store = self.store.clone();
        self.keybindings.get_or_insert_with(|| cx.new(|cx| KeybindingsPage::new(store, window, cx))).clone()
    }

    /// Each integration's switch.
    fn integrations(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        for (index, integration) in Integration::ALL.into_iter().enumerate() {
            let on = self.store.read(cx).is_on(integration);
            let store = self.store.clone();
            let switch = Switch::new(("integration", index), on).on_change(move |on, _, cx| {
                store.update(cx, |store, cx| {
                    store.set_integration(integration, on);
                    cx.notify();
                })
            });
            let leading = div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(LOGO_GAP))
                .child(integration_logo(integration, cx))
                .child(label(integration.title(), Some(integration.summary().into()), None, Cut::Wraps, cx))
                .into_any_element();
            rows.push(
                div()
                    .relative()
                    .child(target(format!("integration-{}", integration.id())))
                    .child(switch_row(vec![leading], switch))
                    .into_any_element(),
            );
            if index + 1 < Integration::ALL.len() {
                rows.push(divider_on(Surface::Card, cx).into_any_element());
            }
        }
        let caption = "Each one that is on has a tab in the right panel.";
        vec![group("integrations", "Issue trackers", Some(caption), true, rows, cx)]
    }

    /// Scrolls the target a search picked to the middle of the page, once the page has been drawn
    /// with it.
    fn scroll_to(&self, target: String, window: &mut Window, cx: &mut Context<Self>) {
        cx.on_next_frame(window, move |page, _, cx| {
            let Some(bounds) = Targets::bounds(&target, cx) else { return };
            let scroll = &page.scroll;
            let (viewport, offset) = (scroll.bounds(), scroll.offset());
            let moved = viewport.center().y - bounds.center().y;
            let goal = (offset.y + moved).clamp(-scroll.max_offset().y, px(0.));
            page.scrolling = Some(Tween::new(f32::from(offset.y), f32::from(goal), SPRING).curve(spring));
            cx.notify();
        });
    }

    /// Moves the page on toward what a search picked, for this frame.
    fn step_scrolling(&mut self, window: &mut Window) {
        let Some(tween) = self.scrolling else { return };
        let offset = self.scroll.offset();
        self.scroll.set_offset(point(offset.x, px(tween.value())));
        if tween.moving() {
            window.request_animation_frame();
        } else {
            self.scrolling = None;
        }
    }

    fn removal_confirmation(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let (removal, leaving) = presented("removal", self.removal.clone(), window, cx)?;
        let message = removal.message(self.store.read(cx));
        let store = self.store.clone();
        let shown = Confirmation::new(removal.title()).message(message).answer(
            Answer::new(removal.answer(), move |_, cx| {
                store.update(cx, |store, cx| {
                    removal.run(store);
                    cx.notify();
                })
            })
            .danger(),
        );
        let page = cx.weak_entity();
        Some(
            confirmation(
                "removal",
                shown,
                move |_, cx| {
                    let _ = page.update(cx, |page, cx| {
                        page.removal = None;
                        cx.notify();
                    });
                },
                cx,
            )
            .leaving(leaving),
        )
    }
}

impl Render for SettingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let section = self.store.read(cx).settings.section.unwrap_or_default();
        if section != SettingsSection::Keyboard {
            self.keybindings = None;
        }
        Targets::clear(cx);
        let groups = match section {
            SettingsSection::General => self.general(cx),
            SettingsSection::Keyboard => Vec::new(),
            SettingsSection::Servers => self.servers(cx),
            SettingsSection::Accounts => self.agents(cx),
            SettingsSection::Projects => self.projects(cx),
            SettingsSection::TextGeneration => self.text_generation(cx),
            SettingsSection::PullRequests => self.pull_requests(cx),
            SettingsSection::Integrations => self.integrations(cx),
        };
        let keybindings = (section == SettingsSection::Keyboard).then(|| self.keybindings(window, cx));
        if let Some(target) = self.store.read(cx).settings.target.clone() {
            self.store.update(cx, |store, _| store.settings.target = None);
            match &keybindings {
                Some(keybindings) => keybindings.update(cx, |page, _| page.reveal(target)),
                None => self.scroll_to(target, window, cx),
            }
        }
        self.step_scrolling(window);
        let removal = self.removal_confirmation(window, cx);
        let setup = present_sheet("setup", self.setup.clone(), |sheet, leaving| sheet.leaving = leaving, window, cx);
        let account =
            present_sheet("agent-account", self.account.clone(), |sheet, leaving| sheet.leaving = leaving, window, cx);
        div()
            .size_full()
            .pt(px(theme::TOP_BAR))
            .relative()
            .child(match keybindings {
                // Its own list, which lays out only the rows on screen.
                Some(keybindings) => div().size_full().child(keybindings).into_any_element(),
                None => div()
                    .id("settings-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .on_scroll_wheel(cx.listener(|page, _, _, _| page.scrolling = None))
                    .child(
                        column(div().flex().flex_col().gap(px(GROUP_GAP)).children(groups))
                            .pt(px(PAGE_PADDING))
                            .pb(px(END_ROOM)),
                    )
                    .into_any_element(),
            })
            .child(top_fade(c.background))
            .children(setup)
            .children(account)
            .children(removal)
    }
}

/// The page fading in under the top bar.
pub fn top_fade(background: Hsla) -> Div {
    div().absolute().top(px(theme::TOP_BAR)).left_0().right_0().h(px(FADE)).bg(linear_gradient(
        180.,
        linear_color_stop(background, 0.),
        linear_color_stop(background.opacity(0.), 1.),
    ))
}

/// An integration's logo on a tile of its own, as an app's icon sits.
fn integration_logo(integration: Integration, cx: &App) -> AnyElement {
    let c = colors(cx);
    div()
        .size(px(LOGO_TILE))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(Radius::MD))
        .bg(Surface::Card.layer(Layer::Box, c))
        .border_1()
        .border_color(Surface::Card.border(c))
        .text_color(c.foreground)
        .child(icons::symbol(integration.symbol(), 18.))
        .into_any_element()
}

/// A settings control: outline, large, on the card.
fn outline(button: ActionButton) -> ActionButton {
    button.variant(Variant::Outline).size(ControlSize::Large).surface(Surface::Card)
}

fn large(id: impl Into<ElementId>, title: impl Into<SharedString>) -> ActionButton {
    outline(ActionButton::new(id, title))
}

/// A server that is connected and new enough to be told which model writes its text.
fn writes_text(server: &Server) -> bool {
    server.state == State::Connected && server.protocol_version >= 4
}

fn names_branches(server: &Server) -> bool {
    writes_text(server) && server.protocol_version >= 6
}

fn state_label(server: &Server) -> String {
    match server.state {
        State::Connected => "Connected".into(),
        State::Connecting => "Connecting".into(),
        State::Disconnected => "Offline".into(),
        State::Refused => format!("This server no longer accepts {}", crate::models::THIS_DEVICE),
    }
}

/// The line under a server that says whether it has every thread of the others. Always one line,
/// so the rows around it never move.
fn sync_line(sync: &SyncView, cx: &App) -> AnyElement {
    const SIZE: f32 = 11.5;
    let c = colors(cx);
    let symbol = match sync.state {
        SyncState::Synced => div().text_color(c.success).child(icons::symbol("check", SIZE)).into_any_element(),
        SyncState::Copying => Spinner::new(SIZE).render(cx).into_any_element(),
        SyncState::Failed => {
            div().text_color(c.warning).child(icons::symbol("triangle-alert", SIZE - 1.)).into_any_element()
        }
        SyncState::Waiting | SyncState::Off => icons::symbol("circle-dashed", SIZE - 1.).into_any_element(),
    };
    let line = div()
        .flex()
        .items_center()
        .gap(px(5.))
        .text_size(px(SIZE))
        .text_color(c.muted_foreground)
        .child(div().flex_shrink_0().size(px(SIZE)).flex().items_center().justify_center().child(symbol))
        .child(div().min_w_0().whitespace_nowrap().overflow_hidden().text_ellipsis().child(models::sync_line(sync)));
    match &sync.error {
        Some(error) => line.id("sync-line").tooltip(tooltip(error.clone())).into_any_element(),
        None => line.into_any_element(),
    }
}

fn description(server: &Server) -> String {
    if server.state != State::Connected {
        return state_label(server);
    }
    let mut agents: Vec<_> = server.agents.iter().collect();
    agents.sort_by_key(|(agent, _)| agent_name(**agent));
    let agents: Vec<String> =
        agents.into_iter().map(|(agent, version)| format!("{} {version}", agent_name(*agent))).collect();
    let installed = if agents.is_empty() { "no agent installed".to_string() } else { agents.join(", ") };
    format!("Connected · version {} · {installed}", server.version)
}

/// Why its accounts can't be shown or changed, if they can't.
fn accounts_unavailable(server: &Server) -> Option<String> {
    if server.state != State::Connected {
        return Some(description(server));
    }
    (!server.switches_accounts()).then(|| "Update it to give its agents more accounts".into())
}

fn server_label(server: &Server, cx: &App) -> AnyElement {
    label(server.name.clone(), None, Some("server"), Cut::Wraps, cx)
}

/// How a server's writer is told to name the branches it makes, to change and to put back.
pub struct BranchInstructions {
    store: Entity<Store>,
    server: Server,
    text: Entity<gpui_kit::component::input::TextareaState>,
    _subscription: Subscription,
}

impl BranchInstructions {
    fn new(store: Entity<Store>, server: &Server, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let text = text_area_state("How to name a branch", 3, 16, window, cx);
        let instructions = server.branch_instructions.clone();
        text.update(cx, |text, cx| text.set_value(instructions, window, cx));
        let subscription = cx.observe(&text, |_, _, cx| cx.notify());
        Self { store, server: server.clone(), text, _subscription: subscription }
    }

    fn follow(&mut self, server: &Server, window: &mut Window, cx: &mut Context<Self>) {
        if server.branch_instructions != self.server.branch_instructions {
            let instructions = server.branch_instructions.clone();
            self.text.update(cx, |text, cx| text.set_value(instructions, window, cx));
        }
        self.server = server.clone();
        cx.notify();
    }

    fn changed(&self, cx: &App) -> bool {
        self.text.read(cx).value().trim() != self.server.branch_instructions
    }
}

impl Render for BranchInstructions {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let changed = self.changed(cx);
        let unchanged = self.server.branch_instructions == self.server.default_branch_instructions && !changed;
        div().p(px(INSET)).flex().flex_col().gap(px(6.)).child(TextArea::new(&self.text).surface(Surface::Card)).child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.))
                .child(large("branch-reset", "Reset").disabled(unchanged).on_click(cx.listener(
                    |this, _, window, cx| {
                        let default = this.server.default_branch_instructions.clone();
                        this.text.update(cx, |text, cx| text.set_value(default, window, cx));
                        let id = this.server.id.clone();
                        this.store.update(cx, |store, _| store.set_branch_instructions(None, &id));
                    },
                )))
                .child(large("branch-save", "Save").primary().disabled(!changed).on_click(cx.listener(
                    |this, _, _, cx| {
                        let (text, id) = (this.text.read(cx).value().to_string(), this.server.id.clone());
                        this.store.update(cx, |store, _| store.set_branch_instructions(Some(text), &id));
                    },
                ))),
        )
    }
}

/// The shell script that runs in each new worktree of a project before the agent starts there.
pub struct SetupSheet {
    store: Entity<Store>,
    page: WeakEntity<SettingsPage>,
    project: Project,
    script: Entity<gpui_kit::component::input::TextareaState>,
    /// It has closed and fades out.
    pub leaving: bool,
}

impl SetupSheet {
    fn new(
        store: Entity<Store>,
        page: WeakEntity<SettingsPage>,
        project: Project,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let script = text_area_state("cp \"$MOTILE_PROJECT/.env\" . && pnpm install", 5, 14, window, cx);
        let written = project.setup.clone().unwrap_or_default();
        script.update(cx, |script, cx| {
            script.set_value(written, window, cx);
            script.focus(window, cx);
        });
        Self { store, page, project, script, leaving: false }
    }

    fn close(&self, cx: &mut App) {
        let _ = self.page.update(cx, |page, cx| {
            page.setup = None;
            cx.notify();
        });
    }

    fn save(&self, cx: &mut App) {
        let script = self.script.read(cx).value().to_string();
        let project = self.project.clone();
        self.store.update(cx, |store, _| store.set_setup(&project, script));
        self.close(cx);
    }
}

impl Render for SetupSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let content = div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .text_color(c.foreground)
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("Worktree Setup for {}", self.project.name)),
            )
            .child(div().text_size(px(10.)).text_color(c.muted_foreground).child(
                "A shell script that runs in each new worktree before the agent starts there, to install what the work needs. $MOTILE_PROJECT is the project's folder, as in: cp \"$MOTILE_PROJECT/.env\" . && pnpm install",
            ))
            .child(TextArea::new(&self.script).monospaced(true))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(ActionButton::new("setup-cancel", "Cancel").on_click(cx.listener(|this, _, _, cx| this.close(cx))))
                    .child(
                        ActionButton::new("setup-save", "Save")
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            );
        let (close, save) = (cx.entity().downgrade(), cx.entity().downgrade());
        modal("setup", content, move |_, cx| {
            close.update(cx, |this, cx| this.close(cx)).ok();
        })
        .on_return(move |_, cx| {
            save.update(cx, |this, cx| this.save(cx)).ok();
        })
        .width(440.)
        .leaving(self.leaving)
    }
}
