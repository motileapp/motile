//! The text components: Title, Text with the components it holds inline, Caption, Label, Badge,
//! Link, List and its items, Markdown, Quote and Code. Markdown and Code are the transcript's own
//! prose and code boxes.

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::openui::tree::{Glyph, ItemMarker, ListMarker, MarkdownBlock, Rich, UiKind, UiNode};

use super::style::{self, BODY, LABEL, Look, SMALL, Type};
use super::{UiCx, framed, node};
use crate::theme::{Colors, Layer, colors};
use crate::transcript::prose::{self, CodeBox};
use crate::transcript::rows::{self, code_box, open_link};
use crate::ui::rich_text::RichText;
use crate::ui::{Chip, CopyButton};

const LIST_ITEM_GAP: f32 = 10.;
const TIMELINE_STEP: f32 = 18.;
const DESCRIPTION_GAP: f32 = 2.;
const CHILDREN_GAP: f32 = 8.;
const ITEM_EXTRA: f32 = 4.;
const LINK_ARROW: f32 = 0.85;
const LINK_GAP: f32 = 2.;
const QUOTE_INDENT: f32 = 14.;
const CITE_GAP: f32 = 4.;
/// How far below the baseline an inline component's bottom lies, and how much more a badge is
/// raised, in text sizes.
const INLINE_DROP: f32 = 0.15;
const INLINE_BADGE_RAISE: f32 = 0.1;
const INLINE_SIDE: f32 = 1.;

pub fn text(node: &UiNode, cx: &UiCx, window: &mut Window, app: &mut App) -> Stateful<Div> {
    let element = div().id(cx.id(&node.id, "node"));
    let narrow = cx.narrow();
    let id = |part: &str| cx.id(&node.id, part);
    match &node.kind {
        UiKind::Title { text, style, color } => {
            let look = Look::new(style::type_of(*style, narrow), style::color(color, cx, app));
            element.child(rich(id("text"), text, look, cx, app))
        }
        UiKind::Text { text, inlines, style, color, weight, italic, strike, digits, max_lines } => {
            let mut ty = style::type_of(*style, narrow).weight(*weight);
            ty.digits |= *digits;
            let look = Look { ty, color: style::color(color, cx, app), italic: *italic, strike: *strike };
            let body = match inlines.is_empty() {
                true => rich(id("text"), text, look, cx, app),
                false => inlined(id("text"), text, inlines, look, cx, window, app),
            };
            element.child(body.when_some(*max_lines, |body, lines| body.line_clamp(lines as usize).text_ellipsis()))
        }
        UiKind::Caption { text, style, color } => {
            let look = Look::new(style::type_of(*style, narrow), style::color(color, cx, app));
            element.child(rich(id("text"), text, look, cx, app))
        }
        UiKind::Label { text, color } => {
            let look = Look::new(LABEL, style::color(color, cx, app));
            element.child(rich(id("text"), &plain(text), look, cx, app))
        }
        UiKind::Badge { text, tone, icon } => {
            element.flex_none().child(badge(text, tone.as_deref(), icon.as_ref(), cx, app))
        }
        UiKind::Link { text, url } => link(element, text, url.as_deref(), cx, app),
        UiKind::List { items, marker, timeline, wide } => {
            list(element, items, *marker, *timeline, *wide, cx, window, app)
        }
        UiKind::ListItem { .. } => list_item(
            element,
            node,
            Lane::of(ListMarker::Bullet, false, false, false, narrow),
            false,
            false,
            cx,
            window,
            app,
        ),
        UiKind::Markdown { blocks } => markdown(element, &node.id, blocks, cx, app),
        UiKind::Quote { text, cite } => {
            let c = colors(app);
            let words = rich(id("text"), text, Look::new(BODY, c.muted_foreground), cx, app);
            let cite = cite.as_ref().map(|cite| {
                rich(id("cite"), cite, Look::new(SMALL, c.muted_stronger_foreground), cx, app).mt(px(CITE_GAP))
            });
            element
                .relative()
                .pl(px(QUOTE_INDENT))
                .child(div().absolute().top_0().bottom_0().left(px(1.)).w(px(2.)).bg(c.border))
                .child(words)
                .children(cite)
        }
        UiKind::Code { language, code, spans, radius } => {
            let shown = CodeBox {
                language: language.clone(),
                code: code.clone().into(),
                spans: spans.as_ref().map(|spans| prose::code_spans(code, spans)),
                indent: 0.,
            };
            let key = format!("{}:{}", cx.row, node.id);
            let code = code_box(&key, &shown, true, cx.key, copy(&key, &shown.code, cx), cx.surface, app);
            element.child(code.rounded(px(style::radius(radius))).mb(px(0.)))
        }
        _ => element,
    }
}

fn plain(text: &str) -> Rich {
    Rich { text: text.to_string(), ..Rich::default() }
}

/// Text of the tree, selectable with the rest of the row, its links opening in the browser.
pub fn rich(id: ElementId, text: &Rich, look: Look, cx: &UiCx, app: &App) -> Div {
    look.ty.text_box().text_color(look.color).child(rich_text(id, text, look, cx, app))
}

fn rich_text(id: ElementId, text: &Rich, look: Look, cx: &UiCx, app: &App) -> RichText {
    let set = style::set(text, look, app);
    let tint = cx.surface.layer(Layer::Control, colors(app));
    RichText::with_runs(id, set.text, set.runs)
        .row(cx.key)
        .links(set.links, open_link)
        .tinted(set.code, tint)
        .tint_size(px(crate::fonts::inline_code_size(look.ty.size)))
}

fn copy(key: &str, code: &SharedString, cx: &UiCx) -> AnyElement {
    let code = code.to_string();
    CopyButton::new(SharedString::from(format!("copy-{key}")), "Copy code", move |_, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(code.clone()))
    })
    .surface(cx.surface.in_box())
    .into_any_element()
}

fn badge(text: &str, tone: Option<&str>, icon: Option<&Glyph>, cx: &UiCx, app: &App) -> Chip {
    let c = colors(app);
    let tone = tone.map(|tone| c.named(tone).unwrap_or(c.foreground));
    Chip::new(text.to_string())
        .surface(cx.surface)
        .when_some(tone, |chip, tone| chip.tone(tone))
        .when_some(icon, |chip, icon| chip.icon(icon.name.clone()))
}

/// How wide a badge is: Chip's padding, symbol and words, and its edge when it has no tone.
pub(super) fn badge_width(text: &str, tone: bool, icon: bool, window: &Window) -> f32 {
    let words = Type { size: 11.5, line: Chip::HEIGHT, weight: FontWeight::MEDIUM, digits: false, mono: false };
    let symbol = if icon { Chip::SYMBOL.side() + 5. } else { 0. };
    let edge = if tone { 0. } else { 2. };
    7. * 2. + edge + symbol + words.width(text, window)
}

fn link(element: Stateful<Div>, text: &str, url: Option<&str>, cx: &UiCx, app: &App) -> Stateful<Div> {
    let c = colors(app);
    let Some(url) = url else {
        return element.child(rich(cx.id(text, "plain"), &plain(text), Look::new(BODY, c.foreground), cx, app));
    };
    let url = url.to_string();
    let arrow = BODY.size * LINK_ARROW;
    element
        .flex()
        .flex_row()
        .items_center()
        .gap(px(LINK_GAP))
        .cursor_pointer()
        .text_color(c.primary)
        .child(BODY.text_box().child(text.to_string()))
        .child(svg().path(crate::ui::icons::path("arrow-up-right")).size(px(arrow)).flex_none().text_color(c.primary))
        .on_click(move |_, _, cx| cx.open_url(&url))
}

/// A Text whose runs hold components: each is laid in its place, as wide as it is, on the line.
#[allow(clippy::too_many_arguments)]
fn inlined(
    id: ElementId,
    text: &Rich,
    inlines: &[std::sync::Arc<UiNode>],
    look: Look,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Div {
    let size = look.ty.size;
    let inline_cx = UiCx { text_size: size, ..cx.clone() };
    let boxes: Vec<(f32, f32, f32)> =
        inlines.iter().map(|inline| inline_size(inline, size, &inline_cx, window, app)).collect();
    let tallest = boxes.iter().map(|(_, height, raise)| height + raise + size * INLINE_DROP).fold(0., f32::max);
    let line = look.ty.line.max((tallest + 4.).ceil());
    let look = Look { ty: Type { line, ..look.ty }, ..look };
    let spacers: Vec<String> =
        boxes.iter().map(|(width, ..)| spacer(width + 2. * INLINE_SIDE, look.ty, window)).collect();
    let (shown, starts) = with_spacers(text, &spacers);
    let text = rich_text(id, &shown, look, cx, app);
    let layout = text.text_layout();
    let placed: Vec<AnyElement> = inlines
        .iter()
        .zip(&boxes)
        .zip(starts)
        .map(|((inline, (_, height, raise)), start)| {
            let at = Placed::new(layout.clone(), start, *height + raise, size, line);
            let child = node(inline, &inline_cx, window, app);
            at.child(framed(&inline.id, child)).into_any_element()
        })
        .collect();
    look.ty.text_box().text_color(look.color).relative().child(text).children(placed)
}

/// An inline component's width and height, and how much higher than the others it sits.
fn inline_size(inline: &UiNode, size: f32, cx: &UiCx, window: &mut Window, app: &mut App) -> (f32, f32, f32) {
    match &inline.kind {
        UiKind::Math { canvas } => {
            let Some(prepared) = super::drawing::scene(&inline.id, canvas, cx.width, size, cx, window, app) else {
                return (0., 0., 0.);
            };
            let scene = &prepared.scene;
            let baseline = scene.baseline.unwrap_or(scene.height);
            (scene.width, scene.height, baseline + size * INLINE_DROP - scene.height)
        }
        UiKind::Icon { size: side, icon, .. } => (if icon.is_some() { *side } else { 0. }, *side, 0.),
        UiKind::Badge { text, tone, icon } => {
            (badge_width(text, tone.is_some(), icon.is_some(), window), Chip::HEIGHT, size * INLINE_BADGE_RAISE)
        }
        _ => (0., 0., 0.),
    }
}

/// The text with each object character in place of a component replaced by spaces that don't
/// break, `spacers` in order, and where each starts in the new text, by bytes.
fn with_spacers(text: &Rich, spacers: &[String]) -> (Rich, Vec<usize>) {
    const OBJECT: u16 = 0xfffc;
    let units: Vec<u16> = text.text.encode_utf16().collect();
    let mut shown: Vec<u16> = Vec::with_capacity(units.len());
    let mut moved = Vec::with_capacity(units.len() + 1);
    let mut starts = Vec::new();
    let mut objects = spacers.iter();
    for unit in &units {
        moved.push(shown.len());
        match (*unit == OBJECT).then(|| objects.next()).flatten() {
            Some(spacer) => {
                starts.push(shown.len());
                shown.extend(spacer.encode_utf16());
            }
            None => shown.push(*unit),
        }
    }
    moved.push(shown.len());
    let at = |unit: u32| moved.get(unit as usize).copied().unwrap_or(shown.len()) as u32;
    let runs = text.runs.iter().map(|[start, len, bits]| [at(*start), at(start + len) - at(*start), *bits]).collect();
    let links = text
        .links
        .iter()
        .map(|link| motile_core::render::markdown::Link {
            start: at(link.start),
            len: at(link.start + link.len) - at(link.start),
            url: link.url.clone(),
        })
        .collect();
    let string = String::from_utf16_lossy(&shown);
    let bytes = |unit: usize| {
        let mut count = 0;
        for (index, character) in string.char_indices() {
            if count >= unit {
                return index;
            }
            count += character.len_utf16();
        }
        string.len()
    };
    let starts = starts.into_iter().map(bytes).collect();
    (Rich { text: string, runs, links }, starts)
}

/// Spaces that don't break, as near `width` wide in text of `ty` as they come.
fn spacer(width: f32, ty: Type, window: &Window) -> String {
    let (wide, narrow) = ('\u{a0}', '\u{202f}');
    let (wide_width, narrow_width) =
        (ty.width(&wide.to_string(), window).max(0.1), ty.width(&narrow.to_string(), window).max(0.1));
    let wides = (width / wide_width).floor().max(0.) as usize;
    let narrows = ((width - wides as f32 * wide_width) / narrow_width).round().max(0.) as usize;
    std::iter::repeat_n(wide, wides).chain(std::iter::repeat_n(narrow, narrows)).collect()
}

/// A component laid where a run of text left room for it: its bottom `drop` text sizes under the
/// line's baseline.
struct Placed {
    layout: TextLayout,
    start: usize,
    height: f32,
    size: f32,
    line: f32,
    child: Option<AnyElement>,
}

impl Placed {
    fn new(layout: TextLayout, start: usize, height: f32, size: f32, line: f32) -> Self {
        Self { layout, start, height, size, line, child: None }
    }

    fn child(mut self, child: AnyElement) -> Self {
        self.child = Some(child);
        self
    }
}

impl IntoElement for Placed {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Placed {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            position: Position::Absolute,
            inset: Edges { top: px(0.).into(), left: px(0.).into(), ..Style::default().inset },
            ..Style::default()
        };
        let children: Vec<LayoutId> = self.child.iter_mut().map(|child| child.request_layout(window, cx)).collect();
        (window.request_layout(style, children, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(child) = &mut self.child else { return };
        let Some(at) = self.layout.position_for_index(self.start) else {
            child.prepaint(window, cx);
            return;
        };
        let baseline = self.line / 2. + self.size * 0.35;
        let top = baseline + self.size * INLINE_DROP - self.height;
        let target = point(at.x + px(INLINE_SIDE), at.y + px(top));
        window.with_element_offset(target - bounds.origin, |window| child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = &mut self.child {
            child.paint(window, cx);
        }
    }
}

/// The lane a list item's marker takes and the room between it and the item's text.
#[derive(Clone, Copy)]
struct Lane {
    width: f32,
    gap: f32,
}

impl Lane {
    fn of(marker: ListMarker, timeline: bool, wide: bool, chip: bool, narrow: bool) -> Self {
        let (width, gap) = match (wide, marker) {
            (true, _) if narrow => (64., 20.),
            (true, _) => (76., if timeline { 24. } else { 16. }),
            _ if chip => (24., 12.),
            (false, ListMarker::Number) => (24., 12.),
            (false, ListMarker::Bullet) if timeline => (24., 12.),
            (false, ListMarker::Bullet) => (14., 8.),
            (false, ListMarker::Plain) => (0., 0.),
        };
        Self { width, gap }
    }
}

#[allow(clippy::too_many_arguments)]
fn list(
    element: Stateful<Div>,
    items: &[std::sync::Arc<UiNode>],
    marker: ListMarker,
    timeline: bool,
    wide: bool,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Stateful<Div> {
    let narrow = cx.narrow();
    let chips = items.iter().any(|item| matches!(&item.kind, UiKind::ListItem { marker: ItemMarker::Chip { .. }, .. }));
    let lane = Lane::of(marker, timeline, wide, chips && !wide, narrow);
    let count = items.len();
    let laid: Vec<AnyElement> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let last = index + 1 == count;
            let element = div().id(cx.id(&item.id, "node"));
            let joined = timeline && (marker != ListMarker::Plain || wide);
            let element = list_item(element, item, lane, joined, last, cx, window, app);
            let element = match timeline {
                true => element.when(!last, |item| item.pb(px(TIMELINE_STEP))),
                false => element.when(index > 0, |item| item.mt(px(LIST_ITEM_GAP))),
            };
            framed(&item.id, super::accessible(element, item.a11y.as_ref()))
        })
        .collect();
    element.flex().flex_col().children(laid)
}

#[allow(clippy::too_many_arguments)]
fn list_item(
    element: Stateful<Div>,
    item: &UiNode,
    lane: Lane,
    joined: bool,
    last: bool,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Stateful<Div> {
    let UiKind::ListItem { title, style, medium, description, marker, children } = &item.kind else {
        return element;
    };
    let c = colors(app);
    let narrow = cx.narrow();
    let mut ty = style::type_of(*style, narrow);
    if *medium {
        ty.weight = FontWeight::MEDIUM;
    }
    let size = ty.size;
    let body_width = cx.width - lane.width - lane.gap;
    let body_cx = cx.within(body_width);
    let id = |part: &str| cx.id(&item.id, part);
    let title = rich(id("title"), title, Look::new(ty, c.foreground), cx, app);
    let description = description.as_ref().map(|text| {
        rich(id("description"), text, Look::new(SMALL, c.muted_foreground), cx, app).mt(px(DESCRIPTION_GAP))
    });
    let nested: Vec<AnyElement> =
        children.iter().map(|child| framed(&child.id, node(child, &body_cx, window, app))).collect();
    let extra = description.is_some() || !nested.is_empty();
    let body = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(title)
        .children(description)
        .when(!nested.is_empty(), |body| body.child(div().mt(px(CHILDREN_GAP)).flex().flex_col().children(nested)))
        .when(extra, |body| body.pb(px(ITEM_EXTRA)));
    let wide = lane.width > 30.;
    let mark = item_marker(marker, ty, lane, joined, wide, cx, c);
    let thread = (joined && !last).then(|| {
        let centre = match wide {
            true => lane.width + if narrow { 9.5 } else { 7.5 },
            false => 12.,
        };
        div()
            .absolute()
            .left(px(centre - 0.5))
            .w(px(1.))
            .top(px(1.7 * size))
            .bottom(px(-TIMELINE_STEP + 0.15 * size))
            .bg(c.border_accent)
    });
    element
        .relative()
        .flex()
        .flex_row()
        .items_start()
        .children(thread)
        .when(lane.width > 0., |row| row.child(div().w(px(lane.width)).flex_none().relative().child(mark)))
        .when(lane.gap > 0., |row| {
            row.child(
                div()
                    .w(px(lane.gap))
                    .flex_none()
                    .relative()
                    .children(wide.then(|| ring_after_lane(joined, ty, narrow, cx, c)).flatten()),
            )
        })
        .child(body)
}

/// A wide timeline's ring, a little after the lane of text markers.
fn ring_after_lane(joined: bool, ty: Type, narrow: bool, cx: &UiCx, c: &Colors) -> Option<Div> {
    joined.then(|| ring(cx, c).absolute().left(px(if narrow { 6. } else { 4. })).top(px(0.55 * ty.size)))
}

fn ring(cx: &UiCx, c: &Colors) -> Div {
    div().size(px(9.)).rounded_full().bg(cx.surface.color(c)).border_2().border_color(c.foreground)
}

#[allow(clippy::too_many_arguments)]
fn item_marker(
    marker: &ItemMarker,
    ty: Type,
    lane: Lane,
    joined: bool,
    wide: bool,
    cx: &UiCx,
    c: &Colors,
) -> AnyElement {
    let first_line = ty.line;
    let centred = |side: f32| div().absolute().top(px((first_line - side) / 2.)).left(px((lane.width - side) / 2.));
    let surface = cx.surface.color(c);
    match marker {
        ItemMarker::Blank => div().into_any_element(),
        ItemMarker::Dot if joined && !wide => {
            ring(cx, c).absolute().top(px((first_line - 9.) / 2.)).left(px(12. - 4.5)).into_any_element()
        }
        ItemMarker::Dot => centred(5.).size(px(5.)).rounded_full().bg(c.muted_stronger_foreground).into_any_element(),
        ItemMarker::Number { number } => centred(24.)
            .size(px(24.))
            .rounded_full()
            .bg(c.foreground)
            .text_color(surface)
            .flex()
            .items_center()
            .justify_center()
            .child(
                Type { size: 13., line: 24., weight: FontWeight::MEDIUM, digits: true, mono: false }
                    .text_box()
                    .child(number.to_string()),
            )
            .into_any_element(),
        ItemMarker::Chip { text } => div()
            .absolute()
            .top(px((first_line - 20.) / 2.))
            .left_0()
            .h(px(20.))
            .min_w(px(20.))
            .px(px(5.))
            .rounded_full()
            .bg(c.foreground)
            .text_color(surface)
            .flex()
            .items_center()
            .justify_center()
            .child(
                Type { size: 12., line: 20., weight: FontWeight::MEDIUM, digits: true, mono: false }
                    .text_box()
                    .child(text.clone()),
            )
            .into_any_element(),
        ItemMarker::Label { text } => {
            Type { size: 13., line: first_line, weight: FontWeight::NORMAL, digits: true, mono: false }
                .text_box()
                .w(px(lane.width))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_color(c.muted_foreground)
                .child(text.clone())
                .into_any_element()
        }
    }
}

fn markdown(element: Stateful<Div>, id: &str, blocks: &[MarkdownBlock], cx: &UiCx, app: &App) -> Stateful<Div> {
    let parts: Vec<AnyElement> = blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let key = format!("{}:{id}:{index}", cx.row);
            match block {
                MarkdownBlock::Prose { prose: text } => {
                    let prepared = prose::prepare(text);
                    let copy = |id: &str, code: &SharedString| copy(id, code, cx);
                    rows::prose_blocks(&key, &prepared, cx.key, &copy, cx.surface, app).into_any_element()
                }
                MarkdownBlock::Code { language, code, spans } => {
                    let shown = CodeBox {
                        language: language.clone(),
                        code: code.clone().into(),
                        spans: spans.as_ref().map(|spans| prose::code_spans(code, spans)),
                        indent: 0.,
                    };
                    code_box(&key, &shown, true, cx.key, copy(&key, &shown.code, cx), cx.surface, app)
                        .into_any_element()
                }
            }
        })
        .collect();
    element.flex().flex_col().text_color(colors(app).muted_foreground).children(parts)
}
