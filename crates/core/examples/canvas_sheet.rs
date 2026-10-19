//! Lays the reference painter's pictures of the canvas cases beside each client's, for a person to
//! look at:
//!
//!     cargo run -p motile-core --example canvas_sheet -- <light|dark> <out.png> [name prefix…]
//!
//! The cases are `fixtures/canvas/reference/cases.json`; each client's pictures are what its tests
//! wrote to `build/canvas/<client>`. A picture a client didn't write is left blank.

#[allow(dead_code)]
#[path = "../tests/paint/mod.rs"]
mod paint;

use std::path::{Path, PathBuf};

use motile_core::render::openui::canvas::{Anchor, Color, Font, Item, Scene, Text, Weight};
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

const COLUMNS: [&str; 4] = ["reference", "apple", "gpui", "android"];
const GAP: f32 = 24.0;
const HEADER: f32 = 40.0;
const CAPTION: f32 = 28.0;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [theme, output, prefixes @ ..] = args.as_slice() else {
        eprintln!("usage: canvas_sheet <light|dark> <out.png> [name prefix…]");
        std::process::exit(2);
    };
    let theme = if theme == "light" { paint::Theme::Light } else { paint::Theme::Dark };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let reference = root.join("crates/core/fixtures/canvas/reference");
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(reference.join("cases.json")).expect("cases.json")).unwrap();
    let names: Vec<String> = cases
        .iter()
        .filter_map(|case| case["name"].as_str().map(str::to_owned))
        .filter(|name| prefixes.is_empty() || prefixes.iter().any(|prefix| name.starts_with(prefix.as_str())))
        .collect();
    let folder = |column: &str| match column {
        "reference" => reference.clone(),
        client => root.join("build/canvas").join(client),
    };
    let picture = |column: &str, name: &str| -> Option<Pixmap> {
        let file: PathBuf = folder(column).join(format!("{name}-{}.png", theme.name()));
        Pixmap::load_png(file).ok()
    };

    let rows: Vec<(String, Vec<Option<Pixmap>>)> =
        names.into_iter().map(|name| (name.clone(), COLUMNS.map(|column| picture(column, &name)).into())).collect();
    let point = |pixels: u32| pixels as f32 / 2.0;
    let column_width = rows
        .iter()
        .flat_map(|(_, pictures)| pictures.iter().flatten().map(|picture| point(picture.width())))
        .fold(160.0, f32::max);
    let row_height = |pictures: &[Option<Pixmap>]| {
        pictures.iter().flatten().map(|picture| point(picture.height())).fold(0.0, f32::max) + CAPTION + GAP
    };

    let mut labels = Vec::new();
    for (index, column) in COLUMNS.iter().enumerate() {
        labels.push(label(column, GAP + index as f32 * (column_width + GAP), HEADER - 14.0, Weight::Semibold));
    }
    let mut placed = Vec::new();
    let mut y = HEADER;
    for (name, pictures) in &rows {
        labels.push(label(name, GAP, y + 18.0, Weight::Medium));
        for (index, picture) in pictures.iter().enumerate() {
            let x = GAP + index as f32 * (column_width + GAP);
            match picture {
                Some(picture) => placed.push((picture, x, y + CAPTION)),
                None => labels.push(label("—", x, y + CAPTION + 16.0, Weight::Regular)),
            }
        }
        y += row_height(pictures);
    }

    let width = GAP + COLUMNS.len() as f32 * (column_width + GAP);
    let sheet = Scene { width, height: y, items: labels, ..Scene::default() };
    let mut pixmap = paint::paint(&sheet, theme, 1.0, None);
    for (picture, x, y) in placed {
        let scale = Transform::from_scale(0.5, 0.5).post_translate(x, y);
        let quality = PixmapPaint { quality: FilterQuality::Bicubic, ..PixmapPaint::default() };
        pixmap.draw_pixmap(0, 0, picture.as_ref(), &quality, scale, None);
    }
    pixmap.save_png(output).expect("the sheet can't be written");
}

fn label(text: &str, x: f32, y: f32, weight: Weight) -> Item {
    Item::Text(Text {
        text: text.to_owned(),
        x,
        y,
        anchor: Anchor::Start,
        font: Font::Ui { size: 14.0, weight },
        color: Color::token("muted-foreground"),
        opacity: 1.0,
    })
}
