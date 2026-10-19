//! Checking a value against a prop's or a param's schema (language.md §8.2, §8.4), as the
//! reference checks its JSON Schema: a mistake removes the smallest unit. An invalid optional
//! prop or array item is left out, one with a default takes it, and an invalid required one makes
//! its parent invalid.

use crate::error::Code;
use crate::node::Node;
use crate::schema::{Field, Schema};
use crate::value::{Map, Value};

/// What a check found.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Keep,
    Replace(Value),
    Invalid,
}

/// What stands in for an invalid value.
pub enum Fallback {
    Default(Value),
    Omit,
    Drop,
}

pub fn fallback(field: &Field) -> Fallback {
    if let Some(default) = &field.default {
        return Fallback::Default(default.clone());
    }
    if field.required() { Fallback::Drop } else { Fallback::Omit }
}

/// A problem, at a JSON pointer below the checked value.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub code: Code,
    pub path: String,
    pub message: String,
}

/// Checks values; `partial` defers the checks that more text could still satisfy: enums and the
/// required keys of objects.
#[derive(Debug, Default)]
pub struct Checker {
    pub partial: bool,
    pub issues: Vec<Issue>,
}

impl Checker {
    pub fn new(partial: bool) -> Checker {
        Checker { partial, issues: Vec::new() }
    }

    fn mismatch(&mut self, path: &str, expected: &str, actual: &str) {
        self.issues.push(Issue {
            code: Code::TypeMismatch,
            path: path.to_owned(),
            message: format!("field \"{path}\" expects {expected} but got {actual}"),
        });
    }

    fn missing(&mut self, path: String, present: bool) {
        let (code, message) = if present {
            (Code::NullRequired, format!("required field \"{path}\" cannot be null"))
        } else {
            (Code::MissingRequired, format!("missing required field \"{path}\""))
        };
        self.issues.push(Issue { code, path, message });
    }

    /// A component where a slot takes data: only a slot of plain types refuses it.
    fn element_position(&mut self, schema: &Schema, component: &str, path: &str) -> bool {
        if schema.is_composite() {
            return false;
        }
        if schema.json_type().is_none() && schema.allowed_values().is_none() {
            return false;
        }
        let expected = schema.json_type().unwrap_or("a literal value");
        self.mismatch(path, expected, &format!("component \"{component}\""));
        true
    }

    /// Data in a slot that takes only components.
    fn component_slot(&mut self, schema: &Schema, actual: &str, path: &str) -> bool {
        let Some(slot) = schema.component_slot() else { return false };
        self.mismatch(path, &slot.join(" | "), actual);
        true
    }

    pub fn value(&mut self, value: &Value, schema: &Schema, path: &str) -> Outcome {
        match value {
            Value::Null => return Outcome::Keep,
            Value::Element(element) => {
                let invalid = self.element_position(schema, &element.component, path);
                return if invalid { Outcome::Invalid } else { Outcome::Keep };
            }
            _ => {}
        }
        if schema.is_composite() {
            let invalid = self.component_slot(schema, slot_type(value), path);
            return if invalid { Outcome::Invalid } else { Outcome::Keep };
        }
        match schema.json_type() {
            Some("object") => self.object(value, schema, path),
            Some("array") => self.array(value, schema, path),
            _ => self.leaf(value, schema, path),
        }
    }

    fn leaf(&mut self, value: &Value, schema: &Schema, path: &str) -> Outcome {
        if let Some(allowed) = schema.allowed_values() {
            let scalar = matches!(value, Value::Bool(_) | Value::Number(_) | Value::String(_));
            if self.partial || !scalar || allowed.iter().any(|allowed| crate::js::same_value_zero(allowed, value)) {
                return Outcome::Keep;
            }
            let expected = allowed.iter().map(Value::to_json_string).collect::<Vec<_>>().join(", ");
            self.mismatch(path, &format!("one of [{expected}]"), &value.to_json_string());
            return Outcome::Invalid;
        }
        let Some(expected @ ("string" | "number" | "boolean")) = schema.json_type() else { return Outcome::Keep };
        if type_of(value) == expected {
            return Outcome::Keep;
        }
        self.mismatch(path, expected, value.type_name());
        Outcome::Invalid
    }

    fn object(&mut self, value: &Value, schema: &Schema, path: &str) -> Outcome {
        let map = match value {
            Value::Object(map) => map,
            Value::Plan(_) => return Outcome::Keep,
            value => {
                self.mismatch(path, "object", value.type_name());
                return Outcome::Invalid;
            }
        };
        let fields = schema.fields().unwrap_or_default();
        let mut out: Option<Map> = None;
        let mut invalid = false;
        if !self.partial {
            for field in fields.iter().filter(|field| field.required()) {
                let present = map.get(&field.name);
                if present.is_some_and(|value| !value.is_null()) {
                    continue;
                }
                if let Some(default) = &field.default {
                    out.get_or_insert_with(|| map.clone()).insert(field.name.clone(), default.clone());
                    continue;
                }
                self.missing(format!("{path}/{}", field.name), present.is_some());
                invalid = true;
            }
        }
        for field in fields {
            let Some(item) = map.get(&field.name) else { continue };
            match self.value(item, &field.schema, &format!("{path}/{}", field.name)) {
                Outcome::Keep => {}
                Outcome::Replace(next) => {
                    out.get_or_insert_with(|| map.clone()).insert(field.name.clone(), next);
                }
                Outcome::Invalid => match fallback(field) {
                    Fallback::Default(default) => {
                        out.get_or_insert_with(|| map.clone()).insert(field.name.clone(), default);
                    }
                    Fallback::Omit => {
                        out.get_or_insert_with(|| map.clone()).shift_remove(&field.name);
                    }
                    Fallback::Drop => invalid = true,
                },
            }
        }
        if invalid {
            return Outcome::Invalid;
        }
        out.map_or(Outcome::Keep, |out| Outcome::Replace(Value::Object(out)))
    }

    fn array(&mut self, value: &Value, schema: &Schema, path: &str) -> Outcome {
        let Value::Array(items) = value else {
            self.mismatch(path, "array", value.type_name());
            return Outcome::Invalid;
        };
        let Some(item_schema) = schema.items() else { return Outcome::Keep };
        let mut out: Option<Vec<Value>> = None;
        for (index, item) in items.iter().enumerate() {
            match self.value(item, item_schema, &format!("{path}/{index}")) {
                Outcome::Keep => {
                    if let Some(out) = &mut out {
                        out.push(item.clone());
                    }
                }
                Outcome::Replace(next) => out.get_or_insert_with(|| items[..index].to_vec()).push(next),
                Outcome::Invalid => {
                    out.get_or_insert_with(|| items[..index].to_vec());
                }
            }
        }
        out.map_or(Outcome::Keep, |out| Outcome::Replace(Value::Array(out)))
    }

    /// Checks a lowered node: constants as values, arrays and objects through their items, and
    /// what is computed later not at all. None when it is invalid.
    pub fn node(&mut self, node: Node, schema: &Schema, path: &str) -> Option<Node> {
        match node {
            Node::Lit(value) => match self.value(&value, schema, path) {
                Outcome::Keep => Some(Node::Lit(value)),
                Outcome::Replace(next) => Some(Node::Lit(next)),
                Outcome::Invalid => None,
            },
            Node::Element(element) => {
                (!self.element_position(schema, &element.component, path)).then_some(Node::Element(element))
            }
            Node::Array(items) => self.array_node(items, schema, path),
            Node::Object(entries) => self.object_node(entries, schema, path),
            node => Some(node),
        }
    }

    fn array_node(&mut self, items: Vec<Node>, schema: &Schema, path: &str) -> Option<Node> {
        if schema.is_composite() {
            return (!self.component_slot(schema, "array", path)).then_some(Node::Array(items));
        }
        match schema.json_type() {
            Some("array") => {
                let Some(item_schema) = schema.items() else { return Some(Node::Array(items)) };
                let items = items
                    .into_iter()
                    .enumerate()
                    .filter_map(|(index, item)| self.node(item, item_schema, &format!("{path}/{index}")))
                    .collect();
                Some(Node::Array(items))
            }
            Some(expected @ ("object" | "string" | "number" | "boolean")) if schema.allowed_values().is_none() => {
                self.mismatch(path, expected, "array");
                None
            }
            _ => Some(Node::Array(items)),
        }
    }

    fn object_node(&mut self, mut entries: Vec<(String, Node)>, schema: &Schema, path: &str) -> Option<Node> {
        if schema.is_composite() {
            return (!self.component_slot(schema, "plain object", path)).then_some(Node::Object(entries));
        }
        match schema.json_type() {
            Some("object") => {}
            Some(expected @ ("array" | "string" | "number" | "boolean")) if schema.allowed_values().is_none() => {
                self.mismatch(path, expected, "object");
                return None;
            }
            _ => return Some(Node::Object(entries)),
        }
        let fields = schema.fields().unwrap_or_default();
        let mut invalid = false;
        if !self.partial {
            for field in fields.iter().filter(|field| field.required()) {
                let position = entries.iter().position(|(key, _)| *key == field.name);
                if position.is_some_and(|index| entries[index].1 != Node::null()) {
                    continue;
                }
                if let Some(default) = &field.default {
                    match position {
                        Some(index) => entries[index].1 = Node::Lit(default.clone()),
                        None => entries.push((field.name.clone(), Node::Lit(default.clone()))),
                    }
                    continue;
                }
                self.missing(format!("{path}/{}", field.name), position.is_some());
                invalid = true;
            }
        }
        let mut out = Vec::with_capacity(entries.len());
        for (key, item) in entries {
            let Some(field) = fields.iter().find(|field| field.name == key) else {
                out.push((key, item));
                continue;
            };
            match self.node(item, &field.schema, &format!("{path}/{key}")) {
                Some(item) => out.push((key, item)),
                None => match fallback(field) {
                    Fallback::Default(default) => out.push((key, Node::Lit(default))),
                    Fallback::Omit => {}
                    Fallback::Drop => invalid = true,
                },
            }
        }
        (!invalid).then_some(Node::Object(out))
    }
}

/// `typeof`: arrays, objects, components and plans are all "object".
fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        _ => "object",
    }
}

fn slot_type(value: &Value) -> &'static str {
    match value {
        Value::Array(_) => "array",
        Value::Object(_) | Value::Plan(_) => "plain object",
        value => type_of(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(value: &str, schema: &Schema) -> (Outcome, Vec<Code>) {
        let mut checker = Checker::new(false);
        let outcome = checker.value(&Value::from_json_str(value).unwrap(), schema, "/p");
        (outcome, checker.issues.into_iter().map(|issue| issue.code).collect())
    }

    #[test]
    fn checks_leaves_objects_and_arrays() {
        assert_eq!(check("5", &Schema::String), (Outcome::Invalid, vec![Code::TypeMismatch]));
        assert_eq!(check("[1]", &Schema::String), (Outcome::Invalid, vec![Code::TypeMismatch]));
        assert_eq!(check("\"x\"", &Schema::enumeration(["a"])), (Outcome::Invalid, vec![Code::TypeMismatch]));
        assert_eq!(check("[1]", &Schema::enumeration(["a"])), (Outcome::Keep, vec![]));
        assert_eq!(check("5", &Schema::union([Schema::Number, Schema::String])), (Outcome::Keep, vec![]));
        let numbers = Schema::array(Schema::Number);
        assert_eq!(
            check("[1, \"x\", 3]", &numbers),
            (Outcome::Replace(Value::from_json_str("[1, 3]").unwrap()), vec![Code::TypeMismatch])
        );
        let object = Schema::object([
            Schema::String.field("a"),
            Schema::Number.field("b").with_default(2),
            Schema::Boolean.field("c").optional(),
        ]);
        assert_eq!(
            check(r#"{"a": "x", "c": 1}"#, &object),
            (Outcome::Replace(Value::from_json_str(r#"{"a": "x", "b": 2}"#).unwrap()), vec![Code::TypeMismatch])
        );
        assert_eq!(check(r#"{"a": null}"#, &object), (Outcome::Invalid, vec![Code::NullRequired]));
        let mut partial = Checker::new(true);
        assert_eq!(partial.value(&Value::from("x"), &Schema::enumeration(["a"]), ""), Outcome::Keep);
    }
}
