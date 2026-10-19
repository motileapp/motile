//! The lowered program: references resolved, components checked against the library, and
//! everything that can't change folded into values. What is left reads state, query results or
//! loop variables, and the evaluator computes it again when they change.

use std::sync::Arc;

use crate::ast::{BinaryOp, UnaryOp};
use crate::builtins::Builtin;
use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A value that never changes: data, a component whose props never change, an action plan.
    Lit(Value),
    /// An unresolved reference, or a `Query` in a value position: left out of arrays, else null.
    Nothing,
    Array(Vec<Node>),
    Object(Vec<(String, Node)>),
    Element(Arc<ElementNode>),
    /// `$name`.
    State(String),
    /// A reference to a query statement: its result.
    Query(String),
    /// A reference to a mutation statement: `{status, data, error}`.
    Mutation(String),
    /// `@Each`'s loop variable.
    Loop(String),
    Unary(UnaryOp, Box<Node>),
    Binary(BinaryOp, Box<Node>, Box<Node>),
    Ternary(Box<Node>, Box<Node>, Box<Node>),
    Member(Box<Node>, String),
    Index(Box<Node>, Box<Node>),
    /// `$name = expression`: a binding's write, read as the variable elsewhere.
    Assign(String, Arc<Node>),
    /// A built-in function but `@Each`, its arguments by position.
    Builtin(Builtin, Vec<Node>),
    /// `@Each(array, varName, template)`.
    Each(Box<Node>, String, Arc<Node>),
    /// A library function, its arguments by param, None where absent.
    Function(String, Vec<Option<Node>>),
    /// A library action, its arguments by param.
    Custom(String, Vec<Option<Node>>),
    /// `Action([...])`: the list of actions.
    Steps(Box<Node>),
    ToAssistant(Box<Node>, Option<Box<Node>>),
    OpenUrl(Box<Node>),
    /// `@Set` inside an `@Each` template: the loop variables are put into the value when the
    /// action is built.
    Set(String, Arc<Node>),
}

/// A component call whose props are not all constant.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementNode {
    pub component: String,
    pub statement: Option<String>,
    /// In the library's order.
    pub props: Vec<(String, Node)>,
    pub partial: bool,
    /// What its props read, nested components included.
    pub deps: Vec<Dep>,
    /// It calls a library function or action, whose result may change on its own.
    pub volatile: bool,
    /// It reads a loop variable.
    pub scoped: bool,
}

/// An input of the evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Dep {
    State(String),
    Query(String),
    Mutation(String),
}

impl Node {
    pub fn null() -> Node {
        Node::Lit(Value::Null)
    }

    pub fn is_lit(&self) -> bool {
        matches!(self, Node::Lit(_))
    }

    pub fn as_lit(&self) -> Option<&Value> {
        let Node::Lit(value) = self else { return None };
        Some(value)
    }

    /// The node's direct children.
    pub fn children(&self) -> Vec<&Node> {
        match self {
            Node::Array(items) => items.iter().collect(),
            Node::Object(entries) => entries.iter().map(|(_, value)| value).collect(),
            Node::Element(element) => element.props.iter().map(|(_, value)| value).collect(),
            Node::Unary(_, operand) | Node::Member(operand, _) | Node::Steps(operand) | Node::OpenUrl(operand) => {
                vec![operand]
            }
            Node::Binary(_, left, right) | Node::Index(left, right) => vec![left, right],
            Node::Ternary(cond, then, otherwise) => vec![cond, then, otherwise],
            Node::Assign(_, value) | Node::Set(_, value) => vec![value],
            Node::Builtin(_, args) => args.iter().collect(),
            Node::Each(array, _, template) => vec![array, template],
            Node::Function(_, args) | Node::Custom(_, args) => args.iter().flatten().collect(),
            Node::ToAssistant(message, context) => {
                std::iter::once(message.as_ref()).chain(context.as_deref()).collect()
            }
            Node::Lit(_) | Node::Nothing | Node::State(_) | Node::Query(_) | Node::Mutation(_) | Node::Loop(_) => {
                Vec::new()
            }
        }
    }

    /// Whether it reads a loop variable.
    pub fn is_scoped(&self) -> bool {
        match self {
            Node::Loop(_) => true,
            Node::Element(element) => element.scoped,
            node => node.children().into_iter().any(Node::is_scoped),
        }
    }

    /// Whether it gives a component in some case: a root such as `$dark ? darkPage : lightPage`.
    pub fn contains_element(&self) -> bool {
        match self {
            Node::Element(_) | Node::Lit(Value::Element(_)) => true,
            node => node.children().into_iter().any(Node::contains_element),
        }
    }

    /// Adds what it reads to `deps`; true when it calls a library function or action.
    pub fn collect_deps(&self, deps: &mut Vec<Dep>) -> bool {
        match self {
            Node::State(name) | Node::Assign(name, _) => {
                deps.push(Dep::State(name.clone()));
                if let Node::Assign(_, value) = self {
                    return value.collect_deps(deps);
                }
                false
            }
            Node::Query(name) => {
                deps.push(Dep::Query(name.clone()));
                false
            }
            Node::Mutation(name) => {
                deps.push(Dep::Mutation(name.clone()));
                false
            }
            Node::Element(element) => {
                deps.extend(element.deps.iter().cloned());
                element.volatile
            }
            Node::Function(_, args) | Node::Custom(_, args) => {
                args.iter().flatten().for_each(|arg| {
                    arg.collect_deps(deps);
                });
                true
            }
            node => node.children().into_iter().fold(false, |volatile, child| child.collect_deps(deps) | volatile),
        }
    }
}

impl ElementNode {
    pub fn new(component: String, props: Vec<(String, Node)>, partial: bool) -> ElementNode {
        let mut deps = Vec::new();
        let volatile = props.iter().fold(false, |volatile, (_, node)| node.collect_deps(&mut deps) | volatile);
        deps.sort();
        deps.dedup();
        let scoped = props.iter().any(|(_, node)| node.is_scoped());
        ElementNode { component, statement: None, props, partial, deps, volatile, scoped }
    }
}
