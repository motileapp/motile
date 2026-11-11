//! The way the agents of the Gateway accounts reach the account's gateway: an HTTP listener on the
//! loopback interface, which each agent calls with the token of its thread, and an iroh connection
//! to the gateway that carries each of its requests on a stream of its own, `GATEWAY_ALPN`. Claude Code
//! calls `/claude` and Codex `/codex`; Codex's WebSockets are bridged message by message. The
//! token never leaves the server: the gateway is told the thread instead. Any thread's agent calls the
//! gateway's MCP servers at `/mcp/<slug>`, which pass on as `Mcp` requests.

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Weak};
use std::time::Duration;

use anyhow::{Context, bail};
use axum::body::Body;
use axum::extract::ws::{CloseFrame, Message as WebSocketMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_lite::StreamExt;
use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr, EndpointId};
use motile_protocol::frame::{read_frame, write_frame};
use motile_protocol::gateway_wire::{
    GatewayAccount, GatewayReply, GatewayRequest, MCP_APPROVED, MCP_NEEDS_APPROVAL, WsMessage,
};
use motile_protocol::wire::{Agent, AgentLimits};
use motile_protocol::{GATEWAY_ALPN, now};
use tokio::sync::{Mutex, mpsc, watch};

use crate::hub::Hub;
use crate::mcp::Tokens;

/// What the server's own calls, a title or a commit message, are sent to the gateway as.
pub const SERVER_THREAD: &str = "server";
/// What Codex reads the thread's token from, as `config.toml` says.
pub const CODEX_TOKEN_VARIABLE: &str = "MOTILE_GATEWAY_TOKEN";
pub const CODEX_ACTOR_HEADER: &str = "x-openai-actor-authorization";
const NO_GATEWAY: &str = "Your account has no gateway. Add one in Settings, or switch the thread to another account.";
const DIAL_WITHIN: Duration = Duration::from_secs(20);
const ANSWER_WITHIN: Duration = Duration::from_secs(30);
const CHUNK: usize = 64 * 1024;
const MAX_MCP_REQUEST: usize = 16 * 1024 * 1024;
/// What isn't the request's own, or would give the token away.
const NOT_PASSED: [&str; 16] = [
    "host",
    MCP_APPROVED,
    "authorization",
    CODEX_ACTOR_HEADER,
    "x-api-key",
    "connection",
    "keep-alive",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "sec-websocket-key",
    "sec-websocket-version",
    "sec-websocket-extensions",
    "sec-websocket-accept",
];

/// The variables the agent of a Gateway account runs with: where the gateway is, and `token`.
pub fn variables(agent: Agent, port: u16, token: &str) -> Vec<(String, String)> {
    let variables = match agent {
        Agent::Claude => vec![
            ("ANTHROPIC_BASE_URL", format!("http://127.0.0.1:{port}/claude")),
            ("ANTHROPIC_AUTH_TOKEN", token.to_string()),
            // A conversation's cache lasts an hour, as it does on a subscription, and not five minutes.
            ("ENABLE_PROMPT_CACHING_1H", "1".to_string()),
        ],
        Agent::Codex => vec![(CODEX_TOKEN_VARIABLE, token.to_string())],
    };
    variables.into_iter().map(|(name, value)| (name.to_string(), value)).collect()
}

/// The variable a turn's agent is given its thread's token in.
pub fn token_variable(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "ANTHROPIC_AUTH_TOKEN",
        Agent::Codex => CODEX_TOKEN_VARIABLE,
    }
}

/// Has Codex in `folder` call the gateway on `port`, over WebSockets where it can.
pub fn write_codex_config(folder: &Path, port: u16) -> anyhow::Result<()> {
    std::fs::create_dir_all(folder)?;
    let config = format!(
        r#"model_provider = "motile"

[model_providers.motile]
name = "Motile Gateway"
base_url = "http://127.0.0.1:{port}/codex"
env_key = "{CODEX_TOKEN_VARIABLE}"
wire_api = "responses"
supports_websockets = true
"#
    );
    std::fs::write(folder.join("config.toml"), config)?;
    Ok(())
}

/// The connection to the account's gateway, dialled when a request needs it and again when it drops.
pub struct Tunnel {
    endpoint: Endpoint,
    key: watch::Receiver<Option<String>>,
    connection: Mutex<Option<(String, Connection)>>,
    /// The gateway's account each pinned thread goes to.
    pins: std::sync::Mutex<HashMap<String, String>>,
    /// The gateway's account that last answered each thread.
    placed: std::sync::Mutex<HashMap<String, String>>,
    /// When the limit that last refused each thread resets, which Claude Code doesn't say on a
    /// Gateway account.
    limit_resets: std::sync::Mutex<HashMap<String, f64>>,
}

impl Tunnel {
    pub fn new(endpoint: Endpoint, key: watch::Receiver<Option<String>>) -> Arc<Self> {
        Arc::new(Self {
            endpoint,
            key,
            connection: Mutex::default(),
            pins: std::sync::Mutex::default(),
            placed: std::sync::Mutex::default(),
            limit_resets: std::sync::Mutex::default(),
        })
    }

    /// Pins the thread's requests to one of the gateway's accounts, or lets the gateway place them.
    pub fn pin(&self, thread: &str, pin: Option<&str>) {
        let mut pins = self.pins.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match pin {
            Some(pin) => pins.insert(thread.to_string(), pin.to_string()),
            None => pins.remove(thread),
        };
    }

    fn pin_of(&self, thread: &str) -> Option<String> {
        self.pins.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(thread).cloned()
    }

    /// The id of the gateway's account that last answered the thread.
    pub fn placed(&self, thread: &str) -> Option<String> {
        self.placed.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(thread).cloned()
    }

    fn answered(&self, thread: &str, account: Option<String>) {
        let Some(account) = account else { return };
        self.placed.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(thread.to_string(), account);
    }

    /// When the limit that last refused the thread resets, if it is yet to.
    pub fn take_limit_reset(&self, thread: &str) -> Option<f64> {
        let reset = self.limit_resets.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(thread);
        reset.filter(|at| *at > now())
    }

    fn note_limit(&self, thread: &str, status: u16, headers: &[(String, String)]) {
        if status != 429 {
            return;
        }
        let mut resets = self.limit_resets.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match limit_reset(headers, now()) {
            Some(at) => resets.insert(thread.to_string(), at),
            None => resets.remove(thread),
        };
    }

    /// The gateway's accounts, which the server's agents can be pinned to.
    pub async fn accounts(&self) -> anyhow::Result<Vec<GatewayAccount>> {
        let asked = async {
            let (mut send, mut recv) = self.open(&GatewayRequest::Accounts).await?;
            send.finish()?;
            match read_frame(&mut recv).await?.context("Your gateway hung up.")? {
                GatewayReply::Accounts { accounts } => Ok(accounts),
                GatewayReply::Error { message } => bail!(message),
                other => bail!("Your gateway answered {other:?}."),
            }
        };
        tokio::time::timeout(ANSWER_WITHIN, asked).await.context("Your gateway didn't answer in time.")?
    }

    /// The slugs of the gateway's MCP servers that are on.
    pub async fn mcp_servers(&self) -> anyhow::Result<Vec<String>> {
        let asked = async {
            let (mut send, mut recv) = self.open(&GatewayRequest::McpServers).await?;
            send.finish()?;
            match read_frame(&mut recv).await?.context("Your gateway hung up.")? {
                GatewayReply::McpServers { mcp_servers } => Ok(mcp_servers),
                GatewayReply::Error { message } => bail!(message),
                other => bail!("Your gateway answered {other:?}."),
            }
        };
        tokio::time::timeout(ANSWER_WITHIN, asked).await.context("Your gateway didn't answer in time.")?
    }

    pub fn has_gateway(&self) -> bool {
        self.key.borrow().is_some()
    }

    /// A stream to the gateway, which `request` starts.
    pub async fn open(&self, request: &GatewayRequest) -> anyhow::Result<(SendStream, RecvStream)> {
        let Some(key) = self.key.borrow().clone() else { bail!(NO_GATEWAY) };
        let connection = self.connection(&key, false).await?;
        let (mut send, recv) = match connection.open_bi().await {
            Ok(stream) => stream,
            Err(error) => {
                tracing::debug!("the connection to the gateway dropped: {error:#}");
                self.connection(&key, true).await?.open_bi().await.context("Your gateway hung up.")?
            }
        };
        write_frame(&mut send, request).await?;
        Ok((send, recv))
    }

    async fn connection(&self, key: &str, again: bool) -> anyhow::Result<Connection> {
        let mut kept = self.connection.lock().await;
        if let Some((kept_key, connection)) = kept.as_ref()
            && !again
            && kept_key == key
            && connection.close_reason().is_none()
        {
            return Ok(connection.clone());
        }
        let id = EndpointId::from_str(key).context("Your gateway's key isn't one.")?;
        let mut address = EndpointAddr::new(id);
        if let Some(direct) = std::env::var("MOTILE_GATEWAY_ADDR").ok().and_then(|direct| direct.parse().ok()) {
            address = address.with_ip_addr(direct);
        }
        let dialled = tokio::time::timeout(DIAL_WITHIN, self.endpoint.connect(address, GATEWAY_ALPN)).await;
        let connection = dialled.context("Your gateway didn't answer.")?.context("Your gateway can't be reached.")?;
        *kept = Some((key.to_string(), connection.clone()));
        Ok(connection)
    }

    /// The limits of the account a new thread of `agent` would start on, if the gateway has one.
    pub async fn pool(&self, agent: Agent) -> anyhow::Result<Option<AgentLimits>> {
        let asked = async {
            let (mut send, mut recv) = self.open(&GatewayRequest::Pool { agent }).await?;
            send.finish()?;
            match read_frame(&mut recv).await?.context("Your gateway hung up.")? {
                GatewayReply::Pool { limits } => Ok(limits),
                GatewayReply::Error { message } => bail!(message),
                other => bail!("Your gateway answered {other:?}."),
            }
        };
        tokio::time::timeout(ANSWER_WITHIN, asked).await.context("Your gateway didn't answer in time.")?
    }
}

#[derive(Clone)]
struct Way {
    tunnel: Arc<Tunnel>,
    tokens: Arc<Tokens>,
    /// The token of the server's own calls.
    own: Arc<str>,
    /// What asks the user to approve a call of an MCP server's tool.
    hub: Weak<Hub>,
}

/// The thread the request's token is for: one `tokens` keeps, or the server's own calls for `own`.
fn thread_of(tokens: &Tokens, own: &str, headers: &HeaderMap) -> Option<String> {
    let header = |name: HeaderName| headers.get(name).and_then(|value| value.to_str().ok());
    let bearer = header(header::AUTHORIZATION).and_then(|value| value.strip_prefix("Bearer "));
    let token = bearer.or_else(|| header(HeaderName::from_static("x-api-key")))?;
    match token == own {
        true => Some(SERVER_THREAD.to_string()),
        false => tokens.thread_of(token),
    }
}

/// Listens on a port of the loopback interface that the system picks, for the threads whose tokens
/// `tokens` keeps and for the server's own calls, made with `own`.
pub async fn serve(tunnel: Arc<Tunnel>, tokens: Arc<Tokens>, own: String, hub: Weak<Hub>) -> anyhow::Result<u16> {
    let listener =
        tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.context("The way to the gateway can't listen.")?;
    let port = listener.local_addr()?.port();
    let app = axum::Router::new().fallback(handle).with_state(Way { tunnel, tokens, own: own.into(), hub });
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            tracing::error!("the way to the gateway stopped: {error:#}");
        }
    });
    Ok(port)
}

async fn handle(State(way): State<Way>, request: Request) -> Response {
    let (mut parts, body) = request.into_parts();
    let Some(thread) = thread_of(&way.tokens, &way.own, &parts.headers) else {
        return refused(StatusCode::UNAUTHORIZED, "This token isn't one of your server's threads.");
    };
    if !way.tunnel.has_gateway() {
        return refused(StatusCode::SERVICE_UNAVAILABLE, NO_GATEWAY);
    }
    let headers = passed(&parts.headers);
    if let Some(mcp_server) = mcp_server_of(&parts) {
        let answered = mcp(&way, thread, mcp_server, parts.method.to_string(), headers, body).await;
        return answered.unwrap_or_else(|error| refused(StatusCode::BAD_GATEWAY, &motile_protocol::error_text(&error)));
    }
    let Some((agent, path)) = agent_and_path(&parts) else {
        return refused(StatusCode::NOT_FOUND, "Call /claude, /codex or /mcp.");
    };
    let pin = way.tunnel.pin_of(&thread);
    let asking = thread.clone();
    let upgrading =
        parts.headers.get(header::UPGRADE).is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"));
    let answered = match upgrading {
        true => match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
            Ok(upgrade) => {
                bridge(&way.tunnel, &asking, GatewayRequest::WebSocket { thread, agent, path, headers, pin }, upgrade)
                    .await
            }
            Err(rejection) => return rejection.into_response(),
        },
        false => {
            let request = GatewayRequest::Http { thread, agent, method: parts.method.to_string(), path, headers, pin };
            tunnel(&way.tunnel, &asking, request, body).await
        }
    };
    answered.unwrap_or_else(|error| refused(StatusCode::BAD_GATEWAY, &motile_protocol::error_text(&error)))
}

/// An agent's request to the gateway's MCP server `mcp_server`. A call that waits for the user's
/// approval is asked of the user in the thread, and sent again approved, or refused.
async fn mcp(
    way: &Way,
    thread: String,
    mcp_server: String,
    method: String,
    mut headers: Vec<(String, String)>,
    body: Body,
) -> anyhow::Result<Response> {
    let body = axum::body::to_bytes(body, MAX_MCP_REQUEST).await.context("The request's body didn't arrive whole.")?;
    let request = |headers: &[(String, String)]| GatewayRequest::Mcp {
        thread: thread.clone(),
        mcp_server: mcp_server.clone(),
        method: method.clone(),
        headers: headers.to_vec(),
    };
    let response = tunnel(&way.tunnel, &thread, request(&headers), Body::from(body.clone())).await?;
    if !response.headers().contains_key(MCP_NEEDS_APPROVAL) {
        return Ok(response);
    }
    let Some(hub) = way.hub.upgrade() else { return Ok(response) };
    let message: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    let tool = message["params"]["name"].as_str().unwrap_or_default();
    let arguments = &message["params"]["arguments"];
    let input = if arguments.is_object() { arguments.to_string() } else { "{}".to_string() };
    if !hub.ask_for_mcp_call(&thread, format!("mcp__{mcp_server}__{tool}"), input).await {
        return Ok(refused_call(&message));
    }
    headers.push((MCP_APPROVED.to_string(), "1".to_string()));
    tunnel(&way.tunnel, &thread, request(&headers), Body::from(body)).await
}

/// The answer to a tool call the user refused.
fn refused_call(message: &serde_json::Value) -> Response {
    let content = serde_json::json!([{"type": "text", "text": "The user refused this call."}]);
    let answer =
        serde_json::json!({"jsonrpc": "2.0", "id": message["id"], "result": {"content": content, "isError": true}});
    axum::Json(answer).into_response()
}

/// Where a thread's agent reaches the gateway's MCP server `slug`.
pub fn mcp_server_url(port: u16, slug: &str) -> String {
    format!("http://127.0.0.1:{port}/mcp/{slug}")
}

/// The MCP server a request's path is for: `/mcp/<slug>`.
fn mcp_server_of(parts: &Parts) -> Option<String> {
    let slug = parts.uri.path().strip_prefix("/mcp/")?;
    (!slug.is_empty() && !slug.contains('/')).then(|| slug.to_string())
}

/// The agent the request's path is for, and the path it asks for under it.
fn agent_and_path(parts: &Parts) -> Option<(Agent, String)> {
    let path = parts.uri.path_and_query().map(|path| path.as_str()).unwrap_or("/");
    let (agent, rest) = match () {
        _ if path.starts_with("/claude") => (Agent::Claude, &path["/claude".len()..]),
        _ if path.starts_with("/codex") => (Agent::Codex, &path["/codex".len()..]),
        _ => return None,
    };
    match rest.chars().next() {
        None => Some((agent, "/".to_string())),
        Some('/') => Some((agent, rest.to_string())),
        Some('?') => Some((agent, format!("/{rest}"))),
        Some(_) => None,
    }
}

fn passed(headers: &HeaderMap) -> Vec<(String, String)> {
    let kept = headers.iter().filter(|(name, _)| !NOT_PASSED.contains(&name.as_str()));
    kept.filter_map(|(name, value)| Some((name.to_string(), value.to_str().ok()?.to_string()))).collect()
}

/// An error in the shape both agents read, an `error` with a `message`.
fn refused(status: StatusCode, message: &str) -> Response {
    let body = serde_json::json!({"type": "error", "error": {"type": "api_error", "message": message}});
    (status, axum::Json(body)).into_response()
}

/// Sends the request and its body to the gateway as they come, and answers with what it answers, its
/// body as it comes.
async fn tunnel(tunnel: &Tunnel, thread: &str, request: GatewayRequest, body: Body) -> anyhow::Result<Response> {
    let (mut send, mut recv) = tunnel.open(&request).await?;
    tokio::spawn(async move {
        let mut chunks = body.into_data_stream();
        while let Some(chunk) = chunks.next().await {
            let written = match chunk {
                Ok(chunk) => send.write_all(&chunk).await.is_ok(),
                Err(_) => false,
            };
            if !written {
                let _ = send.reset(0u32.into());
                return;
            }
        }
        let _ = send.finish();
    });
    let reply = read_frame(&mut recv).await?.context("Your gateway hung up.")?;
    let (status, headers) = match reply {
        GatewayReply::HttpHead { status, headers, account } => {
            tunnel.answered(thread, account);
            tunnel.note_limit(thread, status, &headers);
            (status, headers)
        }
        GatewayReply::Error { message } => return Ok(refused(StatusCode::BAD_GATEWAY, &message)),
        other => bail!("Your gateway answered {other:?}."),
    };
    let mut response = Response::new(body_of(recv));
    *response.status_mut() = StatusCode::from_u16(status).context("Your gateway answered with no status.")?;
    add_headers(response.headers_mut(), headers);
    Ok(response)
}

/// When a limit error says its limit resets: Anthropic's reset, or after its `retry-after`.
fn limit_reset(headers: &[(String, String)], now: f64) -> Option<f64> {
    let number = |name: &str| {
        let (_, value) = headers.iter().find(|(header, _)| header.eq_ignore_ascii_case(name))?;
        value.trim().parse::<f64>().ok()
    };
    number("anthropic-ratelimit-unified-reset").or_else(|| number("retry-after").map(|seconds| now + seconds))
}

fn add_headers(to: &mut HeaderMap, headers: Vec<(String, String)>) {
    for (name, value) in headers {
        if NOT_PASSED.contains(&name.to_ascii_lowercase().as_str()) || name.eq_ignore_ascii_case("content-length") {
            continue;
        }
        let (Ok(name), Ok(value)) = (HeaderName::from_bytes(name.as_bytes()), HeaderValue::from_str(&value)) else {
            continue;
        };
        to.append(name, value);
    }
}

fn body_of(recv: RecvStream) -> Body {
    let chunks = futures_lite::stream::unfold(Some(recv), |recv| async move {
        let mut recv = recv?;
        match recv.read_chunk(CHUNK).await {
            Ok(Some(bytes)) => Some((Ok(bytes), Some(recv))),
            Ok(None) => None,
            Err(error) => Some((Err(std::io::Error::other(error)), None)),
        }
    });
    Body::from_stream(chunks)
}

/// Opens the WebSocket at the gateway, and once it is open there, the agent's, passing each message
/// on until either side closes.
async fn bridge(
    tunnel: &Tunnel,
    thread: &str,
    request: GatewayRequest,
    upgrade: WebSocketUpgrade,
) -> anyhow::Result<Response> {
    let (send, mut recv) = tunnel.open(&request).await?;
    let headers = match read_frame(&mut recv).await?.context("Your gateway hung up.")? {
        GatewayReply::WebSocketOpen { headers, account } => {
            tunnel.answered(thread, account);
            headers
        }
        GatewayReply::Error { message } => return Ok(refused(StatusCode::BAD_GATEWAY, &message)),
        other => bail!("Your gateway answered {other:?}."),
    };
    let mut response = upgrade.on_upgrade(move |socket| pass_messages(socket, send, recv));
    add_headers(response.headers_mut(), headers);
    Ok(response)
}

async fn pass_messages(mut socket: WebSocket, mut send: SendStream, mut recv: RecvStream) {
    let (from_gateway, mut gateway_messages) = mpsc::channel(16);
    let reading = tokio::spawn(async move {
        while let Ok(Some(message)) = read_frame::<_, WsMessage>(&mut recv).await {
            if from_gateway.send(message).await.is_err() {
                return;
            }
        }
    });
    loop {
        tokio::select! {
            message = socket.recv() => {
                let Some(Ok(message)) = message else { break };
                let Some(message) = to_wire(message) else { continue };
                let closing = matches!(message, WsMessage::Close { .. });
                if write_frame(&mut send, &message).await.is_err() || closing {
                    break;
                }
            }
            message = gateway_messages.recv() => {
                let Some(message) = message else {
                    let _ = socket.send(WebSocketMessage::Close(None)).await;
                    break;
                };
                let closing = matches!(message, WsMessage::Close { .. });
                let Some(message) = from_wire(message) else { break };
                if socket.send(message).await.is_err() || closing {
                    break;
                }
            }
        }
    }
    let _ = send.finish();
    reading.abort();
}

fn to_wire(message: WebSocketMessage) -> Option<WsMessage> {
    Some(match message {
        WebSocketMessage::Text(text) => WsMessage::Text { text: text.to_string() },
        WebSocketMessage::Binary(data) => WsMessage::Binary { data: BASE64.encode(data) },
        WebSocketMessage::Close(frame) => {
            let (code, reason) =
                frame.map(|frame| (frame.code, frame.reason.to_string())).unwrap_or((1000, String::new()));
            WsMessage::Close { code, reason }
        }
        WebSocketMessage::Ping(_) | WebSocketMessage::Pong(_) => return None,
    })
}

fn from_wire(message: WsMessage) -> Option<WebSocketMessage> {
    Some(match message {
        WsMessage::Text { text } => WebSocketMessage::Text(text.into()),
        WsMessage::Binary { data } => WebSocketMessage::Binary(BASE64.decode(data).ok()?.into()),
        WsMessage::Close { code, reason } => WebSocketMessage::Close(Some(CloseFrame { code, reason: reason.into() })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(uri: &str, headers: &[(&str, &str)]) -> Parts {
        let mut request = axum::http::Request::builder().uri(uri);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        request.body(()).unwrap().into_parts().0
    }

    #[test]
    fn a_token_is_its_threads_and_the_servers_own_is_the_servers() {
        let tokens = Tokens::default();
        let token = tokens.mint("thread-1");
        let thread_of =
            |headers: &[(&str, &str)]| thread_of(&tokens, "own-token", &parts("/claude/v1/messages", headers).headers);
        let bearer = format!("Bearer {token}");
        assert_eq!(thread_of(&[("authorization", &bearer)]).as_deref(), Some("thread-1"));
        assert_eq!(thread_of(&[("x-api-key", &token)]).as_deref(), Some("thread-1"));
        assert_eq!(thread_of(&[("authorization", "Bearer own-token")]).as_deref(), Some(SERVER_THREAD));
        assert_eq!(thread_of(&[("authorization", "Bearer someone-elses")]), None);
        assert_eq!(thread_of(&[]), None);
        tokens.forget("thread-1");
        assert_eq!(thread_of(&[("authorization", &bearer)]), None);
    }

    #[test]
    fn a_path_says_its_agent_and_what_it_asks_for() {
        let of = |uri: &str| agent_and_path(&parts(uri, &[]));
        assert_eq!(of("/claude/v1/messages?beta=true"), Some((Agent::Claude, "/v1/messages?beta=true".into())));
        assert_eq!(of("/codex/responses"), Some((Agent::Codex, "/responses".into())));
        assert_eq!(of("/codex?x=1"), Some((Agent::Codex, "/?x=1".into())));
        assert_eq!(of("/claudette/v1/messages"), None);
        assert_eq!(of("/v1/messages"), None);
    }

    #[test]
    fn an_mcp_servers_path_names_its_slug() {
        let of = |uri: &str| mcp_server_of(&parts(uri, &[]));
        assert_eq!(of("/mcp/linear").as_deref(), Some("linear"));
        assert_eq!(of("/mcp/linear?x=1").as_deref(), Some("linear"));
        assert_eq!(of("/mcp/"), None);
        assert_eq!(of("/mcp/linear/extra"), None);
        assert_eq!(of("/claude/v1/messages"), None);
        assert_eq!(mcp_server_url(4100, "linear"), "http://127.0.0.1:4100/mcp/linear");
    }

    #[test]
    fn the_token_and_the_connections_own_headers_stay_on_the_server() {
        let headers = parts(
            "/claude/v1/messages",
            &[
                ("authorization", "Bearer secret"),
                (CODEX_ACTOR_HEADER, "Bearer secret"),
                ("x-api-key", "secret"),
                ("host", "127.0.0.1"),
                ("anthropic-version", "2023-06-01"),
                ("content-type", "application/json"),
            ],
        )
        .headers;
        let passed = passed(&headers);
        assert_eq!(
            passed,
            [
                ("anthropic-version".to_string(), "2023-06-01".to_string()),
                ("content-type".to_string(), "application/json".to_string())
            ]
        );
    }

    #[test]
    fn each_agent_is_pointed_at_the_gateway_with_the_token() {
        let claude: std::collections::HashMap<_, _> = variables(Agent::Claude, 4100, "t").into_iter().collect();
        assert_eq!(claude["ANTHROPIC_BASE_URL"], "http://127.0.0.1:4100/claude");
        assert_eq!(claude["ANTHROPIC_AUTH_TOKEN"], "t");
        assert_eq!(claude["ENABLE_PROMPT_CACHING_1H"], "1");
        assert_eq!(variables(Agent::Codex, 4100, "t"), [(CODEX_TOKEN_VARIABLE.to_string(), "t".to_string())]);

        let folder = tempfile::tempdir().unwrap();
        let codex = folder.path().join("codex");
        write_codex_config(&codex, 4100).unwrap();
        let config = std::fs::read_to_string(codex.join("config.toml")).unwrap();
        assert!(config.starts_with("model_provider = \"motile\"\n"), "{config}");
        for line in [
            "[model_providers.motile]",
            "base_url = \"http://127.0.0.1:4100/codex\"",
            "env_key = \"MOTILE_GATEWAY_TOKEN\"",
            "wire_api = \"responses\"",
            "supports_websockets = true",
        ] {
            assert!(config.lines().any(|kept| kept == line), "{line} in {config}");
        }
    }
}
