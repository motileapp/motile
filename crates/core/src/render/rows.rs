//! A thread's transcript as the rows an app draws: one per user message, stretch of prose, code
//! block, tool call and so on. The apps keep a list of these and apply the splices sent to them.
//!
//! The work of a turn takes little room: tool calls that follow one another are one row that
//! opens into them, and once a turn has ended, everything before its last message folds behind
//! a single row.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use motile_protocol::wire::{Denial, Item, ItemKind, ToolCall, ToolStatus};
use serde::Serialize;
use serde_json::Value;

use super::highlight::{self, Incremental, Spans};
use super::markdown::{self, Block, Prose};

/// Tool output beyond this is cut; nobody reads more of it in a chat.
const MAX_OUTPUT_CHARS: usize = 20_000;

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Row {
    /// Stable while the row's content grows, so the app can update it in place.
    pub id: String,
    pub item: String,
    /// The row belongs to the group above it, which is open.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub nested: bool,
    #[serde(flatten)]
    pub kind: RowKind,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RowKind {
    User {
        text: String,
        attachments: Vec<String>,
        at: f64,
    },
    Prose {
        #[serde(flatten)]
        prose: Prose,
    },
    Code {
        language: String,
        code: String,
        /// `None` until the code has been highlighted; the app asks for it when the row is seen.
        spans: Option<Spans>,
    },
    Tool {
        #[serde(flatten)]
        tool: Tool,
    },
    Thinking {
        text: String,
    },
    /// Tool calls that followed one another, as one row.
    Group {
        /// What they did, "Read 3 files and ran 2 commands", or while the turn runs, what the
        /// latest one is doing.
        title: String,
        /// What the latest one acts on, while the turn runs.
        target: String,
        icon: &'static str,
        running: bool,
        failed: bool,
        open: bool,
    },
    /// Stands for everything a finished turn did before its last message.
    Fold {
        duration_ms: Option<u64>,
        stopped: bool,
        open: bool,
    },
    Error {
        message: String,
    },
    TurnEnd {
        duration_ms: Option<u64>,
        cost_usd: Option<f64>,
        is_error: bool,
        stopped: bool,
        denials: Vec<Denial>,
        /// The turn's fold says how long it took, so this row doesn't.
        folded: bool,
    },
}

/// A tool call, worded for a person.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Tool {
    pub name: String,
    pub icon: &'static str,
    /// What it does or did: "Ran", "Reading".
    pub verb: String,
    /// What it acts on: the command, the file, the search.
    pub target: String,
    pub status: ToolStatus,
    /// The input in full, shown when the row is opened.
    pub input: String,
    pub input_language: String,
    pub output: Option<String>,
}

pub struct Splice {
    pub start: usize,
    pub remove: usize,
    pub rows: Vec<Row>,
}

#[derive(Default)]
pub struct Transcript {
    cwd: String,
    items: Vec<Item>,
    /// The rows of each item on its own, in step with `items`.
    rendered: Vec<Vec<Row>>,
    /// The rows as they are shown: tool calls grouped and finished turns folded.
    rows: Vec<Row>,
    /// The groups and folds that are open, by row id.
    opened: HashSet<String>,
    /// Highlighters for the code blocks of items that are still streaming, by row id.
    streaming: HashMap<String, Incremental>,
}

impl Transcript {
    pub fn new(cwd: &str) -> Self {
        Self { cwd: cwd.to_string(), ..Self::default() }
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn item(&self, id: &str) -> Option<&Item> {
        self.items.iter().rfind(|item| item.id == id)
    }

    pub fn clear(&mut self) {
        *self = Self::new(&self.cwd);
    }

    /// Fills an empty transcript with stored items, in order.
    pub fn load(&mut self, items: Vec<Item>) {
        self.rendered = items.iter().map(|item| render(item, &self.cwd, None)).collect();
        self.items = items;
        self.show();
    }

    /// Puts earlier items in front of the ones already loaded.
    pub fn prepend(&mut self, items: Vec<Item>) -> Option<Splice> {
        let rendered: Vec<Vec<Row>> = items.iter().map(|item| render(item, &self.cwd, None)).collect();
        self.rendered.splice(0..0, rendered);
        self.items.splice(0..0, items);
        self.show()
    }

    /// Adds the item or replaces the one with its id. `live` items are highlighted as they grow.
    pub fn upsert(&mut self, item: Item, live: bool) -> Option<Splice> {
        let index = match self.items.iter().rposition(|existing| existing.id == item.id) {
            Some(index) => {
                self.items[index] = item;
                index
            }
            None => {
                let index = self.items.partition_point(|existing| existing.seq <= item.seq);
                self.items.insert(index, item);
                self.rendered.insert(index, Vec::new());
                index
            }
        };
        self.rerender(index, live)
    }

    /// Appends streamed text to an assistant item. The rows are rendered by `refresh`.
    pub fn append_text(&mut self, id: &str, text: &str, rev: u64) -> bool {
        let Some(item) = self.items.iter_mut().rfind(|item| item.id == id) else { return false };
        let ItemKind::Assistant { text: current } = &mut item.kind else { return false };
        current.push_str(text);
        item.rev = rev;
        true
    }

    /// Renders the item again after its text grew.
    pub fn refresh(&mut self, id: &str) -> Option<Splice> {
        let index = self.items.iter().rposition(|item| item.id == id)?;
        self.rerender(index, true)
    }

    /// Opens or closes a group or a fold.
    pub fn toggle(&mut self, row_id: &str) -> Option<Splice> {
        if !self.opened.remove(row_id) {
            self.opened.insert(row_id.to_string());
        }
        self.show()
    }

    fn rerender(&mut self, index: usize, live: bool) -> Option<Splice> {
        let streaming = live.then_some(&mut self.streaming);
        self.rendered[index] = render(&self.items[index], &self.cwd, streaming);
        self.show()
    }

    /// Works out the rows to show and returns how they differ from the ones shown before.
    fn show(&mut self) -> Option<Splice> {
        let shown = present(&self.items, &self.rendered, &self.opened);
        let same_start = self.rows.iter().zip(&shown).take_while(|(before, after)| *before == after.as_ref()).count();
        if same_start == self.rows.len() && same_start == shown.len() {
            return None;
        }
        let rest = (self.rows.len() - same_start).min(shown.len() - same_start);
        let old_end = self.rows.iter().rev().take(rest);
        let same_end = old_end.zip(shown.iter().rev()).take_while(|(before, after)| *before == after.as_ref()).count();

        let changed = shown[same_start..shown.len() - same_end].iter().map(|row| row.as_ref().clone());
        let splice =
            Splice { start: same_start, remove: self.rows.len() - same_start - same_end, rows: changed.collect() };
        self.rows.splice(same_start..same_start + splice.remove, splice.rows.iter().cloned());
        Some(splice)
    }

    /// Stops keeping highlighters for a finished turn's code blocks.
    pub fn end_streaming(&mut self) {
        self.streaming.clear();
    }

    /// The code blocks among `row_ids` that have no highlighting yet.
    pub fn unhighlighted(&self, row_ids: &[String]) -> Vec<(String, String, String)> {
        let wanted = self.rows.iter().filter(|row| row_ids.contains(&row.id));
        let code = wanted.filter_map(|row| match &row.kind {
            RowKind::Code { language, code, spans: None } => Some((row.id.clone(), language.clone(), code.clone())),
            _ => None,
        });
        code.collect()
    }

    /// Stores highlighting that was computed elsewhere. `false` if the code changed meanwhile.
    pub fn set_spans(&mut self, row_id: &str, code: &str, spans: Spans) -> bool {
        let rendered = self.rendered.iter_mut().flatten();
        let mut stored = false;
        for row in rendered.chain(&mut self.rows).filter(|row| row.id == row_id) {
            let RowKind::Code { code: current, spans: slot, .. } = &mut row.kind else { continue };
            if current != code {
                continue;
            }
            *slot = Some(spans.clone());
            stored = true;
        }
        stored
    }
}

/// The rows to show for the items: each turn's, one turn after the other.
fn present<'a>(items: &'a [Item], rendered: &'a [Vec<Row>], opened: &HashSet<String>) -> Vec<Cow<'a, Row>> {
    let mut shown = Vec::new();
    let mut start = 0;
    while start < items.len() {
        let next_message = (start + 1..items.len()).find(|&index| matches!(items[index].kind, ItemKind::User { .. }));
        let end = next_message.unwrap_or(items.len());
        present_turn(&items[start..end], &rendered[start..end], opened, &mut shown);
        start = end;
    }
    shown
}

/// A turn is the user's message and what the agent did until the next one.
fn present_turn<'a>(
    items: &'a [Item],
    rendered: &'a [Vec<Row>],
    opened: &HashSet<String>,
    shown: &mut Vec<Cow<'a, Row>>,
) {
    let work_start = usize::from(matches!(items[0].kind, ItemKind::User { .. }));
    shown.extend(rendered[..work_start].iter().flatten().map(Cow::Borrowed));

    let ended = items.iter().rev().find_map(|item| match &item.kind {
        ItemKind::TurnEnd { summary } => Some(summary),
        _ => None,
    });
    let failed = items.iter().any(|item| matches!(item.kind, ItemKind::Error { .. }));
    let answer = items.iter().rposition(|item| matches!(item.kind, ItemKind::Assistant { .. }));
    // Once the turn has ended well, what led up to its last message folds away.
    let fold = match (ended, answer) {
        (Some(summary), Some(answer)) if !failed && answer > work_start => Some((summary, answer)),
        _ => None,
    };

    let mut index = work_start;
    if let Some((summary, answer)) = fold {
        let id = format!("{}/fold", items[work_start].id);
        let open = opened.contains(&id);
        let kind = RowKind::Fold { duration_ms: summary.duration_ms, stopped: summary.stopped, open };
        shown.push(Cow::Owned(Row { id, item: items[work_start].id.clone(), nested: false, kind }));
        if !open {
            index = answer;
        }
    }

    while index < items.len() {
        if !is_work(&items[index]) {
            shown.extend(rendered[index].iter().map(|row| match &row.kind {
                RowKind::TurnEnd { .. } if fold.is_some() => Cow::Owned(folded(row)),
                _ => Cow::Borrowed(row),
            }));
            index += 1;
            continue;
        }
        let run_end = (index..items.len()).find(|&next| !is_work(&items[next])).unwrap_or(items.len());
        let run = &rendered[index..run_end];
        index = run_end;
        if run.len() < 2 {
            shown.extend(run.iter().flatten().map(Cow::Borrowed));
            continue;
        }
        let Some(first) = run[0].first() else { continue };
        let id = format!("{}/group", first.item);
        let open = opened.contains(&id);
        // The calls a running turn is making now are named; the ones behind it are summed up.
        let live = ended.is_none() && run_end == items.len();
        shown.push(Cow::Owned(Row { id, item: first.item.clone(), nested: false, kind: group(run, live, open) }));
        if open {
            shown.extend(run.iter().flatten().map(|row| Cow::Owned(Row { nested: true, ..row.clone() })));
        }
    }
}

fn is_work(item: &Item) -> bool {
    matches!(item.kind, ItemKind::Tool { .. } | ItemKind::Thinking { .. })
}

fn folded(row: &Row) -> Row {
    let mut row = row.clone();
    if let RowKind::TurnEnd { folded, .. } = &mut row.kind {
        *folded = true;
    }
    row
}

fn group(run: &[Vec<Row>], live: bool, open: bool) -> RowKind {
    let tools: Vec<&Tool> = run
        .iter()
        .flatten()
        .filter_map(|row| match &row.kind {
            RowKind::Tool { tool } => Some(tool),
            _ => None,
        })
        .collect();
    let running = tools.iter().any(|tool| tool.status == ToolStatus::Running);
    let failed = tools.iter().any(|tool| tool.status == ToolStatus::Failed);
    let one_icon = tools.first().map(|tool| tool.icon).filter(|icon| tools.iter().all(|tool| tool.icon == *icon));
    let (title, target, icon) = match (live, run.last().and_then(|rows| rows.first()).map(|row| &row.kind)) {
        (true, Some(RowKind::Tool { tool })) => (tool.verb.clone(), tool.target.clone(), tool.icon),
        _ => (summary(&tools), String::new(), one_icon.unwrap_or("tool")),
    };
    RowKind::Group { title, target, icon, running, failed, open }
}

/// What the calls did, by kind and in the order the kinds first appear: "Read 3 files, ran 2
/// commands and changed 1 file".
fn summary(tools: &[&Tool]) -> String {
    let mut kinds: Vec<&str> = Vec::new();
    for tool in tools {
        if !kinds.contains(&tool.icon) {
            kinds.push(tool.icon);
        }
    }
    let parts: Vec<String> = kinds
        .iter()
        .map(|&kind| {
            let of_kind = tools.iter().filter(|tool| tool.icon == kind);
            let count = match kind {
                // A file edited three times is one file changed.
                "edit" => of_kind.map(|tool| tool.target.as_str()).collect::<HashSet<_>>().len(),
                _ => of_kind.count(),
            };
            counted(kind, count)
        })
        .collect();
    let Some((last, others)) = parts.split_last() else { return "Thought".to_string() };
    if others.is_empty() {
        return last.clone();
    }
    let lower = |part: &String| {
        let mut letters = part.chars();
        letters.next().map(|first| first.to_lowercase().chain(letters).collect::<String>()).unwrap_or_default()
    };
    let rest: Vec<String> = others.iter().skip(1).map(lower).collect();
    let listed = std::iter::once(others[0].clone()).chain(rest).collect::<Vec<_>>().join(", ");
    format!("{listed} and {}", lower(last))
}

fn counted(kind: &str, count: usize) -> String {
    let of = |one: &str, many: &str| format!("{count} {}", if count == 1 { one } else { many });
    match kind {
        "terminal" => format!("Ran {}", of("command", "commands")),
        "file" => format!("Read {}", of("file", "files")),
        "edit" => format!("Changed {}", of("file", "files")),
        "search" => format!("Searched {}", of("time", "times")),
        "web" => format!("Used the web {}", of("time", "times")),
        "agent" => format!("Ran {}", of("agent", "agents")),
        "todo" => "Updated the plan".to_string(),
        _ => format!("Used {}", of("tool", "tools")),
    }
}

fn render(item: &Item, cwd: &str, streaming: Option<&mut HashMap<String, Incremental>>) -> Vec<Row> {
    let row = |index: usize, kind: RowKind| Row {
        id: format!("{}/{index}", item.id),
        item: item.id.clone(),
        nested: false,
        kind,
    };
    match &item.kind {
        ItemKind::User { text, attachments } => {
            let attachments = attachments.iter().map(|path| file_name(path).to_string()).collect();
            vec![row(0, RowKind::User { text: text.clone(), attachments, at: item.created_at })]
        }
        ItemKind::Assistant { text } => render_markdown(item, text, streaming),
        ItemKind::Thinking { text } => vec![row(0, RowKind::Thinking { text: text.clone() })],
        ItemKind::Tool { call } => vec![row(0, RowKind::Tool { tool: describe(call, cwd) })],
        ItemKind::Error { message } => vec![row(0, RowKind::Error { message: message.clone() })],
        ItemKind::TurnEnd { summary } => vec![row(
            0,
            RowKind::TurnEnd {
                duration_ms: summary.duration_ms,
                cost_usd: summary.cost_usd,
                is_error: summary.is_error,
                stopped: summary.stopped,
                denials: summary.denials.clone(),
                folded: false,
            },
        )],
    }
}

fn render_markdown(item: &Item, text: &str, mut streaming: Option<&mut HashMap<String, Incremental>>) -> Vec<Row> {
    let blocks = markdown::parse(text);
    let mut rows = Vec::with_capacity(blocks.len());
    for (index, block) in blocks.into_iter().enumerate() {
        let id = format!("{}/{index}", item.id);
        let kind = match block {
            Block::Prose(prose) => RowKind::Prose { prose },
            Block::Code { language, code } => {
                let spans = match &mut streaming {
                    Some(streaming) => {
                        let highlighter = streaming.entry(id.clone()).or_insert_with(|| Incremental::new(&language));
                        Some(highlighter.advance(&code))
                    }
                    None => highlight::cached(&language, &code),
                };
                RowKind::Code { language, code, spans }
            }
        };
        rows.push(Row { id, item: item.id.clone(), nested: false, kind });
    }
    rows
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The path as the user thinks of it: relative to the thread's folder when inside it.
fn short_path(path: &str, cwd: &str) -> String {
    match path.strip_prefix(cwd).and_then(|rest| rest.strip_prefix('/')) {
        Some(relative) if !cwd.is_empty() => relative.to_string(),
        _ => path.to_string(),
    }
}

fn language_of(path: &str) -> String {
    let name = file_name(path);
    match name.rsplit_once('.') {
        Some((_, extension)) => extension.to_lowercase(),
        None => name.to_lowercase(),
    }
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    if line.len() == text.trim().len() { line.to_string() } else { format!("{line} …") }
}

fn cap(text: &str) -> String {
    if text.chars().count() <= MAX_OUTPUT_CHARS {
        return text.to_string();
    }
    let kept: String = text.chars().take(MAX_OUTPUT_CHARS).collect();
    format!("{kept}\n… the rest was cut")
}

fn diff(old: &str, new: &str) -> String {
    let removed = old.lines().map(|line| format!("-{line}"));
    let added = new.lines().map(|line| format!("+{line}"));
    removed.chain(added).collect::<Vec<_>>().join("\n")
}

fn describe(call: &ToolCall, cwd: &str) -> Tool {
    let input: Value = serde_json::from_str(&call.input).unwrap_or_default();
    let text = |key: &str| input[key].as_str().unwrap_or_default().to_string();
    let running = call.status == ToolStatus::Running;
    let verb = |doing: &str, did: &str| if running { doing.to_string() } else { did.to_string() };
    let path = short_path(&text("file_path"), cwd);

    let (icon, verb, target, shown_input, input_language) = match call.name.as_str() {
        "Bash" => {
            ("terminal", verb("Running", "Ran"), first_line(&text("command")), text("command"), "bash".to_string())
        }
        "Read" => ("file", verb("Reading", "Read"), path, String::new(), String::new()),
        "Write" => ("edit", verb("Writing", "Wrote"), path, text("content"), language_of(&text("file_path"))),
        "Edit" | "MultiEdit" | "NotebookEdit" => {
            let changes = match input["edits"].as_array() {
                Some(edits) => edits
                    .iter()
                    .map(|edit| {
                        diff(
                            edit["old_string"].as_str().unwrap_or_default(),
                            edit["new_string"].as_str().unwrap_or_default(),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                None => diff(&text("old_string"), &text("new_string")),
            };
            ("edit", verb("Editing", "Edited"), path, changes, "diff".to_string())
        }
        "Grep" | "Glob" => {
            ("search", verb("Searching for", "Searched for"), text("pattern"), String::new(), String::new())
        }
        "WebSearch" => {
            ("web", verb("Searching the web for", "Searched the web for"), text("query"), String::new(), String::new())
        }
        "WebFetch" => ("web", verb("Fetching", "Fetched"), text("url"), text("prompt"), String::new()),
        "Task" | "Agent" => {
            ("agent", verb("Running an agent:", "Ran an agent:"), text("description"), text("prompt"), String::new())
        }
        "TodoWrite" => {
            let todos = input["todos"].as_array().map(Vec::as_slice).unwrap_or_default();
            let lines: Vec<String> = todos
                .iter()
                .map(|todo| {
                    let mark = match todo["status"].as_str() {
                        Some("completed") => "☑",
                        Some("in_progress") => "◐",
                        _ => "☐",
                    };
                    format!("{mark} {}", todo["content"].as_str().unwrap_or_default())
                })
                .collect();
            ("todo", verb("Updating", "Updated"), "the plan".to_string(), lines.join("\n"), String::new())
        }
        name => {
            let (verb_text, target) = match name.strip_prefix("mcp__").and_then(|rest| rest.split_once("__")) {
                Some((server, tool)) => (verb("Using", "Used"), format!("{server}: {tool}")),
                None => (verb("Using", "Used"), name.to_string()),
            };
            let pretty = serde_json::to_string_pretty(&input).unwrap_or_default();
            let shown = if pretty == "{}" { String::new() } else { pretty };
            ("tool", verb_text, target, shown, "json".to_string())
        }
    };

    Tool {
        name: call.name.clone(),
        icon,
        verb,
        target,
        status: call.status,
        input: cap(&shown_input),
        input_language,
        output: call.output.as_deref().filter(|output| !output.trim().is_empty()).map(cap),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, seq: u64, kind: ItemKind) -> Item {
        Item { id: id.to_string(), seq, rev: seq + 1, created_at: 0.0, kind }
    }

    fn assistant(id: &str, seq: u64, text: &str) -> Item {
        item(id, seq, ItemKind::Assistant { text: text.to_string() })
    }

    fn tool(name: &str, input: Value, status: ToolStatus) -> Tool {
        let call = ToolCall { id: "t".into(), name: name.into(), input: input.to_string(), output: None, status };
        describe(&call, "/srv/api")
    }

    fn apply(rows: &mut Vec<Row>, splice: Splice) {
        rows.splice(splice.start..splice.start + splice.remove, splice.rows);
    }

    #[test]
    fn streamed_text_only_replaces_the_rows_that_changed() {
        let mut transcript = Transcript::new("/srv/api");
        let mut shown: Vec<Row> = Vec::new();
        let user = item("u", 0, ItemKind::User { text: "Hi".into(), attachments: vec!["/tmp/a/notes.txt".into()] });
        apply(&mut shown, transcript.upsert(user, true).unwrap());
        apply(
            &mut shown,
            transcript.upsert(assistant("a", 1, "Intro\n\n```rust\nfn a() {}\n```\n\nOut"), true).unwrap(),
        );
        assert_eq!(shown.len(), 4);

        assert!(transcript.append_text("a", "ro", 9));
        let splice = transcript.refresh("a").unwrap();
        assert_eq!((splice.start, splice.remove, splice.rows.len()), (3, 1, 1), "only the last row is replaced");
        apply(&mut shown, splice);

        assert_eq!(shown, transcript.rows());
        assert!(matches!(&shown[0].kind, RowKind::User { attachments, .. } if attachments == &["notes.txt"]));
        assert!(matches!(&shown[3].kind, RowKind::Prose { prose } if prose.text == "Outro"));
        assert_eq!(shown[3].id, "a/2");
        assert!(transcript.refresh("a").is_none(), "nothing changed, nothing to send");
    }

    #[test]
    fn streaming_code_is_highlighted_as_it_grows_and_stored_code_when_asked() {
        let mut live = Transcript::new("");
        live.upsert(assistant("a", 0, "```rust\nfn a() {}\n"), true);
        assert!(matches!(&live.rows()[0].kind, RowKind::Code { spans: Some(spans), .. } if !spans.is_empty()));

        let mut stored = Transcript::new("");
        stored.load(vec![assistant("b", 0, "```rust\nfn never_seen_before() {}\n```")]);
        assert!(matches!(&stored.rows()[0].kind, RowKind::Code { spans: None, .. }));
        let wanted = stored.unhighlighted(&["b/0".to_string()]);
        assert_eq!(wanted.len(), 1);
        let (row_id, language, code) = &wanted[0];
        assert!(stored.set_spans(row_id, code, highlight::highlight(language, code)));
        assert!(stored.unhighlighted(&["b/0".to_string()]).is_empty());
        assert!(!stored.set_spans(row_id, "something else", Spans::default()));
    }

    #[test]
    fn an_item_that_arrives_late_goes_where_its_position_says() {
        let mut transcript = Transcript::new("");
        transcript.upsert(assistant("c", 2, "third"), false);
        transcript.upsert(assistant("a", 0, "first"), false);
        let splice = transcript.upsert(assistant("b", 1, "second"), false).unwrap();

        assert_eq!(splice.start, 1);
        let ids: Vec<&str> = transcript.rows().iter().map(|row| row.item.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn rows_are_flat_json_objects_tagged_by_kind() {
        let mut transcript = Transcript::new("");
        transcript.upsert(assistant("a", 0, "Hi **you**\n\n```sh\nls\n```"), false);
        let rows = serde_json::to_value(transcript.rows()).unwrap();

        assert_eq!(
            rows,
            serde_json::json!([
                {
                    "id": "a/0", "item": "a", "kind": "prose", "text": "Hi you",
                    "runs": [[3, 3, 1]], "links": [],
                    "paras": [{"start": 0, "len": 6, "kind": "body"}],
                },
                {"id": "a/1", "item": "a", "kind": "code", "language": "sh", "code": "ls", "spans": null},
            ])
        );
    }

    fn call(id: &str, seq: u64, name: &str, input: Value, status: ToolStatus) -> Item {
        let call = ToolCall { id: id.into(), name: name.into(), input: input.to_string(), output: None, status };
        item(id, seq, ItemKind::Tool { call })
    }

    /// The rows in short: their kind, and for groups and folds what they say.
    fn outline(transcript: &Transcript) -> Vec<String> {
        let rows = transcript.rows().iter().map(|row| match &row.kind {
            RowKind::User { .. } => "user".to_string(),
            RowKind::Prose { prose } => prose.text.clone(),
            RowKind::Tool { tool } if row.nested => format!("  {} {}", tool.verb, tool.target),
            RowKind::Tool { tool } => format!("{} {}", tool.verb, tool.target),
            RowKind::Group { title, target, open, .. } => {
                format!("[{}]{}", format!("{title} {target}").trim(), if *open { " open" } else { "" })
            }
            RowKind::Fold { open, .. } => format!("fold{}", if *open { " open" } else { "" }),
            RowKind::TurnEnd { folded, .. } => format!("end{}", if *folded { " folded" } else { "" }),
            _ => "other".to_string(),
        });
        rows.collect()
    }

    fn working_turn() -> Transcript {
        let done = ToolStatus::Succeeded;
        let mut transcript = Transcript::new("/srv/api");
        transcript.load(vec![
            item("u", 0, ItemKind::User { text: "Fix it".into(), attachments: Vec::new() }),
            assistant("a1", 1, "Looking."),
            call("t1", 2, "Read", serde_json::json!({"file_path": "/srv/api/a.rs"}), done),
            call("t2", 3, "Read", serde_json::json!({"file_path": "/srv/api/b.rs"}), done),
            call("t3", 4, "Edit", serde_json::json!({"file_path": "/srv/api/a.rs"}), done),
            assistant("a2", 5, "Testing."),
            call("t4", 6, "Bash", serde_json::json!({"command": "cargo check"}), done),
            call("t5", 7, "Bash", serde_json::json!({"command": "cargo test"}), ToolStatus::Running),
        ]);
        transcript
    }

    #[test]
    fn tool_calls_in_a_row_are_one_row_that_opens() {
        let mut transcript = working_turn();
        // The calls being made now are named; the ones behind them are summed up.
        assert_eq!(
            outline(&transcript),
            ["user", "Looking.", "[Read 2 files and changed 1 file]", "Testing.", "[Running cargo test]"]
        );

        let mut shown = transcript.rows().to_vec();
        let splice = transcript.toggle("t1/group").unwrap();
        assert_eq!((splice.start, splice.remove, splice.rows.len()), (2, 1, 4));
        apply(&mut shown, splice);
        assert_eq!(shown, transcript.rows());
        assert_eq!(
            outline(&transcript)[2..6],
            ["[Read 2 files and changed 1 file] open", "  Read a.rs", "  Read b.rs", "  Edited a.rs"]
        );

        apply(&mut shown, transcript.toggle("t1/group").unwrap());
        assert_eq!(shown, transcript.rows());
        assert_eq!(shown.len(), 5);
    }

    #[test]
    fn a_finished_turn_folds_what_led_to_its_last_message() {
        let mut transcript = working_turn();
        let done = serde_json::json!({"command": "cargo test"});
        transcript.upsert(call("t5", 7, "Bash", done, ToolStatus::Succeeded), true);
        transcript.upsert(assistant("a3", 8, "Fixed."), true);
        assert_eq!(
            outline(&transcript)[3..],
            ["Testing.", "[Ran 2 commands]", "Fixed."],
            "nothing folds while it runs"
        );

        let summary = motile_protocol::wire::TurnSummary { duration_ms: Some(42_000), ..Default::default() };
        transcript.upsert(item("e", 9, ItemKind::TurnEnd { summary }), true);
        assert_eq!(outline(&transcript), ["user", "fold", "Fixed.", "end folded"]);

        transcript.toggle("a1/fold");
        assert_eq!(
            outline(&transcript),
            [
                "user",
                "fold open",
                "Looking.",
                "[Read 2 files and changed 1 file]",
                "Testing.",
                "[Ran 2 commands]",
                "Fixed.",
                "end folded"
            ]
        );

        // The next turn is its own: it neither joins the fold nor is folded while it runs.
        transcript.toggle("a1/fold");
        transcript.upsert(item("u2", 10, ItemKind::User { text: "Thanks".into(), attachments: Vec::new() }), true);
        transcript.upsert(assistant("b1", 11, "Welcome."), true);
        assert_eq!(outline(&transcript), ["user", "fold", "Fixed.", "end folded", "user", "Welcome."]);
    }

    #[test]
    fn a_turn_with_one_message_or_an_error_is_shown_as_it_is() {
        let summary = motile_protocol::wire::TurnSummary::default();
        let mut short = Transcript::new("");
        short.load(vec![
            item("u", 0, ItemKind::User { text: "Hi".into(), attachments: Vec::new() }),
            assistant("a", 1, "Hello."),
            item("e", 2, ItemKind::TurnEnd { summary: summary.clone() }),
        ]);
        assert_eq!(outline(&short), ["user", "Hello.", "end"]);

        let mut failed = Transcript::new("");
        failed.load(vec![
            item("u", 0, ItemKind::User { text: "Hi".into(), attachments: Vec::new() }),
            call("t", 1, "Bash", serde_json::json!({"command": "ls"}), ToolStatus::Succeeded),
            assistant("a", 2, "Listing."),
            item("x", 3, ItemKind::Error { message: "The agent stopped.".into() }),
            item("e", 4, ItemKind::TurnEnd { summary }),
        ]);
        assert_eq!(outline(&failed), ["user", "Ran ls", "Listing.", "other", "end"]);
    }

    #[test]
    fn tool_calls_are_worded_for_a_person() {
        let running = tool("Bash", serde_json::json!({"command": "cargo test\ncargo clippy"}), ToolStatus::Running);
        assert_eq!((running.verb.as_str(), running.target.as_str()), ("Running", "cargo test …"));
        assert_eq!((running.input.as_str(), running.input_language.as_str()), ("cargo test\ncargo clippy", "bash"));

        let read = tool("Read", serde_json::json!({"file_path": "/srv/api/src/main.rs"}), ToolStatus::Succeeded);
        assert_eq!((read.verb.as_str(), read.target.as_str(), read.icon), ("Read", "src/main.rs", "file"));

        let edit = serde_json::json!({"file_path": "/etc/hosts", "old_string": "a\nb", "new_string": "c"});
        let edited = tool("Edit", edit, ToolStatus::Succeeded);
        assert_eq!((edited.target.as_str(), edited.input.as_str()), ("/etc/hosts", "-a\n-b\n+c"));
        assert_eq!(edited.input_language, "diff");

        let mcp = tool("mcp__linear__get_issue", serde_json::json!({"id": "UNB-1"}), ToolStatus::Failed);
        assert_eq!((mcp.verb.as_str(), mcp.target.as_str()), ("Used", "linear: get_issue"));
    }
}
