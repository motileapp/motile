//! The core's one loop. Commands from the client and events from the servers' links arrive on a single
//! channel and are handled in turn, so the state needs no locks. Anything that waits on the
//! network runs in a task of its own and reports back through the same channel.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use iroh::Endpoint;
use motile_protocol::auth_api::{Device, DeviceKind, Me};
use motile_protocol::auth_client::{AuthClient, DeviceDescription};
use motile_protocol::identity::{DeviceKey, random_token};
use motile_protocol::now;
use motile_protocol::wire::{
    Activity, AgentLimits, ContinueSettings, FileKind, Item, ItemKind, Message, ModelInfo, Project,
    PullRequestSettings, Request, ServerInfo, Thread, ToolStatus, Worktree,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use crate::api::{AccountView, Command, Config, Event, ProjectView, ServerView, ThreadView};
use crate::browse;
use crate::cache::{Cache, Page};
use crate::connection::{ServerAddr, bind};
use crate::follow::{self, Followed};
use crate::git;
use crate::limits;
use crate::linear;
use crate::link::{Link, LinkEvent, State, Status};
use crate::media::{self, MediaCache};
use crate::pull_request;
use crate::render::agents;
use crate::render::diff;
use crate::render::highlight::{self, Spans};
use crate::render::rows::{Splice, Transcript, Uncoloured};
use crate::usage;

/// The newest items are rendered and sent first, so a long thread opens at once.
const FIRST_ITEMS: usize = 30;
/// Streamed text is rendered at most this often.
const RENDER_EVERY: Duration = Duration::from_millis(33);
const SAVE_EVERY: Duration = Duration::from_secs(1);
const TICK: Duration = Duration::from_secs(2);
/// How long a server's update is still shown while the server can't be reached to confirm it.
const UNCONFIRMED_UPDATE: Duration = Duration::from_secs(60);
/// How often the client is told how far a download is.
const PROGRESS_EVERY: Duration = Duration::from_millis(200);
/// Where the images among a server's files are kept while they are shown, in the data folder.
const SHOWN_FILES: &str = "files";
/// How many ticks pass between account checks when nobody is waiting for a server.
const ACCOUNT_CHECK_TICKS: u64 = 30;
/// How often the client looks for limits that have grown old, about once a minute.
const LIMITS_CHECK_TICKS: u64 = 30;
/// The servers before this one don't keep what their agents spend.
const USAGE_PROTOCOL: u32 = 10;
/// The servers before this one can't say what the agents' logins have used of their plans.
const LIMITS_PROTOCOL: u32 = 17;
/// What a server said of its agents' logins is read again once it is this old.
const LIMITS_STALE: Duration = Duration::from_secs(5 * 60);
/// An endpoint that didn't run for this long isn't trusted to reach anything anymore.
const STALE_AFTER: Duration = Duration::from_secs(10);
/// An endpoint that reached no server for this long is replaced, in case the fault is its own.
const WEDGED_AFTER: Duration = Duration::from_secs(60);

pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

#[derive(Clone)]
pub struct Handle {
    inputs: mpsc::UnboundedSender<Input>,
}

impl Handle {
    pub fn send(&self, id: u64, command: Command) {
        let _ = self.inputs.send(Input::Command { id, command });
    }

    /// Disconnects from the servers and ends the core. What it knows is already in the cache.
    pub fn stop(&self) {
        let _ = self.inputs.send(Input::Stop);
    }
}

enum Input {
    Command {
        id: u64,
        command: Command,
    },
    Link(String, LinkEvent),
    Endpoint {
        key: String,
        result: Result<Endpoint, String>,
    },
    AccountChecked {
        key: String,
        result: Result<Me, String>,
    },
    SignedIn {
        id: u64,
        result: Result<Me, String>,
    },
    Highlighted {
        thread_id: String,
        row_id: String,
        para: Option<usize>,
        code: String,
        spans: Spans,
    },
    IconFetched {
        server_id: String,
    },
    /// The server took the model that writes its titles and commit messages.
    TextModelSet {
        server_id: String,
        model: Option<String>,
    },
    /// The server took what it is to do with pull requests by itself.
    PullRequestSettingsSet {
        server_id: String,
        settings: PullRequestSettings,
    },
    /// The server took what it is to go on with by itself.
    ContinueSettingsSet {
        server_id: String,
        settings: ContinueSettings,
    },
    /// The server took the instructions for naming its branches, or went back to its own.
    BranchInstructionsSet {
        server_id: String,
        instructions: Option<String>,
    },
    MediaFetched {
        id: String,
        result: Result<String, String>,
    },
    Uploaded {
        key: String,
        result: Result<Value, String>,
    },
    /// What to show again at once next time, kept in the cache.
    Keep {
        key: String,
        value: Value,
    },
    Render,
    Tick,
    Stop,
}

struct Server {
    device: Device,
    link: Option<Arc<Link>>,
    status: Status,
    /// Since when the server hasn't been reached, while it isn't.
    away_since: Option<Instant>,
    info: Option<ServerInfo>,
    threads: HashMap<String, Thread>,
}

struct OpenThread {
    server_id: String,
    transcript: Transcript,
    /// Whether the thread has turns before the ones in the transcript. They are in the cache.
    earlier: bool,
    /// Whether the catch-up is done and updates now arrive in revision order.
    live: bool,
    rev: u64,
    /// The tool calls that started an agent, in order.
    agents: Vec<Item>,
    /// The agent whose transcript the client shows: the tool call that started it, and what it did.
    agent: Option<(String, Transcript)>,
    /// Items whose streamed text hasn't been rendered yet.
    unrendered: HashSet<String>,
    /// Items whose streamed text isn't in the cache yet.
    unsaved: HashSet<String>,
    saved_at: Instant,
}

struct Core {
    config: Config,
    sink: EventSink,
    inputs: mpsc::UnboundedSender<Input>,
    link_events: mpsc::UnboundedSender<(String, LinkEvent)>,
    auth: AuthClient,
    cache: Cache,
    key: Arc<DeviceKey>,
    endpoint: Option<Endpoint>,
    /// When the core last ran. A gap says the system had the client paused: asleep or suspended.
    ran_at: SystemTime,
    /// When the endpoint last had a server answer it, or was opened.
    reached_at: Instant,
    me: Me,
    account_error: Option<String>,
    servers: Vec<Server>,
    open: HashMap<String, OpenThread>,
    /// Active threads that aren't open, kept current in the cache.
    followed: HashMap<String, Followed>,
    /// The secret and the state of the sign-in the browser is busy with.
    pending_sign_in: Option<(String, String)>,
    watch_servers: bool,
    render_scheduled: bool,
    ticks: u64,
    /// The icon files that have been asked for, so none is asked for twice.
    icons_asked: HashSet<String>,
    media: Arc<MediaCache>,
    /// How many bytes the fetched images and videos may take.
    media_limit: u64,
    /// The commands waiting for each image or video that is being fetched.
    media_waiting: HashMap<String, Vec<u64>>,
    /// The files on their way to a server, by the key the client gave: the command that waits for
    /// each, and what stops it.
    uploads: HashMap<String, (u64, AbortHandle)>,
    /// The folders last listed for `browse`: of which server and directory, and whether with
    /// the hidden ones.
    browsed: Browsed,
}

type Browsed = Arc<std::sync::Mutex<Option<((String, String, bool), Vec<String>)>>>;

/// Starts the core on the current tokio runtime.
pub fn start(config: Config, sink: EventSink) -> anyhow::Result<Handle> {
    std::fs::create_dir_all(&config.data_dir)
        .with_context(|| format!("{} can't be created.", config.data_dir.display()))?;
    let key = DeviceKey::load_or_create(&key_file(&config))?;
    let cache = Cache::open(&config.data_dir.join("cache.sqlite"))?;

    let (inputs, mut received) = mpsc::unbounded_channel();
    let (link_events, mut from_links) = mpsc::unbounded_channel();
    let forward = inputs.clone();
    tokio::spawn(async move {
        while let Some((server_id, event)) = from_links.recv().await {
            if forward.send(Input::Link(server_id, event)).is_err() {
                break;
            }
        }
    });
    let ticker = inputs.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(TICK).await;
            if ticker.send(Input::Tick).is_err() {
                break;
            }
        }
    });

    // The images of files that were looked at last time.
    let _ = std::fs::remove_dir_all(config.data_dir.join(SHOWN_FILES));
    let mut core = Core {
        auth: AuthClient::new(&config.auth_url),
        me: cache.account().unwrap_or_default(),
        media: Arc::new(MediaCache::new(config.data_dir.join("media"))),
        media_limit: config.media_limit.unwrap_or(media::LIMIT),
        media_waiting: HashMap::new(),
        uploads: HashMap::new(),
        browsed: Browsed::default(),
        config,
        sink,
        inputs: inputs.clone(),
        link_events,
        cache,
        key: Arc::new(key),
        endpoint: None,
        ran_at: SystemTime::now(),
        reached_at: Instant::now(),
        account_error: None,
        servers: Vec::new(),
        open: HashMap::new(),
        followed: HashMap::new(),
        pending_sign_in: None,
        watch_servers: false,
        render_scheduled: false,
        ticks: 0,
        icons_asked: HashSet::new(),
    };
    // Loading the syntax definitions takes a moment; better now than at the first code block.
    tokio::task::spawn_blocking(|| highlight::highlight("rust", "fn main() {}"));
    tokio::spawn(async move {
        core.begin();
        while let Some(input) = received.recv().await {
            if matches!(input, Input::Stop) {
                break;
            }
            core.handle(input);
        }
        core.stop().await;
    });
    Ok(Handle { inputs })
}

fn key_file(config: &Config) -> PathBuf {
    config.data_dir.join("device.key")
}

fn unexpected(answer: &Message) -> String {
    format!("Your server gave an unexpected answer. Update it and try again. It said: {answer:?}")
}

fn error_text(error: anyhow::Error) -> String {
    motile_protocol::error_text(&error)
}

impl Core {
    fn emit(&self, event: Event) {
        (self.sink)(event);
    }

    fn reply(&self, id: u64, result: Result<Value, String>) {
        reply(&self.sink, id, result);
    }

    /// Shows what is known from last time, then goes to find out what is true now.
    fn begin(&mut self) {
        self.emit_account();
        self.sync_servers();
        self.emit(Event::Restored);
        self.bind_endpoint();
        self.check_account();
    }

    fn handle(&mut self, input: Input) {
        match input {
            Input::Command { id, command } => self.command(id, command),
            Input::Link(server_id, event) => self.link_event(&server_id, event),
            Input::Endpoint { key, result } => self.endpoint_bound(&key, result),
            Input::AccountChecked { key, result } => self.account_checked(&key, result),
            Input::SignedIn { id, result } => self.signed_in(id, result),
            Input::Highlighted { thread_id, row_id, para, code, spans } => {
                let Some(open) = self.open.get_mut(&thread_id) else { return };
                let of_agent = open.agent.as_mut().map(|(_, transcript)| transcript);
                let Some(para) = para else {
                    let stored = open.transcript.set_spans(&row_id, &code, spans.clone())
                        || of_agent.is_some_and(|transcript| transcript.set_spans(&row_id, &code, spans.clone()));
                    if stored {
                        self.emit(Event::Spans { thread_id, row_id, spans });
                    }
                    return;
                };
                if let Some(splice) = open.transcript.set_para_spans(&row_id, para, &code, spans.clone()) {
                    return self.emit_rows(&thread_id, false, splice);
                }
                let splice = of_agent.and_then(|transcript| transcript.set_para_spans(&row_id, para, &code, spans));
                if let Some(splice) = splice {
                    self.emit_agent_rows(&thread_id, false, splice);
                }
            }
            Input::TextModelSet { server_id, model } => {
                let Some(server) = self.server_mut(&server_id) else { return };
                let Some(info) = &mut server.info else { return };
                info.text_model = model;
                let info = info.clone();
                self.cache.set_server_info(&server_id, &info);
                self.emit_servers();
            }
            Input::PullRequestSettingsSet { server_id, settings } => {
                let Some(server) = self.server_mut(&server_id) else { return };
                let Some(info) = &mut server.info else { return };
                info.pull_request_settings = settings;
                let info = info.clone();
                self.cache.set_server_info(&server_id, &info);
                self.emit_servers();
            }
            Input::ContinueSettingsSet { server_id, settings } => {
                let Some(server) = self.server_mut(&server_id) else { return };
                let Some(info) = &mut server.info else { return };
                info.continue_settings = settings;
                let info = info.clone();
                self.cache.set_server_info(&server_id, &info);
                self.emit_servers();
            }
            Input::BranchInstructionsSet { server_id, instructions } => {
                let Some(server) = self.server_mut(&server_id) else { return };
                let Some(info) = &mut server.info else { return };
                let naming = &mut info.branch_instructions;
                naming.text = instructions.unwrap_or_else(|| naming.default.clone());
                let info = info.clone();
                self.cache.set_server_info(&server_id, &info);
                self.emit_servers();
            }
            Input::IconFetched { server_id } => {
                let projects = self.cache.projects(&server_id);
                self.emit_projects(&server_id, projects);
            }
            Input::MediaFetched { id, result } => {
                let answer = result.map(|path| json!({ "path": path }));
                for waiting in self.media_waiting.remove(&id).unwrap_or_default() {
                    self.reply(waiting, answer.clone());
                }
            }
            Input::Uploaded { key, result } => {
                let Some((id, _)) = self.uploads.remove(&key) else { return };
                self.reply(id, result);
            }
            Input::Keep { key, value } => self.cache.keep(&key, &value),
            Input::Render => self.render(),
            Input::Stop => {}
            Input::Tick => {
                self.ticks += 1;
                self.replace_stale_endpoint();
                self.replace_wedged_endpoint();
                self.forget_unconfirmed_updates();
                for (thread_id, followed) in &mut self.followed {
                    followed.save(&self.cache, thread_id);
                }
                if self.watch_servers || self.ticks.is_multiple_of(ACCOUNT_CHECK_TICKS) {
                    self.check_account();
                }
                if self.ticks.is_multiple_of(LIMITS_CHECK_TICKS) {
                    let servers: Vec<String> =
                        self.servers.iter().map(|server| server.device.public_key.clone()).collect();
                    for server_id in servers {
                        self.read_limits(&server_id);
                    }
                }
            }
        }
    }

    async fn stop(mut self) {
        for (thread_id, open) in &mut self.open {
            save_streamed(&self.cache, thread_id, open);
        }
        for (thread_id, followed) in &mut self.followed {
            followed.save(&self.cache, thread_id);
        }
        for server in &self.servers {
            if let Some(link) = &server.link {
                link.shutdown();
            }
        }
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close().await;
        }
    }

    // ---- account ----

    fn signed_in_now(&self) -> bool {
        self.me.user.is_some()
    }

    fn emit_account(&self) {
        self.emit(Event::Account {
            account: AccountView {
                signed_in: self.signed_in_now(),
                user: self.me.user.clone(),
                device_key: self.key.public(),
                auth_url: self.auth.base_url().to_string(),
                error: self.account_error.clone(),
            },
        });
    }

    fn device_description(&self) -> (String, String) {
        (self.config.device_name.clone(), self.config.platform.clone())
    }

    fn check_account(&self) {
        if !self.signed_in_now() {
            return;
        }
        let (auth, key, inputs) = (self.auth.clone(), self.key.clone(), self.inputs.clone());
        tokio::spawn(async move {
            let result = auth.me(&key).await.map_err(error_text);
            let _ = inputs.send(Input::AccountChecked { key: key.public(), result });
        });
    }

    fn account_checked(&mut self, key: &str, result: Result<Me, String>) {
        if key != self.key.public() || !self.signed_in_now() {
            return;
        }
        match result {
            Ok(me) if me.user.is_none() => {
                // The device was removed from the account somewhere else.
                self.forget_account();
            }
            Ok(me) => {
                let changed = me != self.me || self.account_error.is_some();
                self.account_error = None;
                if !changed {
                    return;
                }
                self.cache.set_account(&me);
                self.me = me;
                self.emit_account();
                self.sync_servers();
            }
            Err(error) => {
                if self.account_error.as_deref() == Some(&error) {
                    return;
                }
                self.account_error = Some(error);
                self.emit_account();
            }
        }
    }

    fn begin_sign_in(&mut self) -> Value {
        let (verifier, state) = (random_token(), random_token());
        let url = self.auth.sign_in_url(&verifier, &state);
        self.pending_sign_in = Some((verifier, state));
        json!({ "url": url })
    }

    fn complete_sign_in(&mut self, id: u64, url: &str) -> anyhow::Result<()> {
        let parameters = callback_parameters(url);
        let Some((verifier, state)) = self.pending_sign_in.take() else {
            bail!("No sign-in is in progress.");
        };
        if parameters.get("state") != Some(&state) {
            bail!("That answer belongs to another sign-in. Try again.");
        }
        if parameters.contains_key("error") {
            bail!("The sign-in was cancelled.");
        }
        let code = parameters.get("code").context("The sign-in came back without a code.")?.clone();
        self.exchange(id, code, verifier);
        Ok(())
    }

    fn exchange(&self, id: u64, code: String, verifier: String) {
        let (auth, key, inputs) = (self.auth.clone(), self.key.clone(), self.inputs.clone());
        let (name, platform) = self.device_description();
        tokio::spawn(async move {
            let device = DeviceDescription { name: &name, platform: &platform };
            let result = auth.exchange(&key, &code, &verifier, &device).await.map_err(error_text);
            let _ = inputs.send(Input::SignedIn { id, result });
        });
    }

    fn dev_sign_in(&self, id: u64, email: String) {
        let (auth, key, inputs) = (self.auth.clone(), self.key.clone(), self.inputs.clone());
        let (name, platform) = self.device_description();
        tokio::spawn(async move {
            let verifier = random_token();
            let device = DeviceDescription { name: &name, platform: &platform };
            let signed_in = async {
                let code = auth.dev_login(&verifier, &email).await?;
                auth.exchange(&key, &code, &verifier, &device).await
            };
            let _ = inputs.send(Input::SignedIn { id, result: signed_in.await.map_err(error_text) });
        });
    }

    fn signed_in(&mut self, id: u64, result: Result<Me, String>) {
        let me = match result {
            Ok(me) => me,
            Err(error) => return self.reply(id, Err(error)),
        };
        self.cache.set_account(&me);
        self.me = me;
        self.account_error = None;
        self.emit_account();
        self.sync_servers();
        self.reply(id, Ok(json!({})));
    }

    /// Tells the auth server this device is gone, then forgets everything about the account.
    fn sign_out(&mut self) {
        let (auth, key) = (self.auth.clone(), self.key.clone());
        tokio::spawn(async move {
            if let Err(error) = auth.remove_device(&key, "self").await {
                tracing::warn!("couldn't remove this device from the account: {error:#}");
            }
        });
        self.forget_account();
    }

    /// Makes this a device nobody knows: no account, no cache, and a new key, so that it is
    /// signed out even if the auth server couldn't be told.
    fn forget_account(&mut self) {
        self.me = Me::default();
        self.account_error = None;
        self.pending_sign_in = None;
        self.sync_servers();
        self.open.clear();
        self.followed.clear();
        self.cache.clear();
        self.media.clear();
        if let Some(endpoint) = self.endpoint.take() {
            tokio::spawn(async move { endpoint.close().await });
        }
        let _ = std::fs::remove_file(key_file(&self.config));
        match DeviceKey::load_or_create(&key_file(&self.config)) {
            Ok(key) => self.key = Arc::new(key),
            Err(error) => tracing::error!("couldn't make a new device key: {error}"),
        }
        self.emit_account();
        self.bind_endpoint();
    }

    // ---- servers ----

    /// Has the endpoint look at the network again and the links that wait to dial do it now.
    fn network_changed(&self) {
        if let Some(endpoint) = self.endpoint.clone() {
            tokio::spawn(async move { endpoint.network_change().await });
        }
        for link in self.servers.iter().filter_map(|server| server.link.as_ref()) {
            link.retry_now();
        }
    }

    /// Opens a new endpoint for the links once the client ran again after a pause. The system
    /// cuts what a paused endpoint had open without a word, and the endpoint can go on dialing
    /// through it for long.
    fn replace_stale_endpoint(&mut self) {
        let now = SystemTime::now();
        let paused = now.duration_since(self.ran_at).unwrap_or_default();
        self.ran_at = now;
        if paused < STALE_AFTER || self.endpoint.is_none() {
            return;
        }
        self.bind_endpoint();
    }

    /// Opens a new endpoint when the one there is has long reached none of the servers. They may
    /// all be down, but an endpoint can also get stuck for good, and only a new one tells.
    fn replace_wedged_endpoint(&mut self) {
        let reaches =
            self.servers.iter().any(|server| matches!(server.status.state, State::Connected | State::Refused));
        if reaches || self.servers.is_empty() || self.endpoint.is_none() {
            self.reached_at = Instant::now();
            return;
        }
        if self.reached_at.elapsed() < WEDGED_AFTER {
            return;
        }
        self.reached_at = Instant::now();
        self.bind_endpoint();
    }

    fn bind_endpoint(&self) {
        let (key, inputs, local_only) = (self.key.clone(), self.inputs.clone(), self.config.local_only);
        tokio::spawn(async move {
            let result = bind(&key, local_only).await.map_err(error_text);
            let _ = inputs.send(Input::Endpoint { key: key.public(), result });
        });
    }

    fn endpoint_bound(&mut self, key: &str, result: Result<Endpoint, String>) {
        if key != self.key.public() {
            return;
        }
        match result {
            Ok(endpoint) => {
                for link in self.servers.iter().filter_map(|server| server.link.as_ref()) {
                    link.redial_on(endpoint.clone());
                }
                if let Some(stale) = self.endpoint.replace(endpoint) {
                    tokio::spawn(async move { stale.close().await });
                }
                self.connect_servers();
            }
            Err(error) => tracing::error!("the network endpoint couldn't be opened: {error}"),
        }
    }

    /// Makes the servers match the account's: drops the ones that are gone, adds the new ones.
    fn sync_servers(&mut self) {
        let wanted: Vec<Device> =
            self.me.devices.iter().filter(|device| device.kind == DeviceKind::Server).cloned().collect();
        let (kept, gone): (Vec<Server>, Vec<Server>) = std::mem::take(&mut self.servers)
            .into_iter()
            .partition(|server| wanted.iter().any(|device| device.public_key == server.device.public_key));
        self.servers = kept;
        for server in gone {
            let server_id = server.device.public_key;
            if let Some(link) = server.link {
                link.shutdown();
            }
            self.open.retain(|_, open| open.server_id != server_id);
            self.followed.retain(|_, followed| followed.server_id != server_id);
            self.cache.remove_server(&server_id);
            self.emit(Event::Threads { server_id: server_id.clone(), threads: Vec::new() });
            self.emit_projects(&server_id, Vec::new());
        }

        let mut added = Vec::new();
        for device in wanted {
            if let Some(server) = self.servers.iter_mut().find(|server| server.device.public_key == device.public_key) {
                server.device = device;
                continue;
            }
            let server_id = device.public_key.clone();
            let cached = self.cache.threads(&server_id);
            let views: Vec<ThreadView> = cached
                .iter()
                .map(|cached| ThreadView {
                    unread: is_unread(&cached.thread, cached.seen_at),
                    thread: cached.thread.clone(),
                    server_id: server_id.clone(),
                })
                .collect();
            let threads = cached.into_iter().map(|cached| (cached.thread.id.clone(), cached.thread)).collect();
            self.servers.push(Server {
                device,
                link: None,
                status: Status::default(),
                away_since: Some(Instant::now()),
                info: self.cache.server_info(&server_id),
                threads,
            });
            added.push((server_id, views));
        }
        self.connect_servers();
        self.emit_servers();
        // What the cache remembers of the new servers, until they answer themselves.
        for (server_id, threads) in added {
            let projects = self.cache.projects(&server_id);
            self.emit_projects(&server_id, projects);
            self.emit(Event::Threads { server_id, threads });
        }
    }

    fn connect_servers(&mut self) {
        let Some(endpoint) = &self.endpoint else { return };
        let direct = self.config.direct_addr.as_deref().and_then(|address| address.parse().ok());
        for server in self.servers.iter_mut().filter(|server| server.link.is_none()) {
            let server_id = server.device.public_key.clone();
            let address = ServerAddr { key: server_id.clone(), direct };
            let link = Link::connect(endpoint.clone(), address, self.link_events.clone());
            // Threads the client opened before there was a connection to follow them on.
            for (thread_id, open) in self.open.iter().filter(|(_, open)| open.server_id == server_id) {
                link.open(thread_id.clone(), open.rev);
            }
            for (thread_id, followed) in self.followed.iter().filter(|(_, followed)| followed.server_id == server_id) {
                link.open(thread_id.clone(), followed.rev);
            }
            server.link = Some(link);
        }
    }

    fn emit_servers(&self) {
        let view = |server: &Server| ServerView {
            id: server.device.public_key.clone(),
            name: server.device.name.clone(),
            short_name: short_name(&server.device.name),
            platform: server.device.platform.clone(),
            state: server.status.state,
            error: server.status.error.clone(),
            path: server.status.path,
            rtt_ms: server.status.rtt_ms,
            info: server.info.clone(),
            short_model_names: server.info.iter().flat_map(|info| &info.models).filter_map(short_model_name).collect(),
        };
        self.emit(Event::Servers { servers: self.servers.iter().map(view).collect() });
    }

    /// Forgets the update of a server that has been away too long for it to still be under way.
    fn forget_unconfirmed_updates(&mut self) {
        let mut forgotten = Vec::new();
        for server in &mut self.servers {
            let away_too_long = server.away_since.is_some_and(|since| since.elapsed() > UNCONFIRMED_UPDATE);
            let Some(info) = server.info.as_mut().filter(|info| away_too_long && info.update.is_some()) else {
                continue;
            };
            info.update = None;
            forgotten.push((server.device.public_key.clone(), info.clone()));
        }
        if forgotten.is_empty() {
            return;
        }
        for (server_id, info) in &forgotten {
            self.cache.set_server_info(server_id, info);
        }
        self.emit_servers();
    }

    fn server_mut(&mut self, server_id: &str) -> Option<&mut Server> {
        self.servers.iter_mut().find(|server| server.device.public_key == server_id)
    }

    fn link(&self, server_id: &str) -> Result<Arc<Link>, String> {
        let server = self.servers.iter().find(|server| server.device.public_key == server_id);
        server
            .and_then(|server| server.link.clone())
            .ok_or_else(|| "That server isn't connected. Try again when it is back.".to_string())
    }

    /// Tells the client about a server's projects, each with its icon if this device has the file.
    /// Icons it doesn't have yet are fetched, and the projects are told again when they arrive.
    fn emit_projects(&mut self, server_id: &str, projects: Vec<Project>) {
        let folder = self.config.data_dir.join("icons");
        let view = |project: Project| {
            let file = project.icon.as_ref().map(|icon| folder.join(format!("{}-{icon}", project.id)));
            let icon_path = file.filter(|file| file.is_file()).map(|file| file.to_string_lossy().into_owned());
            let git_control = project.git.as_ref().map(git::control);
            let controlled = |worktree: &Worktree| Some((worktree.path.clone(), git::control(worktree.git.as_ref()?)));
            let worktree_controls = project.worktrees.iter().filter_map(controlled).collect();
            ProjectView { project, icon_path, git_control, worktree_controls }
        };
        let views: Vec<ProjectView> = projects.into_iter().map(view).collect();
        for view in views.iter().filter(|view| view.icon_path.is_none()) {
            self.fetch_icon(server_id, &view.project);
        }
        self.emit(Event::Projects { server_id: server_id.to_string(), projects: views });
    }

    fn fetch_icon(&mut self, server_id: &str, project: &Project) {
        let Some(icon) = &project.icon else { return };
        let Ok(link) = self.link(server_id) else { return };
        let name = format!("{}-{icon}", project.id);
        if !self.icons_asked.insert(name.clone()) {
            return;
        }
        let folder = self.config.data_dir.join("icons");
        let (inputs, server_id, project_id) = (self.inputs.clone(), server_id.to_string(), project.id.clone());
        tokio::spawn(async move {
            let fetched = async {
                let request = Request::ProjectIcon { project_id: project_id.clone() };
                let Message::Icon { data } = link.request(&request).await? else {
                    bail!("Your server didn't answer with an icon.");
                };
                save_icon(&folder, &project_id, &name, &BASE64.decode(data)?)
            };
            match fetched.await {
                Ok(()) => drop(inputs.send(Input::IconFetched { server_id })),
                Err(error) => tracing::debug!(project_id, "couldn't fetch a project's icon: {error:#}"),
            }
        });
    }

    /// Answers with where the image or video is on this device, fetching it first if it isn't.
    fn find_media(&mut self, id: u64, server_id: &str, media_id: String) {
        if let Some(file) = self.media.get(&media_id) {
            return self.reply(id, Ok(json!({ "path": file.to_string_lossy() })));
        }
        if let Some(waiting) = self.media_waiting.get_mut(&media_id) {
            return waiting.push(id);
        }
        let Some(unfinished) = self.media.unfinished(&media_id) else {
            return self.reply(id, Err("That isn't the name of an image or a video.".to_string()));
        };
        let link = match self.link(server_id) {
            Ok(link) => link,
            Err(error) => return self.reply(id, Err(error)),
        };
        self.media_waiting.insert(media_id.clone(), vec![id]);
        let (cache, inputs, sink) = (self.media.clone(), self.inputs.clone(), self.sink.clone());
        let limit = self.media_limit;
        tokio::spawn(async move {
            let mut told = Instant::now();
            let progress = |received, size| {
                if told.elapsed() < PROGRESS_EVERY {
                    return;
                }
                told = Instant::now();
                sink(Event::MediaProgress { id: media_id.clone(), received, size });
            };
            let fetched = async {
                link.media(&media_id, &unfinished, progress).await?;
                let file = media::finish(&unfinished)?;
                cache.trim(limit);
                anyhow::Ok(file.to_string_lossy().into_owned())
            };
            let result = fetched.await.map_err(error_text);
            if result.is_err() {
                let _ = std::fs::remove_file(&unfinished);
            }
            let _ = inputs.send(Input::MediaFetched { id: media_id, result });
        });
    }

    /// Sends a file to the server and answers with its path there. An image or a video is kept
    /// on this device too, so it shows without being fetched back.
    fn upload(&mut self, id: u64, server_id: &str, key: String, file: String, poster_of: Option<String>) {
        let link = match self.link(server_id) {
            Ok(link) => link,
            Err(error) => return self.reply(id, Err(error)),
        };
        let (cache, inputs, sink, told_key) = (self.media.clone(), self.inputs.clone(), self.sink.clone(), key.clone());
        let limit = self.media_limit;
        let upload = tokio::spawn(async move {
            let key = &key;
            let mut told = Instant::now();
            let progress = |sent, size| {
                if told.elapsed() < PROGRESS_EVERY && sent < size {
                    return;
                }
                told = Instant::now();
                sink(Event::UploadProgress { key: key.clone(), sent, size });
            };
            let file = PathBuf::from(file);
            let sent = async {
                let (path, media_id) = link.upload(&file, poster_of, progress).await?;
                if let Some(media_id) = media_id.clone() {
                    tokio::task::spawn_blocking(move || {
                        cache.keep(&media_id, &file);
                        cache.trim(limit);
                    })
                    .await?;
                }
                anyhow::Ok(json!({ "path": path, "media": media_id }))
            };
            let result = sent.await.map_err(error_text);
            let _ = inputs.send(Input::Uploaded { key: key.clone(), result });
        });
        if let Some((waiting, replaced)) = self.uploads.insert(told_key, (id, upload.abort_handle())) {
            replaced.abort();
            self.reply(waiting, Err("The upload was stopped.".to_string()));
        }
    }

    fn thread_view(&self, server_id: &str, thread: &Thread) -> ThreadView {
        ThreadView {
            unread: is_unread(thread, self.cache.seen_at(&thread.id)),
            thread: thread.clone(),
            server_id: server_id.to_string(),
        }
    }

    // ---- what the servers say ----

    fn link_event(&mut self, server_id: &str, event: LinkEvent) {
        match event {
            LinkEvent::Status(status) => {
                let refused = status.state == State::Refused;
                let Some(server) = self.server_mut(server_id) else { return };
                server.away_since = match status.state {
                    State::Connected => None,
                    _ => server.away_since.or_else(|| Some(Instant::now())),
                };
                server.status = status;
                self.emit_servers();
                // The server may have been removed from the account, or this device.
                if refused {
                    self.check_account();
                }
            }
            LinkEvent::List(message) => self.list_message(server_id, message),
            LinkEvent::Thread { thread_id, message } if self.open.contains_key(&thread_id) => {
                self.thread_message(&thread_id, message)
            }
            LinkEvent::Thread { thread_id, message } => self.followed_message(&thread_id, message),
        }
    }

    fn list_message(&mut self, server_id: &str, message: Message) {
        match message {
            Message::Welcome { server: info, threads, projects } => {
                self.cache.set_server_info(server_id, &info);
                self.cache.set_threads(server_id, &threads);
                self.cache.set_projects(server_id, &projects);
                let views = threads.iter().map(|thread| self.thread_view(server_id, thread)).collect();
                let Some(server) = self.server_mut(server_id) else { return };
                server.info = Some(info);
                server.threads = threads.into_iter().map(|thread| (thread.id.clone(), thread)).collect();
                // Threads deleted while the client was away are no longer followed.
                let known: HashSet<String> = server.threads.keys().cloned().collect();
                self.open.retain(|thread_id, open| open.server_id != server_id || known.contains(thread_id));
                self.followed
                    .retain(|thread_id, followed| followed.server_id != server_id || known.contains(thread_id));
                self.emit_servers();
                self.emit(Event::Threads { server_id: server_id.to_string(), threads: views });
                self.emit_projects(server_id, projects);
                for thread_id in known {
                    self.refollow(server_id, &thread_id);
                }
                self.read_limits(server_id);
            }
            Message::Server { server: info } => {
                self.cache.set_server_info(server_id, &info);
                let Some(server) = self.server_mut(server_id) else { return };
                server.info = Some(info);
                self.emit_servers();
            }
            Message::Projects { projects } => {
                self.cache.set_projects(server_id, &projects);
                self.emit_projects(server_id, projects);
            }
            Message::ThreadUpsert { thread } => {
                self.cache.upsert_thread(server_id, &thread);
                let view = self.thread_view(server_id, &thread);
                let Some(server) = self.server_mut(server_id) else { return };
                let thread_id = thread.id.clone();
                server.threads.insert(thread_id.clone(), thread);
                self.emit(Event::ThreadUpsert { thread: view });
                self.refollow(server_id, &thread_id);
            }
            Message::ThreadDeleted { thread_id } => {
                self.cache.remove_thread(&thread_id);
                self.open.remove(&thread_id);
                self.followed.remove(&thread_id);
                if let Some(server) = self.server_mut(server_id) {
                    server.threads.remove(&thread_id);
                }
                self.emit(Event::ThreadDeleted { thread_id });
            }
            other => tracing::debug!("unexpected message on the thread list: {other:?}"),
        }
    }

    /// Reads what the server's agents' logins have used, unless that is recent, so that the usage
    /// view opens with it current.
    fn read_limits(&self, server_id: &str) {
        let Some(server) = self.servers.iter().find(|server| server.device.public_key == server_id) else { return };
        let (Some(link), Some(info)) = (server.link.clone(), server.info.as_ref()) else { return };
        if info.protocol < LIMITS_PROTOCOL || self.kept_limits(server_id).is_some_and(|(fresh, _)| fresh) {
            return;
        }
        let (inputs, server_id) = (self.inputs.clone(), server_id.to_string());
        tokio::spawn(async move {
            let Ok(Message::Limits { agents }) = link.request(&Request::Limits { refresh: false }).await else {
                return;
            };
            let _ = inputs.send(keep_limits(&server_id, &agents));
        });
    }

    /// What the server last said of its agents' logins, and whether it is recent.
    fn kept_limits(&self, server_id: &str) -> Option<(bool, Vec<AgentLimits>)> {
        let (at, agents): (f64, Vec<AgentLimits>) = self.cache.kept(&format!("limits:{server_id}"))?;
        Some((now() - at < LIMITS_STALE.as_secs_f64(), agents))
    }

    /// Follows an active thread that isn't open, until its agent is done.
    fn refollow(&mut self, server_id: &str, thread_id: &str) {
        if self.open.contains_key(thread_id) {
            return;
        }
        let Ok(link) = self.link(server_id) else { return };
        let server = self.servers.iter().find(|server| server.device.public_key == server_id);
        let active = server.and_then(|server| server.threads.get(thread_id)).is_some_and(follow::is_active);
        match self.followed.get_mut(thread_id) {
            None if active => {
                let since = self.cache.synced_rev(thread_id);
                let followed = Followed::new(server_id.to_string(), since, false, true);
                self.followed.insert(thread_id.to_string(), followed);
                link.open(thread_id.to_string(), since);
            }
            Some(followed) if !active && followed.is_done() => {
                followed.save(&self.cache, thread_id);
                self.followed.remove(thread_id);
                link.close(thread_id);
            }
            _ => {}
        }
    }

    fn followed_message(&mut self, thread_id: &str, message: Message) {
        let Some(followed) = self.followed.get_mut(thread_id) else { return };
        if !followed.take(&self.cache, thread_id, message) {
            self.followed.remove(thread_id);
            return;
        }
        let server_id = followed.server_id.clone();
        self.refollow(&server_id, thread_id);
    }

    fn thread_message(&mut self, thread_id: &str, message: Message) {
        let Some(open) = self.open.get_mut(thread_id) else { return };
        match message {
            Message::Opened { reset, activity } => {
                open.live = false;
                if reset {
                    open.transcript.clear();
                    open.agents.clear();
                    if let Some((_, transcript)) = &mut open.agent {
                        transcript.clear();
                    }
                    open.earlier = false;
                    open.rev = 0;
                    open.unrendered.clear();
                    open.unsaved.clear();
                }
                self.cache.set_activity(thread_id, &activity);
                let (queued, event) = activity_changed(thread_id, &mut open.transcript, activity);
                if reset {
                    self.cache.clear_items(thread_id);
                    self.emit_rows(thread_id, true, Splice { start: 0, remove: 0, rows: Vec::new() });
                    self.emit_agent_rows(thread_id, true, Splice { start: 0, remove: 0, rows: Vec::new() });
                    self.emit(Event::Agents { thread_id: thread_id.to_string(), agents: Vec::new() });
                }
                if let Some(queued) = queued {
                    self.emit_rows(thread_id, false, queued);
                }
                self.emit(Event::Live { thread_id: thread_id.to_string(), live: false });
                self.emit(event);
            }
            Message::Items { items } => {
                let live = open.live;
                let mut splices = Vec::new();
                let mut agent_splices = Vec::new();
                let mut agents_changed = false;
                for item in &items {
                    open.rev = if live { open.rev.max(item.rev) } else { open.rev };
                    open.unrendered.remove(&item.id);
                    open.unsaved.remove(&item.id);
                    if let Some(agent) = agents::view(item) {
                        agents_changed = true;
                        note_agent(&mut open.agents, item);
                        let shown = open.agent.as_mut().filter(|(id, _)| id == &item.id);
                        let settled = agent.status != ToolStatus::Running;
                        agent_splices.extend(shown.and_then(|(_, transcript)| transcript.set_settled(settled)));
                    }
                    // What an agent did only goes to the cache, unless that agent is being shown.
                    if let Some(parent) = &item.parent {
                        let shown = open.agent.as_mut().filter(|(id, _)| id == parent);
                        agent_splices.extend(shown.and_then(|(_, transcript)| transcript.upsert(item.clone(), false)));
                        continue;
                    }
                    // An item before the loaded turns only goes to the cache.
                    let loaded = !open.earlier || open.transcript.first_seq().is_none_or(|first| item.seq >= first);
                    if !loaded {
                        continue;
                    }
                    splices.extend(open.transcript.upsert(item.clone(), live));
                }
                let synced = live.then_some(open.rev);
                let mut to_save: Vec<&Item> = items.iter().collect();
                // The revision being recorded covers streamed text that hasn't been saved yet.
                let unsaved: Vec<Item> = match live {
                    true => open.unsaved.drain().filter_map(|id| open.transcript.item(&id).cloned()).collect(),
                    false => Vec::new(),
                };
                to_save.extend(&unsaved);
                self.cache.save_items(thread_id, &to_save, synced);
                open.saved_at = Instant::now();
                let agents = agents_changed.then(|| agents_event(thread_id, &open.agents));
                for splice in merge(splices) {
                    self.emit_rows(thread_id, false, splice);
                }
                for splice in merge(agent_splices) {
                    self.emit_agent_rows(thread_id, false, splice);
                }
                if let Some(agents) = agents {
                    self.emit(agents);
                }
            }
            Message::Synced { rev } => {
                open.live = true;
                open.rev = rev;
                self.cache.save_items(thread_id, &[], Some(rev));
                self.emit(Event::Live { thread_id: thread_id.to_string(), live: true });
            }
            Message::TextDelta { id, text, rev } => {
                if !open.transcript.append_text(&id, &text, rev) {
                    return;
                }
                open.rev = rev;
                open.unrendered.insert(id.clone());
                open.unsaved.insert(id);
                self.schedule_render();
            }
            Message::Activity { activity } => {
                if !activity.running {
                    open.transcript.end_streaming();
                }
                self.cache.set_activity(thread_id, &activity);
                let (queued, event) = activity_changed(thread_id, &mut open.transcript, activity);
                if let Some(queued) = queued {
                    self.emit_rows(thread_id, false, queued);
                }
                self.emit(event);
            }
            Message::Error { message } => {
                self.open.remove(thread_id);
                self.emit(Event::ThreadError { thread_id: thread_id.to_string(), message });
            }
            other => tracing::debug!("unexpected message on a thread: {other:?}"),
        }
    }

    fn emit_rows(&self, thread_id: &str, reset: bool, splice: Splice) {
        self.emit(Event::Rows {
            thread_id: thread_id.to_string(),
            reset,
            start: splice.start,
            remove: splice.remove,
            rows: splice.rows,
            earlier: self.open.get(thread_id).is_some_and(|open| open.earlier),
        });
    }

    fn emit_agent_rows(&self, thread_id: &str, reset: bool, splice: Splice) {
        let Some((agent_id, _)) = self.open.get(thread_id).and_then(|open| open.agent.as_ref()) else { return };
        self.emit(Event::AgentRows {
            thread_id: thread_id.to_string(),
            agent_id: agent_id.clone(),
            reset,
            start: splice.start,
            remove: splice.remove,
            rows: splice.rows,
        });
    }

    fn open_agent(&mut self, thread_id: &str, agent_id: &str) {
        let Some(open) = self.open.get_mut(thread_id) else { return };
        let call = open.agents.iter().find(|item| item.id == agent_id);
        let started = call.and_then(agents::view);
        let mut items = self.cache.agent_items(thread_id, agent_id);
        // What the agent was asked to do is the message its transcript starts with.
        if let (Some(call), Some(agent)) = (call, &started)
            && !agent.prompt.is_empty()
        {
            let kind = ItemKind::User { text: agent.prompt.clone(), attachments: Vec::new() };
            items.insert(0, Item { id: format!("{agent_id}/prompt"), kind, ..call.clone() });
        }
        let mut transcript = open.transcript.beside();
        transcript.set_settled(started.is_none_or(|agent| agent.status != ToolStatus::Running));
        transcript.load(items);
        let rows = transcript.rows().to_vec();
        open.agent = Some((agent_id.to_string(), transcript));
        self.emit_agent_rows(thread_id, true, Splice { start: 0, remove: 0, rows });
    }

    fn schedule_render(&mut self) {
        if self.render_scheduled {
            return;
        }
        self.render_scheduled = true;
        let inputs = self.inputs.clone();
        tokio::spawn(async move {
            tokio::time::sleep(RENDER_EVERY).await;
            let _ = inputs.send(Input::Render);
        });
    }

    /// Renders the text that streamed in since the last time, and saves it now and then.
    fn render(&mut self) {
        self.render_scheduled = false;
        let mut events = Vec::new();
        for (thread_id, open) in &mut self.open {
            for id in std::mem::take(&mut open.unrendered) {
                let Some(splice) = open.transcript.refresh(&id) else { continue };
                events.push(Event::Rows {
                    thread_id: thread_id.clone(),
                    reset: false,
                    start: splice.start,
                    remove: splice.remove,
                    rows: splice.rows,
                    earlier: open.earlier,
                });
            }
            if open.unsaved.is_empty() || open.saved_at.elapsed() < SAVE_EVERY {
                continue;
            }
            save_streamed(&self.cache, thread_id, open);
        }
        for event in events {
            self.emit(event);
        }
    }

    // ---- commands ----

    fn command(&mut self, id: u64, command: Command) {
        match command {
            Command::BeginSignIn => {
                let url = self.begin_sign_in();
                self.reply(id, Ok(url));
            }
            Command::CompleteSignIn { url } => {
                if let Err(error) = self.complete_sign_in(id, &url) {
                    self.reply(id, Err(error_text(error)));
                }
            }
            Command::DevSignIn { email } => self.dev_sign_in(id, email),
            Command::SignOut => {
                self.sign_out();
                self.reply(id, Ok(json!({})));
            }
            Command::RefreshAccount => {
                self.check_account();
                self.reply(id, Ok(json!({})));
            }
            Command::Foreground => {
                self.replace_stale_endpoint();
                self.network_changed();
                self.check_account();
                self.reply(id, Ok(json!({})));
            }
            Command::NetworkChanged => {
                self.network_changed();
                self.reply(id, Ok(json!({})));
            }
            Command::WatchServers { on } => {
                self.watch_servers = on;
                if on {
                    self.check_account();
                }
                self.reply(id, Ok(json!({})));
            }
            Command::CreateEnrollToken => {
                let (auth, key, sink) = (self.auth.clone(), self.key.clone(), self.sink.clone());
                tokio::spawn(async move {
                    let token = auth.create_enroll_token(&key).await.map_err(error_text);
                    let token = token.map(|token| {
                        let spans = highlight::command(&token.command);
                        json!({
                            "token": token.token,
                            "command": token.command,
                            "expires_at": token.expires_at,
                            "spans": spans,
                        })
                    });
                    reply(&sink, id, token);
                });
            }
            Command::RemoveServer { server_id } => {
                let (auth, key, sink, inputs) =
                    (self.auth.clone(), self.key.clone(), self.sink.clone(), self.inputs.clone());
                tokio::spawn(async move {
                    let removed = auth.remove_device(&key, &server_id).await.map_err(error_text);
                    let result = auth.me(&key).await.map_err(error_text);
                    let _ = inputs.send(Input::AccountChecked { key: key.public(), result });
                    reply(&sink, id, removed.map(|_| json!({})));
                });
            }
            Command::OpenThread { server_id, thread_id } => {
                let result = self.open_thread(&server_id, &thread_id);
                self.reply(id, result.map(|_| json!({})));
            }
            Command::CloseThread { thread_id } => {
                self.close_thread(&thread_id);
                self.reply(id, Ok(json!({})));
            }
            Command::MarkSeen { thread_id } => {
                self.mark_seen(&thread_id);
                self.reply(id, Ok(json!({})));
            }
            Command::Request { server_id, request } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let answer = link.request(&request).await.map_err(error_text);
                    reply(&sink, id, answer.map(|message| serde_json::to_value(message).unwrap_or_default()));
                });
            }
            Command::Browse { server_id, query } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let server = self.servers.iter().find(|server| server.device.public_key == server_id);
                let home = server.and_then(|server| server.info.as_ref()).map(|info| info.home.clone());
                let Some(typed) = browse::typed(&query, &home.clone().unwrap_or_default()) else {
                    return self.reply(id, Err("Start the path with / or ~/.".to_string()));
                };
                let (sink, browsed) = (self.sink.clone(), self.browsed.clone());
                tokio::spawn(async move {
                    let home = home.unwrap_or_default();
                    // Typing a name narrows the folders that are here already.
                    let key = (server_id, typed.directory.clone(), typed.leaf.starts_with('.'));
                    let known =
                        browsed.lock().unwrap().clone().filter(|(known, _)| *known == key && !typed.leaf.is_empty());
                    let folders = match known {
                        Some((_, folders)) => Ok(folders),
                        None => {
                            let list = Request::ListDir { path: Some(key.1.clone()), icons: false, hidden: key.2 };
                            match link.request(&list).await {
                                Ok(Message::Dir { folders, .. }) => {
                                    *browsed.lock().unwrap() = Some((key, folders.clone()));
                                    Ok(folders)
                                }
                                Ok(other) => Err(format!(
                                    "Your server gave an unexpected answer. Update it and try again. It said: {other:?}"
                                )),
                                Err(error) => Err(error_text(error)),
                            }
                        }
                    };
                    let listing =
                        folders.map(|folders| browse::listing(&typed.directory, &typed.leaf, &folders, &home));
                    reply(&sink, id, listing.map(|listing| serde_json::to_value(listing).unwrap_or_default()));
                });
            }
            Command::Send { server_id, thread_id, new_thread, text, attachments, now } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let sent = async {
                        match link.request(&Request::Send { thread_id, new_thread, text, attachments, now }).await? {
                            Message::Sent { thread_id } => Ok(json!({ "thread_id": thread_id })),
                            other => bail!(
                                "Your server gave an unexpected answer. Update it and try again. It said: {other:?}"
                            ),
                        }
                    };
                    reply(&sink, id, sent.await.map_err(error_text));
                });
            }
            Command::UpdateServer { server_id, when } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let (events, waits) = (sink.clone(), sink.clone());
                    let waiting_id = server_id.clone();
                    let report = move |received, total| {
                        events(Event::ServerUpdate { server_id: server_id.clone(), received, total, waiting: false });
                    };
                    let waiting = move || {
                        let server_id = waiting_id.clone();
                        waits(Event::ServerUpdate { server_id, received: 0, total: None, waiting: true });
                    };
                    let updated = link.update(when, report, waiting).await;
                    reply(&sink, id, updated.map(|_| json!({})).map_err(error_text));
                });
            }
            Command::GitRun { server_id, project_id, action, thread_id, message, paths, new_branch } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let events = sink.clone();
                    let started = |stage| {
                        events(Event::GitProgress {
                            project_id: project_id.clone(),
                            thread_id: thread_id.clone(),
                            stage,
                        })
                    };
                    let request = Request::GitRun {
                        project_id: project_id.clone(),
                        action,
                        thread_id: thread_id.clone(),
                        message,
                        paths,
                        new_branch,
                    };
                    let done = link.git_run(&request, started).await;
                    reply(
                        &sink,
                        id,
                        done.map(|done| serde_json::to_value(done).unwrap_or_default()).map_err(error_text),
                    );
                });
            }
            Command::SetTextModel { server_id, model } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let (sink, inputs) = (self.sink.clone(), self.inputs.clone());
                tokio::spawn(async move {
                    let set = link.request(&Request::SetTextModel { model: model.clone() }).await;
                    if set.is_ok() {
                        let _ = inputs.send(Input::TextModelSet { server_id, model });
                    }
                    reply(&sink, id, set.map(|_| json!({})).map_err(error_text));
                });
            }
            Command::SetPullRequestSettings { server_id, settings } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let (sink, inputs) = (self.sink.clone(), self.inputs.clone());
                tokio::spawn(async move {
                    let request = Request::SetPullRequestSettings {
                        done_on_merge: Some(settings.done_on_merge),
                        remove_merged_worktrees: Some(settings.remove_merged_worktrees),
                    };
                    let set = link.request(&request).await;
                    if set.is_ok() {
                        let _ = inputs.send(Input::PullRequestSettingsSet { server_id, settings });
                    }
                    reply(&sink, id, set.map(|_| json!({})).map_err(error_text));
                });
            }
            Command::SetContinueSettings { server_id, settings } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let (sink, inputs) = (self.sink.clone(), self.inputs.clone());
                tokio::spawn(async move {
                    let request = Request::SetContinueSettings {
                        after_limits: Some(settings.after_limits),
                        after_restarts: Some(settings.after_restarts),
                    };
                    let set = link.request(&request).await;
                    if set.is_ok() {
                        let _ = inputs.send(Input::ContinueSettingsSet { server_id, settings });
                    }
                    reply(&sink, id, set.map(|_| json!({})).map_err(error_text));
                });
            }
            Command::SetBranchInstructions { server_id, instructions } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let instructions = instructions.map(|text| text.trim().to_string()).filter(|text| !text.is_empty());
                let (sink, inputs) = (self.sink.clone(), self.inputs.clone());
                tokio::spawn(async move {
                    let request = Request::SetBranchInstructions { instructions: instructions.clone() };
                    let set = link.request(&request).await;
                    if set.is_ok() {
                        let _ = inputs.send(Input::BranchInstructionsSet { server_id, instructions });
                    }
                    reply(&sink, id, set.map(|_| json!({})).map_err(error_text));
                });
            }
            Command::SetProjectIcon { server_id, project_id, path } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let set = link.request(&Request::SetProjectIcon { project_id, path }).await;
                    reply(&sink, id, set.map(|_| json!({})).map_err(error_text));
                });
            }
            Command::Diff { server_id, project_id, thread_id, scope, path } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let (patch, truncated) = match link.request(&Request::Diff { project_id, thread_id, scope }).await {
                        Ok(Message::Diff { patch, truncated }) => (patch, truncated),
                        Ok(other) => return reply(&sink, id, Err(unexpected(&other))),
                        Err(error) => return reply(&sink, id, Err(error_text(error))),
                    };
                    // Reading and highlighting a long patch takes a while.
                    let _ = tokio::task::spawn_blocking(move || {
                        let mut files = diff::parse(&patch);
                        if let Some(path) = path {
                            files.retain(|file| file.path == path);
                        }
                        reply(&sink, id, Ok(json!({ "files": files, "truncated": truncated })));
                        for (file, changed) in files.iter().enumerate() {
                            let lines = diff::highlight(changed);
                            if lines.iter().any(|spans| !spans.is_empty()) {
                                sink(Event::CodeSpans { id, file, lines });
                            }
                        }
                    })
                    .await;
                });
            }
            Command::PullRequest { server_id, project_id, thread_id, number, method } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let answer = link.request(&Request::PullRequest { project_id, thread_id, number }).await;
                    let detail = match answer {
                        Ok(Message::PullRequest { pull_request }) => pull_request,
                        Ok(other) => return reply(&sink, id, Err(unexpected(&other))),
                        Err(error) => return reply(&sink, id, Err(error_text(error))),
                    };
                    // Its Markdown is parsed and its code highlighted.
                    let view = tokio::task::spawn_blocking(move || pull_request::view(&detail, method)).await;
                    reply(&sink, id, view.map(|view| json!(view)).map_err(|error| error.to_string()));
                });
            }
            Command::PullRequestAction { server_id, project_id, thread_id, number, action, method, text } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let request = Request::PullRequestAction { project_id, thread_id, number, action, method, text };
                    let (title, url, detail) = match link.request(&request).await {
                        Ok(Message::PullRequestDone { title, url, pull_request }) => (title, url, pull_request),
                        Ok(other) => return reply(&sink, id, Err(unexpected(&other))),
                        Err(error) => return reply(&sink, id, Err(error_text(error))),
                    };
                    let view = tokio::task::spawn_blocking(move || pull_request::view(&detail, method)).await;
                    let answer = view.map(|view| json!({ "title": title, "url": url, "view": view }));
                    reply(&sink, id, answer.map_err(|error| error.to_string()));
                });
            }
            Command::PullRequestEdit { server_id, project_id, thread_id, number, edit, method } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let request = Request::PullRequestEdit { project_id, thread_id, number, edit };
                    let (title, detail) = match link.request(&request).await {
                        Ok(Message::PullRequestDone { title, pull_request, .. }) => (title, pull_request),
                        Ok(other) => return reply(&sink, id, Err(unexpected(&other))),
                        Err(error) => return reply(&sink, id, Err(error_text(error))),
                    };
                    let view = tokio::task::spawn_blocking(move || pull_request::view(&detail, method)).await;
                    let answer = view.map(|view| json!({ "title": title, "view": view }));
                    reply(&sink, id, answer.map_err(|error| error.to_string()));
                });
            }
            Command::PullRequests { server_id, project_id, thread_id, state } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let answer = link.request(&Request::PullRequests { project_id, thread_id, state }).await;
                    let answer = match answer {
                        Ok(Message::PullRequests { pull_requests }) => {
                            Ok(json!({ "rows": pull_request::rows(&pull_requests) }))
                        }
                        Ok(other) => Err(unexpected(&other)),
                        Err(error) => Err(error_text(error)),
                    };
                    reply(&sink, id, answer);
                });
            }
            Command::LinearIssue { server_id, workspace, issue, comment } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let request = match comment {
                        Some(body) => Request::LinearComment { workspace, issue, body },
                        None => Request::LinearIssue { workspace, issue },
                    };
                    let detail = match link.request(&request).await {
                        Ok(Message::LinearIssueDetail { detail }) => detail,
                        Ok(other) => return reply(&sink, id, Err(unexpected(&other))),
                        Err(error) => return reply(&sink, id, Err(error_text(error))),
                    };
                    let page = tokio::task::spawn_blocking(move || linear::page(&detail)).await;
                    reply(&sink, id, page.map(|page| json!({ "page": page })).map_err(|error| error.to_string()));
                });
            }
            Command::LinearIssues { server_id, workspace, team, mine, closed, states, search } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let answer =
                        link.request(&Request::LinearIssues { workspace, team, mine, closed, states, search }).await;
                    let answer = match answer {
                        Ok(Message::LinearIssues { issues }) => Ok(json!({ "groups": linear::groups(&issues) })),
                        Ok(other) => Err(unexpected(&other)),
                        Err(error) => Err(error_text(error)),
                    };
                    reply(&sink, id, answer);
                });
            }
            Command::Usage { bucket_secs, buckets, utc_offset_secs, servers, kept } => {
                let key = usage_key(bucket_secs, buckets, servers.as_ref());
                if kept {
                    let last: Option<Value> = self.cache.kept(&key);
                    return self.reply(id, last.ok_or_else(|| "Nothing was kept.".to_string()));
                }
                let window = usage::Window::ending(now(), bucket_secs, buckets, utc_offset_secs);
                let asked = self
                    .servers
                    .iter()
                    .filter(|server| servers.as_ref().is_none_or(|ids| ids.contains(&server.device.public_key)))
                    .filter(|server| server.info.as_ref().is_some_and(|info| info.protocol >= USAGE_PROTOCOL));
                let mut links: Vec<(String, Arc<Link>)> = Vec::new();
                let mut project_names = HashMap::new();
                let mut server_names = HashMap::new();
                for server in asked {
                    let Some(link) = server.link.clone() else { continue };
                    let server_id = server.device.public_key.clone();
                    for project in self.cache.projects(&server_id) {
                        project_names.insert((server_id.clone(), project.id), project.name);
                    }
                    server_names.insert(server_id.clone(), server.device.name.clone());
                    links.push((server_id, link));
                }
                if links.is_empty() {
                    return self.reply(id, Err("None of the servers is connected.".to_string()));
                }
                let request = Request::Usage {
                    since: window.since,
                    until: window.until,
                    bucket_secs: window.bucket_secs,
                    utc_offset_secs: window.utc_offset_secs,
                };
                let mut asked = tokio::task::JoinSet::new();
                for (server_id, link) in links {
                    let request = request.clone();
                    asked.spawn(async move { (server_id, link.request(&request).await) });
                }
                let (sink, inputs) = (self.sink.clone(), self.inputs.clone());
                tokio::spawn(async move {
                    let mut spent = Vec::new();
                    let mut failure = None;
                    while let Some(Ok((server_id, answer))) = asked.join_next().await {
                        match answer {
                            Ok(Message::Usage { buckets }) => {
                                let of_server = |bucket| usage::Spent { server_id: server_id.clone(), bucket };
                                spent.extend(buckets.into_iter().map(of_server));
                            }
                            Ok(other) => failure = Some(unexpected(&other)),
                            Err(error) => failure = Some(error_text(error)),
                        }
                    }
                    let answer = match failure {
                        Some(failure) if spent.is_empty() => Err(failure),
                        _ => serde_json::to_value(usage::view(&spent, window, &project_names, &server_names))
                            .map_err(|error| error.to_string()),
                    };
                    if let Ok(view) = &answer {
                        let _ = inputs.send(Input::Keep { key, value: view.clone() });
                    }
                    reply(&sink, id, answer);
                });
            }
            Command::Limits { refresh, kept, servers } => {
                let chosen = self
                    .servers
                    .iter()
                    .filter(|server| servers.as_ref().is_none_or(|ids| ids.contains(&server.device.public_key)));
                let mut asked = tokio::task::JoinSet::new();
                let mut notes = Vec::new();
                let mut reads = Vec::new();
                let mut stale = false;
                for server in chosen {
                    let Some(info) = server.info.as_ref() else { continue };
                    let name = server.device.name.clone();
                    if info.protocol < LIMITS_PROTOCOL {
                        notes.push(format!("Update {name} to see its limits."));
                        continue;
                    }
                    let server_id = server.device.public_key.clone();
                    let last = self.kept_limits(&server_id);
                    let fresh = last.as_ref().is_some_and(|(fresh, _)| *fresh);
                    let link = server.link.clone().filter(|_| !kept && (refresh || !fresh));
                    let Some(link) = link else {
                        stale |= !fresh;
                        let agents = last.map(|(_, agents)| agents).unwrap_or_default();
                        reads.extend(agents.into_iter().map(|limits| limits::Read { server: name.clone(), limits }));
                        continue;
                    };
                    asked.spawn(async move { (server_id, name, link.request(&Request::Limits { refresh }).await) });
                }
                let (sink, inputs) = (self.sink.clone(), self.inputs.clone());
                tokio::spawn(async move {
                    while let Some(Ok((server_id, server, answer))) = asked.join_next().await {
                        let agents = match answer {
                            Ok(Message::Limits { agents }) => agents,
                            Ok(other) => {
                                notes.push(format!("Couldn't read the limits on {server}. {}", unexpected(&other)));
                                continue;
                            }
                            Err(error) => {
                                notes.push(format!("Couldn't read the limits on {server}. {}", error_text(error)));
                                continue;
                            }
                        };
                        let _ = inputs.send(keep_limits(&server_id, &agents));
                        reads.extend(agents.into_iter().map(|limits| limits::Read { server: server.clone(), limits }));
                    }
                    reads.sort_by(|a, b| a.server.cmp(&b.server));
                    let sections = limits::sections(reads, now());
                    reply(&sink, id, Ok(json!({ "sections": sections, "notes": notes, "stale": stale })));
                });
            }
            Command::Markdown { text } => {
                let sink = self.sink.clone();
                tokio::task::spawn_blocking(move || {
                    reply(&sink, id, Ok(json!({ "blocks": pull_request::text(&text) })))
                });
            }
            Command::LinePrompt { number, url, head, path, line, code, note } => {
                let prompt = pull_request::line_prompt(number, &url, &head, &path, line, &code, &note);
                self.reply(id, Ok(json!({ "prompt": prompt })));
            }
            Command::File { server_id, project_id, thread_id, path, blob } => {
                let shown = shown_file(&self.config.data_dir.join(SHOWN_FILES), &server_id, &path, blob.as_deref());
                // A blob is the same file whenever it is asked for.
                if let Some(kind) = blob.as_ref().and_then(|_| FileKind::shown(&path))
                    && let Ok(kept) = std::fs::metadata(&shown)
                {
                    return self.reply(id, Ok(json!({ "kind": kind, "size": kept.len(), "file": shown })));
                }
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let request = Request::ReadFile { project_id, thread_id, path: path.clone(), blob };
                    let unfinished = shown.with_added_extension(format!("{id}.part"));
                    let read = async {
                        std::fs::create_dir_all(shown.parent().context("Files are kept in a folder.")?)?;
                        link.file(&request, &unfinished).await
                    };
                    let (kind, size, bytes) = match read.await {
                        Ok(file) => file,
                        Err(error) => {
                            let _ = std::fs::remove_file(&unfinished);
                            return reply(&sink, id, Err(error_text(error)));
                        }
                    };
                    let _ = tokio::task::spawn_blocking(move || match kind {
                        FileKind::Binary => reply(&sink, id, Ok(json!({ "kind": kind, "size": size }))),
                        FileKind::Image | FileKind::Video => {
                            let kept = std::fs::rename(&unfinished, &shown).map_err(|error| error.to_string());
                            reply(&sink, id, kept.map(|_| json!({ "kind": kind, "size": size, "file": shown })));
                        }
                        FileKind::Text => {
                            let truncated = (bytes.len() as u64) < size;
                            let lines = diff::lines_of(&String::from_utf8_lossy(&bytes), truncated);
                            let answer = json!({ "kind": kind, "size": size, "lines": lines, "truncated": truncated });
                            reply(&sink, id, Ok(answer));
                            let lines = diff::highlight_lines(&path, &lines);
                            if lines.iter().any(|spans| !spans.is_empty()) {
                                sink(Event::CodeSpans { id, file: 0, lines });
                            }
                        }
                    })
                    .await;
                });
            }
            Command::Upload { server_id, key, file, poster_of } => self.upload(id, &server_id, key, file, poster_of),
            Command::CancelUpload { key } => {
                if let Some((waiting, upload)) = self.uploads.remove(&key) {
                    upload.abort();
                    self.reply(waiting, Err("The upload was stopped.".to_string()));
                }
                self.reply(id, Ok(json!({})));
            }
            Command::Media { server_id, media_id } => self.find_media(id, &server_id, media_id),
            Command::Storage => {
                let (media, sink, limit) = (self.media.clone(), self.sink.clone(), self.media_limit);
                tokio::task::spawn_blocking(move || {
                    reply(&sink, id, Ok(json!({ "media_bytes": media.size(), "media_limit": limit })));
                });
            }
            Command::ClearMedia => {
                self.media.clear();
                self.reply(id, Ok(json!({})));
            }
            Command::Highlight { thread_id, row_ids } => {
                self.highlight(&thread_id, &row_ids);
                self.reply(id, Ok(json!({})));
            }
            Command::ToggleRow { thread_id, row_id } => {
                let splice = self.open.get_mut(&thread_id).and_then(|open| open.transcript.toggle(&row_id));
                if let Some(splice) = splice {
                    self.emit_rows(&thread_id, false, splice);
                }
                let of_agent = self.open.get_mut(&thread_id).and_then(|open| open.agent.as_mut());
                if let Some(splice) = of_agent.and_then(|(_, transcript)| transcript.toggle(&row_id)) {
                    self.emit_agent_rows(&thread_id, false, splice);
                }
                self.reply(id, Ok(json!({})));
            }
            Command::OpenAgent { thread_id, agent_id } => {
                self.open_agent(&thread_id, &agent_id);
                self.reply(id, Ok(json!({})));
            }
            Command::CloseAgent { thread_id } => {
                if let Some(open) = self.open.get_mut(&thread_id) {
                    open.agent = None;
                }
                self.reply(id, Ok(json!({})));
            }
            Command::LoadEarlier { thread_id } => {
                self.load_earlier(&thread_id);
                self.reply(id, Ok(json!({})));
            }
            Command::TrimEarlier { thread_id, keep_rows } => {
                self.trim_earlier(&thread_id, keep_rows);
                self.reply(id, Ok(json!({})));
            }
        }
    }

    /// The turns let go stay in the cache, once their streamed text is there.
    fn trim_earlier(&mut self, thread_id: &str, keep_rows: usize) {
        let Some(open) = self.open.get_mut(thread_id) else { return };
        save_streamed(&self.cache, thread_id, open);
        let Some(splice) = open.transcript.trim(keep_rows) else { return };
        open.earlier = true;
        self.emit_rows(thread_id, false, splice);
    }

    fn load_earlier(&mut self, thread_id: &str) {
        let Some(open) = self.open.get_mut(thread_id).filter(|open| open.earlier) else { return };
        let page = self.cache.page(thread_id, open.transcript.first_seq());
        open.earlier = page.earlier;
        let splice = open.transcript.prepend(page.items).unwrap_or(Splice { start: 0, remove: 0, rows: Vec::new() });
        self.emit_rows(thread_id, false, splice);
    }

    fn open_thread(&mut self, server_id: &str, thread_id: &str) -> Result<(), String> {
        if let Some(open) = self.open.get(thread_id) {
            let rows = open.transcript.rows().to_vec();
            let agents = agents_event(thread_id, &open.agents);
            let live = open.live;
            self.emit_rows(thread_id, true, Splice { start: 0, remove: 0, rows });
            self.emit(Event::Live { thread_id: thread_id.to_string(), live });
            self.emit(agents);
            return Ok(());
        }
        let server =
            self.servers.iter().find(|server| server.device.public_key == server_id).ok_or("That server is gone.")?;
        let thread = server.threads.get(thread_id).cloned();
        // A followed thread's stream carries on, and the cache it saves to is where the thread opens from.
        let followed = self.followed.remove(thread_id);
        let live = followed.is_some_and(|mut followed| {
            followed.save(&self.cache, thread_id);
            followed.live
        });
        let cwd = thread.as_ref().map(|thread| thread.cwd.clone()).unwrap_or_default();
        let link = server.link.clone();

        let Page { mut items, earlier } = self.cache.page(thread_id, None);
        let since = self.cache.synced_rev(thread_id);
        let newest = items.split_off(items.len().saturating_sub(FIRST_ITEMS));
        let mut transcript = Transcript::new(&cwd);
        transcript.load(newest);
        let rows = |reset, splice: Splice| Event::Rows {
            thread_id: thread_id.to_string(),
            reset,
            start: splice.start,
            remove: splice.remove,
            rows: splice.rows,
            earlier,
        };
        self.emit(rows(true, Splice { start: 0, remove: 0, rows: transcript.rows().to_vec() }));
        if !items.is_empty()
            && let Some(rest) = transcript.prepend(items)
        {
            self.emit(rows(false, rest));
        }
        // What the agent was last seen doing, until the server says what it does now.
        if let Some(thread) = thread {
            let activity = restored_activity(&thread, self.cache.activity(thread_id));
            let (queued, event) = activity_changed(thread_id, &mut transcript, activity);
            if let Some(queued) = queued {
                self.emit(rows(false, queued));
            }
            self.emit(event);
        }

        let agents = self.cache.agents(thread_id);
        self.emit(agents_event(thread_id, &agents));
        let open = OpenThread {
            server_id: server_id.to_string(),
            transcript,
            agents,
            agent: None,
            earlier,
            live,
            rev: since,
            unrendered: HashSet::new(),
            unsaved: HashSet::new(),
            saved_at: Instant::now(),
        };
        self.open.insert(thread_id.to_string(), open);
        if live {
            self.emit(Event::Live { thread_id: thread_id.to_string(), live });
        }
        if let Some(link) = link {
            link.open(thread_id.to_string(), since);
        }
        Ok(())
    }

    fn close_thread(&mut self, thread_id: &str) {
        let Some(mut open) = self.open.remove(thread_id) else { return };
        save_streamed(&self.cache, thread_id, &mut open);
        let server = self.servers.iter().find(|server| server.device.public_key == open.server_id);
        let active = server.and_then(|server| server.threads.get(thread_id)).is_some_and(follow::is_active);
        if active {
            let working = self.cache.activity(thread_id).is_some_and(|activity| follow::is_working(&activity));
            let followed = Followed::new(open.server_id, open.rev, open.live, working);
            self.followed.insert(thread_id.to_string(), followed);
            return;
        }
        if let Ok(link) = self.link(&open.server_id) {
            link.close(thread_id);
        }
    }

    fn mark_seen(&mut self, thread_id: &str) {
        self.cache.set_seen(thread_id, now());
        let found = self.servers.iter().find_map(|server| {
            server.threads.get(thread_id).map(|thread| (server.device.public_key.clone(), thread.clone()))
        });
        let Some((server_id, thread)) = found else { return };
        self.emit(Event::ThreadUpsert { thread: ThreadView { thread, server_id, unread: false } });
    }

    fn highlight(&self, thread_id: &str, row_ids: &[String]) {
        let Some(open) = self.open.get(thread_id) else { return };
        let of_agent = open.agent.iter().flat_map(|(_, transcript)| transcript.unhighlighted(row_ids));
        for Uncoloured { row_id, para, language, code } in
            open.transcript.unhighlighted(row_ids).into_iter().chain(of_agent)
        {
            let (inputs, thread_id) = (self.inputs.clone(), thread_id.to_string());
            tokio::task::spawn_blocking(move || {
                let spans = highlight::highlight(&language, &code);
                let _ = inputs.send(Input::Highlighted { thread_id, row_id, para, code, spans });
            });
        }
    }
}

/// Writes a project's icon where `emit_projects` looks for it, and removes the icons the project
/// had before.
/// What the client is told when a thread's activity changes: the rows of the messages that wait
/// for the agent, when those changed, and what the agent is doing.
fn activity_changed(thread_id: &str, transcript: &mut Transcript, activity: Activity) -> (Option<Splice>, Event) {
    let queued = transcript.set_queued(activity.queued.clone());
    let waiting = activity.approvals.iter().map(|approval| transcript.waiting(approval)).collect();
    (queued, Event::Activity { thread_id: thread_id.to_string(), activity, waiting })
}

/// Writes an image among a server's files where the client can read it, and answers with where
/// that is. The same file is written to the same place.
/// Where this device keeps a file it shows of a server's folder, or of one of its blobs.
fn shown_file(folder: &std::path::Path, server_id: &str, path: &str, blob: Option<&str>) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (server_id, path, blob).hash(&mut hasher);
    let extension = std::path::Path::new(path).extension().and_then(|extension| extension.to_str()).unwrap_or("bin");
    folder.join(format!("{:016x}.{extension}", hasher.finish()))
}

fn short_name(name: &str) -> String {
    let short: String = name.chars().take(10).collect();
    short.trim_end().to_string()
}

fn short_model_name(model: &ModelInfo) -> Option<(String, String)> {
    let short = model.name.strip_prefix("Claude ").or_else(|| model.name.strip_prefix("GPT-"))?;
    Some((model.id.clone(), short.to_string()))
}

fn save_icon(folder: &std::path::Path, project_id: &str, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
    std::fs::create_dir_all(folder)?;
    let unfinished = folder.join(format!("{name}.part"));
    std::fs::write(&unfinished, bytes)?;
    std::fs::rename(&unfinished, folder.join(name))?;
    let older = std::fs::read_dir(folder)?.flatten().filter(|entry| {
        let file = entry.file_name();
        let file = file.to_string_lossy();
        file.starts_with(&format!("{project_id}-")) && file != name
    });
    for entry in older {
        let _ = std::fs::remove_file(entry.path());
    }
    Ok(())
}

fn agents_event(thread_id: &str, started: &[Item]) -> Event {
    Event::Agents { thread_id: thread_id.to_string(), agents: started.iter().filter_map(agents::view).collect() }
}

/// Keeps the tool call that started an agent, in the place the thread has it.
fn note_agent(started: &mut Vec<Item>, item: &Item) {
    match started.iter_mut().find(|known| known.id == item.id) {
        Some(known) => *known = item.clone(),
        None => {
            let index = started.partition_point(|known| known.seq <= item.seq);
            started.insert(index, item.clone());
        }
    }
}

fn save_streamed(cache: &Cache, thread_id: &str, open: &mut OpenThread) {
    let unsaved: Vec<Item> = open.unsaved.drain().filter_map(|id| open.transcript.item(&id).cloned()).collect();
    if unsaved.is_empty() {
        return;
    }
    let items: Vec<&Item> = unsaved.iter().collect();
    cache.save_items(thread_id, &items, open.live.then_some(open.rev));
    open.saved_at = Instant::now();
}

fn keep_limits(server_id: &str, agents: &[AgentLimits]) -> Input {
    Input::Keep { key: format!("limits:{server_id}"), value: json!([now(), agents]) }
}

/// Where the usage of a period and of some servers, or all of them, is kept.
fn usage_key(bucket_secs: u32, buckets: u32, servers: Option<&Vec<String>>) -> String {
    let mut servers = servers.cloned().unwrap_or_default();
    servers.sort();
    format!("usage:{bucket_secs}:{buckets}:{}", servers.join(","))
}

fn reply(sink: &EventSink, id: u64, result: Result<Value, String>) {
    let (ok, value) = match result {
        Ok(value) => (true, value),
        Err(error) => (false, json!({ "error": error })),
    };
    sink(Event::Reply { id, ok, value });
}

/// What to show before the server answers. The cached activity dates from when the thread was
/// last open here, so the thread's flags, which the list keeps current, have the last word.
fn restored_activity(thread: &Thread, cached: Option<Activity>) -> Activity {
    if !thread.running && !thread.monitoring {
        return Activity::default();
    }
    let mut activity = cached.unwrap_or_default();
    activity.running = thread.running;
    activity.monitoring = thread.monitoring;
    if !thread.needs_approval {
        activity.approvals.clear();
    }
    activity
}

/// A turn ended after the user last looked. A thread never opened here doesn't nag.
fn is_unread(thread: &Thread, seen_at: Option<f64>) -> bool {
    match (thread.turn_ended_at, seen_at) {
        (Some(ended), Some(seen)) => ended > seen && !thread.running,
        _ => false,
    }
}

/// Joins splices that continue one another, as a catch-up's appended items do.
fn merge(splices: Vec<Splice>) -> Vec<Splice> {
    let mut merged: Vec<Splice> = Vec::new();
    for splice in splices {
        match merged.last_mut() {
            Some(last) if splice.remove == 0 && splice.start == last.start + last.rows.len() => {
                last.rows.extend(splice.rows);
            }
            _ => merged.push(splice),
        }
    }
    merged
}

/// What the address a sign-in came back to says. A browser carries the fragment of the page it
/// was sent on from over to that address, and it is no part of the answer.
fn callback_parameters(url: &str) -> HashMap<String, String> {
    let without_fragment = url.split_once('#').map_or(url, |(address, _)| address);
    let query = without_fragment.split_once('?').map(|(_, query)| query).unwrap_or_default();
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(name, value)| (name.to_string(), percent_decode(value)))
        .collect()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = (bytes[index] == b'%').then(|| text.get(index + 1..index + 3)).flatten();
        match hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(if bytes[index] == b'+' { b' ' } else { bytes[index] });
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_server_name_is_cut_to_ten_characters() {
        assert_eq!(short_name("studio"), "studio");
        assert_eq!(short_name("build-box1"), "build-box1");
        assert_eq!(short_name("Yektas-MacBook-Pro"), "Yektas-Mac");
        assert_eq!(short_name("my server one"), "my server");
    }

    #[test]
    fn a_model_name_drops_its_maker() {
        let model = |name: &str| ModelInfo {
            id: "id".into(),
            name: name.into(),
            agent: motile_protocol::wire::Agent::Claude,
            efforts: Vec::new(),
            default_effort: None,
        };
        let short = |name: &str| short_model_name(&model(name)).map(|(_, short)| short);
        assert_eq!(short("Claude Opus 5.5").as_deref(), Some("Opus 5.5"));
        assert_eq!(short("GPT-6.1-Sol").as_deref(), Some("6.1-Sol"));
        assert_eq!(short("Codex Auto Review"), None);
    }

    #[test]
    fn a_fragment_on_the_sign_in_callback_is_no_part_of_its_answer() {
        let parameters = callback_parameters("motile://auth?code=abc&state=one%20two#identifier");
        assert_eq!(parameters.get("code").map(String::as_str), Some("abc"));
        assert_eq!(parameters.get("state").map(String::as_str), Some("one two"));
    }

    #[test]
    fn query_values_are_decoded() {
        assert_eq!(percent_decode("a%20b+c%2Fd"), "a b c/d");
        assert_eq!(percent_decode("100%"), "100%");
    }

    fn thread(running: bool, monitoring: bool, needs_approval: bool) -> Thread {
        Thread {
            id: "t".into(),
            title: String::new(),
            project_id: "p".into(),
            cwd: "/srv".into(),
            agent: motile_protocol::wire::Agent::Claude,
            model: None,
            effort: None,
            access: motile_protocol::wire::Access::Full,
            plan: false,
            created_at: 1.0,
            updated_at: 2.0,
            done_at: None,
            undone_at: None,
            running,
            monitoring,
            needs_approval,
            agents: 0,
            turn_ended_at: None,
            pull_request: None,
            watching: false,
            git_stage: None,
            interruption: None,
            rev: 3,
        }
    }

    fn approval() -> motile_protocol::wire::Approval {
        motile_protocol::wire::Approval { id: "a".into(), tool_name: "Bash".into(), input: "{}".into() }
    }

    #[test]
    fn an_idle_thread_restores_no_activity() {
        let cached = Activity { running: true, started_at: Some(5.0), ..Activity::default() };
        assert_eq!(restored_activity(&thread(false, false, false), Some(cached)), Activity::default());
    }

    #[test]
    fn a_working_thread_restores_its_start_and_the_approval_still_waiting() {
        let cached =
            Activity { running: true, started_at: Some(5.0), approvals: vec![approval()], ..Activity::default() };
        let restored = restored_activity(&thread(true, false, true), Some(cached.clone()));
        assert_eq!(restored, cached);
    }

    #[test]
    fn an_approval_answered_since_no_longer_waits() {
        let cached = Activity { running: true, approvals: vec![approval()], ..Activity::default() };
        assert!(restored_activity(&thread(true, false, false), Some(cached)).approvals.is_empty());
    }

    #[test]
    fn a_monitoring_thread_never_opened_here_restores_from_its_flags() {
        let restored = restored_activity(&thread(false, true, false), None);
        assert_eq!(restored, Activity { monitoring: true, ..Activity::default() });
    }
}
