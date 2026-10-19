//! OpenUI Lang 1.0 (`spec/language.md` and `spec/prompt.md` of thesysdev/openui): the library
//! format, the values programs compute with, and the JavaScript semantics the spec asks every port
//! to reproduce.
//!
//! A [`Value`] is data (null, a boolean, a number, a string, an array, an object with its keys in
//! order) or one of the evaluator's own values, a component instance ([`Element`]) or an action
//! plan ([`ActionPlan`]). They share one enum because a program mixes them freely: a list of
//! children holds strings beside components, and a function such as `@Slice` gets either. Elements
//! and plans act as objects in expressions (`js`) and serialize as null.
//!
//! `fixtures/` holds the spec's conformance cases (`openui/1.0`, thesysdev/openui at 5a85af6) and
//! the library open-intelligent-ui generates (`oiu`, at 0321343), each with its project's licence, and
//! the prompts this crate writes (`prompt`).

pub mod builtins;
pub mod components;
pub mod element;
pub mod format;
pub mod functions;
pub mod js;
pub mod library;
pub mod motile;
pub mod prompt;
pub mod schema;
pub mod value;

pub use element::{ActionPlan, ActionStep, Element};
pub use library::{
    ActionDef, Args, Call, ComponentDef, Env, FixedEnv, FnError, FunctionDef, FunctionImpl, Group, Library, LibraryDef,
    LibraryError,
};
pub use prompt::{PromptOptions, Tool, ToolSpec, generate_system_prompt};
pub use schema::{Field, Schema};
pub use value::{Map, Value};
