//! The way the agents of the Proxy accounts reach the account's proxy: an HTTP listener on the
//! loopback interface, which each agent calls with the token of its thread, and an iroh connection
//! to the proxy that carries each of its requests on a stream of its own, `PROXY_ALPN`. Claude Code
//! calls `/claude` and Codex `/codex`; Codex's WebSockets are bridged message by message. The
//! token never leaves the server: the proxy is told the thread instead.

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
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
use motile_protocol::PROXY_ALPN;
use motile_protocol::frame::{read_frame, write_frame};
use motile_protocol::proxy_wire::{ProxyAccount, ProxyReply, ProxyRequest, WsMessage};
use motile_protocol::wire::{Agent, AgentLimits};
use tokio::sync::{Mutex, mpsc, watch};

use crate::mcp::Tokens;

/// What the server's own calls, a title or a commit message, are sent to the proxy as.
pub const SERVER_THREAD: &str = "server";
/// What Codex reads the thread's token from, as `config.toml` says.
pub const CODEX_TOKEN_VARIABLE: &str = "MOTILE_PROXY_TOKEN";
const NO_PROXY: &str = "Your account has no proxy. Add one in Settings, or switch the thread to another account.";
const DIAL_WITHIN: Duration = Duration::from_secs(20);
const ANSWER_WITHIN: Duration = Duration::from_secs(30);
const CHUNK: usize = 64 * 1024;
/// What isn't the request's own, or would give the token away.
const NOT_PASSED: [&str; 14] = [
    "host",
    "authorization",
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

/// The variables the agent of a Proxy account runs with: where the proxy is, and `token`.
pub fn variables(agent: Agent, port: u16, token: &str) -> Vec<(String, String)> {
    let variables = match agent {
        Agent::Claude => vec![
            ("ANTHROPIC_BASE_URL", format!("http://127.0.0.1:{port}/claude")),
            ("ANTHROPIC_AUTH_TOKEN", token.to_string()),
            // A conversation's cache lasts an hour, as it does on a subscription, and not five minutes.
            ("ENABLE_PROMPT_CACHING_1H", "1".to_string()),
            // It asks the proxy for the models the account adds, as the 1M context window.
            ("CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY", "1".to_string()),
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

/// Has Codex in `folder` call the proxy on `port`, over WebSockets where it can.
pub fn write_codex_config(folder: &Path, port: u16) -> anyhow::Result<()> {
    std::fs::create_dir_all(folder)?;
    let config = format!(
        r#"model_provider = "motile"

[model_providers.motile]
name = "Motile Proxy"
base_url = "http://127.0.0.1:{port}/codex"
env_key = "{CODEX_TOKEN_VARIABLE}"
wire_api = "responses"
supports_websockets = true
"#
    );
    std::fs::write(folder.join("config.toml"), config)?;
    Ok(())
}

/// The connection to the account's proxy, dialled when a request needs it and again when it drops.
pub struct Tunnel {
    endpoint: Endpoint,
    key: watch::Receiver<Option<String>>,
    connection: Mutex<Option<(String, Connection)>>,
    /// The proxy's account each pinned thread goes to.
    pins: std::sync::Mutex<HashMap<String, String>>,
    /// The proxy's account that last answered each thread.
    placed: std::sync::Mutex<HashMap<String, String>>,
}

impl Tunnel {
    pub fn new(endpoint: Endpoint, key: watch::Receiver<Option<String>>) -> Arc<Self> {
        Arc::new(Self {
            endpoint,
            key,
            connection: Mutex::default(),
            pins: std::sync::Mutex::default(),
            placed: std::sync::Mutex::default(),
        })
    }

    /// Pins the thread's requests to one of the proxy's accounts, or lets the proxy place them.
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

    /// The id of the proxy's account that last answered the thread.
    pub fn placed(&self, thread: &str) -> Option<String> {
        self.placed.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(thread).cloned()
    }

    fn answered(&self, thread: &str, account: Option<String>) {
        let Some(account) = account else { return };
        self.placed.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(thread.to_string(), account);
    }

    /// The proxy's accounts, which the server's agents can be pinned to.
    pub async fn accounts(&self) -> anyhow::Result<Vec<ProxyAccount>> {
        let asked = async {
            let (mut send, mut recv) = self.open(&ProxyRequest::Accounts).await?;
            send.finish()?;
            match read_frame(&mut recv).await?.context("Your proxy hung up.")? {
                ProxyReply::Accounts { accounts } => Ok(accounts),
                ProxyReply::Error { message } => bail!(message),
                other => bail!("Your proxy answered {other:?}."),
            }
        };
        tokio::time::timeout(ANSWER_WITHIN, asked).await.context("Your proxy didn't answer in time.")?
    }

    pub fn has_proxy(&self) -> bool {
        self.key.borrow().is_some()
    }

    /// A stream to the proxy, which `request` starts.
    pub async fn open(&self, request: &ProxyRequest) -> anyhow::Result<(SendStream, RecvStream)> {
        let Some(key) = self.key.borrow().clone() else { bail!(NO_PROXY) };
        let connection = self.connection(&key, false).await?;
        let (mut send, recv) = match connection.open_bi().await {
            Ok(stream) => stream,
            Err(error) => {
                tracing::debug!("the connection to the proxy dropped: {error:#}");
                self.connection(&key, true).await?.open_bi().await.context("Your proxy hung up.")?
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
        let id = EndpointId::from_str(key).context("Your proxy's key isn't one.")?;
        let mut address = EndpointAddr::new(id);
        if let Some(direct) = std::env::var("MOTILE_PROXY_ADDR").ok().and_then(|direct| direct.parse().ok()) {
            address = address.with_ip_addr(direct);
        }
        let dialled = tokio::time::timeout(DIAL_WITHIN, self.endpoint.connect(address, PROXY_ALPN)).await;
        let connection = dialled.context("Your proxy didn't answer.")?.context("Your proxy can't be reached.")?;
        *kept = Some((key.to_string(), connection.clone()));
        Ok(connection)
    }

    /// The limits of the account a new thread of `agent` would start on, if the proxy has one.
    pub async fn pool(&self, agent: Agent) -> anyhow::Result<Option<AgentLimits>> {
        let asked = async {
            let (mut send, mut recv) = self.open(&ProxyRequest::Pool { agent }).await?;
            send.finish()?;
            match read_frame(&mut recv).await?.context("Your proxy hung up.")? {
                ProxyReply::Pool { limits } => Ok(limits),
                ProxyReply::Error { message } => bail!(message),
                other => bail!("Your proxy answered {other:?}."),
            }
        };
        tokio::time::timeout(ANSWER_WITHIN, asked).await.context("Your proxy didn't answer in time.")?
    }
}

#[derive(Clone)]
struct Way {
    tunnel: Arc<Tunnel>,
    tokens: Arc<Tokens>,
    /// The token of the server's own calls.
    own: Arc<str>,
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
pub async fn serve(tunnel: Arc<Tunnel>, tokens: Arc<Tokens>, own: String) -> anyhow::Result<u16> {
    let listener =
        tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.context("The way to the proxy can't listen.")?;
    let port = listener.local_addr()?.port();
    let app = axum::Router::new().fallback(handle).with_state(Way { tunnel, tokens, own: own.into() });
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            tracing::error!("the way to the proxy stopped: {error:#}");
        }
    });
    Ok(port)
}

async fn handle(State(way): State<Way>, request: Request) -> Response {
    let (mut parts, body) = request.into_parts();
    let Some(thread) = thread_of(&way.tokens, &way.own, &parts.headers) else {
        return refused(StatusCode::UNAUTHORIZED, "This token isn't one of your server's threads.");
    };
    let Some((agent, path)) = agent_and_path(&parts) else {
        return refused(StatusCode::NOT_FOUND, "Call /claude or /codex.");
    };
    if !way.tunnel.has_proxy() {
        return refused(StatusCode::SERVICE_UNAVAILABLE, NO_PROXY);
    }
    let headers = passed(&parts.headers);
    let pin = way.tunnel.pin_of(&thread);
    let asking = thread.clone();
    let upgrading =
        parts.headers.get(header::UPGRADE).is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"));
    let answered = match upgrading {
        true => match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
            Ok(upgrade) => {
                bridge(&way.tunnel, &asking, ProxyRequest::WebSocket { thread, agent, path, headers, pin }, upgrade)
                    .await
            }
            Err(rejection) => return rejection.into_response(),
        },
        false => {
            let request = ProxyRequest::Http { thread, agent, method: parts.method.to_string(), path, headers, pin };
            tunnel(&way.tunnel, &asking, request, body).await
        }
    };
    answered.unwrap_or_else(|error| refused(StatusCode::BAD_GATEWAY, &motile_protocol::error_text(&error)))
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

/// Sends the request and its body to the proxy as they come, and answers with what it answers, its
/// body as it comes.
async fn tunnel(tunnel: &Tunnel, thread: &str, request: ProxyRequest, body: Body) -> anyhow::Result<Response> {
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
    let reply = read_frame(&mut recv).await?.context("Your proxy hung up.")?;
    let (status, headers) = match reply {
        ProxyReply::HttpHead { status, headers, account } => {
            tunnel.answered(thread, account);
            (status, headers)
        }
        ProxyReply::Error { message } => return Ok(refused(StatusCode::BAD_GATEWAY, &message)),
        other => bail!("Your proxy answered {other:?}."),
    };
    let mut response = Response::new(body_of(recv));
    *response.status_mut() = StatusCode::from_u16(status).context("Your proxy answered with no status.")?;
    add_headers(response.headers_mut(), headers);
    Ok(response)
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

/// Opens the WebSocket at the proxy, and once it is open there, the agent's, passing each message
/// on until either side closes.
async fn bridge(
    tunnel: &Tunnel,
    thread: &str,
    request: ProxyRequest,
    upgrade: WebSocketUpgrade,
) -> anyhow::Result<Response> {
    let (send, mut recv) = tunnel.open(&request).await?;
    let headers = match read_frame(&mut recv).await?.context("Your proxy hung up.")? {
        ProxyReply::WebSocketOpen { headers, account } => {
            tunnel.answered(thread, account);
            headers
        }
        ProxyReply::Error { message } => return Ok(refused(StatusCode::BAD_GATEWAY, &message)),
        other => bail!("Your proxy answered {other:?}."),
    };
    let mut response = upgrade.on_upgrade(move |socket| pass_messages(socket, send, recv));
    add_headers(response.headers_mut(), headers);
    Ok(response)
}

async fn pass_messages(mut socket: WebSocket, mut send: SendStream, mut recv: RecvStream) {
    let (from_proxy, mut proxy_messages) = mpsc::channel(16);
    let reading = tokio::spawn(async move {
        while let Ok(Some(message)) = read_frame::<_, WsMessage>(&mut recv).await {
            if from_proxy.send(message).await.is_err() {
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
            message = proxy_messages.recv() => {
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
    fn the_token_and_the_connections_own_headers_stay_on_the_server() {
        let headers = parts(
            "/claude/v1/messages",
            &[
                ("authorization", "Bearer secret"),
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
    fn each_agent_is_pointed_at_the_proxy_with_the_token() {
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
            "env_key = \"MOTILE_PROXY_TOKEN\"",
            "wire_api = \"responses\"",
            "supports_websockets = true",
        ] {
            assert!(config.lines().any(|kept| kept == line), "{line} in {config}");
        }
    }
}
