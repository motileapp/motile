//! The containers: Col, Row, Box, Grid, Card, Carousel and Popover, and Divider and Spacer.

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::openui::host::UiEvent;
use motile_core::render::openui::tree::{
    Align, Basis, Direction, Justify, Length, LengthUnit, Sizing, Slot, UiKind, UiNode, grid_columns,
};

use super::style::{self, Look};
use super::{UiCx, framed, node};
use crate::theme::{Layer, Radius, Surface, colors};
use crate::ui::GlassButton;

const CARD_PADDING: f32 = 20.;
const NARROW_CARD_PADDING: f32 = 16.;
const CAROUSEL_SHARE: f32 = 0.85;
const CAROUSEL_BLEED: f32 = 4.;
const CAROUSEL_BUTTON_INSET: f32 = 8.;
const CAROUSEL_PAGING: Duration = Duration::from_millis(320);
const POPOVER_WIDTH: f32 = 280.;
const POPOVER_MARGIN: f32 = 12.;

pub fn layout(node: &UiNode, cx: &UiCx, window: &mut Window, app: &mut App) -> Stateful<Div> {
    let element = div().id(cx.id(&node.id, "node"));
    match &node.kind {
        UiKind::Col { children, align, spacing } => column(element, children, *align, spacing, false, cx, window, app),
        UiKind::Row { children, gap, align, justify, wrap, sizing } => {
            let line = Line { gap: *gap, align: *align, justify: *justify, wrap: *wrap };
            row(element, children, sizing, line, cx, window, app)
        }
        UiKind::Box { .. } => boxed(element, node, cx, window, app),
        UiKind::Grid { children, columns, min_child_width, gap } => {
            grid(element, children, *columns, *min_child_width, *gap, cx, window, app)
        }
        UiKind::Card { children, soft, nested, padding, spacing } => {
            let c = colors(app);
            let padding = padding.unwrap_or(if cx.narrow() { NARROW_CARD_PADDING } else { CARD_PADDING });
            let radius = if *nested { Radius::MD } else { Radius::LG };
            let (inner, border) = match soft {
                true => (cx.surface.in_box(), 0.),
                false => (Surface::Card, 1.),
            };
            let inner_cx = cx.on(inner).within(cx.width - 2. * (padding + border));
            let element = element
                .p(px(padding))
                .rounded(px(radius))
                .when(*soft, |card| card.bg(cx.surface.layer(Layer::Box, c)))
                .when(!soft, |card| card.bg(c.card).border_1().border_color(c.border_card));
            column(element, children, Align::Stretch, spacing, false, &inner_cx, window, app)
        }
        UiKind::Carousel { children, item_width, gap, fill } => {
            carousel(element, &node.id, children, *item_width, *gap, fill, cx, window, app)
        }
        UiKind::Popover { trigger, content, open } => {
            popover(element, &node.id, trigger, content, *open, cx, window, app)
        }
        UiKind::Divider {} => element.h(px(1.)).w_full().flex_none().bg(cx.surface.border(colors(app))),
        _ => element,
    }
}

/// Whether a node keeps its own width in a column that stretches its children.
pub(super) fn keeps_width(kind: &UiKind) -> bool {
    match kind {
        UiKind::Button { block, .. } | UiKind::SegmentedControl { block, .. } => !block,
        UiKind::IconButton { .. } | UiKind::Badge { .. } | UiKind::Icon { .. } | UiKind::Link { .. } => true,
        _ => false,
    }
}

/// Children stacked, each `spacing` below the one before.
#[allow(clippy::too_many_arguments)]
pub fn column(
    element: Stateful<Div>,
    children: &[Arc<UiNode>],
    align: Align,
    spacing: &[f32],
    fixed_height: bool,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Stateful<Div> {
    let element = element.flex().flex_col();
    let element = match align {
        Align::Stretch => element.items_stretch(),
        Align::Start => element.items_start(),
        Align::Center => element.items_center().text_center(),
        Align::End => element.items_end().text_right(),
    };
    let laid: Vec<AnyElement> = children
        .iter()
        .enumerate()
        .map(|(index, child)| {
            let space = spacing.get(index).copied().unwrap_or(0.);
            let flex = match &child.kind {
                UiKind::Box { flex: Some(flex), .. } if fixed_height => Some(*flex),
                _ => None,
            };
            let laid = node(child, cx, window, app)
                .max_w_full()
                .when(space > 0., |child| child.mt(px(space)))
                .when(align == Align::Stretch && keeps_width(&child.kind), |child| child.self_start())
                .when_some(flex, |child, flex| grown(child.flex_basis(px(0.)).min_h_0(), flex));
            framed(&child.id, laid)
        })
        .collect();
    element.children(laid)
}

fn grown(element: Stateful<Div>, grow: f32) -> Stateful<Div> {
    element.flex_grow(grow).flex_shrink(0.)
}

/// How a row lays its children out.
#[derive(Clone, Copy)]
pub struct Line {
    pub gap: f32,
    pub align: Align,
    pub justify: Justify,
    pub wrap: bool,
}

/// Children side by side, each sized as `sizing` says.
pub fn row(
    element: Stateful<Div>,
    children: &[Arc<UiNode>],
    sizing: &[Sizing],
    line: Line,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Stateful<Div> {
    let element = element.flex().flex_row().gap(px(line.gap)).when(line.wrap, |row| row.flex_wrap());
    let element = aligned(element, line.align);
    let element = justified(element, line.justify);
    let laid: Vec<AnyElement> = children
        .iter()
        .enumerate()
        .map(|(index, child)| {
            let sizing = sizing.get(index).copied().unwrap_or(Sizing {
                basis: Basis::Content,
                length: None,
                grow: 0.,
                shrink: true,
            });
            let width = basis_width(&sizing, cx);
            let laid = sized(node(child, &cx.within(width.unwrap_or(cx.width)), window, app), &sizing, width);
            framed(&child.id, laid)
        })
        .collect();
    element.children(laid)
}

fn aligned(element: Stateful<Div>, align: Align) -> Stateful<Div> {
    match align {
        Align::Start => element.items_start(),
        Align::Center => element.items_center(),
        Align::End => element.items_end(),
        Align::Stretch => element.items_stretch(),
    }
}

fn justified(element: Stateful<Div>, justify: Justify) -> Stateful<Div> {
    match justify {
        Justify::Start => element.justify_start(),
        Justify::Center => element.justify_center(),
        Justify::End => element.justify_end(),
        Justify::Between => element.justify_between(),
    }
}

/// The width a row's child starts at when it is known before laying out: a length, or a media
/// row's image.
fn basis_width(sizing: &Sizing, cx: &UiCx) -> Option<f32> {
    match sizing.basis {
        Basis::Length => sizing.length.map(|length| points(length, cx)),
        Basis::Media => Some((cx.width / 3.).clamp(120., 200.)),
        Basis::Content | Basis::Zero | Basis::Row => None,
    }
}

fn sized(element: Stateful<Div>, sizing: &Sizing, width: Option<f32>) -> Stateful<Div> {
    let element = match (sizing.basis, width) {
        (Basis::Zero, _) => grown(element.flex_basis(px(0.)), sizing.grow),
        (Basis::Row, _) => element.flex_basis(relative(1.)),
        (Basis::Length | Basis::Media, Some(width)) => element.w(px(width)).flex_basis(px(width)),
        _ => element,
    };
    match sizing.shrink {
        true => element.flex_shrink(1.).min_w_0(),
        false => element.flex_shrink_0(),
    }
}

/// A length in points; a fraction is of `cx.width`.
fn points(length: Length, cx: &UiCx) -> f32 {
    match length.unit {
        LengthUnit::Points => length.value,
        LengthUnit::Fraction => length.value * cx.width,
        LengthUnit::Column => length.value * cx.column,
        LengthUnit::Window => length.value * cx.window_height,
    }
}

fn boxed(element: Stateful<Div>, node: &UiNode, cx: &UiCx, window: &mut Window, app: &mut App) -> Stateful<Div> {
    let UiKind::Box {
        children,
        direction,
        gap,
        spacing,
        sizing,
        padding,
        background,
        inner_surface,
        radius,
        border,
        width,
        height,
        flex: _,
        align,
        justify,
        wrap,
    } = &node.kind
    else {
        return element;
    };
    let c = colors(app);
    let own_width = width.map(|width| points(width, cx)).unwrap_or(cx.width).min(cx.width);
    let edge = if *border { 1. } else { 0. };
    let inner = if *inner_surface { cx.surface.in_box() } else { cx.surface };
    let inner_cx = cx.on(inner).within(own_width - 2. * (padding + edge));
    let fixed_height = matches!(height, Some(Length { unit: LengthUnit::Points | LengthUnit::Window, .. }));
    let element = element
        .p(px(*padding))
        .when_some(background.as_ref(), |element, background| element.bg(style::color(background, cx, app)))
        .when(*border, |element| element.border_1().border_color(cx.surface.border(c)))
        .when_some(radius.as_ref(), |element, radius| element.rounded(px(style::radius(radius))).overflow_hidden())
        .when_some(*width, |element, width| match width.unit {
            LengthUnit::Fraction => element.w(relative(width.value)),
            _ => element.w(px(points(width, cx))),
        })
        .when_some(*height, |element, height| match height.unit {
            LengthUnit::Fraction => element.h(relative(height.value)),
            LengthUnit::Column => element.h(px(height.value * cx.column)),
            LengthUnit::Window => element.h(px(height.value * cx.window_height)),
            LengthUnit::Points => element.h(px(height.value)),
        });
    match direction {
        Direction::Col => {
            let element = justified(element, *justify);
            column(element, children, *align, spacing, fixed_height, &inner_cx, window, app)
        }
        Direction::Row => {
            let line = Line { gap: *gap, align: *align, justify: *justify, wrap: *wrap };
            row(element, children, sizing, line, &inner_cx, window, app)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn grid(
    element: Stateful<Div>,
    children: &[Arc<UiNode>],
    columns: u32,
    min_child_width: Option<f32>,
    gap: f32,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Stateful<Div> {
    let count = grid_columns(columns, min_child_width, gap, cx.width).max(1) as usize;
    let cell = (cx.width - (count - 1) as f32 * gap) / count as f32;
    let cell_cx = cx.within(cell);
    let lines: Vec<Div> = children
        .chunks(count)
        .map(|cells| {
            let mut line = div().flex().flex_row().items_stretch().gap(px(gap));
            for index in 0..count {
                let cell = match cells.get(index) {
                    Some(child) => {
                        let laid = grown(node(child, &cell_cx, window, app).flex_basis(px(0.)), 1.).min_w_0();
                        framed(&child.id, laid)
                    }
                    None => div().flex_basis(px(0.)).flex_grow(1.).into_any_element(),
                };
                line = line.child(cell);
            }
            line
        })
        .collect();
    element.flex().flex_col().gap(px(gap)).children(lines)
}

/// Where a carousel's track is, and the paging under way.
#[derive(Default)]
struct Track {
    scroll: ScrollHandle,
    paging: Option<Paging>,
}

#[derive(Clone, Copy)]
struct Paging {
    from: f32,
    to: f32,
    since: Instant,
}

#[allow(clippy::too_many_arguments)]
fn carousel(
    element: Stateful<Div>,
    id: &str,
    children: &[Arc<UiNode>],
    item_width: f32,
    gap: f32,
    fill: &[bool],
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Stateful<Div> {
    let item = item_width.clamp(80., 640.).min(cx.width * CAROUSEL_SHARE);
    let track = window.use_keyed_state(cx.id(id, "track"), app, |_, _| Track::default());
    step_paging(&track, window, app);
    let item_cx = cx.within(item);
    let items: Vec<AnyElement> = children
        .iter()
        .enumerate()
        .map(|(index, child)| {
            let fills = fill.get(index).copied().unwrap_or(false);
            let laid =
                node(child, &item_cx, window, app).w(px(item)).flex_none().when(fills, |item| item.self_stretch());
            framed(&child.id, laid)
        })
        .collect();
    let scroll = track.read(app).scroll.clone();
    let strip = div()
        .id(cx.id(id, "strip"))
        .py(px(CAROUSEL_BLEED))
        .my(px(-CAROUSEL_BLEED))
        .overflow_x_scroll()
        .track_scroll(&scroll)
        .flex()
        .flex_row()
        .items_start()
        .gap(px(gap))
        .children(items);
    let offset = -f32::from(scroll.offset().x);
    let end = f32::from(scroll.max_offset().x);
    let edges: Vec<f32> = (0..children.len()).map(|index| index as f32 * (item + gap)).collect();
    let button = |forward: bool| {
        let (track, edges) = (track.clone(), edges.clone());
        let symbol = if forward { "chevron-right" } else { "chevron-left" };
        let help = if forward { "Next" } else { "Previous" };
        let side = div().absolute().top_0().bottom_0().flex().items_center();
        let side = if forward { side.right(px(CAROUSEL_BUTTON_INSET)) } else { side.left(px(CAROUSEL_BUTTON_INSET)) };
        side.child(GlassButton::new(cx.id(id, symbol), symbol, help).on_click(move |_, window, cx| {
            page(&track, &edges, forward, window, cx);
        }))
    };
    element
        .relative()
        .child(strip)
        .when(offset > CAROUSEL_BLEED, |carousel| carousel.child(button(false)))
        .when(end - offset > CAROUSEL_BLEED, |carousel| carousel.child(button(true)))
}

/// Moves the track by most of what shows, to the edge of the item that then leads.
fn page(track: &Entity<Track>, edges: &[f32], forward: bool, window: &mut Window, cx: &mut App) {
    let scroll = track.read(cx).scroll.clone();
    let shown = f32::from(scroll.bounds().size.width);
    let from = -f32::from(scroll.offset().x);
    let end = f32::from(scroll.max_offset().x);
    let goal = if forward { from + shown * CAROUSEL_SHARE } else { from - shown * CAROUSEL_SHARE };
    let snapped = match forward {
        true => edges.iter().copied().rfind(|edge| *edge > from + 1. && *edge <= goal),
        false => edges.iter().copied().find(|edge| *edge >= goal && *edge < from - 1.),
    };
    let to = snapped.unwrap_or(goal).clamp(0., end.max(0.));
    if crate::media::native::reduces_motion() {
        scroll.set_offset(point(px(-to), px(0.)));
    } else {
        track.update(cx, |track, _| track.paging = Some(Paging { from, to, since: Instant::now() }));
    }
    window.refresh();
}

fn step_paging(track: &Entity<Track>, window: &mut Window, cx: &mut App) {
    let Some(paging) = track.read(cx).paging else { return };
    let progress = (paging.since.elapsed().as_secs_f32() / CAROUSEL_PAGING.as_secs_f32()).min(1.);
    let at = paging.from + (paging.to - paging.from) * crate::ui::ease_out(progress);
    track.read(cx).scroll.set_offset(point(px(-at), px(0.)));
    if progress >= 1. {
        track.update(cx, |track, _| track.paging = None);
    } else {
        window.request_animation_frame();
    }
}

/// A slot's text or components, its text in `color`.
pub fn slot(slot: &Slot, id: &str, color: Hsla, cx: &UiCx, window: &mut Window, app: &mut App) -> Vec<AnyElement> {
    let mut parts = Vec::new();
    if let Some(text) = &slot.text {
        let ty = style::type_of(slot.style, cx.narrow());
        let look = Look::new(ty, color);
        parts.push(super::text::rich(cx.id(id, "text"), text, look, cx, app).into_any_element());
    }
    for child in &slot.children {
        parts.push(framed(&child.id, node(child, cx, window, app)));
    }
    parts
}

#[allow(clippy::too_many_arguments)]
fn popover(
    element: Stateful<Div>,
    id: &str,
    trigger: &Slot,
    content: &Slot,
    open: bool,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Stateful<Div> {
    let c = colors(app);
    let hovered = window.use_keyed_state(cx.id(id, "hovered"), app, |_, _| false);
    let lit = *hovered.read(app);
    let ink = if lit { c.muted_foreground } else { c.foreground };
    let parts = window
        .with_text_style(Some(TextStyleRefinement { color: Some(ink), ..Default::default() }), |window| {
            slot(trigger, &format!("{id}/trigger"), ink, cx, window, app)
        });
    let (node_id, toggles) = (id.to_string(), cx.clone());
    let focus = window.use_keyed_state(cx.id(id, "focus"), app, |_, cx| cx.focus_handle()).read(app).clone();
    let trigger = window.use_keyed_state(cx.id(id, "trigger"), app, |_, _| None);
    let was_open = window.use_keyed_state(cx.id(id, "was-open"), app, |_, _| false);
    if open && !*was_open.read(app) {
        window.focus(&focus, app);
    }
    was_open.update(app, |was, _| *was = open);
    let panel = open.then(|| {
        let width = POPOVER_WIDTH.min(f32::from(window.viewport_size().width) - 2. * POPOVER_MARGIN);
        let inner_cx = cx.on(Surface::Popover).within(width - 28.);
        let parts = window
            .with_text_style(Some(TextStyleRefinement { color: Some(c.foreground), ..Default::default() }), |window| {
                slot(content, &format!("{id}/content"), c.foreground, &inner_cx, window, app)
            });
        let closes = cx.clone();
        let close_id = id.to_string();
        let close =
            Rc::new(move |_: &mut Window, cx: &mut App| closes.send(&close_id, UiEvent::Expand { open: false }, cx));
        let card = crate::ui::popover_panel(cx.id(id, "panel"), width, &focus, close, app).children(parts);
        crate::ui::anchored_popover(*trigger.read(app), card, window)
    });
    element
        .relative()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.))
        .cursor_pointer()
        .text_color(ink)
        .aria_expanded(open)
        .children(parts)
        .on_hover(move |hover, _, cx| {
            hovered.update(cx, |lit, cx| {
                *lit = *hover;
                cx.notify();
            })
        })
        .on_click(move |_, _, cx| toggles.send(&node_id, UiEvent::Expand { open: !open }, cx))
        .child(crate::ui::tracks_bounds(trigger))
        .children(panel)
}
