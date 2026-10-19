//! What a message carries to the agent from programs, as the chip on it shows: what a program sent
//! with `@ToAssistant`, and the state of the programs the user changed before writing it.

use motile_protocol::wire::ProgramState;
use serde::Serialize;
use serde_json::Value;

const LABEL: &str = "Form values";
const CLICKED: &str = "User clicked: ";

/// The chip on a message and the values it opens.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct SentValues {
    pub label: String,
    pub entries: Vec<SentValue>,
}

/// A value by its name: a field's label, a `$variable`, or a path into the context, `table.seat`.
/// An empty object names nothing and is left out.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct SentValue {
    pub name: String,
    /// Text as it is, anything else, and empty text, as JSON.
    pub value: String,
}

pub fn sent_values(context: Option<&Value>, programs: &[ProgramState]) -> Option<SentValues> {
    let mut entries = Vec::new();
    match context {
        Some(Value::Array(parts)) => {
            let clicked = parts.first().and_then(Value::as_str).is_some_and(|first| first.starts_with(CLICKED));
            for part in &parts[usize::from(clicked)..] {
                flatten("", part, &mut entries);
            }
        }
        Some(context) => flatten("", context, &mut entries),
        None => {}
    }
    for program in programs {
        let Some(snapshot) = program.state.as_object().filter(|state| state.contains_key("state")) else {
            flatten("", &program.state, &mut entries);
            continue;
        };
        for part in ["state", "fields", "forms"] {
            flatten("", snapshot.get(part).unwrap_or(&Value::Null), &mut entries);
        }
    }
    (!entries.is_empty()).then(|| SentValues { label: LABEL.to_owned(), entries })
}

fn flatten(name: &str, value: &Value, entries: &mut Vec<SentValue>) {
    if let Some(field) = field_value(value) {
        return flatten(name, field, entries);
    }
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                let path = if name.is_empty() { key.clone() } else { format!("{name}.{key}") };
                flatten(&path, value, entries);
            }
        }
        Value::Null if name.is_empty() => {}
        value => {
            let name = if name.is_empty() { "context" } else { name };
            let text = value.as_str().filter(|text| !text.is_empty());
            let value = text.map_or_else(|| value.to_string(), str::to_owned);
            entries.push(SentValue { name: name.to_owned(), value });
        }
    }
}

/// A form field as a program's store keeps it, `{"value": …, "componentType": …}`: its value.
fn field_value(value: &Value) -> Option<&Value> {
    let field = value.as_object()?;
    let only_a_field = field.keys().all(|key| key == "value" || key == "componentType");
    field.get("value").filter(|_| only_a_field)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn entries(values: Option<SentValues>) -> Vec<(String, String)> {
        let values = values.expect("a chip");
        assert_eq!(values.label, "Form values");
        values.entries.into_iter().map(|entry| (entry.name, entry.value)).collect()
    }

    fn pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect()
    }

    #[test]
    fn a_sent_form_is_its_fields_and_the_programs_context() {
        let context = json!(["User clicked: Book it", {"Booking": {"Name": {"value": "Ada"}, "Guests": {"value": 4, "componentType": "Input"}}, "trip": {}}, {"table": 7, "seat": {"row": "B"}, "notes": ""}]);
        assert_eq!(
            entries(sent_values(Some(&context), &[])),
            pairs(&[("Booking.Guests", "4"), ("Booking.Name", "Ada"), ("notes", "\"\""), ("seat.row", "B"), ("table", "7")])
        );
        assert_eq!(entries(sent_values(Some(&json!("hidden")), &[])), pairs(&[("context", "hidden")]));
    }

    #[test]
    fn a_typed_message_shows_the_state_of_the_programs_it_changed() {
        let changed = |state: Value| ProgramState { item_id: "a".into(), block: 1, state };
        let snapshot = changed(json!({"state": {"$days": "30"}, "fields": {"Note": {"value": ["x"]}}, "forms": {}}));
        assert_eq!(
            entries(sent_values(None, &[snapshot, changed(json!({"$mode": "item"}))])),
            pairs(&[("$days", "30"), ("Note", "[\"x\"]"), ("$mode", "item")])
        );
    }

    #[test]
    fn nothing_sent_has_no_chip() {
        assert_eq!(sent_values(None, &[]), None);
        assert_eq!(sent_values(Some(&json!(["User clicked: Go", {}])), &[]), None);
    }
}
