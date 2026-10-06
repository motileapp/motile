//! The Linear workspaces the servers are connected to, and the issues the Linear tabs list, as
//! the Mac app's `Core/Linear.swift` keeps them.

use std::collections::{HashMap, HashSet};

use gpui_kit::*;
use motile_core::api::Command;
use motile_core::linear::{Group, Page};
use motile_protocol::wire::{
    LinearChange, LinearConnection, LinearIssue, LinearIssueDetail, LinearStateKind, LinearTeam, LinearUser,
    NewLinearIssue, Request,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Store;
use crate::panel::state::{Loaded, PanelTab, PanelTarget};

/// The kinds of status in the order an issue goes through them, with what Linear calls them.
pub const KINDS: [(LinearStateKind, &str, &str); 6] = [
    (LinearStateKind::Triage, "triage", "Triage"),
    (LinearStateKind::Backlog, "backlog", "Backlog"),
    (LinearStateKind::Unstarted, "unstarted", "Todo"),
    (LinearStateKind::Started, "started", "In Progress"),
    (LinearStateKind::Completed, "completed", "Done"),
    (LinearStateKind::Canceled, "canceled", "Canceled"),
];

pub fn kind_of(name: &str) -> Option<LinearStateKind> {
    KINDS.iter().find(|(_, known, _)| *known == name).map(|(kind, _, _)| *kind)
}

pub fn state_symbol(kind: LinearStateKind) -> &'static str {
    match kind {
        LinearStateKind::Triage => "circle-dot-dashed",
        LinearStateKind::Backlog => "circle-dashed",
        LinearStateKind::Started => "circle-dot",
        LinearStateKind::Completed => "circle-check",
        LinearStateKind::Canceled => "circle-x",
        LinearStateKind::Unstarted => "circle",
    }
}

/// Linear's priorities in the order it offers them, with what it calls them.
pub const PRIORITIES: [(u8, &str); 5] = [(0, "No priority"), (1, "Urgent"), (2, "High"), (3, "Medium"), (4, "Low")];

pub fn priority_name(priority: u8) -> &'static str {
    PRIORITIES.iter().find(|(value, _)| *value == priority).map(|(_, name)| *name).unwrap_or("No priority")
}

pub fn priority_symbol(priority: u8) -> &'static str {
    match priority {
        1 => "triangle-alert",
        2 => "signal-high",
        3 => "signal-medium",
        4 => "signal-low",
        _ => "ellipsis",
    }
}

/// What the Linear tab lists for a project, kept so that it opens as it was left.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct LinearChoice {
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub team: Option<String>,
    #[serde(default = "yes")]
    pub mine: bool,
    /// The kinds of status listed, by name, or none for all of them.
    #[serde(default = "default_states")]
    pub states: Vec<String>,
}

fn yes() -> bool {
    true
}

fn default_states() -> Vec<String> {
    vec!["unstarted".into(), "started".into()]
}

impl Default for LinearChoice {
    fn default() -> Self {
        Self { workspace: None, team: None, mine: true, states: default_states() }
    }
}

/// What a list is asked with. Two threads of one project ask the same.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct Query {
    server_id: String,
    workspace: String,
    team: Option<String>,
    mine: bool,
    states: Vec<String>,
    search: String,
}

#[derive(Default)]
pub struct Linear {
    /// By server, as last heard.
    connections: HashMap<String, Vec<LinearConnection>>,
    /// The server that waits for the user to approve it at Linear.
    pub connecting: Option<String>,
    pub error: Option<String>,
    /// By workspace.
    pub teams: HashMap<String, Vec<LinearTeam>>,
    /// Who can be assigned, by workspace.
    pub users: HashMap<String, Vec<LinearUser>>,
    /// The lists read so far, by what asked for them, so that a tab opens as it was left.
    lists: HashMap<Query, Loaded<Vec<Group>>>,
    /// The open issues, by id.
    pub pages: HashMap<String, Loaded<Page>>,
    /// The issues that are being changed, those commented on as `comment:` and their id, those
    /// read as `read:` and their id, `issues` while the list is read and `create` while one is
    /// filed.
    pub working: HashSet<String>,
    /// By project, once read from the preferences.
    choices: HashMap<String, LinearChoice>,
    choices_loaded: bool,
    /// What the tab lists now.
    listed: Option<Query>,
    session: Option<crate::sign_in::Session>,
}

impl Linear {
    pub fn connected(&self, server_id: &str) -> &[LinearConnection] {
        self.connections.get(server_id).map(Vec::as_slice).unwrap_or_default()
    }

    /// The issues the tab lists; nothing while none have been asked for.
    pub fn issues(&self) -> Option<&Loaded<Vec<Group>>> {
        self.lists.get(self.listed.as_ref()?)
    }

    pub fn states_of(&self, workspace: &str, team: &str) -> Vec<motile_protocol::wire::LinearState> {
        let teams = self.teams.get(workspace);
        teams
            .and_then(|teams| teams.iter().find(|known| known.id == team))
            .map(|team| team.states.clone())
            .unwrap_or_default()
    }

    pub fn is_working(&self, key: &str) -> bool {
        self.working.contains(key)
    }
}

type Done = Box<dyn FnOnce(&mut Store, &mut Context<Store>)>;

impl Store {
    /// Why Linear can't be shown here, when it can't.
    pub fn linear_unavailable(&self) -> Option<String> {
        let target = self.panel_target()?;
        let server = self.server(Some(&target.server_id))?;
        (server.protocol_version < 12).then(|| format!("Update {} to connect it to Linear.", server.name))
    }

    /// What the tab lists for the project: what was chosen there last, in a workspace that is
    /// still connected.
    pub fn linear_choice(&self, target: &PanelTarget) -> LinearChoice {
        let kept = match self.linear.choices_loaded {
            true => self.linear.choices.get(&target.project_id).cloned(),
            false => self.kept_linear_choices().remove(&target.project_id),
        };
        let mut choice = kept.unwrap_or_default();
        let connected = self.linear.connected(&target.server_id);
        if connected.iter().any(|connection| Some(&connection.id) == choice.workspace.as_ref()) {
            return choice;
        }
        choice.workspace = connected.first().map(|connection| connection.id.clone());
        choice.team = None;
        choice
    }

    fn kept_linear_choices(&self) -> HashMap<String, LinearChoice> {
        self.prefs.get("linear.choices").unwrap_or_default()
    }

    fn load_linear_choices(&mut self) {
        if self.linear.choices_loaded {
            return;
        }
        self.linear.choices = self.kept_linear_choices();
        self.linear.choices_loaded = true;
    }

    pub fn linear_choose(&mut self, target: &PanelTarget, change: impl FnOnce(&mut LinearChoice)) {
        self.load_linear_choices();
        let mut choice = self.linear_choice(target);
        change(&mut choice);
        self.linear.choices.insert(target.project_id.clone(), choice);
        let choices = self.linear.choices.clone();
        self.prefs.set("linear.choices", choices);
    }

    /// Asks the server which workspaces it is connected to, showing the ones it had last time
    /// until it answers.
    pub fn linear_read(&mut self, server_id: &str) {
        let Some(server) = self.server(Some(server_id)) else { return };
        if server.protocol_version < 12 {
            return;
        }
        if !self.linear.connections.contains_key(server_id) {
            let kept: Option<Vec<LinearConnection>> = self.prefs.get(&format!("linear-{server_id}"));
            if let Some(kept) = kept {
                self.linear.connections.insert(server_id.to_string(), kept);
            }
        }
        self.linear_ask(server_id, Request::LinearStatus);
    }

    /// Has the user pick a workspace and approve Motile at Linear in the browser, then hands
    /// what Linear sent back to the server, which alone can finish with it.
    pub fn linear_connect(&mut self, server_id: &str) {
        if self.linear.connecting.is_some() {
            return;
        }
        self.linear.connecting = Some(server_id.to_string());
        self.linear.error = None;
        let server = server_id.to_string();
        self.request_then(server_id, Request::LinearConnect, move |store, result, cx| {
            let url = match result {
                Ok(answer) => answer["url"].as_str().map(String::from),
                Err(error) => {
                    store.linear.connecting = None;
                    store.linear.error = Some(error);
                    return;
                }
            };
            let Some(url) = url else {
                store.linear.connecting = None;
                store.linear.error = Some("The connection couldn't be started.".into());
                return;
            };
            let Some((session, answer)) = crate::sign_in::start(&url) else {
                // Without the system's sheet the browser can't hand the approval back.
                cx.open_url(&url);
                store.linear.connecting = None;
                return;
            };
            store.linear.session = Some(session);
            cx.spawn(async move |this, cx| {
                let callback = answer.await.ok().flatten();
                let _ = this.update(cx, |store, cx| {
                    store.linear.session = None;
                    store.linear_heard(&server, callback);
                    cx.notify();
                });
            })
            .detach();
        });
    }

    /// The browser has come back from Linear, with a code or a refusal.
    fn linear_heard(&mut self, server_id: &str, callback: Option<String>) {
        let sent = callback.map(|url| query_items(&url)).unwrap_or_default();
        let value = |name: &str| sent.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone());
        let (Some(code), Some(state)) = (value("code"), value("state")) else {
            self.linear.connecting = None;
            if let Some(refusal) = value("error").filter(|refusal| refusal != "access_denied") {
                self.linear.error = Some(format!("Linear didn't connect: {refusal}"));
            }
            return;
        };
        self.linear_ask(server_id, Request::LinearFinish { code, state });
    }

    pub fn linear_disconnect(&mut self, workspace: &str, server_id: &str) {
        self.linear_ask(server_id, Request::LinearDisconnect { workspace: workspace.to_string() });
    }

    /// Sends a request that `Linear` answers, with the server's workspaces.
    fn linear_ask(&mut self, server_id: &str, request: Request) {
        let finishing = matches!(request, Request::LinearFinish { .. });
        let asking_status = matches!(request, Request::LinearStatus);
        let server = server_id.to_string();
        self.request_then(server_id, request, move |store, result, _| {
            if finishing {
                store.linear.connecting = None;
            }
            let answer = match result {
                Ok(answer) => answer,
                Err(error) => {
                    if !asking_status {
                        store.linear.error = Some(error);
                    }
                    return;
                }
            };
            let before: HashSet<String> =
                store.linear.connected(&server).iter().map(|connection| connection.id.clone()).collect();
            let heard: Vec<LinearConnection> =
                serde_json::from_value(answer["connections"].clone()).unwrap_or_default();
            store.prefs.set(&format!("linear-{server}"), heard.clone());
            store.linear.connections.insert(server.clone(), heard);
            if !finishing {
                return;
            }
            let Some(target) = store.panel_target() else { return };
            let added = store.linear.connected(&server).iter().find(|connection| !before.contains(&connection.id));
            let Some(added) = added.map(|connection| connection.id.clone()) else { return };
            store.linear_choose(&target, |choice| {
                choice.workspace = Some(added);
                choice.team = None;
            });
        });
    }

    /// Asks the server for the issues the project's tab lists, or for the ones that have the
    /// words of `search`, and for the workspace's teams and users when they aren't known yet. A
    /// list read before shows at once while it is read again.
    pub fn linear_load(&mut self, target: &PanelTarget, search: &str) {
        self.load_linear_choices();
        let choice = self.linear_choice(target);
        let Some(workspace) = choice.workspace else { return };
        let query = Query {
            server_id: target.server_id.clone(),
            workspace: workspace.clone(),
            team: choice.team,
            mine: choice.mine,
            states: choice.states,
            search: search.trim().to_string(),
        };
        if let Some(listed) = self.linear.listed.take().filter(|listed| *listed != query && !listed.search.is_empty()) {
            self.linear.lists.remove(&listed);
        }
        self.linear.listed = Some(query.clone());
        if !self.linear.teams.contains_key(&workspace) {
            self.linear_load_teams(&workspace, &target.server_id);
        }
        self.linear_fetch(query);
    }

    fn linear_fetch(&mut self, query: Query) {
        self.linear.lists.entry(query.clone()).or_insert(Loaded::Loading);
        self.linear.working.insert("issues".into());
        let states: Vec<LinearStateKind> = query.states.iter().filter_map(|name| kind_of(name)).collect();
        // A server that doesn't read `states` yet goes by `closed`.
        let closed = states.is_empty()
            || states.iter().any(|kind| matches!(kind, LinearStateKind::Completed | LinearStateKind::Canceled));
        let request = Request::LinearIssues {
            workspace: query.workspace.clone(),
            team: query.team.clone(),
            mine: query.mine,
            closed,
            states,
            search: (!query.search.is_empty()).then(|| query.search.clone()),
        };
        let command = Command::Request { server_id: query.server_id.clone(), request };
        self.ask_read(command, read_groups, move |store, result, _| {
            let shown = store.linear.listed.as_ref() == Some(&query);
            if shown {
                store.linear.working.remove("issues");
            }
            match result.and_then(|read| read) {
                Ok(groups) => {
                    store.linear.lists.insert(query, Loaded::Ready(groups));
                    if shown {
                        store.linear.error = None;
                    }
                }
                Err(error) => {
                    if !shown {
                        return;
                    }
                    if store.linear.lists.get(&query).and_then(Loaded::value).is_none() {
                        store.linear.lists.insert(query.clone(), Loaded::Failed(error));
                    } else {
                        store.linear.error = Some(error);
                    }
                    // Linear may have ended the connection.
                    store.linear_ask(&query.server_id, Request::LinearStatus);
                }
            }
        });
    }

    /// Reads the issue for its tab.
    pub fn linear_load_issue(&mut self, id: &str, workspace: &str, server_id: &str) {
        let request = Request::LinearIssue { workspace: workspace.to_string(), issue: id.to_string() };
        self.linear_read_issue(server_id, request, id, false, None);
    }

    /// Says `body` on the issue, then reads it again; `done` runs once it has been said.
    pub fn linear_comment(
        &mut self,
        id: &str,
        workspace: &str,
        server_id: &str,
        body: String,
        done: impl FnOnce(&mut Store, &mut Context<Store>) + 'static,
    ) {
        let request = Request::LinearComment { workspace: workspace.to_string(), issue: id.to_string(), body };
        self.linear_read_issue(server_id, request, id, true, Some(Box::new(done)));
    }

    fn linear_read_issue(&mut self, server_id: &str, request: Request, id: &str, commenting: bool, done: Option<Done>) {
        if commenting {
            self.linear.working.insert(format!("comment:{id}"));
        }
        if self.linear.pages.get(id).and_then(Loaded::value).is_none() {
            self.linear.pages.insert(id.to_string(), Loaded::Loading);
        }
        self.linear.working.insert(format!("read:{id}"));
        let id = id.to_string();
        let command = Command::Request { server_id: server_id.to_string(), request };
        self.ask_read(command, read_page, move |store, result, cx| {
            store.linear.working.remove(&format!("read:{id}"));
            if commenting {
                store.linear.working.remove(&format!("comment:{id}"));
            }
            match result.and_then(|read| read) {
                Ok(page) => {
                    store.linear.pages.insert(id, Loaded::Ready(page));
                    store.linear.error = None;
                    if let Some(done) = done {
                        done(store, cx);
                    }
                }
                Err(error) => {
                    if store.linear.pages.get(&id).and_then(Loaded::value).is_none() {
                        store.linear.pages.insert(id, Loaded::Failed(error));
                    } else {
                        store.linear.error = Some(error);
                    }
                }
            }
        });
    }

    /// Changes the issue's status, assignee or priority at Linear, then reads what shows it again.
    pub fn linear_change(&mut self, id: &str, change: LinearChange, workspace: &str, server_id: &str) {
        if self.linear.working.contains(id) {
            return;
        }
        self.linear.working.insert(id.to_string());
        let (id, workspace, server) = (id.to_string(), workspace.to_string(), server_id.to_string());
        let request = Request::LinearUpdate { workspace: workspace.clone(), issue: id.clone(), change };
        self.request_then(server_id, request, move |store, result, _| {
            store.linear.working.remove(&id);
            if let Err(error) = result {
                store.linear.error = Some(error);
            }
            store.linear_refresh(Some(&id), &workspace, &server);
        });
    }

    /// Files an issue; `done` gets its id and identifier.
    pub fn linear_create(
        &mut self,
        issue: NewLinearIssue,
        workspace: &str,
        server_id: &str,
        done: impl FnOnce(&mut Store, String, String, &mut Context<Store>) + 'static,
    ) {
        if self.linear.working.contains("create") {
            return;
        }
        self.linear.working.insert("create".into());
        let (workspace, server) = (workspace.to_string(), server_id.to_string());
        let request = Request::LinearCreate { workspace: workspace.clone(), issue };
        self.request_then(server_id, request, move |store, result, cx| {
            store.linear.working.remove("create");
            match result {
                Ok(answer) => {
                    let filed = &answer["issue"];
                    let id = filed["id"].as_str().unwrap_or_default().to_string();
                    let identifier = filed["identifier"].as_str().unwrap_or_default().to_string();
                    store.linear.error = None;
                    store.linear_refresh(None, &workspace, &server);
                    done(store, id, identifier, cx);
                }
                Err(error) => store.linear.error = Some(error),
            }
        });
    }

    /// Puts what the issue asks for in the open draft's composer, or from a thread in that of a
    /// new draft. An issue nobody works on yet is taken: assigned to the user and moved to the
    /// first status of work.
    pub fn linear_work(&mut self, page: &Page, workspace: &str, target: &PanelTarget, cx: &mut Context<Self>) {
        let mut taking = LinearChange::default();
        let states = self.linear.states_of(workspace, &page.row.team);
        let waiting =
            matches!(page.state.kind, LinearStateKind::Triage | LinearStateKind::Backlog | LinearStateKind::Unstarted);
        if waiting && let Some(started) = states.iter().find(|state| state.kind == LinearStateKind::Started) {
            taking.state = Some(started.id.clone());
        }
        let me = self.linear.users.get(workspace).and_then(|users| users.iter().find(|user| user.me));
        if page.row.assignee_id.is_none()
            && let Some(me) = me
        {
            taking.assignee = Some(me.id.clone());
        }
        if taking != LinearChange::default() {
            self.linear_change(&page.row.id, taking, workspace, &target.server_id);
        }
        self.linear_start_work(page.prompt.clone(), &target.project_id, cx);
    }

    /// Puts the prompt in the composer of the open draft. From a thread it opens a draft in the
    /// project first, which takes the panel's open tab along.
    fn linear_start_work(&mut self, prompt: String, project_id: &str, cx: &mut Context<Self>) {
        if self.selected_draft().is_none() {
            let tab = self.panel_tabs().active;
            self.start_new_thread(Some(project_id.to_string()), cx);
            if let Some(tab) = tab {
                self.open_tab(tab);
            }
        }
        let written = self.draft().trim().to_string();
        let text = if written.is_empty() { prompt } else { format!("{written}\n\n{prompt}") };
        self.set_draft(text);
        if self.panel_maximized() {
            self.toggle_panel_maximized();
        }
        self.draft_version += 1;
        self.composer_focus += 1;
    }

    fn linear_refresh(&mut self, id: Option<&str>, workspace: &str, server_id: &str) {
        if let Some(id) = id.filter(|id| self.linear.pages.contains_key(*id)) {
            self.linear_load_issue(id, workspace, server_id);
        }
        if let Some(listed) = self.linear.listed.clone().filter(|listed| listed.server_id == server_id) {
            self.linear_fetch(listed);
        }
    }

    fn linear_load_teams(&mut self, workspace: &str, server_id: &str) {
        let workspace = workspace.to_string();
        self.request_then(server_id, Request::LinearTeams { workspace: workspace.clone() }, move |store, result, _| {
            let Ok(answer) = result else { return };
            let teams = serde_json::from_value(answer["teams"].clone()).unwrap_or_default();
            let users = serde_json::from_value(answer["users"].clone()).unwrap_or_default();
            store.linear.teams.insert(workspace.clone(), teams);
            store.linear.users.insert(workspace, users);
        });
    }

    /// The teams and users of the workspace, asked for when they aren't known yet.
    pub fn linear_need_teams(&mut self, workspace: &str, server_id: &str) {
        if self.linear.teams.contains_key(workspace) {
            return;
        }
        self.linear_load_teams(workspace, server_id);
    }

    pub fn open_linear_issue(&mut self, workspace: String, id: String, identifier: String) {
        self.open_tab(PanelTab::LinearIssue { workspace, id, identifier });
    }
}

fn read_groups(answer: Value) -> Result<Vec<Group>, String> {
    if answer["type"] != "linear_issues" {
        return Err(unexpected(&answer));
    }
    let issues: Vec<LinearIssue> =
        serde_json::from_value(answer["issues"].clone()).map_err(|error| error.to_string())?;
    Ok(motile_core::linear::groups(&issues))
}

fn read_page(answer: Value) -> Result<Page, String> {
    if answer["type"] != "linear_issue_detail" {
        return Err(unexpected(&answer));
    }
    let detail: LinearIssueDetail =
        serde_json::from_value(answer["detail"].clone()).map_err(|error| error.to_string())?;
    Ok(motile_core::linear::page(&detail))
}

fn unexpected(answer: &Value) -> String {
    answer["message"].as_str().map(String::from).unwrap_or_else(|| "Your server answered with something else.".into())
}

/// The names and values of a URL's query.
fn query_items(url: &str) -> Vec<(String, String)> {
    let Some((_, query)) = url.split_once('?') else { return Vec::new() };
    let query = query.split('#').next().unwrap_or_default();
    query
        .split('&')
        .filter(|item| !item.is_empty())
        .map(|item| {
            let (name, value) = item.split_once('=').unwrap_or((item, ""));
            (decode(name), decode(value))
        })
        .collect()
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = (bytes[index] == b'%' && index + 2 < bytes.len())
            .then(|| std::str::from_utf8(&bytes[index + 1..index + 3]).ok())
            .flatten()
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match hex {
            Some(byte) => {
                out.push(byte);
                index += 3;
            }
            None => {
                out.push(if bytes[index] == b'+' { b' ' } else { bytes[index] });
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_query_of_a_callback_is_read() {
        let items = query_items("motile://linear?code=abc%20d&state=s1#x");
        assert_eq!(items, vec![("code".to_string(), "abc d".to_string()), ("state".to_string(), "s1".to_string())]);
        assert!(query_items("motile://linear").is_empty());
    }
}
