//! The gateway over real iroh connections on this machine, with a fake standing in for Anthropic,
//! OpenAI and their sign-ins.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::ws::{Message as WsFrame, WebSocket, WebSocketUpgrade};
use axum::extract::{Form, OriginalUri, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use iroh::endpoint::{Connection, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey};
use motile_gateway::access::Access;
use motile_gateway::config::Upstreams;
use motile_gateway::pricing::Prices;
use motile_gateway::serve::BindOptions;
use motile_protocol::GATEWAY_ALPN;
use motile_protocol::frame::{read_frame, write_frame};
use motile_protocol::gateway_wire::{
    AccountChange, AccountStatus, GatewayAccount, GatewayReply, GatewayRequest, GatewayState, MCP_APPROVED,
    MCP_NEEDS_APPROVAL, Manage, McpAdding, McpAuth, McpChange, McpKey, McpStatus, ToolKind, ToolPermission, WsMessage,
};
use motile_protocol::identity::DeviceKey;
use motile_protocol::wire::Agent;
use serde_json::{Value, json};
use tokio::sync::Notify;

const TIMEOUT: Duration = Duration::from_secs(30);

/// What the fake upstream saw of one request.
#[derive(Clone, Debug)]
struct Seen {
    path: String,
    headers: HeaderMap,
    body: String,
}

impl Seen {
    fn header(&self, name: &str) -> &str {
        self.headers.get(name).and_then(|value| value.to_str().ok()).unwrap_or_default()
    }
}

#[derive(Default)]
struct Fake {
    seen: Mutex<Vec<Seen>>,
    /// Access tokens whose requests are answered with a limit error.
    limited: Mutex<HashSet<String>>,
    /// Seconds Claude's tokens last.
    lifetime: AtomicU64,
    renewals: AtomicUsize,
    device_polls: AtomicUsize,
    codex_accounts: AtomicUsize,
    refuse_renewal: AtomicBool,
    /// The next response streams its first event, then waits for `release`.
    hold: AtomicBool,
    release: Notify,
    /// The next response breaks off after its first event.
    break_off: AtomicBool,
    polls: AtomicUsize,
    /// Where the fake is, for the metadata its MCP gives.
    base: Mutex<String>,
    /// The generation of the MCP login that works; a token of an older one is refused.
    mcp_generation: AtomicU64,
    mcp_renewals: AtomicUsize,
    mcp_refuse_renewal: AtomicBool,
}

impl Fake {
    fn seen(&self, path: &str) -> Vec<Seen> {
        self.seen.lock().unwrap().iter().filter(|seen| seen.path == path).cloned().collect()
    }

    fn see(&self, path: &str, headers: &HeaderMap, body: &[u8]) {
        let body = String::from_utf8_lossy(body).into_owned();
        self.seen.lock().unwrap().push(Seen { path: path.to_string(), headers: headers.clone(), body });
    }
}

type Shared = State<Arc<Fake>>;

fn bearer(headers: &HeaderMap) -> String {
    let value = headers.get("authorization").and_then(|value| value.to_str().ok()).unwrap_or_default();
    value.trim_start_matches("Bearer ").to_string()
}

fn now() -> u64 {
    motile_protocol::now() as u64
}

fn jwt(claims: Value) -> String {
    format!("e30.{}.sig", URL_SAFE_NO_PAD.encode(claims.to_string()))
}

async fn claude_token(State(fake): Shared, Json(body): Json<Value>) -> Response {
    let lifetime = fake.lifetime.load(Ordering::SeqCst);
    let org = match body["grant_type"].as_str() {
        Some("authorization_code") => {
            assert_eq!(body["client_id"], "9d1c250a-e61b-44d9-88ed-5944d1962f5e");
            assert!(body["code_verifier"].as_str().is_some_and(|verifier| verifier.len() >= 43));
            let code = body["code"].as_str().unwrap();
            if code.starts_with("wrong") {
                return (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid_grant"}))).into_response();
            }
            code.to_string()
        }
        Some("refresh_token") => {
            if fake.refuse_renewal.load(Ordering::SeqCst) {
                let refused =
                    json!({"error": "invalid_grant", "error_description": "Refresh token not found or invalid"});
                return (StatusCode::BAD_REQUEST, Json(refused)).into_response();
            }
            fake.renewals.fetch_add(1, Ordering::SeqCst);
            body["refresh_token"].as_str().unwrap().trim_start_matches("refresh-").to_string()
        }
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let generation = fake.renewals.load(Ordering::SeqCst);
    let (person, organization) =
        org.split_once('.').map_or((None, org.as_str()), |(person, team)| (Some(person), team));
    Json(json!({
        "access_token": format!("claude-{org}-{generation}"),
        "refresh_token": format!("refresh-{org}"),
        "expires_in": lifetime,
        "account": {"uuid": person, "email_address": format!("{org}@example.com")},
        "organization": {"uuid": organization, "name": "Org"},
    }))
    .into_response()
}

async fn claude_profile(headers: HeaderMap) -> Json<Value> {
    let org = bearer(&headers).split('-').nth(1).unwrap_or_default().to_string();
    Json(json!({
        "account": {"email": format!("{org}@example.com")},
        "organization": {"uuid": org, "organization_type": "claude_max", "rate_limit_tier": "default_claude_max_20x"},
    }))
}

async fn claude_usage(State(fake): Shared) -> Json<Value> {
    fake.polls.fetch_add(1, Ordering::SeqCst);
    Json(json!({"five_hour": {"utilization": 10.0, "resets_at": null}}))
}

async fn messages(State(fake): Shared, headers: HeaderMap, body: Bytes) -> Response {
    fake.see("/v1/messages", &headers, &body);
    let token = bearer(&headers);
    if fake.limited.lock().unwrap().contains(&token) {
        let reset = (now() + 3600).to_string();
        let error = json!({"type": "error", "error": {"type": "rate_limit_error", "message": "limit"}});
        let headers = [
            ("anthropic-ratelimit-unified-status", "rejected".to_string()),
            ("anthropic-ratelimit-unified-reset", reset),
        ];
        return (StatusCode::TOO_MANY_REQUESTS, headers, Json(error)).into_response();
    }
    if !token.starts_with("claude-") {
        let error =
            json!({"type": "error", "error": {"type": "authentication_error", "message": "Invalid bearer token"}});
        return (StatusCode::UNAUTHORIZED, Json(error)).into_response();
    }
    let start = json!({"type": "message_start", "message": {"model": "claude-opus-4-5", "usage": {
        "input_tokens": 10, "cache_creation_input_tokens": 100, "cache_read_input_tokens": 1000, "output_tokens": 1}}});
    let delta = json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 50}});
    let first = format!("event: message_start\ndata: {start}\n\n");
    let last = format!("event: message_delta\ndata: {delta}\n\n");
    let hold = fake.hold.swap(false, Ordering::SeqCst);
    let break_off = fake.break_off.swap(false, Ordering::SeqCst);
    let events = async_stream(fake.clone(), first, last, hold, break_off);
    let headers = [
        ("content-type", "text/event-stream".to_string()),
        ("anthropic-ratelimit-unified-5h-utilization", "0.25".to_string()),
        ("anthropic-ratelimit-unified-5h-reset", (now() + 3600).to_string()),
        ("anthropic-ratelimit-unified-7d-utilization", "0.5".to_string()),
    ];
    (headers, Body::from_stream(events)).into_response()
}

/// The first event at once, then the last, after `release` when held, or an error in its place.
fn async_stream(
    fake: Arc<Fake>,
    first: String,
    last: String,
    hold: bool,
    break_off: bool,
) -> impl futures_util::Stream<Item = Result<Bytes, std::io::Error>> {
    futures_util::stream::unfold(0, move |step| {
        let (fake, first, last) = (fake.clone(), first.clone(), last.clone());
        async move {
            match step {
                0 => Some((Ok(Bytes::from(first)), 1)),
                1 if break_off => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Some((Err(std::io::Error::other("cut")), 2))
                }
                1 => {
                    if hold {
                        fake.release.notified().await;
                    }
                    Some((Ok(Bytes::from(last)), 2))
                }
                _ => None,
            }
        }
    })
}

async fn device_code() -> Json<Value> {
    Json(json!({"device_auth_id": "device-1", "user_code": "ABCD-1234", "interval": "1"}))
}

async fn device_token(State(fake): Shared, Json(body): Json<Value>) -> Response {
    assert_eq!(body["user_code"], "ABCD-1234");
    if fake.device_polls.fetch_add(1, Ordering::SeqCst).is_multiple_of(2) {
        return StatusCode::FORBIDDEN.into_response();
    }
    Json(json!({"authorization_code": "auth-code", "code_verifier": "verifier", "code_challenge": "c"})).into_response()
}

async fn openai_token(State(fake): Shared, Form(form): Form<Vec<(String, String)>>) -> Json<Value> {
    let field = |name: &str| form.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str());
    assert_eq!(field("client_id"), Some("app_EMoamEEZ73f0CkXaXp7hrann"));
    assert_eq!(field("code"), Some("auth-code"));
    let account = fake.codex_accounts.fetch_add(1, Ordering::SeqCst) + 1;
    let id_token = jwt(json!({"email": "c@example.com", "https://api.openai.com/auth": {
        "chatgpt_account_id": format!("acct-{account}"), "chatgpt_plan_type": "pro"}}));
    let access = jwt(json!({"exp": now() + 86400 * 5, "account": account}));
    Json(json!({"id_token": id_token, "access_token": access, "refresh_token": "codex-refresh"}))
}

async fn codex_http(State(fake): Shared, headers: HeaderMap, body: Bytes) -> Response {
    fake.see("/backend-api/codex/responses", &headers, &body);
    if fake.limited.lock().unwrap().contains(&bearer(&headers)) {
        let error = json!({"error": {"type": "usage_limit_reached", "resets_at": now() + 3600}});
        return (StatusCode::TOO_MANY_REQUESTS, Json(error)).into_response();
    }
    let completed = json!({"type": "response.completed", "response": {"model": "gpt-5", "usage": {
        "input_tokens": 500, "input_tokens_details": {"cached_tokens": 400}, "output_tokens": 20}}});
    let headers = [
        ("content-type", "text/event-stream"),
        ("x-codex-primary-used-percent", "5"),
        ("x-codex-primary-window-minutes", "300"),
    ];
    (headers, format!("event: response.completed\ndata: {completed}\n\n")).into_response()
}

async fn codex_image(
    State(fake): Shared,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Json<Value> {
    fake.see(uri.path(), &headers, &body);
    Json(json!({"created": 0, "data": [{"b64_json": "aW1hZ2U="}]}))
}

async fn codex_socket(State(fake): Shared, headers: HeaderMap, upgrade: WebSocketUpgrade) -> Response {
    fake.see("/backend-api/codex/responses (ws)", &headers, b"");
    upgrade.on_upgrade(move |socket| answer_socket(fake, socket))
}

async fn answer_socket(fake: Arc<Fake>, mut socket: WebSocket) {
    while let Some(Ok(WsFrame::Text(text))) = socket.recv().await {
        let headers = HeaderMap::new();
        fake.see("ws response.create", &headers, text.as_bytes());
        let limits = json!({"type": "codex.rate_limits", "rate_limits": {
            "primary": {"used_percent": 7.0, "window_minutes": 300, "reset_at": now() + 3600}}});
        let completed = json!({"type": "response.completed", "response": {"model": "gpt-5", "usage": {
            "input_tokens": 300, "input_tokens_details": {"cached_tokens": 100}, "output_tokens": 30}}});
        for event in [limits, completed] {
            if socket.send(WsFrame::Text(event.to_string().into())).await.is_err() {
                return;
            }
        }
    }
}

async fn codex_usage() -> Json<Value> {
    Json(json!({"plan_type": "pro", "rate_limit": {
        "primary_window": {"used_percent": 3, "limit_window_seconds": 18000, "reset_at": now() + 3600}},
        "rate_limit_reset_credits": {"available_count": 1}}))
}

fn mcp_tools() -> Value {
    json!([
        {"name": "list_issues", "description": "Lists issues", "inputSchema": {"type": "object"},
         "annotations": {"readOnlyHint": true}},
        {"name": "create_issue", "description": "Files an issue", "inputSchema": {"type": "object",
         "properties": {"title": {"type": "string"}}}},
    ])
}

/// A tracker's MCP, whose answers to calls come as events and the rest as JSON.
async fn mcp_answer(fake: &Fake, path: &str, headers: &HeaderMap, body: &[u8]) -> Response {
    fake.see(path, headers, body);
    let message: Value = serde_json::from_slice(body).unwrap_or_default();
    let id = message["id"].clone();
    let result = match message["method"].as_str() {
        Some("initialize") => json!({"protocolVersion": "2025-06-18", "capabilities": {"tools": {}},
                                     "serverInfo": {"name": "fake-tracker", "title": "Fake Tracker"}}),
        Some("tools/list") => json!({"tools": mcp_tools()}),
        Some("tools/call") => {
            let text = format!("called {} with {}", message["params"]["name"], message["params"]["arguments"]);
            let answer = json!({"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": text}]}});
            let events = format!("event: message\ndata: {answer}\n\n");
            return ([("content-type", "text/event-stream")], events).into_response();
        }
        _ => return StatusCode::ACCEPTED.into_response(),
    };
    let answer = json!({"jsonrpc": "2.0", "id": id, "result": result});
    ([("mcp-session-id", "session-1")], Json(answer)).into_response()
}

async fn mcp_with_key(State(fake): Shared, headers: HeaderMap, body: Bytes) -> Response {
    if headers.get("x-api-key").and_then(|value| value.to_str().ok()) != Some("k1") {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    mcp_answer(&fake, "/mcp/key", &headers, &body).await
}

async fn mcp_with_oauth(State(fake): Shared, headers: HeaderMap, body: Bytes) -> Response {
    let working = format!("mcp-access-{}", fake.mcp_generation.load(Ordering::SeqCst));
    if bearer(&headers) != working {
        let base = fake.base.lock().unwrap().clone();
        let challenge = format!(r#"Bearer resource_metadata="{base}/.well-known/oauth-protected-resource/mcp/oauth""#);
        return (StatusCode::UNAUTHORIZED, [("www-authenticate", challenge)]).into_response();
    }
    mcp_answer(&fake, "/mcp/oauth", &headers, &body).await
}

async fn mcp_resource(State(fake): Shared) -> Json<Value> {
    let base = fake.base.lock().unwrap().clone();
    Json(json!({"resource": format!("{base}/mcp/oauth"), "authorization_servers": [format!("{base}/as")]}))
}

async fn mcp_authorization_server(State(fake): Shared) -> Json<Value> {
    let base = fake.base.lock().unwrap().clone();
    Json(json!({
        "issuer": format!("{base}/as"),
        "authorization_endpoint": format!("{base}/as/authorize"),
        "token_endpoint": format!("{base}/as/token"),
        "registration_endpoint": format!("{base}/as/register"),
    }))
}

async fn mcp_register(State(fake): Shared, Json(body): Json<Value>) -> Json<Value> {
    let base = fake.base.lock().unwrap().clone();
    assert_eq!(body["redirect_uris"], json!([format!("{base}/mcp/callback")]));
    Json(json!({"client_id": "client-1"}))
}

async fn mcp_token(State(fake): Shared, Form(form): Form<Vec<(String, String)>>) -> Response {
    let param = |name: &str| form.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str());
    assert_eq!(param("client_id"), Some("client-1"));
    match param("grant_type") {
        Some("authorization_code") => {
            assert!(param("code_verifier").is_some_and(|verifier| verifier.len() >= 43));
            let base = fake.base.lock().unwrap().clone();
            assert_eq!(param("redirect_uri"), Some(format!("{base}/mcp/callback").as_str()));
            if param("code") != Some("good-code") {
                return (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid_grant"}))).into_response();
            }
        }
        Some("refresh_token") => {
            if fake.mcp_refuse_renewal.load(Ordering::SeqCst) {
                return (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid_grant"}))).into_response();
            }
            fake.mcp_renewals.fetch_add(1, Ordering::SeqCst);
        }
        _ => return StatusCode::BAD_REQUEST.into_response(),
    }
    let generation = fake.mcp_generation.load(Ordering::SeqCst);
    Json(
        json!({"access_token": format!("mcp-access-{generation}"), "refresh_token": "mcp-refresh", "expires_in": 3600}),
    )
    .into_response()
}

async fn serve_fake(fake: Arc<Fake>) -> String {
    let app = Router::new()
        .route("/mcp/key", post(mcp_with_key))
        .route("/mcp/oauth", post(mcp_with_oauth))
        .route("/.well-known/oauth-protected-resource/mcp/oauth", get(mcp_resource))
        .route("/.well-known/oauth-authorization-server/as", get(mcp_authorization_server))
        .route("/as/register", post(mcp_register))
        .route("/as/token", post(mcp_token))
        .route("/v1/oauth/token", post(claude_token))
        .route("/api/oauth/profile", get(claude_profile))
        .route("/api/oauth/usage", get(claude_usage))
        .route("/v1/messages", post(messages))
        .route("/api/accounts/deviceauth/usercode", post(device_code))
        .route("/api/accounts/deviceauth/token", post(device_token))
        .route("/oauth/token", post(openai_token))
        .route("/backend-api/codex/responses", get(codex_socket).post(codex_http))
        .route("/backend-api/codex/images/generations", post(codex_image))
        .route("/backend-api/codex/images/edits", post(codex_image))
        .route("/backend-api/wham/usage", get(codex_usage))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    *fake.base.lock().unwrap() = format!("http://{address}");
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

struct Harness {
    _dir: tempfile::TempDir,
    fake: Arc<Fake>,
    gateway_key: String,
    gateway_address: SocketAddr,
    server_key: DeviceKey,
    client_key: DeviceKey,
    gateway: Arc<motile_gateway::gateway::Gateway>,
    _endpoint: Endpoint,
}

impl Harness {
    async fn start() -> Self {
        let fake = Arc::new(Fake { lifetime: AtomicU64::new(8 * 3600), ..Fake::default() });
        let base = serve_fake(fake.clone()).await;
        let dir = tempfile::tempdir().unwrap();
        let key = DeviceKey::generate();
        let gateway_key = key.public();
        let server_key = DeviceKey::generate();
        let client_key = DeviceKey::generate();
        let prices = Prices::from_list(&json!({
            "claude-opus-4-5": {"litellm_provider": "anthropic", "input_cost_per_token": 5e-6, "output_cost_per_token": 2.5e-5},
            "gpt-5": {"litellm_provider": "openai", "input_cost_per_token": 1.25e-6, "output_cost_per_token": 1e-5},
        }));
        let options = motile_gateway::Options {
            data_dir: dir.path().to_path_buf(),
            key,
            access: Access::fixed(&[server_key.public()], &[client_key.public()]),
            upstreams: Upstreams::at(&base),
            bind: BindOptions { local_only: true, port: None },
            prices: Some(prices),
        };
        let (gateway, endpoint) = motile_gateway::start(options).await.unwrap();
        let port = endpoint.bound_sockets().iter().find(|address| address.is_ipv4()).unwrap().port();
        let gateway_address = SocketAddr::from(([127, 0, 0, 1], port));
        Self { _dir: dir, fake, gateway_key, gateway_address, server_key, client_key, gateway, _endpoint: endpoint }
    }

    async fn connect(&self, key: &DeviceKey) -> anyhow::Result<Link> {
        let endpoint =
            Endpoint::builder(presets::Minimal).secret_key(SecretKey::from_bytes(&key.to_bytes())).bind().await?;
        let address = EndpointAddr::new(EndpointId::from_str(&self.gateway_key)?).with_ip_addr(self.gateway_address);
        let connection = endpoint.connect(address, GATEWAY_ALPN).await?;
        Ok(Link { connection, _endpoint: endpoint })
    }

    async fn server(&self) -> Link {
        self.connect(&self.server_key).await.unwrap()
    }

    async fn client(&self) -> Link {
        self.connect(&self.client_key).await.unwrap()
    }

    async fn manage(&self, command: Manage) -> GatewayReply {
        ask(&*self.client().await, &GatewayRequest::Manage { command }).await
    }

    async fn state(&self) -> GatewayState {
        let GatewayReply::State { state } = self.manage(Manage::State).await else { panic!("a state") };
        state
    }

    /// Signs in the Claude organisation `org`, as a client does.
    async fn sign_in_claude(&self, org: &str) -> GatewayState {
        let GatewayReply::SignInStarted { sign_in } = self.manage(Manage::SignIn { agent: Agent::Claude }).await else {
            panic!("a sign-in")
        };
        let url = reqwest::Url::parse(&sign_in.url).unwrap();
        let query: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        let param = |name: &str| query.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone());
        assert_eq!(param("code_challenge_method").as_deref(), Some("S256"));
        assert_eq!(param("redirect_uri").as_deref(), Some("https://platform.claude.com/oauth/code/callback"));
        let state = param("state").unwrap();
        let wrong = Manage::FinishSignIn { id: sign_in.id.clone(), code: Some(format!("wrong#{state}")) };
        let GatewayReply::Error { message } = self.manage(wrong).await else { panic!("a wrong code is refused") };
        assert_eq!(message, "That code didn't work. Paste it again.");
        let finish = Manage::FinishSignIn { id: sign_in.id, code: Some(format!("{org}#{state}")) };
        let GatewayReply::State { state } = self.manage(finish).await else {
            panic!("the same sign-in takes the code")
        };
        state
    }

    async fn sign_in_codex(&self) -> GatewayState {
        let GatewayReply::SignInStarted { sign_in } = self.manage(Manage::SignIn { agent: Agent::Codex }).await else {
            panic!("a sign-in")
        };
        let GatewayReply::State { state } = self.manage(Manage::FinishSignIn { id: sign_in.id, code: None }).await
        else {
            panic!("a state")
        };
        state
    }

    async fn codex(&self, connection: &Connection, thread: &str, input: Value) -> String {
        let body = json!({"model": "gpt-5", "stream": true, "store": false, "input": input}).to_string();
        let (head, _) = http(connection, thread, Agent::Codex, "/responses", vec![], body).await;
        assert!(matches!(head, GatewayReply::HttpHead { status: 200, .. }), "{head:?}");
        let seen = self.fake.seen("/backend-api/codex/responses");
        seen.last().unwrap().header("chatgpt-account-id").to_string()
    }

    async fn claude(&self, connection: &Connection, thread: &str) -> (u16, String) {
        let body = json!({"model": "claude-opus-4-5", "stream": true, "messages": []}).to_string();
        let headers = vec![
            ("authorization".to_string(), "Bearer thread-token".to_string()),
            ("anthropic-beta".to_string(), "claude-code-20250219".to_string()),
        ];
        let (head, body) = http(connection, thread, Agent::Claude, "/v1/messages", headers, body).await;
        let GatewayReply::HttpHead { status, .. } = head else { panic!("a response, not {head:?}") };
        (status, body)
    }

    fn account(&self, state: &GatewayState, id: &str) -> GatewayAccount {
        state.accounts.iter().find(|account| account.id == id).cloned().unwrap()
    }

    /// The organisation whose token the fake saw last on Messages.
    fn last_org(&self) -> String {
        let seen = self.fake.seen("/v1/messages");
        bearer(&seen.last().unwrap().headers).split('-').nth(1).unwrap().to_string()
    }
}

/// A connection to the gateway, with the endpoint it needs to stay up.
struct Link {
    connection: Connection,
    _endpoint: Endpoint,
}

impl std::ops::Deref for Link {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        &self.connection
    }
}

async fn ask(connection: &Connection, request: &GatewayRequest) -> GatewayReply {
    let (mut send, mut recv) = connection.open_bi().await.unwrap();
    write_frame(&mut send, request).await.unwrap();
    send.finish().unwrap();
    tokio::time::timeout(TIMEOUT, read_frame(&mut recv)).await.unwrap().unwrap().unwrap()
}

async fn http(
    connection: &Connection,
    thread: &str,
    agent: Agent,
    path: &str,
    headers: Vec<(String, String)>,
    body: String,
) -> (GatewayReply, String) {
    http_pinned(connection, thread, agent, path, headers, body, None).await
}

async fn http_pinned(
    connection: &Connection,
    thread: &str,
    agent: Agent,
    path: &str,
    headers: Vec<(String, String)>,
    body: String,
    pin: Option<&str>,
) -> (GatewayReply, String) {
    let (mut send, mut recv) = connection.open_bi().await.unwrap();
    let request = GatewayRequest::Http {
        thread: thread.into(),
        agent,
        method: "POST".into(),
        path: path.into(),
        headers,
        pin: pin.map(String::from),
    };
    write_frame(&mut send, &request).await.unwrap();
    send.write_all(body.as_bytes()).await.unwrap();
    send.finish().unwrap();
    let head: GatewayReply = tokio::time::timeout(TIMEOUT, read_frame(&mut recv)).await.unwrap().unwrap().unwrap();
    let body = tokio::time::timeout(TIMEOUT, recv.read_to_end(1 << 24)).await.unwrap().unwrap_or_default();
    (head, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn two_seats_of_one_team_are_two_accounts() {
    let harness = Harness::start().await;
    harness.sign_in_claude("ann.team").await;
    let state = harness.sign_in_claude("bob.team").await;
    let mut seats: Vec<_> =
        state.accounts.iter().map(|account| (account.id.as_str(), account.email.as_deref().unwrap())).collect();
    seats.sort();
    assert_eq!(seats, [("ann/team", "ann.team@example.com"), ("bob/team", "bob.team@example.com")]);
}

#[tokio::test]
async fn a_claude_login_signs_in_and_a_threads_request_streams_through_with_its_token() {
    let harness = Harness::start().await;
    let state = harness.sign_in_claude("alpha").await;
    let account = harness.account(&state, "alpha");
    assert_eq!((account.email.as_deref(), account.plan.as_deref()), (Some("alpha@example.com"), Some("Max 20x")));
    assert_eq!((account.status, account.next), (AccountStatus::Ready, true));

    harness.fake.hold.store(true, Ordering::SeqCst);
    let server = harness.server().await;
    let (mut send, mut recv) = server.open_bi().await.unwrap();
    let headers =
        vec![("x-api-key".into(), "sk-thread".into()), ("anthropic-beta".into(), "claude-code-20250219".into())];
    let request = GatewayRequest::Http {
        thread: "t1".into(),
        agent: Agent::Claude,
        method: "POST".into(),
        path: "/v1/messages".into(),
        headers,
        pin: None,
    };
    write_frame(&mut send, &request).await.unwrap();
    send.write_all(json!({"model": "claude-opus-4-5", "stream": true}).to_string().as_bytes()).await.unwrap();
    send.finish().unwrap();
    let head: GatewayReply = read_frame(&mut recv).await.unwrap().unwrap();
    let GatewayReply::HttpHead { status, headers, account } = head else { panic!("a response") };
    assert_eq!((status, account.as_deref()), (200, Some("alpha")), "the head names the account that answered");
    assert!(headers.iter().any(|(name, value)| name == "content-type" && value == "text/event-stream"));

    let mut first = vec![0u8; 64];
    let read = tokio::time::timeout(TIMEOUT, recv.read(&mut first)).await.unwrap().unwrap().unwrap();
    assert!(String::from_utf8_lossy(&first[..read]).starts_with("event: message_start"), "streams before it ends");
    harness.fake.release.notify_one();
    let rest = tokio::time::timeout(TIMEOUT, recv.read_to_end(1 << 20)).await.unwrap().unwrap();
    assert!(String::from_utf8_lossy(&rest).contains("message_delta"));

    let seen = &harness.fake.seen("/v1/messages")[0];
    assert_eq!(seen.header("authorization"), "Bearer claude-alpha-0");
    assert_eq!(seen.header("x-api-key"), "");
    assert_eq!(seen.header("anthropic-beta"), "claude-code-20250219,oauth-2025-04-20");

    let entries = eventually_log(&harness, 1).await;
    let entry = &entries[0];
    assert_eq!((entry.thread.as_str(), entry.account.as_deref(), entry.status), ("t1", Some("alpha"), 200));
    assert_eq!(
        (entry.input_tokens, entry.output_tokens, entry.cache_read_tokens, entry.cache_write_tokens),
        (10, 50, 1000, 100)
    );
    assert_eq!(entry.server, harness.server_key.public());
    let state = harness.state().await;
    assert_eq!(state.log_head, Some(entry.id));
    let windows = harness.account(&state, "alpha").windows;
    let used: Vec<_> = windows.iter().map(|window| (window.label.as_str(), window.used_percent)).collect();
    assert_eq!(used, [("Session", 25.0), ("Weekly", 50.0)]);

    let GatewayReply::Usage { rows } = harness.manage(Manage::Usage { since: 0.0 }).await else { panic!("usage") };
    assert_eq!((rows[0].account.as_str(), rows[0].model.as_str(), rows[0].requests), ("alpha", "claude-opus-4-5", 1));
    let GatewayReply::Pool { limits } = ask(&server, &GatewayRequest::Pool { agent: Agent::Claude }).await else {
        panic!()
    };
    let limits = limits.unwrap();
    assert_eq!((limits.account_name.as_str(), limits.plan.as_deref()), ("Gateway", Some("Max 20x")));
}

async fn eventually_log(harness: &Harness, count: usize) -> Vec<motile_protocol::gateway_wire::LogEntry> {
    for _ in 0..100 {
        let GatewayReply::Log { entries } =
            harness.manage(Manage::Log { before: None, errors_only: false, limit: 50 }).await
        else {
            panic!("a log")
        };
        if entries.len() >= count {
            return entries;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the log never had {count} entries");
}

#[tokio::test]
async fn tokens_about_to_expire_are_renewed_and_a_refused_renewal_asks_to_sign_in_again() {
    let harness = Harness::start().await;
    harness.fake.lifetime.store(60, Ordering::SeqCst);
    harness.sign_in_claude("alpha").await;
    let server = harness.server().await;

    harness.fake.lifetime.store(8 * 3600, Ordering::SeqCst);
    assert_eq!(harness.claude(&server, "t1").await.0, 200);
    let renewals = harness.fake.renewals.load(Ordering::SeqCst);
    assert!(renewals >= 1, "renewed before it went out");
    let seen = harness.fake.seen("/v1/messages");
    assert_eq!(seen.last().unwrap().header("authorization"), format!("Bearer claude-alpha-{renewals}"));

    harness.fake.refuse_renewal.store(true, Ordering::SeqCst);
    let access = harness.gateway.account("alpha").unwrap().tokens.access;
    assert_eq!(harness.gateway.renew("alpha", &access).await, None);
    assert_eq!(harness.account(&harness.state().await, "alpha").status, AccountStatus::SignInAgain);

    harness.fake.refuse_renewal.store(false, Ordering::SeqCst);
    harness.fake.lifetime.store(8 * 3600, Ordering::SeqCst);
    let state = harness.sign_in_claude("alpha").await;
    assert_eq!(state.accounts.len(), 1, "the same organisation signs in again in place");
    assert_eq!(harness.account(&state, "alpha").status, AccountStatus::Ready);
}

#[tokio::test]
async fn a_thread_stays_on_its_account_while_new_threads_go_where_the_rank_says() {
    let harness = Harness::start().await;
    harness.sign_in_claude("alpha").await;
    harness.sign_in_claude("beta").await;
    let server = harness.server().await;

    harness.claude(&server, "t1").await;
    let first = harness.last_org();
    let other = if first == "alpha" { "beta" } else { "alpha" };
    let lower = Manage::UpdateAccount { id: first.clone(), change: AccountChange::Priority { priority: 1 } };
    harness.manage(lower).await;

    for _ in 0..3 {
        harness.claude(&server, "t1").await;
        assert_eq!(harness.last_org(), first, "the thread stays with its cache");
    }
    harness.claude(&server, "t2").await;
    assert_eq!(harness.last_org(), other, "a new thread starts on the higher tier");

    let off = Manage::UpdateAccount { id: first.clone(), change: AccountChange::Enabled { enabled: false } };
    harness.manage(off).await;
    harness.claude(&server, "t1").await;
    assert_eq!(harness.last_org(), other, "an account turned off loses its threads at their next request");
}

#[tokio::test]
async fn a_limit_before_anything_streamed_rests_the_account_and_the_next_one_answers() {
    let harness = Harness::start().await;
    harness.sign_in_claude("alpha").await;
    harness.sign_in_claude("beta").await;
    let server = harness.server().await;
    harness.claude(&server, "t1").await;
    let first = harness.last_org();
    let other = if first == "alpha" { "beta" } else { "alpha" };

    harness.fake.limited.lock().unwrap().insert(format!("claude-{first}-0"));
    let (status, body) = harness.claude(&server, "t1").await;
    assert_eq!(status, 200);
    assert!(body.contains("message_delta"));
    assert_eq!(harness.last_org(), other);
    let state = harness.state().await;
    let AccountStatus::Resting { until: Some(until) } = harness.account(&state, &first).status else {
        panic!("it rests")
    };
    assert!(until > motile_protocol::now() + 3000.0);
    let entries = eventually_log(&harness, 2).await;
    assert_eq!((entries[0].retries, entries[0].account.as_deref()), (1, Some(other)));

    harness.fake.limited.lock().unwrap().insert(format!("claude-{other}-0"));
    let (status, body) = harness.claude(&server, "t1").await;
    assert_eq!(status, 429, "with every account spent the agent hears the limit");
    assert!(body.contains("rate_limit_error"));
}

#[tokio::test]
async fn a_pinned_thread_goes_only_to_its_account_and_hears_its_limit() {
    let harness = Harness::start().await;
    harness.sign_in_claude("alpha").await;
    harness.sign_in_claude("beta").await;
    let server = harness.server().await;
    let GatewayReply::Accounts { accounts } = ask(&server, &GatewayRequest::Accounts).await else { panic!("accounts") };
    let mut ids: Vec<_> = accounts.iter().map(|account| account.id.as_str()).collect();
    ids.sort();
    assert_eq!(ids, ["alpha", "beta"], "a server sees the accounts it can pin to");
    harness.claude(&server, "free").await;
    let first = harness.last_org();
    let other = if first == "alpha" { "beta" } else { "alpha" };
    let pinned = |thread: &'static str| {
        let server = &server;
        async move {
            let headers = vec![("authorization".to_string(), "Bearer thread-token".to_string())];
            let body = json!({"model": "claude-opus-4-5", "stream": true, "messages": []}).to_string();
            http_pinned(server, thread, Agent::Claude, "/v1/messages", headers, body, Some(other)).await
        }
    };

    for thread in ["p1", "p2"] {
        let (head, _) = pinned(thread).await;
        assert!(matches!(head, GatewayReply::HttpHead { status: 200, .. }), "{head:?}");
        assert_eq!(harness.last_org(), other, "a pinned thread goes to its account");
    }

    harness.fake.limited.lock().unwrap().insert(format!("claude-{other}-0"));
    let (head, _) = pinned("p1").await;
    assert!(matches!(head, GatewayReply::HttpHead { status: 429, .. }), "{head:?}");
    assert_eq!(harness.last_org(), other, "it never moves to another account");
    let (head, body) = pinned("p1").await;
    let GatewayReply::HttpHead { status: 429, headers, .. } = head else { panic!("a limit error, got {head:?}") };
    assert!(body.contains("is at its limit"), "{body}");
    let header = |name: &str| headers.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str());
    assert!(header("anthropic-ratelimit-unified-reset").is_some());
    assert!(
        header("anthropic-ratelimit-unified-representative-claim").is_some(),
        "Claude Code only reads the reset beside the window that ran out"
    );

    let (status, _) = harness.claude(&server, "free").await;
    assert_eq!((status, harness.last_org()), (200, first), "a thread that isn't pinned goes on");
}

#[tokio::test]
async fn a_response_that_breaks_off_after_it_streamed_is_not_replayed() {
    let harness = Harness::start().await;
    harness.sign_in_claude("alpha").await;
    harness.sign_in_claude("beta").await;
    let server = harness.server().await;

    harness.fake.break_off.store(true, Ordering::SeqCst);
    let (status, body) = harness.claude(&server, "t1").await;
    assert_eq!(status, 200);
    assert!(body.contains("message_start") && !body.contains("message_delta"));
    assert_eq!(harness.fake.seen("/v1/messages").len(), 1);
    let entries = eventually_log(&harness, 1).await;
    assert!(entries[0].error.as_deref().is_some_and(|error| error.contains("broke off")));
}

#[tokio::test]
async fn a_codex_account_signs_in_with_a_device_code_and_its_websocket_is_bridged() {
    let harness = Harness::start().await;
    let GatewayReply::SignInStarted { sign_in } = harness.manage(Manage::SignIn { agent: Agent::Codex }).await else {
        panic!("a sign-in")
    };
    assert_eq!(sign_in.user_code.as_deref(), Some("ABCD-1234"));
    assert!(sign_in.url.ends_with("/codex/device"));
    let GatewayReply::State { state } = harness.manage(Manage::FinishSignIn { id: sign_in.id, code: None }).await
    else {
        panic!("a state")
    };
    let account = harness.account(&state, "acct-1");
    assert_eq!(
        (account.agent, account.plan.as_deref(), account.email.as_deref()),
        (Agent::Codex, Some("Pro"), Some("c@example.com"))
    );

    let server = harness.server().await;
    let (mut send, mut recv) = server.open_bi().await.unwrap();
    let headers = vec![
        ("authorization".into(), "Bearer thread-token".into()),
        ("openai-beta".into(), "responses_websockets=2026-02-06".into()),
    ];
    let open = GatewayRequest::WebSocket {
        thread: "c1".into(),
        agent: Agent::Codex,
        path: "/responses".into(),
        headers,
        pin: None,
    };
    write_frame(&mut send, &open).await.unwrap();
    let reply: GatewayReply = tokio::time::timeout(TIMEOUT, read_frame(&mut recv)).await.unwrap().unwrap().unwrap();
    assert!(matches!(reply, GatewayReply::WebSocketOpen { .. }), "{reply:?}");
    let handshake = &harness.fake.seen("/backend-api/codex/responses (ws)")[0];
    assert_eq!(handshake.header("authorization"), format!("Bearer {}", jwt_access(&harness)));
    assert_eq!(handshake.header("chatgpt-account-id"), "acct-1");
    assert_eq!(handshake.header("openai-beta"), "responses_websockets=2026-02-06");

    let create = json!({"type": "response.create", "model": "gpt-5", "input": [{"type": "message", "role": "user"}]});
    write_frame(&mut send, &WsMessage::Text { text: create.to_string() }).await.unwrap();
    let mut kinds = Vec::new();
    while kinds.last().map(String::as_str) != Some("response.completed") {
        let message: WsMessage = tokio::time::timeout(TIMEOUT, read_frame(&mut recv)).await.unwrap().unwrap().unwrap();
        let WsMessage::Text { text } = message else { panic!("text") };
        kinds.push(serde_json::from_str::<Value>(&text).unwrap()["type"].as_str().unwrap().to_string());
    }
    assert_eq!(kinds, ["codex.rate_limits", "response.completed"]);
    write_frame(&mut send, &WsMessage::Close { code: 1000, reason: String::new() }).await.unwrap();

    let entries = eventually_log(&harness, 1).await;
    let entry = &entries[0];
    assert_eq!(
        (entry.thread.as_str(), entry.account.as_deref(), entry.model.as_deref()),
        ("c1", Some("acct-1"), Some("gpt-5"))
    );
    assert_eq!((entry.input_tokens, entry.cache_read_tokens, entry.output_tokens), (200, 100, 30));
    let windows = harness.account(&harness.state().await, "acct-1").windows;
    assert_eq!((windows[0].label.as_str(), windows[0].used_percent), ("Session", 7.0));
}

#[tokio::test]
async fn codex_images_reuse_the_threads_login_without_forwarding_its_local_credentials() {
    let harness = Harness::start().await;
    harness.sign_in_codex().await;
    let server = harness.server().await;
    let account = harness.codex(&server, "image-thread", json!([])).await;
    for path in ["/images/generations", "/images/edits"] {
        let body = json!({"model": "gpt-image-2", "prompt": "A neon glass whale"}).to_string();
        let headers = vec![
            ("authorization".into(), "Bearer thread-token".into()),
            ("x-openai-actor-authorization".into(), "Bearer thread-token".into()),
        ];
        let (head, response) = http(&server, "image-thread", Agent::Codex, path, headers, body.clone()).await;
        assert!(matches!(head, GatewayReply::HttpHead { status: 200, .. }), "{head:?}");
        assert_eq!(serde_json::from_str::<Value>(&response).unwrap()["data"][0]["b64_json"], "aW1hZ2U=");
        let seen = harness.fake.seen(&format!("/backend-api/codex{path}"));
        let seen = seen.last().unwrap();
        assert_eq!(seen.body, body);
        assert_eq!(seen.header("authorization"), format!("Bearer {}", jwt_access(&harness)));
        assert_eq!(seen.header("chatgpt-account-id"), account);
        assert_eq!(seen.header("x-openai-actor-authorization"), "");
    }
}

fn jwt_access(harness: &Harness) -> String {
    harness.gateway.account("acct-1").unwrap().tokens.access
}

#[tokio::test]
async fn only_the_accounts_servers_send_traffic_and_only_its_clients_manage() {
    let harness = Harness::start().await;
    let stranger = harness.connect(&DeviceKey::generate()).await;
    let refused = match stranger {
        Err(_) => true,
        Ok(connection) => match connection.open_bi().await {
            Err(_) => true,
            Ok((mut send, mut recv)) => {
                let _ = write_frame(&mut send, &GatewayRequest::Manage { command: Manage::State }).await;
                read_frame::<_, GatewayReply>(&mut recv).await.ok().flatten().is_none()
            }
        },
    };
    assert!(refused, "a device of another account gets nothing");

    let server = harness.server().await;
    let reply = ask(&server, &GatewayRequest::Manage { command: Manage::State }).await;
    assert!(matches!(reply, GatewayReply::Error { .. }), "a server can't manage the gateway");
    let client = harness.client().await;
    let reply = ask(&client, &GatewayRequest::Pool { agent: Agent::Claude }).await;
    assert!(matches!(reply, GatewayReply::Error { .. }), "a client sends no traffic");
}

#[tokio::test]
async fn a_new_account_shows_its_limits_at_once() {
    let harness = Harness::start().await;
    let state = harness.sign_in_claude("alpha").await;
    let windows = harness.account(&state, "alpha").windows;
    assert_eq!((windows[0].label.as_str(), windows[0].used_percent), ("Session", 10.0));
    assert!(harness.fake.polls.load(Ordering::SeqCst) > 0);
}

#[tokio::test]
async fn a_codex_thread_that_moves_leaves_the_reasoning_its_old_account_encrypted_behind() {
    let harness = Harness::start().await;
    harness.sign_in_codex().await;
    harness.sign_in_codex().await;
    let server = harness.server().await;
    let user = |text: &str| json!({"type": "message", "role": "user", "content": text});
    let reasoning = |from: &str| json!({"type": "reasoning", "summary": [], "encrypted_content": from});

    let first = harness.codex(&server, "c1", json!([user("hi")])).await;
    let access = harness.gateway.account(&first).unwrap().tokens.access;
    harness.fake.limited.lock().unwrap().insert(access);

    let input = json!([user("hi"), reasoning(&first), user("more")]);
    let second = harness.codex(&server, "c1", input).await;
    assert_ne!(second, first, "it failed over");
    let sent: Value =
        serde_json::from_str(&harness.fake.seen("/backend-api/codex/responses").last().unwrap().body).unwrap();
    assert_eq!(sent["input"].as_array().unwrap().len(), 2, "the reasoning from {first} stayed behind");

    let input = json!([user("hi"), reasoning(&first), user("more"), reasoning(&second), user("again")]);
    assert_eq!(harness.codex(&server, "c1", input).await, second);
    let sent: Value =
        serde_json::from_str(&harness.fake.seen("/backend-api/codex/responses").last().unwrap().body).unwrap();
    let kept: Vec<_> =
        sent["input"].as_array().unwrap().iter().filter_map(|item| item["encrypted_content"].as_str()).collect();
    assert_eq!(kept, [second.as_str()], "its new account's own reasoning goes along");
}

/// An agent's MCP message to the MCP server `slug`, as a server sends it.
async fn mcp(connection: &Connection, slug: &str, message: Value) -> (u16, Value) {
    let (status, _, answer) = mcp_call(connection, slug, message, &[]).await;
    (status, answer)
}

/// The answer's headers and its message, to a request with `extra` headers.
async fn mcp_with(
    connection: &Connection,
    slug: &str,
    message: Value,
    extra: &[(&str, &str)],
) -> (Vec<(String, String)>, Value) {
    let (_, headers, answer) = mcp_call(connection, slug, message, extra).await;
    (headers, answer)
}

async fn mcp_call(
    connection: &Connection,
    slug: &str,
    message: Value,
    extra: &[(&str, &str)],
) -> (u16, Vec<(String, String)>, Value) {
    let (mut send, mut recv) = connection.open_bi().await.unwrap();
    let mut headers = vec![
        ("authorization".to_string(), "Bearer thread-token".to_string()),
        ("accept".to_string(), "application/json, text/event-stream".to_string()),
        ("content-type".to_string(), "application/json".to_string()),
    ];
    headers.extend(extra.iter().map(|(name, value)| (name.to_string(), value.to_string())));
    let request = GatewayRequest::Mcp { thread: "t1".into(), mcp_server: slug.into(), method: "POST".into(), headers };
    write_frame(&mut send, &request).await.unwrap();
    send.write_all(message.to_string().as_bytes()).await.unwrap();
    send.finish().unwrap();
    let head: GatewayReply = tokio::time::timeout(TIMEOUT, read_frame(&mut recv)).await.unwrap().unwrap().unwrap();
    let GatewayReply::HttpHead { status, headers, .. } = head else { panic!("an answer, not {head:?}") };
    let body = tokio::time::timeout(TIMEOUT, recv.read_to_end(1 << 24)).await.unwrap().unwrap_or_default();
    let body = String::from_utf8_lossy(&body).into_owned();
    let data = body.lines().filter_map(|line| line.strip_prefix("data: ")).collect::<Vec<_>>().join("\n");
    let answer = serde_json::from_str(if data.is_empty() { &body } else { &data }).unwrap_or(Value::String(body));
    (status, headers, answer)
}

fn call(id: u64, tool: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": tool, "arguments": {"title": "Hi"}}})
}

fn tool_names(answer: &Value) -> Vec<&str> {
    answer["result"]["tools"].as_array().unwrap().iter().map(|tool| tool["name"].as_str().unwrap()).collect()
}

#[tokio::test]
async fn a_mcp_server_with_a_key_gives_agents_only_the_tools_that_are_on() {
    let harness = Harness::start().await;
    let base = harness.fake.base.lock().unwrap().clone();
    let url = format!("{base}/mcp/key");
    let add = |key| Manage::AddMcpServer { url: url.clone(), name: None, key };
    let reply = harness.manage(add(None)).await;
    assert_eq!(reply, GatewayReply::McpAdding { adding: McpAdding::NeedsKey });
    let key = McpKey { header: "x-api-key".into(), value: "k1".into() };
    let GatewayReply::McpAdding { adding: McpAdding::Added { id } } = harness.manage(add(Some(key))).await else {
        panic!("added with its key")
    };
    let mcp_servers = harness.state().await.mcp_servers.unwrap();
    let mcp_server = &mcp_servers[0];
    assert_eq!(
        (mcp_server.id.as_str(), mcp_server.name.as_str(), mcp_server.slug.as_str()),
        (id.as_str(), "Fake Tracker", "Fake_Tracker")
    );
    assert_eq!(mcp_server.auth, McpAuth::Key);
    let writes: Vec<(&str, bool)> = mcp_server.tools.iter().map(|tool| (tool.name.as_str(), tool.writes)).collect();
    assert_eq!(writes, [("list_issues", false), ("create_issue", true)]);

    let server = harness.server().await;
    let GatewayReply::McpServers { mcp_servers } = ask(&server, &GatewayRequest::McpServers).await else {
        panic!("slugs")
    };
    assert_eq!(mcp_servers, ["Fake_Tracker"]);
    let list = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
    assert_eq!(tool_names(&mcp(&server, "Fake_Tracker", list.clone()).await.1), ["list_issues", "create_issue"]);
    let seen = harness.fake.seen("/mcp/key");
    assert_eq!(seen.last().unwrap().header("x-api-key"), "k1");
    assert_eq!(seen.last().unwrap().header("authorization"), "", "the agent's token stays on the gateway");

    let (head, _) = mcp_with(&server, "Fake_Tracker", call(7, "create_issue"), &[]).await;
    assert!(head.iter().any(|(name, _)| name == MCP_NEEDS_APPROVAL), "a tool that may write asks to start with");

    let block = McpChange::GroupPermission { kind: ToolKind::Other, permission: ToolPermission::Block };
    harness.manage(Manage::UpdateMcpServer { id: id.clone(), change: block }).await;
    assert_eq!(tool_names(&mcp(&server, "Fake_Tracker", list).await.1), ["list_issues"]);
    let calls_before = harness.fake.seen("/mcp/key").len();
    let (status, refused) = mcp(&server, "Fake_Tracker", call(2, "create_issue")).await;
    assert_eq!(status, 200);
    assert_eq!(refused["result"]["isError"], true);
    assert_eq!(harness.fake.seen("/mcp/key").len(), calls_before, "a blocked tool never reaches the MCP server");

    let asks = McpChange::ToolPermission { name: "create_issue".into(), permission: ToolPermission::Ask };
    harness.manage(Manage::UpdateMcpServer { id: id.clone(), change: asks }).await;
    let (head, waits) = mcp_with(&server, "Fake_Tracker", call(5, "create_issue"), &[]).await;
    assert!(head.iter().any(|(name, _)| name == MCP_NEEDS_APPROVAL), "a tool that asks waits for the user");
    assert_eq!(waits["result"]["isError"], true);
    assert_eq!(harness.fake.seen("/mcp/key").len(), calls_before, "nor does one the user hasn't approved");
    let (_, approved) = mcp_with(&server, "Fake_Tracker", call(6, "create_issue"), &[(MCP_APPROVED, "1")]).await;
    assert_eq!(approved["result"]["content"][0]["text"], r#"called "create_issue" with {"title":"Hi"}"#);
    assert_eq!(
        harness.fake.seen("/mcp/key").last().unwrap().header(MCP_APPROVED),
        "",
        "the approval stays on the gateway"
    );
    let (_, called) = mcp(&server, "Fake_Tracker", call(3, "list_issues")).await;
    assert_eq!(called["result"]["content"][0]["text"], r#"called "list_issues" with {"title":"Hi"}"#);

    let GatewayReply::McpCalls { calls } =
        harness.manage(Manage::McpCalls { before: None, errors_only: false, limit: 10 }).await
    else {
        panic!("calls")
    };
    let logged: Vec<(&str, bool)> = calls.iter().map(|call| (call.tool.as_str(), call.error.is_some())).collect();
    assert_eq!(logged, [("list_issues", false), ("create_issue", false), ("create_issue", true)]);
    assert_eq!((calls[0].thread.as_deref(), calls[0].mcp_server.as_str()), (Some("t1"), id.as_str()));
    let GatewayReply::McpCall { call: Some(detail) } = harness.manage(Manage::McpCall { id: calls[0].id }).await else {
        panic!("a call")
    };
    assert_eq!(detail.arguments, r#"{"title":"Hi"}"#);

    let rename = McpChange::Name { name: "Unbind Banano".into() };
    harness.manage(Manage::UpdateMcpServer { id: id.clone(), change: rename }).await;
    let GatewayReply::McpServers { mcp_servers } = ask(&server, &GatewayRequest::McpServers).await else {
        panic!("slugs")
    };
    assert_eq!(mcp_servers, ["Unbind_Banano"], "agents call it by its new name");

    harness.manage(Manage::UpdateMcpServer { id: id.clone(), change: McpChange::Enabled { enabled: false } }).await;
    let GatewayReply::McpServers { mcp_servers } = ask(&server, &GatewayRequest::McpServers).await else {
        panic!("slugs")
    };
    assert!(mcp_servers.is_empty(), "an MCP server that is off isn't given to agents");
    let (_, off) = mcp(&server, "Unbind_Banano", call(4, "list_issues")).await;
    assert_eq!(off["result"]["isError"], true);
}

#[tokio::test]
async fn a_mcp_server_signs_in_once_and_its_login_is_renewed_when_refused() {
    let harness = Harness::start().await;
    let base = harness.fake.base.lock().unwrap().clone();
    let url = format!("{base}/mcp/oauth");
    let reply =
        harness.manage(Manage::AddMcpServer { url: url.clone(), name: Some("Tracker".into()), key: None }).await;
    let GatewayReply::McpAdding { adding: McpAdding::SignIn { state, url: page } } = reply else {
        panic!("a sign-in, not {reply:?}")
    };
    let page = reqwest::Url::parse(&page).unwrap();
    assert!(page.as_str().starts_with(&format!("{base}/as/authorize?")));
    let query: Vec<(String, String)> = page.query_pairs().into_owned().collect();
    let param = |name: &str| query.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str());
    assert_eq!(param("redirect_uri"), Some(format!("{base}/mcp/callback").as_str()));
    assert_eq!(param("client_id"), Some("client-1"));
    assert_eq!(param("code_challenge_method"), Some("S256"));
    assert_eq!(param("resource"), Some(url.as_str()));
    assert_eq!(param("state"), Some(state.as_str()));

    let wrong = harness.manage(Manage::FinishMcpSignIn { state: state.clone(), code: "bad".into() }).await;
    assert!(matches!(wrong, GatewayReply::Error { .. }));
    let reply = harness.manage(Manage::AddMcpServer { url, name: Some("Tracker".into()), key: None }).await;
    let GatewayReply::McpAdding { adding: McpAdding::SignIn { state, .. } } = reply else { panic!("again") };
    let finished = harness.manage(Manage::FinishMcpSignIn { state, code: "good-code".into() }).await;
    let GatewayReply::McpAdding { adding: McpAdding::Added { id } } = finished else {
        panic!("added, not {finished:?}")
    };
    let mcp_server = harness.state().await.mcp_servers.unwrap().remove(0);
    assert_eq!((mcp_server.slug.as_str(), mcp_server.auth, mcp_server.tools.len()), ("Tracker", McpAuth::SignIn, 2));

    let server = harness.server().await;
    harness.fake.mcp_generation.store(1, Ordering::SeqCst);
    let (_, called) = mcp(&server, "Tracker", call(1, "list_issues")).await;
    assert!(called["result"]["content"][0]["text"].as_str().unwrap().starts_with("called"), "{called}");
    assert_eq!(harness.fake.mcp_renewals.load(Ordering::SeqCst), 1, "renewed once it was refused");

    harness.fake.mcp_refuse_renewal.store(true, Ordering::SeqCst);
    harness.fake.mcp_generation.store(2, Ordering::SeqCst);
    let (_, refused) = mcp(&server, "Tracker", call(2, "list_issues")).await;
    assert!(refused["result"]["content"][0]["text"].as_str().unwrap().contains("Sign in again"), "{refused}");
    let mcp_server = harness.state().await.mcp_servers.unwrap().remove(0);
    assert_eq!(mcp_server.status, McpStatus::SignInAgain);

    let GatewayReply::McpAdding { adding: McpAdding::SignIn { state, .. } } =
        harness.manage(Manage::SignInMcpServer { id: id.clone() }).await
    else {
        panic!("signs in again")
    };
    harness.fake.mcp_refuse_renewal.store(false, Ordering::SeqCst);
    harness.manage(Manage::FinishMcpSignIn { state, code: "good-code".into() }).await;
    let mcp_servers = harness.state().await.mcp_servers.unwrap();
    assert_eq!((mcp_servers.len(), &mcp_servers[0].status), (1, &McpStatus::Ready), "signed in again in place");
    let GatewayReply::ToolResult { result } =
        harness.manage(Manage::TryTool { id, tool: "list_issues".into(), arguments: json!({}) }).await
    else {
        panic!("a result")
    };
    assert_eq!(result.error, None);
    assert_eq!(result.result["content"][0]["text"], r#"called "list_issues" with {}"#);
}
