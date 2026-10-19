//! OpenUI's map style, `packages/theme/map-style-light.json` and `-dark.json`, read into the
//! layers the painter draws: the part of MapLibre's style language they use, with its
//! expressions. What it doesn't know draws nothing, or as MapLibre's default, never a panic.

use std::sync::LazyLock;

use serde_json::Value;

/// A colour, its channels 0–1, not premultiplied.
pub type Rgba = [f32; 4];

const LIGHT: &str = include_str!("../../../../../packages/theme/map-style-light.json");
const DARK: &str = include_str!("../../../../../packages/theme/map-style-dark.json");

static LIGHT_STYLE: LazyLock<Style> = LazyLock::new(|| Style::parse(LIGHT));
static DARK_STYLE: LazyLock<Style> = LazyLock::new(|| Style::parse(DARK));

/// The style of the appearance.
pub fn style(dark: bool) -> &'static Style {
    if dark { &DARK_STYLE } else { &LIGHT_STYLE }
}

pub struct Style {
    pub background: Rgba,
    pub layers: Vec<Layer>,
    /// The tile properties the layers read, which a tile keeps.
    pub keys: Vec<String>,
}

/// Where a layer's features come from.
#[derive(Clone, PartialEq, Debug)]
pub enum Source {
    /// A layer of the vector tiles, by its name.
    Tiles(String),
    Route,
    Pins,
}

pub struct Layer {
    pub id: String,
    pub source: Source,
    pub minzoom: f64,
    pub maxzoom: f64,
    filter: Option<Value>,
    /// What orders its features: those with more drawn over the others.
    pub sort_key: Value,
    pub paint: Paint,
}

pub enum Paint {
    Fill {
        color: Value,
        opacity: Value,
    },
    Line {
        color: Value,
        width: Value,
        opacity: Value,
        round: bool,
    },
    Circle {
        radius: Value,
        color: Value,
        opacity: Value,
        blur: Value,
        translate: Value,
        stroke_width: Value,
        stroke_color: Value,
    },
    Symbol(Symbol),
}

pub struct Symbol {
    pub text: Value,
    pub icon: Value,
    pub size: Value,
    pub bold: bool,
    pub letter_spacing: Value,
    pub uppercase: bool,
    pub max_width: Value,
    pub offset: Value,
    /// The text hangs under its point, else it is centred on it.
    pub top: bool,
    pub color: Value,
    pub halo_color: Value,
    pub halo_width: Value,
    /// Placed whatever lies under it, as the pins' marks and labels are.
    pub overlaps: bool,
}

/// A feature's properties, as the expressions read them.
pub type Props = [(String, Value)];

impl Style {
    pub fn parse(json: &str) -> Style {
        let root: Value = serde_json::from_str(json).unwrap_or_else(|error| {
            tracing::warn!("couldn't read the map style: {error}");
            Value::Null
        });
        let layers: Vec<Layer> = root["layers"].as_array().into_iter().flatten().filter_map(Layer::parse).collect();
        let background = root["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|layer| layer["type"] == "background")
            .and_then(|layer| color(&layer["paint"]["background-color"]))
            .unwrap_or([0., 0., 0., 1.]);
        let mut keys = Vec::new();
        for layer in layers.iter().filter(|layer| matches!(layer.source, Source::Tiles(_))) {
            gather_keys(layer.filter.as_ref().unwrap_or(&Value::Null), &mut keys);
            if let Paint::Symbol(symbol) = &layer.paint {
                gather_keys(&symbol.text, &mut keys);
            }
        }
        keys.push("rank".to_owned());
        keys.sort();
        keys.dedup();
        Style { background, layers, keys }
    }

    pub fn layer(&self, id: &str) -> Option<&Layer> {
        self.layers.iter().find(|layer| layer.id == id)
    }
}

impl Layer {
    fn parse(json: &Value) -> Option<Layer> {
        let paint = &json["paint"];
        let layout = &json["layout"];
        let source = match json["source"].as_str()? {
            "route" => Source::Route,
            "pins" => Source::Pins,
            _ => Source::Tiles(json["source-layer"].as_str()?.to_owned()),
        };
        let or = |value: &Value, default: Value| if value.is_null() { default } else { value.clone() };
        let paint = match json["type"].as_str()? {
            "fill" => {
                Paint::Fill { color: paint["fill-color"].clone(), opacity: or(&paint["fill-opacity"], 1.0.into()) }
            }
            "line" => Paint::Line {
                color: paint["line-color"].clone(),
                width: or(&paint["line-width"], 1.0.into()),
                opacity: or(&paint["line-opacity"], 1.0.into()),
                round: layout["line-cap"] == "round",
            },
            "circle" => Paint::Circle {
                radius: or(&paint["circle-radius"], 5.0.into()),
                color: or(&paint["circle-color"], "#000000".into()),
                opacity: or(&paint["circle-opacity"], 1.0.into()),
                blur: or(&paint["circle-blur"], 0.0.into()),
                translate: or(&paint["circle-translate"], serde_json::json!([0, 0])),
                stroke_width: or(&paint["circle-stroke-width"], 0.0.into()),
                stroke_color: or(&paint["circle-stroke-color"], "#000000".into()),
            },
            "symbol" => Paint::Symbol(Symbol {
                text: or(&layout["text-field"], "".into()),
                icon: or(&layout["icon-image"], "".into()),
                size: or(&layout["text-size"], 16.0.into()),
                bold: layout["text-font"].as_array().is_some_and(|fonts| {
                    fonts.iter().any(|font| font.as_str().is_some_and(|font| font.contains("Bold")))
                }),
                letter_spacing: or(&layout["text-letter-spacing"], 0.0.into()),
                uppercase: layout["text-transform"] == "uppercase",
                max_width: or(&layout["text-max-width"], 10.0.into()),
                offset: or(&layout["text-offset"], serde_json::json!([0, 0])),
                top: layout["text-anchor"] == "top",
                color: or(&paint["text-color"], "#000000".into()),
                halo_color: or(&paint["text-halo-color"], "rgba(0,0,0,0)".into()),
                halo_width: or(&paint["text-halo-width"], 0.0.into()),
                overlaps: layout["text-allow-overlap"] == true,
            }),
            _ => return None,
        };
        Some(Layer {
            id: json["id"].as_str().unwrap_or_default().to_owned(),
            source,
            minzoom: json["minzoom"].as_f64().unwrap_or(0.),
            maxzoom: json["maxzoom"].as_f64().unwrap_or(24.),
            filter: json.get("filter").cloned(),
            sort_key: [&layout["circle-sort-key"], &layout["symbol-sort-key"]]
                .into_iter()
                .find(|key| !key.is_null())
                .cloned()
                .unwrap_or(0.0.into()),
            paint,
        })
    }

    pub fn shows_at(&self, zoom: f64) -> bool {
        zoom >= self.minzoom && zoom < self.maxzoom
    }

    /// Whether the layer draws a feature: its filter, in the legacy syntax or as an expression.
    pub fn takes(&self, zoom: f64, props: &Props) -> bool {
        self.filter.as_ref().is_none_or(|filter| passes(filter, zoom, props))
    }
}

fn gather_keys(expression: &Value, keys: &mut Vec<String>) {
    let Some(items) = expression.as_array() else { return };
    let key = items.get(1).and_then(Value::as_str);
    match (items.first().and_then(Value::as_str), key) {
        (Some("get" | "has" | "!has"), Some(key)) => keys.push(key.to_owned()),
        (Some("==" | "!=" | "in" | "!in"), Some(key)) => keys.push(key.to_owned()),
        _ => {}
    }
    items.iter().for_each(|item| gather_keys(item, keys));
}

fn property<'a>(props: &'a Props, key: &str) -> Option<&'a Value> {
    props.iter().find(|(name, _)| name == key).map(|(_, value)| value)
}

fn passes(filter: &Value, zoom: f64, props: &Props) -> bool {
    let Some(items) = filter.as_array() else { return truthy(&eval(filter, zoom, props)) };
    let op = items.first().and_then(Value::as_str).unwrap_or_default();
    let Some(key) = items.get(1).and_then(Value::as_str) else {
        return match op {
            "all" => items[1..].iter().all(|item| passes(item, zoom, props)),
            "any" => items[1..].iter().any(|item| passes(item, zoom, props)),
            "none" => !items[1..].iter().any(|item| passes(item, zoom, props)),
            _ => truthy(&eval(filter, zoom, props)),
        };
    };
    let value = property(props, key);
    let rest = &items[2..];
    match op {
        "==" => value == rest.first(),
        "!=" => value != rest.first(),
        "in" => value.is_some_and(|value| rest.contains(value)),
        "!in" => value.is_none_or(|value| !rest.contains(value)),
        "has" => value.is_some(),
        "!has" => value.is_none(),
        "all" | "any" | "none" => truthy(&eval(filter, zoom, props)),
        _ => true,
    }
}

pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(on) => *on,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.),
        Value::String(text) => !text.is_empty(),
        _ => true,
    }
}

/// A MapLibre expression's value for a feature at a zoom. An operator it doesn't know is null.
pub fn eval(expression: &Value, zoom: f64, props: &Props) -> Value {
    let Some(items) = expression.as_array() else { return expression.clone() };
    let Some(op) = items.first().and_then(Value::as_str) else { return expression.clone() };
    let arg = |index: usize| items.get(index).map_or(Value::Null, |item| eval(item, zoom, props));
    let number = |index: usize| arg(index).as_f64();
    match op {
        "literal" => items.get(1).cloned().unwrap_or(Value::Null),
        "get" => arg(1).as_str().and_then(|key| property(props, key)).cloned().unwrap_or(Value::Null),
        "has" => Value::Bool(arg(1).as_str().is_some_and(|key| property(props, key).is_some())),
        "zoom" => zoom.into(),
        "!" => Value::Bool(!truthy(&arg(1))),
        "boolean" => (1..items.len()).map(arg).find(Value::is_boolean).unwrap_or(Value::Bool(false)),
        "number" => (1..items.len()).map(arg).find(Value::is_number).unwrap_or(Value::Null),
        "string" | "to-string" => match arg(1) {
            Value::String(text) => text.into(),
            Value::Null => "".into(),
            other => other.to_string().into(),
        },
        "upcase" => arg(1).as_str().map_or(Value::Null, |text| text.to_uppercase().into()),
        "downcase" => arg(1).as_str().map_or(Value::Null, |text| text.to_lowercase().into()),
        "coalesce" => (1..items.len()).map(arg).find(|value| !value.is_null()).unwrap_or(Value::Null),
        "case" => {
            let mut index = 1;
            while index + 1 < items.len() {
                if truthy(&arg(index)) {
                    return arg(index + 1);
                }
                index += 2;
            }
            arg(index)
        }
        "match" => {
            let input = arg(1);
            let mut index = 2;
            while index + 1 < items.len() {
                let labels = &items[index];
                let hit = match labels {
                    Value::Array(labels) => labels.contains(&input),
                    label => *label == input,
                };
                if hit {
                    return arg(index + 1);
                }
                index += 2;
            }
            arg(index)
        }
        "==" => Value::Bool(arg(1) == arg(2)),
        "!=" => Value::Bool(arg(1) != arg(2)),
        "<" | ">" | "<=" | ">=" => {
            let (Some(a), Some(b)) = (number(1), number(2)) else { return Value::Bool(false) };
            Value::Bool(match op {
                "<" => a < b,
                ">" => a > b,
                "<=" => a <= b,
                _ => a >= b,
            })
        }
        "all" => Value::Bool((1..items.len()).all(|index| truthy(&arg(index)))),
        "any" => Value::Bool((1..items.len()).any(|index| truthy(&arg(index)))),
        "in" => {
            let needle = arg(1);
            Value::Bool(match arg(2) {
                Value::Array(haystack) => haystack.contains(&needle),
                Value::String(text) => needle.as_str().is_some_and(|needle| text.contains(needle)),
                _ => false,
            })
        }
        "interpolate" => interpolate(items, zoom, props),
        "step" => {
            let Some(input) = number(1) else { return Value::Null };
            let mut out = arg(2);
            let mut index = 3;
            while index + 1 < items.len() {
                if number(index).is_some_and(|stop| input >= stop) {
                    out = arg(index + 1);
                }
                index += 2;
            }
            out
        }
        _ => Value::Null,
    }
}

fn interpolate(items: &[Value], zoom: f64, props: &Props) -> Value {
    let base = match items.get(1).and_then(Value::as_array).map(Vec::as_slice) {
        Some([kind, base]) if kind == "exponential" => base.as_f64().unwrap_or(1.),
        _ => 1.,
    };
    let Some(input) = items.get(2).map(|input| eval(input, zoom, props)).and_then(|input| input.as_f64()) else {
        return Value::Null;
    };
    let stops: Vec<(f64, f64)> = items[3.min(items.len())..]
        .chunks(2)
        .filter_map(|pair| Some((pair.first()?.as_f64()?, eval(pair.get(1)?, zoom, props).as_f64()?)))
        .collect();
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else { return Value::Null };
    if input <= first.0 {
        return first.1.into();
    }
    if input >= last.0 {
        return last.1.into();
    }
    let Some(pair) = stops.windows(2).find(|pair| input < pair[1].0) else { return last.1.into() };
    let ((z0, v0), (z1, v1)) = (pair[0], pair[1]);
    let share =
        if base == 1. { (input - z0) / (z1 - z0) } else { (base.powf(input - z0) - 1.) / (base.powf(z1 - z0) - 1.) };
    (v0 + (v1 - v0) * share).into()
}

/// A number the expression gives, or `default`.
pub fn number(expression: &Value, zoom: f64, props: &Props, default: f64) -> f64 {
    eval(expression, zoom, props).as_f64().unwrap_or(default)
}

/// A pair of numbers, as an offset or a translation.
pub fn pair(expression: &Value, zoom: f64, props: &Props) -> [f64; 2] {
    let value = eval(expression, zoom, props);
    let at = |index: usize| value.get(index).and_then(Value::as_f64).unwrap_or(0.);
    [at(0), at(1)]
}

/// The text a symbol shows: a string, a number, or the legacy `{key}` tokens.
pub fn text(expression: &Value, zoom: f64, props: &Props) -> String {
    let value = match expression {
        Value::String(template) if template.contains('{') => return tokens(template, props),
        expression => eval(expression, zoom, props),
    };
    match value {
        Value::String(text) => text,
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

fn tokens(template: &str, props: &Props) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else { break };
        if let Some(Value::String(value)) = property(props, &rest[start + 1..start + end]) {
            out.push_str(value);
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

/// A colour the expression gives, at `opacity`.
pub fn paint_color(expression: &Value, zoom: f64, props: &Props, opacity: f64) -> Option<Rgba> {
    let [r, g, b, a] = color(&eval(expression, zoom, props))?;
    Some([r, g, b, a * opacity.clamp(0., 1.) as f32])
}

/// `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb(…)` and `rgba(…)`.
pub fn color(value: &Value) -> Option<Rgba> {
    let text = value.as_str()?.trim();
    if let Some(hex) = text.strip_prefix('#') {
        let digits: Vec<u8> =
            hex.chars().map(|digit| digit.to_digit(16).map(|digit| digit as u8)).collect::<Option<_>>()?;
        let channels: Vec<f32> = match digits.len() {
            3 | 4 => digits.iter().map(|digit| (digit * 17) as f32 / 255.).collect(),
            6 | 8 => digits.chunks(2).map(|pair| (pair[0] * 16 + pair[1]) as f32 / 255.).collect(),
            _ => return None,
        };
        return Some([channels[0], channels[1], channels[2], channels.get(3).copied().unwrap_or(1.)]);
    }
    let inner = text.strip_prefix("rgba(").or_else(|| text.strip_prefix("rgb("))?.strip_suffix(')')?;
    let parts: Vec<f32> = inner.split(',').map(|part| part.trim().parse().ok()).collect::<Option<_>>()?;
    let [r, g, b] = [parts.first()?, parts.get(1)?, parts.get(2)?].map(|channel| channel / 255.);
    Some([r, g, b, parts.get(3).copied().unwrap_or(1.)])
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn props(pairs: &[(&str, Value)]) -> Vec<(String, Value)> {
        pairs.iter().map(|(key, value)| (key.to_string(), value.clone())).collect()
    }

    #[test]
    fn both_styles_read_whole() {
        for dark in [false, true] {
            let style = style(dark);
            let ids: Vec<&str> = style.layers.iter().map(|layer| layer.id.as_str()).collect();
            assert_eq!(
                ids,
                [
                    "landcover",
                    "parks",
                    "school",
                    "water",
                    "waterways",
                    "buildings",
                    "streets",
                    "main-roads",
                    "ferries",
                    "neighborhood-labels",
                    "city-labels",
                    "route",
                    "pin-shadow",
                    "pin",
                    "pin-selected",
                    "pin-mark",
                    "pin-label"
                ]
            );
            assert_eq!(style.keys, ["class", "name", "name_en", "rank"]);
        }
        assert_eq!(style(true).background, color(&json!("#18191e")).unwrap());
        let buildings = style(true).layer("buildings").unwrap();
        assert_eq!((buildings.minzoom, buildings.source.clone()), (14., Source::Tiles("building".into())));
    }

    #[test]
    fn widths_follow_the_zoom() {
        let Paint::Line { width, round, .. } = &style(true).layer("streets").unwrap().paint else { panic!() };
        assert!(round);
        assert_eq!(number(width, 8., &[], 0.), 0.3);
        assert!((number(width, 13., &[], 0.) - 2.15).abs() < 1e-9);
        assert_eq!(number(width, 20., &[], 0.), 8.);
    }

    #[test]
    fn filters_read_the_class() {
        let style = style(false);
        let roads = style.layer("main-roads").unwrap();
        let streets = style.layer("streets").unwrap();
        let ferries = style.layer("ferries").unwrap();
        let primary = props(&[("class", json!("primary"))]);
        let minor = props(&[("class", json!("minor"))]);
        assert!(roads.takes(12., &primary) && !roads.takes(12., &minor));
        assert!(streets.takes(12., &minor) && !streets.takes(12., &primary) && streets.takes(12., &[]));
        assert!(ferries.takes(12., &props(&[("class", json!("ferry"))])) && !ferries.takes(12., &minor));
        assert!(style.layer("water").unwrap().takes(12., &[]));
    }

    #[test]
    fn pins_take_their_look_from_the_style() {
        let style = style(true);
        let Paint::Circle { radius, color, stroke_width, .. } = &style.layer("pin").unwrap().paint else { panic!() };
        let numbered = props(&[("label", json!("Coit Tower")), ("number", json!("2")), ("selected", json!(false))]);
        let emoji = props(&[("label", json!("Ferry")), ("icon", json!("☕")), ("selected", json!(true))]);
        assert_eq!((number(radius, 12., &numbered, 0.), number(radius, 12., &emoji, 0.)), (11., 16.));
        assert_eq!(number(stroke_width, 12., &numbered, 0.), 2.);
        assert_eq!(paint_color(color, 12., &numbered, 1.), super::color(&json!("#d0d2d9")));
        let selected = style.layer("pin-selected").unwrap();
        assert!(selected.takes(12., &emoji) && !selected.takes(12., &numbered));
        let Paint::Symbol(mark) = &style.layer("pin-mark").unwrap().paint else { panic!() };
        assert_eq!((text(&mark.text, 12., &numbered), text(&mark.icon, 12., &numbered)), ("2".into(), "".into()));
        assert_eq!((text(&mark.text, 12., &emoji), text(&mark.icon, 12., &emoji)), ("".into(), "☕".into()));
        let Paint::Symbol(label) = &style.layer("pin-label").unwrap().paint else { panic!() };
        assert_eq!(pair(&label.offset, 12., &numbered), [0., 1.4]);
        assert!(label.top && label.bold && label.overlaps);
    }

    #[test]
    fn labels_prefer_english_names() {
        let Paint::Symbol(city) = &style(true).layer("city-labels").unwrap().paint else { panic!() };
        let both = props(&[("name", json!("München")), ("name_en", json!("Munich"))]);
        assert_eq!(text(&city.text, 10., &both), "Munich");
        assert_eq!(text(&city.text, 10., &props(&[("name", json!("Oakland"))])), "Oakland");
        assert_eq!(
            text(&json!("{name} ({class})"), 10., &props(&[("name", json!("A")), ("class", json!("b"))])),
            "A (b)"
        );
    }

    #[test]
    fn what_it_doesnt_know_is_harmless() {
        let style = Style::parse(
            r#"{"layers":[{"id":"x","type":"heatmap","source":"s"},{"id":"y","type":"fill","source":"openmaptiles","source-layer":"water","filter":["within",{}],"paint":{"fill-color":["unknown-op",1]}}]}"#,
        );
        assert_eq!(style.layers.len(), 1);
        let layer = &style.layers[0];
        assert!(!layer.takes(10., &[]));
        let Paint::Fill { color, .. } = &layer.paint else { panic!() };
        assert_eq!(paint_color(color, 10., &[], 1.), None);
        assert!(Style::parse("not json").layers.is_empty());
        assert_eq!(super::color(&json!("rgba(255, 0, 0, 0.5)")), Some([1., 0., 0., 0.5]));
        assert_eq!(super::color(&json!("#fff")), Some([1., 1., 1., 1.]));
    }
}
