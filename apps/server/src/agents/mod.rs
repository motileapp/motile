//! Running Claude Code and Codex: the command for a turn, the lines its process is given, and the
//! events read from its output. Both CLIs print one JSON object per line; `claude.rs` and
//! `codex.rs` turn those into the same `AgentEvent`s so the rest of the server doesn't care which
//! agent is running.

pub mod ask;
pub mod claude;
pub mod codex;
pub mod environment;
pub mod models;

use std::collections::HashMap;

use motile_protocol::wire::{Access, Agent, Approval, Subagent, Tokens, ToolCall, TurnSummary};

use crate::handoff::Handoff;
use crate::mcp::McpAccess;

const SHOWING_MEDIA: &str = "You can show the user an image or a video by embedding it in your reply as a \
     Markdown image with the absolute path of the file, like ![what it shows](/path/to/file.png). Give a \
     video a preview image as its title, like ![what it shows](/path/to/video.mp4 \"/path/to/preview.png\"), \
     so that it shows before it plays. Any other file embedded the same way, like \
     ![The report](/path/to/report.pdf), is sent to the user to download; send a folder as an archive.";

/// Told to an agent when it starts: where it runs, and that it shows or sends what it made instead
/// of naming a file. Claude Code knows its own model and effort; Codex is told them.
pub fn instructions(turn: &Turn) -> String {
    let harness = match turn.agent {
        Agent::Claude => "Claude Code",
        Agent::Codex => "Codex",
    };
    let mut text = format!("In case you're asked: you are running in Motile through the {harness} harness");
    if turn.agent == Agent::Codex {
        if let Some(model) = turn.model {
            text += &format!(", as {model}");
        }
        if let Some(effort) = turn.effort {
            text += &format!(" with {effort} reasoning effort");
        }
    }
    text + ". No need to mention this otherwise. " + SHOWING_MEDIA
}

/// The agents present a plan with this tool call; allowing it has the plan carried out.
pub const PLAN_TOOL: &str = "ExitPlanMode";

#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    /// The agent's own session, and the model it runs.
    Session {
        id: String,
        model: Option<String>,
    },
    /// The agent has taken the turn it was started for, with what it was told of the thread.
    Accepted,
    /// The session the agent was told to resume isn't there; nothing of the turn was done.
    SessionMissing,
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
        /// The turn was stopped for a message the agent was given, which it answers next.
        preempted: bool,
    },
    /// What the agent spent, by model. Claude Code counts from the start of its session
    /// (`total`), Codex since it last said.
    Usage {
        spent: Vec<ModelUsage>,
        total: bool,
        /// How large the model's context is, and how much of it the session's last request took.
        context_window: Option<u64>,
        context_used: Option<u64>,
    },
    /// A tool call the turn waits with until the user has allowed or refused it.
    Approval(Approval),
    /// The agent no longer waits for that answer.
    ApprovalWithdrawn {
        id: String,
    },
    /// Something an agent did that the thread's agent started with the tool call `parent`.
    Sub {
        parent: String,
        event: Box<AgentEvent>,
    },
    /// How far the agent is that the tool call `tool_id` started. What is `None` is as it was.
    Task {
        tool_id: String,
        agent: Subagent,
    },
    Compacting {
        active: bool,
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
    /// The agent's usage limit ended the turn. `resets_at` is when it resets, when that is known.
    Limited {
        resets_at: Option<f64>,
    },
    /// Lines the agent's process has to be given now.
    Write(String),
    Failed {
        message: String,
    },
}

/// When the agent may work again: once the last of the limits that stopped it resets. `None`
/// when no limit said when it resets.
fn latest_reset(resets: impl IntoIterator<Item = Option<f64>>) -> Option<f64> {
    let resets: Option<Vec<f64>> = resets.into_iter().collect();
    resets?.into_iter().reduce(f64::max)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelUsage {
    pub model: String,
    pub tokens: Tokens,
    /// What the agent says it cost at the API's prices.
    pub cost_usd: Option<f64>,
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
    /// What the agent is told of the thread before the prompt, which Codex is given as messages.
    pub handoff: Option<&'a Handoff>,
    /// Where the agent reads the thread from.
    pub mcp: Option<&'a McpAccess>,
}

/// What a turn of a process that runs already is started with.
#[derive(Clone, Copy, Default)]
pub struct Settings<'a> {
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub plan: bool,
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

/// A prompt for a process whose turn has ended; it starts the next one with `settings`, which
/// Claude Code is told apart. `id` comes back in `Taken`. `None` while the agent's own session
/// isn't known yet.
pub fn input(agent: Agent, session_id: Option<&str>, prompt: &str, id: &str, settings: Settings) -> Option<String> {
    match agent {
        Agent::Claude => Some(claude::input(prompt, id)),
        Agent::Codex => Some(codex::input(session_id?, prompt, id, settings)),
    }
}

/// A prompt for a process whose turn runs, to take at once. `None` while the turn it is for
/// isn't known yet.
pub fn steer(agent: Agent, session_id: Option<&str>, turn_id: Option<&str>, prompt: &str, id: &str) -> Option<String> {
    match agent {
        Agent::Claude => Some(claude::steer(prompt, id)),
        Agent::Codex => Some(codex::steer(session_id?, turn_id?, prompt, id)),
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
    Claude(Box<claude::Parser>),
    Codex(Box<codex::Parser>),
}

impl Parser {
    /// Reads the output of the process started for `turn` with this prompt.
    pub fn new(turn: &Turn, cwd: &str, prompt: &str, prompt_id: &str) -> Self {
        match turn.agent {
            Agent::Claude => Self::Claude(Box::default()),
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
