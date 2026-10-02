//! The core's one loop. Commands from the app and events from the hosts' links arrive on a single
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
use motile_protocol::wire::{HostInfo, Item, Message, Project, Request, Thread};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::api::{AccountView, Command, Config, Event, HostView, ProjectView, ThreadView};
use crate::cache::Cache;
use crate::connection::{HostAddr, bind};
use crate::link::{Link, LinkEvent, State, Status};
use crate::render::highlight::{self, Spans};
use crate::render::rows::{Splice, Transcript};

/// The newest items are rendered and sent first, so a long thread opens at once.
const FIRST_ITEMS: usize = 30;
/// Streamed text is rendered at most this often.
const RENDER_EVERY: Duration = Duration::from_millis(33);
const SAVE_EVERY: Duration = Duration::from_secs(1);
const TICK: Duration = Duration::from_secs(2);
/// How many ticks pass between account checks when nobody is waiting for a host.
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

    /// Disconnects from the hosts and ends the core. What it knows is already in the cache.
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
    Highlighted { thread_id: String, row_id: String, code: String, spans: Spans },
    IconFetched { host_id: String },
    Render,
    Tick,
    Stop,
}

struct Host {
    device: Device,
    link: Option<Arc<Link>>,
    status: Status,
    info: Option<HostInfo>,
    threads: HashMap<String, Thread>,
}

struct OpenThread {
    host_id: String,
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
    hosts: Vec<Host>,
    open: HashMap<String, OpenThread>,
    /// The secret and the state of the sign-in the browser is busy with.
    pending_sign_in: Option<(String, String)>,
    watch_hosts: bool,
    render_scheduled: bool,
    ticks: u64,
    /// The icon files that have been asked for, so none is asked for twice.
    icons_asked: HashSet<String>,
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
        while let Some((host_id, event)) = from_links.recv().await {
            if forward.send(Input::Link(host_id, event)).is_err() {
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
        config,
        sink,
        inputs: inputs.clone(),
        link_events,
        cache,
        key: Arc::new(key),
        endpoint: None,
        account_error: None,
        hosts: Vec::new(),
        open: HashMap::new(),
        pending_sign_in: None,
        watch_hosts: false,
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
        self.sync_hosts();
        self.bind_endpoint();
        self.check_account();
    }

    fn handle(&mut self, input: Input) {
        match input {
            Input::Command { id, command } => self.command(id, command),
            Input::Link(host_id, event) => self.link_event(&host_id, event),
            Input::Endpoint { key, result } => self.endpoint_bound(&key, result),
            Input::AccountChecked { key, result } => self.account_checked(&key, result),
            Input::SignedIn { id, result } => self.signed_in(id, result),
            Input::Highlighted { thread_id, row_id, code, spans } => {
                let Some(open) = self.open.get_mut(&thread_id) else { return };
                if open.transcript.set_spans(&row_id, &code, spans.clone()) {
                    self.emit(Event::Spans { thread_id, row_id, spans });
                }
            }
            Input::IconFetched { host_id } => {
                let projects = self.cache.projects(&host_id);
                self.emit_projects(&host_id, projects);
            }
            Input::Render => self.render(),
            Input::Stop => {}
            Input::Tick => {
                self.ticks += 1;
                if self.watch_hosts || self.ticks.is_multiple_of(ACCOUNT_CHECK_TICKS) {
                    self.check_account();
                }
            }
        }
    }

    async fn stop(mut self) {
        for (thread_id, open) in &mut self.open {
            save_streamed(&self.cache, thread_id, open);
        }
        for host in &self.hosts {
            if let Some(link) = &host.link {
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
                self.sync_hosts();
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
        self.sync_hosts();
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
        self.sync_hosts();
        self.open.clear();
        self.cache.clear();
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

    // ---- hosts ----

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
                self.connect_hosts();
            }
            Err(error) => tracing::error!("the network endpoint couldn't be opened: {error}"),
        }
    }

    /// Makes the hosts match the account's: drops the ones that are gone, adds the new ones.
    fn sync_hosts(&mut self) {
        let wanted: Vec<Device> =
            self.me.devices.iter().filter(|device| device.kind == DeviceKind::Host).cloned().collect();
        let (kept, gone): (Vec<Host>, Vec<Host>) = std::mem::take(&mut self.hosts)
            .into_iter()
            .partition(|host| wanted.iter().any(|device| device.public_key == host.device.public_key));
        self.hosts = kept;
        for host in gone {
            let host_id = host.device.public_key;
            if let Some(link) = host.link {
                link.shutdown();
            }
            self.open.retain(|_, open| open.host_id != host_id);
            self.cache.remove_host(&host_id);
            self.emit(Event::Threads { host_id: host_id.clone(), threads: Vec::new() });
            self.emit_projects(&host_id, Vec::new());
        }

        let mut added = Vec::new();
        for device in wanted {
            if let Some(host) = self.hosts.iter_mut().find(|host| host.device.public_key == device.public_key) {
                host.device = device;
                continue;
            }
            let host_id = device.public_key.clone();
            let cached = self.cache.threads(&host_id);
            let views: Vec<ThreadView> = cached
                .iter()
                .map(|cached| ThreadView {
                    unread: is_unread(&cached.thread, cached.seen_at),
                    thread: cached.thread.clone(),
                    host_id: host_id.clone(),
                })
                .collect();
            let threads = cached.into_iter().map(|cached| (cached.thread.id.clone(), cached.thread)).collect();
            self.hosts.push(Host {
                device,
                link: None,
                status: Status::default(),
                info: self.cache.host_info(&host_id),
                threads,
            });
            added.push((host_id, views));
        }
        self.connect_hosts();
        self.emit_hosts();
        // What the cache remembers of the new hosts, until they answer themselves.
        for (host_id, threads) in added {
            let projects = self.cache.projects(&host_id);
            self.emit_projects(&host_id, projects);
            self.emit(Event::Threads { host_id, threads });
        }
    }

    fn connect_hosts(&mut self) {
        let Some(endpoint) = &self.endpoint else { return };
        let direct = self.config.direct_addr.as_deref().and_then(|address| address.parse().ok());
        for host in self.hosts.iter_mut().filter(|host| host.link.is_none()) {
            let host_id = host.device.public_key.clone();
            let address = HostAddr { key: host_id.clone(), direct };
            let link = Link::connect(endpoint.clone(), address, self.link_events.clone());
            // Threads the app opened before there was a connection to follow them on.
            for (thread_id, open) in self.open.iter().filter(|(_, open)| open.host_id == host_id) {
                link.open(thread_id.clone(), open.rev);
            }
            host.link = Some(link);
        }
    }

    fn emit_hosts(&self) {
        let view = |host: &Host| HostView {
            id: host.device.public_key.clone(),
            name: host.device.name.clone(),
            platform: host.device.platform.clone(),
            state: host.status.state,
            error: host.status.error.clone(),
            path: host.status.path,
            rtt_ms: host.status.rtt_ms,
            info: host.info.clone(),
        };
        self.emit(Event::Hosts { hosts: self.hosts.iter().map(view).collect() });
    }

    fn host_mut(&mut self, host_id: &str) -> Option<&mut Host> {
        self.hosts.iter_mut().find(|host| host.device.public_key == host_id)
    }

    fn link(&self, host_id: &str) -> Result<Arc<Link>, String> {
        let host = self.hosts.iter().find(|host| host.device.public_key == host_id);
        host.and_then(|host| host.link.clone()).ok_or_else(|| "Not connected to that host.".to_string())
    }

    /// Tells the app about a host's projects, each with its icon if this device has the file.
    /// Icons it doesn't have yet are fetched, and the projects are told again when they arrive.
    fn emit_projects(&mut self, host_id: &str, projects: Vec<Project>) {
        let folder = self.config.data_dir.join("icons");
        let view = |project: Project| {
            let file = project.icon.as_ref().map(|icon| folder.join(format!("{}-{icon}", project.id)));
            let icon_path = file.filter(|file| file.is_file()).map(|file| file.to_string_lossy().into_owned());
            ProjectView { project, icon_path }
        };
        let views: Vec<ProjectView> = projects.into_iter().map(view).collect();
        for view in views.iter().filter(|view| view.icon_path.is_none()) {
            self.fetch_icon(host_id, &view.project);
        }
        self.emit(Event::Projects { host_id: host_id.to_string(), projects: views });
    }

    fn fetch_icon(&mut self, host_id: &str, project: &Project) {
        let Some(icon) = &project.icon else { return };
        let Ok(link) = self.link(host_id) else { return };
        let name = format!("{}-{icon}", project.id);
        if !self.icons_asked.insert(name.clone()) {
            return;
        }
        let folder = self.config.data_dir.join("icons");
        let (inputs, host_id, project_id) = (self.inputs.clone(), host_id.to_string(), project.id.clone());
        tokio::spawn(async move {
            let fetched = async {
                let request = Request::ProjectIcon { project_id: project_id.clone() };
                let Message::Icon { data } = link.request(&request).await? else {
                    bail!("The host didn't answer with an icon.");
                };
                save_icon(&folder, &project_id, &name, &BASE64.decode(data)?)
            };
            match fetched.await {
                Ok(()) => drop(inputs.send(Input::IconFetched { host_id })),
                Err(error) => tracing::debug!(project_id, "couldn't fetch a project's icon: {error:#}"),
            }
        });
    }

    fn thread_view(&self, host_id: &str, thread: &Thread) -> ThreadView {
        ThreadView {
            unread: is_unread(thread, self.cache.seen_at(&thread.id)),
            thread: thread.clone(),
            host_id: host_id.to_string(),
        }
    }

    // ---- what the hosts say ----

    fn link_event(&mut self, host_id: &str, event: LinkEvent) {
        match event {
            LinkEvent::Status(status) => {
                let refused = status.state == State::Refused;
                let Some(host) = self.host_mut(host_id) else { return };
                host.status = status;
                self.emit_hosts();
                // The host may have been removed from the account, or this device.
                if refused {
                    self.check_account();
                }
            }
            LinkEvent::List(message) => self.list_message(host_id, message),
            LinkEvent::Thread { thread_id, message } => self.thread_message(&thread_id, message),
        }
    }

    fn list_message(&mut self, host_id: &str, message: Message) {
        match message {
            Message::Welcome { host: info, threads, projects } => {
                self.cache.set_host_info(host_id, &info);
                self.cache.set_threads(host_id, &threads);
                self.cache.set_projects(host_id, &projects);
                let views = threads.iter().map(|thread| self.thread_view(host_id, thread)).collect();
                let Some(host) = self.host_mut(host_id) else { return };
                host.info = Some(info);
                host.threads = threads.into_iter().map(|thread| (thread.id.clone(), thread)).collect();
                // Threads deleted while the app was away are no longer followed.
                let known: HashSet<String> = host.threads.keys().cloned().collect();
                self.open.retain(|thread_id, open| open.host_id != host_id || known.contains(thread_id));
                self.emit_hosts();
                self.emit(Event::Threads { host_id: host_id.to_string(), threads: views });
                self.emit_projects(host_id, projects);
            }
            Message::Projects { projects } => {
                self.cache.set_projects(host_id, &projects);
                self.emit_projects(host_id, projects);
            }
            Message::ThreadUpsert { thread } => {
                self.cache.upsert_thread(host_id, &thread);
                let view = self.thread_view(host_id, &thread);
                let Some(host) = self.host_mut(host_id) else { return };
                host.threads.insert(thread.id.clone(), thread);
                self.emit(Event::ThreadUpsert { thread: view });
            }
            Message::ThreadDeleted { thread_id } => {
                self.cache.remove_thread(&thread_id);
                self.open.remove(&thread_id);
                if let Some(host) = self.host_mut(host_id) {
                    host.threads.remove(&thread_id);
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
                open.live = false;
                if reset {
                    open.transcript.clear();
                    open.rev = 0;
                    open.unrendered.clear();
                    open.unsaved.clear();
                    self.cache.clear_items(thread_id);
                    self.emit_rows(thread_id, true, Splice { start: 0, remove: 0, rows: Vec::new() });
                }
                self.emit(Event::Activity { thread_id: thread_id.to_string(), activity });
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
                self.emit(Event::Activity { thread_id: thread_id.to_string(), activity });
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
            Command::WatchHosts { on } => {
                self.watch_hosts = on;
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
            Command::RemoveHost { host_id } => {
                let (auth, key, sink, inputs) =
                    (self.auth.clone(), self.key.clone(), self.sink.clone(), self.inputs.clone());
                tokio::spawn(async move {
                    let removed = auth.remove_device(&key, &host_id).await.map_err(error_text);
                    let result = auth.me(&key).await.map_err(error_text);
                    let _ = inputs.send(Input::AccountChecked { key: key.public(), result });
                    reply(&sink, id, removed.map(|_| json!({})));
                });
            }
            Command::OpenThread { host_id, thread_id } => {
                let result = self.open_thread(&host_id, &thread_id);
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
            Command::Request { host_id, request } => {
                let link = match self.link(&host_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let answer = link.request(&request).await.map_err(error_text);
                    reply(&sink, id, answer.map(|message| serde_json::to_value(message).unwrap_or_default()));
                });
            }
            Command::Send { host_id, thread_id, new_thread, text, files } => {
                let link = match self.link(&host_id) {
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
                            other => bail!("Unexpected answer from the host: {other:?}"),
                        }
                    };
                    reply(&sink, id, sent.await.map_err(error_text));
                });
            }
            Command::UpdateHost { host_id } => {
                let link = match self.link(&host_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let events = sink.clone();
                    let report = move |received, total| {
                        events(Event::HostUpdate { host_id: host_id.clone(), received, total });
                    };
                    reply(&sink, id, link.update(report).await.map(|_| json!({})).map_err(error_text));
                });
            }
            Command::SetProjectIcon { host_id, project_id, file } => {
                let link = match self.link(&host_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let set = async {
                        let path = match &file {
                            Some(file) => Some(link.upload(std::path::Path::new(file)).await?),
                            None => None,
                        };
                        link.request(&Request::SetProjectIcon { project_id, path }).await
                    };
                    reply(&sink, id, set.await.map(|_| json!({})).map_err(error_text));
                });
            }
            Command::Highlight { thread_id, row_ids } => {
                self.highlight(&thread_id, &row_ids);
                self.reply(id, Ok(json!({})));
            }
        }
    }

    fn open_thread(&mut self, host_id: &str, thread_id: &str) -> Result<(), String> {
        if let Some(open) = self.open.get(thread_id) {
            let rows = open.transcript.rows().to_vec();
            self.emit_rows(thread_id, true, Splice { start: 0, remove: 0, rows });
            return Ok(());
        }
        let host = self.hosts.iter().find(|host| host.device.public_key == host_id).ok_or("That host is gone.")?;
        let cwd = host.threads.get(thread_id).map(|thread| thread.cwd.clone()).unwrap_or_default();
        let link = host.link.clone();

        let mut items = self.cache.items(thread_id);
        let since = self.cache.synced_rev(thread_id);
        let newest = items.split_off(items.len().saturating_sub(FIRST_ITEMS));
        let mut transcript = Transcript::new(&cwd);
        transcript.load(newest);
        self.emit_rows(thread_id, true, Splice { start: 0, remove: 0, rows: transcript.rows().to_vec() });
        if !items.is_empty() {
            let earlier = transcript.prepend(items);
            self.emit_rows(thread_id, false, Splice { start: 0, remove: 0, rows: earlier });
        }

        let open = OpenThread {
            host_id: host_id.to_string(),
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
        if let Ok(link) = self.link(&open.host_id) {
            link.close(thread_id);
        }
    }

    fn mark_seen(&mut self, thread_id: &str) {
        self.cache.set_seen(thread_id, now());
        let found = self.hosts.iter().find_map(|host| {
            host.threads.get(thread_id).map(|thread| (host.device.public_key.clone(), thread.clone()))
        });
        let Some((host_id, thread)) = found else { return };
        self.emit(Event::ThreadUpsert { thread: ThreadView { thread, host_id, unread: false } });
    }

    fn highlight(&self, thread_id: &str, row_ids: &[String]) {
        let Some(open) = self.open.get(thread_id) else { return };
        for (row_id, language, code) in open.transcript.unhighlighted(row_ids) {
            let (inputs, thread_id) = (self.inputs.clone(), thread_id.to_string());
            tokio::task::spawn_blocking(move || {
                let spans = highlight::highlight(&language, &code);
                let _ = inputs.send(Input::Highlighted { thread_id, row_id, code, spans });
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
