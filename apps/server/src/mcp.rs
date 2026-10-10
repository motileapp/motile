//! The `read_thread` tool, with which an agent reads what a handoff left out: an MCP server on
//! the loopback interface that only the agents your server started can call, each with the token
//! of its thread.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use motile_protocol::wire::{Agent, HandoffEnd, Item, ItemKind};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ServerCapabilities, ServerConfig};
use rmcp::schemars::{self, JsonSchema};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::agents::PLAN_TOOL;
use crate::handoff::tool_status;
use crate::store::Store;

pub const SERVER_NAME: &str = "motile";
/// The tool as the agents name it.
pub const READ_THREAD_TOOL: &str = "mcp__motile__read_thread";
/// What Claude Code reads its `Authorization` header from.
pub const AUTHORIZATION_VARIABLE: &str = "MOTILE_MCP_AUTHORIZATION";
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

    pub fn knows(&self, token: &str) -> bool {
        self.lock().contains_key(&hash(token))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<[u8; 32], String>> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn hash(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

/// Serves the tool on a port of the loopback interface that the system picks, and answers it.
pub async fn serve(store: Arc<Store>, tokens: Arc<Tokens>) -> anyhow::Result<u16> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.context("The thread reader can't listen.")?;
    let port = listener.local_addr()?.port();
    let config = StreamableHttpServerConfig::default().with_legacy_session_mode(false).with_json_response(true);
    let reader = move || Ok(Reader { store: store.clone(), tool_router: Reader::tool_router() });
    let service = StreamableHttpService::new(reader, Arc::new(LocalSessionManager::default()), config);
    let app =
        axum::Router::new().route_service("/mcp", service).layer(middleware::from_fn_with_state(tokens, authorize));
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            tracing::error!("the thread reader stopped: {error:#}");
        }
    });
    Ok(port)
}

async fn authorize(State(tokens): State<Arc<Tokens>>, request: Request, next: Next) -> Response {
    let given = request.headers().get(header::AUTHORIZATION).and_then(|value| value.to_str().ok());
    let token = given.and_then(|value| value.strip_prefix("Bearer "));
    if !token.is_some_and(|token| tokens.knows(token)) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}

#[derive(Clone)]
struct Reader {
    store: Arc<Store>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl Reader {
    /// Reads a thread of this server: its messages, or everything that happened in it, a page at
    /// a time.
    #[tool(name = "read_thread", annotations(title = "Read a thread", read_only_hint = true))]
    async fn read_thread(&self, Parameters(request): Parameters<ReadThread>) -> CallToolResult {
        match read_thread(&self.store, &request) {
            Ok(page) => CallToolResult::success(vec![ContentBlock::text(page.to_string())]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(format!("{error:#}"))]),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Reader {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }
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
        ItemKind::User { .. } | ItemKind::Assistant { .. } => true,
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
        ItemKind::User { text, .. } => ("user", "done", text.clone()),
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
            plan: false,
            created_at: 0.0,
            updated_at: 0.0,
            done_at: None,
            position: 0.0,
            running: false,
            monitoring: false,
            needs_approval: false,
            agents: 0,
            turn_ended_at: None,
            pull_request: None,
            watching: false,
            git_stage: None,
            interruption: None,
            rev: 0,
        };
        let stored =
            StoredThread { thread, session_id: None, title_source: TitleSource::User, next_seq: 0, worktree: None };
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
            ItemKind::User { text: "Fix the login".into(), attachments: vec![] },
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
        assert!(tokens.knows(&token) && !tokens.knows("guess"));
        assert!(!tokens.lock().keys().any(|kept| hex::encode(kept) == token));
    }
}
