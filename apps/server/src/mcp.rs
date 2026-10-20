//! The `motile` MCP server, on the loopback interface, which only the agents your server started
//! can call, each with the token of its thread: `read_thread` reads what a handoff left out,
//! `openui_guide` teaches programs, `check_ui` checks one before the agent shows it, and the other
//! tools start agents on any of your server's accounts and follow them.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use anyhow::Context;
use axum::extract::{Request, State};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use motile_protocol::wire::{Agent, HandoffEnd, Item, ItemKind};
use rmcp::handler::server::common::Extension;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ServerCapabilities, ServerConfig};
use rmcp::schemars::{self, JsonSchema};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::agents::PLAN_TOOL;
use crate::handoff::tool_status;
use crate::hub::Hub;
use crate::store::Store;
use crate::ui;

pub const SERVER_NAME: &str = "motile";
/// The tools as the agents name them.
pub const TOOLS: [&str; 9] = [
    "mcp__motile__read_thread",
    "mcp__motile__list_accounts",
    "mcp__motile__start_agent",
    "mcp__motile__agent_status",
    "mcp__motile__message_agent",
    "mcp__motile__answer_agent",
    "mcp__motile__stop_agent",
    "mcp__motile__openui_guide",
    "mcp__motile__check_ui",
];
/// What Claude Code reads its `Authorization` header from.
pub const AUTHORIZATION_VARIABLE: &str = "MOTILE_MCP_AUTHORIZATION";
/// Longer than the longest an agent's call waits, so that the agent's own client never gives up
/// on it first.
pub const TOOL_TIMEOUT_SECS: u64 = 65 * 60;
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 100;
const DEFAULT_CHARS: usize = 20_000;
const MAX_CHARS: usize = 50_000;

/// Where an agent reads threads, and the token it does so with.
#[derive(Clone, Debug)]
pub struct McpAccess {
    pub url: String,
    pub token: String,
}

/// The threads' tokens, kept only as their hashes, by the thread each is for.
#[derive(Default)]
pub struct Tokens(std::sync::Mutex<HashMap<[u8; 32], String>>);

impl Tokens {
    pub fn mint(&self, thread_id: &str) -> String {
        let token = hex::encode(rand::random::<[u8; 32]>());
        self.lock().insert(hash(&token), thread_id.to_string());
        token
    }

    /// The thread the token is for.
    pub fn thread_of(&self, token: &str) -> Option<String> {
        self.lock().get(&hash(token)).cloned()
    }

    pub fn forget(&self, thread_id: &str) {
        self.lock().retain(|_, kept| kept != thread_id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<[u8; 32], String>> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn hash(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

/// The thread whose agent calls.
#[derive(Clone)]
struct Caller(String);

/// Serves the tools on a port of the loopback interface that the system picks, and answers them.
pub async fn serve(hub: Weak<Hub>, tokens: Arc<Tokens>) -> anyhow::Result<u16> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.context("The thread reader can't listen.")?;
    let port = listener.local_addr()?.port();
    let config = StreamableHttpServerConfig::default().with_legacy_session_mode(false).with_json_response(true);
    let tools = move || Ok(Tools { hub: hub.clone(), tool_router: Tools::tool_router() });
    let service = StreamableHttpService::new(tools, Arc::new(LocalSessionManager::default()), config);
    let app =
        axum::Router::new().route_service("/mcp", service).layer(middleware::from_fn_with_state(tokens, authorize));
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            tracing::error!("the thread reader stopped: {error:#}");
        }
    });
    Ok(port)
}

async fn authorize(State(tokens): State<Arc<Tokens>>, mut request: Request, next: Next) -> Response {
    let given = request.headers().get(header::AUTHORIZATION).and_then(|value| value.to_str().ok());
    let token = given.and_then(|value| value.strip_prefix("Bearer "));
    let Some(thread_id) = token.and_then(|token| tokens.thread_of(token)) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    request.extensions_mut().insert(Caller(thread_id));
    next.run(request).await
}

#[derive(Clone)]
struct Tools {
    hub: Weak<Hub>,
    tool_router: ToolRouter<Self>,
}

impl Tools {
    fn hub(&self) -> anyhow::Result<Arc<Hub>> {
        self.hub.upgrade().context("Your server is stopping.")
    }
}

fn caller(parts: &Parts) -> anyhow::Result<String> {
    parts.extensions.get::<Caller>().map(|caller| caller.0.clone()).context("The caller has no thread.")
}

fn answered<T: Serialize>(result: anyhow::Result<T>) -> CallToolResult {
    match result.and_then(|value| Ok(serde_json::to_string(&value)?)) {
        Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
        Err(error) => CallToolResult::error(vec![ContentBlock::text(format!("{error:#}"))]),
    }
}

#[tool_router]
impl Tools {
    /// Reads a thread of this server: its messages, or everything that happened in it, a page at
    /// a time.
    #[tool(name = "read_thread", annotations(title = "Read a thread", read_only_hint = true))]
    async fn read_thread(&self, Parameters(request): Parameters<ReadThread>) -> CallToolResult {
        answered(self.hub().and_then(|hub| read_thread(hub.store(), &request)))
    }

    /// How to write an openui-lang program: the language, Motile's components and functions, the
    /// tools a program calls and examples, in pages. Read every page before you write a program,
    /// once in a thread.
    #[tool(name = "openui_guide", annotations(title = "Read the OpenUI guide", read_only_hint = true))]
    async fn openui_guide(&self, Parameters(request): Parameters<OpenuiGuide>) -> CallToolResult {
        let page = request.page.unwrap_or(1);
        match ui::guide_page(page) {
            Some(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
            None => CallToolResult::error(vec![ContentBlock::text(format!("The guide has no page {page}."))]),
        }
    }

    /// Checks an openui-lang program, with or without its fence, as Motile's clients will read it:
    /// whether it is valid, its errors with hints, the names it references and never defines
    /// (`unresolved`) and the statements root can't reach (`orphans`). One with no root statement
    /// is a patch, checked merged over this thread's latest program. Call it before sending a
    /// program you aren't sure of.
    #[tool(name = "check_ui", annotations(title = "Check a program", read_only_hint = true))]
    async fn check_ui(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<CheckUi>,
    ) -> CallToolResult {
        let checked = async {
            let items = self.hub()?.store().program_items(&caller(&parts)?)?;
            Ok(ui::check(&request.program, ui::latest(&items).as_deref()))
        };
        answered(checked.await)
    }

    /// Lists the accounts of your server you can start agents on: other Claude Code logins and
    /// Codex, with the models each offers and how much of its plan's limits it has used. Yours is
    /// marked.
    #[tool(name = "list_accounts", annotations(title = "List accounts", read_only_hint = true))]
    async fn list_accounts(&self, Extension(parts): Extension<Parts>) -> CallToolResult {
        let accounts = async { self.hub()?.accounts_for(&caller(&parts)?).await };
        answered(accounts.await)
    }

    /// Starts an agent in a thread of its own on one of your server's accounts, another Claude Code
    /// login or Codex, to work on a task while you go on. It is given only `prompt`, none of this
    /// conversation, so say everything it needs. It works in your folder unless `worktree` is set.
    /// Without `wait` this returns at once, and you are told when the agent finishes or asks
    /// something: end your turn then instead of checking on it. With `wait` it returns when the
    /// agent finishes, asks a question, or `timeout_seconds` pass, which never stops the agent.
    #[tool(name = "start_agent", annotations(title = "Start an agent"))]
    async fn start_agent(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<StartAgent>,
    ) -> CallToolResult {
        let started = async { self.hub()?.start_child(&caller(&parts)?, request).await };
        answered(started.await)
    }

    /// How the agents you started are doing, with the last reply of each that has finished. With
    /// `wait`, returns once all of them have finished, one asks a question, or `timeout_seconds`
    /// pass.
    #[tool(name = "agent_status", annotations(title = "Check on agents", read_only_hint = true))]
    async fn agent_status(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<AgentStatus>,
    ) -> CallToolResult {
        let wait = request.wait.then(|| {
            let seconds = request.timeout_seconds.map(std::time::Duration::from_secs);
            seconds.unwrap_or(crate::hub::WAIT).min(crate::hub::LONGEST_WAIT)
        });
        let states = async { self.hub()?.child_states(&caller(&parts)?, &request.ids, wait).await };
        answered(states.await)
    }

    /// Gives an agent you started another message. It starts the agent's next turn, or reaches it
    /// at once while it works, and you are told when that turn ends.
    #[tool(name = "message_agent", annotations(title = "Message an agent"))]
    async fn message_agent(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<MessageAgent>,
    ) -> CallToolResult {
        let state = async { self.hub()?.message_child(&caller(&parts)?, &request.id, request.text).await };
        answered(state.await)
    }

    /// Answers the questions an agent you started asks, each question's text to the answer.
    #[tool(name = "answer_agent", annotations(title = "Answer an agent"))]
    async fn answer_agent(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<AnswerAgent>,
    ) -> CallToolResult {
        let state = async { self.hub()?.answer_child(&caller(&parts)?, &request.id, request.answers).await };
        answered(state.await)
    }

    /// Stops an agent you started, and the agents it started.
    #[tool(name = "stop_agent", annotations(title = "Stop an agent"))]
    async fn stop_agent(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<StopAgent>,
    ) -> CallToolResult {
        let state = async { self.hub()?.stop_child(&caller(&parts)?, &request.id).await };
        answered(state.await)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Tools {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct StartAgent {
    /// Everything the agent needs to know to do the task. It sees nothing of your conversation.
    pub prompt: String,
    /// A few words its thread is called by.
    pub title: Option<String>,
    /// The part it plays, such as "reviewer". Its prompt starts by telling it.
    pub role: Option<String>,
    /// The account it runs on, by its name or id from list_accounts. Yours when neither this nor
    /// `agent` is given.
    pub account: Option<String>,
    /// "claude" or "codex": that agent's default account, or yours when you are that agent.
    pub agent: Option<String>,
    /// A model the account offers. Yours when it runs your agent, and the account's default
    /// otherwise.
    pub model: Option<String>,
    /// A reasoning effort the model offers.
    pub effort: Option<String>,
    /// "supervised", "accept_edits", "auto" or "full", never more than yours, which it has when
    /// this isn't given. Leave it unless the user asks: a supervised agent waits for the user at
    /// every action that needs approval.
    pub access: Option<String>,
    /// Works in a new git worktree from your branch instead of your folder. Changes you haven't
    /// committed aren't in it.
    #[serde(default)]
    pub worktree: bool,
    /// Returns once the agent finishes, asks a question, or `timeout_seconds` pass.
    #[serde(default)]
    pub wait: bool,
    /// How long `wait` waits: 600 when not given, at most 3600.
    pub timeout_seconds: Option<u64>,
    /// The same id for each retry of one request, so that a retry doesn't start a second agent.
    pub request_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct OpenuiGuide {
    /// The page, from 1, which it is when not given.
    page: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
struct CheckUi {
    /// The program's openui-lang text.
    program: String,
}

#[derive(Deserialize, JsonSchema)]
struct AgentStatus {
    /// The ids start_agent gave you.
    ids: Vec<String>,
    /// Returns once all of them have finished, one asks a question, or `timeout_seconds` pass.
    #[serde(default)]
    wait: bool,
    /// How long `wait` waits: 600 when not given, at most 3600.
    timeout_seconds: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
struct MessageAgent {
    id: String,
    text: String,
}

#[derive(Deserialize, JsonSchema)]
struct AnswerAgent {
    id: String,
    /// Each question's text, as the agent asked it, to the answer.
    answers: HashMap<String, String>,
}

#[derive(Deserialize, JsonSchema)]
struct StopAgent {
    id: String,
}

#[derive(Deserialize, JsonSchema, Default)]
pub struct ReadThread {
    /// The thread's id.
    pub thread_id: String,
    /// `messages` for what the user and the agents said and the plans they made, `activity` for
    /// everything, the agents' tool calls and the agents they started included.
    #[serde(default)]
    pub view: View,
    /// Only items after this position. -1 to start from the first.
    pub after: Option<i64>,
    /// How many items, at most 100. 50 when not given.
    pub limit: Option<usize>,
    /// Only this item.
    pub item_id: Option<String>,
    /// Where each item's text starts, in characters. Page through a long one with
    /// `next_text_offset`.
    pub text_offset: Option<usize>,
    /// How much of each item's text, at most 50,000 characters. 20,000 when not given.
    pub max_chars_per_item: Option<usize>,
}

#[derive(Deserialize, JsonSchema, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum View {
    #[default]
    Messages,
    Activity,
}

/// A page of the thread's items.
pub fn read_thread(store: &Store, request: &ReadThread) -> anyhow::Result<Value> {
    let thread_id = &request.thread_id;
    let thread = store.thread_summary(thread_id)?.context("No thread on this server has that id.")?;
    let items = match &request.item_id {
        Some(item_id) => vec![store.item(thread_id, item_id)?.context("That thread has no item with that id.")?],
        None => {
            let after = request.after.unwrap_or(-1);
            let items = store.items_since(thread_id, 0)?.into_iter();
            items.filter(|item| item.seq as i64 > after && shown(item, request.view)).collect()
        }
    };
    let limit = request.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let max_chars = request.max_chars_per_item.unwrap_or(DEFAULT_CHARS).clamp(1, MAX_CHARS);
    let offset = request.text_offset.unwrap_or(0);
    let page: Vec<&Item> = items.iter().take(limit).collect();
    let entries: Vec<Value> = page.iter().map(|item| entry(item, offset, max_chars)).collect();
    Ok(json!({
        "thread": {"id": thread_id, "title": thread.title, "project": thread.project,
                   "agent": agent_name(thread.agent), "model": thread.model},
        "items": entries,
        "next_position": page.last().map(|item| item.seq),
        "has_more": items.len() > page.len(),
    }))
}

fn shown(item: &Item, view: View) -> bool {
    if view == View::Activity {
        return true;
    }
    let message = match &item.kind {
        ItemKind::User { .. } | ItemKind::Assistant { .. } | ItemKind::Notice { .. } => true,
        ItemKind::Tool { call } => call.name == PLAN_TOOL,
        _ => false,
    };
    message && item.parent.is_none()
}

fn entry(item: &Item, offset: usize, max_chars: usize) -> Value {
    let (kind, status, text) = described(item);
    let length = text.chars().count();
    let slice: String = text.chars().skip(offset).take(max_chars).collect();
    let end = offset.saturating_add(max_chars);
    let created_at = chrono::DateTime::from_timestamp_millis((item.created_at * 1000.0) as i64);
    json!({
        "position": item.seq,
        "item_id": item.id,
        "parent": item.parent,
        "type": kind,
        "status": status,
        "text": slice,
        "text_truncated": end < length,
        "next_text_offset": (end < length).then_some(end),
        "created_at": created_at.map(|at| at.to_rfc3339()),
    })
}

fn described(item: &Item) -> (&'static str, &'static str, String) {
    match &item.kind {
        ItemKind::User { text, context, programs, .. } => ("user", "done", ui::told(text, context.as_ref(), programs)),
        ItemKind::Assistant { text } => ("assistant", "done", text.clone()),
        ItemKind::Thinking { text } => ("thinking", "done", text.clone()),
        ItemKind::Tool { call } => {
            let input: Value = serde_json::from_str(&call.input).unwrap_or_default();
            let output = call.output.as_deref().map(|output| format!("\n{output}")).unwrap_or_default();
            let status = tool_status(call.status);
            match call.name.as_str() {
                "Bash" => ("command", status, format!("$ {}{output}", input["command"].as_str().unwrap_or_default())),
                PLAN_TOOL => ("plan", status, input["plan"].as_str().unwrap_or_default().to_string()),
                name => ("tool", status, format!("{name} {}{output}", call.input)),
            }
        }
        ItemKind::Error { message } => ("error", "done", message.clone()),
        ItemKind::TurnEnd { summary } if summary.stopped => ("turn_end", "done", "Turn stopped".to_string()),
        ItemKind::TurnEnd { .. } => ("turn_end", "done", "Turn ended".to_string()),
        ItemKind::Handoff { from, to } => ("handoff", "done", format!("{} → {}", end_name(from), end_name(to))),
        ItemKind::Notice { text, .. } => ("notice", "done", text.clone()),
        ItemKind::Unknown => ("unknown", "done", String::new()),
    }
}

fn end_name(end: &HandoffEnd) -> String {
    end.name.clone().or_else(|| end.model.clone()).unwrap_or_else(|| agent_name(end.agent).to_string())
}

fn agent_name(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "Claude Code",
        Agent::Codex => "Codex",
    }
}

#[cfg(test)]
mod tests {
    use motile_protocol::wire::{Thread, ToolCall, ToolStatus};

    use super::*;
    use crate::store::{StoredThread, TitleSource};

    fn store_with_thread() -> (tempfile::TempDir, Store) {
        let folder = tempfile::tempdir().unwrap();
        let store = Store::open(&folder.path().join("motile.db")).unwrap();
        let thread = Thread {
            id: "t".into(),
            title: "Fix the login".into(),
            project_id: "p".into(),
            cwd: "/tmp".into(),
            agent: Agent::Codex,
            agent_account: "codex".into(),
            model: Some("gpt-6.1-sol".into()),
            effort: None,
            access: Default::default(),
            created_at: 0.0,
            updated_at: 0.0,
            done_at: None,
            position: 0.0,
            running: false,
            monitoring: false,
            needs_approval: false,
            asking: None,
            agents: 0,
            turn_ended_at: None,
            pull_request: None,
            watching: false,
            git_stage: None,
            interruption: None,
            parent: None,
            rev: 0,
        };
        let stored = StoredThread {
            thread,
            session_id: None,
            title_source: TitleSource::User,
            next_seq: 0,
            worktree: None,
            reports: false,
        };
        store.save_thread(&stored).unwrap();
        let call = |name: &str, input: &str| ToolCall {
            id: String::new(),
            name: name.into(),
            input: input.into(),
            output: Some("ok".into()),
            status: ToolStatus::Succeeded,
            agent: None,
        };
        let kinds = [
            ItemKind::User {
                text: "Fix the login".into(),
                attachments: vec![],
                skills: Vec::new(),
                context: None,
                programs: Vec::new(),
            },
            ItemKind::Thinking { text: "hmm".into() },
            ItemKind::Tool { call: call("Bash", r#"{"command":"cargo test"}"#) },
            ItemKind::Tool { call: call("Read", r#"{"file_path":"/a"}"#) },
            ItemKind::Assistant { text: "x".repeat(25) },
            ItemKind::TurnEnd { summary: Default::default() },
        ];
        for (seq, kind) in kinds.into_iter().enumerate() {
            let item = Item {
                id: format!("i{seq}"),
                seq: seq as u64,
                rev: 1,
                created_at: 0.0,
                media: vec![],
                parent: None,
                ui: Default::default(),
                kind,
            };
            store.save_item("t", &item).unwrap();
        }
        (folder, store)
    }

    fn read(store: &Store, request: ReadThread) -> Value {
        read_thread(store, &request).unwrap()
    }

    #[test]
    fn the_messages_and_the_activity_are_read_a_page_at_a_time() {
        let (_folder, store) = store_with_thread();
        let messages = read(&store, ReadThread { thread_id: "t".into(), ..Default::default() });
        assert_eq!(
            messages["thread"],
            json!({"id": "t", "title": "Fix the login", "project": "", "agent": "Codex", "model": "gpt-6.1-sol"})
        );
        let types: Vec<&str> =
            messages["items"].as_array().unwrap().iter().map(|item| item["type"].as_str().unwrap()).collect();
        assert_eq!(types, ["user", "assistant"]);
        assert_eq!((&messages["next_position"], &messages["has_more"]), (&json!(4), &json!(false)));

        let request = |after| ReadThread {
            thread_id: "t".into(),
            view: View::Activity,
            after,
            limit: Some(2),
            ..Default::default()
        };
        let first = read(&store, request(None));
        assert_eq!(first["items"][1]["text"], "hmm");
        assert_eq!((&first["next_position"], &first["has_more"]), (&json!(1), &json!(true)));
        let second = read(&store, request(Some(1)));
        assert_eq!(second["items"][0]["text"], "$ cargo test\nok");
        assert_eq!(second["items"][0]["status"], "succeeded");
        assert_eq!(second["items"][1]["text"], "Read {\"file_path\":\"/a\"}\nok");
        let last = read(&store, request(Some(4)));
        assert_eq!(last["items"][0]["text"], "Turn ended");
        assert_eq!(last["has_more"], false);
    }

    #[test]
    fn a_long_item_is_read_in_slices_until_none_is_left() {
        let (_folder, store) = store_with_thread();
        let slice = |offset| {
            let request = ReadThread {
                thread_id: "t".into(),
                item_id: Some("i4".into()),
                text_offset: offset,
                max_chars_per_item: Some(10),
                ..Default::default()
            };
            read(&store, request)["items"][0].clone()
        };
        let first = slice(None);
        assert_eq!((&first["text_truncated"], &first["next_text_offset"]), (&json!(true), &json!(10)));
        assert_eq!(slice(Some(20))["text"], "xxxxx");
        assert_eq!(slice(Some(20))["next_text_offset"], Value::Null);
        assert!(read_thread(&store, &ReadThread { thread_id: "nope".into(), ..Default::default() }).is_err());
    }

    #[test]
    fn a_token_is_known_by_its_hash_only() {
        let tokens = Tokens::default();
        let token = tokens.mint("t");
        assert_eq!(token.len(), 64);
        assert_eq!((tokens.thread_of(&token).as_deref(), tokens.thread_of("guess")), (Some("t"), None));
        assert!(!tokens.lock().keys().any(|kept| hex::encode(kept) == token));
    }
}
