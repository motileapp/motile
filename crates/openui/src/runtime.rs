//! A rendered program, as a host drives it: text in, a tree out, user input and tool results in,
//! tool calls, timers and events out. Nothing here waits: every call returns the [`Effects`] the
//! host carries out, and results come back through [`Runtime::tool_result`] and
//! [`Runtime::timer`].

use std::collections::HashMap;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::element::{ActionStep, Element};
use crate::error::{Code, Error, Source};
use crate::eval::{self, Evaluator, Inputs, idle_mutation};
use crate::js;
use crate::library::{Env, Library};
use crate::node::{Dep, Node};
use crate::parser::{Parsed, Parser};
use crate::program::{Lowerer, Program, QueryDecl};
use crate::query::{Completion, Queries, QueryNode, Requests, Timer, ToolCall, ToolError};
use crate::rules;
use crate::store::{Field, Store};
use crate::value::{Map, Value};

/// How a library's components take part in forms (language.md §5.3), which the language leaves
/// to each library.
pub trait Forms {
    /// The form name a component gives the fields inside it, when it is a form.
    fn form(&self, element: &Element) -> Option<String>;
    /// The field a control keeps its value in when no `$variable` is bound to it.
    fn field(&self, element: &Element) -> Option<FieldSpec>;
}

/// A control's field.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldSpec {
    /// The key it is kept under in its form, or in the page.
    pub name: String,
    /// The prop that holds its value.
    pub prop: String,
    pub component_type: Option<String>,
    /// Its validation rules (§5.4).
    pub rules: Option<Value>,
}

/// No component is a form or a field.
pub struct NoForms;

impl Forms for NoForms {
    fn form(&self, _element: &Element) -> Option<String> {
        None
    }

    fn field(&self, _element: &Element) -> Option<FieldSpec> {
        None
    }
}

/// A step from a component to one inside it: a prop or an object's key, or an array's index.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Step {
    Key(String),
    Index(usize),
}

/// An event for the host (§6.3): `continue_conversation`, `open_url`, or a library action.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub kind: String,
    /// What the user reads: the message of `@ToAssistant`, empty for the others.
    pub message: String,
    pub params: Map,
    /// The form around the component that triggered it.
    pub form_name: Option<String>,
    /// The form's fields, or the page's variables and fields, when the action ran.
    pub form_state: Value,
}

impl Event {
    /// `{type, humanFriendlyMessage, params, formName?, formState}`.
    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("type".into(), self.kind.as_str().into());
        map.insert("humanFriendlyMessage".into(), self.message.as_str().into());
        map.insert("params".into(), Value::Object(self.params.clone()));
        if let Some(form) = &self.form_name {
            map.insert("formName".into(), form.as_str().into());
        }
        map.insert("formState".into(), self.form_state.clone());
        Value::Object(map)
    }
}

/// What the host does after a call.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Effects {
    /// The tree changed: draw [`Runtime::root`] again.
    pub render: bool,
    /// The error list changed, to this.
    pub errors: Option<Vec<Error>>,
    /// Tool calls to make, each answered with [`Runtime::tool_result`].
    pub calls: Vec<ToolCall>,
    /// Intervals to keep, each firing [`Runtime::timer`].
    pub timers: Vec<Timer>,
    /// Intervals to stop.
    pub cancel: Vec<u64>,
    pub events: Vec<Event>,
    /// The user or an action changed the state: save [`Runtime::snapshot`].
    pub state: bool,
}

impl Effects {
    fn absorb(&mut self, requests: Requests) {
        self.calls.extend(requests.calls);
        self.timers.extend(requests.timers);
        self.cancel.extend(requests.cancel);
    }

    /// Adds what a later call asked for.
    pub fn merge(&mut self, other: Effects) {
        self.render |= other.render;
        if other.errors.is_some() {
            self.errors = other.errors;
        }
        self.calls.extend(other.calls);
        self.timers.extend(other.timers);
        self.cancel.extend(other.cancel);
        self.events.extend(other.events);
        self.state |= other.state;
    }
}

/// The rest of a plan, waiting for its mutation.
#[derive(Debug)]
struct Continuation {
    steps: Vec<ActionStep>,
    form: Option<String>,
}

pub struct Runtime {
    library: Arc<Library>,
    env: Arc<dyn Env + Send + Sync>,
    forms: Arc<dyn Forms + Send + Sync>,
    parser: Parser,
    lowerer: Lowerer,
    program: Program,
    store: Store,
    queries: Queries,
    cache: eval::Cache,
    root: Option<Value>,
    runtime_errors: Vec<Error>,
    emitted: Vec<Error>,
    streaming: bool,
    plans: HashMap<u64, Continuation>,
    /// The defaults of the queries that have not run yet.
    defaults: HashMap<String, Value>,
    defaults_version: u64,
    field_errors: IndexMap<(Option<String>, String), String>,
}

struct RuntimeInputs<'a> {
    store: &'a Store,
    queries: &'a Queries,
    defaults: &'a HashMap<String, Value>,
    defaults_version: u64,
    /// The user's value, `$value`, while a binding's write is evaluated.
    value: Option<&'a Value>,
}

impl Inputs for RuntimeInputs<'_> {
    fn state(&self, name: &str) -> Value {
        if name == "$value"
            && let Some(value) = self.value
        {
            return value.clone();
        }
        self.store.get(name).cloned().unwrap_or_default()
    }

    fn query(&self, statement: &str) -> Value {
        self.queries.result(statement).or_else(|| self.defaults.get(statement).cloned()).unwrap_or_default()
    }

    fn mutation(&self, statement: &str) -> Value {
        self.queries.mutation(statement).unwrap_or_else(idle_mutation)
    }

    fn version(&self, dep: &Dep) -> u64 {
        match dep {
            Dep::State(name) => self.store.version(name),
            Dep::Query(statement) => self.queries.version(statement) + self.defaults_version,
            Dep::Mutation(statement) => self.queries.version(statement),
        }
    }
}

impl Runtime {
    pub fn new(library: Arc<Library>, env: Arc<dyn Env + Send + Sync>, forms: Arc<dyn Forms + Send + Sync>) -> Runtime {
        Runtime {
            library,
            env,
            forms,
            parser: Parser::new(),
            lowerer: Lowerer::default(),
            program: Program::default(),
            store: Store::default(),
            queries: Queries::default(),
            cache: eval::Cache::default(),
            root: None,
            runtime_errors: Vec::new(),
            emitted: Vec::new(),
            streaming: false,
            plans: HashMap::new(),
            defaults: HashMap::new(),
            defaults_version: 0,
            field_errors: IndexMap::new(),
        }
    }

    /// Adds a chunk of the response; `complete` once the stream has ended.
    pub fn push(&mut self, chunk: &str, complete: bool) -> Effects {
        let parsed = self.parser.push(chunk, complete);
        self.load(parsed)
    }

    /// Sets the whole response so far. A text that doesn't extend the previous one, such as an
    /// edited program, is parsed again from the start; the state stays.
    pub fn set_text(&mut self, text: &str, complete: bool) -> Effects {
        let parsed = self.parser.set(text, complete);
        self.load(parsed)
    }

    /// The text received so far.
    pub fn text(&self) -> &str {
        self.parser.text()
    }

    /// The rendered tree: the entry's component with every prop evaluated.
    pub fn root(&self) -> Option<&Arc<Element>> {
        match &self.root {
            Some(Value::Element(element)) => Some(element),
            _ => None,
        }
    }

    pub fn program(&self) -> &Program {
        &self.program
    }

    pub fn is_streaming(&self) -> bool {
        self.streaming
    }

    /// The errors last reported (§8.3).
    pub fn errors(&self) -> &[Error] {
        &self.emitted
    }

    /// A `$variable`'s value.
    pub fn state(&self, name: &str) -> Option<&Value> {
        self.store.get(name)
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The queries fetching now, and those fetching again with a result showing.
    pub fn loading(&self) -> (Vec<&str>, Vec<&str>) {
        (self.queries.loading(), self.queries.refetching())
    }

    /// Everything the user and the actions set, to restore with [`Runtime::restore`].
    pub fn snapshot(&self) -> Value {
        self.store.snapshot()
    }

    /// Applies saved state over the declared defaults.
    pub fn restore(&mut self, snapshot: &Value) -> Effects {
        let mut effects = Effects::default();
        let changed = self.store.restore(snapshot);
        self.state_changed(&changed, &mut effects);
        self.render(&mut effects);
        effects
    }

    fn load(&mut self, parsed: Parsed) -> Effects {
        let mut effects = Effects::default();
        if !parsed.complete && !self.streaming && !self.emitted.is_empty() {
            self.emitted.clear();
            effects.errors = Some(Vec::new());
        }
        self.streaming = !parsed.complete;
        self.program = Program::build(&self.library, &parsed, &mut self.lowerer);
        self.initialize_state();
        self.update_defaults();
        if parsed.complete {
            self.sync_calls(&mut effects);
        }
        self.render(&mut effects);
        effects
    }

    fn inputs(&self) -> RuntimeInputs<'_> {
        RuntimeInputs {
            store: &self.store,
            queries: &self.queries,
            defaults: &self.defaults,
            defaults_version: self.defaults_version,
            value: None,
        }
    }

    fn evaluate(&self, node: &Node, statement: Option<&str>) -> Value {
        let inputs = self.inputs();
        Evaluator::new(&self.library, self.env.as_ref(), &inputs).in_statement(statement).eval(node, None)
    }

    fn initialize_state(&mut self) {
        let mut defaults = IndexMap::new();
        for (name, node) in &self.program.state {
            defaults.insert(name.clone(), self.evaluate(node, Some(name)));
        }
        self.store.initialize(&defaults);
    }

    /// The defaults of the queries that have not run, which their references read meanwhile.
    fn update_defaults(&mut self) {
        let mut defaults = HashMap::new();
        for query in self.program.queries.iter().filter(|query| !self.queries.has_query(&query.statement)) {
            defaults.insert(query.statement.clone(), self.evaluate(&query.defaults, Some(&query.statement)));
        }
        if defaults != self.defaults {
            self.defaults = defaults;
            self.defaults_version += 1;
        }
    }

    /// Once the stream has ended: the mutations registered, every query fetched.
    fn sync_calls(&mut self, effects: &mut Effects) {
        let mutations: Vec<(String, String)> = self
            .program
            .mutations
            .iter()
            .map(|mutation| {
                (mutation.statement.clone(), tool_name(&self.evaluate(&mutation.tool, Some(&mutation.statement))))
            })
            .collect();
        self.queries.register_mutations(&mutations);
        let mut requests = Requests::default();
        let statements: Vec<&str> = self.program.queries.iter().map(|query| query.statement.as_str()).collect();
        self.queries.retain(&statements, &mut requests);
        let nodes: Vec<QueryNode> = self.program.queries.iter().map(|query| self.query_node(query)).collect();
        for node in nodes {
            self.queries.update(node, &mut requests);
        }
        effects.absorb(requests);
        self.update_defaults();
    }

    fn query_node(&self, query: &QueryDecl) -> QueryNode {
        let statement = Some(query.statement.as_str());
        let deps = if query.deps.is_empty() {
            Value::Null
        } else {
            Value::Object(
                query
                    .deps
                    .iter()
                    .map(|name| (name.clone(), self.store.get(name).cloned().unwrap_or_default()))
                    .collect(),
            )
        };
        QueryNode {
            statement: query.statement.clone(),
            tool: tool_name(&self.evaluate(&query.tool, statement)),
            args: self.evaluate(&query.args, statement),
            defaults: self.evaluate(&query.defaults, statement),
            refresh: query.refresh.as_ref().map_or(0.0, |refresh| js::to_number(&self.evaluate(refresh, statement))),
            deps,
        }
    }

    /// After variables changed: the queries that name one in their arguments fetch again.
    fn state_changed(&mut self, changed: &[String], effects: &mut Effects) {
        if changed.is_empty() || self.streaming {
            return;
        }
        let nodes: Vec<QueryNode> = self
            .program
            .queries
            .iter()
            .filter(|query| query.deps.iter().any(|dep| changed.contains(dep)))
            .map(|query| self.query_node(query))
            .collect();
        let mut requests = Requests::default();
        for node in nodes {
            self.queries.update(node, &mut requests);
        }
        effects.absorb(requests);
    }

    /// Evaluates the tree again and reports the errors when they changed.
    fn render(&mut self, effects: &mut Effects) {
        let inputs = RuntimeInputs {
            store: &self.store,
            queries: &self.queries,
            defaults: &self.defaults,
            defaults_version: self.defaults_version,
            value: None,
        };
        let mut evaluator = Evaluator::new(&self.library, self.env.as_ref(), &inputs)
            .with_cache(&mut self.cache)
            .in_statement(self.program.entry.as_deref());
        let value = self.program.root.as_ref().map(|root| evaluator.eval(root, None));
        let mut errors = std::mem::take(&mut evaluator.errors);
        drop(evaluator);
        self.cache.sweep();
        let root = match value {
            Some(element @ Value::Element(_)) => Some(element),
            Some(_) => {
                let error = Error::new(Source::Runtime, Code::NoRoot, "root does not evaluate to a component")
                    .statement(Some("root"))
                    .hint("Make root's value a component in every case");
                errors.push(error);
                None
            }
            None => None,
        };
        let changed = match (&self.root, &root) {
            (Some(before), Some(after)) => !same_tree(before, after),
            (before, after) => before.is_some() != after.is_some(),
        };
        if changed {
            self.root = root;
            effects.render = true;
        }
        self.runtime_errors = errors;
        if self.streaming {
            return;
        }
        let mut errors = Vec::new();
        if self.program.has_text && self.program.count == 0 {
            let message = "Response could not be parsed as valid openui-lang";
            let root = self.library.root().unwrap_or("Root");
            let hint = format!("The entire response must be valid openui-lang code starting with root = {root}(...)");
            errors.push(Error::new(Source::Parser, Code::ParseFailed, message).hint(hint));
        }
        errors.extend(self.program.errors.iter().cloned());
        errors.extend(self.runtime_errors.iter().cloned());
        errors.extend(self.queries.errors());
        if errors != self.emitted {
            self.emitted = errors.clone();
            effects.errors = Some(errors);
        }
    }

    /// The component at `path` from the root, and the form around it.
    fn locate(&self, path: &[Step]) -> Option<(Arc<Element>, Option<String>)> {
        let mut current = self.root.as_ref()?;
        let mut form = current.as_element().and_then(|element| self.forms.form(element));
        for step in path {
            current = match (step, current) {
                (Step::Key(key), Value::Element(element)) => element.props.get(key)?,
                (Step::Key(key), Value::Object(map)) => map.get(key)?,
                (Step::Index(index), Value::Array(items)) => items.get(*index)?,
                _ => return None,
            };
            if let Some(name) = current.as_element().and_then(|element| self.forms.form(element)) {
                form = Some(name);
            }
        }
        let Value::Element(element) = current else { return None };
        Some((element.clone(), form))
    }

    /// The path to the first component of a statement.
    pub fn find(&self, statement: &str) -> Option<Vec<Step>> {
        fn search(value: &Value, statement: &str, path: &mut Vec<Step>) -> bool {
            match value {
                Value::Element(element) if element.statement.as_deref() == Some(statement) => true,
                Value::Element(element) => element.props.iter().any(|(key, value)| {
                    path.push(Step::Key(key.clone()));
                    search(value, statement, path) || path.pop().is_none()
                }),
                Value::Array(items) => items.iter().enumerate().any(|(index, value)| {
                    path.push(Step::Index(index));
                    search(value, statement, path) || path.pop().is_none()
                }),
                Value::Object(map) => map.iter().any(|(key, value)| {
                    path.push(Step::Key(key.clone()));
                    search(value, statement, path) || path.pop().is_none()
                }),
                _ => false,
            }
        }
        let mut path = Vec::new();
        search(self.root.as_ref()?, statement, &mut path).then_some(path)
    }

    /// What a control shows: its bound variable, else the value the user gave its field, else
    /// its prop.
    pub fn value_at(&self, path: &[Step], prop: &str) -> Option<Value> {
        let (element, form) = self.locate(path)?;
        if let Some(binding) = element.bindings.get(prop) {
            return self.store.get(&binding.variable).cloned();
        }
        let field = self.forms.field(&element).filter(|field| field.prop == prop);
        let stored =
            field.and_then(|field| self.store.field(form.as_deref(), &field.name).map(|field| field.value.clone()));
        stored.or_else(|| element.props.get(prop).cloned())
    }

    /// The validation error a control's field shows.
    pub fn error_at(&self, path: &[Step]) -> Option<&str> {
        let (element, form) = self.locate(path)?;
        let field = self.forms.field(&element)?;
        self.field_errors.get(&(form, field.name)).map(String::as_str)
    }

    /// The user changed a control's value: a bound variable, or the control's field.
    pub fn input(&mut self, path: &[Step], prop: &str, value: Value) -> Effects {
        let mut effects = Effects::default();
        let Some((element, form)) = self.locate(path) else { return effects };
        let mut changed = Vec::new();
        if let Some(binding) = element.bindings.get(prop) {
            let next = match &binding.write {
                Some(write) => {
                    let inputs = RuntimeInputs { value: Some(&value), ..self.inputs() };
                    Evaluator::new(&self.library, self.env.as_ref(), &inputs).eval(write, None)
                }
                None => value.clone(),
            };
            if self.store.set(&binding.variable, next, false) {
                changed.push(binding.variable.clone());
            }
        } else if let Some(field) = self.forms.field(&element).filter(|field| field.prop == prop) {
            let stored = Field { value: value.clone(), component_type: field.component_type.clone() };
            effects.state |= self.store.set_field(form.as_deref(), &field.name, stored);
        }
        if let Some(field) = self.forms.field(&element).filter(|field| field.prop == prop) {
            let error = field.rules.as_ref().and_then(|rules| rules::validate(&value, rules));
            let key = (form, field.name);
            let before = self.field_errors.get(&key).cloned();
            match error.clone() {
                Some(error) => self.field_errors.insert(key, error),
                None => self.field_errors.shift_remove(&key),
            };
            effects.render |= before != error;
        }
        effects.state |= !changed.is_empty();
        self.state_changed(&changed, &mut effects);
        self.render(&mut effects);
        effects
    }

    /// Checks every field of the form around the control at `path`, or of the page outside
    /// forms: whether all pass. Their errors show at [`Runtime::error_at`].
    pub fn validate_form(&mut self, path: &[Step]) -> bool {
        let Some((_, form)) = self.locate(path) else { return true };
        let Some(root) = self.root.clone() else { return true };
        let mut fields = Vec::new();
        self.collect_fields(&root, None, form.as_deref(), &mut fields);
        let mut valid = true;
        for (element, field) in fields {
            let value = match element.bindings.get(&field.prop) {
                Some(binding) => self.store.get(&binding.variable).cloned().unwrap_or_default(),
                None => self
                    .store
                    .field(form.as_deref(), &field.name)
                    .map(|field| field.value.clone())
                    .or_else(|| element.props.get(&field.prop).cloned())
                    .unwrap_or_default(),
            };
            let key = (form.clone(), field.name.clone());
            match field.rules.as_ref().and_then(|rules| rules::validate(&value, rules)) {
                Some(error) => {
                    valid = false;
                    self.field_errors.insert(key, error);
                }
                None => {
                    self.field_errors.shift_remove(&key);
                }
            }
        }
        valid
    }

    fn collect_fields(
        &self,
        value: &Value,
        form: Option<&str>,
        target: Option<&str>,
        out: &mut Vec<(Arc<Element>, FieldSpec)>,
    ) {
        match value {
            Value::Element(element) => {
                let own = self.forms.form(element);
                let form = own.as_deref().or(form);
                if form == target
                    && let Some(field) = self.forms.field(element)
                {
                    out.push((element.clone(), field));
                }
                element.props.values().for_each(|value| self.collect_fields(value, form, target, out));
            }
            Value::Array(items) => items.iter().for_each(|value| self.collect_fields(value, form, target, out)),
            Value::Object(map) => map.values().for_each(|value| self.collect_fields(value, form, target, out)),
            _ => {}
        }
    }

    /// The user pressed a control: runs the action plan in its prop.
    pub fn press(&mut self, path: &[Step], prop: &str) -> Effects {
        let mut effects = Effects::default();
        let Some((element, form)) = self.locate(path) else { return effects };
        let Some(Value::Plan(plan)) = element.props.get(prop) else { return effects };
        let mut changed = Vec::new();
        self.run(plan.steps.clone(), form, &mut changed, &mut effects);
        effects.state |= !changed.is_empty();
        self.state_changed(&changed, &mut effects);
        self.render(&mut effects);
        effects
    }

    /// A control's default action: sends the message to the assistant with the form state.
    pub fn ask(&mut self, path: &[Step], message: &str) -> Effects {
        let mut effects = Effects::default();
        let Some((_, form)) = self.locate(path) else { return effects };
        let step = ActionStep::ToAssistant { message: message.to_owned(), context: None };
        self.run(vec![step], form, &mut Vec::new(), &mut effects);
        effects
    }

    /// Runs steps in order. A mutation is awaited: the steps after it wait for its result, and
    /// do not run when it fails.
    fn run(&mut self, steps: Vec<ActionStep>, form: Option<String>, changed: &mut Vec<String>, effects: &mut Effects) {
        for (index, step) in steps.iter().enumerate() {
            let event = |kind: &str, message: &str, params: Map, store: &Store| Event {
                kind: kind.to_owned(),
                message: message.to_owned(),
                params,
                form_name: form.clone(),
                form_state: store.form_state(form.as_deref()),
            };
            match step {
                ActionStep::Run { statement, mutation: true } => {
                    if self.streaming {
                        return;
                    }
                    let args = match self.program.mutations.iter().find(|mutation| mutation.statement == *statement) {
                        Some(mutation) => self.evaluate(&mutation.args, Some(statement)),
                        None => return,
                    };
                    let mut requests = Requests::default();
                    let fired = self.queries.fire(statement, args, &mut requests);
                    effects.absorb(requests);
                    let Some(call) = fired else { return };
                    self.plans.insert(call, Continuation { steps: steps[index + 1..].to_vec(), form: form.clone() });
                    return;
                }
                ActionStep::Run { statement, mutation: false } => {
                    if !self.streaming {
                        let mut requests = Requests::default();
                        self.queries.invalidate(statement, &mut requests);
                        effects.absorb(requests);
                    }
                }
                ActionStep::ToAssistant { message, context } => {
                    let mut params = Map::new();
                    if let Some(context) = context {
                        params.insert("context".into(), context.clone());
                    }
                    effects.events.push(event("continue_conversation", message, params, &self.store));
                }
                ActionStep::OpenUrl { url } => {
                    let params = Map::from_iter([("url".to_owned(), Value::from(url.as_str()))]);
                    effects.events.push(event("open_url", "", params, &self.store));
                }
                ActionStep::Custom { name, params } => {
                    effects.events.push(event(name, "", params.clone(), &self.store));
                }
                ActionStep::Set { variable, value } => {
                    let value = self.evaluate(value, None);
                    if self.store.set(variable, value, false) {
                        changed.push(variable.clone());
                    }
                }
                ActionStep::Reset { variables } => {
                    for variable in variables {
                        let default = match self.program.state.get(variable) {
                            Some(node) => self.evaluate(node, Some(variable)),
                            None => Value::Null,
                        };
                        if self.store.set(variable, default, true) {
                            changed.push(variable.clone());
                        }
                    }
                }
            }
        }
    }

    /// A tool call's result. A mutation's lets the plan that ran it go on.
    pub fn tool_result(&mut self, call: u64, result: Result<Value, ToolError>) -> Effects {
        let mut effects = Effects::default();
        let mut requests = Requests::default();
        let completion = self.queries.complete(call, result, &mut requests);
        effects.absorb(requests);
        let continuation = self.plans.remove(&call);
        if let (Completion::Mutation { ok: true, .. }, Some(continuation)) = (completion, continuation) {
            let mut changed = Vec::new();
            self.run(continuation.steps, continuation.form, &mut changed, &mut effects);
            effects.state |= !changed.is_empty();
            self.state_changed(&changed, &mut effects);
        }
        self.render(&mut effects);
        effects
    }

    /// A refresh interval elapsed.
    pub fn timer(&mut self, timer: u64) -> Effects {
        let mut effects = Effects::default();
        let mut requests = Requests::default();
        self.queries.timer(timer, &mut requests);
        effects.absorb(requests);
        effects
    }

    /// Fetches the failed queries again.
    pub fn retry(&mut self) -> Effects {
        let mut effects = Effects::default();
        let mut requests = Requests::default();
        for statement in self.queries.failed() {
            self.queries.invalidate(&statement, &mut requests);
        }
        effects.absorb(requests);
        effects
    }
}

/// Equal trees, without comparing what both share.
fn same_tree(a: &Value, b: &Value) -> bool {
    let same_map = |a: &Map, b: &Map| {
        a.len() == b.len() && a.iter().zip(b).all(|((key_a, a), (key_b, b))| key_a == key_b && same_tree(a, b))
    };
    match (a, b) {
        (Value::Element(a), Value::Element(b)) => {
            Arc::ptr_eq(a, b)
                || (a.component == b.component
                    && a.statement == b.statement
                    && a.partial == b.partial
                    && a.bindings == b.bindings
                    && same_map(&a.props, &b.props))
        }
        (Value::Array(a), Value::Array(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_tree(a, b)),
        (Value::Object(a), Value::Object(b)) => same_map(a, b),
        (Value::Plan(a), Value::Plan(b)) => Arc::ptr_eq(a, b) || a == b,
        (a, b) => a == b,
    }
}

/// A tool's name, as the program computes it.
fn tool_name(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        value => js::to_string(value),
    }
}
