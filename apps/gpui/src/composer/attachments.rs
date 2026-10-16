//! The attached files above the text: a tile for an image or a video and a chip for any other
//! file, each saying how far its upload is. A click on a tile opens it in the viewer.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::media::pictures::{Picture, Pictures};
use crate::models::{Attachment, UploadState};
use crate::store::Store;
use crate::theme::{Layer, Opacity, Radius, ShadowSize, Surface, colors};
use crate::ui::{ActionButton, Variant, icons};

pub const TILE: f32 = 64.;

pub fn attachments(store: &Entity<Store>, cx: &mut App) -> impl IntoElement {
    let attached = store.read(cx).attachments();
    let pictured: Vec<Attachment> = attached.iter().filter(|attachment| attachment.pictured).cloned().collect();
    let viewed: Vec<_> = pictured.iter().filter_map(Attachment::viewed).collect();
    let mut items: Vec<AnyElement> = Vec::new();
    for (index, attachment) in pictured.iter().enumerate() {
        let (opens, viewed) = (store.clone(), viewed.clone());
        let tile = tile(attachment, store.clone(), cx).on_click(move |_, _, cx| {
            cx.stop_propagation();
            let viewed = viewed.clone();
            opens.update(cx, |store, cx| {
                store.view(viewed, index);
                cx.notify();
            })
        });
        items.push(tile.into_any_element());
    }
    for attachment in attached.iter().filter(|attachment| !attachment.pictured) {
        items.push(chip(attachment, store, cx).into_any_element());
    }
    div()
        .id("attachments")
        .overflow_x_scroll()
        .child(div().px(px(14.)).pt(px(12.)).flex().items_end().gap(px(8.)).children(items))
}

/// An image or a video: its picture, how far it is on its way to the server, and the button that
/// removes it.
fn tile(attachment: &Attachment, store: Entity<Store>, cx: &mut App) -> Stateful<Div> {
    let c = colors(cx);
    let picture: Option<AnyElement> = match &attachment.file {
        Some(file) => {
            let (key, max_pixels) = (format!("tile:{}", attachment.id), (TILE * 4.) as u32);
            let picture = match attachment.video {
                true => Pictures::load_video_file(&key, file, max_pixels, cx),
                false => Pictures::load_file(&key, file, max_pixels, cx),
            };
            match picture {
                Picture::Ready(shown) => Some(
                    img(ImageSource::Render(shown.image))
                        .size(px(TILE))
                        .object_fit(ObjectFit::Contain)
                        .into_any_element(),
                ),
                _ => None,
            }
        }
        None => attachment
            .attached()
            .picture()
            .cloned()
            .map(|id| crate::media::MediaImage::new(store.clone(), id).size(TILE).into_any_element()),
    };
    let remove = store.clone();
    let id = attachment.id.clone();
    let tile = div()
        .id(SharedString::from(format!("tile-{}", attachment.id)))
        .relative()
        .size(px(TILE))
        .flex_shrink_0()
        .cursor_default()
        .rounded(px(Radius::LG))
        .overflow_hidden()
        .bg(c.background_secondary)
        .border_1()
        .border_color(c.border);
    crate::ui::dims(tile, cx)
        .children(picture)
        .when(attachment.video, |tile| {
            tile.child(
                div().absolute().inset_0().flex().items_center().justify_center().child(
                    div()
                        .rounded_full()
                        .shadow(c.shadow(ShadowSize::Sm, Opacity::ShadowStrongest))
                        .child(icons::symbol("play", 16.).text_color(c.foreground_over_picture)),
                ),
            )
        })
        .child(div().absolute().bottom_0().left_0().right_0().child(progress(attachment, &store, true, cx)))
        .child(
            div().absolute().top(px(2.)).right(px(2.)).child(
                ActionButton::icon(SharedString::from(format!("remove-{}", attachment.id)), "x", "Remove")
                    .small()
                    .round(true)
                    .overlay()
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
        .cursor_default()
        .flex()
        .items_center()
        .gap(px(5.))
        .pl(px(9.))
        .pr(px(2.))
        .py(px(2.))
        .rounded_full()
        .bg(Surface::Composer.layer(Layer::Chip, c))
        .border_1()
        .border_color(c.border_accent)
        .text_size(px(12.))
        .child(icons::symbol("file", 12.))
        .child(div().whitespace_nowrap().child(attachment.name.clone()))
        .when_some(attachment.bytes, |chip, bytes| {
            chip.child(div().text_color(c.muted_foreground).child(file_size(bytes)))
        })
        .child(progress(attachment, store, false, cx))
        .child(
            ActionButton::icon(SharedString::from(format!("remove-chip-{}", attachment.id)), "x", "Remove")
                .small()
                .round(true)
                .surface(Surface::Composer)
                .row(Some(Layer::Chip))
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
                    .bg(c.scrim)
                    .flex()
                    .justify_center()
                    .text_size(px(10.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c.foreground_over_picture)
                    .font_features(crate::ui::tabular_numbers())
                    .child(percent)
                    .into_any_element()
            } else {
                div()
                    .text_color(c.muted_foreground)
                    .font_features(crate::ui::tabular_numbers())
                    .child(percent)
                    .into_any_element()
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
            let strip = div()
                .id(SharedString::from(format!("retry-{}", attachment.id)))
                .w_full()
                .py(px(2.))
                .bg(c.destructive)
                .flex()
                .items_center()
                .justify_center()
                .gap(px(3.))
                .text_size(px(10.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.destructive_foreground);
            crate::ui::dims(strip, cx)
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

/// "Zero KB", "1 byte", "12.3 MB", as the Mac writes a file's size.
pub fn file_size(bytes: u64) -> String {
    crate::format::file_bytes(bytes)
}
