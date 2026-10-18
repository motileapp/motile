//! The threads and drafts: opening them, sending, and changing a thread's settings.

use std::collections::HashMap;

use gpui_kit::prelude::*;
use motile_core::api::Command;
use motile_protocol::wire::{
    Access, AgentAccount, CopiedThread, Interruption, ModelInfo, NewThread, NewWorktree, Request, ThreadChange,
};

use super::{Continuation, PanelPage, Selection, SendingDraft, Store, ThreadDraft, UndoNotice};
use crate::models::*;
use crate::transcript::model::RowModel;

impl Store {
    /// Whether a message sent while the agent works steers the turn that runs instead of
    /// waiting for it.
    pub const STEERS_KEY: &str = "send.steers";

    /// It steers until the setting says otherwise.
    pub fn steers(&self) -> bool {
        self.prefs.get(Self::STEERS_KEY).unwrap_or(true)
    }

    pub(super) fn apply_threads(&mut self, new: Vec<ThreadInfo>, server_id: &str, cx: &mut Context<Self>) {
        let mut kept: HashMap<String, ThreadInfo> =
            self.threads.drain().filter(|(_, thread)| thread.server_id != server_id).collect();
        for thread in new {
            kept.insert(thread.id.clone(), thread);
        }
        self.set_threads(kept);
        if let Selection::Thread(id) = &self.selection
            && !self.threads.contains_key(id)
        {
            self.open_empty_draft(cx);
        }
        self.mark_open_thread_seen();
        self.restore_selection(cx);
    }

    pub(super) fn upsert(&mut self, thread: ThreadInfo) {
        let before = self.threads.get(&thread.id).cloned();
        if before.as_ref() == Some(&thread) {
            return;
        }
        self.set_thread(Some(thread.clone()), &thread.id);
        self.mark_open_thread_seen();
        let Some(before) = before else { return };
        if !before.is_done() && thread.is_done() && before.pull_request != thread.pull_request {
            self.sidebar.revealed_thread_id = Some(thread.id.clone());
        }
        if self.selection != Selection::Thread(thread.id.clone()) {
            return;
        }
        if before.turn_ended_at != thread.turn_ended_at || (before.running && !thread.running) {
            self.workspace_version += 1;
        }
    }

    /// A reply in the open thread has been seen once Motile is in front.
    pub(super) fn mark_open_thread_seen(&mut self) {
        if !self.app_active {
            return;
        }
        let Some(thread) = self.selected_thread().filter(|thread| thread.unread) else { return };
        let thread_id = thread.id.clone();
        self.send(Command::MarkSeen { thread_id });
    }

    pub(super) fn remove_thread(&mut self, id: &str, cx: &mut Context<Self>) {
        self.set_thread(None, id);
        self.forget_tabs(id);
        if self.selection == Selection::Thread(id.to_string()) {
            self.open_empty_draft(cx);
        }
    }

    pub fn select(&mut self, new: Selection, cx: &mut Context<Self>) {
        self.last_selection = None;
        self.shows_usage = false;
        let reopens = self.selected_thread().is_some() && self.open_thread_id.is_none();
        if new == self.selection && !reopens {
            return;
        }
        if let Some(open) = self.open_thread_id.take() {
            self.send(Command::CloseThread { thread_id: open });
        }
        let left = self.selected_draft().cloned();
        self.set_selection(new.clone());
        if self.sidebar.revealed_thread_id.as_ref().map(|id| Selection::Thread(id.clone())) != Some(new.clone()) {
            self.sidebar.revealed_thread_id = None;
        }
        if let Some(left) = left {
            if let Some(sending) = self.sending_drafts.get_mut(&left.id) {
                sending.left = true;
            } else if self.preview(&left).is_none() {
                self.remove_draft(&left.id);
            }
        }
        let preview = self.selected_draft().cloned().and_then(|draft| self.preview(&draft));
        self.set_opened_draft_preview(preview);
        self.activity = Activity::default();
        self.transcript_is_empty = true;
        self.right_panel.turns.clear();
        self.show_agents(cx);
        self.agents.clear();
        self.draft_version += 1;
        let thread = match &new {
            Selection::Thread(id) => self.threads.get(id).cloned(),
            Selection::Draft(_) => None,
        };
        let Some(thread) = thread else {
            self.transcript.update(cx, |transcript, cx| {
                transcript.begin(None, false, false);
                cx.notify();
            });
            self.show_sending_message(cx);
            self.ensure_draft_project();
            self.read_git(
                true,
                None::<fn(&mut Store, Vec<motile_protocol::wire::ChangedFile>, &mut Context<Store>)>,
                cx,
            );
            self.refresh_composer_models();
            return;
        };
        self.open(&thread, cx);
    }

    /// A draft whose first message is still on its way shows it as it was sent.
    fn show_sending_message(&mut self, cx: &mut Context<Self>) {
        let Some(sending) = self.selected_draft().and_then(|draft| self.sending_drafts.get(&draft.id)).cloned() else {
            return;
        };
        self.activity = sending.activity.clone();
        let pending = RowModel::pending(sending.text, sending.attachments, false);
        self.transcript.update(cx, |transcript, cx| {
            transcript.set_activity(sending.activity);
            transcript.set_pending(Some(pending));
            cx.notify();
        });
        self.transcript_is_empty = false;
    }

    fn open(&mut self, thread: &ThreadInfo, cx: &mut Context<Self>) {
        self.open_thread_id = Some(thread.id.clone());
        // What the cached thread says, so Stop and the monitoring strip show at once.
        self.activity = Activity { running: thread.running, monitoring: thread.monitoring, ..Activity::default() };
        let id = thread.id.clone();
        self.transcript.update(cx, |transcript, cx| {
            transcript.begin(Some(id.clone()), false, true);
            cx.notify();
        });
        let command = Command::OpenThread { server_id: thread.server_id.clone(), thread_id: thread.id.clone() };
        self.ask(command, move |store, result, cx| {
            if result.is_ok() || store.transcript.read(cx).thread_id.as_deref() != Some(&id) {
                return;
            }
            store.transcript.update(cx, |transcript, cx| {
                transcript.drop_kept_rows();
                cx.notify();
            });
        });
        self.send(Command::MarkSeen { thread_id: thread.id.clone() });
        self.read_git(true, None::<fn(&mut Store, Vec<motile_protocol::wire::ChangedFile>, &mut Context<Store>)>, cx);
    }

    /// What the toolbar button and ⌘N do: with one project there is nothing to pick and the draft
    /// opens at once, without one it starts without a project, and with more the panel asks which.
    pub fn new_thread(&mut self, cx: &mut Context<Self>) {
        if self.recent_projects().len() > 1 {
            return self.open_panel(PanelPage::Projects);
        }
        let first = self.first_live_project().map(|project| project.id.clone());
        self.start_new_thread(first, cx);
    }

    /// Opens a draft of a new thread that goes on from the open thread, whose server was removed,
    /// on the server of the project the user picks in it.
    pub fn continue_on_another_server(&mut self, cx: &mut Context<Self>) {
        let Some(thread) = self.selected_thread().filter(|thread| self.is_removed(&thread.server_id)) else { return };
        let from = Continuation {
            origin: thread.server_id.clone(),
            thread_id: thread.id.clone(),
            title: thread.title.clone(),
        };
        self.start_new_thread(None, cx);
        self.update_draft(|draft| draft.from = Some(from));
        self.composer_focus += 1;
    }

    /// The open draft no longer goes on from a removed server's thread.
    pub fn drop_continuation(&mut self) {
        self.update_draft(|draft| draft.from = None);
    }

    /// Opens an empty draft. The one that was open stays in the sidebar if something was written
    /// in it.
    pub fn start_new_thread(&mut self, project_id: Option<String>, cx: &mut Context<Self>) {
        self.open_empty_draft(cx);
        if let Some(project_id) = project_id {
            self.set_new_thread_project(Some(project_id), cx);
        }
    }

    /// Where the app goes when what was open is gone.
    pub(super) fn open_empty_draft(&mut self, cx: &mut Context<Self>) {
        let draft = self.empty_draft();
        self.select(Selection::Draft(draft.id), cx);
    }

    pub fn set_new_thread_project(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        self.move_draft(id);
        self.upload_to_composer_server();
        self.read_git(true, None::<fn(&mut Store, Vec<motile_protocol::wire::ChangedFile>, &mut Context<Store>)>, cx);
    }

    pub fn discard(&mut self, draft: &ThreadDraft, cx: &mut Context<Self>) {
        if self.is_sending(&draft.id) {
            return;
        }
        let was_open = self.selection == Selection::Draft(draft.id.clone());
        self.remove_draft(&draft.id);
        if !was_open {
            return;
        }
        let next = self.thread_drafts.iter().rev().find(|draft| !self.is_sending(&draft.id)).cloned();
        if let Some(next) = next {
            return self.select(Selection::Draft(next.id), cx);
        }
        if let Some(next) = self.active_threads().first().map(|thread| thread.id.clone()) {
            return self.select(Selection::Thread(next), cx);
        }
        self.open_empty_draft(cx);
    }

    pub fn can_send(&self) -> bool {
        if self.is_sending(&self.draft_key()) {
            return false;
        }
        let Some(server) = self.composer_server().filter(|server| server.connected()) else { return false };
        if self.selected_thread().is_none()
            && self.project(self.selected_draft().and_then(|draft| draft.project_id.as_deref())).is_none()
        {
            return false;
        }
        let attachments = self.attachments();
        if !attachments
            .iter()
            .all(|attachment| attachment.state == UploadState::Ready && attachment.server_id == server.id)
        {
            return false;
        }
        !self.draft().trim().is_empty() || !attachments.is_empty()
    }

    /// Why a message with these attachments can't be sent yet.
    pub fn attachments_hold(&self) -> Option<&'static str> {
        let attachments = self.attachments();
        if attachments.iter().any(|attachment| matches!(attachment.state, UploadState::Failed(_))) {
            return Some("Try the attachment that failed again, or remove it");
        }
        attachments
            .iter()
            .any(|attachment| attachment.state != UploadState::Ready)
            .then_some("Waiting for the attachments to upload")
    }

    /// Sends what is written. While the agent works, `alternate` sends it the other way than the
    /// settings say: queued instead of steering, or the other way round.
    pub fn send_message(&mut self, alternate: bool, cx: &mut Context<Self>) {
        if !self.can_send() {
            return;
        }
        let text = self.draft().trim().to_string();
        let attached = self.attachments();
        let key = self.draft_key();
        let paths: Vec<String> = attached.iter().filter_map(|attachment| attachment.path.clone()).collect();
        let existing = self.selected_thread().cloned();
        let steers = existing.is_some() && self.steers() != alternate;
        let command = if let Some(thread) = &existing {
            Command::Send {
                server_id: thread.server_id.clone(),
                thread_id: Some(thread.id.clone()),
                new_thread: None,
                text: text.clone(),
                attachments: paths,
                now: steers,
            }
        } else {
            let draft = self.selected_draft().cloned();
            let project = draft.as_ref().and_then(|draft| self.project(draft.project_id.as_deref()).cloned());
            let (Some(draft), Some(project), Some(model)) = (draft, project, self.composer_model()) else {
                self.fail("No agent is installed. Install Claude Code or Codex on your server and try again.");
                return;
            };
            let worktree = if self.draft_uses_worktree() {
                self.draft_base().map(|base| NewWorktree { base, branch: None })
            } else {
                None
            };
            let account =
                self.composer_account().map_or_else(|| default_account_id(model.agent).into(), |account| account.id);
            let new_thread = NewThread {
                project_id: project.id.clone(),
                agent: model.agent,
                agent_account: Some(account),
                model: Some(model.id.clone()),
                effort: self.composer_effort(),
                access: draft.access,
                worktree,
                from: draft
                    .from
                    .as_ref()
                    .map(|from| CopiedThread { origin: from.origin.clone(), thread_id: from.thread_id.clone() }),
            };
            self.activity = Activity::starting();
            let activity = self.activity.clone();
            let sending = SendingDraft {
                text: text.clone(),
                attachments: attached.iter().map(Attachment::attached).collect(),
                activity: activity.clone(),
                left: false,
            };
            self.sending_drafts.insert(draft.id.clone(), sending);
            self.transcript.update(cx, |transcript, cx| {
                transcript.set_activity(activity);
                cx.notify();
            });
            Command::Send {
                server_id: project.server_id.clone(),
                thread_id: None,
                new_thread: Some(new_thread),
                text: text.clone(),
                attachments: paths,
                now: false,
            }
        };
        let Command::Send { server_id, .. } = &command else { unreachable!() };
        let server_id = server_id.clone();
        self.set_draft(String::new());
        self.draft_version += 1;
        self.attachments_by_key.remove(&key);
        // The server queues what is sent while the agent works, unless the message steers it.
        let queued = existing.is_some() && self.activity.running && !steers;
        let pending = RowModel::pending(text.clone(), attached.iter().map(Attachment::attached).collect(), queued);
        self.transcript.update(cx, |transcript, cx| {
            transcript.set_pending(Some(pending));
            cx.notify();
        });
        self.transcript_is_empty = false;

        let was_new = existing.is_none();
        self.ask(command, move |store, result, cx| {
            if was_new {
                store.sending_drafts.remove(&key);
            }
            match result {
                Err(error) => {
                    // The message goes back to where it was written, wherever the app is now.
                    store.set_text(text, &key);
                    store.draft_version += 1;
                    if !attached.is_empty() {
                        store.attachments_by_key.insert(key.clone(), attached);
                    }
                    store.fail(error);
                    if store.draft_key() != key {
                        return;
                    }
                    if was_new {
                        store.activity = Activity::default();
                    }
                    let activity = store.activity.clone();
                    let is_empty = store.transcript.update(cx, |transcript, cx| {
                        transcript.set_pending(None);
                        transcript.set_activity(activity);
                        cx.notify();
                        transcript.is_empty()
                    });
                    store.transcript_is_empty = is_empty;
                }
                Ok(value) => {
                    if !was_new {
                        return;
                    }
                    let thread_id = value["thread_id"].as_str().unwrap_or_default().to_string();
                    store.open_new_thread(thread_id, server_id, key, cx);
                }
            }
        });
    }

    /// Replaces a draft with the thread its first message created. If the draft is still open,
    /// the thread opens in its place with the message kept on screen.
    fn open_new_thread(&mut self, id: String, server_id: String, draft_id: String, cx: &mut Context<Self>) {
        let was_open = self.selection == Selection::Draft(draft_id.clone());
        self.remove_draft(&draft_id);
        self.move_tabs(&draft_id, &id);
        if !was_open {
            return;
        }
        self.set_selection(Selection::Thread(id.clone()));
        self.open_thread_id = Some(id.clone());
        let adopted = id.clone();
        self.transcript.update(cx, |transcript, _| transcript.adopt(adopted));
        self.send(Command::OpenThread { server_id, thread_id: id.clone() });
        self.send(Command::MarkSeen { thread_id: id });
    }

    /// The open thread, unless it is a removed server's, which nothing can be asked of.
    pub fn changeable_thread(&self) -> Option<&ThreadInfo> {
        self.selected_thread().filter(|thread| !self.on_removed_server(thread))
    }

    pub fn stop_thread(&mut self) {
        let Some(thread) = self.changeable_thread() else { return };
        let (server_id, thread_id) = (thread.server_id.clone(), thread.id.clone());
        self.request(&server_id, Request::Stop { thread_id });
    }

    /// Allows or refuses a tool call the agent waits with. `answers` is what was chosen, by
    /// question, when the call asks questions.
    pub fn answer(&mut self, approval_id: &str, allow: bool, answers: HashMap<String, String>) {
        let Some(thread) = self.changeable_thread() else { return };
        let (server_id, thread_id) = (thread.server_id.clone(), thread.id.clone());
        self.request(&server_id, Request::Answer { thread_id, approval_id: approval_id.to_string(), allow, answers });
    }

    /// Gives the agent a queued message now, in the turn that runs.
    pub fn send_now(&mut self, message_id: &str) {
        let Some(thread) = self.changeable_thread() else { return };
        let (server_id, thread_id) = (thread.server_id.clone(), thread.id.clone());
        self.request(&server_id, Request::SendQueued { thread_id, message_id: message_id.to_string() });
    }

    /// Takes a queued message back into the composer of its thread, after what is written there.
    pub fn take_back(&mut self, message_id: &str) {
        let Some(thread) = self.changeable_thread().cloned() else { return };
        let Some(message) = self.activity.queued.iter().find(|queued| queued.id == message_id).cloned() else { return };
        let key = thread.id.clone();
        let request = Request::CancelQueued { thread_id: thread.id.clone(), message_id: message_id.to_string() };
        let server_id = thread.server_id.clone();
        self.request_then(&server_id, request, move |store, result, _| {
            if let Err(error) = result {
                store.fail(error);
                return;
            }
            let written: Vec<String> = [store.drafts.get(&key).cloned().unwrap_or_default(), message.text.clone()]
                .into_iter()
                .filter(|text| !text.is_empty())
                .collect();
            store.set_text(written.join("\n\n"), &key);
            let back: Vec<Attachment> = message
                .attachments
                .iter()
                .map(|path| {
                    let shown = message.media.iter().find(|media| &media.src == path).map(|media| AttachedFile {
                        name: String::new(),
                        media: Some(media.id.clone()),
                        video: media.video,
                        poster: media.poster.clone(),
                    });
                    Attachment::from_server(path, shown.as_ref(), &thread.server_id)
                })
                .collect();
            let mut attached = store.attachments_by_key.get(&key).cloned().unwrap_or_default();
            attached.extend(back);
            if attached.is_empty() {
                store.attachments_by_key.remove(&key);
            } else {
                store.attachments_by_key.insert(key.clone(), attached);
            }
            store.draft_version += 1;
            store.composer_focus += 1;
        });
    }

    pub fn rename(&mut self, thread_id: &str, title: &str) {
        let title = title.trim().to_string();
        let Some(thread) = self.threads.get(thread_id).cloned() else { return };
        if title.is_empty() || title == thread.title {
            return;
        }
        let change = ThreadChange { title: Some(title.clone()), ..Default::default() };
        self.update_thread(&thread, change, |thread| thread.thread.title = title);
    }

    pub fn delete(&mut self, thread_id: &str) {
        let Some(thread) = self.threads.get(thread_id).filter(|thread| !self.on_removed_server(thread)) else { return };
        let server_id = thread.server_id.clone();
        self.request(&server_id, Request::Delete { thread_id: thread_id.to_string() });
    }

    /// Has the thread or the draft work with `model` under `account`. A thread that moves to an
    /// account without its session goes on in a new one, which is told what was said.
    pub fn set_model(&mut self, model: &ModelInfo, account: &AgentAccount) {
        let Some(thread) = self.selected_thread().cloned() else {
            let (id, account) = (model.id.clone(), account.id.clone());
            self.update_draft(|draft| {
                draft.model = Some(id);
                draft.agent_account = Some(account);
            });
            return;
        };
        let effort = thread.effort.clone().filter(|effort| model.efforts.contains(effort));
        let change = ThreadChange {
            model: Some(model.id.clone()),
            effort: Some(effort.clone().unwrap_or_default()),
            agent_account: (account.id != thread.agent_account).then(|| account.id.clone()),
            ..Default::default()
        };
        let (model, account) = (model.clone(), account.id.clone());
        self.update_thread(&thread, change, |thread| {
            thread.thread.model = Some(model.id);
            thread.thread.effort = effort;
            thread.thread.agent = model.agent;
            thread.thread.agent_account = account;
        });
    }

    pub fn set_effort(&mut self, effort: &str) {
        let effort = effort.to_string();
        let Some(thread) = self.selected_thread().cloned() else {
            self.update_draft(|draft| draft.effort = Some(effort));
            return;
        };
        let change = ThreadChange { effort: Some(effort.clone()), ..Default::default() };
        self.update_thread(&thread, change, |thread| thread.thread.effort = Some(effort));
    }

    pub fn set_access(&mut self, access: Access) {
        let Some(thread) = self.selected_thread().cloned() else {
            self.update_draft(|draft| draft.access = access);
            return;
        };
        let change = ThreadChange { access: Some(access), ..Default::default() };
        self.update_thread(&thread, change, |thread| thread.thread.access = access);
    }

    /// Has the agent of the open thread go on with what it was doing when it was interrupted.
    pub fn continue_thread(&mut self) {
        let Some(thread) = self.changeable_thread() else { return };
        let (server_id, thread_id) = (thread.server_id.clone(), thread.id.clone());
        self.request(&server_id, Request::Continue { thread_id });
    }

    /// Whether the open thread goes on by itself once its usage limit resets.
    pub fn set_continues(&mut self, on: bool) {
        let Some(thread) = self.selected_thread().cloned() else { return };
        let Some(Interruption::Limit { resets_at, .. }) = thread.interruption else { return };
        let change = ThreadChange { continues: Some(on), ..Default::default() };
        self.update_thread(&thread, change, |thread| {
            thread.thread.interruption = Some(Interruption::Limit { resets_at, continues: on })
        });
    }

    /// Marks threads done or brings them back. A thread that is working or monitoring can't be
    /// marked done.
    pub fn set_done(&mut self, ids: &[String], done: bool, from_sidebar: bool, cx: &mut Context<Self>) {
        let changed: Vec<ThreadInfo> = ids
            .iter()
            .filter_map(|id| self.threads.get(id))
            .filter(|thread| thread.is_done() != done && !(done && thread.busy()) && !self.on_removed_server(thread))
            .cloned()
            .collect();
        if changed.is_empty() {
            return;
        }
        // Leaving the thread that was just put away, for the next one that is still active.
        if let (true, true, Selection::Thread(open)) = (done, from_sidebar, self.selection.clone())
            && changed.iter().any(|thread| thread.id == open)
        {
            let active: Vec<String> = self.active_threads().iter().map(|thread| thread.id.clone()).collect();
            let position = active.iter().position(|id| *id == open).unwrap_or(0);
            let remaining: Vec<&String> =
                active.iter().filter(|id| !changed.iter().any(|thread| &thread.id == *id)).collect();
            let next = (!remaining.is_empty()).then(|| remaining[position.min(remaining.len() - 1)].clone());
            match next {
                Some(next) => self.select(Selection::Thread(next), cx),
                None => self.open_empty_draft(cx),
            }
        }
        let now = now();
        for thread in &changed {
            let change = ThreadChange { done: Some(done), ..Default::default() };
            self.update_thread(thread, change, |thread| thread.thread.done_at = done.then_some(now));
        }
        if !done {
            self.clear_undo();
            return;
        }
        let text =
            if changed.len() == 1 { "Done".to_string() } else { format!("Marked {} threads done", changed.len()) };
        self.show_undo(UndoNotice { thread_ids: changed.iter().map(|thread| thread.id.clone()).collect(), text }, cx);
    }

    pub fn toggle_done(&mut self, cx: &mut Context<Self>) {
        let Some(thread) = self.changeable_thread() else { return };
        let (id, done) = (thread.id.clone(), thread.is_done());
        self.set_done(&[id], !done, false, cx);
    }

    fn show_undo(&mut self, notice: UndoNotice, cx: &mut Context<Self>) {
        self.undo = Some(notice);
        self.schedule_undo_dismissal(cx);
    }

    pub fn perform_undo(&mut self, cx: &mut Context<Self>) {
        let Some(notice) = self.clear_undo() else { return };
        self.set_done(&notice.thread_ids, false, false, cx);
    }

    /// Changes a thread on its server, and here at once so the app doesn't wait for the answer.
    pub(super) fn update_thread(
        &mut self,
        thread: &ThreadInfo,
        change: ThreadChange,
        locally: impl FnOnce(&mut ThreadInfo),
    ) {
        if self.on_removed_server(thread) {
            return;
        }
        let mut changed = thread.clone();
        locally(&mut changed);
        self.set_thread(Some(changed), &thread.id);
        let before = thread.clone();
        let request = Request::Update { thread_id: thread.id.clone(), change };
        self.request_then(&thread.server_id, request, move |store, result, _| {
            if let Err(error) = result {
                let id = before.id.clone();
                store.set_thread(Some(before), &id);
                store.fail(error);
            }
        });
    }
}
