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
        GitStage::Merge => "Merging PR",
        GitStage::AutoMerge => "Setting Auto-merge",
        GitStage::UpdateBranch => "Updating Branch",
        GitStage::Close => "Closing PR",
        GitStage::Reopen => "Reopening PR",
        GitStage::Revert => "Reverting PR",
        GitStage::Unknown => "Working",
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

    fn protocol_of(&self, project: &Project) -> u32 {
        self.server(Some(&project.server_id)).map_or(0, |server| server.protocol_version)
    }

    /// Whether the project's server is new enough to start threads in worktrees of their own, and
    /// its branch has a commit to start them from.
    pub fn can_use_worktrees(&self, project: &Project) -> bool {
        project.branch.is_some()
            && !project.git.as_ref().is_some_and(|git| git.no_commit)
            && self.protocol_of(project) >= 6
    }

    /// Whether the open draft starts its thread in a new worktree.
    pub fn draft_uses_worktree(&self) -> bool {
        let Some(draft) = self.selected_draft() else { return false };
        let Some(project) = self.project(draft.project_id.as_deref()) else { return false };
        draft.worktree == Some(true) && self.can_use_worktrees(project)
    }

    /// The branch the open draft's worktree starts from: the one picked, the one picked last in
    /// its project, or the default one.
    pub fn draft_base(&self) -> Option<String> {
        let draft = self.selected_draft()?;
        let project = self.project(draft.project_id.as_deref())?;
        draft
            .base
            .clone()
            .or_else(|| self.prefs.string(&format!("new.base-{}", project.id)))
            .or_else(|| project.git.as_ref().and_then(|git| git.default_branch.clone()))
            .or_else(|| project.branch.clone())
    }

    /// The project and the base of the open draft's worktree, when its thread starts in one.
    pub fn draft_worktree_key(&self) -> Option<String> {
        if !self.draft_uses_worktree() {
            return None;
        }
        let project_id = self.selected_draft()?.project_id.clone()?;
        Some(format!("{project_id} {}", self.draft_base()?))
    }

    /// What the open draft's worktree starts at: its base, or the remote's when that has commits
    /// the local one lacks.
    pub fn draft_start(&self) -> Option<String> {
        let base = self.draft_base()?;
        Some(self.draft_worktree_key().and_then(|key| self.worktree_starts.get(&key).cloned()).unwrap_or(base))
    }

    /// Asks the project's server what the open draft's worktree would start at. With `fetch` the
    /// remote is asked first, and a fetch that fails is said.
    pub fn read_worktree_start(&mut self, fetch: bool) {
        let (Some(key), Some(base)) = (self.draft_worktree_key(), self.draft_base()) else { return };
        let Some(project) = self.project(self.selected_draft().and_then(|draft| draft.project_id.as_deref())).cloned()
        else {
            return;
        };
        if self.protocol_of(&project) < 13 {
            return;
        }
        let request = Request::WorktreeStart { project_id: project.id.clone(), base: base.clone(), fetch };
        let server_id = project.server_id.clone();
        self.request_then(&server_id, request, move |store, result, cx| {
            let Ok(answer) = result else { return };
            store.worktree_starts.insert(key, answer["start"].as_str().unwrap_or_default().to_string());
            let Some(problem) = answer["problem"].as_str() else { return };
            let mut notice = GitNotice::new(&project.checkout_id(), format!("Couldn't fetch {base}"));
            notice.description = Some(problem.to_string());
            notice.failed = true;
            store.show_git_notice(notice, cx);
        });
    }

    /// The open draft's base, when its worktree would start from the remote's instead, which has
    /// commits the local one lacks.
    pub fn draft_base_to_pull(&self) -> Option<String> {
        let (base, start) = (self.draft_base()?, self.draft_start()?);
        let project = self.project(self.selected_draft()?.project_id.as_deref())?;
        (start != base && self.protocol_of(project) >= 20).then_some(base)
    }

    pub fn pulling_draft_base(&self) -> bool {
        self.draft_worktree_key().is_some_and(|key| self.pulling_bases.contains(&key))
    }

    /// Brings the open draft's local base up to the remote's, for its worktree to start from it.
    pub fn pull_draft_base(&mut self) {
        let (Some(key), Some(base)) = (self.draft_worktree_key(), self.draft_base_to_pull()) else { return };
        let Some(project) = self.project(self.selected_draft().and_then(|draft| draft.project_id.as_deref())).cloned()
        else {
            return;
        };
        if !self.pulling_bases.insert(key.clone()) {
            return;
        }
        self.git_notice = None;
        let request = Request::UpdateBase { project_id: project.id.clone(), base: base.clone() };
        let server_id = project.server_id.clone();
        self.request_then(&server_id, request, move |store, result, cx| {
            store.pulling_bases.remove(&key);
            let notice = match result {
                Ok(answer) => {
                    store.worktree_starts.insert(key, answer["start"].as_str().unwrap_or_default().to_string());
                    GitNotice::new(&project.checkout_id(), format!("Pulled {base}"))
                }
                Err(error) => {
                    let mut notice = GitNotice::new(&project.checkout_id(), format!("Couldn't pull {base}"));
                    notice.description = Some(error.into());
                    notice.failed = true;
                    notice
                }
            };
            store.show_git_notice(notice, cx);
        });
    }

    /// The git notice of the checkout git works in from here, or of the project the open draft
    /// starts a worktree in.
    pub fn open_git_notice(&self) -> Option<GitNotice> {
        let notice = self.git_notice.as_ref()?;
        let draft_project = self
            .draft_uses_worktree()
            .then(|| self.project(self.selected_draft()?.project_id.as_deref()).cloned())
            .flatten();
        let checkout = self.git_project().or(draft_project)?.checkout_id();
        (notice.checkout_id == checkout).then(|| notice.clone())
    }

    /// The line over the thread's title: its project, its branch, and whether it starts in a new
    /// worktree.
    pub fn composer_project_line(&self) -> Option<Vec<String>> {
        let project = self.composer_project();
        let folder = self.selected_thread().map(|thread| crate::models::last_component(&thread.cwd));
        let name = project.as_ref().map(|project| project.name.clone()).or(folder)?;
        if self.draft_uses_worktree()
            && let Some(start) = self.draft_start()
        {
            return Some(vec![name, start, "New worktree".into()]);
        }
        let Some(branch) = project.and_then(|project| project.branch) else { return Some(vec![name]) };
        Some(vec![name, branch])
    }

    pub fn set_draft_worktree(&mut self, worktree: bool, cx: &mut Context<Self>) {
        self.update_draft(|draft| draft.worktree = Some(worktree));
        self.read_git(true, None::<FilesRead>, cx);
    }

    pub fn set_draft_base(&mut self, branch: String) {
        self.update_draft(|draft| draft.base = Some(branch.clone()));
        let Some(project_id) = self.selected_draft().and_then(|draft| draft.project_id.clone()) else { return };
        self.prefs.set(&format!("new.base-{project_id}"), branch);
    }

    /// Whether the project's server is new enough to list and switch branches.
    pub fn can_switch_branches(&self, project: &Project) -> bool {
        project.branch.is_some() && self.protocol_of(project) >= 3
    }

    /// Opens the branch picker at once, on the branches last listed or on placeholders, and lists
    /// them again.
    pub fn show_branches(&mut self, project: &Project) {
        self.listed_branches = self.known_branches.get(&project.id).cloned().map(Ok);
        self.branches_project_id = Some(project.id.clone());
        self.shows_branches = true;
        self.list_branches(project);
    }

    /// Lists the branches of the project the composer is on before the picker is opened.
    fn list_composer_branches(&mut self) {
        let Some(project) = self.composer_project().filter(|project| self.can_switch_branches(project)) else {
            return;
        };
        self.list_branches(&project);
    }

    fn list_branches(&mut self, project: &Project) {
        let request = Request::Branches { project_id: project.id.clone() };
        let project_id = project.id.clone();
        self.request_then(&project.server_id, request, move |store, result, _| {
            let listed = result
                .map_err(String::from)
                .map(|value| serde_json::from_value::<Vec<Branch>>(value["branches"].clone()).unwrap_or_default());
            if let Ok(branches) = &listed {
                store.known_branches.insert(project_id.clone(), branches.clone());
            }
            if store.branches_project_id.as_ref() != Some(&project_id) || !store.shows_branches {
                return;
            }
            store.listed_branches = Some(listed);
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
        self.request_then(&project.server_id, request, move |store, result, cx| {
            done(store, result.err().map(String::from), cx)
        });
    }

    /// Whether the project's server is new enough to commit, push and open pull requests.
    pub fn can_use_git(&self, project: &Project) -> bool {
        self.protocol_of(project) >= 4
    }

    /// Whether the project's folder is no git repository and its server can make it one.
    pub fn can_initialize_git(&self, project: &Project) -> bool {
        !project.no_project && project.branch.is_none() && project.git.is_none() && self.protocol_of(project) >= 16
    }

    pub fn initialize_git(&mut self, project: &Project) {
        if !self.initializing_git.insert(project.id.clone()) {
            return;
        }
        let project_id = project.id.clone();
        let request = Request::InitRepository { project_id: project.id.clone() };
        self.request_then(&project.server_id, request, move |store, result, _| {
            store.initializing_git.remove(&project_id);
            if let Err(error) = result {
                store.fail(error);
            }
        });
    }

    /// Has the server read the repository git works in from here again, which the project then
    /// arrives with. With `fetch` the remote is asked first, and a fetch that fails is said.
    pub fn read_git(
        &mut self,
        fetch: bool,
        done: Option<impl FnOnce(&mut Store, Vec<ChangedFile>, &mut Context<Store>) + 'static>,
        _cx: &mut Context<Self>,
    ) {
        if fetch {
            self.list_composer_branches();
        }
        let Some(project) = self.git_project() else { return };
        let thread_id = self.selected_thread().map(|thread| thread.id.clone());
        let request = Request::GitStatus { project_id: project.id.clone(), thread_id, fetch };
        let server_id = project.server_id.clone();
        self.request_then(&server_id, request, move |store, result, cx| match result {
            Ok(answer) => {
                if let Some(problem) = answer["problem"].as_str() {
                    let mut notice = GitNotice::new(&project.checkout_id(), "Couldn't fetch from the remote".into());
                    notice.description = Some(problem.to_string());
                    notice.failed = true;
                    store.show_git_notice(notice, cx);
                }
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
                    notice.description = Some(error.into());
                    notice.failed = true;
                    notice
                }
            };
            store.show_git_notice(notice, cx);
        });
    }

    /// What worked goes away by itself; what failed stays until it is closed.
    pub(super) fn show_git_notice(&mut self, mut notice: GitNotice, cx: &mut Context<Self>) {
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
