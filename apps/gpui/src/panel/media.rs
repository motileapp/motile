//! The Media tab: the images and videos of the thread, the newest first, in squares that fill the
//! panel's width. `MediaView.swift`.

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::gallery::Shown;

use super::view::RightPanelView;
use crate::media::pictures::{Picture, Pictures};
use crate::store::Store;
use crate::store::gallery::{self, Gallery};
use crate::theme::{IconSize, TextSize, colors};
use crate::ui::menu::Menu;
use crate::ui::{ActionButton, icons};

pub const EMPTY: &str = "The images and videos of the thread show up here.";
/// How narrow a square gets before a column goes.
const SMALLEST_SIDE: f32 = 128.;
const FEWEST_COLUMNS: usize = 3;
/// The gap drawn between squares, which each square's clicks reach across to its neighbours'.
const GAP: f32 = 2.;
/// The steps, in points, a square's picture is decoded at, so that resizing the panel doesn't
/// decode it again at every size.
const DECODE_STEP: f32 = 64.;

/// How many columns the squares are in, and how wide each is, for the grid's width.
pub fn grid(width: f32) -> (usize, f32) {
    let columns = FEWEST_COLUMNS.max((width / SMALLEST_SIDE) as usize);
    (columns, width / columns as f32)
}

/// How large a square's picture is decoded on its longer side, for its shorter one to fill the
/// square at `scale` pixels a point.
pub fn decoded_pixels(side: f32, scale: f32, pixels: Option<(u32, u32)>) -> u32 {
    let square = (side / DECODE_STEP).ceil() * DECODE_STEP * scale;
    let Some((width, height)) = pixels.filter(|(width, height)| *width > 0 && *height > 0) else {
        return (square * 2.) as u32;
    };
    let ratio = width.max(height) as f32 / width.min(height) as f32;
    (square * ratio.min(4.)).round() as u32
}

pub struct MediaView {
    store: Entity<Store>,
    thread_id: String,
    scroll: UniformListScrollHandle,
    /// The grid's width as it was last laid out.
    width: Rc<Cell<f32>>,
}

impl MediaView {
    pub fn new(store: Entity<Store>, thread_id: String, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self { store, thread_id, scroll: UniformListScrollHandle::new(), width: Rc::default() }
    }

    fn rows(&self, range: Range<usize>, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(media) = self.media(cx) else { return Vec::new() };
        let (columns, side) = grid(self.width.get());
        range
            .map(|row| {
                let squares = (row * columns..((row + 1) * columns).min(media.len()))
                    .map(|index| square(&self.store, &media, index, side, window, cx));
                div().flex().h(px(side)).children(squares).into_any_element()
            })
            .collect()
    }

    fn media(&self, cx: &App) -> Option<Rc<Vec<Shown>>> {
        let gallery: &Gallery = self.store.read(cx).gallery.as_ref()?;
        (gallery.thread_id == self.thread_id).then(|| gallery.media.clone())
    }
}

impl Render for MediaView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(media) = self.media(cx) else { return RightPanelView::loading(cx) };
        if media.is_empty() {
            return RightPanelView::message(EMPTY, false, cx);
        }
        let c = colors(cx);
        let bar = RightPanelView::bar(
            div()
                .text_size(TextSize::Xs)
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.muted_foreground)
                .child(gallery::summary(&media)),
            cx,
        );
        let (measured, view) = (self.width.clone(), cx.entity().downgrade());
        let rows = match self.width.get() > 0. {
            true => media.len().div_ceil(grid(self.width.get()).0),
            false => 0,
        };
        let body = div()
            .size_full()
            .relative()
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let width = f32::from(bounds.size.width);
                        if (measured.get() - width).abs() < 0.5 {
                            return;
                        }
                        measured.set(width);
                        let _ = view.update(cx, |_, cx| cx.notify());
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                uniform_list("media-grid", rows, cx.processor(|this, range, window, cx| this.rows(range, window, cx)))
                    .track_scroll(&self.scroll)
                    .size_full()
                    .pb(px(super::END_ROOM)),
            );
        RightPanelView::surface(bar, body.into_any_element())
    }
}

/// One image or video, cropped to a square. The gap around it is only drawn: the square takes
/// clicks up to its neighbours'.
fn square(
    store: &Entity<Store>,
    media: &Rc<Vec<Shown>>,
    index: usize,
    side: f32,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let c = colors(cx);
    let shown = &media[index];
    let picture = gallery::picture(shown).map(|id| {
        let pixels = decoded_pixels(side, window.scale_factor(), shown.width.zip(shown.height));
        Pictures::load(store, &format!("square{pixels}:{id}"), id, pixels, cx)
    });
    let image = match &picture {
        Some(Picture::Ready(picture)) => Some(picture.image.clone()),
        _ => None,
    };
    let missing = matches!(picture, None | Some(Picture::Failed(_)));
    let video = shown.video;
    let open = {
        let (store, media) = (store.clone(), media.clone());
        move |cx: &mut App| {
            let viewed = media.iter().map(gallery::viewed).collect();
            store.update(cx, |store, cx| {
                store.view(viewed, index);
                cx.notify();
            });
        }
    };
    let play = open.clone();
    let (menu_store, shown_in, menu_media, name) =
        (store.clone(), gallery::shown_in(shown), shown.media.clone(), shown.name.clone());
    div()
        .id(("gallery-square", index))
        .flex_none()
        .size(px(side))
        .p(px(GAP / 2.))
        .cursor_pointer()
        .child(
            div()
                .relative()
                .size_full()
                .overflow_hidden()
                .bg(c.background_secondary)
                .flex()
                .items_center()
                .justify_center()
                .when_some(image.clone(), |square, image| {
                    square.child(
                        img(ImageSource::Render(image)).absolute().inset_0().size_full().object_fit(ObjectFit::Cover),
                    )
                })
                .when(missing, |square| {
                    square.child(
                        icons::symbol(if video { "play" } else { "image" }, IconSize::Lg)
                            .text_color(c.muted_foreground),
                    )
                })
                // As a reply's video shows it.
                .when(video && image.is_some(), |square| {
                    square.child(
                        ActionButton::icon(("gallery-play", index), "play", "Play")
                            .overlay()
                            .large()
                            .round(true)
                            .on_click(move |_, _, cx| play(cx)),
                    )
                }),
        )
        .tooltip(crate::ui::tooltip(shown.name.clone()))
        .on_click(move |_, _, cx| open(cx))
        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
            let (show_store, add_store, shown_in) = (menu_store.clone(), menu_store.clone(), shown_in.clone());
            let (media, name) = (menu_media.clone(), name.clone());
            Menu::new()
                .symbol_item("Show in Chat", "message-square-text", move |_, cx| {
                    show_store.update(cx, |store, cx| {
                        store.show_in_chat(&shown_in, cx);
                        cx.notify();
                    })
                })
                .symbol_item("Add to Message", "paperclip", move |_, cx| {
                    add_store.update(cx, |store, cx| store.add_to_message(&media, &name, cx))
                })
                .show(event.position, window, cx);
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{decoded_pixels, grid};

    #[test]
    fn the_squares_fill_the_width_in_columns_no_narrower_than_the_smallest_side_but_three_at_least() {
        assert_eq!(grid(340.), (3, 340. / 3.));
        assert_eq!(grid(512.), (4, 128.));
        assert_eq!(grid(900.), (7, 900. / 7.));
    }

    #[test]
    fn a_picture_is_decoded_in_steps_for_its_shorter_side_to_fill_the_square() {
        assert_eq!(decoded_pixels(113., 2., Some((1600, 1600))), 256);
        assert_eq!(decoded_pixels(120., 2., Some((1600, 900))), 455);
        assert_eq!(decoded_pixels(120., 2., Some((100, 1000))), 1024);
        assert_eq!(decoded_pixels(120., 1., None), 256);
    }
}
