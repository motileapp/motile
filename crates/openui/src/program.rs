//! Materializing a parse against the library (language.md §2, §3.6, §8): the entry, references
//! resolved where they are written, components checked and their arguments mapped to props,
//! function and action arguments checked when they are literal, the errors, the unresolved
//! names and the orphans.
//!
//! A statement referenced from two places is lowered once and shared. While streaming, a
//! statement whose lowering read only completed statements is kept for the next chunk.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use indexmap::IndexMap;

use crate::ast::{Expr, Kind, Statement};
use crate::builtins::Builtin;
use crate::check::{Checker, Fallback, fallback};
use crate::element::{ActionPlan, ActionStep};
use crate::error::{Code, Error, Source};
use crate::eval;
use crate::library::{ComponentDef, Library};
use crate::node::{ElementNode, Node};
use crate::parser::{self, Parsed};
use crate::schema::Field;
use crate::value::Value;

const ROOT: &str = "root";

/// A parse, materialized.
#[derive(Debug, Clone, Default)]
pub struct Program {
    /// The entry's value: a component, or an expression that picks one (`$t ? A : B`).
    pub root: Option<Node>,
    pub entry: Option<String>,
    /// The parser's errors (`no-root` included), each once.
    pub errors: Vec<Error>,
    /// References to missing statements, and those that close a cycle.
    pub unresolved: Vec<String>,
    /// Value statements the entry cannot reach.
    pub orphans: Vec<String>,
    /// The text ended inside an unclosed string or bracket.
    pub incomplete: bool,
    /// The stream has ended.
    pub complete: bool,
    /// Every statement read.
    pub count: usize,
    pub has_text: bool,
    /// The state declarations and the `$names` used without one, with their defaults.
    pub state: IndexMap<String, Node>,
    pub queries: Vec<QueryDecl>,
    pub mutations: Vec<MutationDecl>,
}

/// `name = Query(tool, args, defaults, refreshSeconds?)`.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryDecl {
    pub statement: String,
    pub tool: Node,
    pub args: Node,
    pub defaults: Node,
    pub refresh: Option<Node>,
    /// The `$variables` written in its arguments: a change fetches again.
    pub deps: Vec<String>,
}

/// `name = Mutation(tool, args)`.
#[derive(Debug, Clone, PartialEq)]
pub struct MutationDecl {
    pub statement: String,
    pub tool: Node,
    pub args: Node,
}

impl Program {
    /// Parses and materializes a whole text.
    pub fn parse(library: &Library, text: &str) -> Program {
        Program::build(library, &parser::parse(text), &mut Lowerer::default())
    }

    pub fn build(library: &Library, parsed: &Parsed, lowerer: &mut Lowerer) -> Program {
        lowerer.pass += 1;
        let mut pass = Pass {
            library,
            parsed,
            cache: &mut lowerer.cache,
            pass: lowerer.pass,
            partial: !parsed.complete,
            errors: Vec::new(),
            unresolved: Vec::new(),
            reached: Vec::new(),
            touched: Vec::new(),
            visiting: Vec::new(),
            scope: Vec::new(),
            cycles: 0,
            statement: None,
        };
        let entry = pick_entry(parsed, library.root());
        let root = entry.as_ref().map(|(name, _)| pass.resolve(name, Position::Value)).filter(Node::contains_element);
        let state = pass.state();
        let (queries, mutations) = pass.calls();

        pass.statement = entry.as_ref().map(|(name, _)| name.clone());
        let component = library.root().unwrap_or_default();
        match &entry {
            None => {
                let message = if component.is_empty() {
                    "No root statement".to_owned()
                } else {
                    format!("No root statement, and no statement calls the root component {component}")
                };
                pass.no_root(component, message, false);
            }
            Some((name, _)) if root.is_none() && !resolves_to_component(parsed, name) => {
                pass.no_root(component, "root does not evaluate to a component".to_owned(), false);
            }
            Some((_, true)) => {
                let message = format!(
                    "No root statement. Rendering the first statement that calls the root component {component}"
                );
                pass.no_root(component, message, root.is_some());
            }
            Some(_) => {}
        }

        let reached: HashSet<&str> = pass.reached.iter().map(String::as_str).collect();
        let orphans = match &entry {
            Some((entry, _)) => parsed
                .statements
                .values()
                .filter(|statement| statement.kind == Kind::Value && statement.name != *entry)
                .filter(|statement| !reached.contains(statement.name.as_str()))
                .map(|statement| statement.name.clone())
                .collect(),
            None => Vec::new(),
        };
        let mut unresolved = Vec::new();
        for name in pass.unresolved {
            if !unresolved.contains(&name) {
                unresolved.push(name);
            }
        }
        let mut errors = Vec::new();
        for error in pass.errors {
            if !errors.contains(&error) {
                errors.push(error);
            }
        }
        let used = lowerer.pass;
        lowerer.cache.retain(|_, cached| cached.used == used);
        Program {
            root,
            entry: entry.map(|(name, _)| name),
            errors,
            unresolved,
            orphans,
            incomplete: parsed.incomplete,
            complete: parsed.complete,
            count: parsed.count,
            has_text: parsed.has_text,
            state,
            queries,
            mutations,
        }
    }
}

/// `root`, else the first value statement that calls the library's root component, with whether
/// it is that fallback.
fn pick_entry(parsed: &Parsed, root: Option<&str>) -> Option<(String, bool)> {
    if parsed.statements.contains_key(ROOT) {
        return Some((ROOT.to_owned(), false));
    }
    let root = root?;
    parsed
        .statements
        .values()
        .find(|statement| {
            statement.kind == Kind::Value
                && matches!(&statement.expr, Expr::Call { name, at: false, .. } if name == root)
        })
        .map(|statement| (statement.name.clone(), true))
}

/// Whether the statement's value is a component call, through references: one that validation
/// dropped has reported its own error.
fn resolves_to_component(parsed: &Parsed, name: &str) -> bool {
    let mut seen = HashSet::new();
    let mut expr = parsed.statement(name).map(|statement| &statement.expr);
    while let Some(Expr::Ref(name)) = expr {
        if !seen.insert(name) {
            return false;
        }
        expr = parsed.statement(name).map(|statement| &statement.expr);
    }
    matches!(expr, Some(Expr::Call { name, at: false, .. }) if !is_reserved_call(name) && name != "Action")
}

fn is_reserved_call(name: &str) -> bool {
    name == "Query" || name == "Mutation"
}

/// What the lowering of statements keeps between chunks.
#[derive(Debug, Default)]
pub struct Lowerer {
    cache: HashMap<Key, Cached>,
    pass: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Position {
    /// A direct argument of a component or a direct element of an array there.
    Value,
    /// Anywhere else.
    Computed,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct Key {
    name: String,
    position: Position,
    partial: bool,
}

#[derive(Debug)]
struct Cached {
    statement: Arc<Statement>,
    node: Node,
    errors: Vec<Error>,
    reached: Vec<String>,
    /// Every statement its lowering read, as it was.
    deps: Vec<(String, Arc<Statement>)>,
    used: u64,
}

struct Pass<'a> {
    library: &'a Library,
    parsed: &'a Parsed,
    cache: &'a mut HashMap<Key, Cached>,
    pass: u64,
    partial: bool,
    errors: Vec<Error>,
    unresolved: Vec<String>,
    reached: Vec<String>,
    touched: Vec<(String, Arc<Statement>)>,
    visiting: Vec<String>,
    /// The loop variables of the `@Each` templates around.
    scope: Vec<String>,
    cycles: usize,
    /// The statement being lowered, which errors name.
    statement: Option<String>,
}

impl Pass<'_> {
    fn error(&mut self, code: Code, component: &str, path: &str, message: String, hint: Option<String>) {
        let mut error = Error::new(Source::Parser, code, message)
            .statement(self.statement.as_deref())
            .component(component)
            .path(path);
        error.hint = hint;
        self.errors.push(error);
    }

    fn no_root(&mut self, component: &str, message: String, warning: bool) {
        let hint = match component {
            "" => "Start the program with root = Component(...)".to_owned(),
            component => format!("Start the program with root = {component}(...)"),
        };
        self.error(Code::NoRoot, component, "", message, Some(hint));
        if warning && let Some(error) = self.errors.last_mut() {
            error.severity = Some(crate::error::Severity::Warning);
        }
    }

    fn checked(&mut self, checker: Checker, component: &str, signature: &str) {
        for issue in checker.issues {
            let hint = Some(signature_hint(signature));
            self.error(issue.code, component, &issue.path, issue.message, hint);
        }
    }

    /// A statement's value where `name` is written.
    fn resolve(&mut self, name: &str, position: Position) -> Node {
        if self.visiting.iter().any(|visiting| visiting == name) {
            self.cycles += 1;
            self.unresolved.push(name.to_owned());
            return Node::Nothing;
        }
        let Some(statement) = self.parsed.statements.get(name) else {
            self.unresolved.push(name.to_owned());
            return Node::Nothing;
        };
        self.reached.push(name.to_owned());
        self.touched.push((name.to_owned(), statement.clone()));
        match statement.kind {
            Kind::Query => return Node::Query(name.to_owned()),
            Kind::Mutation => return Node::Mutation(name.to_owned()),
            Kind::Value | Kind::State => {}
        }
        let key = Key { name: name.to_owned(), position, partial: self.partial };
        if let Some(cached) = self.cache.get_mut(&key)
            && Arc::ptr_eq(&cached.statement, statement)
            && cached
                .deps
                .iter()
                .all(|(name, dep)| self.parsed.statements.get(name).is_some_and(|current| Arc::ptr_eq(current, dep)))
        {
            cached.used = self.pass;
            self.errors.extend(cached.errors.iter().cloned());
            self.reached.extend(cached.reached.iter().cloned());
            self.touched.extend(cached.deps.iter().cloned());
            return cached.node.clone();
        }

        let marks = (self.errors.len(), self.unresolved.len(), self.cycles, self.reached.len(), self.touched.len());
        self.visiting.push(name.to_owned());
        let statement_before = self.statement.replace(name.to_owned());
        let scope = std::mem::take(&mut self.scope);
        let node = tag(self.lower(&statement.expr, position), name);
        self.scope = scope;
        self.statement = statement_before;
        self.visiting.pop();
        if self.unresolved.len() == marks.1 && self.cycles == marks.2 {
            let cached = Cached {
                statement: statement.clone(),
                node: node.clone(),
                errors: self.errors[marks.0..].to_vec(),
                reached: self.reached[marks.3..].to_vec(),
                deps: self.touched[marks.4..].to_vec(),
                used: self.pass,
            };
            self.cache.insert(key, cached);
        }
        node
    }

    fn lower(&mut self, expr: &Expr, position: Position) -> Node {
        match expr {
            Expr::Str(text) => Node::Lit(Value::String(text.clone())),
            Expr::Num(value) => Node::Lit(Value::Number(*value)),
            Expr::Bool(value) => Node::Lit(Value::Bool(*value)),
            Expr::Null => Node::null(),
            Expr::Ref(name) if self.scope.contains(name) => Node::Loop(name.clone()),
            Expr::Ref(name) => self.resolve(name, position),
            Expr::State(name) => Node::State(name.clone()),
            Expr::Array(items) => self.array(items, position),
            Expr::Object(entries) => {
                let entries = entries.iter().map(|(key, value)| (key.clone(), self.operand(value))).collect();
                fold(Node::Object(entries))
            }
            Expr::Call { name, args, at } => self.call(name, args, *at, position),
            Expr::Unary(op, operand) => fold(Node::Unary(*op, Box::new(self.operand(operand)))),
            Expr::Binary(op, left, right) => {
                let left = self.operand(left);
                fold(Node::Binary(*op, Box::new(left), Box::new(self.operand(right))))
            }
            Expr::Ternary(cond, then, otherwise) => {
                let cond = self.operand(cond);
                let then = self.operand(then);
                fold(Node::Ternary(Box::new(cond), Box::new(then), Box::new(self.operand(otherwise))))
            }
            Expr::Member(object, field) => fold(Node::Member(Box::new(self.operand(object)), field.clone())),
            Expr::Index(object, index) => {
                let object = self.operand(object);
                fold(Node::Index(Box::new(object), Box::new(self.operand(index))))
            }
            Expr::Assign(target, value) => Node::Assign(target.clone(), Arc::new(self.operand(value))),
        }
    }

    /// A computed expression: what is missing is null.
    fn operand(&mut self, expr: &Expr) -> Node {
        match self.lower(expr, Position::Computed) {
            Node::Nothing => Node::null(),
            node => node,
        }
    }

    /// Unresolved references and components that were dropped are left out.
    fn array(&mut self, items: &[Expr], position: Position) -> Node {
        let mut nodes = Vec::with_capacity(items.len());
        for item in items {
            let node = self.lower(item, position);
            let dropped = matches!(item, Expr::Call { .. } | Expr::Ref(_)) && node == Node::null();
            if node == Node::Nothing || dropped {
                continue;
            }
            nodes.push(node);
        }
        fold(Node::Array(nodes))
    }

    fn call(&mut self, name: &str, args: &[Expr], at: bool, position: Position) -> Node {
        if is_reserved_call(name) {
            if position == Position::Computed {
                return Node::null();
            }
            let message = format!("{name}() must be declared as a top-level statement, not used inline as a value");
            let hint = format!("Declare as a top-level statement: myVar = {name}(...)");
            self.error(Code::InlineReserved, name, "", message, Some(hint));
            return Node::Nothing;
        }
        if !at && name != "Action" {
            let library = self.library;
            return match library.component(name) {
                Some(def) => self.element(def, args),
                None => {
                    let available = library.components().map(|component| component.name.as_str());
                    let available = available.collect::<Vec<_>>().join(", ");
                    let message = format!(
                        "Unknown component \"{name}\" — not found in catalog or builtins. Available components: {available}"
                    );
                    if position == Position::Computed {
                        args.iter().for_each(|arg| drop(self.lower(arg, Position::Computed)));
                    }
                    self.error(
                        Code::UnknownComponent,
                        name,
                        "",
                        message,
                        Some(format!("Available components: {available}")),
                    );
                    Node::null()
                }
            };
        }
        if let Some(builtin) = Builtin::from_name(name) {
            return self.builtin(builtin, args);
        }
        let library = self.library;
        if let Some(def) = library.function(name) {
            return match self.call_args(name, &def.params, args, false) {
                Some(args) => Node::Function(name.to_owned(), args),
                None => Node::null(),
            };
        }
        if let Some(def) = library.action(name) {
            return match self.call_args(name, &def.params, args, true) {
                Some(args) => Node::Custom(name.to_owned(), args),
                None => empty_plan(),
            };
        }
        let mut available: Vec<String> =
            Builtin::FUNCTIONS.iter().chain(&Builtin::ACTIONS).map(|b| format!("@{}", b.name())).collect();
        available.extend(library.functions().map(|function| format!("@{}", function.name)));
        available.extend(library.actions().map(|action| format!("@{}", action.name)));
        let message =
            format!("Unknown function \"@{name}\": not a built-in or action step. The call evaluates to null");
        self.error(
            Code::UnknownFunction,
            name,
            "",
            message,
            Some(format!("Available functions: {}", available.join(", "))),
        );
        Node::null()
    }

    fn builtin(&mut self, builtin: Builtin, args: &[Expr]) -> Node {
        match builtin {
            Builtin::Each => {
                let var = match args.get(1) {
                    Some(Expr::Ref(name) | Expr::Str(name)) if args.len() >= 3 => name.clone(),
                    _ => {
                        args.iter().for_each(|arg| drop(self.operand(arg)));
                        return Node::Lit(Value::Array(Vec::new()));
                    }
                };
                let array = self.operand(&args[0]);
                self.scope.push(var.clone());
                let template = self.operand(&args[2]);
                self.scope.pop();
                fold(Node::Each(Box::new(array), var, Arc::new(template)))
            }
            Builtin::Action => {
                let list = args.first().map_or(Node::Lit(Value::Array(Vec::new())), |list| self.operand(list));
                fold(Node::Steps(Box::new(list)))
            }
            Builtin::Run => match args.first().map(|target| self.operand(target)) {
                Some(Node::Query(statement)) => plan(ActionStep::Run { statement, mutation: false }),
                Some(Node::Mutation(statement)) => plan(ActionStep::Run { statement, mutation: true }),
                _ => empty_plan(),
            },
            Builtin::Set => {
                let (Some(Expr::State(variable)), Some(value)) = (args.first(), args.get(1)) else {
                    return empty_plan();
                };
                let value = self.operand(value);
                if value.is_scoped() {
                    return Node::Set(variable.clone(), Arc::new(value));
                }
                plan(ActionStep::Set { variable: variable.clone(), value: Arc::new(value) })
            }
            Builtin::Reset => {
                let variables: Vec<String> = args
                    .iter()
                    .filter_map(|arg| match arg {
                        Expr::State(name) => Some(name.clone()),
                        _ => None,
                    })
                    .collect();
                if variables.is_empty() {
                    return empty_plan();
                }
                plan(ActionStep::Reset { variables })
            }
            Builtin::ToAssistant => {
                let message = args.first().map_or(Node::null(), |message| self.operand(message));
                let context = args.get(1).map(|context| Box::new(self.operand(context)));
                fold(Node::ToAssistant(Box::new(message), context))
            }
            Builtin::OpenUrl => {
                let url = args.first().map_or(Node::null(), |url| self.operand(url));
                fold(Node::OpenUrl(Box::new(url)))
            }
            builtin => {
                let args = args.iter().map(|arg| self.operand(arg)).collect();
                fold(Node::Builtin(builtin, args))
            }
        }
    }

    /// A library function's or action's arguments by param. Literal ones are checked now; an
    /// invalid required one, or for an action any invalid one, makes the call invalid.
    fn call_args(&mut self, name: &str, params: &[Field], args: &[Expr], action: bool) -> Option<Vec<Option<Node>>> {
        let lowered: Vec<Node> = args.iter().map(|arg| self.operand(arg)).collect();
        let errors_before = self.errors.len();
        let signature = crate::schema::hint_signature(name, params);
        if args.len() > params.len() {
            let message = format!(
                "{name} takes {} arg(s), got {} ({} excess dropped)",
                params.len(),
                args.len(),
                args.len() - params.len()
            );
            self.error(Code::ExcessArgs, name, "", message, Some(signature_hint(&signature)));
        }
        let mut mapped = Vec::with_capacity(params.len());
        let mut invalid = false;
        let mut checker = Checker::new(self.partial);
        for (index, param) in params.iter().enumerate() {
            let node = lowered.get(index).cloned();
            if !args.get(index).is_none_or(|arg| self.literal(arg, 0)) {
                mapped.push(node);
                continue;
            }
            let path = format!("/{}", param.name);
            let Some(node) = node.filter(|node| *node != Node::null()) else {
                let present = index < args.len();
                if param.required() && param.default.is_none() {
                    self.missing(name, &path, present, &signature);
                    invalid = true;
                }
                mapped.push(present.then(Node::null));
                continue;
            };
            match checker.node(node, &param.schema, &path) {
                Some(node) => mapped.push(Some(node)),
                None => match fallback(param) {
                    Fallback::Default(default) => mapped.push(Some(Node::Lit(default))),
                    Fallback::Omit => mapped.push(None),
                    Fallback::Drop => {
                        invalid = true;
                        mapped.push(None);
                    }
                },
            }
        }
        self.checked(checker, name, &signature);
        if action && self.errors.len() > errors_before {
            invalid = true;
        }
        (!invalid).then_some(mapped)
    }

    fn missing(&mut self, component: &str, path: &str, present: bool, signature: &str) {
        let (code, message) = if present {
            (Code::NullRequired, format!("required field \"{path}\" cannot be null — signature: {signature}"))
        } else {
            (Code::MissingRequired, format!("missing required field \"{path}\" — signature: {signature}"))
        };
        self.error(code, component, path, message, Some(signature_hint(signature)));
    }

    /// Literal syntax, through references to literal statements.
    fn literal(&self, expr: &Expr, depth: usize) -> bool {
        match expr {
            Expr::Str(_) | Expr::Num(_) | Expr::Bool(_) | Expr::Null => true,
            Expr::Array(items) => items.iter().all(|item| self.literal(item, depth)),
            Expr::Object(entries) => entries.iter().all(|(_, value)| self.literal(value, depth)),
            Expr::Ref(name) if depth < 32 && !self.scope.contains(name) => self
                .parsed
                .statement(name)
                .is_some_and(|statement| statement.kind == Kind::Value && self.literal(&statement.expr, depth + 1)),
            _ => false,
        }
    }

    /// A component call: its arguments mapped to props and checked (§8.4). A prop that is
    /// absent or null takes its default.
    fn element(&mut self, def: &ComponentDef, args: &[Expr]) -> Node {
        let name = def.name.as_str();
        let signature = def.hint();
        let mut props: Vec<(String, Node)> = Vec::with_capacity(def.props.len());
        let mut dropped = Vec::new();
        let mut checker = Checker::new(self.partial);
        for (param, arg) in def.props.iter().zip(args) {
            let node = match self.lower(arg, Position::Value) {
                Node::Nothing => Node::null(),
                node => node,
            };
            match checker.node(node, &param.schema, &format!("/{}", param.name)) {
                Some(node) => props.push((param.name.clone(), node)),
                None => match fallback(param) {
                    Fallback::Default(default) => props.push((param.name.clone(), Node::Lit(default))),
                    Fallback::Omit => {}
                    Fallback::Drop => dropped.push(param.name.as_str()),
                },
            }
        }
        self.checked(checker, name, &signature);
        if args.len() > def.props.len() {
            let message = format!(
                "{name} takes {} arg(s), got {} ({} excess dropped)",
                def.props.len(),
                args.len(),
                args.len() - def.props.len()
            );
            self.error(Code::ExcessArgs, name, "", message, Some(signature_hint(&signature)));
        }
        let mut missing = false;
        for param in def.props.iter().filter(|param| !dropped.contains(&param.name.as_str())) {
            let index = props.iter().position(|(prop, _)| *prop == param.name);
            if index.is_some_and(|index| props[index].1 != Node::null()) {
                continue;
            }
            if let Some(default) = &param.default {
                match index {
                    Some(index) => props[index].1 = Node::Lit(default.clone()),
                    None => props.push((param.name.clone(), Node::Lit(default.clone()))),
                }
                continue;
            }
            if !param.required() {
                continue;
            }
            self.missing(name, &format!("/{}", param.name), index.is_some(), &signature);
            missing = true;
        }
        if missing || !dropped.is_empty() {
            return Node::null();
        }
        props.sort_by_key(|(prop, _)| def.props.iter().position(|param| param.name == *prop));
        fold(Node::Element(Arc::new(ElementNode::new(name.to_owned(), props, self.partial))))
    }

    /// The state declarations with their defaults, then the `$names` used without one.
    fn state(&mut self) -> IndexMap<String, Node> {
        let mut state = IndexMap::new();
        for statement in self.parsed.statements.values().filter(|statement| statement.kind == Kind::State) {
            self.statement = Some(statement.name.clone());
            let node = self.lower(&statement.expr, Position::Value);
            state.insert(statement.name.clone(), if node == Node::Nothing { Node::null() } else { node });
        }
        self.statement = None;
        let growing = self.parsed.growing.as_deref();
        for statement in self.parsed.statements.values() {
            let pending = growing.is_some() && self.parsed.is_pending(&statement.name);
            for name in &statement.states {
                if pending && Some(name.as_str()) == growing {
                    continue;
                }
                state.entry(name.clone()).or_insert_with(Node::null);
            }
        }
        state
    }

    fn calls(&mut self) -> (Vec<QueryDecl>, Vec<MutationDecl>) {
        let mut queries = Vec::new();
        let mut mutations = Vec::new();
        for statement in self.parsed.statements.values() {
            let args = statement.call_args();
            self.statement = Some(statement.name.clone());
            let arg = |pass: &mut Self, index: usize| args.get(index).map(|arg| pass.operand(arg));
            match statement.kind {
                Kind::Query => {
                    let tool = arg(self, 0).unwrap_or_else(Node::null);
                    let call_args = arg(self, 1).unwrap_or_else(Node::null);
                    let defaults = arg(self, 2).unwrap_or_else(Node::null);
                    let refresh = arg(self, 3);
                    let deps = args.get(1).map(Expr::states).unwrap_or_default();
                    let statement = statement.name.clone();
                    queries.push(QueryDecl { statement, tool, args: call_args, defaults, refresh, deps });
                }
                Kind::Mutation => {
                    let tool = arg(self, 0).unwrap_or_else(Node::null);
                    let call_args = arg(self, 1).unwrap_or_else(Node::null);
                    mutations.push(MutationDecl { statement: statement.name.clone(), tool, args: call_args });
                }
                Kind::Value | Kind::State => {}
            }
        }
        self.statement = None;
        (queries, mutations)
    }
}

fn signature_hint(signature: &str) -> String {
    format!("Signature: {signature} — * marks required")
}

fn plan(step: ActionStep) -> Node {
    Node::Lit(Value::from(ActionPlan { steps: vec![step] }))
}

fn empty_plan() -> Node {
    Node::Lit(Value::from(ActionPlan::default()))
}

/// Marks the components a statement's value is with the statement's name.
fn tag(node: Node, name: &str) -> Node {
    match node {
        Node::Lit(Value::Element(element)) => {
            let mut element = Arc::unwrap_or_clone(element);
            element.statement = Some(name.to_owned());
            Node::Lit(Value::from(element))
        }
        Node::Element(element) => {
            let mut element = Arc::unwrap_or_clone(element);
            element.statement = Some(name.to_owned());
            Node::Element(Arc::new(element))
        }
        node => node,
    }
}

/// A node whose children are all constant, computed now. What reads state, a query, a loop
/// variable or calls a library function or action stays.
fn fold(node: Node) -> Node {
    let foldable = matches!(
        node,
        Node::Element(_)
            | Node::Array(_)
            | Node::Object(_)
            | Node::Unary(..)
            | Node::Binary(..)
            | Node::Ternary(..)
            | Node::Member(..)
            | Node::Index(..)
            | Node::Builtin(..)
            | Node::Each(..)
            | Node::Steps(_)
            | Node::ToAssistant(..)
            | Node::OpenUrl(_)
    );
    let constant = foldable && node.children().into_iter().all(Node::is_lit);
    if !constant {
        return node;
    }
    Node::Lit(eval::constant(&node))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{ComponentDef, LibraryDef};
    use crate::schema::Schema;

    fn library() -> Library {
        let node = Schema::array(Schema::Any);
        Library::new(LibraryDef {
            root: Some("Stack".into()),
            components: vec![
                ComponentDef::new("Stack", "", [node.field("children")]),
                ComponentDef::new(
                    "Text",
                    "",
                    [
                        Schema::String.field("text"),
                        Schema::enumeration(["sm", "md"]).field("size").optional().with_default("md"),
                    ],
                ),
            ],
            ..LibraryDef::default()
        })
        .unwrap()
    }

    fn root_value(program: &Program) -> Value {
        eval::constant(program.root.as_ref().unwrap())
    }

    #[test]
    fn fills_defaults_and_names_statements() {
        let program =
            Program::parse(&library(), "root = Stack([t, Text(\"b\", null), Text(\"c\", \"xl\")])\nt = Text(\"a\")");
        let root = root_value(&program);
        let children = root.as_element().unwrap().props["children"].as_array().unwrap();
        let sizes: Vec<&Value> = children.iter().map(|child| &child.as_element().unwrap().props["size"]).collect();
        assert_eq!(sizes, [&Value::from("md"); 3]);
        assert_eq!(children[0].as_element().unwrap().statement.as_deref(), Some("t"));
        assert_eq!(root.as_element().unwrap().statement.as_deref(), Some("root"));
        assert_eq!(program.errors.len(), 1);
        assert_eq!(program.errors[0].code, Code::TypeMismatch);
    }

    #[test]
    fn reports_inline_queries_only_in_value_positions() {
        let program = Program::parse(
            &library(),
            "root = Stack([Query(\"t\", {}, {}), Text(Query(\"t\", {}, {}) == null ? \"n\" : \"v\")])",
        );
        let codes: Vec<Code> = program.errors.iter().map(|error| error.code).collect();
        assert_eq!(codes, [Code::InlineReserved]);
        let children = root_value(&program).as_element().unwrap().props["children"].as_array().unwrap().len();
        assert_eq!(children, 1);
    }

    #[test]
    fn keeps_the_lowering_of_completed_statements_while_streaming() {
        let library = library();
        let mut parser = crate::parser::Parser::new();
        let mut lowerer = Lowerer::default();
        let element = |program: &Program| match program.root.as_ref().unwrap() {
            Node::Lit(Value::Element(root)) => root.props["children"].as_array().unwrap()[0].clone(),
            node => panic!("{node:?}"),
        };
        let first = Program::build(
            &library,
            &parser.push("root = Stack([t, u])\nt = Text(\"a\")\nu = Text(\"b", false),
            &mut lowerer,
        );
        let second = Program::build(&library, &parser.push("c\")", false), &mut lowerer);
        let (Value::Element(first), Value::Element(second)) = (element(&first), element(&second)) else { panic!() };
        assert!(Arc::ptr_eq(&first, &second));
    }
}
