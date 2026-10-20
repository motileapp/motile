//! The components a client paints rather than lays out: Chart, Svg, Math, Celebration and
//! Flashcard's flip. Each is a pure function of its props and the width it has, giving a
//! [`Scene`] in points that Core Graphics, Compose's `Canvas` and GPUI's `canvas()` paint as it is.

pub mod chart;
pub mod math;
pub mod motion;
pub mod scene;
pub mod svg;

use motile_openui::{Map, Value};
use serde::Deserialize;

pub use scene::*;

/// The text size a formula is set beside when none is given: Motile's prose.
const TEXT_SIZE: f64 = 14.0;

/// What `layout_json` reads: `{"kind": "Chart", "props": {…}, "width": 640, "text_size": 14}`.
/// `props` are the component's, by name, as the program gives them; an Svg's `children` are
/// `{"kind", "props"}` objects of its shapes, in order. `text_size` is the size of the text around
/// a formula.
#[derive(Deserialize, Debug)]
pub struct Canvas {
    pub kind: String,
    #[serde(default)]
    pub props: Value,
    #[serde(default)]
    pub width: f64,
    #[serde(default = "text_size")]
    pub text_size: f64,
}

fn text_size() -> f64 {
    TEXT_SIZE
}

/// The scene of a component of `kind` with its `props`, `width` wide beside text `text_size` tall.
pub fn layout(kind: &str, props: &Map, width: f64, text_size: f64) -> Scene {
    match kind {
        "Chart" => chart::layout(props, width),
        "Svg" => svg::layout(props, width),
        "Math" => {
            let tex = props.get("tex").map(motile_openui::js::to_string).unwrap_or_default();
            let inline = props.get("inline").is_some_and(motile_openui::js::truthy);
            math::layout(&tex, inline, text_size)
        }
        "Celebration" => motion::celebration(),
        "Flashcard" => motion::flip(),
        _ => Scene::empty(width, 0.0),
    }
}

/// [`layout`] from a [`Canvas`] as JSON to its scene as JSON; an empty scene when it can't be read.
pub fn layout_json(input: &str) -> String {
    let scene = match serde_json::from_str::<Canvas>(input) {
        Ok(canvas) => {
            let empty = Map::new();
            let props = canvas.props.as_object().unwrap_or(&empty);
            layout(&canvas.kind, props, canvas.width, canvas.text_size)
        }
        Err(error) => {
            tracing::warn!("couldn't read a canvas to lay out: {error}");
            Scene::default()
        }
    };
    serde_json::to_string(&scene).unwrap_or_default()
}
