//! Lays each client's pictures of a program beside the others', at both widths, for a person to
//! look at:
//!
//!     cargo run -p motile-core --example program_sheet -- <out folder> [program…]
//!
//! The programs are the golden trees of `fixtures/openui`; each client's pictures are what its tests
//! wrote to `build/programs/<client>/<program>-<width>.png`, at any number of pixels a point. It
//! writes `<out folder>/<program>.png`, dark, one point a pixel. A picture a client didn't write is
//! left blank.

#[allow(dead_code)]
#[path = "../tests/paint/mod.rs"]
mod paint;

use std::path::Path;

use motile_core::render::openui::canvas::{Anchor, Color, Font, Item, Scene, Text, Weight};
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

const CLIENTS: [(&str, &str); 4] = [("mac", "Mac"), ("ios", "iOS"), ("gpui", "GPUI"), ("android", "Android")];
const WIDTHS: [u32; 2] = [390, 720];
const GAP: f32 = 24.0;
const HEADER: f32 = 44.0;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [output, only @ ..] = args.as_slice() else {
        eprintln!("usage: program_sheet <out folder> [program…]");
        std::process::exit(2);
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut programs: Vec<String> = std::fs::read_dir(root.join("crates/core/fixtures/openui"))
        .expect("the golden trees")
        .filter_map(|entry| entry.ok()?.path().file_stem()?.to_str().map(str::to_owned))
        .filter(|name| only.is_empty() || only.contains(name))
        .collect();
    programs.sort();
    std::fs::create_dir_all(output).expect("the sheets' folder");
    for program in programs {
        let pictures = WIDTHS.map(|width| {
            CLIENTS.map(|(client, _)| {
                let file = root.join("build/programs").join(client).join(format!("{program}-{width}.png"));
                Pixmap::load_png(file).ok()
            })
        });
        sheet(&program, &pictures).save_png(Path::new(output).join(format!("{program}.png"))).expect("the sheet");
    }
}

/// The pictures in a row a width, a column a client.
fn sheet(program: &str, pictures: &[[Option<Pixmap>; 4]; 2]) -> Pixmap {
    let height_of = |picture: &Pixmap, width: u32| picture.height() as f32 * width as f32 / picture.width() as f32;
    let mut labels = vec![label(program, GAP, 28.0, 16.0, Weight::Semibold)];
    let mut placed = Vec::new();
    let mut y = HEADER;
    let mut sheet_width: f32 = 0.0;
    for (row, width) in pictures.iter().zip(WIDTHS) {
        let tallest = row.iter().flatten().map(|picture| height_of(picture, width)).fold(0.0, f32::max);
        let mut x = GAP;
        for ((_, name), picture) in CLIENTS.iter().zip(row) {
            labels.push(label(&format!("{name} · {width}"), x, y + 14.0, 13.0, Weight::Medium));
            match picture {
                Some(picture) => placed.push((picture, x, y + 24.0, width as f32 / picture.width() as f32)),
                None => labels.push(label("—", x, y + 44.0, 13.0, Weight::Regular)),
            }
            x += width as f32 + GAP;
        }
        sheet_width = sheet_width.max(x);
        y += 24.0 + tallest + GAP;
    }
    let scene = Scene { width: sheet_width, height: y, items: labels, ..Scene::default() };
    let mut pixmap = paint::paint(&scene, paint::Theme::Dark, 1.0, None);
    let quality = PixmapPaint { quality: FilterQuality::Bicubic, ..PixmapPaint::default() };
    for (picture, x, y, scale) in placed {
        pixmap.draw_pixmap(
            0,
            0,
            picture.as_ref(),
            &quality,
            Transform::from_scale(scale, scale).post_translate(x, y),
            None,
        );
    }
    pixmap
}

fn label(text: &str, x: f32, y: f32, size: f32, weight: Weight) -> Item {
    Item::Text(Text {
        text: text.to_owned(),
        x,
        y,
        anchor: Anchor::Start,
        font: Font::Ui { size, weight },
        color: Color::token("muted-foreground"),
        opacity: 1.0,
    })
}
