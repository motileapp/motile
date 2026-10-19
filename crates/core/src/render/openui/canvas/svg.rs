//! Svg and its shapes, as open-intelligent-ui's `svg.tsx` lets a browser draw them: every value
//! checked as it checks it, shapes painted in the text colour unless told otherwise, with round
//! caps and joins, and the viewBox fitted to the frame, centred (`xMidYMid meet`).

use std::str::FromStr;

use kurbo::{Affine, Point, SvgArc, Vec2};
use motile_openui::{Map, Value, js};
use svgtypes::{PathParser, PathSegment, PointsParser};

use super::scene::{Anchor, Color, Font, Item, Paint, PathBuilder, Scene, Stroke, Text, Weight, round};

/// Longer path data or points are dropped.
const MAX_DATA: usize = 20_000;
const MAX_TRANSFORM: usize = 500;
const DEFAULT_VIEW_BOX: [f64; 4] = [0.0, 0.0, 100.0, 100.0];
/// What `em` and `rem` count in a length.
const EM: f64 = 14.0;
const DEFAULT_TEXT_SIZE: f64 = 12.0;
const BLACK: Color = Color::Rgba([0, 0, 0, 255]);

/// The scene of an Svg's props in a column `width` wide. Its children are `{kind, props}` objects
/// (or the evaluator's elements), lists of them flattened.
pub fn layout(props: &Map, column: f64) -> Scene {
    let view_box = props.get("viewBox").and_then(Value::as_str).and_then(view_box).unwrap_or(DEFAULT_VIEW_BOX);
    let [vx, vy, vw, vh] = view_box;
    let width_prop = props.get("width").filter(|width| !width.is_null());
    let width = match width_prop.and_then(|width| length(width, column)) {
        Some(width) => width.min(column),
        None if width_prop.is_none() && vw > 0.0 => column.min((vw * 1.5).max(280.0)),
        None => column,
    };
    let height = props.get("height").and_then(|height| length(height, f64::NAN)).filter(|height| !height.is_nan());
    if vw <= 0.0 || vh <= 0.0 {
        return Scene::empty(width, height.unwrap_or(150.0));
    }
    let height = height.unwrap_or(width * vh / vw);
    let scale = (width / vw).min(height / vh);
    let fit = Affine::translate(((width - vw * scale) / 2.0 - vx * scale, (height - vh * scale) / 2.0 - vy * scale))
        * Affine::scale(scale);
    let mut items = Vec::new();
    if let Some(children) = props.get("children") {
        draw_children(children, fit, 1.0, &mut items);
    }
    Scene { width: round(width), height: round(height), items, ..Scene::default() }
}

fn draw_children(children: &Value, transform: Affine, opacity: f64, items: &mut Vec<Item>) {
    match children {
        Value::Array(children) => children.iter().for_each(|child| draw_children(child, transform, opacity, items)),
        Value::Element(element) => draw(&element.component, &element.props, transform, opacity, items),
        Value::Object(node) => {
            let (Some(kind), Some(props)) =
                (node.get("kind").and_then(Value::as_str), node.get("props").and_then(Value::as_object))
            else {
                return;
            };
            draw(kind, props, transform, opacity, items);
        }
        _ => {}
    }
}

fn draw(kind: &str, props: &Map, transform: Affine, opacity: f64, items: &mut Vec<Item>) {
    let n = |key: &str| props.get(key).and_then(Value::as_number).filter(|n| n.is_finite());
    let at = |key: &str| n(key).unwrap_or(0.0);
    let size = |key: &str| n(key).unwrap_or(0.0).max(0.0);
    let mut path = PathBuilder::new(transform);
    let filled = match kind {
        "G" => {
            let transform = match props.get("transform").and_then(Value::as_str).and_then(group_transform) {
                Some(group) => transform * group,
                None => transform,
            };
            let opacity = opacity * unit(props.get("opacity")).unwrap_or(1.0);
            if let Some(children) = props.get("children") {
                draw_children(children, transform, opacity, items);
            }
            return;
        }
        "SvgText" => {
            items.extend(text(props, transform, opacity));
            return;
        }
        "Circle" => {
            let r = size("r");
            if r > 0.0 {
                path.ellipse(at("cx"), at("cy"), r, r);
            }
            true
        }
        "Ellipse" => {
            let (rx, ry) = (size("rx"), size("ry"));
            if rx > 0.0 && ry > 0.0 {
                path.ellipse(at("cx"), at("cy"), rx, ry);
            }
            true
        }
        "Rect" => {
            let (width, height) = (size("width"), size("height"));
            let radius = n("rx").unwrap_or(0.0).max(0.0);
            if width > 0.0 && height > 0.0 {
                path.rounded_rect(at("x"), at("y"), width, height, radius.min(width / 2.0), radius.min(height / 2.0));
            }
            true
        }
        "Line" => {
            path.move_to(at("x1"), at("y1"));
            path.line_to(at("x2"), at("y2"));
            false
        }
        "Path" => {
            if let Some(data) = checked(props.get("d"), is_path_char) {
                path_data(data, &mut path);
            }
            props.get("fill").is_some()
        }
        "Polyline" | "Polygon" => {
            if let Some(points) = checked(props.get("points"), is_points_char) {
                for (k, (x, y)) in PointsParser::from(points).enumerate() {
                    if k == 0 { path.move_to(x, y) } else { path.line_to(x, y) }
                }
                if kind == "Polygon" {
                    path.close();
                }
            }
            kind == "Polygon"
        }
        _ => return,
    };
    let Some(path) = path.finish() else { return };
    let paint = paint(props, filled, transform, opacity);
    if paint.fill.is_none() && paint.stroke.is_none() {
        return;
    }
    items.push(Item::Path { path, paint });
}

/// `fill`, `stroke`, `strokeWidth` and `opacity`: a filled shape takes the text colour unless it
/// has only a stroke; any other has a 2 wide stroke in the text colour.
fn paint(props: &Map, filled: bool, transform: Affine, opacity: f64) -> Paint {
    let fill = match props.get("fill") {
        Some(fill) => css_color(fill, Some(BLACK)),
        None if filled && props.get("stroke").is_none() => Some(Color::Current),
        None => None,
    };
    let stroke = match props.get("stroke") {
        Some(stroke) => css_color(stroke, None),
        None if filled => None,
        None => Some(Color::Current),
    };
    let declared = props.get("strokeWidth").and_then(Value::as_number).filter(|n| n.is_finite() && *n >= 0.0);
    let width = declared.unwrap_or(if filled { 1.0 } else { 2.0 }) * mean_scale(transform);
    let stroke = stroke.filter(|_| width > 0.0).map(|color| Stroke::new(color, width).round());
    Paint { fill, stroke, opacity: (opacity * unit(props.get("opacity")).unwrap_or(1.0)) as f32, ..Paint::default() }
}

fn text(props: &Map, transform: Affine, opacity: f64) -> Option<Item> {
    let text = props.get("text").map(js::to_string).filter(|text| !text.is_empty())?;
    let number = |key: &str| props.get(key).and_then(Value::as_number).filter(|n| n.is_finite());
    let position = transform * Point::new(number("x").unwrap_or(0.0), number("y").unwrap_or(0.0));
    let color = match props.get("fill") {
        Some(fill) => css_color(fill, Some(BLACK))?,
        None => Color::Current,
    };
    let anchor = match props.get("anchor").and_then(Value::as_str) {
        Some("middle") => Anchor::Middle,
        Some("end") => Anchor::End,
        _ => Anchor::Start,
    };
    let weight = match props.get("weight").and_then(Value::as_str) {
        Some("semibold") => Weight::Semibold,
        Some("medium") => Weight::Medium,
        _ => Weight::Regular,
    };
    let size = number("size").unwrap_or(DEFAULT_TEXT_SIZE) * mean_scale(transform);
    Some(Item::Text(Text {
        text,
        x: round(position.x),
        y: round(position.y),
        anchor,
        font: Font::Ui { size: round(size), weight },
        color,
        opacity: opacity as f32,
    }))
}

/// How much a transform enlarges lengths, on average over directions.
fn mean_scale(transform: Affine) -> f64 {
    transform.determinant().abs().sqrt()
}

fn unit(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_number).filter(|n| n.is_finite()).map(|n| n.clamp(0.0, 1.0))
}

/// A colour the model wrote: one of OpenUI's names, `none`, `currentColor` or a CSS colour.
/// A value that could load something is none; one a browser can't read is `invalid`, the
/// property's initial value.
fn css_color(value: &Value, invalid: Option<Color>) -> Option<Color> {
    let text = js::trim(value.as_str()?);
    if let Some(token) = openui_color(text) {
        return Some(token);
    }
    if text.is_empty() || !safe_css(text) {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    match lower.as_str() {
        "none" => None,
        "currentcolor" => Some(Color::Current),
        _ => match svgtypes::Color::from_str(&lower) {
            Ok(color) => Some(Color::Rgba([color.red, color.green, color.blue, color.alpha])),
            Err(_) => invalid,
        },
    }
}

/// OpenUI's colour names as text colours (`crates/openui/LAYOUT.md`).
pub fn openui_color(name: &str) -> Option<Color> {
    Some(match name {
        "ink" | "info" => Color::token("foreground"),
        "muted" => Color::token("muted-foreground"),
        "faint" => Color::token("muted-stronger-foreground"),
        "canvas" => Color::Surface(super::scene::Layer::Base),
        "surface" => Color::Surface(super::scene::Layer::Control),
        "success" => Color::token("success"),
        "warning" => Color::token("warning"),
        "danger" => Color::token("destructive"),
        "purple" => Color::token("merged"),
        "pink" => Color::token("pink"),
        "chart1" => Color::token("chart-1"),
        "chart2" => Color::token("chart-2"),
        "chart3" => Color::token("chart-3"),
        "chart4" => Color::token("chart-4"),
        "chart5" => Color::token("chart-5"),
        "chart6" => Color::token("chart-6"),
        _ => return None,
    })
}

/// `style.ts`'s `isSafeCss`: nothing that loads a resource or breaks out of the value.
fn safe_css(text: &str) -> bool {
    const FUNCTIONS: [&str; 10] =
        ["url", "image", "image-set", "cross-fade", "element", "expression", "paint", "var", "attr", "env"];
    let lower = text.to_ascii_lowercase();
    if lower.contains("@import") || lower.contains(['\\', ';', '{', '}', '<', '>']) {
        return false;
    }
    !FUNCTIONS
        .iter()
        .any(|name| lower.match_indices(name).any(|(at, _)| lower[at + name.len()..].trim_start().starts_with('(')))
}

fn checked(value: Option<&Value>, allowed: fn(char) -> bool) -> Option<&str> {
    value.and_then(Value::as_str).filter(|text| text.len() < MAX_DATA && text.chars().all(allowed))
}

fn is_points_char(c: char) -> bool {
    c.is_ascii_digit() || js::is_whitespace(c) || matches!(c, ',' | '.' | 'e' | 'E' | '+' | '-')
}

fn is_path_char(c: char) -> bool {
    is_points_char(c) || "MmLlHhVvCcSsQqTtAaZz".contains(c)
}

/// Four numbers apart by spaces or one comma, as `/^\s*[-\d.]+(\s*[\s,]\s*[-\d.]+){3}\s*$/`.
fn view_box(text: &str) -> Option<[f64; 4]> {
    if text.len() >= MAX_DATA {
        return None;
    }
    let mut numbers = Vec::new();
    let mut rest = text.trim_matches(js::is_whitespace);
    loop {
        let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '-' || c == '.')).unwrap_or(rest.len());
        if end == 0 {
            return None;
        }
        numbers.push(js::string_to_number(&rest[..end]));
        rest = &rest[end..];
        if rest.is_empty() {
            break;
        }
        let gap = rest.trim_start_matches(js::is_whitespace);
        let gap = gap.strip_prefix(',').unwrap_or(gap).trim_start_matches(js::is_whitespace);
        if gap.len() == rest.len() {
            return None;
        }
        rest = gap;
    }
    let numbers: [f64; 4] = numbers.try_into().ok()?;
    numbers.iter().all(|n| n.is_finite()).then_some(numbers)
}

/// A length as `style.ts`'s `size` accepts it, in points: a number, or a string with px, %, em,
/// rem or vw. A percentage is of `reference`; a height has none.
fn length(value: &Value, reference: f64) -> Option<f64> {
    if let Some(number) = value.as_number() {
        return number.is_finite().then_some(number.max(0.0));
    }
    let text = js::trim(value.as_str()?);
    let digits = text.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(text.len());
    let (number, unit) = text.split_at(digits);
    let mut parts = number.split('.');
    let whole = parts.next().is_some_and(|part| !part.is_empty());
    let fraction = parts.next().is_none_or(|part| !part.is_empty());
    if !whole || !fraction || parts.next().is_some() {
        return None;
    }
    let number: f64 = number.parse().ok()?;
    match unit {
        "" | "px" => Some(number),
        "%" | "vw" => Some(number / 100.0 * reference),
        "em" | "rem" => Some(number * EM),
        _ => None,
    }
}

/// A G's `transform`: translate, rotate, scale, skewX, skewY and matrix, each with as many
/// numbers as it takes, so that one built from state not set yet is dropped whole.
fn group_transform(text: &str) -> Option<Affine> {
    if text.len() > MAX_TRANSFORM {
        return None;
    }
    let mut rest = text;
    let mut any = false;
    while !rest.is_empty() {
        rest = rest.trim_start_matches(js::is_whitespace);
        let name_end = rest.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(rest.len());
        let (name, after) = rest.split_at(name_end);
        let args = after.strip_prefix('(')?;
        let close = args.find([')', '('])?;
        if args.as_bytes()[close] != b')' {
            return None;
        }
        let counts: &[usize] = match name {
            "translate" | "scale" => &[1, 2],
            "rotate" => &[1, 3],
            "skewX" | "skewY" => &[1],
            "matrix" => &[6],
            _ => return None,
        };
        let numbers: Vec<&str> =
            args[..close].split(|c: char| c == ',' || js::is_whitespace(c)).filter(|n| !n.is_empty()).collect();
        if !counts.contains(&numbers.len()) || !numbers.iter().all(|n| js::string_to_number(n).is_finite()) {
            return None;
        }
        rest = args[close + 1..].trim_start_matches(js::is_whitespace);
        any = true;
    }
    if !any {
        return None;
    }
    let transform = svgtypes::Transform::from_str(js::trim(text)).ok()?;
    Some(Affine::new([transform.a, transform.b, transform.c, transform.d, transform.e, transform.f]))
}

/// Path data into the builder, absolute, with arcs as cubics; drawn up to its first error.
fn path_data(data: &str, path: &mut PathBuilder) {
    let mut current = Point::ZERO;
    let mut start = Point::ZERO;
    let mut last_cubic: Option<Point> = None;
    let mut last_quad: Option<Point> = None;
    for segment in PathParser::from(data) {
        let Ok(segment) = segment else { return };
        let base = current;
        let origin = |abs: bool| if abs { Point::ZERO } else { base };
        let (mut cubic, mut quad) = (None, None);
        match segment {
            PathSegment::MoveTo { abs, x, y } => {
                let o = origin(abs);
                current = Point::new(o.x + x, o.y + y);
                start = current;
                path.move_to(current.x, current.y);
            }
            PathSegment::LineTo { abs, x, y } => {
                let o = origin(abs);
                current = Point::new(o.x + x, o.y + y);
                path.line_to(current.x, current.y);
            }
            PathSegment::HorizontalLineTo { abs, x } => {
                current.x = if abs { x } else { current.x + x };
                path.line_to(current.x, current.y);
            }
            PathSegment::VerticalLineTo { abs, y } => {
                current.y = if abs { y } else { current.y + y };
                path.line_to(current.x, current.y);
            }
            PathSegment::CurveTo { abs, x1, y1, x2, y2, x, y } => {
                let o = origin(abs);
                let control = Point::new(o.x + x2, o.y + y2);
                current = Point::new(o.x + x, o.y + y);
                path.cubic_to(o.x + x1, o.y + y1, control.x, control.y, current.x, current.y);
                cubic = Some(control);
            }
            PathSegment::SmoothCurveTo { abs, x2, y2, x, y } => {
                let o = origin(abs);
                let first = last_cubic.map_or(current, |last| current + (current - last));
                let control = Point::new(o.x + x2, o.y + y2);
                current = Point::new(o.x + x, o.y + y);
                path.cubic_to(first.x, first.y, control.x, control.y, current.x, current.y);
                cubic = Some(control);
            }
            PathSegment::Quadratic { abs, x1, y1, x, y } => {
                let o = origin(abs);
                let control = Point::new(o.x + x1, o.y + y1);
                current = Point::new(o.x + x, o.y + y);
                path.quad_to(control.x, control.y, current.x, current.y);
                quad = Some(control);
            }
            PathSegment::SmoothQuadratic { abs, x, y } => {
                let o = origin(abs);
                let control = last_quad.map_or(current, |last| current + (current - last));
                current = Point::new(o.x + x, o.y + y);
                path.quad_to(control.x, control.y, current.x, current.y);
                quad = Some(control);
            }
            PathSegment::EllipticalArc { abs, rx, ry, x_axis_rotation, large_arc, sweep, x, y } => {
                let o = origin(abs);
                let from = current;
                current = Point::new(o.x + x, o.y + y);
                path.arc(SvgArc {
                    from,
                    to: current,
                    radii: Vec2::new(rx.abs(), ry.abs()),
                    x_rotation: x_axis_rotation.to_radians(),
                    large_arc,
                    sweep,
                });
            }
            PathSegment::ClosePath { .. } => {
                path.close();
                current = start;
            }
        }
        last_cubic = cubic;
        last_quad = quad;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_view_boxes_as_the_reference_checks_them() {
        assert_eq!(view_box("0 0 100 50"), Some([0.0, 0.0, 100.0, 50.0]));
        assert_eq!(view_box(" -10, -10 20 , 20 "), Some([-10.0, -10.0, 20.0, 20.0]));
        assert_eq!(view_box("0 0 100"), None);
        assert_eq!(view_box("0,,0 1 1"), None);
        assert_eq!(view_box("0 0 1e2 1"), None);
    }

    #[test]
    fn keeps_only_whole_transforms() {
        assert!(group_transform("rotate(30 50 50)").is_some());
        assert!(group_transform(" translate(10) scale(2, 3) ").is_some());
        assert!(group_transform("rotate( 90 80)").is_none());
        assert!(group_transform("rotate(90) junk").is_none());
        assert!(group_transform("spin(90)").is_none());
        assert!(group_transform(&format!("translate({})", "1".repeat(600))).is_none());
        let moved = group_transform("translate(10 5)").unwrap();
        assert_eq!(moved * Point::ZERO, Point::new(10.0, 5.0));
    }

    #[test]
    fn reads_colours_as_a_browser_would() {
        let fill = |text: &str| css_color(&text.into(), Some(BLACK));
        assert_eq!(fill("muted"), Some(Color::token("muted-foreground")));
        assert_eq!(fill("#ff0000"), Some(Color::Rgba([255, 0, 0, 255])));
        assert_eq!(fill("Red"), Some(Color::Rgba([255, 0, 0, 255])));
        assert_eq!(fill("rgba(0, 0, 255, 0.5)"), Some(Color::Rgba([0, 0, 255, 128])));
        assert_eq!(fill("none"), None);
        assert_eq!(fill("currentColor"), Some(Color::Current));
        assert_eq!(fill("url(#x)"), None);
        assert_eq!(fill("bogus"), Some(BLACK));
        assert_eq!(css_color(&"bogus".into(), None), None);
    }

    #[test]
    fn reads_lengths() {
        assert_eq!(length(&Value::from(120.0), 600.0), Some(120.0));
        assert_eq!(length(&"50%".into(), 600.0), Some(300.0));
        assert_eq!(length(&"2em".into(), 600.0), Some(28.0));
        assert_eq!(length(&"80".into(), 600.0), Some(80.0));
        assert_eq!(length(&"auto".into(), 600.0), None);
        assert_eq!(length(&".5px".into(), 600.0), None);
    }
}
