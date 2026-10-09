//! `claude -p --input-format stream-json --output-format stream-json --verbose
//! --include-partial-messages --replay-user-messages --permission-prompt-tool stdio`. Prompts are
//! written to stdin one JSON line each, and the process stays for as long as stdin is open or
//! something it started is still running. A prompt written once the turn has ended starts the
//! next one. One written with priority `now` while it works is taken at once: what the turn runs
//! moves to the background, or the turn is stopped, with a result that says so, and the prompt is
//! answered next. It repeats a prompt on stdout when it takes it. It asks on stdout before a tool
//! call that needs approval and reads the answer from stdin, where it also takes changed settings.

use std::collections::HashMap;

use motile_protocol::wire::{Access, Approval, Subagent, Tokens, ToolStatus, TurnSummary};
use serde_json::{Value, json};

use super::{AgentEvent, Background, ModelUsage, Turn};

/// Background tasks that watch a command: the Monitor tool's and shells left running.
const WATCH_TASKS: [&str; 4] = ["local_bash", "shell", "monitor", "monitor_mcp"];
/// Background tasks that are only bookkeeping.
const IDLE_TASKS: [&str; 2] = ["plan", "dream"];

fn permission_mode(plan: bool, access: Access) -> &'static str {
    match (plan, access) {
        (true, _) => "plan",
        (false, Access::Supervised) => "default",
        (false, Access::AcceptEdits) => "acceptEdits",
        (false, Access::Auto) => "auto",
        (false, Access::Full) => "bypassPermissions",
    }
}

pub fn arguments(turn: &Turn) -> Vec<String> {
    let mut arguments: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--replay-user-messages",
        "--permission-prompt-tool",
        "stdio",
        // Lets a thread be given full access while its process runs.
        "--allow-dangerously-skip-permissions",
        "--permission-mode",
        permission_mode(turn.plan, turn.access),
    ]
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
    arguments.extend(["--append-system-prompt".to_string(), super::instructions(turn)]);
    arguments
}

pub fn input(prompt: &str, id: &str) -> String {
    format!("{}\n", json!({"type": "user", "uuid": id, "message": {"role": "user", "content": prompt}}))
}

pub fn steer(prompt: &str, id: &str) -> String {
    let message = json!({"role": "user", "content": prompt});
    let line = json!({"type": "user", "uuid": id, "message": message, "priority": "now", "origin": {"kind": "human"}});
    format!("{line}\n")
}

fn leaves_plan_mode(approval: &Approval) -> bool {
    approval.tool_name == super::PLAN_TOOL
}

/// The line that allows or refuses the tool call the process asked about. `answers` is what the
/// user chose when the call asked them questions, and `access` what an approved plan is carried
/// out with.
pub fn answer(approval: &Approval, allow: bool, answers: &HashMap<String, String>, access: Access) -> String {
    let mut decision = json!({"behavior": "deny", "message": "The user refused this."});
    if allow {
        let mut input: Value = serde_json::from_str(&approval.input).unwrap_or_default();
        if !answers.is_empty() {
            input["answers"] = json!(answers);
        }
        decision = json!({"behavior": "allow", "updatedInput": input});
    }
    if allow && leaves_plan_mode(approval) {
        let mode = json!({"type": "setMode", "mode": permission_mode(false, access), "destination": "session"});
        decision["updatedPermissions"] = json!([mode]);
    }
    let response = json!({"subtype": "success", "request_id": approval.id, "response": decision});
    format!("{}\n", json!({"type": "control_response", "response": response}))
}

fn control(request: Value) -> String {
    format!("{}\n", json!({"type": "control_request", "request_id": random_id(), "request": request}))
}

/// Lines that give a running process the settings of its thread that changed.
pub fn model_line(model: Option<&str>) -> String {
    control(json!({"subtype": "set_model", "model": model}))
}

/// Offered as an effort, though to Claude Code it is `xhigh` with a setting that has it plan a
/// workflow of agents for every task.
pub const ULTRACODE: &str = "ultracode";

pub fn effort_line(effort: Option<&str>) -> String {
    let ultracode = effort == Some(ULTRACODE);
    let level = if ultracode { Some("xhigh") } else { effort };
    let settings = json!({"effortLevel": level, "ultracode": ultracode});
    control(json!({"subtype": "apply_flag_settings", "settings": settings}))
}

pub fn access_line(plan: bool, access: Access) -> String {
    control(json!({"subtype": "set_permission_mode", "mode": permission_mode(plan, access)}))
}

#[derive(Default)]
pub struct Parser {
    message_id: String,
    open_block: Option<(u64, String)>,
    anonymous_blocks: u64,
    /// The turn has ended; the next one starts in the same process.
    ended: bool,
    /// The agents it started, by its name for each: the tool call that started one, and whether
    /// it still works.
    tasks: HashMap<String, (String, bool)>,
    compacting: bool,
    /// What Claude Code said about a failed API call, which it reports again as the turn's error.
    api_error: Option<String>,
    /// The failed API call was refused for the usage limit.
    rate_limited: bool,
    /// The usage limits that refuse the agent, by window, with when each resets.
    refused: HashMap<String, Option<f64>>,
}

impl Parser {
    pub fn parse(&mut self, line: &str) -> Vec<AgentEvent> {
        let Ok(object) = serde_json::from_str::<Value>(line) else { return vec![] };
        if let Some(parent) = object["parent_tool_use_id"].as_str() {
            return self.parse_subagent(parent, &object);
        }
        match object["type"].as_str() {
            Some("system") => self.parse_system(&object),
            Some("stream_event") => self.parse_stream_event(&object["event"]),
            Some("assistant") if object["error"].is_string() => {
                self.api_error = Some(content_text(&object["message"]["content"]));
                self.rate_limited = object["error"] == "rate_limit";
                vec![]
            }
            Some("rate_limit_event") => {
                self.note_rate_limit(&object["rate_limit_info"]);
                vec![]
            }
            Some("assistant") => self.parse_assistant(&object["message"]),
            Some("user") if object["isReplay"] == true => match object["uuid"].as_str() {
                Some(id) => vec![AgentEvent::Taken { id: id.to_string() }],
                None => vec![],
            },
            Some("user") => parse_user(&object["message"]),
            Some("control_request") => parse_control_request(&object),
            Some("control_cancel_request") => {
                let id = object["request_id"].as_str().unwrap_or_default().to_string();
                vec![AgentEvent::ApprovalWithdrawn { id }]
            }
            // A resumed session whose monitor was cut off starts by ending a turn nobody took.
            Some("result") if object["num_turns"] == 0 && object["is_error"] == false => vec![],
            Some("result") => {
                self.ended = true;
                let usage = parse_usage(&object["modelUsage"]);
                let limited = self.limit(&object);
                usage.into_iter().chain(limited).chain([parse_result(&object, self.api_error.take())]).collect()
            }
            _ => vec![],
        }
    }

    /// Keeps which usage limits refuse the agent. One that allows overage doesn't.
    fn note_rate_limit(&mut self, info: &Value) {
        let window = info["rateLimitType"].as_str().unwrap_or("unknown").to_string();
        let overage = matches!(info["overageStatus"].as_str(), Some("allowed" | "allowed_warning"))
            || info["isUsingOverage"] == true
            || info["overageInUse"] == true;
        match info["status"].as_str() {
            Some("rejected") if !overage => self.refused.insert(window, info["resetsAt"].as_f64()),
            Some(_) => self.refused.remove(&window),
            None => None,
        };
    }

    /// The usage limit that ended the turn this result ends, if one did.
    fn limit(&mut self, result: &Value) -> Option<AgentEvent> {
        let is_error =
            result["is_error"].as_bool().unwrap_or(result["subtype"] != "success") || self.api_error.is_some();
        let reason = result["terminal_reason"].as_str();
        let status = result["api_error_status"].as_u64();
        let limited = std::mem::take(&mut self.rate_limited)
            || !self.refused.is_empty()
            || status == Some(429)
            || reason == Some("blocking_limit");
        let for_limit =
            matches!(reason, None | Some("api_error" | "blocking_limit")) && matches!(status, None | Some(429));
        if !is_error || !limited || !for_limit {
            return None;
        }
        Some(AgentEvent::Limited { resets_at: super::latest_reset(self.refused.values().copied()) })
    }

    /// What an agent the thread's agent started says and does, which arrives in whole messages.
    fn parse_subagent(&mut self, parent: &str, object: &Value) -> Vec<AgentEvent> {
        let events = match object["type"].as_str() {
            Some("assistant") => self.parse_assistant(&object["message"]),
            Some("user") => parse_user(&object["message"]),
            Some("control_request") => return parse_control_request(object),
            _ => vec![],
        };
        let sub = |event| AgentEvent::Sub { parent: parent.to_string(), event: Box::new(event) };
        events.into_iter().map(sub).collect()
    }

    fn task_started(&mut self, object: &Value) -> Vec<AgentEvent> {
        let kind = object["task_type"].as_str().unwrap_or_default();
        if WATCH_TASKS.contains(&kind) || IDLE_TASKS.contains(&kind) {
            return vec![];
        }
        let (Some(task_id), Some(tool_id)) = (object["task_id"].as_str(), object["tool_use_id"].as_str()) else {
            return vec![];
        };
        // An agent that is sent a message later starts again; it stays with the call that made it.
        let task = self.tasks.entry(task_id.to_string()).or_insert_with(|| (tool_id.to_string(), true));
        task.1 = true;
        let agent = Subagent { kind: text(&object["subagent_type"]), ..subagent(ToolStatus::Running, object) };
        vec![AgentEvent::Task { tool_id: task.0.clone(), agent }]
    }

    fn task_progress(&mut self, object: &Value) -> Vec<AgentEvent> {
        let Some((tool_id, true)) = self.tasks.get(object["task_id"].as_str().unwrap_or_default()) else {
            return vec![];
        };
        let agent = Subagent { progress: text(&object["description"]), ..subagent(ToolStatus::Running, object) };
        vec![AgentEvent::Task { tool_id: tool_id.clone(), agent }]
    }

    fn task_ended(&mut self, object: &Value) -> Vec<AgentEvent> {
        let Some((tool_id, working)) = self.tasks.get_mut(object["task_id"].as_str().unwrap_or_default()) else {
            return vec![];
        };
        *working = false;
        let status = if object["status"] == "completed" { ToolStatus::Succeeded } else { ToolStatus::Failed };
        let agent = Subagent { result: text(&object["summary"]), ..subagent(status, object) };
        vec![AgentEvent::Task { tool_id: tool_id.clone(), agent }]
    }

    fn set_compacting(&mut self, active: bool) -> Vec<AgentEvent> {
        if std::mem::replace(&mut self.compacting, active) == active {
            return vec![];
        }
        vec![AgentEvent::Compacting { active }]
    }

    fn parse_system(&mut self, object: &Value) -> Vec<AgentEvent> {
        match object["subtype"].as_str() {
            Some("task_started") => self.task_started(object),
            Some("task_progress") => self.task_progress(object),
            Some("task_notification") => self.task_ended(object),
            Some("status") => self.set_compacting(object["status"] == "compacting"),
            Some("compact_boundary") => self.set_compacting(false),
            Some("init") => {
                let session_id = object["session_id"].as_str();
                let session = session_id.map(|id| AgentEvent::Session { id: id.to_string() });
                let woke = std::mem::take(&mut self.ended).then_some(AgentEvent::Woke);
                session.into_iter().chain(woke).collect()
            }
            Some("background_tasks_changed") => vec![AgentEvent::Background(background(&object["tasks"]))],
            _ => vec![],
        }
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

fn text(value: &Value) -> Option<String> {
    value.as_str().filter(|text| !text.is_empty()).map(String::from)
}

/// What a message about an agent says of how much it has used.
fn subagent(status: ToolStatus, object: &Value) -> Subagent {
    let usage = &object["usage"];
    Subagent {
        kind: None,
        status,
        progress: None,
        result: None,
        tokens: usage["total_tokens"].as_u64(),
        tool_uses: usage["tool_uses"].as_u64(),
        duration_ms: usage["duration_ms"].as_u64(),
    }
}

fn background(tasks: &Value) -> Background {
    let tasks = tasks.as_array().map(Vec::as_slice).unwrap_or_default();
    let kinds = tasks.iter().map(|task| task["task_type"].as_str().unwrap_or_default());
    let working: Vec<&str> = kinds.filter(|kind| !IDLE_TASKS.contains(kind)).collect();
    let watches = working.iter().filter(|kind| WATCH_TASKS.contains(kind)).count();
    Background { watches, agents: working.len() - watches }
}

fn parse_control_request(object: &Value) -> Vec<AgentEvent> {
    let request = &object["request"];
    if request["subtype"] != "can_use_tool" {
        return vec![];
    }
    let (Some(id), Some(tool_name)) = (object["request_id"].as_str(), request["tool_name"].as_str()) else {
        return vec![];
    };
    let approval =
        Approval { id: id.to_string(), tool_name: tool_name.to_string(), input: object_json(&request["input"]) };
    vec![AgentEvent::Approval(approval)]
}

/// What the session has spent so far on each model, which every result repeats.
fn parse_usage(models: &Value) -> Option<AgentEvent> {
    let spent = model_usage(models);
    (!spent.is_empty()).then_some(AgentEvent::Usage { spent, total: true })
}

/// A result's `modelUsage`: what its process's session has spent on each model.
pub fn model_usage(models: &Value) -> Vec<ModelUsage> {
    let spent = |(model, usage): (&String, &Value)| ModelUsage {
        model: model.clone(),
        tokens: Tokens {
            input: usage["inputTokens"].as_u64().unwrap_or_default(),
            cache_read: usage["cacheReadInputTokens"].as_u64().unwrap_or_default(),
            cache_write: usage["cacheCreationInputTokens"].as_u64().unwrap_or_default(),
            output: usage["outputTokens"].as_u64().unwrap_or_default(),
        },
        cost_usd: usage["costUSD"].as_f64(),
    };
    models.as_object().into_iter().flatten().map(spent).collect()
}

fn parse_result(object: &Value, api_error: Option<String>) -> AgentEvent {
    let is_error = object["is_error"].as_bool().unwrap_or(object["subtype"] != "success");
    let result_text = object["result"].as_str().filter(|text| is_error && !text.is_empty()).map(String::from);
    let summary = TurnSummary {
        duration_ms: object["duration_ms"].as_u64(),
        is_error: is_error || api_error.is_some(),
        ..TurnSummary::default()
    };
    let reason = object["terminal_reason"].as_str().unwrap_or_default();
    AgentEvent::Completed {
        summary,
        result_text: result_text.or(api_error),
        preempted: matches!(reason, "aborted_streaming" | "aborted_tools"),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_result_says_what_the_session_has_spent_on_each_model() {
        let result = r#"{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.5,"modelUsage":{
            "claude-haiku-4-5-20251001":{"inputTokens":28,"outputTokens":197,"cacheReadInputTokens":58455,
            "cacheCreationInputTokens":14737,"costUSD":0.0363325}}}"#;
        let events = Parser::default().parse(result);
        let [AgentEvent::Usage { spent, total: true }, AgentEvent::Completed { summary, .. }] = &events[..] else {
            panic!("expected what it spent and then the turn's end, got {events:?}")
        };
        let tokens = Tokens { input: 28, cache_read: 58455, cache_write: 14737, output: 197 };
        let haiku = ModelUsage { model: "claude-haiku-4-5-20251001".to_string(), tokens, cost_usd: Some(0.0363325) };
        assert_eq!(spent, &vec![haiku]);
        assert_eq!(summary.cost_usd, None, "the session's cost isn't the turn's");
    }

    #[test]
    fn a_watch_that_outlives_the_turn_is_reported_and_wakes_the_agent() {
        let mut parser = Parser::default();
        let tasks = r#"{"type":"system","subtype":"background_tasks_changed","tasks":[
            {"task_id":"a","task_type":"local_bash","description":"deploy status"},
            {"task_id":"b","task_type":"local_agent","description":"review"},
            {"task_id":"c","task_type":"plan"}]}"#;
        let init = r#"{"type":"system","subtype":"init","session_id":"s1"}"#;
        let session = AgentEvent::Session { id: "s1".to_string() };

        assert_eq!(parser.parse(init), vec![session.clone()]);
        assert_eq!(parser.parse(tasks), vec![AgentEvent::Background(Background { watches: 1, agents: 1 })]);
        let ended = parser.parse(r#"{"type":"result","subtype":"success","is_error":false}"#);
        assert!(matches!(ended[..], [AgentEvent::Completed { .. }]));
        assert_eq!(parser.parse(init), vec![session.clone(), AgentEvent::Woke]);
        assert_eq!(parser.parse(init), vec![session]);
    }

    #[test]
    fn an_agent_it_starts_reports_how_far_it_is_and_what_it_does() {
        let mut parser = Parser::default();
        let started = r#"{"type":"system","subtype":"task_started","task_id":"a1","tool_use_id":"toolu_1",
            "description":"Count the files","subagent_type":"Explore","task_type":"local_agent"}"#;
        let shell = r#"{"type":"system","subtype":"task_started","task_id":"b1","tool_use_id":"toolu_2",
            "task_type":"local_bash"}"#;
        let read = r#"{"type":"assistant","parent_tool_use_id":"toolu_1","message":{"id":"m1","content":[
            {"type":"tool_use","id":"toolu_3","name":"Read","input":{"file_path":"a.txt"}}]}}"#;
        let progress = r#"{"type":"system","subtype":"task_progress","task_id":"a1","description":"Reading a.txt",
            "usage":{"total_tokens":20840,"tool_uses":3,"duration_ms":29021}}"#;
        let ended = r#"{"type":"system","subtype":"task_notification","task_id":"a1","status":"completed",
            "summary":"Three files","usage":{"total_tokens":21059,"tool_uses":3,"duration_ms":30714}}"#;
        let agent = |status| Subagent {
            kind: None,
            status,
            progress: None,
            result: None,
            tokens: None,
            tool_uses: None,
            duration_ms: None,
        };
        let task = |agent| vec![AgentEvent::Task { tool_id: "toolu_1".to_string(), agent }];

        let explore = Subagent { kind: Some("Explore".into()), ..agent(ToolStatus::Running) };
        assert_eq!(parser.parse(started), task(explore));
        assert_eq!(parser.parse(shell), vec![]);
        let input = AgentEvent::ToolInput {
            id: "toolu_3".into(),
            name: "Read".into(),
            input: r#"{"file_path":"a.txt"}"#.into(),
        };
        let sub = AgentEvent::Sub { parent: "toolu_1".into(), event: Box::new(input) };
        assert_eq!(parser.parse(read), vec![sub]);
        let reading = Subagent {
            progress: Some("Reading a.txt".into()),
            tokens: Some(20840),
            tool_uses: Some(3),
            duration_ms: Some(29021),
            ..agent(ToolStatus::Running)
        };
        assert_eq!(parser.parse(progress), task(reading));
        let done = Subagent {
            result: Some("Three files".into()),
            tokens: Some(21059),
            tool_uses: Some(3),
            duration_ms: Some(30714),
            ..agent(ToolStatus::Succeeded)
        };
        assert_eq!(parser.parse(ended), task(done));
        assert_eq!(parser.parse(progress), vec![]);
    }

    #[test]
    fn it_says_when_it_makes_its_conversation_shorter() {
        let mut parser = Parser::default();
        let status = |status: &str| format!(r#"{{"type":"system","subtype":"status","status":{status}}}"#);

        assert_eq!(parser.parse(&status(r#""requesting""#)), vec![]);
        assert_eq!(parser.parse(&status(r#""compacting""#)), vec![AgentEvent::Compacting { active: true }]);
        assert_eq!(parser.parse(&status("null")), vec![AgentEvent::Compacting { active: false }]);
    }

    #[test]
    fn the_empty_turn_a_resumed_session_starts_with_is_left_out() {
        let mut parser = Parser::default();
        let empty = r#"{"type":"result","subtype":"success","is_error":false,"num_turns":0,"result":""}"#;
        let init = r#"{"type":"system","subtype":"init","session_id":"s1"}"#;

        assert_eq!(parser.parse(empty), vec![]);
        assert_eq!(parser.parse(init), vec![AgentEvent::Session { id: "s1".to_string() }]);
        let failed = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"num_turns":0}"#;
        assert!(matches!(parser.parse(failed)[..], [AgentEvent::Completed { .. }]));
    }

    #[test]
    fn a_failed_api_call_is_reported_once_as_the_turns_error() {
        let mut parser = Parser::default();
        let message = r#"{"type":"assistant","error":"billing_error","message":{"id":"m1","model":"<synthetic>",
            "content":[{"type":"text","text":"You're out of usage credits."}]}}"#;
        let failed = r#"{"type":"result","subtype":"success","is_error":true,"result":"You're out of usage credits."}"#;
        let ended = r#"{"type":"result","subtype":"success","is_error":false,"result":"Cut off"}"#;

        assert_eq!(parser.parse(message), vec![]);
        let [AgentEvent::Completed { summary, result_text, .. }] = &parser.parse(failed)[..] else { panic!() };
        assert!(summary.is_error);
        assert_eq!(result_text.as_deref(), Some("You're out of usage credits."));

        assert_eq!(parser.parse(message), vec![]);
        let [AgentEvent::Completed { summary, result_text, .. }] = &parser.parse(ended)[..] else { panic!() };
        assert!(summary.is_error);
        assert_eq!(result_text.as_deref(), Some("You're out of usage credits."));
    }

    #[test]
    fn a_turn_the_usage_limit_ends_says_when_the_limit_resets() {
        let mut parser = Parser::default();
        let refused = |window: &str, resets: u64| {
            format!(
                r#"{{"type":"rate_limit_event","rate_limit_info":{{"status":"rejected","rateLimitType":"{window}",
                "resetsAt":{resets},"overageStatus":"rejected"}}}}"#
            )
        };
        let message = r#"{"type":"assistant","error":"rate_limit","message":{"id":"m1","model":"<synthetic>",
            "content":[{"type":"text","text":"You've hit your limit"}]}}"#;
        let failed = r#"{"type":"result","subtype":"success","is_error":true,"result":"You've hit your limit"}"#;

        assert_eq!(parser.parse(&refused("five_hour", 1_800_000_000)), vec![]);
        assert_eq!(parser.parse(&refused("seven_day", 1_800_003_600)), vec![]);
        assert_eq!(parser.parse(message), vec![]);
        let events = parser.parse(failed);
        let [AgentEvent::Limited { resets_at }, AgentEvent::Completed { summary, result_text, .. }] = &events[..]
        else {
            panic!("expected the limit and then the turn's end, got {events:?}")
        };
        assert_eq!(*resets_at, Some(1_800_003_600.0), "the agent works again once both limits reset");
        assert!(summary.is_error);
        assert_eq!(result_text.as_deref(), Some("You've hit your limit"));
    }

    #[test]
    fn a_limit_that_allows_overage_or_was_lifted_ends_no_turn() {
        let mut parser = Parser::default();
        let overage = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour",
            "resetsAt":1800000000,"overageStatus":"allowed"}}"#;
        let refused = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour",
            "resetsAt":1800000000}}"#;
        let allowed =
            r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","rateLimitType":"five_hour"}}"#;
        let failed =
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"terminal_reason":"model_error"}"#;
        let ended = r#"{"type":"result","subtype":"success","is_error":false}"#;

        parser.parse(overage);
        assert!(matches!(parser.parse(failed)[..], [AgentEvent::Completed { .. }]));
        parser.parse(refused);
        parser.parse(allowed);
        assert!(matches!(parser.parse(failed)[..], [AgentEvent::Completed { .. }]));
        parser.parse(refused);
        assert!(
            matches!(parser.parse(ended)[..], [AgentEvent::Completed { .. }]),
            "a turn that ended well wasn't limited"
        );
        let blocked = r#"{"type":"result","subtype":"success","is_error":true,"terminal_reason":"blocking_limit"}"#;
        assert!(matches!(
            parser.parse(blocked)[..],
            [AgentEvent::Limited { resets_at: Some(_) }, AgentEvent::Completed { .. }]
        ));
    }

    #[test]
    fn a_tool_call_that_needs_approval_is_asked_about_and_answered() {
        let mut parser = Parser::default();
        let asked = r#"{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool",
            "tool_name":"Bash","input":{"command":"touch x"},"tool_use_id":"toolu_1"}}"#;
        let approval = Approval { id: "r1".into(), tool_name: "Bash".into(), input: r#"{"command":"touch x"}"#.into() };
        assert_eq!(parser.parse(asked), vec![AgentEvent::Approval(approval.clone())]);
        let withdrawn = r#"{"type":"control_cancel_request","request_id":"r1"}"#;
        assert_eq!(parser.parse(withdrawn), vec![AgentEvent::ApprovalWithdrawn { id: "r1".into() }]);

        let nothing = HashMap::new();
        let allowed: Value = serde_json::from_str(&answer(&approval, true, &nothing, Access::Full)).unwrap();
        assert_eq!(allowed["response"]["request_id"], "r1");
        assert_eq!(
            allowed["response"]["response"],
            json!({"behavior": "allow", "updatedInput": {"command": "touch x"}})
        );
        let refused: Value = serde_json::from_str(&answer(&approval, false, &nothing, Access::Full)).unwrap();
        assert_eq!(refused["response"]["response"]["behavior"], "deny");
    }

    #[test]
    fn questions_are_answered_and_an_approved_plan_is_carried_out_with_the_threads_access() {
        let questions =
            Approval { id: "r1".into(), tool_name: "AskUserQuestion".into(), input: r#"{"questions":[]}"#.into() };
        let answers = HashMap::from([("Which color?".to_string(), "Blue".to_string())]);
        let answered: Value = serde_json::from_str(&answer(&questions, true, &answers, Access::Full)).unwrap();
        let input = &answered["response"]["response"]["updatedInput"];
        assert_eq!(input, &json!({"questions": [], "answers": {"Which color?": "Blue"}}));

        let plan =
            Approval { id: "r2".into(), tool_name: "ExitPlanMode".into(), input: r#"{"plan":"1. Do it"}"#.into() };
        let approved: Value = serde_json::from_str(&answer(&plan, true, &HashMap::new(), Access::AcceptEdits)).unwrap();
        let permissions = &approved["response"]["response"]["updatedPermissions"];
        assert_eq!(permissions, &json!([{"type": "setMode", "mode": "acceptEdits", "destination": "session"}]));
    }

    #[test]
    fn a_prompt_is_one_line_of_json() {
        let line = input("first\nsecond \"quoted\"", "m1");
        assert_eq!(line.matches('\n').count(), 1);
        let message: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(message["message"]["content"], "first\nsecond \"quoted\"");
    }

    #[test]
    fn a_prompt_sent_now_is_taken_at_once_and_a_reply_stopped_for_it_says_so() {
        let line: Value = serde_json::from_str(&steer("Also this", "m1")).unwrap();
        assert_eq!(
            (&line["uuid"], &line["priority"], &line["origin"]["kind"]),
            (&json!("m1"), &json!("now"), &json!("human"))
        );
        let mut parser = Parser::default();
        let stopped = r#"{"type":"result","subtype":"success","is_error":false,"terminal_reason":"aborted_streaming"}"#;
        assert!(matches!(parser.parse(stopped)[..], [AgentEvent::Completed { preempted: true, .. }]));
        let ended = r#"{"type":"result","subtype":"success","is_error":false,"terminal_reason":"completed"}"#;
        assert!(matches!(parser.parse(ended)[..], [AgentEvent::Completed { preempted: false, .. }]));
    }

    #[test]
    fn a_prompt_the_agent_repeats_has_been_taken() {
        let mut parser = Parser::default();
        let repeated = r#"{"type":"user","uuid":"m1","isReplay":true,"message":{"role":"user","content":"Also this"}}"#;
        assert_eq!(parser.parse(repeated), vec![AgentEvent::Taken { id: "m1".to_string() }]);
    }
}
