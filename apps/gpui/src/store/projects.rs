//! Projects, the command panel that adds them, and the settings of the servers they are on.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Command;
use motile_protocol::wire::{GitHubState, Repo, Request};

use super::{AddingProject, PanelPage, Store};
use crate::models::{FolderListing, Project, RemoteFolder, Server};

fn github_state(value: &serde_json::Value) -> Option<GitHubState> {
    serde_json::from_value(value["state"].clone()).ok()
}

impl Store {
    pub(super) fn apply_projects(&mut self, new: Vec<Project>, server_id: &str, cx: &mut Context<Self>) {
        let git_before = self.git_project().and_then(|project| project.git);
        self.projects.retain(|project| project.server_id != server_id);
        self.projects.extend(new);
        self.projects.sort_by(|a, b| a.created_at.total_cmp(&b.created_at));
        self.ensure_draft_project();
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

    pub fn list_folder(
        &mut self,
        server_id: &str,
        path: Option<String>,
        icons: bool,
        done: impl FnOnce(&mut Store, Result<RemoteFolder, String>, &mut Context<Store>) + 'static,
    ) {
        let request = Request::ListDir { path, icons, hidden: false };
        self.request_then(server_id, request, move |store, result, cx| {
            let folder = result.map(|value| serde_json::from_value::<RemoteFolder>(value).unwrap_or_default());
            done(store, folder, cx);
        });
    }

    pub fn add_project_at(&mut self, server_id: &str, path: String) {
        let server = server_id.to_string();
        self.request_then(server_id, Request::AddProject { path: path.clone() }, move |store, result, cx| {
            if let Err(error) = result {
                store.fail(error);
                return;
            }
            // The new project is the one the next thread starts in.
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(300)).await;
                let _ = this.update(cx, |store, cx| {
                    let trimmed =
                        if path.len() > 1 && path.ends_with('/') { &path[..path.len() - 1] } else { &path[..] };
                    let found =
                        store.projects.iter().find(|project| project.server_id == server && project.path == trimmed);
                    let Some(id) = found.map(|project| project.id.clone()) else { return };
                    store.set_new_thread_project(Some(id), cx);
                    cx.notify();
                });
            })
            .detach();
        });
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

    pub fn browse(
        &mut self,
        server_id: &str,
        query: String,
        done: impl FnOnce(&mut Store, Result<FolderListing, String>, &mut Context<Store>) + 'static,
    ) {
        let command = Command::Browse { server_id: server_id.to_string(), query };
        self.ask(command, move |store, result, cx| {
            let listing = result.map(|value| serde_json::from_value::<FolderListing>(value).unwrap_or_default());
            done(store, listing, cx);
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

    /// Asks the server for its GitHub repositories. The ones it listed before stay until then.
    pub fn load_repos(&mut self, server_id: &str) {
        self.repo_errors.remove(server_id);
        let server = server_id.to_string();
        self.request_then(server_id, Request::GithubRepos, move |store, result, _| match result {
            Ok(answer) if answer["type"] == "repos" => {
                let repos: Vec<Repo> = serde_json::from_value(answer["repos"].clone()).unwrap_or_default();
                store.repos.insert(server, repos);
            }
            Ok(answer) => {
                store.repos.remove(&server);
                store.heard_github(&answer, &server);
            }
            Err(error) => {
                store.repo_errors.insert(server, error);
            }
        });
    }

    pub fn new_project_named(&mut self, name: String, server_id: &str) {
        self.add(Request::NewProject { name: name.clone() }, name, server_id);
    }

    pub fn clone_repo(&mut self, repo: String, server_id: &str) {
        self.add(Request::CloneRepo { repo: repo.clone() }, repo, server_id);
    }

    fn add(&mut self, request: Request, name: String, server_id: &str) {
        if self.adding_project.is_some() {
            return;
        }
        self.adding_project = Some(AddingProject { server_id: server_id.to_string(), name });
        self.panel_notice = None;
        self.request_then(server_id, request, |store, result, cx| {
            store.adding_project = None;
            match result {
                Ok(answer) => {
                    store.projects_added += 1;
                    store.awaited_project_id = answer["project_id"].as_str().map(String::from);
                    store.open_awaited_project(cx);
                }
                Err(error) if store.panel.is_some() => store.panel_notice = Some(error),
                Err(error) => store.fail(error),
            }
        });
    }

    /// Starts a thread in the project that was just made, once the server has told about it.
    fn open_awaited_project(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.project(self.awaited_project_id.as_deref()).map(|project| project.id.clone()) else {
            return;
        };
        self.awaited_project_id = None;
        if self.selected_draft().is_some() {
            return self.set_new_thread_project(Some(id), cx);
        }
        self.start_new_thread(Some(id), cx);
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
