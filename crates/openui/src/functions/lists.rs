//! Lists held in `$variables`: copies with items added, removed, updated or toggled, lookups,
//! slices, ranges and random picks.

use std::collections::HashSet;
use std::ops::Range;

use super::{MAX_ITEMS, Required, js_order, list, read, same, too_many};
use crate::js;
use crate::library::{Args, FnError, FunctionDef};
use crate::schema::Schema;
use crate::value::Value;

pub(super) fn functions() -> Vec<FunctionDef> {
    let list_or_text = || Schema::union([Schema::array(Schema::Any), Schema::String]);
    vec![
        FunctionDef::new(
            "Append",
            "Copy of the list with item added at the end",
            [list().field("list"), Schema::Any.field("item")],
            append,
        )
        .returns(list()),
        FunctionDef::new(
            "Prepend",
            "Copy of the list with item added at the start",
            [list().field("list"), Schema::Any.field("item")],
            prepend,
        )
        .returns(list()),
        FunctionDef::new("Concat", "Joins two lists", [list().field("list"), list().field("more")], concat)
            .returns(list()),
        FunctionDef::new(
            "Remove",
            "Copy without the items whose field equals value; field \"\" compares the items themselves",
            [list().field("list"), Schema::String.field("field"), Schema::Any.field("value")],
            remove,
        )
        .returns(list()),
        FunctionDef::new(
            "RemoveAt",
            "Copy without the item at index (0-based; negative counts from the end)",
            [list().field("list"), Schema::Number.field("index")],
            remove_at,
        )
        .returns(list()),
        FunctionDef::new(
            "Update",
            "Copy where items whose field equals value get setField set to setValue",
            [
                list().field("list"),
                Schema::String.field("field"),
                Schema::Any.field("value"),
                Schema::String.field("setField"),
                Schema::Any.field("setValue"),
            ],
            update,
        )
        .returns(list()),
        FunctionDef::new(
            "Toggle",
            "Adds item if missing, removes it if present (a set of strings or numbers)",
            [list().field("list"), Schema::Any.field("item")],
            toggle,
        )
        .returns(list()),
        FunctionDef::new(
            "Includes",
            "True if the list has item, or the text contains it",
            [list_or_text().field("list"), Schema::Any.field("item")],
            includes,
        )
        .returns(Schema::Boolean),
        FunctionDef::new(
            "Find",
            "First item whose field equals value, or null",
            [list().field("list"), Schema::String.field("field"), Schema::Any.field("value")],
            find,
        )
        .returns(Schema::Any),
        FunctionDef::new(
            "IndexOf",
            "Index of item in the list, or -1",
            [list().field("list"), Schema::Any.field("item")],
            index_of,
        )
        .returns(Schema::Number),
        FunctionDef::new(
            "Take",
            "The first n items; a negative n takes the last n",
            [list().field("list"), Schema::Number.field("n")],
            take,
        )
        .returns(list()),
        FunctionDef::new(
            "Slice",
            "Items, or characters of text, from start up to end (not included); negative counts from the end",
            [list_or_text().field("list"), Schema::Number.field("start"), Schema::Number.field("end").optional()],
            slice,
        )
        .returns(list_or_text()),
        FunctionDef::new("Unique", "Distinct items in first-seen order", [list().field("list")], unique)
            .returns(list()),
        FunctionDef::new("Reverse", "Copy in reverse order", [list().field("list")], reverse).returns(list()),
        FunctionDef::new(
            "Range",
            "Numbers from start up to end (not included) by step (default 1), e.g. @Range(0, 3) is [0, 1, 2]",
            [Schema::Number.field("start"), Schema::Number.field("end"), Schema::Number.field("step").optional()],
            range,
        )
        .returns(Schema::array(Schema::Number)),
        FunctionDef::new(
            "Pick",
            "A random item, or null for an empty list; draws anew each time, so call it inside @Set",
            [list().field("list")],
            pick,
        )
        .returns(Schema::Any),
        FunctionDef::new("Shuffle", "Copy in random order; call it inside @Set", [list().field("list")], shuffle)
            .returns(list()),
        FunctionDef::new(
            "Flatten",
            "One list from a list of lists, e.g. [[1, 2], [3]] is [1, 2, 3]",
            [list().field("list")],
            flatten,
        )
        .returns(list()),
        FunctionDef::new(
            "Chunk",
            "The list cut into lists of size items, e.g. for weeks of 7 days or pages",
            [list().field("list"), Schema::Number.field("size")],
            chunk,
        )
        .returns(Schema::array(list())),
    ]
}

/// Where `slice` puts an index: a negative one counts from the end, NaN is 0.
fn bound(index: f64, len: usize) -> usize {
    let index = if index.is_nan() { 0.0 } else { index.trunc() };
    let at = if index < 0.0 { (len as f64 + index).max(0.0) } else { index.min(len as f64) };
    at as usize
}

/// `slice(start, end)`'s range.
fn slice_range(len: usize, start: f64, end: Option<f64>) -> Range<usize> {
    let start = bound(start, len);
    let end = end.map_or(len, |end| bound(end, len));
    start..end.max(start)
}

fn append(args: &Args<'_>) -> Result<Value, FnError> {
    let list = args.need_list("list")?;
    let item = args.need("item")?;
    Ok(list.iter().chain([item]).cloned().collect())
}

fn prepend(args: &Args<'_>) -> Result<Value, FnError> {
    let list = args.need_list("list")?;
    let item = args.need("item")?;
    Ok([item].into_iter().chain(list).cloned().collect())
}

fn concat(args: &Args<'_>) -> Result<Value, FnError> {
    let list = args.need_list("list")?;
    let more = args.need_list("more")?;
    if list.len() + more.len() > MAX_ITEMS {
        return Err(too_many());
    }
    Ok(list.iter().chain(more).cloned().collect())
}

fn remove(args: &Args<'_>) -> Result<Value, FnError> {
    let list = args.need_list("list")?;
    let field = args.need_str("field")?;
    let value = args.need("value")?;
    Ok(list.iter().filter(|item| !same(&read(item, field), value)).cloned().collect())
}

fn remove_at(args: &Args<'_>) -> Result<Value, FnError> {
    let list = args.need_list("list")?;
    let at = args.need_number("index")?.trunc();
    let index = if at < 0.0 { list.len() as f64 + at } else { at };
    Ok(list
        .iter()
        .enumerate()
        .filter(|(position, _)| *position as f64 != index)
        .map(|(_, item)| item.clone())
        .collect())
}

fn update(args: &Args<'_>) -> Result<Value, FnError> {
    let list = args.need_list("list")?;
    let field = args.need_str("field")?;
    let value = args.need("value")?;
    let set_field = args.need_str("setField")?;
    let set_value = args.need("setValue")?;
    let items = list.iter().map(|item| {
        let Value::Object(map) = item else { return item.clone() };
        if !same(&read(item, field), value) {
            return item.clone();
        }
        let mut map = map.clone();
        map.insert(set_field.to_owned(), set_value.clone());
        Value::Object(js_order(map))
    });
    Ok(items.collect())
}

fn toggle(args: &Args<'_>) -> Result<Value, FnError> {
    let list = args.need_list("list")?;
    let item = args.need("item")?;
    if list.iter().any(|existing| same(existing, item)) {
        return Ok(list.iter().filter(|existing| !same(existing, item)).cloned().collect());
    }
    Ok(list.iter().chain([item]).cloned().collect())
}

fn includes(args: &Args<'_>) -> Result<Value, FnError> {
    let item = args.need("item")?;
    match args.get("list") {
        Value::String(text) => Ok(Value::Bool(text.contains(&js::to_string(item)))),
        Value::Array(list) => Ok(Value::Bool(list.iter().any(|existing| same(existing, item)))),
        _ => Err(super::expects("list", "array or string")),
    }
}

fn find(args: &Args<'_>) -> Result<Value, FnError> {
    let list = args.need_list("list")?;
    let field = args.need_str("field")?;
    let value = args.need("value")?;
    Ok(list.iter().find(|item| same(&read(item, field), value)).cloned().unwrap_or_default())
}

fn index_of(args: &Args<'_>) -> Result<Value, FnError> {
    let list = args.need_list("list")?;
    let item = args.need("item")?;
    let index = list.iter().position(|existing| same(existing, item));
    Ok(Value::Number(index.map_or(-1.0, |index| index as f64)))
}

fn take(args: &Args<'_>) -> Result<Value, FnError> {
    let list = args.need_list("list")?;
    let n = args.need_number("n")?.trunc();
    let range = if n >= 0.0 { slice_range(list.len(), 0.0, Some(n)) } else { slice_range(list.len(), n, None) };
    Ok(Value::Array(list[range].to_vec()))
}

fn slice(args: &Args<'_>) -> Result<Value, FnError> {
    let start = args.need_number("start")?;
    let end = args.number("end");
    match args.get("list") {
        Value::String(text) => {
            let units: Vec<u16> = text.encode_utf16().collect();
            let range = slice_range(units.len(), start, end);
            Ok(Value::String(String::from_utf16_lossy(&units[range])))
        }
        Value::Array(list) => Ok(Value::Array(list[slice_range(list.len(), start, end)].to_vec())),
        _ => Err(super::expects("list", "array or string")),
    }
}

fn unique(args: &Args<'_>) -> Result<Value, FnError> {
    let list = args.need_list("list")?;
    let mut seen = HashSet::new();
    let key = |item: &Value| match item {
        Value::Array(_) | Value::Object(_) | Value::Element(_) | Value::Plan(_) => {
            format!("o:{}", item.to_json_string())
        }
        item => format!("p:{}", js::to_string(item)),
    };
    Ok(list.iter().filter(|item| seen.insert(key(item))).cloned().collect())
}

fn reverse(args: &Args<'_>) -> Result<Value, FnError> {
    Ok(args.need_list("list")?.iter().rev().cloned().collect())
}

fn range(args: &Args<'_>) -> Result<Value, FnError> {
    let start = args.need_number("start")?;
    let end = args.need_number("end")?;
    let step = args.number("step").unwrap_or(1.0);
    if step == 0.0 || !step.is_finite() {
        return Err(FnError::new("step must be a non-zero number"));
    }
    let count = ((end - start) / step).ceil();
    if count > MAX_ITEMS as f64 {
        return Err(too_many());
    }
    let count = if count > 0.0 { count as usize } else { 0 };
    let whole = start.fract() == 0.0 && step.fract() == 0.0;
    let at = |index: usize| {
        let value = start + index as f64 * step;
        if whole { value } else { to_precision_12(value) }
    };
    Ok((0..count).map(|index| Value::Number(at(index))).collect())
}

/// `Number(value.toPrecision(12))`.
fn to_precision_12(value: f64) -> f64 {
    format!("{value:.11e}").parse().unwrap_or(value)
}

fn pick(args: &Args<'_>) -> Result<Value, FnError> {
    let list = args.need_list("list")?;
    if list.is_empty() {
        return Ok(Value::Null);
    }
    let index = (args.env().random() * list.len() as f64).floor() as usize;
    Ok(list[index.min(list.len() - 1)].clone())
}

fn shuffle(args: &Args<'_>) -> Result<Value, FnError> {
    let mut out = args.need_list("list")?.to_vec();
    for index in (1..out.len()).rev() {
        let other = (args.env().random() * (index + 1) as f64).floor() as usize;
        out.swap(index, other.min(index));
    }
    Ok(Value::Array(out))
}

fn flatten(args: &Args<'_>) -> Result<Value, FnError> {
    let list = args.need_list("list")?;
    let mut out = Vec::new();
    for item in list {
        match item {
            Value::Array(items) => out.extend(items.iter().cloned()),
            item => out.push(item.clone()),
        }
    }
    if out.len() > MAX_ITEMS {
        return Err(too_many());
    }
    Ok(Value::Array(out))
}

fn chunk(args: &Args<'_>) -> Result<Value, FnError> {
    let list = args.need_list("list")?;
    let size = args.need_number("size")?.trunc();
    if size.is_nan() || size < 1.0 {
        return Err(FnError::new("size must be 1 or more"));
    }
    let size = if size >= list.len() as f64 { list.len().max(1) } else { size as usize };
    Ok(list.chunks(size).map(|chunk| Value::Array(chunk.to_vec())).collect())
}
