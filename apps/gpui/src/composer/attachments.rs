//! The attached files above the text: a tile for an image or a video and a chip for any other
//! file, each saying how far its upload is. A click on a tile opens it in the viewer.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::models::{Attachment, UploadState};
use crate::store::Store;
use crate::theme::{Radius, Surface, colors};
use crate::ui::{ActionButton, Variant, icons};

pub const TILE: f32 = 64.;

pub fn attachments(store: &Entity<Store>, cx: &App) -> impl IntoElement {
    let attached = store.read(cx).attachments();
    let pictured: Vec<Attachment> = attached.iter().filter(|attachment| attachment.pictured).cloned().collect();
    let viewed: Vec<_> = pictured.iter().filter_map(Attachment::viewed).collect();
    div().id("attachments").overflow_x_scroll().child(
        div()
            .px(px(14.))
            .pt(px(12.))
            .flex()
            .items_end()
            .gap(px(8.))
            .children(pictured.iter().enumerate().map(|(index, attachment)| {
                let (store, viewed) = (store.clone(), viewed.clone());
                tile(attachment, store.clone(), cx).on_click(move |_, _, cx| {
                    let viewed = viewed.clone();
                    store.update(cx, |store, cx| {
                        store.view(viewed, index);
                        cx.notify();
                    })
                })
            }))
            .children(
                attached.iter().filter(|attachment| !attachment.pictured).map(|attachment| chip(attachment, store, cx)),
            ),
    )
}

/// An image or a video: its picture, how far it is on its way to the server, and the button that
/// removes it.
fn tile(attachment: &Attachment, store: Entity<Store>, cx: &App) -> Stateful<Div> {
    let c = colors(cx);
    let picture: Option<AnyElement> = attachment
        .file
        .as_ref()
        .filter(|_| !attachment.video)
        .map(|file| img(file.clone()).size(px(TILE)).object_fit(ObjectFit::Cover).into_any_element())
        .or_else(|| {
            let id = attachment.attached().picture().cloned()?;
            Some(crate::media::MediaImage::new(store.clone(), id).size(TILE).cover().into_any_element())
        });
    let remove = store.clone();
    let id = attachment.id.clone();
    div()
        .id(SharedString::from(format!("tile-{}", attachment.id)))
        .relative()
        .size(px(TILE))
        .flex_shrink_0()
        .rounded(px(Radius::CARD))
        .overflow_hidden()
        .bg(c.background_secondary)
        .border_1()
        .border_color(c.border)
        .active(|tile| tile.opacity(0.7))
        .children(picture)
        .when(attachment.video, |tile| {
            tile.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icons::symbol("play", 16.).text_color(white())),
            )
        })
        .child(div().absolute().bottom_0().left_0().right_0().child(progress(attachment, &store, true, cx)))
        .child(
            // Over a picture the button is as in the dark, on a backdrop that shows on any picture.
            div().absolute().top(px(2.)).right(px(2.)).rounded_full().bg(hsla(0., 0., 0., 0.6)).child(
                ActionButton::icon(SharedString::from(format!("remove-{}", attachment.id)), "x", "Remove")
                    .small()
                    .round(true)
                    .tint(Some(white()))
                    .on_click(move |_, _, cx| {
                        remove.update(cx, |store, cx| {
                            store.remove_attachment(&id);
                            cx.notify();
                        })
                    }),
            ),
        )
        .tooltip(crate::ui::tooltip(attachment.name.clone()))
}

/// A file that isn't shown as a picture: its name, its size, and how far it is on its way to the
/// server.
fn chip(attachment: &Attachment, store: &Entity<Store>, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let remove = store.clone();
    let id = attachment.id.clone();
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(5.))
        .pl(px(9.))
        .pr(px(2.))
        .py(px(2.))
        .rounded_full()
        .bg(Surface::Composer.next().color(c))
        .text_size(px(12.))
        .child(icons::symbol("file", 12.))
        .child(div().whitespace_nowrap().child(attachment.name.clone()))
        .when_some(attachment.bytes, |chip, bytes| chip.child(div().text_color(c.secondary).child(file_size(bytes))))
        .child(progress(attachment, store, false, cx))
        .child(
            ActionButton::icon(SharedString::from(format!("remove-chip-{}", attachment.id)), "x", "Remove")
                .small()
                .round(true)
                .surface(Surface::ComposerSecondary)
                .on_click(move |_, _, cx| {
                    remove.update(cx, |store, cx| {
                        store.remove_attachment(&id);
                        cx.notify();
                    })
                }),
        )
}

/// How far an attachment is on its way to the server, or the button that tries again when it
/// didn't get there. Nothing once it is there.
fn progress(attachment: &Attachment, store: &Entity<Store>, on_picture: bool, cx: &App) -> AnyElement {
    let c = colors(cx);
    match &attachment.state {
        UploadState::Ready => div().into_any_element(),
        UploadState::Uploading(fraction) => {
            let percent = format!("{}%", (fraction * 100.) as u32);
            if on_picture {
                div()
                    .w_full()
                    .py(px(2.))
                    .bg(hsla(0., 0., 0., 0.6))
                    .flex()
                    .justify_center()
                    .text_size(px(10.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(white())
                    .child(percent)
                    .into_any_element()
            } else {
                div().text_color(c.secondary).child(percent).into_any_element()
            }
        }
        UploadState::Failed(reason) => {
            let (store, id) = (store.clone(), attachment.id.clone());
            let help = format!("{reason} Click to try again.");
            let retry = move |cx: &mut App| {
                store.update(cx, |store, cx| {
                    store.retry_attachment(&id);
                    cx.notify();
                })
            };
            if !on_picture {
                return ActionButton::new(SharedString::from(format!("retry-{}", attachment.id)), "Retry")
                    .symbol("rotate-cw")
                    .help(help)
                    .variant(Variant::Danger)
                    .small()
                    .round(true)
                    .on_click(move |_, _, cx| retry(cx))
                    .into_any_element();
            }
            div()
                .id(SharedString::from(format!("retry-{}", attachment.id)))
                .w_full()
                .py(px(2.))
                .bg(c.danger.opacity(0.85))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(3.))
                .text_size(px(10.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(white())
                .active(|retry| retry.opacity(0.7))
                .child(icons::symbol("rotate-cw", 10.))
                .child("Retry")
                .tooltip(crate::ui::tooltip(help))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    retry(cx)
                })
                .into_any_element()
        }
    }
}

/// "12 KB", "3.4 MB", as the Mac writes a file's size.
pub fn file_size(bytes: u64) -> String {
    let bytes = bytes as f64;
    if bytes < 1000. {
        return format!("{} bytes", bytes as u64);
    }
    let units = ["KB", "MB", "GB", "TB"];
    let mut value = bytes / 1000.;
    let mut unit = 0;
    while value >= 1000. && unit < units.len() - 1 {
        value /= 1000.;
        unit += 1;
    }
    if value < 10. && unit > 0 {
        format!("{value:.1} {}", units[unit])
    } else {
        format!("{} {}", value.round() as u64, units[unit])
    }
}
