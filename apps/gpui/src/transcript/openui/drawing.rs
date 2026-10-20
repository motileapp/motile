//! What core lays out as a canvas scene and the canvas painters draw: Chart, Math and Svg with its
//! shapes, Flashcard's flip and Celebration's confetti; and HtmlView, in `web`.

use std::sync::Arc;
use std::time::Instant;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::openui::canvas::Motion;
use motile_core::render::openui::host::UiEvent;
use motile_core::render::openui::tree::{Canvas, Slot, UiKind, UiNode};

use super::UiCx;
use super::layout::slot;
use super::style;
use crate::theme::{Layer, colors};
use crate::transcript::canvas::{ConfettiOverlay, Prepared, SceneView, laid_out, squashed, turned};

const MATH_MARGIN: f32 = 4.;
const CHART_HEIGHT: f32 = 240.;
const CARD_MIN_HEIGHT: f32 = 168.;
const FACE_PADDING: (f32, f32, f32) = (20., 18., 30.);
const FACE_GAP: f32 = 8.;
const HINT_BOTTOM: f32 = 10.;

pub fn drawing(node: &UiNode, cx: &UiCx, window: &mut Window, app: &mut App) -> Stateful<Div> {
    let element = div().id(cx.id(&node.id, "node"));
    let id = node.id.as_str();
    match &node.kind {
        UiKind::Chart { canvas } => {
            let c = colors(app);
            match scene(id, canvas, cx.width, cx.text_size, cx, window, app) {
                Some(prepared) => element.w_full().child(
                    SceneView::new(cx.id(id, "chart"), prepared, cx.width).surface(cx.surface).current(c.foreground),
                ),
                None => {
                    let height = canvas.props.get("height").and_then(|height| height.as_number());
                    element.w_full().h(px(height.map_or(CHART_HEIGHT, |height| height.clamp(140., 480.) as f32)))
                }
            }
        }
        UiKind::Math { canvas } => {
            let c = colors(app);
            let Some(prepared) = scene(id, canvas, cx.width, cx.text_size, cx, window, app) else {
                return element;
            };
            let width = prepared.scene.width;
            let view = SceneView::new(cx.id(id, "math"), prepared, width).surface(cx.surface).current(c.foreground);
            match inline(canvas) {
                true => element.child(view),
                false => element.w_full().py(px(MATH_MARGIN)).child(
                    div().id(cx.id(id, "scroll")).w_full().overflow_x_scroll().flex().justify_center().child(view),
                ),
            }
        }
        UiKind::Svg { canvas } => {
            let c = colors(app);
            let Some(prepared) = scene(id, canvas, cx.width, cx.text_size, cx, window, app) else {
                return element;
            };
            let width = prepared.scene.width;
            element
                .w(px(width))
                .flex_none()
                .self_center()
                .child(SceneView::new(cx.id(id, "svg"), prepared, width).surface(cx.surface).current(c.foreground))
        }
        UiKind::Flashcard { front, back, hint, flipped, canvas, radius } => {
            flashcard(element, id, front, back, hint, *flipped, canvas, style::radius(radius), cx, window, app)
        }
        UiKind::Celebration { burst, canvas } => celebration(element, id, *burst, canvas, cx, window, app),
        UiKind::HtmlView { html, height, title, streaming, radius } => {
            super::web::html_view(element, id, html, *height, title, *streaming, style::radius(radius), cx, window, app)
        }
        _ => element,
    }
}

/// The scene of a canvas `width` wide beside text `text_size` big, laid out off the main thread;
/// none until it is.
pub fn scene(
    id: &str,
    canvas: &Canvas,
    width: f32,
    text_size: f32,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Option<Arc<Prepared>> {
    let input = serde_json::json!({
        "kind": canvas.kind,
        "props": canvas.props,
        "width": width,
        "text_size": text_size,
    });
    laid_out(cx.id(id, "scene"), input, window, app)
}

/// A formula that lies in a run of text.
pub fn inline(canvas: &Canvas) -> bool {
    canvas.props.get("inline").and_then(|inline| inline.as_bool()).unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
fn flashcard(
    element: Stateful<Div>,
    id: &str,
    front: &Slot,
    back: &Slot,
    hint: &str,
    flipped: bool,
    canvas: &Canvas,
    radius: f32,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Stateful<Div> {
    let c = colors(app);
    let flip =
        scene(id, canvas, cx.width, cx.text_size, cx, window, app).and_then(|prepared| match &prepared.scene.motion {
            Some(Motion::Flip(flip)) => Some(flip.clone()),
            _ => None,
        });
    let turn = match &flip {
        Some(flip) => turned(cx.id(id, "turn"), flip, flipped, window, app),
        None => crate::transcript::canvas::Turn { scale: 1., back: flipped },
    };
    let hovered = window.use_keyed_state(cx.id(id, "hovered"), app, |_, _| false);
    let border = if *hovered.read(app) { c.border_accent } else { cx.surface.border(c) };
    let face_width = cx.width - 2. * FACE_PADDING.1 - 2.;
    let face = |slot_of: &Slot, back: bool, ghost: bool, window: &mut Window, app: &mut App| {
        let face_cx = match ghost {
            true => UiCx { row: format!("{}~ghost", cx.row).into(), ..cx.within(face_width) },
            false => cx.within(face_width),
        };
        let face_cx = if back { face_cx.on(cx.surface.in_box()) } else { face_cx };
        let fill = if back { cx.surface.layer(Layer::Box, c) } else { cx.surface.color(c) };
        let part = if back { "back" } else { "front" };
        let parts = window
            .with_text_style(Some(TextStyleRefinement { color: Some(c.foreground), ..Default::default() }), |window| {
                slot(slot_of, &format!("{id}/{part}"), c.foreground, &face_cx, window, app)
            });
        let hint = (!back && !hint.is_empty()).then(|| {
            style::TICK
                .text_box()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(HINT_BOTTOM))
                .text_center()
                .text_color(c.muted_stronger_foreground)
                .child(hint.to_string())
        });
        div()
            .relative()
            .w_full()
            .min_h(px(CARD_MIN_HEIGHT))
            .pt(px(FACE_PADDING.0))
            .px(px(FACE_PADDING.1))
            .pb(px(FACE_PADDING.2))
            .rounded(px(radius))
            .border_1()
            .border_color(border)
            .bg(fill)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(FACE_GAP))
            .text_center()
            .children(parts)
            .children(hint)
    };
    let ghost_front = face(front, false, true, window, app).invisible();
    let ghost_back = face(back, true, true, window, app).invisible().ml(relative(-1.));
    let shown = face(if turn.back { back } else { front }, turn.back, false, window, app).size_full();
    let (sender, node_id) = (cx.clone(), id.to_string());
    element
        .relative()
        .w_full()
        .flex()
        .flex_row()
        .items_stretch()
        .cursor_pointer()
        .child(ghost_front)
        .child(ghost_back)
        .child(div().absolute().inset_0().child(squashed(shown, turn.scale)))
        .on_hover(move |hover, _, cx| {
            hovered.update(cx, |lit, cx| {
                *lit = *hover;
                cx.notify();
            })
        })
        .on_click(move |_, _, cx| sender.send(&node_id, UiEvent::Flip, cx))
}

/// Which burst played last, and when the one playing started.
struct Bursts {
    seen: u32,
    playing: Option<Instant>,
}

#[allow(clippy::too_many_arguments)]
fn celebration(
    element: Stateful<Div>,
    id: &str,
    burst: u32,
    canvas: &Canvas,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Stateful<Div> {
    let bursts = window.use_keyed_state(cx.id(id, "bursts"), app, |_, _| Bursts { seen: burst, playing: None });
    if bursts.read(app).seen != burst {
        bursts.update(app, |bursts, _| {
            bursts.seen = burst;
            bursts.playing = Some(Instant::now());
        });
    }
    let Some(started) = bursts.read(app).playing else { return element };
    let Some(prepared) = scene(id, canvas, cx.width, cx.text_size, cx, window, app) else { return element };
    let Some(Motion::Confetti(confetti)) = &prepared.scene.motion else { return element };
    if started.elapsed().as_secs_f32() >= confetti.duration {
        bursts.update(app, |bursts, _| bursts.playing = None);
        return element;
    }
    element.child(ConfettiOverlay::new(Arc::new(confetti.clone()), started))
}
