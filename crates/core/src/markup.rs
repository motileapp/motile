//! Marking up an image to show an agent what to change: pen strokes, arrows, boxes and numbered
//! pins that each carry a note, in the image's pixels. A client passes the pointer in and paints
//! the scene that comes back over the image; the core keeps the marks and their history, writes the
//! image out with them for the composer, and lists the pins' notes for the message.

use std::collections::HashMap;
use std::path::{Path as FilePath, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, RgbaImage};
use kurbo::Affine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::render::openui::canvas::math::label_outline;
use crate::render::openui::canvas::{Cap, Color, Item, Join, Paint, PathBuilder, Scene, Stroke};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Pen,
    Arrow,
    Box,
    Pin,
}

/// The colours marks come in: tokens that are the same in light and dark, since they end up in the
/// image.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Ink {
    Red,
    Yellow,
    Green,
    Blue,
    White,
}

impl Ink {
    const ALL: [Ink; 5] = [Ink::Red, Ink::Yellow, Ink::Green, Ink::Blue, Ink::White];

    fn token(self) -> &'static str {
        match self {
            Ink::Red => "markup-red",
            Ink::Yellow => "markup-yellow",
            Ink::Green => "markup-green",
            Ink::Blue => "markup-blue",
            Ink::White => "markup-white",
        }
    }

    /// What is written and ringed on it.
    fn contrast(self) -> &'static str {
        match self {
            Ink::Yellow | Ink::White => "markup-ink",
            _ => "markup-white",
        }
    }
}

type Point = [f64; 2];

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Mark {
    Pen { ink: Ink, points: Vec<Point> },
    Arrow { ink: Ink, from: Point, to: Point },
    Box { ink: Ink, from: Point, to: Point },
    Pin { ink: Ink, at: Point, note: String },
}

enum Gesture {
    /// The last mark follows the pointer.
    Drawing,
    /// A pin follows the pointer once it has gone further than the slop. `before` is what undo goes
    /// back to, unless placing the pin already recorded it.
    Moving { mark: usize, grab: Point, start: Point, moved: bool, before: Option<Vec<Mark>> },
}

/// One image being marked up.
pub struct Markup {
    width: f64,
    height: f64,
    marks: Vec<Mark>,
    undo: Vec<Vec<Mark>>,
    redo: Vec<Vec<Mark>>,
    gesture: Option<Gesture>,
    selected: Option<usize>,
    slop: f64,
}

const HISTORY: usize = 200;
/// A shadow under every mark, so that it reads on light and dark pictures alike.
const HALO: Color = Color::Rgba([0, 0, 0, 72]);

impl Markup {
    pub fn new(width: f64, height: f64) -> Markup {
        Markup {
            width: width.max(1.0),
            height: height.max(1.0),
            marks: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            gesture: None,
            selected: None,
            slop: 0.0,
        }
    }

    /// The width of a stroke: a 240th of the image's longer side, so marks look alike on any image
    /// fitted to the screen.
    fn unit(&self) -> f64 {
        (self.width.max(self.height) / 240.0).max(2.0)
    }

    fn pin_radius(&self) -> f64 {
        self.unit() * 3.8
    }

    fn clamp(&self, [x, y]: Point) -> Point {
        [x.clamp(0.0, self.width), y.clamp(0.0, self.height)]
    }

    /// Where a pin can be: wholly inside the image, ring and all.
    fn pin_place(&self, point: Point) -> Point {
        place_within(point, self.pin_bounds())
    }

    fn pin_bounds(&self) -> [f64; 4] {
        let inset = (self.pin_radius() + self.unit()).min(self.width / 2.0).min(self.height / 2.0);
        [inset, inset, self.width - inset, self.height - inset]
    }

    fn pin_at(&self, [x, y]: Point) -> Option<usize> {
        let reach = self.pin_radius() + self.slop;
        self.marks.iter().enumerate().rev().find_map(|(index, mark)| match mark {
            Mark::Pin { at: [px, py], .. } if (px - x).hypot(py - y) <= reach => Some(index),
            _ => None,
        })
    }

    fn record(&mut self, before: Vec<Mark>) {
        self.undo.push(before);
        if self.undo.len() > HISTORY {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// The pointer goes down: a pin under it is picked up, otherwise `tool` starts a mark.
    pub fn down(&mut self, tool: Tool, ink: Ink, point: Point, slop: f64) {
        self.slop = slop.max(0.0);
        self.selected = None;
        let point = self.clamp(point);
        if let Some(mark) = self.pin_at(point) {
            let Mark::Pin { at, .. } = self.marks[mark] else { return };
            let grab = [at[0] - point[0], at[1] - point[1]];
            let before = Some(self.marks.clone());
            self.gesture = Some(Gesture::Moving { mark, grab, start: point, moved: false, before });
            return;
        }
        self.record(self.marks.clone());
        self.marks.push(match tool {
            Tool::Pen => Mark::Pen { ink, points: vec![point] },
            Tool::Arrow => Mark::Arrow { ink, from: point, to: point },
            Tool::Box => Mark::Box { ink, from: point, to: point },
            Tool::Pin => Mark::Pin { ink, at: self.pin_place(point), note: String::new() },
        });
        self.gesture = Some(match tool {
            Tool::Pin => {
                self.selected = Some(self.marks.len() - 1);
                Gesture::Moving {
                    mark: self.marks.len() - 1,
                    grab: [0.0, 0.0],
                    start: point,
                    moved: true,
                    before: None,
                }
            }
            _ => Gesture::Drawing,
        });
    }

    pub fn drag(&mut self, point: Point) {
        let point = self.clamp(point);
        let spacing = self.unit() * 0.3;
        let pin_bounds = self.pin_bounds();
        match &mut self.gesture {
            Some(Gesture::Drawing) => match self.marks.last_mut() {
                Some(Mark::Pen { points, .. }) => {
                    let Some(&[x, y]) = points.last() else { return };
                    if (point[0] - x).hypot(point[1] - y) >= spacing {
                        points.push(point);
                    }
                }
                Some(Mark::Arrow { to, .. } | Mark::Box { to, .. }) => *to = point,
                _ => {}
            },
            Some(Gesture::Moving { mark, grab, start, moved, .. }) => {
                *moved = *moved || (point[0] - start[0]).hypot(point[1] - start[1]) > self.slop;
                if !*moved {
                    return;
                }
                let at = place_within([point[0] + grab[0], point[1] + grab[1]], pin_bounds);
                if let Some(Mark::Pin { at: pin, .. }) = self.marks.get_mut(*mark) {
                    *pin = at;
                }
            }
            None => {}
        }
    }

    /// The pointer comes up: an arrow or a box too short to see is dropped, a pin that wasn't
    /// moved is selected, to write its note.
    pub fn up(&mut self, point: Point) {
        self.drag(point);
        let shortest = self.slop.max(self.unit() * 2.0);
        match self.gesture.take() {
            Some(Gesture::Drawing) => {
                let short = match self.marks.last() {
                    Some(Mark::Arrow { from, to, .. } | Mark::Box { from, to, .. }) => {
                        (to[0] - from[0]).hypot(to[1] - from[1]) < shortest
                    }
                    _ => false,
                };
                if short && let Some(before) = self.undo.pop() {
                    self.marks = before;
                }
            }
            Some(Gesture::Moving { mark, moved, before, .. }) => match before {
                Some(before) if moved => self.record(before),
                Some(_) => self.selected = Some(mark),
                None => {}
            },
            None => {}
        }
    }

    /// A second finger came down to zoom: the mark the gesture started is dropped, and a pin it
    /// picked up goes back where it was.
    pub fn cancel(&mut self) {
        self.selected = None;
        match self.gesture.take() {
            Some(Gesture::Moving { before: Some(before), .. }) => self.marks = before,
            Some(_) => {
                if let Some(before) = self.undo.pop() {
                    self.marks = before;
                }
            }
            None => {}
        }
    }

    pub fn select(&mut self, mark: Option<usize>) {
        self.selected = mark.filter(|mark| matches!(self.marks.get(*mark), Some(Mark::Pin { .. })));
    }

    pub fn set_note(&mut self, mark: usize, text: &str) {
        let text = text.trim();
        let Some(Mark::Pin { note, .. }) = self.marks.get(mark) else { return };
        if note == text {
            return;
        }
        self.record(self.marks.clone());
        if let Some(Mark::Pin { note, .. }) = self.marks.get_mut(mark) {
            *note = text.to_owned();
        }
    }

    pub fn remove(&mut self, mark: usize) {
        if mark >= self.marks.len() {
            return;
        }
        self.record(self.marks.clone());
        self.marks.remove(mark);
        self.selected = None;
    }

    pub fn undo(&mut self) {
        let Some(before) = self.undo.pop() else { return };
        self.redo.push(std::mem::replace(&mut self.marks, before));
        (self.selected, self.gesture) = (None, None);
    }

    pub fn redo(&mut self) {
        let Some(after) = self.redo.pop() else { return };
        self.undo.push(std::mem::replace(&mut self.marks, after));
        (self.selected, self.gesture) = (None, None);
    }

    /// The pins in the order they were placed, numbered from 1.
    fn pins(&self) -> impl Iterator<Item = (usize, usize, &Mark)> {
        self.marks
            .iter()
            .enumerate()
            .filter(|(_, mark)| matches!(mark, Mark::Pin { .. }))
            .enumerate()
            .map(|(number, (index, mark))| (number + 1, index, mark))
    }

    /// What the message says of the pins that have a note, by their numbers on `name`.
    pub fn notes(&self, name: &str) -> Option<String> {
        let lines: Vec<String> = self
            .pins()
            .filter_map(|(number, _, mark)| match mark {
                Mark::Pin { note, .. } if !note.is_empty() => Some(format!("{number}. {note}")),
                _ => None,
            })
            .collect();
        if lines.is_empty() {
            return None;
        }
        Some(format!("Notes on {name}:\n{}", lines.join("\n")))
    }

    /// The marks in the image's pixels, with the selected pin ringed unless it is `flat`.
    pub fn scene(&self, flat: bool) -> Scene {
        let mut items = Vec::new();
        let unit = self.unit();
        let mut number = 0;
        for (index, mark) in self.marks.iter().enumerate() {
            match mark {
                Mark::Pen { ink, points } => pen(&mut items, *ink, points, unit),
                Mark::Arrow { ink, from, to } => arrow(&mut items, *ink, *from, *to, unit),
                Mark::Box { ink, from, to } => rect(&mut items, *ink, *from, *to, unit),
                Mark::Pin { ink, at, .. } => {
                    number += 1;
                    let selected = !flat && self.selected == Some(index);
                    pin(&mut items, *ink, *at, number, self.pin_radius(), unit, selected);
                }
            }
        }
        Scene { items, ..Scene::empty(self.width, self.height) }
    }

    fn state(&self, id: u64) -> Value {
        let pins: Vec<Value> = self
            .pins()
            .filter_map(|(number, index, mark)| match mark {
                Mark::Pin { ink, at, note } => Some(json!({
                    "mark": index, "number": number, "x": at[0], "y": at[1], "radius": self.pin_radius(), "note": note,
                    "ink": ink,
                })),
                _ => None,
            })
            .collect();
        json!({
            "id": id,
            "width": self.width,
            "height": self.height,
            "scene": self.scene(false),
            "pins": pins,
            "selected": self.selected,
            "can_undo": !self.undo.is_empty(),
            "can_redo": !self.redo.is_empty(),
            "marked": !self.marks.is_empty(),
            "inks": Ink::ALL,
        })
    }
}

fn place_within([x, y]: Point, [left, top, right, bottom]: [f64; 4]) -> Point {
    [x.clamp(left, right), y.clamp(top, bottom)]
}

fn stroke(color: Color, width: f64) -> Stroke {
    Stroke::new(color, width).round()
}

fn ink(ink: Ink) -> Color {
    Color::token(ink.token())
}

fn path(items: &mut Vec<Item>, build: impl Fn(&mut PathBuilder), paint: Paint) {
    let mut builder = PathBuilder::new(Affine::IDENTITY);
    build(&mut builder);
    items.extend(builder.finish().map(|path| Item::Path { path, paint }));
}

/// A stroke through the midpoints of its points, which their points bend it towards.
fn pen(items: &mut Vec<Item>, color: Ink, points: &[Point], unit: f64) {
    let [first, ..] = points else { return };
    if points.len() == 1 {
        let [cx, cy] = first.map(|v| v as f32);
        let r = unit as f32 / 2.0;
        items.push(Item::Circle { cx, cy, r: r + unit as f32 * 0.4, paint: Paint::fill(HALO) });
        items.push(Item::Circle { cx, cy, r, paint: Paint::fill(ink(color)) });
        return;
    }
    let build = |builder: &mut PathBuilder| {
        builder.move_to(first[0], first[1]);
        for pair in points[1..].windows(2) {
            let ([x, y], [nx, ny]) = (pair[0], pair[1]);
            builder.quad_to(x, y, (x + nx) / 2.0, (y + ny) / 2.0);
        }
        let [x, y] = points[points.len() - 1];
        builder.line_to(x, y);
    };
    path(items, build, Paint::stroke(stroke(HALO, unit * 1.8)));
    path(items, build, Paint::stroke(stroke(ink(color), unit)));
}

fn arrow(items: &mut Vec<Item>, color: Ink, from: Point, to: Point, unit: f64) {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let length = dx.hypot(dy);
    if length < 1.0 {
        return;
    }
    let (ux, uy) = (dx / length, dy / length);
    let head = (unit * 5.5).min(length * 0.6);
    let half = head * 0.62;
    let (bx, by) = (to[0] - ux * head, to[1] - uy * head);
    let (overlap_x, overlap_y) = (bx + ux * head * 0.3, by + uy * head * 0.3);
    let shaft = |builder: &mut PathBuilder| {
        builder.move_to(from[0], from[1]);
        builder.line_to(overlap_x, overlap_y);
    };
    let tip = |builder: &mut PathBuilder| {
        builder.move_to(to[0], to[1]);
        builder.line_to(bx - uy * half, by + ux * half);
        builder.line_to(bx + uy * half, by - ux * half);
        builder.close();
    };
    let width = unit * 1.15;
    path(items, shaft, Paint::stroke(stroke(HALO, width + unit * 0.8)));
    path(items, tip, Paint::stroke(stroke(HALO, unit * 1.3)));
    path(items, shaft, Paint::stroke(stroke(ink(color), width)));
    path(items, tip, Paint { stroke: Some(stroke(ink(color), unit * 0.5)), ..Paint::fill(ink(color)) });
}

fn rect(items: &mut Vec<Item>, color: Ink, from: Point, to: Point, unit: f64) {
    let (x, y) = (from[0].min(to[0]), from[1].min(to[1]));
    let (width, height) = ((to[0] - from[0]).abs(), (to[1] - from[1]).abs());
    let radius = (unit * 1.6).min(width / 2.0).min(height / 2.0);
    let shape = |builder: &mut PathBuilder| builder.rounded_rect(x, y, width, height, radius, radius);
    path(items, shape, Paint::stroke(stroke(HALO, unit * 1.8)));
    path(items, shape, Paint::stroke(stroke(ink(color), unit)));
}

fn pin(items: &mut Vec<Item>, color: Ink, at: Point, number: usize, radius: f64, unit: f64, selected: bool) {
    let [cx, cy] = at.map(|v| v as f32);
    let (r, unit) = (radius as f32, unit as f32);
    let contrast = Color::token(color.contrast());
    if selected {
        let ring = r + unit * 1.5;
        items.push(Item::Circle { cx, cy, r: ring, paint: Paint::stroke(Stroke::new(HALO, (unit * 1.4).into())) });
        items.push(Item::Circle {
            cx,
            cy,
            r: ring,
            paint: Paint::stroke(Stroke::new(ink(color), (unit * 0.7).into())),
        });
    }
    items.push(Item::Circle { cx, cy: cy + unit * 0.35, r: r + unit * 0.45, paint: Paint::fill(HALO) });
    let ring = Stroke::new(contrast.clone(), (unit * 0.7).into());
    items.push(Item::Circle { cx, cy, r, paint: Paint { stroke: Some(ring), ..Paint::fill(ink(color)) } });
    let label = label_outline(&number.to_string(), radius * 0.9, at[0], at[1]);
    items.extend(label.map(|path| Item::Path { path, paint: Paint::fill(contrast) }));
}

/// The colours of the tokens marks use, from `packages/theme/tokens.json`.
fn token_colors() -> &'static HashMap<String, [u8; 4]> {
    static COLORS: OnceLock<HashMap<String, [u8; 4]>> = OnceLock::new();
    COLORS.get_or_init(|| {
        let tokens: Value =
            serde_json::from_str(include_str!("../../../packages/theme/tokens.json")).unwrap_or_default();
        let Some(colors) = tokens["colors"]["light"].as_object() else { return HashMap::new() };
        colors.iter().filter_map(|(name, hex)| Some((name.clone(), hex_color(hex.as_str()?)?))).collect()
    })
}

fn hex_color(hex: &str) -> Option<[u8; 4]> {
    let hex = hex.strip_prefix('#')?;
    let byte = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?, if hex.len() == 8 { byte(6)? } else { 255 }])
}

fn sk_color(color: &Color) -> tiny_skia::Color {
    let [r, g, b, a] = match color {
        Color::Rgba(rgba) => *rgba,
        Color::Token(name) => token_colors().get(name.as_ref()).copied().unwrap_or([255, 0, 255, 255]),
        _ => [0, 0, 0, 0],
    };
    tiny_skia::Color::from_rgba8(r, g, b, a)
}

fn sk_path(path: &crate::render::openui::canvas::Path) -> Option<tiny_skia::Path> {
    let mut builder = tiny_skia::PathBuilder::new();
    let mut points = path.points.as_chunks::<2>().0.iter().map(|[x, y]| (*x, *y));
    for verb in path.verbs.chars() {
        match verb {
            'M' => points.next().map(|(x, y)| builder.move_to(x, y))?,
            'L' => points.next().map(|(x, y)| builder.line_to(x, y))?,
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

fn paint_shape(
    pixmap: &mut tiny_skia::Pixmap,
    shape: &tiny_skia::Path,
    paint: &Paint,
    transform: tiny_skia::Transform,
) {
    let mut sk = tiny_skia::Paint { anti_alias: true, ..tiny_skia::Paint::default() };
    if let Some(fill) = &paint.fill {
        sk.set_color(sk_color(fill));
        pixmap.fill_path(shape, &sk, tiny_skia::FillRule::Winding, transform, None);
    }
    let Some(line) = &paint.stroke else { return };
    sk.set_color(sk_color(&line.color));
    let stroke = tiny_skia::Stroke {
        width: line.width,
        line_cap: match line.cap {
            Cap::Butt => tiny_skia::LineCap::Butt,
            Cap::Round => tiny_skia::LineCap::Round,
            Cap::Square => tiny_skia::LineCap::Square,
        },
        line_join: match line.join {
            Join::Miter => tiny_skia::LineJoin::Miter,
            Join::Round => tiny_skia::LineJoin::Round,
            Join::Bevel => tiny_skia::LineJoin::Bevel,
        },
        ..tiny_skia::Stroke::default()
    };
    pixmap.stroke_path(shape, &sk, &stroke, transform, None);
}

fn paint_scene(pixmap: &mut tiny_skia::Pixmap, scene: &Scene, transform: tiny_skia::Transform) {
    for item in &scene.items {
        match item {
            Item::Path { path, paint } => {
                let Some(shape) = sk_path(path) else { continue };
                paint_shape(pixmap, &shape, paint, transform);
            }
            Item::Circle { cx, cy, r, paint } => {
                let Some(shape) = tiny_skia::PathBuilder::from_circle(*cx, *cy, *r) else { continue };
                paint_shape(pixmap, &shape, paint, transform);
            }
            _ => {}
        }
    }
}

/// Reads `source` as it shows, turned as its camera says.
fn decode(source: &FilePath) -> Result<RgbaImage, String> {
    let reader =
        ImageReader::open(source).and_then(|reader| reader.with_guessed_format()).map_err(|e| e.to_string())?;
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut picture = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    picture.apply_orientation(orientation);
    Ok(picture.into_rgba8())
}

/// Writes `source` with `scene` painted over it into a new folder under `folder`, as a JPEG when
/// it was one and a PNG otherwise, and answers where.
fn flatten(scene: &Scene, source: &FilePath, folder: &FilePath, name: &str) -> Result<PathBuf, String> {
    let picture = decode(source)?;
    let (width, height) = picture.dimensions();
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or("The image is empty.")?;
    for (pixel, rgba) in pixmap.pixels_mut().iter_mut().zip(picture.pixels()) {
        let [r, g, b, a] = rgba.0;
        *pixel = tiny_skia::ColorU8::from_rgba(r, g, b, a).premultiply();
    }
    let transform = tiny_skia::Transform::from_scale(width as f32 / scene.width, height as f32 / scene.height);
    paint_scene(&mut pixmap, scene, transform);
    let pixels = pixmap.pixels().iter().flat_map(|pixel| {
        let color = pixel.demultiply();
        [color.red(), color.green(), color.blue(), color.alpha()]
    });
    let marked = RgbaImage::from_raw(width, height, pixels.collect()).ok_or("The image couldn't be drawn.")?;
    let jpeg = ImageFormat::from_path(source).ok() == Some(ImageFormat::Jpeg);
    let stem = FilePath::new(name).file_stem().and_then(|stem| stem.to_str()).unwrap_or("Image");
    let folder = folder.join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let file = folder.join(format!("{stem} marked up.{}", if jpeg { "jpg" } else { "png" }));
    let written = match jpeg {
        true => {
            let output = std::fs::File::create(&file).map_err(|e| e.to_string())?;
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(output), 92);
            DynamicImage::ImageRgba8(marked).to_rgb8().write_with_encoder(encoder)
        }
        false => marked.save_with_format(&file, ImageFormat::Png),
    };
    written.map_err(|e| e.to_string())?;
    Ok(file)
}

fn sessions() -> &'static Mutex<HashMap<u64, Markup>> {
    static SESSIONS: OnceLock<Mutex<HashMap<u64, Markup>>> = OnceLock::new();
    SESSIONS.get_or_init(Mutex::default)
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Ask {
    /// `marks` are what an earlier `flatten` answered, to go on with them.
    Open {
        width: f64,
        height: f64,
        #[serde(default)]
        marks: Option<String>,
    },
    /// `slop` is how far, in the image's pixels, a touch may land from a pin and still take it.
    Down {
        id: u64,
        tool: Tool,
        ink: Ink,
        x: f64,
        y: f64,
        #[serde(default)]
        slop: f64,
    },
    Move {
        id: u64,
        x: f64,
        y: f64,
    },
    Up {
        id: u64,
        x: f64,
        y: f64,
    },
    Cancel {
        id: u64,
    },
    Select {
        id: u64,
        mark: Option<usize>,
    },
    Note {
        id: u64,
        mark: usize,
        text: String,
    },
    Remove {
        id: u64,
        mark: usize,
    },
    Undo {
        id: u64,
    },
    Redo {
        id: u64,
    },
    /// Writes `source` marked up under `folder`. `name` is what the image is called in the message.
    Flatten {
        id: u64,
        source: PathBuf,
        folder: PathBuf,
        name: String,
    },
    Close {
        id: u64,
    },
    /// The composer's `text` with the `notes` of a marked-up image where its `earlier` notes were.
    PlaceNotes {
        #[serde(default)]
        notes: Option<String>,
        #[serde(default)]
        earlier: Option<String>,
        text: String,
    },
}

/// The message with `notes` in the place of `earlier`, or after it, a blank line apart, when they
/// aren't in it anymore.
pub fn place_notes(notes: Option<&str>, earlier: Option<&str>, text: &str) -> String {
    if let Some(earlier) = earlier.filter(|earlier| !earlier.is_empty())
        && let Some(at) = text.find(earlier)
    {
        let (before, after) = (text[..at].trim_end(), text[at + earlier.len()..].trim_start());
        return [before, notes.unwrap_or_default(), after]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
    }
    let Some(notes) = notes else { return text.to_owned() };
    match text.trim_end() {
        "" => notes.to_owned(),
        written => format!("{written}\n\n{notes}"),
    }
}

/// Answers a client marking up an image: JSON in, JSON out, on any thread. Every answer but
/// `flatten`'s, `close`'s and `place_notes`'s is the image's state: its scene, its pins, the selected one, and
/// whether there is anything to undo or redo.
pub fn markup_json(input: &str) -> String {
    let ask: Ask = match serde_json::from_str(input) {
        Ok(ask) => ask,
        Err(error) => return json!({ "error": error.to_string() }).to_string(),
    };
    answer(ask).to_string()
}

fn answer(ask: Ask) -> Value {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let Ok(mut sessions) = sessions().lock() else { return json!({ "error": "The markup is unavailable." }) };
    if let Ask::PlaceNotes { notes, earlier, text } = &ask {
        return json!({ "text": place_notes(notes.as_deref(), earlier.as_deref(), text) });
    }
    let (id, markup) = match ask {
        Ask::PlaceNotes { .. } => return json!({}),
        Ask::Open { width, height, marks } => {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let mut markup = Markup::new(width, height);
            markup.marks = marks.and_then(|marks| serde_json::from_str(&marks).ok()).unwrap_or_default();
            return sessions.entry(id).or_insert(markup).state(id);
        }
        Ask::Close { id } => {
            sessions.remove(&id);
            return json!({});
        }
        Ask::Flatten { id, source, folder, name } => {
            let Some(markup) = sessions.get(&id) else { return json!({ "error": "The markup is closed." }) };
            let (scene, notes) = (markup.scene(true), markup.notes(&name));
            let marks = serde_json::to_string(&markup.marks).unwrap_or_default();
            drop(sessions);
            return match flatten(&scene, &source, &folder, &name) {
                Ok(file) => json!({ "file": file, "notes": notes, "marks": marks }),
                Err(error) => json!({ "error": error }),
            };
        }
        Ask::Down { id, .. }
        | Ask::Move { id, .. }
        | Ask::Up { id, .. }
        | Ask::Cancel { id }
        | Ask::Select { id, .. }
        | Ask::Note { id, .. }
        | Ask::Remove { id, .. }
        | Ask::Undo { id }
        | Ask::Redo { id } => match sessions.get_mut(&id) {
            Some(markup) => (id, markup),
            None => return json!({ "error": "The markup is closed." }),
        },
    };
    match ask {
        Ask::Down { tool, ink, x, y, slop, .. } => markup.down(tool, ink, [x, y], slop),
        Ask::Move { x, y, .. } => markup.drag([x, y]),
        Ask::Up { x, y, .. } => markup.up([x, y]),
        Ask::Cancel { .. } => markup.cancel(),
        Ask::Select { mark, .. } => markup.select(mark),
        Ask::Note { mark, text, .. } => markup.set_note(mark, &text),
        Ask::Remove { mark, .. } => markup.remove(mark),
        Ask::Undo { .. } => markup.undo(),
        Ask::Redo { .. } => markup.redo(),
        Ask::Open { .. } | Ask::Close { .. } | Ask::Flatten { .. } | Ask::PlaceNotes { .. } => {}
    }
    markup.state(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(input: Value) -> Value {
        serde_json::from_str(&markup_json(&input.to_string())).unwrap()
    }

    fn stroke(id: u64, tool: &str, from: Point, to: Point) -> Value {
        ask(json!({ "op": "down", "id": id, "tool": tool, "ink": "red", "x": from[0], "y": from[1], "slop": 4 }));
        ask(json!({ "op": "move", "id": id, "x": to[0], "y": to[1] }));
        ask(json!({ "op": "up", "id": id, "x": to[0], "y": to[1] }))
    }

    fn open(width: u32, height: u32) -> u64 {
        ask(json!({ "op": "open", "width": width, "height": height }))["id"].as_u64().unwrap()
    }

    #[test]
    fn marks_undo_and_redo_and_a_box_too_small_to_see_is_dropped() {
        let id = open(600, 400);

        let state = stroke(id, "box", [100.0, 100.0], [300.0, 200.0]);
        assert_eq!((state["marked"].as_bool(), state["can_undo"].as_bool()), (Some(true), Some(true)));
        let state = stroke(id, "arrow", [50.0, 50.0], [51.0, 51.0]);
        assert_eq!(ask(json!({ "op": "undo", "id": id }))["marked"], false);
        assert_eq!(state["marked"], true);

        let state = ask(json!({ "op": "redo", "id": id }));
        assert_eq!((state["marked"].as_bool(), state["can_redo"].as_bool()), (Some(true), Some(false)));
    }

    #[test]
    fn pins_are_numbered_selected_when_tapped_moved_when_dragged_and_their_notes_listed() {
        let id = open(1000, 800);

        let state = stroke(id, "pin", [100.0, 100.0], [100.0, 100.0]);
        assert_eq!(state["selected"], 0);
        ask(json!({ "op": "note", "id": id, "mark": 0, "text": "  Make this bolder " }));
        stroke(id, "pen", [400.0, 400.0], [500.0, 450.0]);
        stroke(id, "pin", [700.0, 300.0], [700.0, 300.0]);

        let tapped = stroke(id, "pen", [102.0, 98.0], [102.0, 98.0]);
        assert_eq!(tapped["selected"], 0);
        assert_eq!(tapped["pins"].as_array().unwrap().len(), 2);
        let dragged = stroke(id, "arrow", [700.0, 300.0], [650.0, 350.0]);
        assert_eq!(dragged["selected"], Value::Null);
        assert_eq!((dragged["pins"][1]["number"].as_u64(), dragged["pins"][1]["x"].as_f64()), (Some(2), Some(650.0)));

        ask(json!({ "op": "note", "id": id, "mark": 2, "text": "Align with the title" }));
        let edge = stroke(id, "pin", [1000.0, 800.0], [1000.0, 800.0]);
        let (x, y, radius) = (
            edge["pins"][2]["x"].as_f64().unwrap(),
            edge["pins"][2]["y"].as_f64().unwrap(),
            edge["pins"][2]["radius"].as_f64().unwrap(),
        );
        assert!(x + radius < 1000.0 && y + radius < 800.0, "a pin at the corner stays inside the image");
        ask(json!({ "op": "remove", "id": id, "mark": 3 }));
        let markup = sessions().lock().unwrap();
        assert_eq!(
            markup[&id].notes("shot.png").as_deref(),
            Some("Notes on shot.png:\n1. Make this bolder\n2. Align with the title")
        );
    }

    #[test]
    fn a_pinch_drops_the_mark_it_started_and_puts_a_moved_pin_back() {
        let id = open(1000, 800);
        stroke(id, "pin", [100.0, 100.0], [100.0, 100.0]);

        ask(json!({ "op": "down", "id": id, "tool": "pin", "ink": "red", "x": 500, "y": 500, "slop": 4 }));
        let state = ask(json!({ "op": "cancel", "id": id }));
        assert_eq!((state["pins"].as_array().unwrap().len(), &state["selected"]), (1, &Value::Null));

        ask(json!({ "op": "down", "id": id, "tool": "pen", "ink": "red", "x": 100, "y": 100, "slop": 4 }));
        ask(json!({ "op": "move", "id": id, "x": 300, "y": 300 }));
        let state = ask(json!({ "op": "cancel", "id": id }));
        assert_eq!((state["pins"][0]["x"].as_f64(), state["selected"].clone()), (Some(100.0), Value::Null));

        ask(json!({ "op": "down", "id": id, "tool": "pen", "ink": "red", "x": 400, "y": 400, "slop": 4 }));
        ask(json!({ "op": "move", "id": id, "x": 500, "y": 450 }));
        let state = ask(json!({ "op": "cancel", "id": id }));
        assert_eq!((state["pins"].as_array().unwrap().len(), state["can_undo"].as_bool()), (1, Some(true)));
        assert_eq!(ask(json!({ "op": "undo", "id": id }))["marked"], false);
    }

    #[test]
    fn notes_go_after_the_message_or_in_the_place_of_those_they_follow() {
        let notes = "Notes on shot.png:\n1. Bolder";
        assert_eq!(place_notes(Some(notes), None, ""), notes);
        assert_eq!(place_notes(Some(notes), None, "Fix these \n"), format!("Fix these\n\n{notes}"));

        let earlier = "Notes on shot.png:\n1. Bold";
        let text = format!("Fix these\n\n{earlier}\n\nThanks");
        assert_eq!(place_notes(Some(notes), Some(earlier), &text), format!("Fix these\n\n{notes}\n\nThanks"));
        assert_eq!(place_notes(None, Some(earlier), &text), "Fix these\n\nThanks");
        assert_eq!(place_notes(Some(notes), Some("gone"), "Fix these"), format!("Fix these\n\n{notes}"));
    }

    #[test]
    fn flattening_draws_the_marks_into_the_image_at_its_full_size_and_keeps_them_to_go_on() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("shot.png");
        RgbaImage::from_pixel(1200, 800, image::Rgba([255, 255, 255, 255])).save(&source).unwrap();
        let id = open(600, 400);
        stroke(id, "box", [100.0, 100.0], [300.0, 300.0]);

        let answer = ask(json!({
            "op": "flatten", "id": id, "source": source, "folder": folder.path(), "name": "shot.png",
        }));
        let file = PathBuf::from(answer["file"].as_str().unwrap());
        assert!(file.ends_with("shot marked up.png"));
        assert_eq!(answer["notes"], Value::Null);

        let marked = image::open(&file).unwrap().into_rgba8();
        assert_eq!(marked.dimensions(), (1200, 800));
        let [r, g, b, _] = marked.get_pixel(400, 200).0;
        assert!(r > 200 && g < 120 && b < 120, "the box's left edge is red: {r} {g} {b}");
        assert_eq!(marked.get_pixel(400, 400).0, [255, 255, 255, 255]);

        let reopened = ask(json!({ "op": "open", "width": 600, "height": 400, "marks": answer["marks"] }));
        assert_eq!((reopened["marked"].as_bool(), reopened["can_undo"].as_bool()), (Some(true), Some(false)));
    }
}
