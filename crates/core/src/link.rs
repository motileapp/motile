//! Keeps the app connected to one server: reconnects when the connection drops, and follows the
//! thread list and the threads the app has open, catching each up from the revision it had.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_lite::StreamExt;
use iroh::Endpoint;
use iroh::endpoint::PathEvent;
use motile_protocol::wire::{FileKind, GitStage, Message, Request};
use serde::Serialize;
use tokio::sync::{Notify, mpsc};
use tokio::task::AbortHandle;

use crate::connection::{Closed, Connection, PathKind, ServerAddr};

const RETRY_DELAYS: [Duration; 4] =
    [Duration::from_millis(300), Duration::from_secs(1), Duration::from_secs(2), Duration::from_secs(5)];
const RTT_REFRESH: Duration = Duration::from_secs(5);
/// How long to wait before asking again a server that turned this device away.
const REFUSED_RETRY: Duration = Duration::from_secs(10);

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
    endpoint: Endpoint,
    events: mpsc::UnboundedSender<(String, LinkEvent)>,
    inner: Mutex<Inner>,
    /// Ends the wait before the next dial.
    wake: Notify,
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
    /// The connection was closed here to dial again, so its end is no failure to tell about.
    redialing: bool,
}

struct OpenThread {
    /// The revision the app has everything up to; a reconnect asks for what came after.
    synced: u64,
    follower: Option<AbortHandle>,
}

impl Link {
    pub fn connect(
        endpoint: Endpoint,
        server: ServerAddr,
        events: mpsc::UnboundedSender<(String, LinkEvent)>,
    ) -> Arc<Self> {
        let link = Arc::new(Self {
            server_id: server.key.clone(),
            endpoint,
            events,
            inner: Mutex::default(),
            wake: Notify::new(),
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

    /// Lets go of the connection and dials again at once. For when the app comes back after a
    /// time in which the system may have cut the connection without saying so.
    pub fn redial(&self) {
        let connection = {
            let mut inner = self.lock();
            inner.redialing = inner.connection.is_some();
            inner.connection.clone()
        };
        if let Some(connection) = connection {
            connection.close();
        }
        self.wake.notify_one();
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
            let closed = match Connection::dial(&self.endpoint, &server).await {
                Ok(connection) => {
                    self.attach(&connection);
                    let closed = connection.closed().await;
                    self.drop_connection();
                    closed
                }
                Err(error) => Closed { refused: false, reason: format!("{error:#}") },
            };

            if std::mem::take(&mut self.lock().redialing) {
                failures = 0;
                continue;
            }

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
            self.update_status(|status| {
                *status = Status {
                    state: if closed.refused { State::Refused } else { State::Disconnected },
                    error: Some(closed.reason),
                    ..Status::default()
                }
            });
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = self.wake.notified() => failures = 0,
            }
        }
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
    }

    async fn follow_list(self: Arc<Self>, connection: Connection) {
        // The stream ends when the app fell behind; following again starts from a fresh list.
        while let Ok(mut follow) = connection.follow(&Request::Subscribe).await {
            while let Ok(Some(message)) = follow.next().await {
                let welcomed = matches!(message, Message::Welcome { .. });
                self.emit(LinkEvent::List(message));
                if !welcomed {
                    continue;
                }
                // Said after the list itself, so whoever hears "connected" already has it.
                let path = connection.path();
                self.update_status(|status| {
                    status.state = State::Connected;
                    status.error = None;
                    status.path = path.map(|(kind, _)| kind);
                    status.rtt_ms = path.map(|(_, rtt)| rtt.as_millis() as u64);
                });
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
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
            self.update_status(|status| {
                status.path = Some(kind);
                status.rtt_ms = Some(rtt.as_millis() as u64);
            });
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

    fn connection(&self) -> anyhow::Result<Connection> {
        let connection = self.lock().connection.clone();
        connection.ok_or_else(|| anyhow::anyhow!("Your server isn't connected. Try again when it is back."))
    }

    pub async fn request(&self, request: &Request) -> anyhow::Result<Message> {
        match self.connection()?.request(request).await? {
            Message::Error { message } => Err(anyhow::anyhow!(message)),
            message => Ok(message),
        }
    }

    /// Has the server install the latest release, telling `progress` how far the download is.
    pub async fn update(&self, mut progress: impl FnMut(u64, Option<u64>)) -> anyhow::Result<()> {
        let mut follow = self.connection()?.follow(&Request::UpdateServer).await?;
        loop {
            // A server from before it could update itself closes the stream without an answer.
            let Some(message) = follow.next().await? else {
                anyhow::bail!("This server is too old to update itself. Run the install command on its machine again.");
            };
            match message {
                Message::Updating { received, total } => progress(received, total),
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
        let mut follow = self.connection()?.follow(request).await?;
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
        progress: impl FnMut(u64, u64),
    ) -> anyhow::Result<(String, Option<String>)> {
        self.connection()?.upload(path, poster_of, progress).await
    }

    pub async fn file(&self, request: &Request) -> anyhow::Result<(FileKind, u64, Vec<u8>)> {
        self.connection()?.file(request).await
    }

    pub async fn media(&self, id: &str, file: &Path, progress: impl FnMut(u64, u64)) -> anyhow::Result<()> {
        self.connection()?.media(id, file, progress).await
    }
}
