//! A library: its components, groups, functions and actions, checked as `createLibrary` checks them
//! and as prompt.md §3.1 asks. It writes the JSON Schema and the LibrarySpec the reference writes,
//! and reads a LibrarySpec back.

use std::collections::HashSet;
use std::fmt;

use indexmap::IndexMap;

use crate::builtins::{self, Builtin};
use crate::schema::{self, ACTION_EXPRESSION, ACTION_REF_BUILTINS, Field, JsonRefs, Schema};
use crate::value::{Map, Value};

const JSON_SCHEMA_DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

#[derive(Debug, Clone, PartialEq)]
pub struct ComponentDef {
    pub name: String,
    pub description: String,
    /// In positional order.
    pub props: Vec<Field>,
}

impl ComponentDef {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        props: impl IntoIterator<Item = Field>,
    ) -> ComponentDef {
        ComponentDef { name: name.into(), description: description.into(), props: props.into_iter().collect() }
    }

    pub fn prop(&self, name: &str) -> Option<&Field> {
        self.props.iter().find(|prop| prop.name == name)
    }

    pub fn signature(&self) -> String {
        schema::signature(&self.name, &self.props, None)
    }

    /// The short signature of an error's hint.
    pub fn hint(&self) -> String {
        schema::hint_signature(&self.name, &self.props)
    }
}

/// A titled part of the prompt's component list. A group lists its components in its own order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Group {
    pub name: String,
    pub components: Vec<String>,
    /// Lines printed under the group's components.
    pub notes: Vec<String>,
}

impl Group {
    pub fn new<S: Into<String>>(name: impl Into<String>, components: impl IntoIterator<Item = S>) -> Group {
        Group { name: name.into(), components: components.into_iter().map(Into::into).collect(), notes: Vec::new() }
    }

    pub fn with_notes<S: Into<String>>(mut self, notes: impl IntoIterator<Item = S>) -> Group {
        self.notes = notes.into_iter().map(Into::into).collect();
        self
    }
}

/// A function's body. It gets the arguments already checked against the params, and returns
/// null for "no value" or an error for what the spec reports as a `runtime-error`.
pub type FunctionImpl = fn(&Args<'_>) -> Result<Value, FnError>;

/// `@Name(...)`, a function a program computes with.
#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    /// In positional order.
    pub params: Vec<Field>,
    pub returns: Option<Schema>,
    /// None for a function read from a LibrarySpec, until [`Library::implement`] gives it one.
    pub run: Option<FunctionImpl>,
}

impl FunctionDef {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        params: impl IntoIterator<Item = Field>,
        run: FunctionImpl,
    ) -> FunctionDef {
        FunctionDef {
            name: name.into(),
            description: description.into(),
            params: params.into_iter().collect(),
            returns: None,
            run: Some(run),
        }
    }

    pub fn returns(mut self, schema: Schema) -> FunctionDef {
        self.returns = Some(schema);
        self
    }

    pub fn param(&self, name: &str) -> Option<&Field> {
        self.params.iter().find(|param| param.name == name)
    }

    pub fn signature(&self) -> String {
        schema::signature(&self.name, &self.params, self.returns.as_ref())
    }

    pub fn hint(&self) -> String {
        schema::hint_signature(&self.name, &self.params)
    }
}

/// `@Name(...)` in an action position, run by the host: `@CopyToClipboard`.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionDef {
    pub name: String,
    pub description: String,
    /// In positional order.
    pub params: Vec<Field>,
}

impl ActionDef {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        params: impl IntoIterator<Item = Field>,
    ) -> ActionDef {
        ActionDef { name: name.into(), description: description.into(), params: params.into_iter().collect() }
    }

    pub fn param(&self, name: &str) -> Option<&Field> {
        self.params.iter().find(|param| param.name == name)
    }

    pub fn signature(&self) -> String {
        schema::signature(&self.name, &self.params, None)
    }

    pub fn hint(&self) -> String {
        schema::hint_signature(&self.name, &self.params)
    }
}

/// What a function may read beyond its arguments: `@Now`, `@Random`, `@Pick`, `@Shuffle`, and the
/// local time zone of the dates functions.
pub trait Env {
    /// Milliseconds since the Unix epoch, as `Date.now()`.
    fn now(&self) -> f64;
    /// From 0 up to 1, as `Math.random()`.
    fn random(&self) -> f64;
    /// Minutes the local time is ahead of UTC at that instant, `-getTimezoneOffset()`.
    fn utc_offset_minutes(&self, at: f64) -> f64;
}

/// An [`Env`] that always answers the same, for tests.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FixedEnv {
    pub now: f64,
    pub random: f64,
    pub utc_offset_minutes: f64,
}

impl Env for FixedEnv {
    fn now(&self) -> f64 {
        self.now
    }

    fn random(&self) -> f64 {
        self.random
    }

    fn utc_offset_minutes(&self, _at: f64) -> f64 {
        self.utc_offset_minutes
    }
}

static NULL: Value = Value::Null;

/// A call's arguments by param name. A missing argument, or a `null` that skipped one, is null.
pub struct Args<'a> {
    params: &'a [Field],
    values: &'a [Value],
    env: &'a dyn Env,
}

impl<'a> Args<'a> {
    /// The arguments in the params' order.
    pub fn new(params: &'a [Field], values: &'a [Value], env: &'a dyn Env) -> Args<'a> {
        Args { params, values, env }
    }

    pub fn get(&self, name: &str) -> &'a Value {
        let Some(index) = self.params.iter().position(|param| param.name == name) else { return &NULL };
        self.values.get(index).unwrap_or(&NULL)
    }

    pub fn str(&self, name: &str) -> Option<&'a str> {
        self.get(name).as_str()
    }

    pub fn number(&self, name: &str) -> Option<f64> {
        self.get(name).as_number()
    }

    pub fn bool(&self, name: &str) -> Option<bool> {
        self.get(name).as_bool()
    }

    pub fn array(&self, name: &str) -> Option<&'a [Value]> {
        self.get(name).as_array()
    }

    pub fn object(&self, name: &str) -> Option<&'a Map> {
        self.get(name).as_object()
    }

    pub fn env(&self) -> &'a dyn Env {
        self.env
    }
}

/// A function that failed: the call is null and reports a `runtime-error` with the message.
#[derive(Debug, Clone, PartialEq)]
pub struct FnError {
    pub message: String,
}

impl FnError {
    pub fn new(message: impl Into<String>) -> FnError {
        FnError { message: message.into() }
    }
}

impl fmt::Display for FnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FnError {}

/// What an `@Name` calls, looked up as the spec says: the built-ins first.
#[derive(Debug, Clone, Copy)]
pub enum Call<'a> {
    Builtin(Builtin),
    Function(&'a FunctionDef),
    Action(&'a ActionDef),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LibraryError {
    /// Not an identifier with an uppercase first letter.
    InvalidName {
        kind: &'static str,
        name: String,
    },
    /// A keyword, `Query`, `Mutation`, `Action`, or a built-in's name.
    Reserved {
        kind: &'static str,
        name: String,
    },
    /// Taken by another component, function or action.
    Duplicate {
        kind: &'static str,
        name: String,
    },
    MissingRoot(String),
    UnknownGroupComponent {
        group: String,
        component: String,
    },
    RequiredAfterOptional {
        component: String,
        prop: String,
    },
    /// A schema refers to a component or an action the library doesn't have.
    UnknownReference {
        name: String,
        reference: String,
    },
    UnknownFunction(String),
    InvalidSpec(String),
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LibraryError::InvalidName { kind, name } => {
                write!(f, "The {kind} \"{name}\" must be an identifier that starts with an uppercase letter.")
            }
            LibraryError::Reserved { kind, name } => write!(f, "The {kind} \"{name}\" uses a name the language keeps."),
            LibraryError::Duplicate { kind, name } => {
                write!(f, "The {kind} \"{name}\" has the name of another component, function or action.")
            }
            LibraryError::MissingRoot(root) => write!(f, "The root component \"{root}\" is not in the library."),
            LibraryError::UnknownGroupComponent { group, component } => {
                write!(f, "The group \"{group}\" lists \"{component}\", which is not in the library.")
            }
            LibraryError::RequiredAfterOptional { component, prop } => {
                write!(f, "The required prop \"{prop}\" of \"{component}\" comes after an optional one.")
            }
            LibraryError::UnknownReference { name, reference } => {
                write!(f, "\"{name}\" refers to \"{reference}\", which is not in the library.")
            }
            LibraryError::UnknownFunction(name) => write!(f, "The function \"{name}\" is not in the library."),
            LibraryError::InvalidSpec(reason) => write!(f, "The LibrarySpec can't be read: {reason}"),
        }
    }
}

impl std::error::Error for LibraryError {}

/// What a library is made of, for [`Library::new`].
#[derive(Debug, Clone, Default)]
pub struct LibraryDef {
    pub id: Option<String>,
    pub version: Option<String>,
    pub root: Option<String>,
    pub components: Vec<ComponentDef>,
    pub groups: Vec<Group>,
    pub functions: Vec<FunctionDef>,
    pub actions: Vec<ActionDef>,
}

#[derive(Debug, Clone)]
pub struct Library {
    id: Option<String>,
    version: Option<String>,
    root: Option<String>,
    components: IndexMap<String, ComponentDef>,
    groups: Vec<Group>,
    functions: IndexMap<String, FunctionDef>,
    actions: IndexMap<String, ActionDef>,
}

impl Library {
    pub fn new(def: LibraryDef) -> Result<Library, LibraryError> {
        let mut names = HashSet::new();
        for component in &def.components {
            claim("component", &component.name, &mut names)?;
            check_prop_order(component)?;
        }
        for function in &def.functions {
            claim("function", &function.name, &mut names)?;
        }
        for action in &def.actions {
            claim("action", &action.name, &mut names)?;
        }
        let library = Library {
            id: def.id,
            version: def.version,
            root: def.root,
            components: def.components.into_iter().map(|component| (component.name.clone(), component)).collect(),
            groups: def.groups,
            functions: def.functions.into_iter().map(|function| (function.name.clone(), function)).collect(),
            actions: def.actions.into_iter().map(|action| (action.name.clone(), action)).collect(),
        };
        library.check_references()?;
        Ok(library)
    }

    fn check_references(&self) -> Result<(), LibraryError> {
        if let Some(root) = self.root.as_ref().filter(|root| !self.components.contains_key(*root)) {
            return Err(LibraryError::MissingRoot(root.clone()));
        }
        for group in &self.groups {
            let Some(component) = group.components.iter().find(|name| !self.components.contains_key(*name)) else {
                continue;
            };
            return Err(LibraryError::UnknownGroupComponent {
                group: group.name.clone(),
                component: component.clone(),
            });
        }
        let mut refs = Vec::new();
        let schemas = self.components.values().map(|component| (&component.name, &component.props, None));
        let schemas = schemas
            .chain(
                self.functions.values().map(|function| (&function.name, &function.params, function.returns.as_ref())),
            )
            .chain(self.actions.values().map(|action| (&action.name, &action.params, None)));
        for (name, fields, returns) in schemas {
            refs.clear();
            schema::object_json(fields, &mut refs);
            if let Some(returns) = returns {
                returns.to_json(&mut refs);
            }
            let Some(reference) = refs.iter().find(|reference| !self.defines(reference)) else { continue };
            return Err(LibraryError::UnknownReference { name: name.clone(), reference: reference.clone() });
        }
        Ok(())
    }

    /// Whether a `$ref` to the name resolves: a component, `ActionExpression` or an action.
    fn defines(&self, name: &str) -> bool {
        self.components.contains_key(name)
            || self.actions.contains_key(name)
            || name == ACTION_EXPRESSION
            || ACTION_REF_BUILTINS.contains(&name)
    }

    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    /// The component the entry rule falls back to without a `root` statement.
    pub fn root(&self) -> Option<&str> {
        self.root.as_deref()
    }

    pub fn component(&self, name: &str) -> Option<&ComponentDef> {
        self.components.get(name)
    }

    /// In the library's order, the prompt's when there are no groups.
    pub fn components(&self) -> impl Iterator<Item = &ComponentDef> {
        self.components.values()
    }

    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    pub fn function(&self, name: &str) -> Option<&FunctionDef> {
        self.functions.get(name)
    }

    pub fn functions(&self) -> impl Iterator<Item = &FunctionDef> {
        self.functions.values()
    }

    pub fn action(&self, name: &str) -> Option<&ActionDef> {
        self.actions.get(name)
    }

    pub fn actions(&self) -> impl Iterator<Item = &ActionDef> {
        self.actions.values()
    }

    /// What `@name(...)` calls: a built-in, then a custom function, then a custom action.
    pub fn call(&self, name: &str) -> Option<Call<'_>> {
        if let Some(builtin) = Builtin::from_name(name) {
            return Some(Call::Builtin(builtin));
        }
        if let Some(function) = self.functions.get(name) {
            return Some(Call::Function(function));
        }
        self.actions.get(name).map(Call::Action)
    }

    /// Gives a function read from a LibrarySpec its body.
    pub fn implement(&mut self, name: &str, run: FunctionImpl) -> Result<(), LibraryError> {
        let Some(function) = self.functions.get_mut(name) else {
            return Err(LibraryError::UnknownFunction(name.to_owned()));
        };
        function.run = Some(run);
        Ok(())
    }

    /// `library.toJSONSchema()`: every component in `$defs`, and every action a prop refers to, in
    /// the order Zod meets them.
    pub fn to_json_schema(&self) -> Value {
        let mut defs = Map::new();
        for name in self.components.keys() {
            self.define(name, &mut defs);
        }
        let mut out = Map::new();
        out.insert("$schema".into(), JSON_SCHEMA_DIALECT.into());
        out.insert("type".into(), "object".into());
        out.insert(
            "properties".into(),
            self.components
                .keys()
                .map(|name| (name.clone(), Value::from_iter([("$ref", Value::from(format!("#/$defs/{name}")))])))
                .collect(),
        );
        if !self.components.is_empty() {
            out.insert("required".into(), self.components.keys().map(|name| Value::from(name.as_str())).collect());
        }
        out.insert("additionalProperties".into(), false.into());
        if !defs.is_empty() {
            out.insert("$defs".into(), Value::Object(defs));
        }
        if !self.functions.is_empty() {
            let functions = self.functions.values().map(|function| {
                (function.name.clone(), call_json(&function.description, &function.params, function.returns.as_ref()))
            });
            out.insert("functions".into(), functions.collect());
        }
        if !self.actions.is_empty() {
            let actions = self
                .actions
                .values()
                .map(|action| (action.name.clone(), call_json(&action.description, &action.params, None)));
            out.insert("actions".into(), actions.collect());
        }
        Value::Object(out)
    }

    fn define(&self, name: &str, defs: &mut Map) {
        if defs.contains_key(name) {
            return;
        }
        defs.insert(name.to_owned(), Value::Null);
        let mut refs = Vec::new();
        let body = self.def_json(name, &mut refs);
        defs.insert(name.to_owned(), body);
        for reference in refs {
            self.define(&reference, defs);
        }
    }

    fn def_json(&self, name: &str, refs: &mut Vec<String>) -> Value {
        if let Some(component) = self.components.get(name) {
            let mut json = schema::object_json(&component.props, refs);
            if let Value::Object(map) = &mut json
                && !component.description.is_empty()
            {
                map.insert("description".into(), component.description.as_str().into());
            }
            return json;
        }
        if name == ACTION_EXPRESSION {
            return builtins::action_expression_schema().to_json(refs);
        }
        if let Some(schema) = builtins::builtin_action_ref_schema(name) {
            return schema.to_json(refs);
        }
        let Some(action) = self.actions.get(name) else { return Value::Object(Map::new()) };
        Schema::object([Schema::literal(name).field("type"), Schema::Object(action.params.clone()).field("params")])
            .to_json(refs)
    }

    /// `library.toSpec()`: the signatures and groups the prompt is written from, and the schema.
    pub fn to_spec(&self) -> Value {
        let mut out = Map::new();
        if let Some(root) = &self.root {
            out.insert("root".into(), root.as_str().into());
        }
        let components = self
            .components
            .values()
            .map(|component| (component.name.clone(), spec_entry(component.signature(), &component.description)));
        out.insert("components".into(), components.collect());
        if !self.groups.is_empty() {
            out.insert("componentGroups".into(), self.groups.iter().map(group_json).collect());
        }
        if !self.functions.is_empty() {
            let functions = self
                .functions
                .values()
                .map(|function| (function.name.clone(), spec_entry(function.signature(), &function.description)));
            out.insert("functions".into(), functions.collect());
        }
        if !self.actions.is_empty() {
            let actions = self
                .actions
                .values()
                .map(|action| (action.name.clone(), spec_entry(action.signature(), &action.description)));
            out.insert("actions".into(), actions.collect());
        }
        out.insert("schema".into(), self.to_json_schema());
        if let Some(id) = &self.id {
            out.insert("id".into(), id.as_str().into());
        }
        if let Some(version) = &self.version {
            out.insert("version".into(), version.as_str().into());
        }
        Value::Object(out)
    }

    pub fn from_spec_json(text: &str) -> Result<Library, LibraryError> {
        let spec = Value::from_json_str(text).map_err(|error| LibraryError::InvalidSpec(error.to_string()))?;
        Library::from_spec(&spec)
    }

    /// Reads a LibrarySpec (prompt.md §2). Positional order is the key order of the schema's
    /// `properties`; bindable props are the ones the signatures print as `$binding<…>`. Its
    /// functions have no bodies until [`Library::implement`] gives them one.
    pub fn from_spec(spec: &Value) -> Result<Library, LibraryError> {
        let invalid = |reason: &str| LibraryError::InvalidSpec(reason.to_owned());
        let schema = spec.get("schema").ok_or_else(|| invalid("it has no schema"))?;
        let defs = schema.get("$defs").and_then(Value::as_object);
        let actions = schema.get("actions").and_then(Value::as_object);
        let names: Vec<&str> = match schema.get("properties").and_then(Value::as_object) {
            Some(properties) => properties.keys().map(String::as_str).collect(),
            None => defs
                .into_iter()
                .flat_map(|defs| defs.keys())
                .map(String::as_str)
                .filter(|name| {
                    *name != ACTION_EXPRESSION
                        && !ACTION_REF_BUILTINS.contains(name)
                        && !actions.is_some_and(|actions| actions.contains_key(*name))
                })
                .collect(),
        };
        let refs = JsonRefs::new(schema).with_components(names.iter().copied());
        let entries = spec.get("components");
        let mut components = Vec::new();
        for name in &names {
            let def =
                defs.and_then(|defs| defs.get(*name)).ok_or_else(|| invalid(&format!("{name} has no $defs entry")))?;
            let entry = entries.and_then(|entries| entries.get(name));
            let signature = entry.and_then(|entry| entry.get("signature")).and_then(Value::as_str).unwrap_or_default();
            let mut props = schema::fields_from_json(def, &refs);
            for prop in &mut props {
                prop.bindable = binds(signature, &prop.name);
            }
            let description = def
                .get("description")
                .or_else(|| entry.and_then(|entry| entry.get("description")))
                .and_then(Value::as_str)
                .unwrap_or_default();
            components.push(ComponentDef::new(*name, description, props));
        }
        let calls = |key: &str| schema.get(key).and_then(Value::as_object).into_iter().flatten();
        let functions = calls("functions")
            .map(|(name, call)| FunctionDef {
                name: name.clone(),
                description: call.get("description").and_then(Value::as_str).unwrap_or_default().to_owned(),
                params: call.get("params").map(|params| schema::fields_from_json(params, &refs)).unwrap_or_default(),
                returns: call.get("returns").map(|returns| Schema::from_json(returns, &refs)),
                run: None,
            })
            .collect();
        let actions = calls("actions")
            .map(|(name, call)| ActionDef {
                name: name.clone(),
                description: call.get("description").and_then(Value::as_str).unwrap_or_default().to_owned(),
                params: call.get("params").map(|params| schema::fields_from_json(params, &refs)).unwrap_or_default(),
            })
            .collect();
        let text = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_owned);
        let strings = |value: Option<&Value>| -> Vec<String> {
            value
                .and_then(Value::as_array)
                .unwrap_or_default()
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        };
        let groups = spec
            .get("componentGroups")
            .and_then(Value::as_array)
            .unwrap_or_default()
            .iter()
            .map(|group| Group {
                name: text(group.get("name")).unwrap_or_default(),
                components: strings(group.get("components")),
                notes: strings(group.get("notes")),
            })
            .collect();
        Library::new(LibraryDef {
            id: text(spec.get("id")),
            version: text(spec.get("version")),
            root: text(spec.get("root")),
            components,
            groups,
            functions,
            actions,
        })
    }
}

fn claim(kind: &'static str, name: &str, names: &mut HashSet<String>) -> Result<(), LibraryError> {
    let mut chars = name.chars();
    let valid =
        chars.next().is_some_and(|c| c.is_ascii_uppercase()) && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        return Err(LibraryError::InvalidName { kind, name: name.to_owned() });
    }
    if builtins::is_reserved(name) || name == ACTION_EXPRESSION {
        return Err(LibraryError::Reserved { kind, name: name.to_owned() });
    }
    if !names.insert(name.to_owned()) {
        return Err(LibraryError::Duplicate { kind, name: name.to_owned() });
    }
    Ok(())
}

/// Required props come first; one with a default counts as optional.
fn check_prop_order(component: &ComponentDef) -> Result<(), LibraryError> {
    let mut optional = false;
    for prop in &component.props {
        let required = prop.required() && prop.default.is_none();
        if required && optional {
            return Err(LibraryError::RequiredAfterOptional {
                component: component.name.clone(),
                prop: prop.name.clone(),
            });
        }
        optional |= !required;
    }
    Ok(())
}

/// Whether a signature prints the prop as `$binding<…>`.
fn binds(signature: &str, prop: &str) -> bool {
    ["(", " "].iter().any(|before| {
        signature.contains(&format!("{before}{prop}: $binding<"))
            || signature.contains(&format!("{before}{prop}?: $binding<"))
    })
}

fn call_json(description: &str, params: &[Field], returns: Option<&Schema>) -> Value {
    let mut refs = Vec::new();
    let mut map = Map::new();
    map.insert("description".into(), description.into());
    map.insert("params".into(), schema::object_json(params, &mut refs));
    if let Some(returns) = returns {
        map.insert("returns".into(), returns.to_json(&mut refs));
    }
    Value::Object(map)
}

fn spec_entry(signature: String, description: &str) -> Value {
    Value::from_iter([("signature", Value::from(signature)), ("description", Value::from(description))])
}

fn group_json(group: &Group) -> Value {
    let mut map = Map::new();
    map.insert("name".into(), group.name.as_str().into());
    map.insert("components".into(), group.components.iter().map(|name| Value::from(name.as_str())).collect());
    if !group.notes.is_empty() {
        map.insert("notes".into(), group.notes.iter().map(|note| Value::from(note.as_str())).collect());
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> Value {
        let path = format!("{}/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
        Value::from_json_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn text(value: &Value) -> String {
        serde_json::to_string(value).unwrap()
    }

    fn at<'a>(value: &'a Value, path: &[&str]) -> &'a Value {
        path.iter().fold(value, |value, key| value.get(key).unwrap_or_else(|| panic!("no {key} in {path:?}")))
    }

    /// Part of open-intelligent-ui's library, written as its Zod schemas are.
    fn placeholder() -> Library {
        let spec = fixture("oiu/spec.json");
        let description = |name: &str| at(&spec, &["components", name, "description"]).as_str().unwrap().to_owned();
        let node = Schema::Any.named("Node");
        let children = Schema::array(node);
        let color = Schema::String.named("Color");
        let length = Schema::union([Schema::Number, Schema::String]).named("Length");
        let radius =
            Schema::union([Schema::enumeration(["sm", "md", "lg", "xl", "full"]), Schema::Number]).named("Radius");
        let align = Schema::enumeration(["start", "center", "end", "stretch"]);
        let option = Schema::union([
            Schema::String,
            Schema::object([Schema::String.field("value"), Schema::String.field("label")]),
        ]);
        let component = |name: &str, props: Vec<Field>| ComponentDef::new(name, description(name), props);
        let components = vec![
            component(
                "Col",
                vec![
                    children.clone().field("children"),
                    Schema::Number.field("gap").optional(),
                    align.clone().field("align").optional(),
                ],
            ),
            component(
                "Text",
                vec![
                    Schema::union([Schema::String, children.clone()]).field("text"),
                    Schema::enumeration(["xs", "sm", "md", "lg"]).field("size").optional(),
                    color.clone().field("color").optional(),
                    Schema::enumeration(["normal", "medium", "semibold"]).field("weight").optional(),
                    Schema::Boolean.field("italic").optional(),
                    Schema::Boolean.field("lineThrough").optional(),
                    Schema::Boolean.field("tabularNums").optional(),
                    Schema::Number.field("maxLines").optional(),
                ],
            ),
            component(
                "Button",
                vec![
                    Schema::String.field("label"),
                    Schema::Action.field("action").optional(),
                    Schema::enumeration(["solid", "outline", "soft", "ghost"]).field("variant").optional(),
                    Schema::enumeration(["primary", "danger", "success"]).field("color").optional(),
                    Schema::enumeration(["sm", "md", "lg"]).field("size").optional(),
                    Schema::String.field("icon").optional(),
                    Schema::Boolean.field("disabled").optional(),
                    Schema::Boolean.field("block").optional(),
                ],
            ),
            component(
                "Input",
                vec![
                    Schema::union([Schema::String, Schema::Number]).field("value").bindable(),
                    Schema::String.field("label").optional(),
                    Schema::enumeration(["text", "number"]).field("type").optional(),
                    Schema::String.field("placeholder").optional(),
                ],
            ),
            component(
                "Box",
                vec![
                    children.clone().field("children").optional(),
                    Schema::enumeration(["row", "col"]).field("direction").optional(),
                    Schema::Number.field("gap").optional(),
                    Schema::Number.field("padding").optional(),
                    color.field("background").optional(),
                    radius.field("radius").optional(),
                    Schema::Boolean.field("border").optional(),
                    length.clone().field("width").optional(),
                    length.field("height").optional(),
                    Schema::Number.field("flex").optional(),
                    align.field("align").optional(),
                    Schema::enumeration(["start", "center", "end", "between"]).field("justify").optional(),
                    Schema::Boolean.field("wrap").optional(),
                ],
            ),
            component(
                "Select",
                vec![
                    Schema::String.field("value").bindable(),
                    Schema::array(option).field("options"),
                    Schema::String.field("label").optional(),
                    Schema::String.field("placeholder").optional(),
                ],
            ),
            component(
                "Map",
                vec![
                    Schema::array(Schema::component("MapMarker")).field("markers"),
                    Schema::Boolean.field("route").optional(),
                    Schema::String.field("selected").optional().bindable(),
                    Schema::Number.field("height").optional(),
                ],
            ),
            component(
                "MapMarker",
                vec![
                    Schema::String.field("label"),
                    Schema::Number.field("lat"),
                    Schema::Number.field("lng"),
                    Schema::String.field("category").optional(),
                    Schema::String.field("icon").optional(),
                ],
            ),
            component(
                "Chart",
                vec![
                    Schema::enumeration(["bar", "line", "area", "pie", "donut", "scatter"]).field("type"),
                    Schema::array(Schema::record(Schema::union([Schema::String, Schema::Number]))).field("data"),
                    Schema::String.field("x").optional(),
                    Schema::array(Schema::String).field("series").optional(),
                    Schema::enumeration(["number", "currency", "percent"]).field("format").optional(),
                    Schema::String.field("currency").optional(),
                    Schema::Boolean.field("stacked").optional(),
                    Schema::Number.field("height").optional(),
                ],
            ),
        ];
        let function_description =
            |name: &str| at(&spec, &["functions", name, "description"]).as_str().unwrap().to_owned();
        let change = FunctionDef::new(
            "Change",
            function_description("Change"),
            [Schema::Number.field("current"), Schema::Number.field("previous")],
            |args| {
                let (Some(current), Some(previous)) = (args.number("current"), args.number("previous")) else {
                    return Ok(Value::Null);
                };
                if previous == 0.0 {
                    return Ok(Value::Null);
                }
                Ok(Value::Number((current - previous) / previous.abs() * 100.0))
            },
        )
        .returns(Schema::Number.nullable());
        let join = FunctionDef::new(
            "Join",
            function_description("Join"),
            [
                Schema::array(Schema::Any).field("list"),
                Schema::String.field("sep").optional(),
                Schema::String.field("field").optional(),
            ],
            |args| {
                let sep = args.str("sep").unwrap_or(", ");
                let items = args.array("list").unwrap_or_default().iter().filter(|item| !item.is_null());
                Ok(Value::from(items.map(crate::js::to_string).collect::<Vec<_>>().join(sep)))
            },
        )
        .returns(Schema::String);
        let copy = ActionDef::new(
            "CopyToClipboard",
            at(&spec, &["actions", "CopyToClipboard", "description"]).as_str().unwrap(),
            [Schema::String.field("text")],
        );
        Library::new(LibraryDef {
            root: Some("Col".into()),
            components,
            groups: vec![
                Group::new("Layout", ["Col", "Box"]).with_notes(["- gap counts 4px steps."]),
                Group::new("Text", ["Text"]),
            ],
            functions: vec![change, join],
            actions: vec![copy],
            ..LibraryDef::default()
        })
        .unwrap()
    }

    #[test]
    fn prints_signatures_as_the_reference() {
        let spec = fixture("oiu/spec.json");
        let library = placeholder();
        for component in library.components() {
            assert_eq!(
                component.signature(),
                at(&spec, &["components", &component.name, "signature"]).as_str().unwrap()
            );
        }
        for function in library.functions() {
            assert_eq!(function.signature(), at(&spec, &["functions", &function.name, "signature"]).as_str().unwrap());
        }
        let copy = library.action("CopyToClipboard").unwrap();
        assert_eq!(copy.signature(), at(&spec, &["actions", "CopyToClipboard", "signature"]).as_str().unwrap());
    }

    #[test]
    fn writes_the_reference_json_schema() {
        let spec = fixture("oiu/spec.json");
        let reference = at(&spec, &["schema"]);
        let schema = placeholder().to_json_schema();
        let keys: Vec<&String> = schema.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            ["$schema", "type", "properties", "required", "additionalProperties", "$defs", "functions", "actions"]
        );
        for key in ["$schema", "type", "additionalProperties"] {
            assert_eq!(text(at(&schema, &[key])), text(at(reference, &[key])));
        }
        assert_eq!(text(at(&schema, &["properties", "Map"])), text(at(reference, &["properties", "Map"])));
        let defs = at(&schema, &["$defs"]).as_object().unwrap();
        let names: Vec<&String> = defs.keys().collect();
        assert_eq!(
            names,
            ["Col", "Text", "Button", "ActionExpression", "Input", "Box", "Select", "Map", "MapMarker", "Chart"]
        );
        for (name, def) in defs {
            assert_eq!(text(def), text(at(reference, &["$defs", name])), "{name}");
        }
        for name in ["Change", "Join"] {
            assert_eq!(text(at(&schema, &["functions", name])), text(at(reference, &["functions", name])));
        }
        assert_eq!(text(at(&schema, &["actions"])), text(at(reference, &["actions"])));
    }

    #[test]
    fn writes_restricted_action_slots() {
        let library = Library::new(LibraryDef {
            components: vec![ComponentDef::new(
                "Share",
                "",
                [Schema::union([Schema::action_ref("CopyToClipboard"), Schema::action_ref("OpenUrl")]).field("share")],
            )],
            actions: vec![ActionDef::new("CopyToClipboard", "Copies", [Schema::String.field("text")])],
            ..LibraryDef::default()
        })
        .unwrap();
        let schema = library.to_json_schema();
        assert_eq!(
            text(at(&schema, &["$defs"])),
            concat!(
                r##"{"Share":{"type":"object","properties":{"share":{"anyOf":[{"$ref":"#/$defs/CopyToClipboard"},{"$ref":"#/$defs/OpenUrl"}]}},"required":["share"],"additionalProperties":false},"##,
                r#""CopyToClipboard":{"type":"object","properties":{"type":{"type":"string","const":"CopyToClipboard"},"params":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"],"additionalProperties":false}},"required":["type","params"],"additionalProperties":false},"#,
                r#""OpenUrl":{"type":"object","properties":{"type":{"type":"string","const":"open_url"},"url":{"type":"string"}},"required":["type","url"],"additionalProperties":false}}"#,
            )
        );
        assert_eq!(library.component("Share").unwrap().signature(), "Share(share: @CopyToClipboard | @OpenUrl)");
    }

    #[test]
    fn writes_the_spec_in_the_reference_order() {
        let spec = placeholder().to_spec();
        let keys: Vec<&String> = spec.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["root", "components", "componentGroups", "functions", "actions", "schema"]);
        assert_eq!(
            text(at(&spec, &["componentGroups"])),
            r#"[{"name":"Layout","components":["Col","Box"],"notes":["- gap counts 4px steps."]},{"name":"Text","components":["Text"]}]"#
        );
    }

    #[test]
    fn reads_its_own_spec_back() {
        let library = placeholder();
        let read = Library::from_spec(&library.to_spec()).unwrap();
        assert_eq!(text(&read.to_json_schema()), text(&library.to_json_schema()));
        assert_eq!(read.groups(), library.groups());
        assert_eq!(read.root(), Some("Col"));
        for name in ["Input", "Select", "Map", "Button", "MapMarker", "Chart"] {
            assert_eq!(read.component(name).unwrap().signature(), library.component(name).unwrap().signature());
        }
        assert!(read.function("Change").unwrap().run.is_none());
    }

    #[test]
    fn loads_the_conformance_library() {
        let spec = fixture("openui/1.0/library.json");
        let mut library = Library::from_spec(&spec).unwrap();
        assert_eq!(library.id(), Some("fixtures"));
        assert_eq!(library.version(), Some("1.0.0"));
        assert_eq!(library.root(), Some("Stack"));
        assert_eq!(library.components().count(), 12);
        for name in ["Text", "Tag", "Metric", "Button", "Input", "SelectItem", "Col"] {
            assert_eq!(
                library.component(name).unwrap().signature(),
                at(&spec, &["components", name, "signature"]).as_str().unwrap()
            );
        }
        assert_eq!(library.function("Upper").unwrap().signature(), "Upper(text: string) → string");
        assert_eq!(library.action("CopyToClipboard").unwrap().signature(), "CopyToClipboard(text: string)");

        let bindable = |component: &str, prop: &str| library.component(component).unwrap().prop(prop).unwrap().bindable;
        assert!(bindable("Input", "value") && bindable("Select", "value"));
        assert!(!bindable("Input", "name") && !bindable("SelectItem", "value"));

        let button = library.component("Button").unwrap();
        assert_eq!(
            button.prop("share").unwrap().schema,
            Schema::union([Schema::action_ref("CopyToClipboard"), Schema::action_ref("OpenUrl")])
        );
        assert_eq!(button.hint(), "Button(label*: string, action, variant: \"primary\"|\"secondary\", share)");
        let children = library.component("Stack").unwrap().prop("children").unwrap();
        assert_eq!(children.schema.items().and_then(Schema::component_slot).map(|slot| slot.len()), Some(12));
        let columns = library.component("Table").unwrap().prop("columns").unwrap();
        assert_eq!(columns.schema.items().and_then(Schema::component_slot), Some(vec!["Col"]));

        library
            .implement("Upper", |args| Ok(Value::from(args.str("text").unwrap_or_default().to_uppercase())))
            .unwrap();
        let Some(Call::Function(upper)) = library.call("Upper") else { panic!("Upper is not a function") };
        let values = [Value::from("shout")];
        let result = (upper.run.unwrap())(&Args::new(&upper.params, &values, &FixedEnv::default()));
        assert_eq!(result, Ok(Value::from("SHOUT")));
        assert!(matches!(library.call("Count"), Some(Call::Builtin(Builtin::Count))));
        assert!(matches!(library.call("CopyToClipboard"), Some(Call::Action(_))));
        assert!(library.call("Nope").is_none());
    }

    #[test]
    fn reads_arguments_by_name() {
        let library = placeholder();
        let join = library.function("Join").unwrap();
        let env = FixedEnv::default();
        let values = [Value::from_json_str(r#"["a", null, 2]"#).unwrap(), Value::Null];
        let args = Args::new(&join.params, &values, &env);
        assert_eq!(args.array("list").map(<[Value]>::len), Some(3));
        assert_eq!(args.get("sep"), &Value::Null);
        assert_eq!(args.get("field"), &Value::Null);
        assert_eq!(args.get("nope"), &Value::Null);
        assert_eq!((join.run.unwrap())(&args), Ok(Value::from("a, 2")));
        let change = library.function("Change").unwrap();
        let values = [Value::Number(120.0), Value::Number(100.0)];
        assert_eq!((change.run.unwrap())(&Args::new(&change.params, &values, &env)), Ok(Value::Number(20.0)));
    }

    #[test]
    fn refuses_what_create_library_refuses() {
        let text = || ComponentDef::new("Text", "", [Schema::String.field("text")]);
        let build = |def: LibraryDef| Library::new(def).err();
        let function = |name: &str| FunctionDef::new(name, "", [], |_| Ok(Value::Null));
        assert_eq!(
            build(LibraryDef { components: vec![text()], functions: vec![function("Count")], ..LibraryDef::default() }),
            Some(LibraryError::Reserved { kind: "function", name: "Count".into() })
        );
        assert_eq!(
            build(LibraryDef {
                components: vec![text()],
                actions: vec![ActionDef::new("Text", "", [])],
                ..LibraryDef::default()
            }),
            Some(LibraryError::Duplicate { kind: "action", name: "Text".into() })
        );
        assert_eq!(
            build(LibraryDef { components: vec![ComponentDef::new("Query", "", [])], ..LibraryDef::default() }),
            Some(LibraryError::Reserved { kind: "component", name: "Query".into() })
        );
        assert_eq!(
            build(LibraryDef { functions: vec![function("percent")], ..LibraryDef::default() }),
            Some(LibraryError::InvalidName { kind: "function", name: "percent".into() })
        );
        assert_eq!(
            build(LibraryDef { root: Some("Card".into()), components: vec![text()], ..LibraryDef::default() }),
            Some(LibraryError::MissingRoot("Card".into()))
        );
        assert_eq!(
            build(LibraryDef {
                components: vec![text()],
                groups: vec![Group::new("Media", ["Image"])],
                ..LibraryDef::default()
            }),
            Some(LibraryError::UnknownGroupComponent { group: "Media".into(), component: "Image".into() })
        );
        let late = ComponentDef::new("Late", "", [Schema::String.field("a").optional(), Schema::String.field("b")]);
        assert_eq!(
            build(LibraryDef { components: vec![late], ..LibraryDef::default() }),
            Some(LibraryError::RequiredAfterOptional { component: "Late".into(), prop: "b".into() })
        );
        let defaulted = ComponentDef::new(
            "Fine",
            "",
            [Schema::String.field("a").optional(), Schema::String.field("b").with_default("x")],
        );
        assert_eq!(build(LibraryDef { components: vec![defaulted], ..LibraryDef::default() }), None);
        let list = ComponentDef::new("List", "", [Schema::array(Schema::component("Item")).field("items")]);
        assert_eq!(
            build(LibraryDef { components: vec![list], ..LibraryDef::default() }),
            Some(LibraryError::UnknownReference { name: "List".into(), reference: "Item".into() })
        );
    }
}
