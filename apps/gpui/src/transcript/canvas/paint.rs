//! Painting a scene: its shapes, tessellated once off the main thread in the scene's points and
//! moved to where it lies, and its labels in the app's typeface.

use gpui_kit::*;
use kurbo::{BezPath, PathEl, Shape};
use lyon::tessellation::{LineCap, LineJoin};
use motile_core::render::openui::canvas::{
    Anchor, Cap, Color, Font as Role, Gradient, Item, Join, Layer as SceneLayer, Paint, Path as ScenePath, Scene, Stop,
    Stroke, Text, Weight,
};

use crate::theme::{Colors, Layer, Opacity, Surface, UI_FONT};
use crate::transcript::rows::font;
use crate::ui::tabular_numbers;

/// How far a curve's segments may stray from it, in points: finer than a pixel at any scale.
const TOLERANCE: f32 = 0.02;
const MITER_LIMIT: f32 = 4.;

/// The colours a scene takes where it lies: the appearance's tokens, the surface under it and the
/// colour of the text around it.
#[derive(Clone, Copy)]
pub struct Palette {
    pub colors: &'static Colors,
    pub surface: Surface,
    pub current: Hsla,
}

impl Palette {
    pub fn new(colors: &'static Colors, surface: Surface) -> Self {
        Self { colors, surface, current: colors.foreground }
    }

    pub fn current(self, current: Hsla) -> Self {
        Self { current, ..self }
    }

    /// The colour at `opacity` of its own alpha.
    pub fn resolve(&self, color: &Color, opacity: f32) -> Hsla {
        let c = self.colors;
        let named = |name: &str| c.named(name).unwrap_or(c.foreground);
        let resolved = match color {
            Color::Current => self.current,
            Color::Token(name) => named(name),
            Color::Wash(name) => c.at(named(name), Opacity::ColorTint),
            Color::Surface(layer) => match layer {
                SceneLayer::Base => self.surface.color(c),
                SceneLayer::Border => self.surface.border(c),
                SceneLayer::Row => self.surface.layer(Layer::Row, c),
                SceneLayer::Control => self.surface.layer(Layer::Control, c),
                SceneLayer::Box => self.surface.layer(Layer::Box, c),
            },
            Color::Rgba([r, g, b, a]) => {
                let channel = |value: u8| f32::from(value) / 255.;
                Rgba { r: channel(*r), g: channel(*g), b: channel(*b), a: channel(*a) }.into()
            }
        };
        resolved.opacity(opacity.clamp(0., 1.))
    }
}

/// What a fill or a stroke is painted in: a colour at an opacity, or a gradient of no more than
/// two stops, which GPUI draws, at the paint's opacity.
#[derive(Clone)]
enum Ink {
    Color(Color, f32),
    Gradient(Gradient, f32),
}

enum Mark {
    Shape(Path<Pixels>, Ink),
    Text(Text),
}

/// A scene with its shapes tessellated, in its own points: its items, and the items each hit
/// lights.
pub struct Prepared {
    pub scene: Scene,
    items: Vec<Vec<Mark>>,
    lit: Vec<Vec<Mark>>,
}

impl Prepared {
    /// Takes the time tessellating does, so it belongs off the main thread.
    pub fn new(scene: Scene) -> Self {
        let items = scene.items.iter().map(marks).collect();
        let lit = scene.hits.iter().map(|hit| hit.lit.iter().flat_map(marks).collect()).collect();
        Self { scene, items, lit }
    }
}

/// Paints the scene with its top left corner at `origin` and the hit `lit` lit, as the reference
/// painter does: its items in order, the lit ones just before the item the hit names.
pub fn paint_scene(
    prepared: &Prepared,
    origin: Point<Pixels>,
    palette: &Palette,
    lit: Option<usize>,
    window: &mut Window,
    cx: &mut App,
) {
    let hit = lit.and_then(|index| Some((prepared.scene.hits.get(index)?.at as usize, prepared.lit.get(index)?)));
    for (index, marks) in prepared.items.iter().enumerate() {
        if let Some((_, lit)) = hit.filter(|(at, _)| *at == index) {
            paint_marks(lit, origin, palette, window, cx);
        }
        paint_marks(marks, origin, palette, window, cx);
    }
    if let Some((_, lit)) = hit.filter(|(at, _)| *at >= prepared.items.len()) {
        paint_marks(lit, origin, palette, window, cx);
    }
}

fn paint_marks(marks: &[Mark], origin: Point<Pixels>, palette: &Palette, window: &mut Window, cx: &mut App) {
    for mark in marks {
        match mark {
            Mark::Shape(path, ink) => paint_shape(path, ink, origin, palette, window),
            Mark::Text(text) => paint_text(text, origin, palette, window, cx),
        }
    }
}

fn paint_shape(path: &Path<Pixels>, ink: &Ink, origin: Point<Pixels>, palette: &Palette, window: &mut Window) {
    let path = placed(path, origin);
    let background = match ink {
        Ink::Color(color, opacity) => solid_background(palette.resolve(color, *opacity)),
        Ink::Gradient(gradient, opacity) => {
            let clipped = path.bounds.intersect(&window.content_mask().bounds);
            if clipped.size.width <= px(0.) || clipped.size.height <= px(0.) {
                return;
            }
            gradient_background(gradient, *opacity, origin, clipped, palette)
        }
    };
    window.paint_path(path, background);
}

fn placed(path: &Path<Pixels>, origin: Point<Pixels>) -> Path<Pixels> {
    let mut path = path.clone();
    for vertex in &mut path.vertices {
        vertex.xy_position += origin;
    }
    path.bounds.origin += origin;
    path
}

/// GPUI's gradient on a path, which runs at an angle across the path's bounds clipped to what is
/// shown, made to run from the gradient's start to its end in the scene: the angle is set so
/// that GPUI's direction, which it stretches to the bounds' longer side, is the gradient's, and
/// the stops are put where the gradient's lie along it.
fn gradient_background(
    gradient: &Gradient,
    opacity: f32,
    origin: Point<Pixels>,
    bounds: Bounds<Pixels>,
    palette: &Palette,
) -> Background {
    let stop_color = |stop: &Stop| palette.resolve(&stop.color, stop.opacity * opacity);
    let (Some(first), Some(last)) = (gradient.stops.first(), gradient.stops.last()) else {
        return solid_background(transparent_black());
    };
    let (dx, dy) = (gradient.x2 - gradient.x1, gradient.y2 - gradient.y1);
    let length = dx.hypot(dy);
    if length < 1e-4 || gradient.stops.len() == 1 {
        return solid_background(stop_color(last));
    }
    let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let (stretch_x, stretch_y) = if width > height { (1., height / width) } else { (width / height, 1.) };
    let turn = (dy / stretch_y).atan2(dx / stretch_x);
    let direction = (turn.cos() * stretch_x, turn.sin() * stretch_y);
    let reach = if direction.0.abs() > direction.1.abs() { width } else { height };
    let unit = (dx / length, dy / length);
    let start = (f32::from(origin.x) + gradient.x1, f32::from(origin.y) + gradient.y1);
    let center = bounds.center();
    let from_center = (start.0 - f32::from(center.x)) * unit.0 + (start.1 - f32::from(center.y)) * unit.1;
    let at = |share: f32| (from_center + share * length + reach / 2.) / reach;
    let end = if last.at - first.at < 1e-4 { first.at + 1e-4 } else { last.at };
    linear_gradient(
        turn.to_degrees() + 90.,
        linear_color_stop(stop_color(first), at(first.at)),
        linear_color_stop(stop_color(last), at(end)),
    )
}

fn paint_text(text: &Text, origin: Point<Pixels>, palette: &Palette, window: &mut Window, cx: &mut App) {
    if text.text.is_empty() {
        return;
    }
    let (font, size) = text_font(&text.font);
    let color = palette.resolve(&text.color, text.opacity);
    let run =
        TextRun { len: text.text.len(), font, color, background_color: None, underline: None, strikethrough: None };
    let shaped = window.text_system().shape_line(text.text.clone().into(), px(size), &[run], None);
    let left = match text.anchor {
        Anchor::Start => px(text.x),
        Anchor::Middle => px(text.x) - shaped.width / 2.,
        Anchor::End => px(text.x) - shaped.width,
    };
    let top = origin.y + px(text.y) - shaped.ascent;
    let line_height = shaped.ascent + shaped.descent;
    let _ = shaped.paint(point(origin.x + left, top), line_height, TextAlign::Left, None, window, cx);
}

/// The app's typeface for a label's role, and its size.
pub fn text_font(role: &Role) -> (Font, f32) {
    match role {
        Role::Tick => (Font { features: tabular_numbers(), ..font(UI_FONT, FontWeight::NORMAL, false) }, 11.),
        Role::Caption => (font(UI_FONT, FontWeight::NORMAL, false), 12.),
        Role::Ui { size, weight } => (font(UI_FONT, font_weight(*weight), false), *size),
    }
}

fn font_weight(weight: Weight) -> FontWeight {
    match weight {
        Weight::Regular => FontWeight::NORMAL,
        Weight::Medium => FontWeight::MEDIUM,
        Weight::Semibold => FontWeight::SEMIBOLD,
    }
}

fn marks(item: &Item) -> Vec<Mark> {
    match item {
        Item::Path { path, paint } => shapes(bez_path(path), paint),
        Item::Rect { x, y, width, height, rx, ry, paint } => shapes(
            Some(rounded_rect(*x as f64, *y as f64, *width as f64, *height as f64, *rx as f64, *ry as f64)),
            paint,
        ),
        Item::Circle { cx, cy, r, paint } => {
            shapes(Some(kurbo::Circle::new((*cx as f64, *cy as f64), *r as f64).to_path(0.001)), paint)
        }
        Item::Text(text) => vec![Mark::Text(text.clone())],
    }
}

/// A shape's fill, then its stroke. A gradient isn't faded by `fill_opacity`.
fn shapes(path: Option<BezPath>, paint: &Paint) -> Vec<Mark> {
    let Some(path) = path else { return Vec::new() };
    let mut marks = Vec::new();
    match (paint.gradient.as_ref().filter(|gradient| !gradient.stops.is_empty()), &paint.fill) {
        (Some(gradient), _) => marks.extend(gradient_bands(&path, gradient, paint.opacity)),
        (None, Some(color)) => {
            if let Some(filled) = filled(&path) {
                marks.push(Mark::Shape(filled, Ink::Color(color.clone(), paint.fill_opacity * paint.opacity)));
            }
        }
        (None, None) => {}
    }
    if let Some(stroke) = &paint.stroke
        && let Some(stroked) = stroked(&path, stroke)
    {
        marks.push(Mark::Shape(stroked, Ink::Color(stroke.color.clone(), paint.opacity)));
    }
    marks
}

/// A gradient fill as GPUI paints it: one shape with two stops, or for more stops, the shape cut
/// across the gradient into a band for each two stops beside each other, the first and the last
/// reaching on past its ends.
fn gradient_bands(path: &BezPath, gradient: &Gradient, opacity: f32) -> Vec<Mark> {
    let Some(filled) = filled(path) else { return Vec::new() };
    if gradient.stops.len() <= 2 {
        return vec![Mark::Shape(filled, Ink::Gradient(gradient.clone(), opacity))];
    }
    let (dx, dy) = (gradient.x2 - gradient.x1, gradient.y2 - gradient.y1);
    let squared = dx * dx + dy * dy;
    if squared < 1e-8 {
        return vec![Mark::Shape(filled, Ink::Gradient(gradient.clone(), opacity))];
    }
    let along =
        |p: Point<Pixels>| ((f32::from(p.x) - gradient.x1) * dx + (f32::from(p.y) - gradient.y1) * dy) / squared;
    let bands = gradient.stops.len() - 1;
    gradient
        .stops
        .windows(2)
        .enumerate()
        .filter(|(_, pair)| pair[1].at > pair[0].at)
        .filter_map(|(index, pair)| {
            let low = (index > 0).then_some(pair[0].at);
            let high = (index + 1 < bands).then_some(pair[1].at);
            let band = cut(&filled, &along, low, high)?;
            let point_at = |share: f32| (gradient.x1 + dx * share, gradient.y1 + dy * share);
            let ((x1, y1), (x2, y2)) = (point_at(pair[0].at), point_at(pair[1].at));
            let stops = vec![Stop { at: 0., ..pair[0].clone() }, Stop { at: 1., ..pair[1].clone() }];
            Some(Mark::Shape(band, Ink::Gradient(Gradient { x1, y1, x2, y2, stops }, opacity)))
        })
        .collect()
}

/// The part of the tessellated shape where `along` lies between `low` and `high`.
fn cut(
    path: &Path<Pixels>,
    along: &impl Fn(Point<Pixels>) -> f32,
    low: Option<f32>,
    high: Option<f32>,
) -> Option<Path<Pixels>> {
    let mut band: Option<Path<Pixels>> = None;
    for triangle in path.vertices.as_chunks::<3>().0 {
        let mut polygon: Vec<Point<Pixels>> = triangle.iter().map(|vertex| vertex.xy_position).collect();
        if let Some(low) = low {
            polygon = clipped(&polygon, |p| along(p) - low);
        }
        if let Some(high) = high {
            polygon = clipped(&polygon, |p| high - along(p));
        }
        for at in 1..polygon.len().saturating_sub(1) {
            let band = band.get_or_insert_with(|| Path::new(polygon[0]));
            let flat = (point(0., 1.), point(0., 1.), point(0., 1.));
            band.push_triangle((polygon[0], polygon[at], polygon[at + 1]), flat);
        }
    }
    band
}

/// The polygon's part where `side` isn't negative.
fn clipped(polygon: &[Point<Pixels>], side: impl Fn(Point<Pixels>) -> f32) -> Vec<Point<Pixels>> {
    let mut kept = Vec::with_capacity(polygon.len() + 2);
    for (index, &from) in polygon.iter().enumerate() {
        let to = polygon[(index + 1) % polygon.len()];
        let (a, b) = (side(from), side(to));
        if a >= 0. {
            kept.push(from);
        }
        if (a >= 0.) != (b >= 0.) {
            let t = a / (a - b);
            kept.push(point(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t));
        }
    }
    kept
}

fn filled(path: &BezPath) -> Option<Path<Pixels>> {
    let options = FillOptions::non_zero().with_tolerance(TOLERANCE);
    tessellated(path.elements(), PathStyle::Fill(options))
}

/// The stroke's outline, dashed as Skia dashes: afresh in each part of the path, and only when its
/// lengths are even in number and add up to something.
fn stroked(path: &BezPath, stroke: &Stroke) -> Option<Path<Pixels>> {
    if stroke.width <= 0. {
        return None;
    }
    let options = StrokeOptions::default()
        .with_line_width(stroke.width)
        .with_line_cap(match stroke.cap {
            Cap::Butt => LineCap::Butt,
            Cap::Round => LineCap::Round,
            Cap::Square => LineCap::Square,
        })
        .with_line_join(match stroke.join {
            Join::Miter => LineJoin::Miter,
            Join::Round => LineJoin::Round,
            Join::Bevel => LineJoin::Bevel,
        })
        .with_miter_limit(MITER_LIMIT)
        .with_tolerance(TOLERANCE);
    let style = PathStyle::Stroke(options);
    let dashes: Vec<f64> = stroke.dash.iter().map(|length| *length as f64).collect();
    let dashed = dashes.len() >= 2
        && dashes.len().is_multiple_of(2)
        && dashes.iter().all(|length| length.is_finite() && *length >= 0.)
        && dashes.iter().sum::<f64>() > 0.;
    if !dashed {
        return tessellated(path.elements(), style);
    }
    let dashed: Vec<PathEl> = kurbo::dash(path.elements().iter().copied(), 0., &dashes).collect();
    tessellated(&dashed, style)
}

pub fn tessellated(elements: &[PathEl], style: PathStyle) -> Option<Path<Pixels>> {
    let at = |p: kurbo::Point| point(px(p.x as f32), px(p.y as f32));
    let mut builder = PathBuilder::fill().with_style(style);
    for element in elements {
        match *element {
            PathEl::MoveTo(p) => builder.move_to(at(p)),
            PathEl::LineTo(p) => builder.line_to(at(p)),
            PathEl::QuadTo(control, p) => builder.curve_to(at(p), at(control)),
            PathEl::CurveTo(a, b, p) => builder.cubic_bezier_to(at(p), at(a), at(b)),
            PathEl::ClosePath => builder.close(),
        }
    }
    builder.build().ok().filter(|path| !path.vertices.is_empty())
}

/// The path of a scene's verbs and points, or none when they don't agree.
fn bez_path(path: &ScenePath) -> Option<BezPath> {
    let mut points = path.points.as_chunks::<2>().0.iter().map(|[x, y]| kurbo::Point::new(*x as f64, *y as f64));
    let mut bez = BezPath::new();
    for verb in path.verbs.chars() {
        match verb {
            'M' => bez.move_to(points.next()?),
            'L' => bez.line_to(points.next()?),
            'Q' => bez.quad_to(points.next()?, points.next()?),
            'C' => bez.curve_to(points.next()?, points.next()?, points.next()?),
            'Z' => bez.close_path(),
            _ => return None,
        }
    }
    bez.elements().iter().any(|element| !matches!(element, PathEl::MoveTo(_) | PathEl::ClosePath)).then_some(bez)
}

/// A rectangle whose corners are quarter ellipses, as core's scenes draw one.
pub fn rounded_rect(x: f64, y: f64, width: f64, height: f64, rx: f64, ry: f64) -> BezPath {
    const KAPPA: f64 = 0.552_284_749_830_793_6;
    let (right, bottom) = (x + width, y + height);
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let round = rx > 0. && ry > 0.;
    let mut path = BezPath::new();
    path.move_to((x + rx, y));
    path.line_to((right - rx, y));
    if round {
        path.curve_to((right - rx + kx, y), (right, y + ry - ky), (right, y + ry));
    }
    path.line_to((right, bottom - ry));
    if round {
        path.curve_to((right, bottom - ry + ky), (right - rx + kx, bottom), (right - rx, bottom));
    }
    path.line_to((x + rx, bottom));
    if round {
        path.curve_to((x + rx - kx, bottom), (x, bottom - ry + ky), (x, bottom - ry));
    }
    path.line_to((x, y + ry));
    if round {
        path.curve_to((x, y + ry - ky), (x + rx - kx, y), (x + rx, y));
    }
    path.close_path();
    path
}
