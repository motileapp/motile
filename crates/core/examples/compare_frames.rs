//! Compares two clients' frame dumps of the corpus node by node, for `scripts/compare-frames.sh`:
//!
//!     cargo run -p motile-core --example compare_frames -- <reference folder> <folder> [--mobile] [--known file]
//!
//! Each folder holds `<program>-<width>.json`, `{"<node id>": [x, y, width, height]}`, for every
//! golden tree of `fixtures/openui` at 390 and 720. What doesn't depend on text must agree within a
//! point: the gaps between siblings, a container's padding (where its first child starts), the
//! heights of controls, chips and lines, and the widths a column, a grid or a fixed length gives.
//! What text decides is held to the typefaces' differences: a text's width within 12%, or 35% with
//! figures in it, and 2 points (on the corpus's one-line texts DM Sans, GPUI's, sets words within
//! 11% of SF, the Mac's, 19 of 20 times, and its tabular figures within 23%, at worst 35%), and
//! its height within whole lines, since every line height is fixed: one line, and one more for
//! every twenty it has, which a container adds up from its texts. `--mobile`
//! takes the line heights enlarged by `Platform.scale`, as iOS and Android draw them. A line of
//! `--known` (`<program> <component> <why>`) lets the nodes of that component in that program differ,
//! with what lies around them, until the client that strays is fixed. Exits 1 when anything else
//! differs more than that.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde_json::Value;

const WIDTHS: [u32; 2] = [390, 720];
const SCALE: f32 = 1.14;
/// A text set in another typeface is this much wider or narrower at most, its figures more, and 2
/// points of rounding.
const TEXT_WIDTH: f32 = 0.12;
const FIGURES_WIDTH: f32 = 0.35;
const TEXT_WIDTH_SLACK: f32 = 2.0;
/// The components whose height is the control's, the chip's or the line's, whatever they say.
const FIXED_HEIGHT: &[&str] = &["Button", "IconButton", "Badge", "SegmentedControl", "Divider", "Icon", "Slider"];
/// The components that keep their own width in a column that stretches its children.
const OWN_WIDTH: &[&str] = &["Badge", "Button", "IconButton", "Icon", "Link", "SegmentedControl", "Svg"];

struct Node {
    component: String,
    props: Value,
    parent: Option<String>,
    /// The children by the prop that holds them, in order.
    groups: Vec<Vec<String>>,
}

#[derive(Clone, Copy, PartialEq)]
enum Flow {
    Column,
    Row,
    Grid,
    Other,
}

struct Comparison<'a> {
    nodes: &'a BTreeMap<String, Node>,
    reference: &'a HashMap<String, [f32; 4]>,
    other: &'a HashMap<String, [f32; 4]>,
    mobile: bool,
    known: Vec<&'a str>,
    allowances: HashMap<String, f32>,
}

#[derive(Default)]
struct Tally {
    checks: usize,
    within_text: usize,
    known: usize,
    drift: Vec<String>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mobile = args.iter().any(|arg| arg == "--mobile");
    let known_file = args.iter().position(|arg| arg == "--known").and_then(|at| args.get(at + 1));
    let folders: Vec<&String> = args.iter().filter(|arg| !arg.starts_with("--") && Some(*arg) != known_file).collect();
    let [reference, other] = folders.as_slice() else {
        eprintln!("usage: compare_frames <reference folder> <folder> [--mobile] [--known file]");
        std::process::exit(2);
    };
    let known_text = known_file.map(|file| std::fs::read_to_string(file).expect("the known drift")).unwrap_or_default();
    let known: Vec<(&str, &str)> = known_text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            Some((words.next()?, words.next()?))
        })
        .collect();
    let trees = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/openui");
    let mut programs: Vec<String> = std::fs::read_dir(&trees)
        .expect("the golden trees")
        .filter_map(|entry| entry.ok()?.path().file_stem()?.to_str().map(str::to_owned))
        .collect();
    programs.sort();
    let mut tally = Tally::default();
    for program in &programs {
        let tree: Value =
            serde_json::from_str(&std::fs::read_to_string(trees.join(format!("{program}.json"))).unwrap())
                .expect("a golden tree");
        let nodes = index(&tree);
        let known: Vec<&str> =
            known.iter().filter(|(name, _)| name == program).map(|(_, component)| *component).collect();
        for width in WIDTHS {
            let name = format!("{program}-{width}");
            let (Some(reference), Some(other)) = (dump(reference, &name), dump(other, &name)) else {
                tally.drift.push(format!("{name}: no dump"));
                continue;
            };
            let mut comparison = Comparison {
                nodes: &nodes,
                reference: &reference,
                other: &other,
                mobile,
                known: known.clone(),
                allowances: HashMap::new(),
            };
            comparison.compare(&name, &mut tally);
        }
    }
    println!(
        "{} checks of {} programs at {WIDTHS:?}: {} differ within what text allows, {} known, {} beyond",
        tally.checks,
        programs.len(),
        tally.within_text,
        tally.known,
        tally.drift.len()
    );
    for line in &tally.drift {
        println!("  {line}");
    }
    if !tally.drift.is_empty() {
        std::process::exit(1);
    }
}

fn dump(folder: &str, name: &str) -> Option<HashMap<String, [f32; 4]>> {
    let text = std::fs::read_to_string(Path::new(folder).join(format!("{name}.json"))).ok()?;
    let frames: HashMap<String, Vec<f32>> = serde_json::from_str(&text).ok()?;
    Some(
        frames
            .into_iter()
            .filter(|(_, frame)| frame.len() == 4)
            .map(|(id, frame)| (id, [frame[0], frame[1], frame[2], frame[3]]))
            .collect(),
    )
}

/// Every node the program shows in its row, by id: not a popover's panel, which shows over it.
fn index(tree: &Value) -> BTreeMap<String, Node> {
    fn walk(value: &Value, parent: Option<&str>, group: &mut Vec<String>, nodes: &mut BTreeMap<String, Node>) {
        match value {
            Value::Object(map) => {
                let (Some(id), Some(component)) =
                    (map.get("id").and_then(Value::as_str), map.get("component").and_then(Value::as_str))
                else {
                    return map.values().for_each(|value| walk(value, parent, group, nodes));
                };
                group.push(id.to_owned());
                let mut groups = Vec::new();
                for (key, value) in map {
                    if component == "Popover" && key == "content" || key == "a11y" {
                        continue;
                    }
                    let mut children = Vec::new();
                    walk(value, Some(id), &mut children, nodes);
                    if !children.is_empty() {
                        groups.push(children);
                    }
                }
                let mut props: serde_json::Map<String, Value> = map
                    .iter()
                    .filter(|(key, value)| !value.is_array() && !value.is_object() || *key == "sizing")
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                if let Some(rows) = map.get("rows").and_then(Value::as_array) {
                    props.insert("row_count".into(), rows.len().into());
                }
                let props = Value::Object(props);
                nodes.insert(
                    id.to_owned(),
                    Node { component: component.to_owned(), props, parent: parent.map(str::to_owned), groups },
                );
            }
            Value::Array(list) => list.iter().for_each(|value| walk(value, parent, group, nodes)),
            _ => {}
        }
    }
    let mut nodes = BTreeMap::new();
    walk(tree, None, &mut Vec::new(), &mut nodes);
    nodes
}

impl Comparison<'_> {
    fn compare(&mut self, name: &str, tally: &mut Tally) {
        for (id, node) in self.nodes {
            if node.component == "Celebration" {
                continue;
            }
            match (self.reference.contains_key(id), self.other.contains_key(id)) {
                (true, true) => {}
                (false, false) => continue,
                // A flashcard's back may not be laid out until it turns.
                _ if self.in_known(id) || id.contains("/back/") => {
                    tally.known += 1;
                    continue;
                }
                (shown, _) => {
                    let which = if shown { "the reference" } else { "this one" };
                    tally.drift.push(format!("{name} {} {id:?}: only one draws it ({which})", node.component));
                    continue;
                }
            }
            if node.component != "Spacer" {
                self.sizes(name, id, tally);
            }
            for group in &node.groups {
                self.spacing(name, id, node, group, tally);
            }
        }
    }

    /// Whether the node, or one around it, is of a component this program is known to draw apart.
    fn in_known(&self, id: &str) -> bool {
        let mut at = Some(id.to_owned());
        while let Some(id) = at {
            let Some(node) = self.nodes.get(&id) else { return false };
            if self.known.contains(&node.component.as_str()) {
                return true;
            }
            at = node.parent.clone();
        }
        false
    }

    fn sizes(&mut self, name: &str, id: &str, tally: &mut Tally) {
        let (a, b) = (self.reference[id], self.other[id]);
        let width_tolerance = match self.width_is_fixed(id) {
            true => 1.0,
            false => self.text_width(id, a[2].max(b[2])),
        };
        self.check(name, id, "width", a[2], b[2], width_tolerance, tally);
        let height_tolerance = self.height_allowance(id).max(1.0);
        self.check(name, id, "height", a[3], b[3], height_tolerance, tally);
    }

    /// A column that stretches its children, a grid's cell or a fixed length gives the node its
    /// width; text gives it in a row, a column that doesn't stretch, or a component's own parts.
    fn width_is_fixed(&self, id: &str) -> bool {
        let node = &self.nodes[id];
        if matches!(node.component.as_str(), "IconButton" | "Icon") {
            return true;
        }
        let Some(parent_id) = &node.parent else { return true };
        let parent = &self.nodes[parent_id];
        let given = match flow(parent) {
            Flow::Grid => true,
            Flow::Column => {
                let stretches = matches!(parent.props["align"].as_str(), None | Some("stretch"));
                let block = node.props["block"].as_bool() == Some(true);
                stretches && (block || !OWN_WIDTH.contains(&node.component.as_str()))
            }
            Flow::Row => {
                let index = parent.groups.iter().flatten().position(|child| child == id);
                let sizing = index.and_then(|index| parent.props["sizing"].get(index));
                sizing.is_some_and(|sizing| sizing["basis"] == "length" && sizing["shrink"] == false)
            }
            Flow::Other => false,
        };
        given && self.width_is_fixed(parent_id)
    }

    /// How much wider or narrower text set in another typeface may make a node `width` wide.
    fn text_width(&self, id: &str, width: f32) -> f32 {
        let share = if self.has_figures(id) { FIGURES_WIDTH } else { TEXT_WIDTH };
        TEXT_WIDTH_SLACK + share * width
    }

    /// Whether the node or one inside it sets tabular figures: titles, digits, readouts.
    fn has_figures(&self, id: &str) -> bool {
        let node = &self.nodes[id];
        let figures = node.props["digits"].as_bool() == Some(true)
            || matches!(node.component.as_str(), "Title" | "Timer" | "Slider" | "Badge")
            || node.props["style"]
                .as_str()
                .is_some_and(|style| style.starts_with("title") || style.starts_with("card_"));
        figures || node.groups.iter().flatten().any(|child| self.has_figures(child))
    }

    /// How much more or less tall text may make the node: nothing for a control or a picture, whole
    /// lines for a text, what its children may for a container.
    fn height_allowance(&mut self, id: &str) -> f32 {
        if let Some(allowance) = self.allowances.get(id) {
            return *allowance;
        }
        let node = &self.nodes[id];
        let shown = |frames: &HashMap<String, [f32; 4]>| frames.get(id).map_or(0.0, |frame| frame[3]);
        let allowance = if self.in_known(id) {
            (shown(self.reference) - shown(self.other)).abs()
        } else if FIXED_HEIGHT.contains(&node.component.as_str()) {
            0.0
        } else if let Some(line) = line_height(node, self.mobile)
            .filter(|_| node.groups.is_empty() || matches!(node.component.as_str(), "Text" | "Table"))
        {
            let lines = (shown(self.reference) / line).round().max(1.0);
            let parts: Vec<String> = node.groups.iter().flatten().cloned().collect();
            let parts: f32 = parts.iter().map(|part| self.height_allowance(part)).sum();
            // Each row of a table wraps where its columns' words end.
            let wrapped = match node.props["row_count"].as_u64() {
                Some(rows) => rows as f32 + 1.0,
                None => (lines / 20.0).ceil() + 1.0,
            };
            line * wrapped + parts
        } else if node.groups.is_empty() {
            0.0
        } else {
            let children: Vec<String> = node.groups.iter().flatten().cloned().collect();
            let allowances: Vec<f32> = children.iter().map(|child| self.height_allowance(child)).collect();
            match flow(node) {
                // Text decides where a row that wraps breaks: it may take one line more or fewer.
                Flow::Row if node.props["wrap"].as_bool() == Some(true) => {
                    let tallest = children
                        .iter()
                        .filter_map(|child| self.reference.get(child))
                        .map(|frame| frame[3])
                        .fold(0.0, f32::max);
                    allowances.into_iter().fold(0.0, f32::max)
                        + tallest
                        + node.props["gap"].as_f64().unwrap_or(0.0) as f32
                }
                Flow::Row => allowances.into_iter().fold(0.0, f32::max),
                _ => allowances.into_iter().sum(),
            }
        };
        self.allowances.insert(id.to_owned(), allowance);
        allowance
    }

    /// The gaps between siblings and where the first starts in its parent, which only the
    /// parent's own numbers decide.
    fn spacing(&mut self, name: &str, id: &str, node: &Node, group: &[String], tally: &mut Tally) {
        let flow = flow(node);
        if flow == Flow::Other {
            return;
        }
        let shown: Vec<&String> = group
            .iter()
            .filter(|child| self.reference.contains_key(*child) && self.other.contains_key(*child))
            .filter(|child| !matches!(self.nodes[*child].component.as_str(), "Celebration"))
            .collect();
        let justify = node.props["justify"].as_str().unwrap_or("start");
        let align = node.props["align"].as_str();
        for pair in shown.windows(2) {
            let (first, next) = (pair[0], pair[1]);
            if [first, next].iter().any(|child| self.nodes[*child].component == "Spacer" || self.in_known(child)) {
                continue;
            }
            let (a1, a2, b1, b2) = (self.reference[first], self.reference[next], self.other[first], self.other[next]);
            let below = |one: [f32; 4], two: [f32; 4]| two[1] >= one[1] + one[3] - 0.5;
            let beside = |one: [f32; 4], two: [f32; 4]| two[0] >= one[0] + one[2] - 0.5;
            if below(a1, a2) && below(b1, b2) {
                let gap = |one: [f32; 4], two: [f32; 4]| two[1] - one[1] - one[3];
                let tolerance = if flow == Flow::Grid { self.height_allowance(first).max(1.0) } else { 1.0 };
                self.check(name, next, "gap above", gap(a1, a2), gap(b1, b2), tolerance, tally);
            } else if beside(a1, a2) && beside(b1, b2) {
                let gap = |one: [f32; 4], two: [f32; 4]| two[0] - one[0] - one[2];
                let spread = flow == Flow::Row && justify != "start";
                let tolerance = if spread { self.text_width(id, node_width(self, id)) } else { 1.0 };
                self.check(name, next, "gap before", gap(a1, a2), gap(b1, b2), tolerance, tally);
            } else if node.props["wrap"].as_bool() != Some(true) && flow != Flow::Grid {
                tally.checks += 1;
                tally.drift.push(format!(
                    "{name} {} {next:?}: placed below its sibling in one, beside it in the other",
                    self.nodes[next].component
                ));
            }
        }
        let Some(first) = shown.first() else { return };
        let (a, b, ac, bc) = (self.reference[id], self.other[id], self.reference[*first], self.other[*first]);
        let top_given = match flow {
            Flow::Row => matches!(align, Some("start") | Some("stretch")),
            _ => !matches!(justify, "center" | "end"),
        };
        let top = if top_given { 1.0 } else { self.height_allowance(id).max(1.0) };
        self.check(name, first, "top inset", ac[1] - a[1], bc[1] - b[1], top, tally);
        let left_given = match flow {
            Flow::Column => !matches!(align, Some("center") | Some("end")) || self.width_is_fixed(first),
            _ => justify == "start",
        };
        let left = if left_given { 1.0 } else { self.text_width(id, a[2].max(b[2])) };
        self.check(name, first, "left inset", ac[0] - a[0], bc[0] - b[0], left, tally);
    }

    #[allow(clippy::too_many_arguments)]
    fn check(&self, name: &str, id: &str, what: &str, reference: f32, other: f32, tolerance: f32, tally: &mut Tally) {
        tally.checks += 1;
        let differs = (reference - other).abs();
        if differs <= 1.0 {
            return;
        }
        if self.in_known(id) {
            tally.known += 1;
        } else if differs <= tolerance {
            tally.within_text += 1;
        } else {
            let component = &self.nodes[id].component;
            tally.drift.push(format!(
                "{name} {component} {id:?} {what}: {reference:.1} against {other:.1}, {tolerance:.1} allowed"
            ));
        }
    }
}

fn node_width(comparison: &Comparison, id: &str) -> f32 {
    comparison.reference[id][2].max(comparison.other[id][2])
}

fn flow(node: &Node) -> Flow {
    match node.component.as_str() {
        "Row" => Flow::Row,
        "Box" if node.props["direction"] == "row" => Flow::Row,
        "Grid" => Flow::Grid,
        "Col" | "Card" | "Box" | "Form" | "Pressable" => Flow::Column,
        _ => Flow::Other,
    }
}

/// The height of a line of the node's text, as `LAYOUT.md`'s type table has it.
fn line_height(node: &Node, mobile: bool) -> Option<f32> {
    let line: f32 = match node.props["style"].as_str() {
        Some("body") | Some("title_lg") => 24.0,
        Some("body_card") | Some("title_md") | Some("card_heading") => 22.0,
        Some("large") | Some("title_xl") => 26.0,
        Some("small") | Some("title_sm") | Some("card_title") => 20.0,
        Some("caption") | Some("label") | Some("card_subtitle") | Some("readout") | Some("code") => 18.0,
        Some("title2xl") => 30.0,
        Some("title3xl") | Some("timer") => 36.0,
        Some("tick") => 14.0,
        Some(_) => 24.0,
        None => match node.component.as_str() {
            "Markdown" | "Quote" | "List" | "ListItem" | "Link" | "Checkbox" | "RadioGroup" | "Input" | "TextArea"
            | "Select" | "DatePicker" | "Table" | "Code" | "Chart" | "Timer" | "Flashcard" | "Popover" => 24.0,
            _ => return None,
        },
    };
    Some(if mobile { (line * SCALE).round() } else { line })
}
