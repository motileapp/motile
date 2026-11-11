//! The core's one loop. Commands from the client and events from the servers' links arrive on a single
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
use motile_openui::ToolError;
use motile_protocol::auth_api::{Device, DeviceKind, Me};
use motile_protocol::auth_client::{AuthClient, DeviceDescription};
use motile_protocol::gateway_wire::{GatewayReply, GatewaySignIn, GatewayState, Manage, McpAdding, McpChange};
use motile_protocol::identity::{DeviceKey, random_token};
use motile_protocol::now;
use motile_protocol::wire::{
    Access, AccountSkills, Activity, Agent, AgentLimits, Bucket, ContinueSettings, FileKind, Item, ItemKind, Media,
    Message, Project, PullRequestSettings, Request, ServerInfo, Skill, Thread, ThreadMatch, ToolStatus, Worktree,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use crate::api::{
    AccountView, Children, Command, Config, Event, FindFrom, ProjectView, ScrollAt, Section, ServerView, SyncState,
    SyncView, ThreadView, WaitingChild,
};
use crate::away::{Away, STALE_AFTER};
use crate::backup::{self, BACKUP_PROTOCOL};
use crate::browse;
use crate::cache::{Cache, Page};
use crate::connection::{NOT_CONNECTED, ServerAddr, bind, leave_relays, offline};
use crate::find::{self, Find};
use crate::follow::{self, Followed};
use crate::gateway::{self, GatewayEvent, GatewayLink};
use crate::git;
use crate::keybindings::Keybindings;
use crate::limits;
use crate::linear;
use crate::link::{Link, LinkEvent, State, Status};
use crate::mcp_servers;
use crate::media::{self, MediaCache};
use crate::outbox::{Outbox, Outgoing};
use crate::pull_request;
use crate::render::agents::{self, ThreadState};
use crate::render::diff;
use crate::render::highlight::{self, Spans};
use crate::render::openui::host::{Out, Programs, TimerKey};
use crate::render::rows::{Row, RowKind, Splice, Transcript, Uncoloured, collapses, short_model_name};
use crate::ricrac;
use crate::search;
use crate::skills;
use crate::sync::{self as relay, Origin, SYNC_PROTOCOL};
use crate::{activity, usage};

/// The newest items are rendered and sent first, so a long thread opens at once.
const FIRST_ITEMS: usize = 30;
/// How many threads the client left stay open and current, so that going back to one is instant.
const LEFT_OPEN: usize = 4;
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
/// The first protocol whose servers keep what was done.
const ACTIVITY_PROTOCOL: u32 = 40;
/// The servers before this one can't say what the agents' accounts have used of their plans.
const LIMITS_PROTOCOL: u32 = 17;
/// The servers before this one can't search their threads' messages.
const SEARCH_PROTOCOL: u32 = 26;
/// The first protocol whose servers list the skills of their projects.
const SKILLS_PROTOCOL: u32 = 28;
/// The first protocol whose servers keep the state of programs and run their tools and lookups:
/// `SetUi`, `UiTool`, `UiImages`, `UiRoute`, and a message's `context`.
const UI_PROTOCOL: u32 = 34;
/// How long the skills a server listed are offered before it is asked again.
const SKILLS_FRESH: Duration = Duration::from_secs(30);
/// How long the search has to stay as it is before the servers are asked.
const SEARCH_DELAY: Duration = Duration::from_millis(120);
/// How long a search waits for a server.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the Limits tab waits for the gateway's accounts.
const GATEWAY_LIMITS_WAIT: Duration = Duration::from_secs(5);
/// What a server said of its agents' accounts is read again once it is this old.
const LIMITS_STALE: Duration = Duration::from_secs(5 * 60);
/// How long the core may take over one input before the log says so.
const BUSY_AFTER: Duration = Duration::from_secs(1);
/// How long a message that didn't reach its server waits before it is sent again.
const OUTBOX_RETRY: Duration = Duration::from_secs(3);
/// An endpoint that reached no server for this long is replaced, in case the fault is its own.
const WEDGED_AFTER: Duration = Duration::from_secs(60);
/// How long after a server connects or the account changes the servers' threads are copied.
const RELAY_SOON: Duration = Duration::from_secs(2);
/// How long after a thread changes the servers' threads are copied, so a turn's changes go together.
const RELAY_AFTER_CHANGE: Duration = Duration::from_secs(10);
/// How many ticks pass between rounds of copying when nothing asks for one, about five minutes.
const RELAY_TICKS: u64 = 150;
/// A round of copying that ends sooner than this never shows under its servers: a turn's few
/// changes would only make the line flicker.
const RELAY_QUIET: Duration = Duration::from_secs(3);

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
    /// The rows of an open thread that finding searches, read.
    FindIndexed {
        thread_id: String,
        entries: Vec<find::Entry>,
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
    /// What a server found for the search, and whether it was the last to answer.
    Searched {
        query: String,
        asked: String,
        found: Vec<search::Found>,
        last: bool,
    },
    /// A server took the bucket this device keeps, or couldn't.
    BackupGiven {
        server_id: String,
        result: Result<(), String>,
    },
    /// The first server checked the bucket: this device keeps it now.
    BackupChecked {
        bucket: Option<Bucket>,
    },
    /// Time to copy the servers' threads between them.
    Relay,
    RelayProgress(relay::Progress),
    Relayed(relay::Round),
    /// A server's copy of a removed server's projects and threads.
    CopiesFetched {
        origin: String,
        projects: Vec<Project>,
        threads: Vec<Thread>,
    },
    /// A server's copy of the transcript of a removed server's thread.
    CopyOpened {
        thread_id: String,
        result: Result<Vec<Item>, String>,
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
    /// What a program's tool call answered.
    UiToolDone {
        thread_id: String,
        row_id: String,
        serial: u64,
        call: u64,
        result: Result<Value, ToolError>,
    },
    UiTick {
        thread_id: String,
        key: TimerKey,
    },
    /// Time for the programs' saves and lookups that wait.
    UiWake {
        thread_id: String,
    },
    UiImagesFound {
        thread_id: String,
        query: String,
        images: Vec<Media>,
    },
    UiRouted {
        thread_id: String,
        stops: Vec<[f64; 2]>,
        points: Option<Vec<[f64; 2]>>,
    },
    UiHighlighted {
        thread_id: String,
        language: String,
        code: String,
    },
    /// From the link to the gateway with that key.
    Gateway {
        key: String,
        event: GatewayEvent,
    },
    /// What the gateway answered the command `id` with, other than its state.
    GatewayAnswered {
        id: u64,
        answer: GatewayReply,
    },
    /// The gateway finished the sign-in, or couldn't; for the command `reply_to`, when one waits.
    GatewaySignInEnded {
        sign_in: String,
        reply_to: Option<u64>,
        result: Result<GatewayState, String>,
    },
    /// What the server answered a message of the outbox with.
    Delivered {
        message_id: String,
        result: Result<Message, String>,
    },
    /// Time to send again what didn't reach its server.
    SendOutbox,
    Render,
    Tick,
    Stop,
}

struct Server {
    device: Device,
    /// The server was removed from the account. Its threads are the copies its account's other
    /// servers keep.
    removed: bool,
    link: Option<Arc<Link>>,
    status: Status,
    /// Since when the server hasn't been reached, while it isn't.
    away_since: Option<Instant>,
    info: Option<ServerInfo>,
    /// What its agents' accounts had used when it last said, and when that was.
    limits: Vec<AgentLimits>,
    limits_read_at: Option<f64>,
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
    /// How the threads its agents work in are doing, by their ids.
    threads: HashMap<String, ThreadState>,
    /// The agent whose transcript the client shows: the tool call that started it, and what it did.
    agent: Option<(String, Transcript)>,
    /// Items whose streamed text hasn't been rendered yet.
    unrendered: HashSet<String>,
    /// Items whose streamed text isn't in the cache yet.
    unsaved: HashSet<String>,
    saved_at: Instant,
    /// The programs its rows show, running.
    programs: Programs,
    /// The timers of its programs, which stop with it.
    ui_timers: UiTimers,
    /// When the client left it. It stays open and current until it is one too many, but the client
    /// isn't told about it.
    left: Option<Instant>,
    /// What the client finds in it, while it does.
    find: Option<Find>,
}

/// A place the client left a transcript at, with the item of its row, which the transcript loads
/// back to when the thread opens again.
struct Place {
    at: ScrollAt,
    item: String,
    seq: u64,
}

impl Place {
    /// The place among the rows, or the top of the first row of its item when its row is gone.
    fn among(&self, rows: &[Row]) -> Option<ScrollAt> {
        if rows.iter().any(|row| row.id == self.at.row_id) {
            return Some(self.at.clone());
        }
        let row = rows.iter().find(|row| row.item == self.item)?;
        Some(ScrollAt { row_id: row.id.clone(), delta: 0.0 })
    }
}

/// Puts the cache's earlier pages in front of the transcript until it has the item at `seq`.
fn load_back_to(cache: &Cache, thread_id: &str, transcript: &mut Transcript, earlier: &mut bool, seq: u64) {
    while *earlier && transcript.first_seq().is_some_and(|first| first > seq) {
        let page = cache.page(thread_id, transcript.first_seq());
        *earlier = page.earlier;
        transcript.prepend(page.items);
    }
}

#[derive(Default)]
struct UiTimers(HashMap<TimerKey, AbortHandle>);

impl UiTimers {
    fn set(&mut self, key: TimerKey, task: AbortHandle) {
        if let Some(before) = self.0.insert(key, task) {
            before.abort();
        }
    }

    fn stop(&mut self, key: &TimerKey) {
        if let Some(task) = self.0.remove(key) {
            task.abort();
        }
    }
}

impl Drop for UiTimers {
    fn drop(&mut self) {
        self.0.values().for_each(AbortHandle::abort);
    }
}

struct Core {
    config: Config,
    sink: EventSink,
    inputs: mpsc::UnboundedSender<Input>,
    link_events: mpsc::UnboundedSender<(String, LinkEvent)>,
    auth: AuthClient,
    cache: Cache,
    outbox: Outbox,
    key: Arc<DeviceKey>,
    endpoint: Option<Endpoint>,
    /// Whether the client was paused, asleep or suspended, or in the background, where Android
    /// may cut its network without pausing it. The links ask it too.
    away: Away,
    /// When the endpoint last had a server answer it, or was opened.
    reached_at: Instant,
    /// The key of the endpoint being opened, while it is.
    binding: Option<String>,
    me: Me,
    account_error: Option<String>,
    servers: Vec<Server>,
    open: HashMap<String, OpenThread>,
    /// Where the client left each transcript, by its thread and its agent.
    places: HashMap<(String, Option<String>), Place>,
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
    /// The commands waiting for each image, video or file that is being fetched, and what stops it.
    media_waiting: HashMap<String, (Vec<u64>, AbortHandle)>,
    /// The photos each server found for programs' queries, so that a thread opened again shows them
    /// at once.
    photos: HashMap<String, HashMap<String, Vec<Media>>>,
    /// The files on their way to a server, by the key the client gave: the command that waits for
    /// each, and what stops it.
    uploads: HashMap<String, (u64, AbortHandle)>,
    /// The folders last listed for `browse`: of which server and directory, and whether with the
    /// hidden ones.
    browsed: Browsed,
    /// The images last listed for `project_images`: of which server and project, and the
    /// project's folder with the paths inside it.
    searched_images: SearchedImages,
    /// The skills each server's projects have, by server and project, and when they were listed.
    /// Held while they are asked for, so that what is typed meanwhile waits for the answer.
    skills: Skills,
    keybindings: Keybindings,
    search: Search,
    relay: Relay,
    gateway: Gateway,
    backup: Backup,
}

/// The account's gateway, as far as the client knows it.
#[derive(Default)]
struct Gateway {
    /// A view that shows it is open.
    watching: bool,
    link: Option<Arc<GatewayLink>>,
    status: gateway::Status,
    /// Its last state, with its key.
    state: Option<(String, GatewayState)>,
    sign_in: Option<gateway::SignIn>,
    /// The account the sign-in being started signs in again.
    signing_in_again: Option<String>,
}

/// The account's bucket, as this device keeps it and gives it to the servers.
struct Backup {
    saved: backup::Saved,
    /// The servers being given the bucket now.
    giving: HashSet<String>,
    /// Why a server couldn't take it, by server id, until it connects again.
    failed: HashMap<String, String>,
}

/// The rounds that copy the servers' threads between them.
#[derive(Default)]
struct Relay {
    running: bool,
    /// Something changed while a round ran: another follows it.
    again: bool,
    timer: Option<AbortHandle>,
    /// The device is on a network that costs by the byte.
    metered: bool,
    /// How far each server's logs went at the last round.
    marks: HashMap<String, relay::Marks>,
    /// The removed servers whose copies have been read since the client started.
    read: HashSet<String>,
    /// What the running round carries, with how far each log has reached.
    copying: Vec<(relay::Transfer, u64)>,
    /// When the running round planned what it carries, until the lines under its servers show it.
    quiet_since: Option<Instant>,
    /// Why the last round couldn't copy to a server, by server id.
    failed: HashMap<String, relay::Failure>,
    /// The servers the last round left behind until a network that isn't metered.
    waiting: HashSet<String>,
    /// When a round last left each server's copies whole.
    synced_at: HashMap<String, f64>,
}

type Skills = Arc<tokio::sync::Mutex<HashMap<(String, String), (Instant, Vec<AccountSkills>)>>>;

/// The sidebar's search through the messages.
#[derive(Default)]
struct Search {
    query: String,
    found: Vec<search::Found>,
    asking: Option<AbortHandle>,
}

type Browsed = Arc<std::sync::Mutex<Option<((String, String, bool), Vec<String>)>>>;
type SearchedImages = Arc<std::sync::Mutex<Option<((String, String), (String, Vec<String>))>>>;

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
        photos: HashMap::new(),
        uploads: HashMap::new(),
        browsed: Browsed::default(),
        searched_images: SearchedImages::default(),
        skills: Skills::default(),
        keybindings: Keybindings::load(&config.data_dir, &config.platform),
        backup: Backup { saved: backup::Saved::load(&config.data_dir), giving: HashSet::new(), failed: HashMap::new() },
        config,
        sink,
        inputs: inputs.clone(),
        link_events,
        outbox: Outbox::load(&cache),
        cache,
        key: Arc::new(key),
        endpoint: None,
        away: Away::new(),
        reached_at: Instant::now(),
        binding: None,
        account_error: None,
        servers: Vec::new(),
        open: HashMap::new(),
        places: HashMap::new(),
        followed: HashMap::new(),
        pending_sign_in: None,
        watch_servers: false,
        render_scheduled: false,
        ticks: 0,
        icons_asked: HashSet::new(),
        search: Search::default(),
        relay: Relay::default(),
        gateway: Gateway::default(),
    };
    // Loading the syntax definitions takes a moment; better now than at the first code block.
    tokio::task::spawn_blocking(|| highlight::highlight("rust", "fn main() {}"));
    tokio::spawn(async move {
        core.begin();
        while let Some(input) = received.recv().await {
            if matches!(input, Input::Stop) {
                break;
            }
            let started = Instant::now();
            core.handle(input);
            if started.elapsed() >= BUSY_AFTER {
                tracing::warn!(took_ms = started.elapsed().as_millis() as u64, "the core was busy");
            }
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

/// Asks the server for the skills of the project of `key` and keeps what it lists.
async fn list_skills(link: Arc<Link>, key: (String, String), listed: Skills) {
    let Ok(Message::Skills { accounts }) = link.request(&Request::Skills { project_id: key.1.clone() }).await else {
        return;
    };
    listed.lock().await.insert(key, (Instant::now(), accounts));
}

impl Core {
    fn emit(&self, event: Event) {
        if transcript_of(&event).is_some_and(|thread_id| self.is_left(thread_id)) {
            return;
        }
        (self.sink)(event);
    }

    fn is_left(&self, thread_id: &str) -> bool {
        self.open.get(thread_id).is_some_and(|open| open.left.is_some())
    }

    fn reply(&self, id: u64, result: Result<Value, String>) {
        reply(&self.sink, id, result);
    }

    /// Answers with what `answer` makes of the skills of `account` in the project, as the server
    /// last listed them. Once that is older than `SKILLS_FRESH` it is asked again behind the
    /// answer; only a project it never listed waits for it. A server too old to list them has none.
    fn answer_with_skills(
        &self,
        id: u64,
        server_id: String,
        project_id: String,
        account: String,
        answer: impl FnOnce(&[Skill]) -> Value + Send + 'static,
    ) {
        let server = self.servers.iter().find(|server| server.device.public_key == server_id);
        let protocol = server.and_then(|server| server.info.as_ref()).map_or(0, |info| info.protocol);
        let link = match self.link(&server_id) {
            Ok(link) if protocol >= SKILLS_PROTOCOL => link,
            Ok(_) => return self.reply(id, Ok(answer(&[]))),
            Err(error) => return self.reply(id, Err(error)),
        };
        let (sink, listed) = (self.sink.clone(), self.skills.clone());
        tokio::spawn(async move {
            let kept = listed.clone();
            let mut listed = listed.lock().await;
            let key = (server_id, project_id.clone());
            match listed.get_mut(&key) {
                Some((at, _)) if at.elapsed() >= SKILLS_FRESH => {
                    *at = Instant::now();
                    tokio::spawn(list_skills(link, key.clone(), kept));
                }
                Some(_) => {}
                None => match link.request(&Request::Skills { project_id }).await {
                    Ok(Message::Skills { accounts }) => {
                        listed.insert(key.clone(), (Instant::now(), accounts));
                    }
                    Ok(_) => {}
                    Err(error) => return reply(&sink, id, Err(error_text(error))),
                },
            }
            let accounts = listed.get(&key).map(|(_, accounts)| accounts.as_slice()).unwrap_or_default();
            let skills =
                accounts.iter().find(|listed| listed.account == account).map(|listed| listed.skills.as_slice());
            reply(&sink, id, Ok(answer(skills.unwrap_or_default())));
        });
    }

    /// Shows what is known from last time, then goes to find out what is true now.
    fn begin(&mut self) {
        self.emit_keybindings();
        self.emit_account();
        self.sync_servers();
        self.emit_outbox();
        self.emit(Event::Restored);
        self.bind_endpoint();
        self.check_account();
    }

    fn handle(&mut self, input: Input) {
        match input {
            Input::Command { id, command } => self.command(id, command),
            Input::FindIndexed { thread_id, entries } => self.find_indexed(&thread_id, entries),
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
            Input::BackupGiven { server_id, result } => {
                self.backup.giving.remove(&server_id);
                match result {
                    Ok(()) => drop(self.backup.failed.remove(&server_id)),
                    Err(error) => drop(self.backup.failed.insert(server_id, error)),
                }
                self.emit_servers();
            }
            Input::BackupChecked { bucket } => {
                if let Err(error) = self.backup.saved.save(bucket) {
                    tracing::warn!("couldn't keep the bucket's keys: {error}");
                }
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
                let waiting = self.media_waiting.remove(&id).map(|(waiting, _)| waiting).unwrap_or_default();
                for waiting in waiting {
                    self.reply(waiting, answer.clone());
                }
            }
            Input::Uploaded { key, result } => {
                let Some((id, _)) = self.uploads.remove(&key) else { return };
                self.reply(id, result);
            }
            Input::Searched { query, asked, found, last } => {
                if self.search.query != query {
                    return;
                }
                search::merge(&mut self.search.found, &asked, found);
                if last {
                    self.search.asking = None;
                }
                self.emit_search(!last);
            }
            Input::Keep { key, value } => {
                self.cache.keep(&key, &value);
                if let Some(server_id) = key.strip_prefix("limits:") {
                    self.limits_kept(server_id);
                }
            }
            Input::Relay => self.start_relay(),
            Input::RelayProgress(progress) => self.relay_progress(progress),
            Input::Relayed(round) => self.relayed(round),
            Input::CopiesFetched { origin, projects, threads } => self.copies_fetched(&origin, projects, threads),
            Input::CopyOpened { thread_id, result } => self.copy_opened(&thread_id, result),
            Input::UiToolDone { thread_id, row_id, serial, call, result } => {
                self.with_programs(&thread_id, |programs| programs.tool_result(&row_id, serial, call, result))
            }
            Input::UiTick { thread_id, key } => {
                if self.is_left(&thread_id) {
                    return;
                }
                self.with_programs(&thread_id, |programs| programs.tick(&key))
            }
            Input::UiWake { thread_id } => self.with_programs(&thread_id, |programs| programs.wake(Instant::now())),
            Input::UiImagesFound { thread_id, query, images } => {
                let server_id = self.open.get(&thread_id).map(|open| open.server_id.clone());
                if let Some(server_id) = server_id.filter(|_| !images.is_empty()) {
                    self.photos.entry(server_id).or_default().insert(query.clone(), images.clone());
                }
                self.with_programs(&thread_id, |programs| programs.images(&query, images))
            }
            Input::UiRouted { thread_id, stops, points } => {
                self.with_programs(&thread_id, |programs| programs.route(&stops, points))
            }
            Input::UiHighlighted { thread_id, language, code } => {
                self.with_programs(&thread_id, |programs| programs.highlighted(&language, &code))
            }
            Input::Gateway { key, event } => self.gateway_event(&key, event),
            Input::GatewayAnswered { id, answer } => self.gateway_answered(id, answer),
            Input::GatewaySignInEnded { sign_in, reply_to, result } => {
                self.gateway_sign_in_ended(&sign_in, reply_to, result)
            }
            Input::Delivered { message_id, result } => self.delivered(&message_id, result),
            Input::SendOutbox => self.send_outbox(),
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
                if self.ticks.is_multiple_of(RELAY_TICKS) {
                    self.schedule_relay(RELAY_SOON);
                }
                if self.relay.quiet_since.is_some_and(|since| since.elapsed() >= RELAY_QUIET) {
                    self.relay.quiet_since = None;
                    self.emit_servers();
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
        if let Some(link) = self.gateway.link.take() {
            link.shutdown();
        }
        if let Some(timer) = self.relay.timer.take() {
            timer.abort();
        }
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close().await;
        }
    }

    // ---- account ----

    fn signed_in_now(&self) -> bool {
        self.me.user.is_some()
    }

    fn emit_keybindings(&self) {
        self.emit(Event::Keybindings { keybindings: self.keybindings.view() });
    }

    fn keybindings_changed(&self, id: u64, result: Result<(), String>) {
        if result.is_ok() {
            self.emit_keybindings();
        }
        self.reply(id, result.map(|()| json!({})));
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
        self.outbox.clear();
        self.emit_outbox();
        self.media.clear();
        if let Err(error) = self.backup.saved.save(None) {
            tracing::warn!("couldn't forget the bucket's keys: {error}");
        }
        self.backup.failed.clear();
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
            link.check_now();
        }
        if let Some(link) = &self.gateway.link {
            link.retry_now();
        }
    }

    /// Opens a new endpoint for the links once the client was away, paused or in the background,
    /// for a while, and says whether it does. The system cuts what a paused endpoint had open
    /// without a word, and the endpoint can go on dialing through it for long.
    fn replace_stale_endpoint(&mut self) -> bool {
        let paused = self.away.take();
        if paused < STALE_AFTER || self.endpoint.is_none() {
            return false;
        }
        tracing::info!(paused_ms = paused.as_millis() as u64, "replacing the endpoint after a pause");
        self.bind_endpoint();
        true
    }

    /// Opens a new endpoint when the one there is has long reached none of the servers. They may
    /// all be down, but an endpoint can also get stuck for good, and only a new one tells.
    fn replace_wedged_endpoint(&mut self) {
        let linked: Vec<&Server> = self.servers.iter().filter(|server| !server.removed).collect();
        let reaches = linked.iter().any(|server| matches!(server.status.state, State::Connected | State::Refused));
        if reaches || linked.is_empty() || self.endpoint.is_none() {
            self.reached_at = Instant::now();
            return;
        }
        if self.reached_at.elapsed() < WEDGED_AFTER {
            return;
        }
        self.reached_at = Instant::now();
        tracing::warn!("replacing the endpoint, which reached no server for a minute");
        self.bind_endpoint();
    }

    /// Opens a new endpoint once the one there is has let go of the relays. Two endpoints of one
    /// key take the relay from each other.
    fn bind_endpoint(&mut self) {
        if self.binding.as_deref() == Some(self.key.public().as_str()) {
            return;
        }
        self.binding = Some(self.key.public());
        // What ends on the old endpoint ends because it is closed here, which says nothing of the
        // servers: the links show as dialing until the new one is open.
        self.retire_links();
        let (key, inputs, local_only) = (self.key.clone(), self.inputs.clone(), self.config.local_only);
        let stale = self.endpoint.clone();
        tokio::spawn(async move {
            if let Some(stale) = stale {
                leave_relays(&stale).await;
                tokio::spawn(async move { stale.close().await });
            }
            let result = bind(&key, local_only).await.map_err(error_text);
            let _ = inputs.send(Input::Endpoint { key: key.public(), result });
        });
    }

    fn endpoint_bound(&mut self, key: &str, result: Result<Endpoint, String>) {
        if self.binding.as_deref() == Some(key) {
            self.binding = None;
        }
        if key != self.key.public() {
            return;
        }
        match result {
            Ok(endpoint) => {
                tracing::info!("the endpoint is open");
                for link in self.servers.iter().filter_map(|server| server.link.as_ref()) {
                    link.redial_on(endpoint.clone());
                }
                if let Some(link) = &self.gateway.link {
                    link.redial_on(endpoint.clone());
                }
                if let Some(stale) = self.endpoint.replace(endpoint) {
                    tokio::spawn(async move { stale.close().await });
                }
                self.connect_servers();
                self.sync_gateway();
            }
            Err(error) => {
                tracing::error!("the network endpoint couldn't be opened: {error}");
                let error = format!("The network couldn't be opened: {error}");
                for link in self.servers.iter().filter_map(|server| server.link.as_ref()) {
                    link.no_endpoint(error.clone());
                }
                if let Some(link) = &self.gateway.link {
                    link.no_endpoint(error.clone());
                }
            }
        }
    }

    /// Has the links let go of the endpoint while a new one is bound, and wait for it.
    fn retire_links(&self) {
        for link in self.servers.iter().filter_map(|server| server.link.as_ref()) {
            link.retire();
        }
        if let Some(link) = &self.gateway.link {
            link.retire();
        }
    }

    /// The account's servers and the ones it removed, which aren't deleted completely, each with
    /// whether it was removed.
    fn wanted_servers(&self) -> Vec<(Device, bool)> {
        let linked = self.me.devices.iter().filter(|device| device.kind == DeviceKind::Server);
        let mut wanted: Vec<(Device, bool)> = linked.map(|device| (device.clone(), false)).collect();
        for gone in &self.me.removed {
            let known = wanted.iter().any(|(device, _)| device.public_key == gone.public_key);
            if known || self.me.deleted.contains(&gone.public_key) {
                continue;
            }
            let device = Device {
                public_key: gone.public_key.clone(),
                kind: DeviceKind::Server,
                name: gone.name.clone(),
                platform: gone.platform.clone(),
                created_at: gone.removed_at,
            };
            wanted.push((device, true));
        }
        wanted
    }

    /// Makes the servers match the account's: drops the ones that are gone, adds the new ones,
    /// and keeps the ones it removed as removed.
    fn sync_servers(&mut self) {
        let wanted = self.wanted_servers();
        let (kept, gone): (Vec<Server>, Vec<Server>) = std::mem::take(&mut self.servers)
            .into_iter()
            .partition(|server| wanted.iter().any(|(device, _)| device.public_key == server.device.public_key));
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
        for (device, removed) in wanted {
            if let Some(server) = self.servers.iter_mut().find(|server| server.device.public_key == device.public_key) {
                server.device = device;
                if server.removed != removed {
                    server.removed = removed;
                    if let Some(link) = server.link.take().filter(|_| removed) {
                        link.shutdown();
                        server.status = Status { state: State::Disconnected, ..Status::default() };
                    }
                }
                continue;
            }
            let server_id = device.public_key.clone();
            let cached = self.cache.threads(&server_id);
            let threads: HashMap<String, Thread> =
                cached.iter().map(|cached| (cached.thread.id.clone(), cached.thread.clone())).collect();
            let views: Vec<ThreadView> = cached
                .iter()
                .map(|cached| {
                    let children = children_of(&threads, &cached.thread.id);
                    view_of(&server_id, &cached.thread, cached.seen_at, children)
                })
                .collect();
            self.servers.push(Server {
                device,
                removed,
                link: None,
                status: Status::default(),
                away_since: Some(Instant::now()),
                info: self.cache.server_info(&server_id),
                limits_read_at: self.limits_read(&server_id).map(|(at, _)| at),
                limits: self.limits_read(&server_id).map(|(_, limits)| limits).unwrap_or_default(),
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
        self.schedule_relay(RELAY_SOON);
        self.sync_gateway();
    }

    // ---- gateway ----

    fn gateway_device(&self) -> Option<&Device> {
        self.me.devices.iter().find(|device| device.kind == DeviceKind::Gateway)
    }

    /// Connects to the account's gateway while a view shows it, and lets go of it otherwise or once
    /// it is another.
    fn sync_gateway(&mut self) {
        let key = self.gateway_device().map(|device| device.public_key.clone());
        if self.gateway.state.is_none() {
            self.gateway.state = self.cache.kept::<(String, GatewayState)>("gateway");
        }
        if self.gateway.state.as_ref().is_some_and(|(kept, _)| Some(kept) != key.as_ref()) {
            self.gateway.state = None;
            self.gateway.sign_in = None;
        }
        let wanted = key.filter(|_| self.gateway.watching);
        let linked = self.gateway.link.as_ref().map(|link| link.key.clone());
        if wanted != linked || self.gateway.link.is_some() && self.endpoint.is_none() {
            if let Some(link) = self.gateway.link.take() {
                link.shutdown();
            }
            self.gateway.status = gateway::Status::default();
            if let (Some(key), Some(endpoint)) = (wanted, self.endpoint.clone()) {
                let direct = self.config.direct_addrs.get(&key).or(self.config.direct_addr.as_ref());
                let address = ServerAddr { key: key.clone(), direct: direct.and_then(|address| address.parse().ok()) };
                let inputs = self.inputs.clone();
                let emit: gateway::Emit = Arc::new(move |event| {
                    let _ = inputs.send(Input::Gateway { key: key.clone(), event });
                });
                let link = GatewayLink::connect(endpoint, address, self.away.clone(), emit);
                if self.binding.is_some() {
                    link.retire();
                }
                self.gateway.link = Some(link);
            }
        }
        self.emit_gateway();
    }

    fn emit_gateway(&self) {
        let shown = gateway::Shown {
            device: self.gateway_device(),
            status: self.gateway.link.as_ref().map(|_| &self.gateway.status),
            state: self.gateway.state.as_ref().map(|(_, state)| state),
            sign_in: self.gateway.sign_in.as_ref(),
            now: now(),
            utc_offset_secs: utc_offset_secs(),
        };
        let view = gateway::view(shown, &Names(&self.servers));
        self.emit(Event::Gateway { gateway: Box::new(view) });
    }

    fn gateway_event(&mut self, key: &str, event: GatewayEvent) {
        if self.gateway.link.as_ref().is_none_or(|link| link.key != key) {
            return;
        }
        match event {
            GatewayEvent::Status(status) => self.gateway.status = status,
            GatewayEvent::State(state) => {
                if self.gateway.state.as_ref().is_some_and(|(_, known)| *known == state) {
                    return;
                }
                let kept = (key.to_string(), state);
                self.cache.keep("gateway", &kept);
                self.gateway.state = Some(kept);
            }
        }
        self.emit_gateway();
    }

    /// Sends a command to the gateway. The state it answers with is taken in and the command answered
    /// with nothing; any other answer goes to `gateway_answered`.
    fn gateway_manage(&self, id: u64, command: Manage) {
        let Some(link) = self.gateway.link.clone() else {
            return self.reply(id, Err("Open your gateway's settings first.".to_string()));
        };
        let (sink, inputs) = (self.sink.clone(), self.inputs.clone());
        tokio::spawn(async move {
            match link.manage(command).await {
                Ok(GatewayReply::State { state }) => {
                    let _ = inputs.send(Input::Gateway { key: link.key.clone(), event: GatewayEvent::State(state) });
                    reply(&sink, id, Ok(json!({})));
                }
                Ok(GatewayReply::Captured { request: Some(request) }) => {
                    let request = tokio::task::spawn_blocking(move || gateway::readable_request(request)).await;
                    let answer = request.map(|request| GatewayReply::Captured { request: Some(request) });
                    let _ = inputs.send(Input::GatewayAnswered { id, answer: answer.unwrap_or(GatewayReply::Ok) });
                }
                Ok(answer) => {
                    let _ = inputs.send(Input::GatewayAnswered { id, answer });
                }
                Err(error) => reply(&sink, id, Err(error_text(error))),
            }
        });
    }

    /// The input schema of an MCP server's tool, as the gateway last said it.
    fn mcp_server_tool_schema(&self, mcp_server: &str, tool: &str) -> Option<serde_json::Value> {
        let (_, state) = self.gateway.state.as_ref()?;
        let mcp_server = state.mcp_servers.as_ref()?.iter().find(|kept| kept.id == mcp_server)?;
        Some(mcp_server.tools.iter().find(|kept| kept.name == tool)?.input_schema.clone())
    }

    fn gateway_answered(&mut self, id: u64, answer: GatewayReply) {
        let accounts = self.gateway.state.as_ref().map(|(_, state)| state.accounts.as_slice()).unwrap_or_default();
        let kept_mcp_servers =
            self.gateway.state.as_ref().and_then(|(_, state)| state.mcp_servers.as_deref()).unwrap_or_default();
        let names = Names(&self.servers);
        let rows = |entries: &[_]| gateway::log_rows(entries, accounts, &names, now(), utc_offset_secs());
        let call_rows = |calls: &[_]| mcp_servers::call_rows(calls, kept_mcp_servers, &names, now(), utc_offset_secs());
        match answer {
            GatewayReply::McpAdding { adding } => {
                let answer = match adding {
                    McpAdding::Added { id } => json!({ "added": id }),
                    McpAdding::SignIn { state, url } => json!({ "sign_in": { "state": state, "url": url } }),
                    McpAdding::NeedsKey => json!({ "needs_key": true }),
                };
                self.reply(id, Ok(answer));
                if let Some(link) = self.gateway.link.clone() {
                    let inputs = self.inputs.clone();
                    tokio::spawn(async move {
                        if let Ok(GatewayReply::State { state }) = link.manage(Manage::State).await {
                            let _ = inputs
                                .send(Input::Gateway { key: link.key.clone(), event: GatewayEvent::State(state) });
                        }
                    });
                }
            }
            GatewayReply::McpCalls { calls } => {
                let more = calls.len() as u32 >= mcp_servers::CALLS_PAGE;
                self.reply(id, Ok(json!({ "rows": call_rows(&calls), "more": more })));
            }
            GatewayReply::McpCall { call } => {
                let view = call.map(|detail| {
                    let row = call_rows(std::slice::from_ref(&detail.call)).remove(0);
                    mcp_servers::call_view(detail, row)
                });
                self.reply(id, Ok(json!({ "call": view })));
            }
            GatewayReply::ToolResult { result } => {
                let answered = mcp_servers::answered(&result.result, result.error);
                self.reply(id, Ok(serde_json::to_value(answered).unwrap_or_default()));
            }
            GatewayReply::SignInStarted { sign_in } => self.gateway_sign_in_started(id, sign_in),
            GatewayReply::Log { entries } => {
                let more = entries.len() as u32 >= gateway::LOG_PAGE;
                self.reply(id, Ok(json!({ "rows": rows(&entries), "more": more })));
            }
            GatewayReply::Captured { request } => {
                let view = request.map(|request| {
                    let row = rows(std::slice::from_ref(&request.entry)).remove(0);
                    gateway::captured_view(request, row)
                });
                self.reply(id, Ok(json!({ "request": view })));
            }
            GatewayReply::Ok => self.reply(id, Ok(json!({}))),
            other => self.reply(id, Err(gateway::unexpected(&other))),
        }
    }

    /// Codex's sign-in is finished at once: the gateway answers once the user approved it.
    fn gateway_sign_in_started(&mut self, id: u64, started: GatewaySignIn) {
        self.reply(id, Ok(json!({ "url": started.url, "user_code": started.user_code })));
        let codex = started.agent == Agent::Codex;
        let sign_in_id = started.id.clone();
        let again = self.gateway.signing_in_again.take();
        self.gateway.sign_in = Some(gateway::SignIn { started, again, waiting: codex, error: None, task: None });
        if codex {
            self.gateway_finish_sign_in(sign_in_id, None, None);
        }
        self.emit_gateway();
    }

    fn gateway_finish_sign_in(&mut self, sign_in: String, code: Option<String>, reply_to: Option<u64>) {
        let Some(link) = self.gateway.link.clone() else {
            if let Some(id) = reply_to {
                self.reply(id, Err("Open your gateway's settings first.".to_string()));
            }
            return;
        };
        let inputs = self.inputs.clone();
        let command = Manage::FinishSignIn { id: sign_in.clone(), code };
        let task = tokio::spawn(async move {
            let result = match link.manage(command).await {
                Ok(GatewayReply::State { state }) => Ok(state),
                Ok(other) => Err(gateway::unexpected(&other)),
                Err(error) => Err(error_text(error)),
            };
            let _ = inputs.send(Input::GatewaySignInEnded { sign_in, reply_to, result });
        });
        if let Some(sign_in) = &mut self.gateway.sign_in {
            sign_in.waiting = true;
            sign_in.error = None;
            sign_in.task = Some(task.abort_handle());
        }
    }

    fn gateway_sign_in_ended(&mut self, sign_in: &str, reply_to: Option<u64>, result: Result<GatewayState, String>) {
        if let Some(id) = reply_to {
            self.reply(id, result.as_ref().map(|_| json!({})).map_err(Clone::clone));
        }
        let Some(current) = self.gateway.sign_in.as_mut().filter(|current| current.started.id == sign_in) else {
            return;
        };
        current.task = None;
        match result {
            Ok(state) => {
                self.gateway.sign_in = None;
                if let Some(link) = &self.gateway.link {
                    let key = link.key.clone();
                    return self.gateway_event(&key, GatewayEvent::State(state));
                }
            }
            Err(error) => {
                current.waiting = false;
                current.error = Some(error);
            }
        }
        self.emit_gateway();
    }

    fn connect_servers(&mut self) {
        let Some(endpoint) = &self.endpoint else { return };
        for server in self.servers.iter_mut().filter(|server| server.link.is_none() && !server.removed) {
            let server_id = server.device.public_key.clone();
            let direct = self.config.direct_addrs.get(&server_id).or(self.config.direct_addr.as_ref());
            let address =
                ServerAddr { key: server_id.clone(), direct: direct.and_then(|address| address.parse().ok()) };
            let link = Link::connect(endpoint.clone(), address, self.away.clone(), self.link_events.clone());
            if let Some(client) = &self.config.client {
                link.describe(client.info());
            }
            // Threads the client opened before there was a connection to follow them on.
            for (thread_id, open) in self.open.iter().filter(|(_, open)| open.server_id == server_id) {
                link.open(thread_id.clone(), open.rev);
            }
            for (thread_id, followed) in self.followed.iter().filter(|(_, followed)| followed.server_id == server_id) {
                link.open(thread_id.clone(), followed.rev);
            }
            if self.binding.is_some() {
                link.retire();
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
            removed: server.removed,
            state: server.status.state,
            error: server.status.error.clone(),
            path: server.status.path,
            rtt_ms: server.status.rtt_ms,
            info: server.info.clone(),
            short_model_names: server
                .info
                .iter()
                .flat_map(|info| &info.models)
                .filter_map(|model| Some((model.id.clone(), short_model_name(&model.name)?.to_string())))
                .collect(),
            roomier_accounts: server
                .info
                .as_ref()
                .map(|info| accounts(server, info, &working(server, info)).roomier(&info.models, now()))
                .unwrap_or_default(),
            account_limits: server
                .info
                .as_ref()
                .map(|info| limits::by_account(&info.agent_accounts, &server.limits, now()))
                .unwrap_or_default(),
            next_accounts: server
                .info
                .as_ref()
                .map(|info| limits::next_accounts(&info.agent_accounts, &server.limits))
                .unwrap_or_default(),
            accounts_by_room: server
                .info
                .as_ref()
                .map(|info| accounts(server, info, &working(server, info)).by_room(now()))
                .unwrap_or_default(),
            sync: self.sync_view(server),
        };
        self.emit(Event::Servers { servers: self.servers.iter().map(view).collect() });
        let infos = self.servers.iter().filter(|server| !server.removed).filter_map(|server| server.info.as_ref());
        self.emit(Event::Backup { backup: backup::view(self.backup.saved.bucket.as_ref(), infos) });
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
        if let Some(server) = server.filter(|server| server.removed) {
            let name = &server.device.name;
            return Err(format!(
                "“{name}” is unlinked, so its threads are read-only. You can delete “{name}” in settings to remove them."
            ));
        }
        server.and_then(|server| server.link.clone()).ok_or_else(|| NOT_CONNECTED.to_string())
    }

    /// Tells the client about a server's projects, each with its icon if this device has the file.
    /// Icons it doesn't have yet are fetched, and the projects are told again when they arrive.
    fn emit_projects(&mut self, server_id: &str, projects: Vec<Project>) {
        let folder = self.config.data_dir.join("icons");
        let threads =
            self.servers.iter().find(|server| server.device.public_key == server_id).map(|server| &server.threads);
        let keys: Vec<String> = projects.iter().map(|project| project_key(server_id, project, &projects)).collect();
        let view = |(project, key): (Project, String)| {
            let file = project.icon.as_ref().map(|icon| folder.join(format!("{}-{icon}", project.id)));
            let icon_path = file.filter(|file| file.is_file()).map(|file| file.to_string_lossy().into_owned());
            let git_control = project.git.as_ref().map(|git| git::control(git, None));
            // A worktree's button names the pull request of the thread that works in it.
            let own = |worktree: &Worktree| {
                let thread =
                    threads?.values().find(|thread| thread.project_id == project.id && thread.cwd == worktree.path);
                thread?.pull_request.as_ref()
            };
            let controlled = |worktree: &Worktree| {
                Some((worktree.path.clone(), git::control(worktree.git.as_ref()?, own(worktree))))
            };
            let worktree_controls = project.worktrees.iter().filter_map(controlled).collect();
            ProjectView { project, key, icon_path, git_control, worktree_controls }
        };
        let views: Vec<ProjectView> = projects.into_iter().zip(keys).map(view).collect();
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

    /// Answers with where the image, the video or the file is on this device, fetching it first if
    /// it isn't.
    fn find_media(&mut self, id: u64, server_id: &str, media_id: String) {
        if let Some(file) = self.media.get(&media_id) {
            return self.reply(id, Ok(json!({ "path": file.to_string_lossy() })));
        }
        if let Some((waiting, _)) = self.media_waiting.get_mut(&media_id) {
            return waiting.push(id);
        }
        let Some(unfinished) = self.media.unfinished(&media_id) else {
            return self.reply(id, Err("That isn't the name of a file your server keeps.".to_string()));
        };
        let server = self.servers.iter().find(|server| server.device.public_key == server_id);
        let link = match server.filter(|server| server.removed) {
            Some(server) => match self.bucket_holder(server_id) {
                Some(link) => link,
                None => {
                    let only_there = format!("Only “{}” had this, and it's unlinked.", server.device.name);
                    return self.reply(id, Err(only_there));
                }
            },
            None => match self.link(server_id) {
                Ok(link) => link,
                Err(error) => return self.reply(id, Err(error)),
            },
        };
        let (cache, inputs, sink, waiting_key) =
            (self.media.clone(), self.inputs.clone(), self.sink.clone(), media_id.clone());
        let limit = self.media_limit;
        let fetch = tokio::spawn(async move {
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
        self.media_waiting.insert(waiting_key, (vec![id], fetch.abort_handle()));
    }

    fn cancel_media(&mut self, media_id: &str) {
        let Some((waiting, fetch)) = self.media_waiting.remove(media_id) else { return };
        fetch.abort();
        if let Some(unfinished) = self.media.unfinished(media_id) {
            let _ = std::fs::remove_file(unfinished);
        }
        for waiting in waiting {
            self.reply(waiting, Err("The download was stopped.".to_string()));
        }
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
        let threads = self.servers.iter().find(|server| server.device.public_key == server_id);
        let children = threads.and_then(|server| children_of(&server.threads, &thread.id));
        view_of(server_id, thread, self.cache.seen_at(&thread.id), children)
    }

    /// Shows the open thread's agents again once a thread one of them works in changed how it is
    /// doing, which their tool calls say only later.
    fn refresh_agents(&mut self, thread_id: &str) {
        let Some(open) = self.open.get_mut(thread_id) else { return };
        let Some(server) = self.servers.iter().find(|server| server.device.public_key == open.server_id) else {
            return;
        };
        let threads = ThreadState::of_children(&server.threads, thread_id);
        if threads == open.threads {
            return;
        }
        open.threads = threads;
        let splice = open.transcript.set_agents(&open.agents, &open.threads);
        let agents = agents_event(thread_id, &open.agents, &open.threads);
        if let Some(splice) = splice {
            self.emit_rows(thread_id, false, splice);
        }
        self.emit(agents);
    }

    /// Tells the clients again of the threads whose agents started the thread's, which show how
    /// it is doing.
    fn emit_ancestors(&mut self, server_id: &str, parent: Option<&str>) {
        let Some(server) = self.servers.iter().find(|server| server.device.public_key == server_id) else { return };
        let mut seen = HashSet::new();
        let mut ancestors = Vec::new();
        let mut next = parent.map(String::from);
        while let Some(id) = next.filter(|id| seen.insert(id.clone())) {
            let Some(thread) = server.threads.get(&id) else { break };
            next = thread.parent.as_ref().map(|parent| parent.thread_id.clone());
            ancestors.push(thread.clone());
        }
        for thread in ancestors {
            let view = self.thread_view(server_id, &thread);
            self.emit(Event::ThreadUpsert { thread: Box::new(view) });
        }
    }

    // ---- copies of the servers' threads ----

    /// Whether the server has every thread of the account's other servers it can reach.
    fn sync_view(&self, server: &Server) -> Option<SyncView> {
        if server.removed {
            return None;
        }
        if let Some(view) = self.backup_line(server) {
            return Some(view);
        }
        let id = &server.device.public_key;
        let synced_at = self.relay.synced_at.get(id).copied();
        let view = |state, text: &str| Some(SyncView { state, text: text.into(), error: None, synced_at });
        let others: Vec<&Server> = self.servers.iter().filter(|other| other.device.public_key != *id).collect();
        if others.is_empty() {
            return Some(SyncView {
                state: SyncState::Off,
                text: "Nothing to sync with yet".into(),
                error: None,
                synced_at: None,
            });
        }
        if server.info.as_ref().is_some_and(|info| info.protocol < SYNC_PROTOCOL) {
            return Some(SyncView {
                state: SyncState::Off,
                text: "Update it to sync".into(),
                error: None,
                synced_at: None,
            });
        }
        if server.status.state != State::Connected {
            return view(SyncState::Off, "Not syncing");
        }
        if self.carries_visibly(id) {
            let copying: Vec<&(relay::Transfer, u64)> =
                self.relay.copying.iter().filter(|(transfer, _)| transfer.target == *id).collect();
            let total: u64 = copying.iter().map(|(transfer, _)| transfer.until.saturating_sub(transfer.after)).sum();
            let done: u64 = copying
                .iter()
                .map(|(transfer, reached)| reached.min(&transfer.until).saturating_sub(transfer.after))
                .sum();
            let text = format!("Syncing · {}%", done * 100 / total.max(1));
            return view(SyncState::Copying, &text);
        }
        if let Some(failure) = self.relay.failed.get(id) {
            let from = failure
                .origin
                .as_ref()
                .and_then(|origin| self.servers.iter().find(|server| server.device.public_key == *origin));
            let text = match from {
                Some(from) => format!("Couldn't sync from {}", from.device.name),
                None => "Couldn't sync".into(),
            };
            return Some(SyncView { state: SyncState::Failed, text, error: Some(failure.error.clone()), synced_at });
        }
        if self.relay.waiting.contains(id) {
            return view(SyncState::Waiting, "Paused until Wi-Fi");
        }
        let peers = self.peers();
        if !others.iter().any(|other| other.removed || peers.contains_key(&other.device.public_key)) {
            return view(SyncState::Waiting, "Others offline");
        }
        match synced_at {
            Some(_) => view(SyncState::Synced, "Synced"),
            None => view(SyncState::Copying, "Syncing"),
        }
    }

    fn backs_up(&self) -> bool {
        let uses_it =
            |server: &Server| !server.removed && server.info.as_ref().is_some_and(|info| info.backup.is_some());
        self.backup.saved.bucket.is_some() || self.servers.iter().any(uses_it)
    }

    /// What the line under a connected server says of its backup, while the account has one and
    /// no client carries threads to it.
    fn backup_line(&self, server: &Server) -> Option<SyncView> {
        let info = server.info.as_ref().filter(|_| self.backs_up() && server.status.state == State::Connected)?;
        if info.protocol < BACKUP_PROTOCOL {
            let view =
                SyncView { state: SyncState::Off, text: "Update it to back up".into(), error: None, synced_at: None };
            return Some(view);
        }
        let id = &server.device.public_key;
        if self.relay.failed.contains_key(id) {
            return None;
        }
        let line = backup::line(info.backup.as_ref(), self.backup.failed.get(id), self.backup.saved.bucket.is_some());
        // The server's own work comes first; what the client carries to it shows only while the
        // backup has nothing to say, so the line never swaps between the two.
        let busy = matches!(line.state, SyncState::Copying | SyncState::Failed);
        if !busy && self.carries_visibly(id) {
            return None;
        }
        Some(line)
    }

    /// Whether the running round still carries to the server and has run long enough to say so.
    fn carries_visibly(&self, id: &str) -> bool {
        self.relay.quiet_since.is_none()
            && self.relay.copying.iter().any(|(transfer, reached)| transfer.target == id && *reached < transfer.until)
    }

    /// Gives the bucket this device keeps to a connected server that has none.
    fn give_backup(&mut self, server_id: &str) {
        let Some(bucket) = self.backup.saved.bucket.clone() else { return };
        let Some(server) = self.servers.iter().find(|server| server.device.public_key == server_id) else { return };
        let lacks = server.info.as_ref().is_some_and(|info| info.protocol >= BACKUP_PROTOCOL && info.backup.is_none());
        let Some(link) = server.link.clone().filter(|_| lacks && !server.removed) else { return };
        if self.backup.failed.contains_key(server_id) || !self.backup.giving.insert(server_id.to_string()) {
            return;
        }
        let (inputs, server_id) = (self.inputs.clone(), server_id.to_string());
        tokio::spawn(async move {
            let result =
                link.request(&Request::SetBackup { bucket: Some(bucket) }).await.map(|_| ()).map_err(error_text);
            let _ = inputs.send(Input::BackupGiven { server_id, result });
        });
    }

    /// The connected servers that can back up, the first one first.
    fn backup_links(&self) -> Vec<(String, Arc<Link>)> {
        let mut links: Vec<(String, Arc<Link>)> = self
            .servers
            .iter()
            .filter(|server| !server.removed && server.status.state == State::Connected)
            .filter(|server| server.info.as_ref().is_some_and(|info| info.protocol >= BACKUP_PROTOCOL))
            .filter_map(|server| Some((server.device.public_key.clone(), server.link.clone()?)))
            .collect();
        links.sort_by(|a, b| a.0.cmp(&b.0));
        links
    }

    /// Has the first connected server check the bucket, then keeps it and gives it to the others;
    /// or stops every connected server's backup.
    fn set_backup(&mut self, id: u64, bucket: Option<Bucket>) {
        let links = self.backup_links();
        if bucket.is_some() && links.is_empty() {
            return self.reply(id, Err("Connect one of your servers so it can check the bucket.".into()));
        }
        let (sink, inputs) = (self.sink.clone(), self.inputs.clone());
        tokio::spawn(async move {
            let request = Request::SetBackup { bucket: bucket.clone() };
            let mut links = links.into_iter();
            if bucket.is_some() {
                let Some((server_id, first)) = links.next() else { return };
                if let Err(error) = first.request(&request).await {
                    return reply(&sink, id, Err(error_text(error)));
                }
                let _ = inputs.send(Input::BackupGiven { server_id, result: Ok(()) });
            }
            let _ = inputs.send(Input::BackupChecked { bucket });
            reply(&sink, id, Ok(json!({})));
            for (server_id, link) in links {
                let result = link.request(&request).await.map(|_| ()).map_err(error_text);
                let _ = inputs.send(Input::BackupGiven { server_id, result });
            }
        });
    }

    fn schedule_relay(&mut self, delay: Duration) {
        if self.relay.timer.is_some() || !self.signed_in_now() {
            return;
        }
        let inputs = self.inputs.clone();
        let timer = tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = inputs.send(Input::Relay);
        });
        self.relay.timer = Some(timer.abort_handle());
    }

    /// The connected servers that keep copies of the others' threads.
    fn peers(&self) -> HashMap<String, Arc<Link>> {
        let syncs = |server: &&Server| {
            server.status.state == State::Connected
                && server.info.as_ref().is_some_and(|info| info.protocol >= SYNC_PROTOCOL)
        };
        let peers = self.servers.iter().filter(syncs);
        peers.filter_map(|server| Some((server.device.public_key.clone(), server.link.clone()?))).collect()
    }

    /// Copies what each connected server lacks of the account's threads, one round at a time.
    fn start_relay(&mut self) {
        self.relay.timer = None;
        if self.relay.running {
            self.relay.again = true;
            return;
        }
        let peers = self.peers();
        if peers.is_empty() {
            return;
        }
        let origin = |server: &Server| {
            let origin = Origin { name: server.device.name.clone(), platform: server.device.platform.clone() };
            (server.device.public_key.clone(), origin)
        };
        let origins = self.servers.iter().map(origin).collect();
        let (inputs, metered) = (self.inputs.clone(), self.relay.metered);
        self.relay.running = true;
        tokio::spawn(async move {
            let progress = inputs.clone();
            let report = move |step| drop(progress.send(Input::RelayProgress(step)));
            let round = relay::round(peers, origins, metered, report).await;
            let _ = inputs.send(Input::Relayed(round));
        });
    }

    fn relay_progress(&mut self, progress: relay::Progress) {
        match progress {
            relay::Progress::Planned(transfers) => {
                self.relay.copying = transfers.into_iter().map(|transfer| (transfer.clone(), transfer.after)).collect();
                self.relay.quiet_since = Some(Instant::now());
            }
            relay::Progress::Carried { origin, target, mark } => {
                let mut copying = self.relay.copying.iter_mut();
                let Some((_, reached)) =
                    copying.find(|(transfer, _)| transfer.origin == origin && transfer.target == target)
                else {
                    return;
                };
                *reached = mark.change;
            }
        }
        self.emit_servers();
    }

    fn relayed(&mut self, round: relay::Round) {
        self.relay.running = false;
        self.relay.copying.clear();
        self.relay.quiet_since = None;
        let origins: Vec<String> = self.servers.iter().map(|server| server.device.public_key.clone()).collect();
        let behind: HashSet<String> =
            relay::plan(&round.marks, &origins, false).into_iter().map(|transfer| transfer.target).collect();
        self.relay.waiting = if self.relay.metered { behind.clone() } else { HashSet::new() };
        for server_id in round.marks.keys() {
            if !round.failed.contains_key(server_id) && !behind.contains(server_id) {
                self.relay.synced_at.insert(server_id.clone(), now());
            }
        }
        let furthest = |marks: &HashMap<String, relay::Marks>, origin: &str| {
            let copies = marks.values().filter_map(|server| server.copies.get(origin));
            copies.map(|copy| (copy.change, copy.log_id.clone())).max()
        };
        let mut changed = round.changed;
        for server in self.servers.iter().filter(|server| server.removed) {
            let origin = &server.device.public_key;
            if furthest(&self.relay.marks, origin) != furthest(&round.marks, origin) {
                changed.insert(origin.clone());
            }
        }
        self.relay.failed = round.failed;
        self.relay.marks = round.marks;
        self.emit_servers();
        let removed: Vec<String> = self
            .servers
            .iter()
            .filter(|server| server.removed)
            .map(|server| server.device.public_key.clone())
            .collect();
        for origin in removed {
            if changed.contains(&origin) || !self.relay.read.contains(&origin) {
                self.fetch_copies(&origin);
            }
        }
        if std::mem::take(&mut self.relay.again) {
            self.schedule_relay(RELAY_SOON);
        }
    }

    /// The connected server whose copy of the removed server's threads goes furthest.
    fn holder(&self, origin: &str) -> Option<Arc<Link>> {
        let peers = self.peers();
        let holder = relay::best_holder(&self.relay.marks, origin).and_then(|holder| peers.get(&holder).cloned());
        holder.or_else(|| {
            let mut ids: Vec<&String> = peers.keys().collect();
            ids.sort();
            ids.first().and_then(|id| peers.get(*id).cloned())
        })
    }

    /// The connected server that keeps a copy of the removed server's threads and can fetch its
    /// media from the bucket.
    fn bucket_holder(&self, origin: &str) -> Option<Arc<Link>> {
        let peers = self.peers();
        let backs_up = |id: &String| {
            let server = self.servers.iter().find(|server| server.device.public_key == *id);
            server.and_then(|server| server.info.as_ref()).is_some_and(|info| info.backup.is_some())
        };
        let best = relay::best_holder(&self.relay.marks, origin).filter(|id| backs_up(id) && peers.contains_key(id));
        let mut ids: Vec<&String> = peers.keys().filter(|id| backs_up(id)).collect();
        ids.sort();
        let id = best.or_else(|| ids.first().map(|id| (*id).clone()))?;
        peers.get(&id).cloned()
    }

    fn fetch_copies(&mut self, origin: &str) {
        let Some(link) = self.holder(origin) else { return };
        let (inputs, origin) = (self.inputs.clone(), origin.to_string());
        tokio::spawn(async move {
            match link.request(&Request::Copies { origin: origin.clone() }).await {
                Ok(Message::Copies { projects, threads }) => {
                    let _ = inputs.send(Input::CopiesFetched { origin, projects, threads });
                }
                Ok(other) => tracing::debug!("unexpected answer to copies: {other:?}"),
                Err(error) => tracing::debug!(origin, "couldn't read a removed server's threads: {error:#}"),
            }
        });
    }

    /// Shows a removed server's threads as the copy has them. None of them works any more.
    fn copies_fetched(&mut self, origin: &str, projects: Vec<Project>, threads: Vec<Thread>) {
        if !self.servers.iter().any(|server| server.device.public_key == origin && server.removed) {
            return;
        }
        self.relay.read.insert(origin.to_string());
        let threads: Vec<Thread> = threads.into_iter().map(stopped).collect();
        self.cache.set_threads(origin, &threads);
        self.cache.set_projects(origin, &projects);
        let behind: Vec<String> = threads
            .iter()
            .filter(|thread| self.open.contains_key(&thread.id) && self.cache.synced_rev(&thread.id) < thread.rev)
            .map(|thread| thread.id.clone())
            .collect();
        let Some(server) = self.server_mut(origin) else { return };
        server.threads = threads.iter().map(|thread| (thread.id.clone(), thread.clone())).collect();
        let views = threads.iter().map(|thread| self.thread_view(origin, thread)).collect();
        self.emit(Event::Threads { server_id: origin.to_string(), threads: views });
        self.emit_projects(origin, projects);
        for thread_id in behind {
            self.open_copy(origin, &thread_id);
        }
    }

    /// Reads the transcript of a removed server's thread from a server's copy of it.
    fn open_copy(&self, origin: &str, thread_id: &str) {
        let Some(link) = self.holder(origin) else { return };
        let (inputs, origin, thread_id) = (self.inputs.clone(), origin.to_string(), thread_id.to_string());
        tokio::spawn(async move {
            let result = link.open_copy(&origin, &thread_id).await.map_err(error_text);
            let _ = inputs.send(Input::CopyOpened { thread_id, result });
        });
    }

    /// Shows the transcript the copy has, in place of what the cache had.
    fn copy_opened(&mut self, thread_id: &str, result: Result<Vec<Item>, String>) {
        let Some(open) = self.open.get(thread_id) else { return };
        let items = match result {
            Ok(items) => items,
            Err(error) => return tracing::debug!(thread_id, "couldn't read a removed server's thread: {error}"),
        };
        let server = self.servers.iter().find(|server| server.device.public_key == open.server_id);
        let rev = server.and_then(|server| server.threads.get(thread_id)).map_or(0, |thread| thread.rev);
        let rev = items.iter().map(|item| item.rev).max().unwrap_or(0).max(rev);
        self.thread_message(thread_id, Message::Opened { reset: true, activity: Activity::default() });
        self.thread_message(thread_id, Message::Items { items });
        self.thread_message(thread_id, Message::Synced { rev });
    }

    // ---- what the servers say ----

    fn link_event(&mut self, server_id: &str, event: LinkEvent) {
        match event {
            LinkEvent::Status(status) => {
                let refused = status.state == State::Refused;
                let connected = status.state == State::Connected;
                let Some(server) = self.server_mut(server_id) else { return };
                server.away_since = match status.state {
                    State::Connected => None,
                    _ => server.away_since.or_else(|| Some(Instant::now())),
                };
                server.status = status;
                self.emit_servers();
                if connected {
                    self.send_outbox();
                }
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
                let Some(server) = self.server_mut(server_id) else { return };
                server.info = Some(info);
                self.reprogram(server_id, true);
                let Some(server) = self.server_mut(server_id) else { return };
                server.threads = threads.iter().map(|thread| (thread.id.clone(), thread.clone())).collect();
                let views = threads.iter().map(|thread| self.thread_view(server_id, thread)).collect();
                let Some(server) = self.server_mut(server_id) else { return };
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
                    self.refresh_agents(&thread_id);
                }
                self.read_limits(server_id);
                self.schedule_relay(RELAY_SOON);
                self.backup.failed.remove(server_id);
                self.give_backup(server_id);
            }
            Message::Server { server: info } => {
                self.cache.set_server_info(server_id, &info);
                let Some(server) = self.server_mut(server_id) else { return };
                let backup_changed = server.info.as_ref().map(|before| &before.backup) != Some(&info.backup);
                server.info = Some(info);
                self.reprogram(server_id, false);
                self.emit_servers();
                self.give_backup(server_id);
                if backup_changed {
                    self.schedule_relay(RELAY_SOON);
                }
            }
            Message::Projects { projects } => {
                self.cache.set_projects(server_id, &projects);
                self.emit_projects(server_id, projects);
            }
            Message::ThreadUpsert { thread } => {
                self.cache.upsert_thread(server_id, &thread);
                let Some(server) = self.server_mut(server_id) else { return };
                let thread_id = thread.id.clone();
                let pull_request = thread.pull_request.clone();
                let parent = thread.parent.as_ref().map(|parent| parent.thread_id.clone());
                let before = server.threads.insert(thread_id.clone(), thread.clone());
                let view = self.thread_view(server_id, &thread);
                self.emit(Event::ThreadUpsert { thread: Box::new(view) });
                self.emit_ancestors(server_id, parent.as_deref());
                if let Some(parent) = &parent {
                    self.refresh_agents(parent);
                }
                if before.and_then(|before| before.pull_request) != pull_request {
                    let projects = self.cache.projects(server_id);
                    self.emit_projects(server_id, projects);
                }
                self.refollow(server_id, &thread_id);
                self.schedule_relay(RELAY_AFTER_CHANGE);
            }
            Message::ThreadDeleted { thread_id } => {
                self.cache.remove_thread(&thread_id);
                self.open.remove(&thread_id);
                self.followed.remove(&thread_id);
                let removed = self.server_mut(server_id).and_then(|server| server.threads.remove(&thread_id));
                self.emit(Event::ThreadDeleted { thread_id });
                let parent = removed.and_then(|thread| thread.parent).map(|parent| parent.thread_id);
                self.emit_ancestors(server_id, parent.as_deref());
                if let Some(parent) = &parent {
                    self.refresh_agents(parent);
                }
                self.schedule_relay(RELAY_AFTER_CHANGE);
            }
            other => tracing::debug!("unexpected message on the thread list: {other:?}"),
        }
    }

    /// Reads what the server's agents' accounts have used, unless that is recent, so that the usage
    /// view opens with it current.
    fn read_limits(&self, server_id: &str) {
        let Some(server) = self.servers.iter().find(|server| server.device.public_key == server_id) else { return };
        let (Some(link), Some(info)) = (server.link.clone(), server.info.as_ref()) else { return };
        if info.protocol < LIMITS_PROTOCOL || self.kept_limits(server_id).is_some_and(|(fresh, _)| fresh) {
            return;
        }
        let (inputs, server_id) = (self.inputs.clone(), server_id.to_string());
        tokio::spawn(async move {
            let Ok(Message::Limits { agents }) = link.request(&Request::Limits).await else {
                return;
            };
            let _ = inputs.send(keep_limits(&server_id, &agents));
        });
    }

    fn limits_kept(&mut self, server_id: &str) {
        let Some((at, limits)) = self.limits_read(server_id) else { return };
        let Some(server) = self.servers.iter_mut().find(|server| server.device.public_key == server_id) else { return };
        if server.limits == limits && server.limits_read_at == Some(at) {
            return;
        }
        server.limits = limits;
        server.limits_read_at = Some(at);
        self.emit_servers();
    }

    /// What the server last said of its agents' accounts, and whether it is recent.
    fn kept_limits(&self, server_id: &str) -> Option<(bool, Vec<AgentLimits>)> {
        let (at, agents) = self.limits_read(server_id)?;
        Some((now() - at < LIMITS_STALE.as_secs_f64(), agents))
    }

    /// What the server last said of its agents' accounts, and when.
    fn limits_read(&self, server_id: &str) -> Option<(f64, Vec<AgentLimits>)> {
        self.cache.kept(&format!("limits:{server_id}"))
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
                let mut cleared = Vec::new();
                if reset {
                    cleared = open.programs.clear();
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
                self.carry_out(thread_id, cleared);
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
                    if let Some(agent) = agents::view(item, &open.threads) {
                        agents_changed = true;
                        note_agent(&mut open.agents, item);
                        if let Some((_, transcript)) = open.agent.as_mut().filter(|(id, _)| id == &item.id) {
                            agent_splices.extend(transcript.set_settled(agent.status != ToolStatus::Running));
                            let workflow = agents::workflow_items(item);
                            agent_splices
                                .extend(workflow.into_iter().filter_map(|child| transcript.upsert(child, false)));
                        }
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
                if agents_changed {
                    splices.extend(open.transcript.set_agents(&open.agents, &open.threads));
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
                let agents = agents_changed.then(|| agents_event(thread_id, &open.agents, &open.threads));
                let restored: Vec<Out> = items.iter().flat_map(|item| open.programs.saved(item)).collect();
                self.carry_out(thread_id, restored);
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
                let settled = if activity.running { None } else { open.transcript.end_streaming() };
                self.cache.set_activity(thread_id, &activity);
                let (queued, event) = activity_changed(thread_id, &mut open.transcript, activity);
                if let Some(settled) = settled {
                    self.emit_rows(thread_id, false, settled);
                }
                if let Some(queued) = queued {
                    self.emit_rows(thread_id, false, queued);
                }
                self.emit(event);
            }
            Message::Error { message } => {
                let left = open.left.is_some();
                self.open.remove(thread_id);
                if left {
                    return;
                }
                self.emit(Event::ThreadError { thread_id: thread_id.to_string(), message });
            }
            other => tracing::debug!("unexpected message on a thread: {other:?}"),
        }
    }

    fn emit_rows(&mut self, thread_id: &str, reset: bool, mut splice: Splice) {
        if self.is_left(thread_id) {
            return;
        }
        self.dress(thread_id, &mut splice.rows);
        let finds = self.open.get_mut(thread_id).is_some_and(|open| {
            let Some(find) = open.find.as_mut() else { return false };
            for row in &splice.rows {
                let Some(seq) = open.transcript.item(&row.item).map(|item| item.seq) else { continue };
                find.update(seq, row);
            }
            true
        });
        let place = self.places.get(&(thread_id.to_string(), None)).filter(|_| reset);
        self.emit(Event::Rows {
            thread_id: thread_id.to_string(),
            reset,
            start: splice.start,
            remove: splice.remove,
            at: place.and_then(|place| place.among(&splice.rows)),
            rows: splice.rows,
            earlier: self.open.get(thread_id).is_some_and(|open| open.earlier),
        });
        if finds {
            self.emit_found(thread_id);
        }
    }

    /// Finds `query` in the thread, reading its rows from the cache the first time.
    fn find(&mut self, thread_id: &str, query: &str, from: Option<FindFrom>, item_id: Option<String>) {
        let Some(open) = self.open.get_mut(thread_id) else { return };
        let place = from.and_then(|from| {
            let row = open.transcript.rows().iter().find(|row| row.id == from.row_id)?;
            Some((open.transcript.item(&row.item)?.seq, find::block(&row.id), from.offset))
        });
        let find = match open.find.as_mut() {
            Some(find) => find,
            None => {
                let mut items = self.cache.thread_items(thread_id);
                for item in open.transcript.items() {
                    match items.iter().position(|cached| cached.id == item.id) {
                        Some(index) => items[index] = item.clone(),
                        None => items.push(item.clone()),
                    }
                }
                items.sort_by_key(|item| item.seq);
                let (inputs, thread_id, cwd) =
                    (self.inputs.clone(), thread_id.to_string(), open.transcript.cwd().to_string());
                tokio::task::spawn_blocking(move || {
                    let entries = find::index(&items, &cwd);
                    let _ = inputs.send(Input::FindIndexed { thread_id, entries });
                });
                open.find.insert(Find::default())
            }
        };
        find.from = place;
        find.item = item_id.clone();
        let moved = find.search(query, place, item_id.as_deref());
        self.emit_found(thread_id);
        self.reveal_searched(thread_id, moved);
    }

    /// Brings the current match into sight when it moved, or the message the sidebar's search
    /// found once the thread is searched and that has no match of its own.
    fn reveal_searched(&mut self, thread_id: &str, moved: bool) {
        let Some(find) = self.open.get(thread_id).and_then(|open| open.find.as_ref()) else { return };
        let item = find.item.clone().filter(|_| find.ready && find.current_index().is_none());
        if let Some(item) = item {
            self.reveal_item(thread_id, &item);
        } else if moved {
            self.reveal_found(thread_id);
        }
    }

    fn find_indexed(&mut self, thread_id: &str, entries: Vec<find::Entry>) {
        let Some(open) = self.open.get_mut(thread_id) else { return };
        let Some(find) = open.find.as_mut() else { return };
        find.set_entries(entries);
        for row in open.transcript.rows() {
            let Some(seq) = open.transcript.item(&row.item).map(|item| item.seq) else { continue };
            find.update(seq, row);
        }
        let (query, from, item) = (find.query().to_string(), find.from, find.item.clone());
        let moved = find.search(&query, from, item.as_deref());
        self.emit_found(thread_id);
        self.reveal_searched(thread_id, moved);
    }

    fn emit_found(&mut self, thread_id: &str) {
        let Some(open) = self.open.get(thread_id) else { return };
        let Some(find) = open.find.as_ref() else { return };
        let event = Event::Found {
            thread_id: thread_id.to_string(),
            query: find.query().to_string(),
            count: find.count(),
            current: find.current_index(),
            rows: find.found_rows(open.transcript.rows()),
            ready: find.ready,
        };
        self.emit(event);
    }

    /// Brings the current match into sight: its turn loaded, its fold open.
    fn reveal_found(&mut self, thread_id: &str) {
        let hit = self.open.get(thread_id).and_then(|open| open.find.as_ref()?.current());
        let Some(hit) = hit else { return };
        self.show_row(thread_id, hit.seq, &hit.row_id);
        self.emit(Event::Reveal {
            thread_id: thread_id.to_string(),
            row_id: hit.row_id,
            range: Some([hit.start, hit.len]),
        });
    }

    fn reveal_item(&mut self, thread_id: &str, item_id: &str) {
        let Some(item) = self.cache.item(thread_id, item_id) else { return };
        let Some(open) = self.open.get(thread_id) else { return };
        let row_id = crate::render::rows::rendered(&item, open.transcript.cwd()).first().map(|row| row.id.clone());
        let Some(row_id) = row_id else { return };
        self.show_row(thread_id, item.seq, &row_id);
        self.emit(Event::Reveal { thread_id: thread_id.to_string(), row_id, range: None });
    }

    /// Loads the turns back to the item at `seq` and opens the fold the row is in.
    fn show_row(&mut self, thread_id: &str, seq: u64, row_id: &str) {
        while self
            .open
            .get(thread_id)
            .is_some_and(|open| open.earlier && open.transcript.first_seq().is_some_and(|first| first > seq))
        {
            self.load_earlier(thread_id);
        }
        let splice = self.open.get_mut(thread_id).and_then(|open| open.transcript.show_row(row_id));
        if let Some(splice) = splice {
            self.emit_rows(thread_id, false, splice);
        }
    }

    fn emit_agent_rows(&mut self, thread_id: &str, reset: bool, mut splice: Splice) {
        let shown = self.open.get(thread_id).filter(|open| open.left.is_none());
        let Some((agent_id, _)) = shown.and_then(|open| open.agent.as_ref()) else { return };
        let agent_id = agent_id.clone();
        self.dress(thread_id, &mut splice.rows);
        let place = self.places.get(&(thread_id.to_string(), Some(agent_id.clone()))).filter(|_| reset);
        self.emit(Event::AgentRows {
            thread_id: thread_id.to_string(),
            agent_id,
            reset,
            start: splice.start,
            remove: splice.remove,
            at: place.and_then(|place| place.among(&splice.rows)),
            rows: splice.rows,
        });
    }

    /// Runs the programs among the rows about to be sent, and puts their trees in them.
    fn dress(&mut self, thread_id: &str, rows: &mut [Row]) {
        if !rows.iter().any(|row| matches!(row.kind, RowKind::Ui { .. })) {
            return;
        }
        self.refresh_programs(thread_id);
        let Some(open) = self.open.get_mut(thread_id) else { return };
        let (transcript, agent) = (&open.transcript, open.agent.as_ref().map(|(_, transcript)| transcript));
        let saved = |item_id: &str, block: u32| {
            let item = transcript.item(item_id).or_else(|| agent.and_then(|agent| agent.item(item_id)));
            item.and_then(|item| item.ui.get(&block).cloned())
        };
        let outs = open.programs.dress(rows, &saved);
        self.carry_out(thread_id, outs);
    }

    /// Tells the thread's programs what its server can do and how much the thread may.
    fn refresh_programs(&mut self, thread_id: &str) {
        let Some(open) = self.open.get(thread_id) else { return };
        let server = self.servers.iter().find(|server| server.device.public_key == open.server_id);
        let limited = runs_no_programs(server.and_then(|server| server.info.as_ref()));
        let thread = server.and_then(|server| server.threads.get(thread_id));
        let full_access = thread.is_some_and(|thread| thread.access == Access::Full);
        let Some(open) = self.open.get_mut(thread_id) else { return };
        open.programs.limited = limited;
        open.programs.full_access = full_access;
    }

    /// Draws the programs of the server's open threads again once what the server can do is known
    /// or changed, and fetches again what failed once it can run programs or has just `connected`.
    fn reprogram(&mut self, server_id: &str, connected: bool) {
        let thread_ids: Vec<String> =
            self.open.iter().filter(|(_, open)| open.server_id == server_id).map(|(id, _)| id.clone()).collect();
        for thread_id in thread_ids {
            let Some(before) = self.open.get(&thread_id).map(|open| open.programs.limited) else { continue };
            self.refresh_programs(&thread_id);
            let Some(open) = self.open.get_mut(&thread_id) else { continue };
            let limited = open.programs.limited;
            let changed = limited != before;
            if changed {
                self.redraw_programs(&thread_id);
            }
            if !limited && (changed || connected) {
                self.with_programs(&thread_id, Programs::retry);
            }
        }
    }

    fn redraw_programs(&mut self, thread_id: &str) {
        let Some(open) = self.open.get(thread_id) else { return };
        let program_rows = |rows: &[Row]| -> Vec<Splice> {
            let programs = rows.iter().enumerate().filter(|(_, row)| matches!(row.kind, RowKind::Ui { .. }));
            programs.map(|(start, row)| Splice { start, remove: 1, rows: vec![row.clone()] }).collect()
        };
        let rows = program_rows(open.transcript.rows());
        let agent_rows = open.agent.as_ref().map(|(_, agent)| program_rows(agent.rows())).unwrap_or_default();
        for splice in rows {
            self.emit_rows(thread_id, false, splice);
        }
        for splice in agent_rows {
            self.emit_agent_rows(thread_id, false, splice);
        }
    }

    fn with_programs(&mut self, thread_id: &str, act: impl FnOnce(&mut Programs) -> Vec<Out>) {
        self.refresh_programs(thread_id);
        let Some(open) = self.open.get_mut(thread_id) else { return };
        let outs = act(&mut open.programs);
        self.carry_out(thread_id, outs);
    }

    /// Does what the thread's programs asked for.
    fn carry_out(&mut self, thread_id: &str, outs: Vec<Out>) {
        if outs.is_empty() {
            return;
        }
        let Some(server_id) = self.open.get(thread_id).map(|open| open.server_id.clone()) else { return };
        let link = self.link(&server_id);
        let inputs = self.inputs.clone();
        let mut sends = Vec::new();
        for out in outs {
            let thread_id = thread_id.to_string();
            match out {
                Out::Patch { row_id, ops } => self.emit(Event::UiPatch { thread_id, row_id, ops }),
                Out::Confirm { row_id, call_id, command } => {
                    self.emit(Event::UiConfirm { thread_id, row_id, call_id, command })
                }
                Out::OpenUrl { url } => self.emit(Event::OpenUrl { url }),
                Out::Copy { text } => self.emit(Event::Copy { text }),
                Out::Send { text, context } => sends.push((text, context)),
                Out::Call { row_id, serial, call, tool, args, confirmed } => {
                    let (link, inputs) = (link.clone(), inputs.clone());
                    tokio::spawn(async move {
                        let request = Request::UiTool { thread_id: thread_id.clone(), tool, args, confirmed };
                        let answer = match link {
                            Ok(link) => link.request(&request).await.map_err(error_text),
                            Err(error) => Err(error),
                        };
                        let result = match answer {
                            Ok(Message::UiToolResult { result }) => Ok(result),
                            Ok(Message::UiToolError { code, .. }) if code == "tool-not-found" => {
                                Err(ToolError::NotFound { available: Vec::new() })
                            }
                            Ok(Message::UiToolError { message, .. } | Message::Error { message }) => {
                                Err(ToolError::Failed(message))
                            }
                            Ok(other) => Err(ToolError::Failed(unexpected(&other))),
                            Err(error) => Err(ToolError::Failed(error)),
                        };
                        let _ = inputs.send(Input::UiToolDone { thread_id, row_id, serial, call, result });
                    });
                }
                Out::Interval { key, seconds } => {
                    let every = Duration::from_secs_f64(seconds.max(1.0));
                    let (inputs, ticking, ticked) = (inputs.clone(), key.clone(), thread_id.clone());
                    let task = tokio::spawn(async move {
                        loop {
                            tokio::time::sleep(every).await;
                            let tick = Input::UiTick { thread_id: ticked.clone(), key: ticking.clone() };
                            if inputs.send(tick).is_err() {
                                return;
                            }
                        }
                    });
                    if let Some(open) = self.open.get_mut(&thread_id) {
                        open.ui_timers.set(key, task.abort_handle());
                    }
                }
                Out::Stop { key } => {
                    if let Some(open) = self.open.get_mut(&thread_id) {
                        open.ui_timers.stop(&key);
                    }
                }
                Out::Save { item_id, block, state } => {
                    let Ok(link) = link.clone() else { continue };
                    tokio::spawn(async move {
                        if let Err(error) = link.request(&Request::SetUi { thread_id, item_id, block, state }).await {
                            tracing::debug!("couldn't keep a program's state: {error:#}");
                        }
                    });
                }
                Out::Images { query } => {
                    let (link, inputs) = (link.clone(), inputs.clone());
                    tokio::spawn(async move {
                        let request = Request::UiImages { thread_id: thread_id.clone(), query: query.clone() };
                        let images = match link {
                            Ok(link) => match link.request(&request).await {
                                Ok(Message::UiImages { images }) => images,
                                _ => Vec::new(),
                            },
                            Err(_) => Vec::new(),
                        };
                        let _ = inputs.send(Input::UiImagesFound { thread_id, query, images });
                    });
                }
                Out::Route { stops } => {
                    let (link, inputs) = (link.clone(), inputs.clone());
                    tokio::spawn(async move {
                        let request = Request::UiRoute { stops: stops.clone() };
                        let points = match link {
                            Ok(link) => match link.request(&request).await {
                                Ok(Message::UiRoute { points }) => Some(points),
                                _ => None,
                            },
                            Err(_) => None,
                        };
                        let _ = inputs.send(Input::UiRouted { thread_id, stops, points });
                    });
                }
                Out::Highlight { language, code } => {
                    let inputs = inputs.clone();
                    tokio::task::spawn_blocking(move || {
                        highlight::highlight(&language, &code);
                        let _ = inputs.send(Input::UiHighlighted { thread_id, language, code });
                    });
                }
                Out::Wake { after } => {
                    let inputs = inputs.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(after).await;
                        let _ = inputs.send(Input::UiWake { thread_id });
                    });
                }
            }
        }
        if sends.is_empty() {
            return;
        }
        let Ok(link) = link else { return };
        let thread_id = thread_id.to_string();
        tokio::spawn(async move {
            for (text, context) in sends {
                let request = Request::Send {
                    thread_id: Some(thread_id.clone()),
                    new_thread: None,
                    text,
                    attachments: Vec::new(),
                    now: false,
                    context,
                    message_id: None,
                };
                if let Err(error) = link.request(&request).await {
                    tracing::warn!("couldn't send a program's message: {error:#}");
                }
            }
        });
    }

    fn emit_outbox(&self) {
        self.emit(Event::Outbox { messages: self.outbox.messages().to_vec() });
    }

    /// Sends each server the next message of the outbox, if it is connected and has none on its
    /// way. A message that may have reached a server already only goes again to one that knows it
    /// by its id; to an older one it goes back to where it was written.
    fn send_outbox(&mut self) {
        for server_id in self.outbox.waiting_servers() {
            let Some(message) = self.outbox.next(&server_id).cloned() else { continue };
            let Some(server) = self.servers.iter().find(|server| server.device.public_key == server_id) else {
                continue;
            };
            let link = match self.link(&server_id) {
                Ok(link) => link,
                Err(_) if !server.removed => continue,
                Err(error) => {
                    self.unsend(&message.id, error);
                    continue;
                }
            };
            if server.status.state != State::Connected {
                continue;
            }
            let protocol = server.info.as_ref().map_or(0, |info| info.protocol);
            if !message.can_send_again(protocol) {
                let name = &server.device.name;
                let error = format!(
                    "Your message may not have reached {name}, which is too old to be sent it again safely, so it's back where you wrote it. Update {name}."
                );
                self.unsend(&message.id, error);
                continue;
            }
            let Some(message) = self.outbox.start(&self.cache, &message.id) else { continue };
            let inputs = self.inputs.clone();
            tokio::spawn(async move {
                let result = link.request(&message.request()).await.map_err(error_text);
                let _ = inputs.send(Input::Delivered { message_id: message.id, result });
            });
        }
    }

    fn delivered(&mut self, message_id: &str, result: Result<Message, String>) {
        match result {
            Ok(Message::Sent { thread_id }) => {
                let Some(message) = self.outbox.remove(&self.cache, message_id) else { return };
                let draft_id = message.new_thread.is_some().then_some(message.thread_id);
                self.emit(Event::Sent { message_id: message.id, thread_id, draft_id });
                self.emit_outbox();
            }
            Ok(other) => {
                let error =
                    format!("Your server gave an unexpected answer. Update it and try again. It said: {other:?}");
                self.unsend(message_id, error);
            }
            Err(error) if offline(&error) => {
                tracing::warn!(message_id, "a message didn't reach its server: {error}");
                let error = error.trim_end_matches(" Try again when it is back.");
                let error = format!("{error} It is sent again once your server answers.");
                self.outbox.failed(&self.cache, message_id, error);
                self.emit_outbox();
                let inputs = self.inputs.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(OUTBOX_RETRY).await;
                    let _ = inputs.send(Input::SendOutbox);
                });
            }
            Err(error) => self.unsend(message_id, error),
        }
        self.send_outbox();
    }

    /// Takes the message out of the outbox, for the client to put back where it was written.
    fn unsend(&mut self, message_id: &str, error: String) {
        let Some(message) = self.outbox.remove(&self.cache, message_id) else { return };
        tracing::warn!(message_id, "a message was given back: {error}");
        self.emit(Event::Unsent { message: Box::new(message), error });
        self.emit_outbox();
    }

    fn open_agent(&mut self, thread_id: &str, agent_id: &str) {
        let Some(open) = self.open.get_mut(thread_id) else { return };
        let call = open.agents.iter().find(|item| item.id == agent_id);
        let started = call.and_then(|call| agents::view(call, &open.threads));
        let workflow = call.map(agents::workflow_items).filter(|items| !items.is_empty());
        let mut items = workflow.unwrap_or_else(|| self.cache.agent_items(thread_id, agent_id));
        // What the agent was asked to do is the message its transcript starts with.
        if let (Some(call), Some(agent)) = (call, &started)
            && !agent.prompt.is_empty()
        {
            let kind = ItemKind::User {
                text: agent.prompt.clone(),
                attachments: Vec::new(),
                skills: Vec::new(),
                context: None,
                programs: Vec::new(),
            };
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
        let mut splices = Vec::new();
        for (thread_id, open) in &mut self.open {
            for id in std::mem::take(&mut open.unrendered) {
                let Some(splice) = open.transcript.refresh(&id) else { continue };
                splices.push((thread_id.clone(), splice));
            }
            if open.unsaved.is_empty() || open.saved_at.elapsed() < SAVE_EVERY {
                continue;
            }
            save_streamed(&self.cache, thread_id, open);
        }
        for (thread_id, splice) in splices {
            self.emit_rows(&thread_id, false, splice);
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
            Command::Background => {
                self.away.backgrounded();
                self.reply(id, Ok(json!({})));
            }
            Command::Foreground => {
                tracing::info!("came back");
                self.away.foregrounded();
                if !self.replace_stale_endpoint() {
                    self.network_changed();
                }
                self.check_account();
                self.reply(id, Ok(json!({})));
            }
            Command::NetworkChanged => {
                tracing::info!("the network changed");
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
            Command::DeleteServerCompletely { server_id } => {
                let (auth, key, sink, inputs) =
                    (self.auth.clone(), self.key.clone(), self.sink.clone(), self.inputs.clone());
                tokio::spawn(async move {
                    let deleted = auth.delete_server_completely(&key, &server_id).await.map_err(error_text);
                    let result = auth.me(&key).await.map_err(error_text);
                    let _ = inputs.send(Input::AccountChecked { key: key.public(), result });
                    reply(&sink, id, deleted.map(|_| json!({})));
                });
            }
            Command::WatchGateway { on } => {
                self.gateway.watching = on;
                self.sync_gateway();
                self.reply(id, Ok(json!({})));
            }
            Command::AddGateway => {
                let (auth, key, sink) = (self.auth.clone(), self.key.clone(), self.sink.clone());
                let auth_url =
                    Some(self.config.auth_url.clone()).filter(|url| url != motile_protocol::DEFAULT_AUTH_URL);
                tokio::spawn(async move {
                    let token = auth.create_enroll_token(&key).await.map_err(error_text);
                    let enroll = token.and_then(|token| {
                        let enroll = gateway::enroll(token.token, token.expires_at, auth_url.as_deref(), now());
                        serde_json::to_value(enroll).map_err(|error| error.to_string())
                    });
                    reply(&sink, id, enroll);
                });
            }
            Command::RemoveGateway => {
                let Some(gateway_key) = self.gateway_device().map(|device| device.public_key.clone()) else {
                    return self.reply(id, Ok(json!({})));
                };
                let (auth, key, sink, inputs) =
                    (self.auth.clone(), self.key.clone(), self.sink.clone(), self.inputs.clone());
                tokio::spawn(async move {
                    let removed = auth.remove_device(&key, &gateway_key).await.map_err(error_text);
                    let result = auth.me(&key).await.map_err(error_text);
                    let _ = inputs.send(Input::AccountChecked { key: key.public(), result });
                    reply(&sink, id, removed.map(|_| json!({})));
                });
            }
            Command::GatewaySignIn { agent, account } => {
                self.gateway.sign_in = None;
                self.gateway.signing_in_again = account;
                self.gateway_manage(id, Manage::SignIn { agent });
                self.emit_gateway();
            }
            Command::GatewayFinishSignIn { code } => {
                let Some(sign_in) = self.gateway.sign_in.as_ref().map(|sign_in| sign_in.started.id.clone()) else {
                    return self.reply(id, Err("That sign-in is over. Start it again.".to_string()));
                };
                self.gateway_finish_sign_in(sign_in, Some(code.trim().to_string()), Some(id));
                self.emit_gateway();
            }
            Command::GatewayCancelSignIn => {
                self.gateway.sign_in = None;
                self.emit_gateway();
                self.reply(id, Ok(json!({})));
            }
            Command::GatewayUpdateAccount { account, change } => {
                self.gateway_manage(id, Manage::UpdateAccount { id: account, change })
            }
            Command::GatewayRemoveAccount { account } => self.gateway_manage(id, Manage::RemoveAccount { id: account }),
            Command::GatewayLog { before, errors_only } => {
                self.gateway_manage(id, Manage::Log { before, errors_only, limit: gateway::LOG_PAGE })
            }
            Command::GatewayCaptured { entry } => self.gateway_manage(id, Manage::Captured { id: entry }),
            Command::GatewayCapture { capture } => self.gateway_manage(id, Manage::Capture { capture }),
            Command::McpServerAdd { url, name, header, key } => {
                let key = mcp_servers::key(&header, &key);
                self.gateway_manage(id, Manage::AddMcpServer { url: url.trim().to_string(), name, key })
            }
            Command::McpServerSignIn { mcp_server } => {
                self.gateway_manage(id, Manage::SignInMcpServer { id: mcp_server })
            }
            Command::McpServerFinishSignIn { address } => match mcp_servers::sent_back(&address) {
                Ok((state, code)) => self.gateway_manage(id, Manage::FinishMcpSignIn { state, code }),
                Err(error) => self.reply(id, Err(error)),
            },
            Command::McpServerUpdate { mcp_server, change } => {
                self.gateway_manage(id, Manage::UpdateMcpServer { id: mcp_server, change })
            }
            Command::McpServerKey { mcp_server, header, key } => match mcp_servers::key(&header, &key) {
                Some(key) => {
                    self.gateway_manage(id, Manage::UpdateMcpServer { id: mcp_server, change: McpChange::Key { key } })
                }
                None => self.reply(id, Err("Paste the key first.".to_string())),
            },
            Command::McpServerRefresh { mcp_server } => {
                self.gateway_manage(id, Manage::RefreshMcpServer { id: mcp_server })
            }
            Command::McpServerRemove { mcp_server } => {
                self.gateway_manage(id, Manage::RemoveMcpServer { id: mcp_server })
            }
            Command::McpServerCalls { before, errors_only } => {
                self.gateway_manage(id, Manage::McpCalls { before, errors_only, limit: mcp_servers::CALLS_PAGE })
            }
            Command::McpServerCall { call } => self.gateway_manage(id, Manage::McpCall { id: call }),
            Command::McpServerFields { mcp_server, tool } => match self.mcp_server_tool_schema(&mcp_server, &tool) {
                Some(schema) => self.reply(id, Ok(json!({ "fields": mcp_servers::fields(&schema) }))),
                None => self.reply(id, Err("That tool is gone.".to_string())),
            },
            Command::McpServerTry { mcp_server, tool, values } => {
                let Some(schema) = self.mcp_server_tool_schema(&mcp_server, &tool) else {
                    return self.reply(id, Err("That tool is gone.".to_string()));
                };
                match mcp_servers::arguments(&schema, &values) {
                    Ok(arguments) => self.gateway_manage(id, Manage::TryTool { id: mcp_server, tool, arguments }),
                    Err(error) => self.reply(id, Err(error)),
                }
            }
            Command::SetMetered { metered } => {
                let was = std::mem::replace(&mut self.relay.metered, metered);
                if was && !metered {
                    self.relay.waiting.clear();
                    self.schedule_relay(RELAY_SOON);
                }
                self.reply(id, Ok(json!({})));
            }
            Command::OpenThread { server_id, thread_id } => {
                let result = self.open_thread(&server_id, &thread_id);
                self.reply(id, result.map(|_| json!({})));
            }
            Command::CloseThread { thread_id } => {
                self.close_thread(&thread_id);
                self.reply(id, Ok(json!({})));
            }
            Command::SearchThreads { query } => self.search_threads(&query),
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
                                Ok(other) => Err(unexpected(&other)),
                                Err(error) => Err(error_text(error)),
                            }
                        }
                    };
                    let listing =
                        folders.map(|folders| browse::listing(&typed.directory, &typed.leaf, &folders, &home));
                    reply(&sink, id, listing.map(|listing| serde_json::to_value(listing).unwrap_or_default()));
                });
            }
            Command::ProjectImages { server_id, project_id, query } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let (sink, searched) = (self.sink.clone(), self.searched_images.clone());
                tokio::spawn(async move {
                    // Typing narrows the images listed when the search started.
                    let key = (server_id, project_id);
                    let known =
                        searched.lock().unwrap().clone().filter(|(known, _)| *known == key && !query.is_empty());
                    let listed = match known {
                        Some((_, listed)) => Ok(listed),
                        None => match link.request(&Request::ProjectImages { project_id: key.1.clone() }).await {
                            Ok(Message::ProjectImages { folder, paths }) => {
                                *searched.lock().unwrap() = Some((key, (folder.clone(), paths.clone())));
                                Ok((folder, paths))
                            }
                            Ok(other) => Err(unexpected(&other)),
                            Err(error) => Err(error_text(error)),
                        },
                    };
                    let images =
                        listed.map(|(folder, paths)| json!({ "images": browse::images(&folder, &paths, &query) }));
                    reply(&sink, id, images);
                });
            }
            Command::SkillMenu { server_id, project_id, account, before_cursor } => {
                if skills::typed(&before_cursor).is_none() {
                    return self.reply(id, Ok(serde_json::to_value(skills::Menu::default()).unwrap_or_default()));
                }
                self.answer_with_skills(id, server_id, project_id, account, move |skills| {
                    serde_json::to_value(skills::menu(&before_cursor, skills).unwrap_or_default()).unwrap_or_default()
                });
            }
            Command::PrefetchSkills { server_id, project_id } => {
                self.answer_with_skills(id, server_id, project_id, String::new(), |_| Value::Null);
            }
            Command::SkillChips { server_id, project_id, account, text } => {
                if motile_protocol::skills::mentions(&text).is_empty() {
                    return self.reply(id, Ok(json!({ "chips": [] })));
                }
                self.answer_with_skills(
                    id,
                    server_id,
                    project_id,
                    account,
                    move |skills| json!({ "chips": skills::chips(&text, skills) }),
                );
            }
            Command::Collapses { text } => {
                self.reply(id, Ok(json!({ "collapsible": collapses(&text, &[]) })));
            }
            Command::Send { server_id, thread_id, new_thread, text, attachments, now, shown } => {
                if let Err(error) = self.link(&server_id) {
                    return self.reply(id, Err(error));
                }
                let mut new_thread = new_thread;
                let draft_id = new_thread
                    .as_mut()
                    .map(|new_thread| new_thread.id.get_or_insert_with(|| uuid::Uuid::new_v4().to_string()).clone());
                let Some(thread_id) = thread_id.or(draft_id) else {
                    return self.reply(id, Err("No thread was given.".to_string()));
                };
                let message_id = uuid::Uuid::new_v4().to_string();
                let message = Outgoing {
                    id: message_id.clone(),
                    server_id,
                    thread_id: thread_id.clone(),
                    new_thread,
                    text,
                    attachments,
                    now,
                    shown,
                    sent_at: motile_protocol::now(),
                    tries: 0,
                    error: None,
                    on_the_way: false,
                };
                self.outbox.add(&self.cache, message);
                self.reply(id, Ok(json!({ "message_id": message_id, "thread_id": thread_id })));
                self.emit_outbox();
                self.send_outbox();
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
                        pull_default_branch: Some(settings.pull_default_branch),
                    };
                    let set = link.request(&request).await;
                    if set.is_ok() {
                        let _ = inputs.send(Input::PullRequestSettingsSet { server_id, settings });
                    }
                    reply(&sink, id, set.map(|_| json!({})).map_err(error_text));
                });
            }
            Command::SetBackup { bucket } => self.set_backup(id, bucket),
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
            Command::RicracIssue { server_id, workspace, issue, comment } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let request = match comment {
                        Some(body) => Request::RicracComment { workspace, issue, body },
                        None => Request::RicracIssue { workspace, issue },
                    };
                    let detail = match link.request(&request).await {
                        Ok(Message::RicracIssueDetail { detail }) => detail,
                        Ok(other) => return reply(&sink, id, Err(unexpected(&other))),
                        Err(error) => return reply(&sink, id, Err(error_text(error))),
                    };
                    let page = tokio::task::spawn_blocking(move || ricrac::page(&detail)).await;
                    reply(&sink, id, page.map(|page| json!({ "page": page })).map_err(|error| error.to_string()));
                });
            }
            Command::RicracIssues { server_id, workspace, team, mine, closed, states, search } => {
                let link = match self.link(&server_id) {
                    Ok(link) => link,
                    Err(error) => return self.reply(id, Err(error)),
                };
                let sink = self.sink.clone();
                tokio::spawn(async move {
                    let answer =
                        link.request(&Request::RicracIssues { workspace, team, mine, closed, states, search }).await;
                    let answer = match answer {
                        Ok(Message::RicracIssues { issues }) => Ok(json!({ "groups": ricrac::groups(&issues) })),
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
                let mut project_names = HashMap::new();
                let mut server_names = HashMap::new();
                let mut proxied = HashMap::new();
                let (removed, holder) = self.removed_and_holder();
                let removed: Vec<String> = removed
                    .into_iter()
                    .filter(|origin| servers.as_ref().is_none_or(|ids| ids.contains(origin)))
                    .collect();
                for origin in &removed {
                    for project in self.cache.projects(origin) {
                        project_names.insert((origin.clone(), project.id), project.name);
                    }
                    let name = self.servers.iter().find(|server| &server.device.public_key == origin);
                    server_names
                        .insert(origin.clone(), name.map(|server| server.device.name.clone()).unwrap_or_default());
                }
                let holder_link = holder.as_ref().and_then(|holder| self.peers().get(holder).cloned());
                let asked = self
                    .servers
                    .iter()
                    .filter(|server| servers.as_ref().is_none_or(|ids| ids.contains(&server.device.public_key)))
                    .filter(|server| server.info.as_ref().is_some_and(|info| info.protocol >= USAGE_PROTOCOL));
                let mut links: Vec<(String, Arc<Link>)> = Vec::new();
                for server in asked {
                    let Some(link) = server.link.clone() else { continue };
                    let server_id = server.device.public_key.clone();
                    for project in self.cache.projects(&server_id) {
                        project_names.insert((server_id.clone(), project.id), project.name);
                    }
                    server_names.insert(server_id.clone(), server.device.name.clone());
                    let proxies =
                        server.info.iter().flat_map(|info| &info.agent_accounts).filter(|account| account.gateway);
                    for account in proxies {
                        proxied.insert((server_id.clone(), account.agent, account.name.clone()), account.pin.is_some());
                    }
                    links.push((server_id, link));
                }
                let holder = holder_link.and(holder).filter(|_| !removed.is_empty());
                if let Some(holder) = holder.as_ref().filter(|holder| !links.iter().any(|(id, _)| id == *holder)) {
                    links.extend(self.peers().get(holder).map(|link| (holder.clone(), link.clone())));
                }
                if links.is_empty() {
                    return self.reply(id, Err("None of the servers is connected.".to_string()));
                }
                let mut asked = tokio::task::JoinSet::new();
                let chosen = |server_id: &String| servers.as_ref().is_none_or(|ids| ids.contains(server_id));
                for (server_id, link) in links {
                    // The server that answers for the removed servers counts only its own when it wasn't chosen.
                    let own = chosen(&server_id);
                    let request = Request::Usage {
                        since: window.since,
                        until: window.until,
                        bucket_secs: window.bucket_secs,
                        utc_offset_secs: window.utc_offset_secs,
                        removed: if holder.as_ref() == Some(&server_id) { removed.clone() } else { Vec::new() },
                    };
                    asked.spawn(async move { (server_id, own, link.request(&request).await) });
                }
                let (sink, inputs) = (self.sink.clone(), self.inputs.clone());
                tokio::spawn(async move {
                    let mut spent = Vec::new();
                    let mut failure = None;
                    while let Some(Ok((server_id, own, answer))) = asked.join_next().await {
                        match answer {
                            Ok(Message::Usage { buckets, copies }) => {
                                let of_server = |bucket| usage::Spent { server_id: server_id.clone(), bucket };
                                if own {
                                    spent.extend(buckets.into_iter().map(of_server));
                                }
                                for copy in copies {
                                    let of_origin = |bucket| usage::Spent { server_id: copy.origin.clone(), bucket };
                                    spent.extend(copy.buckets.into_iter().map(of_origin));
                                }
                            }
                            Ok(other) => failure = Some(unexpected(&other)),
                            Err(error) => failure = Some(error_text(error)),
                        }
                    }
                    let answer = match failure {
                        Some(failure) if spent.is_empty() => Err(failure),
                        _ => serde_json::to_value(usage::view(&spent, window, &project_names, &server_names, &proxied))
                            .map_err(|error| error.to_string()),
                    };
                    if let Ok(view) = &answer {
                        let _ = inputs.send(Input::Keep { key, value: view.clone() });
                    }
                    reply(&sink, id, answer);
                });
            }
            Command::Activity { bucket_secs, buckets, utc_offset_secs, servers, kept } => {
                let key = format!("activity:{}", &usage_key(bucket_secs, buckets, servers.as_ref())["usage:".len()..]);
                if kept {
                    let last: Option<Value> = self.cache.kept(&key);
                    return self.reply(id, last.ok_or_else(|| "Nothing was kept.".to_string()));
                }
                let window = usage::Window::ending(now(), bucket_secs, buckets, utc_offset_secs);
                let mut names = activity::Names {
                    this_device: self.key.public(),
                    devices: self
                        .me
                        .devices
                        .iter()
                        .map(|device| (device.public_key.clone(), (device.name.clone(), device.platform.clone())))
                        .collect(),
                    ..activity::Names::default()
                };
                let (removed, holder) = self.removed_and_holder();
                let removed: Vec<String> = removed
                    .into_iter()
                    .filter(|origin| servers.as_ref().is_none_or(|ids| ids.contains(origin)))
                    .collect();
                for origin in &removed {
                    for project in self.cache.projects(origin) {
                        names.projects.insert((origin.clone(), project.id), project.name);
                    }
                    let name = self.servers.iter().find(|server| &server.device.public_key == origin);
                    names
                        .servers
                        .insert(origin.clone(), name.map(|server| server.device.name.clone()).unwrap_or_default());
                }
                let holder_link = holder.as_ref().and_then(|holder| self.peers().get(holder).cloned());
                let asked = self
                    .servers
                    .iter()
                    .filter(|server| servers.as_ref().is_none_or(|ids| ids.contains(&server.device.public_key)))
                    .filter(|server| server.info.as_ref().is_some_and(|info| info.protocol >= ACTIVITY_PROTOCOL));
                let mut links: Vec<(String, Arc<Link>)> = Vec::new();
                for server in asked {
                    let Some(link) = server.link.clone() else { continue };
                    let server_id = server.device.public_key.clone();
                    for project in self.cache.projects(&server_id) {
                        names.projects.insert((server_id.clone(), project.id), project.name);
                    }
                    names.servers.insert(server_id.clone(), server.device.name.clone());
                    links.push((server_id, link));
                }
                let holder = holder_link.and(holder).filter(|_| !removed.is_empty());
                if let Some(holder) = holder.as_ref().filter(|holder| !links.iter().any(|(id, _)| id == *holder)) {
                    links.extend(self.peers().get(holder).map(|link| (holder.clone(), link.clone())));
                }
                if links.is_empty() {
                    let outdated = self.servers.iter().any(|server| server.info.is_some());
                    let why = if outdated {
                        "Update your servers to see what was done on them."
                    } else {
                        "None of the servers is connected."
                    };
                    return self.reply(id, Err(why.to_string()));
                }
                let mut asked = tokio::task::JoinSet::new();
                let chosen = |server_id: &String| servers.as_ref().is_none_or(|ids| ids.contains(server_id));
                for (server_id, link) in links {
                    let own = chosen(&server_id);
                    let request = Request::ActivityReport {
                        since: window.since,
                        until: window.until,
                        bucket_secs: window.bucket_secs,
                        utc_offset_secs: window.utc_offset_secs,
                        removed: if holder.as_ref() == Some(&server_id) { removed.clone() } else { Vec::new() },
                    };
                    asked.spawn(async move { (server_id, own, link.request(&request).await) });
                }
                let (sink, inputs) = (self.sink.clone(), self.inputs.clone());
                tokio::spawn(async move {
                    let mut done = Vec::new();
                    let mut failure = None;
                    while let Some(Ok((server_id, own, answer))) = asked.join_next().await {
                        match answer {
                            Ok(Message::ActivityReport { report, copies }) => {
                                // A server that wasn't chosen still says which clients it knows.
                                let report = if own {
                                    report
                                } else {
                                    motile_protocol::wire::ActivityReport {
                                        clients: report.clients,
                                        ..Default::default()
                                    }
                                };
                                done.push(activity::Done { server_id, report });
                                for copy in copies {
                                    done.push(activity::Done { server_id: copy.origin, report: copy.report });
                                }
                            }
                            Ok(other) => failure = Some(unexpected(&other)),
                            Err(error) => failure = Some(error_text(error)),
                        }
                    }
                    let answer = match failure {
                        Some(failure) if done.is_empty() => Err(failure),
                        _ => serde_json::to_value(activity::view(&done, window, &names))
                            .map_err(|error| error.to_string()),
                    };
                    if let Ok(view) = &answer {
                        let _ = inputs.send(Input::Keep { key, value: view.clone() });
                    }
                    reply(&sink, id, answer);
                });
            }
            Command::Limits { kept, servers } => {
                let chosen = self
                    .servers
                    .iter()
                    .filter(|server| !server.removed)
                    .filter(|server| servers.as_ref().is_none_or(|ids| ids.contains(&server.device.public_key)));
                let mut asked = tokio::task::JoinSet::new();
                let mut notes = Vec::new();
                let mut reads = Vec::new();
                let mut proxied: Vec<(String, Agent, String)> = Vec::new();
                for server in chosen {
                    let Some(info) = server.info.as_ref() else { continue };
                    let name = server.device.name.clone();
                    let proxies = info.agent_accounts.iter().filter(|account| account.gateway);
                    proxied.extend(proxies.map(|account| (name.clone(), account.agent, account.name.clone())));
                    if info.protocol < LIMITS_PROTOCOL {
                        notes.push(format!("Update {name} to see its limits."));
                        continue;
                    }
                    let server_id = server.device.public_key.clone();
                    let Some(link) = server.link.clone().filter(|_| !kept) else {
                        let agents = self.limits_read(&server_id).map(|(_, agents)| agents).unwrap_or_default();
                        reads.extend(agents.into_iter().map(|limits| (name.clone(), limits)));
                        continue;
                    };
                    asked.spawn(async move { (server_id, name, link.request(&Request::Limits).await) });
                }
                let kept_accounts =
                    self.gateway.state.as_ref().map(|(_, state)| state.accounts.clone()).unwrap_or_default();
                let gateway_link = self.gateway.link.clone().filter(|_| !kept && self.gateway_device().is_some());
                let (sink, inputs) = (self.sink.clone(), self.inputs.clone());
                let told = inputs.clone();
                let asked_gateway = tokio::spawn(async move {
                    let link = gateway_link?;
                    let answer = tokio::time::timeout(GATEWAY_LIMITS_WAIT, link.manage(Manage::State)).await;
                    let Ok(Ok(GatewayReply::State { state })) = answer else { return None };
                    let accounts = state.accounts.clone();
                    let _ = told.send(Input::Gateway { key: link.key.clone(), event: GatewayEvent::State(state) });
                    Some(accounts)
                });
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
                        reads.extend(agents.into_iter().map(|limits| (server.clone(), limits)));
                    }
                    let accounts = asked_gateway.await.ok().flatten().unwrap_or(kept_accounts);
                    let mut reads: Vec<limits::Read> = reads
                        .into_iter()
                        .map(|(server, limits)| {
                            let gateway =
                                proxied.contains(&(server.clone(), limits.agent, limits.account_name.clone()));
                            limits::Read { server, limits, gateway }
                        })
                        .collect();
                    reads.sort_by(|a, b| a.server.cmp(&b.server));
                    let sections = limits::sections(reads, &accounts, now());
                    reply(&sink, id, Ok(json!({ "sections": sections, "notes": notes })));
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
            Command::CancelMedia { media_id } => {
                self.cancel_media(&media_id);
                self.reply(id, Ok(json!({})));
            }
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
            Command::ReloadKeybindings => {
                if self.keybindings.reload() {
                    self.emit_keybindings();
                }
                self.reply(id, Ok(json!({})));
            }
            Command::SetKeybinding { command, key, when, replace } => {
                let result = self.keybindings.set(&command, &key, when.as_deref(), replace.as_ref());
                self.keybindings_changed(id, result);
            }
            Command::RemoveKeybinding { rule } => {
                let result = self.keybindings.remove(&rule);
                self.keybindings_changed(id, result);
            }
            Command::ResetKeybinding { command } => {
                let result = self.keybindings.reset(&command);
                self.keybindings_changed(id, result);
            }
            Command::CheckKeybinding { key, when, row } => {
                let check = self.keybindings.check(&key, when.as_deref(), row.as_ref());
                self.reply(id, Ok(check));
            }
            Command::KeybindingsFile => {
                let path = self.keybindings.file().map(|path| json!({ "path": path }));
                self.reply(id, path);
            }
            Command::Highlight { thread_id, row_ids } => {
                self.highlight(&thread_id, &row_ids);
                self.reply(id, Ok(json!({})));
            }
            Command::Ui { thread_id, row_id, node, event } => {
                self.with_programs(&thread_id, |programs| programs.event(&row_id, &node, event));
                self.reply(id, Ok(json!({})));
            }
            Command::UiConfirm { thread_id, row_id, call_id, allow } => {
                self.with_programs(&thread_id, |programs| programs.confirm(&row_id, call_id, allow));
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
            Command::KeepPlace { thread_id, agent_id, at } => {
                self.keep_place(thread_id, agent_id, at);
                self.reply(id, Ok(json!({})));
            }
            Command::LoadEarlier { thread_id } => {
                self.load_earlier(&thread_id);
                self.reply(id, Ok(json!({})));
            }
            Command::Find { thread_id, query, from, item_id } => {
                self.find(&thread_id, &query, from, item_id);
                self.reply(id, Ok(json!({})));
            }
            Command::FindStep { thread_id, back } => {
                let stepped = self
                    .open
                    .get_mut(&thread_id)
                    .and_then(|open| open.find.as_mut())
                    .is_some_and(|find| find.step(back));
                if stepped {
                    self.emit_found(&thread_id);
                    self.reveal_found(&thread_id);
                }
                self.reply(id, Ok(json!({})));
            }
            Command::FindEnd { thread_id } => {
                if let Some(open) = self.open.get_mut(&thread_id) {
                    open.find = None;
                }
                self.reply(id, Ok(json!({})));
            }
            Command::TrimEarlier { thread_id, keep_rows } => {
                self.trim_earlier(&thread_id, keep_rows);
                self.reply(id, Ok(json!({})));
            }
        }
    }

    /// The turns let go stay in the cache, once their streamed text is there.
    /// Keeps what was found that still fits the search, and asks the servers again once it has
    /// stayed as it is for a moment.
    fn search_threads(&mut self, query: &str) {
        let query = search::normalized(query);
        if query == self.search.query {
            return;
        }
        if let Some(asking) = self.search.asking.take() {
            asking.abort();
        }
        search::keep_fitting(&mut self.search.found, &query);
        self.search.query = query.clone();
        let links: Vec<(String, Arc<Link>)> = self
            .servers
            .iter()
            .filter(|server| server.info.as_ref().is_some_and(|info| info.protocol >= SEARCH_PROTOCOL))
            .filter_map(|server| Some((server.device.public_key.clone(), server.link.clone()?)))
            .collect();
        let (removed, holder) = self.removed_and_holder();
        if !search::searchable(&query) || links.is_empty() {
            self.search.found.clear();
            return self.emit_search(false);
        }
        self.emit_search(true);
        let inputs = self.inputs.clone();
        let asking = tokio::spawn(async move {
            tokio::time::sleep(SEARCH_DELAY).await;
            let mut asked = tokio::task::JoinSet::new();
            for (server_id, link) in links {
                // One server searches its copies of the removed servers' threads too.
                let removed = if holder.as_ref() == Some(&server_id) { removed.clone() } else { Vec::new() };
                let request = Request::SearchThreads { query: query.clone(), limit: search::LIMIT, removed };
                asked.spawn(async move {
                    let answer = tokio::time::timeout(SEARCH_TIMEOUT, link.request(&request)).await;
                    let Ok(Ok(Message::ThreadMatches { matches })) = answer else { return (server_id, Vec::new()) };
                    let found = |thread: ThreadMatch| search::Found {
                        server_id: thread.origin.clone().unwrap_or_else(|| server_id.clone()),
                        asked: server_id.clone(),
                        thread,
                    };
                    let found = matches.into_iter().map(found).collect();
                    (server_id, found)
                });
            }
            while let Some(answer) = asked.join_next().await {
                let (server_id, found) = answer.unwrap_or_default();
                let last = asked.is_empty();
                let _ = inputs.send(Input::Searched { query: query.clone(), asked: server_id, found, last });
            }
        });
        self.search.asking = Some(asking.abort_handle());
    }

    /// The removed servers, and the connected server that answers for their threads.
    fn removed_and_holder(&self) -> (Vec<String>, Option<String>) {
        let removed: Vec<String> = self
            .servers
            .iter()
            .filter(|server| server.removed)
            .map(|server| server.device.public_key.clone())
            .collect();
        let peers = self.peers();
        let holder = removed
            .iter()
            .find_map(|origin| {
                relay::best_holder(&self.relay.marks, origin).filter(|holder| peers.contains_key(holder))
            })
            .or_else(|| {
                let mut ids: Vec<&String> = peers.keys().collect();
                ids.sort();
                ids.first().map(|id| (*id).clone())
            });
        (removed, holder.filter(|_| !self.servers.iter().all(|server| !server.removed)))
    }

    fn emit_search(&self, searching: bool) {
        let query = &self.search.query;
        let matches = self.search.found.iter().map(|found| search::view(found, query)).collect();
        self.emit(Event::ThreadSearch { query: query.clone(), searching, matches });
    }

    fn keep_place(&mut self, thread_id: String, agent_id: Option<String>, at: Option<ScrollAt>) {
        let open = self.open.get(&thread_id);
        let transcript = match &agent_id {
            None => open.map(|open| &open.transcript),
            Some(agent_id) => open
                .and_then(|open| open.agent.as_ref())
                .filter(|(id, _)| id == agent_id)
                .map(|(_, transcript)| transcript),
        };
        let key = (thread_id, agent_id);
        let Some(at) = at else {
            self.places.remove(&key);
            return;
        };
        let Some(transcript) = transcript else { return };
        let row = transcript.rows().iter().find(|row| row.id == at.row_id);
        let Some(item) = row.and_then(|row| transcript.item(&row.item)) else { return };
        let place = Place { item: item.id.clone(), seq: item.seq, at };
        self.places.insert(key, place);
    }

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

    /// What the agent was last seen doing, until the server says what it does now.
    fn last_activity(&self, server_id: &str, thread_id: &str) -> Option<Activity> {
        let server = self.servers.iter().find(|server| server.device.public_key == server_id)?;
        let thread = server.threads.get(thread_id)?;
        Some(if server.removed {
            Activity::default()
        } else {
            restored_activity(thread, self.cache.activity(thread_id))
        })
    }

    /// Opens the thread's rows with what the agent does and the agents it started, which come
    /// first so that the client shows them with the rows, not a moment after.
    fn open_thread(&mut self, server_id: &str, thread_id: &str) -> Result<(), String> {
        let place = self.places.get(&(thread_id.to_string(), None)).map(|place| place.seq);
        if self.open.contains_key(thread_id) {
            let activity = self.last_activity(server_id, thread_id);
            let open = self.open.get_mut(thread_id).ok_or("The thread closed.")?;
            open.left = None;
            if let Some(seq) = place {
                load_back_to(&self.cache, thread_id, &mut open.transcript, &mut open.earlier, seq);
            }
            let activity = activity.map(|activity| activity_changed(thread_id, &mut open.transcript, activity).1);
            let rows = open.transcript.rows().to_vec();
            let agents = agents_event(thread_id, &open.agents, &open.threads);
            let live = open.live;
            if let Some(activity) = activity {
                self.emit(activity);
            }
            self.emit(agents);
            self.emit_rows(thread_id, true, Splice { start: 0, remove: 0, rows });
            self.emit(Event::Live { thread_id: thread_id.to_string(), live });
            return Ok(());
        }
        let activity = self.last_activity(server_id, thread_id);
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
        let removed = server.removed;

        let Page { mut items, mut earlier } = self.cache.page(thread_id, None);
        let since = self.cache.synced_rev(thread_id);
        let first_shown = items.len().saturating_sub(FIRST_ITEMS);
        let first_shown =
            place.map_or(first_shown, |seq| first_shown.min(items.partition_point(|item| item.seq < seq)));
        let newest = items.split_off(first_shown);
        let agents = self.cache.agents(thread_id);
        let threads = ThreadState::of_children(&server.threads, thread_id);
        let mut transcript = Transcript::new(&cwd);
        transcript.set_agents(&agents, &threads);
        transcript.load(newest);
        if let Some(seq) = place {
            load_back_to(&self.cache, thread_id, &mut transcript, &mut earlier, seq);
        }
        let activity = activity.map(|activity| activity_changed(thread_id, &mut transcript, activity).1);
        let rows = transcript.rows().to_vec();
        let open = OpenThread {
            server_id: server_id.to_string(),
            transcript,
            agents: agents.clone(),
            threads: threads.clone(),
            agent: None,
            earlier,
            live,
            rev: since,
            unrendered: HashSet::new(),
            unsaved: HashSet::new(),
            saved_at: Instant::now(),
            programs: Programs::with_photos(self.photos.get(server_id).unwrap_or(&HashMap::new())),
            ui_timers: UiTimers::default(),
            left: None,
            find: None,
        };
        self.open.insert(thread_id.to_string(), open);
        if let Some(activity) = activity {
            self.emit(activity);
        }
        self.emit(agents_event(thread_id, &agents, &threads));
        self.emit_rows(thread_id, true, Splice { start: 0, remove: 0, rows });
        let open = self.open.get_mut(thread_id).ok_or("The thread closed.")?;
        if let Some(rest) =
            Some(items).filter(|items| !items.is_empty()).and_then(|items| open.transcript.prepend(items))
        {
            self.emit_rows(thread_id, false, rest);
        }
        if live {
            self.emit(Event::Live { thread_id: thread_id.to_string(), live });
        }
        if let Some(link) = link {
            link.open(thread_id.to_string(), since);
        }
        let behind = thread.as_ref().is_none_or(|thread| since < thread.rev);
        if removed && behind {
            self.open_copy(server_id, thread_id);
        }
        Ok(())
    }

    /// Keeps the thread open without showing it, and closes the one left longest ago when too many are.
    fn close_thread(&mut self, thread_id: &str) {
        self.with_programs(thread_id, Programs::flush);
        let Some(open) = self.open.get_mut(thread_id) else { return };
        save_streamed(&self.cache, thread_id, open);
        open.left = Some(Instant::now());
        let left: Vec<(Instant, &String)> =
            self.open.iter().filter_map(|(id, open)| open.left.map(|at| (at, id))).collect();
        if left.len() <= LEFT_OPEN {
            return;
        }
        let Some(oldest) = left.into_iter().min().map(|(_, id)| id.clone()) else { return };
        self.drop_thread(&oldest);
    }

    fn drop_thread(&mut self, thread_id: &str) {
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
        let view = ThreadView { unread: false, ..self.thread_view(&server_id, &thread) };
        self.emit(Event::ThreadUpsert { thread: Box::new(view) });
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

/// The thread whose transcript an event is about, which a thread the client left keeps to itself.
fn transcript_of(event: &Event) -> Option<&str> {
    match event {
        Event::Rows { thread_id, .. }
        | Event::Found { thread_id, .. }
        | Event::Reveal { thread_id, .. }
        | Event::Live { thread_id, .. }
        | Event::Agents { thread_id, .. }
        | Event::AgentRows { thread_id, .. }
        | Event::Spans { thread_id, .. }
        | Event::UiPatch { thread_id, .. }
        | Event::UiConfirm { thread_id, .. }
        | Event::Activity { thread_id, .. }
        | Event::ThreadError { thread_id, .. } => Some(thread_id),
        _ => None,
    }
}

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

/// Names the project alike on every server that has it, so that a picker lists it once: by its
/// repository's origin, and its folder's name too where its server has more than one clone.
fn project_key(server_id: &str, project: &Project, projects: &[Project]) -> String {
    if project.no_project {
        return "no-project".to_string();
    }
    let Some(origin) = &project.origin else { return format!("{server_id}/{}", project.id) };
    let clones = projects.iter().filter(|other| other.origin.as_ref() == Some(origin)).count();
    if clones > 1 { format!("{origin}#{}", project.name) } else { origin.clone() }
}

fn short_name(name: &str) -> String {
    let short: String = name.chars().take(10).collect();
    short.trim_end().to_string()
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

/// How the agents the thread's agent started are doing, theirs too. `None` when it started none.
fn children_of(threads: &HashMap<String, Thread>, thread_id: &str) -> Option<Children> {
    let mut descendants = Vec::new();
    let mut parents = vec![thread_id.to_string()];
    while let Some(parent) = parents.pop() {
        let children =
            threads.values().filter(|thread| thread.parent.as_ref().is_some_and(|of| of.thread_id == parent));
        for child in children {
            if descendants.iter().any(|known: &&Thread| known.id == child.id) || child.id == thread_id {
                continue;
            }
            parents.push(child.id.clone());
            descendants.push(child);
        }
    }
    if descendants.is_empty() {
        return None;
    }
    let mut working: Vec<&&Thread> = descendants.iter().filter(|thread| thread.running || thread.monitoring).collect();
    working.sort_by(|one, other| one.created_at.total_cmp(&other.created_at));
    let running = working.len() as u32;
    let running_count = match running {
        0 => None,
        1 => Some("1 agent".to_string()),
        several => Some(format!("{several} agents")),
    };
    let running_text = running_count.as_ref().map(|count| format!("{count} working"));
    let stop_question = match running {
        0 => None,
        1 => Some("Stop 1 agent?".to_string()),
        several => Some(format!("Stop {several} agents?")),
    };
    let mut waiting: Vec<&&Thread> = descendants.iter().filter(|thread| thread.needs_approval).collect();
    waiting.sort_by(|one, other| one.updated_at.total_cmp(&other.updated_at));
    let waiting: Vec<WaitingChild> = waiting
        .into_iter()
        .map(|thread| WaitingChild {
            thread_id: thread.id.clone(),
            title: thread.title.clone(),
            ask: thread.asking.unwrap_or(motile_protocol::wire::Ask::Approval),
        })
        .collect();
    let waiting_text = match &waiting[..] {
        [] => None,
        [one] => Some(match one.ask {
            motile_protocol::wire::Ask::Approval => format!("{} needs your approval", one.title),
            motile_protocol::wire::Ask::Question => format!("{} asks a question", one.title),
            motile_protocol::wire::Ask::Plan => format!("{} has a plan for you", one.title),
        }),
        several => Some(format!("{} agents need you", several.len())),
    };
    let working = working.iter().take(agents::avatars_shown(working.len())).map(|thread| thread.agent).collect();
    Some(Children { running, working, waiting, waiting_text, running_count, running_text, stop_question })
}

fn agents_event(thread_id: &str, started: &[Item], threads: &HashMap<String, ThreadState>) -> Event {
    let agents = agents::listed(started.iter().filter_map(|item| agents::view(item, threads)));
    Event::Agents { thread_id: thread_id.to_string(), agents }
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

/// The server's agents' accounts with what is known of their limits, for choosing where a new
/// thread starts.
fn accounts<'a>(server: &'a Server, info: &'a ServerInfo, working: &'a HashMap<String, u32>) -> limits::Accounts<'a> {
    limits::Accounts { accounts: &info.agent_accounts, limits: &server.limits, read_at: server.limits_read_at, working }
}

/// How many of the server's threads work now on each of its agents' accounts, by account id. A
/// thread without one works on its agent's first.
fn working(server: &Server, info: &ServerInfo) -> HashMap<String, u32> {
    let mut working: HashMap<String, u32> = HashMap::new();
    for thread in server.threads.values().filter(|thread| thread.running) {
        let first = || info.agent_accounts.iter().find(|account| account.agent == thread.agent);
        let account = match thread.agent_account.as_str() {
            "" => first().map(|account| account.id.clone()),
            id => Some(id.to_string()),
        };
        if let Some(account) = account {
            *working.entry(account).or_default() += 1;
        }
    }
    working
}

/// Names servers and their threads for what the gateway shows.
struct Names<'a>(&'a [Server]);

impl gateway::Naming for Names<'_> {
    fn server(&self, key: &str) -> Option<String> {
        self.0.iter().find(|server| server.device.public_key == key).map(|server| server.device.name.clone())
    }

    fn thread(&self, server: &str, thread: &str) -> Option<String> {
        let server = self.0.iter().find(|known| known.device.public_key == server)?;
        server.threads.get(thread).map(|thread| thread.title.clone())
    }
}

fn utc_offset_secs() -> i32 {
    chrono::Local::now().offset().local_minus_utc()
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
        Err(error) => (false, json!({ "offline": offline(&error), "error": error })),
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

/// A thread of a removed server, which nothing works on any more.
fn stopped(thread: Thread) -> Thread {
    Thread {
        running: false,
        monitoring: false,
        needs_approval: false,
        asking: None,
        agents: 0,
        git_stage: None,
        ..thread
    }
}

fn view_of(server_id: &str, thread: &Thread, seen_at: Option<f64>, children: Option<Children>) -> ThreadView {
    ThreadView {
        unread: is_unread(thread, seen_at),
        woke: thread.woke_at.is_some() && (seen_at.is_none() || after_seen(thread.woke_at, seen_at)),
        section: section_of(thread, children.as_ref()),
        thread: thread.clone(),
        server_id: server_id.to_string(),
        children,
    }
}

/// A turn ended after the user last looked. A thread never opened here doesn't nag.
fn is_unread(thread: &Thread, seen_at: Option<f64>) -> bool {
    after_seen(thread.turn_ended_at, seen_at) && !thread.running && !thread.monitoring
}

fn after_seen(at: Option<f64>, seen_at: Option<f64>) -> bool {
    matches!((at, seen_at), (Some(at), Some(seen)) if at > seen)
}

/// A thread that waits for the user stays among the active ones while its agents work.
fn section_of(thread: &Thread, children: Option<&Children>) -> Section {
    if thread.archived_at.is_some() {
        return Section::Archived;
    }
    if thread.snoozed_until.is_some() {
        return Section::Snoozed;
    }
    let waits = thread.needs_approval || children.is_some_and(|children| !children.waiting.is_empty());
    let works = thread.running || thread.monitoring || children.is_some_and(|children| children.running > 0);
    if works && !waits { Section::Working } else { Section::Active }
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

/// Whether the server can't run programs' tools or keep their state: until its `Welcome` says
/// which protocol it speaks, it is taken for an old one.
fn runs_no_programs(info: Option<&ServerInfo>) -> bool {
    info.is_none_or(|info| info.protocol < UI_PROTOCOL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_is_keyed_by_its_origin_and_by_its_name_where_its_server_has_two_clones() {
        let project = |id: &str, name: &str, origin: Option<&str>, no_project: bool| Project {
            id: id.to_string(),
            path: format!("/code/{name}"),
            name: name.to_string(),
            branch: None,
            git: None,
            icon: None,
            worktrees: Vec::new(),
            setup: None,
            no_project,
            origin: origin.map(str::to_string),
            created_at: 0.0,
        };
        let projects = [
            project("a", "motile", Some("github.com/o/motile"), false),
            project("b", "api", Some("github.com/o/api"), false),
            project("c", "api-2", Some("github.com/o/api"), false),
            project("d", "notes", None, false),
            project("e", "No project", None, true),
        ];

        let keys: Vec<String> = projects.iter().map(|project| project_key("ymac", project, &projects)).collect();

        assert_eq!(
            keys,
            ["github.com/o/motile", "github.com/o/api#api", "github.com/o/api#api-2", "ymac/d", "no-project"]
        );
    }

    #[test]
    fn a_server_runs_programs_once_it_says_it_speaks_their_protocol() {
        let info = |protocol| ServerInfo {
            protocol,
            ..serde_json::from_value(serde_json::json!({
                "version": "0.1.0", "protocol": 0, "hostname": "box", "home": "/root", "agents": [], "models": [],
            }))
            .unwrap()
        };
        assert!(runs_no_programs(None));
        assert!(runs_no_programs(Some(&info(UI_PROTOCOL - 1))));
        assert!(!runs_no_programs(Some(&info(UI_PROTOCOL))));
    }

    #[test]
    fn a_long_server_name_is_cut_to_ten_characters() {
        assert_eq!(short_name("studio"), "studio");
        assert_eq!(short_name("build-box1"), "build-box1");
        assert_eq!(short_name("Yektas-MacBook-Pro"), "Yektas-Mac");
        assert_eq!(short_name("my server one"), "my server");
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
            agent_account: "claude".into(),
            model: None,
            effort: None,
            access: motile_protocol::wire::Access::Full,
            created_at: 1.0,
            updated_at: 2.0,
            archived_at: None,
            snoozed_until: None,
            woke_at: None,
            position: 1.0,
            running,
            monitoring,
            needs_approval,
            asking: None,
            agents: 0,
            turn_ended_at: None,
            pull_request: None,
            watching: false,
            git_stage: None,
            interruption: None,
            parent: None,
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
    fn a_thread_opens_with_the_turns_back_to_where_it_was_left() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(&dir.path().join("cache.sqlite")).unwrap();
        let kind = |seq: u64| match seq % 3 {
            0 => ItemKind::User {
                text: format!("message {seq}"),
                attachments: Vec::new(),
                skills: Vec::new(),
                context: None,
                programs: Vec::new(),
            },
            1 => ItemKind::Assistant { text: format!("reply {seq}") },
            _ => ItemKind::TurnEnd { summary: Default::default() },
        };
        let items: Vec<Item> = (0..600)
            .map(|seq| Item {
                id: format!("i{seq}"),
                seq,
                rev: seq + 1,
                created_at: 0.0,
                media: Vec::new(),
                parent: None,
                ui: Default::default(),
                kind: kind(seq),
            })
            .collect();
        cache.save_items("t", &items.iter().collect::<Vec<_>>(), None);
        let Page { items: newest, mut earlier } = cache.page("t", None);
        let mut transcript = Transcript::new("/srv");
        transcript.load(newest);
        let row_of = |transcript: &Transcript, item: &str| {
            transcript.rows().iter().find(|row| row.item == item).map(|row| row.id.clone())
        };
        assert_eq!(row_of(&transcript, "i201"), None);

        load_back_to(&cache, "t", &mut transcript, &mut earlier, 201);

        let row_id = row_of(&transcript, "i201").unwrap();
        let place = Place { at: ScrollAt { row_id: row_id.clone(), delta: 40.0 }, item: "i201".into(), seq: 201 };
        assert_eq!(place.among(transcript.rows()), Some(place.at.clone()));
        let gone = Place { at: ScrollAt { row_id: "i201/gone".into(), delta: 40.0 }, ..place };
        assert_eq!(gone.among(transcript.rows()), Some(ScrollAt { row_id, delta: 0.0 }));
        assert!(earlier);
        assert_eq!(row_of(&transcript, "i99"), None);
    }

    #[test]
    fn a_monitoring_thread_never_opened_here_restores_from_its_flags() {
        let restored = restored_activity(&thread(false, true, false), None);
        assert_eq!(restored, Activity { monitoring: true, ..Activity::default() });
    }
}
