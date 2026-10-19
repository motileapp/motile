//! The map as a list of what to paint, in points from its top left corner: the tiles' fills and
//! lines by the style's layers, the places' labels that fit, the route and the pins. A pure
//! function of the tiles, the style, the camera, the size, the pins and the line, which the live
//! map, its snapshot and the tests share.

use std::sync::{Arc, LazyLock};

use motile_core::render::openui::map::{Camera, project};
use serde_json::Value;
use ttf_parser::Face;

use super::style::{self, Paint, Props, Rgba, Source, Style, Symbol};
use super::tiles::{Geometry, Slot, Tile};

/// Left, top, right, bottom.
pub type Rect = [f32; 4];

/// MapLibre's line height, in ems.
const LINE_HEIGHT: f32 = 1.2;
/// The room a label keeps around it from the others.
const LABEL_PADDING: f32 = 2.;
/// What a pin's emoji is drawn at, centred in its 34-point circle.
pub const EMOJI_SIZE: f32 = 18.;
/// Points closer than this to the last one kept are dropped.
const NEAR: f32 = 0.35;

/// A pin the map shows: one of the node's with `visible`.
#[derive(Clone, PartialEq, Debug)]
pub struct Pin {
    pub key: String,
    pub label: String,
    pub lat: f64,
    pub lng: f64,
    pub icon: Option<String>,
    pub number: u32,
    pub selected: bool,
}

/// Where the map looks from and how large it is. `narrow` is a window under 520 points wide,
/// where the pins' labels are smaller.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct View {
    pub camera: Camera,
    pub width: f64,
    pub height: f64,
    pub narrow: bool,
}

impl View {
    fn rect(&self) -> Rect {
        [0., 0., self.width as f32, self.height as f32]
    }

    fn zoom(&self) -> f64 {
        self.camera.zoom
    }
}

/// A tile the map draws: the slot it fills and the tile that fills it, its own or one above.
#[derive(Clone)]
pub struct Placed {
    pub slot: Slot,
    pub source: Slot,
    pub tile: Arc<Tile>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct DisplayList {
    pub background: Rgba,
    pub items: Vec<Item>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Item {
    /// Rings filled even-odd, clipped to the tile's slot.
    Fill {
        layer: String,
        clip: Rect,
        rings: Vec<Vec<[f32; 2]>>,
        color: Rgba,
    },
    Line {
        layer: String,
        clip: Rect,
        lines: Vec<Vec<[f32; 2]>>,
        width: f32,
        color: Rgba,
        round: bool,
    },
    /// A circle: filled, ringed outside its radius, or blurred over `blur` of its radius.
    Circle {
        layer: String,
        center: [f32; 2],
        radius: f32,
        color: Option<Rgba>,
        stroke: Option<(Rgba, f32)>,
        blur: f32,
    },
    Text(Label),
}

#[derive(Clone, PartialEq, Debug)]
pub struct Label {
    pub layer: String,
    /// Each line centred on `x`.
    pub lines: Vec<String>,
    pub x: f32,
    pub top: f32,
    pub size: f32,
    pub line_height: f32,
    pub bold: bool,
    pub color: Rgba,
    pub halo: Option<(Rgba, f32)>,
    /// Points between letters.
    pub spacing: f32,
    /// An emoji, drawn in the system's emoji font.
    pub emoji: bool,
    pub bounds: Rect,
}

/// The whole map: `base` and `overlay` in one list.
pub fn paint(style: &Style, tiles: &[Placed], view: &View, pins: &[Pin], line: &[[f64; 2]]) -> DisplayList {
    let mut list = base(style, tiles, view, pins);
    list.items.extend(overlay(style, view, pins, line));
    list
}

/// The tiles and the places' labels that fit around the pins and their labels.
pub fn base(style: &Style, tiles: &[Placed], view: &View, pins: &[Pin]) -> DisplayList {
    let zoom = view.zoom();
    let mut taken: Vec<Rect> = pin_items(style, view, pins).iter().filter_map(bounds).collect();
    let mut layers: Vec<Vec<Item>> = Vec::new();
    let mut candidates: Vec<Vec<(f64, Label)>> = Vec::new();
    for layer in &style.layers {
        let mut items = Vec::new();
        let mut labels = Vec::new();
        if let Source::Tiles(name) = &layer.source
            && layer.shows_at(zoom)
        {
            for placed in tiles {
                let Some(features) = placed.tile.layers.get(name) else { continue };
                let clip = intersect(slot_rect(placed.slot, view), view.rect());
                if clip[0] >= clip[2] || clip[1] >= clip[3] {
                    continue;
                }
                let origin = slot_rect(placed.source, view);
                let shapes = Shapes { origin, clip };
                for feature in features.iter().filter(|feature| layer.takes(zoom, &feature.props)) {
                    if !shapes.touches(feature.bounds) {
                        continue;
                    }
                    let props = &feature.props;
                    match (&layer.paint, &feature.geometry) {
                        (Paint::Fill { color, opacity }, Geometry::Polygons(rings)) => {
                            let Some(color) =
                                style::paint_color(color, zoom, props, style::number(opacity, zoom, props, 1.))
                            else {
                                continue;
                            };
                            let rings: Vec<_> = rings.iter().filter_map(|ring| shapes.path(ring, 3)).collect();
                            if rings.is_empty() || area(&rings) < 0.25 {
                                continue;
                            }
                            add_fill(&mut items, &layer.id, clip, rings, color);
                        }
                        (
                            Paint::Line { color, width, opacity, round },
                            Geometry::Lines(lines) | Geometry::Polygons(lines),
                        ) => {
                            let Some(color) =
                                style::paint_color(color, zoom, props, style::number(opacity, zoom, props, 1.))
                            else {
                                continue;
                            };
                            let width = style::number(width, zoom, props, 1.) as f32;
                            let lines: Vec<_> = lines.iter().filter_map(|line| shapes.path(line, 2)).collect();
                            if lines.is_empty() || width <= 0. {
                                continue;
                            }
                            add_line(&mut items, &layer.id, clip, lines, width, color, *round);
                        }
                        (Paint::Symbol(symbol), Geometry::Points(points)) => {
                            for point in points {
                                let at = shapes.point(*point);
                                if !inside(clip, at) {
                                    continue;
                                }
                                let rank =
                                    props.iter().find(|(key, _)| key == "rank").and_then(|(_, rank)| rank.as_f64());
                                if let Some(label) = label(&layer.id, symbol, zoom, props, at, None) {
                                    labels.push((rank.unwrap_or(f64::MAX), label));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        layers.push(items);
        candidates.push(labels);
    }
    for labels in candidates.iter_mut().rev() {
        labels.sort_by(|a, b| a.0.total_cmp(&b.0));
        labels.retain(|(_, label)| {
            let padded = pad(label.bounds, LABEL_PADDING);
            let fits = inside(view.rect(), [label.x, label.top]) && !taken.iter().any(|other| overlaps(*other, padded));
            if fits {
                taken.push(padded);
            }
            fits
        });
    }
    let items = layers
        .into_iter()
        .zip(candidates)
        .flat_map(|(items, labels)| items.into_iter().chain(labels.into_iter().map(|(_, label)| Item::Text(label))))
        .collect();
    DisplayList { background: style.background, items }
}

/// The route and the pins at the camera, which the live map draws over its tiles every frame.
pub fn overlay(style: &Style, view: &View, pins: &[Pin], line: &[[f64; 2]]) -> Vec<Item> {
    let zoom = view.zoom();
    let mut items = Vec::new();
    for layer in style.layers.iter().filter(|layer| layer.shows_at(zoom)) {
        match (&layer.source, &layer.paint) {
            (Source::Route, Paint::Line { color, width, opacity, round }) if line.len() >= 2 => {
                let Some(color) = style::paint_color(color, zoom, &[], style::number(opacity, zoom, &[], 1.)) else {
                    continue;
                };
                let points: Vec<[f32; 2]> = line.iter().map(|at| at_point(view, at[0], at[1])).collect();
                let width = style::number(width, zoom, &[], 1.) as f32;
                items.push(Item::Line {
                    layer: layer.id.clone(),
                    clip: view.rect(),
                    lines: vec![points],
                    width,
                    color,
                    round: *round,
                });
            }
            (Source::Pins, _) => items.extend(pin_layer(layer, view, pins)),
            _ => {}
        }
    }
    items
}

fn pin_items(style: &Style, view: &View, pins: &[Pin]) -> Vec<Item> {
    let zoom = view.zoom();
    style
        .layers
        .iter()
        .filter(|layer| layer.source == Source::Pins && layer.shows_at(zoom))
        .flat_map(|layer| pin_layer(layer, view, pins))
        .collect()
}

/// The pins' properties, as the style reads them: `icon` only for an emoji pin.
pub fn pin_props(pin: &Pin) -> Vec<(String, Value)> {
    let mut props = vec![
        ("key".to_owned(), pin.key.clone().into()),
        ("label".to_owned(), pin.label.clone().into()),
        ("number".to_owned(), pin.number.to_string().into()),
        ("selected".to_owned(), pin.selected.into()),
    ];
    if let Some(icon) = &pin.icon {
        props.push(("icon".to_owned(), icon.clone().into()));
    }
    props
}

/// The pins in the order they are drawn: by the layer's sort key, the selected one last.
fn sorted<'a>(pins: &'a [Pin], key: &Value, zoom: f64) -> Vec<(&'a Pin, Vec<(String, Value)>)> {
    let mut pins: Vec<_> = pins.iter().map(|pin| (pin, pin_props(pin))).collect();
    pins.sort_by(|a, b| style::number(key, zoom, &a.1, 0.).total_cmp(&style::number(key, zoom, &b.1, 0.)));
    pins
}

fn pin_layer(layer: &style::Layer, view: &View, pins: &[Pin]) -> Vec<Item> {
    let zoom = view.zoom();
    let mut items = Vec::new();
    for (pin, props) in sorted(pins, &layer.sort_key, zoom) {
        if !layer.takes(zoom, &props) {
            continue;
        }
        let at = at_point(view, pin.lat, pin.lng);
        match &layer.paint {
            Paint::Circle { radius, color, opacity, blur, translate, stroke_width, stroke_color } => {
                let shift = style::pair(translate, zoom, &props);
                let stroke = style::number(stroke_width, zoom, &props, 0.) as f32;
                let fill = style::paint_color(color, zoom, &props, style::number(opacity, zoom, &props, 1.))
                    .filter(|fill| fill[3] > 0.);
                let ring = style::paint_color(stroke_color, zoom, &props, 1.)
                    .filter(|_| stroke > 0.)
                    .map(|color| (color, stroke));
                items.push(Item::Circle {
                    layer: layer.id.clone(),
                    center: [at[0] + shift[0] as f32, at[1] + shift[1] as f32],
                    radius: style::number(radius, zoom, &props, 5.) as f32,
                    color: fill,
                    stroke: ring,
                    blur: style::number(blur, zoom, &props, 0.) as f32,
                });
            }
            Paint::Symbol(symbol) => {
                let icon = style::text(&symbol.icon, zoom, &props);
                if !icon.is_empty() {
                    items.push(Item::Text(emoji(&layer.id, icon, at)));
                }
                let narrow = view.narrow && symbol.top;
                if let Some(label) = label(&layer.id, symbol, zoom, &props, at, narrow.then_some((10., 120.))) {
                    items.push(Item::Text(label));
                }
            }
            _ => {}
        }
    }
    items
}

fn emoji(layer: &str, icon: String, at: [f32; 2]) -> Label {
    let half = EMOJI_SIZE / 2.;
    let line_height = EMOJI_SIZE * LINE_HEIGHT;
    Label {
        layer: layer.to_owned(),
        lines: vec![icon],
        x: at[0],
        top: at[1] - line_height / 2.,
        size: EMOJI_SIZE,
        line_height,
        bold: false,
        color: [0., 0., 0., 1.],
        halo: None,
        spacing: 0.,
        emoji: true,
        bounds: [at[0] - half, at[1] - half, at[0] + half, at[1] + half],
    }
}

/// A symbol's text at a point, wrapped at its most width; `small` is the size and the most
/// width that stand in for the style's.
fn label(
    layer: &str,
    symbol: &Symbol,
    zoom: f64,
    props: &Props,
    at: [f32; 2],
    small: Option<(f32, f32)>,
) -> Option<Label> {
    let mut text = style::text(&symbol.text, zoom, props);
    if symbol.uppercase {
        text = text.to_uppercase();
    }
    if text.trim().is_empty() {
        return None;
    }
    let style_size = style::number(&symbol.size, zoom, props, 16.) as f32;
    let (size, max_width) =
        small.unwrap_or((style_size, style::number(&symbol.max_width, zoom, props, 10.) as f32 * style_size));
    let spacing = style::number(&symbol.letter_spacing, zoom, props, 0.) as f32 * size;
    let lines = wrap(&text, size, symbol.bold, spacing, max_width);
    let width = lines.iter().map(|line| measure(line, size, symbol.bold, spacing)).fold(0., f32::max);
    let line_height = size * LINE_HEIGHT;
    let height = line_height * lines.len() as f32;
    let offset = style::pair(&symbol.offset, zoom, props);
    let x = at[0] + offset[0] as f32 * size;
    let anchor = at[1] + offset[1] as f32 * size;
    let top = if symbol.top { anchor } else { anchor - height / 2. };
    let halo_width = style::number(&symbol.halo_width, zoom, props, 0.) as f32;
    let halo = style::paint_color(&symbol.halo_color, zoom, props, 1.).filter(|color| color[3] > 0. && halo_width > 0.);
    Some(Label {
        layer: layer.to_owned(),
        lines,
        x,
        top,
        size,
        line_height,
        bold: symbol.bold,
        color: style::paint_color(&symbol.color, zoom, props, 1.).unwrap_or([0., 0., 0., 1.]),
        halo: halo.map(|color| (color, halo_width)),
        spacing,
        emoji: false,
        bounds: [x - width / 2., top, x + width / 2., top + height],
    })
}

/// The pin under a point of the map, by its drawn circle and its ring: the one drawn last when
/// two overlap.
pub fn hit(style: &Style, view: &View, pins: &[Pin], point: [f32; 2]) -> Option<usize> {
    let layer = style.layer("pin")?;
    let Paint::Circle { radius, stroke_width, .. } = &layer.paint else { return None };
    let zoom = view.zoom();
    sorted(pins, &layer.sort_key, zoom).iter().rev().find_map(|(pin, props)| {
        let at = at_point(view, pin.lat, pin.lng);
        let reach = style::number(radius, zoom, props, 5.) + style::number(stroke_width, zoom, props, 0.);
        let distance = f64::from(point[0] - at[0]).hypot(f64::from(point[1] - at[1]));
        (distance <= reach).then(|| pins.iter().position(|other| std::ptr::eq(other, *pin))).flatten()
    })
}

fn at_point(view: &View, lat: f64, lng: f64) -> [f32; 2] {
    let [x, y] = project(view.camera, view.width, view.height, lat, lng);
    [x as f32, y as f32]
}

fn slot_rect(slot: Slot, view: &View) -> Rect {
    slot.rect(view.camera, view.width, view.height).map(|edge| edge as f32)
}

/// Where a tile's coordinates land, and what of them shows.
struct Shapes {
    origin: Rect,
    clip: Rect,
}

impl Shapes {
    fn point(&self, [u, v]: [f32; 2]) -> [f32; 2] {
        let [left, top, right, bottom] = self.origin;
        [left + u * (right - left), top + v * (bottom - top)]
    }

    fn touches(&self, bounds: Rect) -> bool {
        let [x0, y0] = self.point([bounds[0], bounds[1]]);
        let [x1, y1] = self.point([bounds[2], bounds[3]]);
        overlaps(self.clip, pad([x0, y0, x1, y1], 1.))
    }

    /// The points in the map, without those too near the last kept; none with fewer than `least`.
    fn path(&self, points: &[[f32; 2]], least: usize) -> Option<Vec<[f32; 2]>> {
        let mut out: Vec<[f32; 2]> = Vec::with_capacity(points.len());
        for point in points {
            let at = self.point(*point);
            if out.last().is_some_and(|last| (last[0] - at[0]).abs() < NEAR && (last[1] - at[1]).abs() < NEAR) {
                continue;
            }
            out.push(at);
        }
        (out.len() >= least).then_some(out)
    }
}

fn add_fill(items: &mut Vec<Item>, layer: &str, clip: Rect, rings: Vec<Vec<[f32; 2]>>, color: Rgba) {
    if let Some(Item::Fill { rings: kept, color: same, clip: was, .. }) = items.last_mut()
        && *same == color
        && *was == clip
    {
        kept.extend(rings);
        return;
    }
    items.push(Item::Fill { layer: layer.to_owned(), clip, rings, color });
}

fn add_line(
    items: &mut Vec<Item>,
    layer: &str,
    clip: Rect,
    lines: Vec<Vec<[f32; 2]>>,
    width: f32,
    color: Rgba,
    round: bool,
) {
    if let Some(Item::Line { lines: kept, color: same, width: wide, clip: was, .. }) = items.last_mut()
        && *same == color
        && *wide == width
        && *was == clip
    {
        kept.extend(lines);
        return;
    }
    items.push(Item::Line { layer: layer.to_owned(), clip, lines, width, color, round });
}

fn area(rings: &[Vec<[f32; 2]>]) -> f32 {
    let [x0, y0, x1, y1] =
        rings.iter().flatten().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |[x0, y0, x1, y1], [x, y]| {
            [x0.min(*x), y0.min(*y), x1.max(*x), y1.max(*y)]
        });
    (x1 - x0) * (y1 - y0)
}

/// What an item covers, for the labels to keep off.
fn bounds(item: &Item) -> Option<Rect> {
    match item {
        Item::Circle { center, radius, stroke, color, .. } if color.is_some() || stroke.is_some() => {
            let reach = radius + stroke.map_or(0., |(_, width)| width);
            Some([center[0] - reach, center[1] - reach, center[0] + reach, center[1] + reach])
        }
        Item::Text(label) => Some(label.bounds),
        _ => None,
    }
}

pub fn overlaps(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

fn intersect(a: Rect, b: Rect) -> Rect {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]
}

fn inside(rect: Rect, [x, y]: [f32; 2]) -> bool {
    x >= rect[0] && x <= rect[2] && y >= rect[1] && y <= rect[3]
}

fn pad(rect: Rect, by: f32) -> Rect {
    [rect[0] - by, rect[1] - by, rect[2] + by, rect[3] + by]
}

static REGULAR: LazyLock<Option<Face<'static>>> = LazyLock::new(|| Face::parse(crate::assets::FONTS[0], 0).ok());
static BOLD: LazyLock<Option<Face<'static>>> = LazyLock::new(|| Face::parse(crate::assets::FONTS[3], 0).ok());

/// How wide DM Sans sets the text, which GPUI sets in it too.
pub fn measure(text: &str, size: f32, bold: bool, spacing: f32) -> f32 {
    let face = if bold { &*BOLD } else { &*REGULAR };
    let Some(face) = face else { return text.chars().count() as f32 * size * 0.55 };
    let scale = size / f32::from(face.units_per_em());
    let advance = |c: char| {
        face.glyph_index(c)
            .and_then(|glyph| face.glyph_hor_advance(glyph))
            .map_or(size * 0.55, |units| f32::from(units) * scale)
    };
    let count = text.chars().count();
    text.chars().map(advance).sum::<f32>() + spacing * count.saturating_sub(1) as f32
}

/// The text in lines no wider than `max_width`, broken between words.
fn wrap(text: &str, size: f32, bold: bool, spacing: f32, max_width: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        if let Some(last) = lines.last_mut() {
            let joined = format!("{last} {word}");
            if measure(&joined, size, bold, spacing) <= max_width {
                *last = joined;
                continue;
            }
        }
        lines.push(word.to_owned());
    }
    lines
}

#[cfg(test)]
pub mod tests {
    use std::collections::BTreeMap;

    use motile_core::render::openui::map::{Framing, frame};
    use serde_json::json;

    use super::super::tiles;
    use super::*;

    pub fn fixture_pins() -> Vec<Pin> {
        let node: Value =
            serde_json::from_str(include_str!("../../../../../crates/core/fixtures/openui/map.json")).unwrap();
        fn find(value: &Value) -> Option<&Value> {
            if value["component"] == "Map" {
                return Some(value);
            }
            match value {
                Value::Array(items) => items.iter().find_map(find),
                Value::Object(map) => map.values().find_map(find),
                _ => None,
            }
        }
        let map = find(&node).unwrap();
        map["pins"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pin| Pin {
                key: pin["key"].as_str().unwrap().into(),
                label: pin["label"].as_str().unwrap().into(),
                lat: pin["lat"].as_f64().unwrap(),
                lng: pin["lng"].as_f64().unwrap(),
                icon: pin["icon"].as_str().map(String::from),
                number: pin["number"].as_u64().unwrap() as u32,
                selected: pin["selected"].as_bool().unwrap(),
            })
            .collect()
    }

    pub fn fixture_line() -> Vec<[f64; 2]> {
        fixture_pins().iter().map(|pin| [pin.lat, pin.lng]).collect()
    }

    /// The fixture's pins fitted in a map of 720 by 400, as the map frames them once the program ends.
    pub fn fixture_view() -> View {
        let points: Vec<[f64; 2]> = fixture_line();
        View { camera: frame(&points, 720., 400., Framing::Fit).unwrap(), width: 720., height: 400., narrow: false }
    }

    pub fn placed(view: &View) -> Vec<Placed> {
        let tiles = tiles::tests::fixtures();
        let slots = tiles::cover(view.camera, view.width, view.height);
        tiles::stand_ins(&slots, |id| tiles.contains_key(&id))
            .into_iter()
            .map(|(slot, source)| Placed { slot, source, tile: tiles[&source.id()].clone() })
            .collect()
    }

    fn summary(list: &DisplayList) -> Value {
        let round = |value: f32| (f64::from(value) * 10.).round() / 10.;
        let mut layers: BTreeMap<String, Value> = BTreeMap::new();
        let mut texts = Vec::new();
        for item in &list.items {
            match item {
                Item::Fill { layer, rings, .. } => {
                    let entry =
                        layers.entry(layer.clone()).or_insert_with(|| json!({"items": 0, "rings": 0, "points": 0}));
                    entry["items"] = (entry["items"].as_u64().unwrap() + 1).into();
                    entry["rings"] = (entry["rings"].as_u64().unwrap() + rings.len() as u64).into();
                    entry["points"] =
                        (entry["points"].as_u64().unwrap() + rings.iter().map(Vec::len).sum::<usize>() as u64).into();
                }
                Item::Line { layer, lines, width, .. } => {
                    let entry =
                        layers.entry(layer.clone()).or_insert_with(|| json!({"items": 0, "lines": 0, "points": 0}));
                    entry["items"] = (entry["items"].as_u64().unwrap() + 1).into();
                    entry["lines"] = (entry["lines"].as_u64().unwrap() + lines.len() as u64).into();
                    entry["points"] =
                        (entry["points"].as_u64().unwrap() + lines.iter().map(Vec::len).sum::<usize>() as u64).into();
                    entry["width"] = round(*width).into();
                }
                Item::Circle { layer, center, radius, .. } => texts
                    .push(json!({"layer": layer, "circle": [round(center[0]), round(center[1])], "radius": radius})),
                Item::Text(label) => texts.push(json!({
                    "layer": label.layer,
                    "lines": label.lines,
                    "at": [round(label.x), round(label.top)],
                    "size": label.size,
                })),
            }
        }
        json!({"layers": layers, "marks": texts})
    }

    #[test]
    fn the_fixture_paints_as_it_did() {
        let view = fixture_view();
        let list = paint(super::super::style::style(true), &placed(&view), &view, &fixture_pins(), &fixture_line());
        let summary = serde_json::to_string_pretty(&summary(&list)).unwrap() + "\n";
        let golden = concat!(env!("CARGO_MANIFEST_DIR"), "/src/transcript/map/fixtures/painted.json");
        if std::env::var_os("OPENUI_BLESS").is_some() {
            std::fs::write(golden, &summary).unwrap();
        }
        let expected = std::fs::read_to_string(golden).unwrap_or_default();
        assert!(summary == expected, "the painted map changed; OPENUI_BLESS=1 rewrites {golden}:\n{summary}");
    }

    #[test]
    fn the_layers_draw_in_the_styles_order_inside_their_tiles() {
        let view = fixture_view();
        let list = paint(super::super::style::style(true), &placed(&view), &view, &fixture_pins(), &fixture_line());
        let style = super::super::style::style(true);
        let order = |id: &str| style.layers.iter().position(|layer| layer.id == id).unwrap();
        let layers: Vec<usize> = list
            .items
            .iter()
            .map(|item| match item {
                Item::Fill { layer, .. } | Item::Line { layer, .. } | Item::Circle { layer, .. } => order(layer),
                Item::Text(label) => order(&label.layer),
            })
            .collect();
        assert!(layers.windows(2).all(|pair| pair[0] <= pair[1]), "{layers:?}");
        for item in &list.items {
            let Item::Fill { clip, rings, .. } = item else { continue };
            assert!(clip[0] >= 0. && clip[1] >= 0. && clip[2] <= 720. && clip[3] <= 400.);
            assert!(rings.iter().all(|ring| ring.len() >= 3));
        }
        assert!(!list.items.iter().any(|item| matches!(item, Item::Fill { layer, .. } if layer == "buildings")));
        let water =
            list.items.iter().filter(|item| matches!(item, Item::Fill { layer, .. } if layer == "water")).count();
        assert_eq!(water, 4, "one fill of water in each tile");
    }

    #[test]
    fn the_route_and_the_pins_land_on_their_places() {
        let view = fixture_view();
        let pins = fixture_pins();
        let items = overlay(super::super::style::style(true), &view, &pins, &fixture_line());
        let Some(Item::Line { lines, width, round, .. }) = items.first() else { panic!("{items:?}") };
        assert_eq!((*width, *round, lines[0].len()), (3.2, true, 4));
        let ferry = at_point(&view, 37.7955, -122.3937);
        assert_eq!(lines[0][0], ferry);
        let circles: Vec<(&str, f32)> = items
            .iter()
            .filter_map(|item| match item {
                Item::Circle { layer, radius, .. } => Some((layer.as_str(), *radius)),
                _ => None,
            })
            .collect();
        assert_eq!(circles.iter().filter(|(layer, _)| *layer == "pin").count(), 4);
        assert_eq!(
            circles.iter().filter(|(layer, _)| *layer == "pin-selected").collect::<Vec<_>>(),
            [&("pin-selected", 18.)]
        );
        let last_pin = items.iter().rev().find_map(|item| match item {
            Item::Circle { layer, center, .. } if layer == "pin" => Some(*center),
            _ => None,
        });
        assert_eq!(last_pin, Some(ferry), "the selected pin is drawn over the others");
        let numbered = items.iter().find_map(|item| match item {
            Item::Text(label) if label.layer == "pin-mark" && !label.emoji => Some(label.lines.clone()),
            _ => None,
        });
        assert_eq!(numbered, Some(vec!["2".to_owned()]));
        let emoji = items.iter().filter(|item| matches!(item, Item::Text(label) if label.emoji)).count();
        assert_eq!(emoji, 3);
        let shadow = items.iter().find_map(|item| match item {
            Item::Circle { layer, blur, center, .. } if layer == "pin-shadow" => Some((*blur, *center)),
            _ => None,
        });
        let coit = at_point(&view, pins[1].lat, pins[1].lng);
        assert_eq!(shadow, Some((0.5, [coit[0], coit[1] + 2.])), "under the first pin drawn, two lower");
    }

    #[test]
    fn labels_keep_off_the_pins_and_each_other() {
        let view = fixture_view();
        let style = super::super::style::style(true);
        let pins = fixture_pins();
        let list = base(style, &placed(&view), &view, &pins);
        let labels: Vec<&Label> = list
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Text(label) => Some(label),
                _ => None,
            })
            .collect();
        assert!(!labels.is_empty());
        for (index, label) in labels.iter().enumerate() {
            for other in &labels[index + 1..] {
                assert!(!overlaps(label.bounds, other.bounds), "{:?} {:?}", label.lines, other.lines);
            }
        }
        let pin_boxes: Vec<Rect> = pin_items(style, &view, &pins).iter().filter_map(bounds).collect();
        for label in &labels {
            assert!(pin_boxes.iter().all(|pin| !overlaps(*pin, label.bounds)), "{:?}", label.lines);
        }
        let without_pins = base(style, &placed(&view), &view, &[]);
        let count = |list: &DisplayList| list.items.iter().filter(|item| matches!(item, Item::Text(_))).count();
        assert!(count(&without_pins) >= count(&list));
    }

    #[test]
    fn a_later_label_that_overlaps_is_dropped() {
        let style = Style::parse(
            r#"{"layers":[{"id":"places","type":"symbol","source":"openmaptiles","source-layer":"place",
                "layout":{"text-field":["get","name"],"text-size":10}}]}"#,
        );
        let tile = Tile {
            layers: [(
                "place".to_owned(),
                [("Alpha", 1, 0.5), ("Beta", 2, 0.5), ("Gamma", 3, 0.9)]
                    .into_iter()
                    .map(|(name, rank, y)| tiles::Feature {
                        props: vec![("name".into(), name.into()), ("rank".into(), rank.into())],
                        geometry: Geometry::Points(vec![[0.5, y]]),
                        bounds: [0.5, y, 0.5, y],
                    })
                    .collect(),
            )]
            .into(),
        };
        let view = View { camera: Camera { lat: 0., lng: 0., zoom: 0. }, width: 512., height: 512., narrow: false };
        let slot = Slot { z: 0, x: 0, y: 0 };
        let list = base(&style, &[Placed { slot, source: slot, tile: Arc::new(tile) }], &view, &[]);
        let names: Vec<String> = list
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Text(label) => Some(label.lines.join(" ")),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["Alpha", "Gamma"]);
    }

    #[test]
    fn a_press_finds_the_pin_by_its_circle() {
        let view = fixture_view();
        let style = super::super::style::style(true);
        let pins = fixture_pins();
        let coit = at_point(&view, pins[1].lat, pins[1].lng);
        assert_eq!(hit(style, &view, &pins, coit), Some(1));
        assert_eq!(hit(style, &view, &pins, [coit[0] + 12.5, coit[1]]), Some(1), "inside its ring");
        assert_eq!(hit(style, &view, &pins, [coit[0] + 13.5, coit[1]]), None);
        let bridge = at_point(&view, pins[3].lat, pins[3].lng);
        assert_eq!(hit(style, &view, &pins, [bridge[0], bridge[1] + 16.5]), Some(3), "an emoji pin is larger");
        assert_eq!(hit(style, &view, &pins, [5., 5.]), None);
    }

    #[test]
    fn labels_wrap_at_their_width() {
        assert_eq!(wrap("Golden Gate Bridge", 12., true, 0., 156.), ["Golden Gate Bridge"]);
        assert_eq!(wrap("Golden Gate Bridge", 12., true, 0., 80.), ["Golden Gate", "Bridge"]);
        assert!(measure("WIDE", 10., false, 0.8) > measure("WIDE", 10., false, 0.));
    }
}
