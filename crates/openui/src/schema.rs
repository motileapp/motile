//! The types of props and params, as the Zod schemas libraries are written with. A schema prints
//! the prompt's signature (`signature.ts`), writes the JSON Schema `library.toJSONSchema()` writes,
//! and is read back from one.
//!
//! Validation works on the JSON Schema in the reference, so the accessors below answer what that
//! JSON says: a union of plain types becomes `{"type": [...]}` and goes unchecked, a `$ref` or an
//! `anyOf` is checked only when every option is a component.

use std::collections::HashSet;

use crate::js;
use crate::value::{Map, Value};

pub const ACTION_EXPRESSION: &str = "ActionExpression";
/// The built-in actions a restricted action slot can name, beside the library's own.
pub const ACTION_REF_BUILTINS: [&str; 2] = ["OpenUrl", "ToAssistant"];

#[derive(Debug, Clone, PartialEq)]
pub enum Schema {
    String,
    Number,
    Boolean,
    Null,
    Any,
    /// `z.enum([...])`.
    Enum(Vec<String>),
    /// `z.literal(value)`: a string, number, boolean or null.
    Literal(Value),
    Array(Box<Schema>),
    /// `z.object({...})`, its fields in positional order.
    Object(Vec<Field>),
    /// `z.record(z.string(), values)`.
    Record(Box<Schema>),
    Union(Vec<Schema>),
    Nullable(Box<Schema>),
    /// A component's `.ref`: that component.
    Component(String),
    /// `action()`: any action, `ActionExpression` in the prompt.
    Action,
    /// One action a restricted slot takes, `@Name` in the prompt: a custom one, `OpenUrl` or
    /// `ToAssistant`. A slot of several is a union of them.
    ActionRef(String),
    /// `tagSchemaId(schema, name)`: prints as the name, writes the schema's JSON.
    Named(String, Box<Schema>),
}

/// A prop of a component, or a param of a function or an action.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub schema: Schema,
    /// `.optional()`.
    pub optional: bool,
    /// `.default(value)`: fills the prop when it is missing or invalid.
    pub default: Option<Value>,
    /// `reactive(...)`: takes a `$variable` two ways, `$binding<type>` in the prompt.
    pub bindable: bool,
    pub description: Option<String>,
}

impl Schema {
    pub fn array(items: Schema) -> Schema {
        Schema::Array(Box::new(items))
    }

    pub fn object(fields: impl IntoIterator<Item = Field>) -> Schema {
        Schema::Object(fields.into_iter().collect())
    }

    pub fn record(values: Schema) -> Schema {
        Schema::Record(Box::new(values))
    }

    pub fn union(options: impl IntoIterator<Item = Schema>) -> Schema {
        Schema::Union(options.into_iter().collect())
    }

    pub fn enumeration<S: Into<String>>(values: impl IntoIterator<Item = S>) -> Schema {
        Schema::Enum(values.into_iter().map(Into::into).collect())
    }

    pub fn literal(value: impl Into<Value>) -> Schema {
        Schema::Literal(value.into())
    }

    pub fn component(name: impl Into<String>) -> Schema {
        Schema::Component(name.into())
    }

    pub fn action_ref(name: impl Into<String>) -> Schema {
        Schema::ActionRef(name.into())
    }

    pub fn nullable(self) -> Schema {
        Schema::Nullable(Box::new(self))
    }

    pub fn named(self, name: impl Into<String>) -> Schema {
        Schema::Named(name.into(), Box::new(self))
    }

    pub fn field(self, name: impl Into<String>) -> Field {
        Field::new(name, self)
    }

    /// The schema under its names and nullability.
    fn base(&self) -> &Schema {
        match self {
            Schema::Named(_, inner) | Schema::Nullable(inner) => inner.base(),
            schema => schema,
        }
    }

    fn unnamed(&self) -> &Schema {
        match self {
            Schema::Named(_, inner) => inner.unnamed(),
            schema => schema,
        }
    }

    /// The type as a signature prints it: `string`, `"a" | "b"`, `Node[]`, `{value: number}`…
    pub fn annotation(&self) -> String {
        match self {
            Schema::Nullable(inner) => inner.annotation(),
            Schema::ActionRef(name) => format!("@{name}"),
            Schema::Named(name, _) | Schema::Component(name) => name.clone(),
            Schema::Action => ACTION_EXPRESSION.to_owned(),
            Schema::Union(options) if !options.is_empty() => {
                options.iter().map(Schema::annotation).collect::<Vec<_>>().join(" | ")
            }
            Schema::Array(items) if matches!(items.base(), Schema::Union(_)) => format!("({})[]", items.annotation()),
            Schema::Array(items) => format!("{}[]", items.annotation()),
            Schema::String => "string".to_owned(),
            Schema::Number => "number".to_owned(),
            Schema::Boolean => "boolean".to_owned(),
            Schema::Record(values) => format!("Record<string, {}>", values.annotation()),
            Schema::Enum(values) => values.iter().map(|value| format!("\"{value}\"")).collect::<Vec<_>>().join(" | "),
            Schema::Literal(Value::String(text)) => format!("\"{text}\""),
            Schema::Literal(value) => js::to_string(value),
            Schema::Object(fields) => {
                format!("{{{}}}", fields.iter().map(Field::signature_part).collect::<Vec<_>>().join(", "))
            }
            Schema::Any | Schema::Null | Schema::Union(_) => "any".to_owned(),
        }
    }

    /// The short type of an error hint (`buildParamsSignature`), read from the JSON Schema with
    /// its action `$ref`s written out: a component's name, `"a"|"b"`, `string`, `array`…
    pub fn hint_type(&self) -> Option<String> {
        match self {
            Schema::Named(_, inner) => inner.hint_type(),
            Schema::Component(name) => Some(name.clone()),
            Schema::Enum(values) => Some(
                values.iter().map(|value| Value::from(value.as_str()).to_json_string()).collect::<Vec<_>>().join("|"),
            ),
            Schema::Literal(value) => Some(value.to_json_string()),
            Schema::ActionRef(_) => Some("object".to_owned()),
            schema => schema.json_type().map(str::to_owned),
        }
    }

    /// Its JSON Schema's `type` when that is one name: `string`, `number`, `boolean`, `null`,
    /// `array` or `object`. A composite or an `any` has none.
    pub fn json_type(&self) -> Option<&'static str> {
        match self.unnamed() {
            Schema::String | Schema::Enum(_) => Some("string"),
            Schema::Number => Some("number"),
            Schema::Boolean => Some("boolean"),
            Schema::Null => Some("null"),
            Schema::Literal(value) => literal_type(value),
            Schema::Array(_) => Some("array"),
            Schema::Object(_) | Schema::Record(_) => Some("object"),
            _ => None,
        }
    }

    /// Whether its JSON Schema is a `$ref` or an `anyOf`.
    pub fn is_composite(&self) -> bool {
        match self.unnamed() {
            Schema::Component(_) | Schema::Action | Schema::ActionRef(_) => true,
            Schema::Union(_) | Schema::Nullable(_) => self.to_json(&mut Vec::new()).get("anyOf").is_some(),
            _ => false,
        }
    }

    /// The components a slot takes when it takes nothing else: data there is a `type-mismatch`.
    pub fn component_slot(&self) -> Option<Vec<&str>> {
        match self.unnamed() {
            Schema::Component(name) => Some(vec![name.as_str()]),
            Schema::Union(options) if !options.is_empty() => options
                .iter()
                .map(|option| match option.unnamed() {
                    Schema::Component(name) => Some(name.as_str()),
                    _ => None,
                })
                .collect(),
            _ => None,
        }
    }

    /// The values an enum or a literal allows.
    pub fn allowed_values(&self) -> Option<Vec<Value>> {
        match self.unnamed() {
            Schema::Enum(values) => Some(values.iter().map(|value| Value::from(value.as_str())).collect()),
            Schema::Literal(value) => Some(vec![value.clone()]),
            _ => None,
        }
    }

    /// An object's fields; none for a record, whose keys are free.
    pub fn fields(&self) -> Option<&[Field]> {
        match self.unnamed() {
            Schema::Object(fields) => Some(fields),
            Schema::Record(_) => Some(&[]),
            _ => None,
        }
    }

    /// An array's items.
    pub fn items(&self) -> Option<&Schema> {
        let Schema::Array(items) = self.unnamed() else { return None };
        Some(items)
    }

    /// Its JSON Schema, as Zod writes it. The names it refers to through `$defs` (components,
    /// `ActionExpression`, action refs) are added to `refs` in the order they come.
    pub fn to_json(&self, refs: &mut Vec<String>) -> Value {
        match self {
            Schema::String => json_type("string"),
            Schema::Number => json_type("number"),
            Schema::Boolean => json_type("boolean"),
            Schema::Null => json_type("null"),
            Schema::Any => Value::Object(Map::new()),
            Schema::Enum(values) => {
                let mut map = Map::new();
                map.insert("type".into(), "string".into());
                map.insert("enum".into(), values.iter().map(|value| Value::from(value.as_str())).collect());
                Value::Object(map)
            }
            Schema::Literal(value) => {
                let mut map = Map::new();
                map.insert("type".into(), literal_type(value).unwrap_or("null").into());
                map.insert("const".into(), value.clone());
                Value::Object(map)
            }
            Schema::Array(items) => {
                let mut map = Map::new();
                map.insert("type".into(), "array".into());
                map.insert("items".into(), items.to_json(refs));
                Value::Object(map)
            }
            Schema::Object(fields) => object_json(fields, refs),
            Schema::Record(values) => {
                let mut map = Map::new();
                map.insert("type".into(), "object".into());
                map.insert("propertyNames".into(), json_type("string"));
                map.insert("additionalProperties".into(), values.to_json(refs));
                Value::Object(map)
            }
            Schema::Union(options) => union_json(options.iter().map(|option| option.to_json(refs)).collect()),
            Schema::Nullable(inner) => union_json(vec![inner.to_json(refs), json_type("null")]),
            Schema::Component(name) | Schema::ActionRef(name) => reference(name, refs),
            Schema::Action => reference(ACTION_EXPRESSION, refs),
            Schema::Named(_, inner) => inner.to_json(refs),
        }
    }

    /// Reads a JSON Schema back. What JSON cannot say is lost: names given with
    /// [`Schema::named`] read as what they name, and bindable props are only marked in signatures.
    pub fn from_json(json: &Value, refs: &JsonRefs<'_>) -> Schema {
        read_schema(json, refs, 0)
    }
}

impl Field {
    pub fn new(name: impl Into<String>, schema: Schema) -> Field {
        Field { name: name.into(), schema, optional: false, default: None, bindable: false, description: None }
    }

    pub fn optional(mut self) -> Field {
        self.optional = true;
        self
    }

    pub fn with_default(mut self, value: impl Into<Value>) -> Field {
        self.default = Some(value.into());
        self
    }

    pub fn bindable(mut self) -> Field {
        self.bindable = true;
        self
    }

    pub fn describe(mut self, description: impl Into<String>) -> Field {
        self.description = Some(description.into());
        self
    }

    /// Listed in the JSON Schema's `required`. A prop with a default is, as Zod writes its output.
    pub fn required(&self) -> bool {
        !self.optional
    }

    /// Marked `?` in signatures: optional, with a default, or nullable.
    pub fn prints_optional(&self) -> bool {
        self.optional || self.default.is_some() || matches!(self.schema, Schema::Nullable(_))
    }

    /// `name?: type`, as a signature lists it.
    pub fn signature_part(&self) -> String {
        let mark = if self.prints_optional() { "?" } else { "" };
        let annotation = self.schema.annotation();
        if self.bindable {
            return format!("{}{mark}: $binding<{annotation}>", self.name);
        }
        format!("{}{mark}: {annotation}", self.name)
    }

    /// `name*: type`, as an error hint lists it: the star marks a required prop.
    pub fn hint_part(&self) -> String {
        let mark = if self.required() { "*" } else { "" };
        match self.schema.hint_type() {
            Some(hint) => format!("{}{mark}: {hint}", self.name),
            None => format!("{}{mark}", self.name),
        }
    }

    fn to_json(&self, refs: &mut Vec<String>) -> Value {
        let mut json = self.schema.to_json(refs);
        let Value::Object(map) = &mut json else { return json };
        if let Some(default) = &self.default {
            map.insert("default".into(), default.clone());
        }
        if let Some(description) = &self.description {
            map.insert("description".into(), description.as_str().into());
        }
        json
    }
}

/// `Name(a: string, b?: number) → string`, the prompt's line for a component, function or action.
pub fn signature(name: &str, fields: &[Field], returns: Option<&Schema>) -> String {
    let params = fields.iter().map(Field::signature_part).collect::<Vec<_>>().join(", ");
    match returns {
        Some(returns) => format!("{name}({params}) → {}", returns.annotation()),
        None => format!("{name}({params})"),
    }
}

/// `Name(a*: string, b: "x"|"y")`, the short signature of an error's hint.
pub fn hint_signature(name: &str, fields: &[Field]) -> String {
    format!("{name}({})", fields.iter().map(Field::hint_part).collect::<Vec<_>>().join(", "))
}

/// The JSON Schema of an object of these fields.
pub fn object_json(fields: &[Field], refs: &mut Vec<String>) -> Value {
    let mut map = Map::new();
    map.insert("type".into(), "object".into());
    map.insert("properties".into(), fields.iter().map(|field| (field.name.clone(), field.to_json(refs))).collect());
    let required: Vec<Value> =
        fields.iter().filter(|field| field.required()).map(|field| field.name.as_str().into()).collect();
    if !required.is_empty() {
        map.insert("required".into(), Value::Array(required));
    }
    map.insert("additionalProperties".into(), false.into());
    Value::Object(map)
}

fn json_type(name: &str) -> Value {
    Value::from_iter([("type", Value::from(name))])
}

fn literal_type(value: &Value) -> Option<&'static str> {
    match value {
        Value::String(_) => Some("string"),
        Value::Number(_) => Some("number"),
        Value::Bool(_) => Some("boolean"),
        Value::Null => Some("null"),
        _ => None,
    }
}

fn reference(name: &str, refs: &mut Vec<String>) -> Value {
    refs.push(name.to_owned());
    Value::from_iter([("$ref", Value::from(format!("#/$defs/{name}")))])
}

/// `anyOf`, or one `type` list when every option is a plain `{"type": name}`.
fn union_json(options: Vec<Value>) -> Value {
    let types: Option<Vec<Value>> = options.iter().map(plain_type).collect();
    match types {
        Some(types) if !types.is_empty() => Value::from_iter([("type", Value::Array(types))]),
        _ => Value::from_iter([("anyOf", Value::Array(options))]),
    }
}

fn plain_type(json: &Value) -> Option<Value> {
    let map = json.as_object()?;
    if map.len() != 1 {
        return None;
    }
    let name @ Value::String(_) = map.get("type")? else { return None };
    Some(name.clone())
}

/// What the `$ref`s of a library's JSON Schema name.
#[derive(Debug, Default)]
pub struct JsonRefs<'a> {
    components: HashSet<&'a str>,
    actions: HashSet<&'a str>,
    defs: Option<&'a Map>,
}

impl<'a> JsonRefs<'a> {
    /// From a library's JSON Schema: components are the names in its `properties`, actions those
    /// in its `actions` and the built-in ones.
    pub fn new(schema: &'a Value) -> JsonRefs<'a> {
        let names = |key: &str| -> HashSet<&'a str> {
            schema
                .get(key)
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|map| map.keys())
                .map(String::as_str)
                .collect()
        };
        let mut actions = names("actions");
        actions.extend(ACTION_REF_BUILTINS);
        JsonRefs { components: names("properties"), actions, defs: schema.get("$defs").and_then(Value::as_object) }
    }

    pub fn with_components(mut self, components: impl IntoIterator<Item = &'a str>) -> JsonRefs<'a> {
        self.components = components.into_iter().collect();
        self
    }

    fn resolve(&self, reference: &str, depth: usize) -> Schema {
        let name = reference.rsplit('/').next().unwrap_or(reference);
        if self.components.contains(name) {
            return Schema::Component(name.to_owned());
        }
        if name == ACTION_EXPRESSION {
            return Schema::Action;
        }
        if self.actions.contains(name) {
            return Schema::ActionRef(name.to_owned());
        }
        let Some(def) = self.defs.and_then(|defs| defs.get(name)) else { return Schema::Any };
        if depth > 32 {
            return Schema::Any;
        }
        read_schema(def, self, depth + 1)
    }
}

fn read_schema(json: &Value, refs: &JsonRefs<'_>, depth: usize) -> Schema {
    let Some(map) = json.as_object() else { return Schema::Any };
    if let Some(Value::String(reference)) = map.get("$ref") {
        return refs.resolve(reference, depth);
    }
    if let Some(Value::Array(options)) = map.get("anyOf").or_else(|| map.get("oneOf")) {
        return read_union(options.iter().map(|option| read_schema(option, refs, depth)).collect());
    }
    if let Some(Value::Array(values)) = map.get("enum") {
        let strings: Option<Vec<String>> = values.iter().map(|value| value.as_str().map(str::to_owned)).collect();
        return match strings {
            Some(strings) => Schema::Enum(strings),
            None => read_union(values.iter().cloned().map(Schema::Literal).collect()),
        };
    }
    if let Some(value) = map.get("const") {
        return Schema::Literal(value.clone());
    }
    match map.get("type") {
        Some(Value::String(name)) => read_type(name, map, refs, depth),
        Some(Value::Array(names)) => read_union(
            names.iter().filter_map(Value::as_str).map(|name| read_type(name, &Map::new(), refs, depth)).collect(),
        ),
        _ => Schema::Any,
    }
}

fn read_type(name: &str, map: &Map, refs: &JsonRefs<'_>, depth: usize) -> Schema {
    match name {
        "string" => Schema::String,
        "number" | "integer" => Schema::Number,
        "boolean" => Schema::Boolean,
        "null" => Schema::Null,
        "array" => Schema::array(map.get("items").map_or(Schema::Any, |items| read_schema(items, refs, depth))),
        "object" if map.contains_key("properties") => Schema::Object(read_fields(map, refs, depth)),
        "object" => match map.get("additionalProperties") {
            Some(values @ Value::Object(_)) => Schema::record(read_schema(values, refs, depth)),
            _ if map.contains_key("propertyNames") => Schema::record(Schema::Any),
            _ => Schema::Object(Vec::new()),
        },
        _ => Schema::Any,
    }
}

/// A union, nullable when one of its options is null.
fn read_union(mut options: Vec<Schema>) -> Schema {
    let Some(null) = options.iter().position(|option| *option == Schema::Null) else { return Schema::Union(options) };
    if options.len() == 1 {
        return Schema::Null;
    }
    options.remove(null);
    let inner = if options.len() == 1 { options.remove(0) } else { Schema::Union(options) };
    inner.nullable()
}

/// The fields of an object's JSON Schema, in the order of its `properties`.
pub fn fields_from_json(json: &Value, refs: &JsonRefs<'_>) -> Vec<Field> {
    let Some(map) = json.as_object() else { return Vec::new() };
    read_fields(map, refs, 0)
}

fn read_fields(map: &Map, refs: &JsonRefs<'_>, depth: usize) -> Vec<Field> {
    let required: Vec<&str> =
        map.get("required").and_then(Value::as_array).unwrap_or_default().iter().filter_map(Value::as_str).collect();
    let Some(properties) = map.get("properties").and_then(Value::as_object) else { return Vec::new() };
    properties
        .iter()
        .map(|(name, json)| Field {
            name: name.clone(),
            schema: read_schema(json, refs, depth),
            optional: !required.contains(&name.as_str()),
            default: json.get("default").cloned(),
            bindable: false,
            description: json.get("description").and_then(Value::as_str).map(str::to_owned),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(schema: &Schema) -> String {
        serde_json::to_string(&schema.to_json(&mut Vec::new())).unwrap()
    }

    fn node() -> Schema {
        Schema::Any.named("Node")
    }

    #[test]
    fn prints_types_as_signature_ts() {
        let choice = Schema::union([
            Schema::String,
            Schema::object([
                Schema::String.field("value"),
                Schema::String.field("label"),
                Schema::String.field("description").optional(),
            ]),
        ]);
        let cases = [
            (Schema::array(node()), "Node[]"),
            (Schema::union([Schema::String, Schema::array(node())]), "string | Node[]"),
            (Schema::array(choice), "(string | {value: string, label: string, description?: string})[]"),
            (
                Schema::array(Schema::record(Schema::union([Schema::String, Schema::Number]))),
                "Record<string, string | number>[]",
            ),
            (Schema::array(Schema::array(Schema::Any)), "any[][]"),
            (Schema::enumeration(["a", "b"]), "\"a\" | \"b\""),
            (
                Schema::union([Schema::action_ref("CopyToClipboard"), Schema::action_ref("OpenUrl")]),
                "@CopyToClipboard | @OpenUrl",
            ),
            (Schema::Action, "ActionExpression"),
            (Schema::Number.nullable(), "number"),
            (Schema::array(Schema::union([Schema::Number, Schema::String]).named("Length")), "(Length)[]"),
            (Schema::literal("x"), "\"x\""),
            (Schema::literal(2), "2"),
            (Schema::Null, "any"),
            (Schema::object([]), "{}"),
        ];
        for (schema, expected) in cases {
            assert_eq!(schema.annotation(), expected);
        }
    }

    #[test]
    fn prints_fields_with_their_marks() {
        let fields = [
            Schema::union([Schema::String, Schema::Number]).field("value").bindable(),
            Schema::String.field("selected").optional().bindable(),
            Schema::Number.field("step").with_default(1),
            Schema::Number.nullable().field("previous"),
            Schema::Action.field("action").optional(),
        ];
        assert_eq!(
            signature("X", &fields, Some(&Schema::String)),
            "X(value: $binding<string | number>, selected?: $binding<string>, step?: number, previous?: number, \
             action?: ActionExpression) → string"
        );
        assert_eq!(hint_signature("X", &fields), "X(value*, selected: string, step*: number, previous*, action)");
    }

    #[test]
    fn writes_json_schema_as_zod() {
        assert_eq!(json(&Schema::union([Schema::Number, Schema::String])), r#"{"type":["number","string"]}"#);
        assert_eq!(json(&Schema::Number.nullable()), r#"{"type":["number","null"]}"#);
        assert_eq!(
            json(&Schema::union([Schema::enumeration(["sm", "md"]), Schema::Number])),
            r#"{"anyOf":[{"type":"string","enum":["sm","md"]},{"type":"number"}]}"#
        );
        assert_eq!(
            json(&Schema::record(Schema::Any)),
            r#"{"type":"object","propertyNames":{"type":"string"},"additionalProperties":{}}"#
        );
        assert_eq!(json(&Schema::literal("open_url")), r#"{"type":"string","const":"open_url"}"#);
        assert_eq!(json(&Schema::array(node())), r#"{"type":"array","items":{}}"#);
        assert_eq!(
            json(&Schema::object([
                Schema::String.field("a").with_default("x").describe("A"),
                Schema::Number.field("b").optional()
            ])),
            r#"{"type":"object","properties":{"a":{"type":"string","default":"x","description":"A"},"b":{"type":"number"}},"required":["a"],"additionalProperties":false}"#
        );
        let mut refs = Vec::new();
        let slot = Schema::union([Schema::action_ref("CopyToClipboard"), Schema::Action, Schema::component("Card")]);
        assert_eq!(
            serde_json::to_string(&slot.to_json(&mut refs)).unwrap(),
            r##"{"anyOf":[{"$ref":"#/$defs/CopyToClipboard"},{"$ref":"#/$defs/ActionExpression"},{"$ref":"#/$defs/Card"}]}"##
        );
        assert_eq!(refs, ["CopyToClipboard", "ActionExpression", "Card"]);
    }

    #[test]
    fn reads_back_what_it_writes() {
        let schema =
            Value::from_json_str(r#"{"properties": {"Card": {}}, "actions": {"CopyToClipboard": {}}}"#).unwrap();
        let refs = JsonRefs::new(&schema);
        let cases = [
            Schema::union([Schema::Number, Schema::String]),
            Schema::Number.nullable(),
            Schema::union([Schema::String, Schema::object([Schema::String.field("v")])]).nullable(),
            Schema::array(Schema::record(Schema::union([Schema::String, Schema::Number]))),
            Schema::union([Schema::action_ref("CopyToClipboard"), Schema::action_ref("OpenUrl")]),
            Schema::array(Schema::component("Card")),
            Schema::Action,
            Schema::literal(true),
            Schema::object([Schema::String.field("a").with_default("x"), Schema::Any.field("b").optional()]),
            Schema::object([]),
            Schema::enumeration(["a"]),
        ];
        for schema in cases {
            let written = schema.to_json(&mut Vec::new());
            assert_eq!(Schema::from_json(&written, &refs), schema, "{written:?}");
        }
        let bare = Value::from_json_str(r#"{"type": "array"}"#).unwrap();
        assert_eq!(Schema::from_json(&bare, &refs), Schema::array(Schema::Any));
    }

    #[test]
    fn answers_what_validation_asks() {
        let slot = Schema::union([Schema::component("Card"), Schema::component("Text")]);
        assert_eq!(slot.component_slot(), Some(vec!["Card", "Text"]));
        assert!(slot.is_composite());
        assert_eq!(Schema::union([Schema::String, Schema::component("Card")]).component_slot(), None);
        assert!(!Schema::union([Schema::Number, Schema::String]).is_composite());
        assert!(Schema::union([Schema::String, Schema::array(node())]).is_composite());
        assert!(!Schema::Number.nullable().is_composite());
        assert_eq!(Schema::union([Schema::Number, Schema::String]).json_type(), None);
        assert_eq!(Schema::String.named("Color").json_type(), Some("string"));
        assert_eq!(Schema::enumeration(["a"]).allowed_values(), Some(vec![Value::from("a")]));
        assert_eq!(Schema::record(Schema::Any).fields(), Some(&[][..]));
        assert_eq!(Schema::array(node()).items(), Some(&node()));
    }
}
