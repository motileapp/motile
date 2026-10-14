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
use crate::ui::{ActionButton, TOOLBAR_MARGIN, TOOLBAR_WIDTH, even};
use crate::usage::UsageRoute;

const SIDEBAR_WIDTHS: (f32, f32) = (240., 420.);
/// What the sidebar always leaves to the thread, however wide it was dragged.
const THREAD_MIN_WIDTH: f32 = 500.;
/// The sidebar's width when it hasn't been dragged.
pub const SIDEBAR_WIDTH: f32 = 280.;
/// The line between a pane and what is beside it is grabbed this far into each.
pub const THREAD_REACH: f32 = 8.;
pub const PANE_REACH: f32 = 4.;

/// How narrow and how wide the sidebar may be in a window this wide. The settings' sections are
/// as wide.
pub fn sidebar_widths(window_width: f32) -> (f32, f32) {
    let widest = SIDEBAR_WIDTHS.1.min(window_width - 1. - THREAD_MIN_WIDTH);
    (SIDEBAR_WIDTHS.0, SIDEBAR_WIDTHS.0.max(widest))
}

/// The sidebar's width as the user dragged it, within what the window allows.
pub fn shown_sidebar_width(store: &Store, window_width: f32) -> f32 {
    let widths = sidebar_widths(window_width);
    store.prefs.f32_or("sidebar.width", SIDEBAR_WIDTH).clamp(widths.0, widths.1)
}
/// What the right panel leaves to the thread. Without that much room, it lies over the thread.
const THREAD_BESIDE_PANEL: f32 = 400.;
/// How far the top bar's content starts past the sidebar's button while the sidebar is hidden.
const PAST_SIDEBAR_BUTTON: f32 = 46.;
const PANEL_BUTTONS_INSET: f32 = 10.;

/// A drag of the line beside the sidebar or the right panel: where it started, the width then,
/// and how narrow and how wide the window lets it be.
#[derive(Clone, Copy, PartialEq)]
enum Resizing {
    Sidebar { start_x: f32, start_width: f32, widths: (f32, f32) },
    Panel { start_x: f32, start_width: f32, widths: (f32, f32) },
}

pub struct MainView {
    store: Entity<Store>,
    sidebar: Entity<Sidebar>,
    thread: Entity<ThreadPane>,
    panel: Entity<SidePanelView>,
    /// What the agents spent, in place of the thread and the right panel while it is open.
    usage: Option<Entity<UsageRoute>>,
    resizing: Option<Resizing>,
    /// A press in the top bar that moves the window once it drags.
    moving: std::rc::Rc<std::cell::Cell<bool>>,
    _subscriptions: Vec<Subscription>,
}

/// A button in the window's top bar. The buttons touch: the space seen between them is theirs.
fn toolbar_button(id: &'static str, symbol: &'static str, help: impl Into<SharedString>) -> ActionButton {
    ActionButton::icon(id, symbol, help).margin(even(TOOLBAR_MARGIN))
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
            usage: None,
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

    /// Drawn over the top bar's end: the right panel's buttons.
    fn panel_buttons(&self, cx: &mut Context<Self>) -> Div {
        let store = self.store.read(cx);
        let open = store.side_panel.is_open;
        let maximized = store.panel_maximized();
        let can_maximize = store.can_maximize_panel();
        let maximize_help = if maximized {
            "Restore the right panel (Esc)".to_string()
        } else {
            store.shortcuts.help("Maximize the right panel", "rightPanel.toggleMaximized")
        };
        let side_help = store
            .shortcuts
            .help(if open { "Hide the right panel" } else { "Show the right panel" }, "rightPanel.toggle");
        let maximize = self.store.clone();
        let side = self.store.clone();
        div()
            .flex()
            .when(open, |bar| {
                bar.child(
                    toolbar_button(
                        "maximize-panel",
                        if maximized { "minimize-2" } else { "maximize-2" },
                        maximize_help,
                    )
                    .disabled(!can_maximize)
                    .on_click(move |_, _, cx| {
                        maximize.update(cx, |store, cx| {
                            store.toggle_panel_maximized();
                            cx.notify();
                        })
                    }),
                )
            })
            .child(toolbar_button("toggle-panel", "panel-right", side_help).on_click(move |_, _, cx| {
                side.update(cx, |store, cx| {
                    let open = !store.side_panel.is_open;
                    store.set_panel_open(open);
                    cx.notify();
                })
            }))
    }

    fn resize(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(resizing) = self.resizing else { return };
        let (key, width, bounds) = match resizing {
            Resizing::Sidebar { start_x, start_width, widths } => ("sidebar.width", start_width + x - start_x, widths),
            Resizing::Panel { start_x, start_width, widths } => ("panel.width", start_width - (x - start_x), widths),
        };
        let width = width.clamp(bounds.0, bounds.1);
        self.store.update(cx, |store, cx| {
            store.prefs.set(key, width);
            cx.notify();
        });
    }

    /// The line between the thread and what is beside it. Dragging it makes the sidebar or the
    /// side panel wider or narrower. With `grows_left` the pane is on the right of the line, so
    /// it grows when the line goes left.
    fn pane_divider(
        &self,
        id: &'static str,
        grows_left: bool,
        start: impl Fn(f32) -> Resizing + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        let c = colors(cx);
        let grab_left = if grows_left { -THREAD_REACH } else { -PANE_REACH };
        div().relative().w(px(1.)).h_full().flex_shrink_0().bg(c.border).child(
            div()
                .id(id)
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(grab_left))
                .w(px(THREAD_REACH + 1. + PANE_REACH))
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
        let window_width = f32::from(crate::window_frame::content_size(window).width);
        let store = self.store.read(cx);
        let hidden = store.prefs.bool("sidebar.hidden");
        let panel_width = store.prefs.f32_or("panel.width", 460.);
        let panel_open = store.side_panel.is_open;
        let maximized = store.panel_maximized();
        let trailing_room = crate::window_frame::trailing_room(window, cx);
        let leading_room = crate::window_frame::leading_room(window, cx);
        let past_window_buttons = leading_room + PAST_SIDEBAR_BUTTON;

        let sidebar_widths = sidebar_widths(window_width);
        let shown_width = shown_sidebar_width(store, window_width);
        let rest = window_width - if hidden { 0. } else { shown_width + 1. };
        let fits = panel_open && rest - 1. - THREAD_BESIDE_PANEL >= PANEL_WIDTHS.0;
        let beside = fits && !maximized;
        let panel_widths =
            (PANEL_WIDTHS.0, PANEL_WIDTHS.0.max(if fits { rest - 1. - THREAD_BESIDE_PANEL } else { rest - 1. }));
        let shown_panel = panel_width.clamp(panel_widths.0, panel_widths.1);

        let thread_width = if beside || (maximized && fits) { rest - 1. - shown_panel } else { rest };
        let shows_usage = self.store.read(cx).shows_usage;
        let covered = if maximized || shows_usage {
            f32::INFINITY
        } else if panel_open && !beside {
            shown_panel + 1.
        } else {
            0.
        };
        self.thread.update(cx, |thread, cx| {
            thread.title_inset = if hidden { past_window_buttons } else { 20. };
            thread.beside_panel = beside;
            thread.trailing_room = trailing_room;
            thread.covered = covered;
            thread.set_width(thread_width, cx);
        });
        self.panel.update(cx, |panel, _| {
            panel.tab_inset = (maximized && hidden).then_some(past_window_buttons);
            panel.trailing_room = trailing_room;
        });

        if !shows_usage {
            self.usage = None;
        } else if self.usage.is_none() {
            let store = self.store.clone();
            self.usage = Some(cx.new(|cx| UsageRoute::new(store, cx)));
        }
        if let Some(usage) = &self.usage {
            usage.update(cx, |usage, _| {
                usage.title_inset = if hidden { past_window_buttons } else { 20. };
                usage.width = rest;
                usage.trailing_room = trailing_room;
            });
        }
        let toggle_sidebar = cx.listener(|this, _, _, cx| this.toggle_sidebar(cx));
        let panel = self.panel.clone();

        // Behind the maximized panel the thread keeps the width it has beside it, so the
        // transcript isn't laid out again for a width nobody sees.
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
            .when(maximized && !shows_usage, |area| area.child(div().absolute().inset_0().child(panel.clone())))
            .when(panel_open && !beside && !maximized && !shows_usage, |area| {
                area.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right_0()
                        .occlude()
                        .flex()
                        .bg(c.background)
                        .shadow(c.shadow(theme::ShadowSize::Lg, theme::Opacity::ShadowStronger))
                        .child(self.pane_divider(
                            "panel-divider-over",
                            true,
                            move |x| Resizing::Panel { start_x: x, start_width: shown_panel, widths: panel_widths },
                            cx,
                        ))
                        .child(div().w(px(shown_panel)).h_full().child(panel.clone())),
                )
            })
            .when_some(self.usage.clone(), |area, usage| area.child(div().absolute().inset_0().child(usage)));

        div()
            .id("main")
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
            // Put away while hidden, not dropped, so that it keeps its scroll and its search.
            .child(
                div()
                    .relative()
                    .w(px(if hidden { 0. } else { shown_width }))
                    .h_full()
                    .flex_shrink_0()
                    .overflow_hidden()
                    .when(hidden, |sidebar| sidebar.invisible())
                    .child(div().w(px(shown_width)).h_full().child(self.sidebar.clone())),
            )
            .when(!hidden, |main| {
                main.child(self.pane_divider(
                    "sidebar-divider",
                    false,
                    move |x| Resizing::Sidebar { start_x: x, start_width: shown_width, widths: sidebar_widths },
                    cx,
                ))
            })
            .child(thread)
            .when(beside && !shows_usage, |main| {
                main.child(self.pane_divider(
                    "panel-divider",
                    true,
                    move |x| Resizing::Panel { start_x: x, start_width: shown_panel, widths: panel_widths },
                    cx,
                ))
                .child(div().w(px(shown_panel)).h_full().flex_shrink_0().child(panel.clone()))
            })
            // The buttons at the start of the top bar, past the window's own. The room is the
            // same shown and hidden, so the sidebar's button keeps its place.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(leading_room))
                    .h(px(theme::TOP_BAR))
                    .w(px(3. * TOOLBAR_WIDTH))
                    .flex()
                    .items_center()
                    .child(
                        toolbar_button(
                            "toggle-sidebar",
                            "panel-left",
                            self.store
                                .read(cx)
                                .shortcuts
                                .help(if hidden { "Show the sidebar" } else { "Hide the sidebar" }, "sidebar.toggle"),
                        )
                        .on_click(toggle_sidebar),
                    ),
            )
            // Drawn over the top bar's end, so that the right panel's button keeps its place
            // when the one beside it comes and goes.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right(px(PANEL_BUTTONS_INSET + trailing_room))
                    .h(px(theme::TOP_BAR))
                    .flex()
                    .items_center()
                    .when(!shows_usage, |bar| bar.child(self.panel_buttons(cx))),
            )
    }
}
