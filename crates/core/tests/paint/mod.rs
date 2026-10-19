//! The reference painter of canvas scenes, with tiny-skia: what each client's picture of a scene is
//! compared with. Colours come from `packages/theme/tokens.json`, on the transcript's surface, and
//! labels are set in DM Sans, the GPUI app's typeface.

use std::collections::HashMap;

use motile_core::render::openui::canvas::{
    Anchor, Cap, Color, Font, Item, Join, Layer, Paint, Path, Scene, Text, Weight,
};
use tiny_skia::{
    FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Paint as SkPaint, PathBuilder, Pixmap, Point, Shader,
    SpreadMode, Stroke, StrokeDash, Transform,
};
use ttf_parser::{Face, OutlineBuilder};

const FONTS: [(Weight, &[u8]); 3] = [
    (Weight::Regular, include_bytes!("../../../../apps/gpui/assets/fonts/DMSans-Regular.ttf")),
    (Weight::Medium, include_bytes!("../../../../apps/gpui/assets/fonts/DMSans-Medium.ttf")),
    (Weight::Semibold, include_bytes!("../../../../apps/gpui/assets/fonts/DMSans-SemiBold.ttf")),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn name(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

/// The tokens of one appearance.
pub struct Palette {
    colors: HashMap<String, [u8; 4]>,
    tint: f32,
}

impl Palette {
    pub fn new(theme: Theme) -> Palette {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/theme/tokens.json");
        let tokens: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let colors = tokens["colors"][theme.name()]
            .as_object()
            .unwrap()
            .iter()
            .filter_map(|(name, hex)| Some((name.clone(), hex_color(hex.as_str()?)?)))
            .collect();
        let tint = tokens["opacity"][theme.name()]["color-tint"].as_f64().unwrap() as f32;
        Palette { colors, tint }
    }

    fn token(&self, name: &str) -> [u8; 4] {
        self.colors.get(name).copied().unwrap_or([255, 0, 255, 255])
    }

    /// A colour as the transcript's surface shows it.
    pub fn resolve(&self, color: &Color, opacity: f32) -> tiny_skia::Color {
        let [r, g, b, a] = match color {
            Color::Current => self.token("foreground"),
            Color::Token(name) => self.token(name),
            Color::Wash(name) => {
                let [r, g, b, a] = self.token(name);
                [r, g, b, (a as f32 * self.tint).round() as u8]
            }
            Color::Surface(layer) => self.token(match layer {
                Layer::Base => "background",
                Layer::Border => "border",
                Layer::Row => "background-accent-larger",
                Layer::Control => "background-accent",
                Layer::Box => "background-secondary",
            }),
            Color::Rgba(rgba) => *rgba,
        };
        tiny_skia::Color::from_rgba8(r, g, b, (a as f32 * opacity.clamp(0.0, 1.0)).round() as u8)
    }
}

fn hex_color(hex: &str) -> Option<[u8; 4]> {
    let hex = hex.strip_prefix('#')?;
    let channel = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?, if hex.len() == 8 { channel(6)? } else { 255 }])
}

/// The scene on the transcript's background, `scale` pixels to the point, with the hit `hover`
/// lit. A pie's legend and the tooltips are the clients' and aren't painted.
pub fn paint(scene: &Scene, theme: Theme, scale: f32, hover: Option<usize>) -> Pixmap {
    let palette = Palette::new(theme);
    let width = (scene.width * scale).ceil().max(1.0) as u32;
    let height = (scene.height * scale).ceil().max(1.0) as u32;
    let mut pixmap = Pixmap::new(width, height).unwrap();
    pixmap.fill(palette.resolve(&Color::Surface(Layer::Base), 1.0));
    let transform = Transform::from_scale(scale, scale);
    let hit = hover.and_then(|index| scene.hits.get(index));
    for (index, item) in scene.items.iter().enumerate() {
        if let Some(hit) = hit.filter(|hit| hit.at as usize == index) {
            hit.lit.iter().for_each(|item| draw(&mut pixmap, item, &palette, transform));
        }
        draw(&mut pixmap, item, &palette, transform);
    }
    if let Some(hit) = hit.filter(|hit| hit.at as usize >= scene.items.len()) {
        hit.lit.iter().for_each(|item| draw(&mut pixmap, item, &palette, transform));
    }
    pixmap
}

fn draw(pixmap: &mut Pixmap, item: &Item, palette: &Palette, transform: Transform) {
    match item {
        Item::Path { path, paint } => shape(pixmap, sk_path(path), paint, palette, transform),
        Item::Rect { x, y, width, height, rx, ry, paint } => {
            let mut builder = PathBuilder::new();
            rounded_rect(&mut builder, *x, *y, *width, *height, *rx, *ry);
            shape(pixmap, builder.finish(), paint, palette, transform);
        }
        Item::Circle { cx, cy, r, paint } => {
            shape(pixmap, PathBuilder::from_circle(*cx, *cy, *r), paint, palette, transform)
        }
        Item::Text(text) => {
            let (size, weight) = font_of(&text.font);
            let face = face(weight);
            let k = size / face.units_per_em() as f32;
            let glyphs: Vec<_> = text.text.chars().filter_map(|c| face.glyph_index(c)).collect();
            let advance: f32 = glyphs.iter().map(|g| face.glyph_hor_advance(*g).unwrap_or(0) as f32 * k).sum();
            let mut x = match text.anchor {
                Anchor::Start => text.x,
                Anchor::Middle => text.x - advance / 2.0,
                Anchor::End => text.x - advance,
            };
            let mut builder = PathBuilder::new();
            for glyph in glyphs {
                face.outline_glyph(glyph, &mut Glyph { builder: &mut builder, k, x, y: text.y });
                x += face.glyph_hor_advance(glyph).unwrap_or(0) as f32 * k;
            }
            let paint = Paint {
                fill: Some(text.color.clone()),
                gradient: None,
                fill_opacity: 1.0,
                stroke: None,
                opacity: text.opacity,
            };
            shape(pixmap, builder.finish(), &paint, palette, transform);
        }
    }
}

/// Where a label could lie in any client's typeface: its box in DM Sans, widened by a fifth and
/// 3 points on each side, from an ascent above its baseline to a descent below, as x, y, width,
/// height. A client's picture is compared with the reference's outside these.
pub fn text_box(text: &Text) -> [f32; 4] {
    let (size, _) = font_of(&text.font);
    let (left, right) = text_span(text);
    let advance = right - left;
    let pad = advance * 0.2 + 3.0;
    let (top, bottom) = (text.y - size - 2.0, text.y + size * 0.35 + 2.0);
    [left - pad, top, advance + pad * 2.0, bottom - top]
}

/// Where a line of text starts and ends, in DM Sans.
pub fn text_span(text: &Text) -> (f32, f32) {
    let (size, weight) = font_of(&text.font);
    let face = face(weight);
    let k = size / face.units_per_em() as f32;
    let advance: f32 = text
        .text
        .chars()
        .filter_map(|c| face.glyph_index(c))
        .map(|glyph| face.glyph_hor_advance(glyph).unwrap_or(0) as f32 * k)
        .sum();
    let left = match text.anchor {
        Anchor::Start => text.x,
        Anchor::Middle => text.x - advance / 2.0,
        Anchor::End => text.x - advance,
    };
    (left, left + advance)
}

fn font_of(font: &Font) -> (f32, Weight) {
    match font {
        Font::Tick => (11.0, Weight::Regular),
        Font::Caption => (12.0, Weight::Regular),
        Font::Ui { size, weight } => (*size, *weight),
    }
}

fn face(weight: Weight) -> Face<'static> {
    let data = FONTS.iter().find(|(w, _)| *w == weight).map_or(FONTS[0].1, |(_, data)| data);
    Face::parse(data, 0).unwrap()
}

fn shape(pixmap: &mut Pixmap, path: Option<tiny_skia::Path>, paint: &Paint, palette: &Palette, transform: Transform) {
    let Some(path) = path else { return };
    let gradient = paint.gradient.as_ref().and_then(|gradient| {
        let stops = gradient
            .stops
            .iter()
            .map(|stop| GradientStop::new(stop.at, palette.resolve(&stop.color, stop.opacity * paint.opacity)))
            .collect();
        LinearGradient::new(
            Point::from_xy(gradient.x1, gradient.y1),
            Point::from_xy(gradient.x2, gradient.y2),
            stops,
            SpreadMode::Pad,
            Transform::identity(),
        )
    });
    let fill = gradient.or_else(|| {
        let color = paint.fill.as_ref()?;
        Some(Shader::SolidColor(palette.resolve(color, paint.fill_opacity * paint.opacity)))
    });
    if let Some(shader) = fill {
        let sk = SkPaint { shader, anti_alias: true, ..SkPaint::default() };
        pixmap.fill_path(&path, &sk, FillRule::Winding, transform, None);
    }
    let Some(stroke) = &paint.stroke else { return };
    let sk = SkPaint {
        shader: Shader::SolidColor(palette.resolve(&stroke.color, paint.opacity)),
        anti_alias: true,
        ..SkPaint::default()
    };
    let line = Stroke {
        width: stroke.width,
        miter_limit: 4.0,
        line_cap: match stroke.cap {
            Cap::Butt => LineCap::Butt,
            Cap::Round => LineCap::Round,
            Cap::Square => LineCap::Square,
        },
        line_join: match stroke.join {
            Join::Miter => LineJoin::Miter,
            Join::Round => LineJoin::Round,
            Join::Bevel => LineJoin::Bevel,
        },
        dash: StrokeDash::new(stroke.dash.clone(), 0.0),
    };
    pixmap.stroke_path(&path, &sk, &line, transform, None);
}

fn sk_path(path: &Path) -> Option<tiny_skia::Path> {
    let mut builder = PathBuilder::new();
    let mut points = path.points.as_chunks::<2>().0.iter().map(|[x, y]| (*x, *y));
    for verb in path.verbs.chars() {
        match verb {
            'M' => {
                let (x, y) = points.next()?;
                builder.move_to(x, y);
            }
            'L' => {
                let (x, y) = points.next()?;
                builder.line_to(x, y);
            }
            'Q' => {
                let ((x1, y1), (x, y)) = (points.next()?, points.next()?);
                builder.quad_to(x1, y1, x, y);
            }
            'C' => {
                let ((x1, y1), (x2, y2), (x, y)) = (points.next()?, points.next()?, points.next()?);
                builder.cubic_to(x1, y1, x2, y2, x, y);
            }
            'Z' => builder.close(),
            _ => return None,
        }
    }
    builder.finish()
}

fn rounded_rect(builder: &mut PathBuilder, x: f32, y: f32, width: f32, height: f32, rx: f32, ry: f32) {
    const KAPPA: f32 = 0.552_284_8;
    let (right, bottom) = (x + width, y + height);
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    builder.move_to(x + rx, y);
    builder.line_to(right - rx, y);
    builder.cubic_to(right - rx + kx, y, right, y + ry - ky, right, y + ry);
    builder.line_to(right, bottom - ry);
    builder.cubic_to(right, bottom - ry + ky, right - rx + kx, bottom, right - rx, bottom);
    builder.line_to(x + rx, bottom);
    builder.cubic_to(x + rx - kx, bottom, x, bottom - ry + ky, x, bottom - ry);
    builder.line_to(x, y + ry);
    builder.cubic_to(x, y + ry - ky, x + rx - kx, y, x + rx, y);
    builder.close();
}

struct Glyph<'a> {
    builder: &'a mut PathBuilder,
    k: f32,
    x: f32,
    y: f32,
}

impl OutlineBuilder for Glyph<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to(self.x + x * self.k, self.y - y * self.k);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(self.x + x * self.k, self.y - y * self.k);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (k, ox, oy) = (self.k, self.x, self.y);
        self.builder.quad_to(ox + x1 * k, oy - y1 * k, ox + x * k, oy - y * k);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (k, ox, oy) = (self.k, self.x, self.y);
        self.builder.cubic_to(ox + x1 * k, oy - y1 * k, ox + x2 * k, oy - y2 * k, ox + x * k, oy - y * k);
    }

    fn close(&mut self) {
        self.builder.close();
    }
}
