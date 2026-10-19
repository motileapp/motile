//! The painter's display list in GPUI: its fills and lines made into paths off the main thread
//! (a `Scene`), moved to the camera every frame without being made again, and its circles and
//! labels drawn as they are.

use gpui_kit::*;
use lyon::tessellation::{LineCap, LineJoin};
use motile_core::render::openui::map::world;

use super::paint::{DisplayList, Item, Label, Rect, View};
use super::style::Rgba;
use crate::theme::UI_FONT;
use crate::transcript::rows::font;

/// Rings or lines put in one path at most, so that its vertices fit its 16-bit indices.
const FILL_POINTS: usize = 16_000;
const LINE_POINTS: usize = 2_500;

/// The tiles' shapes as paths and their labels, as the painter drew them for `view`.
pub struct Scene {
    pub view: View,
    pub background: Hsla,
    shapes: Vec<Shape>,
    labels: Vec<Label>,
}

struct Shape {
    path: Path<Pixels>,
    color: Hsla,
    clip: Rect,
}

pub fn hsla([r, g, b, a]: Rgba) -> Hsla {
    Hsla::from(gpui_kit::Rgba { r, g, b, a })
}

impl Scene {
    /// Makes the list's fills and lines into paths: slow, so never on the main thread.
    pub fn new(list: DisplayList, view: View) -> Scene {
        let mut shapes = Vec::new();
        let mut labels = Vec::new();
        for item in list.items {
            match item {
                Item::Fill { clip, rings, color, .. } => {
                    for chunk in chunks(&rings, FILL_POINTS) {
                        let mut builder = PathBuilder::fill().with_style(PathStyle::Fill(FillOptions::even_odd()));
                        for ring in chunk {
                            builder.add_polygon(&points(ring), true);
                        }
                        push(&mut shapes, builder, color, clip);
                    }
                }
                Item::Line { clip, lines, width, color, round, .. } => {
                    let lines: Vec<&[[f32; 2]]> = lines.iter().flat_map(|line| pieces(line, LINE_POINTS)).collect();
                    for chunk in chunks(&lines, LINE_POINTS) {
                        let mut builder = stroke(width, round);
                        for line in chunk {
                            builder.add_polygon(&points(line), false);
                        }
                        push(&mut shapes, builder, color, clip);
                    }
                }
                Item::Text(label) => labels.push(label),
                Item::Circle { .. } => {}
            }
        }
        Scene { view, background: hsla(list.background), shapes, labels }
    }

    /// Whether any tile drew.
    pub fn has_tiles(&self) -> bool {
        !self.shapes.is_empty()
    }

    /// Paints the scene in `bounds` as the map looks from `now`: moved and scaled from the
    /// camera it was made for.
    pub fn paint(&self, now: &View, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let shift = Shift::new(&self.view, now, bounds.origin);
        for shape in &self.shapes {
            let mut path = shape.path.clone();
            for vertex in &mut path.vertices {
                vertex.xy_position = shift.point(vertex.xy_position);
            }
            path.bounds =
                Bounds::from_corners(shift.point(path.bounds.origin), shift.point(path.bounds.bottom_right()));
            let clip = shift.rect(shape.clip).intersect(&bounds);
            window.with_content_mask(Some(ContentMask { bounds: clip }), |window| window.paint_path(path, shape.color));
        }
        for label in &self.labels {
            let at = shift.point(point(px(label.x), px(label.top)));
            paint_label(label, point(at.x - px(label.x), at.y - px(label.top)), window, cx);
        }
    }
}

fn chunks<T: AsRef<[[f32; 2]]>>(parts: &[T], most: usize) -> Vec<&[T]> {
    let mut out = Vec::new();
    let (mut start, mut count) = (0, 0);
    for (index, part) in parts.iter().enumerate() {
        if count > 0 && count + part.as_ref().len() > most {
            out.push(&parts[start..index]);
            (start, count) = (index, 0);
        }
        count += part.as_ref().len();
    }
    if start < parts.len() {
        out.push(&parts[start..]);
    }
    out
}

/// A line in pieces of at most `most` points, each starting where the last ended.
fn pieces(line: &[[f32; 2]], most: usize) -> Vec<&[[f32; 2]]> {
    let step = most.max(2) - 1;
    (0..line.len().saturating_sub(1)).step_by(step).map(|start| &line[start..(start + most).min(line.len())]).collect()
}

fn points(line: &[[f32; 2]]) -> Vec<Point<Pixels>> {
    line.iter().map(|[x, y]| point(px(*x), px(*y))).collect()
}

fn stroke(width: f32, round: bool) -> PathBuilder {
    let options = StrokeOptions::default().with_line_width(width);
    let options = match round {
        true => options.with_line_cap(LineCap::Round).with_line_join(LineJoin::Round),
        false => options,
    };
    PathBuilder::stroke(px(width)).with_style(PathStyle::Stroke(options))
}

fn push(shapes: &mut Vec<Shape>, builder: PathBuilder, color: Rgba, clip: Rect) {
    match builder.build() {
        Ok(path) => shapes.push(Shape { path, color: hsla(color), clip }),
        Err(error) => tracing::debug!("a map path couldn't be made: {error}"),
    }
}

/// How a point drawn for one camera lies for another, in the window.
struct Shift {
    scale: f32,
    from: [f32; 2],
    to: [f32; 2],
}

impl Shift {
    fn new(drawn: &View, now: &View, origin: Point<Pixels>) -> Shift {
        let scale = (now.camera.zoom - drawn.camera.zoom).exp2();
        let (was, is) = (world(drawn.camera.lat, drawn.camera.lng), world(now.camera.lat, now.camera.lng));
        let zoom = now.camera.zoom.exp2();
        let to = [
            (now.width / 2. + (was[0] - is[0]) * zoom) as f32 + f32::from(origin.x),
            (now.height / 2. + (was[1] - is[1]) * zoom) as f32 + f32::from(origin.y),
        ];
        Shift { scale: scale as f32, from: [drawn.width as f32 / 2., drawn.height as f32 / 2.], to }
    }

    fn point(&self, at: Point<Pixels>) -> Point<Pixels> {
        point(
            px((f32::from(at.x) - self.from[0]) * self.scale + self.to[0]),
            px((f32::from(at.y) - self.from[1]) * self.scale + self.to[1]),
        )
    }

    fn rect(&self, [left, top, right, bottom]: Rect) -> Bounds<Pixels> {
        Bounds::from_corners(self.point(point(px(left), px(top))), self.point(point(px(right), px(bottom))))
    }
}

/// Paints items as they are, moved by `offset`: the route and the pins, every frame.
pub fn paint_items(items: &[Item], bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let offset = bounds.origin;
    let moved = |[x, y]: [f32; 2]| [x + f32::from(offset.x), y + f32::from(offset.y)];
    for item in items {
        match item {
            Item::Line { lines, width, color, round, .. } => {
                let mut builder = stroke(*width, *round);
                for line in lines {
                    builder.add_polygon(&points(&line.iter().map(|at| moved(*at)).collect::<Vec<_>>()), false);
                }
                if let Ok(path) = builder.build() {
                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        window.paint_path(path, hsla(*color))
                    });
                }
            }
            Item::Fill { rings, color, .. } => {
                let mut builder = PathBuilder::fill().with_style(PathStyle::Fill(FillOptions::even_odd()));
                for ring in rings {
                    builder.add_polygon(&points(&ring.iter().map(|at| moved(*at)).collect::<Vec<_>>()), true);
                }
                if let Ok(path) = builder.build() {
                    window.paint_path(path, hsla(*color));
                }
            }
            Item::Circle { center, radius, color, stroke, blur, .. } => {
                paint_circle(moved(*center), *radius, *color, *stroke, *blur, window)
            }
            Item::Text(label) => paint_label(label, offset, window, cx),
        }
    }
}

/// A circle, its ring outside its radius; a blurred one fades out over `blur` of its radius, as
/// MapLibre's `circle-blur`, in a few steps.
fn paint_circle(
    center: [f32; 2],
    radius: f32,
    color: Option<Rgba>,
    stroke: Option<(Rgba, f32)>,
    blur: f32,
    window: &mut Window,
) {
    let disc = |reach: f32| {
        Bounds::new(point(px(center[0] - reach), px(center[1] - reach)), size(px(reach * 2.), px(reach * 2.)))
    };
    if blur > 0. {
        let Some([r, g, b, a]) = color else { return };
        const STEPS: usize = 6;
        let each = 1. - (1. - a).powf(1. / STEPS as f32);
        for step in 0..STEPS {
            let reach = radius * (1. - blur) + radius * blur * (step as f32 + 0.5) / STEPS as f32;
            window.paint_quad(fill(disc(reach), hsla([r, g, b, each])).corner_radii(Corners::all(px(reach))));
        }
        return;
    }
    let width = stroke.map_or(0., |(_, width)| width);
    let reach = radius + width;
    let fill = color.map_or(transparent_black(), hsla);
    let edge = stroke.map_or(transparent_black(), |(color, _)| hsla(color));
    window.paint_quad(quad(
        disc(reach),
        Corners::all(px(reach)),
        fill,
        Edges::all(px(width)),
        edge,
        BorderStyle::default(),
    ));
}

/// A label's lines, each centred on its `x`, in DM Sans or the system's emoji, with its halo
/// drawn around it first.
pub fn paint_label(label: &Label, offset: Point<Pixels>, window: &mut Window, cx: &mut App) {
    let weight = if label.bold { FontWeight::BOLD } else { FontWeight::NORMAL };
    let line_height = px(label.line_height);
    for (index, line) in label.lines.iter().enumerate() {
        let top = offset.y + px(label.top + label.line_height * index as f32);
        let letters: Vec<String> = match label.spacing > 0. {
            true => line.chars().map(String::from).collect(),
            false => vec![line.clone()],
        };
        let shape = |text: &str, color: Hsla, window: &mut Window| {
            let text = SharedString::from(text.to_owned());
            let run = TextRun {
                len: text.len(),
                font: font(UI_FONT, weight, false),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window.text_system().shape_line(text, px(label.size), &[run], None)
        };
        let color = hsla(label.color);
        let shaped: Vec<ShapedLine> = letters.iter().map(|letter| shape(letter, color, window)).collect();
        let width: f32 = shaped.iter().map(|line| f32::from(line.width)).sum::<f32>()
            + label.spacing * letters.len().saturating_sub(1) as f32;
        let left = offset.x + px(label.x - width / 2.);
        if let Some((halo, reach)) = label.halo {
            let halo = hsla(halo);
            let halos: Vec<ShapedLine> = letters.iter().map(|letter| shape(letter, halo, window)).collect();
            let diagonal = reach * std::f32::consts::FRAC_1_SQRT_2;
            let around = [
                (reach, 0.),
                (-reach, 0.),
                (0., reach),
                (0., -reach),
                (diagonal, diagonal),
                (-diagonal, diagonal),
                (diagonal, -diagonal),
                (-diagonal, -diagonal),
            ];
            for (dx, dy) in around {
                paint_letters(&halos, left + px(dx), top + px(dy), label.spacing, line_height, window, cx);
            }
        }
        paint_letters(&shaped, left, top, label.spacing, line_height, window, cx);
    }
}

fn paint_letters(
    letters: &[ShapedLine],
    left: Pixels,
    top: Pixels,
    spacing: f32,
    line_height: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    let mut x = left;
    for letter in letters {
        let _ = letter.paint(point(x, top), line_height, TextAlign::Left, None, window, cx);
        x += letter.width + px(spacing);
    }
}

#[cfg(test)]
mod tests {
    use super::{chunks, pieces};

    #[test]
    fn long_lists_are_cut_in_parts_that_fit_a_path() {
        let rings = vec![vec![[0., 0.]; 10]; 5];
        let parts: Vec<usize> = chunks(&rings, 25).iter().map(|part| part.len()).collect();
        assert_eq!(parts, [2, 2, 1]);
        let huge = vec![vec![[0., 0.]; 40]];
        assert_eq!(chunks(&huge, 25).len(), 1, "a part larger than the most stays whole");
        assert!(chunks::<Vec<[f32; 2]>>(&[], 25).is_empty());
        let line: Vec<[f32; 2]> = (0..7).map(|x| [x as f32, 0.]).collect();
        let cut: Vec<Vec<f32>> = pieces(&line, 3).iter().map(|piece| piece.iter().map(|at| at[0]).collect()).collect();
        assert_eq!(cut, [vec![0., 1., 2.], vec![2., 3., 4.], vec![4., 5., 6.]]);
    }
}
