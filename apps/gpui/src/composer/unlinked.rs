//! In place of the composer's box for a thread whose server was unlinked: the box, saying why
//! the thread can only be read, with the ways on from it under that.

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{RADIUS, margin};
use crate::models::Server;
use crate::store::Store;
use crate::store::settings::SettingsSection;
use crate::theme::{self, IconSize, Opacity, Radius, Surface, TextSize, colors};
use crate::ui::{ActionButton, icons, skill_chip};

/// How far the words stand from the box's side, as the composer's text does, its border within.
const TEXT_INSET: f32 = 14.;
const TEXT_SIZE: TextSize = TextSize::Sm;

pub fn unlinked_box(store: &Entity<Store>, server: &Server, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let can_continue = store.read(cx).has_live_servers();
    let (settings, elsewhere) = (store.clone(), store.clone());
    let title = div()
        .flex()
        .items_center()
        .gap(px(6.))
        .text_color(c.warning)
        .child(icons::symbol("unlink", IconSize::Sm))
        .child(div().flex_none().text_size(TEXT_SIZE).font_weight(FontWeight::MEDIUM).child("Read-only:"))
        .child(name_chip(server.name.clone(), cx))
        .child(div().min_w_0().text_size(TEXT_SIZE).font_weight(FontWeight::MEDIUM).truncate().child("is unlinked"));
    let buttons = div()
        .flex()
        .justify_end()
        .child(
            ActionButton::new("unlinked-settings", "Settings")
                .help(format!("Delete “{}” and its threads in Settings", server.name))
                .round(true)
                .surface(Surface::Composer)
                .margin(margin(1., 4.))
                .on_click(move |_, _, cx| {
                    settings.update(cx, |store, cx| {
                        store.open_settings(SettingsSection::Servers, None);
                        cx.notify();
                    })
                }),
        )
        .child(
            ActionButton::new("unlinked-continue", "Continue")
                .help("Start a thread on another server with everything that happened here")
                .primary()
                .round(true)
                .surface(Surface::Composer)
                .margin(margin(4., 8.))
                .disabled(!can_continue)
                .on_click(move |_, _, cx| {
                    elsewhere.update(cx, |store, cx| {
                        store.continue_on_another_server(cx);
                        cx.notify();
                    })
                }),
        );
    div()
        .id("unlinked-box")
        .flex()
        .flex_col()
        .gap(px(6.))
        .pt(px(TEXT_INSET - 1.))
        .bg(c.composer)
        .rounded(px(RADIUS))
        .border_1()
        .border_color(c.border_composer)
        .shadow(c.shadow(theme::ShadowSize::Md, theme::Opacity::Shadow))
        .child(title.px(px(TEXT_INSET - 1.)))
        .child(div().px(px(TEXT_INSET - 1.)).text_size(TEXT_SIZE).text_color(c.muted_foreground).child(
            "This thread is read-only. You can continue it on another server, or delete the server in settings.",
        ))
        .child(buttons)
}

/// The server's name, set apart as a skill is in a message: on a wash of the title's colour,
/// inside an edge.
fn name_chip(name: String, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .flex_none()
        .px(px(skill_chip::PADDING))
        .rounded(px(Radius::XS))
        .border_1()
        .border_color(c.at(c.warning, Opacity::ColorTintChart))
        .bg(c.wash(c.warning, false))
        .text_size(TEXT_SIZE)
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .child(name)
}
