//! A thread's transcript as the rows an app draws: one per user message, stretch of prose, code
//! block, tool call and so on. The apps keep a list of these and apply the splices sent to them.
//!
//! The work of a turn takes little room: tool calls that follow one another are one row that
//! opens into them, and once a turn has ended, everything before its last message folds behind
//! a single row.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use motile_protocol::wire::{
    Approval, Change, ChangedFile, Item, ItemKind, Media, Queued, ToolCall, ToolStatus, TurnChanges,
};
use serde::Serialize;
use serde_json::Value;

use super::highlight::{self, Incremental, Spans};
use super::markdown::{self, Block, ParaKind, Prose};

/// Tool output beyond this is cut; nobody reads more of it in a chat.
const MAX_OUTPUT_CHARS: usize = 20_000;
/// A turn that changed more files than this shows them with their folders closed.
const OPEN_UP_TO_FILES: usize = 12;

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

/// A file attached to a message. An image or a video is shown: the app asks the core for the
/// file `media` names, and for a video's `poster` until it plays.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Attached {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub video: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RowKind {
    User {
        text: String,
        attachments: Vec<Attached>,
        at: f64,
    },
    Prose {
        #[serde(flatten)]
        prose: Prose,
        /// What is right above it in the reply; the space above the row depends on it.
        #[serde(skip_serializing_if = "Option::is_none")]
        after: Option<After>,
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
    /// An image or a video the reply shows. The app asks the core for the file `media` names.
    Media {
        media: String,
        video: bool,
        /// In pixels, for an image: the row has its size before the file is there.
        width: Option<u32>,
        height: Option<u32>,
        size: u64,
        /// What the agent said it shows.
        alt: String,
        /// The file's name where the agent made it.
        name: String,
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
        /// When the latest one that still runs started.
        #[serde(skip_serializing_if = "Option::is_none")]
        started_at: Option<f64>,
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
    /// What a finished turn changed in the thread's folder. The row's item is the one that ends
    /// the turn, which names the turn when its diff is asked for.
    Changes {
        files: usize,
        added: u32,
        removed: u32,
        /// When the turn ended.
        at: f64,
        /// The files under their folders, each folder before what is in it.
        entries: Vec<ChangeEntry>,
    },
    TurnEnd {
        duration_ms: Option<u64>,
        cost_usd: Option<f64>,
        is_error: bool,
        stopped: bool,
        /// The turn's fold says how long it took, so this row doesn't.
        folded: bool,
    },
    /// A message that waits for the agent to take it. The row's item is the message.
    Queued {
        text: String,
        attachments: Vec<Attached>,
        /// How it waits: queued, held, or being given to the agent.
        status: &'static str,
        /// The agent is being given it, so it can no longer be sent now or taken back.
        sending: bool,
    },
}

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum After {
    Prose,
    Table,
    Code,
    Media,
    /// A tool call, thinking, or the row that stands for several of them.
    Work,
}

impl After {
    fn row(kind: &RowKind) -> Option<Self> {
        match kind {
            RowKind::Prose { prose, .. } => Some(match prose.paras.last().map(|para| &para.kind) {
                Some(ParaKind::Cell { .. }) => Self::Table,
                Some(ParaKind::Pre { .. }) => Self::Code,
                _ => Self::Prose,
            }),
            RowKind::Code { .. } => Some(Self::Code),
            RowKind::Media { .. } => Some(Self::Media),
            RowKind::Tool { .. } | RowKind::Thinking { .. } | RowKind::Group { .. } | RowKind::Fold { .. } => {
                Some(Self::Work)
            }
            _ => None,
        }
    }
}

/// A file a turn changed, or a folder with such files in it.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct ChangeEntry {
    /// What opens and closes a folder, through `Transcript::toggle`.
    pub id: String,
    /// A folder with nothing else in it is named together with the one inside it: `src/render`.
    pub name: String,
    pub path: String,
    pub depth: usize,
    pub folder: bool,
    pub open: bool,
    /// How a file changed.
    pub change: Option<Change>,
    /// The lines added and removed in the file, or in all the files of the folder.
    pub added: u32,
    pub removed: u32,
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
    /// When it started, while it runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<f64>,
    /// It started an agent, whose transcript is named by the row's item.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub agent: bool,
    /// What that agent is doing now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
}

/// The tool call Claude Code presents its plan with. Allowing it lets the agent carry the plan out.
const PLAN_TOOL: &str = "ExitPlanMode";
/// The tool call Claude Code asks the user questions with.
const QUESTION_TOOL: &str = "AskUserQuestion";

/// A tool call the turn waits with until the user has answered it.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Waiting {
    pub id: String,
    pub icon: &'static str,
    /// What is asked for: the tool, or what to do with a plan.
    pub title: String,
    /// What the tool acts on: the command, the file.
    pub target: String,
    /// What the buttons that allow and refuse it say.
    pub allow: &'static str,
    pub refuse: &'static str,
    /// The questions the agent asks with it; allowing it takes an answer to each.
    pub questions: Vec<Question>,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Question {
    pub text: String,
    pub options: Vec<Choice>,
    /// More than one option can be chosen.
    pub multiple: bool,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Choice {
    pub label: String,
    pub detail: String,
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
    /// Highlighters for the code blocks of items that are still streaming, by row id, and for
    /// code inside prose, by row id and paragraph.
    streaming: HashMap<String, Incremental>,
    /// The messages that wait for the agent; their rows come after everything else.
    queued: Vec<Queued>,
    /// Nothing more is coming, as in the transcript of an agent that has ended.
    settled: bool,
}

/// Code that came without highlighting: a code row, or the code paragraph `para` of a prose row.
pub struct Uncoloured {
    pub row_id: String,
    pub para: Option<usize>,
    pub language: String,
    pub code: String,
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

    /// An empty transcript for something else that happened in the same folder.
    pub fn beside(&self) -> Self {
        Self::new(&self.cwd)
    }

    pub fn clear(&mut self) {
        *self = Self::new(&self.cwd);
    }

    pub fn waiting(&self, approval: &Approval) -> Waiting {
        let call = ToolCall {
            id: approval.id.clone(),
            name: approval.tool_name.clone(),
            input: approval.input.clone(),
            output: None,
            status: ToolStatus::Running,
            agent: None,
        };
        let tool = describe(&call, &self.cwd, 0.0);
        let (title, target, allow, refuse) = match approval.tool_name.as_str() {
            PLAN_TOOL => ("The plan is ready".to_string(), String::new(), "Implement", "Keep planning"),
            QUESTION_TOOL => ("The agent has a question".to_string(), String::new(), "Answer", "Skip"),
            _ => (tool.name, tool.target, "Allow", "Refuse"),
        };
        let input: Value = serde_json::from_str(&approval.input).unwrap_or_default();
        Waiting { id: approval.id.clone(), icon: tool.icon, title, target, allow, refuse, questions: questions(&input) }
    }

    /// Fills an empty transcript with stored items, in order.
    pub fn load(&mut self, items: Vec<Item>) {
        self.rendered = items.iter().map(|item| render(item, &self.cwd, None)).collect();
        self.items = items;
        self.show();
    }

    /// Where the first loaded item stands in the thread.
    pub fn first_seq(&self) -> Option<u64> {
        self.items.first().map(|item| item.seq)
    }

    /// Lets go of the turns before the one that has the first of the last `keep_rows` rows.
    pub fn trim(&mut self, keep_rows: usize) -> Option<Splice> {
        let kept = &self.rows[self.rows.len().checked_sub(keep_rows)?].item;
        let index = self.items.iter().position(|item| &item.id == kept)?;
        let is_turn_end = |item: &Item| matches!(item.kind, ItemKind::TurnEnd { .. });
        let cut = (1..=index).rev().find(|&index| is_turn_end(&self.items[index - 1]))?;
        self.items.drain(..cut);
        self.rendered.drain(..cut);
        self.show()
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

    /// Says whether more is coming. The last tool calls of a transcript that has ended are
    /// summed up like the ones before them.
    pub fn set_settled(&mut self, settled: bool) -> Option<Splice> {
        self.settled = settled;
        self.show()
    }

    pub fn set_queued(&mut self, queued: Vec<Queued>) -> Option<Splice> {
        self.queued = queued;
        self.show()
    }

    /// Opens or closes a group, a fold, or a folder of changed files.
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
        let mut shown = present(&self.items, &self.rendered, &self.opened, self.settled);
        shown.extend(queued_rows(&self.queued).into_iter().map(Cow::Owned));
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

    /// The code among `row_ids` that has no highlighting yet.
    pub fn unhighlighted(&self, row_ids: &[String]) -> Vec<Uncoloured> {
        let mut found = Vec::new();
        for row in self.rows.iter().filter(|row| row_ids.contains(&row.id)) {
            match &row.kind {
                RowKind::Code { language, code, spans: None } => found.push(Uncoloured {
                    row_id: row.id.clone(),
                    para: None,
                    language: language.clone(),
                    code: code.clone(),
                }),
                RowKind::Prose { prose, .. } => {
                    for (index, para) in prose.paras.iter().enumerate() {
                        let ParaKind::Pre { language, code, spans: None, .. } = &para.kind else { continue };
                        found.push(Uncoloured {
                            row_id: row.id.clone(),
                            para: Some(index),
                            language: language.clone(),
                            code: code.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
        found
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

    /// Stores the highlighting of code inside prose, and returns the row to show again.
    pub fn set_para_spans(&mut self, row_id: &str, para: usize, code: &str, spans: Spans) -> Option<Splice> {
        let row = self.rendered.iter_mut().flatten().find(|row| row.id == row_id)?;
        let RowKind::Prose { prose, .. } = &mut row.kind else { return None };
        let ParaKind::Pre { code: current, spans: slot, .. } = &mut prose.paras.get_mut(para)?.kind else {
            return None;
        };
        if current != code {
            return None;
        }
        *slot = Some(spans);
        self.show()
    }
}

fn attached(paths: &[String], media: &[Media]) -> Vec<Attached> {
    let file = |path: &String| {
        let shown = media.iter().find(|media| &media.src == path);
        Attached {
            name: file_name(path).to_string(),
            media: shown.map(|media| media.id.clone()),
            video: shown.is_some_and(|media| media.video),
            poster: shown.and_then(|media| media.poster.clone()),
        }
    };
    paths.iter().map(file).collect()
}

/// The queued messages as rows, each saying how it waits.
fn queued_rows(queued: &[Queued]) -> Vec<Row> {
    let status = |message: &Queued| match (message.sending, message.held) {
        (true, _) => "Sending…",
        (false, true) => "Held",
        (false, false) => "Queued",
    };
    let row = |message: &Queued| Row {
        id: format!("queued/{}", message.id),
        item: message.id.clone(),
        nested: false,
        kind: RowKind::Queued {
            text: message.text.clone(),
            attachments: attached(&message.attachments, &message.media),
            status: status(message),
            sending: message.sending,
        },
    };
    queued.iter().map(row).collect()
}

/// The rows to show for the items: each turn's, one turn after the other.
fn present<'a>(
    items: &'a [Item],
    rendered: &'a [Vec<Row>],
    opened: &HashSet<String>,
    settled: bool,
) -> Vec<Cow<'a, Row>> {
    let mut shown = Vec::new();
    let mut start = 0;
    while start < items.len() {
        let next_turn = (start + 1..items.len()).find(|&index| {
            matches!(items[index].kind, ItemKind::User { .. })
                || matches!(items[index - 1].kind, ItemKind::TurnEnd { .. })
        });
        let end = next_turn.unwrap_or(items.len());
        let is_turn_end = |item: &Item| matches!(item.kind, ItemKind::TurnEnd { .. });
        let went_on = !items[start..end].iter().any(is_turn_end);
        let part = Part {
            took_message: start > 0 && !is_turn_end(&items[start - 1]),
            last: end == items.len() && !settled,
            // The user said more while the agent worked, and the turn has ended since.
            ended_after: items[end..].first().filter(|_| went_on && items[end..].iter().any(is_turn_end)),
        };
        present_turn(&items[start..end], &rendered[start..end], part, opened, &mut shown);
        start = end;
    }
    join(&mut shown);
    shown
}

/// Tells every prose row what is right above it.
fn join(shown: &mut [Cow<Row>]) {
    let mut above = None;
    for row in shown {
        let after = std::mem::replace(&mut above, After::row(&row.kind));
        if after.is_none() || !matches!(row.kind, RowKind::Prose { .. }) {
            continue;
        }
        let RowKind::Prose { after: slot, .. } = &mut row.to_mut().kind else { continue };
        *slot = after;
    }
}

/// Where a part of the transcript stands among the others. A turn the user spoke into is
/// several parts: one up to each message the agent took while it worked.
struct Part<'a> {
    /// It starts with a message the agent took while it worked.
    took_message: bool,
    /// Nothing comes after it, and more may.
    last: bool,
    /// The message the agent took after this part, once the turn that went on with it has ended.
    ended_after: Option<&'a Item>,
}

/// A turn is the user's message and what the agent did until it ended, or what an agent that
/// monitors did when it went back to work by itself.
fn present_turn<'a>(
    items: &'a [Item],
    rendered: &'a [Vec<Row>],
    part: Part,
    opened: &HashSet<String>,
    shown: &mut Vec<Cow<'a, Row>>,
) {
    let work_start = usize::from(matches!(items[0].kind, ItemKind::User { .. }));
    shown.extend(rendered[..work_start].iter().flatten().map(Cow::Borrowed));

    let ended = items.iter().rev().find_map(|item| match &item.kind {
        ItemKind::TurnEnd { summary } => Some((summary, item.created_at)),
        _ => None,
    });
    let failed = items.iter().any(|item| matches!(item.kind, ItemKind::Error { .. }));
    let answer = items.iter().rposition(|item| matches!(item.kind, ItemKind::Assistant { .. }));
    let since_start = |until: f64| Some(((until - items[0].created_at).max(0.0) * 1000.0) as u64);
    // Once the turn has ended well, what led up to its last message folds away. So does all of
    // what the agent did before the user said more.
    let fold = match (ended, answer, part.ended_after) {
        (Some((summary, at)), Some(answer), _) if !failed && answer > work_start => {
            let duration_ms = if part.took_message { since_start(at) } else { summary.duration_ms };
            Some((duration_ms, summary.stopped, answer))
        }
        (None, _, Some(next)) if !failed && items.len() > work_start => {
            Some((since_start(next.created_at), false, items.len()))
        }
        _ => None,
    };

    let mut index = work_start;
    if let Some((duration_ms, stopped, shown_from)) = fold {
        let id = format!("{}/fold", items[work_start].id);
        let open = opened.contains(&id);
        let kind = RowKind::Fold { duration_ms, stopped, open };
        shown.push(Cow::Owned(Row { id, item: items[work_start].id.clone(), nested: false, kind }));
        if !open {
            index = shown_from;
        }
    }

    while index < items.len() {
        if !is_work(&items[index]) {
            shown.extend(changes_row(&items[index], opened).map(Cow::Owned));
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
        let live = ended.is_none() && part.last && run_end == items.len();
        shown.push(Cow::Owned(Row { id, item: first.item.clone(), nested: false, kind: group(run, live, open) }));
        if open {
            shown.extend(run.iter().flatten().map(|row| Cow::Owned(Row { nested: true, ..row.clone() })));
        }
    }
}

/// The row that says what the turn changed, for the item that ends a turn which changed something.
fn changes_row(item: &Item, opened: &HashSet<String>) -> Option<Row> {
    let ItemKind::TurnEnd { summary } = &item.kind else { return None };
    let TurnChanges { files, .. } = summary.changes.as_ref().filter(|changes| !changes.files.is_empty())?;
    let id = format!("{}/changes", item.id);
    let mut entries = Vec::new();
    let tree = Tree { row_id: &id, opened, open: files.len() <= OPEN_UP_TO_FILES };
    tree.list(&files.iter().collect::<Vec<_>>(), "", 0, &mut entries);
    let kind = RowKind::Changes {
        files: files.len(),
        added: files.iter().map(|file| file.added).sum(),
        removed: files.iter().map(|file| file.removed).sum(),
        at: item.created_at,
        entries,
    };
    Some(Row { id, item: item.id.clone(), nested: false, kind })
}

/// How the changed files of a turn are listed under their folders.
struct Tree<'a> {
    row_id: &'a str,
    /// The folders that were opened or closed by hand.
    opened: &'a HashSet<String>,
    /// Whether a folder is open until then.
    open: bool,
}

impl Tree<'_> {
    /// Lists `files`, which are all inside `folder`: its folders first, then its files, each by name.
    fn list(&self, files: &[&ChangedFile], folder: &str, depth: usize, entries: &mut Vec<ChangeEntry>) {
        let inside = |file: &ChangedFile| file.path[folder.len()..].split_once('/').map(|(name, _)| name.to_string());
        let mut folders: Vec<String> = files.iter().filter_map(|file| inside(file)).collect();
        folders.sort_by_key(|name| name.to_lowercase());
        folders.dedup();
        for name in folders {
            let within: Vec<&ChangedFile> =
                files.iter().copied().filter(|file| inside(file).as_deref() == Some(name.as_str())).collect();
            let path = shared_folder(&within);
            let id = format!("{}/{path}", self.row_id);
            let open = self.open != self.opened.contains(&id);
            entries.push(ChangeEntry {
                id,
                name: path[folder.len()..].to_string(),
                path: path.clone(),
                depth,
                folder: true,
                open,
                change: None,
                added: within.iter().map(|file| file.added).sum(),
                removed: within.iter().map(|file| file.removed).sum(),
            });
            if open {
                self.list(&within, &format!("{path}/"), depth + 1, entries);
            }
        }
        let mut direct: Vec<&ChangedFile> = files.iter().copied().filter(|file| inside(file).is_none()).collect();
        direct.sort_by_key(|file| file.path.to_lowercase());
        for file in direct {
            entries.push(ChangeEntry {
                id: format!("{}/{}", self.row_id, file.path),
                name: file.path[folder.len()..].to_string(),
                path: file.path.clone(),
                depth,
                folder: false,
                open: false,
                change: Some(file.change),
                added: file.added,
                removed: file.removed,
            });
        }
    }
}

/// The deepest folder all the files are in.
fn shared_folder(files: &[&ChangedFile]) -> String {
    let folder_of = |file: &ChangedFile| file.path.rsplit_once('/').map_or("", |(folder, _)| folder).to_string();
    let mut shared = files.first().map(|file| folder_of(file)).unwrap_or_default();
    for file in files {
        while !file.path.starts_with(&format!("{shared}/")) {
            shared = shared.rsplit_once('/').map_or("", |(above, _)| above).to_string();
        }
    }
    shared
}

fn is_work(item: &Item) -> bool {
    match &item.kind {
        ItemKind::Tool { call } => call.name != PLAN_TOOL,
        ItemKind::Thinking { .. } => true,
        _ => false,
    }
}

fn questions(input: &Value) -> Vec<Question> {
    let text = |value: &Value| value.as_str().unwrap_or_default().to_string();
    let asked = input["questions"].as_array().map(Vec::as_slice).unwrap_or_default();
    asked
        .iter()
        .map(|question| {
            let options = question["options"].as_array().map(Vec::as_slice).unwrap_or_default();
            let options = options
                .iter()
                .map(|option| Choice { label: text(&option["label"]), detail: text(&option["description"]) })
                .collect();
            Question {
                text: text(&question["question"]),
                options,
                multiple: question["multiSelect"].as_bool().unwrap_or(false),
            }
        })
        .collect()
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
    let started_at = tools.iter().rev().find_map(|tool| tool.started_at);
    let failed = tools.iter().any(|tool| tool.status == ToolStatus::Failed);
    let one_icon = tools.first().map(|tool| tool.icon).filter(|icon| tools.iter().all(|tool| tool.icon == *icon));
    let agents = tools.iter().filter(|tool| tool.agent && tool.started_at.is_some()).count();
    let (title, target, icon) = match (live, run.last().and_then(|rows| rows.first()).map(|row| &row.kind)) {
        _ if agents > 1 => (format!("Running {agents} agents"), String::new(), "agent"),
        (true, Some(RowKind::Tool { tool })) => (tool.verb.clone(), tool.target.clone(), tool.icon),
        _ => (summary(&tools), String::new(), one_icon.unwrap_or("tool")),
    };
    RowKind::Group { title, target, icon, running: started_at.is_some(), failed, open, started_at }
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
        "question" => format!("Asked {}", of("question", "questions")),
        "watch" => format!("Started {}", of("watch", "watches")),
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
            let attachments = attached(attachments, &item.media);
            vec![row(0, RowKind::User { text: text.clone(), attachments, at: item.created_at })]
        }
        ItemKind::Assistant { text } => render_markdown(item, text, streaming),
        ItemKind::Thinking { text } => vec![row(0, RowKind::Thinking { text: text.clone() })],
        // A plan is read like a reply, not opened like a tool call.
        ItemKind::Tool { call } if call.name == PLAN_TOOL => {
            let input: Value = serde_json::from_str(&call.input).unwrap_or_default();
            render_markdown(item, input["plan"].as_str().unwrap_or_default(), streaming)
        }
        ItemKind::Tool { call } => vec![row(0, RowKind::Tool { tool: describe(call, cwd, item.created_at) })],
        ItemKind::Error { message } => vec![row(0, RowKind::Error { message: message.clone() })],
        ItemKind::TurnEnd { summary } => vec![row(
            0,
            RowKind::TurnEnd {
                duration_ms: summary.duration_ms,
                cost_usd: summary.cost_usd,
                is_error: summary.is_error,
                stopped: summary.stopped,
                folded: false,
            },
        )],
    }
}

fn render_markdown(item: &Item, text: &str, mut streaming: Option<&mut HashMap<String, Incremental>>) -> Vec<Row> {
    let shown: Vec<&str> = item.media.iter().map(|media| media.src.as_str()).collect();
    let blocks = markdown::parse_showing(text, &shown);
    let mut rows = Vec::with_capacity(blocks.len());
    for (index, block) in blocks.into_iter().enumerate() {
        let id = format!("{}/{index}", item.id);
        let kind = match block {
            Block::Prose(mut prose) => {
                for (para_index, para) in prose.paras.iter_mut().enumerate() {
                    let ParaKind::Pre { language, code, spans, .. } = &mut para.kind else { continue };
                    *spans = colour(&format!("{id}/{para_index}"), language, code, &mut streaming);
                }
                RowKind::Prose { prose, after: None }
            }
            Block::Code { language, code } => {
                let spans = colour(&id, &language, &code, &mut streaming);
                RowKind::Code { language, code, spans }
            }
            Block::Image { src, alt } => {
                let Some(media) = item.media.iter().find(|media| media.src == src) else { continue };
                RowKind::Media {
                    media: media.id.clone(),
                    video: media.video,
                    width: media.width,
                    height: media.height,
                    size: media.size,
                    alt,
                    name: file_name(&src).to_string(),
                }
            }
        };
        rows.push(Row { id, item: item.id.clone(), nested: false, kind });
    }
    rows
}

/// Highlights streaming code as it grows; other code only if it has been highlighted before.
fn colour(
    key: &str,
    language: &str,
    code: &str,
    streaming: &mut Option<&mut HashMap<String, Incremental>>,
) -> Option<Spans> {
    match streaming {
        Some(streaming) => {
            let highlighter = streaming.entry(key.to_string()).or_insert_with(|| Incremental::new(language));
            Some(highlighter.advance(code))
        }
        None => highlight::cached(language, code),
    }
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

pub(crate) fn language_of(path: &str) -> String {
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

/// `at` is when the call started. A call that started an agent runs for as long as that agent.
fn describe(call: &ToolCall, cwd: &str, at: f64) -> Tool {
    let input: Value = serde_json::from_str(&call.input).unwrap_or_default();
    let text = |key: &str| input[key].as_str().unwrap_or_default().to_string();
    let status = call.agent.as_ref().map_or(call.status, |agent| agent.status);
    let running = status == ToolStatus::Running;
    let verb = |doing: &str, did: &str| if running { doing.to_string() } else { did.to_string() };
    let path = short_path(&text("file_path"), cwd);

    let (icon, verb, target, shown_input, input_language) = match call.name.as_str() {
        "Bash" => {
            ("terminal", verb("Running", "Ran"), first_line(&text("command")), text("command"), "bash".to_string())
        }
        "Read" => ("file", verb("Reading", "Read"), path, String::new(), String::new()),
        "Write" => ("edit", verb("Writing", "Wrote"), path, text("content"), language_of(&text("file_path"))),
        "Edit" | "MultiEdit" | "NotebookEdit" => {
            let changes = match (input["edits"].as_array(), input["changes"].as_array()) {
                (Some(edits), _) => edits
                    .iter()
                    .map(|edit| {
                        diff(
                            edit["old_string"].as_str().unwrap_or_default(),
                            edit["new_string"].as_str().unwrap_or_default(),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                // Codex says what it changed as diffs, one for each file.
                (None, Some(changes)) => {
                    changes.iter().filter_map(|change| change["diff"].as_str()).collect::<Vec<_>>().join("\n")
                }
                (None, None) => diff(&text("old_string"), &text("new_string")),
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
        PLAN_TOOL => ("todo", verb("Proposing", "Proposed"), "a plan".to_string(), String::new(), String::new()),
        QUESTION_TOOL => {
            let asked = input["questions"][0]["question"].as_str().unwrap_or_default().to_string();
            ("question", verb("Asking", "Asked"), asked, String::new(), String::new())
        }
        "Monitor" => {
            let watched = Some(text("description")).filter(|description| !description.is_empty());
            let target = watched.unwrap_or_else(|| first_line(&text("command")));
            ("watch", verb("Starting to watch", "Started watching"), target, text("command"), "bash".to_string())
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
        status,
        input: cap(&shown_input),
        input_language,
        output: call.output.as_deref().filter(|output| !output.trim().is_empty()).map(cap),
        started_at: running.then_some(at),
        agent: call.agent.is_some(),
        progress: call.agent.as_ref().filter(|_| running).and_then(|agent| agent.progress.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, seq: u64, kind: ItemKind) -> Item {
        Item { id: id.to_string(), seq, rev: seq + 1, created_at: 0.0, media: Vec::new(), parent: None, kind }
    }

    fn assistant(id: &str, seq: u64, text: &str) -> Item {
        item(id, seq, ItemKind::Assistant { text: text.to_string() })
    }

    fn tool(name: &str, input: Value, status: ToolStatus) -> Tool {
        let call =
            ToolCall { id: "t".into(), name: name.into(), input: input.to_string(), output: None, status, agent: None };
        describe(&call, "/srv/api", 0.0)
    }

    fn apply(rows: &mut Vec<Row>, splice: Splice) {
        rows.splice(splice.start..splice.start + splice.remove, splice.rows);
    }

    #[test]
    fn a_message_shows_the_images_and_videos_attached_to_it() {
        use motile_protocol::wire::Media;
        let attachments =
            vec!["/up/1/shot.png".to_string(), "/up/2/notes.txt".to_string(), "/up/3/demo.mov".to_string()];
        let media = |id: &str, src: &str, video, poster: Option<&str>| Media {
            id: id.into(),
            src: src.into(),
            video,
            size: 10,
            width: None,
            height: None,
            poster: poster.map(str::to_string),
        };
        let mut message = item("u", 0, ItemKind::User { text: "Look".into(), attachments });
        message.media = vec![
            media("abc.png", "/up/1/shot.png", false, None),
            media("def.mov", "/up/3/demo.mov", true, Some("fed.jpg")),
        ];
        let mut transcript = Transcript::new("/srv/api");
        transcript.load(vec![message]);

        let row = serde_json::to_value(&transcript.rows()[0]).unwrap();
        assert_eq!(
            row["attachments"],
            serde_json::json!([
                { "name": "shot.png", "media": "abc.png" },
                { "name": "notes.txt" },
                { "name": "demo.mov", "media": "def.mov", "video": true, "poster": "fed.jpg" },
            ])
        );
    }

    #[test]
    fn an_image_the_server_kept_is_a_row_with_its_size_and_any_other_is_a_link() {
        use motile_protocol::wire::Media;
        let mut reply = assistant("a", 0, "Done:\n\n![The page](/tmp/shots/page.png)\n\n![Gone](/tmp/gone.png)");
        let kept = Media {
            id: "abc.png".into(),
            src: "/tmp/shots/page.png".into(),
            video: false,
            size: 2048,
            width: Some(640),
            height: Some(400),
            poster: None,
        };
        reply.media = vec![kept];
        let mut transcript = Transcript::new("/srv/api");
        transcript.load(vec![reply]);

        let rows = transcript.rows();
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows[1].kind,
            RowKind::Media {
                media: "abc.png".into(),
                video: false,
                width: Some(640),
                height: Some(400),
                size: 2048,
                alt: "The page".into(),
                name: "page.png".into(),
            }
        );
        assert!(
            matches!(&rows[2].kind, RowKind::Prose { prose, .. } if prose.text == "Gone" && prose.links.len() == 1)
        );
        assert_eq!(serde_json::to_value(&rows[1]).unwrap()["kind"], "media");
    }

    #[test]
    fn prose_knows_what_is_right_above_it() {
        let long = "word ".repeat(900);
        let reply = format!(
            "Intro\n\n```rust\nfn a() {{}}\n```\n\n## After code\n\n{long}\n\n## After a cut\n\n| A |\n|---|\n| 1 |"
        );
        let mut transcript = Transcript::new("");
        transcript.load(vec![
            item("u", 0, ItemKind::User { text: "Hi".into(), attachments: Vec::new() }),
            assistant("a", 1, &reply),
            assistant("b", 2, "The next message"),
            call("t", 3, "Read", serde_json::json!({ "file_path": "/srv/api/hello.py" }), ToolStatus::Succeeded),
            assistant("c", 4, "## After a tool call"),
        ]);

        let prose: Vec<(&str, Option<After>)> = transcript
            .rows()
            .iter()
            .filter_map(|row| match &row.kind {
                RowKind::Prose { prose, after } => Some((prose.text.split('\n').next().unwrap_or_default(), *after)),
                _ => None,
            })
            .collect();
        assert_eq!(
            prose,
            vec![
                ("Intro", None),
                ("After code", Some(After::Code)),
                ("After a cut", Some(After::Prose)),
                ("The next message", Some(After::Table)),
                ("After a tool call", Some(After::Work)),
            ]
        );
        let cut = transcript.rows().iter().find(|row| row.id == "a/3").unwrap();
        assert_eq!(serde_json::to_value(cut).unwrap()["after"], "prose");
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
        assert!(matches!(&shown[0].kind, RowKind::User { attachments, .. } if attachments[0].name == "notes.txt"));
        assert!(matches!(&shown[3].kind, RowKind::Prose { prose, .. } if prose.text == "Outro"));
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
        let Uncoloured { row_id, para: None, language, code } = &wanted[0] else { panic!("a code row") };
        assert!(stored.set_spans(row_id, code, highlight::highlight(language, code)));
        assert!(stored.unhighlighted(&["b/0".to_string()]).is_empty());
        assert!(!stored.set_spans(row_id, "something else", Spans::default()));
    }

    fn pre_spans(row: &Row) -> Vec<Option<Spans>> {
        let RowKind::Prose { prose, .. } = &row.kind else { panic!("expected prose") };
        let spans = prose.paras.iter().filter_map(|para| match &para.kind {
            ParaKind::Pre { spans, .. } => Some(spans.clone()),
            _ => None,
        });
        spans.collect()
    }

    #[test]
    fn code_inside_a_list_is_highlighted_while_streaming_and_when_asked() {
        let reply = "1. Run:\n\n   ```rust\n   fn listed_and_never_seen() {}\n   ```\n2. Done";
        let mut live = Transcript::new("");
        live.upsert(assistant("a", 0, reply), true);
        assert!(matches!(&pre_spans(&live.rows()[0])[..], [Some(spans)] if !spans.is_empty()));

        let mut stored = Transcript::new("");
        stored.load(vec![assistant("b", 0, &reply.replace("listed", "stored"))]);
        assert_eq!(pre_spans(&stored.rows()[0]), vec![None]);
        let wanted = stored.unhighlighted(&["b/0".to_string()]);
        let [Uncoloured { row_id, para: Some(para), language, code }] = &wanted[..] else { panic!("one paragraph") };
        assert_eq!(code, "fn stored_and_never_seen() {}");

        assert!(stored.set_para_spans(row_id, *para, "changed meanwhile", Spans::default()).is_none());
        let splice = stored.set_para_spans(row_id, *para, code, highlight::highlight(language, code)).unwrap();
        assert_eq!((splice.start, splice.remove), (0, 1));
        assert!(matches!(&pre_spans(&splice.rows[0])[..], [Some(spans)] if !spans.is_empty()));
        assert!(stored.unhighlighted(&["b/0".to_string()]).is_empty());
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
    fn earlier_turns_are_let_go_whole_and_come_back_in_front() {
        let turn = |start: u64| {
            vec![
                item(&format!("u{start}"), start, ItemKind::User { text: "Go on".into(), attachments: Vec::new() }),
                assistant(&format!("a{start}"), start + 1, "Done."),
                item(&format!("e{start}"), start + 2, ItemKind::TurnEnd { summary: Default::default() }),
            ]
        };
        let mut transcript = Transcript::new("");
        transcript.load((0..4).flat_map(|index| turn(index * 3)).collect());
        let whole = transcript.rows().to_vec();

        assert!(transcript.trim(12).is_none());
        let splice = transcript.trim(4).unwrap();

        assert_eq!((splice.start, splice.remove, splice.rows.len()), (0, 6, 0));
        assert_eq!(transcript.first_seq(), Some(6));

        let splice = transcript.prepend(turn(0).into_iter().chain(turn(3)).collect()).unwrap();

        assert_eq!((splice.start, splice.remove, splice.rows.len()), (0, 0, 6));
        assert_eq!(transcript.rows(), whole);
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
        let call =
            ToolCall { id: id.into(), name: name.into(), input: input.to_string(), output: None, status, agent: None };
        item(id, seq, ItemKind::Tool { call })
    }

    /// The rows in short: their kind, and for groups and folds what they say.
    fn outline(transcript: &Transcript) -> Vec<String> {
        let rows = transcript.rows().iter().map(|row| match &row.kind {
            RowKind::User { .. } => "user".to_string(),
            RowKind::Prose { prose, .. } => prose.text.clone(),
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

    fn agent_call(id: &str, seq: u64, description: &str, progress: Option<&str>, status: ToolStatus) -> Item {
        let mut item = call(id, seq, "Agent", serde_json::json!({"description": description}), ToolStatus::Succeeded);
        let ItemKind::Tool { call } = &mut item.kind else { unreachable!() };
        call.agent = Some(motile_protocol::wire::Subagent {
            kind: None,
            status,
            progress: progress.map(String::from),
            result: None,
            tokens: None,
            tool_uses: None,
            duration_ms: None,
        });
        item
    }

    #[test]
    fn an_agent_that_goes_on_after_its_call_returned_still_runs_and_says_what_it_does() {
        let mut transcript = Transcript::new("");
        transcript.load(vec![agent_call("t1", 0, "Review", Some("Reading a.rs"), ToolStatus::Running)]);
        let RowKind::Tool { tool } = &transcript.rows()[0].kind else { panic!() };
        assert_eq!((tool.verb.as_str(), tool.status, tool.agent), ("Running an agent:", ToolStatus::Running, true));
        assert_eq!((tool.progress.as_deref(), tool.started_at), (Some("Reading a.rs"), Some(0.0)));

        transcript.upsert(agent_call("t2", 1, "Test", None, ToolStatus::Running), true);
        assert_eq!(outline(&transcript), ["[Running 2 agents]"]);
        transcript.upsert(agent_call("t1", 0, "Review", Some("Reading a.rs"), ToolStatus::Succeeded), true);
        transcript.upsert(agent_call("t2", 1, "Test", None, ToolStatus::Succeeded), true);
        assert_eq!(outline(&transcript), ["[Ran an agent: Test]"]);
        let RowKind::Group { running, started_at, .. } = &transcript.rows()[0].kind else { panic!() };
        assert_eq!((*running, *started_at), (false, None));
    }

    #[test]
    fn the_last_tool_calls_of_a_transcript_that_has_ended_are_summed_up() {
        let done = ToolStatus::Succeeded;
        let mut transcript = Transcript::new("");
        transcript.load(vec![
            call("t1", 0, "Read", serde_json::json!({"file_path": "a.rs"}), done),
            call("t2", 1, "Read", serde_json::json!({"file_path": "b.rs"}), done),
        ]);
        assert_eq!(outline(&transcript), ["[Read b.rs]"]);
        transcript.set_settled(true);
        assert_eq!(outline(&transcript), ["[Read 2 files]"]);
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
    fn work_before_a_message_the_agent_took_folds_once_the_turn_has_ended() {
        let read = |id: &str, seq| {
            call(id, seq, "Read", serde_json::json!({"file_path": "/srv/api/a.rs"}), ToolStatus::Succeeded)
        };
        let at = |created_at, item: Item| Item { created_at, ..item };
        let user = |id: &str, seq| item(id, seq, ItemKind::User { text: "Go".into(), attachments: Vec::new() });
        let mut transcript = Transcript::new("/srv/api");
        transcript.load(vec![
            at(100.0, user("u1", 0)),
            assistant("a1", 1, "Looking."),
            read("t1", 2),
            read("t2", 3),
            at(130.0, user("u2", 4)),
            read("t3", 5),
            assistant("a2", 6, "Done."),
        ]);
        assert_eq!(
            outline(&transcript),
            ["user", "Looking.", "[Read 2 files]", "user", "Read a.rs", "Done."],
            "nothing folds while the turn runs"
        );

        let summary = motile_protocol::wire::TurnSummary { duration_ms: Some(90_000), ..Default::default() };
        transcript.upsert(at(190.0, item("e", 7, ItemKind::TurnEnd { summary })), true);
        assert_eq!(outline(&transcript), ["user", "fold", "user", "fold", "Done.", "end folded"]);
        let folds = transcript.rows().iter().filter_map(|row| match row.kind {
            RowKind::Fold { duration_ms, .. } => duration_ms,
            _ => None,
        });
        assert_eq!(folds.collect::<Vec<_>>(), [30_000, 60_000], "each fold says how long its own part took");

        transcript.toggle("a1/fold");
        assert_eq!(
            outline(&transcript),
            ["user", "fold open", "Looking.", "[Read 2 files]", "user", "fold", "Done.", "end folded"]
        );
    }

    #[test]
    fn what_a_turn_changed_is_listed_under_its_folders_before_the_turn_ends() {
        let file = |path: &str, added, removed| ChangedFile {
            path: path.to_string(),
            from: None,
            change: Change::Modified,
            added,
            removed,
        };
        let files = vec![
            file("README.md", 1, 1),
            file("apps/server/src/git.rs", 40, 2),
            file("apps/server/src/hub.rs", 10, 0),
            file("crates/core/src/render/rows.rs", 5, 5),
        ];
        let changes = TurnChanges { snapshot: "abc".into(), files };
        let summary = motile_protocol::wire::TurnSummary { changes: Some(changes), ..Default::default() };
        let mut transcript = Transcript::new("");
        transcript.load(vec![
            item("u", 0, ItemKind::User { text: "Go".into(), attachments: Vec::new() }),
            assistant("a", 1, "Done."),
            item("e", 2, ItemKind::TurnEnd { summary }),
        ]);

        let listed = |transcript: &Transcript| {
            let row = &transcript.rows()[2];
            let RowKind::Changes { files, added, removed, entries, .. } = &row.kind else { panic!("the changes") };
            assert_eq!((row.id.as_str(), row.item.as_str(), *files, *added, *removed), ("e/changes", "e", 4, 56, 8));
            let line = |entry: &ChangeEntry| format!("{}{} +{}", "  ".repeat(entry.depth), entry.name, entry.added);
            entries.iter().map(line).collect::<Vec<_>>()
        };
        assert_eq!(
            listed(&transcript),
            [
                "apps/server/src +50",
                "  git.rs +40",
                "  hub.rs +10",
                "crates/core/src/render +5",
                "  rows.rs +5",
                "README.md +1"
            ]
        );
        assert!(matches!(transcript.rows()[3].kind, RowKind::TurnEnd { .. }));

        let splice = transcript.toggle("e/changes/apps/server/src").unwrap();
        assert_eq!((splice.start, splice.remove, splice.rows.len()), (2, 1, 1));
        assert_eq!(
            listed(&transcript),
            ["apps/server/src +50", "crates/core/src/render +5", "  rows.rs +5", "README.md +1"]
        );
    }

    #[test]
    fn queued_messages_are_the_last_rows_and_say_how_they_wait() {
        let message = |id: &str, held, sending| Queued {
            id: id.into(),
            text: "Also this".into(),
            attachments: vec!["/tmp/a/notes.txt".into()],
            media: Vec::new(),
            held,
            sending,
        };
        let statuses = |transcript: &Transcript| {
            let rows = transcript.rows().iter().filter_map(|row| match &row.kind {
                RowKind::Queued { status, .. } => Some(*status),
                _ => None,
            });
            rows.collect::<Vec<_>>()
        };
        let mut transcript = Transcript::new("");
        transcript.load(vec![assistant("a", 0, "Working on it.")]);

        let waiting = vec![message("m1", true, false), message("m2", false, false), message("m3", false, false)];
        let splice = transcript.set_queued(waiting).unwrap();
        assert_eq!((splice.start, splice.remove, splice.rows.len()), (1, 0, 3));
        assert_eq!(statuses(&transcript), ["Held", "Queued", "Queued"]);
        let first = &transcript.rows()[1];
        assert_eq!((first.id.as_str(), first.item.as_str()), ("queued/m1", "m1"));
        assert!(matches!(&first.kind, RowKind::Queued { attachments, .. } if attachments[0].name == "notes.txt"));

        transcript.set_queued(vec![message("m2", false, true), message("m3", false, false)]);
        assert_eq!(statuses(&transcript), ["Sending…", "Queued"]);

        // What the agent says next goes above the messages that still wait.
        transcript.upsert(assistant("b", 1, "Still working."), true);
        let kinds: Vec<bool> = transcript.rows().iter().map(|row| matches!(row.kind, RowKind::Queued { .. })).collect();
        assert_eq!(kinds, [false, false, true, true]);
        assert!(transcript.set_queued(Vec::new()).is_some());
        assert!(statuses(&transcript).is_empty());
    }

    #[test]
    fn what_a_monitoring_agent_does_later_is_a_turn_of_its_own() {
        let summary = motile_protocol::wire::TurnSummary::default();
        let mut transcript = Transcript::new("");
        transcript.load(vec![
            item("u", 0, ItemKind::User { text: "Watch the deploy".into(), attachments: Vec::new() }),
            call(
                "t1",
                1,
                "Monitor",
                serde_json::json!({"command": "./status", "description": "deploy"}),
                ToolStatus::Succeeded,
            ),
            assistant("a1", 2, "Watching."),
            item("e1", 3, ItemKind::TurnEnd { summary: summary.clone() }),
            call("t2", 4, "Bash", serde_json::json!({"command": "./logs"}), ToolStatus::Succeeded),
            assistant("a2", 5, "It is healthy."),
            item("e2", 6, ItemKind::TurnEnd { summary }),
            call("t3", 7, "Bash", serde_json::json!({"command": "./logs"}), ToolStatus::Running),
        ]);
        assert_eq!(
            outline(&transcript),
            ["user", "fold", "Watching.", "end folded", "fold", "It is healthy.", "end folded", "Running ./logs"]
        );
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
    fn a_tool_call_that_waits_for_approval_is_worded_like_the_others() {
        let transcript = Transcript::new("/srv/api");
        let input = serde_json::json!({"file_path": "/srv/api/greet.py", "old_string": "a", "new_string": "b"});
        let approval = Approval { id: "r1".into(), tool_name: "Edit".into(), input: input.to_string() };
        let waiting = transcript.waiting(&approval);
        assert_eq!((waiting.id.as_str(), waiting.title.as_str(), waiting.target.as_str()), ("r1", "Edit", "greet.py"));
        assert_eq!((waiting.allow, waiting.refuse, waiting.questions.len()), ("Allow", "Refuse", 0));

        let options =
            serde_json::json!([{"label": "Red", "description": "Warm"}, {"label": "Blue", "description": "Calm"}]);
        let input =
            serde_json::json!({"questions": [{"question": "Which color?", "options": options, "multiSelect": true}]});
        let approval = Approval { id: "r2".into(), tool_name: "AskUserQuestion".into(), input: input.to_string() };
        let waiting = transcript.waiting(&approval);
        let question = &waiting.questions[0];
        assert_eq!((waiting.allow, question.text.as_str(), question.multiple), ("Answer", "Which color?", true));
        assert_eq!(question.options[1], Choice { label: "Blue".into(), detail: "Calm".into() });

        let approval = Approval { id: "r3".into(), tool_name: "ExitPlanMode".into(), input: "{}".into() };
        let waiting = transcript.waiting(&approval);
        assert_eq!(
            (waiting.title.as_str(), waiting.allow, waiting.refuse),
            ("The plan is ready", "Implement", "Keep planning")
        );
    }

    #[test]
    fn a_plan_is_shown_as_prose_and_not_put_away_with_the_tool_calls() {
        let mut transcript = Transcript::new("");
        transcript.load(vec![
            item("u", 0, ItemKind::User { text: "Plan it".into(), attachments: Vec::new() }),
            call("t1", 1, "Read", serde_json::json!({"file_path": "a.rs"}), ToolStatus::Succeeded),
            call("t2", 2, "ToolSearch", serde_json::json!({"query": "select:ExitPlanMode"}), ToolStatus::Succeeded),
            call("p", 3, "ExitPlanMode", serde_json::json!({"plan": "Add `hello`."}), ToolStatus::Running),
        ]);
        assert_eq!(outline(&transcript), ["user", "[Read 1 file and used 1 tool]", "Add hello."]);
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
        let changes = serde_json::json!([{"path": "/srv/api/a.rs", "kind": "update", "diff": "@@ -1 +1 @@\n-a\n+b"}]);
        let changed =
            tool("Edit", serde_json::json!({"file_path": "/srv/api/a.rs", "changes": changes}), ToolStatus::Succeeded);
        assert_eq!((changed.target.as_str(), changed.input.as_str()), ("a.rs", "@@ -1 +1 @@\n-a\n+b"));

        let monitor = serde_json::json!({"command": "tail -f deploy.log", "description": "deploy log"});
        let watching = tool("Monitor", monitor, ToolStatus::Succeeded);
        assert_eq!(
            (watching.verb.as_str(), watching.target.as_str(), watching.icon),
            ("Started watching", "deploy log", "watch")
        );
        assert_eq!(watching.input, "tail -f deploy.log");

        let mcp = tool("mcp__linear__get_issue", serde_json::json!({"id": "UNB-1"}), ToolStatus::Failed);
        assert_eq!((mcp.verb.as_str(), mcp.target.as_str()), ("Used", "linear: get_issue"));
    }
}
