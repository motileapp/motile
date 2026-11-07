//! Finding in the open thread, as a browser finds in a page: the bar's query, and what the core
//! found for it. `Find.swift`.

use std::collections::HashMap;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Command;
use motile_core::find::FoundRow;

use super::Store;
use crate::transcript::find::Marks;

#[derive(Default)]
pub struct FindState {
    /// Whether the bar shows.
    pub shown: bool,
    /// Whether the transcript shows what was found: with the bar, or alone after the sidebar's
    /// search opened a thread at a message.
    pub active: bool,
    pub query: String,
    pub count: usize,
    pub current: Option<usize>,
    /// Counts up when the bar's field should take the keys with what it has selected.
    pub focus_requests: u64,
}

impl FindState {
    /// "3/41", or "0/0" when nothing matches.
    pub fn label(&self) -> String {
        if self.query.trim().is_empty() {
            return String::new();
        }
        match self.current {
            Some(current) if self.count > 0 => format!("{}/{}", current + 1, self.count),
            _ => "0/0".into(),
        }
    }
}

impl Store {
    fn open_transcript_thread(&self, cx: &App) -> Option<String> {
        self.selected_thread()?;
        self.transcript.read(cx).thread_id.clone()
    }

    /// Opens the bar, with `query` in it when given: the selected text, or what the sidebar searched.
    /// It finds from the message `item_id` when that has a match.
    pub fn open_find(&mut self, query: Option<String>, item_id: Option<String>, cx: &mut Context<Self>) {
        let Some(thread_id) = self.open_transcript_thread(cx) else { return };
        if let Some(query) = query {
            self.find.query = query;
        }
        self.find.shown = true;
        self.find.active = true;
        self.find.focus_requests += 1;
        self.send_find(thread_id, item_id, cx);
    }

    /// Lights what the sidebar searched for in the open thread and scrolls to it in the message
    /// `item_id`, without the bar. ⌘F then opens the bar on it, Esc puts it away.
    pub fn light_found(&mut self, query: String, item_id: String, cx: &mut Context<Self>) {
        let Some(thread_id) = self.open_transcript_thread(cx) else { return };
        self.find.query = query;
        self.find.active = true;
        self.send_find(thread_id, Some(item_id), cx);
    }

    pub fn close_find(&mut self, cx: &mut Context<Self>) {
        if !self.find.active {
            return;
        }
        self.find.shown = false;
        self.find.active = false;
        self.find.count = 0;
        self.find.current = None;
        self.transcript.update(cx, |transcript, cx| {
            transcript.set_found(HashMap::new());
            cx.notify();
        });
        let Some(thread_id) = self.transcript.read(cx).thread_id.clone() else { return };
        self.send(Command::FindEnd { thread_id });
    }

    pub fn set_find_query(&mut self, query: String, cx: &mut Context<Self>) {
        if self.find.query == query {
            return;
        }
        self.find.query = query;
        let Some(thread_id) = self.open_transcript_thread(cx) else { return };
        self.send_find(thread_id, None, cx);
    }

    /// Moves to the next match, or with `back` the one before. The bar opens first when it is closed.
    pub fn step_find(&mut self, back: bool, cx: &mut Context<Self>) {
        let Some(thread_id) = self.open_transcript_thread(cx) else { return };
        if !self.find.shown {
            return self.open_find(None, None, cx);
        }
        self.send(Command::FindStep { thread_id, back });
    }

    /// Scrolls the thread to the message.
    pub fn reveal_item(&mut self, thread_id: String, item_id: String) {
        self.send(Command::RevealItem { thread_id, item_id });
    }

    fn send_find(&mut self, thread_id: String, item_id: Option<String>, cx: &App) {
        let from = self.transcript.read(cx).viewport();
        self.send(Command::Find { thread_id, query: self.find.query.clone(), from, item_id });
    }

    /// What the core found, and the matches in the rows the transcript has. One for a query the bar
    /// no longer has is passed over.
    pub(super) fn apply_found(
        &mut self,
        thread_id: String,
        query: String,
        count: usize,
        current: Option<usize>,
        rows: Vec<FoundRow>,
        cx: &mut Context<Self>,
    ) {
        if !self.find.active
            || self.find.query != query
            || self.transcript.read(cx).thread_id.as_deref() != Some(&thread_id)
        {
            return;
        }
        self.find.count = count;
        self.find.current = current;
        let marks: HashMap<String, Marks> = rows.into_iter().map(|row| (row.row_id.clone(), row.into())).collect();
        self.transcript.update(cx, |transcript, cx| {
            transcript.set_found(marks);
            cx.notify();
        });
    }

    pub(super) fn apply_reveal(
        &mut self,
        thread_id: String,
        row_id: String,
        range: Option<[u32; 2]>,
        cx: &mut Context<Self>,
    ) {
        if self.transcript.read(cx).thread_id.as_deref() != Some(&thread_id) {
            return;
        }
        self.transcript.update(cx, |transcript, cx| {
            transcript.reveal(row_id, range);
            cx.notify();
        });
    }
}
