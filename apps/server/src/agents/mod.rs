//! Running Claude Code and Codex: the command for a turn and the events read from its output.
//! Both CLIs print one JSON object per line; `claude.rs` and `codex.rs` turn those into the same
//! `AgentEvent`s so the rest of the server doesn't care which agent is running.

pub mod claude;
pub mod codex;
pub mod environment;
pub mod models;

use motile_protocol::wire::{Access, Agent, Approval, ToolCall, TurnSummary};

/// Told to every agent, so that it shows what it made instead of naming a file. Codex takes it
/// inside a quoted setting, so it has no quotes of its own.
pub const SHOWING_MEDIA: &str = "You can show the user an image or a video by embedding it in your reply as a \
     Markdown image with the absolute path of the file, like ![what it shows](/path/to/file.png).";

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
    /// A tool call the turn waits with until the user has allowed or refused it.
    Approval(Approval),
    /// The agent no longer waits for that answer.
    ApprovalWithdrawn {
        id: String,
    },
    /// What the agent has running in the background, whenever that changes.
    Background(Background),
    /// Another turn started in the same process: for a prompt it was given, or by itself for
    /// something it monitors.
    Woke,
    Failed {
        message: String,
    },
}

/// The work that outlives a turn. Claude Code's process stays until all of it has ended.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Background {
    /// Commands it watches: monitors and shells left running.
    pub watches: usize,
    pub agents: usize,
}

impl Background {
    pub fn is_empty(&self) -> bool {
        self.watches == 0 && self.agents == 0
    }
}

pub struct Turn<'a> {
    pub agent: Agent,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub access: Access,
    pub plan: bool,
    pub session_id: Option<&'a str>,
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

/// The prompt as the agent reads it from stdin.
pub fn input(agent: Agent, prompt: &str) -> String {
    match agent {
        Agent::Claude => claude::input(prompt),
        Agent::Codex => prompt.to_string(),
    }
}

/// Whether the agent's process reads more from stdin than its first prompt.
pub fn takes_more_input(agent: Agent) -> bool {
    agent == Agent::Claude
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
