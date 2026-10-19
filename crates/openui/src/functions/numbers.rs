//! Math: powers, random draws, clamping, percentages, splitting a total, loan payments and units.

use super::{Required, finite, numbers, sum};
use crate::js;
use crate::library::{Args, FnError, FunctionDef};
use crate::schema::Schema;
use crate::value::Value;

pub(super) fn functions() -> Vec<FunctionDef> {
    let number = |name: &str| Schema::Number.field(name);
    vec![
        FunctionDef::new("Pow", "base raised to exponent", [number("base"), number("exponent")], pow)
            .returns(Schema::Number),
        FunctionDef::new("Sqrt", "Square root; null for a negative number", [number("number")], sqrt)
            .returns(Schema::Number),
        FunctionDef::new(
            "Random",
            "Random number from min to max (integer true: a whole number, max included); draws anew each time, so \
             call it inside @Set",
            [number("min"), number("max"), Schema::Boolean.field("integer").optional()],
            random,
        )
        .returns(Schema::Number),
        FunctionDef::new(
            "Clamp",
            "number kept between min and max",
            [number("number"), number("min"), number("max")],
            clamp,
        )
        .returns(Schema::Number),
        FunctionDef::new(
            "Percent",
            "part / total * 100, e.g. @Percent(3, 4) is 75; 0 when total is 0",
            [number("part"), number("total")],
            percent,
        )
        .returns(Schema::Number),
        FunctionDef::new(
            "Lerp",
            "The value a fraction t of the way from a to b",
            [number("a"), number("b"), number("t")],
            lerp,
        )
        .returns(Schema::Number),
        FunctionDef::new(
            "Allocate",
            "Splits total in proportion to weights, rounded to decimals (default 2), so the parts add up to exactly \
             total, e.g. @Allocate(100, [1, 1, 1]) is [33.34, 33.33, 33.33]",
            [number("total"), numbers().field("weights"), number("decimals").optional()],
            allocate,
        )
        .returns(numbers()),
        FunctionDef::new(
            "Payment",
            "Payment per period that repays principal over periods at rate percent per period, e.g. a 30-year loan \
             at 6% a year: @Payment(300000, 6 / 12, 360)",
            [number("principal"), number("rate"), number("periods")],
            payment,
        )
        .returns(Schema::Number),
        FunctionDef::new(
            "Convert",
            "value from one unit to another: mm cm m km in ft yd mi; mg g kg oz lb st; ml l tsp tbsp floz cup pt qt \
             gal (US); m/s km/h mph kn; m2 km2 ft2 ha acre; c f k",
            [number("value"), Schema::String.field("from"), Schema::String.field("to")],
            convert,
        )
        .returns(Schema::Number),
    ]
}

fn pow(args: &Args<'_>) -> Result<Value, FnError> {
    let base = args.need_number("base")?;
    let exponent = args.need_number("exponent")?;
    if base.abs() == 1.0 && exponent.is_infinite() {
        return finite(f64::NAN);
    }
    finite(base.powf(exponent))
}

fn sqrt(args: &Args<'_>) -> Result<Value, FnError> {
    finite(args.need_number("number")?.sqrt())
}

fn random(args: &Args<'_>) -> Result<Value, FnError> {
    let min = args.need_number("min")?;
    let max = args.need_number("max")?;
    let (low, high) = (js_min(min, max), js_max(min, max));
    let draw = args.env().random();
    if args.bool("integer") != Some(true) {
        return Ok(Value::Number(low + draw * (high - low)));
    }
    let (first, last) = (low.ceil(), high.floor());
    if first > last {
        return Err(FnError::new("no whole number between min and max"));
    }
    Ok(Value::Number(first + (draw * (last - first + 1.0)).floor()))
}

/// `Math.min` of two: NaN when either is.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.min(b) }
}

fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }
}

fn clamp(args: &Args<'_>) -> Result<Value, FnError> {
    let number = args.need_number("number")?;
    let min = args.need_number("min")?;
    let max = args.need_number("max")?;
    Ok(Value::Number(js_min(js_max(number, js_min(min, max)), js_max(min, max))))
}

fn percent(args: &Args<'_>) -> Result<Value, FnError> {
    let part = args.need_number("part")?;
    let total = args.need_number("total")?;
    if total == 0.0 {
        return Ok(Value::Number(0.0));
    }
    finite(part / total * 100.0)
}

fn lerp(args: &Args<'_>) -> Result<Value, FnError> {
    let a = args.need_number("a")?;
    let b = args.need_number("b")?;
    let t = args.need_number("t")?;
    finite(a + (b - a) * t)
}

/// Whole units (cents for 2 decimals) in proportion; the units left over go to the largest
/// remainders.
fn allocate(args: &Args<'_>) -> Result<Value, FnError> {
    let total = args.need_number("total")?;
    let weights: Vec<f64> = args.need_list("weights")?.iter().filter_map(Value::as_number).collect();
    let weight_sum = sum(&weights);
    if weights.is_empty() || weights.iter().any(|weight| !weight.is_finite() || *weight < 0.0) {
        return Err(FnError::new("weights must be zero or more"));
    }
    if weight_sum == 0.0 {
        return Ok(weights.iter().map(|_| Value::Number(0.0)).collect());
    }
    let decimals = js::round(args.number("decimals").unwrap_or(2.0)).clamp(0.0, 6.0);
    let scale = 10f64.powf(decimals);
    let units = js::round(total.abs() * scale);
    let exact: Vec<f64> = weights.iter().map(|weight| units * weight / weight_sum).collect();
    let mut parts: Vec<f64> = exact.iter().map(|share| share.floor()).collect();
    let mut left = units - sum(&parts);
    let mut order: Vec<usize> = (0..exact.len()).collect();
    let rest = |index: usize| exact[index] - exact[index].floor();
    order.sort_by(|&a, &b| rest(b).total_cmp(&rest(a)).then(a.cmp(&b)));
    for index in order {
        if left <= 0.0 {
            break;
        }
        parts[index] += 1.0;
        left -= 1.0;
    }
    Ok(parts.into_iter().map(|part| Value::Number(if total < 0.0 { -part } else { part } / scale)).collect())
}

fn payment(args: &Args<'_>) -> Result<Value, FnError> {
    let principal = args.need_number("principal")?;
    let rate = args.need_number("rate")?;
    let periods = args.need_number("periods")?;
    if periods.is_nan() || periods <= 0.0 {
        return Err(FnError::new("periods must be positive"));
    }
    let rate = rate / 100.0;
    if rate == 0.0 {
        return finite(principal / periods);
    }
    finite(principal * rate / (1.0 - (1.0 + rate).powf(-periods)))
}

#[derive(PartialEq)]
enum Kind {
    Length,
    Mass,
    Volume,
    Speed,
    Area,
}

/// A unit's kind and its factor to the kind's base unit (m, kg, l, m/s, m²).
fn unit(name: &str) -> Option<(Kind, f64)> {
    Some(match name {
        "mm" => (Kind::Length, 0.001),
        "cm" => (Kind::Length, 0.01),
        "m" => (Kind::Length, 1.0),
        "km" => (Kind::Length, 1000.0),
        "in" => (Kind::Length, 0.0254),
        "ft" => (Kind::Length, 0.3048),
        "yd" => (Kind::Length, 0.9144),
        "mi" => (Kind::Length, 1609.344),
        "mg" => (Kind::Mass, 1e-6),
        "g" => (Kind::Mass, 0.001),
        "kg" => (Kind::Mass, 1.0),
        "oz" => (Kind::Mass, 0.028349523125),
        "lb" => (Kind::Mass, 0.45359237),
        "st" => (Kind::Mass, 6.35029318),
        "ml" => (Kind::Volume, 0.001),
        "l" => (Kind::Volume, 1.0),
        "tsp" => (Kind::Volume, 0.00492892159375),
        "tbsp" => (Kind::Volume, 0.0147867647812),
        "floz" => (Kind::Volume, 0.0295735295625),
        "cup" => (Kind::Volume, 0.2365882365),
        "pt" => (Kind::Volume, 0.473176473),
        "qt" => (Kind::Volume, 0.946352946),
        "gal" => (Kind::Volume, 3.785411784),
        "m/s" => (Kind::Speed, 1.0),
        "km/h" => (Kind::Speed, 1.0 / 3.6),
        "mph" => (Kind::Speed, 0.44704),
        "kn" => (Kind::Speed, 0.514444),
        "m2" => (Kind::Area, 1.0),
        "km2" => (Kind::Area, 1e6),
        "ft2" => (Kind::Area, 0.09290304),
        "ha" => (Kind::Area, 10000.0),
        "acre" => (Kind::Area, 4046.8564224),
        _ => return None,
    })
}

fn to_celsius(value: f64, unit: &str) -> f64 {
    match unit {
        "f" => (value - 32.0) * 5.0 / 9.0,
        "k" => value - 273.15,
        _ => value,
    }
}

fn from_celsius(value: f64, unit: &str) -> f64 {
    match unit {
        "f" => value * 9.0 / 5.0 + 32.0,
        "k" => value + 273.15,
        _ => value,
    }
}

fn convert(args: &Args<'_>) -> Result<Value, FnError> {
    let value = args.need_number("value")?;
    let from_name = args.need_str("from")?;
    let to_name = args.need_str("to")?;
    let (from, to) = (js::trim(from_name).to_lowercase(), js::trim(to_name).to_lowercase());
    let temperature = |unit: &str| ["c", "f", "k"].contains(&unit);
    if temperature(&from) && temperature(&to) {
        return finite(from_celsius(to_celsius(value, &from), &to));
    }
    match (unit(&from), unit(&to)) {
        (Some((from_kind, from_factor)), Some((to_kind, to_factor))) if from_kind == to_kind => {
            finite(value * from_factor / to_factor)
        }
        _ => Err(FnError::new(format!("cannot convert {from_name} to {to_name}"))),
    }
}
