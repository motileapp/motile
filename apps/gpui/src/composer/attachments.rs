//! The attached files above the text: a tile for each, with its picture for an image or a video
//! and its kind and name for any other file, each saying how far its upload is. A click on a tile opens it in the viewer. A draft that
//! goes on from a removed server's thread has a chip for that thread before them.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::media::pictures::{Picture, Pictures};
use crate::models::{Attachment, UploadState};
use crate::store::Store;
use crate::theme::{ControlSize, IconSize, Layer, Opacity, Radius, ShadowSize, Surface, TextSize, colors};
use crate::ui::{ActionButton, Chip, icons};

pub const TILE: f32 = 96.;
const NAME_INSET_X: f32 = 10.;
const NAME_INSET_Y: f32 = 8.;
const KIND_INSET: f32 = 6.;
/// The remove button sits level with a file's kind.
const REMOVE_INSET: f32 = KIND_INSET - (ControlSize::Small.height() - Chip::COMPACT_HEIGHT) / 2.;
/// A file's kind is cut short before the remove button.
const KIND_WIDTH: f32 = TILE - KIND_INSET - 4. - ControlSize::Small.height() - REMOVE_INSET;
/// How wide the title of the thread a draft goes on from gets before it is cut.
const CONTINUES_WIDTH: f32 = 320.;

pub fn attachments(store: &Entity<Store>, cx: &mut App) -> impl IntoElement {
    let attached = store.read(cx).attachments();
    let pictured: Vec<Attachment> = attached.iter().filter(|attachment| attachment.pictured).cloned().collect();
    let viewed: Vec<_> = pictured.iter().filter_map(Attachment::viewed).collect();
    let mut items: Vec<AnyElement> = Vec::new();
    let continues = store.read(cx).selected_draft().and_then(|draft| draft.from.clone());
    if let Some(from) = continues {
        items.push(continues_chip(&from.title, store, cx).into_any_element());
    }
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
        items.push(file_tile(attachment, store.clone(), cx).into_any_element());
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
    let tile = frame(attachment, cx);
    let tile = crate::ui::dims(tile, cx).children(picture).when(attachment.video, |tile| {
        tile.child(
            div().absolute().inset_0().flex().items_center().justify_center().child(
                div()
                    .rounded_full()
                    .shadow(c.shadow(ShadowSize::Sm, Opacity::ShadowStrongest))
                    .child(icons::symbol("play", IconSize::Lg).text_color(c.foreground_over_picture)),
            ),
        )
    });
    finish(tile, attachment, store, cx)
}

/// A file that isn't shown as a picture: its kind, level with the remove button, and its name.
fn file_tile(attachment: &Attachment, store: Entity<Store>, cx: &mut App) -> Stateful<Div> {
    let c = colors(cx);
    let tile = frame(attachment, cx)
        .child(
            div().size_full().flex().flex_col().justify_end().px(px(NAME_INSET_X)).py(px(NAME_INSET_Y)).child(
                div()
                    .text_size(TextSize::Xxs)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c.foreground)
                    .line_clamp(3)
                    .child(attachment.name.clone()),
            ),
        )
        .children(kind(&attachment.name).map(|kind| {
            div()
                .absolute()
                .top(px(KIND_INSET))
                .left(px(KIND_INSET))
                .child(Chip::new(kind).muted(true).small(true).max_width(KIND_WIDTH))
        }));
    finish(tile, attachment, store, cx)
}

/// The square every attachment lies in.
fn frame(attachment: &Attachment, cx: &App) -> Stateful<Div> {
    let c = colors(cx);
    div()
        .id(SharedString::from(format!("tile-{}", attachment.id)))
        .relative()
        .size(px(TILE))
        .flex_shrink_0()
        .cursor_default()
        .rounded(px(Radius::XL))
        .overflow_hidden()
        .bg(c.background_secondary)
        .border_1()
        .border_color(c.border)
}

/// How far the upload is along the tile's bottom, the button that removes it, and its name and
/// size under the pointer.
fn finish(tile: Stateful<Div>, attachment: &Attachment, store: Entity<Store>, cx: &App) -> Stateful<Div> {
    let remove = store.clone();
    let id = attachment.id.clone();
    let summary = match attachment.bytes {
        Some(bytes) => format!("{}, {}", attachment.name, file_size(bytes)),
        None => attachment.name.clone(),
    };
    tile.child(div().absolute().bottom_0().left_0().right_0().child(progress(attachment, &store, cx)))
        .child(
            div().absolute().top(px(REMOVE_INSET)).right(px(REMOVE_INSET)).child(
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
        .tooltip(crate::ui::tooltip(summary))
}

/// A file's extension, as "PDF", when it has one.
fn kind(name: &str) -> Option<String> {
    let extension = std::path::Path::new(name).extension()?.to_str()?;
    Some(extension.to_uppercase())
}

/// The thread of a removed server the draft goes on from, with the button that lets go of it.
fn continues_chip(title: &str, store: &Entity<Store>, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let store = store.clone();
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
        .text_size(TextSize::Xs)
        .child(icons::symbol("history", IconSize::Sm))
        .child(div().max_w(px(CONTINUES_WIDTH)).truncate().child(format!("Continues “{title}”")))
        .child(
            ActionButton::icon("drop-continuation", "x", "Start without it")
                .small()
                .round(true)
                .surface(Surface::Composer)
                .row(Some(Layer::Chip))
                .on_click(move |_, _, cx| {
                    store.update(cx, |store, cx| {
                        store.drop_continuation();
                        cx.notify();
                    })
                }),
        )
}

/// How far an attachment is on its way to the server, along its tile's bottom edge, or the button
/// that tries again when it didn't get there. Nothing once it is there.
fn progress(attachment: &Attachment, store: &Entity<Store>, cx: &App) -> AnyElement {
    let c = colors(cx);
    match &attachment.state {
        UploadState::Ready => div().into_any_element(),
        UploadState::Uploading(fraction) => div()
            .w_full()
            .py(px(2.))
            .bg(c.scrim)
            .flex()
            .justify_center()
            .text_size(TextSize::Xxs)
            .font_weight(FontWeight::MEDIUM)
            .text_color(c.foreground_over_picture)
            .font_features(crate::ui::tabular_numbers())
            .child(format!("{}%", (fraction * 100.) as u32))
            .into_any_element(),
        UploadState::Failed(reason) => {
            let (store, id) = (store.clone(), attachment.id.clone());
            let help = format!("{reason} Click to try again.");
            let strip = div()
                .id(SharedString::from(format!("retry-{}", attachment.id)))
                .w_full()
                .py(px(2.))
                .bg(c.destructive)
                .flex()
                .items_center()
                .justify_center()
                .gap(px(3.))
                .text_size(TextSize::Xxs)
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.destructive_foreground);
            crate::ui::dims(strip, cx)
                .child(icons::symbol("rotate-cw", IconSize::Xs))
                .child("Retry")
                .tooltip(crate::ui::tooltip(help))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    store.update(cx, |store, cx| {
                        store.retry_attachment(&id);
                        cx.notify();
                    })
                })
                .into_any_element()
        }
    }
}

/// "Zero KB", "1 byte", "12.3 MB", as the Mac writes a file's size.
pub fn file_size(bytes: u64) -> String {
    crate::format::file_bytes(bytes)
}
