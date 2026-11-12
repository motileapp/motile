//! The threads and drafts: opening them, sending, and changing a thread's settings.

use std::collections::HashMap;

use gpui_kit::prelude::*;
use motile_core::api::{Command, Section};
use motile_core::outbox::Outgoing;
use motile_protocol::wire::{
    Access, AgentAccount, CopiedThread, Interruption, ModelContext, ModelInfo, NewThread, NewWorktree, Request,
    ThreadChange,
};

use super::{
    Continuation, CustomSnooze, PanelPage, Selection, SendingDraft, StopQuestion, Store, ThreadDraft, UndoKind,
    UndoNotice, shown_files,
};
use crate::models::*;
use crate::panel::state::PanelTab;
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
        self.open_arrived_threads(cx);
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
        if !before.is_archived() && thread.is_archived() && before.pull_request != thread.pull_request {
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
        let Some(thread) = self.selected_thread().filter(|thread| thread.unread || thread.woke) else { return };
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
        self.close_find(cx);
        if let Some(open) = self.open_thread_id.take() {
            self.keep_place(open.clone(), None, cx);
            self.send(Command::CloseThread { thread_id: open });
        }
        let left = self.selected_draft().cloned();
        self.set_selection(new.clone());
        if self.sidebar.revealed_thread_id.as_ref().map(|id| Selection::Thread(id.clone())) != Some(new.clone()) {
            self.sidebar.revealed_thread_id = None;
        }
        if let Some(left) = left
            && !self.is_sending(&left.id)
            && self.preview(&left).is_none()
        {
            self.remove_draft(&left.id);
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
    pub(super) fn show_sending_message(&mut self, cx: &mut Context<Self>) {
        let Some(sending) = self.selected_draft().and_then(|draft| self.sending_drafts.get(&draft.id)).cloned() else {
            return;
        };
        self.activity = sending.activity();
        let activity = self.activity.clone();
        let pending = RowModel::pending(sending.text, sending.attachments, false);
        self.transcript.update(cx, |transcript, cx| {
            transcript.set_activity(activity);
            transcript.set_pending(Some(pending));
            cx.notify();
        });
        self.transcript_is_empty = false;
    }

    /// Has the core open the thread's transcript, or its agent's, where the transcript leaves it,
    /// unless it still shows the rows of the thread before.
    pub(crate) fn keep_place(&mut self, thread_id: String, agent_id: Option<String>, cx: &Context<Self>) {
        let transcript = if agent_id.is_some() { &self.agent_transcript } else { &self.transcript }.read(cx);
        if transcript.keeps_rows || transcript.thread_id.as_deref() != Some(&thread_id) {
            return;
        }
        let at = transcript.place();
        self.send(Command::KeepPlace { thread_id, agent_id, at });
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
        self.show_outbox_message(cx);
    }

    /// What the toolbar button and ⌘N do: with one project there is nothing to pick and the draft
    /// opens at once, without one it starts without a project, and with more the panel asks which.
    pub fn new_thread(&mut self, cx: &mut Context<Self>) {
        let picked: Vec<&Project> =
            self.pickable_projects().into_iter().filter(|project| !project.no_project).collect();
        if picked.len() > 1 {
            return self.open_panel(PanelPage::Projects);
        }
        let first = picked.first().copied().or_else(|| self.preferred_copy(Project::NO_PROJECT_KEY));
        let first = first.map(|project| project.id.clone());
        self.start_new_thread(first, cx);
    }

    /// Opens a draft of a new thread that goes on from the open thread, whose server was removed,
    /// in its project on a server that has it, or else in "No project".
    pub fn continue_on_another_server(&mut self, cx: &mut Context<Self>) {
        let Some(thread) = self.selected_thread().filter(|thread| self.is_removed(&thread.server_id)) else { return };
        let from = Continuation {
            origin: thread.server_id.clone(),
            thread_id: thread.id.clone(),
            title: thread.title.clone(),
        };
        let project = self
            .project(Some(&thread.project_id))
            .and_then(|project| self.continuation(project))
            .or_else(|| self.preferred_copy(Project::NO_PROJECT_KEY));
        let project_id = project.map(|project| project.id.clone());
        self.start_new_thread(project_id, cx);
        self.update_draft(|draft| draft.from = Some(from));
        self.composer_focus += 1;
    }

    /// The removed server's project on a server that has it: the same repository, or else a folder
    /// of the same name, for an older copy that doesn't say its repository.
    fn continuation(&self, project: &Project) -> Option<&Project> {
        if let Some(copy) = self.preferred_copy(&project.key) {
            return Some(copy);
        }
        let named =
            self.projects.iter().find(|other| !self.is_removed(&other.server_id) && other.name == project.name)?;
        self.preferred_copy(&named.key)
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
        let next = self.listed_threads().next().map(|thread| thread.id.clone());
        if let Some(next) = next {
            return self.select(Selection::Thread(next), cx);
        }
        self.open_empty_draft(cx);
    }

    pub fn can_send(&self) -> bool {
        if let Some(asking) = self.asking() {
            return self.composer_server().is_some_and(|server| server.connected()) && self.can_go_on(asking);
        }
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
        self.has_message()
    }

    /// Whether the composer holds something to send, whether or not it can be sent now.
    pub fn has_message(&self) -> bool {
        !self.draft().trim().is_empty() || !self.attachments().is_empty()
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
        if self.asking().is_some() {
            return self.go_on();
        }
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
                shown: shown_value(&attached),
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
            let account = self.composer_account().map(|account| account.id);
            let new_thread = NewThread {
                id: Some(draft.id.clone()),
                project_id: project.id.clone(),
                agent: model.agent,
                agent_account: account,
                model: Some(self.composer_context().map_or(model.id.clone(), |context| context.model)),
                effort: self.composer_effort(),
                access: draft.access,
                worktree,
                from: draft
                    .from
                    .as_ref()
                    .map(|from| CopiedThread { origin: from.origin.clone(), thread_id: from.thread_id.clone() }),
            };
            let command = Command::Send {
                server_id: project.server_id.clone(),
                thread_id: None,
                new_thread: Some(new_thread),
                text: text.clone(),
                attachments: paths.clone(),
                now: false,
                shown: shown_value(&attached),
            };
            let sending = SendingDraft {
                text: text.clone(),
                attachments: attached.iter().map(Attachment::attached).collect(),
                started_at: now(),
                server_id: project.server_id.clone(),
                paths,
            };
            self.activity = sending.activity();
            let activity = self.activity.clone();
            self.sending_drafts.insert(draft.id.clone(), sending);
            self.transcript.update(cx, |transcript, cx| {
                transcript.set_activity(activity);
                cx.notify();
            });
            command
        };
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
        if existing.is_none() {
            self.ask(command, move |store, result, cx| {
                let Err(error) = result else { return };
                store.unsend(&key, cx);
                store.fail(error);
            });
            return;
        }

        // The core keeps the message until its server has it, so only a server it doesn't know
        // refuses it here.
        self.ask(command, move |store, result, cx| {
            let Err(error) = result else { return };
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
            let activity = store.activity.clone();
            let is_empty = store.transcript.update(cx, |transcript, cx| {
                transcript.set_pending(None);
                transcript.set_activity(activity);
                cx.notify();
                transcript.is_empty()
            });
            store.transcript_is_empty = is_empty;
        });
    }

    /// Takes in the core's outbox: the drafts whose first message is on its way, the drafts kept
    /// for them after a restart, and the errors of the messages that haven't reached their servers.
    pub(super) fn apply_outbox(&mut self, messages: Vec<Outgoing>, cx: &mut Context<Self>) {
        for message in &messages {
            if let Some(error) = &message.error
                && self.told_outbox_errors.get(&message.id) != Some(error)
            {
                self.told_outbox_errors.insert(message.id.clone(), error.clone());
                self.error_message = Some(error.clone());
            }
            let Some(new_thread) = &message.new_thread else { continue };
            if !self.thread_drafts.iter().any(|draft| draft.id == message.thread_id) {
                self.thread_drafts.push(ThreadDraft::of(&message.thread_id, new_thread));
                self.save_thread_drafts();
            }
            self.sending_drafts.entry(message.thread_id.clone()).or_insert_with(|| SendingDraft::of(message));
        }
        self.told_outbox_errors.retain(|id, _| messages.iter().any(|message| &message.id == id));
        self.outbox = messages;
        if self.transcript.read(cx).pending.is_none() {
            self.show_sending_message(cx);
        }
        self.show_outbox_message(cx);
    }

    /// Shows the open thread's message that is still in the outbox, as after a restart, unless
    /// the transcript shows it already.
    fn show_outbox_message(&mut self, cx: &mut Context<Self>) {
        let Selection::Thread(thread_id) = &self.selection else { return };
        let outgoing =
            self.outbox.iter().rev().find(|message| &message.thread_id == thread_id && message.new_thread.is_none());
        let Some(message) = outgoing else { return };
        let transcript = self.transcript.read(cx);
        let shown = transcript.pending.is_some() || transcript.rows.iter().any(|row| row.row.item == message.id);
        if shown || transcript.thread_id.as_deref() != Some(thread_id) {
            return;
        }
        let pending = RowModel::pending(message.text.clone(), shown_files(&message.shown), false);
        self.transcript.update(cx, |transcript, cx| {
            transcript.set_pending(Some(pending));
            cx.notify();
        });
        self.transcript_is_empty = false;
    }

    /// A message the core won't send goes back to where it was written, and says why.
    pub(super) fn unsent(&mut self, message: Outgoing, error: String, cx: &mut Context<Self>) {
        self.error_message = Some(error);
        if message.new_thread.is_some() {
            self.sending_drafts.entry(message.thread_id.clone()).or_insert_with(|| SendingDraft::of(&message));
            return self.unsend(&message.thread_id, cx);
        }
        let key = message.thread_id.clone();
        self.put_back(&key, &SendingDraft::of(&message));
        if self.draft_key() != key {
            return;
        }
        let activity = self.activity.clone();
        let is_empty = self.transcript.update(cx, |transcript, cx| {
            transcript.set_pending(None);
            transcript.set_activity(activity);
            cx.notify();
            transcript.is_empty()
        });
        self.transcript_is_empty = is_empty;
    }

    /// Puts a message back in the composer of `key`, before what was written there since.
    fn put_back(&mut self, key: &str, message: &SendingDraft) {
        let written = self.drafts.get(key).map(|text| text.trim().to_string()).unwrap_or_default();
        let text = if written.is_empty() { message.text.clone() } else { format!("{}\n\n{written}", message.text) };
        self.set_text(text, key);
        self.draft_version += 1;
        let mut back: Vec<Attachment> = message
            .paths
            .iter()
            .zip(message.attachments.iter().map(Some).chain(std::iter::repeat(None)))
            .map(|(path, shown)| Attachment::from_server(path, shown, &message.server_id))
            .collect();
        back.extend(self.attachments_by_key.remove(key).unwrap_or_default());
        if !back.is_empty() {
            self.attachments_by_key.insert(key.to_string(), back);
        }
    }

    /// Puts a draft's first message back in its composer, before what was written there since.
    fn unsend(&mut self, draft_id: &str, cx: &mut Context<Self>) {
        let Some(sending) = self.sending_drafts.remove(draft_id) else { return };
        self.put_back(draft_id, &sending);
        if self.draft_key() != draft_id {
            return;
        }
        self.activity = Activity::default();
        let is_empty = self.transcript.update(cx, |transcript, cx| {
            transcript.set_pending(None);
            transcript.set_activity(Activity::default());
            cx.notify();
            transcript.is_empty()
        });
        self.transcript_is_empty = is_empty;
    }

    /// Replaces the drafts whose threads the client now has with them.
    pub(super) fn open_arrived_threads(&mut self, cx: &mut Context<Self>) {
        let drafts: Vec<String> = self.thread_drafts.iter().map(|draft| draft.id.clone()).collect();
        for draft_id in drafts {
            let awaited = self.awaited_threads.iter().find(|(_, draft)| **draft == draft_id).map(|(id, _)| id.clone());
            let thread_id = if self.threads.contains_key(&draft_id) {
                draft_id.clone()
            } else {
                match awaited.filter(|id| self.threads.contains_key(id)) {
                    Some(id) => id,
                    None => continue,
                }
            };
            self.awaited_threads.remove(&thread_id);
            let server_id = self.threads[&thread_id].server_id.clone();
            self.open_new_thread(thread_id, server_id, draft_id, cx);
        }
    }

    /// Replaces a draft with the thread its first message started. What was written or attached
    /// in the meantime stays in the composer, which is the thread's now. If the draft is still
    /// open, the thread opens in its place with the message kept on screen.
    fn open_new_thread(&mut self, id: String, server_id: String, draft_id: String, cx: &mut Context<Self>) {
        let was_open = self.selection == Selection::Draft(draft_id.clone());
        self.sending_drafts.remove(&draft_id);
        self.thread_drafts.retain(|draft| draft.id != draft_id);
        self.save_thread_drafts();
        if id != draft_id {
            let written = self.drafts.get(&draft_id).cloned().unwrap_or_default();
            self.set_text(written, &id);
            self.set_text(String::new(), &draft_id);
            if let Some(attachments) = self.attachments_by_key.remove(&draft_id) {
                self.attachments_by_key.insert(id.clone(), attachments);
            }
            self.move_tabs(&draft_id, &id);
        }
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

    pub fn stop_thread(&mut self, cx: &mut Context<Self>) {
        let Some(thread) = self.selected_thread() else { return };
        let id = thread.id.clone();
        self.stop_thread_id(&id, cx);
    }

    /// Stops the thread's agent, and the agents it started, asking first while those work.
    pub fn stop_thread_id(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(thread) = self.threads.get(id).filter(|thread| !self.on_removed_server(thread)) else { return };
        let Some(question) = thread.children.as_ref().and_then(|children| children.stop_question.clone()) else {
            return self.stop_now(id);
        };
        self.stopping = Some(StopQuestion { thread_id: id.to_string(), question });
        cx.notify();
    }

    pub fn stop_now(&mut self, id: &str) {
        let Some(thread) = self.threads.get(id).filter(|thread| !self.on_removed_server(thread)) else { return };
        let (server_id, thread_id) = (thread.server_id.clone(), thread.id.clone());
        self.request(&server_id, Request::Stop { thread_id });
    }

    /// Opens the thread an agent the open thread's agent started works in, or the one whose agent
    /// started it.
    pub fn open_started_thread(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.threads.contains_key(id) {
            return;
        }
        // The agents stay in view on the way between a thread and the agents it started.
        let shows_agents = self.right_panel.is_open && self.panel_tabs().active == Some(PanelTab::Agents);
        self.select(Selection::Thread(id.to_string()), cx);
        if shows_agents {
            self.open_tab(PanelTab::Agents);
        }
    }

    /// Allows or refuses a tool call the agent waits with. `answers` is what was chosen, by
    /// question, when the call asks questions.
    pub fn answer(&mut self, approval_id: &str, allow: bool, answers: HashMap<String, String>) {
        let Some(thread) = self.changeable_thread() else { return };
        let (server_id, thread_id) = (thread.server_id.clone(), thread.id.clone());
        self.request(&server_id, Request::Answer { thread_id, approval_id: approval_id.to_string(), allow, answers });
        self.question_answers.remove(approval_id);
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
                    let shown = message.media.iter().find(|media| &media.src == path && !media.file).map(|media| {
                        AttachedFile {
                            name: String::new(),
                            media: Some(media.id.clone()),
                            video: media.video,
                            poster: media.poster.clone(),
                            ..AttachedFile::default()
                        }
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
    /// The model's context window stays the one the composer had where the model offers it.
    pub fn set_model(&mut self, model: &ModelInfo, account: &AgentAccount) {
        let label = self.composer_context().map(|context| context.label);
        let same = model.contexts.iter().find(|context| Some(&context.label) == label.as_ref());
        let id = same.or_else(|| model.context(None)).map_or(model.id.clone(), |context| context.model.clone());
        let Some(thread) = self.selected_thread().cloned() else {
            let account = account.id.clone();
            self.update_draft(|draft| {
                draft.model = Some(id);
                draft.agent_account = Some(account);
            });
            return;
        };
        let effort = thread.effort.clone().filter(|effort| model.efforts.contains(effort));
        let change = ThreadChange {
            model: Some(id.clone()),
            effort: Some(effort.clone().unwrap_or_default()),
            agent_account: (account.id != thread.agent_account).then(|| account.id.clone()),
            ..Default::default()
        };
        let (agent, account) = (model.agent, account.id.clone());
        self.update_thread(&thread, change, |thread| {
            thread.thread.model = Some(id);
            thread.thread.effort = effort;
            thread.thread.agent = agent;
            thread.thread.agent_account = account;
        });
    }

    /// Has the thread or the draft run its model with the context window.
    pub fn set_context(&mut self, context: &ModelContext) {
        let id = context.model.clone();
        let Some(thread) = self.selected_thread().cloned() else {
            self.update_draft(|draft| draft.model = Some(id));
            return;
        };
        let change = ThreadChange { model: Some(id.clone()), ..Default::default() };
        self.update_thread(&thread, change, |thread| thread.thread.model = Some(id));
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

    /// Archives threads or brings them back. A thread that is working or monitoring can't be
    /// archived.
    pub fn set_archived(&mut self, ids: &[String], archived: bool, from_sidebar: bool, cx: &mut Context<Self>) {
        let changed: Vec<ThreadInfo> = ids
            .iter()
            .filter_map(|id| self.threads.get(id))
            .filter(|thread| {
                thread.is_archived() != archived
                    && !(archived && thread.unfinished())
                    && !self.on_removed_server(thread)
            })
            .cloned()
            .collect();
        if changed.is_empty() {
            return;
        }
        if archived && from_sidebar {
            self.leave(&changed, cx);
        }
        let now = now();
        for thread in &changed {
            let change = ThreadChange { archived: Some(archived), ..Default::default() };
            self.update_thread(thread, change, |thread| {
                thread.thread.archived_at = archived.then_some(now);
                thread.thread.snoozed_until = None;
                thread.section = if archived { Section::Archived } else { thread.awake_section() };
            });
        }
        if !archived {
            self.clear_undo();
            return;
        }
        let text =
            if changed.len() == 1 { "Archived".to_string() } else { format!("Archived {} threads", changed.len()) };
        let thread_ids = changed.iter().map(|thread| thread.id.clone()).collect();
        self.show_undo(UndoNotice { kind: UndoKind::Archive, thread_ids, text }, cx);
    }

    /// Snoozes threads until `until`, out of the sidebar's way until then.
    pub fn snooze(&mut self, ids: &[String], until: f64, from_sidebar: bool, cx: &mut Context<Self>) {
        let changed: Vec<ThreadInfo> =
            ids.iter().filter_map(|id| self.threads.get(id)).filter(|thread| self.snoozes(thread)).cloned().collect();
        if changed.is_empty() {
            return;
        }
        if from_sidebar {
            self.leave(&changed, cx);
        }
        for thread in &changed {
            let change = ThreadChange { snoozed_until: Some(until), ..Default::default() };
            self.update_thread(thread, change, |thread| {
                thread.thread.snoozed_until = Some(until);
                thread.thread.archived_at = None;
                thread.section = Section::Snoozed;
            });
        }
        let text =
            if changed.len() == 1 { "Snoozed".to_string() } else { format!("Snoozed {} threads", changed.len()) };
        let thread_ids = changed.iter().map(|thread| thread.id.clone()).collect();
        self.show_undo(UndoNotice { kind: UndoKind::Snooze, thread_ids, text }, cx);
    }

    /// Brings snoozed threads back among the active ones.
    pub fn wake(&mut self, ids: &[String]) {
        let now = now();
        let snoozed: Vec<ThreadInfo> = ids
            .iter()
            .filter_map(|id| self.threads.get(id))
            .filter(|thread| thread.section == Section::Snoozed && !self.on_removed_server(thread))
            .cloned()
            .collect();
        for thread in &snoozed {
            let change = ThreadChange { snoozed_until: Some(0.), ..Default::default() };
            self.update_thread(thread, change, |thread| {
                thread.thread.snoozed_until = None;
                thread.thread.position = now;
                thread.section = thread.awake_section();
            });
        }
    }

    /// Snoozes the open thread until the time the menus offer at `index`, as they offer them now.
    pub fn snooze_open_thread(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(thread) = self.changeable_thread() else { return };
        let Some((_, until)) = snooze_presets().get(index).cloned() else { return };
        let id = thread.id.clone();
        self.snooze(&[id], until, false, cx);
    }

    pub fn snoozes(&self, thread: &ThreadInfo) -> bool {
        !self.on_removed_server(thread) && self.server(Some(&thread.server_id)).is_some_and(Server::snoozes)
    }

    /// Opens the modal that snoozes threads until a day and a time the user picks.
    pub fn begin_custom_snooze(&mut self, ids: &[String], from_sidebar: bool) {
        self.custom_snooze = Some(CustomSnooze { thread_ids: ids.to_vec(), from_sidebar });
    }

    /// Leaves the open thread when it is put away, for the next one the sidebar lists.
    fn leave(&mut self, threads: &[ThreadInfo], cx: &mut Context<Self>) {
        let Selection::Thread(open) = self.selection.clone() else { return };
        if !threads.iter().any(|thread| thread.id == open) {
            return;
        }
        let listed: Vec<String> = self.listed_threads().map(|thread| thread.id.clone()).collect();
        let position = listed.iter().position(|id| *id == open).unwrap_or(0);
        let remaining: Vec<&String> =
            listed.iter().filter(|id| !threads.iter().any(|thread| &thread.id == *id)).collect();
        match remaining.get(position.min(remaining.len().saturating_sub(1))) {
            Some(next) => self.select(Selection::Thread((*next).clone()), cx),
            None => self.open_empty_draft(cx),
        }
    }

    pub fn toggle_archived(&mut self, cx: &mut Context<Self>) {
        let Some(thread) = self.changeable_thread() else { return };
        let (id, archived) = (thread.id.clone(), thread.is_archived());
        self.set_archived(&[id], !archived, false, cx);
    }

    fn show_undo(&mut self, notice: UndoNotice, cx: &mut Context<Self>) {
        self.undo = Some(notice);
        self.schedule_undo_dismissal(cx);
    }

    pub fn perform_undo(&mut self, cx: &mut Context<Self>) {
        let Some(notice) = self.clear_undo() else { return };
        match notice.kind {
            UndoKind::Archive => self.set_archived(&notice.thread_ids, false, false, cx),
            UndoKind::Snooze => self.wake(&notice.thread_ids),
        }
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

/// The times a snooze menu offers, as the core picks them, each with its title: "In 1 Hour
/// (3:15 PM)", "Next Week (Mon 9:00 AM)".
pub fn snooze_presets() -> Vec<(String, f64)> {
    let today = chrono::Local::now().date_naive();
    motile_core::snooze::presets(chrono::Local::now())
        .into_iter()
        .map(|preset| {
            let day = chrono::DateTime::from_timestamp(preset.until as i64, 0)
                .map(|time| time.with_timezone(&chrono::Local).date_naive());
            let soon = day.is_some_and(|day| (day - today).num_days() <= 1);
            let when = if soon {
                crate::format::date(preset.until, "jmm", "%H:%M")
            } else {
                crate::format::date(preset.until, "EEEjmm", "%a %H:%M")
            };
            (format!("{} ({when})", preset.label), preset.until)
        })
        .collect()
}

/// What the client shows of a message's attachments while it travels, kept with it in the outbox.
fn shown_value(attached: &[Attachment]) -> serde_json::Value {
    let files: Vec<AttachedFile> = attached.iter().map(Attachment::attached).collect();
    serde_json::to_value(files).unwrap_or_default()
}
