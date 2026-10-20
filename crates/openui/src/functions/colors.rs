//! Colors: a hex color part of the way to another.

use super::Required;
use crate::js;
use crate::library::{Args, FnError, FunctionDef};
use crate::schema::Schema;
use crate::value::Value;

pub(super) fn functions() -> Vec<FunctionDef> {
    vec![
        FunctionDef::new(
            "MixColor",
            "The hex color a fraction t of the way from one hex color to another, e.g. @MixColor(\"#ffffff\", \
             \"#0285ff\", 0.5) for a heatmap or a tint",
            [Schema::String.field("from"), Schema::String.field("to"), Schema::Number.field("t")],
            mix_color,
        )
        .returns(Schema::String),
    ]
}

/// The red, green and blue of `#rgb`, `#rrggbb` or the same without `#`.
fn channels(color: &str) -> Result<[f64; 3], FnError> {
    let trimmed = js::trim(color);
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    let valid = matches!(hex.len(), 3 | 6) && hex.bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        return Err(FnError::new(format!("not a hex color: {color}")));
    }
    let hex: String = if hex.len() == 3 { hex.chars().flat_map(|c| [c, c]).collect() } else { hex.to_owned() };
    let channel = |index: usize| u8::from_str_radix(&hex[index..index + 2], 16).map_or(0.0, f64::from);
    Ok([channel(0), channel(2), channel(4)])
}

fn mix_color(args: &Args<'_>) -> Result<Value, FnError> {
    let from = channels(args.need_str("from")?)?;
    let to = channels(args.need_str("to")?)?;
    let t = args.need_number("t")?;
    let k = if t.is_nan() { f64::NAN } else { t.clamp(0.0, 1.0) };
    let hex: String =
        (0..3).map(|index| format!("{:02x}", js::round(from[index] + (to[index] - from[index]) * k) as i64)).collect();
    Ok(Value::String(format!("#{hex}")))
}
