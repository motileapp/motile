//! The Ricrac workspaces the servers are connected to, and the issues the Ricrac tabs list, as
//! the Mac app's `Core/Ricrac.swift` keeps them.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui_kit::*;
use motile_core::api::Command;
use motile_protocol::wire::{
    Label, NewRicracIssue, Request, RicracChange, RicracConnection, RicracState, RicracStateKind, RicracTeam,
    RicracUser,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Store;
use super::integrations::Integration;
use super::linear::query_items;
use super::pull_requests::{Text, read_text};
use crate::panel::state::{Loaded, PanelTab, PanelTarget};
use crate::prefs::Prefs;

/// The issues of one status, as the core groups them.
#[derive(Deserialize, Clone, Debug)]
pub struct Group {
    pub state: RicracState,
    pub rows: Vec<Row>,
}

/// An issue as the list shows it.
#[derive(Deserialize, Clone, Debug)]
pub struct Row {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub url: String,
    pub priority: u8,
    pub priority_label: String,
    pub team: String,
    pub assignee: Option<String>,
    pub assignee_id: Option<String>,
    /// The assignee's first letters, for where the name has no room.
    pub initials: Option<String>,
    pub labels: Vec<Label>,
    pub updated_at: f64,
}

/// One issue as its tab shows it, its Markdown ready to draw.
#[derive(Deserialize, Clone, Debug)]
pub struct Page {
    pub row: Row,
    pub state: RicracState,
    #[serde(deserialize_with = "read_text")]
    pub description: Vec<Text>,
    /// The oldest first.
    pub comments: Vec<Comment>,
    /// What hands the issue to an agent.
    pub prompt: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub initials: String,
    pub at: f64,
    #[serde(deserialize_with = "read_text")]
    pub body: Vec<Text>,
}

/// The kinds of status in the order an issue goes through them, with what Ricrac calls them.
pub const KINDS: [(RicracStateKind, &str, &str); 6] = [
    (RicracStateKind::Triage, "triage", "Triage"),
    (RicracStateKind::Backlog, "backlog", "Backlog"),
    (RicracStateKind::Unstarted, "unstarted", "Todo"),
    (RicracStateKind::Started, "started", "In Progress"),
    (RicracStateKind::Completed, "completed", "Done"),
    (RicracStateKind::Canceled, "canceled", "Canceled"),
];

pub fn kind_of(name: &str) -> Option<RicracStateKind> {
    KINDS.iter().find(|(_, known, _)| *known == name).map(|(kind, _, _)| *kind)
}

pub fn state_symbol(kind: RicracStateKind) -> &'static str {
    match kind {
        RicracStateKind::Triage => "circle-dot-dashed",
        RicracStateKind::Backlog => "circle-dashed",
        RicracStateKind::Started => "circle-dot",
        RicracStateKind::Completed => "circle-check",
        RicracStateKind::Canceled => "circle-x",
        RicracStateKind::Unstarted => "circle",
    }
}

/// Ricrac's priorities in the order it offers them, with what it calls them.
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

/// The workspace and team the Ricrac tab lists for a project.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
struct RicracPick {
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    team: Option<String>,
}

/// How the Ricrac tab lists issues, the same in every project.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct RicracFilter {
    #[serde(default = "yes")]
    pub mine: bool,
    /// The kinds of status listed, by name, or none for all of them.
    #[serde(default = "default_states")]
    pub states: Vec<String>,
    /// The statuses folded up, by id.
    #[serde(default)]
    pub folded: HashSet<String>,
}

/// What the Ricrac tab lists for a project.
#[derive(Clone, PartialEq, Debug)]
pub struct RicracChoice {
    pub workspace: Option<String>,
    pub team: Option<String>,
    pub mine: bool,
    pub states: Vec<String>,
}

fn yes() -> bool {
    true
}

fn default_states() -> Vec<String> {
    vec!["unstarted".into(), "started".into()]
}

impl Default for RicracFilter {
    fn default() -> Self {
        Self { mine: true, states: default_states(), folded: HashSet::new() }
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
pub struct Ricrac {
    /// By server, as last heard.
    connections: HashMap<String, Vec<RicracConnection>>,
    /// The server that waits for the user to approve it at Ricrac.
    pub connecting: Option<String>,
    pub error: Option<String>,
    /// By workspace.
    pub teams: HashMap<String, Vec<RicracTeam>>,
    /// Who can be assigned, by workspace.
    pub users: HashMap<String, Vec<RicracUser>>,
    /// The lists read so far, by what asked for them, so that a tab opens as it was left.
    lists: HashMap<Query, Loaded<Rc<Vec<Group>>>>,
    /// The open issues, by id.
    pub pages: HashMap<String, Loaded<Page>>,
    /// The issues that are being changed, those commented on as `comment:` and their id, those
    /// read as `read:` and their id, `issues` while the list is read and `create` while one is
    /// filed.
    pub working: HashSet<String>,
    /// By project.
    picks: HashMap<String, RicracPick>,
    /// What was picked last, which a project that has none of its own starts from.
    last_pick: Option<RicracPick>,
    pub filter: RicracFilter,
    /// What the tab searches for, by project, while the app runs.
    pub searches: HashMap<String, String>,
    /// What the tab lists now.
    listed: Option<Query>,
    session: Option<crate::sign_in::Session>,
}

impl Ricrac {
    pub fn kept(prefs: &Prefs) -> Self {
        Self {
            picks: prefs.get("ricrac.choices").unwrap_or_default(),
            last_pick: prefs.get("ricrac.last"),
            filter: prefs.get("ricrac.filter").unwrap_or_default(),
            ..Self::default()
        }
    }

    pub fn connected(&self, server_id: &str) -> &[RicracConnection] {
        self.connections.get(server_id).map(Vec::as_slice).unwrap_or_default()
    }

    /// The issues the tab lists; nothing while none have been asked for.
    pub fn issues(&self) -> Option<&Loaded<Rc<Vec<Group>>>> {
        self.lists.get(self.listed.as_ref()?)
    }

    pub fn states_of(&self, workspace: &str, team: &str) -> Vec<motile_protocol::wire::RicracState> {
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
    /// Why Ricrac can't be shown here, when it can't.
    pub fn ricrac_unavailable(&self) -> Option<String> {
        let target = self.panel_target()?;
        let server = self.server(Some(&target.server_id))?;
        (server.protocol_version < 29).then(|| format!("Update {} to connect it to Ricrac.", server.name))
    }

    /// The panel's server is connected to a Ricrac workspace.
    pub fn ricrac_connected(&self) -> bool {
        if !self.is_on(Integration::Ricrac) {
            return false;
        }
        let Some(target) = self.panel_target().filter(|_| self.ricrac_unavailable().is_none()) else { return false };
        !self.ricrac.connected(&target.server_id).is_empty()
    }

    pub fn ricrac_choice(&self, target: &PanelTarget) -> RicracChoice {
        let pick = self.ricrac_picked(target);
        let filter = &self.ricrac.filter;
        RicracChoice { workspace: pick.workspace, team: pick.team, mine: filter.mine, states: filter.states.clone() }
    }

    /// What was picked for the project last, or else for any project, in a workspace the server
    /// is still connected to.
    fn ricrac_picked(&self, target: &PanelTarget) -> RicracPick {
        let connected = self.ricrac.connected(&target.server_id);
        let kept = [self.ricrac.picks.get(&target.project_id), self.ricrac.last_pick.as_ref()];
        let still = kept
            .into_iter()
            .flatten()
            .find(|pick| connected.iter().any(|connection| Some(&connection.id) == pick.workspace.as_ref()));
        let Some(pick) = still else {
            return RicracPick { workspace: connected.first().map(|connection| connection.id.clone()), team: None };
        };
        pick.clone()
    }

    pub fn ricrac_pick(&mut self, target: &PanelTarget, workspace: Option<String>, team: Option<String>) {
        let pick = RicracPick { workspace, team };
        self.ricrac.picks.insert(target.project_id.clone(), pick.clone());
        self.ricrac.last_pick = Some(pick.clone());
        self.prefs.set("ricrac.choices", self.ricrac.picks.clone());
        self.prefs.set("ricrac.last", pick);
    }

    pub fn ricrac_change_filter(&mut self, change: impl FnOnce(&mut RicracFilter)) {
        change(&mut self.ricrac.filter);
        self.prefs.set("ricrac.filter", self.ricrac.filter.clone());
    }

    pub fn ricrac_fold(&mut self, state: &str) {
        self.ricrac_change_filter(|filter| {
            if !filter.folded.remove(state) {
                filter.folded.insert(state.to_string());
            }
        });
    }

    /// Asks the server which workspaces it is connected to, showing the ones it had last time
    /// until it answers.
    pub fn ricrac_read(&mut self, server_id: &str) {
        let Some(server) = self.server(Some(server_id)) else { return };
        if server.protocol_version < 29 {
            return;
        }
        if !self.ricrac.connections.contains_key(server_id) {
            let kept: Option<Vec<RicracConnection>> = self.prefs.get(&format!("ricrac-{server_id}"));
            if let Some(kept) = kept {
                self.ricrac.connections.insert(server_id.to_string(), kept);
            }
        }
        self.ricrac_ask(server_id, Request::RicracStatus);
    }

    /// Has the user pick a workspace and approve Motile at Ricrac in the browser, then hands
    /// what Ricrac sent back to the server, which alone can finish with it.
    pub fn ricrac_connect(&mut self, server_id: &str) {
        if self.ricrac.connecting.is_some() {
            return;
        }
        self.ricrac.connecting = Some(server_id.to_string());
        self.ricrac.error = None;
        let server = server_id.to_string();
        self.request_then(server_id, Request::RicracConnect, move |store, result, cx| {
            let url = match result {
                Ok(answer) => answer["url"].as_str().map(String::from),
                Err(error) => {
                    store.ricrac.connecting = None;
                    store.ricrac.error = Some(error.into());
                    return;
                }
            };
            let Some(url) = url else {
                store.ricrac.connecting = None;
                store.ricrac.error = Some("The connection couldn't be started.".into());
                return;
            };
            let Some((session, answer)) = crate::sign_in::start(&url, motile_protocol::RICRAC_REDIRECT, cx) else {
                // Without the system's sheet the browser can't hand the approval back.
                cx.open_url(&url);
                store.ricrac.connecting = None;
                return;
            };
            store.ricrac.session = Some(session);
            cx.spawn(async move |this, cx| {
                let callback = answer.await.ok().flatten();
                let _ = this.update(cx, |store, cx| {
                    store.ricrac.session = None;
                    store.ricrac_heard(&server, callback);
                    cx.notify();
                });
            })
            .detach();
        });
    }

    /// Gives up waiting for the browser, which can't say that the user left it.
    pub fn ricrac_cancel_connect(&mut self) {
        self.ricrac.session = None;
        self.ricrac.connecting = None;
    }

    /// The browser has come back from Ricrac, with a code or a refusal.
    fn ricrac_heard(&mut self, server_id: &str, callback: Option<String>) {
        let sent = callback.map(|url| query_items(&url)).unwrap_or_default();
        let value = |name: &str| sent.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone());
        let (Some(code), Some(state)) = (value("code"), value("state")) else {
            self.ricrac.connecting = None;
            if let Some(refusal) = value("error").filter(|refusal| refusal != "access_denied") {
                self.ricrac.error = Some(format!("Ricrac didn't connect: {refusal}"));
            }
            return;
        };
        self.ricrac_ask(server_id, Request::RicracFinish { code, state });
    }

    pub fn ricrac_disconnect(&mut self, workspace: &str, server_id: &str) {
        self.ricrac_ask(server_id, Request::RicracDisconnect { workspace: workspace.to_string() });
    }

    /// Sends a request that `Ricrac` answers, with the server's workspaces.
    fn ricrac_ask(&mut self, server_id: &str, request: Request) {
        let finishing = matches!(request, Request::RicracFinish { .. });
        let asking_status = matches!(request, Request::RicracStatus);
        let server = server_id.to_string();
        self.request_then(server_id, request, move |store, result, _| {
            if finishing {
                store.ricrac.connecting = None;
            }
            let answer = match result {
                Ok(answer) => answer,
                Err(error) => {
                    if !asking_status {
                        store.ricrac.error = Some(error.into());
                    }
                    return;
                }
            };
            let before: HashSet<String> =
                store.ricrac.connected(&server).iter().map(|connection| connection.id.clone()).collect();
            let heard: Vec<RicracConnection> =
                serde_json::from_value(answer["connections"].clone()).unwrap_or_default();
            store.prefs.set(&format!("ricrac-{server}"), heard.clone());
            store.ricrac.connections.insert(server.clone(), heard);
            if !finishing {
                return;
            }
            let Some(target) = store.panel_target() else { return };
            let added = store.ricrac.connected(&server).iter().find(|connection| !before.contains(&connection.id));
            let Some(added) = added.map(|connection| connection.id.clone()) else { return };
            store.ricrac_pick(&target, Some(added), None);
        });
    }

    /// Asks the server for the issues the project's tab lists, or for the ones that have the
    /// words of `search`, and for the workspace's teams and users when they aren't known yet. A
    /// list read before shows at once while it is read again.
    pub fn ricrac_load(&mut self, target: &PanelTarget, search: &str) {
        let choice = self.ricrac_choice(target);
        let Some(workspace) = choice.workspace else { return };
        let query = Query {
            server_id: target.server_id.clone(),
            workspace: workspace.clone(),
            team: choice.team,
            mine: choice.mine,
            states: choice.states,
            search: search.trim().to_string(),
        };
        if let Some(listed) = self.ricrac.listed.take().filter(|listed| *listed != query && !listed.search.is_empty()) {
            self.ricrac.lists.remove(&listed);
        }
        self.ricrac.listed = Some(query.clone());
        if !self.ricrac.teams.contains_key(&workspace) {
            self.ricrac_load_teams(&workspace, &target.server_id);
        }
        self.ricrac_fetch(query);
    }

    fn ricrac_fetch(&mut self, query: Query) {
        self.ricrac.lists.entry(query.clone()).or_insert(Loaded::Loading);
        self.ricrac.working.insert("issues".into());
        let states: Vec<RicracStateKind> = query.states.iter().filter_map(|name| kind_of(name)).collect();
        // A server that doesn't read `states` yet goes by `closed`.
        let closed = states.is_empty()
            || states.iter().any(|kind| matches!(kind, RicracStateKind::Completed | RicracStateKind::Canceled));
        let command = Command::RicracIssues {
            server_id: query.server_id.clone(),
            workspace: query.workspace.clone(),
            team: query.team.clone(),
            mine: query.mine,
            closed,
            states,
            search: (!query.search.is_empty()).then(|| query.search.clone()),
        };
        self.ask_read(command, read_groups, move |store, result, _| {
            let shown = store.ricrac.listed.as_ref() == Some(&query);
            if shown {
                store.ricrac.working.remove("issues");
            }
            match result.map_err(String::from).and_then(|read| read) {
                Ok(groups) => {
                    store.ricrac.lists.insert(query, Loaded::Ready(Rc::new(groups)));
                    if shown {
                        store.ricrac.error = None;
                    }
                }
                Err(error) => {
                    if !shown {
                        return;
                    }
                    if store.ricrac.lists.get(&query).and_then(Loaded::value).is_none() {
                        store.ricrac.lists.insert(query.clone(), Loaded::Failed(error));
                    } else {
                        store.ricrac.error = Some(error);
                    }
                    // Ricrac may have ended the connection.
                    store.ricrac_ask(&query.server_id, Request::RicracStatus);
                }
            }
        });
    }

    /// Reads the issue for its tab.
    pub fn ricrac_load_issue(&mut self, id: &str, workspace: &str, server_id: &str) {
        self.ricrac_read_issue(server_id, workspace, id, None, None);
    }

    /// Says `body` on the issue, then reads it again; `done` runs once it has been said.
    pub fn ricrac_comment(
        &mut self,
        id: &str,
        workspace: &str,
        server_id: &str,
        body: String,
        done: impl FnOnce(&mut Store, &mut Context<Store>) + 'static,
    ) {
        self.ricrac_read_issue(server_id, workspace, id, Some(body), Some(Box::new(done)));
    }

    /// Reads the issue, and with `comment` says that on it first.
    fn ricrac_read_issue(
        &mut self,
        server_id: &str,
        workspace: &str,
        id: &str,
        comment: Option<String>,
        done: Option<Done>,
    ) {
        let commenting = comment.is_some();
        if commenting {
            self.ricrac.working.insert(format!("comment:{id}"));
        }
        if self.ricrac.pages.get(id).and_then(Loaded::value).is_none() {
            self.ricrac.pages.insert(id.to_string(), Loaded::Loading);
        }
        self.ricrac.working.insert(format!("read:{id}"));
        let id = id.to_string();
        let command = Command::RicracIssue {
            server_id: server_id.to_string(),
            workspace: workspace.to_string(),
            issue: id.clone(),
            comment,
        };
        self.ask_read(command, read_page, move |store, result, cx| {
            store.ricrac.working.remove(&format!("read:{id}"));
            if commenting {
                store.ricrac.working.remove(&format!("comment:{id}"));
            }
            match result.map_err(String::from).and_then(|read| read) {
                Ok(page) => {
                    store.ricrac.pages.insert(id, Loaded::Ready(page));
                    store.ricrac.error = None;
                    if let Some(done) = done {
                        done(store, cx);
                    }
                }
                Err(error) => {
                    if store.ricrac.pages.get(&id).and_then(Loaded::value).is_none() {
                        store.ricrac.pages.insert(id, Loaded::Failed(error));
                    } else {
                        store.ricrac.error = Some(error);
                    }
                }
            }
        });
    }

    /// Changes the issue's status, assignee or priority at Ricrac, then reads what shows it again.
    pub fn ricrac_change(&mut self, id: &str, change: RicracChange, workspace: &str, server_id: &str) {
        if self.ricrac.working.contains(id) {
            return;
        }
        self.ricrac.working.insert(id.to_string());
        let (id, workspace, server) = (id.to_string(), workspace.to_string(), server_id.to_string());
        let request = Request::RicracUpdate { workspace: workspace.clone(), issue: id.clone(), change };
        self.request_then(server_id, request, move |store, result, _| {
            store.ricrac.working.remove(&id);
            if let Err(error) = result {
                store.ricrac.error = Some(error.into());
            }
            store.ricrac_refresh(Some(&id), &workspace, &server);
        });
    }

    /// Files an issue; `done` gets its id and identifier.
    pub fn ricrac_create(
        &mut self,
        issue: NewRicracIssue,
        workspace: &str,
        server_id: &str,
        done: impl FnOnce(&mut Store, String, String, &mut Context<Store>) + 'static,
    ) {
        if self.ricrac.working.contains("create") {
            return;
        }
        self.ricrac.working.insert("create".into());
        let (workspace, server) = (workspace.to_string(), server_id.to_string());
        let request = Request::RicracCreate { workspace: workspace.clone(), issue };
        self.request_then(server_id, request, move |store, result, cx| {
            store.ricrac.working.remove("create");
            match result {
                Ok(answer) => {
                    let filed = &answer["issue"];
                    let id = filed["id"].as_str().unwrap_or_default().to_string();
                    let identifier = filed["identifier"].as_str().unwrap_or_default().to_string();
                    store.ricrac.error = None;
                    store.ricrac_refresh(None, &workspace, &server);
                    done(store, id, identifier, cx);
                }
                Err(error) => store.ricrac.error = Some(error.into()),
            }
        });
    }

    /// Puts what the issue asks for in the open draft's composer, or from a thread in that of a
    /// new draft. An issue nobody works on yet is taken: assigned to the user and moved to the
    /// first status of work.
    pub fn ricrac_work(&mut self, page: &Page, workspace: &str, target: &PanelTarget, cx: &mut Context<Self>) {
        let mut taking = RicracChange::default();
        let states = self.ricrac.states_of(workspace, &page.row.team);
        let waiting =
            matches!(page.state.kind, RicracStateKind::Triage | RicracStateKind::Backlog | RicracStateKind::Unstarted);
        if waiting && let Some(started) = states.iter().find(|state| state.kind == RicracStateKind::Started) {
            taking.state = Some(started.id.clone());
        }
        let me = self.ricrac.users.get(workspace).and_then(|users| users.iter().find(|user| user.me));
        if page.row.assignee_id.is_none()
            && let Some(me) = me
        {
            taking.assignee = Some(me.id.clone());
        }
        if taking != RicracChange::default() {
            self.ricrac_change(&page.row.id, taking, workspace, &target.server_id);
        }
        self.ricrac_start_work(page.prompt.clone(), &target.project_id, cx);
    }

    /// Puts the prompt in the composer of the open draft. From a thread it opens a draft in the
    /// project first, which takes the panel's open tab along.
    fn ricrac_start_work(&mut self, prompt: String, project_id: &str, cx: &mut Context<Self>) {
        if self.selected_draft().is_none() {
            if self.project(Some(project_id)).is_none() {
                return;
            }
            let tab = self.panel_tabs().active.filter(|tab| tab.blank_number().is_none());
            self.start_new_thread(Some(project_id.to_string()), cx);
            if let Some(tab) = tab {
                self.open_tab(tab);
            }
        }
        self.hand_off(prompt);
    }

    fn ricrac_refresh(&mut self, id: Option<&str>, workspace: &str, server_id: &str) {
        if let Some(id) = id.filter(|id| self.ricrac.pages.contains_key(*id)) {
            self.ricrac_load_issue(id, workspace, server_id);
        }
        if let Some(listed) = self.ricrac.listed.clone().filter(|listed| listed.server_id == server_id) {
            self.ricrac_fetch(listed);
        }
    }

    fn ricrac_load_teams(&mut self, workspace: &str, server_id: &str) {
        let workspace = workspace.to_string();
        self.request_then(server_id, Request::RicracTeams { workspace: workspace.clone() }, move |store, result, _| {
            let Ok(answer) = result else { return };
            let teams = serde_json::from_value(answer["teams"].clone()).unwrap_or_default();
            let users = serde_json::from_value(answer["users"].clone()).unwrap_or_default();
            store.ricrac.teams.insert(workspace.clone(), teams);
            store.ricrac.users.insert(workspace, users);
        });
    }

    /// The teams and users of the workspace, asked for when they aren't known yet.
    pub fn ricrac_need_teams(&mut self, workspace: &str, server_id: &str) {
        if self.ricrac.teams.contains_key(workspace) {
            return;
        }
        self.ricrac_load_teams(workspace, server_id);
    }

    pub fn open_ricrac_issue(&mut self, workspace: String, id: String, identifier: String) {
        self.open_tab(PanelTab::RicracIssue { workspace, id, identifier });
    }
}

fn read_groups(answer: Value) -> Result<Vec<Group>, String> {
    serde_json::from_value(answer["groups"].clone()).map_err(|error| format!("The issues couldn't be read: {error}"))
}

fn read_page(answer: Value) -> Result<Page, String> {
    serde_json::from_value(answer["page"].clone()).map_err(|error| format!("The issue couldn't be read: {error}"))
}
