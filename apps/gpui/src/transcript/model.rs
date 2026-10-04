//! The rows of a transcript, kept in step with the core. Rows are made ready to draw on the
//! core's thread (`prepare`), so the main thread only splices them in.

use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

use gpui_kit::*;
use motile_core::api::Event;
use motile_core::render::rows::{Row, RowKind};

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

    /// A user message shown the moment it is sent, before the server has it.
    pub fn pending(text: String, attachments: Vec<AttachedFile>) -> Self {
        let attachments = attachments
            .into_iter()
            .map(|file| motile_core::render::rows::Attached {
                name: file.name,
                media: file.media,
                video: file.video,
                poster: file.poster,
            })
            .collect();
        Self::new(Row {
            id: "pending".into(),
            item: "pending".into(),
            nested: false,
            kind: RowKind::User { text, attachments, at: crate::models::now() },
        })
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
    /// The row of the transcript at that index.
    Row(usize),
    /// The line that says the agent works.
    Working,
    /// The message on its way to the server.
    Pending,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Key {
    Row(String),
    Working,
    Pending,
}

/// The rows of the open thread, and the list that shows them: a row for each of them, the line
/// that says the agent works above the messages that wait for it, and the message on its way.
pub struct Transcript {
    pub thread_id: Option<String>,
    pub rows: Vec<Arc<RowModel>>,
    pub pending: Option<Arc<RowModel>>,
    pub activity: Activity,
    /// The thread has turns before the first row.
    pub earlier: bool,
    pub list: ListState,
    /// What the list shows, in order.
    pub items: Vec<Item>,
    keys: Vec<Key>,
    /// The row that ended the turn the server still reports as running.
    end_of_running_turn: Option<String>,
    /// Counts up whenever the rows change, so views that keep state about them can tell.
    pub version: u64,
}

impl Transcript {
    pub fn new() -> Self {
        let list = ListState::new(0, ListAlignment::Top, px(1000.));
        list.set_follow_mode(FollowMode::Tail);
        Self {
            thread_id: None,
            rows: Vec::new(),
            pending: None,
            activity: Activity::default(),
            earlier: false,
            list,
            items: Vec::new(),
            keys: Vec::new(),
            end_of_running_turn: None,
            version: 0,
        }
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

    /// Where the working line goes: above the messages that wait for the agent, which are the
    /// last rows the core sends.
    fn working_index(&self) -> usize {
        let mut index = self.rows.len();
        while index > 0 && self.rows[index - 1].is_queued() {
            index -= 1;
        }
        index
    }

    /// Lays the items out again and has the list measure the ones that changed: those with a
    /// new key, and the rows in `changed`.
    fn sync(&mut self, changed: &HashSet<String>) {
        self.sync_with(changed, false);
    }

    fn sync_with(&mut self, changed: &HashSet<String>, pending_changed: bool) {
        let working = self.shows_working().then(|| self.working_index());
        let mut items = Vec::with_capacity(self.rows.len() + 2);
        let mut keys = Vec::with_capacity(self.rows.len() + 2);
        for (index, row) in self.rows.iter().enumerate() {
            if working == Some(index) {
                items.push(Item::Working);
                keys.push(Key::Working);
            }
            items.push(Item::Row(index));
            keys.push(Key::Row(row.row.id.clone()));
        }
        if working == Some(self.rows.len()) {
            items.push(Item::Working);
            keys.push(Key::Working);
        }
        if self.pending.is_some() {
            items.push(Item::Pending);
            keys.push(Key::Pending);
        }
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
        self.items = items;
        self.keys = keys;
        self.version += 1;
    }

    /// Starts showing another thread, or none. Its rows follow from the core.
    pub fn begin(&mut self, thread_id: Option<String>) {
        self.thread_id = thread_id;
        self.rows.clear();
        self.pending = None;
        self.activity = Activity::default();
        self.earlier = false;
        self.end_of_running_turn = None;
        self.keys.clear();
        self.items.clear();
        self.list.reset(0);
        self.list.set_follow_mode(FollowMode::Tail);
        self.version += 1;
    }

    /// The message on screen now belongs to a thread; its rows are about to arrive.
    pub fn adopt(&mut self, thread_id: String) {
        self.thread_id = Some(thread_id);
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
            self.rows = rows;
            self.end_of_running_turn = None;
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
        self.rows.splice(start..start + remove, rows);
        if self.activity.running
            && let Some(last) = self.rows.last()
            && matches!(last.row.kind, RowKind::TurnEnd { .. })
            && changed.contains(&last.row.id)
        {
            self.end_of_running_turn = Some(last.row.id.clone());
        }
        self.sync(&changed);
    }

    pub fn set_pending(&mut self, pending: Option<RowModel>) {
        self.pending = pending.map(Arc::new);
        self.sync_with(&HashSet::new(), true);
        if self.pending.is_some() {
            self.list.scroll_to_end();
        }
    }

    pub fn set_activity(&mut self, activity: Activity) {
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
        self.rows[index] = Arc::new(RowModel::new(row));
        let mut changed = HashSet::new();
        changed.insert(row_id.to_string());
        self.sync(&changed);
    }

    pub fn row_index(&self, row_id: &str) -> Option<usize> {
        self.rows.iter().position(|model| model.row.id == row_id)
    }
}
