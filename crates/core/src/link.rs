//! Keeps the client connected to one server: reconnects when the connection drops, and follows the
//! thread list and the threads the client has open or that are active, catching each up from the
//! revision it had.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use futures_lite::StreamExt;
use iroh::endpoint::PathEvent;
use iroh::{Endpoint, TransportAddr};
use motile_protocol::wire::{FileKind, GitStage, Item, Message, Request, RestartWhen};
use serde::Serialize;
use tokio::sync::{Notify, mpsc};
use tokio::task::AbortHandle;

use crate::away::Away;
use crate::connection::{Closed, Connection, NOT_CONNECTED, PathKind, ServerAddr, lost, offline};

const RETRY_DELAYS: [Duration; 4] =
    [Duration::from_millis(300), Duration::from_secs(1), Duration::from_secs(2), Duration::from_secs(5)];
const RTT_REFRESH: Duration = Duration::from_secs(5);
/// How long to wait before asking again a server that turned this device away.
const REFUSED_RETRY: Duration = Duration::from_secs(10);
/// A dial that takes longer than this is given up and started again.
const DIAL_TIMEOUT: Duration = Duration::from_secs(30);
/// A connection whose server hasn't sent its thread list by then is given up and dialed again.
const WELCOME_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a request made while the link dials again waits for the connection.
const RECONNECT_WAIT: Duration = Duration::from_secs(15);
/// How many times a file is sent from its start when the connection is lost on the way.
const UPLOAD_TRIES: u32 = 3;
/// A request that has had no answer for this long has the link check that the connection still
/// carries requests at all.
const SUSPECT_AFTER: Duration = Duration::from_secs(10);
/// How often a connection that looks fine is checked anyway.
const CHECK_EVERY: Duration = Duration::from_secs(30);
/// How long a check waits for the server's `Pong`.
const PING_WITHIN: Duration = Duration::from_secs(5);
/// The first protocol whose servers answer `Ping`.
const PING_PROTOCOL: u32 = 37;

#[derive(Clone, Debug)]
pub enum LinkEvent {
    Status(Status),
    /// `Welcome`, then the changes to the thread list and the projects.
    List(Message),
    /// What `Open` answers for the thread; `Error` if it couldn't be opened.
    Thread {
        thread_id: String,
        message: Message,
    },
}

#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    Connecting,
    /// The server has answered with its thread list.
    Connected,
    Disconnected,
    /// The server turned this device away.
    Refused,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub state: State,
    pub error: Option<String>,
    pub path: Option<PathKind>,
    pub rtt_ms: Option<u64>,
}

pub struct Link {
    server_id: String,
    endpoint: Mutex<Endpoint>,
    /// Whether a connection that ended went because the client was away, which isn't an outage.
    away: Away,
    events: mpsc::UnboundedSender<(String, LinkEvent)>,
    inner: Mutex<Inner>,
    /// Ends the wait before the next dial.
    wake: Notify,
    /// Ends the dial or the connection there is, to dial again at once.
    restart: Notify,
    /// Tells the requests that wait for a connection that there is one.
    attached: Notify,
    /// Ends the connection there is, which stopped carrying requests, for `Inner::given_up`.
    give_up: Notify,
}

#[derive(Default)]
struct Inner {
    status: Status,
    connection: Option<Connection>,
    /// The task that dials and redials.
    dialer: Option<AbortHandle>,
    /// Tasks tied to the current connection.
    followers: Vec<AbortHandle>,
    open: HashMap<String, OpenThread>,
    /// Where the server was last reached, for dialing it on a new endpoint.
    known: Vec<TransportAddr>,
    /// Why the connection there is was given up.
    given_up: Option<String>,
    /// The protocol the server speaks, once it has said.
    protocol: u32,
}

struct OpenThread {
    /// The revision the client has everything up to; a reconnect asks for what came after.
    synced: u64,
    follower: Option<AbortHandle>,
}

impl Link {
    pub fn connect(
        endpoint: Endpoint,
        server: ServerAddr,
        away: Away,
        events: mpsc::UnboundedSender<(String, LinkEvent)>,
    ) -> Arc<Self> {
        let link = Arc::new(Self {
            server_id: server.key.clone(),
            endpoint: Mutex::new(endpoint),
            away,
            events,
            inner: Mutex::default(),
            wake: Notify::new(),
            restart: Notify::new(),
            attached: Notify::new(),
            give_up: Notify::new(),
        });
        let dialer = tokio::spawn(link.clone().dial_forever(server));
        link.lock().dialer = Some(dialer.abort_handle());
        link
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn emit(&self, event: LinkEvent) {
        let _ = self.events.send((self.server_id.clone(), event));
    }

    fn update_status(&self, change: impl FnOnce(&mut Status)) {
        let mut inner = self.lock();
        let before = inner.status.clone();
        change(&mut inner.status);
        if inner.status.state != before.state {
            tracing::info!(server = self.server_id, state = ?inner.status.state, error = ?inner.status.error, "link");
        }
        if inner.status != before {
            self.emit(LinkEvent::Status(inner.status.clone()));
        }
    }

    pub fn status(&self) -> Status {
        self.lock().status.clone()
    }

    /// Stops for good. The link isn't used again.
    pub fn shutdown(&self) {
        let dialer = self.lock().dialer.take();
        if let Some(dialer) = dialer {
            dialer.abort();
        }
        self.drop_connection();
    }

    /// Lets go of the dial or the connection there is and dials again at once, on `endpoint`.
    /// For when the old endpoint can't be trusted to reach anything anymore.
    pub fn redial_on(&self, endpoint: Endpoint) {
        *self.endpoint.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = endpoint;
        self.restart.notify_one();
    }

    /// Dials now if the link is waiting to dial again. For when the network has changed.
    pub fn retry_now(&self) {
        self.wake.notify_one();
    }

    fn drop_connection(&self) {
        let mut inner = self.lock();
        for follower in inner.followers.drain(..) {
            follower.abort();
        }
        for follower in inner.open.values_mut().filter_map(|open| open.follower.take()) {
            follower.abort();
        }
        if let Some(connection) = inner.connection.take() {
            connection.close();
        }
    }

    async fn dial_forever(self: Arc<Self>, server: ServerAddr) {
        let mut failures = 0;
        loop {
            self.update_status(|status| {
                *status = Status { state: State::Connecting, error: status.error.take(), ..Status::default() }
            });
            let endpoint = self.endpoint.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
            let started = SystemTime::now();
            let closed = tokio::select! {
                biased;
                _ = self.restart.notified() => None,
                closed = self.connect_until_closed(&endpoint, &server) => Some(closed),
            };
            self.drop_connection();
            let Some(closed) = closed else {
                failures = 0;
                continue;
            };

            // A server that turns the device away still completes the handshake first, so only a
            // connection that got its thread list counts as having worked.
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
            // What lived through the client being away ends as it comes back, and says nothing of
            // the server: it shows as dialing again, and only a dial from after says why it fails.
            let quiet = !closed.refused && self.away.excuses(started);
            if quiet {
                tracing::info!(server = self.server_id, reason = closed.reason, "a connection from while away ended");
            }
            self.update_status(|status| {
                *status = match quiet {
                    true => Status { state: State::Connecting, ..Status::default() },
                    false => Status {
                        state: if closed.refused { State::Refused } else { State::Disconnected },
                        error: Some(closed.reason),
                        ..Status::default()
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

    async fn connect_until_closed(self: &Arc<Self>, endpoint: &Endpoint, server: &ServerAddr) -> Closed {
        let known = self.lock().known.clone();
        let dialed = Connection::dial_on(endpoint, server, &known, motile_protocol::ALPN);
        let connection = match tokio::time::timeout(DIAL_TIMEOUT, dialed).await {
            Ok(Ok(connection)) => connection,
            Ok(Err(error)) => return Closed { refused: false, reason: format!("{error:#}") },
            Err(_) => return Closed { refused: false, reason: "Your server didn't answer in time.".to_string() },
        };
        self.attach(&connection);
        let welcomed = async {
            tokio::time::sleep(WELCOME_TIMEOUT).await;
            if self.status().state == State::Connected {
                std::future::pending::<()>().await;
            }
        };
        let checked = async {
            loop {
                tokio::time::sleep(CHECK_EVERY).await;
                self.check(&connection).await;
            }
        };
        tokio::select! {
            closed = connection.closed() => closed,
            () = welcomed => Closed { refused: false, reason: "Your server didn't send its threads in time.".to_string() },
            () = self.give_up.notified() => {
                let reason = self.lock().given_up.take().unwrap_or_default();
                Closed { refused: false, reason }
            }
            () = checked => unreachable!(),
        }
    }

    /// Checks the connection there is now, as when the client comes back or the network changes,
    /// which may have cut it without a word.
    pub fn check_now(self: &Arc<Self>) {
        let Some(connection) = self.lock().connection.clone() else { return };
        let link = self.clone();
        tokio::spawn(async move { link.check(&connection).await });
    }

    /// Gives `connection` up when it no longer carries a request there and back, so that the link
    /// dials again and says why. The connection can look open all along: QUIC keeps it alive
    /// while nothing gets through.
    async fn check(&self, connection: &Connection) -> Option<String> {
        if self.lock().protocol < PING_PROTOCOL || connection.is_closed() {
            return None;
        }
        let Err(error) = connection.ping(PING_WITHIN).await else { return None };
        let path = connection.path().map(|(kind, _)| format!(", last over {}", format!("{kind:?}").to_lowercase()));
        let reason = format!("Your server stopped answering: {error}{}.", path.unwrap_or_default());
        tracing::warn!(server = self.server_id, reason, "gave up a connection");
        let mut inner = self.lock();
        if inner.connection.as_ref().is_some_and(|current| current.is(connection)) {
            inner.given_up = Some(reason.clone());
            drop(inner);
            self.give_up.notify_waiters();
        }
        Some(reason)
    }

    /// Starts using a connection whose handshake is done. It counts as connected once the server
    /// sends its thread list.
    fn attach(self: &Arc<Self>, connection: &Connection) {
        let mut inner = self.lock();
        inner.connection = Some(connection.clone());
        inner.followers.push(tokio::spawn(self.clone().follow_list(connection.clone())).abort_handle());
        inner.followers.push(tokio::spawn(self.clone().watch_path(connection.clone())).abort_handle());
        let open: Vec<String> = inner.open.keys().cloned().collect();
        for thread_id in open {
            let follower = tokio::spawn(self.clone().follow_thread(connection.clone(), thread_id.clone()));
            if let Some(open) = inner.open.get_mut(&thread_id) {
                open.follower = Some(follower.abort_handle());
            }
        }
        self.attached.notify_waiters();
    }

    async fn follow_list(self: Arc<Self>, connection: Connection) {
        // The stream ends when the client fell behind; following again starts from a fresh list.
        while let Ok(mut follow) = connection.follow(&Request::Subscribe).await {
            loop {
                let message = match follow.next().await {
                    Ok(Some(message)) => message,
                    Ok(None) => break,
                    Err(error) => {
                        tracing::warn!(server = self.server_id, "the thread list stopped: {error:#}");
                        break;
                    }
                };
                let welcomed = matches!(message, Message::Welcome { .. });
                if let Message::Welcome { server, .. } = &message {
                    self.lock().protocol = server.protocol;
                }
                self.emit(LinkEvent::List(message));
                if !welcomed {
                    continue;
                }
                // Said after the list itself, so whoever hears "connected" already has it.
                let path = connection.path();
                self.remember_addresses(&connection);
                self.update_status(|status| {
                    status.state = State::Connected;
                    status.error = None;
                    status.path = path.map(|(kind, _)| kind);
                    status.rtt_ms = path.map(|(_, rtt)| rtt.as_millis() as u64);
                });
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        connection.close();
    }

    async fn follow_thread(self: Arc<Self>, connection: Connection, thread_id: String) {
        loop {
            let Some(since) = self.lock().open.get(&thread_id).map(|open| open.synced) else { return };
            let request = Request::Open { thread_id: thread_id.clone(), since };
            let Ok(mut follow) = connection.follow(&request).await else { return };
            let mut live = false;
            while let Ok(Some(message)) = follow.next().await {
                let failed = matches!(message, Message::Error { .. });
                self.note_progress(&thread_id, &message, &mut live);
                self.emit(LinkEvent::Thread { thread_id: thread_id.clone(), message });
                if failed {
                    self.lock().open.remove(&thread_id);
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Remembers how far the thread has been received. Until `Synced`, items arrive in
    /// transcript order rather than by revision, so only what comes after it counts.
    fn note_progress(&self, thread_id: &str, message: &Message, live: &mut bool) {
        let rev = match message {
            Message::Opened { reset: true, .. } => Some(0),
            Message::Synced { rev } => {
                *live = true;
                Some(*rev)
            }
            Message::Items { items } if *live => items.iter().map(|item| item.rev).max(),
            Message::TextDelta { rev, .. } if *live => Some(*rev),
            _ => None,
        };
        let Some(rev) = rev else { return };
        if let Some(open) = self.lock().open.get_mut(thread_id) {
            open.synced = rev;
        }
    }

    async fn watch_path(self: Arc<Self>, connection: Connection) {
        let mut events = connection.path_events();
        loop {
            let selected = tokio::select! {
                event = events.next() => match event {
                    Some(PathEvent::Selected { .. } | PathEvent::Lagged { .. }) => true,
                    Some(_) => false,
                    None => return,
                },
                _ = tokio::time::sleep(RTT_REFRESH) => true,
            };
            if !selected {
                continue;
            }
            let Some((kind, rtt)) = connection.path() else { continue };
            self.remember_addresses(&connection);
            self.update_status(|status| {
                status.path = Some(kind);
                status.rtt_ms = Some(rtt.as_millis() as u64);
            });
        }
    }

    fn remember_addresses(&self, connection: &Connection) {
        let addresses = connection.addresses();
        if !addresses.is_empty() {
            self.lock().known = addresses;
        }
    }

    /// Follows the thread from revision `since`, now and after every reconnect.
    pub fn open(self: &Arc<Self>, thread_id: String, since: u64) {
        let mut inner = self.lock();
        if inner.open.contains_key(&thread_id) {
            return;
        }
        let follower = inner
            .connection
            .clone()
            .map(|connection| tokio::spawn(self.clone().follow_thread(connection, thread_id.clone())).abort_handle());
        inner.open.insert(thread_id, OpenThread { synced: since, follower });
    }

    pub fn close(&self, thread_id: &str) {
        let follower = self.lock().open.remove(thread_id).and_then(|open| open.follower);
        if let Some(follower) = follower {
            follower.abort();
        }
    }

    /// The connection there is, or the next one while the link dials again.
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
        tokio::time::timeout(RECONNECT_WAIT, next).await.map_err(|_| anyhow::anyhow!(NOT_CONNECTED))
    }

    /// Asks the server and waits for its answer. One that takes long has the connection checked,
    /// which ends the wait with an error if the connection no longer carries anything.
    pub async fn request(&self, request: &Request) -> anyhow::Result<Message> {
        let connection = self.connection().await?;
        let asked = connection.request(request);
        tokio::pin!(asked);
        let answer = tokio::select! {
            answer = &mut asked => answer,
            () = tokio::time::sleep(SUSPECT_AFTER) => match self.check(&connection).await {
                Some(reason) => Err(lost(reason)),
                None => asked.await,
            }
        };
        match answer {
            Ok(Message::Error { message }) => Err(anyhow::anyhow!(message)),
            Ok(message) => Ok(message),
            Err(error) => {
                tracing::warn!(server = self.server_id, "a request failed: {error:#}");
                Err(error)
            }
        }
    }

    /// Has the server install the latest release, telling `progress` how far the download is
    /// and `waiting` once it waits for its agents to finish before it restarts.
    pub async fn update(
        &self,
        when: Option<RestartWhen>,
        mut progress: impl FnMut(u64, Option<u64>),
        mut waiting: impl FnMut(),
    ) -> anyhow::Result<()> {
        let mut follow = self.connection().await?.follow(&Request::UpdateServer { when }).await?;
        loop {
            // A server from before it could update itself closes the stream without an answer.
            let Some(message) = follow.next().await? else {
                anyhow::bail!("This server is too old to update itself. Run the install command on its machine again.");
            };
            match message {
                Message::Updating { received, total } => progress(received, total),
                Message::UpdateWaiting => waiting(),
                Message::Ok => return Ok(()),
                Message::Error { message } => anyhow::bail!(message),
                other => {
                    anyhow::bail!("Your server gave an unexpected answer. Update it and try again. It said: {other:?}")
                }
            }
        }
    }

    /// Sends a `GitRun`, telling `started` each stage, and answers with the server's `GitDone`.
    pub async fn git_run(&self, request: &Request, mut started: impl FnMut(GitStage)) -> anyhow::Result<Message> {
        let mut follow = self.connection().await?.follow(request).await?;
        loop {
            let Some(message) = follow.next().await? else {
                anyhow::bail!("Your server didn't answer. Try again.");
            };
            match message {
                Message::GitProgress { stage } => started(stage),
                Message::Error { message } => anyhow::bail!(message),
                done => return Ok(done),
            }
        }
    }

    pub async fn upload(
        &self,
        path: &Path,
        poster_of: Option<String>,
        mut progress: impl FnMut(u64, u64),
    ) -> anyhow::Result<(String, Option<String>)> {
        let mut tries = 1;
        loop {
            let sent = self.connection().await?.upload(path, poster_of.clone(), &mut progress).await;
            match sent {
                Err(error) if tries < UPLOAD_TRIES && offline(&error.to_string()) => tries += 1,
                sent => return sent,
            }
        }
    }

    pub async fn file(&self, request: &Request, shown: &Path) -> anyhow::Result<(FileKind, u64, Vec<u8>)> {
        self.connection().await?.file(request, shown).await
    }

    /// The transcript of a removed server's thread, from this server's copy of it.
    pub async fn open_copy(&self, origin: &str, thread_id: &str) -> anyhow::Result<Vec<Item>> {
        let request = Request::OpenCopy { origin: origin.to_string(), thread_id: thread_id.to_string() };
        let mut follow = self.connection().await?.follow(&request).await?;
        let mut items = Vec::new();
        while let Some(message) = follow.next().await? {
            match message {
                Message::Items { items: piece } => items.extend(piece),
                Message::Error { message } => anyhow::bail!(message),
                _ => {}
            }
        }
        Ok(items)
    }

    pub async fn media(&self, id: &str, file: &Path, progress: impl FnMut(u64, u64)) -> anyhow::Result<()> {
        self.connection().await?.media(id, file, progress).await
    }
}

#[cfg(test)]
mod tests {
    use motile_protocol::ALPN;
    use motile_protocol::identity::DeviceKey;

    use super::*;
    use crate::connection::bind;

    /// A server that takes every stream and never answers one.
    async fn silent_server() -> (Endpoint, ServerAddr) {
        let endpoint =
            Endpoint::builder(iroh::endpoint::presets::Minimal).alpns(vec![ALPN.to_vec()]).bind().await.unwrap();
        let accepting = endpoint.clone();
        tokio::spawn(async move {
            while let Some(incoming) = accepting.accept().await {
                let Ok(connection) = incoming.await else { continue };
                tokio::spawn(async move {
                    let mut streams = Vec::new();
                    while let Ok(stream) = connection.accept_bi().await {
                        streams.push(stream);
                    }
                });
            }
        });
        let port = endpoint.bound_sockets().iter().find(|address| address.is_ipv4()).unwrap().port();
        let address = format!("{}@127.0.0.1:{port}", endpoint.id()).parse().unwrap();
        (endpoint, address)
    }

    /// A server that sends its threads, then takes every other stream and never answers it, as
    /// a connection that looks open but carries nothing does.
    async fn server_that_stops_answering() -> (Endpoint, ServerAddr) {
        let endpoint =
            Endpoint::builder(iroh::endpoint::presets::Minimal).alpns(vec![ALPN.to_vec()]).bind().await.unwrap();
        let accepting = endpoint.clone();
        tokio::spawn(async move {
            while let Some(incoming) = accepting.accept().await {
                let Ok(connection) = incoming.await else { continue };
                tokio::spawn(async move {
                    let mut streams = Vec::new();
                    while let Ok((mut send, recv)) = connection.accept_bi().await {
                        if streams.is_empty() {
                            let server = serde_json::json!({
                                "version": "1.0.0", "protocol": PING_PROTOCOL, "hostname": "studio", "home": "/",
                                "agents": [], "models": [],
                            });
                            let server = serde_json::from_value(server).unwrap();
                            let welcome = Message::Welcome { server, threads: Vec::new(), projects: Vec::new() };
                            motile_protocol::frame::write_frame(&mut send, &welcome).await.unwrap();
                        }
                        streams.push((send, recv));
                    }
                });
            }
        });
        let port = endpoint.bound_sockets().iter().find(|address| address.is_ipv4()).unwrap().port();
        let address = format!("{}@127.0.0.1:{port}", endpoint.id()).parse().unwrap();
        (endpoint, address)
    }

    #[tokio::test]
    async fn a_connection_that_stops_carrying_requests_is_given_up_and_says_why() {
        let (_server, address) = server_that_stops_answering().await;
        let endpoint = bind(&DeviceKey::generate(), true).await.unwrap();
        let (events, mut heard) = mpsc::unbounded_channel();
        let link = Link::connect(endpoint, address, Away::new(), events);
        while link.status().state != State::Connected {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let within = SUSPECT_AFTER + PING_WITHIN + Duration::from_secs(5);
        let request = Request::Skills { project_id: "p".to_string() };
        let failed = tokio::time::timeout(within, link.request(&request)).await.expect("the request waited forever");
        let given_up = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some((_, event)) = heard.recv().await {
                if let LinkEvent::Status(status) = event
                    && status.state == State::Disconnected
                {
                    return status;
                }
            }
            panic!("the link stopped");
        })
        .await
        .expect("the link still says it is connected");

        let error = failed.unwrap_err().to_string();
        assert!(offline(&error), "{error}");
        assert!(error.contains("Your server stopped answering"), "{error}");
        assert!(given_up.error.unwrap().starts_with("Your server stopped answering: a check got no answer within 5s"));
        link.shutdown();
    }

    #[tokio::test]
    async fn a_connection_that_ends_while_the_client_is_away_dials_again_without_saying_why() {
        let (_server, address) = server_that_stops_answering().await;
        let endpoint = bind(&DeviceKey::generate(), true).await.unwrap();
        let (events, mut heard) = mpsc::unbounded_channel();
        let away = Away::new();
        let link = Link::connect(endpoint, address, away.clone(), events);
        while link.status().state != State::Connected {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        while heard.try_recv().is_ok() {}

        away.backgrounded();
        let within = SUSPECT_AFTER + PING_WITHIN + Duration::from_secs(5);
        let request = Request::Skills { project_id: "p".to_string() };
        let _ = tokio::time::timeout(within, link.request(&request)).await.expect("the request waited forever");
        let dialing = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some((_, event)) = heard.recv().await {
                if let LinkEvent::Status(status) = event
                    && status.state != State::Connected
                {
                    return status;
                }
            }
            panic!("the link stopped");
        })
        .await
        .expect("the link still says it is connected");

        assert_eq!(dialing, Status { state: State::Connecting, ..Status::default() });
        link.shutdown();
    }

    #[tokio::test]
    async fn a_server_that_never_sends_its_threads_is_dialed_again() {
        let (_server, address) = silent_server().await;
        let endpoint = bind(&DeviceKey::generate(), true).await.unwrap();
        let (events, mut heard) = mpsc::unbounded_channel();
        let link = Link::connect(endpoint, address, Away::new(), events);

        let given_up = tokio::time::timeout(WELCOME_TIMEOUT + Duration::from_secs(10), async {
            while let Some((_, event)) = heard.recv().await {
                if let LinkEvent::Status(status) = event
                    && status.state == State::Disconnected
                {
                    return status;
                }
            }
            panic!("the link stopped");
        })
        .await
        .expect("the link waited for the threads forever");

        assert_eq!(given_up.error.as_deref(), Some("Your server didn't send its threads in time."));
        link.shutdown();
    }
}
