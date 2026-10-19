//! The values a program computes with. Objects keep their keys in order, since key order is
//! argument order; deserialize JSON straight into a [`Value`] to keep it, as `serde_json::Value`
//! sorts its keys.

use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};

use crate::element::{ActionPlan, Element};
use crate::js;

pub type Map = IndexMap<String, Value>;

/// Integers up to here serialize without a fraction, as JavaScript writes them.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(Map),
    Element(Arc<Element>),
    Plan(Arc<ActionPlan>),
}

impl Value {
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn as_bool(&self) -> Option<bool> {
        let Value::Bool(value) = self else { return None };
        Some(*value)
    }

    pub fn as_number(&self) -> Option<f64> {
        let Value::Number(value) = self else { return None };
        Some(*value)
    }

    pub fn as_str(&self) -> Option<&str> {
        let Value::String(value) = self else { return None };
        Some(value)
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        let Value::Array(items) = self else { return None };
        Some(items)
    }

    pub fn as_object(&self) -> Option<&Map> {
        let Value::Object(map) = self else { return None };
        Some(map)
    }

    pub fn as_element(&self) -> Option<&Element> {
        let Value::Element(element) = self else { return None };
        Some(element)
    }

    pub fn as_plan(&self) -> Option<&ActionPlan> {
        let Value::Plan(plan) = self else { return None };
        Some(plan)
    }

    /// An object's field.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object()?.get(key)
    }

    /// The name error messages use: `typeof`, with arrays as "array" and null as "null".
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) | Value::Element(_) | Value::Plan(_) => "object",
        }
    }

    pub fn from_json_str(text: &str) -> Result<Value, serde_json::Error> {
        serde_json::from_str(text)
    }

    pub fn to_json(&self) -> serde_json::Value {
        self.into()
    }

    /// `JSON.stringify(value)`: compact, numbers as JavaScript writes them.
    pub fn to_json_string(&self) -> String {
        let mut out = String::new();
        write_json(self, &mut out);
        out
    }
}

fn write_json(value: &Value, out: &mut String) {
    match value {
        Value::Null | Value::Element(_) | Value::Plan(_) => out.push_str("null"),
        Value::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) if value.is_finite() => out.push_str(&js::number_to_string(*value)),
        Value::Number(_) => out.push_str("null"),
        Value::String(text) => write_json_string(text, out),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_json(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (index, (key, item)) in map.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_json_string(key, out);
                out.push(':');
                write_json(item, out);
            }
            out.push('}');
        }
    }
}

fn write_json_string(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

impl fmt::Display for Value {
    /// `String(value)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&js::to_string(self))
    }
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Value::Null | Value::Element(_) | Value::Plan(_) => serializer.serialize_unit(),
            Value::Bool(value) => serializer.serialize_bool(*value),
            Value::Number(value) if value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER => {
                serializer.serialize_i64(*value as i64)
            }
            Value::Number(value) if value.is_finite() => serializer.serialize_f64(*value),
            Value::Number(_) => serializer.serialize_unit(),
            Value::String(text) => serializer.serialize_str(text),
            Value::Array(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            Value::Object(map) => {
                let mut out = serializer.serialize_map(Some(map.len()))?;
                for (key, item) in map {
                    out.serialize_entry(key, item)?;
                }
                out.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(ValueVisitor)
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value as f64))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value as f64))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Value, E> {
        Ok(Value::Number(value))
    }

    fn visit_str<E>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        Value::deserialize(deserializer)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
        let mut map = Map::with_capacity(access.size_hint().unwrap_or(0));
        while let Some((key, item)) = access.next_entry()? {
            map.insert(key, item);
        }
        Ok(Value::Object(map))
    }
}

impl From<&serde_json::Value> for Value {
    fn from(value: &serde_json::Value) -> Value {
        match value {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(value) => Value::Bool(*value),
            serde_json::Value::Number(number) => Value::Number(number.as_f64().unwrap_or(0.0)),
            serde_json::Value::String(text) => Value::String(text.clone()),
            serde_json::Value::Array(items) => Value::Array(items.iter().map(Value::from).collect()),
            serde_json::Value::Object(map) => {
                Value::Object(map.iter().map(|(key, item)| (key.clone(), Value::from(item))).collect())
            }
        }
    }
}

impl From<serde_json::Value> for Value {
    fn from(value: serde_json::Value) -> Value {
        Value::from(&value)
    }
}

impl From<&Value> for serde_json::Value {
    fn from(value: &Value) -> serde_json::Value {
        match value {
            Value::Null | Value::Element(_) | Value::Plan(_) => serde_json::Value::Null,
            Value::Bool(value) => serde_json::Value::Bool(*value),
            Value::Number(value) if value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER => {
                serde_json::Value::from(*value as i64)
            }
            Value::Number(value) => {
                serde_json::Number::from_f64(*value).map_or(serde_json::Value::Null, serde_json::Value::Number)
            }
            Value::String(text) => serde_json::Value::String(text.clone()),
            Value::Array(items) => serde_json::Value::Array(items.iter().map(serde_json::Value::from).collect()),
            Value::Object(map) => serde_json::Value::Object(
                map.iter().map(|(key, item)| (key.clone(), serde_json::Value::from(item))).collect(),
            ),
        }
    }
}

impl From<Value> for serde_json::Value {
    fn from(value: Value) -> serde_json::Value {
        serde_json::Value::from(&value)
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Value {
        Value::Bool(value)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Value {
        Value::Number(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Value {
        Value::Number(value.into())
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Value {
        Value::Number(value as f64)
    }
}

impl From<usize> for Value {
    fn from(value: usize) -> Value {
        Value::Number(value as f64)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Value {
        Value::String(value.to_owned())
    }
}

impl From<String> for Value {
    fn from(value: String) -> Value {
        Value::String(value)
    }
}

impl From<Vec<Value>> for Value {
    fn from(items: Vec<Value>) -> Value {
        Value::Array(items)
    }
}

impl From<Map> for Value {
    fn from(map: Map) -> Value {
        Value::Object(map)
    }
}

impl From<Element> for Value {
    fn from(element: Element) -> Value {
        Value::Element(Arc::new(element))
    }
}

impl From<ActionPlan> for Value {
    fn from(plan: ActionPlan) -> Value {
        Value::Plan(Arc::new(plan))
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Value {
        value.map_or(Value::Null, Into::into)
    }
}

impl FromIterator<Value> for Value {
    fn from_iter<I: IntoIterator<Item = Value>>(items: I) -> Value {
        Value::Array(items.into_iter().collect())
    }
}

impl<K: Into<String>> FromIterator<(K, Value)> for Value {
    fn from_iter<I: IntoIterator<Item = (K, Value)>>(entries: I) -> Value {
        Value::Object(entries.into_iter().map(|(key, value)| (key.into(), value)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_key_order_from_json() {
        let value = Value::from_json_str(r#"{"z": 1, "a": [true, null, "x"], "m": {"b": 2.5, "a": -0}}"#).unwrap();
        let keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["z", "a", "m"]);
        assert_eq!(serde_json::to_string(&value).unwrap(), r#"{"z":1,"a":[true,null,"x"],"m":{"b":2.5,"a":0}}"#);
        assert_eq!(value.to_json_string(), r#"{"z":1,"a":[true,null,"x"],"m":{"b":2.5,"a":0}}"#);
    }

    #[test]
    fn stringifies_like_javascript() {
        let value: Value = [
            Value::from(1e21),
            Value::from(0.1),
            Value::from(f64::NAN),
            Value::from("a\"\\\n\u{1}é"),
            Value::from(Element::new("Text", Map::new())),
        ]
        .into_iter()
        .collect();
        assert_eq!(value.to_json_string(), r#"[1e+21,0.1,null,"a\"\\\n\u0001é",null]"#);
    }

    #[test]
    fn converts_to_and_from_serde_json() {
        let json = serde_json::json!({"n": 3, "f": 1.5, "list": [null, false, "s"]});
        let value = Value::from(&json);
        assert_eq!(value.get("n"), Some(&Value::Number(3.0)));
        assert_eq!(serde_json::Value::from(&value), json);
    }
}
