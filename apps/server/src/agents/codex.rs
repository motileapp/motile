//! `codex app-server`: JSON-RPC over stdin and stdout, one message a line. The process is asked
//! to initialize, to start or resume the thread, and to start a turn; what the turn does comes
//! back as notifications. It asks before a command or an edit that needs approval and waits for
//! the answer. A prompt steered into a running turn joins it after its next tool call, and comes
//! back as an item when it does; one that comes once the turn has ended starts the next turn. A
//! plan is presented when its turn has ended, and carried out in a turn of its own. An agent it
//! starts is a thread of its own, whose notifications arrive among the others.

use std::collections::HashMap;

use motile_protocol::wire::{Access, Approval, Subagent, Tokens, ToolCall, ToolStatus, TurnSummary};
use serde_json::{Value, json};

use super::{AgentEvent, ModelUsage, PLAN_TOOL, Turn};

const INITIALIZE: u64 = 1;
const THREAD: u64 = 2;
const FIRST_TURN: u64 = 3;
const QUESTION_TOOL: &str = "AskUserQuestion";

pub fn arguments() -> Vec<String> {
    vec!["app-server".to_string()]
}

fn line(message: Value) -> String {
    format!("{message}\n")
}

/// The first line a new process is given. The parser asks for the rest as the answers arrive.
pub fn opening() -> String {
    let client = json!({"name": "motile", "title": "Motile", "version": env!("CARGO_PKG_VERSION")});
    let params = json!({"clientInfo": client, "capabilities": {"experimentalApi": true}});
    line(json!({"id": INITIALIZE, "method": "initialize", "params": params}))
}

fn turn_params(thread_id: &str, prompt: &str, id: &str) -> Value {
    json!({"threadId": thread_id, "input": [{"type": "text", "text": prompt}], "clientUserMessageId": id})
}

/// A prompt for a process whose turn has ended; it starts the next one.
pub fn input(thread_id: &str, prompt: &str, id: &str) -> String {
    line(json!({"id": format!("message:{id}"), "method": "turn/start", "params": turn_params(thread_id, prompt, id)}))
}

/// A prompt for the turn that runs, which takes it after its next tool call.
pub fn steer(thread_id: &str, turn_id: &str, prompt: &str, id: &str) -> String {
    let mut params = turn_params(thread_id, prompt, id);
    params["expectedTurnId"] = json!(turn_id);
    line(json!({"id": format!("message:{id}"), "method": "turn/steer", "params": params}))
}

pub fn stop(thread_id: &str, turn_id: &str) -> String {
    let params = json!({"threadId": thread_id, "turnId": turn_id});
    line(json!({"id": "stop", "method": "turn/interrupt", "params": params}))
}

fn collaboration_mode(plan: bool, model: &str, effort: Option<&str>) -> Value {
    let settings = json!({"model": model, "reasoning_effort": effort, "developer_instructions": null});
    json!({"mode": if plan { "plan" } else { "default" }, "settings": settings})
}

/// What Codex asks about, how its sandbox is set, and who answers what it asks.
fn permissions(access: Access) -> (&'static str, &'static str, &'static str) {
    match access {
        Access::Supervised => ("untrusted", "read-only", "user"),
        Access::AcceptEdits => ("on-request", "workspace-write", "user"),
        Access::Auto => ("on-request", "workspace-write", "auto_review"),
        Access::Full => ("never", "danger-full-access", "user"),
    }
}

/// The line that answers what the process asked. `None` when there is nothing to tell it: a
/// plan that is refused only ends there.
pub fn answer(approval: &Approval, allow: bool, answers: &HashMap<String, String>) -> Option<String> {
    let (kind, request_id) = approval.id.split_once(':')?;
    let input: Value = serde_json::from_str(&approval.input).unwrap_or_default();
    let result = match kind {
        "command" | "file" => json!({"decision": if allow { "accept" } else { "decline" }}),
        "permissions" => json!({"permissions": if allow { input["permissions"].clone() } else { json!({}) }}),
        "questions" => {
            let questions = input["questions"].as_array().map(Vec::as_slice).unwrap_or_default();
            let chosen = questions.iter().filter_map(|question| {
                let answer = answers.get(question["question"].as_str()?).filter(|_| allow)?;
                Some((question["id"].as_str()?.to_string(), json!({"answers": [answer]})))
            });
            json!({"answers": chosen.collect::<serde_json::Map<String, Value>>()})
        }
        "plan" if allow => {
            let codex = &input["codex"];
            let mut params = turn_params(codex["thread_id"].as_str()?, "Implement the plan.", "implement");
            params["collaborationMode"] = collaboration_mode(false, codex["model"].as_str()?, codex["effort"].as_str());
            return Some(line(json!({"id": "implement", "method": "turn/start", "params": params})));
        }
        _ => return None,
    };
    let id: Value = serde_json::from_str(request_id).ok()?;
    Some(line(json!({"id": id, "result": result})))
}

/// Whether what Codex says of an error is that a usage limit was reached. It names the error, or
/// is an object keyed by its name.
fn is_limit(info: &Value) -> bool {
    let name = info.as_str().or_else(|| info.as_object()?.keys().next().map(String::as_str));
    matches!(name, Some("usageLimitExceeded" | "rateLimitExceeded"))
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
    /// What the thread and its first turn are asked for with.
    thread: Value,
    resumes: bool,
    prompt: String,
    prompt_id: String,
    model: Option<String>,
    effort: Option<String>,
    plan: bool,
    thread_id: Option<String>,
    /// The turn has ended; the next one starts in the same process.
    ended: bool,
    /// The plan the running turn has presented.
    presented: Option<String>,
    /// The edits that have started, by item: Codex asks about one by its item alone.
    edits: HashMap<String, Value>,
    /// What the process waits for an answer to, by its request.
    asked: HashMap<String, String>,
    /// The agents it started, by the thread of each: the item that started it.
    agents: HashMap<String, String>,
    /// What each of them said last, which is what it reports.
    reports: HashMap<String, String>,
    /// What each thread, its own and its agents', had spent in all when it last said.
    spent: HashMap<String, [u64; 4]>,
    /// The usage limit stopped the turn that runs.
    limited: bool,
    /// When the usage limits that ran out reset, as Codex last said.
    resets_at: Option<f64>,
}

impl Parser {
    pub fn new(turn: &Turn, cwd: &str, prompt: &str, prompt_id: &str) -> Self {
        let (approval_policy, sandbox, reviewer) = permissions(turn.access);
        let mut thread = json!({
            "cwd": cwd,
            "approvalPolicy": approval_policy,
            "sandbox": sandbox,
            "approvalsReviewer": reviewer,
            "developerInstructions": super::instructions(turn),
        });
        if let Some(model) = turn.model {
            thread["model"] = json!(model);
        }
        if let Some(session_id) = turn.session_id {
            thread["threadId"] = json!(session_id);
            thread["excludeTurns"] = json!(true);
        }
        Self {
            thread,
            resumes: turn.session_id.is_some(),
            prompt: prompt.to_string(),
            prompt_id: prompt_id.to_string(),
            model: turn.model.map(String::from),
            effort: turn.effort.map(String::from),
            plan: turn.plan,
            thread_id: None,
            ended: false,
            presented: None,
            edits: HashMap::new(),
            asked: HashMap::new(),
            agents: HashMap::new(),
            reports: HashMap::new(),
            spent: HashMap::new(),
            limited: false,
            resets_at: None,
        }
    }

    pub fn parse(&mut self, line: &str) -> Vec<AgentEvent> {
        let Ok(message) = serde_json::from_str::<Value>(line) else { return vec![] };
        match (message["method"].as_str(), message.get("id")) {
            (Some(method), Some(id)) => self.parse_request(method, id, &message["params"]),
            (Some(method), None) => self.parse_notification(method, &message["params"]),
            (None, Some(id)) => self.parse_answer(id, &message),
            (None, None) => vec![],
        }
    }

    /// The answers to what starts the thread: each leads to the next request.
    fn parse_answer(&mut self, id: &Value, message: &Value) -> Vec<AgentEvent> {
        let Some(id) = id.as_u64() else { return vec![] };
        if let Some(error) = message["error"]["message"].as_str() {
            return vec![AgentEvent::Failed { message: error.to_string() }];
        }
        match id {
            INITIALIZE => {
                let method = if self.resumes { "thread/resume" } else { "thread/start" };
                let request = json!({"id": THREAD, "method": method, "params": self.thread});
                vec![AgentEvent::Write(line(json!({"method": "initialized"})) + &line(request))]
            }
            THREAD => {
                let result = &message["result"];
                let Some(thread_id) = result["thread"]["id"].as_str() else { return vec![] };
                self.thread_id = Some(thread_id.to_string());
                let model = self.model.take().or_else(|| result["model"].as_str().map(String::from));
                let mut params = turn_params(thread_id, &self.prompt, &self.prompt_id);
                if let Some(model) = &model {
                    params["model"] = json!(model);
                    params["collaborationMode"] = collaboration_mode(self.plan, model, self.effort.as_deref());
                }
                if let Some(effort) = &self.effort {
                    params["effort"] = json!(effort);
                }
                self.model = model;
                let request = json!({"id": FIRST_TURN, "method": "turn/start", "params": params});
                vec![AgentEvent::Session { id: thread_id.to_string() }, AgentEvent::Write(line(request))]
            }
            _ => vec![],
        }
    }

    fn parse_notification(&mut self, method: &str, params: &Value) -> Vec<AgentEvent> {
        if method == "thread/tokenUsage/updated" {
            return self.parse_usage(params).into_iter().collect();
        }
        if method == "account/rateLimits/updated" {
            self.note_rate_limits(&params["rateLimits"]);
            return vec![];
        }
        let own = self.thread_id.as_deref();
        if let Some(thread_id) = params["threadId"].as_str().filter(|id| own.is_some_and(|own| own != *id)) {
            return self.parse_subagent(method, thread_id, params);
        }
        match method {
            "turn/started" => {
                self.presented = None;
                let woke = std::mem::take(&mut self.ended).then_some(AgentEvent::Woke);
                let turn = params["turn"]["id"].as_str().map(|id| AgentEvent::Turn { id: id.to_string() });
                woke.into_iter().chain(turn).chain([AgentEvent::Thinking { active: true }]).collect()
            }
            "item/started" => self.parse_item(&params["item"], false),
            "item/completed" => self.parse_item(&params["item"], true),
            "item/agentMessage/delta" => match (params["itemId"].as_str(), params["delta"].as_str()) {
                (Some(id), Some(text)) => vec![AgentEvent::TextDelta { id: id.to_string(), text: text.to_string() }],
                _ => vec![],
            },
            "turn/plan/updated" => todo_list(params),
            "turn/completed" => self.parse_turn_end(&params["turn"]),
            "error" if params["willRetry"] != true => {
                self.limited |= is_limit(&params["error"]["codexErrorInfo"]);
                vec![]
            }
            "serverRequest/resolved" => match self.asked.remove(&params["requestId"].to_string()) {
                Some(id) => vec![AgentEvent::ApprovalWithdrawn { id }],
                None => vec![],
            },
            _ => vec![],
        }
    }

    /// What happens in the thread of an agent it started.
    fn parse_subagent(&mut self, method: &str, thread_id: &str, params: &Value) -> Vec<AgentEvent> {
        let Some(parent) = self.agents.get(thread_id).cloned() else { return vec![] };
        let task = |status, result| {
            let agent = Subagent {
                kind: None,
                status,
                progress: None,
                result,
                tokens: None,
                tool_uses: None,
                duration_ms: params["turn"]["durationMs"].as_u64(),
            };
            vec![AgentEvent::Task { tool_id: parent.clone(), agent }]
        };
        match method {
            "turn/started" => task(ToolStatus::Running, None),
            "turn/completed" => {
                let failed = params["turn"]["status"] == "failed";
                let status = if failed { ToolStatus::Failed } else { ToolStatus::Succeeded };
                task(status, self.reports.remove(thread_id))
            }
            "item/started" | "item/completed" => {
                let events = self.parse_item(&params["item"], method == "item/completed");
                let shown = events.into_iter().filter(|event| {
                    matches!(event, AgentEvent::Text { .. } | AgentEvent::ThinkingText { .. } | AgentEvent::Tool { .. })
                });
                let mut events = Vec::new();
                for event in shown {
                    if let AgentEvent::Text { text, .. } = &event {
                        self.reports.insert(thread_id.to_string(), text.clone());
                    }
                    events.push(AgentEvent::Sub { parent: parent.clone(), event: Box::new(event) });
                }
                events
            }
            _ => vec![],
        }
    }

    /// What a thread spent since it last said: how far its total has grown, or its last answer
    /// alone when the total is one this process hasn't seen grow, as a resumed thread's is.
    fn parse_usage(&mut self, params: &Value) -> Option<AgentEvent> {
        let usage = &params["tokenUsage"];
        let counts = |counts: &Value| {
            ["inputTokens", "cachedInputTokens", "cacheWriteInputTokens", "outputTokens"]
                .map(|name| counts[name].as_u64().unwrap_or_default())
        };
        let (total, last) = (counts(&usage["total"]), counts(&usage["last"]));
        let known = self.spent.insert(params["threadId"].as_str()?.to_string(), total);
        let [input, cache_read, cache_write, output] = match known {
            Some(known) if (0..4).all(|index| total[index] >= known[index]) => {
                [0, 1, 2, 3].map(|index| total[index] - known[index])
            }
            _ => last,
        };
        // Codex counts what it read from the cache and wrote to it as input too.
        let input = input.saturating_sub(cache_read + cache_write);
        let tokens = Tokens { input, cache_read, cache_write, output };
        if tokens == Tokens::default() {
            return None;
        }
        let spent = ModelUsage { model: self.model.clone().unwrap_or_default(), tokens, cost_usd: None };
        Some(AgentEvent::Usage { spent: vec![spent], total: false })
    }

    /// Keeps when the limits that ran out reset. Only the account's own limits count, not a
    /// model's.
    fn note_rate_limits(&mut self, limits: &Value) {
        if limits["limitId"].as_str().is_some_and(|id| id != "codex") {
            return;
        }
        let windows = [&limits["primary"], &limits["secondary"]];
        let exhausted =
            windows.into_iter().filter(|window| window["usedPercent"].as_f64().is_some_and(|used| used >= 100.0));
        let resets: Vec<Option<f64>> = exhausted.map(|window| window["resetsAt"].as_f64()).collect();
        self.resets_at = if resets.is_empty() { None } else { super::latest_reset(resets) };
    }

    fn parse_turn_end(&mut self, turn: &Value) -> Vec<AgentEvent> {
        self.ended = true;
        let failed = turn["status"] == "failed";
        let summary = TurnSummary { is_error: failed, ..Default::default() };
        let result_text = turn["error"]["message"].as_str().map(String::from);
        let limited = std::mem::take(&mut self.limited) || is_limit(&turn["error"]["codexErrorInfo"]);
        let mut events: Vec<AgentEvent> =
            (failed && limited).then_some(AgentEvent::Limited { resets_at: self.resets_at }).into_iter().collect();
        events.push(AgentEvent::Completed { summary, result_text, preempted: false });
        events.extend(self.plan_approval());
        events
    }

    /// Asks what to do with the plan the turn presented.
    fn plan_approval(&mut self) -> Option<AgentEvent> {
        let plan = self.presented.take()?;
        let codex = json!({"thread_id": self.thread_id, "model": self.model, "effort": self.effort});
        let approval = Approval {
            id: format!("plan:{}", uuid::Uuid::new_v4()),
            tool_name: PLAN_TOOL.to_string(),
            input: json!({"plan": plan, "codex": codex}).to_string(),
        };
        Some(AgentEvent::Approval(approval))
    }

    fn parse_item(&mut self, item: &Value, completed: bool) -> Vec<AgentEvent> {
        let Some(id) = item["id"].as_str().map(String::from) else { return vec![] };
        let status = match item["status"].as_str() {
            Some("failed" | "declined") => ToolStatus::Failed,
            Some("inProgress") => ToolStatus::Running,
            _ if completed => ToolStatus::Succeeded,
            _ => ToolStatus::Running,
        };
        let tool = |name: &str, input: Value, output: Option<String>| {
            let call = ToolCall {
                id: id.clone(),
                name: name.to_string(),
                input: input.to_string(),
                output,
                status,
                agent: None,
            };
            vec![AgentEvent::Thinking { active: false }, AgentEvent::Tool { call }]
        };

        match item["type"].as_str() {
            Some("userMessage") => match item["clientId"].as_str() {
                Some(message_id) if !completed => vec![AgentEvent::Taken { id: message_id.to_string() }],
                _ => vec![],
            },
            Some("agentMessage") if !completed => vec![AgentEvent::TextStarted { id }],
            Some("agentMessage") => {
                let text = item["text"].as_str().unwrap_or_default().to_string();
                vec![AgentEvent::Thinking { active: false }, AgentEvent::Text { id, text }]
            }
            Some("reasoning") => {
                let parts = item["summary"].as_array().map(Vec::as_slice).unwrap_or_default();
                let text = parts.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n\n");
                if text.is_empty() { vec![] } else { vec![AgentEvent::ThinkingText { id, text }] }
            }
            Some("commandExecution") => {
                let output = item["aggregatedOutput"].as_str().filter(|output| !output.is_empty() || completed);
                let command = shell_command(item["command"].as_str().unwrap_or_default());
                let mut events = tool("Bash", json!({ "command": command }), output.map(String::from));
                if let (Some(code), Some(AgentEvent::Tool { call })) = (item["exitCode"].as_i64(), events.last_mut())
                    && code != 0
                {
                    call.status = ToolStatus::Failed;
                }
                events
            }
            Some("fileChange") => {
                let edit = edit_input(item);
                let changes = edit["changes"].as_array().map(Vec::as_slice).unwrap_or_default();
                let lines: Vec<String> = changes
                    .iter()
                    .map(|change| {
                        let kind = change["kind"].as_str().unwrap_or("update");
                        format!("{kind} {}", change["path"].as_str().unwrap_or_default())
                    })
                    .collect();
                match completed {
                    true => self.edits.remove(&id),
                    false => self.edits.insert(id.clone(), edit.clone()),
                };
                tool("Edit", edit, completed.then(|| lines.join("\n")))
            }
            Some("mcpToolCall") => {
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
            Some("webSearch") => tool("WebSearch", json!({ "query": item["query"] }), None),
            Some("subAgentActivity") if item["kind"] == "started" && !completed => {
                let Some(thread_id) = item["agentThreadId"].as_str() else { return vec![] };
                self.agents.insert(thread_id.to_string(), id.clone());
                let agent = Subagent {
                    kind: None,
                    status: ToolStatus::Running,
                    progress: None,
                    result: None,
                    tokens: None,
                    tool_uses: None,
                    duration_ms: None,
                };
                let description = agent_title(item["agentPath"].as_str().unwrap_or_default());
                let call = ToolCall {
                    id: id.clone(),
                    name: "Agent".to_string(),
                    input: json!({ "description": description }).to_string(),
                    output: None,
                    status: ToolStatus::Succeeded,
                    agent: Some(agent),
                };
                vec![AgentEvent::Thinking { active: false }, AgentEvent::Tool { call }]
            }
            Some("plan") if completed && self.plan => {
                let plan = item["text"].as_str().unwrap_or_default();
                self.presented = Some(plan.to_string());
                tool(PLAN_TOOL, json!({ "plan": plan }), None)
            }
            _ => vec![],
        }
    }

    /// What the process asks before it goes on. What nobody can be asked is refused, so that the
    /// turn doesn't wait for it.
    fn parse_request(&mut self, method: &str, id: &Value, params: &Value) -> Vec<AgentEvent> {
        let (kind, tool_name, input) = match method {
            "item/commandExecution/requestApproval" => {
                let command = shell_command(params["command"].as_str().unwrap_or_default());
                ("command", "Bash", json!({ "command": command }))
            }
            "item/fileChange/requestApproval" => {
                let edit = params["itemId"].as_str().and_then(|item| self.edits.get(item));
                ("file", "Edit", edit.cloned().unwrap_or_else(|| json!({})))
            }
            "item/permissions/requestApproval" => {
                let input = json!({"description": params["reason"], "permissions": params["permissions"]});
                ("permissions", "Permissions", input)
            }
            "item/tool/requestUserInput" => {
                let asked = params["questions"].as_array().map(Vec::as_slice).unwrap_or_default();
                let question = |asked: &Value| {
                    json!({
                        "id": asked["id"],
                        "question": asked["question"],
                        "header": asked["header"],
                        "options": asked["options"],
                        "multiSelect": false,
                    })
                };
                let questions: Vec<Value> = asked.iter().map(question).collect();
                ("questions", QUESTION_TOOL, json!({ "questions": questions }))
            }
            "mcpServer/elicitation/request" => {
                return vec![AgentEvent::Write(line(json!({"id": id, "result": {"action": "decline"}})))];
            }
            _ => {
                let error = json!({"code": -32601, "message": format!("Motile doesn't answer {method}.")});
                return vec![AgentEvent::Write(line(json!({"id": id, "error": error})))];
            }
        };
        let approval =
            Approval { id: format!("{kind}:{id}"), tool_name: tool_name.to_string(), input: input.to_string() };
        self.asked.insert(id.to_string(), approval.id.clone());
        vec![AgentEvent::Approval(approval)]
    }
}

/// Codex names an agent by a path, `/root/review_auth`; this is its last part as words.
fn agent_title(path: &str) -> String {
    let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or_default().replace('_', " ");
    let mut letters = name.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => "Agent".to_string(),
    }
}

/// A file change as the `Edit` tool call it is shown as.
fn edit_input(item: &Value) -> Value {
    let changed = item["changes"].as_array().map(Vec::as_slice).unwrap_or_default();
    let change =
        |change: &Value| json!({"path": change["path"], "kind": change["kind"]["type"], "diff": change["diff"]});
    let changes: Vec<Value> = changed.iter().map(change).collect();
    let file_path = changed.first().and_then(|change| change["path"].as_str()).unwrap_or_default();
    json!({ "file_path": file_path, "changes": changes })
}

/// The steps Codex has set itself, as one tool call that is replaced when they change.
fn todo_list(params: &Value) -> Vec<AgentEvent> {
    let Some(turn_id) = params["turnId"].as_str() else { return vec![] };
    let steps = params["plan"].as_array().map(Vec::as_slice).unwrap_or_default();
    let todo = |step: &Value| {
        let status = match step["status"].as_str() {
            Some("inProgress") => "in_progress",
            Some("completed") => "completed",
            _ => "pending",
        };
        json!({ "content": step["step"], "status": status })
    };
    let todos: Vec<Value> = steps.iter().map(todo).collect();
    let call = ToolCall {
        id: format!("{turn_id}:todos"),
        name: "TodoWrite".to_string(),
        input: json!({ "todos": todos }).to_string(),
        output: None,
        status: ToolStatus::Succeeded,
        agent: None,
    };
    vec![AgentEvent::Tool { call }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parser(turn: Turn) -> Parser {
        Parser::new(&turn, "/srv/api", "Fix it", "m1")
    }

    fn turn(access: Access, plan: bool) -> Turn<'static> {
        Turn {
            agent: motile_protocol::wire::Agent::Codex,
            model: None,
            effort: Some("high"),
            access,
            plan,
            session_id: None,
        }
    }

    fn written(events: &[AgentEvent]) -> Vec<Value> {
        let lines = events.iter().filter_map(|event| match event {
            AgentEvent::Write(lines) => Some(lines.lines().map(|line| serde_json::from_str(line).unwrap())),
            _ => None,
        });
        lines.flatten().collect()
    }

    #[test]
    fn what_an_agent_it_started_does_stays_out_of_its_own_turn() {
        let mut parser = parser(turn(Access::Full, false));
        parser.parse(r#"{"id":2,"result":{"thread":{"id":"t1"}}}"#);
        let started = parser.parse(
            r#"{"method":"item/started","params":{"threadId":"t1","item":{"type":"subAgentActivity","id":"c1",
                "kind":"started","agentThreadId":"t2","agentPath":"/root/read_a"}}}"#,
        );
        let [_, AgentEvent::Tool { call }] = &started[..] else { panic!("{started:?}") };
        assert_eq!((call.name.as_str(), call.input.as_str()), ("Agent", r#"{"description":"Read a"}"#));
        assert_eq!(call.agent.as_ref().map(|agent| agent.status), Some(ToolStatus::Running));

        let ran = parser.parse(
            r#"{"method":"item/completed","params":{"threadId":"t2","item":{"type":"commandExecution","id":"e1",
                "command":"/bin/zsh -lc ls","status":"completed","aggregatedOutput":"a.txt\n","exitCode":0}}}"#,
        );
        assert!(matches!(&ran[..], [AgentEvent::Sub { parent, event }]
            if parent == "c1" && matches!(&**event, AgentEvent::Tool { call } if call.name == "Bash")));
        parser.parse(
            r#"{"method":"item/completed","params":{"threadId":"t2","item":{"type":"agentMessage","id":"m1",
                "text":"It says hi"}}}"#,
        );
        let ended = parser.parse(
            r#"{"method":"turn/completed","params":{"threadId":"t2","turn":{"status":"completed","durationMs":1800}}}"#,
        );
        let [AgentEvent::Task { tool_id, agent }] = &ended[..] else { panic!("{ended:?}") };
        assert_eq!((tool_id.as_str(), agent.status, agent.duration_ms), ("c1", ToolStatus::Succeeded, Some(1800)));
        assert_eq!(agent.result.as_deref(), Some("It says hi"));

        let stranger = r#"{"method":"turn/completed","params":{"threadId":"t9","turn":{"status":"completed"}}}"#;
        assert_eq!(parser.parse(stranger), vec![]);
    }

    #[test]
    fn what_a_thread_spent_is_how_far_its_total_grew() {
        let said = |thread: &str, total: [u64; 4], last: [u64; 4]| {
            let counts = |[input, cached, written, output]: [u64; 4]| {
                json!({"inputTokens": input, "cachedInputTokens": cached, "cacheWriteInputTokens": written,
                       "outputTokens": output})
            };
            let usage = json!({"total": counts(total), "last": counts(last)});
            json!({"method": "thread/tokenUsage/updated", "params": {"threadId": thread, "tokenUsage": usage}})
                .to_string()
        };
        let spent = |events: Vec<AgentEvent>| match &events[..] {
            [AgentEvent::Usage { spent, total: false }] => (spent[0].model.clone(), spent[0].tokens),
            other => panic!("expected what it spent, got {other:?}"),
        };
        let mut parser = parser(turn(Access::Full, false));
        parser.parse(r#"{"id":2,"result":{"thread":{"id":"t1"},"model":"gpt-6"}}"#);

        // A resumed thread's total counts what was spent before: only its last answer is new.
        let resumed = spent(parser.parse(&said("t1", [90_000, 80_000, 0, 900], [15_000, 12_000, 0, 50])));
        let tokens = Tokens { input: 3_000, cache_read: 12_000, cache_write: 0, output: 50 };
        assert_eq!(resumed, ("gpt-6".to_string(), tokens));

        let grown = spent(parser.parse(&said("t1", [120_000, 108_000, 500, 1_000], [1, 1, 1, 1])));
        assert_eq!(grown.1, Tokens { input: 1_500, cache_read: 28_000, cache_write: 500, output: 100 });
        assert_eq!(parser.parse(&said("t1", [120_000, 108_000, 500, 1_000], [1, 1, 1, 1])), vec![]);

        let agent = spent(parser.parse(&said("t2", [700, 0, 0, 30], [700, 0, 0, 30])));
        assert_eq!(agent.1, Tokens { input: 700, cache_read: 0, cache_write: 0, output: 30 });
    }

    #[test]
    fn shell_wrapper_is_removed_from_commands() {
        assert_eq!(shell_command("/bin/bash -lc 'cat greet.py'"), "cat greet.py");
        assert_eq!(
            shell_command(r#"/bin/bash -lc "pwd; rg -g 'AGENTS.md' \"a b\" $HOME""#),
            r#"pwd; rg -g 'AGENTS.md' "a b" $HOME"#
        );
        assert_eq!(shell_command("ls -la"), "ls -la");
    }

    #[test]
    fn the_thread_and_its_first_turn_are_asked_for_as_the_answers_arrive() {
        let mut parser = parser(turn(Access::Supervised, true));
        let asked = written(&parser.parse(r#"{"id":1,"result":{}}"#));
        assert_eq!(asked[0], json!({"method": "initialized"}));
        assert_eq!(asked[1]["method"], "thread/start");
        let thread = &asked[1]["params"];
        assert_eq!(
            (&thread["cwd"], &thread["approvalPolicy"], &thread["sandbox"]),
            (&json!("/srv/api"), &json!("untrusted"), &json!("read-only"))
        );

        let events = parser.parse(r#"{"id":2,"result":{"thread":{"id":"t1"},"model":"gpt-6"}}"#);
        assert_eq!(events[0], AgentEvent::Session { id: "t1".to_string() });
        let first_turn = &written(&events)[0];
        assert_eq!(first_turn["method"], "turn/start");
        let params = &first_turn["params"];
        assert_eq!(params["input"][0]["text"], "Fix it");
        assert_eq!(
            (&params["threadId"], &params["clientUserMessageId"], &params["effort"]),
            (&json!("t1"), &json!("m1"), &json!("high"))
        );
        assert_eq!(params["collaborationMode"]["mode"], "plan");
        assert_eq!(params["collaborationMode"]["settings"]["model"], "gpt-6", "the model Codex chose is named");

        let resumed = Turn { session_id: Some("t0"), ..turn(Access::Full, false) };
        let asked = written(&self::parser(resumed).parse(r#"{"id":1,"result":{}}"#));
        assert_eq!((&asked[1]["method"], &asked[1]["params"]["threadId"]), (&json!("thread/resume"), &json!("t0")));
        assert_eq!(asked[1]["params"]["sandbox"], "danger-full-access");
    }

    #[test]
    fn a_start_that_is_refused_fails_the_turn() {
        let mut parser = parser(turn(Access::Full, false));
        let refused = parser.parse(r#"{"id":2,"error":{"code":-32600,"message":"no rollout found"}}"#);
        assert_eq!(refused, vec![AgentEvent::Failed { message: "no rollout found".to_string() }]);
    }

    #[test]
    fn a_prompt_sent_now_steers_the_turn_that_runs() {
        let request: Value = serde_json::from_str(&steer("thread1", "turn1", "Also this", "m2")).unwrap();
        assert_eq!(request["method"], "turn/steer");
        assert_eq!(request["params"]["threadId"], "thread1");
        assert_eq!(request["params"]["expectedTurnId"], "turn1");
        assert_eq!(request["params"]["clientUserMessageId"], "m2");
        assert_eq!(request["params"]["input"][0]["text"], "Also this");
    }

    #[test]
    fn a_prompt_the_turn_took_and_a_turn_that_follows_are_reported() {
        let mut parser = parser(turn(Access::Full, false));
        let taken = r#"{"method":"item/started","params":{"item":{"type":"userMessage","id":"u","clientId":"m2"}}}"#;
        assert_eq!(parser.parse(taken), vec![AgentEvent::Taken { id: "m2".to_string() }]);

        let started = r#"{"method":"turn/started","params":{"turn":{"id":"turn1"}}}"#;
        let turn = AgentEvent::Turn { id: "turn1".to_string() };
        assert_eq!(parser.parse(started), vec![turn.clone(), AgentEvent::Thinking { active: true }]);
        let ended = parser.parse(r#"{"method":"turn/completed","params":{"turn":{"id":"turn1","status":"failed","error":{"message":"Out of credits"}}}}"#);
        let summary = TurnSummary { is_error: true, ..Default::default() };
        let completed =
            AgentEvent::Completed { summary, result_text: Some("Out of credits".to_string()), preempted: false };
        assert_eq!(ended, vec![completed]);
        assert_eq!(parser.parse(started), vec![AgentEvent::Woke, turn, AgentEvent::Thinking { active: true }]);
    }

    #[test]
    fn a_turn_the_usage_limit_ends_says_when_the_limit_resets() {
        let mut parser = parser(turn(Access::Full, false));
        parser.parse(r#"{"id":2,"result":{"thread":{"id":"t1"}}}"#);
        let limits = r#"{"method":"account/rateLimits/updated","params":{"rateLimits":{"limitId":"codex",
            "primary":{"usedPercent":100,"resetsAt":1800000000},"secondary":{"usedPercent":40,"resetsAt":1800500000}}}}"#;
        let model = r#"{"method":"account/rateLimits/updated","params":{"rateLimits":{"limitId":"spark",
            "primary":{"usedPercent":100,"resetsAt":1900000000}}}}"#;
        let error = r#"{"method":"error","params":{"threadId":"t1","turnId":"u1","willRetry":false,
            "error":{"message":"You've hit your usage limit.","codexErrorInfo":"usageLimitExceeded"}}}"#;
        let failed = r#"{"method":"turn/completed","params":{"threadId":"t1","turn":{"id":"u1","status":"failed",
            "error":{"message":"You've hit your usage limit."}}}}"#;

        assert_eq!(parser.parse(limits), vec![]);
        assert_eq!(parser.parse(model), vec![], "a model's own limit isn't the account's");
        assert_eq!(parser.parse(error), vec![]);
        let ended = parser.parse(failed);
        assert!(
            matches!(&ended[..], [AgentEvent::Limited { resets_at: Some(at) }, AgentEvent::Completed { .. }] if *at == 1_800_000_000.0)
        );

        let other = r#"{"method":"turn/completed","params":{"threadId":"t1","turn":{"status":"failed",
            "error":{"message":"Bad request","codexErrorInfo":{"badRequest":{}}}}}}"#;
        assert!(matches!(parser.parse(other)[..], [AgentEvent::Completed { .. }]));
    }

    #[test]
    fn a_command_and_an_edit_that_need_approval_are_asked_about_and_answered() {
        let mut parser = parser(turn(Access::Supervised, false));
        let asked = parser.parse(
            r#"{"id":0,"method":"item/commandExecution/requestApproval","params":{"itemId":"c","command":"/bin/bash -lc 'touch x'"}}"#,
        );
        let [AgentEvent::Approval(command)] = &asked[..] else { panic!("expected an approval, got {asked:?}") };
        assert_eq!((command.tool_name.as_str(), command.input.as_str()), ("Bash", r#"{"command":"touch x"}"#));
        let nothing = HashMap::new();
        let allowed: Value = serde_json::from_str(&answer(command, true, &nothing).unwrap()).unwrap();
        assert_eq!(allowed, json!({"id": 0, "result": {"decision": "accept"}}));
        let refused: Value = serde_json::from_str(&answer(command, false, &nothing).unwrap()).unwrap();
        assert_eq!(refused["result"]["decision"], "decline");
        let resolved = parser.parse(r#"{"method":"serverRequest/resolved","params":{"requestId":0}}"#);
        assert_eq!(resolved, vec![AgentEvent::ApprovalWithdrawn { id: command.id.clone() }]);

        parser.parse(
            r#"{"method":"item/started","params":{"item":{"type":"fileChange","id":"e","status":"inProgress",
            "changes":[{"path":"/srv/api/a.rs","kind":{"type":"update"},"diff":"-a\n+b"}]}}}"#,
        );
        let asked = parser.parse(r#"{"id":"r2","method":"item/fileChange/requestApproval","params":{"itemId":"e"}}"#);
        let [AgentEvent::Approval(edit)] = &asked[..] else { panic!("expected an approval, got {asked:?}") };
        let input: Value = serde_json::from_str(&edit.input).unwrap();
        assert_eq!((edit.tool_name.as_str(), &input["file_path"]), ("Edit", &json!("/srv/api/a.rs")));
        let allowed: Value = serde_json::from_str(&answer(edit, true, &nothing).unwrap()).unwrap();
        assert_eq!(allowed["id"], "r2", "the answer names the request as Codex did");
    }

    #[test]
    fn questions_are_answered_by_their_ids() {
        let mut parser = parser(turn(Access::Full, true));
        let asked = parser.parse(
            r#"{"id":4,"method":"item/tool/requestUserInput","params":{"itemId":"q","questions":[{"id":"color",
            "header":"Color","question":"Which color?","options":[{"label":"Blue","description":"Calm"}]}]}}"#,
        );
        let [AgentEvent::Approval(questions)] = &asked[..] else { panic!("expected questions, got {asked:?}") };
        assert_eq!(questions.tool_name, "AskUserQuestion");
        let chosen = HashMap::from([("Which color?".to_string(), "Blue".to_string())]);
        let answered: Value = serde_json::from_str(&answer(questions, true, &chosen).unwrap()).unwrap();
        assert_eq!(answered, json!({"id": 4, "result": {"answers": {"color": {"answers": ["Blue"]}}}}));
    }

    #[test]
    fn a_plan_is_presented_when_its_turn_ends_and_carried_out_in_the_next() {
        let mut parser = parser(turn(Access::AcceptEdits, true));
        parser.parse(r#"{"id":2,"result":{"thread":{"id":"t1"},"model":"gpt-6"}}"#);
        let plan = parser
            .parse(r###"{"method":"item/completed","params":{"item":{"type":"plan","id":"p","text":"## Do it"}}}"###);
        assert!(
            matches!(&plan[..], [_, AgentEvent::Tool { call }] if call.name == "ExitPlanMode" && call.status == ToolStatus::Succeeded)
        );
        let ended =
            parser.parse(r#"{"method":"turn/completed","params":{"turn":{"id":"turn1","status":"completed"}}}"#);
        let [AgentEvent::Completed { .. }, AgentEvent::Approval(approval)] = &ended[..] else {
            panic!("expected the turn's end and then the plan, got {ended:?}")
        };

        let nothing = HashMap::new();
        assert_eq!(answer(approval, false, &nothing), None);
        let implement: Value = serde_json::from_str(&answer(approval, true, &nothing).unwrap()).unwrap();
        assert_eq!(implement["method"], "turn/start");
        assert_eq!(implement["params"]["threadId"], "t1");
        assert_eq!(implement["params"]["collaborationMode"]["mode"], "default");
        assert_eq!(implement["params"]["collaborationMode"]["settings"]["model"], "gpt-6");
    }
}
