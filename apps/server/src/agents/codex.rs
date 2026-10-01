//! `codex exec --json`. Codex reports whole items rather than streaming text.

use motile_protocol::wire::{Access, ToolCall, ToolStatus, TurnSummary};
use serde_json::{Value, json};

use super::{AgentEvent, Turn};

pub fn arguments(turn: &Turn) -> Vec<String> {
    let mut arguments: Vec<String> = ["exec", "--json", "--skip-git-repo-check"].map(String::from).into();
    match (turn.plan, turn.access) {
        // `codex exec` can't stop to ask, so a supervised turn only reads.
        (true, _) | (false, Access::Supervised) => arguments.extend(["-s".to_string(), "read-only".to_string()]),
        (false, Access::AcceptEdits) => arguments.extend(["-s".to_string(), "workspace-write".to_string()]),
        (false, Access::Auto) => arguments.push("--approve-for-me".to_string()),
        (false, Access::Full) => arguments.push("--dangerously-bypass-approvals-and-sandbox".to_string()),
    }
    if let Some(effort) = turn.effort {
        arguments.extend(["-c".to_string(), format!("model_reasoning_effort=\"{effort}\"")]);
    }
    if let Some(model) = turn.model {
        arguments.extend(["-m".to_string(), model.to_string()]);
    }
    // The options above belong to `exec`, so they come before `resume`.
    if let Some(session_id) = turn.session_id {
        arguments.extend(["resume".to_string(), session_id.to_string()]);
    }
    // Read the prompt from stdin.
    arguments.push("-".to_string());
    arguments
}

/// Codex runs commands as `/bin/bash -lc '<command>'`; this is the command inside.
fn shell_command(command: &str) -> String {
    let Some(quoted) = ["/bin/bash -lc ", "bash -lc ", "/bin/zsh -lc ", "/bin/sh -c "]
        .iter()
        .find_map(|prefix| command.strip_prefix(prefix))
    else {
        return command.to_string();
    };
    if let Some(inner) = quoted.strip_prefix('\'').and_then(|rest| rest.strip_suffix('\'')) {
        return inner.replace("'\\''", "'");
    }
    let Some(inner) = quoted.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')) else {
        return quoted.to_string();
    };
    let mut unescaped = String::new();
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        match (character, characters.clone().next()) {
            ('\\', Some(next @ ('"' | '\\' | '$' | '`'))) => {
                unescaped.push(next);
                characters.next();
            }
            _ => unescaped.push(character),
        }
    }
    unescaped
}

pub struct Parser {
    /// Codex numbers items from zero in every turn; this keeps their ids apart within a chat.
    turn_id: String,
}

impl Default for Parser {
    fn default() -> Self {
        Self { turn_id: uuid::Uuid::new_v4().simple().to_string() }
    }
}

impl Parser {
    pub fn parse(&mut self, line: &str) -> Vec<AgentEvent> {
        let Ok(object) = serde_json::from_str::<Value>(line) else { return vec![] };
        match object["type"].as_str() {
            Some("thread.started") => match object["thread_id"].as_str() {
                Some(id) => vec![AgentEvent::Session { id: id.to_string() }],
                None => vec![],
            },
            Some("turn.started") => vec![AgentEvent::Thinking { active: true }],
            Some("item.started" | "item.updated" | "item.completed") => {
                self.parse_item(&object["item"], object["type"] == "item.completed")
            }
            Some("turn.completed") => {
                vec![AgentEvent::Completed { summary: TurnSummary::default(), result_text: None }]
            }
            Some("turn.failed") => vec![AgentEvent::Completed {
                summary: TurnSummary { is_error: true, ..Default::default() },
                result_text: object["error"]["message"].as_str().map(String::from),
            }],
            Some("error") => match object["message"].as_str() {
                Some(message) => vec![AgentEvent::Failed { message: message.to_string() }],
                None => vec![],
            },
            _ => vec![],
        }
    }

    fn parse_item(&self, item: &Value, completed: bool) -> Vec<AgentEvent> {
        let Some(item_id) = item["id"].as_str() else { return vec![] };
        let id = format!("{}:{item_id}", self.turn_id);
        let text = || item["text"].as_str().unwrap_or_default().to_string();
        let tool = |name: &str, input: Value, output: Option<String>| {
            let status = match item["status"].as_str() {
                Some("failed" | "declined") => ToolStatus::Failed,
                Some("in_progress") => ToolStatus::Running,
                _ if completed => ToolStatus::Succeeded,
                _ => ToolStatus::Running,
            };
            let call = ToolCall { id: id.clone(), name: name.to_string(), input: input.to_string(), output, status };
            vec![AgentEvent::Thinking { active: false }, AgentEvent::Tool { call }]
        };

        match item["type"].as_str() {
            Some("agent_message") => {
                vec![AgentEvent::Thinking { active: false }, AgentEvent::Text { id, text: text() }]
            }
            Some("reasoning") => vec![AgentEvent::ThinkingText { id, text: text() }],
            Some("command_execution") => {
                let output = item["aggregated_output"].as_str().filter(|output| !output.is_empty() || completed);
                let mut events = tool(
                    "Bash",
                    json!({ "command": shell_command(item["command"].as_str().unwrap_or_default()) }),
                    output.map(String::from),
                );
                if let (Some(code), Some(AgentEvent::Tool { call })) = (item["exit_code"].as_i64(), events.last_mut())
                    && code != 0
                {
                    call.status = ToolStatus::Failed;
                }
                events
            }
            Some("file_change") => {
                let changes = item["changes"].as_array().map(Vec::as_slice).unwrap_or_default();
                let lines: Vec<String> = changes
                    .iter()
                    .map(|change| {
                        let kind = change["kind"].as_str().unwrap_or("update");
                        format!("{kind} {}", change["path"].as_str().unwrap_or_default())
                    })
                    .collect();
                let file_path = changes.first().and_then(|change| change["path"].as_str()).unwrap_or_default();
                let output = completed.then(|| lines.join("\n"));
                tool("Edit", json!({ "file_path": file_path, "changes": item["changes"] }), output)
            }
            Some("mcp_tool_call") => {
                let name = format!(
                    "mcp__{}__{}",
                    item["server"].as_str().unwrap_or_default(),
                    item["tool"].as_str().unwrap_or_default()
                );
                let output = match (item["error"]["message"].as_str(), &item["result"]) {
                    (Some(message), _) => Some(message.to_string()),
                    (None, Value::Null) => None,
                    (None, result) => Some(result.to_string()),
                };
                let input = if item["arguments"].is_object() { item["arguments"].clone() } else { json!({}) };
                tool(&name, input, output)
            }
            Some("web_search") => tool("WebSearch", json!({ "query": item["query"] }), None),
            Some("todo_list") => {
                let items = item["items"].as_array().map(Vec::as_slice).unwrap_or_default();
                let todos: Vec<Value> = items
                    .iter()
                    .map(|todo| {
                        let status = if todo["completed"] == true { "completed" } else { "pending" };
                        json!({ "content": todo["text"], "status": status })
                    })
                    .collect();
                tool("TodoWrite", json!({ "todos": todos }), None)
            }
            Some("error") => match item["message"].as_str() {
                Some(message) if completed => vec![AgentEvent::Failed { message: message.to_string() }],
                _ => vec![],
            },
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_wrapper_is_removed_from_commands() {
        assert_eq!(shell_command("/bin/bash -lc 'cat greet.py'"), "cat greet.py");
        assert_eq!(
            shell_command(r#"/bin/bash -lc "pwd; rg -g 'AGENTS.md' \"a b\" $HOME""#),
            r#"pwd; rg -g 'AGENTS.md' "a b" $HOME"#
        );
        assert_eq!(shell_command("ls -la"), "ls -la");
    }
}
