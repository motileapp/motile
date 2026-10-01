//! `claude -p --output-format stream-json --verbose --include-partial-messages`.

use motile_protocol::wire::{Access, Denial, TurnSummary};
use serde_json::Value;

use super::{AgentEvent, Turn};

pub fn arguments(turn: &Turn) -> Vec<String> {
    let mode = match (turn.plan, turn.access) {
        (true, _) => "plan",
        (false, Access::Supervised) => "default",
        (false, Access::AcceptEdits) => "acceptEdits",
        (false, Access::Auto) => "auto",
        (false, Access::Full) => "bypassPermissions",
    };
    let mut arguments: Vec<String> =
        ["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--permission-mode", mode]
            .map(String::from)
            .into();
    if let Some(model) = turn.model {
        arguments.extend(["--model".to_string(), model.to_string()]);
    }
    if let Some(effort) = turn.effort {
        arguments.extend(["--effort".to_string(), effort.to_string()]);
    }
    if let Some(session_id) = turn.session_id {
        arguments.extend(["--resume".to_string(), session_id.to_string()]);
    }
    // Variadic, so it must come last.
    if !turn.allowed_tools.is_empty() {
        arguments.push("--allowedTools".to_string());
        arguments.extend(turn.allowed_tools.iter().cloned());
    }
    arguments
}

/// The `--allowedTools` rule that grants exactly the denied call.
pub fn allow_rule(denial: &Denial) -> String {
    if denial.tool_name != "Bash" {
        return denial.tool_name.clone();
    }
    let input: Value = serde_json::from_str(&denial.input).unwrap_or_default();
    match input["command"].as_str() {
        Some(command) => format!("Bash({command})"),
        None => denial.tool_name.clone(),
    }
}

#[derive(Default)]
pub struct Parser {
    message_id: String,
    open_block: Option<(u64, String)>,
    anonymous_blocks: u64,
}

impl Parser {
    pub fn parse(&mut self, line: &str) -> Vec<AgentEvent> {
        let Ok(object) = serde_json::from_str::<Value>(line) else { return vec![] };
        // Only the main conversation is shown, not what subagents do.
        if object["parent_tool_use_id"].is_string() {
            return vec![];
        }
        match object["type"].as_str() {
            Some("system") => self.parse_system(&object),
            Some("stream_event") => self.parse_stream_event(&object["event"]),
            Some("assistant") => self.parse_assistant(&object["message"]),
            Some("user") => parse_user(&object["message"]),
            Some("result") => vec![parse_result(&object)],
            _ => vec![],
        }
    }

    fn parse_system(&self, object: &Value) -> Vec<AgentEvent> {
        if object["subtype"] != "init" {
            return vec![];
        }
        let Some(session_id) = object["session_id"].as_str() else { return vec![] };
        vec![AgentEvent::Session { id: session_id.to_string() }]
    }

    fn block_id(&self, index: u64) -> String {
        format!("{}#{index}", self.message_id)
    }

    fn parse_stream_event(&mut self, event: &Value) -> Vec<AgentEvent> {
        let index = event["index"].as_u64().unwrap_or(0);
        match event["type"].as_str() {
            Some("message_start") => {
                self.message_id = event["message"]["id"].as_str().map(String::from).unwrap_or_else(random_id);
                vec![]
            }
            Some("content_block_start") => {
                let block = &event["content_block"];
                let block_type = block["type"].as_str().unwrap_or_default().to_string();
                self.open_block = Some((index, block_type.clone()));
                match block_type.as_str() {
                    "text" => vec![AgentEvent::TextStarted { id: self.block_id(index) }],
                    "thinking" | "redacted_thinking" => vec![AgentEvent::Thinking { active: true }],
                    "tool_use" => match (block["id"].as_str(), block["name"].as_str()) {
                        (Some(id), Some(name)) => {
                            vec![AgentEvent::ToolStarted { id: id.to_string(), name: name.to_string() }]
                        }
                        _ => vec![],
                    },
                    _ => vec![],
                }
            }
            Some("content_block_delta") => {
                let delta = &event["delta"];
                match (delta["type"].as_str(), delta["text"].as_str()) {
                    (Some("text_delta"), Some(text)) => {
                        vec![AgentEvent::TextDelta { id: self.block_id(index), text: text.to_string() }]
                    }
                    _ => vec![],
                }
            }
            Some("content_block_stop") => {
                let was_thinking =
                    matches!(&self.open_block, Some((_, kind)) if kind == "thinking" || kind == "redacted_thinking");
                self.open_block = None;
                if was_thinking { vec![AgentEvent::Thinking { active: false }] } else { vec![] }
            }
            _ => vec![],
        }
    }

    fn parse_assistant(&mut self, message: &Value) -> Vec<AgentEvent> {
        let message_id = message["id"].as_str().unwrap_or_default();
        let blocks = message["content"].as_array().map(Vec::as_slice).unwrap_or_default();
        let mut events = Vec::new();
        for block in blocks {
            match block["type"].as_str() {
                Some("text") => {
                    let Some(text) = block["text"].as_str() else { continue };
                    events.push(AgentEvent::Text { id: self.completed_block_id(message_id), text: text.to_string() });
                }
                Some("thinking") => {
                    let Some(text) = block["thinking"].as_str().filter(|text| !text.is_empty()) else { continue };
                    let id = self.completed_block_id(message_id);
                    events.push(AgentEvent::ThinkingText { id, text: text.to_string() });
                }
                Some("tool_use") => {
                    let (Some(id), Some(name)) = (block["id"].as_str(), block["name"].as_str()) else { continue };
                    events.push(AgentEvent::ToolInput {
                        id: id.to_string(),
                        name: name.to_string(),
                        input: object_json(&block["input"]),
                    });
                }
                _ => {}
            }
        }
        events
    }

    /// The id of the block that streamed this content, when partial messages are on.
    fn completed_block_id(&mut self, message_id: &str) -> String {
        if let (true, Some((index, _))) = (message_id == self.message_id, &self.open_block) {
            return self.block_id(*index);
        }
        self.anonymous_blocks += 1;
        format!("{message_id}#done{}", self.anonymous_blocks)
    }
}

fn parse_user(message: &Value) -> Vec<AgentEvent> {
    let blocks = message["content"].as_array().map(Vec::as_slice).unwrap_or_default();
    blocks
        .iter()
        .filter(|block| block["type"] == "tool_result")
        .filter_map(|block| {
            Some(AgentEvent::ToolResult {
                id: block["tool_use_id"].as_str()?.to_string(),
                output: content_text(&block["content"]),
                is_error: block["is_error"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

fn parse_result(object: &Value) -> AgentEvent {
    let denials = object["permission_denials"].as_array().map(Vec::as_slice).unwrap_or_default();
    let denials = denials
        .iter()
        .filter_map(|denial| {
            Some(Denial {
                tool_name: denial["tool_name"].as_str()?.to_string(),
                tool_use_id: denial["tool_use_id"].as_str().unwrap_or_default().to_string(),
                input: object_json(&denial["tool_input"]),
            })
        })
        .collect();
    let summary = TurnSummary {
        duration_ms: object["duration_ms"].as_u64(),
        cost_usd: object["total_cost_usd"].as_f64(),
        is_error: object["is_error"].as_bool().unwrap_or(object["subtype"] != "success"),
        stopped: false,
        denials,
    };
    AgentEvent::Completed { summary, result_text: object["result"].as_str().map(String::from) }
}

fn content_text(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }
    let blocks = content.as_array().map(Vec::as_slice).unwrap_or_default();
    let parts: Vec<&str> = blocks
        .iter()
        .filter_map(|block| match block["type"].as_str() {
            Some("text") => block["text"].as_str(),
            Some("image") => Some("[image]"),
            _ => None,
        })
        .collect();
    parts.join("\n")
}

fn object_json(value: &Value) -> String {
    if value.is_object() { value.to_string() } else { "{}".to_string() }
}

fn random_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
