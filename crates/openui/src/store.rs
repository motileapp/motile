//! The state store (language.md §5.1, §5.3): the `$variables`, the page-level fields and the
//! fields of each form. A declared default applies only while the variable has no value the
//! user or the host gave it.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use crate::value::{Map, Value};

/// A control's value outside a binding, with the kind of component that wrote it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Field {
    pub value: Value,
    pub component_type: Option<String>,
}

impl Field {
    /// `{value, componentType}`.
    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("value".into(), self.value.clone());
        if let Some(component_type) = &self.component_type {
            map.insert("componentType".into(), component_type.as_str().into());
        }
        Value::Object(map)
    }

    fn from_value(value: &Value) -> Field {
        Field {
            value: value.get("value").cloned().unwrap_or_default(),
            component_type: value.get("componentType").and_then(Value::as_str).map(str::to_owned),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Store {
    vars: IndexMap<String, Value>,
    /// Variables that still hold their declared default, which a later parse may replace.
    pristine: HashSet<String>,
    fields: IndexMap<String, Field>,
    forms: IndexMap<String, IndexMap<String, Field>>,
    versions: HashMap<String, u64>,
    clock: u64,
}

impl Store {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.vars.get(name)
    }

    pub fn vars(&self) -> &IndexMap<String, Value> {
        &self.vars
    }

    /// A number that changes whenever the variable does.
    pub fn version(&self, name: &str) -> u64 {
        self.versions.get(name).copied().unwrap_or_default()
    }

    /// Sets a variable; `pristine` when it is a declared default again (`@Reset`). Whether the
    /// value changed.
    pub fn set(&mut self, name: &str, value: Value, pristine: bool) -> bool {
        if pristine {
            self.pristine.insert(name.to_owned());
        } else {
            self.pristine.remove(name);
        }
        if self.vars.get(name).is_some_and(|current| same(current, &value)) {
            return false;
        }
        self.vars.insert(name.to_owned(), value);
        self.clock += 1;
        self.versions.insert(name.to_owned(), self.clock);
        true
    }

    /// Applies the declared defaults: to a variable with no value, and over one that still holds
    /// a default read earlier. Keys are never deleted. The variables that changed.
    pub fn initialize(&mut self, defaults: &IndexMap<String, Value>) -> Vec<String> {
        let mut changed = Vec::new();
        for (name, value) in defaults {
            let replace = match self.vars.get(name) {
                None => true,
                Some(current) => self.pristine.contains(name) && !same(current, value),
            };
            if replace {
                self.set(name, value.clone(), true);
                changed.push(name.clone());
            }
        }
        changed
    }

    pub fn field(&self, form: Option<&str>, name: &str) -> Option<&Field> {
        match form {
            Some(form) => self.forms.get(form)?.get(name),
            None => self.fields.get(name),
        }
    }

    /// Sets a field of a form, or of the page. Whether it changed.
    pub fn set_field(&mut self, form: Option<&str>, name: &str, field: Field) -> bool {
        let fields = match form {
            Some(form) => self.forms.entry(form.to_owned()).or_default(),
            None => &mut self.fields,
        };
        if fields.get(name) == Some(&field) {
            return false;
        }
        fields.insert(name.to_owned(), field);
        true
    }

    /// What an event carries (§6.3): inside a form, that form's fields; outside, the page's
    /// variables and its page-level fields.
    pub fn form_state(&self, form: Option<&str>) -> Value {
        if let Some(form) = form {
            let fields = self.forms.get(form).map(fields_value).unwrap_or_else(|| Value::Object(Map::new()));
            return Value::from_iter([(form, fields)]);
        }
        let mut map: Map = self.vars.iter().map(|(name, value)| (name.clone(), value.clone())).collect();
        map.extend(self.fields.iter().map(|(name, field)| (name.clone(), field.to_value())));
        Value::Object(map)
    }

    /// Everything the user and the actions set, to restore later: `{state, fields, forms}`.
    pub fn snapshot(&self) -> Value {
        let state: Map = self
            .vars
            .iter()
            .filter(|(name, _)| !self.pristine.contains(*name))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        let forms: Map = self.forms.iter().map(|(name, fields)| (name.clone(), fields_value(fields))).collect();
        Value::from_iter([
            ("state", Value::Object(state)),
            ("fields", fields_value(&self.fields)),
            ("forms", Value::Object(forms)),
        ])
    }

    /// Applies a snapshot over the defaults. The variables that changed.
    pub fn restore(&mut self, snapshot: &Value) -> Vec<String> {
        let mut changed = Vec::new();
        for (name, value) in snapshot.get("state").and_then(Value::as_object).into_iter().flatten() {
            if self.set(name, value.clone(), false) {
                changed.push(name.clone());
            }
        }
        for (name, field) in snapshot.get("fields").and_then(Value::as_object).into_iter().flatten() {
            self.set_field(None, name, Field::from_value(field));
        }
        for (form, fields) in snapshot.get("forms").and_then(Value::as_object).into_iter().flatten() {
            for (name, field) in fields.as_object().into_iter().flatten() {
                self.set_field(Some(form), name, Field::from_value(field));
            }
        }
        changed
    }
}

fn fields_value(fields: &IndexMap<String, Field>) -> Value {
    Value::Object(fields.iter().map(|(name, field)| (name.clone(), field.to_value())).collect())
}

/// Equal values; NaN equals NaN, so that setting it again changes nothing.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) if a.is_nan() && b.is_nan() => true,
        (a, b) => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults(entries: &[(&str, Value)]) -> IndexMap<String, Value> {
        entries.iter().map(|(name, value)| (name.to_string(), value.clone())).collect()
    }

    #[test]
    fn keeps_what_the_user_set_over_defaults() {
        let mut store = Store::default();
        assert_eq!(store.initialize(&defaults(&[("$a", Value::from("tr"))])), ["$a"]);
        assert_eq!(store.initialize(&defaults(&[("$a", Value::from("truncated"))])), ["$a"]);
        assert_eq!(store.get("$a"), Some(&Value::from("truncated")));
        store.set("$a", Value::from("mine"), false);
        assert!(store.initialize(&defaults(&[("$a", Value::from("other"))])).is_empty());
        assert_eq!(store.get("$a"), Some(&Value::from("mine")));
        store.set("$a", Value::from("other"), true);
        assert_eq!(store.initialize(&defaults(&[("$a", Value::from("again"))])), ["$a"]);
    }

    #[test]
    fn snapshots_and_restores() {
        let mut store = Store::default();
        store.initialize(&defaults(&[("$a", Value::from(1)), ("$b", Value::from(2))]));
        store.set("$b", Value::from(3), false);
        store.set_field(None, "note", Field { value: Value::from("rush"), component_type: Some("input".into()) });
        store.set_field(Some("contact"), "email", Field { value: Value::from("a@b.co"), component_type: None });
        let snapshot = store.snapshot();
        assert_eq!(
            snapshot.to_json_string(),
            r#"{"state":{"$b":3},"fields":{"note":{"value":"rush","componentType":"input"}},"forms":{"contact":{"email":{"value":"a@b.co"}}}}"#
        );
        let mut restored = Store::default();
        restored.restore(&snapshot);
        restored.initialize(&defaults(&[("$a", Value::from(1)), ("$b", Value::from(2))]));
        assert_eq!(restored.get("$b"), Some(&Value::from(3)));
        assert_eq!(
            restored.form_state(Some("contact")).to_json_string(),
            r#"{"contact":{"email":{"value":"a@b.co"}}}"#
        );
        assert_eq!(
            restored.form_state(None).to_json_string(),
            r#"{"$b":3,"$a":1,"note":{"value":"rush","componentType":"input"}}"#
        );
    }
}
