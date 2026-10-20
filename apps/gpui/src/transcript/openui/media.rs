//! Media and data: SearchImage and Image, Icon, Table, YouTube (in `web`) and Map (in `map`).

use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::openui::host::UiEvent;
use motile_core::render::openui::tree::{
    Fit, PhotoState, Photos, Radius, TableCell, TableColumn, TextStyle, UiKind, UiNode,
};

use super::style::{self, LABEL, Look, SMALL, Type};
use super::text::rich;
use super::{UiCx, framed, node};
use crate::media::pictures::{Picture, Pictures};
use crate::theme::{Layer, Surface, colors};
use crate::transcript::kit::ShimmerBox;

const FADE_IN: Duration = Duration::from_millis(300);
const MISSING_SYMBOL: f32 = 26.;

pub fn media(node: &UiNode, cx: &UiCx, window: &mut Window, app: &mut App) -> Stateful<Div> {
    let element = div().id(cx.id(&node.id, "node"));
    match &node.kind {
        UiKind::SearchImage { aspect_ratio, radius, fit, photos, .. } => {
            image(element, &node.id, None, *aspect_ratio, radius, *fit, photos, cx, window, app)
        }
        UiKind::Image { src, aspect_ratio, radius, fit, photos, .. } => {
            image(element, &node.id, src.as_deref(), *aspect_ratio, radius, *fit, photos, cx, window, app)
        }
        UiKind::Icon { icon, size, color } => {
            let color = style::color(color, cx, app);
            element.size(px(*size)).flex_none().when_some(icon.as_ref(), |element, icon| {
                element.child(svg().path(crate::ui::icons::path(&icon.name)).size(px(*size)).text_color(color))
            })
        }
        UiKind::Table { columns, rows, radius } => table(element, &node.id, columns, rows, radius, cx, window, app),
        UiKind::YouTube { url: Some(url), title, radius } => {
            super::web::youtube(element, &node.id, url, title, style::radius(radius), cx, window, app)
        }
        UiKind::Map { .. } => super::map::map(element, node, cx, window, app),
        _ => element,
    }
}

/// When a picture first showed in this box, to fade it in.
struct Shown {
    since: Option<Instant>,
    /// The picture was already there when the box first drew, so it doesn't fade in.
    at_once: Option<bool>,
}

#[allow(clippy::too_many_arguments)]
fn image(
    element: Stateful<Div>,
    id: &str,
    src: Option<&str>,
    aspect_ratio: f32,
    radius: &Radius,
    fit: Fit,
    photos: &Photos,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Stateful<Div> {
    let c = colors(app);
    let width = cx.width.max(1.);
    let max_pixels = (width.max(width / aspect_ratio) * window.scale_factor()).ceil() as u32;
    let picture = match (photos.state, src) {
        (PhotoState::Idle, Some(src)) => web_picture(id, src, max_pixels, cx, window, app),
        (PhotoState::Found, _) => photo(&photos.media, max_pixels, cx, app),
        (PhotoState::Waiting, _) => Picture::Loading,
        _ => Picture::Failed(crate::media::pictures::Failure::Decode),
    };
    let shown = window.use_keyed_state(cx.id(id, "shown"), app, |_, _| Shown { since: None, at_once: None });
    let ready = matches!(picture, Picture::Ready(_));
    if shown.read(app).at_once.is_none() {
        shown.update(app, |shown, _| shown.at_once = Some(ready));
    }
    if ready && shown.read(app).since.is_none() {
        shown.update(app, |shown, _| shown.since = Some(Instant::now()));
    }
    let opacity = match (shown.read(app).at_once, shown.read(app).since) {
        (Some(false), Some(since)) if !crate::media::native::reduces_motion() => {
            let progress = since.elapsed().as_secs_f32() / FADE_IN.as_secs_f32();
            if progress < 1. {
                window.request_animation_frame();
            }
            ease(progress.min(1.))
        }
        _ => 1.,
    };
    let radius = style::radius(radius);
    let fit = match fit {
        Fit::Cover => ObjectFit::Cover,
        Fit::Contain => ObjectFit::Contain,
    };
    let content = match picture {
        Picture::Ready(shown) => img(ImageSource::Render(shown.image))
            .size_full()
            .rounded(px(radius))
            .object_fit(fit)
            .opacity(opacity)
            .into_any_element(),
        Picture::Loading => {
            ShimmerBox::new(cx.id(id, "shimmer"), c.wash(c.foreground, false)).radius(radius).into_any_element()
        }
        Picture::Failed(_) => div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path(crate::ui::icons::path("image"))
                    .size(px(MISSING_SYMBOL))
                    .text_color(c.muted_stronger_foreground),
            )
            .into_any_element(),
    };
    element
        .relative()
        .w_full()
        .h(px(width / aspect_ratio.max(0.01)))
        .rounded(px(radius))
        .overflow_hidden()
        .bg(cx.surface.layer(Layer::Box, c))
        .child(content)
}

fn ease(progress: f32) -> f32 {
    progress * progress * (3. - 2. * progress)
}

/// The best photo core found that can be shown: the next when one fails.
fn photo(media: &[String], max_pixels: u32, cx: &UiCx, app: &mut App) -> Picture {
    let Some(store) = cx.store.clone() else { return Picture::Loading };
    let mut last = Picture::Failed(crate::media::pictures::Failure::Decode);
    for media in media {
        let key = format!("ui:{media}:{max_pixels}");
        match Pictures::load(&store, &key, media, max_pixels, app) {
            Picture::Failed(failure) => last = Picture::Failed(failure),
            picture => return picture,
        }
    }
    last
}

/// A picture at a web address, fetched once into the temporary folder; when it can't be had, core
/// is told so and looks for photos instead.
fn web_picture(id: &str, src: &str, max_pixels: u32, cx: &UiCx, window: &mut Window, app: &mut App) -> Picture {
    let mut hasher = DefaultHasher::new();
    src.hash(&mut hasher);
    let name = format!("{:016x}", hasher.finish());
    let file = std::env::temp_dir().join("motile-web-images").join(&name);
    let picture = Pictures::load_web(&format!("web:{name}:{max_pixels}"), src, &file, max_pixels, app);
    let Picture::Failed(_) = picture else { return picture };
    let told = window.use_keyed_state(cx.id(id, "fallback"), app, |_, _| false);
    if !*told.read(app) {
        told.update(app, |told, _| *told = true);
        let (sender, node_id) = (cx.clone(), id.to_string());
        app.defer(move |app| sender.send(&node_id, UiEvent::Fallback, app));
    }
    Picture::Loading
}

/// A table's padding: vertical and horizontal, for its header and its cells.
struct Padding {
    header: (f32, f32),
    cell: (f32, f32),
}

impl Padding {
    fn of(narrow: bool) -> Self {
        match narrow {
            true => Self { header: (8., 10.), cell: (8., 10.) },
            false => Self { header: (8., 12.), cell: (10., 12.) },
        }
    }
}

/// The table's column widths, as the automatic table layout shares `width` out: each column at
/// least its widest word and at most its widest cell on one line, with their padding.
fn column_widths(least: &[f32], most: &[f32], width: f32) -> Vec<f32> {
    let (sum_least, sum_most) = (least.iter().sum::<f32>(), most.iter().sum::<f32>());
    if sum_most <= width && sum_most > 0. {
        return most.iter().map(|most| most + (width - sum_most) * most / sum_most).collect();
    }
    if sum_least <= width {
        let count = least.len() as f32;
        let spread: f32 = least.iter().zip(most).map(|(least, most)| most - least).sum();
        return least
            .iter()
            .zip(most)
            .map(|(least, most)| match spread > 0. {
                true => least + (width - sum_least) * (most - least) / spread,
                false => least + (width - sum_least) / count,
            })
            .collect();
    }
    least.to_vec()
}

#[allow(clippy::too_many_arguments)]
fn table(
    element: Stateful<Div>,
    id: &str,
    columns: &[TableColumn],
    rows: &[Vec<TableCell>],
    radius: &Radius,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Stateful<Div> {
    let c = colors(app);
    let padding = Padding::of(cx.narrow());
    let header_type = LABEL;
    let cell_type = |numeric: bool| Type { digits: numeric, ..SMALL };
    let inner = cx.width - 2.;
    let mut least = Vec::with_capacity(columns.len());
    let mut most = Vec::with_capacity(columns.len());
    for (index, column) in columns.iter().enumerate() {
        // A header is one line: all of it is as unbreakable as a word.
        let header = header_type.width(&column.title, window) + 2. * padding.header.1;
        let (mut low, mut high) = (header, header);
        for row in rows {
            let (word, line) = match row.get(index) {
                Some(TableCell::Text { text }) => {
                    let ty = cell_type(column.numeric);
                    let line = ty.width(&text.text, window);
                    let word = if column.numeric { line } else { widest_word(&text.text, ty, window) };
                    (word, line)
                }
                Some(TableCell::Node { node }) => match node_width(node, cx.narrow(), window) {
                    Some(width) => (width, width),
                    None => (UNKNOWN_CELL.0, UNKNOWN_CELL.1),
                },
                None => (0., 0.),
            };
            low = low.max(word + 2. * padding.cell.1);
            high = high.max(line + 2. * padding.cell.1);
        }
        least.push(low.ceil());
        most.push(high.max(low).ceil());
    }
    let widths = column_widths(&least, &most, inner);
    let total: f32 = widths.iter().sum();
    let card_cx = cx.on(Surface::Card);
    // The lines lie inside the rows and the card, as they do on the Mac, taking no room.
    let line =
        |last: bool| {
            div().relative().flex().flex_row().items_stretch().when(!last, |row| {
                row.child(div().absolute().bottom_0().left_0().right_0().h(px(1.)).bg(c.border_card))
            })
        };
    let header = line(rows.is_empty()).children(columns.iter().zip(&widths).map(|(column, width)| {
        header_type
            .text_box()
            .w(px(*width))
            .flex_none()
            .py(px(padding.header.0))
            .px(px(padding.header.1))
            .text_color(c.foreground)
            .whitespace_nowrap()
            .overflow_hidden()
            .when(column.numeric, |cell| cell.text_right())
            .child(column.title.clone())
    }));
    let count = rows.len();
    let body: Vec<Div> = rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            line(row_index + 1 == count).children(columns.iter().zip(&widths).enumerate().map(
                |(index, (column, width))| {
                    let cell = div()
                        .w(px(*width))
                        .flex_none()
                        .py(px(padding.cell.0))
                        .px(px(padding.cell.1))
                        .flex()
                        .flex_col()
                        .justify_center()
                        .when(column.numeric, |cell| cell.items_end().text_right());
                    match row.get(index) {
                        Some(TableCell::Text { text }) => {
                            let look = Look::new(cell_type(column.numeric), c.foreground);
                            let words =
                                rich(cx.id(id, &format!("cell-{row_index}-{index}")), text, look, &card_cx, app);
                            cell.child(words.when(column.numeric, |words| words.whitespace_nowrap()))
                        }
                        Some(TableCell::Node { node: child }) => {
                            let inner_cx = card_cx.within(width - 2. * padding.cell.1);
                            let laid = node(child, &inner_cx, window, app)
                                .max_w_full()
                                .when(!column.numeric && super::layout::keeps_width(&child.kind), |laid| {
                                    laid.self_start()
                                });
                            cell.child(framed(&child.id, laid))
                        }
                        None => cell,
                    }
                },
            ))
        })
        .collect();
    let grid = div().w(px(total)).flex().flex_col().child(header).children(body);
    let radius = style::radius(radius);
    let header_fill = div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(header_type.line + 2. * padding.header.0))
        .rounded_t(px(radius))
        .bg(c.card_accent);
    element
        .w_full()
        .relative()
        .rounded(px(radius))
        .bg(c.card)
        .overflow_hidden()
        .child(header_fill)
        .child(div().id(cx.id(id, "scroll")).w_full().overflow_x_scroll().child(grid))
        .child(div().absolute().inset_0().rounded(px(radius)).border_1().border_color(c.border_card))
}

fn widest_word(text: &str, ty: Type, window: &Window) -> f32 {
    text.split_whitespace().map(|word| ty.width(word, window)).fold(0., f32::max)
}

/// A cell's component that has no width of its own counts this wide at least and at most.
const UNKNOWN_CELL: (f32, f32) = (120., 240.);

/// How wide a component in a cell is on one line: a text with its inline parts, a badge, an icon.
fn node_width(node: &UiNode, narrow: bool, window: &Window) -> Option<f32> {
    let words =
        |text: &str, style: TextStyle| style::type_of(style, narrow).width(&text.replace('\u{fffc}', ""), window);
    match &node.kind {
        UiKind::Badge { text, tone, icon } => {
            Some(super::text::badge_width(text, tone.is_some(), icon.is_some(), window))
        }
        UiKind::Icon { size, .. } => Some(*size),
        UiKind::Text { text, inlines, style, .. } => {
            let parts: f32 =
                inlines.iter().filter_map(|inline| node_width(inline, narrow, window)).map(|width| width + 2.).sum();
            Some(words(&text.text, *style) + parts)
        }
        UiKind::Title { text, style, .. } | UiKind::Caption { text, style, .. } => Some(words(&text.text, *style)),
        UiKind::Label { text, .. } => Some(style::LABEL.width(text, window)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::column_widths;

    #[test]
    fn columns_share_the_room_as_an_automatic_table_does() {
        assert_eq!(column_widths(&[20., 20.], &[100., 50.], 300.), vec![200., 100.]);
        assert_eq!(column_widths(&[40., 20.], &[100., 50.], 120.), vec![80., 40.]);
        assert_eq!(column_widths(&[40., 20.], &[100., 50.], 50.), vec![40., 20.]);
    }
}
