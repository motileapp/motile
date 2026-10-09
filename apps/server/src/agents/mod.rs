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

use motile_protocol::wire::{Access, Agent, Approval, Item, ItemKind, Subagent, Tokens, ToolCall, TurnSummary};

const SHOWING_MEDIA: &str = "You can show the user an image or a video by embedding it in your reply as a \
     Markdown image with the absolute path of the file, like ![what it shows](/path/to/file.png).";

/// Told to an agent when it starts: where it runs, and that it shows what it made instead of
/// naming a file. Claude Code knows its own model and effort; Codex is told them.
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

/// How much of what was said before a session's first prompt is told, from the latest back.
const EARLIER_CONVERSATION_CHARS: usize = 60_000;
const TOOL_INPUT_CHARS: usize = 300;

/// The prompt that starts a new session of a thread, with what was said in the thread before
/// its last message, which is the prompt's. A thread that moved to an account without its
/// session goes on from there.
pub fn with_earlier_conversation(items: &[Item], prompt: String) -> String {
    let last_message = items.iter().rposition(|item| matches!(item.kind, ItemKind::User { .. }));
    let earlier = &items[..last_message.unwrap_or_default()];
    let mut told: Vec<String> = Vec::new();
    let mut length = 0;
    for item in earlier.iter().rev().filter(|item| item.parent.is_none()) {
        let said = match &item.kind {
            ItemKind::User { text, .. } if !text.is_empty() => format!("User:\n{text}"),
            ItemKind::Assistant { text } if !text.is_empty() => format!("You:\n{text}"),
            ItemKind::Tool { call } => {
                let input: String = call.input.chars().take(TOOL_INPUT_CHARS).collect();
                format!("You used {}: {input}", call.name)
            }
            _ => continue,
        };
        length += said.len();
        if length > EARLIER_CONVERSATION_CHARS {
            told.push("(What was said before this is left out.)".to_string());
            break;
        }
        told.push(said);
    }
    if told.is_empty() {
        return prompt;
    }
    told.reverse();
    format!(
        "This thread went on in another session, which you can't see. Here is what was said there, \
         oldest first:\n\n<earlier_conversation>\n{}\n</earlier_conversation>\n\nThe user's new message:\n\n{prompt}",
        told.join("\n\n")
    )
}

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
        /// The turn was stopped for a message the agent was given, which it answers next.
        preempted: bool,
    },
    /// What the agent spent, by model. Claude Code counts from the start of its session
    /// (`total`), Codex since it last said.
    Usage {
        spent: Vec<ModelUsage>,
        total: bool,
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

/// A prompt for a process whose turn has ended; it starts the next one. `id` comes back in
/// `Taken`. `None` while the agent's own session isn't known yet.
pub fn input(agent: Agent, session_id: Option<&str>, prompt: &str, id: &str) -> Option<String> {
    match agent {
        Agent::Claude => Some(claude::input(prompt, id)),
        Agent::Codex => Some(codex::input(session_id?, prompt, id)),
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

#[cfg(test)]
mod tests {
    use motile_protocol::wire::ToolStatus;

    use super::*;

    fn item(kind: ItemKind) -> Item {
        Item { id: String::new(), seq: 0, rev: 0, created_at: 0.0, media: vec![], parent: None, kind }
    }

    fn user(text: &str) -> Item {
        item(ItemKind::User { text: text.into(), attachments: vec![] })
    }

    #[test]
    fn a_new_session_is_told_what_was_said_before_the_last_message() {
        let call = ToolCall {
            id: "t".into(),
            name: "Bash".into(),
            input: r#"{"command":"ls"}"#.into(),
            output: Some("a lot".into()),
            status: ToolStatus::Succeeded,
            agent: None,
        };
        let items = [
            user("Fix the login"),
            item(ItemKind::Thinking { text: "hmm".into() }),
            item(ItemKind::Tool { call }),
            item(ItemKind::Assistant { text: "Fixed it.".into() }),
            user("Now the tests"),
        ];
        let prompt = with_earlier_conversation(&items, "Now the tests".into());
        let told = "User:\nFix the login\n\nYou used Bash: {\"command\":\"ls\"}\n\nYou:\nFixed it.";
        assert!(prompt.contains(told), "{prompt}");
        assert!(prompt.ends_with("The user's new message:\n\nNow the tests"));
        assert!(!prompt.contains("hmm") && !prompt.contains("a lot"));
    }

    #[test]
    fn a_thread_with_nothing_said_before_starts_with_the_prompt_alone() {
        assert_eq!(with_earlier_conversation(&[user("Hello")], "Hello".into()), "Hello");
    }

    #[test]
    fn only_the_latest_of_a_long_conversation_is_told() {
        let long = "x".repeat(EARLIER_CONVERSATION_CHARS / 2);
        let items = [user("opening"), user(&long), user(&long), user("last")];
        let prompt = with_earlier_conversation(&items, "last".into());
        assert!(prompt.contains("left out") && !prompt.contains("opening"));
    }
}
