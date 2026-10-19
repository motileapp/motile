//! The painter's display list as a picture, with tiny-skia, for the tests: what the map looks
//! like without a window. Labels are set in DM Sans, the emoji in the Mac's emoji font when it
//! is there.

use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Rect as SkRect, Shader, Stroke,
    Transform,
};
use ttf_parser::{Face, OutlineBuilder};

use super::paint::{DisplayList, Item, Label, Rect};
use super::style::Rgba;

const EMOJI_FONT: &str = "/System/Library/Fonts/Apple Color Emoji.ttc";

/// The list in a map `width` by `height` points, `scale` pixels to the point, on `water` until
/// tiles draw, its corners rounded by `radius`.
pub fn picture(list: &DisplayList, width: f32, height: f32, scale: f32, water: Rgba, radius: f32) -> Pixmap {
    let mut pixmap = Pixmap::new((width * scale).ceil() as u32, (height * scale).ceil() as u32).unwrap();
    let transform = Transform::from_scale(scale, scale);
    let has_tiles = list.items.iter().any(|item| matches!(item, Item::Fill { .. } | Item::Line { .. }));
    pixmap.fill(color(if has_tiles { list.background } else { water }));
    for item in &list.items {
        match item {
            Item::Fill { clip, rings, color: fill, .. } => {
                let Some(path) = polyline(rings, true) else { continue };
                let mask = clipped(&pixmap, *clip, transform);
                pixmap.fill_path(&path, &paint(*fill), FillRule::EvenOdd, transform, mask.as_ref());
            }
            Item::Line { clip, lines, width, color: line, round, .. } => {
                let Some(path) = polyline(lines, false) else { continue };
                let mask = clipped(&pixmap, *clip, transform);
                pixmap.stroke_path(&path, &paint(*line), &stroke(*width, *round), transform, mask.as_ref());
            }
            Item::Circle { center, radius, color: fill, stroke: ring, blur, .. } => {
                if *blur > 0. {
                    let Some([r, g, b, a]) = fill else { continue };
                    let steps = 6;
                    let each = 1. - (1. - a).powf(1. / steps as f32);
                    for step in 0..steps {
                        let reach = radius * (1. - blur) + radius * blur * (step as f32 + 0.5) / steps as f32;
                        let Some(disc) = PathBuilder::from_circle(center[0], center[1], reach) else { continue };
                        pixmap.fill_path(&disc, &paint([*r, *g, *b, each]), FillRule::Winding, transform, None);
                    }
                    continue;
                }
                let width = ring.map_or(0., |(_, width)| width);
                if let (Some(fill), Some(disc)) = (fill, PathBuilder::from_circle(center[0], center[1], *radius)) {
                    pixmap.fill_path(&disc, &paint(*fill), FillRule::Winding, transform, None);
                }
                if let (Some((edge, _)), Some(outer)) =
                    (ring, PathBuilder::from_circle(center[0], center[1], radius + width / 2.))
                {
                    pixmap.stroke_path(&outer, &paint(*edge), &stroke(width, false), transform, None);
                }
            }
            Item::Text(label) => text(&mut pixmap, label, transform),
        }
    }
    round_corners(&mut pixmap, radius * scale);
    pixmap
}

fn color([r, g, b, a]: Rgba) -> Color {
    Color::from_rgba(r, g, b, a).unwrap_or(Color::BLACK)
}

fn paint(rgba: Rgba) -> Paint<'static> {
    Paint { shader: Shader::SolidColor(color(rgba)), anti_alias: true, ..Paint::default() }
}

fn stroke(width: f32, round: bool) -> Stroke {
    Stroke {
        width,
        line_cap: if round { LineCap::Round } else { LineCap::Butt },
        line_join: if round { LineJoin::Round } else { LineJoin::Miter },
        ..Stroke::default()
    }
}

fn polyline(parts: &[Vec<[f32; 2]>], close: bool) -> Option<tiny_skia::Path> {
    let mut builder = PathBuilder::new();
    for part in parts {
        let Some((first, rest)) = part.split_first() else { continue };
        builder.move_to(first[0], first[1]);
        rest.iter().for_each(|at| builder.line_to(at[0], at[1]));
        if close {
            builder.close();
        }
    }
    builder.finish()
}

fn clipped(pixmap: &Pixmap, [left, top, right, bottom]: Rect, transform: Transform) -> Option<Mask> {
    let mut mask = Mask::new(pixmap.width(), pixmap.height())?;
    let rect = PathBuilder::from_rect(SkRect::from_ltrb(left, top, right, bottom)?);
    mask.fill_path(&rect, FillRule::Winding, false, transform);
    Some(mask)
}

fn round_corners(pixmap: &mut Pixmap, radius: f32) {
    let (width, height) = (pixmap.width() as f32, pixmap.height() as f32);
    let Some(mut mask) = Mask::new(pixmap.width(), pixmap.height()) else { return };
    let mut builder = PathBuilder::new();
    let k = 0.552_284_8 * radius;
    builder.move_to(radius, 0.);
    builder.line_to(width - radius, 0.);
    builder.cubic_to(width - radius + k, 0., width, radius - k, width, radius);
    builder.line_to(width, height - radius);
    builder.cubic_to(width, height - radius + k, width - radius + k, height, width - radius, height);
    builder.line_to(radius, height);
    builder.cubic_to(radius - k, height, 0., height - radius + k, 0., height - radius);
    builder.line_to(0., radius);
    builder.cubic_to(0., radius - k, radius - k, 0., radius, 0.);
    builder.close();
    let Some(shape) = builder.finish() else { return };
    mask.fill_path(&shape, FillRule::Winding, true, Transform::identity());
    pixmap.apply_mask(&mask);
}

struct Outline<'a> {
    builder: &'a mut PathBuilder,
    scale: f32,
    x: f32,
    y: f32,
}

impl OutlineBuilder for Outline<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to(self.x + x * self.scale, self.y - y * self.scale);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(self.x + x * self.scale, self.y - y * self.scale);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.builder.quad_to(
            self.x + x1 * self.scale,
            self.y - y1 * self.scale,
            self.x + x * self.scale,
            self.y - y * self.scale,
        );
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.builder.cubic_to(
            self.x + x1 * self.scale,
            self.y - y1 * self.scale,
            self.x + x2 * self.scale,
            self.y - y2 * self.scale,
            self.x + x * self.scale,
            self.y - y * self.scale,
        );
    }
    fn close(&mut self) {
        self.builder.close();
    }
}

fn text(pixmap: &mut Pixmap, label: &Label, transform: Transform) {
    if label.emoji {
        emoji(pixmap, label, transform);
        return;
    }
    let data = crate::assets::FONTS[if label.bold { 3 } else { 0 }];
    let Ok(face) = Face::parse(data, 0) else { return };
    let scale = label.size / f32::from(face.units_per_em());
    let ascent = f32::from(face.ascender()) * scale;
    let descent = f32::from(-face.descender()) * scale;
    let mut builder = PathBuilder::new();
    for (index, line) in label.lines.iter().enumerate() {
        let width = super::paint::measure(line, label.size, label.bold, label.spacing);
        let mut x = label.x - width / 2.;
        let top = label.top + label.line_height * index as f32;
        let baseline = top + (label.line_height - ascent - descent) / 2. + ascent;
        for c in line.chars() {
            let Some(glyph) = face.glyph_index(c) else { continue };
            face.outline_glyph(glyph, &mut Outline { builder: &mut builder, scale, x, y: baseline });
            x += f32::from(face.glyph_hor_advance(glyph).unwrap_or(0)) * scale + label.spacing;
        }
    }
    let Some(path) = builder.finish() else { return };
    if let Some((halo, width)) = label.halo {
        pixmap.stroke_path(
            &path,
            &paint(halo),
            &Stroke { width: width * 2., line_join: LineJoin::Round, ..Stroke::default() },
            transform,
            None,
        );
    }
    pixmap.fill_path(&path, &paint(label.color), FillRule::Winding, transform, None);
}

fn emoji(pixmap: &mut Pixmap, label: &Label, transform: Transform) {
    let Ok(data) = std::fs::read(EMOJI_FONT) else { return };
    let Ok(face) = Face::parse(&data, 0) else { return };
    let Some(c) = label.lines.first().and_then(|line| line.chars().next()) else { return };
    let Some(glyph) = face.glyph_index(c) else { return };
    let side = label.size * transform.sx;
    let Some(image) = face.glyph_raster_image(glyph, side.ceil() as u16) else { return };
    let Ok(decoded) = image::load_from_memory(image.data) else { return };
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut premultiplied = rgba.into_raw();
    for pixel in premultiplied.chunks_mut(4) {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = (u16::from(*channel) * alpha / 255) as u8;
        }
    }
    let Some(size) = tiny_skia::IntSize::from_wh(width, height) else { return };
    let Some(glyph_pixmap) = Pixmap::from_vec(premultiplied, size) else { return };
    let fit = side / width.max(height) as f32;
    let center = [
        (label.bounds[0] + label.bounds[2]) / 2. * transform.sx,
        (label.bounds[1] + label.bounds[3]) / 2. * transform.sy,
    ];
    let placed = Transform::from_row(
        fit,
        0.,
        0.,
        fit,
        center[0] - width as f32 * fit / 2.,
        center[1] - height as f32 * fit / 2.,
    );
    pixmap.draw_pixmap(
        0,
        0,
        glyph_pixmap.as_ref(),
        &PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..PixmapPaint::default() },
        placed,
        None,
    );
}
