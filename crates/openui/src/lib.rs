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

pub mod ast;
pub mod builtins;
pub mod check;
pub mod components;
pub mod element;
pub mod error;
pub mod eval;
pub mod fences;
pub mod format;
pub mod functions;
pub mod js;
pub mod lexer;
pub mod library;
pub mod merge;
pub mod message;
pub mod motile;
pub mod node;
pub mod parser;
pub mod program;
pub mod prompt;
pub mod query;
pub mod rules;
pub mod runtime;
pub mod schema;
pub mod serialize;
pub mod source;
pub mod store;
pub mod value;

pub use element::{ActionPlan, ActionStep, Binding, Element};
pub use error::{Code, Error, Severity, Source};
pub use library::{
    ActionDef, Args, Call, ComponentDef, Env, FixedEnv, FnError, FunctionDef, FunctionImpl, Group, Library, LibraryDef,
    LibraryError,
};
pub use merge::merge;
pub use parser::{Parsed, Parser, parse};
pub use program::{Lowerer, MutationDecl, Program, QueryDecl};
pub use prompt::{PromptOptions, Tool, ToolSpec, generate_system_prompt};
pub use query::{Timer, ToolCall, ToolError};
pub use runtime::{Effects, Event, FieldSpec, Forms, NoForms, Runtime, Step};
pub use schema::{Field, Schema};
pub use serialize::serialize;
pub use value::{Map, Value};
