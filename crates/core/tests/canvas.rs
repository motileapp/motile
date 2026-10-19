//! Canvas scenes against their goldens in `fixtures/canvas`: each scene as JSON, and a few painted
//! by the reference painter. `OPENUI_BLESS=1` writes them anew.

mod paint;

use std::time::Instant;

use motile_core::render::openui::canvas::{self, Color, Item, Motion, Scene};
use motile_openui::{Map, Value};
use paint::Theme;

const WIDTHS: [f64; 2] = [390.0, 720.0];

fn props(json: &str) -> Map {
    Value::from_json_str(json).unwrap().as_object().unwrap().clone()
}

fn fixture(name: &str) -> String {
    format!("{}/fixtures/canvas/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn blessing() -> bool {
    std::env::var_os("OPENUI_BLESS").is_some()
}

fn golden(name: &str, actual: &str) {
    let file = fixture(name);
    if blessing() {
        std::fs::write(&file, actual).unwrap();
    }
    let expected = std::fs::read_to_string(&file).unwrap_or_default();
    assert!(expected == actual, "{name} changed; run with OPENUI_BLESS=1 to write it");
}

/// Compares a painted scene with its golden picture, allowing for antialiasing's rounding.
fn golden_png(name: &str, scene: &Scene, theme: Theme, hover: Option<usize>) {
    let pixmap = paint::paint(scene, theme, 2.0, hover);
    let file = fixture(&format!("{name}-{}.png", theme.name()));
    if blessing() {
        pixmap.save_png(&file).unwrap();
    }
    let expected = tiny_skia::Pixmap::load_png(&file).unwrap_or_else(|_| panic!("{file} is missing"));
    assert_eq!((expected.width(), expected.height()), (pixmap.width(), pixmap.height()), "{name}'s size changed");
    let differ = expected
        .data()
        .chunks(4)
        .zip(pixmap.data().chunks(4))
        .filter(|(a, b)| a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 8));
    let share = differ.count() as f64 / (pixmap.width() * pixmap.height()) as f64;
    assert!(share < 0.002, "{name} looks different ({:.2}% of its pixels)", share * 100.0);
}

fn json(scene: &Scene) -> String {
    serde_json::to_string(scene).unwrap()
}

fn chart(name: &str) -> Map {
    let monthly = r#"[{"month": "Jan", "sales": 120, "returns": -14}, {"month": "Feb", "sales": 98, "returns": -9},
        {"month": "Mar", "sales": 143, "returns": -21}, {"month": "Apr", "sales": 160, "returns": -12},
        {"month": "May", "sales": 151, "returns": -18}, {"month": "Jun", "sales": 187, "returns": -25}]"#;
    let drones = r#"[{"year": "2025", "Photography": 617000000, "Inspection": 410000000},
        {"year": "2027", "Photography": 820000000, "Inspection": 690000000},
        {"year": "2030", "Photography": 1240000000, "Inspection": 1310000000},
        {"year": "2034", "Photography": 1900000000, "Inspection": 2350000000}]"#;
    let days: Vec<String> = (0..40)
        .map(|day| {
            format!(
                r#"{{"day": "Day {}", "visits": {}, "signups": {}}}"#,
                day + 1,
                300 + (day * 37) % 210,
                20 + (day * 13) % 45
            )
        })
        .collect();
    let days = format!("[{}]", days.join(","));
    match name {
        "bar" => props(&format!(r#"{{"type": "bar", "data": {monthly}, "x": "month", "series": ["sales"]}}"#)),
        "bar-grouped" => props(&format!(r#"{{"type": "bar", "data": {monthly}}}"#)),
        "bar-stacked" => props(&format!(
            r#"{{"type": "bar", "data": {drones}, "x": "year", "series": ["Photography", "Inspection"], "format": "currency", "stacked": true}}"#
        )),
        "line" => props(&format!(r#"{{"type": "line", "data": {monthly}, "height": 200}}"#)),
        "line-many" => props(&format!(r#"{{"type": "line", "data": {days}, "x": "day"}}"#)),
        "area" => props(&format!(r#"{{"type": "area", "data": {days}, "series": ["visits"], "height": 220}}"#)),
        "area-stacked" => props(&format!(
            r#"{{"type": "area", "data": {drones}, "x": "year", "format": "currency", "stacked": true}}"#
        )),
        "pie" => props(
            r#"{"type": "pie", "data": [{"mode": "Walk", "share": 31}, {"mode": "Bike", "share": 12.5},
                {"mode": "Transit", "share": 38}, {"mode": "Car", "share": 18.5}], "x": "mode", "series": ["share"], "format": "percent"}"#,
        ),
        "donut" => props(
            r#"{"type": "donut", "data": [{"item": "Rent", "amount": 1450}, {"item": "Food", "amount": 520},
                {"item": "Travel", "amount": 0}, {"item": "Savings", "amount": 800}], "format": "currency", "currency": "eur"}"#,
        ),
        "donut-whole" => props(r#"{"type": "donut", "data": [{"item": "Everything", "amount": 3}]}"#),
        "scatter" => props(
            r#"{"type": "scatter", "data": [{"car": "Civic", "hp": 158, "mpg": 36}, {"car": "Model 3", "hp": 271, "mpg": 132},
                {"car": "F-150", "hp": 400, "mpg": 20}, {"car": "Prius", "hp": 194, "mpg": 57}, {"car": "Mustang", "hp": 480, "mpg": 19}],
                "x": "hp", "series": ["mpg"]}"#,
        ),
        "percent" => props(
            r#"{"type": "line", "data": [{"q": "Q1", "margin": 12.5}, {"q": "Q2", "margin": 14.25}, {"q": "Q3", "margin": 9.75}], "format": "percent"}"#,
        ),
        "empty" => props(r#"{"type": "bar", "data": []}"#),
        _ => unreachable!(),
    }
}

const CHARTS: [&str; 13] = [
    "bar",
    "bar-grouped",
    "bar-stacked",
    "line",
    "line-many",
    "area",
    "area-stacked",
    "pie",
    "donut",
    "donut-whole",
    "scatter",
    "percent",
    "empty",
];

#[test]
fn charts_match_their_goldens() {
    for name in CHARTS {
        let props = chart(name);
        let scenes: Vec<String> = WIDTHS
            .iter()
            .map(|width| format!("{width}: {}\n", json(&canvas::layout("Chart", &props, *width, 14.0))))
            .collect();
        golden(&format!("chart-{name}.txt"), &scenes.concat());
    }
}

#[test]
fn charts_paint_as_their_golden_pictures() {
    let at = |name: &str| canvas::layout("Chart", &chart(name), 560.0, 14.0);
    golden_png("chart-bar", &at("bar"), Theme::Dark, Some(3));
    golden_png("chart-bar-grouped", &at("bar-grouped"), Theme::Light, None);
    golden_png("chart-line", &at("line"), Theme::Dark, Some(2));
    golden_png("chart-area-stacked", &at("area-stacked"), Theme::Light, Some(1));
    golden_png("chart-donut", &at("donut"), Theme::Dark, None);
    golden_png("chart-scatter", &at("scatter"), Theme::Dark, Some(1));
}

#[test]
fn charts_follow_the_references_rules() {
    let scene = canvas::layout("Chart", &chart("bar-stacked"), 720.0, 14.0);
    let ticks: Vec<&str> = scene
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Text(text) if text.anchor == canvas::Anchor::End => Some(text.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(ticks, ["$0", "$1B", "$2B", "$3B", "$4B", "$5B"]);
    assert_eq!(scene.hits[2].tip.rows[1].value, "$1.31B");
    assert_eq!(scene.label.as_deref(), Some("bar chart of Photography, Inspection by year"));
    let rounded: Vec<f32> = scene
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Rect { rx, .. } => Some(*rx),
            _ => None,
        })
        .collect();
    assert_eq!(rounded, [0.0, 4.0, 0.0, 4.0, 0.0, 4.0, 0.0, 4.0]);

    let many = canvas::layout("Chart", &chart("line-many"), 390.0, 14.0);
    let labels =
        many.items.iter().filter(|item| matches!(item, Item::Text(text) if text.text.starts_with("Day"))).count();
    assert!(labels < 40 && labels > 1, "{labels} labels");
    assert!(!many.items.iter().any(|item| matches!(item, Item::Circle { .. })));

    let pie = canvas::layout("Chart", &chart("pie"), 720.0, 14.0);
    let legend = pie.legend.unwrap();
    assert!(legend.beside);
    let shares: Vec<_> = legend.entries.iter().map(|entry| entry.share.clone().unwrap()).collect();
    assert_eq!(shares, ["31%", "12.5%", "38%", "18.5%"]);
    assert_eq!(pie.width, 200.0);
    let narrow = canvas::layout("Chart", &chart("pie"), 300.0, 14.0);
    assert_eq!(narrow.width, 186.0);
    assert!(!narrow.legend.unwrap().beside);
}

#[test]
fn a_chart_lays_out_well_under_a_millisecond() {
    let props = chart("line-many");
    let started = Instant::now();
    for _ in 0..50 {
        canvas::layout("Chart", &props, 720.0, 14.0);
    }
    let each = started.elapsed() / 50;
    let limit = if cfg!(debug_assertions) { 20 } else { 1 };
    assert!(each.as_millis() < limit, "a chart took {each:?}");
}

fn svg(name: &str) -> Map {
    let shapes = r#"{"viewBox": "0 0 320 80", "width": null, "height": null, "children": [
        {"kind": "Rect", "props": {"x": 10, "y": 10, "width": 60, "height": 40, "fill": "info", "stroke": null, "strokeWidth": null, "opacity": 0.8, "rx": 8}},
        {"kind": "Ellipse", "props": {"cx": 120, "cy": 30, "rx": 34, "ry": 20, "fill": "success"}},
        {"kind": "Polygon", "props": {"points": "180,50 205,10 230,50", "fill": "warning"}},
        {"kind": "Polyline", "props": {"points": "250,45 265,15 280,40 295,12", "stroke": "danger", "strokeWidth": 3}},
        {"kind": "SvgText", "props": {"text": "Rect · Ellipse · Polygon · Polyline", "x": 160, "y": 72, "size": 11, "fill": "muted", "anchor": "middle"}}]}"#;
    let bike = r#"{"viewBox": "0 0 320 160", "children": [
        {"kind": "Path", "props": {"d": "M 90 40 L 240 62 A 18 18 0 0 1 240 98 L 90 120 A 40 40 0 0 1 90 40 Z", "fill": null, "stroke": "faint", "strokeWidth": 2}},
        {"kind": "G", "props": {"transform": "rotate(30 90 80)", "children": [
            {"kind": "Circle", "props": {"cx": 90, "cy": 80, "r": 40, "fill": "surface", "stroke": "ink", "strokeWidth": 3}},
            {"kind": "Line", "props": {"x1": 90, "y1": 80, "x2": 90, "y2": 44, "stroke": "ink", "strokeWidth": 4}},
            {"kind": "Circle", "props": {"cx": 90, "cy": 80, "r": 6, "fill": "ink"}}]}},
        {"kind": "G", "props": {"transform": "rotate(75 240 80)", "children": [
            {"kind": "Circle", "props": {"cx": 240, "cy": 80, "r": 18, "fill": "surface", "stroke": "ink", "strokeWidth": 3}},
            {"kind": "Line", "props": {"x1": 240, "y1": 80, "x2": 240, "y2": 64, "stroke": "info", "strokeWidth": 3}},
            {"kind": "Circle", "props": {"cx": 240, "cy": 80, "r": 4, "fill": "ink"}}]}},
        {"kind": "SvgText", "props": {"text": "Rear 75°", "x": 240, "y": 132, "size": 12, "fill": "muted", "anchor": "middle"}}]}"#;
    let paths = r##"{"viewBox": "-10 -10 220 120", "width": 300, "children": [
        {"kind": "Path", "props": {"d": "M0 50 C 20 0, 40 0, 60 50 S 100 100, 120 50 Q 140 0 160 50 T 200 50", "stroke": "chart1", "strokeWidth": 2.5}},
        {"kind": "Path", "props": {"d": "m10 80 h40 v20 h-40 z m60 0 l20 20 l20 -20 z", "fill": "#e46d91", "opacity": 0.5}},
        {"kind": "Path", "props": {"d": "M150 90 a20 10 30 1 0 30 0", "stroke": "rgb(75, 156, 166)", "strokeWidth": 2}},
        {"kind": "Path", "props": {"d": "M0 0 L 10 10 url(#x)", "stroke": "ink"}},
        {"kind": "G", "props": {"transform": "rotate( 90 80)", "opacity": 0.5, "children": [
            {"kind": "Rect", "props": {"x": 0, "y": 0, "width": 20, "height": 6, "fill": "bogus"}}]}},
        {"kind": "G", "props": {"transform": "translate(200 0) scale(0.5)", "children": [
            {"kind": "SvgText", "props": {"text": "end", "x": 0, "y": 20, "size": 20, "anchor": "end", "weight": "semibold"}}]}}]}"##;
    props(match name {
        "shapes" => shapes,
        "bike" => bike,
        "paths" => paths,
        _ => unreachable!(),
    })
}

#[test]
fn svgs_match_their_goldens() {
    for name in ["shapes", "bike", "paths"] {
        let props = svg(name);
        let scenes: Vec<String> = WIDTHS
            .iter()
            .map(|width| format!("{width}: {}\n", json(&canvas::layout("Svg", &props, *width, 14.0))))
            .collect();
        golden(&format!("svg-{name}.txt"), &scenes.concat());
    }
    golden_png("svg-shapes", &canvas::layout("Svg", &svg("shapes"), 480.0, 14.0), Theme::Dark, None);
    golden_png("svg-bike", &canvas::layout("Svg", &svg("bike"), 480.0, 14.0), Theme::Light, None);
    golden_png("svg-paths", &canvas::layout("Svg", &svg("paths"), 480.0, 14.0), Theme::Dark, None);
}

#[test]
fn svgs_fit_their_frames() {
    let shapes = canvas::layout("Svg", &svg("shapes"), 720.0, 14.0);
    assert_eq!((shapes.width, shapes.height), (480.0, 120.0));
    let narrow = canvas::layout("Svg", &svg("shapes"), 200.0, 14.0);
    assert_eq!((narrow.width, narrow.height), (200.0, 50.0));
    let paths = canvas::layout("Svg", &svg("paths"), 720.0, 14.0);
    assert_eq!((paths.width, paths.height), (300.0, 163.64));
    let fills: Vec<Option<Color>> = paths
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Path { paint, .. } => Some(paint.fill.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(fills, [None, Some(Color::Rgba([228, 109, 145, 255])), None, Some(Color::Rgba([0, 0, 0, 255]))]);
}

const FORMULAS: [&str; 18] = [
    r"E = mc^2",
    r"\frac{a}{b} + \frac{1}{1 + \frac{1}{x}}",
    r"\sqrt{2} + \sqrt[3]{x^2 + 1}",
    r"\sum_{i=1}^{n} i = \frac{n(n+1)}{2}",
    r"\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}",
    r"\lim_{x \to 0} \frac{\sin x}{x} = 1",
    r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
    r"\begin{bmatrix} 1 & 0 & 0 \\ 0 & 1 & 0 \\ 0 & 0 & 1 \end{bmatrix}",
    r"f(x) = \begin{cases} 1 & x > 0 \\ 0 & \text{otherwise} \end{cases}",
    r"\text{area} = \pi r^2",
    r"\hat{x} + \vec{v} + \bar{y} + \tilde{n} + \dot{a}",
    r"\bar{x} \sim \mathcal{N}\left(\mu, \frac{\sigma^2}{n}\right)",
    r"\binom{n}{k} = \frac{n!}{k!(n-k)!}",
    r"\alpha \beta \Gamma \in \mathbb{R}, \forall \epsilon > 0",
    r"\ce{H2O + CO2 -> H2CO3}",
    r"\ce{2H2 + O2 -> 2H2O}",
    r"\textcolor{red}{x} + y \colorbox{yellow}{z}",
    r"\overbrace{a + b}^{n} \underline{c}",
];

#[test]
fn formulas_are_glyph_outlines() {
    let mut summary = String::new();
    for tex in FORMULAS {
        for inline in [false, true] {
            let scene = canvas::math::layout(tex, inline, 14.0);
            assert!(!scene.items.is_empty(), "{tex} drew nothing");
            for item in &scene.items {
                match item {
                    Item::Path { paint, .. } => {
                        assert_ne!(paint.fill, Some(Color::Rgba([204, 0, 0, 255])), "{tex} failed")
                    }
                    Item::Rect { .. } => {}
                    other => panic!("{tex} drew {other:?}"),
                }
            }
            let verbs: usize = scene
                .items
                .iter()
                .map(|item| match item {
                    Item::Path { path, .. } => path.verbs.len(),
                    _ => 1,
                })
                .sum();
            summary += &format!(
                "{tex} | {} | {} × {}, baseline {}, {} items, {verbs} verbs\n",
                if inline { "inline" } else { "block" },
                scene.width,
                scene.height,
                scene.baseline.unwrap(),
                scene.items.len()
            );
        }
    }
    golden("math.txt", &summary);
    let coloured = canvas::math::layout(FORMULAS[16], false, 14.0);
    let fills: Vec<Option<Color>> = coloured
        .items
        .iter()
        .map(|item| match item {
            Item::Path { paint, .. } | Item::Rect { paint, .. } => paint.fill.clone(),
            _ => None,
        })
        .collect();
    assert_eq!(
        fills,
        [
            Some(Color::Rgba([255, 0, 0, 255])),
            Some(Color::Current),
            Some(Color::Rgba([255, 255, 0, 255])),
            Some(Color::Current)
        ]
    );
    let block = canvas::math::layout(FORMULAS[4], false, 14.0);
    let inline = canvas::math::layout(FORMULAS[4], true, 14.0);
    assert!(block.height > inline.height);
    golden("math-integral.txt", &json(&block));
    golden_png("math-integral", &block, Theme::Dark, None);
    golden_png("math-matrix", &canvas::math::layout(FORMULAS[6], false, 14.0), Theme::Light, None);
    golden_png("math-chemistry", &canvas::math::layout(FORMULAS[14], false, 14.0), Theme::Dark, None);
}

#[test]
fn a_formula_that_cant_be_read_shows_its_source() {
    let scene = canvas::layout("Math", &props(r#"{"tex": "\\frac{1}{"}"#), 400.0, 14.0);
    let [Item::Path { paint, .. }] = scene.items.as_slice() else { panic!("{:?}", scene.items) };
    assert_eq!(paint.fill, Some(Color::Rgba([204, 0, 0, 255])));
    assert!(scene.width > 30.0 && scene.height > 10.0);
    golden_png("math-error", &scene, Theme::Dark, None);
}

#[test]
fn celebration_and_flip_are_motion() {
    let Some(Motion::Confetti(confetti)) = canvas::layout("Celebration", &Map::new(), 0.0, 14.0).motion else {
        panic!()
    };
    assert_eq!(confetti.pieces.len(), 70);
    let third = &confetti.pieces[3];
    assert_eq!((third.x, third.delay, third.duration, third.drift, third.turn), (0.11, 0.12, 1.359, -87.0, 291.0));
    assert_eq!(third.color, Color::token("chart-4"));
    let Some(Motion::Flip(flip)) = canvas::layout("Flashcard", &Map::new(), 300.0, 14.0).motion else { panic!() };
    assert_eq!(flip.duration, 0.45);
}

#[test]
fn layout_reads_and_writes_json() {
    let input = r#"{"kind": "Chart", "props": {"type": "bar", "data": [{"k": "a", "v": 1}, {"k": "b", "v": 2}]}, "width": 300}"#;
    let output = canvas::layout_json(input);
    let scene: Scene = serde_json::from_str(&output).unwrap();
    assert_eq!(scene, canvas::layout("Chart", &chart_of(input), 300.0, 14.0));
    assert_eq!(serde_json::to_string(&scene).unwrap(), output);
    assert!(output.contains(r#""fill":"chart-1""#), "{output}");
    assert!(output.contains(r#""fill":{"surface":"row"}"#), "{output}");
    assert_eq!(canvas::layout_json("not json"), r#"{"width":0.0,"height":0.0,"items":[]}"#);
    let math = canvas::layout_json(r#"{"kind": "Math", "props": {"tex": "x^2", "inline": true}, "text_size": 16}"#);
    let math: Scene = serde_json::from_str(&math).unwrap();
    assert!(math.baseline.is_some());
}

fn chart_of(input: &str) -> Map {
    let value = Value::from_json_str(input).unwrap();
    value.get("props").unwrap().as_object().unwrap().clone()
}
