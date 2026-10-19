//! The evaluator (language.md §3, §6.3): a lowered program to values, components and action
//! plans, reading the state store and the query results through [`Inputs`].
//!
//! A component whose inputs did not change since the last evaluation is not evaluated again: it
//! is the same value as before, so a host can tell what changed by pointer.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use indexmap::IndexMap;

use crate::ast::{BinaryOp, UnaryOp};
use crate::builtins::Builtin;
use crate::check::{Checker, Fallback, Outcome, fallback};
use crate::element::{ActionPlan, ActionStep, Binding, Element};
use crate::error::{Code, Error, Source};
use crate::js;
use crate::library::{Args, Env, FixedEnv, Library};
use crate::node::{Dep, ElementNode, Node};
use crate::schema::Field;
use crate::value::{Map, Value};

/// What an evaluation reads.
pub trait Inputs {
    /// A `$variable`'s value, null when unset.
    fn state(&self, name: &str) -> Value;
    /// A query's latest result, or its defaults.
    fn query(&self, statement: &str) -> Value;
    /// A mutation's `{status, data, error}`.
    fn mutation(&self, statement: &str) -> Value;
    /// A number that changes whenever the input does.
    fn version(&self, dep: &Dep) -> u64;
}

/// No state set, no query returned.
pub struct NoInputs;

impl Inputs for NoInputs {
    fn state(&self, _name: &str) -> Value {
        Value::Null
    }

    fn query(&self, _statement: &str) -> Value {
        Value::Null
    }

    fn mutation(&self, _statement: &str) -> Value {
        idle_mutation()
    }

    fn version(&self, _dep: &Dep) -> u64 {
        0
    }
}

/// A mutation that has not run: `{status: "idle", data: null, error: null}`.
pub fn idle_mutation() -> Value {
    Value::from_iter([("status", Value::from("idle")), ("data", Value::Null), ("error", Value::Null)])
}

/// The components evaluated last time, by their lowered node, with their inputs' versions. An
/// entry holds its node, so that no other node takes its address while it is kept.
#[derive(Debug, Default)]
pub struct Cache {
    entries: HashMap<usize, (Arc<ElementNode>, Vec<u64>, Value)>,
    used: HashSet<usize>,
}

impl Cache {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.used.clear();
    }

    /// Forgets the components the last evaluation did not use.
    pub fn sweep(&mut self) {
        let used = std::mem::take(&mut self.used);
        self.entries.retain(|key, _| used.contains(key));
    }
}

/// `@Each`'s loop variables.
#[derive(Clone, Copy)]
pub struct Scope<'s> {
    name: &'s str,
    value: &'s Value,
    parent: Option<&'s Scope<'s>>,
}

fn lookup<'s>(scope: Option<&'s Scope<'s>>, name: &str) -> Option<&'s Value> {
    let mut scope = scope;
    while let Some(current) = scope {
        if current.name == name {
            return Some(current.value);
        }
        scope = current.parent;
    }
    None
}

pub struct Evaluator<'a> {
    library: Option<&'a Library>,
    env: &'a dyn Env,
    inputs: &'a dyn Inputs,
    cache: Option<&'a mut Cache>,
    /// The runtime errors met, each a prop that became null.
    pub errors: Vec<Error>,
    /// The statement, component and prop being evaluated, which errors name.
    statement: Option<String>,
    at: Option<(String, String)>,
}

/// A node whose children are all constant.
pub fn constant(node: &Node) -> Value {
    let env = FixedEnv::default();
    Evaluator {
        library: None,
        env: &env,
        inputs: &NoInputs,
        cache: None,
        errors: Vec::new(),
        statement: None,
        at: None,
    }
    .eval(node, None)
}

impl<'a> Evaluator<'a> {
    pub fn new(library: &'a Library, env: &'a dyn Env, inputs: &'a dyn Inputs) -> Evaluator<'a> {
        Evaluator { library: Some(library), env, inputs, cache: None, errors: Vec::new(), statement: None, at: None }
    }

    pub fn with_cache(mut self, cache: &'a mut Cache) -> Evaluator<'a> {
        self.cache = Some(cache);
        self
    }

    /// Errors name this statement when no component around says another.
    pub fn in_statement(mut self, statement: Option<&str>) -> Evaluator<'a> {
        self.statement = statement.map(str::to_owned);
        self
    }

    fn report(&mut self, message: String) {
        let error = match &self.at {
            Some((component, prop)) => Error::new(
                Source::Runtime,
                Code::RuntimeError,
                format!("Evaluating prop \"{prop}\" on {component} failed: {message}"),
            )
            .component(component.clone())
            .hint(format!("Check the expression used for prop \"{prop}\"")),
            None => Error::new(Source::Runtime, Code::RuntimeError, message),
        };
        self.errors.push(error.statement(self.statement.as_deref()));
    }

    pub fn eval(&mut self, node: &Node, scope: Option<&Scope<'_>>) -> Value {
        match node {
            Node::Lit(value) => value.clone(),
            Node::Nothing => Value::Null,
            Node::Array(items) => items.iter().map(|item| self.eval(item, scope)).collect(),
            Node::Object(entries) => {
                Value::Object(entries.iter().map(|(key, value)| (key.clone(), self.eval(value, scope))).collect())
            }
            Node::Element(element) => self.element(element, scope),
            Node::State(name) | Node::Assign(name, _) => self.inputs.state(name),
            Node::Query(statement) => self.inputs.query(statement),
            Node::Mutation(statement) => self.inputs.mutation(statement),
            Node::Loop(name) => lookup(scope, name).cloned().unwrap_or_default(),
            Node::Unary(UnaryOp::Not, operand) => Value::Bool(!js::truthy(&self.eval(operand, scope))),
            Node::Unary(UnaryOp::Neg, operand) => Value::Number(js::negate(&self.eval(operand, scope))),
            Node::Binary(op, left, right) => self.binary(*op, left, right, scope),
            Node::Ternary(cond, then, otherwise) => {
                let branch = if js::truthy(&self.eval(cond, scope)) { then } else { otherwise };
                self.eval(branch, scope)
            }
            Node::Member(object, field) => js::member(&self.eval(object, scope), field),
            Node::Index(object, index) => {
                let object = self.eval(object, scope);
                js::index(&object, &self.eval(index, scope))
            }
            Node::Builtin(builtin, args) => {
                let args: Vec<Value> = args.iter().map(|arg| self.eval(arg, scope)).collect();
                builtin_function(*builtin, &args)
            }
            Node::Each(array, var, template) => {
                let Value::Array(items) = self.eval(array, scope) else { return Value::Array(Vec::new()) };
                items
                    .iter()
                    .map(|item| {
                        let inner = Scope { name: var, value: item, parent: scope };
                        self.eval(template, Some(&inner))
                    })
                    .collect()
            }
            Node::Function(name, args) => self.function(name, args, scope),
            Node::Custom(name, args) => self.custom(name, args, scope),
            Node::Steps(list) => {
                let Value::Array(items) = self.eval(list, scope) else { return Value::from(ActionPlan::default()) };
                let steps = items.iter().filter_map(Value::as_plan).flat_map(|plan| plan.steps.iter().cloned());
                Value::from(ActionPlan { steps: steps.collect() })
            }
            Node::ToAssistant(message, context) => {
                let message = match self.eval(message, scope) {
                    Value::Null => String::new(),
                    message => js::to_string(&message),
                };
                let context =
                    context.as_ref().map(|context| self.eval(context, scope)).filter(|value| !value.is_null());
                step(ActionStep::ToAssistant { message, context })
            }
            Node::OpenUrl(url) => {
                let url = match self.eval(url, scope) {
                    Value::Null => String::new(),
                    url => js::to_string(&url),
                };
                step(ActionStep::OpenUrl { url })
            }
            Node::Set(variable, value) => {
                step(ActionStep::Set { variable: variable.clone(), value: Arc::new(substitute(value, scope, &[])) })
            }
        }
    }

    fn binary(&mut self, op: BinaryOp, left: &Node, right: &Node, scope: Option<&Scope<'_>>) -> Value {
        let left = self.eval(left, scope);
        match op {
            BinaryOp::And if !js::truthy(&left) => return left,
            BinaryOp::Or if js::truthy(&left) => return left,
            BinaryOp::And | BinaryOp::Or => return self.eval(right, scope),
            _ => {}
        }
        let right = self.eval(right, scope);
        let compare = |test: fn(f64, f64) -> bool| Value::Bool(test(js::to_number(&left), js::to_number(&right)));
        match op {
            BinaryOp::Eq => Value::Bool(js::loose_eq(&left, &right)),
            BinaryOp::NotEq => Value::Bool(!js::loose_eq(&left, &right)),
            BinaryOp::Greater => compare(|a, b| a > b),
            BinaryOp::Less => compare(|a, b| a < b),
            BinaryOp::GreaterEq => compare(|a, b| a >= b),
            BinaryOp::LessEq => compare(|a, b| a <= b),
            BinaryOp::Add => js::add(&left, &right),
            BinaryOp::Sub => Value::Number(js::subtract(&left, &right)),
            BinaryOp::Mul => Value::Number(js::multiply(&left, &right)),
            BinaryOp::Div => Value::Number(js::divide(&left, &right)),
            BinaryOp::Rem => Value::Number(js::remainder(&left, &right)),
            BinaryOp::And | BinaryOp::Or => Value::Null,
        }
    }

    fn element(&mut self, node: &Arc<ElementNode>, scope: Option<&Scope<'_>>) -> Value {
        let key = Arc::as_ptr(node) as usize;
        let cacheable = !node.volatile && !node.scoped && self.cache.is_some();
        let fingerprint: Vec<u64> =
            if cacheable { node.deps.iter().map(|dep| self.inputs.version(dep)).collect() } else { Vec::new() };
        if cacheable
            && let Some(cache) = self.cache.as_deref_mut()
            && let Some((_, versions, value)) = cache.entries.get(&key)
            && *versions == fingerprint
        {
            cache.used.insert(key);
            return value.clone();
        }

        let def = self.library.and_then(|library| library.component(&node.component));
        let statement = node.statement.clone().or_else(|| self.statement.clone());
        let saved_statement = std::mem::replace(&mut self.statement, statement);
        let saved_at = self.at.take();
        let mut props = Map::new();
        let mut bindings = IndexMap::new();
        for (name, prop) in &node.props {
            self.at = Some((node.component.clone(), name.clone()));
            let bindable = def.and_then(|def| def.prop(name)).is_some_and(|field| field.bindable);
            let value = match prop {
                Node::State(variable) if bindable => {
                    bindings.insert(name.clone(), Binding { variable: variable.clone(), write: None });
                    self.inputs.state(variable)
                }
                Node::Assign(variable, write) if bindable => {
                    let write = Arc::new(substitute(write, scope, &[]));
                    bindings.insert(name.clone(), Binding { variable: variable.clone(), write: Some(write) });
                    self.inputs.state(variable)
                }
                prop => self.eval(prop, scope),
            };
            if !value.is_null() {
                props.insert(name.clone(), value);
            }
        }
        self.at = saved_at;
        self.statement = saved_statement;
        let value = Value::from(Element {
            component: node.component.clone(),
            statement: node.statement.clone(),
            props,
            partial: node.partial,
            bindings,
        });
        if cacheable && let Some(cache) = self.cache.as_deref_mut() {
            cache.entries.insert(key, (node.clone(), fingerprint, value.clone()));
            cache.used.insert(key);
        }
        value
    }

    /// A library function's or action's arguments, checked against its params (§3.6): None
    /// when the call is null (or the action does nothing). Explicit nulls take the default.
    fn call_args(
        &mut self,
        name: &str,
        params: &[Field],
        args: &[Option<Node>],
        strict: bool,
        scope: Option<&Scope<'_>>,
    ) -> Option<Vec<Option<Value>>> {
        let raw: Vec<Option<Value>> = params
            .iter()
            .enumerate()
            .map(|(index, _)| args.get(index).and_then(Option::as_ref).map(|arg| self.eval(arg, scope)))
            .collect();
        let misses = params.iter().zip(&raw).any(|(param, value)| {
            param.required() && param.default.is_none() && value.as_ref().is_none_or(Value::is_null)
        });
        if misses {
            return None;
        }
        let mut checker = Checker::new(false);
        let mut invalid = false;
        let mut values = Vec::with_capacity(params.len());
        for (param, value) in params.iter().zip(raw) {
            let Some(value) = value.filter(|value| !value.is_null()) else {
                values.push(param.default.clone());
                continue;
            };
            match checker.value(&value, &param.schema, &format!("/{}", param.name)) {
                Outcome::Keep => values.push(Some(value)),
                Outcome::Replace(next) => values.push(Some(next)),
                Outcome::Invalid => match fallback(param) {
                    Fallback::Default(default) => values.push(Some(default)),
                    Fallback::Omit => values.push(None),
                    Fallback::Drop => {
                        invalid = true;
                        values.push(None);
                    }
                },
            }
        }
        let failed = !checker.issues.is_empty();
        for issue in checker.issues {
            self.report(format!("@{name}: {}", issue.message));
        }
        (!invalid && !(strict && failed)).then_some(values)
    }

    fn function(&mut self, name: &str, args: &[Option<Node>], scope: Option<&Scope<'_>>) -> Value {
        let Some(def) = self.library.and_then(|library| library.function(name)) else { return Value::Null };
        let Some(values) = self.call_args(name, &def.params, args, false, scope) else { return Value::Null };
        let Some(run) = def.run else {
            self.report(format!("@{name} has no implementation"));
            return Value::Null;
        };
        let values: Vec<Value> = values.into_iter().map(Option::unwrap_or_default).collect();
        let result = match run(&Args::new(&def.params, &values, self.env)) {
            Ok(result) => result,
            Err(error) => {
                self.report(format!("@{name} threw: {error}"));
                return Value::Null;
            }
        };
        let Some(returns) = &def.returns else { return result };
        let mut checker = Checker::new(false);
        let outcome = checker.value(&result, returns, "/returns");
        for issue in checker.issues {
            self.report(format!("@{name}: {}", issue.message));
        }
        match outcome {
            Outcome::Keep => result,
            Outcome::Replace(next) => next,
            Outcome::Invalid => Value::Null,
        }
    }

    fn custom(&mut self, name: &str, args: &[Option<Node>], scope: Option<&Scope<'_>>) -> Value {
        let Some(def) = self.library.and_then(|library| library.action(name)) else {
            return Value::from(ActionPlan::default());
        };
        let Some(values) = self.call_args(name, &def.params, args, true, scope) else {
            return Value::from(ActionPlan::default());
        };
        let params: Map =
            def.params.iter().zip(values).filter_map(|(param, value)| Some((param.name.clone(), value?))).collect();
        step(ActionStep::Custom { name: name.to_owned(), params })
    }
}

fn step(step: ActionStep) -> Value {
    Value::from(ActionPlan { steps: vec![step] })
}

/// The node with the loop variables in scope put in as values, so that an action built in an
/// `@Each` template keeps its element. `shadowed` are the variables of templates inside.
pub fn substitute(node: &Node, scope: Option<&Scope<'_>>, shadowed: &[&str]) -> Node {
    if scope.is_none() || !node.is_scoped() {
        return node.clone();
    }
    let sub = |node: &Node| substitute(node, scope, shadowed);
    let boxed = |node: &Node| Box::new(substitute(node, scope, shadowed));
    match node {
        Node::Loop(name) if !shadowed.contains(&name.as_str()) => {
            lookup(scope, name).map_or_else(Node::null, |value| Node::Lit(value.clone()))
        }
        Node::Array(items) => Node::Array(items.iter().map(sub).collect()),
        Node::Object(entries) => Node::Object(entries.iter().map(|(key, value)| (key.clone(), sub(value))).collect()),
        Node::Element(element) => {
            let props = element.props.iter().map(|(name, value)| (name.clone(), sub(value))).collect();
            let mut next = ElementNode::new(element.component.clone(), props, element.partial);
            next.statement = element.statement.clone();
            Node::Element(Arc::new(next))
        }
        Node::Unary(op, operand) => Node::Unary(*op, boxed(operand)),
        Node::Binary(op, left, right) => Node::Binary(*op, boxed(left), boxed(right)),
        Node::Ternary(cond, then, otherwise) => Node::Ternary(boxed(cond), boxed(then), boxed(otherwise)),
        Node::Member(object, field) => Node::Member(boxed(object), field.clone()),
        Node::Index(object, index) => Node::Index(boxed(object), boxed(index)),
        Node::Assign(target, value) => Node::Assign(target.clone(), Arc::new(sub(value))),
        Node::Builtin(builtin, args) => Node::Builtin(*builtin, args.iter().map(sub).collect()),
        Node::Each(array, var, template) => {
            let mut inner: Vec<&str> = shadowed.to_vec();
            inner.push(var);
            Node::Each(boxed(array), var.clone(), Arc::new(substitute(template, scope, &inner)))
        }
        Node::Function(name, args) => {
            Node::Function(name.clone(), args.iter().map(|arg| arg.as_ref().map(sub)).collect())
        }
        Node::Custom(name, args) => Node::Custom(name.clone(), args.iter().map(|arg| arg.as_ref().map(sub)).collect()),
        Node::Steps(list) => Node::Steps(boxed(list)),
        Node::ToAssistant(message, context) => {
            Node::ToAssistant(boxed(message), context.as_ref().map(|context| boxed(context)))
        }
        Node::OpenUrl(url) => Node::OpenUrl(boxed(url)),
        Node::Set(variable, value) => Node::Set(variable.clone(), Arc::new(sub(value))),
        node => node.clone(),
    }
}

/// A built-in function (§3.4) on its evaluated arguments.
pub fn builtin_function(builtin: Builtin, args: &[Value]) -> Value {
    let arg = |index: usize| args.get(index).unwrap_or(&Value::Null);
    let array = arg(0).as_array();
    let numbers = || array.unwrap_or_default().iter().map(js::to_number);
    match builtin {
        Builtin::Count => Value::from(array.map_or(0, <[Value]>::len)),
        Builtin::First => array.and_then(<[Value]>::first).cloned().unwrap_or_default(),
        Builtin::Last => array.and_then(<[Value]>::last).cloned().unwrap_or_default(),
        Builtin::Sum => Value::Number(numbers().sum()),
        Builtin::Avg => match array {
            Some(items) if !items.is_empty() => Value::Number(numbers().sum::<f64>() / items.len() as f64),
            _ => Value::Number(0.0),
        },
        Builtin::Min => Value::Number(numbers().reduce(f64::min).unwrap_or(0.0)),
        Builtin::Max => Value::Number(numbers().reduce(f64::max).unwrap_or(0.0)),
        Builtin::Sort => sort(arg(0), arg(1), arg(2)),
        Builtin::Filter => filter(arg(0), arg(1), arg(2), arg(3)),
        Builtin::Round => {
            let number = js::to_number(arg(0));
            let decimals = if arg(1).is_null() { 0.0 } else { js::to_number(arg(1)) };
            let factor = 10f64.powf(decimals);
            Value::Number(js::round(number * factor) / factor)
        }
        Builtin::Abs => Value::Number(js::to_number(arg(0)).abs()),
        Builtin::Floor => Value::Number(js::to_number(arg(0)).floor()),
        Builtin::Ceil => Value::Number(js::to_number(arg(0)).ceil()),
        Builtin::Each
        | Builtin::Run
        | Builtin::ToAssistant
        | Builtin::OpenUrl
        | Builtin::Set
        | Builtin::Reset
        | Builtin::Action => Value::Null,
    }
}

/// A field by dot path: `"customer.name"`.
fn field_at(item: &Value, path: &str) -> Value {
    let mut parts = path.split('.');
    let first = js::property(item, parts.next().unwrap_or_default());
    parts.fold(first, |value, part| js::property(&value, part))
}

fn sort(array: &Value, field: &Value, direction: &Value) -> Value {
    let Value::Array(items) = array else { return array.clone() };
    let field = if field.is_null() { String::new() } else { js::to_string(field) };
    let descending = !direction.is_null() && js::to_string(direction) == "desc";
    struct Key {
        number: Option<f64>,
        text: String,
    }
    let key = |item: &Value| {
        let value = if field.is_empty() { item.clone() } else { field_at(item, &field) };
        let number = match &value {
            Value::Number(number) => Some(*number),
            Value::String(text) if !text.is_empty() && !js::string_to_number(text).is_nan() => {
                Some(js::string_to_number(text))
            }
            _ => None,
        };
        let text = if value.is_null() { String::new() } else { js::to_string(&value) };
        Key { number, text }
    };
    let mut keyed: Vec<(Key, Value)> = items.iter().map(|item| (key(item), item.clone())).collect();
    merge_sort(&mut keyed, &|(a, _), (b, _)| {
        let order = match (a.number, b.number) {
            (Some(a), Some(b)) => (a - b).partial_cmp(&0.0).unwrap_or(Ordering::Equal),
            _ => js::locale_compare(&a.text, &b.text),
        };
        if descending { order.reverse() } else { order }
    });
    Value::Array(keyed.into_iter().map(|(_, item)| item).collect())
}

/// A stable sort that keeps going whatever the comparison says.
fn merge_sort<T>(items: &mut Vec<T>, compare: &dyn Fn(&T, &T) -> Ordering) {
    if items.len() < 2 {
        return;
    }
    let mut right = items.split_off(items.len() / 2);
    merge_sort(items, compare);
    merge_sort(&mut right, compare);
    let left = std::mem::take(items);
    items.reserve(left.len() + right.len());
    let mut left = left.into_iter().peekable();
    let mut right = right.into_iter().peekable();
    loop {
        let take_right = match (left.peek(), right.peek()) {
            (Some(a), Some(b)) => compare(b, a) == Ordering::Less,
            (Some(_), None) => false,
            (None, Some(_)) => true,
            (None, None) => break,
        };
        let next = if take_right { right.next() } else { left.next() };
        items.extend(next);
    }
}

fn filter(array: &Value, field: &Value, operator: &Value, value: &Value) -> Value {
    let Value::Array(items) = array else { return Value::Array(Vec::new()) };
    let field = if field.is_null() { String::new() } else { js::to_string(field) };
    let operator = if operator.is_null() { "==".to_owned() } else { js::to_string(operator) };
    let numbers = |test: fn(f64, f64) -> bool, item: &Value| test(js::to_number(item), js::to_number(value));
    let text = |value: &Value| if value.is_null() { String::new() } else { js::to_string(value) };
    let kept = items.iter().filter(|item| {
        let item = if field.is_empty() { (*item).clone() } else { field_at(item, &field) };
        match operator.as_str() {
            "==" => js::loose_eq(&item, value),
            "!=" => !js::loose_eq(&item, value),
            ">" => numbers(|a, b| a > b, &item),
            "<" => numbers(|a, b| a < b, &item),
            ">=" => numbers(|a, b| a >= b, &item),
            "<=" => numbers(|a, b| a <= b, &item),
            "contains" => text(&item).contains(&text(value)),
            _ => false,
        }
    });
    Value::Array(kept.cloned().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(text: &str) -> Value {
        Value::from_json_str(text).unwrap()
    }

    #[test]
    fn computes_the_builtins() {
        let rows = json(r#"[{"n": 1, "s": "a"}, {"n": "3", "s": "b"}, {"n": 2, "s": "ab"}]"#);
        let n = |text: &str| Value::from(text);
        assert_eq!(builtin_function(Builtin::Count, std::slice::from_ref(&rows)), Value::from(3));
        assert_eq!(builtin_function(Builtin::Count, &[n("x")]), Value::from(0));
        assert_eq!(builtin_function(Builtin::Sum, &[json(r#"[1, "2", true, null]"#)]), Value::from(4));
        assert_eq!(builtin_function(Builtin::Avg, &[json("[]")]), Value::from(0));
        assert_eq!(builtin_function(Builtin::Min, &[json(r#"[3, "1", 2]"#)]), Value::from(1));
        assert_eq!(builtin_function(Builtin::Max, &[json("[]")]), Value::from(0));
        assert_eq!(builtin_function(Builtin::First, &[json("[]")]), Value::Null);
        assert_eq!(builtin_function(Builtin::Round, &[Value::from(2.345), Value::from(2)]), Value::from(2.35));
        assert_eq!(builtin_function(Builtin::Round, &[Value::from(-2.5)]), Value::from(-2));
        assert_eq!(
            builtin_function(Builtin::Filter, &[rows.clone(), n("n"), n(">="), Value::from(2)]),
            json(r#"[{"n": "3", "s": "b"}, {"n": 2, "s": "ab"}]"#)
        );
        assert_eq!(
            builtin_function(Builtin::Filter, &[rows.clone(), n("s"), n("contains"), n("a")]).as_array().unwrap().len(),
            2
        );
        assert_eq!(builtin_function(Builtin::Filter, &[rows.clone(), n("s"), n("~"), n("a")]), json("[]"));
        assert_eq!(
            builtin_function(Builtin::Filter, &[json("[1, \"1\", 2]"), n(""), Value::Null, Value::from(1)]),
            json(r#"[1, "1"]"#)
        );
        assert_eq!(
            builtin_function(Builtin::Sort, &[rows, n("n"), n("desc")]),
            json(r#"[{"n": "3", "s": "b"}, {"n": 2, "s": "ab"}, {"n": 1, "s": "a"}]"#)
        );
        let nested = json(r#"[{"c": {"name": "b"}}, {"c": {"name": "a"}}]"#);
        assert_eq!(
            builtin_function(Builtin::Sort, &[nested, n("c.name")]),
            json(r#"[{"c": {"name": "a"}}, {"c": {"name": "b"}}]"#)
        );
    }

    #[test]
    fn sorts_mixed_values_without_panicking() {
        let mixed = json(r#"["b", 2, null, "10", {"a": 1}, "a", 1, "x", "9"]"#);
        let sorted = builtin_function(Builtin::Sort, &[mixed, Value::from("")]);
        assert_eq!(sorted.as_array().unwrap().len(), 9);
    }
}
