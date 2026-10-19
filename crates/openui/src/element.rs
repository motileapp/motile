//! The evaluator's values beside data: component instances and action plans. Their shapes belong to
//! the evaluator, which adds what it needs.

use crate::value::Map;

/// A component instance: the component and its props, mapped from the arguments by position.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub component: String,
    /// The statement whose whole value this is; none for a component written inline.
    pub statement: Option<String>,
    pub props: Map,
    /// Still streaming.
    pub partial: bool,
}

/// What an action prop runs, its steps in order: `Action([...])`, or one action alone.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActionPlan {
    pub steps: Vec<ActionStep>,
}

/// One step of a plan (`@Set`, `@Run`, `@ToAssistant`, a custom action…), defined with the evaluator.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionStep {}
