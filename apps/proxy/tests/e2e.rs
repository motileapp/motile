//! The proxy over real iroh connections on this machine, with a fake standing in for Anthropic,
//! OpenAI and their sign-ins.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::ws::{Message as WsFrame, WebSocket, WebSocketUpgrade};
use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use iroh::endpoint::{Connection, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey};
use motile_protocol::PROXY_ALPN;
use motile_protocol::frame::{read_frame, write_frame};
use motile_protocol::identity::DeviceKey;
use motile_protocol::proxy_wire::{
    AccountChange, AccountStatus, Manage, ProxyAccount, ProxyReply, ProxyRequest, ProxyState, WsMessage,
};
use motile_protocol::wire::Agent;
use motile_proxy::access::Access;
use motile_proxy::config::Upstreams;
use motile_proxy::pricing::Prices;
use motile_proxy::serve::BindOptions;
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
    Json(json!({
        "access_token": format!("claude-{org}-{generation}"),
        "refresh_token": format!("refresh-{org}"),
        "expires_in": lifetime,
        "account": {"uuid": "account", "email_address": format!("{org}@example.com")},
        "organization": {"uuid": org, "name": "Org"},
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

async fn serve_fake(fake: Arc<Fake>) -> String {
    let app = Router::new()
        .route("/v1/oauth/token", post(claude_token))
        .route("/api/oauth/profile", get(claude_profile))
        .route("/api/oauth/usage", get(claude_usage))
        .route("/v1/messages", post(messages))
        .route("/api/accounts/deviceauth/usercode", post(device_code))
        .route("/api/accounts/deviceauth/token", post(device_token))
        .route("/oauth/token", post(openai_token))
        .route("/backend-api/codex/responses", get(codex_socket).post(codex_http))
        .route("/backend-api/wham/usage", get(codex_usage))
        .with_state(fake);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

struct Harness {
    _dir: tempfile::TempDir,
    fake: Arc<Fake>,
    proxy_key: String,
    proxy_address: SocketAddr,
    server_key: DeviceKey,
    client_key: DeviceKey,
    proxy: Arc<motile_proxy::proxy::Proxy>,
    _endpoint: Endpoint,
}

impl Harness {
    async fn start() -> Self {
        let fake = Arc::new(Fake { lifetime: AtomicU64::new(8 * 3600), ..Fake::default() });
        let base = serve_fake(fake.clone()).await;
        let dir = tempfile::tempdir().unwrap();
        let key = DeviceKey::generate();
        let proxy_key = key.public();
        let server_key = DeviceKey::generate();
        let client_key = DeviceKey::generate();
        let prices = Prices::from_list(&json!({
            "claude-opus-4-5": {"litellm_provider": "anthropic", "input_cost_per_token": 5e-6, "output_cost_per_token": 2.5e-5},
            "gpt-5": {"litellm_provider": "openai", "input_cost_per_token": 1.25e-6, "output_cost_per_token": 1e-5},
        }));
        let options = motile_proxy::Options {
            data_dir: dir.path().to_path_buf(),
            key,
            access: Access::fixed(&[server_key.public()], &[client_key.public()]),
            upstreams: Upstreams::at(&base),
            bind: BindOptions { local_only: true, port: None },
            prices: Some(prices),
        };
        let (proxy, endpoint) = motile_proxy::start(options).await.unwrap();
        let port = endpoint.bound_sockets().iter().find(|address| address.is_ipv4()).unwrap().port();
        let proxy_address = SocketAddr::from(([127, 0, 0, 1], port));
        Self { _dir: dir, fake, proxy_key, proxy_address, server_key, client_key, proxy, _endpoint: endpoint }
    }

    async fn connect(&self, key: &DeviceKey) -> anyhow::Result<Link> {
        let endpoint =
            Endpoint::builder(presets::Minimal).secret_key(SecretKey::from_bytes(&key.to_bytes())).bind().await?;
        let address = EndpointAddr::new(EndpointId::from_str(&self.proxy_key)?).with_ip_addr(self.proxy_address);
        let connection = endpoint.connect(address, PROXY_ALPN).await?;
        Ok(Link { connection, _endpoint: endpoint })
    }

    async fn server(&self) -> Link {
        self.connect(&self.server_key).await.unwrap()
    }

    async fn client(&self) -> Link {
        self.connect(&self.client_key).await.unwrap()
    }

    async fn manage(&self, command: Manage) -> ProxyReply {
        ask(&*self.client().await, &ProxyRequest::Manage { command }).await
    }

    async fn state(&self) -> ProxyState {
        let ProxyReply::State { state } = self.manage(Manage::State).await else { panic!("a state") };
        state
    }

    /// Signs in the Claude organisation `org`, as a client does.
    async fn sign_in_claude(&self, org: &str) -> ProxyState {
        let ProxyReply::SignInStarted { sign_in } = self.manage(Manage::SignIn { agent: Agent::Claude }).await else {
            panic!("a sign-in")
        };
        let url = reqwest::Url::parse(&sign_in.url).unwrap();
        let query: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        let param = |name: &str| query.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone());
        assert_eq!(param("code_challenge_method").as_deref(), Some("S256"));
        assert_eq!(param("redirect_uri").as_deref(), Some("https://platform.claude.com/oauth/code/callback"));
        let state = param("state").unwrap();
        let wrong = Manage::FinishSignIn { id: sign_in.id.clone(), code: Some(format!("wrong#{state}")) };
        let ProxyReply::Error { message } = self.manage(wrong).await else { panic!("a wrong code is refused") };
        assert_eq!(message, "That code didn't work. Paste it again.");
        let finish = Manage::FinishSignIn { id: sign_in.id, code: Some(format!("{org}#{state}")) };
        let ProxyReply::State { state } = self.manage(finish).await else { panic!("the same sign-in takes the code") };
        state
    }

    async fn sign_in_codex(&self) -> ProxyState {
        let ProxyReply::SignInStarted { sign_in } = self.manage(Manage::SignIn { agent: Agent::Codex }).await else {
            panic!("a sign-in")
        };
        let ProxyReply::State { state } = self.manage(Manage::FinishSignIn { id: sign_in.id, code: None }).await else {
            panic!("a state")
        };
        state
    }

    async fn codex(&self, connection: &Connection, thread: &str, input: Value) -> String {
        let body = json!({"model": "gpt-5", "stream": true, "store": false, "input": input}).to_string();
        let (head, _) = http(connection, thread, Agent::Codex, "/responses", vec![], body).await;
        assert!(matches!(head, ProxyReply::HttpHead { status: 200, .. }), "{head:?}");
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
        let ProxyReply::HttpHead { status, .. } = head else { panic!("a response, not {head:?}") };
        (status, body)
    }

    fn account(&self, state: &ProxyState, id: &str) -> ProxyAccount {
        state.accounts.iter().find(|account| account.id == id).cloned().unwrap()
    }

    /// The organisation whose token the fake saw last on Messages.
    fn last_org(&self) -> String {
        let seen = self.fake.seen("/v1/messages");
        bearer(&seen.last().unwrap().headers).split('-').nth(1).unwrap().to_string()
    }
}

/// A connection to the proxy, with the endpoint it needs to stay up.
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

async fn ask(connection: &Connection, request: &ProxyRequest) -> ProxyReply {
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
) -> (ProxyReply, String) {
    let (mut send, mut recv) = connection.open_bi().await.unwrap();
    let request =
        ProxyRequest::Http { thread: thread.into(), agent, method: "POST".into(), path: path.into(), headers };
    write_frame(&mut send, &request).await.unwrap();
    send.write_all(body.as_bytes()).await.unwrap();
    send.finish().unwrap();
    let head: ProxyReply = tokio::time::timeout(TIMEOUT, read_frame(&mut recv)).await.unwrap().unwrap().unwrap();
    let body = tokio::time::timeout(TIMEOUT, recv.read_to_end(1 << 24)).await.unwrap().unwrap_or_default();
    (head, String::from_utf8_lossy(&body).into_owned())
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
    let request = ProxyRequest::Http {
        thread: "t1".into(),
        agent: Agent::Claude,
        method: "POST".into(),
        path: "/v1/messages".into(),
        headers,
    };
    write_frame(&mut send, &request).await.unwrap();
    send.write_all(json!({"model": "claude-opus-4-5", "stream": true}).to_string().as_bytes()).await.unwrap();
    send.finish().unwrap();
    let head: ProxyReply = read_frame(&mut recv).await.unwrap().unwrap();
    let ProxyReply::HttpHead { status, headers } = head else { panic!("a response") };
    assert_eq!(status, 200);
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

    let ProxyReply::Usage { rows } = harness.manage(Manage::Usage { since: 0.0 }).await else { panic!("usage") };
    assert_eq!((rows[0].account.as_str(), rows[0].model.as_str(), rows[0].requests), ("alpha", "claude-opus-4-5", 1));
    let ProxyReply::Pool { limits } = ask(&server, &ProxyRequest::Pool { agent: Agent::Claude }).await else {
        panic!()
    };
    let limits = limits.unwrap();
    assert_eq!((limits.account_name.as_str(), limits.plan.as_deref()), ("Proxy", Some("Max 20x")));
}

async fn eventually_log(harness: &Harness, count: usize) -> Vec<motile_protocol::proxy_wire::LogEntry> {
    for _ in 0..100 {
        let ProxyReply::Log { entries } =
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
    let access = harness.proxy.account("alpha").unwrap().tokens.access;
    assert_eq!(harness.proxy.renew("alpha", &access).await, None);
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
    let ProxyReply::SignInStarted { sign_in } = harness.manage(Manage::SignIn { agent: Agent::Codex }).await else {
        panic!("a sign-in")
    };
    assert_eq!(sign_in.user_code.as_deref(), Some("ABCD-1234"));
    assert!(sign_in.url.ends_with("/codex/device"));
    let ProxyReply::State { state } = harness.manage(Manage::FinishSignIn { id: sign_in.id, code: None }).await else {
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
    let open = ProxyRequest::WebSocket { thread: "c1".into(), agent: Agent::Codex, path: "/responses".into(), headers };
    write_frame(&mut send, &open).await.unwrap();
    let reply: ProxyReply = tokio::time::timeout(TIMEOUT, read_frame(&mut recv)).await.unwrap().unwrap().unwrap();
    assert!(matches!(reply, ProxyReply::WebSocketOpen { .. }), "{reply:?}");
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

fn jwt_access(harness: &Harness) -> String {
    harness.proxy.account("acct-1").unwrap().tokens.access
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
                let _ = write_frame(&mut send, &ProxyRequest::Manage { command: Manage::State }).await;
                read_frame::<_, ProxyReply>(&mut recv).await.ok().flatten().is_none()
            }
        },
    };
    assert!(refused, "a device of another account gets nothing");

    let server = harness.server().await;
    let reply = ask(&server, &ProxyRequest::Manage { command: Manage::State }).await;
    assert!(matches!(reply, ProxyReply::Error { .. }), "a server can't manage the proxy");
    let client = harness.client().await;
    let reply = ask(&client, &ProxyRequest::Pool { agent: Agent::Claude }).await;
    assert!(matches!(reply, ProxyReply::Error { .. }), "a client sends no traffic");
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
    let access = harness.proxy.account(&first).unwrap().tokens.access;
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
