//! Math: TeX laid out by RaTeX, as KaTeX lays it out, with its glyphs as outlines of the KaTeX fonts
//! (`fonts/katex`, under the SIL Open Font License), so that a client only fills paths. A formula
//! RaTeX can't read shows its source in KaTeX's error colour, as KaTeX does.

use std::collections::HashMap;
use std::sync::OnceLock;

use kurbo::Affine;
use ratex_layout::{LayoutOptions, layout as layout_box, to_display_list, validate_layout};
use ratex_types::color::Color as TexColor;
use ratex_types::display_item::{DisplayItem, DisplayList};
use ratex_types::math_style::MathStyle;
use ratex_types::path_command::PathCommand;
use ttf_parser::{Face, OutlineBuilder};

use super::scene::{Anchor, Color, Font, Item, Paint, PathBuilder, Scene, Stroke, Text, round};

/// A formula is this much larger than the text around it.
const SCALE: f64 = 1.12;
/// KaTeX's `errorColor`.
const ERROR: Color = Color::Rgba([204, 0, 0, 255]);
/// The width of an unfilled path, in em: RaTeX's 1.5 at 40 to the em.
const STROKE_EM: f64 = 1.5 / 40.0;

macro_rules! fonts {
    ($($name:literal),* $(,)?) => {
        [$(($name, include_bytes!(concat!("../../../../fonts/katex/KaTeX_", $name, ".ttf")).as_slice())),*]
    };
}

const FONTS: [(&str, &[u8]); 20] = fonts![
    "AMS-Regular",
    "Caligraphic-Bold",
    "Caligraphic-Regular",
    "Fraktur-Bold",
    "Fraktur-Regular",
    "Main-Bold",
    "Main-BoldItalic",
    "Main-Italic",
    "Main-Regular",
    "Math-BoldItalic",
    "Math-Italic",
    "SansSerif-Bold",
    "SansSerif-Italic",
    "SansSerif-Regular",
    "Script-Regular",
    "Size1-Regular",
    "Size2-Regular",
    "Size3-Regular",
    "Size4-Regular",
    "Typewriter-Regular",
];

fn faces() -> &'static HashMap<&'static str, Face<'static>> {
    static FACES: OnceLock<HashMap<&'static str, Face<'static>>> = OnceLock::new();
    FACES.get_or_init(|| FONTS.iter().filter_map(|(name, data)| Some((*name, Face::parse(data, 0).ok()?))).collect())
}

/// The scene of `tex` set in text `text_size` points tall: a block (display style) or `inline`.
pub fn layout(tex: &str, inline: bool, text_size: f64) -> Scene {
    let em = text_size * SCALE;
    let Ok(nodes) = ratex_parser::parse(tex) else { return fallback(tex, text_size) };
    let style = if inline { MathStyle::Text } else { MathStyle::Display };
    let options = LayoutOptions { style, ..LayoutOptions::default() };
    let tree = layout_box(&nodes, &options);
    if validate_layout(&tree).is_err() {
        return fallback(tex, text_size);
    }
    let list = to_display_list(&tree);
    Scene {
        width: round(list.width * em),
        height: round((list.height + list.depth) * em),
        baseline: Some(round(list.height * em)),
        label: Some(tex.to_owned()),
        items: draw(&list, em),
        ..Scene::default()
    }
}

/// The display list's items in points, filled glyphs and rules of one colour joined in a path.
fn draw(list: &DisplayList, em: f64) -> Vec<Item> {
    let mut items = Vec::new();
    let mut run: Option<(Color, PathBuilder)> = None;
    for item in &list.items {
        match item {
            DisplayItem::GlyphPath { x, y, scale, font, char_code, color } => {
                let size = scale * em;
                let (px, py) = (x * em, y * em);
                let face = faces().get(font.as_str());
                let glyph =
                    char::from_u32(*char_code).and_then(|c| face.and_then(|face| Some((face, face.glyph_index(c)?))));
                let Some((face, glyph)) = glyph else {
                    flush(&mut run, &mut items);
                    let text = char::from_u32(*char_code).map(String::from).unwrap_or_default();
                    items.push(Item::Text(Text {
                        text,
                        x: round(px),
                        y: round(py),
                        anchor: Anchor::Start,
                        font: Font::Ui { size: round(size), weight: Default::default() },
                        color: color_of(color),
                        opacity: 1.0,
                    }));
                    continue;
                };
                let k = size / face.units_per_em() as f64;
                let path = fill(&mut run, &mut items, color_of(color));
                face.outline_glyph(glyph, &mut Outline { path, transform: Affine::new([k, 0.0, 0.0, -k, px, py]) });
            }
            DisplayItem::Line { x, y, width, thickness, color, dashed: false } => {
                let t = thickness * em;
                fill(&mut run, &mut items, color_of(color)).rounded_rect(
                    x * em,
                    y * em - t / 2.0,
                    width * em,
                    t,
                    0.0,
                    0.0,
                );
            }
            DisplayItem::Line { x, y, width, thickness, color, dashed: true } => {
                flush(&mut run, &mut items);
                let t = thickness * em;
                let mut path = PathBuilder::new(Affine::IDENTITY);
                path.move_to(x * em, y * em);
                path.line_to((x + width) * em, y * em);
                let stroke = Stroke::new(color_of(color), t).dashed(&[t * 3.0, t * 3.0]);
                items.extend(path.finish().map(|path| Item::Path { path, paint: Paint::stroke(stroke) }));
            }
            DisplayItem::Rect { x, y, width, height, color } => {
                flush(&mut run, &mut items);
                items.push(Item::Rect {
                    x: round(x * em),
                    y: round(y * em),
                    width: round(width * em),
                    height: round(height * em),
                    rx: 0.0,
                    ry: 0.0,
                    paint: Paint::fill(color_of(color)),
                });
            }
            DisplayItem::Path { x, y, commands, fill: true, color } => {
                let path = fill(&mut run, &mut items, color_of(color));
                path.set_transform(Affine::new([em, 0.0, 0.0, em, x * em, y * em]));
                trace(commands, path);
                path.set_transform(Affine::IDENTITY);
            }
            DisplayItem::Path { x, y, commands, fill: false, color } => {
                flush(&mut run, &mut items);
                let mut path = PathBuilder::new(Affine::new([em, 0.0, 0.0, em, x * em, y * em]));
                trace(commands, &mut path);
                let stroke = Stroke::new(color_of(color), STROKE_EM * em).round();
                items.extend(path.finish().map(|path| Item::Path { path, paint: Paint::stroke(stroke) }));
            }
        }
    }
    flush(&mut run, &mut items);
    items
}

/// The path that fills in `color`, a new one when the run before was in another colour.
fn fill<'a>(run: &'a mut Option<(Color, PathBuilder)>, items: &mut Vec<Item>, color: Color) -> &'a mut PathBuilder {
    if run.as_ref().is_some_and(|(current, _)| *current != color) {
        flush(run, items);
    }
    &mut run.get_or_insert_with(|| (color, PathBuilder::new(Affine::IDENTITY))).1
}

fn flush(run: &mut Option<(Color, PathBuilder)>, items: &mut Vec<Item>) {
    let Some((color, path)) = run.take() else { return };
    items.extend(path.finish().map(|path| Item::Path { path, paint: Paint::fill(color) }));
}

fn trace(commands: &[PathCommand], path: &mut PathBuilder) {
    for command in commands {
        match *command {
            PathCommand::MoveTo { x, y } => path.move_to(x, y),
            PathCommand::LineTo { x, y } => path.line_to(x, y),
            PathCommand::QuadTo { x1, y1, x, y } => path.quad_to(x1, y1, x, y),
            PathCommand::CubicTo { x1, y1, x2, y2, x, y } => path.cubic_to(x1, y1, x2, y2, x, y),
            PathCommand::Close => path.close(),
        }
    }
}

/// The formula's own colour, black unless `\color` set one: black is the text's colour.
fn color_of(color: &TexColor) -> Color {
    if *color == TexColor::BLACK {
        return Color::Current;
    }
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::Rgba([channel(color.r), channel(color.g), channel(color.b), channel(color.a)])
}

/// The source in KaTeX's error colour, at the text's size, on one line.
fn fallback(tex: &str, text_size: f64) -> Scene {
    let face = &faces()["Main-Regular"];
    let k = text_size / face.units_per_em() as f64;
    let ascent = face.ascender() as f64 * k;
    let descent = -face.descender() as f64 * k;
    let mut path = PathBuilder::new(Affine::IDENTITY);
    let mut x = 0.0;
    for c in tex.chars() {
        let Some(glyph) = face.glyph_index(c) else {
            x += text_size / 2.0;
            continue;
        };
        let mut outline = Outline { path: &mut path, transform: Affine::new([k, 0.0, 0.0, -k, x, ascent]) };
        face.outline_glyph(glyph, &mut outline);
        x += face.glyph_hor_advance(glyph).unwrap_or(0) as f64 * k;
    }
    let items = path.finish().map(|path| Item::Path { path, paint: Paint::fill(ERROR) }).into_iter().collect();
    Scene {
        width: round(x),
        height: round(ascent + descent),
        baseline: Some(round(ascent)),
        label: Some(tex.to_owned()),
        items,
        ..Scene::default()
    }
}

struct Outline<'a> {
    path: &'a mut PathBuilder,
    transform: Affine,
}

impl Outline<'_> {
    fn at(&self, x: f32, y: f32) -> kurbo::Point {
        self.transform * kurbo::Point::new(x as f64, y as f64)
    }
}

impl OutlineBuilder for Outline<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.at(x, y);
        self.path.move_to(p.x, p.y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.at(x, y);
        self.path.line_to(p.x, p.y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (c, p) = (self.at(x1, y1), self.at(x, y));
        self.path.quad_to(c.x, c.y, p.x, p.y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (c1, c2, p) = (self.at(x1, y1), self.at(x2, y2), self.at(x, y));
        self.path.cubic_to(c1.x, c1.y, c2.x, c2.y, p.x, p.y);
    }

    fn close(&mut self) {
        self.path.close();
    }
}
