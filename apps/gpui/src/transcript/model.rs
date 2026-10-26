//! The rows of a transcript, kept in step with the core. Rows are made ready to draw on the
//! core's thread (`prepare`), so the main thread only splices them in.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::*;
use motile_core::api::Event;
use motile_core::render::agents::AgentState;
use motile_core::render::markdown::ParaKind;
use motile_core::render::rows::{After, Row, RowKind};

use super::prose::{self, PreparedProse};
use crate::models::{Activity, AttachedFile};

/// One row of the transcript, with what drawing it needs worked out.
pub struct RowModel {
    pub row: Row,
    /// A prose row's blocks.
    pub prose: Option<PreparedProse>,
    /// A code row's colours, by byte range of its code.
    pub code_spans: Option<Vec<(Range<usize>, u32)>>,
}

impl RowModel {
    pub fn new(row: Row) -> Self {
        let prose = match &row.kind {
            RowKind::Prose { prose, .. } => Some(prose::prepare(prose)),
            _ => None,
        };
        let code_spans = match &row.kind {
            RowKind::Code { code, spans: Some(spans), .. } => Some(prose::code_spans(code, spans)),
            _ => None,
        };
        Self { row, prose, code_spans }
    }

    pub const PENDING_ID: &str = "pending";

    /// A message on its way to the server. One sent while the agent works will wait for it, so
    /// it shows as it will once the server has it.
    pub fn pending(text: String, attachments: Vec<AttachedFile>, queued: bool) -> Self {
        let attachments: Vec<motile_core::render::rows::Attached> = attachments
            .into_iter()
            .map(|file| motile_core::render::rows::Attached {
                name: file.name,
                media: file.media,
                video: file.video,
                poster: file.poster,
            })
            .collect();
        let kind = if queued {
            RowKind::Queued { text, skills: Vec::new(), attachments, status: "Queued" }
        } else {
            RowKind::User {
                text,
                links: Vec::new(),
                skills: Vec::new(),
                attachments,
                values: None,
                at: crate::models::now(),
            }
        };
        Self::new(Row { id: Self::PENDING_ID.into(), item: Self::PENDING_ID.into(), nested: false, kind, after: None })
    }

    pub fn is_pending(&self) -> bool {
        self.row.id == Self::PENDING_ID
    }

    /// The space the row keeps from the row above it. A reply is cut into rows, and its rows are
    /// as far apart as its paragraphs: a paragraph after another, and a tool row after a
    /// paragraph or before one.
    pub fn above(&self) -> f32 {
        let Some(after) = self.row.after else { return 0. };
        match &self.row.kind {
            RowKind::Prose { prose } => {
                let first_heading =
                    prose.paras.first().is_some_and(|para| matches!(para.kind, ParaKind::Heading { .. }));
                let heading = if first_heading { prose::HEADING_GAP } else { 0. };
                match after {
                    After::Prose | After::Media | After::Work => {
                        prose::LINE_SPACING + prose::BLOCK_GAP - super::rows::PROSE_GAP + heading
                    }
                    After::Table => prose::BLOCK_GAP - super::rows::PROSE_GAP + heading,
                    After::Code => heading,
                }
            }
            RowKind::Tool { .. } | RowKind::Thinking { .. } | RowKind::Group { .. } | RowKind::Fold { .. } => {
                match after {
                    After::Prose | After::Media => super::rows::AFTER_PROSE,
                    _ => 0.,
                }
            }
            _ => 0.,
        }
    }

    pub fn is_user(&self) -> bool {
        matches!(self.row.kind, RowKind::User { .. })
    }

    pub fn is_queued(&self) -> bool {
        matches!(self.row.kind, RowKind::Queued { .. })
    }

    /// The row is a message the server has: one in the transcript, or one that waits for the agent.
    pub fn is_sent_message(&self) -> bool {
        self.is_user() || self.is_queued()
    }

    /// The row has code that came without highlighting.
    pub fn needs_highlight(&self) -> bool {
        match &self.row.kind {
            RowKind::Code { spans, .. } => spans.is_none(),
            RowKind::Prose { prose, .. } => prose
                .paras
                .iter()
                .any(|para| matches!(&para.kind, motile_core::render::markdown::ParaKind::Pre { spans: None, .. })),
            _ => false,
        }
    }

    /// The plain text of the row, for copying a whole reply.
    pub fn plain_text(&self) -> Option<String> {
        match &self.row.kind {
            RowKind::Prose { .. } => self.prose.as_ref().map(|prose| prose.plain.clone()),
            RowKind::Code { language, code, .. } => Some(format!("```{language}\n{code}\n```")),
            _ => None,
        }
    }
}

/// What the core's thread made of a transcript's event.
pub enum Prepared {
    Rows {
        thread_id: String,
        reset: bool,
        start: usize,
        remove: usize,
        rows: Vec<Arc<RowModel>>,
        earlier: bool,
    },
    AgentRows {
        thread_id: String,
        agent_id: String,
        reset: bool,
        start: usize,
        remove: usize,
        rows: Vec<Arc<RowModel>>,
    },
}

/// Makes the rows of an event ready to draw; any other event is handed back as it is.
#[allow(clippy::result_large_err)]
pub fn prepare(event: Event) -> Result<Prepared, Event> {
    match event {
        Event::Rows { thread_id, reset, start, remove, rows, earlier } => Ok(Prepared::Rows {
            thread_id,
            reset,
            start,
            remove,
            rows: rows.into_iter().map(|row| Arc::new(RowModel::new(row))).collect(),
            earlier,
        }),
        Event::AgentRows { thread_id, agent_id, reset, start, remove, rows } => Ok(Prepared::AgentRows {
            thread_id,
            agent_id,
            reset,
            start,
            remove,
            rows: rows.into_iter().map(|row| Arc::new(RowModel::new(row))).collect(),
        }),
        event => Err(event),
    }
}

/// A turn of the open thread that changed files.
#[derive(Clone, PartialEq, Debug)]
pub struct TurnChange {
    /// The item that ended the turn.
    pub id: String,
    pub at: f64,
    pub files: usize,
}

/// An item of the list that shows a transcript.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Item {
    /// The room above the first row, which the list counts as content for its scroll bar.
    Top,
    /// The row of the transcript at that index.
    Row(usize),
    /// The line that says the agent works.
    Working,
    /// The message on its way to the server.
    Pending,
    /// The way back to the thread whose agent started this one, above the first row or, with
    /// `true`, under the last.
    StartedBy(bool),
    /// The room under the last row for what floats over the transcript's end.
    Bottom,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Key {
    Top,
    Row(String),
    Working,
    Pending,
    StartedBy(bool),
    Bottom,
}

/// How long a row that streams in takes to fade in.
pub const FADE: Duration = Duration::from_millis(350);

/// The rows of the open thread, and the list that shows them: a row for each of them, the line
/// that says the agent works above the messages that wait for it, and the message on its way.
pub struct Transcript {
    pub thread_id: Option<String>,
    /// Shared with the frame that draws them, so that drawing doesn't copy them.
    pub rows: Rc<Vec<Arc<RowModel>>>,
    pub pending: Option<Arc<RowModel>>,
    pub activity: Activity,
    /// The thread has turns before the first row.
    pub earlier: bool,
    /// The thread whose agent started this one.
    pub started_by: Option<String>,
    /// The thread has caught up with its server. What it had before that is simply there when
    /// the thread opens; only what arrives after it streams in.
    pub live: bool,
    /// The rows shown are another thread's, kept until this one's arrive so the view isn't
    /// empty in between. What is around them stays too, so that they all change in one pass.
    pub keeps_rows: bool,
    /// What the agent of the thread opened does, shown once its rows replace the ones kept.
    opened_activity: Activity,
    pub list: ListState,
    /// What the list shows, in order.
    pub items: Rc<Vec<Item>>,
    keys: Vec<Key>,
    /// The row that ended the turn the server still reports as running.
    end_of_running_turn: Option<String>,
    /// Rows that just streamed in, which fade in, and when they came.
    fresh: HashMap<String, Instant>,
    /// Prose rows that grew while a reply streamed, and when.
    grown: HashMap<String, Instant>,
    /// Counts up whenever the rows change, so views that keep state about them can tell.
    pub version: u64,
}

impl Transcript {
    pub fn new() -> Self {
        // Rows are laid out this far past the viewport, as the Mac keeps views for them.
        let list = ListState::new(0, ListAlignment::Top, px(400.));
        list.set_follow_mode(FollowMode::Tail);
        Self {
            thread_id: None,
            rows: Rc::default(),
            pending: None,
            activity: Activity::default(),
            earlier: false,
            started_by: None,
            live: false,
            keeps_rows: false,
            opened_activity: Activity::default(),
            list,
            items: Rc::default(),
            keys: Vec::new(),
            end_of_running_turn: None,
            fresh: HashMap::new(),
            grown: HashMap::new(),
            version: 0,
        }
    }

    /// Whether what arrives streams in: rows fade in and the view glides after them.
    pub fn animates(&self) -> bool {
        self.live && self.activity.running && !crate::media::native::reduces_motion()
    }

    /// The rows that streamed in and still fade in, and when they came.
    pub fn fresh_rows(&self) -> HashMap<String, Instant> {
        self.fresh.iter().filter(|(_, at)| at.elapsed() < FADE).map(|(id, at)| (id.clone(), *at)).collect()
    }

    /// The prose rows that grew while a reply streamed, and when, while what they grew by fades in.
    pub fn grown_rows(&self) -> HashMap<String, Instant> {
        self.grown.iter().filter(|(_, at)| at.elapsed() < FADE).map(|(id, at)| (id.clone(), *at)).collect()
    }

    /// Whether a time on screen goes on: the turn's, or that of agents that work.
    pub fn ticks(&self) -> bool {
        self.activity.running
            || self.rows.iter().any(|model| match &model.row.kind {
                RowKind::Agent { agent } => {
                    agent.at.is_none() && matches!(agent.state, AgentState::Working | AgentState::Waiting)
                }
                RowKind::Agents { started_at, .. } => started_at.is_some(),
                _ => false,
            })
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.pending.is_none()
    }

    /// The turns that changed files, the first one first.
    pub fn turns(&self) -> Vec<TurnChange> {
        self.rows
            .iter()
            .filter_map(|model| match &model.row.kind {
                RowKind::Changes { files, at, .. } => {
                    Some(TurnChange { id: model.row.item.clone(), at: *at, files: *files })
                }
                _ => None,
            })
            .collect()
    }

    /// Whether the line that says the agent is at work shows. The row that ends a turn replaces
    /// it right away, a moment before the server says that the agent stopped, unless agents it
    /// started work on.
    pub fn shows_working(&self) -> bool {
        if !self.activity.running {
            return false;
        }
        self.activity.agents > 0
            || self.end_of_running_turn.is_none()
            || self.rows.last().map(|row| row.row.id.as_str()) != self.end_of_running_turn.as_deref()
    }

    /// Where the working line goes, counting the message on its way as the last row: above the
    /// messages that wait for the agent, which are the last rows, the one on its way among them.
    /// Without them it goes under every row.
    fn working_index(&self) -> usize {
        let pending_queued = self.pending.as_ref().map(|pending| pending.is_queued());
        let count = self.rows.len() + usize::from(pending_queued.is_some());
        let queued = |index: usize| match self.rows.get(index) {
            Some(row) => row.is_queued(),
            None => pending_queued == Some(true),
        };
        let end = count - usize::from(pending_queued == Some(false));
        let mut index = end;
        while index > 0 && queued(index - 1) {
            index -= 1;
        }
        if index == end { count } else { index }
    }

    /// Lays the items out again and has the list measure the ones that changed: those with a
    /// new key, and the rows in `changed`.
    fn sync(&mut self, changed: &HashSet<String>) {
        self.sync_with(changed, false);
    }

    fn sync_with(&mut self, changed: &HashSet<String>, pending_changed: bool) {
        let working = self.shows_working().then(|| self.working_index());
        let mut items = vec![Item::Top];
        let mut keys = vec![Key::Top];
        if self.started_by.is_some() && !self.earlier {
            items.push(Item::StartedBy(false));
            keys.push(Key::StartedBy(false));
        }
        items.reserve(self.rows.len() + 3);
        keys.reserve(self.rows.len() + 3);
        for (index, row) in self.rows.iter().enumerate() {
            if working == Some(index) {
                items.push(Item::Working);
                keys.push(Key::Working);
            }
            items.push(Item::Row(index));
            keys.push(Key::Row(row.row.id.clone()));
        }
        if self.pending.is_some() {
            if working == Some(self.rows.len()) {
                items.push(Item::Working);
                keys.push(Key::Working);
            }
            items.push(Item::Pending);
            keys.push(Key::Pending);
        }
        if working.is_some() && working == Some(self.rows.len() + usize::from(self.pending.is_some())) {
            items.push(Item::Working);
            keys.push(Key::Working);
        }
        if self.started_by.is_some() {
            items.push(Item::StartedBy(true));
            keys.push(Key::StartedBy(true));
        }
        items.push(Item::Bottom);
        keys.push(Key::Bottom);
        let same = |old: &Key, new: &Key| {
            old == new
                && !matches!(new, Key::Row(id) if changed.contains(id))
                && !(pending_changed && *new == Key::Pending)
        };
        let prefix = self.keys.iter().zip(&keys).take_while(|(old, new)| same(old, new)).count();
        let room = self.keys.len().min(keys.len()) - prefix;
        let suffix =
            self.keys.iter().rev().zip(keys.iter().rev()).take(room).take_while(|(old, new)| same(old, new)).count();
        let old_end = self.keys.len() - suffix;
        let new_count = keys.len() - suffix - prefix;
        if old_end > prefix || new_count > 0 {
            self.list.splice(prefix..old_end, new_count);
        }
        self.items = Rc::new(items);
        self.keys = keys;
        self.version += 1;
    }

    /// Starts showing another thread, or none. Its rows follow from the core. With `keeping_rows`
    /// the rows shown stay until they do, so that the view isn't empty in between.
    pub fn begin(&mut self, thread_id: Option<String>, live: bool, keeping_rows: bool) {
        self.thread_id = thread_id;
        self.pending = None;
        self.earlier = false;
        self.live = live;
        self.opened_activity = Activity::default();
        self.keeps_rows = keeping_rows && !self.rows.is_empty();
        if self.keeps_rows {
            return;
        }
        self.clear();
    }

    /// The thread's rows aren't coming: the view lets go of the ones it kept.
    pub fn drop_kept_rows(&mut self) {
        if std::mem::take(&mut self.keeps_rows) {
            self.clear();
            self.activity = std::mem::take(&mut self.opened_activity);
        }
    }

    fn clear(&mut self) {
        self.rows = Rc::default();
        self.activity = Activity::default();
        self.end_of_running_turn = None;
        self.fresh.clear();
        self.grown.clear();
        self.keys.clear();
        self.items = Rc::default();
        self.list.reset(0);
        self.list.set_follow_mode(FollowMode::Tail);
        self.version += 1;
    }

    /// The message on screen now belongs to a thread; its rows are about to arrive.
    pub fn adopt(&mut self, thread_id: String) {
        self.thread_id = Some(thread_id);
    }

    pub fn set_live(&mut self, live: bool) {
        self.live = live;
    }

    pub fn apply(&mut self, reset: bool, start: usize, remove: usize, rows: Vec<Arc<RowModel>>, earlier: bool) {
        self.earlier = earlier;
        // The server has the message now, so the copy shown while it travelled goes.
        let sent = self.pending.is_some() && rows.iter().any(|row| row.is_sent_message());
        if sent {
            self.pending = None;
        }
        let changed: HashSet<String> = rows.iter().map(|row| row.row.id.clone()).collect();
        if reset {
            if std::mem::take(&mut self.keeps_rows) {
                self.activity = std::mem::take(&mut self.opened_activity);
            }
            self.rows = Rc::new(rows);
            self.end_of_running_turn = None;
            self.fresh.clear();
            self.grown.clear();
            self.keys.clear();
            self.list.reset(0);
            self.list.set_follow_mode(FollowMode::Tail);
            self.sync(&changed);
            self.list.scroll_to_end();
            return;
        }
        if start + remove > self.rows.len() {
            return;
        }
        if self.animates() {
            self.mark_fresh(start, remove, &rows);
        }
        Rc::make_mut(&mut self.rows).splice(start..start + remove, rows);
        if self.activity.running
            && let Some(last) = self.rows.last()
            && matches!(last.row.kind, RowKind::TurnEnd { .. })
            && changed.contains(&last.row.id)
        {
            self.end_of_running_turn = Some(last.row.id.clone());
        }
        self.sync(&changed);
    }

    /// Notes the rows that streamed in and the prose that grew, which fade in.
    fn mark_fresh(&mut self, start: usize, remove: usize, rows: &[Arc<RowModel>]) {
        let now = Instant::now();
        self.fresh.retain(|_, at| at.elapsed() < FADE);
        self.grown.retain(|_, at| at.elapsed() < FADE);
        let known: HashSet<&str> = self.rows[start..start + remove].iter().map(|row| row.row.id.as_str()).collect();
        for row in rows {
            if !known.contains(row.row.id.as_str()) {
                if !row.is_sent_message() {
                    self.fresh.insert(row.row.id.clone(), now);
                }
            } else if row.prose.is_some() {
                self.grown.insert(row.row.id.clone(), now);
            }
        }
    }

    pub fn set_pending(&mut self, pending: Option<RowModel>) {
        self.pending = pending.map(Arc::new);
        self.sync_with(&HashSet::new(), true);
        if self.pending.is_some() {
            self.list.scroll_to_end();
        }
    }

    pub fn set_activity(&mut self, activity: Activity) {
        if self.keeps_rows {
            self.opened_activity = activity;
            return;
        }
        if !activity.running || activity.started_at != self.activity.started_at {
            self.end_of_running_turn = None;
        }
        self.activity = activity;
        self.sync(&HashSet::new());
    }

    pub fn set_spans(&mut self, row_id: &str, spans: motile_core::render::highlight::Spans) {
        let Some(index) = self.rows.iter().rposition(|model| model.row.id == row_id) else { return };
        let RowKind::Code { language, code, .. } = &self.rows[index].row.kind else { return };
        let mut row = self.rows[index].row.clone();
        row.kind = RowKind::Code { language: language.clone(), code: code.clone(), spans: Some(spans) };
        Rc::make_mut(&mut self.rows)[index] = Arc::new(RowModel::new(row));
        let mut changed = HashSet::new();
        changed.insert(row_id.to_string());
        self.sync(&changed);
    }

    /// Applies a program's patch to its row, wherever the row is.
    pub fn patch_ui(&mut self, row_id: &str, ops: &[motile_core::render::openui::UiOp]) {
        let Some(index) = self.rows.iter().rposition(|model| model.row.id == row_id) else { return };
        let mut row = self.rows[index].row.clone();
        let RowKind::Ui { ui } = &mut row.kind else { return };
        super::openui::patch::apply(&mut ui.tree, &mut ui.errors, ops);
        Rc::make_mut(&mut self.rows)[index] = Arc::new(RowModel::new(row));
        let mut changed = HashSet::new();
        changed.insert(row_id.to_string());
        self.sync(&changed);
    }

    /// Has the list measure the room under the last row again, which changed.
    pub fn bottom_changed(&self) {
        let Some(last) = self.items.len().checked_sub(1) else { return };
        self.list.splice(last..last + 1, 1);
    }

    /// Names the thread whose agent started this one, which the list leads back to.
    pub fn set_started_by(&mut self, started_by: Option<String>) {
        if self.keeps_rows || self.started_by == started_by {
            return;
        }
        self.started_by = started_by;
        self.sync(&HashSet::new());
    }

    pub fn row_index(&self, row_id: &str) -> Option<usize> {
        self.rows.iter().position(|model| model.row.id == row_id)
    }

    /// The row whose time and copy button the pointer over the item reveals: the message or the
    /// fold under it, or the end of the reply under it.
    pub fn meta_row(&self, item: &Item) -> Option<String> {
        let index = match item {
            Item::Row(index) => *index,
            Item::Pending => return self.pending.as_ref().filter(|row| row.is_user()).map(|row| row.row.id.clone()),
            Item::Working | Item::Top | Item::Bottom | Item::StartedBy(_) => return None,
        };
        let under = self.rows.get(index)?;
        if under.is_user() || matches!(under.row.kind, RowKind::Fold { .. }) {
            return Some(under.row.id.clone());
        }
        for row in &self.rows[index..] {
            if row.is_user() {
                return None;
            }
            if matches!(row.row.kind, RowKind::TurnEnd { .. }) {
                return Some(row.row.id.clone());
            }
        }
        None
    }
}
