//! The core's one loop. Commands from the app and events from the servers' links arrive on a single
//! channel and are handled in turn, so the state needs no locks. Anything that waits on the
//! network runs in a task of its own and reports back through the same channel.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use iroh::Endpoint;
use motile_protocol::auth_api::{Device, DeviceKind, Me};
use motile_protocol::auth_client::{AuthClient, DeviceDescription};
use motile_protocol::identity::{DeviceKey, random_token};
use motile_protocol::now;
use motile_protocol::wire::{Item, Message, Project, Request, ServerInfo, Thread};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::api::{AccountView, Command, Config, Event, ProjectView, ServerView, ThreadView};
use crate::cache::Cache;
use crate::connection::{ServerAddr, bind};
use crate::link::{Link, LinkEvent, State, Status};
use crate::media::{self, MediaCache};
use crate::render::highlight::{self, Spans};
use crate::render::rows::{Splice, Transcript, Uncoloured};

/// The newest items are rendered and sent first, so a long thread opens at once.
const FIRST_ITEMS: usize = 30;
/// Streamed text is rendered at most this often.
const RENDER_EVERY: Duration = Duration::from_millis(33);
const SAVE_EVERY: Duration = Duration::from_secs(1);
const TICK: Duration = Duration::from_secs(2);
/// How often the app is told how far a download is.
const PROGRESS_EVERY: Duration = Duration::from_millis(200);
/// How many ticks pass between account checks when nobody is waiting for a server.
const ACCOUNT_CHECK_TICKS: u64 = 30;

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
    Command { id: u64, command: Command },
    Link(String, LinkEvent),
    Endpoint { key: String, result: Result<Endpoint, String> },
    AccountChecked { key: String, result: Result<Me, String> },
    SignedIn { id: u64, result: Result<Me, String> },
    Highlighted { thread_id: String, row_id: String, para: Option<usize>, code: String, spans: Spans },
    IconFetched { server_id: String },
    MediaFetched { id: String, result: Result<String, String> },
    Render,
    Tick,
    Stop,
}

struct Server {
    device: Device,
    link: Option<Arc<Link>>,
    status: Status,
    info: Option<ServerInfo>,
    threads: HashMap<String, Thread>,
}

struct OpenThread {
    server_id: String,
    transcript: Transcript,
    /// Whether the catch-up is done and updates now arrive in revision order.
    live: bool,
    rev: u64,
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
    me: Me,
    account_error: Option<String>,
    servers: Vec<Server>,
    open: HashMap<String, OpenThread>,
    /// The secret and the state of the sign-in the browser is busy with.
    pending_sign_in: Option<(String, String)>,
    watch_servers: bool,
    render_scheduled: bool,
    ticks: u64,
    /// The icon files that have been asked for, so none is asked for twice.
    icons_asked: HashSet<String>,
    media: Arc<MediaCache>,
    /// The commands waiting for each image or video that is being fetched.
    media_waiting: HashMap<String, Vec<u64>>,
}

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

    let mut core = Core {
        auth: AuthClient::new(&config.auth_url),
        me: cache.account().unwrap_or_default(),
        media: Arc::new(MediaCache::new(config.data_dir.join("media"))),
        media_waiting: HashMap::new(),
        config,
        sink,
        inputs: inputs.clone(),
        link_events,
        cache,
        key: Arc::new(key),
        endpoint: None,
        account_error: None,
        servers: Vec::new(),
        open: HashMap::new(),
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

fn error_text(error: anyhow::Error) -> String {
    format!("{error:#}")
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
                let Some(para) = para else {
                    if open.transcript.set_spans(&row_id, &code, spans.clone()) {
                        self.emit(Event::Spans { thread_id, row_id, spans });
                    }
                    return;
                };
                if let Some(splice) = open.transcript.set_para_spans(&row_id, para, &code, spans) {
                    self.emit_rows(&thread_id, false, splice);
                }
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
            Input::Render => self.render(),
            Input::Stop => {}
            Input::Tick => {
                self.ticks += 1;
                if self.watch_servers || self.ticks.is_multiple_of(ACCOUNT_CHECK_TICKS) {
                    self.check_account();
                }
            }
        }
    }

    async fn stop(mut self) {
        for (thread_id, open) in &mut self.open {
            save_streamed(&self.cache, thread_id, open);
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
        let query = url.split_once('?').map(|(_, query)| query).unwrap_or_default();
        let parameters: HashMap<String, String> = query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .map(|(name, value)| (name.to_string(), percent_decode(value)))
            .collect();
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
                self.endpoint = Some(endpoint);
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
            // Threads the app opened before there was a connection to follow them on.
            for (thread_id, open) in self.open.iter().filter(|(_, open)| open.server_id == server_id) {
                link.open(thread_id.clone(), open.rev);
            }
            server.link = Some(link);
        }
    }

    fn emit_servers(&self) {
        let view = |server: &Server| ServerView {
            id: server.device.public_key.clone(),
            name: server.device.name.clone(),
            platform: server.device.platform.clone(),
            state: server.status.state,
            error: server.status.error.clone(),
            path: server.status.path,
            rtt_ms: server.status.rtt_ms,
            info: server.info.clone(),
        };
        self.emit(Event::Servers { servers: self.servers.iter().map(view).collect() });
    }

    fn server_mut(&mut self, server_id: &str) -> Option<&mut Server> {
        self.servers.iter_mut().find(|server| server.device.public_key == server_id)
    }

    fn link(&self, server_id: &str) -> Result<Arc<Link>, String> {
        let server = self.servers.iter().find(|server| server.device.public_key == server_id);
        server.and_then(|server| server.link.clone()).ok_or_else(|| "Not connected to that server.".to_string())
    }

    /// Tells the app about a server's projects, each with its icon if this device has the file.
    /// Icons it doesn't have yet are fetched, and the projects are told again when they arrive.
    fn emit_projects(&mut self, server_id: &str, projects: Vec<Project>) {
        let folder = self.config.data_dir.join("icons");
        let view = |project: Project| {
            let file = project.icon.as_ref().map(|icon| folder.join(format!("{}-{icon}", project.id)));
            let icon_path = file.filter(|file| file.is_file()).map(|file| file.to_string_lossy().into_owned());
            ProjectView { project, icon_path }
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
                    bail!("The server didn't answer with an icon.");
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
                cache.trim(media::LIMIT);
                anyhow::Ok(file.to_string_lossy().into_owned())
            };
            let result = fetched.await.map_err(error_text);
            if result.is_err() {
                let _ = std::fs::remove_file(&unfinished);
            }
            let _ = inputs.send(Input::MediaFetched { id: media_id, result });
        });
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
                server.status = status;
                self.emit_servers();
                // The server may have been removed from the account, or this device.
                if refused {
                    self.check_account();
                }
            }
            LinkEvent::List(message) => self.list_message(server_id, message),
            LinkEvent::Thread { thread_id, message } => self.thread_message(&thread_id, message),
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
                // Threads deleted while the app was away are no longer followed.
                let known: HashSet<String> = server.threads.keys().cloned().collect();
                self.open.retain(|thread_id, open| open.server_id != server_id || known.contains(thread_id));
                self.emit_servers();
                self.emit(Event::Threads { server_id: server_id.to_string(), threads: views });
                self.emit_projects(server_id, projects);
            }
            Message::Projects { projects } => {
                self.cache.set_projects(server_id, &projects);
                self.emit_projects(server_id, projects);
            }
            Message::ThreadUpsert { thread } => {
                self.cache.upsert_thread(server_id, &thread);
                let view = self.thread_view(server_id, &thread);
                let Some(server) = self.server_mut(server_id) else { return };
                server.threads.insert(thread.id.clone(), thread);
                self.emit(Event::ThreadUpsert { thread: view });
            }
            Message::ThreadDeleted { thread_id } => {
                self.cache.remove_thread(&thread_id);
                self.open.remove(&thread_id);
                if let Some(server) = self.server_mut(server_id) {
                    server.threads.remove(&thread_id);
                }
                self.emit(Event::ThreadDeleted { thread_id });
            }
            other => tracing::debug!("unexpected message on the thread list: {other:?}"),
        }
    }

    fn thread_message(&mut self, thread_id: &str, message: Message) {
        let Some(open) = self.open.get_mut(thread_id) else { return };
        match message {
            Message::Opened { reset, activity } => {
                let waiting = activity.approvals.iter().map(|approval| open.transcript.waiting(approval)).collect();
                open.live = false;
                if reset {
                    open.transcript.clear();
                    open.rev = 0;
                    open.unrendered.clear();
                    open.unsaved.clear();
                    self.cache.clear_items(thread_id);
                    self.emit_rows(thread_id, true, Splice { start: 0, remove: 0, rows: Vec::new() });
                }
                self.emit(Event::Activity { thread_id: thread_id.to_string(), activity, waiting });
            }
            Message::Items { items } => {
                let live = open.live;
                let mut splices = Vec::new();
                for item in &items {
                    open.rev = if live { open.rev.max(item.rev) } else { open.rev };
                    open.unrendered.remove(&item.id);
                    open.unsaved.remove(&item.id);
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
                for splice in merge(splices) {
                    self.emit_rows(thread_id, false, splice);
                }
            }
            Message::Synced { rev } => {
                open.live = true;
                open.rev = rev;
                self.cache.save_items(thread_id, &[], Some(rev));
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
                let waiting = activity.approvals.iter().map(|approval| open.transcript.waiting(approval)).collect();
                self.emit(Event::Activity { thread_id: thread_id.to_string(), activity, waiting });
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
        });
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
                    reply(&sink, id, token.map(|token| serde_json::to_value(token).unwrap_or_default()));
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
            Command::Send { server_id, thread_id, new_thread, text, files } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let sent = async {
                        let mut attachments = Vec::new();
                        for file in &files {
                            attachments.push(link.upload(std::path::Path::new(file)).await?);
                        }
                        match link.request(&Request::Send { thread_id, new_thread, text, attachments }).await? {
                            Message::Sent { thread_id } => Ok(json!({ "thread_id": thread_id })),
                            other => bail!("Unexpected answer from the server: {other:?}"),
                        }
                    };
                    reply(&sink, id, sent.await.map_err(error_text));
                });
            }
            Command::UpdateServer { server_id } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let events = sink.clone();
                    let report = move |received, total| {
                        events(Event::ServerUpdate { server_id: server_id.clone(), received, total });
                    };
                    reply(&sink, id, link.update(report).await.map(|_| json!({})).map_err(error_text));
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
            Command::Media { server_id, id: media_id } => self.find_media(id, &server_id, media_id),
            Command::Storage => {
                let (media, sink) = (self.media.clone(), self.sink.clone());
                tokio::task::spawn_blocking(move || {
                    reply(&sink, id, Ok(json!({ "media_bytes": media.size(), "media_limit": media::LIMIT })));
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
                self.reply(id, Ok(json!({})));
            }
        }
    }

    fn open_thread(&mut self, server_id: &str, thread_id: &str) -> Result<(), String> {
        if let Some(open) = self.open.get(thread_id) {
            let rows = open.transcript.rows().to_vec();
            self.emit_rows(thread_id, true, Splice { start: 0, remove: 0, rows });
            return Ok(());
        }
        let server =
            self.servers.iter().find(|server| server.device.public_key == server_id).ok_or("That server is gone.")?;
        let cwd = server.threads.get(thread_id).map(|thread| thread.cwd.clone()).unwrap_or_default();
        let link = server.link.clone();

        let mut items = self.cache.items(thread_id);
        let since = self.cache.synced_rev(thread_id);
        let newest = items.split_off(items.len().saturating_sub(FIRST_ITEMS));
        let mut transcript = Transcript::new(&cwd);
        transcript.load(newest);
        self.emit_rows(thread_id, true, Splice { start: 0, remove: 0, rows: transcript.rows().to_vec() });
        if !items.is_empty()
            && let Some(earlier) = transcript.prepend(items)
        {
            self.emit_rows(thread_id, false, earlier);
        }

        let open = OpenThread {
            server_id: server_id.to_string(),
            transcript,
            live: false,
            rev: since,
            unrendered: HashSet::new(),
            unsaved: HashSet::new(),
            saved_at: Instant::now(),
        };
        self.open.insert(thread_id.to_string(), open);
        if let Some(link) = link {
            link.open(thread_id.to_string(), since);
        }
        Ok(())
    }

    fn close_thread(&mut self, thread_id: &str) {
        let Some(mut open) = self.open.remove(thread_id) else { return };
        save_streamed(&self.cache, thread_id, &mut open);
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
        for Uncoloured { row_id, para, language, code } in open.transcript.unhighlighted(row_ids) {
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

fn save_streamed(cache: &Cache, thread_id: &str, open: &mut OpenThread) {
    let unsaved: Vec<Item> = open.unsaved.drain().filter_map(|id| open.transcript.item(&id).cloned()).collect();
    if unsaved.is_empty() {
        return;
    }
    let items: Vec<&Item> = unsaved.iter().collect();
    cache.save_items(thread_id, &items, open.live.then_some(open.rev));
    open.saved_at = Instant::now();
}

fn reply(sink: &EventSink, id: u64, result: Result<Value, String>) {
    let (ok, value) = match result {
        Ok(value) => (true, value),
        Err(error) => (false, json!({ "error": error })),
    };
    sink(Event::Reply { id, ok, value });
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
    fn query_values_are_decoded() {
        assert_eq!(percent_decode("a%20b+c%2Fd"), "a b c/d");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
