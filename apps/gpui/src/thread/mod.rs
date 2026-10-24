//! The right side of the window: the open thread with the composer over its end, or the start
//! of a new one.

mod notice;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::composer::Composer;
use crate::composer::children::agent_bar;
use crate::models::{AgentViewExt, Project};
use crate::store::{PanelPage, Store};
use crate::theme::{self, ControlSize, Radius, colors};
use crate::ui::menu::{Anchor, Menu, MenuIcon};
use crate::ui::modal::{Answer, Confirmation, Modal, confirmation, present_sheet, presented};
use crate::ui::{ActionButton, Highlight, TOOLBAR_WIDTH, icons, logos};

/// The room between the last row and the composer, which the transcript fades out in.
pub const COMPOSER_GAP: f32 = 80.;
const COMPOSER_BOTTOM_GAP: f32 = 16.;
/// How far above the middle the start is, as a part of the window's height.
const START_LIFT: f32 = 0.03;
const HEADLINE_SIZE: f32 = 28.;
const NOTICE_FADE: Duration = Duration::from_millis(150);

/// How far the title starts from the pane's side beside the sidebar.
pub const BESIDE_SIDEBAR_INSET: f32 = 20.;

pub struct ThreadPane {
    store: Entity<Store>,
    composer: Entity<Composer>,
    transcript: Entity<crate::transcript::view::TranscriptView>,
    /// The transcript of an agent the thread's agent started, shown in the thread's place.
    agent_transcript: Entity<crate::transcript::view::TranscriptView>,
    /// How far the title starts from the pane's left edge: past the window's buttons when the
    /// sidebar is hidden.
    pub title_inset: f32,
    /// The right panel is beside the pane, so the window's last button isn't over it.
    pub beside_panel: bool,
    /// What the window's buttons take of the top bar's end, where the app draws them.
    pub trailing_room: f32,
    /// How much of the pane's right side something lies over, which takes no files dropped.
    pub covered: f32,
    /// Files from outside are held over the pane.
    drop_over: bool,
    /// How tall the composer was last drawn, which the transcript leaves room for.
    composer_height: Rc<Cell<f32>>,
    project_menu: Anchor,
    git_menu: Anchor,
    commit_sheet: Option<Entity<crate::git::CommitSheet>>,
    /// The title the window was last given.
    window_title: String,
    _ticks: Task<()>,
}

impl ThreadPane {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe_in(&store, window, |this, store, window, cx| {
            this.follow_commit_sheet(&store, window, cx);
            cx.notify();
        })
        .detach();
        let composer = cx.new(|cx| Composer::new(store.clone(), window, cx));
        let model = store.read(cx).transcript.clone();
        let transcript = cx.new(|cx| crate::transcript::view::TranscriptView::new(store.clone(), model, false, cx));
        let model = store.read(cx).agent_transcript.clone();
        let agent_transcript =
            cx.new(|cx| crate::transcript::view::TranscriptView::new(store.clone(), model, true, cx));
        // The time of the agent shown goes on while it works.
        let ticks = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let Ok(working) = this
                    .read_with(cx, |this, cx| this.store.read(cx).shown_agent().is_some_and(|agent| agent.working()))
                else {
                    break;
                };
                if working && this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        Self {
            store,
            composer,
            transcript,
            agent_transcript,
            title_inset: BESIDE_SIDEBAR_INSET,
            trailing_room: 0.,
            beside_panel: false,
            covered: 0.,
            drop_over: false,
            composer_height: Rc::new(Cell::new(120.)),
            project_menu: Anchor::default(),
            git_menu: Anchor::default(),
            commit_sheet: None,
            window_title: String::new(),
            _ticks: ticks,
        }
    }

    /// The commit sheet is up while the store says which project commits.
    fn follow_commit_sheet(&mut self, store: &Entity<Store>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = store.read(cx).committing_project.clone() else {
            self.commit_sheet = None;
            return;
        };
        if self.commit_sheet.is_none() {
            self.commit_sheet = Some(cx.new(|cx| crate::git::CommitSheet::new(store.clone(), project, window, cx)));
        }
    }

    /// How wide the pane is, which decides how much the composer's controls say.
    pub fn set_width(&mut self, width: f32, cx: &mut Context<Self>) {
        let composer = (width - 2. * theme::COMPOSER_PADDING).min(theme::COMPOSER_WIDTH);
        self.composer.update(cx, |view, cx| {
            if (view.width - composer).abs() > 0.5 {
                view.width = composer;
                cx.notify();
            }
        });
    }

    /// The thread's project and name and its git button, drawn in the window's top bar over
    /// this pane. Without a git button, a folder that is no repository can be made one, and a draft
    /// whose worktree would start from the remote's base can pull it.
    fn top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let project = store.composer_project();
        let parts = store.composer_project_line();
        let title = self.title(cx);
        let git_project = store.git_project();
        let control: Option<AnyElement> = if let Some(button) = crate::git::git_button(&self.store, &self.git_menu, cx)
        {
            Some(button)
        } else if let Some(project) = git_project.filter(|project| store.can_initialize_git(project)) {
            Some(crate::git::initialize_git_button(&self.store, &project, cx).into_any_element())
        } else {
            store
                .draft_base_to_pull()
                .map(|base| crate::git::pull_base_button(&self.store, &base, cx).into_any_element())
        };
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(theme::TOP_BAR))
            .pl(px(self.title_inset))
            // Room for the button that shows the right panel, which is at the window's edge.
            .pr(px(if self.beside_panel { 4. } else { TOOLBAR_WIDTH + 12. + self.trailing_room }))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(ControlSize::Regular.padding()))
                    .children(self.back_button(cx))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .min_w_0()
                            .when_some(parts, |title, parts| title.child(project_line(project.as_ref(), &parts, cx)))
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(c.foreground)
                                    .truncate()
                                    .child(title),
                            ),
                    ),
            )
            .children(control)
    }

    /// Before the title, the button that opens the thread whose agent started the open one. Its
    /// light is as far from the pane's side as from the bar's top, beside the sidebar.
    fn back_button(&self, cx: &App) -> Option<Div> {
        let store = self.store.read(cx);
        if store.shown_agent().is_some() {
            return None;
        }
        let parent = store.parent_thread()?;
        let id = parent.id.clone();
        let handle = self.store.clone();
        let button = ActionButton::icon("parent-thread", "corner-left-up", format!("Open {}", parent.title))
            .large()
            .on_click(move |_, _, cx| {
                handle.update(cx, |store, cx| {
                    store.open_started_thread(&id, cx);
                    cx.notify();
                })
            });
        let size = ControlSize::Large;
        let light = (size.height() - size.symbol_side()) / 2.;
        Some(
            div()
                .flex_none()
                .ml(px((theme::TOP_BAR - size.height()) / 2. - BESIDE_SIDEBAR_INSET))
                .mr(px(-light))
                .child(button),
        )
    }

    /// The thread's name, or the agent's when one is shown in its place.
    fn title(&self, cx: &App) -> String {
        let store = self.store.read(cx);
        if let Some(agent) = store.shown_agent() {
            return agent.title.clone();
        }
        store.selected_thread().map_or_else(|| "New thread".into(), |thread| thread.title.clone())
    }

    /// The empty state of a new thread: a question, and the composer a little above the middle
    /// of the window.
    fn start(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lift = f32::from(crate::window_frame::content_size(window).height) * START_LIFT * 2.;
        div()
            .size_full()
            .pb(px(lift))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(self.start_block(window, cx))
    }

    fn start_block(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let no_projects = store.recent_projects().is_empty() && store.live_no_projects().next().is_none();
        let connected = store.servers.iter().any(|server| server.connected());
        let add = self.store.clone();
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(26.))
            .when(no_projects, |start| {
                start.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(10.))
                        .child(div().text_size(px(28.)).child("Add a project to start"))
                        .child(
                            div()
                                .text_size(px(14.))
                                .text_color(c.muted_foreground)
                                .child("A project is a folder on your server that threads work in."),
                        )
                        .child(
                            div().pt(px(8.)).child(
                                ActionButton::new("add-first-project", "Add Project")
                                    .symbol("folder-plus")
                                    .primary()
                                    .large()
                                    .disabled(!connected)
                                    .on_click(move |_, _, cx| {
                                        add.update(cx, |store, cx| {
                                            store.add_project();
                                            cx.notify();
                                        })
                                    }),
                            ),
                        ),
                )
            })
            .when(!no_projects, |start| {
                start.child(self.headline(window, cx)).child(
                    div().w_full().px(px(theme::COMPOSER_PADDING)).flex().justify_center().child(self.composer.clone()),
                )
            })
    }

    fn headline(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The room between the words is a space of the headline's type.
        let word_space = crate::ui::text_width(" ", HEADLINE_SIZE, FontWeight::NORMAL, window);
        let c = colors(cx);
        let store = self.store.read(cx);
        let selected = store.project(store.selected_draft().and_then(|draft| draft.project_id.as_deref())).cloned();
        let anchor = self.project_menu.clone();
        let handle = self.store.clone();
        let several_servers = store.live_servers().count() > 1;
        let (lead, name) = Project::headline(selected.as_ref());
        let projects: Vec<(String, String, Option<String>, &'static str)> = store
            .recent_projects()
            .into_iter()
            .chain(store.live_no_projects())
            .map(|project| {
                let name = if several_servers {
                    let server = store
                        .server(Some(&project.server_id))
                        .map(|server| server.short_name.clone())
                        .unwrap_or_default();
                    format!("{} · {server}", project.name)
                } else {
                    project.name.clone()
                };
                (project.id.clone(), name, project.icon_path.clone(), project.symbol())
            })
            .collect();
        div()
            .flex()
            .items_center()
            .gap(px(word_space))
            .text_size(px(HEADLINE_SIZE))
            .text_color(c.foreground)
            .child(lead)
            .child(
                div()
                    .id("headline-project")
                    .group("headline-project")
                    .relative()
                    .rounded(px(Radius::LG))
                    .border_1()
                    .border_color(c.border)
                    .child(Highlight::new().radius(Radius::LG).render("headline-project", cx))
                    .child(
                        div()
                            .relative()
                            .pl(px(9.))
                            .pr(px(11.))
                            .py(px(3.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(anchor.track())
                            .child(logos::project_icon(selected.as_ref(), 22., cx))
                            .child(name)
                            .child(icons::symbol("chevron-down", 13.).text_color(c.muted_stronger_foreground)),
                    )
                    .on_click(move |_, window, cx| {
                        let mut menu = Menu::new();
                        for (id, name, icon, symbol) in &projects {
                            let (store, id) = (handle.clone(), id.clone());
                            let icon = match icon {
                                Some(path) => MenuIcon::File(path.clone().into()),
                                None => MenuIcon::Symbol(symbol),
                            };
                            menu = menu.icon_item(name.clone(), icon, true, None, move |_, cx| {
                                store.update(cx, |store, cx| {
                                    store.set_new_thread_project(Some(id.clone()), cx);
                                    cx.notify();
                                })
                            });
                        }
                        let add = handle.clone();
                        menu = menu.separator().item("Add Project", move |_, cx| {
                            add.update(cx, |store, cx| {
                                store.add_project();
                                cx.notify();
                            })
                        });
                        if let Some(project) = selected.clone().filter(|project| !project.no_project) {
                            let (choose, remove) = (handle.clone(), handle.clone());
                            let (chosen, removed) = (project.clone(), project.clone());
                            menu = menu
                                .item(format!("Choose an Icon for “{}”", project.name), move |_, cx| {
                                    choose.update(cx, |store, cx| {
                                        store.open_panel(PanelPage::Icon(chosen.id.clone()));
                                        cx.notify();
                                    })
                                })
                                .item(format!("Remove “{}” from Projects", project.name), move |_, cx| {
                                    remove.update(cx, |store, cx| {
                                        store.removing_project = Some(removed.clone());
                                        cx.notify();
                                    })
                                });
                        }
                        menu.show_under(&anchor, 4., window, cx);
                    }),
            )
    }

    /// Asks before a project leaves the clients, and lets it go only when the user says so.
    fn removal(&self, project: &Project, cx: &mut Context<Self>) -> Modal {
        let store = self.store.read(cx);
        let server =
            store.server(Some(&project.server_id)).map_or("its server".to_string(), |server| server.name.clone());
        let message =
            format!("{} leaves your clients. Its folder stays on {server} with everything in it.", project.name);
        let (remove, cancel, removed) = (self.store.clone(), self.store.clone(), project.clone());
        let asked = Confirmation::new(format!("Remove “{}”?", project.name)).message(message).answer(
            Answer::new("Remove", move |_, cx| {
                remove.update(cx, |store, cx| {
                    store.remove_project(&removed);
                    cx.notify();
                })
            })
            .danger(),
        );
        confirmation(
            "remove-project",
            asked,
            move |_, cx| {
                cancel.update(cx, |store, cx| {
                    store.removing_project = None;
                    cx.notify();
                })
            },
            cx,
        )
    }

    /// Covers the thread while files are held over it, to say they can be dropped. It fades in
    /// and out.
    fn drop_cover(&self, shown: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        div()
            .absolute()
            .top(px(theme::TOP_BAR))
            .left_0()
            .right_0()
            .bottom_0()
            .opacity(shown)
            .bg(c.background)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .absolute()
                    .inset(px(8.))
                    .rounded(px(Radius::XL))
                    .border(px(1.5))
                    .border_dashed()
                    .border_color(c.primary),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .text_color(c.foreground)
                    .child(icons::symbol("paperclip", 28.))
                    .child(div().text_size(px(17.)).font_weight(FontWeight::SEMIBOLD).child("Drop files here")),
            )
    }
}

impl Render for ThreadPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let is_start =
            store.selected_draft().is_some_and(|draft| store.transcript_is_empty && !store.is_sending(&draft.id));
        let pending_git = store.pending_git.clone();
        let notice = store.open_notice();
        let removing = store.removing_project.clone();
        let title = store.selected_thread().map(|thread| thread.title.clone()).unwrap_or("New thread".into());
        let removed = store.open_thread_removed();
        let shown_agent = store.shown_agent().cloned().filter(|_| !is_start);
        let takes_drops = store.composer_server().is_some() && !removed;
        if self.window_title != title {
            window.set_window_title(&title);
            self.window_title = title;
        }
        let pending_git = presented("git-confirm", pending_git, window, cx);
        let removing = presented("remove-project", removing, window, cx);
        let commit_sheet = present_sheet(
            "commit-sheet",
            self.commit_sheet.clone(),
            |sheet, leaving| sheet.leaving = leaving,
            window,
            cx,
        );
        if !cx.has_active_drag() {
            self.drop_over = false;
        }
        let held_over = takes_drops && self.drop_over;
        let cover = crate::ui::motion::eased(
            "drop-cover",
            if held_over { 1. } else { 0. },
            crate::ui::motion::QUICK,
            window,
            cx,
        );
        // The notice slides and fades in and out, and changes in place when another follows it.
        let notice = crate::ui::motion::lingering("notice", notice, NOTICE_FADE, window, cx);
        let notice_shown = crate::ui::motion::eased_from(
            "notice-shown",
            0.,
            if notice.as_ref().is_some_and(|(_, going)| !going) { 1. } else { 0. },
            NOTICE_FADE,
            window,
            cx,
        );
        let measured = self.composer_height.clone();
        // A thread whose server was unlinked ends with the row that says so, in the composer's place.
        let room_below = shown_agent.is_some() || !removed;
        let inset = if room_below { self.composer_height.get() } else { COMPOSER_BOTTOM_GAP };
        let transcript = if shown_agent.is_some() { self.agent_transcript.clone() } else { self.transcript.clone() };
        transcript.update(cx, |transcript, cx| transcript.set_bottom_inset(inset, cx));
        let below: AnyElement = match &shown_agent {
            Some(agent) => agent_bar(&self.store, agent, cx).into_any_element(),
            None => self.composer.clone().into_any_element(),
        };
        div()
            .id("thread-pane")
            .size_full()
            .relative()
            .child(if is_start {
                self.start(window, cx).into_any_element()
            } else {
                div()
                    .size_full()
                    .relative()
                    .child(div().absolute().top(px(theme::TOP_BAR)).left_0().right_0().bottom_0().child(transcript))
                    .when(room_below, |thread| {
                        thread.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .px(px(theme::COMPOSER_PADDING))
                                .pt(px(COMPOSER_GAP))
                                .pb(px(COMPOSER_BOTTOM_GAP))
                                .flex()
                                .justify_center()
                                .child(
                                    canvas(
                                        move |bounds, _, _| measured.set(f32::from(bounds.size.height)),
                                        |_, _, _, _| {},
                                    )
                                    .absolute()
                                    .inset_0(),
                                )
                                .child(below),
                        )
                    })
                    .into_any_element()
            })
            .child(self.top_bar(cx))
            .when_some(notice, |pane, (notice, _)| {
                pane.child(
                    div()
                        .absolute()
                        .top(px(theme::TOP_BAR + 6. - 6. * (1. - notice_shown)))
                        .right(px(14.))
                        .opacity(notice_shown)
                        .child(notice::notice_view(&self.store, &notice, cx)),
                )
            })
            .when(cover > 0., |pane| pane.child(self.drop_cover(cover, cx)))
            .on_drag_move(cx.listener(|this, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                let at = event.event.position;
                let inside = event.bounds.contains(&at) && at.x < event.bounds.right() - px(this.covered);
                if inside != this.drop_over {
                    this.drop_over = inside;
                    cx.notify();
                }
            }))
            .on_drop(cx.listener(move |this, paths: &ExternalPaths, _, cx| {
                let held_over = std::mem::take(&mut this.drop_over);
                if !held_over || !takes_drops {
                    return;
                }
                let files = paths.paths().to_vec();
                this.store.update(cx, |store, cx| {
                    store.attach(files);
                    cx.notify();
                });
            }))
            .children(commit_sheet)
            .when_some(pending_git, |pane, (pending, leaving)| {
                pane.child(crate::git::confirm_alert(&self.store, &pending, cx).leaving(leaving))
            })
            .when_some(removing, |pane, (project, leaving)| pane.child(self.removal(&project, cx).leaving(leaving)))
    }
}

/// The line over a thread's title: its project's icon, then its parts with muted dots between.
fn project_line(project: Option<&Project>, parts: &[String], cx: &App) -> impl IntoElement {
    let c = colors(cx);
    div().min_w_0().flex().items_center().gap(px(4.)).child(logos::project_icon(project, 12., cx)).child(
        div()
            .min_w_0()
            .text_size(px(11.))
            .text_color(c.muted_foreground)
            .truncate()
            .child(crate::ui::control::dotted(parts.join(" · "), c)),
    )
}
