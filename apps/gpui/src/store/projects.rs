//! Projects, the command panel that adds them, and the settings of the servers they are on.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Command;
use motile_protocol::wire::{GitHubState, Repo, Request};

use super::{AddingProject, Notice, PanelPage, Selection, Store};
use crate::bridge::CoreError;
use crate::models::{FolderListing, Project, ProjectImage, Server};

/// Which servers are listing their GitHub repositories now, and when each last did.
#[derive(Default)]
pub struct RepoListing {
    pub listing: HashSet<String>,
    listed: HashMap<String, Instant>,
}

impl RepoListing {
    /// How long a list stays fresh enough not to be asked for again.
    const FRESH_FOR: Duration = Duration::from_secs(60);

    fn is_fresh(&self, server_id: &str) -> bool {
        self.listed.get(server_id).is_some_and(|listed| listed.elapsed() < Self::FRESH_FOR)
    }
}

fn github_state(value: &serde_json::Value) -> Option<GitHubState> {
    serde_json::from_value(value["state"].clone()).ok()
}

impl Store {
    pub(super) fn apply_projects(&mut self, new: Vec<Project>, server_id: &str, cx: &mut Context<Self>) {
        let git_before = self.git_project().and_then(|project| project.git);
        let (none, folders): (Vec<Project>, Vec<Project>) = new.into_iter().partition(|project| project.no_project);
        self.projects.retain(|project| project.server_id != server_id);
        self.projects.extend(folders);
        self.projects.sort_by(|a, b| a.created_at.total_cmp(&b.created_at));
        self.no_projects.retain(|project| project.server_id != server_id);
        self.no_projects.extend(none);
        let order: Vec<&str> = self.servers.iter().map(|server| server.id.as_str()).collect();
        let place = |project: &Project| order.iter().position(|id| *id == project.server_id).unwrap_or(order.len());
        self.no_projects.sort_by_key(|project| place(project));
        self.ensure_draft_project();
        self.pick_awaited_folder(cx);
        self.open_awaited_project(cx);
        // A server that has just started hasn't read the repository yet.
        if let Some(project) = self.git_project()
            && project.server_id == server_id
            && project.git.is_none()
        {
            self.read_git(
                false,
                None::<fn(&mut Store, Vec<motile_protocol::wire::ChangedFile>, &mut Context<Store>)>,
                cx,
            );
        }
        if self.git_project().and_then(|project| project.git) != git_before {
            self.workspace_version += 1;
        }
    }

    /// Adds the folder as a project and opens a thread in it. A folder that is a project already
    /// is opened where it was left.
    pub fn add_project_at(&mut self, server_id: &str, path: String, cx: &mut Context<Self>) {
        let trimmed = if path.len() > 1 && path.ends_with('/') { &path[..path.len() - 1] } else { &path[..] };
        let trimmed = trimmed.to_string();
        let added = self.projects.iter().find(|project| project.server_id == server_id && project.path == trimmed);
        if let Some(project) = added.cloned() {
            return self.go_to(&project, cx);
        }
        let name = trimmed.rsplit('/').find(|part| !part.is_empty()).unwrap_or(&trimmed).to_string();
        let server = server_id.to_string();
        self.request_then(server_id, Request::AddProject { path }, move |store, result, cx| {
            if let Err(error) = result {
                return store.show_notice(Notice::failure(format!("Couldn't add {name}"), error.into()), cx);
            }
            store.awaited_folder = Some((server, trimmed));
            store.pick_awaited_folder(cx);
        });
    }

    fn pick_awaited_folder(&mut self, cx: &mut Context<Self>) {
        let Some((server_id, path)) = &self.awaited_folder else { return };
        let found = self.projects.iter().find(|project| &project.server_id == server_id && &project.path == path);
        let Some(project) = found.cloned() else { return };
        self.awaited_folder = None;
        self.open_thread_in(&project, cx);
    }

    /// Opens a thread in the project: the open draft, moved into it, or a new one.
    pub(super) fn open_thread_in(&mut self, project: &Project, cx: &mut Context<Self>) {
        self.close_settings();
        let movable = self.selected_draft().is_some_and(|draft| !self.is_sending(&draft.id));
        if !movable {
            return self.start_new_thread(Some(project.id.clone()), cx);
        }
        self.set_new_thread_project(Some(project.id.clone()), cx);
    }

    /// Goes to the project's latest thread that is still going, or opens a new one in it.
    fn go_to(&mut self, project: &Project, cx: &mut Context<Self>) {
        let latest = self.active_threads().iter().find(|thread| thread.project_id == project.id);
        let Some(id) = latest.map(|thread| thread.id.clone()) else { return self.open_thread_in(project, cx) };
        self.close_settings();
        self.select(Selection::Thread(id), cx);
    }

    /// Opens the panel on the ways to add a project, after the servers when there is a choice.
    pub fn add_project(&mut self) {
        let page = self.add_project_page();
        self.open_panel(page);
    }

    pub fn add_project_page(&self) -> PanelPage {
        let connected: Vec<&Server> = self.servers.iter().filter(|server| server.connected()).collect();
        match connected.as_slice() {
            [server] => PanelPage::Sources(server.id.clone()),
            _ => PanelPage::Servers,
        }
    }

    /// Whether the server can start a project from a name or from GitHub.
    pub fn starts_projects(&self, server: Option<&Server>) -> bool {
        server.map_or(0, |server| server.protocol_version) >= 5
    }

    /// Lists the folders under the typed path.
    pub fn browse(
        &mut self,
        server_id: &str,
        query: String,
        done: impl FnOnce(&mut Store, Result<FolderListing, String>, &mut Context<Store>) + 'static,
    ) {
        let command = Command::Browse { server_id: server_id.to_string(), query };
        self.ask(command, move |store, result, cx| {
            let listing = result
                .map_err(String::from)
                .map(|value| serde_json::from_value::<FolderListing>(value).unwrap_or_default());
            done(store, listing, cx);
        });
    }

    /// The images in the project's folder that fit `query`, the best first.
    pub fn project_images(
        &mut self,
        project: &Project,
        query: String,
        done: impl FnOnce(&mut Store, Result<Vec<ProjectImage>, String>, &mut Context<Store>) + 'static,
    ) {
        let command =
            Command::ProjectImages { server_id: project.server_id.clone(), project_id: project.id.clone(), query };
        self.ask(command, move |store, result, cx| {
            let images = result
                .map_err(String::from)
                .map(|value| serde_json::from_value::<Vec<ProjectImage>>(value["images"].clone()).unwrap_or_default());
            done(store, images, cx);
        });
    }

    pub fn read_github(&mut self, server_id: &str) {
        let server = server_id.to_string();
        self.request_then(server_id, Request::GithubStatus, move |store, result, _| {
            let Ok(answer) = result else { return };
            store.heard_github(&answer, &server);
        });
    }

    fn heard_github(&mut self, answer: &serde_json::Value, server_id: &str) {
        let Some(state) = github_state(answer) else { return };
        self.github.insert(server_id.to_string(), state);
        self.prefs.set(&format!("github-{server_id}"), state);
    }

    /// Asks the server for its GitHub repositories, unless it listed them in the last minute or
    /// `fresh` asks again anyway. The ones it listed before stay until it answers.
    pub fn load_repos(&mut self, server_id: &str, fresh: bool) {
        if self.repo_listing.listing.contains(server_id) {
            return;
        }
        if !fresh && self.repo_listing.is_fresh(server_id) {
            return;
        }
        self.repo_listing.listing.insert(server_id.to_string());
        self.repo_errors.remove(server_id);
        let server = server_id.to_string();
        self.request_then(server_id, Request::GithubRepos, move |store, result, _| {
            store.repo_listing.listing.remove(&server);
            match result {
                Ok(answer) if answer["type"] == "repos" => {
                    let repos: Vec<Repo> = serde_json::from_value(answer["repos"].clone()).unwrap_or_default();
                    store.repos.insert(server.clone(), repos);
                    store.repo_listing.listed.insert(server, Instant::now());
                }
                Ok(answer) => {
                    store.repos.remove(&server);
                    store.heard_github(&answer, &server);
                }
                Err(error) => {
                    store.repo_errors.insert(server, error.into());
                }
            }
        });
    }

    /// Makes the project and opens a thread in it. The panel waits, so that a name that is taken
    /// can be changed.
    pub fn new_project_named(&mut self, name: String, server_id: &str) {
        if self.adding_project.is_some() {
            return;
        }
        self.adding_project = Some(AddingProject { server_id: server_id.to_string(), name: name.clone() });
        self.panel_notice = None;
        let arrived = |store: &mut Store, project: Project, cx: &mut Context<Store>| {
            store.adding_project = None;
            store.open_thread_in(&project, cx);
        };
        self.add(Request::NewProject { name }, server_id, arrived, |store, error, _| {
            store.adding_project = None;
            if store.panel.is_none() {
                return store.fail(error);
            }
            store.panel_notice = Some(error.into());
        });
    }

    /// Clones the repository while the notice says so, and opens a thread in it once it is there.
    /// Someone who has gone elsewhere meanwhile is offered the thread instead.
    pub fn clone_repo(&mut self, repo: String, server_id: &str, cx: &mut Context<Self>) {
        let into = self.server(Some(server_id)).map(|server| format!(" on {}", server.name)).unwrap_or_default();
        let cloning = Notice::new(format!("Cloning {repo}"), Some(format!("Into ~/projects{into}")));
        self.show_notice(Notice { pending: true, ..cloning }, cx);
        let from = self.selection.clone();
        let failed_repo = repo.clone();
        let arrived = move |store: &mut Store, project: Project, cx: &mut Context<Store>| {
            let cloned = Notice::new(format!("Cloned {repo}"), Some(project.path.clone()));
            if store.selection != from {
                return store.show_notice(Notice { project_id: Some(project.id.clone()), ..cloned }, cx);
            }
            store.show_notice(cloned, cx);
            store.open_thread_in(&project, cx);
        };
        self.add(Request::CloneRepo { repo: failed_repo.clone() }, server_id, arrived, move |store, error, cx| {
            store.show_notice(Notice::failure(format!("Couldn't clone {failed_repo}"), error.into()), cx);
        });
    }

    fn add(
        &mut self,
        request: Request,
        server_id: &str,
        arrived: impl FnOnce(&mut Store, Project, &mut Context<Store>) + 'static,
        failed: impl FnOnce(&mut Store, CoreError, &mut Context<Store>) + 'static,
    ) {
        self.request_then(server_id, request, |store, result, cx| match result {
            Ok(answer) => {
                store.projects_added += 1;
                let id = answer["project_id"].as_str().unwrap_or_default().to_string();
                store.awaited_project = Some((id, Box::new(arrived)));
                store.open_awaited_project(cx);
            }
            Err(error) => failed(store, error, cx),
        });
    }

    /// Does what was planned for the project that was just made, once the server has told about it.
    fn open_awaited_project(&mut self, cx: &mut Context<Self>) {
        let Some((id, _)) = &self.awaited_project else { return };
        let Some(project) = self.project(Some(id)).cloned() else { return };
        let Some((_, arrived)) = self.awaited_project.take() else { return };
        arrived(self, project, cx);
    }

    /// Picks the model that writes titles, commit messages and pull requests on the server.
    /// Without one, the lightest model of the thread's agent writes.
    pub fn set_text_model(&mut self, model: Option<String>, server_id: &str) {
        self.ask(Command::SetTextModel { server_id: server_id.to_string(), model }, |store, result, _| {
            if let Err(error) = result {
                store.fail(error);
            }
        });
    }

    /// Says how the server's writer names branches. Without instructions the server goes back
    /// to its own.
    pub fn set_branch_instructions(&mut self, instructions: Option<String>, server_id: &str) {
        let command = Command::SetBranchInstructions { server_id: server_id.to_string(), instructions };
        self.ask(command, |store, result, _| {
            if let Err(error) = result {
                store.fail(error);
            }
        });
    }

    /// Sets the shell script that runs in every new worktree of the project, or takes it away.
    pub fn set_setup(&mut self, project: &Project, script: String) {
        let script = (!script.trim().is_empty()).then_some(script);
        self.request(&project.server_id, Request::SetProjectSetup { project_id: project.id.clone(), script });
    }

    pub fn remove_project(&mut self, project: &Project) {
        self.request(&project.server_id, Request::RemoveProject { project_id: project.id.clone() });
    }

    /// Makes an image on the project's server its icon. Without one, the project goes back to the
    /// icon found in its folder.
    pub fn set_icon(&mut self, project: &Project, path: Option<String>) {
        let command =
            Command::SetProjectIcon { server_id: project.server_id.clone(), project_id: project.id.clone(), path };
        self.ask(command, |store, result, _| {
            if let Err(error) = result {
                store.fail(error);
            }
        });
    }

    // Command panel

    /// Opens the command panel, also where only unlinked servers' threads are left to find.
    pub fn open_panel(&mut self, page: PanelPage) {
        if !self.account.signed_in || self.servers.is_empty() {
            return;
        }
        self.panel = Some(page);
        let servers: Vec<String> = self
            .servers
            .iter()
            .filter(|server| server.connected() && self.starts_projects(Some(server)))
            .map(|server| server.id.clone())
            .collect();
        for server_id in servers {
            if !self.github.contains_key(&server_id)
                && let Some(state) = self.prefs.get::<GitHubState>(&format!("github-{server_id}"))
            {
                self.github.insert(server_id.clone(), state);
            }
            self.read_github(&server_id);
        }
    }

    pub fn close_panel(&mut self) {
        if self.panel.take().is_none() {
            return;
        }
        self.panel_notice = None;
        self.composer_focus += 1;
    }
}
