//! Shaping rows, as `Query` returns them, for tables and charts: groups, pivots, rankings, ranges,
//! periods, running totals, histograms and summaries.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use super::dates::{Period, next_period, parse_date, period_start};
use super::{
    Agg, MAX_ITEMS, Required, agg, aggregate, compare_text, finite, groups, iso_day, js_order, list, numbers, read,
    sum, to_num,
};
use crate::js;
use crate::library::{Args, FnError, FunctionDef};
use crate::schema::Schema;
use crate::value::{Map, Value};

pub(super) fn functions() -> Vec<FunctionDef> {
    let key_value = || {
        Schema::array(Schema::object([
            Schema::union([Schema::String, Schema::Number]).field("key"),
            Schema::Number.field("value"),
        ]))
    };
    let field = |name: &str| Schema::String.field(name);
    vec![
        FunctionDef::new(
            "GroupBy",
            "One row per distinct key, in first-seen order; value is agg of field (default \"sum\"), or the row count \
             without a field",
            [list().field("rows"), field("key"), field("field").optional(), agg().field("agg").optional()],
            group_by,
        )
        .returns(key_value()),
        FunctionDef::new(
            "Pivot",
            "One row per rowKey value with a column per colKey value holding agg of field, for stacked or grouped \
             charts",
            [
                list().field("rows"),
                field("rowKey"),
                field("colKey"),
                field("field").optional(),
                agg().field("agg").optional(),
            ],
            pivot,
        )
        .returns(Schema::array(Schema::record(Schema::union([Schema::String, Schema::Number])))),
        FunctionDef::new(
            "Top",
            "The n rows with the largest field (\"asc\": the smallest)",
            [
                list().field("rows"),
                field("field"),
                Schema::Number.field("n"),
                Schema::enumeration(["desc", "asc"]).field("direction").optional(),
            ],
            top,
        )
        .returns(list()),
        FunctionDef::new(
            "Between",
            "Rows whose field is from min to max, both included; compares numbers, or text such as \"YYYY-MM-DD\" \
             dates. Leave min or max null for an open end",
            [
                list().field("rows"),
                field("field"),
                Schema::Any.field("min").optional(),
                Schema::Any.field("max").optional(),
            ],
            between,
        )
        .returns(list()),
        FunctionDef::new(
            "Bucket",
            "Rows per day, week (from Monday) or month of dateField, in date order, empty periods 0 for sum and count; \
             key is \"YYYY-MM-DD\", or \"YYYY-MM\" for months",
            [
                list().field("rows"),
                field("dateField"),
                Schema::enumeration(["day", "week", "month"]).field("unit"),
                field("field").optional(),
                agg().field("agg").optional(),
            ],
            bucket,
        )
        .returns(key_value()),
        FunctionDef::new(
            "Cumulative",
            "Copy of the rows with field replaced by its running total (field \"\" for a list of numbers)",
            [list().field("rows"), field("field")],
            cumulative,
        )
        .returns(list()),
        FunctionDef::new(
            "MovingAvg",
            "Copy of the rows with field replaced by the average of its last window values (field \"\" for a list of \
             numbers)",
            [list().field("rows"), field("field"), Schema::Number.field("window")],
            moving_avg,
        )
        .returns(list()),
        FunctionDef::new(
            "Change",
            "Percent change from previous to current, e.g. @Change(120, 100) is 20; null when previous is 0",
            [Schema::Number.field("current"), Schema::Number.field("previous")],
            change,
        )
        .returns(Schema::Number.nullable()),
        FunctionDef::new(
            "Histogram",
            "Counts numbers into equal-width bins (value is the count); min and max fix the range, default the data's",
            [
                numbers().field("values"),
                Schema::Number.field("bins"),
                Schema::Number.field("min").optional(),
                Schema::Number.field("max").optional(),
            ],
            histogram,
        )
        .returns(Schema::array(Schema::object([
            Schema::String.field("key"),
            Schema::Number.field("value"),
            Schema::Number.field("start"),
            Schema::Number.field("end"),
        ]))),
        FunctionDef::new(
            "Stats",
            "Summary of numbers, all zeros for an empty list",
            [numbers().field("values")],
            stats,
        )
        .returns(Schema::object(
            ["count", "min", "max", "mean", "median", "stdev"].map(|name| Schema::Number.field(name)),
        )),
    ]
}

fn key_value(key: Value, value: f64) -> Value {
    Value::from_iter([("key", key), ("value", Value::Number(value))])
}

/// Numbers that are finite; a pluck over rows that lack the field still holds nulls.
fn finite_only(values: &[Value]) -> Vec<f64> {
    values.iter().filter_map(Value::as_number).filter(|value| value.is_finite()).collect()
}

/// Each row with `field` replaced, or the values themselves for field `""`.
fn map_field(rows: &[Value], field: &str, values: Vec<f64>) -> Value {
    if field.is_empty() {
        return values.into_iter().map(Value::Number).collect();
    }
    let rows = rows.iter().zip(values).map(|(row, value)| {
        let mut map = spread(row);
        map.insert(field.to_owned(), Value::Number(value));
        Value::Object(js_order(map))
    });
    rows.collect()
}

/// `{...row}`: an object's fields, an array's or a string's items by index, nothing otherwise.
fn spread(row: &Value) -> Map {
    match row {
        Value::Object(map) => map.clone(),
        Value::Array(items) => {
            items.iter().enumerate().map(|(index, item)| (index.to_string(), item.clone())).collect()
        }
        Value::String(text) => text
            .encode_utf16()
            .enumerate()
            .map(|(index, unit)| (index.to_string(), Value::String(String::from_utf16_lossy(&[unit]))))
            .collect(),
        _ => Map::new(),
    }
}

fn group_by(args: &Args<'_>) -> Result<Value, FnError> {
    let rows = args.need_list("rows")?;
    let key = args.need_str("key")?;
    let field = args.str("field");
    let how = Agg::of(args, field);
    Ok(groups(rows, key).into_iter().map(|(key, members)| key_value(key, aggregate(&members, field, how))).collect())
}

fn pivot(args: &Args<'_>) -> Result<Value, FnError> {
    let rows = args.need_list("rows")?;
    let row_key = args.need_str("rowKey")?;
    let col_key = args.need_str("colKey")?;
    let field = args.str("field");
    let how = Agg::of(args, field);
    let columns: Vec<String> = groups(rows, col_key).into_iter().map(|(column, _)| js::to_string(&column)).collect();
    let out = groups(rows, row_key).into_iter().map(|(key, members)| {
        let mut row = Map::new();
        row.insert("key".to_owned(), key);
        for column in &columns {
            row.insert(column.clone(), Value::Number(0.0));
        }
        for (column, cell) in groups(members.iter().copied(), col_key) {
            row.insert(js::to_string(&column), Value::Number(aggregate(&cell, field, how)));
        }
        Value::Object(js_order(row))
    });
    Ok(out.collect())
}

fn top(args: &Args<'_>) -> Result<Value, FnError> {
    let rows = args.need_list("rows")?;
    let field = args.need_str("field")?;
    let n = args.need_number("n")?;
    let sign = if args.str("direction") == Some("asc") { 1.0 } else { -1.0 };
    let mut ranked: Vec<(usize, Option<f64>)> =
        rows.iter().enumerate().map(|(index, row)| (index, to_num(&read(row, field)))).collect();
    ranked.sort_by(|(a_index, a), (b_index, b)| match (a, b) {
        (Some(a), Some(b)) => {
            let difference = (a - b) * sign;
            difference.partial_cmp(&0.0).filter(|order| order.is_ne()).unwrap_or_else(|| a_index.cmp(b_index))
        }
        (None, None) => a_index.cmp(b_index),
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
    });
    let count = if n.trunc() > 0.0 { n.trunc() as usize } else { 0 };
    Ok(ranked.into_iter().take(count).map(|(index, _)| rows[index].clone()).collect())
}

/// Orders two field values: numbers by value, anything else, such as "YYYY-MM-DD", as text.
fn compare(a: &Value, b: &Value) -> f64 {
    if let (Some(x), Some(y)) = (to_num(a), to_num(b)) {
        return x - y;
    }
    match compare_text(&js::to_string(a), &js::to_string(b)) {
        Ordering::Less => -1.0,
        Ordering::Equal => 0.0,
        Ordering::Greater => 1.0,
    }
}

fn between(args: &Args<'_>) -> Result<Value, FnError> {
    let rows = args.need_list("rows")?;
    let field = args.need_str("field")?;
    let (min, max) = (args.get("min"), args.get("max"));
    let within = |row: &&Value| {
        let value = read(row, field);
        if value.is_null() {
            return false;
        }
        (min.is_null() || compare(&value, min) >= 0.0) && (max.is_null() || compare(&value, max) <= 0.0)
    };
    Ok(rows.iter().filter(within).cloned().collect())
}

fn bucket(args: &Args<'_>) -> Result<Value, FnError> {
    let rows = args.need_list("rows")?;
    let date_field = args.need_str("dateField")?;
    let unit = match args.need_str("unit")? {
        "day" => Period::Day,
        "week" => Period::Week,
        "month" => Period::Month,
        _ => return Err(super::expects("unit", "\"day\" | \"week\" | \"month\"")),
    };
    let field = args.str("field");
    let mut periods: BTreeMap<i64, Vec<&Value>> = BTreeMap::new();
    for row in rows {
        let raw = read(row, date_field);
        if !matches!(raw, Value::String(_) | Value::Number(_)) {
            continue;
        }
        let Ok(parsed) = parse_date(&raw, args.env()) else { continue };
        periods.entry(period_start(parsed.day, unit) as i64).or_default().push(row);
    }
    let (Some(&first), Some(&last)) = (periods.keys().next(), periods.keys().next_back()) else {
        return Ok(Value::Array(Vec::new()));
    };
    let how = Agg::of(args, field);
    let label = |start: f64| -> Result<Value, FnError> {
        let day = iso_day(start)?;
        Ok(Value::String(if unit == Period::Month { day.chars().take(7).collect() } else { day }))
    };
    let mut out = Vec::new();
    if !matches!(how, Agg::Sum | Agg::Count) {
        for (start, members) in &periods {
            out.push(key_value(label(*start as f64)?, aggregate(members, field, how)));
        }
        return Ok(Value::Array(out));
    }
    let mut start = first as f64;
    while start <= last as f64 {
        let value = periods.get(&(start as i64)).map_or(0.0, |members| aggregate(members, field, how));
        out.push(key_value(label(start)?, value));
        if out.len() > MAX_ITEMS {
            return Err(FnError::new(format!("more than {MAX_ITEMS} periods")));
        }
        start = next_period(start, unit);
    }
    Ok(Value::Array(out))
}

fn cumulative(args: &Args<'_>) -> Result<Value, FnError> {
    let rows = args.need_list("rows")?;
    let field = args.need_str("field")?;
    let mut total = 0.0;
    let totals = rows
        .iter()
        .map(|row| {
            total += to_num(&read(row, field)).unwrap_or(0.0);
            total
        })
        .collect();
    Ok(map_field(rows, field, totals))
}

fn moving_avg(args: &Args<'_>) -> Result<Value, FnError> {
    let rows = args.need_list("rows")?;
    let field = args.need_str("field")?;
    let window = args.need_number("window")?.trunc();
    if window.is_nan() || window < 1.0 {
        return Err(FnError::new("window must be at least 1"));
    }
    let values: Vec<f64> = rows.iter().map(|row| to_num(&read(row, field)).unwrap_or(0.0)).collect();
    let span = window.min(values.len().max(1) as f64) as usize;
    let averages = (0..values.len())
        .map(|index| {
            let from = (index + 1).saturating_sub(span);
            sum(&values[from..=index]) / (index + 1 - from) as f64
        })
        .collect();
    Ok(map_field(rows, field, averages))
}

/// No previous value is normal while data loads, so it is null without an error.
fn change(args: &Args<'_>) -> Result<Value, FnError> {
    let current = args.need_number("current")?;
    let previous = args.need_number("previous")?;
    if previous == 0.0 {
        return Ok(Value::Null);
    }
    finite((current - previous) / previous.abs() * 100.0)
}

fn histogram(args: &Args<'_>) -> Result<Value, FnError> {
    let values = finite_only(args.need_list("values")?);
    let bins = args.need_number("bins")?.trunc();
    if !(1.0..=500.0).contains(&bins) {
        return Err(FnError::new("bins must be from 1 to 500"));
    }
    let n = bins as usize;
    let low = args.number("min").unwrap_or_else(|| values.iter().copied().reduce(f64::min).unwrap_or(0.0));
    let mut high = args.number("max").unwrap_or_else(|| values.iter().copied().reduce(f64::max).unwrap_or(1.0));
    if high <= low {
        high = low + 1.0;
    }
    let width = (high - low) / bins;
    let decimals = (1.0 - width.log10().floor()).max(0.0);
    if decimals > 100.0 {
        return Err(FnError::new("toFixed() digits argument must be between 0 and 100"));
    }
    let mut counts = vec![0u32; n];
    for value in values {
        if value < low || value > high {
            continue;
        }
        let index = ((value - low) / width).floor();
        counts[if index >= (n - 1) as f64 { n - 1 } else { index as usize }] += 1;
    }
    let bins = counts.into_iter().enumerate().map(|(index, count)| {
        let start = low + index as f64 * width;
        let key = js::to_fixed(start, decimals as u8)
            .parse::<f64>()
            .map_or_else(|_| js::number_to_string(start), js::number_to_string);
        Value::from_iter([
            ("key", Value::String(key)),
            ("value", Value::from(count as usize)),
            ("start", Value::Number(start)),
            ("end", Value::Number(start + width)),
        ])
    });
    Ok(bins.collect())
}

fn stats(args: &Args<'_>) -> Result<Value, FnError> {
    let values = finite_only(args.need_list("values")?);
    let count = values.len();
    let summary = |numbers: [f64; 6]| {
        let names = ["count", "min", "max", "mean", "median", "stdev"];
        Value::from_iter(names.into_iter().zip(numbers.map(Value::Number)))
    };
    if count == 0 {
        return Ok(summary([0.0; 6]));
    }
    let mut sorted = values.clone();
    sorted.sort_by(f64::total_cmp);
    let mean = sum(&values) / count as f64;
    let middle = count / 2;
    let median = if count % 2 == 1 { sorted[middle] } else { (sorted[middle - 1] + sorted[middle]) / 2.0 };
    let squares: Vec<f64> = values.iter().map(|value| (value - mean) * (value - mean)).collect();
    let stdev = (sum(&squares) / count as f64).sqrt();
    Ok(summary([count as f64, sorted[0], sorted[count - 1], mean, median, stdev]))
}
