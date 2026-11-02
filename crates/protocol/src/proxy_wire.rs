//! What servers and clients say to the account's proxy on `PROXY_ALPN`. Each request is a stream of
//! its own that starts with a `ProxyRequest` frame (`frame.rs`). A server tunnels its agents' HTTP
//! requests and WebSockets; a client manages the proxy's accounts and reads its log.

use serde::{Deserialize, Serialize};

use crate::wire::{Agent, AgentLimits, LimitWindow};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProxyRequest {
    /// A server's HTTP request for one of its threads. The body follows as raw bytes until the
    /// server finishes its side of the stream. The proxy answers `HttpHead` (or `Error`), then the
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
    /// A server's WebSocket for one of its threads (Codex). The proxy answers `WebSocketOpen` (or
    /// `Error`); then both sides send `WsMessage` frames until either sends `Close` or finishes. A
    /// refused upgrade is answered like `Http`: `HttpHead` and its body, 429 when every account is at
    /// its limit. The proxy closes the socket when the thread moves to another account, and Codex opens
    /// a new one. `pin` as for `Http`.
    WebSocket {
        thread: String,
        agent: Agent,
        path: String,
        headers: Vec<(String, String)>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pin: Option<String>,
    },
    /// A server asks for its proxy account's limits: those of the account a new thread of `agent`
    /// would start on. Answered with `Pool`.
    Pool { agent: Agent },
    /// A server asks for the proxy's accounts, which its agents can be pinned to. Answered with
    /// `Accounts`.
    Accounts,
    /// A client manages the proxy.
    Manage { command: Manage },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProxyReply {
    /// `account` is the id of the proxy's account that answered, when one did.
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
        accounts: Vec<ProxyAccount>,
    },
    State {
        state: ProxyState,
    },
    SignInStarted {
        sign_in: ProxySignIn,
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
pub struct ProxyState {
    pub version: String,
    pub accounts: Vec<ProxyAccount>,
    pub capture: Capture,
    /// Until when the capture runs.
    pub capture_until: Option<f64>,
    /// The newest entry of the request log, so that a client shows a new one as it comes.
    #[serde(default)]
    pub log_head: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ProxyAccount {
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
    pub reset_credits: u32,
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
pub struct ProxySignIn {
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
