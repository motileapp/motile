//! One section of the settings: its groups of rows, and the way to the group a search picked.

use std::collections::HashMap;

use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::link::State;
use motile_protocol::wire::PullRequestSettings;

use super::INSET;
use super::usage::UsageView;
use crate::models::{Project, Server, agent_name};
use crate::store::Store;
use crate::store::settings::SettingsSection;
use crate::theme::{self, Appearance, ControlSize, Radius, Surface, colors};
use crate::ui::menu::Menu;
use crate::ui::sheet::sheet;
use crate::ui::{
    ActionButton, ActionMenu, Segmented, Spinner, Switch, Variant, card, card_default, divider, icons, logos,
};
use crate::updater::UpdateState;

/// How wide the groups are at most, however wide the window.
const CONTENT_WIDTH: f32 = 640.;
/// The least a row keeps for what it is about before its controls go under it.
const LEADING_WIDTH: f32 = 200.;

type Group = (&'static str, AnyElement);

pub struct SettingsPage {
    store: Entity<Store>,
    branch_editors: HashMap<String, Entity<BranchInstructions>>,
    pub setup: Option<Entity<SetupSheet>>,
    usage: Option<Entity<UsageView>>,
    scroll: ScrollHandle,
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
            usage: None,
            scroll: ScrollHandle::new(),
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

    fn general(&self, cx: &mut Context<Self>) -> Vec<Group> {
        vec![self.account(cx), self.updates(cx), self.appearance(cx), self.messages(cx), self.storage(cx)]
    }

    fn account(&self, cx: &mut Context<Self>) -> Group {
        let c = colors(cx);
        let account = self.store.read(cx).account.clone();
        let email = account.user.as_ref().map(|user| user.email.clone()).unwrap_or_default();
        let mut trailing = vec![
            div()
                .text_color(c.secondary)
                .child(if account.signed_in { email } else { "Not signed in".into() })
                .into_any_element(),
        ];
        if account.signed_in {
            let store = self.store.clone();
            trailing.push(
                ActionButton::new("sign-out", "Sign Out")
                    .surface(Surface::Secondary)
                    .small()
                    .on_click(move |_, _, cx| {
                        store.update(cx, |store, cx| {
                            store.sign_out(cx);
                            cx.notify();
                        })
                    })
                    .into_any_element(),
            );
        }
        group("account", "Account", None, vec![row(label("Signed in as", None, None, false, cx), trailing)], cx)
    }

    fn updates(&self, cx: &mut Context<Self>) -> Group {
        let (state, current) = {
            let updater = &self.store.read(cx).updater;
            (updater.state.clone(), updater.current)
        };
        let store = self.store.clone();
        let check = ActionButton::new("check-updates", "Check for Updates")
            .surface(Surface::Secondary)
            .small()
            .pending(state == UpdateState::Checking)
            .on_click(move |_, _, cx| store.update(cx, |store, cx| store.updater.check(true, cx)));
        let mut rows =
            vec![row(label(format!("Motile {current}"), None, None, false, cx), vec![check.into_any_element()])];
        if state != UpdateState::Idle {
            rows.push(divider(cx).into_any_element());
            rows.push(self.app_update_row(&state, current, cx));
        }
        group("updates", "Updates", None, rows, cx)
    }

    /// A new version of the client: the check, what it found, and the download that follows.
    fn app_update_row(&self, state: &UpdateState, current: &str, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let line = |text: String, symbol: &'static str, trailing: Option<AnyElement>| {
            div()
                .flex()
                .items_center()
                .gap(px(7.))
                .min_h(px(ControlSize::Small.height()))
                .child(div().text_color(c.secondary).child(icons::symbol(symbol, 13.)))
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(text),
                )
                .child(div().flex_1().min_w(px(4.)))
                .children(trailing)
        };
        let spinner = || {
            div().text_color(c.secondary).child(Spinner::new(ControlSize::Small.symbol()).render(cx)).into_any_element()
        };
        let content = match state {
            UpdateState::Idle => div(),
            UpdateState::Checking => line("Checking for updates".into(), "refresh-cw", Some(spinner())),
            UpdateState::UpToDate => line(format!("Motile {current} is the newest version"), "circle-check", None),
            UpdateState::Available(version) => {
                let download = ActionButton::new("download-update", "Download")
                    .surface(Surface::Secondary)
                    .small()
                    .on_click({
                        let store = self.store.clone();
                        move |_, _, cx| store.update(cx, |store, cx| store.updater.download(cx))
                    })
                    .into_any_element();
                line(format!("Motile {version} is available"), "circle-arrow-down", Some(download))
            }
            UpdateState::Failed(message) => {
                let store = self.store.clone();
                let retry = ActionButton::new("retry-update", "Try Again")
                    .surface(Surface::Secondary)
                    .small()
                    .on_click(move |_, _, cx| store.update(cx, |store, cx| store.updater.check(true, cx)))
                    .into_any_element();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(line("The update didn’t work".into(), "triangle-alert", Some(retry)))
                    .child(div().text_size(px(11.)).text_color(c.secondary).child(message.clone()))
            }
        };
        div().px(px(INSET)).py(px(10.)).child(content).into_any_element()
    }

    fn appearance(&self, cx: &mut Context<Self>) -> Group {
        let chosen = self.store.read(cx).prefs.get::<Appearance>("appearance").unwrap_or_default();
        let store = self.store.clone();
        let picker = Segmented::new(
            "appearance",
            Appearance::ALL.iter().map(|appearance| SharedString::from(appearance.label())).collect(),
            Appearance::ALL.iter().position(|appearance| *appearance == chosen).unwrap_or(0),
        )
        .on_select(move |index, _, cx| {
            store.update(cx, |store, cx| {
                store.prefs.set("appearance", Appearance::ALL[index]);
                cx.notify();
            })
        });
        group(
            "appearance",
            "Appearance",
            None,
            vec![row(label("Theme", None, None, false, cx), vec![picker.into_any_element()])],
            cx,
        )
    }

    fn messages(&self, cx: &mut Context<Self>) -> Group {
        let steers = self.store.read(cx).prefs.bool(Store::STEERS_KEY);
        let description = if steers {
            "The agent reads it at once, in the turn that runs"
        } else {
            "Waits for the turn to end and starts the next one"
        };
        let store = self.store.clone();
        let picker = Segmented::new("steers", vec!["Queue".into(), "Steer".into()], usize::from(steers)).on_select(
            move |index, _, cx| {
                store.update(cx, |store, cx| {
                    store.prefs.set(Store::STEERS_KEY, index == 1);
                    cx.notify();
                })
            },
        );
        let leading = label("Sent while the agent works", Some(description.into()), None, false, cx);
        group("messages", "Messages", None, vec![row(leading, vec![picker.into_any_element()])], cx)
    }

    /// The servers keep every image and video; the ones kept here only make threads open with them.
    fn storage(&self, cx: &mut Context<Self>) -> Group {
        let storage = self.store.read(cx).media_storage;
        let description = match &storage {
            Some(storage) => {
                format!("{} of {} on this Mac. Your servers keep them all.", bytes(storage.used), bytes(storage.limit))
            }
            None => "Kept on this Mac so threads open with them".into(),
        };
        let store = self.store.clone();
        let clear = ActionButton::new("clear-media", "Clear")
            .surface(Surface::Secondary)
            .small()
            .disabled(storage.map_or(0, |storage| storage.used) == 0)
            .on_click(move |_, _, cx| {
                crate::media::MediaFiles::clear(cx);
                store.update(cx, |store, _| store.clear_media());
            });
        let leading = label("Images and videos", Some(description), None, false, cx);
        group("storage", "Storage", None, vec![row(leading, vec![clear.into_any_element()])], cx)
    }

    fn servers(&self, cx: &mut Context<Self>) -> Group {
        let servers = self.store.read(cx).servers.clone();
        let mut rows = Vec::new();
        for (index, server) in servers.iter().enumerate() {
            let (store, removed) = (self.store.clone(), server.clone());
            let mut trailing = Vec::new();
            trailing.extend(server_update_status(&self.store, server, cx));
            trailing.push(
                ActionButton::new(("remove-server", index), "Remove")
                    .surface(Surface::Secondary)
                    .small()
                    .on_click(move |_, _, cx| {
                        store.update(cx, |store, cx| {
                            store.remove_server(&removed);
                            cx.notify();
                        })
                    })
                    .into_any_element(),
            );
            rows.push(row(label(server.name.clone(), Some(description(server)), None, false, cx), trailing));
            rows.push(divider(cx).into_any_element());
        }
        let store = self.store.clone();
        let add = ActionButton::new("add-server", "Add a Server…").surface(Surface::Secondary).small().on_click(
            move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.shows_add_server = true;
                    cx.notify();
                })
            },
        );
        rows.push(row(add, Vec::new()));
        group("servers", "Servers", None, rows, cx)
    }

    fn projects(&self, cx: &mut Context<Self>) -> Group {
        let projects = self.store.read(cx).projects.clone();
        let no_servers = self.store.read(cx).servers.is_empty();
        let mut rows = Vec::new();
        for (index, project) in projects.iter().enumerate() {
            let names_setup =
                self.store.read(cx).server(Some(&project.server_id)).map_or(0, |server| server.protocol_version) >= 6;
            let (store, chosen) = (self.store.clone(), project.clone());
            let icon_menu = ActionMenu::new(("project-icon", index), "Icon", move |_, _| {
                let (choose, reset) = (store.clone(), store.clone());
                let (choose_project, reset_project) = (chosen.clone(), chosen.clone());
                Menu::new()
                    .item("Choose an Image…", move |_, cx| {
                        choose.update(cx, |store, cx| {
                            store.icon_project = Some(choose_project.clone());
                            cx.notify();
                        })
                    })
                    .item("Use the Icon in Its Folder", move |_, cx| {
                        reset.update(cx, |store, _| store.set_icon(&reset_project, None))
                    })
            })
            .button(|button| button.variant(Variant::Secondary).small());
            let mut trailing = vec![icon_menu.into_any_element()];
            if names_setup {
                let setup_project = project.clone();
                trailing.push(
                    ActionButton::new(("project-setup", index), "Setup…")
                        .surface(Surface::Secondary)
                        .small()
                        .help(format!("The script that runs in each new worktree of {}", project.name))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let (store, page) = (this.store.clone(), cx.weak_entity());
                            let project = setup_project.clone();
                            this.setup = Some(cx.new(|cx| SetupSheet::new(store, page, project, window, cx)));
                            cx.notify();
                        }))
                        .into_any_element(),
                );
            }
            let (store, removed) = (self.store.clone(), project.clone());
            trailing.push(
                ActionButton::new(("remove-project", index), "Remove")
                    .surface(Surface::Secondary)
                    .small()
                    .on_click(move |_, _, cx| {
                        store.update(cx, |store, cx| {
                            store.remove_project(&removed);
                            cx.notify();
                        })
                    })
                    .into_any_element(),
            );
            let leading = div()
                .flex()
                .items_center()
                .gap(px(10.))
                .min_w_0()
                .child(div().flex_shrink_0().child(logos::project_icon(Some(project), 26., cx)))
                .child(label(project.name.clone(), Some(project.path.clone()), None, true, cx));
            rows.push(row(leading, trailing));
            rows.push(divider(cx).into_any_element());
        }
        let store = self.store.clone();
        let add = ActionButton::new("add-project", "Add a Project…")
            .surface(Surface::Secondary)
            .small()
            .disabled(no_servers)
            .on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.close_settings();
                    store.add_project();
                    cx.notify();
                })
            });
        rows.push(row(add, Vec::new()));
        group("projects", "Projects", None, rows, cx)
    }

    /// The model that writes thread titles, branch names, commit messages and pull requests, and
    /// how it names branches, by server.
    fn text_generation(&self, cx: &mut Context<Self>) -> Vec<Group> {
        let servers: Vec<Server> =
            self.store.read(cx).servers.iter().filter(|server| writes_text(server)).cloned().collect();
        if servers.is_empty() {
            let text = "The model that writes thread titles, branch names, commit messages and pull requests is set on each of your servers, once one is connected.";
            return vec![("text-model", note(text, cx))];
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
                let mut menu = Menu::new().checked(server.text_model.is_none(), "Automatic", {
                    let (store, id) = (store.clone(), server.id.clone());
                    move |_, cx| store.update(cx, |store, _| store.set_text_model(None, &id))
                });
                for model in &server.models {
                    let (store, id, model_id) = (store.clone(), server.id.clone(), model.id.clone());
                    menu = menu.checked(
                        server.text_model.as_ref() == Some(&model.id),
                        model.name.clone(),
                        move |_, cx| {
                            let model = model_id.clone();
                            store.update(cx, |store, _| store.set_text_model(Some(model), &id))
                        },
                    );
                }
                menu
            })
            .button(|button| button.variant(Variant::Secondary).small());
            rows.push(row(server_label(server, cx), vec![menu.into_any_element()]));
            if index + 1 < servers.len() {
                rows.push(divider(cx).into_any_element());
            }
        }
        let caption = "The model that writes thread titles, branch names, commit messages and pull requests";
        let mut groups = vec![group("text-model", "Model", Some(caption), rows, cx)];
        let naming: Vec<&Server> = servers.iter().filter(|server| names_branches(server)).collect();
        if naming.is_empty() {
            return groups;
        }
        let mut rows = Vec::new();
        for (index, server) in naming.iter().enumerate() {
            if naming.len() > 1 {
                rows.push(row(server_label(server, cx), Vec::new()));
            }
            if let Some(editor) = self.branch_editors.get(&server.id) {
                rows.push(editor.clone().into_any_element());
            }
            if index + 1 < naming.len() {
                rows.push(divider(cx).into_any_element());
            }
        }
        let caption = "How the writer is told to name the branches it makes";
        groups.push(group("branch-names", "Branch names", Some(caption), rows, cx));
        groups
    }

    /// What each server does with pull requests by itself.
    fn pull_requests(&self, cx: &mut Context<Self>) -> Vec<Group> {
        let servers: Vec<Server> = self
            .store
            .read(cx)
            .servers
            .iter()
            .filter(|server| server.state == State::Connected && server.protocol_version >= 9)
            .cloned()
            .collect();
        if servers.is_empty() {
            let text = "What your servers do once a thread's pull request merges is set on each of them, once one is connected.";
            return vec![("merged", note(text, cx))];
        }
        let switch_rows = |id: &'static str,
                           on: fn(PullRequestSettings) -> bool,
                           change: fn(PullRequestSettings, bool) -> PullRequestSettings,
                           cx: &mut Context<Self>| {
            let mut rows = Vec::new();
            for (index, server) in servers.iter().enumerate() {
                let settings = server.pull_request_settings;
                let (store, changed) = (self.store.clone(), server.clone());
                let switch = Switch::new((id, index), on(settings)).on_change(move |on, _, cx| {
                    store.update(cx, |store, cx| {
                        store.set_pull_request_settings(change(settings, on), &changed);
                        cx.notify();
                    })
                });
                rows.push(row(server_label(server, cx), vec![switch.into_any_element()]));
                if index + 1 < servers.len() {
                    rows.push(divider(cx).into_any_element());
                }
            }
            rows
        };
        let merged = switch_rows(
            "done-on-merge",
            |settings| settings.done_on_merge,
            |settings, on| PullRequestSettings { done_on_merge: on, ..settings },
            cx,
        );
        let worktrees = switch_rows(
            "remove-merged-worktrees",
            |settings| settings.remove_merged_worktrees,
            |settings, on| PullRequestSettings { remove_merged_worktrees: on, ..settings },
            cx,
        );
        vec![
            group("merged", "Mark the thread done", Some("When its pull request merges or closes"), merged, cx),
            group(
                "worktrees",
                "Remove the thread's worktree",
                Some(
                    "After its pull request merges, if all of it is pushed. Its branch stays, and the worktree is made again if the thread goes on.",
                ),
                worktrees,
                cx,
            ),
        ]
    }

    fn usage(&mut self, cx: &mut Context<Self>) -> Vec<Group> {
        let store = self.store.clone();
        let usage = self.usage.get_or_insert_with(|| cx.new(|cx| UsageView::new(store, cx))).clone();
        vec![("usage", usage.into_any_element())]
    }
}

impl Render for SettingsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let section = self.store.read(cx).settings.section.unwrap_or_default();
        if section != SettingsSection::Usage {
            self.usage = None;
        }
        let groups = match section {
            SettingsSection::General => self.general(cx),
            SettingsSection::Servers => vec![self.servers(cx)],
            SettingsSection::Projects => vec![self.projects(cx)],
            SettingsSection::TextGeneration => self.text_generation(cx),
            SettingsSection::PullRequests => self.pull_requests(cx),
            SettingsSection::Usage => self.usage(cx),
        };
        if let Some(target) = self.store.read(cx).settings.target.clone() {
            if let Some(index) = groups.iter().position(|(id, _)| *id == target) {
                self.scroll.scroll_to_item(index);
            }
            self.store.update(cx, |store, _| store.settings.target = None);
        }
        div().size_full().pt(px(theme::TOP_BAR)).child(
            div()
                .id("settings-scroll")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .p(px(20.))
                .flex()
                .flex_col()
                .gap(px(22.))
                .children(groups.into_iter().map(|(_, group)| {
                    div().w_full().flex().justify_center().child(div().w_full().max_w(px(CONTENT_WIDTH)).child(group))
                })),
        )
    }
}

/// A server that is connected and new enough to be told which model writes its text.
fn writes_text(server: &Server) -> bool {
    server.state == State::Connected && server.protocol_version >= 4
}

fn names_branches(server: &Server) -> bool {
    writes_text(server) && server.protocol_version >= 6
}

fn description(server: &Server) -> String {
    let mut agents: Vec<_> = server.agents.iter().collect();
    agents.sort_by_key(|(agent, _)| agent_name(**agent));
    let agents: Vec<String> =
        agents.into_iter().map(|(agent, version)| format!("{} {version}", agent_name(*agent))).collect();
    let installed = if agents.is_empty() { "no agent installed".to_string() } else { agents.join(", ") };
    match server.state {
        State::Connected => format!("Connected · version {} · {installed}", server.version),
        State::Connecting => "Connecting…".into(),
        State::Disconnected => "Offline".into(),
        State::Refused => "This server no longer accepts this Mac".into(),
    }
}

/// What stands at the end of a server's line: the offer to update it, or the update as it goes.
fn server_update_status(store: &Entity<Store>, server: &Server, cx: &App) -> Option<AnyElement> {
    let c = colors(cx);
    let state = store.read(cx);
    if let Some(update) = state.server_updates.get(&server.id) {
        let progress = if update.restarting {
            "Restarting".to_string()
        } else if let Some(fraction) = update.fraction {
            format!("Updating {}%", (fraction * 100.) as u32)
        } else {
            "Updating".to_string()
        };
        return Some(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_color(c.secondary)
                .child(div().text_size(px(11.)).child(progress))
                .child(Spinner::new(ControlSize::Small.symbol()).render(cx))
                .into_any_element(),
        );
    }
    if !state.is_outdated(server) {
        return None;
    }
    let help = format!(
        "Install version {} on {}. It restarts, and no agent may be working.",
        state.updater.latest.clone().unwrap_or_default(),
        server.name
    );
    let (handle, server) = (store.clone(), server.clone());
    Some(
        ActionButton::new(SharedString::from(format!("update-{}", server.id)), "Update")
            .small()
            .help(help)
            .on_click(move |_, _, cx| {
                handle.update(cx, |store, cx| {
                    store.update_server(&server);
                    cx.notify();
                })
            })
            .into_any_element(),
    )
}

fn bytes(count: u64) -> String {
    let count = count as f64;
    if count < 1000. {
        return format!("{count} bytes");
    }
    let (value, unit) = [(1e3, "KB"), (1e6, "MB"), (1e9, "GB"), (1e12, "TB")]
        .into_iter()
        .rev()
        .find(|(size, _)| count >= *size)
        .unwrap_or((1e3, "KB"));
    let shown = count / value;
    if shown < 10. && shown.fract() >= 0.05 {
        format!("{shown:.1} {unit}")
    } else {
        format!("{} {unit}", shown.round())
    }
}

/// A quiet title over a bordered card of rows, which a search scrolls to by its `id`.
fn group(id: &'static str, title: &str, caption: Option<&str>, rows: Vec<AnyElement>, cx: &App) -> Group {
    let c = colors(cx);
    let element = div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .px(px(INSET))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(13.)).text_color(c.secondary).child(title.to_string()))
                .when_some(caption, |heading, caption| {
                    heading.child(div().text_size(px(11.5)).text_color(c.tertiary).child(caption.to_string()))
                }),
        )
        .child(card_default(div().flex().flex_col(), cx).children(rows))
        .into_any_element();
    (id, element)
}

/// What the row is about on the left, its controls on the right, with the same room above and
/// below. A row too narrow for both puts its controls under what they are about.
fn row(leading: impl IntoElement, trailing: Vec<AnyElement>) -> AnyElement {
    div()
        .px(px(INSET))
        .py(px(10.))
        .min_h(px(46.))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(10.))
        .child(
            div()
                .flex_grow(1.)
                .flex_shrink(1.)
                .flex_basis(px(LEADING_WIDTH))
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(leading),
        )
        .when(!trailing.is_empty(), |row| row.child(div().flex().items_center().gap(px(10.)).children(trailing)))
        .into_any_element()
}

/// What a row is about: its name, and under it what it does. A description that is a path is
/// cut at its start, not wrapped.
fn label(
    title: impl Into<SharedString>,
    description: Option<String>,
    icon: Option<&'static str>,
    truncates: bool,
    cx: &App,
) -> AnyElement {
    let c = colors(cx);
    div()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(8.))
        .when_some(icon, |label, icon| label.child(div().text_color(c.secondary).child(icons::symbol(icon, 13.))))
        .child(
            div()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(title.into()))
                .when_some(description, |label, description| {
                    label.child(
                        div()
                            .text_size(px(11.5))
                            .text_color(c.secondary)
                            .when(truncates, |text| text.whitespace_nowrap().overflow_hidden().text_ellipsis_start())
                            .child(description),
                    )
                }),
        )
        .into_any_element()
}

fn server_label(server: &Server, cx: &App) -> AnyElement {
    label(server.name.clone(), None, Some("server"), false, cx)
}

/// Said on a page with nothing to set yet.
fn note(text: &str, cx: &App) -> AnyElement {
    let c = colors(cx);
    card_default(div().p(px(INSET)).text_size(px(13.)).text_color(c.secondary).child(text.to_string()), cx)
        .into_any_element()
}

/// How a server's writer is told to name the branches it makes, to change and to put back.
pub struct BranchInstructions {
    store: Entity<Store>,
    server: Server,
    text: Entity<TextareaState>,
    _subscription: Subscription,
}

impl BranchInstructions {
    fn new(store: Entity<Store>, server: &Server, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let text = cx.new(|cx| TextareaState::new(window, cx).default_value(server.branch_instructions.clone()));
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
        div()
            .px(px(INSET))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                card(div().h(px(64.)).p(px(6.)), Radius::CONTROL, cx).child(
                    Textarea::new(&self.text).appearance(false).xsmall().text_size(px(12.)).px_0().py_0().h_full(),
                ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        ActionButton::new("branch-reset", "Reset")
                            .surface(Surface::Secondary)
                            .small()
                            .disabled(unchanged)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let default = this.server.default_branch_instructions.clone();
                                this.text.update(cx, |text, cx| text.set_value(default, window, cx));
                                let id = this.server.id.clone();
                                this.store.update(cx, |store, _| store.set_branch_instructions(None, &id));
                            })),
                    )
                    .child(
                        ActionButton::new("branch-save", "Save")
                            .surface(Surface::Secondary)
                            .primary()
                            .small()
                            .disabled(!changed)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let (text, id) = (this.text.read(cx).value().to_string(), this.server.id.clone());
                                this.store.update(cx, |store, _| store.set_branch_instructions(Some(text), &id));
                            })),
                    ),
            )
    }
}

/// The shell script that runs in each new worktree of a project before the agent starts there.
pub struct SetupSheet {
    store: Entity<Store>,
    page: WeakEntity<SettingsPage>,
    project: Project,
    script: Entity<TextareaState>,
}

impl SetupSheet {
    fn new(
        store: Entity<Store>,
        page: WeakEntity<SettingsPage>,
        project: Project,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let script =
            cx.new(|cx| TextareaState::new(window, cx).default_value(project.setup.clone().unwrap_or_default()));
        script.update(cx, |script, cx| script.focus(window, cx));
        Self { store, page, project, script }
    }

    fn close(&self, cx: &mut App) {
        let _ = self.page.update(cx, |page, cx| {
            page.setup = None;
            cx.notify();
        });
    }
}

impl Render for SetupSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let content = div()
            .id("setup-sheet")
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.close(cx);
                }
            }))
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .text_color(c.text)
            .child(
                div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(format!("Worktree setup for {}", self.project.name)),
            )
            .child(div().text_size(px(10.)).text_color(c.secondary).child(
                "A shell script that runs in each new worktree before the agent starts there, to install what the work needs. $MOTILE_PROJECT is the project's folder, as in: cp \"$MOTILE_PROJECT/.env\" . && pnpm install",
            ))
            .child(
                card(div().h(px(140.)).p(px(6.)).font_family(theme::MONO_FONT), Radius::CONTROL, cx).child(
                    Textarea::new(&self.script)
                        .appearance(false)
                        .xsmall()
                        .text_size(px(12.))
                        .font_family(theme::MONO_FONT)
                        .px_0()
                        .py_0()
                        .h_full(),
                ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(ActionButton::new("setup-cancel", "Cancel").surface(Surface::Secondary).on_click(cx.listener(|this, _, _, cx| this.close(cx))))
                    .child(ActionButton::new("setup-save", "Save").surface(Surface::Secondary).primary().on_click(cx.listener(|this, _, _, cx| {
                        let script = this.script.read(cx).value().to_string();
                        let project = this.project.clone();
                        this.store.update(cx, |store, _| store.set_setup(&project, script));
                        this.close(cx);
                    }))),
            );
        sheet("setup", 440., content, cx)
    }
}
