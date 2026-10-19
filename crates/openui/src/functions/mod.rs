//! The 59 functions of open-intelligent-ui (`src/lib/functions`), the generic `@Name(args)` a page
//! computes with: lists held in `$variables`, math, text, dates and the shaping of `Query` rows.
//! They are pure but for `@Random`, `@Pick`, `@Shuffle`, `@Now` and `@Today`, which read the
//! [`Env`](crate::Env). A bad argument, or an error, makes a call null.

mod colors;
mod data;
mod dates;
mod lists;
mod numbers;
#[cfg(test)]
mod tests;
mod text;

use std::cmp::Ordering;

use crate::js;
use crate::library::{Args, FnError, FunctionDef};
use crate::schema::Schema;
use crate::value::{Map, Value};

pub use dates::{ParsedDate, iso_day, parse_date};

/// The most items a function builds.
pub const MAX_ITEMS: usize = 10_000;

/// In the order open-intelligent-ui lists them.
pub fn functions() -> Vec<FunctionDef> {
    [
        lists::functions(),
        numbers::functions(),
        text::functions(),
        colors::functions(),
        dates::functions(),
        data::functions(),
    ]
    .concat()
}

fn list() -> Schema {
    Schema::array(Schema::Any)
}

fn numbers() -> Schema {
    Schema::array(Schema::Number)
}

fn date() -> Schema {
    Schema::union([Schema::String, Schema::Number])
}

fn agg() -> Schema {
    Schema::enumeration(["sum", "avg", "count", "min", "max"])
}

/// The arguments a function can't do without: missing, null or of the wrong type, the call fails.
trait Required<'a> {
    fn need(&self, name: &str) -> Result<&'a Value, FnError>;
    fn need_number(&self, name: &str) -> Result<f64, FnError>;
    fn need_str(&self, name: &str) -> Result<&'a str, FnError>;
    fn need_list(&self, name: &str) -> Result<&'a [Value], FnError>;
}

impl<'a> Required<'a> for Args<'a> {
    fn need(&self, name: &str) -> Result<&'a Value, FnError> {
        let value = self.get(name);
        if value.is_null() {
            return Err(FnError::new(format!("{name} is required")));
        }
        Ok(value)
    }

    fn need_number(&self, name: &str) -> Result<f64, FnError> {
        self.number(name).ok_or_else(|| expects(name, "number"))
    }

    fn need_str(&self, name: &str) -> Result<&'a str, FnError> {
        self.str(name).ok_or_else(|| expects(name, "string"))
    }

    fn need_list(&self, name: &str) -> Result<&'a [Value], FnError> {
        self.array(name).ok_or_else(|| expects(name, "array"))
    }
}

fn expects(name: &str, kind: &str) -> FnError {
    FnError::new(format!("{name} expects {kind}"))
}

fn too_many() -> FnError {
    FnError::new(format!("more than {MAX_ITEMS} items"))
}

/// A finite number, or the error the reference throws.
fn finite(value: f64) -> Result<Value, FnError> {
    if !value.is_finite() {
        return Err(FnError::new("result is not a finite number"));
    }
    Ok(Value::Number(value))
}

/// Reads `field` from a row, with dot paths; `""` is the row itself.
pub fn read(item: &Value, field: &str) -> Value {
    if field.is_empty() {
        return item.clone();
    }
    let mut current = item.clone();
    for key in field.split('.') {
        if !matches!(current, Value::Object(_) | Value::Array(_)) {
            return Value::Null;
        }
        current = js::property(&current, key);
    }
    current
}

/// Equality by value: lists and objects compare item by item, and a number matches its string
/// form, as ids typed into inputs are often strings.
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Null, Value::Null) => true,
        (Value::Number(_), Value::String(_)) | (Value::String(_), Value::Number(_)) => {
            js::to_string(a) == js::to_string(b)
        }
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(key, item)| y.get(key).is_some_and(|other| same(item, other)))
        }
        (Value::Element(x), Value::Element(y)) => std::sync::Arc::ptr_eq(x, y),
        (Value::Plan(x), Value::Plan(y)) => std::sync::Arc::ptr_eq(x, y),
        _ => false,
    }
}

/// A finite number, or a string that is one.
pub fn to_num(value: &Value) -> Option<f64> {
    let number = match value {
        Value::Number(number) => *number,
        Value::String(text) if !js::trim(text).is_empty() => js::string_to_number(text),
        _ => return None,
    };
    number.is_finite().then_some(number)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Agg {
    Sum,
    Avg,
    Count,
    Min,
    Max,
}

impl Agg {
    fn from_name(name: &str) -> Option<Agg> {
        Some(match name {
            "sum" => Agg::Sum,
            "avg" => Agg::Avg,
            "count" => Agg::Count,
            "min" => Agg::Min,
            "max" => Agg::Max,
            _ => return None,
        })
    }

    /// The `agg` argument, else a sum of the field, else a count of the rows.
    fn of(args: &Args<'_>, field: Option<&str>) -> Agg {
        let given = args.str("agg").and_then(Agg::from_name);
        given.unwrap_or(if field.is_some_and(|field| !field.is_empty()) { Agg::Sum } else { Agg::Count })
    }
}

/// Aggregates the numbers read from rows; a count counts the rows.
fn aggregate(rows: &[&Value], field: Option<&str>, how: Agg) -> f64 {
    if how == Agg::Count {
        return rows.len() as f64;
    }
    let values: Vec<f64> = rows.iter().filter_map(|row| to_num(&read(row, field.unwrap_or_default()))).collect();
    if values.is_empty() {
        return 0.0;
    }
    match how {
        Agg::Sum => sum(&values),
        Agg::Avg => sum(&values) / values.len() as f64,
        Agg::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
        Agg::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        Agg::Count => unreachable!(),
    }
}

/// `values.reduce((a, b) => a + b, 0)`.
fn sum(values: &[f64]) -> f64 {
    values.iter().fold(0.0, |total, value| total + value)
}

/// Rows grouped by a key in first-seen order. A number keys as itself, anything else as text, a
/// missing or empty one as "Unknown".
fn groups<'a>(rows: impl IntoIterator<Item = &'a Value>, key: &str) -> Vec<(Value, Vec<&'a Value>)> {
    let mut out: Vec<(Value, Vec<&'a Value>)> = Vec::new();
    for row in rows {
        let raw = read(row, key);
        let group = match raw {
            Value::Number(number) => Value::Number(number),
            Value::Null => Value::from("Unknown"),
            Value::String(text) if text.is_empty() => Value::from("Unknown"),
            raw => Value::from(js::to_string(&raw)),
        };
        let found = out.iter_mut().find(|(existing, _)| match (existing, &group) {
            (Value::Number(a), Value::Number(b)) => a == b || (a.is_nan() && b.is_nan()),
            (Value::String(a), Value::String(b)) => a == b,
            _ => false,
        });
        match found {
            Some((_, members)) => members.push(row),
            None => out.push((group, vec![row])),
        }
    }
    out
}

/// An object's keys in JavaScript's order: array indices first, ascending, then the others in
/// the order they were added.
fn js_order(map: Map) -> Map {
    let index = |key: &str| key.parse::<u32>().ok().filter(|index| index.to_string() == key && *index != u32::MAX);
    if !map.keys().any(|key| index(key).is_some()) {
        return map;
    }
    let (mut indices, others): (Vec<_>, Vec<_>) = map.into_iter().partition(|(key, _)| index(key).is_some());
    indices.sort_by_key(|(key, _)| index(key));
    indices.into_iter().chain(others).collect()
}

/// `a < b` on strings: by UTF-16 code units.
fn compare_text(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}
