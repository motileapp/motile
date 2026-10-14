//! The settings over the whole window: their sections on the left, as wide as the sidebar and
//! dragged with the same line, the open one's page on the right and its title in the window's top
//! bar. The way back is in the top bar beside the window's buttons and under the sections; Esc and
//! the close command's keys take it too.

mod account_sheet;
mod keybindings;
mod layout;
mod page;
mod sidebar;

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use self::page::SettingsPage;
pub use self::page::top_fade;
use self::sidebar::SettingsSidebar;
use crate::main_view::{PANE_REACH, THREAD_REACH, shown_sidebar_width, sidebar_widths};
use crate::store::Store;
use crate::theme::{self, colors};
use crate::ui::{ActionButton, TOOLBAR_MARGIN, even};

/// Whether the window is the one with the threads in it, which ⌘W must not close while a tab or
/// the settings can be closed instead.
pub fn is_main_window(window: AnyWindowHandle, cx: &App) -> bool {
    window
        .downcast::<gpui_kit::base::Root>()
        .is_some_and(|root| root.read(cx).is_ok_and(|root| root.view().clone().downcast::<crate::root::Root>().is_ok()))
}

pub struct SettingsRoute {
    store: Entity<Store>,
    sidebar: Entity<SettingsSidebar>,
    page: Entity<SettingsPage>,
    focus: FocusHandle,
    /// Where a drag of the line beside the sections started, and the width they had then.
    resizing: Option<(f32, f32)>,
    /// A press in the top bar that moves the window once it drags.
    moving: Rc<Cell<bool>>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsRoute {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sidebar = cx.new(|cx| SettingsSidebar::new(store.clone(), window, cx));
        let page = cx.new(|cx| SettingsPage::new(store.clone(), window, cx));
        let subscriptions = vec![cx.observe(&store, |_, _, cx| cx.notify()), cx.observe(&page, |_, _, cx| cx.notify())];
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self { store, sidebar, page, focus, resizing: None, moving: Default::default(), _subscriptions: subscriptions }
    }

    fn close(&self, cx: &mut App) {
        self.store.update(cx, |store, cx| {
            store.close_route();
            cx.notify();
        });
    }

    /// The line between the sections and the page, which drags the sidebar's width.
    fn divider(&self, width: f32, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        div().relative().w(px(1.)).h_full().flex_shrink_0().bg(c.border).child(
            div()
                .id("settings-divider")
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(-PANE_REACH))
                .w(px(THREAD_REACH + 1. + PANE_REACH))
                .cursor(CursorStyle::ResizeLeftRight)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        this.resizing = Some((f32::from(event.position.x), width));
                        cx.stop_propagation();
                    }),
                ),
        )
    }
}

impl Render for SettingsRoute {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let section = self.store.read(cx).settings.section.unwrap_or_default();
        let width =
            shown_sidebar_width(self.store.read(cx), f32::from(crate::window_frame::content_size(window).width));
        let trailing_room = crate::window_frame::trailing_room(window, cx);
        div()
            .id("settings-route")
            .track_focus(&self.focus)
            .occlude()
            .absolute()
            .inset_0()
            .bg(c.background)
            .text_color(c.foreground)
            .flex()
            .when(self.resizing.is_some(), |route| {
                route
                    .cursor(CursorStyle::ResizeLeftRight)
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                        let Some((start_x, start_width)) = this.resizing else { return };
                        if event.pressed_button != Some(MouseButton::Left) {
                            this.resizing = None;
                            return cx.notify();
                        }
                        let dragged = start_width + f32::from(event.position.x) - start_x;
                        let (narrowest, widest) =
                            sidebar_widths(f32::from(crate::window_frame::content_size(window).width));
                        this.store.update(cx, |store, cx| {
                            store.prefs.set("sidebar.width", dragged.clamp(narrowest, widest));
                            cx.notify();
                        });
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.resizing = None;
                            cx.notify();
                        }),
                    )
            })
            .child(crate::ui::window_drag_strip("settings-top-bar", theme::TOP_BAR, &self.moving))
            .child(div().w(px(width)).h_full().flex_shrink_0().child(self.sidebar.clone()))
            .child(self.divider(width, cx))
            .child(div().flex_1().min_w_0().h_full().child(self.page.clone()))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(width + 1.))
                    .right_0()
                    .h(px(theme::TOP_BAR))
                    // Centred between the window's buttons too, where the app draws them.
                    .px(px(20. + trailing_room))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(section.title()),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(crate::window_frame::leading_room(window, cx)))
                    .h(px(theme::TOP_BAR))
                    .flex()
                    .items_center()
                    .child(
                        ActionButton::new("settings-back", "Back")
                            .symbol("arrow-left")
                            .ghost()
                            .help("Back to the threads (Esc)")
                            .margin(even(TOOLBAR_MARGIN))
                            .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                    ),
            )
    }
}
