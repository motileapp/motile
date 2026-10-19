//! OpenUI's canvas scenes, which core lays out (Chart, Svg, Math, Celebration's confetti and
//! Flashcard's flip), painted as `crates/core/tests/paint` paints them, with a chart's tooltip and
//! legend around them.

mod hit;
mod legend;
mod motion;
mod paint;
mod view;

#[cfg(test)]
pub(crate) mod tests;

use std::sync::Arc;

use gpui_kit::*;
use motile_core::render::openui::canvas::{self as core_canvas, Canvas, Scene};

pub use hit::hit;
pub use legend::{Arrangement, Measure, arrange, height};
pub use motion::{ConfettiOverlay, PieceFrame, Turn, confetti_frames, flip_scale, squashed, turned};
pub use paint::{Palette, Prepared, paint_scene};
pub use view::SceneView;

/// The scene of `{"kind", "props", "width", "text_size"}` as core lays it out; an empty one when
/// it can't be read. It takes a while for a formula, so it runs off the main thread.
pub fn lay_out(input: serde_json::Value) -> Scene {
    let Ok(canvas) = serde_json::from_value::<Canvas>(input) else { return Scene::default() };
    match canvas.props.as_object() {
        Some(props) => core_canvas::layout(&canvas.kind, props, canvas.width, canvas.text_size),
        None => core_canvas::layout(&canvas.kind, &Default::default(), canvas.width, canvas.text_size),
    }
}

/// What lays a canvas out for the element `id`, kept from frame to frame.
#[derive(Default)]
struct Laying {
    input: Option<serde_json::Value>,
    prepared: Option<Arc<Prepared>>,
    _task: Option<Task<()>>,
}

/// The scene of `input` (as [`lay_out`] reads it), laid out and prepared to paint off the main
/// thread. Until the first one is ready there is none; while another input is laid out the last
/// scene stays.
pub fn laid_out(
    id: impl Into<ElementId>,
    input: serde_json::Value,
    window: &mut Window,
    cx: &mut App,
) -> Option<Arc<Prepared>> {
    let state = window.use_keyed_state(id, cx, |_, _| Laying::default());
    if state.read(cx).input.as_ref() != Some(&input) {
        let laying = cx.background_spawn({
            let input = input.clone();
            async move { Arc::new(Prepared::new(lay_out(input))) }
        });
        let weak = state.downgrade();
        let task = cx.spawn(async move |cx| {
            let prepared = laying.await;
            let _ = weak.update(cx, |laying, cx| {
                laying.prepared = Some(prepared);
                cx.notify();
            });
        });
        state.update(cx, |laying, _| {
            laying.input = Some(input);
            laying._task = Some(task);
        });
    }
    state.read(cx).prepared.clone()
}
