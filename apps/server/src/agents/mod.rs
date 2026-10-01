//! Running Claude Code and Codex: the command for a turn and the events read from its output.
//! Both CLIs print one JSON object per line; `claude.rs` and `codex.rs` turn those into the same
//! `AgentEvent`s so the rest of the host doesn't care which agent is running.

pub mod claude;
pub mod codex;
pub mod environment;
pub mod models;

use motile_protocol::wire::{Access, Agent, ToolCall, TurnSummary};

#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    Session {
        id: String,
    },
    TextStarted {
        id: String,
    },
    TextDelta {
        id: String,
        text: String,
    },
    /// The final text of a block; sent even when nothing streamed.
    Text {
        id: String,
        text: String,
    },
    Thinking {
        active: bool,
    },
    ThinkingText {
        id: String,
        text: String,
    },
    ToolStarted {
        id: String,
        name: String,
    },
    ToolInput {
        id: String,
        name: String,
        input: String,
    },
    ToolResult {
        id: String,
        output: String,
        is_error: bool,
    },
    /// A tool call reported whole, as Codex does.
    Tool {
        call: ToolCall,
    },
    Completed {
        summary: TurnSummary,
        result_text: Option<String>,
    },
    Failed {
        message: String,
    },
}

pub struct Turn<'a> {
    pub agent: Agent,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub access: Access,
    pub plan: bool,
    pub session_id: Option<&'a str>,
    pub allowed_tools: &'a [String],
}

impl Turn<'_> {
    /// Arguments for one turn. The prompt goes over stdin.
    pub fn arguments(&self) -> Vec<String> {
        match self.agent {
            Agent::Claude => claude::arguments(self),
            Agent::Codex => codex::arguments(self),
        }
    }
}

pub enum Parser {
    Claude(claude::Parser),
    Codex(codex::Parser),
}

impl Parser {
    pub fn new(agent: Agent) -> Self {
        match agent {
            Agent::Claude => Self::Claude(claude::Parser::default()),
            Agent::Codex => Self::Codex(codex::Parser::default()),
        }
    }

    pub fn parse(&mut self, line: &str) -> Vec<AgentEvent> {
        match self {
            Self::Claude(parser) => parser.parse(line),
            Self::Codex(parser) => parser.parse(line),
        }
    }
}

pub fn executable_name(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude",
        Agent::Codex => "codex",
    }
}
