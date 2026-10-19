//! The Media and data, Play and Drawing groups (`media.tsx`, `table.tsx`, `chart.tsx`, `map/`,
//! `extra.tsx`, `svg.tsx`, `html-view.tsx`).

use std::sync::LazyLock;

use motile_openui::{Element, Value, js};
use regress::Regex;
use unicode_segmentation::UnicodeSegmentation;

use super::controls::number_text;
use super::resolve::{Cx, Part, Pass, flag, items, number, radius, str_of, string};
use super::text;
use super::*;

static NUMERIC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[-+−]?[$€£¥₹]?\s?\d[\d,]*(\.\d+)?\s?(%|[kKmMbB]|bn)?$").expect("a valid pattern"));
static EMPTY_CELL: LazyLock<Regex> =
    LazyLock::new(|| Regex::with_flags("^(n/a|\u{2014}|\u{2013}|-)$", "i").expect("a valid pattern"));
static YOUTUBE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:youtu\.be\/|[?&]v=|\/embed\/|\/shorts\/)([\w-]{11})|^([\w-]{11})$").expect("a valid pattern")
});
static PICTOGRAPHIC: LazyLock<Regex> =
    LazyLock::new(|| Regex::with_flags(r"\p{Extended_Pictographic}", "u").expect("a valid pattern"));

const MAP_NOTE: &str = "Move and zoom the map to explore. Select a place to highlight it.";

impl Pass<'_, '_> {
    fn photo_search(&mut self, query: &str) -> Photos {
        if js::utf16_len(query) > 2 {
            return self.photos(query);
        }
        Photos { state: PhotoState::Missing, media: Vec::new() }
    }

    pub(super) fn search_image(&mut self, element: &Element, cx: Cx) -> UiKind {
        let query = string(element, "query").trim().to_owned();
        UiKind::SearchImage {
            photos: self.photo_search(&query),
            query,
            aspect_ratio: aspect_ratio(element, cx),
            radius: image_radius(element),
            fit: fit(element),
        }
    }

    pub(super) fn image(&mut self, element: &Element, cx: Cx) -> UiKind {
        let alt = string(element, "alt").trim().to_owned();
        let src = str_of(element, "src").and_then(text::https_url);
        let photos = match src.is_none() || self.widgets().failed {
            true => self.photo_search(&alt),
            false => Photos { state: PhotoState::Idle, media: Vec::new() },
        };
        UiKind::Image {
            photos,
            src,
            alt,
            aspect_ratio: aspect_ratio(element, cx),
            radius: image_radius(element),
            fit: fit(element),
        }
    }

    pub(super) fn table(&mut self, element: &Element, cx: Cx) -> UiKind {
        let titles: Vec<String> = match element.prop("columns") {
            Some(Value::Array(columns)) => columns.iter().map(text_of).collect(),
            _ => Vec::new(),
        };
        let rows: Vec<(usize, &Vec<Value>)> = match element.prop("rows") {
            Some(Value::Array(rows)) => rows
                .iter()
                .enumerate()
                .filter_map(|(index, row)| match row {
                    Value::Array(cells) => Some((index, cells)),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let columns = titles
            .into_iter()
            .enumerate()
            .map(|(column, title)| {
                let cells: Vec<&Value> = rows
                    .iter()
                    .filter_map(|(_, cells)| cells.get(column))
                    .filter(|value| !is_empty_cell(value))
                    .collect();
                let numeric = !cells.is_empty()
                    && cells.iter().all(|value| match value {
                        Value::Number(_) => true,
                        Value::String(text) => NUMERIC.find(js::trim(text)).is_some(),
                        _ => false,
                    });
                TableColumn { title, numeric }
            })
            .collect::<Vec<_>>();
        let inner = Cx { card: true, typography: false, cell: false, media: false, part: Part::Plain, ..cx };
        let rows = rows
            .iter()
            .map(|(row, cells)| {
                (0..columns.len())
                    .map(|column| {
                        let value = cells.get(column).unwrap_or(&Value::Null);
                        match value {
                            Value::Element(_) => {
                                let steps = [motile_openui::Step::Index(*row), motile_openui::Step::Index(column)];
                                match self.item("rows", &steps, value, inner) {
                                    Some(node) => TableCell::Node { node },
                                    None => TableCell::Text { text: Rich::default() },
                                }
                            }
                            Value::Number(number) => TableCell::Text { text: text::plain(&number_text(*number)) },
                            value => TableCell::Text { text: text::inline(&text_of(value)) },
                        }
                    })
                    .collect()
            })
            .collect();
        UiKind::Table { columns, rows }
    }

    pub(super) fn map(&mut self, element: &Element) -> UiKind {
        let mut seen: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        let mut pins = Vec::new();
        for (order, (_, value)) in items(element.prop("markers")).into_iter().enumerate() {
            let Some(marker) = value.as_element().filter(|marker| !(self.ctx.streaming && marker.partial)) else {
                continue;
            };
            let Some(label) = str_of(marker, "label").filter(|label| !label.trim().is_empty()) else { continue };
            let (Some(lat), Some(lng)) = (number(marker, "lat"), number(marker, "lng")) else { continue };
            if lat.abs() > 90.0 || lng.abs() > 180.0 {
                continue;
            }
            let count = seen.entry(label.to_owned()).or_default();
            *count += 1;
            pins.push(MapPin {
                key: if *count > 1 { format!("{label} ({count})") } else { label.to_owned() },
                label: label.to_owned(),
                lat,
                lng,
                category: str_of(marker, "category")
                    .map(str::trim)
                    .filter(|category| !category.is_empty())
                    .map(str::to_owned),
                icon: str_of(marker, "icon").and_then(pin_emoji),
                number: order as u32 + 1,
                selected: false,
                visible: true,
            });
        }
        let mut categories: Vec<String> = Vec::new();
        for category in pins.iter().filter_map(|pin| pin.category.clone()) {
            if !categories.contains(&category) {
                categories.push(category);
            }
        }
        let selected = self.value(element, "selected").and_then(|value| value.as_str().map(str::to_owned));
        let filter = self.widgets().filter.filter(|filter| categories.contains(filter));
        for pin in &mut pins {
            pin.selected = selected.as_deref() == Some(pin.label.as_str());
            pin.visible = filter.is_none() || pin.category == filter;
        }
        let stops: Vec<[f64; 2]> = pins.iter().filter(|pin| pin.visible).map(|pin| [pin.lat, pin.lng]).collect();
        let (route, line) = match flag(element, "route") {
            false => (RouteKind::Off, Vec::new()),
            true if stops.len() < 2 => (RouteKind::Loading, stops),
            true => match self.route(&stops) {
                Route::Loading => (RouteKind::Loading, stops),
                Route::Failed => (RouteKind::Overview, stops),
                Route::Street(points) => (RouteKind::Street, points),
            },
        };
        let note = match route {
            RouteKind::Off => MAP_NOTE.to_owned(),
            RouteKind::Street => format!(
                "{MAP_NOTE} The line follows streets (OSRM / OpenStreetMap, driving profile); it is an overview, not turn-by-turn directions."
            ),
            _ => format!("{MAP_NOTE} The line joins the places in order. Street routing is loading or unavailable."),
        };
        UiKind::Map {
            pins,
            route,
            line,
            selected,
            height: number(element, "height").unwrap_or(400.0).clamp(200.0, 720.0) as f32,
            categories,
            filter,
            note,
        }
    }

    pub(super) fn flashcard(&mut self, element: &Element, cx: Cx) -> UiKind {
        let inner =
            Cx { flow: false, card: false, typography: false, cell: false, media: false, part: Part::Plain, ..cx };
        let face = |pass: &mut Self, prop: &str, word: TextStyle| match element.prop(prop) {
            Some(Value::String(text)) => Slot { text: Some(text::inline(text)), style: word, children: Vec::new() },
            _ => pass.slot(element, prop, TextStyle::Body, inner),
        };
        let front = face(self, "front", TextStyle::Word);
        let back = face(self, "back", TextStyle::WordBack);
        let hint = str_of(element, "hint").filter(|hint| !hint.is_empty()).unwrap_or("Tap to flip").to_owned();
        UiKind::Flashcard { front, back, hint, flipped: self.widgets().flipped, canvas: canvas(element) }
    }

    pub(super) fn celebration(&mut self, element: &Element) -> UiKind {
        UiKind::Celebration { burst: self.widgets().burst, canvas: canvas(element) }
    }

    pub(super) fn html_view(&mut self, element: &Element) -> UiKind {
        UiKind::HtmlView {
            html: string(element, "html"),
            height: number(element, "height")
                .filter(|height| *height > 0.0)
                .map(|height| height.clamp(80.0, 2400.0) as f32),
            title: str_of(element, "title").filter(|title| !title.is_empty()).unwrap_or("Interactive view").to_owned(),
            streaming: self.ctx.streaming,
        }
    }
}

/// `String(value ?? "")`.
fn text_of(value: &Value) -> String {
    if value.is_null() { String::new() } else { js::to_string(value) }
}

fn is_empty_cell(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.is_empty() || EMPTY_CELL.find(js::trim(text)).is_some(),
        _ => false,
    }
}

fn aspect_ratio(element: &Element, cx: Cx) -> f32 {
    if cx.media {
        return 4.0 / 3.0;
    }
    number(element, "aspectRatio").filter(|ratio| *ratio > 0.2 && *ratio < 5.0).unwrap_or(1.5) as f32
}

fn image_radius(element: &Element) -> Radius {
    let none = Radius { kind: RadiusKind::Points, points: 0.0 };
    match element.prop("radius") {
        Some(value) => radius(value).unwrap_or(none),
        None => Radius { kind: RadiusKind::Lg, points: 12.0 },
    }
}

fn fit(element: &Element) -> Fit {
    if string(element, "fit") == "contain" { Fit::Contain } else { Fit::Cover }
}

/// A pin's emoji: the first character of its icon, if that is a pictograph.
fn pin_emoji(icon: &str) -> Option<String> {
    let first = icon.trim().graphemes(true).next()?;
    PICTOGRAPHIC.find(first).map(|_| first.to_owned())
}

/// A canvas component's input: its props, any component in them as `{kind, props}`.
pub(super) fn canvas(element: &Element) -> Canvas {
    fn plain(value: &Value) -> Value {
        match value {
            Value::Element(element) => {
                let mut object = motile_openui::Map::new();
                object.insert("kind".into(), Value::from(element.component.as_str()));
                object.insert("props".into(), props(element));
                Value::Object(object)
            }
            Value::Array(values) => Value::Array(values.iter().map(plain).collect()),
            Value::Object(fields) => {
                Value::Object(fields.iter().map(|(key, value)| (key.clone(), plain(value))).collect())
            }
            value => value.clone(),
        }
    }
    fn props(element: &Element) -> Value {
        Value::Object(element.props.iter().map(|(key, value)| (key.clone(), plain(value))).collect())
    }
    Canvas { kind: element.component.clone(), props: props(element) }
}

pub(super) fn map_marker(element: &Element) -> UiKind {
    UiKind::MapMarker {
        label: string(element, "label"),
        lat: number(element, "lat").unwrap_or(0.0),
        lng: number(element, "lng").unwrap_or(0.0),
        category: str_of(element, "category").map(str::to_owned),
        icon: str_of(element, "icon").and_then(pin_emoji),
    }
}

pub(super) fn youtube(element: &Element) -> UiKind {
    let video = string(element, "video");
    let video = video.trim();
    let id = YOUTUBE.find(video).and_then(|found| found.group(1).or_else(|| found.group(2))).map(|range| &video[range]);
    UiKind::YouTube {
        url: id.map(|id| format!("https://www.youtube-nocookie.com/embed/{id}")),
        title: str_of(element, "title").filter(|title| !title.is_empty()).unwrap_or("YouTube video").to_owned(),
    }
}
