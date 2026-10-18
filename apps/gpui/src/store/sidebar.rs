//! What the sidebar keeps that no thread owns: the search through the threads and what the
//! servers found in their messages, the thread it brings into sight, moving threads among the
//! active ones, and the offer to undo marking them done.

use std::collections::HashMap;
use std::time::Duration;

use gpui_kit::prelude::*;
use motile_core::api::Command;
use motile_core::search::MatchView;
use motile_protocol::wire::{Agent, ThreadChange};

use super::{ListedDraft, Selection, Store};
use crate::models::{Server, ThreadInfo, now};

/// How long the offer to undo stays once the pointer isn't on it.
const UNDO_FOR: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct SidebarState {
    /// The thread the sidebar brings into sight, opening Done if it is there: the one its pull
    /// request's end last marked done, or the one opened from the search. It stays until another
    /// is opened.
    pub revealed_thread_id: Option<String>,
    /// What the threads are searched for. Their messages are searched on the servers.
    pub search: String,
    pub message_search: MessageSearch,
    /// The pointer is on the offer to undo, which stays while it is.
    undo_held: bool,
}

/// What the servers found in the threads' messages for the search, by thread.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct MessageSearch {
    pub query: String,
    /// Some server may still answer.
    pub searching: bool,
    pub matches: HashMap<String, MatchView>,
}

impl MessageSearch {
    pub fn new(query: String, searching: bool, matches: Vec<MatchView>) -> Self {
        let mut found = HashMap::new();
        for matched in matches {
            found.entry(matched.thread_id.clone()).or_insert(matched);
        }
        Self { query, searching, matches: found }
    }
}

/// A draft or a thread the search found, with the message it was found by when its title, its
/// project and its pull request don't have what was searched for.
#[derive(Clone, Debug)]
pub enum SearchResult {
    Draft(ListedDraft),
    Thread(Box<ThreadInfo>, Option<MatchView>),
}

impl SearchResult {
    pub fn id(&self) -> String {
        match self {
            SearchResult::Draft(listed) => format!("draft:{}", listed.draft.id),
            SearchResult::Thread(thread, _) => thread.id.clone(),
        }
    }

    pub fn selection(&self) -> Selection {
        match self {
            SearchResult::Draft(listed) => Selection::Draft(listed.draft.id.clone()),
            SearchResult::Thread(thread, _) => Selection::Thread(thread.id.clone()),
        }
    }

    pub fn thread(&self) -> Option<&ThreadInfo> {
        match self {
            SearchResult::Draft(_) => None,
            SearchResult::Thread(thread, _) => Some(thread.as_ref()),
        }
    }
}

fn contains(text: &str, search: &str) -> bool {
    text.to_lowercase().contains(&search.to_lowercase())
}

impl Store {
    /// What the threads are searched for. The servers are asked about their messages; the core
    /// waits for the typing to stop.
    pub fn set_thread_search(&mut self, search: String) {
        if self.sidebar.search == search {
            return;
        }
        self.sidebar.search = search.clone();
        self.send(Command::SearchThreads { query: search });
    }

    pub(super) fn apply_thread_search(&mut self, search: MessageSearch) {
        if self.sidebar.message_search != search {
            self.sidebar.message_search = search;
        }
    }

    fn trimmed_search(&self) -> &str {
        self.sidebar.search.trim()
    }

    pub fn is_searching_threads(&self) -> bool {
        !self.trimmed_search().is_empty()
    }

    /// Whether a thread's title, project or pull request matches what is being searched for.
    pub fn matches(&self, thread: &ThreadInfo, search: &str) -> bool {
        if search.is_empty() {
            return true;
        }
        let project = self.project(Some(&thread.project_id));
        let pull_request = project.and_then(|project| project.pull_request(thread));
        let names = [
            Some(thread.title.clone()),
            project.map(|project| project.name.clone()),
            pull_request.as_ref().map(|pull_request| format!("#{}", pull_request.number)),
            pull_request.as_ref().map(|pull_request| pull_request.title.clone()),
            pull_request.as_ref().map(|pull_request| pull_request.url.clone()),
        ];
        names.into_iter().flatten().any(|name| contains(&name, search))
    }

    /// The drafts whose text or project matches what is being searched for.
    fn searched(&self, drafts: Vec<ListedDraft>, search: &str) -> Vec<ListedDraft> {
        if search.is_empty() {
            return drafts;
        }
        drafts
            .into_iter()
            .filter(|listed| {
                let project = self.project(listed.draft.project_id.as_deref()).map(|project| project.name.as_str());
                contains(&listed.preview, search) || contains(project.unwrap_or_default(), search)
            })
            .collect()
    }

    /// What the sidebar lists while it searches, in one list: the drafts, the threads found by
    /// their title, project or pull request, active before done, then the ones found by a
    /// message. What the servers find later only ever comes below.
    pub fn search_results(&self) -> Vec<SearchResult> {
        let search = self.trimmed_search();
        if search.is_empty() {
            return Vec::new();
        }
        let threads = || self.active_threads().iter().chain(self.done_threads());
        let named: Vec<&ThreadInfo> = threads().filter(|thread| self.matches(thread, search)).collect();
        let found = &self.sidebar.message_search.matches;
        let by_message = threads()
            .filter(|thread| found.contains_key(&thread.id) && !named.iter().any(|named| named.id == thread.id));
        let mut results: Vec<SearchResult> =
            self.searched(self.listed_drafts(), search).into_iter().map(SearchResult::Draft).collect();
        results.extend(named.iter().map(|thread| SearchResult::Thread(Box::new((*thread).clone()), None)));
        results.extend(
            by_message.map(|thread| SearchResult::Thread(Box::new(thread.clone()), found.get(&thread.id).cloned())),
        );
        results
    }

    /// Opens what the search found and ends the search, then shows it in the sidebar.
    pub fn open_search_result(&mut self, result: &SearchResult, cx: &mut Context<Self>) {
        self.select(result.selection(), cx);
        self.set_thread_search(String::new());
        self.sidebar.revealed_thread_id = result.thread().map(|thread| thread.id.clone());
    }

    /// Whether the sidebar offers to start a thread: there is somewhere to start it, and nothing
    /// above the offer.
    pub fn offers_new_thread(&self, drafts: &[ListedDraft]) -> bool {
        if self.is_searching_threads() || !drafts.is_empty() || !self.active_threads().is_empty() {
            return false;
        }
        !self.recent_projects().is_empty() || self.live_no_projects().next().is_some()
    }

    /// The agent a draft starts with: its model's, or its server's first model's.
    pub fn agent_of(&self, draft: &super::ThreadDraft) -> Option<Agent> {
        let project = self.project(draft.project_id.as_deref());
        let server = self.server(project.map(|project| project.server_id.as_str())).or(self.live_servers().next())?;
        let model = server.models.iter().find(|model| Some(&model.id) == draft.model.as_ref());
        model.or(server.models.first()).map(|model| model.agent)
    }

    /// The thread's model as the composer shows it, with the account when its agent has more
    /// than one.
    pub fn model_label(&self, thread: &ThreadInfo) -> String {
        let account = Some(thread.agent_account.as_str()).filter(|account| !account.is_empty());
        let Some(server) = self.server(Some(&thread.server_id)) else { return self.bare_model(thread) };
        let offered: Vec<_> = server.models.iter().filter(|model| model.agent == thread.agent).cloned().collect();
        let Some(model) = Server::model(thread.model.as_deref(), account, &offered) else {
            return self.bare_model(thread);
        };
        server.label(model, account)
    }

    fn bare_model(&self, thread: &ThreadInfo) -> String {
        thread.model.clone().unwrap_or_else(|| crate::models::agent_name(thread.agent).to_string())
    }

    /// Whether the user can move the thread among the active ones.
    pub fn moves(&self, thread: &ThreadInfo) -> bool {
        !thread.is_done()
            && !self.on_removed_server(thread)
            && self.server(Some(&thread.server_id)).is_some_and(Server::moves_threads)
    }

    /// Puts the thread where `index` is among the sidebar's rows without it: `rows` has the id
    /// of each row's active thread, or `None` for a row that isn't one.
    pub fn move_thread(&mut self, thread: &ThreadInfo, index: usize, rows: &[Option<String>]) {
        let others: Vec<&Option<String>> = rows.iter().filter(|row| row.as_deref() != Some(&thread.id)).collect();
        let neighbour = |at: Option<usize>| {
            let id = at.and_then(|at| others.get(at)).and_then(|row| row.as_ref())?;
            self.threads.get(id)
        };
        let (above, below) = (neighbour(index.checked_sub(1)), neighbour(Some(index)));
        let Some(position) = moved_position(thread.position(), above, below, now()) else { return };
        if !self.moves(thread) {
            return;
        }
        let change = ThreadChange { position: Some(position), ..Default::default() };
        self.update_thread(thread, change, |thread| thread.thread.position = position);
    }

    /// Keeps the offer to undo while the pointer is on it, and gives it its full time again once
    /// the pointer leaves.
    pub fn hold_undo(&mut self, held: bool, cx: &mut Context<Self>) {
        if self.sidebar.undo_held == held {
            return;
        }
        self.sidebar.undo_held = held;
        self.schedule_undo_dismissal(cx);
    }

    /// Takes the offer to undo away; the pointer that held it holds nothing now.
    pub(super) fn clear_undo(&mut self) -> Option<super::UndoNotice> {
        self.sidebar.undo_held = false;
        self.undo_task = None;
        self.undo.take()
    }

    pub(super) fn schedule_undo_dismissal(&mut self, cx: &mut Context<Self>) {
        self.undo_task = None;
        if self.undo.is_none() || self.sidebar.undo_held {
            return;
        }
        self.undo_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(UNDO_FOR).await;
            let _ = this.update(cx, |store, cx| {
                store.clear_undo();
                cx.notify();
            });
        }));
    }

    /// Opens the sidebar's servers on an update one of them needs: once for each release, or
    /// whenever the user looked for one.
    pub fn show_server_updates(&mut self, asked: bool) {
        let Some(latest) = self.updater.latest.clone() else { return };
        if !self.servers.iter().any(|server| self.is_outdated(server)) {
            return;
        }
        if !asked && self.prefs.string("sidebar.serversOpenedFor").as_deref() == Some(latest.as_str()) {
            return;
        }
        self.prefs.set("sidebar.serversOpenedFor", latest);
        self.prefs.set("sidebar.serversExpanded", true);
    }
}

/// Where a thread goes between two active ones, or at the top or the bottom when one of them is
/// missing. `None` when it stays where it is.
fn moved_position(current: f64, above: Option<&ThreadInfo>, below: Option<&ThreadInfo>, at: f64) -> Option<f64> {
    let position = match (above, below) {
        (None, None) => return None,
        (None, Some(below)) => at.max(below.position() + 1.),
        (Some(above), None) => above.position() - 1.,
        (Some(above), Some(below)) => (above.position() + below.position()) / 2.,
    };
    (position != current).then_some(position)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(position: f64) -> ThreadInfo {
        let value = serde_json::json!({
            "id": "t", "title": "", "project_id": "", "cwd": "", "agent": "claude", "model": null,
            "effort": null, "access": "full", "created_at": 1.0, "updated_at": 1.0, "done_at": null,
            "position": position, "running": false, "needs_approval": false, "turn_ended_at": null, "rev": 0,
        });
        ThreadInfo { thread: serde_json::from_value(value).unwrap(), server_id: String::new(), unread: false }
    }

    #[test]
    fn a_moved_thread_goes_between_its_neighbours() {
        let (high, low) = (thread(30.), thread(10.));
        assert_eq!(moved_position(5., Some(&high), Some(&low), 100.), Some(20.));
        assert_eq!(moved_position(5., None, Some(&high), 100.), Some(100.));
        assert_eq!(moved_position(5., None, Some(&high), 20.), Some(31.));
        assert_eq!(moved_position(5., Some(&low), None, 100.), Some(9.));
        assert_eq!(moved_position(20., Some(&high), Some(&low), 100.), None);
        assert_eq!(moved_position(5., None, None, 100.), None);
    }
}
