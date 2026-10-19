//! Chart, as open-intelligent-ui's `chart.tsx` draws it: bars, lines and areas over nice ticks,
//! scatter plots, and pies and donuts with their key.

use std::f64::consts::PI;

use kurbo::Affine;
use motile_openui::format::NumberFormat;
use motile_openui::{Map, Value, js};

use super::scene::{
    Anchor, Color, Font, Gradient, Hit, Item, Layer, Legend, LegendEntry, Paint, PathBuilder, Region, Scene, Stop,
    Stroke, Text, Tip, TipRow, round,
};

const COLORS: [&str; 6] = ["chart-1", "chart-2", "chart-3", "chart-4", "chart-5", "chart-6"];
const MAX_SERIES: usize = 8;
const DEFAULT_HEIGHT: f64 = 240.0;
const TOP: f64 = 10.0;
const BOTTOM: f64 = 26.0;
/// How far a tooltip's centre keeps from the plot's ends.
const TIP_INSET: f64 = 70.0;
/// Up to this many rows a line shows a dot on each.
const DOTTED_ROWS: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Bar,
    Line,
    Area,
    Pie,
    Donut,
    Scatter,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Bar => "bar",
            Kind::Line => "line",
            Kind::Area => "area",
            Kind::Pie => "pie",
            Kind::Donut => "donut",
            Kind::Scatter => "scatter",
        }
    }
}

/// How values are written: compact on the axis ("$2B"), in full in a tooltip ("$1.24B").
#[derive(Debug)]
enum Format {
    Number,
    Currency(String),
    Percent,
}

impl Format {
    fn new(kind: Option<&str>, currency: Option<&str>) -> Format {
        match kind {
            Some("percent") => Format::Percent,
            Some("currency") => {
                let code = currency
                    .filter(|code| code.len() == 3 && code.chars().all(|c| c.is_ascii_alphabetic()))
                    .map(str::to_ascii_uppercase);
                Format::Currency(code.unwrap_or_else(|| "USD".to_owned()))
            }
            _ => Format::Number,
        }
    }

    fn write(&self, value: f64, compact: bool) -> String {
        if let Format::Percent = self {
            let fixed = js::to_fixed(value, if compact { 1 } else { 2 });
            return format!("{}%", js::number_to_string(js::string_to_number(&fixed)));
        }
        let big = value.abs() >= if compact { 1000.0 } else { 1e6 };
        let max = if compact && big { 1 } else { 2 };
        let format = match self {
            Format::Currency(code) => NumberFormat::currency(code.as_str(), 0, max),
            _ => NumberFormat::decimal(0, max),
        };
        if big { format.compact() } else { format }.format(value)
    }
}

struct Spec<'a> {
    kind: Kind,
    rows: Vec<&'a Map>,
    x: String,
    series: Vec<String>,
    stacked: bool,
    height: f64,
    format: Format,
}

impl<'a> Spec<'a> {
    fn new(props: &'a Map) -> Option<Spec<'a>> {
        let kind = match props.get("type").and_then(Value::as_str) {
            Some("line") => Kind::Line,
            Some("area") => Kind::Area,
            Some("pie") => Kind::Pie,
            Some("donut") => Kind::Donut,
            Some("scatter") => Kind::Scatter,
            _ => Kind::Bar,
        };
        let rows: Vec<&Map> = props
            .get("data")
            .and_then(Value::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_object)
            .collect();
        let first = *rows.first()?;
        let wanted = |value: &Value| match kind {
            Kind::Scatter => matches!(value, Value::Number(_)),
            _ => matches!(value, Value::String(_)),
        };
        let x = props
            .get("x")
            .and_then(Value::as_str)
            .filter(|x| !x.is_empty() && first.contains_key(*x))
            .or_else(|| first.iter().find(|(_, value)| wanted(value)).map(|(key, _)| key.as_str()))
            .or_else(|| first.keys().next().map(String::as_str))
            .unwrap_or("undefined")
            .to_owned();
        let named: Vec<String> = props
            .get("series")
            .and_then(Value::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(|series| series.as_str().map(str::to_owned))
            .collect();
        let given = props.get("series").and_then(Value::as_array).is_some_and(|series| !series.is_empty());
        let series = if given {
            named
        } else {
            first
                .iter()
                .filter(|(key, value)| **key != x && matches!(value, Value::Number(_)))
                .map(|(key, _)| key.clone())
                .collect()
        };
        let height = props.get("height").and_then(Value::as_number).unwrap_or(DEFAULT_HEIGHT).clamp(140.0, 480.0);
        Some(Spec {
            kind,
            rows,
            x,
            series: series.into_iter().take(MAX_SERIES).collect(),
            stacked: props.get("stacked").is_some_and(js::truthy),
            height,
            format: Format::new(
                props.get("format").and_then(Value::as_str),
                props.get("currency").and_then(Value::as_str),
            ),
        })
    }

    fn label(&self, row: &Map) -> String {
        match row.get(&self.x) {
            None | Some(Value::Null) => String::new(),
            Some(value) => js::to_string(value),
        }
    }
}

/// `Number(value) || 0`.
fn num(value: Option<&Value>) -> f64 {
    let number = value.map_or(f64::NAN, js::to_number);
    if number.is_nan() { 0.0 } else { number }
}

fn series_color(index: usize) -> Color {
    Color::token(COLORS[index % COLORS.len()])
}

/// The scene of a Chart's props at a column `width` wide.
pub fn layout(props: &Map, width: f64) -> Scene {
    let Some(spec) = Spec::new(props) else { return Scene::empty(width, DEFAULT_HEIGHT) };
    if width <= 0.0 {
        return Scene::empty(width.max(0.0), spec.height);
    }
    match spec.kind {
        Kind::Pie | Kind::Donut => pie(&spec, width),
        Kind::Scatter => scatter(&spec, width),
        _ => cartesian(&spec, width),
    }
}

/// A step of 1, 2, 2.5, 5 or 10 times a power of ten, at least `span`.
fn nice_step(span: f64) -> f64 {
    if span <= 0.0 || span.is_nan() {
        return 1.0;
    }
    let power = 10f64.powf(span.log10().floor());
    let n = span / power;
    let factor = match n {
        n if n <= 1.0 => 1.0,
        n if n <= 2.0 => 2.0,
        n if n <= 2.5 => 2.5,
        n if n <= 5.0 => 5.0,
        _ => 10.0,
    };
    factor * power
}

fn ticks(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    let count = ((hi - lo) / step).round();
    if !count.is_finite() || count < 0.0 {
        return Vec::new();
    }
    (0..=count as usize).map(|k| lo + k as f64 * step).collect()
}

/// A label's width by a generous estimate: DM Sans's widest common characters, 0.7 em each.
fn label_width(label: &str, size: f64) -> f64 {
    js::utf16_len(label) as f64 * size * 0.7
}

/// The y axis's gutter: wide enough for its longest tick label.
fn gutter(ticks: &[f64], format: &Format) -> f64 {
    let longest = ticks.iter().map(|tick| label_width(&format.write(*tick, true), 11.0)).fold(0.0, f64::max);
    longest + 12.0
}

fn legend(spec: &Spec) -> Option<Legend> {
    if spec.series.len() < 2 {
        return None;
    }
    let entries = spec
        .series
        .iter()
        .enumerate()
        .map(|(index, series)| LegendEntry { color: series_color(index), label: series.clone(), share: None })
        .collect();
    Some(Legend { beside: false, entries })
}

fn grid_line(x1: f64, x2: f64, y: f64, dashed: bool) -> Item {
    let mut path = PathBuilder::new(Affine::IDENTITY);
    path.move_to(x1, y);
    path.line_to(x2, y);
    let mut stroke = Stroke::new(Color::Surface(Layer::Border), 1.0);
    if dashed {
        stroke = stroke.dashed(&[3.0, 4.0]);
    }
    Item::Path { path: path.finish().unwrap_or_default(), paint: Paint::stroke(stroke) }
}

fn text(text: String, x: f64, y: f64, anchor: Anchor, font: Font, color: Color) -> Item {
    Item::Text(Text { text, x: round(x), y: round(y), anchor, font, color, opacity: 1.0 })
}

fn tick_label(text_: String, x: f64, y: f64, anchor: Anchor) -> Item {
    text(text_, x, y, anchor, Font::Tick, Color::token("muted-stronger-foreground"))
}

/// Where a label centred on `x` goes: against the scene's edge instead where it would cross it.
fn centred(label: &str, x: f64, size: f64, width: f64) -> (f64, Anchor) {
    let half = label_width(label, size) / 2.0;
    if x - half < 0.0 {
        return (0.0, Anchor::Start);
    }
    if x + half > width {
        return (width, Anchor::End);
    }
    (x, Anchor::Middle)
}

fn dot(cx: f64, cy: f64, r: f64, color: Color) -> Item {
    Item::Circle {
        cx: round(cx),
        cy: round(cy),
        r: round(r),
        paint: Paint {
            fill: Some(color),
            stroke: Some(Stroke::new(Color::Surface(Layer::Base), 1.5)),
            ..Paint::default()
        },
    }
}

/// `Math.min(Math.max(x, 70), width - 70)`.
fn tip_x(x: f64, width: f64) -> f64 {
    x.max(TIP_INSET).min(width - TIP_INSET)
}

/// The end of a bar away from its baseline, the only one that is rounded.
#[derive(Clone, Copy, PartialEq)]
enum End {
    Top,
    Bottom,
}

/// A bar whose two corners at `end` are rounded as far as they fit, the others square.
fn bar(x: f64, y: f64, width: f64, height: f64, radius: f64, end: Option<End>, color: Color) -> Option<Item> {
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let radius = radius.min(width / 2.0).min(height);
    let Some(end) = end.filter(|_| radius > 0.0) else {
        let (x, y, width, height) = (round(x), round(y), round(width), round(height));
        return Some(Item::Rect { x, y, width, height, rx: 0.0, ry: 0.0, paint: Paint::fill(color) });
    };
    const KAPPA: f64 = 0.552_284_749_830_793_6;
    let (right, bottom, k) = (x + width, y + height, radius * KAPPA);
    let mut path = PathBuilder::new(Affine::IDENTITY);
    if end == End::Top {
        path.move_to(x, bottom);
        path.line_to(x, y + radius);
        path.cubic_to(x, y + radius - k, x + radius - k, y, x + radius, y);
        path.line_to(right - radius, y);
        path.cubic_to(right - radius + k, y, right, y + radius - k, right, y + radius);
        path.line_to(right, bottom);
    } else {
        path.move_to(x, y);
        path.line_to(right, y);
        path.line_to(right, bottom - radius);
        path.cubic_to(right, bottom - radius + k, right - radius + k, bottom, right - radius, bottom);
        path.line_to(x + radius, bottom);
        path.cubic_to(x + radius - k, bottom, x, bottom - radius + k, x, bottom - radius);
    }
    path.close();
    path.finish().map(|path| Item::Path { path, paint: Paint::fill(color) })
}

/// A number as `toFixed(1)` writes it, as the reference's path data has it.
fn tenth(value: f64) -> f64 {
    js::string_to_number(&js::to_fixed(value, 1))
}

fn cartesian(spec: &Spec, width: f64) -> Scene {
    let Spec { rows, series, kind, height, format, .. } = spec;
    let height = *height;
    let stacked = spec.stacked && *kind != Kind::Line;
    let tops: Vec<Vec<f64>> = rows
        .iter()
        .map(|row| {
            let mut total = 0.0;
            series
                .iter()
                .map(|key| {
                    total += num(row.get(key)).max(0.0);
                    total
                })
                .collect()
        })
        .collect();
    let values: Vec<f64> = if stacked {
        tops.iter().map(|top| top.last().copied().unwrap_or(0.0)).collect()
    } else {
        rows.iter().flat_map(|row| series.iter().map(|key| num(row.get(key)))).collect()
    };
    let max = values.iter().copied().fold(0.0, f64::max);
    let min = values.iter().copied().fold(0.0, f64::min);
    let step = nice_step((max - min) / 5.0);
    let lo = (min / step).floor() * step;
    let hi = (lo + step).max((max / step).ceil() * step);
    let ticks = ticks(lo, hi, step);
    let left = gutter(&ticks, format);
    let right = 4.0;
    let w = (width - left - right).max(10.0);
    let h = (height - TOP - BOTTOM).max(10.0);
    let band = w / rows.len().max(1) as f64;
    let x_at = |i: usize| left + band * i as f64 + band / 2.0;
    let y = |v: f64| TOP + h - (v - lo) / (hi - lo) * h;
    let longest = rows.iter().map(|row| js::utf16_len(&spec.label(row))).max().unwrap_or(0).max(1);
    let every = (((longest as f64 * 6.6 + 10.0) / band).ceil() as usize).max(1);

    let mut items = Vec::new();
    for tick in &ticks {
        items.push(grid_line(left, left + w, y(*tick), *tick != 0.0));
        items.push(tick_label(format.write(*tick, true), left - 10.0, y(*tick) + 4.0, Anchor::End));
    }
    for (i, row) in rows.iter().enumerate().step_by(every) {
        let label = spec.label(row);
        let (x, anchor) = centred(&label, x_at(i), 12.0, width);
        items.push(text(label, x, height - 6.0, anchor, Font::Caption, Color::token("muted-foreground")));
    }
    let under_marks = items.len() as u32;
    if *kind == Kind::Bar {
        for (i, row) in rows.iter().enumerate() {
            let inner = (band * 0.7).min(72.0 * if stacked { 1.0 } else { series.len().max(1) as f64 });
            if stacked {
                let base_of = |si: usize| if si == 0 { 0.0 } else { tops[i][si - 1] };
                let topmost = (0..series.len()).rev().find(|&si| tops[i][si] > base_of(si));
                for (si, &stacked_top) in tops[i].iter().enumerate() {
                    let top = y(stacked_top);
                    let base = y(base_of(si));
                    let end = (Some(si) == topmost).then_some(End::Top);
                    items.extend(bar(
                        x_at(i) - inner / 2.0,
                        top,
                        inner,
                        (base - top).max(0.0),
                        4.0,
                        end,
                        series_color(si),
                    ));
                }
                continue;
            }
            let each = inner / series.len() as f64;
            for (si, key) in series.iter().enumerate() {
                let v = num(row.get(key));
                let x = x_at(i) - inner / 2.0 + each * si as f64 + 1.0;
                let end = if v < 0.0 { End::Bottom } else { End::Top };
                let item = bar(
                    x,
                    y(v).min(y(0.0)),
                    (each - 2.0).max(1.0),
                    (y(0.0) - y(v)).abs(),
                    4.0,
                    Some(end),
                    series_color(si),
                );
                items.extend(item);
            }
        }
    }
    let lines = matches!(kind, Kind::Line | Kind::Area);
    let value_at = |i: usize, si: usize| if stacked { tops[i][si] } else { num(rows[i].get(&series[si])) };
    if lines {
        for (si, _) in series.iter().enumerate() {
            let color = series_color(si);
            let base_at = |i: usize| if stacked && si > 0 { tops[i][si - 1] } else { 0.0 };
            let points: Vec<(f64, f64)> =
                (0..rows.len()).map(|i| (tenth(x_at(i)), tenth(y(value_at(i, si))))).collect();
            if *kind == Kind::Area && rows.len() > 1 {
                let under: Vec<(f64, f64)> =
                    (0..rows.len()).rev().map(|i| (tenth(x_at(i)), tenth(y(base_at(i))))).collect();
                items.extend(area(&points, &under, color.clone(), stacked));
            }
            let mut path = PathBuilder::new(Affine::IDENTITY);
            for (k, (x, y)) in points.iter().enumerate() {
                if k == 0 { path.move_to(*x, *y) } else { path.line_to(*x, *y) }
            }
            let stroke = Stroke::new(color.clone(), 2.2).round();
            items.extend(path.finish().map(|path| Item::Path { path, paint: Paint::stroke(stroke) }));
            if rows.len() <= DOTTED_ROWS {
                items.extend((0..rows.len()).map(|i| dot(x_at(i), y(value_at(i, si)), 3.0, color.clone())));
            }
        }
    }

    let at = if *kind == Kind::Bar { under_marks } else { items.len() as u32 };
    let hits = (0..rows.len())
        .map(|i| {
            let mut lit = Vec::new();
            if *kind == Kind::Bar {
                lit.push(Item::Rect {
                    x: round(left + band * i as f64),
                    y: round(TOP),
                    width: round(band),
                    height: round(h),
                    rx: 0.0,
                    ry: 0.0,
                    paint: Paint::fill(Color::Surface(Layer::Row)),
                });
            } else {
                lit.extend((0..series.len()).map(|si| dot(x_at(i), y(value_at(i, si)), 4.5, series_color(si))));
                let mut guide = PathBuilder::new(Affine::IDENTITY);
                guide.move_to(x_at(i), TOP);
                guide.line_to(x_at(i), TOP + h);
                let stroke = Stroke::new(Color::token("border-accent"), 1.0).dashed(&[3.0, 3.0]);
                lit.extend(guide.finish().map(|path| Item::Path { path, paint: Paint::stroke(stroke) }));
            }
            let rows_ = series
                .iter()
                .enumerate()
                .map(|(si, key)| TipRow {
                    color: Some(series_color(si)),
                    name: if series.len() > 1 { key.clone() } else { String::new() },
                    value: format.write(num(rows[i].get(key)), false),
                })
                .collect();
            Hit {
                region: Region::Rect {
                    x: round(left + band * i as f64),
                    y: 0.0,
                    width: round(band),
                    height: round(height),
                },
                at,
                lit,
                tip: Tip { x: round(tip_x(x_at(i), width)), y: round(TOP), title: spec.label(rows[i]), rows: rows_ },
            }
        })
        .collect();

    Scene {
        width: round(width),
        height: round(height),
        label: Some(format!("{} chart of {} by {}", kind.name(), series.join(", "), spec.x)),
        items,
        hits,
        legend: legend(spec),
        ..Scene::default()
    }
}

/// The band between a series' line and what it stands on, shaded down its height.
fn area(points: &[(f64, f64)], under: &[(f64, f64)], color: Color, stacked: bool) -> Option<Item> {
    let mut path = PathBuilder::new(Affine::IDENTITY);
    for (k, (x, y)) in points.iter().chain(under).enumerate() {
        if k == 0 { path.move_to(*x, *y) } else { path.line_to(*x, *y) }
    }
    path.close();
    let all = || points.iter().chain(under);
    let top = all().map(|(_, y)| *y).fold(f64::INFINITY, f64::min);
    let bottom = all().map(|(_, y)| *y).fold(f64::NEG_INFINITY, f64::max);
    let left = all().map(|(x, _)| *x).fold(f64::INFINITY, f64::min);
    if bottom <= top {
        return None;
    }
    let (from, to) = if stacked { (0.5, 0.3) } else { (0.24, 0.02) };
    let gradient = Gradient {
        x1: round(left),
        y1: round(top),
        x2: round(left),
        y2: round(bottom),
        stops: vec![Stop { at: 0.0, color: color.clone(), opacity: from }, Stop { at: 1.0, color, opacity: to }],
    };
    Some(Item::Path { path: path.finish()?, paint: Paint { gradient: Some(gradient), ..Paint::default() } })
}

struct Axis {
    lo: f64,
    hi: f64,
    ticks: Vec<f64>,
}

fn axis(values: &[f64]) -> Axis {
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = Some(max - min).filter(|span| *span != 0.0 && !span.is_nan());
    let step = nice_step(span.or(Some(max.abs()).filter(|max| *max != 0.0 && !max.is_nan())).unwrap_or(1.0) / 5.0);
    let lo = (min / step).floor() * step;
    let hi = (lo + step).max((max / step).ceil() * step);
    Axis { lo, hi, ticks: ticks(lo, hi, step) }
}

/// An x tick of a scatter plot, as `toLocaleString` writes it: compact from ten thousand.
fn scatter_tick(tick: f64) -> String {
    let value = js::string_to_number(&js::to_fixed(tick, 6));
    if value.abs() < 1e4 {
        return NumberFormat::decimal(0, 3).format(value);
    }
    let thousands = (value.abs().log10() / 3.0).floor().min(4.0);
    let scaled = value.abs() / 10f64.powf(thousands * 3.0);
    NumberFormat::decimal(0, if scaled < 10.0 { 1 } else { 0 }).compact().format(value)
}

fn scatter(spec: &Spec, width: f64) -> Scene {
    let Spec { rows, series, height, format, x, .. } = spec;
    let height = *height;
    let first = rows[0];
    let name = first.iter().find(|(key, value)| *key != x && matches!(value, Value::String(_))).map(|(key, _)| key);
    let ys: Vec<f64> = rows.iter().flat_map(|row| series.iter().map(|key| num(row.get(key)))).collect();
    let xs: Vec<f64> = rows.iter().map(|row| num(row.get(x))).collect();
    if ys.is_empty() {
        return Scene::empty(width, height);
    }
    let ya = axis(&ys);
    let xa = axis(&xs);
    let left = gutter(&ya.ticks, format);
    let w = (width - left - 8.0).max(10.0);
    let h = (height - TOP - BOTTOM).max(10.0);
    let px = |v: f64| left + (v - xa.lo) / (xa.hi - xa.lo) * w;
    let py = |v: f64| TOP + h - (v - ya.lo) / (ya.hi - ya.lo) * h;
    let every = ((xa.ticks.len() as f64 * 44.0 / w).ceil() as usize).max(1);

    let mut items = Vec::new();
    for tick in &ya.ticks {
        items.push(grid_line(left, left + w, py(*tick), true));
        items.push(tick_label(format.write(*tick, true), left - 10.0, py(*tick) + 4.0, Anchor::End));
    }
    for tick in xa.ticks.iter().step_by(every) {
        let label = scatter_tick(*tick);
        let (x, anchor) = centred(&label, px(*tick), 11.0, width);
        items.push(tick_label(label, x, height - 6.0, anchor));
    }
    let point = |cx: f64, cy: f64, r: f64, si: usize| Item::Circle {
        cx: round(cx),
        cy: round(cy),
        r: round(r),
        paint: Paint {
            fill: Some(series_color(si)),
            fill_opacity: 0.78,
            stroke: Some(Stroke::new(Color::Surface(Layer::Base), 1.5)),
            ..Paint::default()
        },
    };
    let mut hits = Vec::new();
    for (si, key) in series.iter().enumerate() {
        for row in rows {
            let (cx, cy) = (px(num(row.get(x))), py(num(row.get(key))));
            items.push(point(cx, cy, 5.0, si));
            let title = match name {
                Some(name) => row.get(name).map_or_else(|| "undefined".to_owned(), js::to_string),
                None => key.clone(),
            };
            let x_value = NumberFormat::decimal(0, 2).format(num(row.get(x)));
            hits.push(Hit {
                region: Region::Circle { cx: round(cx), cy: round(cy), r: 5.0 },
                at: 0,
                lit: vec![point(cx, cy, 6.5, si)],
                tip: Tip {
                    x: round(tip_x(cx, width)),
                    y: round((cy - 70.0).max(0.0)),
                    title,
                    rows: vec![
                        TipRow { color: None, name: x.clone(), value: x_value },
                        TipRow {
                            color: Some(series_color(si)),
                            name: key.clone(),
                            value: format.write(num(row.get(key)), false),
                        },
                    ],
                },
            });
        }
    }
    let count = items.len() as u32;
    for hit in &mut hits {
        hit.at = count;
    }

    Scene {
        width: round(width),
        height: round(height),
        label: Some(format!("scatter chart of {} by {}", series.join(", "), x)),
        items,
        hits,
        legend: legend(spec),
        ..Scene::default()
    }
}

/// The disc's side for a column `width` wide.
fn pie_size(width: f64) -> f64 {
    (if width < 420.0 { width * 0.62 } else { 200.0 }).clamp(80.0, 200.0)
}

fn round_arc(from: kurbo::Point, to: kurbo::Point, radius: f64, large_arc: bool, sweep: bool) -> kurbo::SvgArc {
    kurbo::SvgArc { from, to, radii: kurbo::Vec2::new(radius, radius), x_rotation: 0.0, large_arc, sweep }
}

fn pie(spec: &Spec, width: f64) -> Scene {
    let key = spec.series.first();
    let items: Vec<(String, f64)> =
        spec.rows.iter().map(|row| (spec.label(row), key.map_or(0.0, |key| num(row.get(key))).max(0.0))).collect();
    let sum: f64 = items.iter().map(|(_, value)| value).sum();
    let total = if sum == 0.0 || sum.is_nan() { 1.0 } else { sum };
    let size = pie_size(width);
    let r = size / 2.0;
    let inner = if spec.kind == Kind::Donut { r * 0.62 } else { 0.0 };
    let point = |radius: f64, a: f64| {
        let x = js::string_to_number(&js::to_fixed(r + radius * a.cos(), 2));
        let y = js::string_to_number(&js::to_fixed(r + radius * a.sin(), 2));
        kurbo::Point::new(x, y)
    };

    let mut shapes = Vec::new();
    let mut hits = Vec::new();
    let mut angle = -PI / 2.0;
    for (i, (label, value)) in items.iter().enumerate() {
        let sweep = value / total * PI * 2.0;
        let (a0, a1) = (angle, angle + sweep);
        angle = a1;
        let large = sweep > PI;
        let color = series_color(i);
        if sweep >= PI * 2.0 - 1e-6 {
            let mut path = PathBuilder::new(Affine::IDENTITY);
            if inner > 0.0 {
                path.ellipse(r, r, (r + inner) / 2.0, (r + inner) / 2.0);
                let stroke = Stroke::new(color, r - inner);
                shapes.extend(path.finish().map(|path| Item::Path { path, paint: Paint::stroke(stroke) }));
            } else {
                shapes.push(Item::Circle { cx: round(r), cy: round(r), r: round(r), paint: Paint::fill(color) });
            }
            continue;
        }
        let mut path = PathBuilder::new(Affine::IDENTITY);
        let (outer0, outer1) = (point(r, a0), point(r, a1));
        if inner > 0.0 {
            let (inner0, inner1) = (point(inner, a0), point(inner, a1));
            path.move_to(outer0.x, outer0.y);
            path.arc(round_arc(outer0, outer1, r, large, true));
            path.line_to(inner1.x, inner1.y);
            path.arc(round_arc(inner1, inner0, inner, large, false));
        } else {
            path.move_to(r, r);
            path.line_to(outer0.x, outer0.y);
            path.arc(round_arc(outer0, outer1, r, large, true));
        }
        path.close();
        let paint = Paint {
            fill: Some(color),
            stroke: Some(Stroke::new(Color::Surface(Layer::Base), 2.0)),
            ..Paint::default()
        };
        shapes.extend(path.finish().map(|path| Item::Path { path, paint }));
        let middle = (a0 + a1) / 2.0;
        let reach = (r + inner) / 2.0;
        hits.push(Hit {
            region: Region::Sector {
                cx: round(r),
                cy: round(r),
                inner: round(inner),
                outer: round(r),
                start: a0 as f32,
                end: a1 as f32,
            },
            at: 0,
            lit: Vec::new(),
            tip: Tip {
                x: round(r + reach * middle.cos()),
                y: round(r + reach * middle.sin()),
                title: format!("{label}: {}", spec.format.write(*value, false)),
                rows: Vec::new(),
            },
        });
    }
    let count = shapes.len() as u32;
    for hit in &mut hits {
        hit.at = count;
    }
    let entries = items
        .iter()
        .enumerate()
        .map(|(i, (label, value))| LegendEntry {
            color: series_color(i),
            label: label.clone(),
            share: Some(format!("{}%", js::number_to_string(js::round(value / total * 1000.0) / 10.0))),
        })
        .collect();

    Scene {
        width: round(size),
        height: round(size),
        label: Some(format!("{} chart of {} by {}", spec.kind.name(), key.map_or("values", String::as_str), spec.x)),
        items: shapes,
        hits,
        legend: Some(Legend { beside: width >= size + 24.0 + 160.0, entries }),
        ..Scene::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_nice() {
        let steps: Vec<f64> =
            [0.0, 0.7, 1.0, 1.3, 2.2, 3.0, 7.0, 12.0, 240.0, 0.03].into_iter().map(nice_step).collect();
        assert_eq!(steps, [1.0, 1.0, 1.0, 2.0, 2.5, 5.0, 10.0, 20.0, 250.0, 0.05]);
    }

    #[test]
    fn writes_values_compact_on_the_axis_and_in_full_in_tooltips() {
        let number = Format::new(None, None);
        assert_eq!(number.write(1500.0, true), "1.5K");
        assert_eq!(number.write(1500.0, false), "1,500");
        assert_eq!(number.write(1_240_000.0, false), "1.24M");
        let euro = Format::new(Some("currency"), Some("eur"));
        assert_eq!(euro.write(2_000_000_000.0, true), "€2B");
        assert_eq!(euro.write(12.5, false), "€12.5");
        let bad = Format::new(Some("currency"), Some("euro"));
        assert_eq!(bad.write(3.0, false), "$3");
        let percent = Format::new(Some("percent"), None);
        assert_eq!(percent.write(45.678, true), "45.7%");
        assert_eq!(percent.write(45.0, false), "45%");
    }

    #[test]
    fn writes_scatter_ticks_as_to_locale_string_does() {
        assert_eq!(scatter_tick(2500.0), "2,500");
        assert_eq!(scatter_tick(0.1 + 0.2), "0.3");
        assert_eq!(scatter_tick(15000.0), "15K");
        assert_eq!(scatter_tick(1_500_000.0), "1.5M");
    }
}
