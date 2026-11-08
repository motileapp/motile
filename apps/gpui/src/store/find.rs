//! Finding in the open thread, as a browser finds in a page: the bar's query, and what the core
//! found for it. `Find.swift`.

use std::collections::HashMap;
use std::time::Duration;

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
    /// Counts up each time what the sidebar searched is lit, and when finding ends, so that a
    /// light fades only if it is still the one lit.
    lit: u64,
}

/// How long what the sidebar searched stays lit in the thread it opened, unless the bar opens on it.
const LIT_FOR: Duration = Duration::from_secs(5);

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
        self.send_find(thread_id, item_id, true, cx);
    }

    /// Lights what the sidebar searched for in the open thread, without the bar, and scrolls to it
    /// in the message `item_id`, or to the message when the words are apart there. A server that
    /// doesn't say the message has it scroll to the first match. It fades after a while unless
    /// the bar opened on it; ⌘F opens the bar on it, Esc puts it away.
    pub fn light_found(&mut self, query: String, item_id: Option<String>, cx: &mut Context<Self>) {
        let Some(thread_id) = self.open_transcript_thread(cx) else { return };
        if query.is_empty() {
            return;
        }
        self.find.query = query;
        self.find.active = true;
        self.find.lit += 1;
        let lit = self.find.lit;
        self.send_find(thread_id, item_id, false, cx);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LIT_FOR).await;
            let _ = this.update(cx, |store, cx| {
                if store.find.lit != lit || store.find.shown {
                    return;
                }
                store.close_find(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn close_find(&mut self, cx: &mut Context<Self>) {
        if !self.find.active {
            return;
        }
        self.find.lit += 1;
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
        self.send_find(thread_id, None, true, cx);
    }

    /// Moves to the next match, or with `back` the one before. The bar opens first when it is closed.
    pub fn step_find(&mut self, back: bool, cx: &mut Context<Self>) {
        let Some(thread_id) = self.open_transcript_thread(cx) else { return };
        if !self.find.shown {
            return self.open_find(None, None, cx);
        }
        self.send(Command::FindStep { thread_id, back });
    }

    /// Asks the core to find the query, from the first line the transcript shows under the bar when
    /// `from_viewport`, else from the thread's start.
    fn send_find(&mut self, thread_id: String, item_id: Option<String>, from_viewport: bool, cx: &App) {
        let cover = crate::thread::find_bar::COVER;
        let from = if from_viewport { self.transcript.read(cx).find_from(cover) } else { None };
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
