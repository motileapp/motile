//! Text: numbers as text, joining, plurals, padding, case, splitting, durations and defaults.

use std::sync::LazyLock;

use super::{MAX_ITEMS, Required, list, read, too_many};
use crate::format::NumberFormat;
use crate::js;
use crate::library::{Args, FnError, FunctionDef};
use crate::schema::Schema;
use crate::value::Value;

pub(super) fn functions() -> Vec<FunctionDef> {
    let text = || Schema::String.field("text");
    vec![
        FunctionDef::new(
            "Format",
            "Number as text: \"number\" 12,345.6, \"currency\" $12,345.68 (currency is a code like \"EUR\"), \
             \"percent\" 45 -> 45%, \"compact\" 1.2M ($1.2M with a currency)",
            [
                Schema::Number.field("value"),
                Schema::enumeration(["number", "currency", "percent", "compact"]).field("kind").optional(),
                Schema::Number.field("decimals").optional(),
                Schema::String.field("currency").optional(),
            ],
            format,
        )
        .returns(Schema::String),
        FunctionDef::new(
            "Join",
            "Joins the items (or their field) into text with sep (default \", \"), skipping nulls",
            [list().field("list"), Schema::String.field("sep").optional(), Schema::String.field("field").optional()],
            join,
        )
        .returns(Schema::String),
        FunctionDef::new(
            "Plural",
            "Count and word: @Plural(1, \"guest\") is \"1 guest\", 3 gives \"3 guests\"; plural defaults to word + \"s\"",
            [Schema::Number.field("n"), Schema::String.field("word"), Schema::String.field("plural").optional()],
            plural,
        )
        .returns(Schema::String),
        FunctionDef::new(
            "Pad",
            "Pads the start of value to length with char (default \"0\"), e.g. 7 -> \"07\"",
            [
                Schema::union([Schema::String, Schema::Number]).field("value"),
                Schema::Number.field("length"),
                Schema::String.field("char").optional(),
            ],
            pad,
        )
        .returns(Schema::String),
        FunctionDef::new("Upper", "Text in upper case", [text()], upper).returns(Schema::String),
        FunctionDef::new("Lower", "Text in lower case", [text()], lower).returns(Schema::String),
        FunctionDef::new("Trim", "Text without spaces at the start and end", [text()], trim).returns(Schema::String),
        FunctionDef::new(
            "Capitalize",
            "Each word with a capital first letter, e.g. \"new york\" is \"New York\"",
            [text()],
            capitalize,
        )
        .returns(Schema::String),
        FunctionDef::new(
            "Split",
            "List of the parts of text between sep (default \",\"), each trimmed, empty parts dropped, e.g. \
             @Split(\"a, b,,c\") is [\"a\", \"b\", \"c\"]",
            [text(), Schema::String.field("sep").optional()],
            split,
        )
        .returns(Schema::array(Schema::String)),
        FunctionDef::new(
            "Replace",
            "Text with every find replaced by replacement",
            [text(), Schema::String.field("find"), Schema::String.field("replacement")],
            replace,
        )
        .returns(Schema::String),
        FunctionDef::new(
            "Duration",
            "Seconds as text: \"short\" 1h 20m, 4m 05s, 45s (default); \"clock\" 1:20:00, 4:05",
            [Schema::Number.field("seconds"), Schema::enumeration(["short", "clock"]).field("style").optional()],
            duration,
        )
        .returns(Schema::String),
        FunctionDef::new(
            "Default",
            "value, or fallback when value is null, \"\" or not a number, e.g. an empty number input: @Default($weight, 0)",
            [Schema::Any.field("value").optional(), Schema::Any.field("fallback")],
            default,
        )
        .returns(Schema::Any),
    ]
}

/// A currency code as `Intl` takes it; anything else is USD.
fn currency_code(code: Option<&str>) -> String {
    let upper = code.unwrap_or("USD").to_uppercase();
    if upper.len() == 3 && upper.bytes().all(|b| b.is_ascii_uppercase()) { upper } else { "USD".to_owned() }
}

/// `decimals` caps the digits; only currency pads to it, so 600 never reads "600.0".
fn format(args: &Args<'_>) -> Result<Value, FnError> {
    let value = args.need_number("value")?;
    if !value.is_finite() {
        return Err(FnError::new("value is not a finite number"));
    }
    let decimals = args.number("decimals").map(|decimals| decimals.trunc().clamp(0.0, 10.0));
    if decimals.is_some_and(f64::is_nan) {
        return Err(FnError::new("decimals is not a number"));
    }
    let digits = |max: u32, min: u32| match decimals {
        None => (min, max),
        Some(decimals) => (if min == 0 { 0 } else { decimals as u32 }, decimals as u32),
    };
    let text = match args.str("kind").unwrap_or("number") {
        "currency" => {
            let (min, max) = digits(2, 2);
            NumberFormat::currency(currency_code(args.str("currency")), min, max).format(value)
        }
        "percent" => {
            let (min, max) = digits(1, 0);
            format!("{}%", NumberFormat::decimal(min, max).format(value))
        }
        "compact" => {
            let (min, max) = digits(1, 0);
            let format = match args.str("currency").filter(|code| !code.is_empty()) {
                Some(code) => NumberFormat::currency(currency_code(Some(code)), min, max),
                None => NumberFormat::decimal(min, max),
            };
            format.compact().format(value)
        }
        _ => {
            let (min, max) = digits(2, 0);
            NumberFormat::decimal(min, max).format(value)
        }
    };
    Ok(Value::String(text))
}

fn join(args: &Args<'_>) -> Result<Value, FnError> {
    let list = args.need_list("list")?;
    let sep = args.str("sep").unwrap_or(", ");
    let field = args.str("field").unwrap_or_default();
    let parts: Vec<String> = list
        .iter()
        .map(|item| read(item, field))
        .filter(|value| !value.is_null())
        .map(|value| match value {
            Value::Array(_) | Value::Object(_) | Value::Element(_) | Value::Plan(_) => value.to_json_string(),
            value => js::to_string(&value),
        })
        .collect();
    Ok(Value::String(parts.join(sep)))
}

fn plural(args: &Args<'_>) -> Result<Value, FnError> {
    let n = args.need_number("n")?;
    let word = args.need_str("word")?;
    let count = NumberFormat::decimal(0, 2).format(n);
    if n.abs() == 1.0 {
        return Ok(Value::String(format!("{count} {word}")));
    }
    let plural = args.str("plural").map_or_else(|| format!("{word}s"), str::to_owned);
    Ok(Value::String(format!("{count} {plural}")))
}

fn pad(args: &Args<'_>) -> Result<Value, FnError> {
    let value = match args.get("value") {
        value @ (Value::String(_) | Value::Number(_)) => js::to_string(value),
        _ => return Err(super::expects("value", "string or number")),
    };
    let length = args.need_number("length")?.trunc().min(1000.0);
    let fill = args.str("char").filter(|fill| !fill.is_empty()).unwrap_or("0");
    let units: Vec<u16> = value.encode_utf16().collect();
    let target = if length > 0.0 { length as usize } else { 0 };
    if target <= units.len() {
        return Ok(Value::String(value));
    }
    let padding: Vec<u16> = fill.encode_utf16().cycle().take(target - units.len()).collect();
    Ok(Value::String(String::from_utf16_lossy(&[padding, units].concat())))
}

fn upper(args: &Args<'_>) -> Result<Value, FnError> {
    Ok(Value::String(args.need_str("text")?.to_uppercase()))
}

fn lower(args: &Args<'_>) -> Result<Value, FnError> {
    Ok(Value::String(args.need_str("text")?.to_lowercase()))
}

fn trim(args: &Args<'_>) -> Result<Value, FnError> {
    Ok(Value::from(js::trim(args.need_str("text")?)))
}

static WORD_START: LazyLock<regress::Regex> =
    LazyLock::new(|| regress::Regex::with_flags(r#"(^|[\s\-(/"])(\p{L})"#, "u").expect("a valid pattern"));

fn capitalize(args: &Args<'_>) -> Result<Value, FnError> {
    let text = args.need_str("text")?;
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for found in WORD_START.find_iter(text) {
        let Some(letter) = found.group(2) else { continue };
        out.push_str(&text[copied..letter.start]);
        out.extend(text[letter.clone()].chars().flat_map(char::to_uppercase));
        copied = letter.end;
    }
    out.push_str(&text[copied..]);
    Ok(Value::String(out))
}

fn split(args: &Args<'_>) -> Result<Value, FnError> {
    let text = args.need_str("text")?;
    let sep = args.str("sep").unwrap_or(",");
    let pieces: Vec<String> = if sep.is_empty() {
        text.encode_utf16().map(|unit| String::from_utf16_lossy(&[unit])).collect()
    } else {
        text.split(sep).map(str::to_owned).collect()
    };
    let parts: Vec<Value> =
        pieces.iter().map(|piece| js::trim(piece)).filter(|part| !part.is_empty()).map(Value::from).collect();
    if parts.len() > MAX_ITEMS {
        return Err(too_many());
    }
    Ok(Value::Array(parts))
}

fn replace(args: &Args<'_>) -> Result<Value, FnError> {
    let text = args.need_str("text")?;
    let find = args.need_str("find")?;
    let replacement = args.need_str("replacement")?;
    if find.is_empty() {
        return Ok(Value::from(text));
    }
    Ok(Value::String(text.replace(find, replacement)))
}

fn duration(args: &Args<'_>) -> Result<Value, FnError> {
    let seconds = args.need_number("seconds")?;
    if !seconds.is_finite() {
        return Err(FnError::new("seconds is not a finite number"));
    }
    let sign = if seconds < 0.0 { "-" } else { "" };
    let total = js::round(seconds.abs());
    let (hours, minutes, rest) = ((total / 3600.0).floor(), (total % 3600.0 / 60.0).floor(), total % 60.0);
    let n = js::number_to_string;
    let two = |value: f64| format!("{:0>2}", n(value));
    let text = if args.str("style") == Some("clock") {
        if hours != 0.0 {
            format!("{sign}{}:{}:{}", n(hours), two(minutes), two(rest))
        } else {
            format!("{sign}{}:{}", n(minutes), two(rest))
        }
    } else if hours != 0.0 {
        let minutes = if minutes != 0.0 { format!(" {}m", n(minutes)) } else { String::new() };
        format!("{sign}{}h{minutes}", n(hours))
    } else if minutes != 0.0 {
        let rest = if rest != 0.0 { format!(" {}s", two(rest)) } else { String::new() };
        format!("{sign}{}m{rest}", n(minutes))
    } else {
        format!("{sign}{}s", n(rest))
    };
    Ok(Value::String(text))
}

fn default(args: &Args<'_>) -> Result<Value, FnError> {
    let fallback = args.need("fallback")?;
    let value = args.get("value");
    let empty = match value {
        Value::Null => true,
        Value::String(text) => text.is_empty(),
        Value::Number(number) => number.is_nan(),
        _ => false,
    };
    Ok(if empty { fallback } else { value }.clone())
}
