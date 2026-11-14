//! The account's gateway: the connection to it while a client shows it, and what the Gateway page shows
//! of it: its accounts, the sign-in under way, its request log and how to add one.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use anyhow::{Context, anyhow, bail};
use chrono::{DateTime, FixedOffset};
use iroh::Endpoint;
use motile_protocol::GATEWAY_ALPN;
use motile_protocol::auth_api::Device;
use motile_protocol::gateway_wire::{
    AccountStatus, Capture, CapturedRequest, GatewayAccount, GatewayReply, GatewayRequest, GatewaySignIn, GatewayState,
    LogEntry, Manage,
};
use motile_protocol::wire::Agent;
use serde::Serialize;
use tokio::sync::Notify;
use tokio::task::AbortHandle;

use crate::away::Away;
use crate::connection::{Closed, Connection, ServerAddr, offline};
use crate::limits::{self, Row};
use crate::link::State;
use crate::mcp_servers::{self, Known, McpServerRow};

const RETRY_DELAYS: [Duration; 4] =
    [Duration::from_millis(300), Duration::from_secs(1), Duration::from_secs(2), Duration::from_secs(5)];
const REFUSED_RETRY: Duration = Duration::from_secs(10);
const DIAL_TIMEOUT: Duration = Duration::from_secs(30);
const RECONNECT_WAIT: Duration = Duration::from_secs(15);
/// How often the gateway's state is asked for while a client shows it.
const POLL_EVERY: Duration = Duration::from_secs(3);
/// How many entries of the log a page has.
pub const LOG_PAGE: u32 = 50;
/// How much of a failed response's body the log shows.
const ERROR_EXCERPT: usize = 200;

/// The image the gateway runs from, and where it keeps its accounts.
pub const IMAGE: &str = "ghcr.io/motileapp/motile-gateway:latest";
pub const DATA_DIR: &str = "/data";

const NOT_CONNECTED: &str = "Your gateway isn't connected. Try again when it is back.";
const LOST: &str = "The connection to your gateway was lost. Try again when it is back.";

#[derive(Clone, Debug)]
pub enum GatewayEvent {
    Status(Status),
    State(GatewayState),
}

pub type Emit = Arc<dyn Fn(GatewayEvent) + Send + Sync>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub state: State,
    pub error: Option<String>,
}

/// Keeps a connection to the gateway and asks it for its state every few seconds, dialing again
/// when the connection drops.
pub struct GatewayLink {
    pub key: String,
    /// What the link dials on; none while the core replaces it, when what is on the old one ends
    /// because the core closed it, which says nothing of the server.
    endpoint: Mutex<Option<Endpoint>>,
    /// Whether a connection that ended went because the client was away, which isn't an outage.
    away: Away,
    emit: Emit,
    inner: Mutex<Inner>,
    wake: Notify,
    restart: Notify,
    attached: Notify,
}

#[derive(Default)]
struct Inner {
    status: Status,
    connection: Option<Connection>,
    dialer: Option<AbortHandle>,
    poller: Option<AbortHandle>,
}

impl GatewayLink {
    pub fn connect(endpoint: Endpoint, gateway: ServerAddr, away: Away, emit: Emit) -> Arc<Self> {
        let link = Arc::new(Self {
            key: gateway.key.clone(),
            endpoint: Mutex::new(Some(endpoint)),
            away,
            emit,
            inner: Mutex::default(),
            wake: Notify::new(),
            restart: Notify::new(),
            attached: Notify::new(),
        });
        let dialer = tokio::spawn(link.clone().dial_forever(gateway));
        link.lock().dialer = Some(dialer.abort_handle());
        link
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn update_status(&self, change: impl FnOnce(&mut Status)) {
        let mut inner = self.lock();
        let before = inner.status.clone();
        change(&mut inner.status);
        if inner.status != before {
            (self.emit)(GatewayEvent::Status(inner.status.clone()));
        }
    }

    pub fn shutdown(&self) {
        let dialer = self.lock().dialer.take();
        if let Some(dialer) = dialer {
            dialer.abort();
        }
        self.drop_connection();
    }

    pub fn redial_on(&self, endpoint: Endpoint) {
        *self.endpoint.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(endpoint);
        self.restart.notify_one();
    }

    /// Says why there is no endpoint to dial on, while the link waits for one.
    pub fn no_endpoint(&self, error: String) {
        if self.endpoint.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).is_none() {
            self.update_status(|status| *status = Status { state: State::Disconnected, error: Some(error) });
        }
    }

    /// Lets go of the dial or the connection there is, as the core is about to close the endpoint
    /// they are on, and waits for `redial_on` without saying anything: the link shows as dialing.
    pub fn retire(&self) {
        *self.endpoint.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        self.restart.notify_one();
    }

    pub fn retry_now(&self) {
        self.wake.notify_one();
    }

    fn drop_connection(&self) {
        let mut inner = self.lock();
        if let Some(poller) = inner.poller.take() {
            poller.abort();
        }
        if let Some(connection) = inner.connection.take() {
            connection.close();
        }
    }

    async fn dial_forever(self: Arc<Self>, gateway: ServerAddr) {
        let mut failures = 0;
        loop {
            self.update_status(|status| *status = Status { state: State::Connecting, error: status.error.take() });
            let endpoint = self.endpoint.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
            let Some(endpoint) = endpoint else {
                self.restart.notified().await;
                failures = 0;
                continue;
            };
            let started = SystemTime::now();
            let closed = tokio::select! {
                biased;
                _ = self.restart.notified() => None,
                closed = self.connect_until_closed(&endpoint, &gateway) => Some(closed),
            };
            self.drop_connection();
            let Some(closed) = closed else {
                failures = 0;
                continue;
            };
            let had_connected = self.lock().status.state == State::Connected;
            let delay = match (closed.refused, had_connected) {
                (true, _) => REFUSED_RETRY,
                (false, true) => {
                    failures = 0;
                    RETRY_DELAYS[0]
                }
                (false, false) => {
                    failures += 1;
                    RETRY_DELAYS[(failures - 1).min(RETRY_DELAYS.len() - 1)]
                }
            };
            // As a server's link does: what lived through the client being away dials again quietly.
            let quiet = !closed.refused && self.away.excuses(started);
            self.update_status(|status| {
                *status = match quiet {
                    true => Status { state: State::Connecting, error: None },
                    false => Status {
                        state: if closed.refused { State::Refused } else { State::Disconnected },
                        error: Some(closed.reason).filter(|reason| !reason.is_empty()),
                    },
                }
            });
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = self.wake.notified() => failures = 0,
                _ = self.restart.notified() => failures = 0,
            }
        }
    }

    async fn connect_until_closed(self: &Arc<Self>, endpoint: &Endpoint, gateway: &ServerAddr) -> Closed {
        let dialed =
            tokio::time::timeout(DIAL_TIMEOUT, Connection::dial_on(endpoint, gateway, &[], GATEWAY_ALPN)).await;
        let connection = match dialed {
            Ok(Ok(connection)) => connection,
            Ok(Err(error)) => return Closed { refused: false, reason: format!("{error:#}") },
            Err(_) => return Closed { refused: false, reason: "Your gateway didn't answer in time.".to_string() },
        };
        let poller = tokio::spawn(self.clone().poll(connection.clone()));
        {
            let mut inner = self.lock();
            inner.connection = Some(connection.clone());
            inner.poller = Some(poller.abort_handle());
        }
        self.attached.notify_waiters();
        connection.closed().await
    }

    /// Asks for the state until the connection closes. It counts as connected once the gateway
    /// answers, since a gateway that turns the device away completes the handshake first.
    async fn poll(self: Arc<Self>, connection: Connection) {
        loop {
            match ask(&connection, Manage::State).await {
                Ok(GatewayReply::State { state }) => {
                    (self.emit)(GatewayEvent::State(state));
                    self.update_status(|status| *status = Status { state: State::Connected, error: None });
                }
                Ok(other) => self.update_status(|status| status.error = Some(unexpected(&other))),
                Err(_) if connection.is_closed() => return,
                Err(error) => self.update_status(|status| status.error = Some(motile_protocol::error_text(&error))),
            }
            tokio::time::sleep(POLL_EVERY).await;
        }
    }

    async fn connection(&self) -> anyhow::Result<Connection> {
        let next = async {
            loop {
                let attached = self.attached.notified();
                let connection = self.lock().connection.clone();
                if let Some(connection) = connection.filter(|connection| !connection.is_closed()) {
                    return connection;
                }
                attached.await;
            }
        };
        tokio::time::timeout(RECONNECT_WAIT, next).await.map_err(|_| anyhow!(NOT_CONNECTED))
    }

    /// Sends a command on a stream of its own and waits for its answer, however long it takes.
    pub async fn manage(&self, command: Manage) -> anyhow::Result<GatewayReply> {
        ask(&self.connection().await?, command).await
    }
}

async fn ask(connection: &Connection, command: Manage) -> anyhow::Result<GatewayReply> {
    let answer = connection
        .ask(&GatewayRequest::Manage { command })
        .await
        .map_err(|error| if offline(&error.to_string()) { anyhow!(LOST) } else { error })?;
    match answer.context("Your gateway didn't answer. Try again.")? {
        GatewayReply::Error { message } => bail!(message),
        reply => Ok(reply),
    }
}

pub fn unexpected(reply: &GatewayReply) -> String {
    format!("Your gateway gave an unexpected answer. Update it and try again. It said: {reply:?}")
}

/// A sign-in of an account that the gateway started.
pub struct SignIn {
    pub started: GatewaySignIn,
    /// The account it signs in again.
    pub again: Option<String>,
    /// The gateway is finishing it: Codex's waits until the user approved it in the browser.
    pub waiting: bool,
    pub error: Option<String>,
    pub task: Option<AbortHandle>,
}

impl Drop for SignIn {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Names the servers and threads the gateway mentions by their keys and ids.
pub trait Naming {
    fn server(&self, key: &str) -> Option<String>;
    fn thread(&self, server: &str, thread: &str) -> Option<String>;
}

/// What the Gateway page shows.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct View {
    /// The account has a gateway.
    pub exists: bool,
    pub name: Option<String>,
    pub key: Option<String>,
    /// When it joined the account.
    pub added_at: Option<f64>,
    pub connection: Reach,
    /// "Connected", "Connecting", "Can't reach your gateway", "Your gateway turned this device away".
    pub status_text: String,
    pub error: Option<String>,
    pub version: Option<String>,
    /// Claude Code's first, each agent's by priority.
    pub accounts: Vec<AccountRow>,
    pub capture: CaptureView,
    pub sign_in: Option<SignInView>,
    /// The newest entry of the request log: when it changes, the log has new rows to show.
    pub log_head: Option<i64>,
    /// None while the gateway isn't known, or is too old to have MCP servers.
    pub mcp_servers: Option<Vec<McpServerRow>>,
    /// The newest call of the MCP servers: when it changes, the activity has new rows to show.
    pub calls_head: Option<i64>,
    /// The MCPs the add sheet offers.
    pub known: Vec<Known>,
}

/// How the client stands with its gateway. `Off` while no client view watches it.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Reach {
    Off,
    Connecting,
    Connected,
    Disconnected,
    Refused,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AccountRow {
    pub id: String,
    pub agent: Agent,
    pub name: String,
    /// Who is signed in.
    pub email: Option<String>,
    pub plan: Option<String>,
    pub status: AccountState,
    /// "Off", "Sign in again", "Resting until 14:30". None while it is ready.
    pub status_text: Option<String>,
    /// Lower is used first.
    pub priority: u32,
    /// "Priority 1".
    pub priority_text: String,
    pub enabled: bool,
    /// New threads of its agent start on it.
    pub next: bool,
    pub windows: Vec<Row>,
    /// Why there are no windows.
    pub note: Option<String>,
    pub resets: Option<limits::Resets>,
    pub threads: u32,
    /// "1 thread", "3 threads": the threads on it now. None when none is.
    pub threads_text: Option<String>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AccountState {
    Ready,
    Off,
    SignInAgain,
    Resting,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct CaptureView {
    pub capture: Capture,
    pub on: bool,
    /// "Not capturing.", "Capturing the next 20 requests, until 14:30."
    pub text: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SignInView {
    pub agent: Agent,
    /// "Sign In to Claude Code", "Sign In to Claude Code Again".
    pub title: String,
    /// The id of the account it signs in again, for starting it over.
    pub again: Option<String>,
    /// Who signs in again: the account's email, or its name.
    pub account: Option<String>,
    /// The page to open.
    pub url: String,
    /// The code to enter on that page (Codex).
    pub user_code: Option<String>,
    /// The page shows a code to paste back (Claude).
    pub needs_code: bool,
    pub waiting: bool,
    pub text: String,
    pub error: Option<String>,
}

/// What the Gateway page shows, from what is known of the gateway. `status` is None while no client
/// view watches it.
pub struct Shown<'a> {
    pub device: Option<&'a Device>,
    pub status: Option<&'a Status>,
    pub state: Option<&'a GatewayState>,
    pub sign_in: Option<&'a SignIn>,
    pub now: f64,
    pub utc_offset_secs: i32,
}

pub fn view(shown: Shown<'_>, names: &dyn Naming) -> View {
    let Shown { device, status, state, sign_in, now, utc_offset_secs } = shown;
    let connection = match (device, status) {
        (None, _) | (_, None) => Reach::Off,
        (_, Some(status)) => match status.state {
            State::Connecting => Reach::Connecting,
            State::Connected => Reach::Connected,
            State::Disconnected => Reach::Disconnected,
            State::Refused => Reach::Refused,
        },
    };
    let status_text = match connection {
        Reach::Off => "Not connected",
        Reach::Connecting => "Connecting",
        Reach::Connected => "Connected",
        Reach::Disconnected => "Can't reach your gateway",
        Reach::Refused => "Your gateway turned this device away",
    };
    let state = state.filter(|_| device.is_some());
    let mut accounts: Vec<&GatewayAccount> = state.map(|state| state.accounts.iter().collect()).unwrap_or_default();
    accounts.sort_by_key(|account| (account.agent != Agent::Claude, account.priority));
    let capture = state.map(|state| state.capture.clone()).unwrap_or_default();
    let sign_in = sign_in.map(|sign_in| sign_in_view(sign_in, &accounts));
    View {
        exists: device.is_some(),
        name: device.map(|device| device.name.clone()),
        key: device.map(|device| device.public_key.clone()),
        added_at: device.map(|device| device.created_at),
        connection,
        status_text: status_text.to_string(),
        error: status.and_then(|status| status.error.clone()),
        version: state.map(|state| state.version.clone()),
        accounts: accounts.into_iter().map(|account| account_row(account, now, utc_offset_secs)).collect(),
        capture: CaptureView {
            on: capture != Capture::Off,
            text: capture_text(&capture, state.and_then(|state| state.capture_until), names, now, utc_offset_secs),
            capture,
        },
        sign_in,
        log_head: state.and_then(|state| state.log_head),
        mcp_servers: state.and_then(|state| state.mcp_servers.as_deref()).map(mcp_servers::rows),
        calls_head: state.and_then(|state| state.calls_head),
        known: mcp_servers::known(),
    }
}

fn account_row(account: &GatewayAccount, now: f64, utc_offset_secs: i32) -> AccountRow {
    let (status, status_text) = match &account.status {
        _ if !account.enabled => (AccountState::Off, Some("Off".to_string())),
        AccountStatus::Ready => (AccountState::Ready, None),
        AccountStatus::SignInAgain => (AccountState::SignInAgain, Some("Sign in again".to_string())),
        AccountStatus::Resting { until: Some(until) } if *until > now => {
            (AccountState::Resting, Some(format!("Resting until {}", clock(*until, now, utc_offset_secs))))
        }
        AccountStatus::Resting { until: Some(_) } => (AccountState::Ready, None),
        AccountStatus::Resting { until: None } => (AccountState::Resting, Some("Resting".to_string())),
    };
    let windows: Vec<Row> = account.windows.iter().map(|window| limits::row(window, now)).collect();
    let target = limits::ResetTarget::Gateway { account_id: account.id.clone() };
    AccountRow {
        id: account.id.clone(),
        agent: account.agent,
        name: account.name.clone(),
        email: account.email.clone().filter(|email| *email != account.name),
        plan: account.plan.clone(),
        status,
        status_text,
        priority: account.priority,
        priority_text: format!("Priority {}", account.priority + 1),
        enabled: account.enabled,
        next: account.next,
        note: limits::account_note(account, &windows),
        resets: limits::resets(&account.resets, target, now),
        windows,
        threads: account.threads,
        threads_text: match account.threads {
            0 => None,
            1 => Some("1 thread".to_string()),
            threads => Some(format!("{threads} threads")),
        },
    }
}

fn capture_text(capture: &Capture, until: Option<f64>, names: &dyn Naming, now: f64, utc_offset_secs: i32) -> String {
    let what = match capture {
        Capture::Off => return "Not capturing".to_string(),
        Capture::Next { left: 1 } => "Capturing the next request".to_string(),
        Capture::Next { left } => format!("Capturing the next {left} requests"),
        Capture::Thread { server, thread } => {
            let title = names.thread(server, thread).unwrap_or_else(|| "a thread".to_string());
            let server = names.server(server).unwrap_or_else(|| "a removed server".to_string());
            format!("Capturing {title} on {server}")
        }
    };
    match until.filter(|until| *until > now) {
        Some(until) => format!("{what} until {}", clock(until, now, utc_offset_secs)),
        None => what,
    }
}

fn sign_in_view(sign_in: &SignIn, accounts: &[&GatewayAccount]) -> SignInView {
    let started = &sign_in.started;
    let again = sign_in.again.as_ref().and_then(|id| accounts.iter().find(|account| account.id == *id));
    let title = match again {
        Some(_) => format!("Sign In to {} Again", crate::usage::agent_name(started.agent)),
        None => format!("Sign In to {}", crate::usage::agent_name(started.agent)),
    };
    let needs_code = started.agent == Agent::Claude;
    let text = match needs_code {
        true => "Open the link, sign in, and paste the code it gives you.",
        false => "Open the link, sign in, and enter this code.",
    };
    SignInView {
        agent: started.agent,
        title,
        again: again.map(|account| account.id.clone()),
        account: again.map(|account| account.email.clone().unwrap_or_else(|| account.name.clone())),
        url: started.url.clone(),
        user_code: started.user_code.clone(),
        needs_code,
        waiting: sign_in.waiting,
        text: text.to_string(),
        error: sign_in.error.clone(),
    }
}

/// A request in the gateway's log, as its row shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct LogRow {
    pub id: i64,
    pub at: f64,
    /// "14:32:05" today, "Nov 1, 14:32" before.
    pub time: String,
    pub server_id: String,
    pub server: String,
    pub thread_id: String,
    /// The thread's title, or its id when it isn't known here.
    pub thread: String,
    /// The account's name.
    pub account: Option<String>,
    pub model: Option<String>,
    pub path: String,
    pub status: u16,
    pub ok: bool,
    /// "340 ms", "1.2 s".
    pub latency: String,
    /// "12.3k in · 1.1k out · 80k cache read".
    pub tokens: String,
    /// "1 retry", "2 retries". None without.
    pub retries: Option<String>,
    /// "1.2 MB sent · 40 KB received".
    pub size: String,
    /// The start of the error a failed response gave.
    pub error: Option<String>,
    pub captured: bool,
}

pub fn log_rows(
    entries: &[LogEntry],
    accounts: &[GatewayAccount],
    names: &dyn Naming,
    now: f64,
    utc_offset_secs: i32,
) -> Vec<LogRow> {
    let account_names: HashMap<&str, &str> =
        accounts.iter().map(|account| (account.id.as_str(), account.name.as_str())).collect();
    entries
        .iter()
        .map(|entry| LogRow {
            id: entry.id,
            at: entry.at,
            time: log_time(entry.at, now, utc_offset_secs),
            server_id: entry.server.clone(),
            server: names.server(&entry.server).unwrap_or_else(|| short_key(&entry.server)),
            thread_id: entry.thread.clone(),
            thread: names.thread(&entry.server, &entry.thread).unwrap_or_else(|| entry.thread.clone()),
            account: entry
                .account
                .as_ref()
                .map(|id| account_names.get(id.as_str()).map_or_else(|| id.clone(), |name| name.to_string())),
            model: entry.model.clone(),
            path: entry.path.clone(),
            status: entry.status,
            ok: (200..400).contains(&entry.status),
            latency: latency(entry.latency_ms),
            tokens: tokens(entry),
            retries: match entry.retries {
                0 => None,
                1 => Some("1 retry".to_string()),
                retries => Some(format!("{retries} retries")),
            },
            size: format!("{} sent · {} received", bytes(entry.request_bytes), bytes(entry.response_bytes)),
            error: entry.error.as_deref().map(error_excerpt),
            captured: entry.captured,
        })
        .collect()
}

/// A captured request in full, its JSON bodies laid out to read.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct CapturedView {
    pub row: LogRow,
    pub request_headers: Vec<(String, String)>,
    pub request_body: String,
    pub response_headers: Vec<(String, String)>,
    pub response_body: String,
}

pub fn captured_view(request: CapturedRequest, row: LogRow) -> CapturedView {
    CapturedView {
        row,
        request_headers: request.request_headers,
        request_body: request.request_body,
        response_headers: request.response_headers,
        response_body: request.response_body,
    }
}

/// The request with its secrets hidden and its JSON bodies laid out to read, which takes a while for
/// a long conversation.
pub fn readable_request(request: CapturedRequest) -> CapturedRequest {
    CapturedRequest {
        request_headers: hide_secrets(request.request_headers),
        request_body: readable(request.request_body),
        response_headers: hide_secrets(request.response_headers),
        response_body: readable(request.response_body),
        entry: request.entry,
    }
}

fn hide_secrets(headers: Vec<(String, String)>) -> Vec<(String, String)> {
    const SECRET: [&str; 4] = ["authorization", "x-api-key", "cookie", "set-cookie"];
    headers
        .into_iter()
        .map(|(name, value)| match SECRET.contains(&name.to_lowercase().as_str()) {
            true => (name, "Hidden".to_string()),
            false => (name, value),
        })
        .collect()
}

fn readable(body: String) -> String {
    serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or(body)
}

/// What a client shows to add a gateway: how to run it, and the command that links it to the account,
/// which a shell in it runs.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Enroll {
    pub code: String,
    pub expires_at: f64,
    pub image: String,
    /// `motile-gateway setup <code>`.
    pub command: String,
    /// The highlighting of `command`.
    pub spans: Vec<u32>,
    /// What to do, a sentence each.
    pub steps: Vec<String>,
}

/// `auth_url` is given to the gateway only when it isn't the usual auth server.
pub fn enroll(code: String, expires_at: f64, auth_url: Option<&str>, now: f64) -> Enroll {
    let prefix = auth_url.map(|auth_url| format!("MOTILE_AUTH_URL={auth_url} ")).unwrap_or_default();
    let command = format!("{prefix}motile-gateway setup {code}");
    let minutes = ((expires_at - now) / 60.0).round().max(1.0);
    let steps = vec![
        format!("Run {IMAGE} on a machine that is always on, with a volume at {DATA_DIR}."),
        "Open a shell in it and run the command below.".to_string(),
        format!("The code works once, for {minutes} minutes."),
    ];
    Enroll { spans: crate::render::highlight::command(&command), code, expires_at, image: IMAGE.into(), command, steps }
}

fn zone(utc_offset_secs: i32) -> FixedOffset {
    FixedOffset::east_opt(utc_offset_secs).unwrap_or(FixedOffset::east_opt(0).expect("UTC"))
}

fn local(at: f64, utc_offset_secs: i32) -> Option<DateTime<FixedOffset>> {
    Some(DateTime::from_timestamp(at as i64, 0)?.with_timezone(&zone(utc_offset_secs)))
}

/// "14:30" today, "Tue 14:30" within the week, "Nov 12, 14:30" further.
fn clock(at: f64, now: f64, utc_offset_secs: i32) -> String {
    let (Some(at_local), Some(today)) = (local(at, utc_offset_secs), local(now, utc_offset_secs)) else {
        return String::new();
    };
    match (at - now).abs() {
        _ if at_local.date_naive() == today.date_naive() => at_local.format("%H:%M").to_string(),
        gap if gap < 6.0 * 86400.0 => at_local.format("%a %H:%M").to_string(),
        _ => at_local.format("%b %-d, %H:%M").to_string(),
    }
}

pub(crate) fn log_time(at: f64, now: f64, utc_offset_secs: i32) -> String {
    let (Some(at_local), Some(today)) = (local(at, utc_offset_secs), local(now, utc_offset_secs)) else {
        return String::new();
    };
    match at_local.date_naive() == today.date_naive() {
        true => at_local.format("%H:%M:%S").to_string(),
        false => at_local.format("%b %-d, %H:%M").to_string(),
    }
}

pub(crate) fn latency(ms: u64) -> String {
    match ms {
        ms if ms < 1000 => format!("{ms} ms"),
        ms if ms < 10_000 => format!("{:.1} s", ms as f64 / 1000.0),
        ms => format!("{} s", (ms as f64 / 1000.0).round()),
    }
}

fn count(n: u64) -> String {
    match n {
        n if n < 1000 => n.to_string(),
        n if n < 10_000 => format!("{:.1}k", n as f64 / 1000.0).replace(".0k", "k"),
        n if n < 1_000_000 => format!("{}k", (n as f64 / 1000.0).round()),
        n => format!("{:.1}M", n as f64 / 1_000_000.0).replace(".0M", "M"),
    }
}

fn tokens(entry: &LogEntry) -> String {
    let parts = [
        (entry.input_tokens, "in"),
        (entry.output_tokens, "out"),
        (entry.cache_read_tokens, "cache read"),
        (entry.cache_write_tokens, "cache write"),
    ];
    let said: Vec<String> =
        parts.iter().filter(|(n, _)| *n > 0).map(|(n, what)| format!("{} {what}", count(*n))).collect();
    if said.is_empty() { "No tokens".to_string() } else { said.join(" · ") }
}

fn bytes(n: u64) -> String {
    match n {
        n if n < 1000 => format!("{n} B"),
        n if n < 1_000_000 => format!("{} KB", (n as f64 / 1000.0).round()),
        n if n < 1_000_000_000 => format!("{:.1} MB", n as f64 / 1_000_000.0),
        n => format!("{:.1} GB", n as f64 / 1_000_000_000.0),
    }
}

/// The message of an Anthropic or OpenAI error, or the body's first line, cut short.
fn error_excerpt(body: &str) -> String {
    let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
    let message = parsed.as_ref().and_then(|value| {
        value.pointer("/error/message").or_else(|| value.get("message")).and_then(|message| message.as_str())
    });
    let text = message.unwrap_or_else(|| body.lines().find(|line| !line.trim().is_empty()).unwrap_or_default()).trim();
    match text.char_indices().nth(ERROR_EXCERPT) {
        Some((cut, _)) => format!("{}…", text[..cut].trim_end()),
        None => text.to_string(),
    }
}

pub(crate) fn short_key(key: &str) -> String {
    key.chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use motile_protocol::wire::LimitWindow;

    use super::*;

    const NOW: f64 = 1_762_000_000.0;

    struct Names;

    impl Naming for Names {
        fn server(&self, key: &str) -> Option<String> {
            (key == "s1").then(|| "studio".to_string())
        }

        fn thread(&self, server: &str, thread: &str) -> Option<String> {
            (server == "s1" && thread == "t1").then(|| "Fix the build".to_string())
        }
    }

    fn account(id: &str, agent: Agent, priority: u32) -> GatewayAccount {
        GatewayAccount {
            id: id.into(),
            agent,
            name: id.into(),
            email: Some(format!("{id}@b.c")),
            plan: Some("Max".into()),
            priority,
            enabled: true,
            status: AccountStatus::Ready,
            windows: vec![],
            reset_credits: 0,
            resets: vec![],
            threads: 0,
            next: false,
        }
    }

    fn state(accounts: Vec<GatewayAccount>) -> GatewayState {
        GatewayState {
            version: "0.1.300".into(),
            accounts,
            capture: Capture::Off,
            capture_until: None,
            log_head: None,
            mcp_servers: None,
            calls_head: None,
        }
    }

    fn device() -> Device {
        Device {
            public_key: "p".into(),
            kind: motile_protocol::auth_api::DeviceKind::Gateway,
            name: "Gateway".into(),
            platform: "linux".into(),
            created_at: 1.0,
        }
    }

    fn shown<'a>(device: Option<&'a Device>, status: Option<&'a Status>, state: Option<&'a GatewayState>) -> View {
        view(Shown { device, status, state, sign_in: None, now: NOW, utc_offset_secs: 0 }, &Names)
    }

    #[test]
    fn each_account_says_how_it_stands_claude_codes_first_by_priority() {
        let mut off = account("bench", Agent::Claude, 0);
        off.enabled = false;
        let mut dead = account("dead", Agent::Codex, 0);
        dead.status = AccountStatus::SignInAgain;
        let mut resting = account("tired", Agent::Claude, 1);
        resting.status = AccountStatus::Resting { until: Some(NOW + 3600.0) };
        let mut rested = account("rested", Agent::Claude, 2);
        rested.status = AccountStatus::Resting { until: Some(NOW - 60.0) };
        let mut busy = account("main", Agent::Claude, 0);
        busy.name = "main@b.c".into();
        busy.threads = 3;
        busy.next = true;
        busy.windows = vec![LimitWindow {
            label: "Session".into(),
            used_percent: 40.0,
            resets_at: Some(NOW + 3600.0),
            window_secs: Some(18000),
            warning: false,
        }];
        let state = state(vec![dead, resting, busy, off, rested]);
        let status = Status { state: State::Connected, error: None };
        let view = shown(Some(&device()), Some(&status), Some(&state));
        let rows: Vec<_> = view
            .accounts
            .iter()
            .map(|row| (row.id.as_str(), row.status, row.status_text.as_deref(), row.priority_text.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("main", AccountState::Ready, None, "Priority 1"),
                ("bench", AccountState::Off, Some("Off"), "Priority 1"),
                ("tired", AccountState::Resting, Some("Resting until 13:26"), "Priority 2"),
                ("rested", AccountState::Ready, None, "Priority 3"),
                ("dead", AccountState::SignInAgain, Some("Sign in again"), "Priority 1"),
            ]
        );
        let main = &view.accounts[0];
        assert_eq!(
            (main.next, main.threads_text.as_deref(), main.note.as_deref(), main.email.as_deref()),
            (true, Some("3 threads"), None, None)
        );
        assert_eq!(view.accounts[1].email.as_deref(), Some("bench@b.c"));
        assert_eq!(main.windows[0].used, "40%");
        assert_eq!(view.accounts[4].note.as_deref(), Some("Sign in again to see its limits."));
        assert_eq!(view.accounts[1].note.as_deref(), Some("No limits read yet."));
        assert_eq!((view.status_text.as_str(), view.version.as_deref()), ("Connected", Some("0.1.300")));
    }

    #[test]
    fn signing_an_account_in_again_names_it_under_the_title() {
        let mut dead = account("dead", Agent::Claude, 0);
        dead.status = AccountStatus::SignInAgain;
        let state = state(vec![dead]);
        let titled = |again: Option<&str>| {
            let sign_in = SignIn {
                started: GatewaySignIn {
                    id: "s".into(),
                    agent: Agent::Claude,
                    url: "https://a".into(),
                    user_code: None,
                },
                again: again.map(String::from),
                waiting: false,
                error: None,
                task: None,
            };
            let shown = Shown {
                device: Some(&device()),
                status: None,
                state: Some(&state),
                sign_in: Some(&sign_in),
                now: NOW,
                utc_offset_secs: 0,
            };
            let view = view(shown, &Names).sign_in.unwrap();
            (view.title, view.again, view.account)
        };
        assert_eq!(titled(None), ("Sign In to Claude Code".to_string(), None, None));
        assert_eq!(
            titled(Some("dead")),
            ("Sign In to Claude Code Again".to_string(), Some("dead".to_string()), Some("dead@b.c".to_string()))
        );
    }

    #[test]
    fn the_page_says_whether_there_is_a_gateway_and_how_the_connection_stands() {
        let none = shown(None, None, None);
        assert_eq!((none.exists, none.connection, none.accounts.len()), (false, Reach::Off, 0));

        let kept = state(vec![account("a", Agent::Claude, 0)]);
        let unwatched = shown(Some(&device()), None, Some(&kept));
        assert_eq!((unwatched.exists, unwatched.connection, unwatched.accounts.len()), (true, Reach::Off, 1));

        let refused = Status { state: State::Refused, error: Some("Not your account.".into()) };
        let view = shown(Some(&device()), Some(&refused), None);
        assert_eq!(
            (view.status_text.as_str(), view.error.as_deref()),
            ("Your gateway turned this device away", Some("Not your account."))
        );
    }

    #[test]
    fn the_capture_says_what_it_keeps_and_until_when() {
        let text = |capture: Capture, until| capture_text(&capture, until, &Names, NOW, 0);
        assert_eq!(text(Capture::Off, None), "Not capturing");
        assert_eq!(text(Capture::Next { left: 1 }, None), "Capturing the next request");
        assert_eq!(text(Capture::Next { left: 20 }, Some(NOW + 7200.0)), "Capturing the next 20 requests until 14:26");
        assert_eq!(
            text(Capture::Thread { server: "s1".into(), thread: "t1".into() }, None),
            "Capturing Fix the build on studio"
        );
    }

    fn entry() -> LogEntry {
        LogEntry {
            id: 9,
            at: NOW - 30.0,
            server: "s1".into(),
            thread: "t1".into(),
            account: Some("main".into()),
            model: Some("claude-opus-5-5".into()),
            path: "/v1/messages".into(),
            status: 200,
            latency_ms: 1234,
            input_tokens: 12_345,
            output_tokens: 980,
            cache_read_tokens: 80_000,
            cache_write_tokens: 0,
            retries: 1,
            request_bytes: 1_234_567,
            response_bytes: 40_100,
            error: None,
            captured: true,
        }
    }

    #[test]
    fn a_log_row_names_what_the_entry_points_at_and_sizes_it_for_reading() {
        let failed = LogEntry {
            id: 8,
            at: NOW - 2.0 * 86400.0,
            server: "gone-server-key".into(),
            thread: "t9".into(),
            account: Some("unknown".into()),
            status: 429,
            latency_ms: 80,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            retries: 0,
            error: Some(r#"{"type":"error","error":{"type":"rate_limit_error","message":"Rate limited."}}"#.into()),
            ..entry()
        };
        let rows = log_rows(&[entry(), failed], &[account("main", Agent::Claude, 0)], &Names, NOW, 0);
        let row = &rows[0];
        assert_eq!(
            (row.time.as_str(), row.server.as_str(), row.thread.as_str(), row.account.as_deref(), row.ok),
            ("12:26:10", "studio", "Fix the build", Some("main"), true)
        );
        assert_eq!(
            (row.latency.as_str(), row.tokens.as_str(), row.retries.as_deref(), row.size.as_str()),
            ("1.2 s", "12k in · 980 out · 80k cache read", Some("1 retry"), "1.2 MB sent · 40 KB received")
        );
        let row = &rows[1];
        assert_eq!(
            (row.time.as_str(), row.server.as_str(), row.thread.as_str(), row.account.as_deref(), row.ok),
            ("Oct 30, 12:26", "gone-ser", "t9", Some("unknown"), false)
        );
        assert_eq!((row.latency.as_str(), row.tokens.as_str()), ("80 ms", "No tokens"));
        assert_eq!(row.error.as_deref(), Some("Rate limited."));
    }

    #[test]
    fn an_error_that_isnt_json_shows_its_first_line_cut_short() {
        assert_eq!(error_excerpt("\nupstream timed out\nmore"), "upstream timed out");
        let long = "x".repeat(300);
        assert_eq!(error_excerpt(&long).chars().count(), ERROR_EXCERPT + 1);
    }

    #[test]
    fn a_captured_request_hides_secrets_and_lays_its_json_out() {
        let request = CapturedRequest {
            entry: entry(),
            request_headers: vec![("Authorization".into(), "Bearer abc".into()), ("anthropic-beta".into(), "x".into())],
            request_body: r#"{"model":"opus"}"#.into(),
            response_headers: vec![],
            response_body: "event: message_start".into(),
        };
        let view = captured_view(readable_request(request), log_rows(&[entry()], &[], &Names, NOW, 0).remove(0));
        assert_eq!(view.request_headers[0].1, "Hidden");
        assert_eq!(view.request_headers[1].1, "x");
        assert_eq!(view.request_body, "{\n  \"model\": \"opus\"\n}");
        assert_eq!(view.response_body, "event: message_start");
        assert_eq!(view.row.account.as_deref(), Some("main"));
    }

    /// A gateway on this machine that answers `State` with one account and `Log` with an entry.
    async fn fake_gateway() -> (Endpoint, ServerAddr) {
        use iroh::endpoint::presets;
        use motile_protocol::frame::{read_frame, write_frame};
        let key = motile_protocol::identity::DeviceKey::generate();
        let endpoint = Endpoint::builder(presets::Minimal)
            .secret_key(iroh::SecretKey::from_bytes(&key.to_bytes()))
            .alpns(vec![GATEWAY_ALPN.to_vec()])
            .bind()
            .await
            .unwrap();
        let port = endpoint.bound_sockets().iter().find(|address| address.is_ipv4()).unwrap().port();
        let address = format!("{}@127.0.0.1:{port}", key.public()).parse().unwrap();
        let serving = endpoint.clone();
        tokio::spawn(async move {
            while let Some(incoming) = serving.accept().await {
                let Ok(connection) = incoming.await else { continue };
                tokio::spawn(async move {
                    while let Ok((mut send, mut recv)) = connection.accept_bi().await {
                        let Ok(Some(GatewayRequest::Manage { command })) = read_frame(&mut recv).await else { return };
                        let reply = match command {
                            Manage::State => {
                                GatewayReply::State { state: state(vec![account("main", Agent::Claude, 0)]) }
                            }
                            Manage::Log { .. } => GatewayReply::Log { entries: vec![entry()] },
                            _ => GatewayReply::Error { message: "Not here.".into() },
                        };
                        let _ = write_frame(&mut send, &reply).await;
                        let _ = send.finish();
                    }
                });
            }
        });
        (endpoint, address)
    }

    #[tokio::test]
    async fn the_link_follows_the_gateways_state_and_carries_commands() {
        let (_gateway, address) = fake_gateway().await;
        let client = crate::connection::bind(&motile_protocol::identity::DeviceKey::generate(), true).await.unwrap();
        let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
        let emit: Emit = Arc::new(move |event| {
            let _ = events.send(event);
        });
        let link = GatewayLink::connect(client, address, Away::new(), emit);
        let mut seen = (None, None);
        while seen.0.is_none() || seen.1.is_none() {
            match tokio::time::timeout(Duration::from_secs(10), received.recv()).await.unwrap().unwrap() {
                GatewayEvent::State(state) => seen.0 = Some(state.accounts[0].id.clone()),
                GatewayEvent::Status(status) if status.state == State::Connected => seen.1 = Some(()),
                GatewayEvent::Status(_) => {}
            }
        }
        assert_eq!(seen.0.as_deref(), Some("main"));

        let log = link.manage(Manage::Log { before: None, errors_only: false, limit: LOG_PAGE }).await.unwrap();
        assert!(matches!(log, GatewayReply::Log { entries } if entries[0].id == 9));
        let refused = link.manage(Manage::RemoveAccount { id: "main".into() }).await.unwrap_err();
        assert_eq!(refused.to_string(), "Not here.");
        link.shutdown();
    }

    #[test]
    fn adding_a_gateway_says_what_to_run_and_the_command_that_links_it() {
        let shown = enroll("ABCD1234".into(), NOW + 900.0, None, NOW);
        assert_eq!(shown.command, "motile-gateway setup ABCD1234");
        assert!(shown.steps[0].starts_with("Run ghcr.io/motileapp/motile-gateway:latest"));
        assert!(shown.steps[2].ends_with("for 15 minutes."));
        let dev = enroll("ABCD1234".into(), NOW + 900.0, Some("http://localhost:8787"), NOW);
        assert_eq!(dev.command, "MOTILE_AUTH_URL=http://localhost:8787 motile-gateway setup ABCD1234");
    }
}
