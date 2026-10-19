//! The JavaScript semantics the spec asks every port to reproduce (language.md §3): ECMA-262
//! ToNumber, IsLooselyEqual, ToBoolean, Number::toString and `String(value)`, the operators built on
//! them, member access, `Math.round`, `parseFloat` and `localeCompare`.
//!
//! Elements and plans act as plain objects. Arrays and objects have no identity here, so two of
//! them are never equal, as two objects evaluated apart never are in JavaScript.

use std::cmp::Ordering;
use std::sync::LazyLock;

use icu_collator::options::CollatorOptions;
use icu_collator::{Collator, CollatorBorrowed};

use crate::value::Value;

/// ECMA-262 StringToNumber: NaN when the text is not a number.
pub fn string_to_number(text: &str) -> f64 {
    let text = trim(text);
    if text.is_empty() {
        return 0.0;
    }
    if let Some(value) = radix_number(text) {
        return value;
    }
    let (sign, body) = match text.as_bytes()[0] {
        b'+' => (1.0, &text[1..]),
        b'-' => (-1.0, &text[1..]),
        _ => (1.0, text),
    };
    if body == "Infinity" {
        return sign * f64::INFINITY;
    }
    if decimal_prefix(body) != body.len() {
        return f64::NAN;
    }
    body.parse::<f64>().map_or(f64::NAN, |value| sign * value)
}

/// `0x…`, `0o…` and `0b…`: their value, NaN when a digit is wrong, None for other text.
fn radix_number(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'0' {
        return None;
    }
    let radix = match bytes[1] {
        b'x' | b'X' => 16,
        b'o' | b'O' => 8,
        b'b' | b'B' => 2,
        _ => return None,
    };
    let digits = &text[2..];
    if digits.is_empty() {
        return Some(f64::NAN);
    }
    let mut exact: Option<u128> = Some(0);
    let mut approximate = 0.0;
    for c in digits.chars() {
        let Some(digit) = c.to_digit(radix) else { return Some(f64::NAN) };
        exact = exact.and_then(|value| value.checked_mul(radix.into())?.checked_add(digit.into()));
        approximate = approximate * f64::from(radix) + f64::from(digit);
    }
    Some(exact.map_or(approximate, |value| value as f64))
}

/// The length of the longest prefix that is an unsigned StrDecimalLiteral without `Infinity`.
fn decimal_prefix(text: &str) -> usize {
    let bytes = text.as_bytes();
    let digits_from = |start: usize| bytes[start..].iter().take_while(|b| b.is_ascii_digit()).count();
    let whole = digits_from(0);
    let mut end = whole;
    let mut fraction = 0;
    if bytes.get(end) == Some(&b'.') {
        fraction = digits_from(end + 1);
        if whole > 0 || fraction > 0 {
            end += 1 + fraction;
        }
    }
    if whole == 0 && fraction == 0 {
        return 0;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(bytes.get(end + 1), Some(b'+' | b'-')));
        let exponent = digits_from(end + 1 + sign);
        if exponent > 0 {
            end += 1 + sign + exponent;
        }
    }
    end
}

/// The spec's `toNumber`: a string that is not a number is 0, `true` is 1, anything else but a
/// number is 0.
pub fn to_number(value: &Value) -> f64 {
    match value {
        Value::Number(value) => *value,
        Value::String(text) => {
            let value = string_to_number(text);
            if value.is_nan() { 0.0 } else { value }
        }
        Value::Bool(true) => 1.0,
        _ => 0.0,
    }
}

/// ECMAScript `parseFloat`: the number at the start of the text, NaN when there is none.
pub fn parse_float(text: &str) -> f64 {
    let text = text.trim_start_matches(is_whitespace);
    let (sign, body) = match text.as_bytes().first() {
        Some(b'+') => (1.0, &text[1..]),
        Some(b'-') => (-1.0, &text[1..]),
        _ => (1.0, text),
    };
    if body.starts_with("Infinity") {
        return sign * f64::INFINITY;
    }
    let end = decimal_prefix(body);
    if end == 0 {
        return f64::NAN;
    }
    body[..end].parse::<f64>().map_or(f64::NAN, |value| sign * value)
}

/// ToBoolean.
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => *value != 0.0 && !value.is_nan(),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) | Value::Element(_) | Value::Plan(_) => true,
    }
}

/// IsLooselyEqual, `a == b`.
pub fn loose_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Null, _) | (_, Value::Null) => false,
        (Value::Number(a), Value::Number(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Number(a), Value::String(b)) | (Value::String(b), Value::Number(a)) => *a == string_to_number(b),
        (Value::Bool(a), other) | (other, Value::Bool(a)) => loose_eq(&Value::Number(f64::from(u8::from(*a))), other),
        (primitive @ (Value::Number(_) | Value::String(_)), object)
        | (object, primitive @ (Value::Number(_) | Value::String(_))) => {
            loose_eq(primitive, &Value::String(to_string(object)))
        }
        _ => false,
    }
}

/// SameValueZero, what `includes` and `indexOf`-like lookups compare with: like `===`, but NaN
/// equals NaN.
pub fn same_value_zero(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Number(a), Value::Number(b)) => a == b || (a.is_nan() && b.is_nan()),
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        _ => false,
    }
}

/// Number::toString.
pub fn number_to_string(value: f64) -> String {
    ryu_js::Buffer::new().format(value).to_owned()
}

/// `Number.prototype.toFixed(digits)`, digits up to 100.
pub fn to_fixed(value: f64, digits: u8) -> String {
    ryu_js::Buffer::new().format_to_fixed(value, digits).to_owned()
}

/// `String(value)`.
pub fn to_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => number_to_string(*value),
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| if item.is_null() { String::new() } else { to_string(item) })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) | Value::Element(_) | Value::Plan(_) => "[object Object]".to_owned(),
    }
}

/// `a + b`: joins strings when either side is one, with null as `""`; adds with `toNumber`
/// otherwise.
pub fn add(a: &Value, b: &Value) -> Value {
    if !matches!(a, Value::String(_)) && !matches!(b, Value::String(_)) {
        return Value::Number(to_number(a) + to_number(b));
    }
    let text = |value: &Value| if value.is_null() { String::new() } else { to_string(value) };
    Value::String(text(a) + &text(b))
}

pub fn subtract(a: &Value, b: &Value) -> f64 {
    to_number(a) - to_number(b)
}

pub fn multiply(a: &Value, b: &Value) -> f64 {
    to_number(a) * to_number(b)
}

/// `a / b`, 0 when `b` is 0.
pub fn divide(a: &Value, b: &Value) -> f64 {
    let divisor = to_number(b);
    if divisor == 0.0 { 0.0 } else { to_number(a) / divisor }
}

/// `a % b`, JavaScript's remainder (the dividend's sign), 0 when `b` is 0.
pub fn remainder(a: &Value, b: &Value) -> f64 {
    let divisor = to_number(b);
    if divisor == 0.0 { 0.0 } else { to_number(a) % divisor }
}

/// Unary `-`.
pub fn negate(value: &Value) -> f64 {
    -to_number(value)
}

/// `Math.round`: a half rounds up, toward positive infinity.
pub fn round(value: f64) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    if (-0.5..0.0).contains(&value) {
        return -0.0;
    }
    let floor = value.floor();
    if value - floor >= 0.5 { floor + 1.0 } else { floor }
}

static COLLATOR: LazyLock<Option<CollatorBorrowed<'static>>> =
    LazyLock::new(|| Collator::try_new(Default::default(), CollatorOptions::default()).ok());

/// `a.localeCompare(b)` with no arguments: the root collation, as an English runtime's.
pub fn locale_compare(a: &str, b: &str) -> Ordering {
    let Some(collator) = COLLATOR.as_ref() else { return a.cmp(b) };
    collator.compare(a, b)
}

/// WhiteSpace and LineTerminator, what `trim` and ToNumber skip.
pub fn is_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// `String.prototype.trim`.
pub fn trim(text: &str) -> &str {
    text.trim_matches(is_whitespace)
}

/// A string's `length`: its UTF-16 code units.
pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// `value[key]` with a string key: an object's field, an array's or a string's `length` or item.
pub fn property(value: &Value, key: &str) -> Value {
    match value {
        Value::Object(map) => map.get(key).cloned().unwrap_or_default(),
        Value::Array(items) if key == "length" => Value::from(items.len()),
        Value::Array(items) => array_index(key).and_then(|index| items.get(index)).cloned().unwrap_or_default(),
        Value::String(text) if key == "length" => Value::from(utf16_len(text)),
        Value::String(text) => array_index(key).map_or(Value::Null, |index| code_unit(text, index)),
        _ => Value::Null,
    }
}

/// `"0"`, `"1"`… but not `"01"` or `"+1"`.
fn array_index(key: &str) -> Option<usize> {
    let index = key.parse::<u32>().ok()?;
    if index.to_string() != key {
        return None;
    }
    Some(index as usize)
}

fn code_unit(text: &str, index: usize) -> Value {
    let Some(unit) = text.encode_utf16().nth(index) else { return Value::Null };
    Value::String(String::from_utf16_lossy(&[unit]))
}

/// `a.field` (§3.3): null on null, a pluck on an array (`length` is its count), a field otherwise.
pub fn member(value: &Value, field: &str) -> Value {
    match value {
        Value::Array(items) if field != "length" => items.iter().map(|item| property(item, field)).collect(),
        value => property(value, field),
    }
}

/// `a[i]` (§3.3): arrays by number, anything else by the index as a string; null when either is.
pub fn index(value: &Value, index: &Value) -> Value {
    if value.is_null() || index.is_null() {
        return Value::Null;
    }
    let Value::Array(items) = value else { return property(value, &to_string(index)) };
    let position = to_number(index);
    if position < 0.0 || position.fract() != 0.0 {
        return Value::Null;
    }
    items.get(position as usize).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Map;

    fn n(value: f64) -> Value {
        Value::Number(value)
    }

    fn s(text: &str) -> Value {
        Value::from(text)
    }

    #[test]
    fn converts_strings_to_numbers_as_ecmascript() {
        let cases = [
            ("42", 42.0),
            ("  \t\n4.5e1\u{a0}\u{feff}", 45.0),
            ("", 0.0),
            ("   ", 0.0),
            ("-0", -0.0),
            ("+.5", 0.5),
            ("5.", 5.0),
            ("1E+2", 100.0),
            ("0x1F", 31.0),
            ("0XfF", 255.0),
            ("0o17", 15.0),
            ("0b101", 5.0),
            ("Infinity", f64::INFINITY),
            ("-Infinity", f64::NEG_INFINITY),
            ("0x1fffffffffffff1", 144115188075855860.0),
        ];
        for (text, expected) in cases {
            let value = string_to_number(text);
            assert!(value == expected && value.is_sign_negative() == expected.is_sign_negative(), "{text:?}");
        }
        for text in ["abc", "1e", "1e+", ".", "-0x10", "0x", "0xg", "1_000", "inf", "infinity", "NaN", "12px", "1 2"] {
            assert!(string_to_number(text).is_nan(), "{text:?}");
        }
    }

    #[test]
    fn to_number_never_gives_nan() {
        assert_eq!(to_number(&s("abc")), 0.0);
        assert_eq!(to_number(&s("7")), 7.0);
        assert_eq!(to_number(&Value::Bool(true)), 1.0);
        assert_eq!(to_number(&Value::Bool(false)), 0.0);
        assert_eq!(to_number(&Value::Null), 0.0);
        assert_eq!(to_number(&Value::Array(vec![n(5.0)])), 0.0);
        assert_eq!(to_number(&Value::Object(Map::new())), 0.0);
    }

    #[test]
    fn parses_floats_as_ecmascript() {
        assert_eq!(parse_float("12abc"), 12.0);
        assert_eq!(parse_float("  -3.5e2x"), -350.0);
        assert_eq!(parse_float(".5."), 0.5);
        assert_eq!(parse_float("1e"), 1.0);
        assert_eq!(parse_float("0x10"), 0.0);
        assert_eq!(parse_float("-Infinityx"), f64::NEG_INFINITY);
        assert!(parse_float("abc").is_nan());
        assert!(parse_float("").is_nan());
        assert!(parse_float(".").is_nan());
    }

    #[test]
    fn compares_loosely() {
        let object = Value::Object(Map::new());
        let cases = [
            (n(5.0), s("5"), true),
            (s(""), n(0.0), true),
            (Value::Null, n(0.0), false),
            (Value::Null, Value::Null, true),
            (Value::Null, s(""), false),
            (Value::Bool(true), n(1.0), true),
            (Value::Bool(true), s("1"), true),
            (Value::Bool(false), s(""), true),
            (s("0x10"), n(16.0), true),
            (n(f64::NAN), n(f64::NAN), false),
            (Value::Array(vec![n(1.0), n(2.0)]), s("1,2"), true),
            (Value::Array(vec![n(1.0)]), Value::Bool(true), true),
            (Value::Array(vec![]), n(0.0), true),
            (object.clone(), s("[object Object]"), true),
            (object.clone(), object, false),
            (Value::Array(vec![]), Value::Array(vec![]), false),
            (s("a"), s("a"), true),
            (s("a"), s("b"), false),
        ];
        for (a, b, expected) in cases {
            assert_eq!(loose_eq(&a, &b), expected, "{a:?} == {b:?}");
            assert_eq!(loose_eq(&b, &a), expected, "{b:?} == {a:?}");
        }
    }

    #[test]
    fn follows_to_boolean() {
        for value in [Value::Null, Value::Bool(false), n(0.0), n(-0.0), n(f64::NAN), s("")] {
            assert!(!truthy(&value), "{value:?}");
        }
        for value in [n(-1.0), s("0"), s("false"), Value::Array(vec![]), Value::Object(Map::new())] {
            assert!(truthy(&value), "{value:?}");
        }
    }

    #[test]
    fn writes_numbers_as_javascript() {
        let cases = [
            (0.0, "0"),
            (-0.0, "0"),
            (1.0, "1"),
            (-1.5, "-1.5"),
            (0.1 + 0.2, "0.30000000000000004"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (1.5e-7, "1.5e-7"),
            (0.000001, "0.000001"),
            (123456789.125, "123456789.125"),
            (f64::NAN, "NaN"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
        ];
        for (value, expected) in cases {
            assert_eq!(number_to_string(value), expected);
        }
        assert_eq!(to_fixed(1.005, 2), "1.00");
        assert_eq!(to_fixed(2.5, 0), "3");
    }

    #[test]
    fn writes_every_value_as_string() {
        let list =
            Value::Array(vec![n(1.0), Value::Null, s("a"), Value::Array(vec![n(2.0), n(3.0)]), Value::Bool(true)]);
        assert_eq!(to_string(&list), "1,,a,2,3,true");
        assert_eq!(to_string(&Value::Null), "null");
        assert_eq!(to_string(&Value::Object(Map::new())), "[object Object]");
        assert_eq!(to_string(&n(2.0)), "2");
    }

    #[test]
    fn rounds_as_math_round() {
        let cases = [
            (2.5, 3.0),
            (-2.5, -2.0),
            (-2.6, -3.0),
            (0.49999999999999994, 0.0),
            (-0.4, -0.0),
            (1.4999999999999998, 1.0),
            (4503599627370497.0, 4503599627370497.0),
        ];
        for (value, expected) in cases {
            let rounded = round(value);
            assert!(rounded == expected && rounded.is_sign_negative() == expected.is_sign_negative(), "{value}");
        }
    }

    #[test]
    fn compares_by_locale() {
        let mut words = vec!["b", "B", "a", "é", "Z", "e", "E"];
        words.sort_by(|a, b| locale_compare(a, b));
        assert_eq!(words, ["a", "b", "B", "e", "E", "é", "Z"]);
        assert_eq!(locale_compare("", "a"), Ordering::Less);
        assert_eq!(locale_compare("a", "a"), Ordering::Equal);
        assert_eq!(locale_compare("10", "9"), Ordering::Less);
    }

    #[test]
    fn reads_members_and_indexes() {
        let rows = Value::from_json_str(r#"[{"a": 1}, {"b": 2}, [5, 6], "xy"]"#).unwrap();
        assert_eq!(member(&rows, "a"), Value::from_json_str(r#"[1, null, null, null]"#).unwrap());
        assert_eq!(member(&rows, "0"), Value::from_json_str(r#"[null, null, 5, "x"]"#).unwrap());
        assert_eq!(member(&rows, "length"), n(4.0));
        assert_eq!(member(&Value::Null, "a"), Value::Null);
        assert_eq!(member(&s("héllo"), "length"), n(5.0));
        assert_eq!(index(&rows, &n(2.0)), Value::from_json_str("[5, 6]").unwrap());
        assert_eq!(index(&rows, &s("1")), Value::from_json_str(r#"{"b": 2}"#).unwrap());
        assert_eq!(index(&rows, &n(1.5)), Value::Null);
        assert_eq!(index(&rows, &n(-1.0)), Value::Null);
        assert_eq!(index(&rows, &n(9.0)), Value::Null);
        assert_eq!(index(&rows, &Value::Null), Value::Null);
        let object = Value::from_json_str(r#"{"1": "one", "k": "v"}"#).unwrap();
        assert_eq!(index(&object, &n(1.0)), s("one"));
        assert_eq!(index(&object, &s("k")), s("v"));
        assert_eq!(index(&s("abc"), &n(1.0)), s("b"));
    }

    /// `evaluation/005-op-coerce-values`, each expression of its two columns by hand.
    #[test]
    fn matches_the_coercion_fixture() {
        let path =
            concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/openui/1.0/evaluation/005-op-coerce-values/expected.json");
        let expected = Value::from_json_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let columns = expected.get("root").and_then(|root| root.get("props")).and_then(|props| props.get("children"));
        let table = &columns.and_then(Value::as_array).unwrap()[0];
        let columns = table.get("props").and_then(|props| props.get("columns")).and_then(Value::as_array).unwrap();
        let data = |column: usize| columns[column].get("props").and_then(|props| props.get("data")).cloned().unwrap();

        let unset = Value::Null;
        let or = |a: Value, b: Value| if truthy(&a) { a } else { b };
        let and = |a: Value, b: Value| if truthy(&a) { b } else { a };
        let coerce = vec![
            n(multiply(&s("5"), &n(2.0))),
            n(multiply(&s("abc"), &n(2.0))),
            add(&Value::Bool(true), &n(1.0)),
            add(&Value::Null, &n(1.0)),
            add(&s("a"), &Value::Null),
            Value::Bool(loose_eq(&n(5.0), &s("5"))),
            Value::Bool(loose_eq(&s(""), &n(0.0))),
            Value::Bool(loose_eq(&Value::Null, &n(0.0))),
        ];
        let op = vec![
            n(divide(&n(7.0), &n(0.0))),
            n(remainder(&n(7.0), &n(0.0))),
            add(&s("a"), &n(1.0)),
            add(&add(&n(1.0), &n(2.0)), &s("x")),
            or(unset, s("fallback")),
            and(n(0.0), s("no")),
            Value::Bool(!truthy(&n(0.0))),
            n(negate(&s("3"))),
            Value::Bool(to_number(&n(3.0)) >= to_number(&s("3"))),
        ];
        assert_eq!(Value::Array(coerce), data(0));
        assert_eq!(Value::Array(op), data(1));
    }
}
