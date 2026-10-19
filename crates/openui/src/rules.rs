//! Validation rules of input components (language.md §5.4): `{ required: true, email: true,
//! maxLength: 80 }`, or the legacy list `["required", "minLength:3"]`.

use crate::js;
use crate::value::Value;

/// The first rule the value fails, as the message the field shows; None when it passes.
pub fn validate(value: &Value, rules: &Value) -> Option<String> {
    parse(rules).into_iter().find_map(|(rule, arg)| check(&rule, &arg, value))
}

/// The rules in order, each with its argument; a rule set to false or null is off.
pub fn parse(rules: &Value) -> Vec<(String, Value)> {
    match rules {
        Value::Object(map) => map
            .iter()
            .filter(|(_, arg)| !matches!(arg, Value::Bool(false) | Value::Null))
            .map(|(rule, arg)| (rule.clone(), if *arg == Value::Bool(true) { Value::Null } else { arg.clone() }))
            .collect(),
        Value::Array(items) => items.iter().filter_map(Value::as_str).map(legacy).collect(),
        _ => Vec::new(),
    }
}

/// `"min:8"`: the numeric rules take a number.
fn legacy(rule: &str) -> (String, Value) {
    let Some((name, arg)) = rule.split_once(':') else { return (rule.to_owned(), Value::Null) };
    let numeric = matches!(name, "min" | "max" | "minLength" | "maxLength") && !js::string_to_number(arg).is_nan();
    let arg = if numeric { Value::Number(js::string_to_number(arg)) } else { Value::from(arg) };
    (name.to_owned(), arg)
}

/// Null, `""`, an empty array or an object with no keys; a form field by its value.
fn is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Object(map) if map.contains_key("value") => is_empty(&map["value"]),
        Value::Object(map) => map.is_empty(),
        _ => false,
    }
}

fn arg_number(arg: &Value) -> f64 {
    match arg {
        Value::Number(number) => *number,
        Value::String(text) => js::string_to_number(text),
        Value::Bool(value) => f64::from(u8::from(*value)),
        Value::Null => 0.0,
        _ => f64::NAN,
    }
}

fn number_of(value: &Value) -> f64 {
    match value {
        Value::Number(number) => *number,
        value => js::parse_float(&js::to_string(value)),
    }
}

fn check(rule: &str, arg: &Value, value: &Value) -> Option<String> {
    if rule == "required" {
        if is_empty(value) {
            return Some("This field is required".to_owned());
        }
        let Value::Object(map) = value else { return None };
        let all_off = map.values().all(|item| matches!(item, Value::Bool(false)));
        return all_off.then(|| "At least one option is required".to_owned());
    }
    if is_empty(value) {
        return None;
    }
    let text = value.as_str();
    let failed = match rule {
        "email" => !text.is_some_and(is_email),
        "url" => !text.is_some_and(is_url),
        "numeric" => match value {
            Value::Number(number) => number.is_nan(),
            Value::String(text) => js::trim(text).is_empty() || js::parse_float(text).is_nan(),
            _ => true,
        },
        "min" => !number_of(value).is_nan() && !at_least(number_of(value), arg_number(arg)),
        "max" => !number_of(value).is_nan() && !at_least(arg_number(arg), number_of(value)),
        "minLength" => text.is_some_and(|text| !at_least(js::utf16_len(text) as f64, arg_number(arg))),
        "maxLength" => text.is_some_and(|text| !at_least(arg_number(arg), js::utf16_len(text) as f64)),
        "pattern" => match (text, arg.as_str()) {
            (Some(text), Some(pattern)) => regress::Regex::new(pattern).is_ok_and(|regex| regex.find(text).is_none()),
            _ => false,
        },
        _ => false,
    };
    failed.then(|| message(rule, arg))
}

/// `a >= b`, false when either is NaN.
fn at_least(a: f64, b: f64) -> bool {
    a >= b
}

fn message(rule: &str, arg: &Value) -> String {
    let arg = js::to_string(arg);
    match rule {
        "email" => "Please enter a valid email".to_owned(),
        "url" => "Please enter a valid URL".to_owned(),
        "numeric" => "Must be a number".to_owned(),
        "min" => format!("Must be at least {arg}"),
        "max" => format!("Must be no more than {arg}"),
        "minLength" => format!("Must be at least {arg} characters"),
        "maxLength" => format!("Must be no more than {arg} characters"),
        _ => "Invalid format".to_owned(),
    }
}

/// `^[^\s@]+@[^\s@]+\.[^\s@]+$`.
fn is_email(text: &str) -> bool {
    let Some((local, domain)) = text.split_once('@') else { return false };
    let plain = |part: &str| !part.is_empty() && !part.contains('@') && !part.chars().any(js::is_whitespace);
    if !plain(local) || !plain(domain) {
        return false;
    }
    domain.char_indices().any(|(at, c)| c == '.' && at > 0 && at + 1 < domain.len())
}

/// An absolute URL with a scheme and a host.
fn is_url(text: &str) -> bool {
    url::Url::parse(text).is_ok_and(|url| url.has_host())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(json: &str) -> Value {
        Value::from_json_str(json).unwrap()
    }

    #[test]
    fn checks_each_rule() {
        let cases = [
            (r#"{"email": true}"#, "\"a.b@c\"", false),
            (r#"{"email": true}"#, "\"a@b.\"", false),
            (r#"{"email": true}"#, "\"a@.b.c\"", true),
            (r#"{"url": true}"#, "\"mailto:a@b.co\"", false),
            (r#"{"numeric": true}"#, "\"  \"", false),
            (r#"{"min": "5"}"#, "\"7px\"", true),
            (r#"{"max": 5}"#, "\"x\"", true),
            (r#"{"minLength": 2}"#, "\"é😀\"", true),
            (r#"{"pattern": "(["}"#, "\"x\"", true),
            (r#"{"required": true}"#, r#"{"a": false, "b": false}"#, false),
            (r#"{"required": true}"#, r#"{"value": ""}"#, false),
            (r#"{"required": false, "minLength": 3}"#, "\"\"", true),
            (r#"["required", "minLength:3"]"#, "\"ab\"", false),
            (r#"["required", "pattern:^a"]"#, "\"ab\"", true),
            (r#"{"nope": true}"#, "\"ab\"", true),
        ];
        for (rule, value, valid) in cases {
            let result = validate(&Value::from_json_str(value).unwrap(), &rules(rule));
            assert_eq!(result.is_none(), valid, "{rule} {value}: {result:?}");
        }
        assert_eq!(
            validate(&Value::from(""), &rules(r#"{"required": true, "email": true}"#)).as_deref(),
            Some("This field is required")
        );
    }
}
