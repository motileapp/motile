//! What a canvas component draws: shapes and labels in points from its top left corner, which a
//! client paints in order, and the regions under the pointer that light parts of it and show a
//! tooltip. Colours name tokens, so a scene serves light and dark alike.

use std::borrow::Cow;

use kurbo::{Affine, Point};
use serde::de::Deserializer;
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Scene {
    pub width: f32,
    pub height: f32,
    /// A formula's baseline from the top, which an inline one shares with the text around it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<f32>,
    /// What assistive technologies read for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub items: Vec<Item>,
    /// The last one that holds the pointer wins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hits: Vec<Hit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legend: Option<Legend>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<Motion>,
}

impl Scene {
    pub fn empty(width: f64, height: f64) -> Scene {
        Scene { width: round(width), height: round(height), ..Scene::default() }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
    Path {
        #[serde(flatten)]
        path: Path,
        #[serde(flatten)]
        paint: Paint,
    },
    /// Its corners are ellipses `rx` wide and `ry` tall, already no more than half its sides.
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        #[serde(default, skip_serializing_if = "is_zero")]
        rx: f32,
        #[serde(default, skip_serializing_if = "is_zero")]
        ry: f32,
        #[serde(flatten)]
        paint: Paint,
    },
    Circle {
        cx: f32,
        cy: f32,
        r: f32,
        #[serde(flatten)]
        paint: Paint,
    },
    Text(Text),
}

/// A path as verbs, one letter each (`M` move, `L` line, `Q` quadratic, `C` cubic, `Z` close), and
/// the points they take in order, x then y: one for `M` and `L`, two for `Q`, three for `C`.
/// Filled by the nonzero rule.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Path {
    pub verbs: String,
    pub points: Vec<f32>,
}

/// How a shape is painted: its fill (a colour or a gradient), then its stroke, both at `opacity`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Paint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Gradient>,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub fill_opacity: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub opacity: f32,
}

impl Paint {
    pub fn fill(color: Color) -> Paint {
        Paint { fill: Some(color), ..Paint::default() }
    }

    pub fn stroke(stroke: Stroke) -> Paint {
        Paint { stroke: Some(stroke), ..Paint::default() }
    }
}

impl Default for Paint {
    fn default() -> Paint {
        Paint { fill: None, gradient: None, fill_opacity: 1.0, stroke: None, opacity: 1.0 }
    }
}

/// Miters are limited at 4, as in SVG.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub cap: Cap,
    #[serde(default, skip_serializing_if = "is_default")]
    pub join: Join,
    /// Dash and gap lengths, starting with a dash.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dash: Vec<f32>,
}

impl Stroke {
    pub fn new(color: Color, width: f64) -> Stroke {
        Stroke { color, width: round(width), cap: Cap::Butt, join: Join::Miter, dash: Vec::new() }
    }

    pub fn round(mut self) -> Stroke {
        self.cap = Cap::Round;
        self.join = Join::Round;
        self
    }

    pub fn dashed(mut self, dash: &[f64]) -> Stroke {
        self.dash = dash.iter().map(|length| round(*length)).collect();
        self
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Cap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Join {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// A linear gradient from (`x1`, `y1`) to (`x2`, `y2`), padded beyond its ends.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Gradient {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub stops: Vec<Stop>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Stop {
    /// From 0 at the start to 1 at the end.
    pub at: f32,
    pub color: Color,
    pub opacity: f32,
}

/// A line of text whose baseline starts, centres or ends at (`x`, `y`), by `anchor`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Text {
    pub text: String,
    pub x: f32,
    pub y: f32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub anchor: Anchor,
    #[serde(flatten)]
    pub font: Font,
    pub color: Color,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub opacity: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    #[default]
    Start,
    Middle,
    End,
}

/// The text styles of `crates/openui/LAYOUT.md`, or the UI typeface at a size.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Font {
    /// 11 regular, tabular digits.
    Tick,
    /// 12 regular.
    Caption,
    Ui {
        size: f32,
        #[serde(default, skip_serializing_if = "is_default")]
        weight: Weight,
    },
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Weight {
    #[default]
    Regular,
    Medium,
    Semibold,
}

/// A colour: `"current"`, the text's colour where the scene lies; a token of
/// `packages/theme/tokens.json` by name (`"chart-1"`); `{"wash": token}`, the token at the
/// `color-tint` opacity; `{"surface": layer}`, a layer of the surface the scene lies on; or
/// `[r, g, b, a]`.
#[derive(Clone, PartialEq, Debug)]
pub enum Color {
    Current,
    Token(Cow<'static, str>),
    Wash(Cow<'static, str>),
    Surface(Layer),
    Rgba([u8; 4]),
}

impl Color {
    pub const fn token(name: &'static str) -> Color {
        Color::Token(Cow::Borrowed(name))
    }
}

/// The parts of the surface a scene lies on, as `Surface` names them in the clients.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    /// The surface's own colour.
    Base,
    Border,
    Row,
    Control,
    Box,
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Color::Current => serializer.serialize_str("current"),
            Color::Token(name) => serializer.serialize_str(name),
            Color::Wash(name) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("wash", name)?;
                map.end()
            }
            Color::Surface(layer) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("surface", layer)?;
                map.end()
            }
            Color::Rgba(rgba) => rgba.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Color, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Json {
            Name(String),
            Rgba([u8; 4]),
            Wash { wash: String },
            Surface { surface: Layer },
        }
        Ok(match Json::deserialize(deserializer)? {
            Json::Name(name) if name == "current" => Color::Current,
            Json::Name(name) => Color::Token(Cow::Owned(name)),
            Json::Rgba(rgba) => Color::Rgba(rgba),
            Json::Wash { wash } => Color::Wash(Cow::Owned(wash)),
            Json::Surface { surface } => Color::Surface(surface),
        })
    }
}

/// A part of the scene under the pointer (a tap on touch): while it is there, `lit` is painted
/// before `items[at]` (after them all when `at` is their count) and `tip` shows.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Hit {
    #[serde(flatten)]
    pub region: Region,
    pub at: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lit: Vec<Item>,
    pub tip: Tip,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Region {
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Circle {
        cx: f32,
        cy: f32,
        r: f32,
    },
    /// A ring's part between the radii `inner` and `outer`, from the angle `start` to `end`, in
    /// radians clockwise from the right (y grows down), `start` ≤ `end` ≤ `start` + 2π.
    Sector {
        cx: f32,
        cy: f32,
        inner: f32,
        outer: f32,
        start: f32,
        end: f32,
    },
}

/// A tooltip, centred on `x` with its top at `y`: its title, then a row for each value.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Tip {
    pub x: f32,
    pub y: f32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<TipRow>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct TipRow {
    /// The dot before the name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub value: String,
}

/// A chart's key, which the client lays out: under the plot, or `beside` a pie's disc.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Legend {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub beside: bool,
    pub entries: Vec<LegendEntry>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct LegendEntry {
    pub color: Color,
    pub label: String,
    /// A pie slice's share of the whole, "42.5%".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
}

/// What a client animates rather than paints: Celebration's confetti and Flashcard's flip.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Motion {
    Confetti(Confetti),
    Flip(Flip),
}

/// Pieces that fall over the whole window. Each starts with its top at `top`, its left at its `x`
/// share of the window's width, after its `delay`, and over its `duration` moves `drift` across
/// and `fall` window heights down, turns by `turn` degrees about its centre and fades to
/// `end_opacity`, on the cubic Bézier `curve`; it then stays until `duration` (all in seconds).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Confetti {
    pub duration: f32,
    pub curve: [f32; 4],
    pub width: f32,
    pub height: f32,
    pub radius: f32,
    pub top: f32,
    pub fall: f32,
    pub end_opacity: f32,
    pub pieces: Vec<Piece>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Piece {
    pub x: f32,
    pub color: Color,
    pub delay: f32,
    pub duration: f32,
    pub drift: f32,
    pub turn: f32,
}

/// A card turning over as a horizontal squash about its centre, `duration` seconds long: `frames`
/// give its width's scale at moments of the turn (0 to 1, in time), to interpolate linearly
/// between; the back shows from `swap_at` on. `curve` is the cubic Bézier the frames follow.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Flip {
    pub duration: f32,
    pub curve: [f32; 4],
    pub swap_at: f32,
    pub frames: Vec<[f32; 2]>,
}

/// Builds a [`Path`], each point through `transform`.
pub struct PathBuilder {
    transform: Affine,
    path: Path,
    start: Option<Point>,
    closed: bool,
}

impl PathBuilder {
    pub fn new(transform: Affine) -> PathBuilder {
        PathBuilder { transform, path: Path::default(), start: None, closed: false }
    }

    pub fn set_transform(&mut self, transform: Affine) {
        self.transform = transform;
    }

    /// How finely curves are made of cubics in the builder's own units, for a hundredth of a point.
    pub fn tolerance(&self) -> f64 {
        let [a, b, c, d, _, _] = self.transform.as_coeffs();
        0.01 / (a.hypot(b).max(c.hypot(d))).max(1e-9)
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        self.path.verbs.push('M');
        self.push(Point::new(x, y));
        self.start = Some(Point::new(x, y));
        self.closed = false;
    }

    pub fn line_to(&mut self, x: f64, y: f64) {
        self.reopen();
        self.path.verbs.push('L');
        self.push(Point::new(x, y));
    }

    pub fn quad_to(&mut self, x1: f64, y1: f64, x: f64, y: f64) {
        self.reopen();
        self.path.verbs.push('Q');
        self.push(Point::new(x1, y1));
        self.push(Point::new(x, y));
    }

    pub fn cubic_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
        self.reopen();
        self.path.verbs.push('C');
        self.push(Point::new(x1, y1));
        self.push(Point::new(x2, y2));
        self.push(Point::new(x, y));
    }

    pub fn close(&mut self) {
        if self.start.is_none() || self.closed {
            return;
        }
        self.path.verbs.push('Z');
        self.closed = true;
    }

    /// An SVG arc from the current point, its rotation in radians; a line when it can't be one.
    pub fn arc(&mut self, arc: kurbo::SvgArc) {
        let Some(curve) = kurbo::Arc::from_svg_arc(&arc) else {
            if arc.from != arc.to {
                self.line_to(arc.to.x, arc.to.y);
            }
            return;
        };
        let tolerance = self.tolerance();
        curve.to_cubic_beziers(tolerance, |p1, p2, p| self.cubic_to(p1.x, p1.y, p2.x, p2.y, p.x, p.y));
    }

    /// An ellipse, clockwise from its rightmost point.
    pub fn ellipse(&mut self, cx: f64, cy: f64, rx: f64, ry: f64) {
        self.rounded_rect(cx - rx, cy - ry, rx * 2.0, ry * 2.0, rx, ry);
    }

    /// A rectangle whose corners are quarter ellipses, clockwise from the end of its top left one.
    pub fn rounded_rect(&mut self, x: f64, y: f64, width: f64, height: f64, rx: f64, ry: f64) {
        const KAPPA: f64 = 0.552_284_749_830_793_6;
        let (right, bottom) = (x + width, y + height);
        let (kx, ky) = (rx * KAPPA, ry * KAPPA);
        self.move_to(x + rx, y);
        self.line_to(right - rx, y);
        if rx > 0.0 && ry > 0.0 {
            self.cubic_to(right - rx + kx, y, right, y + ry - ky, right, y + ry);
        }
        self.line_to(right, bottom - ry);
        if rx > 0.0 && ry > 0.0 {
            self.cubic_to(right, bottom - ry + ky, right - rx + kx, bottom, right - rx, bottom);
        }
        self.line_to(x + rx, bottom);
        if rx > 0.0 && ry > 0.0 {
            self.cubic_to(x + rx - kx, bottom, x, bottom - ry + ky, x, bottom - ry);
        }
        self.line_to(x, y + ry);
        if rx > 0.0 && ry > 0.0 {
            self.cubic_to(x, y + ry - ky, x + rx - kx, y, x + rx, y);
        }
        self.close();
    }

    pub fn is_empty(&self) -> bool {
        !self.path.verbs.contains(['L', 'Q', 'C'])
    }

    pub fn finish(self) -> Option<Path> {
        if self.is_empty() {
            return None;
        }
        Some(self.path)
    }

    /// A drawing command right after a close starts from where the closed part began.
    fn reopen(&mut self) {
        if !self.closed {
            return;
        }
        let Some(start) = self.start else { return };
        self.move_to(start.x, start.y);
    }

    fn push(&mut self, point: Point) {
        let point = self.transform * point;
        self.path.points.push(round(point.x));
        self.path.points.push(round(point.y));
    }
}

/// A length for the scene: a hundredth of a point is finer than any screen.
pub fn round(value: f64) -> f32 {
    ((value * 100.0).round() / 100.0) as f32
}

fn one() -> f32 {
    1.0
}

fn is_one(value: &f32) -> bool {
    *value == 1.0
}

fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}
