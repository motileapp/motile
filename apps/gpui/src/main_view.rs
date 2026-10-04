//! The sidebar and the open thread, side by side on the window's one surface, and the side
//! panel beside the thread or over it. The window's top bar is the top of each of them, with
//! the window's buttons and the buttons that show and hide the sidebar and the panel.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::panel::state::WIDTHS as PANEL_WIDTHS;
use crate::panel::view::SidePanelView;
use crate::sidebar::Sidebar;
use crate::store::Store;
use crate::theme::{self, colors};
use crate::thread::ThreadPane;
use crate::ui::{TOOLBAR_MARGIN, TOOLBAR_WIDTH, toolbar_button};

const SIDEBAR_WIDTHS: (f32, f32) = (240., 420.);
/// What the sidebar always leaves to the thread, however wide it was dragged.
const THREAD_MIN_WIDTH: f32 = 500.;
/// What the side panel leaves to the thread. Without that much room, it lies over the thread.
const THREAD_BESIDE_PANEL: f32 = 400.;
/// How far the top bar's content starts from the window's left edge while the sidebar is
/// hidden: past the window's buttons and the ones beside them.
const PAST_WINDOW_BUTTONS: f32 = 240.;
/// Where the button that hides the sidebar starts, past the window's buttons.
const NAVIGATION_LEFT: f32 = 90.;
pub const ROW_INSET: f32 = 10.;

#[derive(Clone, Copy, PartialEq)]
enum Resizing {
    Sidebar { start_x: f32, start_width: f32 },
    Panel { start_x: f32, start_width: f32 },
}

pub struct MainView {
    store: Entity<Store>,
    sidebar: Entity<Sidebar>,
    thread: Entity<ThreadPane>,
    panel: Entity<SidePanelView>,
    resizing: Option<Resizing>,
    /// A press in the top bar that moves the window once it drags.
    moving: std::rc::Rc<std::cell::Cell<bool>>,
    _subscriptions: Vec<Subscription>,
}

impl MainView {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sidebar = cx.new(|cx| Sidebar::new(store.clone(), window, cx));
        let thread = cx.new(|cx| ThreadPane::new(store.clone(), window, cx));
        let panel = cx.new(|cx| SidePanelView::new(store.clone(), window, cx));
        let subscriptions = vec![cx.observe(&store, |_, _, cx| cx.notify())];
        Self {
            store,
            sidebar,
            thread,
            panel,
            resizing: None,
            moving: Default::default(),
            _subscriptions: subscriptions,
        }
    }

    fn sidebar_hidden(&self, cx: &App) -> bool {
        self.store.read(cx).prefs.bool("sidebar.hidden")
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        let hidden = !self.sidebar_hidden(cx);
        self.store.update(cx, |store, cx| {
            store.prefs.set("sidebar.hidden", hidden);
            cx.notify();
        });
    }

    fn project_buttons(&self, _: &mut Context<Self>) -> Div {
        let new_project = self.store.clone();
        let new_thread = self.store.clone();
        div()
            .flex()
            .child(toolbar_button("add-project", "folder.badge.plus", "Add a project").on_click(move |_, _, cx| {
                new_project.update(cx, |store, cx| {
                    store.add_project();
                    cx.notify();
                })
            }))
            .child(
                toolbar_button(
                    "new-thread",
                    "square.and.pencil",
                    "New thread (⌘N). ⇧-click starts one in this project",
                )
                .on_click(move |event, _, cx| {
                    let shift = event.modifiers().shift;
                    new_thread.update(cx, |store, cx| {
                        if shift {
                            let project = store.composer_project().map(|project| project.id);
                            store.start_new_thread(project, cx);
                        } else {
                            store.new_thread(cx);
                        }
                        cx.notify();
                    })
                }),
            )
    }

    fn resize(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(resizing) = self.resizing else { return };
        let (key, width, bounds) = match resizing {
            Resizing::Sidebar { start_x, start_width } => ("sidebar.width", start_width + x - start_x, SIDEBAR_WIDTHS),
            Resizing::Panel { start_x, start_width } => ("panel.width", start_width - (x - start_x), PANEL_WIDTHS),
        };
        let width = width.clamp(bounds.0, bounds.1);
        self.store.update(cx, |store, cx| {
            store.prefs.set(key, width);
            cx.notify();
        });
    }

    /// The line between the thread and what is beside it. Dragging it makes the sidebar or the
    /// side panel wider or narrower.
    fn pane_divider(&self, id: &'static str, start: impl Fn(f32) -> Resizing + 'static, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        div().relative().w(px(1.)).h_full().flex_shrink_0().bg(c.border).child(
            div()
                .id(id)
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(-(theme::RESIZE_GRAB - 1.) / 2.))
                .w(px(theme::RESIZE_GRAB))
                .cursor(CursorStyle::ResizeLeftRight)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        this.resizing = Some(start(f32::from(event.position.x)));
                        cx.stop_propagation();
                    }),
                ),
        )
    }
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let window_width = f32::from(window.viewport_size().width);
        let store = self.store.read(cx);
        let hidden = store.prefs.bool("sidebar.hidden");
        let sidebar_width = store.prefs.f32_or("sidebar.width", 280.);
        let panel_width = store.prefs.f32_or("panel.width", 460.);
        let panel_open = store.side_panel.is_open;
        let maximized = store.panel_maximized();

        let widest = SIDEBAR_WIDTHS.1.min(window_width - 1. - THREAD_MIN_WIDTH);
        let widths = (SIDEBAR_WIDTHS.0, SIDEBAR_WIDTHS.0.max(widest));
        let shown_width = sidebar_width.clamp(widths.0, widths.1);
        let rest = window_width - if hidden { 0. } else { shown_width + 1. };
        let fits = panel_open && rest - 1. - THREAD_BESIDE_PANEL >= PANEL_WIDTHS.0;
        let beside = fits && !maximized;
        let panel_widths =
            (PANEL_WIDTHS.0, PANEL_WIDTHS.0.max(if fits { rest - 1. - THREAD_BESIDE_PANEL } else { rest - 1. }));
        let shown_panel = panel_width.clamp(panel_widths.0, panel_widths.1);

        let thread_width = if beside || (maximized && fits) { rest - 1. - shown_panel } else { rest };
        self.thread.update(cx, |thread, cx| {
            thread.title_inset = if hidden { PAST_WINDOW_BUTTONS } else { 20. };
            thread.beside_panel = beside;
            thread.set_width(thread_width, cx);
        });
        self.panel.update(cx, |panel, _| panel.tab_inset = if maximized && hidden { PAST_WINDOW_BUTTONS } else { 0. });

        let toggle_sidebar = cx.listener(|this, _, _, cx| this.toggle_sidebar(cx));
        let store_handle = self.store.clone();
        let maximize = self.store.clone();
        let side = self.store.clone();
        let panel = self.panel.clone();

        let thread_width = if maximized && fits { Some(rest - 1. - shown_panel) } else { None };
        let thread = div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                div()
                    .h_full()
                    .when_some(thread_width, |pane, width| pane.w(px(width)))
                    .when(thread_width.is_none(), |pane| pane.w_full())
                    .when(maximized, |pane| pane.invisible())
                    .child(self.thread.clone()),
            )
            .when(maximized, |area| area.child(div().absolute().inset_0().child(panel.clone())))
            .when(panel_open && !beside && !maximized, |area| {
                area.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right_0()
                        .flex()
                        .bg(c.background)
                        .shadow(crate::ui::shadow(hsla(0., 0., 0., 0.18), 0., 28.))
                        .child(self.pane_divider(
                            "panel-divider-over",
                            move |x| Resizing::Panel { start_x: x, start_width: shown_panel },
                            cx,
                        ))
                        .child(div().w(px(shown_panel)).h_full().child(panel.clone())),
                )
            });

        div()
            .id("main")
            .group("main-drop")
            .size_full()
            .relative()
            .flex()
            .child(crate::ui::window_drag_strip("top-bar", theme::TOP_BAR, &self.moving))
            .when(self.resizing.is_some(), |main| {
                main.cursor(CursorStyle::ResizeLeftRight)
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        if event.pressed_button != Some(MouseButton::Left) {
                            this.resizing = None;
                            return cx.notify();
                        }
                        this.resize(f32::from(event.position.x), cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.resizing = None;
                            cx.notify();
                        }),
                    )
            })
            .when(!hidden, |main| {
                main.child(
                    div().relative().w(px(shown_width)).h_full().flex_shrink_0().child(self.sidebar.clone()).child(
                        div()
                            .absolute()
                            .top_0()
                            .right(px(ROW_INSET - TOOLBAR_MARGIN))
                            .h(px(theme::TOP_BAR))
                            .flex()
                            .items_center()
                            .child(self.project_buttons(cx)),
                    ),
                )
                .child(self.pane_divider(
                    "sidebar-divider",
                    move |x| Resizing::Sidebar { start_x: x, start_width: shown_width },
                    cx,
                ))
            })
            .child(thread)
            .when(beside, |main| {
                main.child(self.pane_divider(
                    "panel-divider",
                    move |x| Resizing::Panel { start_x: x, start_width: shown_panel },
                    cx,
                ))
                .child(div().w(px(shown_panel)).h_full().flex_shrink_0().child(panel.clone()))
            })
            // The buttons at the start of the top bar, past the window's own.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(NAVIGATION_LEFT))
                    .h(px(theme::TOP_BAR))
                    .w(px(3. * TOOLBAR_WIDTH))
                    .flex()
                    .items_center()
                    .child(
                        toolbar_button(
                            "toggle-sidebar",
                            "sidebar.left",
                            if hidden { "Show the sidebar (⌃⌘S)" } else { "Hide the sidebar (⌃⌘S)" },
                        )
                        .on_click(toggle_sidebar),
                    )
                    .when(hidden, |bar| bar.child(self.project_buttons(cx))),
            )
            // The buttons at the end of the top bar.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right(px(12.))
                    .h(px(theme::TOP_BAR))
                    .flex()
                    .items_center()
                    .when(panel_open, |bar| {
                        bar.child(
                            toolbar_button(
                                "maximize-panel",
                                if maximized {
                                    "arrow.down.right.and.arrow.up.left"
                                } else {
                                    "arrow.up.left.and.arrow.down.right"
                                },
                                if maximized {
                                    "Restore the side panel (⇧⌥⌘B)"
                                } else {
                                    "Maximize the side panel (⇧⌥⌘B)"
                                },
                            )
                            .on_click(move |_, _, cx| {
                                maximize.update(cx, |store, cx| {
                                    store.toggle_panel_maximized();
                                    cx.notify();
                                })
                            }),
                        )
                    })
                    .child(
                        toolbar_button(
                            "toggle-panel",
                            "sidebar.right",
                            if panel_open { "Hide the side panel (⌥⌘B)" } else { "Show the side panel (⌥⌘B)" },
                        )
                        .on_click(move |_, _, cx| {
                            side.update(cx, |store, cx| {
                                let open = !store.side_panel.is_open;
                                store.set_panel_open(open);
                                cx.notify();
                            })
                        }),
                    ),
            )
            .on_drop(move |paths: &ExternalPaths, _, cx| {
                let files = paths.paths().to_vec();
                store_handle.update(cx, |store, cx| {
                    store.drop_targeted = false;
                    store.attach(files);
                    cx.notify();
                });
            })
    }
}
