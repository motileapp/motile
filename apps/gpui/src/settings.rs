//! The Settings window: the account, updates, the appearance, the images and videos kept here,
//! the servers, what writes their text, and the projects.

use std::collections::HashMap;

use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::link::State;

use crate::models::{Project, Server, agent_name};
use crate::store::Store;
use crate::theme::{Appearance, colors, is_dark};
use crate::ui::button::Button;
use crate::ui::menu::{Anchor, Menu};
use crate::ui::sheet::sheet;
use crate::ui::{icons, logos};

const INSET: f32 = 12.;
const WIDTH: f32 = 520.;
const HEIGHT: f32 = 560.;

/// Opens the window, or brings it forward when it is open.
pub fn open(store: &Entity<Store>, cx: &mut App) {
    let open = cx.windows().into_iter().find(|window| {
        window
            .downcast::<gpui_kit::base::Root>()
            .is_some_and(|root| root.read(cx).is_ok_and(|root| root.view().clone().downcast::<SettingsView>().is_ok()))
    });
    if let Some(window) = open {
        let _ = window.update(cx, |_, window, _| window.activate_window());
        return;
    }
    let bounds = Bounds::centered(None, size(px(WIDTH), px(HEIGHT)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("Motile Settings".into()),
            appears_transparent: false,
            traffic_light_position: None,
        }),
        is_resizable: false,
        is_minimizable: false,
        ..Default::default()
    };
    let store = store.clone();
    let opened = gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| SettingsView::new(store, window, cx)));
    if let Err(error) = opened {
        tracing::error!("the settings couldn't open: {error:#}");
    }
}

pub struct SettingsView {
    store: Entity<Store>,
    branch_editors: HashMap<String, Entity<BranchInstructions>>,
    setup: Option<Entity<SetupSheet>>,
    model_menus: HashMap<String, Anchor>,
    icon_menus: HashMap<String, Anchor>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        store.update(cx, |store, _| store.refresh_media_storage());
        let subscriptions = vec![cx.observe_in(&store, window, |this, _, window, cx| {
            this.follow_servers(window, cx);
            cx.notify();
        })];
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let mut view = Self {
            focus,
            store,
            branch_editors: HashMap::new(),
            setup: None,
            model_menus: HashMap::new(),
            icon_menus: HashMap::new(),
            _subscriptions: subscriptions,
        };
        view.follow_servers(window, cx);
        view
    }

    /// Each server that names branches has its own box for how.
    fn follow_servers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let servers: Vec<Server> = self
            .store
            .read(cx)
            .servers
            .iter()
            .filter(|server| writes_text(server) && server.protocol_version >= 6)
            .cloned()
            .collect();
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

    fn account(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let account = self.store.read(cx).account.clone();
        let store = self.store.clone();
        section(
            "Account",
            None,
            vec![row(
                div().child("Signed in as"),
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(div().text_color(c.secondary).child(if account.signed_in {
                        account.user.as_ref().map(|user| user.email.clone()).unwrap_or_default()
                    } else {
                        "Not signed in".into()
                    }))
                    .when(account.signed_in, |trailing| {
                        trailing.child(Button::new("sign-out", "Sign Out").on_click(move |_, _, cx| {
                            store.update(cx, |store, cx| {
                                store.sign_out(cx);
                                cx.notify();
                            })
                        }))
                    }),
            )],
            cx,
        )
    }

    fn appearance(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.store.read(cx).prefs.get::<Appearance>("appearance").unwrap_or_default();
        let store = self.store.clone();
        let picker = segmented(
            "appearance",
            Appearance::ALL.iter().map(|appearance| appearance.label()).collect(),
            Appearance::ALL.iter().position(|appearance| *appearance == chosen).unwrap_or(0),
            move |index, _, cx| {
                store.update(cx, |store, cx| {
                    store.prefs.set("appearance", Appearance::ALL[index]);
                    cx.notify();
                })
            },
            cx,
        );
        section("Appearance", None, vec![row(div().child("Theme"), picker)], cx)
    }

    /// The servers keep every image and video; the ones here only make threads open with them.
    fn storage(&self, cx: &mut Context<Self>) -> AnyElement {
        let storage = self.store.read(cx).media_storage;
        let description = match &storage {
            Some(storage) => {
                format!("{} of {} on this Mac. Your servers keep them all.", bytes(storage.used), bytes(storage.limit))
            }
            None => "Kept on this Mac so threads open with them".into(),
        };
        let store = self.store.clone();
        section(
            "Storage",
            None,
            vec![row(
                two_lines("Images and videos".into(), description, cx),
                Button::new("clear-media", "Clear").disabled(storage.map_or(0, |storage| storage.used) == 0).on_click(
                    move |_, _, cx| {
                        crate::media::MediaFiles::clear(cx);
                        store.update(cx, |store, _| store.clear_media());
                    },
                ),
            )],
            cx,
        )
    }

    fn servers(&self, cx: &mut Context<Self>) -> AnyElement {
        let servers = self.store.read(cx).servers.clone();
        let mut rows = Vec::new();
        for server in &servers {
            let (store, removed) = (self.store.clone(), server.clone());
            rows.push(row(
                two_lines(server.name.clone(), description(server), cx),
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(crate::sidebar::server_update_status(&self.store, server, cx, |_| div().into_any_element()))
                    .child(Button::new(SharedString::from(format!("remove-{}", server.id)), "Remove").on_click(
                        move |_, _, cx| {
                            store.update(cx, |store, cx| {
                                store.remove_server(&removed);
                                cx.notify();
                            })
                        },
                    )),
            ));
            rows.push(divider(cx));
        }
        let store = self.store.clone();
        rows.push(row(
            Button::new("add-server", "Add a Server…").on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.shows_add_server = true;
                    cx.notify();
                });
                activate_main_window(cx);
            }),
            div(),
        ));
        section("Servers", None, rows, cx)
    }

    /// The model that writes thread titles, branch names, commit messages and pull requests, and
    /// how it names branches, by server.
    fn text_generation(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let c = colors(cx);
        let servers: Vec<Server> =
            self.store.read(cx).servers.iter().filter(|server| writes_text(server)).cloned().collect();
        if servers.is_empty() {
            return None;
        }
        let mut rows = Vec::new();
        for (position, server) in servers.iter().enumerate() {
            let anchor = self.model_menus.entry(server.id.clone()).or_default().clone();
            let chosen = server
                .text_model
                .as_ref()
                .and_then(|id| server.models.iter().find(|model| &model.id == id).map(|model| model.name.clone()))
                .unwrap_or("Automatic".into());
            let (store, menu_server) = (self.store.clone(), server.clone());
            let menu_anchor = anchor.clone();
            rows.push(row(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(icons::symbol("server.rack", 13.).text_color(c.secondary))
                    .child(server.name.clone()),
                pop_up(SharedString::from(format!("model-{}", server.id)), chosen, &anchor, cx, move |window, cx| {
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
                    menu.show(menu_anchor.below(), window, cx);
                }),
            ));
            if let Some(editor) = self.branch_editors.get(&server.id) {
                rows.push(editor.clone().into_any_element());
            }
            if position + 1 < servers.len() {
                rows.push(divider(cx));
            }
        }
        Some(section(
            "Text generation",
            Some("The model that writes thread titles, branch names, commit messages and pull requests"),
            rows,
            cx,
        ))
    }

    fn projects(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let c = colors(cx);
        let projects = self.store.read(cx).projects.clone();
        let mut rows = Vec::new();
        for project in &projects {
            let anchor = self.icon_menus.entry(project.id.clone()).or_default().clone();
            let names_setup =
                self.store.read(cx).server(Some(&project.server_id)).map_or(0, |server| server.protocol_version) >= 6;
            let (store, chosen) = (self.store.clone(), project.clone());
            let menu_anchor = anchor.clone();
            let icon_menu = pull_down(
                SharedString::from(format!("icon-{}", project.id)),
                "Icon",
                &anchor,
                cx,
                move |window, cx| {
                    let (choose, reset) = (store.clone(), store.clone());
                    let (choose_project, reset_project) = (chosen.clone(), chosen.clone());
                    Menu::new()
                        .item("Choose an Image…", move |_, cx| {
                            choose.update(cx, |store, cx| {
                                store.icon_project = Some(choose_project.clone());
                                cx.notify();
                            });
                            activate_main_window(cx);
                        })
                        .item("Use the Icon in Its Folder", move |_, cx| {
                            reset.update(cx, |store, _| store.set_icon(&reset_project, None))
                        })
                        .show(menu_anchor.below(), window, cx);
                },
            );
            let (setup_project, removed) = (project.clone(), project.clone());
            let store = self.store.clone();
            rows.push(row(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .min_w_0()
                    .child(div().flex_shrink_0().child(logos::project_icon(Some(project), 26., cx)))
                    .child(
                        div().min_w_0().flex().flex_col().gap(px(2.)).child(project.name.clone()).child(
                            div()
                                .text_size(px(10.))
                                .text_color(c.secondary)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis_start()
                                .child(project.path.clone()),
                        ),
                    ),
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(icon_menu)
                    .when(names_setup, |trailing| {
                        trailing.child(
                            Button::new(SharedString::from(format!("setup-{}", project.id)), "Setup…")
                                .help(format!("The script that runs in each new worktree of {}", project.name))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let store = this.store.clone();
                                    let project = setup_project.clone();
                                    this.setup = Some(cx.new(|cx| SetupSheet::new(store, project, window, cx)));
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        Button::new(SharedString::from(format!("remove-project-{}", project.id)), "Remove").on_click(
                            move |_, _, cx| {
                                store.update(cx, |store, cx| {
                                    store.remove_project(&removed);
                                    cx.notify();
                                })
                            },
                        ),
                    ),
            ));
            rows.push(divider(cx));
        }
        let store = self.store.clone();
        let no_servers = self.store.read(cx).servers.is_empty();
        rows.push(row(
            Button::new("add-project", "Add a Project…").disabled(no_servers).on_click(move |_, _, cx| {
                activate_main_window(cx);
                store.update(cx, |store, cx| {
                    store.add_project();
                    cx.notify();
                });
            }),
            div(),
        ));
        section("Projects", None, rows, cx)
    }
}

/// A server that is connected and new enough to be told which model writes its text.
fn writes_text(server: &Server) -> bool {
    server.state == State::Connected && server.protocol_version >= 4
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

pub fn is_main_window(window: AnyWindowHandle, cx: &App) -> bool {
    window
        .downcast::<gpui_kit::base::Root>()
        .is_some_and(|root| root.read(cx).is_ok_and(|root| root.view().clone().downcast::<crate::root::Root>().is_ok()))
}

fn activate_main_window(cx: &mut App) {
    let main = cx.windows().into_iter().find(|window| is_main_window(*window, cx));
    if let Some(window) = main {
        let _ = window.update(cx, |_, window, _| window.activate_window());
    }
}

/// A titled box of rows.
fn section(title: &str, caption: Option<&str>, rows: Vec<AnyElement>, cx: &App) -> AnyElement {
    let c = colors(cx);
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .px(px(INSET))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(title.to_string()))
                .when_some(caption, |heading, caption| {
                    heading.child(div().text_size(px(10.)).text_color(c.secondary).child(caption.to_string()))
                }),
        )
        .child(div().flex().flex_col().rounded(px(10.)).bg(c.hover).children(rows))
        .into_any_element()
}

/// What the row is about on the left, its controls on the right.
fn row(leading: impl IntoElement, trailing: impl IntoElement) -> AnyElement {
    div()
        .min_h(px(44.))
        .px(px(INSET))
        .py(px(8.))
        .flex()
        .items_center()
        .gap(px(10.))
        .child(div().min_w_0().child(leading))
        .child(div().flex_1().min_w(px(12.)))
        .child(div().flex_shrink_0().child(trailing))
        .into_any_element()
}

fn divider(cx: &App) -> AnyElement {
    div().px(px(INSET)).child(crate::ui::divider(cx)).into_any_element()
}

fn two_lines(title: String, detail: String, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(title)
        .child(div().text_size(px(10.)).text_color(colors(cx).secondary).child(detail))
}

/// A segmented control: its choices side by side, the chosen one raised.
fn segmented(
    id: &str,
    choices: Vec<&'static str>,
    chosen: usize,
    on_choose: impl Fn(usize, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let c = colors(cx);
    let dark = is_dark(cx);
    let on_choose = std::rc::Rc::new(on_choose);
    div()
        .flex()
        .items_center()
        .h(px(22.))
        .p(px(1.))
        .rounded(px(6.))
        .bg(if dark { hsla(0., 0., 1., 0.08) } else { hsla(0., 0., 0., 0.06) })
        .children(choices.into_iter().enumerate().map(move |(index, choice)| {
            let on_choose = on_choose.clone();
            let selected = index == chosen;
            div()
                .id(SharedString::from(format!("{id}-{index}")))
                .h_full()
                .px(px(9.))
                .min_w(px(58.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .text_size(px(13.))
                .when(selected, |segment| {
                    segment.bg(if dark { hsla(0., 0., 1., 0.24) } else { white() }).shadow(crate::ui::shadow(
                        hsla(0., 0., 0., 0.12),
                        0.5,
                        1.5,
                    ))
                })
                .when(!selected && index + 1 != chosen && index + 1 < 3, |segment| {
                    segment.child(div().absolute().right_0().top(px(4.)).bottom(px(4.)).w(px(1.)).bg(c.border))
                })
                .relative()
                .child(choice)
                .on_click(move |_, window, cx| on_choose(index, window, cx))
        }))
}

/// A pop-up button: the chosen item with the arrows that say a menu opens to change it.
fn pop_up(
    id: SharedString,
    title: String,
    anchor: &Anchor,
    cx: &App,
    open: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    menu_button(id, title, "chevron.up.chevron.down", anchor, cx, open)
}

/// A pull-down button: its title, and the menu of what it does.
fn pull_down(
    id: SharedString,
    title: &str,
    anchor: &Anchor,
    cx: &App,
    open: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    menu_button(id, title.to_string(), "chevron.down", anchor, cx, open)
}

fn menu_button(
    id: SharedString,
    title: String,
    symbol: &'static str,
    anchor: &Anchor,
    cx: &App,
    open: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let c = colors(cx);
    let dark = is_dark(cx);
    div()
        .id(id)
        .relative()
        .h(px(22.))
        .min_w(px(if symbol == "chevron.down" { 0. } else { 145. }))
        .pl(px(9.))
        .pr(px(3.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(6.))
        .bg(if dark { hsla(0., 0., 1., 0.1) } else { white() })
        .border_1()
        .border_color(if dark { c.border } else { c.strong_border })
        .shadow(crate::ui::shadow(hsla(0., 0., 0., if dark { 0.25 } else { 0.08 }), 0.5, 1.))
        .text_size(px(13.))
        .child(anchor.track())
        .child(div().flex_1().child(title))
        .child(
            div()
                .size(px(16.))
                .rounded(px(4.))
                .bg(c.accent)
                .flex()
                .items_center()
                .justify_center()
                .child(icons::symbol(symbol, 10.).text_color(white())),
        )
        .on_click(move |_, window, cx| open(window, cx))
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let signed_in = self.store.read(cx).account.signed_in;
        let mut sections = vec![self.account(cx), self.appearance(cx), self.storage(cx)];
        if signed_in {
            sections.push(self.servers(cx));
            sections.extend(self.text_generation(cx));
            sections.push(self.projects(cx));
        }
        let background = if is_dark(cx) { rgb(0x2b2b2d) } else { rgb(0xededed) };
        div()
            .id("settings")
            .track_focus(&self.focus)
            .on_action(|_: &crate::app_menu::Close, window, _| window.remove_window())
            .size_full()
            .bg(background)
            .text_color(c.text)
            .text_size(px(13.))
            .line_height(relative(1.21))
            .font_family(crate::theme::SYSTEM_FONT)
            .child(
                div()
                    .id("settings-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .child(div().p(px(20.)).flex().flex_col().gap(px(22.)).children(sections)),
            )
            .children(self.setup.clone())
    }
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
        let c = colors(cx);
        let changed = self.changed(cx);
        let unchanged = self.server.branch_instructions == self.server.default_branch_instructions && !changed;
        div()
            .px(px(INSET))
            .pb(px(10.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().text_size(px(10.)).text_color(c.secondary).child("Branch names"))
                    .child(div().flex_1())
                    .child(Button::new("branch-reset", "Reset").small().disabled(unchanged).on_click(cx.listener(
                        |this, _, window, cx| {
                            let default = this.server.default_branch_instructions.clone();
                            this.text.update(cx, |text, cx| text.set_value(default, window, cx));
                            let id = this.server.id.clone();
                            this.store.update(cx, |store, _| store.set_branch_instructions(None, &id));
                        },
                    )))
                    .child(Button::new("branch-save", "Save").small().disabled(!changed).on_click(cx.listener(
                        |this, _, _, cx| {
                            let (text, id) = (this.text.read(cx).value().to_string(), this.server.id.clone());
                            this.store.update(cx, |store, _| store.set_branch_instructions(Some(text), &id));
                        },
                    ))),
            )
            .child(
                div().h(px(64.)).p(px(6.)).bg(c.composer).rounded(px(7.)).border_1().border_color(c.border).child(
                    Textarea::new(&self.text).appearance(false).xsmall().text_size(px(12.)).px_0().py_0().h_full(),
                ),
            )
    }
}

/// The shell script that runs in each new worktree of a project before the agent starts there.
pub struct SetupSheet {
    store: Entity<Store>,
    project: Project,
    script: Entity<TextareaState>,
}

impl SetupSheet {
    fn new(store: Entity<Store>, project: Project, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let script =
            cx.new(|cx| TextareaState::new(window, cx).default_value(project.setup.clone().unwrap_or_default()));
        script.update(cx, |script, cx| script.focus(window, cx));
        Self { store, project, script }
    }
}

fn close_setup(window: &mut Window, cx: &mut App) {
    let Some(root) = window.root::<gpui_kit::base::Root>().flatten() else { return };
    let Ok(settings) = root.read(cx).view().clone().downcast::<SettingsView>() else { return };
    settings.update(cx, |settings, cx| {
        settings.setup = None;
        cx.notify();
    });
}

impl Render for SetupSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let content = div()
            .id("setup-sheet")
            .on_key_down(|event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    close_setup(window, cx);
                }
            })
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
                div()
                    .h(px(140.))
                    .p(px(6.))
                    .bg(c.composer)
                    .rounded(px(7.))
                    .border_1()
                    .border_color(c.border)
                    .font_family(crate::theme::MONO_FONT)
                    .child(
                        Textarea::new(&self.script)
                            .appearance(false)
                            .xsmall()
                            .text_size(px(12.))
                            .font_family(crate::theme::MONO_FONT)
                            .px_0()
                            .py_0()
                            .h_full(),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(div().flex_1())
                    .child(Button::new("setup-cancel", "Cancel").on_click(|_, window, cx| close_setup(window, cx)))
                    .child(Button::new("setup-save", "Save").prominent().on_click(cx.listener(|this, _, window, cx| {
                        let script = this.script.read(cx).value().to_string();
                        let project = this.project.clone();
                        this.store.update(cx, |store, _| store.set_setup(&project, script));
                        close_setup(window, cx);
                    }))),
            );
        sheet("setup", 440., content, cx)
    }
}
