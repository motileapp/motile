//! The settings over the whole window: their sections on the left, the open one's page on the
//! right and its title in the window's top bar. The way back is in the top bar beside the
//! window's buttons; Escape and ⌘W take it too.

mod page;
mod sidebar;
mod usage;

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use self::page::SettingsPage;
use self::sidebar::SettingsSidebar;
use crate::store::Store;
use crate::theme::{self, colors};
use crate::ui::{ActionButton, TOOLBAR_MARGIN, even};

pub const SIDEBAR_WIDTH: f32 = 220.;
/// How far a group's rows stand from the card's sides.
pub const INSET: f32 = 14.;
/// Where the Back button starts, past the window's buttons, as the sidebar's own button does.
const BACK_LEFT: f32 = 90.;

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
        Self { store, sidebar, page, focus, moving: Default::default(), _subscriptions: subscriptions }
    }

    fn close(&self, cx: &mut App) {
        self.store.update(cx, |store, cx| {
            store.close_settings();
            cx.notify();
        });
    }
}

impl Render for SettingsRoute {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let section = self.store.read(cx).settings.section.unwrap_or_default();
        let setup = self.page.read(cx).setup.clone();
        div()
            .id("settings-route")
            .track_focus(&self.focus)
            .occlude()
            .absolute()
            .inset_0()
            .bg(c.background)
            .text_color(c.text)
            .flex()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.close(cx);
                }
            }))
            .child(crate::ui::window_drag_strip("settings-top-bar", theme::TOP_BAR, &self.moving))
            .child(div().w(px(SIDEBAR_WIDTH)).h_full().flex_shrink_0().child(self.sidebar.clone()))
            .child(div().w(px(1.)).h_full().flex_shrink_0().bg(c.border))
            .child(div().flex_1().min_w_0().h_full().child(self.page.clone()))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(SIDEBAR_WIDTH + 1.))
                    .right_0()
                    .h(px(theme::TOP_BAR))
                    .px(px(20.))
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
                div().absolute().top_0().left(px(BACK_LEFT)).h(px(theme::TOP_BAR)).flex().items_center().child(
                    ActionButton::new("settings-back", "Back")
                        .symbol("arrow-left")
                        .ghost()
                        .help("Back to the threads (Esc)")
                        .margin(even(TOOLBAR_MARGIN))
                        .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                ),
            )
            .children(setup)
    }
}
