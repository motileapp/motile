//! The evaluator's values beside data: component instances and action plans.

use std::sync::Arc;

use indexmap::IndexMap;

use crate::node::Node;
use crate::value::{Map, Value};

/// A component instance: the component and its props, mapped from the arguments by position.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub component: String,
    /// The statement whose whole value this is; none for a component written inline.
    pub statement: Option<String>,
    /// The props that have a value; a null prop is left out.
    pub props: Map,
    /// Still streaming.
    pub partial: bool,
    /// The bindable props given a `$variable`: the component shows the prop's value and writes
    /// what the user changes back through the binding.
    pub bindings: IndexMap<String, Binding>,
}

impl Element {
    pub fn new(component: impl Into<String>, props: Map) -> Element {
        Element { component: component.into(), statement: None, props, partial: false, bindings: IndexMap::new() }
    }

    pub fn prop(&self, name: &str) -> Option<&Value> {
        self.props.get(name)
    }
}

/// A two-way binding of a prop to a `$variable`.
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub variable: String,
    /// `$variable = expression` in place of the variable: what is written, with `$value` the
    /// user's value.
    pub write: Option<Arc<Node>>,
}

/// What an action prop runs, its steps in order: `Action([...])`, or one action alone.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActionPlan {
    pub steps: Vec<ActionStep>,
}

/// One step of a plan.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionStep {
    /// `@Run(ref)`: runs a mutation, or fetches a query again.
    Run { statement: String, mutation: bool },
    /// `@ToAssistant(message, context?)`.
    ToAssistant { message: String, context: Option<Value> },
    /// `@OpenUrl(url)`.
    OpenUrl { url: String },
    /// `@Set($variable, value)`: the value is evaluated when the step runs.
    Set { variable: String, value: Arc<Node> },
    /// `@Reset($a, $b, ...)`.
    Reset { variables: Vec<String> },
    /// A library action, its params keyed by name.
    Custom { name: String, params: Map },
}
