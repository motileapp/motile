//! What servers and clients say to the account's gateway on `GATEWAY_ALPN`. Each request is a stream of
//! its own that starts with a `GatewayRequest` frame (`frame.rs`). A server tunnels its agents' HTTP
//! requests and WebSockets; a client manages the gateway's accounts and reads its log.

use serde::{Deserialize, Serialize};

use crate::wire::{Agent, AgentLimits, LimitReset, LimitWindow};

/// The header of the gateway's answer to a call that waits for the user's approval, which the
/// server asks for.
pub const MCP_NEEDS_APPROVAL: &str = "motile-needs-approval";
/// The header of a call the user approved, which only the server sets.
pub const MCP_APPROVED: &str = "motile-approved";

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GatewayRequest {
    /// A server's HTTP request for one of its threads. The body follows as raw bytes until the
    /// server finishes its side of the stream. The gateway answers `HttpHead` (or `Error`), then the
    /// response's body as raw bytes until it finishes its side. `path` is the agent's, with its
    /// query, under the base URL it was given: Claude Code's under `ANTHROPIC_BASE_URL`
    /// (`/v1/messages?beta=true`), Codex's under its provider's (`/v1/responses`). `pin` names the
    /// account the thread is pinned to: it goes there and nowhere else, and gets that account's limit
    /// error when it is spent.
    Http {
        thread: String,
        agent: Agent,
        method: String,
        path: String,
        headers: Vec<(String, String)>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pin: Option<String>,
    },
    /// A server's WebSocket for one of its threads (Codex). The gateway answers `WebSocketOpen` (or
    /// `Error`); then both sides send `WsMessage` frames until either sends `Close` or finishes. A
    /// refused upgrade is answered like `Http`: `HttpHead` and its body, 429 when every account is at
    /// its limit. The gateway closes the socket when the thread moves to another account, and Codex opens
    /// a new one. `pin` as for `Http`.
    WebSocket {
        thread: String,
        agent: Agent,
        path: String,
        headers: Vec<(String, String)>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pin: Option<String>,
    },
    /// A server's MCP request to one of the gateway's MCP servers, by its slug, for one of its threads:
    /// passed on to the MCP server's address with its login, like `Http`. Answered `HttpHead` and the
    /// body as it comes. A call of a tool that needs the user's approval is answered with
    /// `MCP_NEEDS_APPROVAL` until it comes again with `MCP_APPROVED`.
    Mcp { thread: String, mcp_server: String, method: String, headers: Vec<(String, String)> },
    /// A server asks for the slugs of the MCP servers that are on, which its agents are given. Answered
    /// with `McpServers`.
    McpServers,
    /// A server asks for its gateway account's limits: those of the account a new thread of `agent`
    /// would start on. Answered with `Pool`.
    Pool { agent: Agent },
    /// A server asks for the gateway's accounts, which its agents can be pinned to. Answered with
    /// `Accounts`.
    Accounts,
    /// A server spends one of the limit resets of the account its threads of `agent` are pinned to,
    /// `account`, or else of the one a new thread would start on. Answered with `Ok`.
    UseReset { agent: Agent, account: Option<String>, reset: String },
    /// A client manages the gateway.
    Manage { command: Manage },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GatewayReply {
    /// `account` is the id of the gateway's account that answered, when one did.
    HttpHead {
        status: u16,
        headers: Vec<(String, String)>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<String>,
    },
    WebSocketOpen {
        headers: Vec<(String, String)>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<String>,
    },
    Pool {
        limits: Option<AgentLimits>,
    },
    Accounts {
        accounts: Vec<GatewayAccount>,
    },
    State {
        state: GatewayState,
    },
    SignInStarted {
        sign_in: GatewaySignIn,
    },
    Log {
        entries: Vec<LogEntry>,
    },
    Captured {
        request: Option<CapturedRequest>,
    },
    Usage {
        rows: Vec<UsageRow>,
    },
    McpServers {
        mcp_servers: Vec<String>,
    },
    McpAdding {
        adding: McpAdding,
    },
    McpCalls {
        calls: Vec<McpCall>,
    },
    McpCall {
        call: Option<McpCallDetail>,
    },
    ToolResult {
        result: ToolResult,
    },
    Ok,
    Error {
        message: String,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    Text {
        text: String,
    },
    /// Base64.
    Binary {
        data: String,
    },
    Close {
        code: u16,
        reason: String,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Manage {
    /// Answered with `State`.
    State,
    /// Starts signing an account in. Answered with `SignInStarted`.
    SignIn { agent: Agent },
    /// Finishes a sign-in: Claude's with the code its page showed, Codex's device code with none,
    /// which waits until the user approved it in the browser. Answered with `State`.
    FinishSignIn { id: String, code: Option<String> },
    /// Answered with `State`.
    UpdateAccount { id: String, change: AccountChange },
    /// Answered with `State`.
    RemoveAccount { id: String },
    /// The request log, newest first, before `before` (an entry's id). Answered with `Log`.
    Log { before: Option<i64>, errors_only: bool, limit: u32 },
    /// A captured request in full. Answered with `Captured`.
    Captured { id: i64 },
    /// Starts or stops capturing requests in full. Answered with `State`.
    Capture { capture: Capture },
    /// Tokens by account and model since `since`. Answered with `Usage`.
    Usage { since: f64 },
    /// Adds the MCP at `url`, with `key` when it takes one, named `name` or as it names itself.
    /// Answered with `McpAdding`.
    AddMcpServer { url: String, name: Option<String>, key: Option<McpKey> },
    /// Signs an MCP server in again. Answered with `McpAdding`.
    SignInMcpServer { id: String },
    /// Finishes a sign-in with what its page sent back to `motile://mcp`. Answered with
    /// `McpAdding`.
    FinishMcpSignIn { state: String, code: String },
    /// Answered with `State`.
    UpdateMcpServer { id: String, change: McpChange },
    /// Asks the MCP server for its tools again. Answered with `State`.
    RefreshMcpServer { id: String },
    /// Answered with `State`.
    RemoveMcpServer { id: String },
    /// The calls agents made, newest first, before `before` (a call's id). Answered with
    /// `McpCalls`.
    McpCalls { before: Option<i64>, errors_only: bool, limit: u32 },
    /// A call with its arguments and result. Answered with `McpCall`.
    McpCall { id: i64 },
    /// Calls an MCP server's tool as an agent would. Answered with `ToolResult`.
    TryTool { id: String, tool: String, arguments: serde_json::Value },
    /// Spends one of the account's limit resets. Answered with `State`.
    UseReset { id: String, reset: String },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct McpKey {
    /// The header it goes in, `Authorization` unless the MCP server says otherwise.
    pub header: String,
    /// The header's whole value: `Bearer` and the key for `Authorization`.
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpAdding {
    /// It works, with its tools listed.
    Added { id: String },
    /// It signs in on `url`, which comes back to `motile://mcp` with a code and `state`.
    SignIn { state: String, url: String },
    /// It answered that it needs a key, which it doesn't sign in for.
    NeedsKey,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpChange {
    Name {
        name: String,
    },
    /// The name agents call it by: `mcp__<slug>__<tool>`. Clients now rename it with its `Name`.
    Slug {
        slug: String,
    },
    /// An MCP server that is off isn't given to agents.
    Enabled {
        enabled: bool,
    },
    /// What agents may do with one tool.
    ToolPermission {
        name: String,
        permission: ToolPermission,
    },
    /// What agents may do with every tool of a kind, and with the ones of that kind it adds later.
    GroupPermission {
        kind: ToolKind,
        permission: ToolPermission,
    },
    Key {
        key: McpKey,
    },
}

/// An MCP server's slug from its name, as Claude Code names claude.ai's connectors: letters, digits,
/// dashes and an underscore for anything else, never two in a row.
pub fn mcp_slug(name: &str) -> String {
    let mut slug = String::new();
    for character in name.trim().chars() {
        if character.is_ascii_alphanumeric() || character == '-' {
            slug.push(character);
        } else if !slug.is_empty() && !slug.ends_with('_') {
            slug.push('_');
        }
    }
    let slug = slug.trim_end_matches('_');
    if slug.is_empty() { "mcp_server".to_string() } else { slug.to_string() }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct McpServer {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub auth: McpAuth,
    pub status: McpStatus,
    pub tools: Vec<McpTool>,
    /// Its icon, as a URL or a `data:` URL.
    pub icon: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum McpAuth {
    None,
    Key,
    SignIn,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpStatus {
    Ready,
    /// Its login stopped working.
    SignInAgain,
    /// It couldn't be reached or refused the last time it was asked.
    Failing {
        error: String,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct McpTool {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
    /// It doesn't say it only reads.
    pub writes: bool,
    /// It isn't blocked, for the clients that only know a switch.
    pub enabled: bool,
    /// `None` from a gateway that only knew `writes`.
    #[serde(default)]
    pub kind: Option<ToolKind>,
    /// `None` from a gateway that only knew `enabled`.
    #[serde(default)]
    pub permission: Option<ToolPermission>,
}

impl McpTool {
    pub fn kind(&self) -> ToolKind {
        self.kind.unwrap_or(if self.writes { ToolKind::Other } else { ToolKind::Read })
    }

    pub fn permission(&self) -> ToolPermission {
        match self.enabled {
            true => self.permission.unwrap_or(ToolPermission::Allow),
            false => ToolPermission::Block,
        }
    }
}

/// What a tool does, as its annotations say.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// It says it only reads.
    Read,
    /// It says it writes or deletes.
    Write,
    /// It says neither.
    Other,
}

/// What agents may do with a tool.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolPermission {
    /// They call it without asking.
    #[default]
    Allow,
    /// Their server asks the user before each call.
    Ask,
    /// They don't see it.
    Block,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct McpCall {
    pub id: i64,
    pub at: f64,
    /// The server's device key, or none for a tool tried from Settings.
    pub server: Option<String>,
    pub thread: Option<String>,
    pub mcp_server: String,
    pub tool: String,
    pub latency_ms: u64,
    /// What went wrong: the tool's error, or the MCP server's.
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct McpCallDetail {
    pub call: McpCall,
    /// JSON, cut at 64 KB.
    pub arguments: String,
    pub result: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ToolResult {
    /// The tool's result as JSON.
    pub result: serde_json::Value,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AccountChange {
    Name {
        name: String,
    },
    /// Lower is used first. A lower tier only takes threads when no account of a higher one can.
    Priority {
        priority: u32,
    },
    /// An account that is off takes no new threads and loses the ones it has at their next request.
    Enabled {
        enabled: bool,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct GatewayState {
    pub version: String,
    pub accounts: Vec<GatewayAccount>,
    pub capture: Capture,
    /// Until when the capture runs.
    pub capture_until: Option<f64>,
    /// The newest entry of the request log, so that a client shows a new one as it comes.
    #[serde(default)]
    pub log_head: Option<i64>,
    /// `None` from a gateway too old to have MCP servers.
    #[serde(default)]
    pub mcp_servers: Option<Vec<McpServer>>,
    /// The newest call of the MCP servers.
    #[serde(default)]
    pub calls_head: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct GatewayAccount {
    /// Claude's organisation, or Codex's account: never the email.
    pub id: String,
    pub agent: Agent,
    pub name: String,
    /// Who is signed in, to show.
    pub email: Option<String>,
    pub plan: Option<String>,
    pub priority: u32,
    pub enabled: bool,
    pub status: AccountStatus,
    pub windows: Vec<LimitWindow>,
    /// How many uses `resets` have left, for the clients from before them.
    pub reset_credits: u32,
    #[serde(default)]
    pub resets: Vec<LimitReset>,
    /// Threads working on it now.
    pub threads: u32,
    /// The account a new thread of its agent starts on.
    pub next: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AccountStatus {
    Ready,
    /// Its tokens stopped working; it has to be signed in again.
    SignInAgain,
    /// It reached a limit and takes no threads until `until`.
    Resting {
        until: Option<f64>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct GatewaySignIn {
    pub id: String,
    pub agent: Agent,
    /// The page to open.
    pub url: String,
    /// The code to enter on that page (Codex's device code). Claude's page shows a code to paste
    /// back instead.
    pub user_code: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Capture {
    #[default]
    Off,
    /// Every request of one thread of one server.
    Thread { server: String, thread: String },
    /// The next `left` requests.
    Next { left: u32 },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct LogEntry {
    pub id: i64,
    pub at: f64,
    /// The server's device key.
    pub server: String,
    pub thread: String,
    pub account: Option<String>,
    pub model: Option<String>,
    pub path: String,
    pub status: u16,
    pub latency_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    /// The accounts tried before the one that answered.
    pub retries: u32,
    pub request_bytes: u64,
    pub response_bytes: u64,
    /// The error's body, for a response that failed.
    pub error: Option<String>,
    pub captured: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct CapturedRequest {
    pub entry: LogEntry,
    pub request_headers: Vec<(String, String)>,
    pub request_body: String,
    pub response_headers: Vec<(String, String)>,
    pub response_body: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct UsageRow {
    pub account: String,
    pub model: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slug_keeps_letters_digits_and_dashes_and_an_underscore_for_the_rest() {
        assert_eq!(mcp_slug("Unbind Banano"), "Unbind_Banano");
        assert_eq!(mcp_slug("  Google  Drive (work) "), "Google_Drive_work");
        assert_eq!(mcp_slug("a__b"), "a_b");
        assert_eq!(mcp_slug("my-server"), "my-server");
        assert_eq!(mcp_slug("¿?"), "mcp_server");
    }
}
