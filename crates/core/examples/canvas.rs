//! Paints a canvas component with the reference painter, to compare a client's picture with:
//!
//!     cargo run -p motile-core --example canvas -- input.json dark out.png [hit]
//!
//! `input.json` is what `motile_ui_layout` reads (`canvas::Canvas`); `hit` lights that hit.

#[path = "../tests/paint/mod.rs"]
mod paint;

use motile_core::render::openui::canvas;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, theme, output, ..] = args.as_slice() else {
        eprintln!("usage: canvas <input.json> <light|dark> <out.png> [hit]");
        std::process::exit(2);
    };
    let input = std::fs::read_to_string(input).expect("the input can't be read");
    let scene: canvas::Scene = serde_json::from_str(&canvas::layout_json(&input)).expect("a scene");
    let theme = if theme == "light" { paint::Theme::Light } else { paint::Theme::Dark };
    let hover = args.get(3).and_then(|hit| hit.parse().ok());
    paint::paint(&scene, theme, 2.0, hover).save_png(output).expect("the picture can't be written");
}
