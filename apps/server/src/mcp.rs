//! The `motile` MCP server, on the loopback interface, which only the agents your server started
//! can call, each with the token of its thread: `read_thread` reads what a handoff left out,
//! `openui_guide` teaches programs, `check_ui` checks one before the agent shows it, the agent
//! tools start agents on any of your server's accounts and follow and drive them and the other
//! threads, and the rest start, list and change threads, projects and pull requests.

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
pub const TOOLS: [&str; 19] = [
    "mcp__motile__read_thread",
    "mcp__motile__list_accounts",
    "mcp__motile__start_agent",
    "mcp__motile__agent_status",
    "mcp__motile__message_agent",
    "mcp__motile__answer_agent",
    "mcp__motile__approve_agent",
    "mcp__motile__stop_agent",
    "mcp__motile__create_thread",
    "mcp__motile__list_threads",
    "mcp__motile__update_thread",
    "mcp__motile__thread_queue",
    "mcp__motile__list_projects",
    "mcp__motile__add_project",
    "mcp__motile__read_pull_request",
    "mcp__motile__link_pull_request",
    "mcp__motile__git_status",
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

/// Where an agent reads threads and reaches the gateway's MCP servers, and the token it does both with.
#[derive(Clone, Debug)]
pub struct McpAccess {
    pub url: String,
    pub token: String,
    /// Each MCP server's slug, which the agent knows it by, and its address.
    pub mcp_servers: Vec<(String, String)>,
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
    /// marked. With a gateway, it lists Gateway, which places agents on the gateway's accounts, and
    /// each of those accounts too, to pin an agent to.
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

    /// How the agents you started are doing, or any threads of your server, with the last reply of
    /// each that has finished, and the questions or the tool call each waits with. With `wait`,
    /// returns once all of them have finished, one asks something, or `timeout_seconds` pass.
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

    /// Gives an agent you started, or any thread of your server with no more access than you, another
    /// message. It starts the agent's next turn, or reaches it at once while it works. You are told
    /// when that turn ends of an agent you started.
    #[tool(name = "message_agent", annotations(title = "Message an agent"))]
    async fn message_agent(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<MessageAgent>,
    ) -> CallToolResult {
        let state = async { self.hub()?.message_child(&caller(&parts)?, &request.id, request.text).await };
        answered(state.await)
    }

    /// Answers the questions an agent you started asks, or the agent of any thread of your server
    /// with no more access than you, each question's text to the answer.
    #[tool(name = "answer_agent", annotations(title = "Answer an agent"))]
    async fn answer_agent(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<AnswerAgent>,
    ) -> CallToolResult {
        let state = async { self.hub()?.answer_child(&caller(&parts)?, &request.id, request.answers).await };
        answered(state.await)
    }

    /// Allows or refuses the tool call or the plan a thread waits with, which agent_status and the
    /// notice you are given show with its approval_id: a thread you started, with start_agent or
    /// create_thread, or one those started. The user allows what the user's threads ask. Allowing
    /// needs full access, yours and the thread's; with less, ask the user to allow it in its thread.
    #[tool(name = "approve_agent", annotations(title = "Approve an agent's tool call"))]
    async fn approve_agent(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<ApproveAgent>,
    ) -> CallToolResult {
        let state = async {
            let hub = self.hub()?;
            hub.approve_child(&caller(&parts)?, &request.id, &request.approval_id, request.allow).await
        };
        answered(state.await)
    }

    /// Stops an agent you started and the agents it started, or any thread of your server with no
    /// more access than you.
    #[tool(name = "stop_agent", annotations(title = "Stop an agent"))]
    async fn stop_agent(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<StopAgent>,
    ) -> CallToolResult {
        let state = async { self.hub()?.stop_child(&caller(&parts)?, &request.id).await };
        answered(state.await)
    }

    /// Starts a thread of its own, which the user's sidebar lists, as the user starts one: not an
    /// agent of yours, so you aren't told how it does. Follow it with agent_status if you need to.
    /// It works in its project's folder unless `worktree` is set.
    #[tool(name = "create_thread", annotations(title = "Create a thread"))]
    async fn create_thread(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<CreateThread>,
    ) -> CallToolResult {
        let created = async { self.hub()?.create_thread(&caller(&parts)?, request).await };
        answered(created.await)
    }

    /// Lists your server's threads, the last active first, or with `query` those whose messages
    /// have its words, the best first.
    #[tool(name = "list_threads", annotations(title = "List threads", read_only_hint = true))]
    async fn list_threads(&self, Parameters(request): Parameters<ListThreads>) -> CallToolResult {
        let listed = async { self.hub()?.list_threads(request).await };
        answered(listed.await)
    }

    /// Changes a thread's title, model, effort, account, access, archive or snooze: yours, or one of
    /// your server's with no more access than you. A field not given stays as it is.
    #[tool(name = "update_thread", annotations(title = "Update a thread"))]
    async fn update_thread(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<UpdateThread>,
    ) -> CallToolResult {
        let updated = async { self.hub()?.update_thread(&caller(&parts)?, request).await };
        answered(updated.await)
    }

    /// The messages that wait for a thread's agent to take them. `send_now` gives one to the agent
    /// at once, `cancel` takes it back, in a thread with no more access than you.
    #[tool(name = "thread_queue", annotations(title = "Manage a thread's queue"))]
    async fn thread_queue(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<ThreadQueue>,
    ) -> CallToolResult {
        let queued = async { self.hub()?.thread_queue(&caller(&parts)?, request).await };
        answered(queued.await)
    }

    /// Your server's projects, the folders its threads work in.
    #[tool(name = "list_projects", annotations(title = "List projects", read_only_hint = true))]
    async fn list_projects(&self) -> CallToolResult {
        let listed = async { Ok(self.hub()?.projects_json().await) };
        answered(listed.await)
    }

    /// Adds a project to your server: a folder that is there with `path`, a new empty repository with
    /// `name`, or a clone of a GitHub repository with `repo`. Give one of them.
    #[tool(name = "add_project", annotations(title = "Add a project"))]
    async fn add_project(&self, Parameters(request): Parameters<AddProject>) -> CallToolResult {
        let added = async { self.hub()?.add_project_for(request).await };
        answered(added.await)
    }

    /// What GitHub says of a thread's pull request, or of another one of its repository by number:
    /// its state, checks, review and latest activity.
    #[tool(name = "read_pull_request", annotations(title = "Read a pull request", read_only_hint = true))]
    async fn read_pull_request(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<ReadPullRequest>,
    ) -> CallToolResult {
        let read = async { self.hub()?.pull_request_for(&caller(&parts)?, request).await };
        answered(read.await)
    }

    /// Links a pull request to a thread, yours unless `thread_id` is given, or unlinks its own. With
    /// `watch` its agent is told when the pull request's checks finish, someone comments on it or it
    /// starts to conflict; the thread is settled once it merges.
    #[tool(name = "link_pull_request", annotations(title = "Link a pull request"))]
    async fn link_pull_request(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<LinkPullRequest>,
    ) -> CallToolResult {
        let linked = async { self.hub()?.link_pull_request_for(&caller(&parts)?, request).await };
        answered(linked.await)
    }

    /// What git says about the folder a thread works in, yours unless `thread_id` is given: its
    /// branch, how far it is from the remote, and the files that changed.
    #[tool(name = "git_status", annotations(title = "Read git status", read_only_hint = true))]
    async fn git_status(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(request): Parameters<GitStatusOf>,
    ) -> CallToolResult {
        let status = async { self.hub()?.git_status_for(&caller(&parts)?, request.thread_id, request.fetch).await };
        answered(status.await)
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
    /// `agent` is given. With a gateway, Gateway places the agent on whichever of the gateway's
    /// accounts has room, and you can pin it to one of them instead when you'd rather choose.
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
    /// The ids start_agent gave you, or the ids of any threads of your server.
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
struct ApproveAgent {
    id: String,
    /// The approval_id agent_status or the notice gave you, so that you allow only what you read.
    approval_id: String,
    /// `true` allows it, `false` refuses it.
    allow: bool,
}

#[derive(Deserialize, JsonSchema)]
struct StopAgent {
    id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateThread {
    /// The thread's first message: everything its agent needs to know. It sees nothing of your
    /// conversation.
    pub prompt: String,
    /// A few words the thread is called by. Its first message names it when this isn't given.
    pub title: Option<String>,
    /// The project it works in, by its id, name or folder from list_projects. Yours when not given.
    pub project: Option<String>,
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
    /// this isn't given. Leave it unless the user asks.
    pub access: Option<String>,
    /// Works in a new git worktree on a branch of its own instead of the project's folder.
    #[serde(default)]
    pub worktree: bool,
    /// The branch the worktree starts from: yours when the thread is in your project, the project's
    /// otherwise.
    pub base: Option<String>,
    /// The same id for each retry of one request, so that a retry doesn't start a second thread.
    pub request_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ListThreads {
    /// Only the threads of this project, by its id, name or folder.
    pub project: Option<String>,
    /// Only the threads whose messages have every word of it, the best first.
    pub query: Option<String>,
    /// Archived threads too.
    #[serde(default)]
    pub archived: bool,
    /// The threads agents started too, which the user's sidebar doesn't list.
    #[serde(default)]
    pub agents: bool,
    /// How many, at most 100. 30 when not given.
    pub limit: Option<usize>,
    /// How many to skip, for the next page: `next_offset` of the last one.
    pub offset: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateThread {
    /// The thread's id. Yours when not given.
    pub thread_id: Option<String>,
    pub title: Option<String>,
    /// A model its account offers; empty for the account's default.
    pub model: Option<String>,
    /// A reasoning effort the model offers; empty for the model's default.
    pub effort: Option<String>,
    /// Moves it to another account, by its name or id from list_accounts. Its agent goes on with
    /// the conversation so far.
    pub account: Option<String>,
    /// "supervised", "accept_edits", "auto" or "full", never more than yours.
    pub access: Option<String>,
    /// `true` archives it, `false` brings it back.
    pub archived: Option<bool>,
    /// Snoozes it until then, an RFC 3339 time. A time that has passed wakes it.
    pub snoozed_until: Option<String>,
}

#[derive(Deserialize, JsonSchema, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum QueueAction {
    List,
    SendNow,
    Cancel,
}

#[derive(Deserialize, JsonSchema)]
pub struct ThreadQueue {
    pub thread_id: String,
    pub action: QueueAction,
    /// The message `send_now` and `cancel` act on, as `list` names it.
    pub message_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AddProject {
    /// A folder on your server.
    pub path: Option<String>,
    /// The name of a new project, made where your server keeps new projects, with a git repository.
    pub name: Option<String>,
    /// A GitHub repository to clone, as owner/name.
    pub repo: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ReadPullRequest {
    /// The thread whose repository it is in. Yours when not given.
    pub thread_id: Option<String>,
    /// The pull request's number. The thread's own when not given.
    pub number: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
pub struct LinkPullRequest {
    /// The thread's id. Yours when not given.
    pub thread_id: Option<String>,
    /// The pull request to link, by its number in the thread's repository.
    pub number: Option<u64>,
    /// Unlinks the thread's pull request.
    #[serde(default)]
    pub unlink: bool,
    /// `true` has the thread's agent told what happens on its open pull request, `false` stops it.
    pub watch: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct GitStatusOf {
    /// The thread's id. Yours when not given.
    thread_id: Option<String>,
    /// Asks the remote first, to say how far behind the branch is.
    #[serde(default)]
    fetch: bool,
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
        ItemKind::Handoff { from, to, .. } => ("handoff", "done", format!("{} → {}", end_name(from), end_name(to))),
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
    use motile_protocol::wire::{Actor, Thread, ToolCall, ToolStatus};

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
            archived_at: None,
            snoozed_until: None,
            woke_at: None,
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
            started_by: None,
        };
        store.save_thread(&stored).unwrap();
        let call = |name: &str, input: &str| ToolCall {
            id: String::new(),
            name: name.into(),
            input: input.into(),
            output: Some("ok".into()),
            status: ToolStatus::Succeeded,
            agent: None,
            decided_by: Actor::Unknown,
        };
        let kinds = [
            ItemKind::User {
                text: "Fix the login".into(),
                attachments: vec![],
                skills: Vec::new(),
                context: None,
                programs: Vec::new(),
                by: Actor::Unknown,
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
