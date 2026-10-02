//! Running Claude Code and Codex: the command for a turn, the lines its process is given, and the
//! events read from its output. Both CLIs print one JSON object per line; `claude.rs` and
//! `codex.rs` turn those into the same `AgentEvent`s so the rest of the server doesn't care which
//! agent is running.

pub mod claude;
pub mod codex;
pub mod environment;
pub mod models;

use std::collections::HashMap;

use motile_protocol::wire::{Access, Agent, Approval, ToolCall, TurnSummary};

/// Told to every agent, so that it shows what it made instead of naming a file.
pub const SHOWING_MEDIA: &str = "You can show the user an image or a video by embedding it in your reply as a \
     Markdown image with the absolute path of the file, like ![what it shows](/path/to/file.png).";

/// The agents present a plan with this tool call; allowing it has the plan carried out.
pub const PLAN_TOOL: &str = "ExitPlanMode";

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
    /// The agent has read the message with this id, which it was given while it worked.
    Taken {
        id: String,
    },
    /// The agent's own name for the turn that has started, which stopping it takes.
    Turn {
        id: String,
    },
    /// Lines the agent's process has to be given now.
    Write(String),
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
    /// Arguments for the process of a turn. The prompt goes over stdin.
    pub fn arguments(&self) -> Vec<String> {
        match self.agent {
            Agent::Claude => claude::arguments(self),
            Agent::Codex => codex::arguments(),
        }
    }
}

/// The first line a new process is given. Claude Code takes the prompt at once; Codex is given
/// it by its parser, once the thread is there.
pub fn opening(agent: Agent, prompt: &str, id: &str) -> String {
    match agent {
        Agent::Claude => claude::input(prompt, id),
        Agent::Codex => codex::opening(),
    }
}

/// A prompt for a process that is there already. `id` comes back in `Taken` when the agent was
/// working. `None` while the agent's own session isn't known yet.
pub fn input(agent: Agent, session_id: Option<&str>, prompt: &str, id: &str) -> Option<String> {
    match agent {
        Agent::Claude => Some(claude::input(prompt, id)),
        Agent::Codex => Some(codex::input(session_id?, prompt, id)),
    }
}

/// The line that allows or refuses what the process asked about. `answers` is what the user chose
/// when it asked them questions, and `access` what an approved plan is carried out with. `None`
/// when the process needs no answer.
pub fn answer(
    agent: Agent,
    approval: &Approval,
    allow: bool,
    answers: &HashMap<String, String>,
    access: Access,
) -> Option<String> {
    match agent {
        Agent::Claude => Some(claude::answer(approval, allow, answers, access)),
        Agent::Codex => codex::answer(approval, allow, answers),
    }
}

/// The line that asks the process to stop its turn. Claude Code is only ever signalled.
pub fn stop(agent: Agent, session_id: Option<&str>, turn_id: Option<&str>) -> Option<String> {
    match agent {
        Agent::Claude => None,
        Agent::Codex => Some(codex::stop(session_id?, turn_id?)),
    }
}

pub enum Parser {
    Claude(claude::Parser),
    Codex(Box<codex::Parser>),
}

impl Parser {
    /// Reads the output of the process started for `turn` with this prompt.
    pub fn new(turn: &Turn, cwd: &str, prompt: &str, prompt_id: &str) -> Self {
        match turn.agent {
            Agent::Claude => Self::Claude(claude::Parser::default()),
            Agent::Codex => Self::Codex(Box::new(codex::Parser::new(turn, cwd, prompt, prompt_id))),
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
