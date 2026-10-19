//! The programs agents write in `openui-lang` fences, as the transcript shows them: each runs in
//! the core (`host`), its tree is resolved for the clients to draw (`tree`), and after the row has
//! been sent with its tree, only what changes goes to the clients, as patches keyed by node id.
//!
//! A node's id is its path from the root, the steps joined by `/`: `children/2/action`.

pub mod canvas;
pub mod host;
pub mod tree;
pub mod web;

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{Local, Offset, TimeZone};
use motile_openui::{Element, Env, FieldSpec, Forms, Library, Step};
use serde::Serialize;
use serde_json::{Map, Value};

use self::tree::UiNode;

/// What a node's id is made from.
pub type Path = Vec<Step>;

/// What a node's JSON is known by: a node whose `component` changes is replaced.
const ID: &str = "id";
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
    pub tree: Option<Box<UiNode>>,
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
    /// The fence patches the latest program before it, when there is one: it has no `root`.
    #[serde(skip)]
    pub patch: bool,
}

impl UiRow {
    pub fn new(fence: String, complete: bool) -> Self {
        let patch = motile_openui::fences::is_patch(&fence);
        Self { source: fence.clone(), complete, fence, patch, ..Self::default() }
    }
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
    /// Some of the node's keys changed: `set` holds their new values, `unset` the keys it lost. A
    /// key whose value is a node, or a list of nodes, changes through the ops of those nodes.
    Props {
        id: String,
        set: Map<String, Value>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        unset: Vec<String>,
    },
    /// A node at `index` of the list `key` of the node `parent`.
    Insert {
        parent: String,
        key: String,
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
    let (Some(old), Some(new)) = (before.as_object(), after.as_object()) else { return };
    if before[ID] != after[ID] || before[COMPONENT] != after[COMPONENT] {
        ops.push(match id.is_empty() {
            true => UiOp::Root { node: Some(after.clone()) },
            false => UiOp::Replace { id, node: after.clone() },
        });
        return;
    }
    let mut set = Map::new();
    let mut inner = Vec::new();
    for (key, value) in new.iter().filter(|(key, _)| *key != ID) {
        let Some(previous) = old.get(key) else {
            set.insert(key.clone(), value.clone());
            continue;
        };
        match (nodes(previous), nodes(value)) {
            _ if previous == value => {}
            _ if is_node(previous) && is_node(value) => diff_node(previous, value, &mut inner),
            (Some(old), Some(new)) => {
                let shared = old.len().min(new.len());
                for (before, after) in old.iter().zip(new) {
                    diff_node(before, after, &mut inner);
                }
                for (index, node) in new.iter().enumerate().skip(shared) {
                    let (parent, key, node) = (id.clone(), key.clone(), node.clone());
                    inner.push(UiOp::Insert { parent, key, index, node });
                }
                for node in old[shared..].iter().rev() {
                    inner.push(UiOp::Remove { id: node[ID].as_str().unwrap_or_default().to_string() });
                }
            }
            _ => {
                set.insert(key.clone(), value.clone());
            }
        }
    }
    let unset: Vec<String> = old.keys().filter(|key| !new.contains_key(*key)).cloned().collect();
    if !set.is_empty() || !unset.is_empty() {
        ops.push(UiOp::Props { id, set, unset });
    }
    ops.extend(inner);
}

fn is_node(value: &Value) -> bool {
    value.get(ID).is_some_and(Value::is_string) && value.get(COMPONENT).is_some()
}

/// A list of nodes, or an empty list, which may become one.
fn nodes(value: &Value) -> Option<&Vec<Value>> {
    value.as_array().filter(|items| items.iter().all(is_node))
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
