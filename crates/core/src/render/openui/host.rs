//! The programs of an open thread, each in a runtime of its own. What the user does to them, the
//! tools' answers, the timers and the lookups go in; what the core is to carry out comes out:
//! patches for the clients, tool calls, timers, messages to the agent and the state to keep.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use motile_openui::{Effects, Element, Library, Runtime, Step, ToolError};
use motile_protocol::wire::{Item, Media};
use serde::Deserialize;
use serde_json::{Value, json};

use super::tree::{PhotoState, Photos, ResolveCtx, Resolver, Route, UiNode, UiState, Widgets};
use super::{CoreEnv, LibraryForms, UiOp, UiRow, diff, node_id, node_path, route_key, seed, value_prop};
use crate::render::highlight::{self, Spans};
use crate::render::rows::{Row, RowKind};

/// How long a program's state stays as it is before it is kept on the server.
const SAVE_AFTER: Duration = Duration::from_millis(500);
/// How long a program wants the photos of a query before they are looked up, so that a query
/// being typed isn't looked up at every letter.
const LOOK_UP_AFTER: Duration = Duration::from_millis(300);
/// How many of the states sent to the server are known again when they come back.
const SAVES_KEPT: usize = 8;
const RUN: &str = "run";
const OLD_SERVER: &str = "Your server needs an update to run this.";
const REFUSED: &str = "The user didn't allow the command.";
const NOT_CLICKED: &str = "run runs only from a click, through Mutation and @Run.";
const CONTEXT: &str = "context and form values = ";

/// What the user did to a node of a program.
#[derive(Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiEvent {
    /// Runs the node's action; a button with none sends its label to the agent.
    Press,
    /// A control's new value: text, a number, a boolean, a date as `YYYY-MM-DD`, an option's value.
    Value {
        value: Value,
    },
    /// Sends `message` to the agent with the values of the form around the node.
    Ask {
        message: String,
    },
    /// A map's marker was picked, or none.
    Select {
        value: Option<String>,
    },
    /// A map shows the markers of one category, or all.
    Filter {
        category: Option<String>,
    },
    /// A flashcard turned over.
    Flip,
    /// An accordion or a popover opened or closed.
    Expand {
        open: bool,
    },
    /// A timer's countdown.
    Start,
    Pause,
    Reset,
    /// An image's `src` couldn't be loaded.
    Fallback,
    /// Fetches the queries that failed again.
    Retry,
}

/// What a timer ticks for: a query that refreshes, or a `Timer` counting down.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Ticker {
    Query(u64),
    Countdown(String),
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct TimerKey {
    pub row_id: String,
    pub serial: u64,
    pub ticker: Ticker,
}

/// What the core carries out for the programs.
#[derive(Clone, PartialEq, Debug)]
pub enum Out {
    Patch {
        row_id: String,
        ops: Vec<UiOp>,
    },
    /// A tool call, answered with [`Programs::tool_result`].
    Call {
        row_id: String,
        serial: u64,
        call: u64,
        tool: String,
        args: Value,
        confirmed: bool,
    },
    /// A command the user is to allow before it runs, answered with [`Programs::confirm`].
    Confirm {
        row_id: String,
        call_id: u64,
        command: String,
    },
    /// Calls [`Programs::tick`] every `seconds` until stopped.
    Interval {
        key: TimerKey,
        seconds: f64,
    },
    Stop {
        key: TimerKey,
    },
    /// A message for the agent, sent like any other.
    Send {
        text: String,
        context: Option<Value>,
    },
    OpenUrl {
        url: String,
    },
    Copy {
        text: String,
    },
    /// The state to keep on the server.
    Save {
        item_id: String,
        block: u32,
        state: Value,
    },
    /// Photos to look up, answered with [`Programs::images`].
    Images {
        query: String,
    },
    /// A route to look up, answered with [`Programs::route`].
    Route {
        stops: Vec<[f64; 2]>,
    },
    /// Calls [`Programs::wake`] after a while.
    Wake {
        after: Duration,
    },
    /// Code to highlight off the core's loop, then [`Programs::highlighted`].
    Highlight {
        language: String,
        code: String,
    },
}

/// Motile's library, built once.
pub fn library() -> Arc<Library> {
    static LIBRARY: OnceLock<Arc<Library>> = OnceLock::new();
    LIBRARY.get_or_init(|| Arc::new(motile_openui::motile::library())).clone()
}

/// What the user did to a node that only the clients see.
#[derive(Clone, PartialEq, Debug, Default)]
struct NodeState {
    open: bool,
    flipped: bool,
    filter: Option<String>,
    /// The marker picked on a map with no `$variable` for it.
    selected: Option<String>,
    failed: bool,
    countdown: Option<Countdown>,
    /// How many times a `Celebration` went off.
    burst: u32,
    /// What its `when` was last.
    when: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Countdown {
    running: bool,
    /// The seconds it started from, which Reset goes back to.
    from: f64,
}

/// What the thread's server found for its programs; none while it is asked.
#[derive(Default)]
struct Lookups {
    images: HashMap<String, Option<Vec<Media>>>,
    /// The street route through stops, or none when there is no street route.
    routes: HashMap<String, Option<Option<Vec<[f64; 2]>>>>,
}

/// The lookups and the highlighting a tree waited for.
#[derive(Default)]
struct Wanted {
    images: HashSet<String>,
    routes: Vec<Vec<[f64; 2]>>,
    code: Vec<(String, String)>,
}

/// A program's state as its resolution reads it.
struct HostState<'a> {
    runtime: &'a Runtime,
    limited: bool,
    lookups: &'a Lookups,
    nodes: &'a HashMap<String, NodeState>,
    wanted: RefCell<Wanted>,
}

impl UiState for HostState<'_> {
    fn value(&self, path: &[Step], prop: &str) -> Option<motile_openui::Value> {
        let selected = self.nodes.get(&node_id(path)).and_then(|node| node.selected.as_ref());
        match selected.filter(|_| prop == "selected") {
            Some(selected) => Some(selected.as_str().into()),
            None => self.runtime.value_at(path, prop),
        }
    }

    fn error(&self, path: &[Step]) -> Option<String> {
        self.runtime.error_at(path).map(str::to_string)
    }

    fn photos(&self, query: &str) -> Photos {
        let missing = Photos { state: PhotoState::Missing, media: Vec::new() };
        if self.limited {
            return missing;
        }
        match self.lookups.images.get(query) {
            Some(Some(found)) if !found.is_empty() => {
                Photos { state: PhotoState::Found, media: found.iter().map(|media| media.id.clone()).collect() }
            }
            Some(Some(_)) => missing,
            Some(None) => Photos { state: PhotoState::Waiting, media: Vec::new() },
            None => {
                self.wanted.borrow_mut().images.insert(query.to_string());
                Photos { state: PhotoState::Waiting, media: Vec::new() }
            }
        }
    }

    fn route(&self, stops: &[[f64; 2]]) -> Route {
        if self.limited {
            return Route::Failed;
        }
        match self.lookups.routes.get(&route_key(stops)) {
            Some(Some(Some(points))) => Route::Street(points.clone()),
            Some(Some(None)) => Route::Failed,
            Some(None) => Route::Loading,
            None => {
                self.wanted.borrow_mut().routes.push(stops.to_vec());
                Route::Loading
            }
        }
    }

    fn widgets(&self, id: &str) -> Widgets {
        let Some(node) = self.nodes.get(id) else { return Widgets::default() };
        Widgets {
            open: node.open,
            flipped: node.flipped,
            running: node.countdown.is_some_and(|countdown| countdown.running),
            started: node.countdown.map(|countdown| countdown.from),
            filter: node.filter.clone(),
            burst: node.burst,
            failed: node.failed,
        }
    }

    fn spans(&self, language: &str, code: &str) -> Option<Spans> {
        let spans = highlight::cached(language, code);
        if spans.is_none() {
            self.wanted.borrow_mut().code.push((language.to_string(), code.to_string()));
        }
        spans
    }
}

struct Hosted {
    /// Tells this program from one that had its row before.
    serial: u64,
    item_id: String,
    block: u32,
    source: String,
    complete: bool,
    runtime: Runtime,
    env: Arc<CoreEnv>,
    resolver: Resolver,
    /// The tree the clients have, as JSON.
    shown: Option<Value>,
    shown_errors: Vec<String>,
    errors: Vec<String>,
    nodes: HashMap<String, NodeState>,
    /// Since when the state has changed without being kept.
    dirty: Option<Instant>,
    /// The states sent to the server lately, which aren't another device's when they come back.
    saved: VecDeque<Value>,
    /// The `run` calls waiting for the user, with their arguments.
    confirming: HashMap<u64, Value>,
    tickers: HashSet<Ticker>,
    /// The photos its tree asked for when it was last resolved.
    wants: HashSet<String>,
}

#[derive(Default)]
pub struct Programs {
    programs: HashMap<String, Hosted>,
    serials: u64,
    /// See [`UiRow::limited`].
    pub limited: bool,
    /// The thread runs commands without asking the user.
    pub full_access: bool,
    lookups: Lookups,
    /// The queries of photos the programs want, since when, until they are looked up.
    wanted: HashMap<String, Instant>,
    /// The code being highlighted.
    highlighting: HashSet<(String, String)>,
    waking: bool,
}

impl Programs {
    /// Runs the programs of the rows about to be sent and puts their trees in them. `saved` is the
    /// state an item keeps for a block.
    pub fn dress(&mut self, rows: &mut [Row], saved: &dyn Fn(&str, u32) -> Option<Value>) -> Vec<Out> {
        let mut outs = Vec::new();
        for row in rows {
            let RowKind::Ui { ui } = &mut row.kind else { continue };
            self.host(&row.id, ui, saved, &mut outs);
            let Some(hosted) = self.programs.get_mut(&row.id) else { continue };
            let (tree, wanted) = view(hosted, &self.lookups, self.limited);
            ui.tree = tree.map(Box::new);
            ui.errors = hosted.errors.clone();
            ui.limited = self.limited;
            hosted.shown = Some(json!(ui.tree));
            hosted.shown_errors = hosted.errors.clone();
            self.want(&row.id, wanted, &mut outs);
        }
        outs
    }

    fn host(&mut self, row_id: &str, ui: &UiRow, saved: &dyn Fn(&str, u32) -> Option<Value>, outs: &mut Vec<Out>) {
        if !self.programs.contains_key(row_id) {
            let Some((item_id, block)) = row_id.rsplit_once('/') else { return };
            let Ok(block) = block.parse() else { return };
            let library = library();
            let env = Arc::new(CoreEnv::new(seed(row_id)));
            let mut runtime = Runtime::new(library.clone(), env.clone(), Arc::new(LibraryForms::new(library)));
            let base = ui.base.as_ref().and_then(|base| self.programs.get(base)).map(|base| base.runtime.snapshot());
            let state = saved(item_id, block).filter(|state| !state.is_null()).map(motile_openui::Value::from);
            if let Some(state) = state.or(base) {
                runtime.restore(&state);
            }
            self.serials += 1;
            let hosted = Hosted {
                serial: self.serials,
                item_id: item_id.to_string(),
                block,
                source: String::new(),
                complete: false,
                runtime,
                env,
                resolver: Resolver::default(),
                shown: None,
                shown_errors: Vec::new(),
                errors: Vec::new(),
                nodes: HashMap::new(),
                dirty: None,
                saved: VecDeque::new(),
                confirming: HashMap::new(),
                tickers: HashSet::new(),
                wants: HashSet::new(),
            };
            self.programs.insert(row_id.to_string(), hosted);
        }
        let Some(hosted) = self.programs.get_mut(row_id) else { return };
        if hosted.source == ui.source && hosted.complete == ui.complete {
            return;
        }
        hosted.source.clone_from(&ui.source);
        hosted.complete = ui.complete;
        hosted.env.rewind();
        let effects = hosted.runtime.set_text(&ui.source, ui.complete);
        self.carry(row_id, effects, outs);
    }

    /// Patches the clients' tree of the program with what changed in it.
    fn repaint(&mut self, row_id: &str, outs: &mut Vec<Out>) {
        let Some(hosted) = self.programs.get_mut(row_id).filter(|hosted| hosted.shown.is_some()) else { return };
        let (tree, wanted) = view(hosted, &self.lookups, self.limited);
        let tree = tree.map(|tree| json!(tree));
        let mut ops = Vec::new();
        diff(hosted.shown.as_ref().filter(|shown| !shown.is_null()), tree.as_ref(), &mut ops);
        if hosted.errors != hosted.shown_errors {
            ops.push(UiOp::Errors { errors: hosted.errors.clone() });
        }
        hosted.shown = Some(tree.unwrap_or_default());
        hosted.shown_errors = hosted.errors.clone();
        if !ops.is_empty() {
            outs.push(Out::Patch { row_id: row_id.to_string(), ops });
        }
        self.want(row_id, wanted, outs);
    }

    fn repaint_all(&mut self, outs: &mut Vec<Out>) {
        let row_ids: Vec<String> = self.programs.keys().cloned().collect();
        for row_id in row_ids {
            self.repaint(&row_id, outs);
        }
    }

    /// Asks for the routes and the highlighting a program's tree needs, and soon for its photos.
    fn want(&mut self, row_id: &str, wanted: Wanted, outs: &mut Vec<Out>) {
        for (language, code) in wanted.code {
            if self.highlighting.insert((language.clone(), code.clone())) {
                outs.push(Out::Highlight { language, code });
            }
        }
        let Some(hosted) = self.programs.get_mut(row_id) else { return };
        if hosted.runtime.is_streaming() || self.limited {
            return;
        }
        let now = Instant::now();
        for query in &wanted.images {
            if !self.lookups.images.contains_key(query) {
                self.wanted.entry(query.clone()).or_insert(now);
            }
        }
        hosted.wants = wanted.images;
        for stops in wanted.routes {
            let key = route_key(&stops);
            if self.lookups.routes.contains_key(&key) {
                continue;
            }
            self.lookups.routes.insert(key, None);
            outs.push(Out::Route { stops });
        }
        if !self.wanted.is_empty() {
            self.wake_in(LOOK_UP_AFTER, outs);
        }
    }

    fn wake_in(&mut self, after: Duration, outs: &mut Vec<Out>) {
        if !self.waking {
            self.waking = true;
            outs.push(Out::Wake { after });
        }
    }

    /// Carries out what a call into a program's runtime asked for.
    fn carry(&mut self, row_id: &str, effects: Effects, outs: &mut Vec<Out>) {
        let mut pending = vec![effects];
        let mut dirty = false;
        while let Some(effects) = pending.pop() {
            let Some(hosted) = self.programs.get_mut(row_id) else { return };
            if let Some(errors) = &effects.errors {
                hosted.errors =
                    errors.iter().filter(|error| error.severity.is_none()).map(|error| error.message.clone()).collect();
            }
            let key = |ticker| TimerKey { row_id: row_id.to_string(), serial: hosted.serial, ticker };
            for timer in effects.timers {
                hosted.tickers.insert(Ticker::Query(timer.id));
                outs.push(Out::Interval { key: key(Ticker::Query(timer.id)), seconds: timer.seconds });
            }
            for timer in effects.cancel {
                hosted.tickers.remove(&Ticker::Query(timer));
                outs.push(Out::Stop { key: key(Ticker::Query(timer)) });
            }
            for call in effects.calls {
                if self.limited {
                    hosted.env.rewind();
                    pending.push(hosted.runtime.tool_result(call.id, Err(ToolError::Failed(OLD_SERVER.into()))));
                    continue;
                }
                if call.tool == RUN && !call.mutation {
                    hosted.env.rewind();
                    pending.push(hosted.runtime.tool_result(call.id, Err(ToolError::Failed(NOT_CLICKED.into()))));
                    continue;
                }
                let args = call.args.to_json();
                if call.tool == RUN && !self.full_access {
                    let command = args["command"].as_str().unwrap_or_default().to_string();
                    hosted.confirming.insert(call.id, args);
                    outs.push(Out::Confirm { row_id: row_id.to_string(), call_id: call.id, command });
                    continue;
                }
                let (serial, call, tool) = (hosted.serial, call.id, call.tool);
                outs.push(Out::Call { row_id: row_id.to_string(), serial, call, tool, args, confirmed: false });
            }
            outs.extend(effects.events.iter().filter_map(|event| told(event, self.limited)));
            dirty |= effects.state;
        }
        if dirty && !self.limited {
            let Some(hosted) = self.programs.get_mut(row_id) else { return };
            hosted.dirty.get_or_insert_with(Instant::now);
            self.wake_in(SAVE_AFTER, outs);
        }
    }

    /// What the user did to the node `node` of the program in the row.
    pub fn event(&mut self, row_id: &str, node: &str, event: UiEvent) -> Vec<Out> {
        let mut outs = Vec::new();
        let Some(path) = node_path(node) else { return outs };
        let Some(hosted) = self.programs.get_mut(row_id) else { return outs };
        let Some(element) = hosted.runtime.root().and_then(|root| element_at(root, &path)) else { return outs };
        let prop = value_prop(&library(), &element.component);
        let action = element.props.iter().find(|(_, value)| value.as_plan().is_some()).map(|(name, _)| name.clone());
        let label = element.prop("label").and_then(|label| label.as_str()).map(str::to_string);
        let bound = prop.as_ref().is_some_and(|prop| element.bindings.contains_key(prop));
        let state = hosted.nodes.entry(node.to_string()).or_default();
        let runtime = &mut hosted.runtime;
        hosted.env.rewind();
        let effects = match event {
            UiEvent::Press => {
                hosted.env.advance();
                match (action, label) {
                    (Some(action), _) => runtime.press(&path, &action),
                    (None, Some(label)) => runtime.ask(&path, &label),
                    (None, None) => Effects::default(),
                }
            }
            UiEvent::Value { value } => match prop {
                Some(prop) => runtime.input(&path, &prop, value.into()),
                None => Effects::default(),
            },
            UiEvent::Ask { message } => runtime.ask(&path, &message),
            UiEvent::Select { value } => match prop.filter(|_| bound) {
                Some(prop) => runtime.input(&path, &prop, value.map(Into::into).unwrap_or_default()),
                None => {
                    state.selected = value;
                    Effects::default()
                }
            },
            UiEvent::Filter { category } => {
                state.filter = category;
                Effects::default()
            }
            UiEvent::Flip => {
                state.flipped = !state.flipped;
                Effects::default()
            }
            UiEvent::Expand { open } => {
                state.open = open;
                Effects::default()
            }
            UiEvent::Fallback => {
                state.failed = true;
                Effects::default()
            }
            UiEvent::Retry => runtime.retry(),
            UiEvent::Start | UiEvent::Pause | UiEvent::Reset => {
                let Some(prop) = prop else { return outs };
                let key = TimerKey {
                    row_id: row_id.to_string(),
                    serial: hosted.serial,
                    ticker: Ticker::Countdown(node.into()),
                };
                let left = seconds_left(runtime, &path, &prop);
                let starting = event == UiEvent::Start;
                match state.countdown {
                    _ if starting && left > 0.0 => {
                        let from = state.countdown.map_or(left, |countdown| countdown.from);
                        state.countdown = Some(Countdown { running: true, from });
                        hosted.tickers.insert(key.ticker.clone());
                        outs.push(Out::Interval { key, seconds: 1.0 });
                        Effects::default()
                    }
                    Some(countdown) if event == UiEvent::Pause => {
                        state.countdown = Some(Countdown { running: false, ..countdown });
                        hosted.tickers.remove(&key.ticker);
                        outs.push(Out::Stop { key });
                        Effects::default()
                    }
                    Some(countdown) if event == UiEvent::Reset => {
                        state.countdown = None;
                        hosted.tickers.remove(&key.ticker);
                        outs.push(Out::Stop { key });
                        runtime.input(&path, &prop, countdown.from.into())
                    }
                    _ => Effects::default(),
                }
            }
        };
        self.carry(row_id, effects, &mut outs);
        self.repaint(row_id, &mut outs);
        outs
    }

    /// The user allowed the command of a `run` call, or not.
    pub fn confirm(&mut self, row_id: &str, call_id: u64, allow: bool) -> Vec<Out> {
        let mut outs = Vec::new();
        let Some(hosted) = self.programs.get_mut(row_id) else { return outs };
        let Some(args) = hosted.confirming.remove(&call_id) else { return outs };
        if allow {
            let (row_id, serial, tool) = (row_id.to_string(), hosted.serial, RUN.to_string());
            outs.push(Out::Call { row_id, serial, call: call_id, tool, args, confirmed: true });
            return outs;
        }
        hosted.env.rewind();
        let effects = hosted.runtime.tool_result(call_id, Err(ToolError::Failed(REFUSED.into())));
        self.carry(row_id, effects, &mut outs);
        self.repaint(row_id, &mut outs);
        outs
    }

    /// What a tool call answered.
    pub fn tool_result(&mut self, row_id: &str, serial: u64, call: u64, result: Result<Value, ToolError>) -> Vec<Out> {
        let mut outs = Vec::new();
        let Some(hosted) = self.programs.get_mut(row_id).filter(|hosted| hosted.serial == serial) else { return outs };
        hosted.env.rewind();
        let effects = hosted.runtime.tool_result(call, result.map(Into::into));
        self.carry(row_id, effects, &mut outs);
        self.repaint(row_id, &mut outs);
        outs
    }

    /// A timer ticked.
    pub fn tick(&mut self, key: &TimerKey) -> Vec<Out> {
        let mut outs = Vec::new();
        let hosted = self.programs.get_mut(&key.row_id).filter(|hosted| hosted.serial == key.serial);
        let Some(hosted) = hosted.filter(|hosted| hosted.tickers.contains(&key.ticker)) else {
            outs.push(Out::Stop { key: key.clone() });
            return outs;
        };
        hosted.env.rewind();
        let effects = match &key.ticker {
            Ticker::Query(timer) => hosted.runtime.timer(*timer),
            Ticker::Countdown(node) => {
                let Some(path) = node_path(node) else { return outs };
                let element = hosted.runtime.root().and_then(|root| element_at(root, &path));
                let Some(prop) = element.and_then(|element| value_prop(&library(), &element.component)) else {
                    return outs;
                };
                let next = (seconds_left(&hosted.runtime, &path, &prop) - 1.0).max(0.0);
                if next == 0.0 {
                    if let Some(countdown) = hosted.nodes.get_mut(node).and_then(|state| state.countdown.as_mut()) {
                        countdown.running = false;
                    }
                    hosted.tickers.remove(&key.ticker);
                    outs.push(Out::Stop { key: key.clone() });
                }
                hosted.runtime.input(&path, &prop, next.into())
            }
        };
        self.carry(&key.row_id, effects, &mut outs);
        self.repaint(&key.row_id, &mut outs);
        outs
    }

    /// Keeps the state that has stayed still long enough, and looks up the photos wanted long
    /// enough.
    pub fn wake(&mut self, now: Instant) -> Vec<Out> {
        self.waking = false;
        let mut outs = Vec::new();
        let mut next: Option<Duration> = None;
        let mut sooner = |left: Duration| next = Some(next.map_or(left, |next: Duration| next.min(left)));
        for hosted in self.programs.values_mut() {
            let Some(since) = hosted.dirty else { continue };
            let waited = now.saturating_duration_since(since);
            if waited < SAVE_AFTER {
                sooner(SAVE_AFTER - waited);
                continue;
            }
            outs.push(save(hosted));
        }
        let wanted = std::mem::take(&mut self.wanted);
        for (query, since) in wanted {
            if !self.programs.values().any(|hosted| hosted.wants.contains(&query)) {
                continue;
            }
            let waited = now.saturating_duration_since(since);
            if waited < LOOK_UP_AFTER {
                sooner(LOOK_UP_AFTER - waited);
                self.wanted.insert(query, since);
                continue;
            }
            self.lookups.images.insert(query.clone(), None);
            outs.push(Out::Images { query });
        }
        if let Some(after) = next {
            self.wake_in(after, &mut outs);
        }
        outs
    }

    /// Fetches again what every program's queries failed to, as a server that couldn't run them
    /// now can.
    pub fn retry(&mut self) -> Vec<Out> {
        let mut outs = Vec::new();
        let row_ids: Vec<String> = self.programs.keys().cloned().collect();
        for row_id in row_ids {
            let Some(hosted) = self.programs.get_mut(&row_id) else { continue };
            hosted.env.rewind();
            let effects = hosted.runtime.retry();
            self.carry(&row_id, effects, &mut outs);
            self.repaint(&row_id, &mut outs);
        }
        outs
    }

    /// Keeps every state that changed now, as the thread closes.
    pub fn flush(&mut self) -> Vec<Out> {
        self.programs.values_mut().filter(|hosted| hosted.dirty.is_some()).map(save).collect()
    }

    /// The photos found for a query.
    pub fn images(&mut self, query: &str, found: Vec<Media>) -> Vec<Out> {
        self.lookups.images.insert(query.to_string(), Some(found));
        let mut outs = Vec::new();
        let wanting: Vec<String> =
            self.programs.iter().filter(|(_, hosted)| hosted.wants.contains(query)).map(|(id, _)| id.clone()).collect();
        for row_id in wanting {
            self.repaint(&row_id, &mut outs);
        }
        outs
    }

    /// The streets through the stops, or none when they couldn't be found.
    pub fn route(&mut self, stops: &[[f64; 2]], points: Option<Vec<[f64; 2]>>) -> Vec<Out> {
        let points = points.filter(|points| points.len() > 1);
        self.lookups.routes.insert(route_key(stops), Some(points));
        let mut outs = Vec::new();
        self.repaint_all(&mut outs);
        outs
    }

    /// Code has been highlighted.
    pub fn highlighted(&mut self, language: &str, code: &str) -> Vec<Out> {
        self.highlighting.remove(&(language.to_string(), code.to_string()));
        let mut outs = Vec::new();
        self.repaint_all(&mut outs);
        outs
    }

    /// Takes the state another device left an item's programs in.
    pub fn saved(&mut self, item: &Item) -> Vec<Out> {
        let mut outs = Vec::new();
        for (block, state) in &item.ui {
            let row_id = format!("{}/{block}", item.id);
            let Some(hosted) = self.programs.get_mut(&row_id) else { continue };
            let ours = hosted.dirty.is_some() || hosted.saved.contains(state);
            if state.is_null() || ours || hosted.runtime.snapshot().to_json() == *state {
                continue;
            }
            hosted.env.rewind();
            let effects = hosted.runtime.restore(&state.into());
            self.carry(&row_id, Effects { state: false, ..effects }, &mut outs);
            self.repaint(&row_id, &mut outs);
        }
        outs
    }

    /// Lets go of every program, as the thread starts over.
    pub fn clear(&mut self) -> Vec<Out> {
        let mut outs = self.flush();
        for (row_id, hosted) in self.programs.drain() {
            for ticker in hosted.tickers {
                outs.push(Out::Stop { key: TimerKey { row_id: row_id.clone(), serial: hosted.serial, ticker } });
            }
        }
        outs
    }

    /// The program's state, for tests.
    #[cfg(test)]
    pub fn snapshot(&self, row_id: &str) -> Option<Value> {
        self.programs.get(row_id).map(|hosted| hosted.runtime.snapshot().to_json())
    }

    /// The tree the clients have, for tests.
    #[cfg(test)]
    pub fn shown(&self, row_id: &str) -> Option<&Value> {
        self.programs.get(row_id)?.shown.as_ref()
    }
}

/// The program's tree, and what it waited for. A `Celebration` whose `when` turned true goes off.
fn view(hosted: &mut Hosted, lookups: &Lookups, limited: bool) -> (Option<UiNode>, Wanted) {
    let Some(root) = hosted.runtime.root().cloned() else { return (None, Wanted::default()) };
    let mut celebrations = Vec::new();
    find(&motile_openui::Value::Element(root.clone()), "Celebration", &mut Vec::new(), &mut celebrations);
    for (id, element) in celebrations {
        let when = element.prop("when").and_then(|when| when.as_bool()).unwrap_or(false);
        let node = hosted.nodes.entry(id).or_default();
        node.burst += u32::from(when && !node.when);
        node.when = when;
    }
    let state =
        HostState { runtime: &hosted.runtime, limited, lookups, nodes: &hosted.nodes, wanted: RefCell::default() };
    let ctx = ResolveCtx { streaming: hosted.runtime.is_streaming(), tools: !limited, state: &state };
    let tree = hosted.resolver.resolve(&root, &ctx);
    (Some(tree), state.wanted.into_inner())
}

/// The components named `component` in a value, by node id.
fn find(value: &motile_openui::Value, component: &str, path: &mut Vec<Step>, found: &mut Vec<(String, Arc<Element>)>) {
    let visit = |step: Step, value: &motile_openui::Value, path: &mut Vec<Step>, found: &mut Vec<_>| {
        path.push(step);
        find(value, component, path, found);
        path.pop();
    };
    match value {
        motile_openui::Value::Element(element) => {
            if element.component == component {
                found.push((node_id(path), element.clone()));
            }
            for (key, value) in &element.props {
                visit(Step::Key(key.clone()), value, path, found);
            }
        }
        motile_openui::Value::Array(items) => {
            for (index, value) in items.iter().enumerate() {
                visit(Step::Index(index), value, path, found);
            }
        }
        motile_openui::Value::Object(map) => {
            for (key, value) in map {
                visit(Step::Key(key.clone()), value, path, found);
            }
        }
        _ => {}
    }
}

fn save(hosted: &mut Hosted) -> Out {
    hosted.dirty = None;
    let state = hosted.runtime.snapshot().to_json();
    hosted.saved.push_back(state.clone());
    if hosted.saved.len() > SAVES_KEPT {
        hosted.saved.pop_front();
    }
    Out::Save { item_id: hosted.item_id.clone(), block: hosted.block, state }
}

fn seconds_left(runtime: &Runtime, path: &[Step], prop: &str) -> f64 {
    let left = runtime.value_at(path, prop).and_then(|value| value.as_number()).unwrap_or(0.0);
    if left.is_finite() { left.floor().max(0.0) } else { 0.0 }
}

/// The component at `path` from the root.
fn element_at<'a>(root: &'a Element, path: &[Step]) -> Option<&'a Element> {
    let mut current: Option<&motile_openui::Value> = None;
    let mut element = root;
    for step in path {
        let value = match (step, current) {
            (Step::Key(key), None) => element.props.get(key)?,
            (Step::Key(key), Some(motile_openui::Value::Object(map))) => map.get(key)?,
            (Step::Index(index), Some(motile_openui::Value::Array(items))) => items.get(*index)?,
            _ => return None,
        };
        match value {
            motile_openui::Value::Element(inner) => {
                element = inner;
                current = None;
            }
            value => current = Some(value),
        }
    }
    current.is_none().then_some(element)
}

/// What an event of a program asks of the core. A message for the agent carries the program's
/// context; an old server that can't take it gets it in the message's text.
fn told(event: &motile_openui::Event, limited: bool) -> Option<Out> {
    let param = |name: &str| event.params.get(name).and_then(|value| value.as_str()).map(str::to_string);
    match event.kind.as_str() {
        "continue_conversation" => {
            let mut context = vec![json!(format!("User clicked: {}", event.message)), event.form_state.to_json()];
            context.extend(event.params.get("context").map(motile_openui::Value::to_json));
            let context = Value::Array(context);
            Some(match limited {
                true => Out::Send { text: format!("{}\n{CONTEXT}{context}", event.message), context: None },
                false => Out::Send { text: event.message.clone(), context: Some(context) },
            })
        }
        "open_url" => {
            let url = param("url")?;
            let web = url.starts_with("https://") || url.starts_with("http://");
            web.then_some(Out::OpenUrl { url })
        }
        "CopyToClipboard" => Some(Out::Copy { text: param("text")? }),
        _ => None,
    }
}
