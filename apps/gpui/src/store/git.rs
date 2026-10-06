//! Branches and git: what the git button does, the commit sheet, and what git said afterwards.

use std::time::Duration;

use gpui_kit::prelude::*;
use motile_core::api::Command;
use motile_core::git::{Confirm, Item};
use motile_protocol::wire::{Branch, ChangedFile, GitAction, GitStage, Request};

use super::Store;
use crate::models::Project;

/// What a git action did, or why it couldn't, shown under the button until it is dismissed.
#[derive(Clone, PartialEq, Debug)]
pub struct GitNotice {
    /// The checkout the run was in: the project's folder or a thread's worktree.
    pub checkout_id: String,
    pub title: String,
    pub description: Option<String>,
    pub failed: bool,
    /// The pull request to open.
    pub url: Option<String>,
    /// The action that follows, like a push after a commit.
    pub next: Option<GitAction>,
    /// Tells two notices with the same words apart, so that the older one's timer leaves the
    /// newer one up.
    pub serial: u64,
}

impl GitNotice {
    fn new(checkout_id: &str, title: String) -> Self {
        Self {
            checkout_id: checkout_id.to_string(),
            title,
            description: None,
            failed: false,
            url: None,
            next: None,
            serial: 0,
        }
    }

    pub fn next_label(&self) -> Option<&'static str> {
        match self.next {
            Some(GitAction::Push) => Some("Push"),
            Some(GitAction::CreatePr) => Some("Create PR"),
            _ => None,
        }
    }
}

/// An action that waits for the user to say where it should happen.
#[derive(Clone, PartialEq, Debug)]
pub struct PendingGit {
    pub project: Project,
    pub action: GitAction,
    pub confirm: Confirm,
    pub message: Option<String>,
    pub paths: Vec<String>,
}

pub fn stage_label(stage: GitStage) -> &'static str {
    match stage {
        GitStage::Message => "Writing Commit",
        GitStage::Commit => "Committing",
        GitStage::Push => "Pushing",
        GitStage::PullRequestText => "Writing PR",
        GitStage::PullRequest => "Creating PR",
        GitStage::Pull => "Pulling",
    }
}

type FilesRead = fn(&mut Store, Vec<ChangedFile>, &mut Context<Store>);

impl Store {
    /// Whether a turn is running in the project's folder, which is when its branch can't be
    /// switched.
    pub fn is_working_in(&self, project: &Project) -> bool {
        self.threads
            .values()
            .any(|thread| thread.project_id == project.id && thread.running && thread.cwd == project.path)
    }

    /// Whether the project's server is new enough to start threads in worktrees of their own.
    pub fn can_use_worktrees(&self, project: &Project) -> bool {
        project.branch.is_some()
            && self.server(Some(&project.server_id)).map_or(0, |server| server.protocol_version) >= 6
    }

    /// Whether the open draft starts its thread in a new worktree.
    pub fn draft_uses_worktree(&self) -> bool {
        let Some(draft) = self.selected_draft() else { return false };
        let Some(project) = self.project(draft.project_id.as_deref()) else { return false };
        draft.worktree == Some(true) && self.can_use_worktrees(project)
    }

    /// The branch the open draft's worktree starts from: the one picked, or the one checked out.
    pub fn draft_base(&self) -> Option<String> {
        let draft = self.selected_draft()?;
        draft.base.clone().or_else(|| self.project(draft.project_id.as_deref())?.branch.clone())
    }

    /// The line over the thread's title: its project, its branch, and whether it starts in a new
    /// worktree.
    pub fn composer_project_line(&self) -> Option<String> {
        let project = self.composer_project();
        let folder = self.selected_thread().map(|thread| crate::models::last_component(&thread.cwd));
        let name = project.as_ref().map(|project| project.name.clone()).or(folder)?;
        if self.draft_uses_worktree()
            && let Some(base) = self.draft_base()
        {
            return Some(format!("{name} · {base} · New worktree"));
        }
        let Some(branch) = project.and_then(|project| project.branch) else { return Some(name) };
        Some(format!("{name} · {branch}"))
    }

    pub fn set_draft_worktree(&mut self, worktree: bool, cx: &mut Context<Self>) {
        self.update_draft(|draft| draft.worktree = Some(worktree));
        self.read_git(true, None::<FilesRead>, cx);
    }

    pub fn set_draft_base(&mut self, branch: String) {
        self.update_draft(|draft| draft.base = Some(branch));
    }

    /// Whether the project's server is new enough to list and switch branches.
    pub fn can_switch_branches(&self, project: &Project) -> bool {
        project.branch.is_some()
            && self.server(Some(&project.server_id)).map_or(0, |server| server.protocol_version) >= 3
    }

    /// Opens the branch picker once the branches are known.
    pub fn show_branches(&mut self, project: &Project) {
        let request = Request::Branches { project_id: project.id.clone() };
        self.request_then(&project.server_id, request, |store, result, _| {
            store.listed_branches = result
                .map(|value| serde_json::from_value::<Vec<Branch>>(value["branches"].clone()).unwrap_or_default());
            store.shows_branches = true;
        });
    }

    /// Checks the branch out in the project's folder, making it first if asked to. `done` gets
    /// what went wrong, if anything.
    pub fn switch_branch(
        &mut self,
        project: &Project,
        name: String,
        create: bool,
        done: impl FnOnce(&mut Store, Option<String>, &mut Context<Store>) + 'static,
    ) {
        let request = Request::SwitchBranch { project_id: project.id.clone(), branch: name, create };
        self.request_then(&project.server_id, request, move |store, result, cx| done(store, result.err(), cx));
    }

    /// Whether the project's server is new enough to commit, push and open pull requests.
    pub fn can_use_git(&self, project: &Project) -> bool {
        self.server(Some(&project.server_id)).map_or(0, |server| server.protocol_version) >= 4
    }

    /// Has the server read the repository git works in from here again, which the project then
    /// arrives with. With `fetch` the remote is asked first.
    pub fn read_git(
        &mut self,
        fetch: bool,
        done: Option<impl FnOnce(&mut Store, Vec<ChangedFile>, &mut Context<Store>) + 'static>,
        _cx: &mut Context<Self>,
    ) {
        let Some(project) = self.git_project() else { return };
        let thread_id = self.selected_thread().map(|thread| thread.id.clone());
        let request = Request::GitStatus { project_id: project.id.clone(), thread_id, fetch };
        self.request_then(&project.server_id, request, move |store, result, cx| match result {
            Ok(answer) => {
                if let Some(done) = done {
                    let files: Vec<ChangedFile> = serde_json::from_value(answer["files"].clone()).unwrap_or_default();
                    done(store, files, cx);
                }
            }
            // Only said when the user is waiting for the answer.
            Err(error) => {
                if done.is_some() {
                    store.fail(error);
                }
            }
        });
    }

    /// What the run in the project's checkout is at, started here or from another client.
    pub fn git_stage(&self, project: &Project) -> Option<GitStage> {
        if let Some(stage) = self.git_stages.get(&project.checkout_id()) {
            return Some(*stage);
        }
        let thread = self.selected_thread().filter(|thread| thread.project_id == project.id)?;
        thread.git_stage
    }

    /// A server said what stage a run is at, in the checkout the thread works in.
    pub(super) fn apply_git_progress(&mut self, project_id: &str, thread_id: Option<&str>, stage: GitStage) {
        let Some(project) = self.project(Some(project_id)).cloned() else { return };
        let seen = thread_id.and_then(|id| self.threads.get(id)).map(|thread| project.seen(thread)).unwrap_or(project);
        if let Some(current) = self.git_stages.get_mut(&seen.checkout_id()) {
            *current = stage;
        }
    }

    /// What a run starts with, until the server says what it is at.
    fn first_stage(action: GitAction, project: &Project, written: bool) -> GitStage {
        match action {
            GitAction::Pull => GitStage::Pull,
            GitAction::Push => GitStage::Push,
            GitAction::CreatePr => {
                let pushes = project.git_control.as_ref().is_some_and(|control| {
                    control.menu.iter().any(|item| item.action == GitAction::Push && item.reason.is_none())
                });
                if pushes { GitStage::Push } else { GitStage::PullRequestText }
            }
            _ => {
                if written {
                    GitStage::Commit
                } else {
                    GitStage::Message
                }
            }
        }
    }

    /// Opens the pull request's tab in the side panel, or the pull request on GitHub when its
    /// number isn't in the address.
    pub fn show_pull_request(&mut self, url: &str, cx: &mut Context<Self>) {
        self.git_notice = None;
        let number = crate::panel::pull_request::parse_number(url);
        if self.pull_request_unavailable().is_some() {
            return cx.open_url(url);
        }
        let Some((number, target)) = number.zip(self.panel_target()) else { return cx.open_url(url) };
        self.show_pull_request_tab(number, &target);
    }

    /// What a click on the git button does: the one action the repository calls for, at once.
    pub fn run_quick_git(&mut self, project: &Project, cx: &mut Context<Self>) {
        let Some(quick) = project.git_control.as_ref().map(|control| control.quick.clone()) else { return };
        if self.git_stage(project).is_some() {
            return;
        }
        if let Some(url) = &quick.url {
            return self.show_pull_request(url, cx);
        }
        let Some(action) = quick.action else {
            let title = quick.hint.clone().unwrap_or(quick.label.clone());
            self.show_git_notice(GitNotice::new(&project.checkout_id(), title), cx);
            return;
        };
        self.start_git(action, project, quick.confirm.clone(), cx);
    }

    /// What a pick from the menu does: a commit opens its sheet, the others happen at once.
    pub fn choose_git(&mut self, item: &Item, project: &Project, cx: &mut Context<Self>) {
        if item.reason.is_some() || self.git_stage(project).is_some() {
            return;
        }
        if item.action != GitAction::Commit {
            return self.start_git(item.action, project, item.confirm.clone(), cx);
        }
        // A sheet takes its size from what it opens with, so the files come first.
        let project = project.clone();
        self.read_git(
            false,
            Some(move |store: &mut Store, files: Vec<ChangedFile>, _: &mut Context<Store>| {
                store.git_files = files;
                store.committing_project = Some(project);
            }),
            cx,
        );
    }

    /// Runs the action, after asking where when it would push from the default branch.
    fn start_git(&mut self, action: GitAction, project: &Project, confirm: Option<Confirm>, cx: &mut Context<Self>) {
        let Some(confirm) = confirm else {
            return self.run_git(action, project, None, Vec::new(), false, cx);
        };
        self.pending_git =
            Some(PendingGit { project: project.clone(), action, confirm, message: None, paths: Vec::new() });
    }

    /// Carries on with the action that waited, on the default branch or on a branch made for it.
    pub fn confirm_git(&mut self, pending: PendingGit, on_new_branch: bool, cx: &mut Context<Self>) {
        self.run_git(pending.action, &pending.project, pending.message, pending.paths, on_new_branch, cx);
    }

    /// Has the project's server carry the action out. It writes the commit message when there
    /// is none, and the pull request. What it did, or what git refused, shows under the button.
    pub fn run_git(
        &mut self,
        action: GitAction,
        project: &Project,
        message: Option<String>,
        paths: Vec<String>,
        new_branch: bool,
        _cx: &mut Context<Self>,
    ) {
        if self.git_stage(project).is_some() {
            return;
        }
        let checkout_id = project.checkout_id();
        let written = message.as_ref().is_some_and(|message| !message.is_empty()) && !new_branch;
        self.git_stages.insert(checkout_id.clone(), Self::first_stage(action, project, written));
        self.git_notice = None;
        let thread_id =
            self.selected_thread().filter(|thread| thread.project_id == project.id).map(|thread| thread.id.clone());
        let command = Command::GitRun {
            server_id: project.server_id.clone(),
            project_id: project.id.clone(),
            action,
            thread_id,
            message: message.filter(|message| !message.is_empty()),
            paths,
            new_branch,
        };
        self.ask(command, move |store, result, cx| {
            store.git_stages.remove(&checkout_id);
            let notice = match result {
                Ok(done) => {
                    let mut notice =
                        GitNotice::new(&checkout_id, done["title"].as_str().unwrap_or_default().to_string());
                    notice.description = done["description"].as_str().map(String::from);
                    notice.url = done["url"].as_str().map(String::from);
                    notice.next = serde_json::from_value(done["next"].clone()).ok();
                    notice
                }
                Err(error) => {
                    let mut notice = GitNotice::new(&checkout_id, "Git stopped".into());
                    notice.description = Some(error);
                    notice.failed = true;
                    notice
                }
            };
            store.show_git_notice(notice, cx);
        });
    }

    /// What worked goes away by itself; what failed stays until it is closed.
    fn show_git_notice(&mut self, mut notice: GitNotice, cx: &mut Context<Self>) {
        notice.serial = self.git_notice.as_ref().map_or(0, |shown| shown.serial) + 1;
        let failed = notice.failed;
        self.git_notice = Some(notice.clone());
        if failed {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(10)).await;
            let _ = this.update(cx, |store, cx| {
                if store.git_notice.as_ref() == Some(&notice) {
                    store.git_notice = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn dismiss_git_notice(&mut self) {
        self.git_notice = None;
    }

    /// The action a notice says comes next, like the push after a commit.
    pub fn run_next_git(&mut self, cx: &mut Context<Self>) {
        let Some(notice) = self.git_notice.clone() else { return };
        let Some(next) = notice.next else { return };
        let Some(project) = self.git_project().filter(|project| project.checkout_id() == notice.checkout_id) else {
            return;
        };
        let confirm = project.git_control.as_ref().and_then(|control| {
            control.menu.iter().find(|item| item.action == next).and_then(|item| item.confirm.clone())
        });
        self.git_notice = None;
        self.start_git(next, &project, confirm, cx);
    }
}
