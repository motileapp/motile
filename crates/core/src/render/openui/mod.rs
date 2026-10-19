//! The programs agents write in `openui-lang` fences, as the transcript shows them: each runs in
//! the core (`host`), its tree is resolved for the clients to draw (`tree`), and after the row has
//! been sent with its tree, only what changes goes to the clients, as patches keyed by node id.
//!
//! A node's id is its path from the root, the steps joined by `/`: `children/2/action`.

pub mod host;
pub mod tree;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{Local, Offset, TimeZone};
use motile_openui::{Element, Env, FieldSpec, Forms, Library, Runtime, Step};
use motile_protocol::wire::Media;
use serde::Serialize;
use serde_json::{Map, Value};

use self::tree::UiNode;

/// What a node's id is made from.
pub type Path = Vec<Step>;

/// The keys of a node's JSON that aren't its props: a node whose `component` changes is replaced.
const ID: &str = "id";
const CHILDREN: &str = "children";
const COMPONENT: &str = "component";

/// A program row: the program and how it is drawn.
#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct UiRow {
    /// The program: the fence's text, or for a patch, the program it patches with it merged in.
    pub source: String,
    /// For a patch, the row of the program it was merged over.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// The fence has closed, or the reply has ended: the program won't grow.
    pub complete: bool,
    /// The program's tree as it is now, set as the row is sent. `ui_patch` events change it after.
    pub tree: Option<UiNode>,
    /// What keeps the program from running as it should, once it is complete.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    /// The thread's server is too old to run the program's tools, keep its state or look up its
    /// images and routes: what needs them is shown disabled.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub limited: bool,
    /// The fence as the agent wrote it.
    #[serde(skip)]
    pub fence: String,
    /// The fence patches the latest program before it: it has no `root` and no `Col` statement.
    #[serde(skip)]
    pub patch: bool,
}

impl UiRow {
    pub fn new(fence: String, complete: bool) -> Self {
        let patch = is_patch(&fence);
        Self { source: fence.clone(), complete, fence, patch, ..Self::default() }
    }
}

/// A fence with no `root` statement and no statement that is a `Col(...)` call, as the agents are
/// told.
fn is_patch(fence: &str) -> bool {
    let parsed = motile_openui::parse(fence);
    let starts_program = |statement: &motile_openui::ast::Statement| {
        statement.name == "root"
            || matches!(&statement.expr, motile_openui::ast::Expr::Call { name, at: false, .. } if name == "Col")
    };
    parsed.count > 0 && !parsed.statements.values().any(|statement| starts_program(statement))
}

/// A change to a program's tree, applied in order.
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum UiOp {
    /// The tree is now `node`, or there is none.
    Root {
        node: Option<Value>,
    },
    /// The node is now `node`, children and all.
    Replace {
        id: String,
        node: Value,
    },
    /// Some of the node's keys changed: `set` holds their new values, `unset` the keys it lost.
    Props {
        id: String,
        set: Map<String, Value>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        unset: Vec<String>,
    },
    /// A child at `index` of the node `parent`.
    Insert {
        parent: String,
        index: usize,
        node: Value,
    },
    Remove {
        id: String,
    },
    Errors {
        errors: Vec<String>,
    },
}

pub fn node_id(path: &[Step]) -> String {
    let step = |step: &Step| match step {
        Step::Index(index) => index.to_string(),
        Step::Key(key) => {
            let escaped = key.replace('~', "~0").replace('/', "~1");
            match escaped.is_empty() || escaped.bytes().all(|byte| byte.is_ascii_digit()) {
                true => format!("~k{escaped}"),
                false => escaped,
            }
        }
    };
    path.iter().map(step).collect::<Vec<_>>().join("/")
}

pub fn node_path(id: &str) -> Option<Path> {
    if id.is_empty() {
        return Some(Vec::new());
    }
    let unescape = |text: &str| text.replace("~1", "/").replace("~0", "~");
    id.split('/')
        .map(|segment| match segment.strip_prefix("~k") {
            Some(key) => Some(Step::Key(unescape(key))),
            None if segment.is_empty() => None,
            None if segment.bytes().all(|byte| byte.is_ascii_digit()) => segment.parse().ok().map(Step::Index),
            None => Some(Step::Key(unescape(segment))),
        })
        .collect()
}

/// A path one step further.
pub fn child(path: &[Step], step: Step) -> Path {
    let mut child = path.to_vec();
    child.push(step);
    child
}

/// The changes that turn the tree `before` into `after`, nodes matched by id.
pub fn diff(before: Option<&Value>, after: Option<&Value>, ops: &mut Vec<UiOp>) {
    match (before, after) {
        (Some(before), Some(after)) => diff_node(before, after, ops),
        (None, None) => {}
        (_, after) => ops.push(UiOp::Root { node: after.cloned() }),
    }
}

fn diff_node(before: &Value, after: &Value, ops: &mut Vec<UiOp>) {
    if before == after {
        return;
    }
    let id = after[ID].as_str().unwrap_or_default().to_string();
    if before[ID] != after[ID] || before[COMPONENT] != after[COMPONENT] {
        ops.push(match id.is_empty() {
            true => UiOp::Root { node: Some(after.clone()) },
            false => UiOp::Replace { id, node: after.clone() },
        });
        return;
    }
    let (Some(old), Some(new)) = (before.as_object(), after.as_object()) else {
        ops.push(UiOp::Root { node: Some(after.clone()) });
        return;
    };
    let own = |key: &String| key != ID && key != CHILDREN;
    let set: Map<String, Value> = new
        .iter()
        .filter(|(key, value)| own(key) && old.get(*key) != Some(*value))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let unset: Vec<String> = old.keys().filter(|key| own(key) && !new.contains_key(*key)).cloned().collect();
    if !set.is_empty() || !unset.is_empty() {
        ops.push(UiOp::Props { id: id.clone(), set, unset });
    }
    let children =
        |node: &Map<String, Value>| node.get(CHILDREN).and_then(Value::as_array).cloned().unwrap_or_default();
    let (old, new) = (children(old), children(new));
    let shared = old.len().min(new.len());
    for (before, after) in old.iter().zip(&new) {
        diff_node(before, after, ops);
    }
    for (index, node) in new.iter().enumerate().skip(shared) {
        ops.push(UiOp::Insert { parent: id.clone(), index, node: node.clone() });
    }
    for node in old[shared..].iter().rev() {
        ops.push(UiOp::Remove { id: node[ID].as_str().unwrap_or_default().to_string() });
    }
}

/// What the user did to a node that only the clients see, kept by the core between renders.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct NodeState {
    /// A flashcard shows its back.
    pub flipped: bool,
    /// An accordion's or a popover's state, once the user changed it.
    pub open: Option<bool>,
    /// A carousel's or a stepper's page, once the user moved it.
    pub index: Option<usize>,
    /// A map's category filter.
    pub filter: Option<String>,
    /// The marker picked on a map with no `$variable` for it.
    pub selected: Option<String>,
    /// An image whose `src` couldn't be loaded: a photo is found for its `alt`.
    pub fallback: bool,
    /// A `Timer`'s countdown, once started.
    pub countdown: Option<Countdown>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Countdown {
    pub running: bool,
    /// The seconds it started from, which Reset goes back to.
    pub from: f64,
}

/// A street route, or the stops in straight lines when the streets couldn't be found.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Route {
    /// Each `[lat, lng]`.
    pub points: Vec<[f64; 2]>,
    /// "Route overview" for the straight lines.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl Route {
    pub fn overview(stops: &[[f64; 2]]) -> Self {
        Self { points: stops.to_vec(), label: Some("Route overview".to_string()) }
    }
}

/// What the thread's server found for its programs, kept while the thread is open.
#[derive(Default)]
pub struct Lookups {
    /// The photos of each query; none while it is asked for.
    pub images: HashMap<String, Option<Vec<Media>>>,
    pub routes: HashMap<String, Option<Route>>,
}

/// The lookups a resolution needed and didn't find.
#[derive(Default)]
pub struct Wanted {
    pub images: HashSet<String>,
    pub routes: Vec<Vec<[f64; 2]>>,
}

/// What resolving a program's tree reads beyond its elements.
pub struct ResolveCtx<'a> {
    /// What a bound control shows (`value_at`), a field's error (`error_at`), the queries loading.
    pub runtime: &'a Runtime,
    /// See [`UiRow::limited`].
    pub limited: bool,
    lookups: &'a Lookups,
    nodes: &'a HashMap<String, NodeState>,
    wanted: RefCell<Wanted>,
}

impl<'a> ResolveCtx<'a> {
    pub fn new(
        runtime: &'a Runtime,
        limited: bool,
        lookups: &'a Lookups,
        nodes: &'a HashMap<String, NodeState>,
    ) -> Self {
        Self { runtime, limited, lookups, nodes, wanted: RefCell::default() }
    }

    /// The photos found for `query`, the best first, or none while they are looked for.
    pub fn images(&self, query: &str) -> Option<&'a [Media]> {
        if self.limited {
            return Some(&[]);
        }
        let found = self.lookups.images.get(query).and_then(Option::as_deref);
        if found.is_none() {
            self.wanted.borrow_mut().images.insert(query.to_string());
        }
        found
    }

    /// The route through the stops, each `[lat, lng]`, or none while it is looked for.
    pub fn route(&self, stops: &[[f64; 2]]) -> Option<Route> {
        if self.limited {
            return Some(Route::overview(stops));
        }
        let found = self.lookups.routes.get(&route_key(stops)).cloned().flatten();
        if found.is_none() {
            self.wanted.borrow_mut().routes.push(stops.to_vec());
        }
        found
    }

    /// What the user did to the node at `path` that only the clients see.
    pub fn node(&self, path: &[Step]) -> Option<&'a NodeState> {
        self.nodes.get(&node_id(path))
    }

    pub fn into_wanted(self) -> Wanted {
        self.wanted.into_inner()
    }
}

pub fn route_key(stops: &[[f64; 2]]) -> String {
    stops.iter().map(|[lat, lng]| format!("{lat:.6},{lng:.6}")).collect::<Vec<_>>().join(";")
}

/// The clock and time zone of the device, and random draws that stay the same from one render to
/// the next: the draws of a call start over from the program's seed, which a press moves on, so
/// `@Random` inside `@Set` draws anew on each click and the page stays still between them.
pub struct CoreEnv {
    seed: AtomicU64,
    draws: AtomicU64,
}

impl CoreEnv {
    pub fn new(seed: u64) -> Self {
        Self { seed: AtomicU64::new(seed), draws: AtomicU64::new(0) }
    }

    /// Starts the draws over, before a call into the runtime.
    pub fn rewind(&self) {
        self.draws.store(0, Ordering::Relaxed);
    }

    /// Moves to the next seed, for a press.
    pub fn advance(&self) {
        let seed = self.seed.load(Ordering::Relaxed);
        self.seed.store(mix(seed), Ordering::Relaxed);
        self.rewind();
    }
}

impl Env for CoreEnv {
    fn now(&self) -> f64 {
        chrono::Utc::now().timestamp_millis() as f64
    }

    fn random(&self) -> f64 {
        let draw = self.draws.fetch_add(1, Ordering::Relaxed);
        let bits = mix(self.seed.load(Ordering::Relaxed) ^ mix(draw));
        (bits >> 11) as f64 / (1u64 << 53) as f64
    }

    fn utc_offset_minutes(&self, at: f64) -> f64 {
        let Some(utc) = chrono::DateTime::from_timestamp_millis(at as i64) else { return 0.0 };
        let offset = Local.offset_from_utc_datetime(&utc.naive_utc()).fix();
        f64::from(offset.local_minus_utc()) / 60.0
    }
}

/// SplitMix64's finalizer.
fn mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A seed of its own for each program, the same each time the thread opens.
pub fn seed(row_id: &str) -> u64 {
    row_id.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

/// The forms of open-intelligent-ui's library (`useStateField`): a `Form` is named by its
/// statement, and a control keeps its value under its label, or its statement's name.
pub struct LibraryForms {
    library: Arc<Library>,
}

impl LibraryForms {
    pub fn new(library: Arc<Library>) -> Self {
        Self { library }
    }
}

/// The prop a control's value is in: its first one that takes a `$variable`.
pub fn value_prop(library: &Library, component: &str) -> Option<String> {
    let def = library.component(component)?;
    def.props.iter().find(|prop| prop.bindable).map(|prop| prop.name.clone())
}

impl Forms for LibraryForms {
    fn form(&self, element: &Element) -> Option<String> {
        (element.component == "Form").then(|| element.statement.clone()).flatten()
    }

    fn field(&self, element: &Element) -> Option<FieldSpec> {
        if element.component == "Map" {
            return None;
        }
        let prop = value_prop(&self.library, &element.component)?;
        let label = element.prop("label").and_then(|label| label.as_str()).filter(|label| !label.is_empty());
        let name = match (label, &element.statement) {
            (Some(label), _) => label.to_string(),
            (None, Some(statement)) => statement.clone(),
            (None, None) => {
                let others = element.props.iter().filter(|(name, _)| **name != prop);
                let text = others.map(|(name, value)| format!("{name}={}", value.to_json_string())).collect::<String>();
                format!("{}#{:x}", element.component, seed(&text))
            }
        };
        Some(FieldSpec { name, prop, component_type: None, rules: None })
    }
}
