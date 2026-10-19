//! The error object of language.md §8.3, as it crosses the wire to the host and the model.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Parser,
    Runtime,
    Query,
    Mutation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Code {
    UnknownComponent,
    MissingRequired,
    NullRequired,
    TypeMismatch,
    ExcessArgs,
    InlineReserved,
    UnknownFunction,
    NoRoot,
    ParseFailed,
    ParseException,
    RuntimeError,
    RenderError,
    ToolNotFound,
    ToolError,
    McpError,
}

impl Code {
    pub fn as_str(self) -> &'static str {
        match self {
            Code::UnknownComponent => "unknown-component",
            Code::MissingRequired => "missing-required",
            Code::NullRequired => "null-required",
            Code::TypeMismatch => "type-mismatch",
            Code::ExcessArgs => "excess-args",
            Code::InlineReserved => "inline-reserved",
            Code::UnknownFunction => "unknown-function",
            Code::NoRoot => "no-root",
            Code::ParseFailed => "parse-failed",
            Code::ParseException => "parse-exception",
            Code::RuntimeError => "runtime-error",
            Code::RenderError => "render-error",
            Code::ToolNotFound => "tool-not-found",
            Code::ToolError => "tool-error",
            Code::McpError => "mcp-error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    pub source: Source,
    pub code: Code,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statement_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Set when the page still renders, as with the fallback entry's `no-root`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity>,
}

impl Error {
    pub fn new(source: Source, code: Code, message: impl Into<String>) -> Error {
        Error {
            source,
            code,
            statement_id: None,
            component: None,
            path: None,
            tool_name: None,
            message: message.into(),
            hint: None,
            severity: None,
        }
    }

    pub fn statement(mut self, statement: Option<&str>) -> Error {
        self.statement_id = statement.map(str::to_owned);
        self
    }

    pub fn component(mut self, component: impl Into<String>) -> Error {
        self.component = Some(component.into());
        self
    }

    pub fn path(mut self, path: impl Into<String>) -> Error {
        let path = path.into();
        self.path = (!path.is_empty()).then_some(path);
        self
    }

    pub fn tool(mut self, tool: impl Into<String>) -> Error {
        self.tool_name = Some(tool.into());
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Error {
        self.hint = Some(hint.into());
        self
    }

    pub fn warning(mut self) -> Error {
        self.severity = Some(Severity::Warning);
        self
    }

    /// Whether the page renders nothing because of it.
    pub fn is_fatal(&self) -> bool {
        matches!(self.code, Code::NoRoot | Code::ParseFailed | Code::ParseException) && self.severity.is_none()
    }
}
