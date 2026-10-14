//! Where the sidebar ends: the ways to the settings and the usage, or back from them, the account,
//! and a new version of the client. Settings ends its sidebar with it too.

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::SIDEBAR_EDGE;
use super::updates::{AppUpdateRow, UpdateLabel};
use crate::store::Store;
use crate::theme::{ControlSize, colors};
use crate::ui::control::Alignment;
use crate::ui::menu::Menu;
use crate::ui::{ActionButton, ActionMenu, TOOLBAR_MARGIN, edges};
use crate::updater::UpdateState;

/// The room around each button that is its own, so the buttons touch.
fn reach() -> Edges<f32> {
    edges(4., TOOLBAR_MARGIN, 4., TOOLBAR_MARGIN)
}

/// How far a button's symbol is from the button's sides.
fn symbol_inset() -> f32 {
    (ControlSize::Regular.height() - ControlSize::Regular.symbol()) / 2.
}

pub struct SidebarFooter {
    store: Entity<Store>,
    _subscriptions: Vec<Subscription>,
}

impl SidebarFooter {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![cx.observe(&store, |_, _, cx| cx.notify())];
        Self { store, _subscriptions: subscriptions }
    }

    /// The account as its picture, which opens a menu with who is signed in, the settings, the
    /// usage and the way to sign out.
    fn account_menu(&self, cx: &App) -> ActionMenu {
        let store = self.store.read(cx);
        let email = store.account.user.as_ref().map(|user| user.email.clone()).unwrap_or_default();
        let initial =
            store.account_name().unwrap_or(&email).chars().next().map(|first| first.to_uppercase().to_string());
        let picture = account_picture(store.account_picture.clone(), initial.unwrap_or_default(), cx);
        let handle = self.store.clone();
        let header = email.clone();
        ActionMenu::icon("account", "circle-user", email, move |_, _| {
            let (settings, usage, sign_out) = (handle.clone(), handle.clone(), handle.clone());
            Menu::new()
                .note(header.clone())
                .item("Settings", move |_, cx| {
                    settings.update(cx, |store, cx| {
                        store.open_settings(Default::default(), None);
                        cx.notify();
                    })
                })
                .item("Usage", move |_, cx| {
                    usage.update(cx, |store, cx| {
                        store.open_usage();
                        cx.notify();
                    })
                })
                .separator()
                .item("Sign Out", move |_, cx| {
                    sign_out.update(cx, |store, cx| {
                        store.sign_out(cx);
                        cx.notify();
                    })
                })
        })
        .button(|button| button.picture(picture).symbol_size(PICTURE_SIDE).margin(reach()))
    }
}

/// The picture's side, which fills its button but for a margin.
const PICTURE_SIDE: f32 = 20.;

/// The account's picture, or the first letter of its name on a disc until there is one.
fn account_picture(file: Option<std::path::PathBuf>, initial: String, cx: &App) -> AnyElement {
    let c = colors(cx);
    let (border, foreground) = (c.border, c.foreground);
    let disc = move || {
        div()
            .size(px(PICTURE_SIDE))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(border)
            .text_color(foreground)
            .text_size(px(PICTURE_SIDE * 0.5))
            .font_weight(FontWeight::SEMIBOLD)
            .child(initial.clone())
            .into_any_element()
    };
    let Some(file) = file else { return disc() };
    img(file).size(px(PICTURE_SIDE)).rounded_full().object_fit(ObjectFit::Cover).with_fallback(disc).into_any_element()
}

impl Render for SidebarFooter {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let state = store.updater.state.clone();
        let route_open = store.shows_usage || store.settings.section.is_some();
        let settings_help = store.shortcuts.help("Settings", "settings.open");
        let handle = self.store.clone();
        let account = self.account_menu(cx);
        let buttons = div()
            .flex()
            .items_center()
            .mx(px(-symbol_inset() - reach().left))
            .my(px(-reach().top))
            .child(account)
            .when(route_open, |row| {
                let store = handle.clone();
                row.child(
                    div().flex_1().min_w_0().child(
                        ActionButton::new("back", "Back")
                            .symbol("arrow-left")
                            .help("Back to the threads (Esc)")
                            .ghost()
                            .fills(true)
                            .alignment(Alignment::Leading)
                            .margin(reach())
                            .on_click(move |_, _, cx| {
                                store.update(cx, |store, cx| {
                                    store.close_route();
                                    cx.notify();
                                })
                            }),
                    ),
                )
            })
            .when(!route_open, |row| {
                let (settings, usage) = (handle.clone(), handle.clone());
                row.child(ActionButton::icon("settings", "settings", settings_help).margin(reach()).on_click(
                    move |_, _, cx| {
                        settings.update(cx, |store, cx| {
                            store.open_settings(Default::default(), None);
                            cx.notify();
                        })
                    },
                ))
                .child(
                    ActionButton::icon(
                        "usage",
                        "chart-column",
                        "Usage: what the agents spent and what is left of their plans",
                    )
                    .margin(reach())
                    .on_click(move |_, _, cx| {
                        usage.update(cx, |store, cx| {
                            store.open_usage();
                            cx.notify();
                        })
                    }),
                )
                .child(div().flex_1())
            })
            .when(state == UpdateState::UpToDate, |row| row.child(UpdateLabel::up_to_date(cx)))
            .child({
                let store = handle.clone();
                ActionButton::icon("check-updates", "refresh-cw", "Check for Updates")
                    .pending(state == UpdateState::Checking)
                    .margin(reach())
                    .on_click(move |_, _, cx| {
                        store.update(cx, |store, cx| {
                            store.updater.check(true, cx);
                            cx.notify();
                        })
                    })
            });
        div()
            .w_full()
            .px(px(SIDEBAR_EDGE))
            .py(px(SIDEBAR_EDGE - 2. - symbol_inset()))
            .flex()
            .flex_col()
            .gap(px(8.))
            .when(state.shows_row(), |footer| footer.child(AppUpdateRow::new(&self.store)))
            .child(buttons)
    }
}
