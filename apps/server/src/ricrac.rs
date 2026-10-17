//! The server's connections to Ricrac workspaces: the user approves Motile's application in a
//! browser, and the token Ricrac grants for it is kept here, never anywhere else. With it the
//! server reads a workspace's teams and issues through Ricrac's operations and changes them.

use std::time::Duration;

use anyhow::{Context, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use motile_protocol::wire::{
    Label, NewRicracIssue, RicracChange, RicracComment, RicracConnection, RicracIssue, RicracIssueDetail, RicracState,
    RicracStateKind, RicracTeam, RicracUser,
};
use motile_protocol::{RICRAC_REDIRECT, now};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::pull_requests::time;
use crate::store::Store;

/// Motile's application at Ricrac. It has no secret: a code is exchanged with the verifier that
/// its sign-in page was opened with.
const CLIENT_ID: &str = "gvawqgi0hgexfxmbjvxc6xuidxsnthgn";
const URL: &str = "https://ricrac.app";
const SCOPE: &str = "read write";
const GRANTS: &str = "ricrac";
const TIMEOUT: Duration = Duration::from_secs(20);
/// A token this close to its end is renewed before it is used.
const RENEW_AHEAD: f64 = 300.0;
const LABELS: usize = 5;

/// Which issues a list is of.
pub struct Wanted {
    pub team: Option<String>,
    pub mine: bool,
    pub closed: bool,
    /// The kinds of status to list, or none for what `closed` says.
    pub states: Vec<RicracStateKind>,
    pub search: Option<String>,
}

pub struct Ricrac {
    client_id: String,
    url: String,
    /// The connection that waits for the user to approve it.
    pending: std::sync::Mutex<Option<Pending>>,
    /// Held while the grants are changed, so that a token is renewed once.
    changing: tokio::sync::Mutex<()>,
}

struct Pending {
    state: String,
    verifier: String,
}

/// What Ricrac granted for a workspace, as it is kept in the settings.
#[derive(Serialize, Deserialize)]
struct Grant {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: f64,
    connection: RicracConnection,
}

#[derive(Deserialize)]
struct Granted {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: f64,
}

/// Ricrac no longer takes the token: the user took the grant back, or it ran out.
#[derive(Debug)]
struct Ended;

impl std::fmt::Display for Ended {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Ricrac ended this connection. Connect the workspace again.")
    }
}

impl std::error::Error for Ended {}

impl Ricrac {
    /// `MOTILE_RICRAC_URL` is a stand-in for Ricrac, and `MOTILE_RICRAC_CLIENT_ID` another
    /// application than Motile's.
    pub fn from_environment() -> Self {
        let url = std::env::var("MOTILE_RICRAC_URL").ok().filter(|url| !url.is_empty());
        let client_id = std::env::var("MOTILE_RICRAC_CLIENT_ID").ok().filter(|id| !id.is_empty());
        Self::new(client_id.unwrap_or_else(|| CLIENT_ID.to_string()), url.unwrap_or_else(|| URL.to_string()))
    }

    fn new(client_id: String, url: String) -> Self {
        Self { client_id, url, pending: std::sync::Mutex::default(), changing: Default::default() }
    }

    pub fn connections(&self, store: &Store) -> Vec<RicracConnection> {
        grants(store).into_iter().map(|grant| grant.connection).collect()
    }

    /// The page where the user picks a workspace and approves the connection. Only the code it
    /// ends with can finish it.
    pub fn connect(&self) -> anyhow::Result<String> {
        let state = uuid::Uuid::new_v4().simple().to_string();
        let verifier = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(&verifier));
        let parameters = [
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", RICRAC_REDIRECT),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("state", &state),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ];
        let url = reqwest::Url::parse_with_params(&format!("{}/oauth/authorize", self.url), parameters)?;
        *self.pending.lock().unwrap() = Some(Pending { state, verifier });
        Ok(url.into())
    }

    /// Keeps what Ricrac grants for the code beside the other workspaces', in place of what was
    /// kept for the same workspace.
    pub async fn finish(&self, store: &Store, code: &str, state: &str) -> anyhow::Result<Vec<RicracConnection>> {
        let pending = self.pending.lock().unwrap().take_if(|pending| pending.state == state);
        let Some(pending) = pending else {
            bail!("This isn't the connection that was started here. Connect Ricrac again.");
        };
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", RICRAC_REDIRECT),
            ("client_id", &self.client_id),
            ("code_verifier", &pending.verifier),
        ];
        let answer = http()?.post(format!("{}/oauth/token", self.url)).form(&form).send().await;
        let answer = answer.context("Ricrac couldn't be reached.")?;
        if !answer.status().is_success() {
            bail!("Ricrac refused the connection: {}", refusal(answer).await);
        }
        let granted: Granted = answer.json().await.context("Ricrac's answer couldn't be read.")?;
        let connection = self.who(&granted.access_token).await?;

        let _changing = self.changing.lock().await;
        let mut grants = grants(store);
        grants.retain(|grant| grant.connection.id != connection.id);
        grants.push(Grant {
            access_token: granted.access_token,
            refresh_token: granted.refresh_token,
            expires_at: now() + granted.expires_in,
            connection,
        });
        keep(store, &grants)?;
        Ok(grants.into_iter().map(|grant| grant.connection).collect())
    }

    /// Forgets the grant even when Ricrac can't be told, so that a workspace can always be
    /// disconnected. Answers with the workspaces that stay.
    pub async fn disconnect(&self, store: &Store, workspace: &str) -> anyhow::Result<Vec<RicracConnection>> {
        let _changing = self.changing.lock().await;
        let mut grants = grants(store);
        let Some(at) = grants.iter().position(|grant| grant.connection.id == workspace) else {
            return Ok(grants.into_iter().map(|grant| grant.connection).collect());
        };
        let grant = grants.remove(at);
        let (token, hint) = match &grant.refresh_token {
            Some(token) => (token, "refresh_token"),
            None => (&grant.access_token, "access_token"),
        };
        let form = [("token", token.as_str()), ("token_type_hint", hint), ("client_id", &self.client_id)];
        let revoked = http()?.post(format!("{}/oauth/revoke", self.url)).form(&form).send().await;
        if let Err(error) = revoked.and_then(|answer| answer.error_for_status()) {
            tracing::warn!("Ricrac didn't take the grant back: {error}");
        }
        keep(store, &grants)?;
        Ok(grants.into_iter().map(|grant| grant.connection).collect())
    }

    /// The teams by name, each with its statuses in the order an issue goes through them, and
    /// the users who can be assigned, by name.
    pub async fn teams(&self, store: &Store, workspace: &str) -> anyhow::Result<(Vec<RicracTeam>, Vec<RicracUser>)> {
        let found = self.ask(store, workspace, "get_workspace", json!({})).await?;
        let mut teams = Vec::new();
        for team in found["teams"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let Some(id) = text(&team["id"]) else { continue };
            let states = self.ask(store, workspace, "list_issue_statuses", json!({ "team": id, "limit": 100 })).await?;
            teams.extend(read_team(team, nodes(&states)));
        }
        teams.sort_by_key(|team| team.name.to_lowercase());
        let found = self.ask(store, workspace, "list_users", json!({ "limit": 250 })).await?;
        let active = nodes(&found).iter().filter(|user| user["active"] != false);
        let mut users: Vec<RicracUser> = active.filter_map(read_user).collect();
        users.sort_by_key(|user| user.name.to_lowercase());
        Ok((teams, users))
    }

    pub async fn issues(&self, store: &Store, workspace: &str, wanted: &Wanted) -> anyhow::Result<Vec<RicracIssue>> {
        let mut asked = json!({ "limit": 200, "orderBy": "updatedAt" });
        if let Some(team) = &wanted.team {
            asked["team"] = json!(team);
        }
        match wanted.search.as_deref().map(str::trim).filter(|words| !words.is_empty()) {
            Some(words) => asked["query"] = json!(words),
            None => {
                if wanted.mine {
                    asked["assignee"] = json!("me");
                }
                if !wanted.states.is_empty() {
                    asked["filter"] = json!({ "state": { "type": { "in": wanted.states } } });
                } else if !wanted.closed {
                    asked["filter"] = json!({ "state": { "type": { "nin": ["completed", "canceled"] } } });
                }
            }
        }
        let mut issues = Vec::new();
        loop {
            let found = self.ask(store, workspace, "list_issues", asked.clone()).await?;
            issues.extend(nodes(&found).iter().filter_map(read_issue));
            let more = found["pageInfo"]["hasNextPage"] == true;
            let Some(cursor) = found["pageInfo"]["endCursor"].as_str().filter(|_| more) else {
                return Ok(issues);
            };
            asked["cursor"] = json!(cursor);
        }
    }

    pub async fn issue(&self, store: &Store, workspace: &str, issue: &str) -> anyhow::Result<RicracIssueDetail> {
        let found = self.ask(store, workspace, "get_issue", json!({ "id": issue, "includeComments": true })).await?;
        read_detail(&found).context("Ricrac doesn't have this issue.")
    }

    pub async fn update(
        &self,
        store: &Store,
        workspace: &str,
        issue: &str,
        change: &RicracChange,
    ) -> anyhow::Result<RicracIssue> {
        let mut asked = json!({ "id": issue });
        if let Some(state) = &change.state {
            asked["state"] = json!(state);
        }
        if let Some(assignee) = &change.assignee {
            asked["assignee"] = if assignee.is_empty() { Value::Null } else { json!(assignee) };
        }
        if let Some(priority) = change.priority {
            asked["priority"] = json!(priority);
        }
        let changed = self.ask(store, workspace, "save_issue", asked).await?;
        read_issue(&changed).context("Ricrac didn't say what became of the issue.")
    }

    /// Answers with the issue as it is with the comment.
    pub async fn comment(
        &self,
        store: &Store,
        workspace: &str,
        issue: &str,
        body: &str,
    ) -> anyhow::Result<RicracIssueDetail> {
        self.ask(store, workspace, "save_comment", json!({ "issueId": issue, "body": body })).await?;
        self.issue(store, workspace, issue).await
    }

    pub async fn create(&self, store: &Store, workspace: &str, new: &NewRicracIssue) -> anyhow::Result<RicracIssue> {
        if new.title.trim().is_empty() {
            bail!("An issue needs a title.");
        }
        let mut asked = json!({ "team": new.team, "title": new.title.trim(), "priority": new.priority });
        if !new.description.trim().is_empty() {
            asked["description"] = json!(new.description);
        }
        if let Some(state) = &new.state {
            asked["state"] = json!(state);
        }
        if let Some(assignee) = &new.assignee {
            asked["assignee"] = json!(assignee);
        }
        let filed = self.ask(store, workspace, "save_issue", asked).await?;
        read_issue(&filed).context("Ricrac didn't say what became of the issue.")
    }

    /// What the operation answers the workspace's user. A connection Ricrac has ended is
    /// forgotten.
    async fn ask(&self, store: &Store, workspace: &str, operation: &str, arguments: Value) -> anyhow::Result<Value> {
        let token = self.token(store, workspace).await?;
        let answer = self.call(&token, operation, arguments).await;
        if answer.as_ref().is_err_and(|error| error.is::<Ended>()) {
            self.forget(store, workspace).await?;
        }
        answer
    }

    /// The workspace's token, renewed first when it is about to run out.
    async fn token(&self, store: &Store, workspace: &str) -> anyhow::Result<String> {
        let _changing = self.changing.lock().await;
        let mut grants = grants(store);
        let Some(grant) = grants.iter_mut().find(|grant| grant.connection.id == workspace) else {
            bail!("Your server isn't connected to this Ricrac workspace.");
        };
        let fresh = grant.expires_at - now() > RENEW_AHEAD;
        let Some(refresh_token) = grant.refresh_token.clone().filter(|_| !fresh) else {
            return Ok(grant.access_token.clone());
        };
        let form = [("grant_type", "refresh_token"), ("refresh_token", &refresh_token), ("client_id", &self.client_id)];
        let answer = http()?.post(format!("{}/oauth/token", self.url)).form(&form).send().await;
        let answer = answer.context("Ricrac couldn't be reached.")?;
        if answer.status().is_client_error() {
            grants.retain(|grant| grant.connection.id != workspace);
            keep(store, &grants)?;
            return Err(Ended.into());
        }
        if !answer.status().is_success() {
            bail!("Ricrac didn't renew the connection: {}", refusal(answer).await);
        }
        let granted: Granted = answer.json().await.context("Ricrac's answer couldn't be read.")?;
        grant.access_token = granted.access_token.clone();
        grant.refresh_token = granted.refresh_token.or(Some(refresh_token));
        grant.expires_at = now() + granted.expires_in;
        keep(store, &grants)?;
        Ok(granted.access_token)
    }

    async fn forget(&self, store: &Store, workspace: &str) -> anyhow::Result<()> {
        let _changing = self.changing.lock().await;
        let mut grants = grants(store);
        grants.retain(|grant| grant.connection.id != workspace);
        keep(store, &grants)
    }

    async fn who(&self, access_token: &str) -> anyhow::Result<RicracConnection> {
        let workspace = self.call(access_token, "get_workspace", json!({})).await?;
        let user = self.call(access_token, "get_user", json!({ "id": "me" })).await?;
        let (Some(id), Some(name), Some(user)) =
            (text(&workspace["id"]), text(&workspace["name"]), text(&user["name"]))
        else {
            bail!("Ricrac didn't say whose workspace this is.");
        };
        Ok(RicracConnection { id, workspace: name, user })
    }

    /// What the operation answers, or what Ricrac said went wrong.
    async fn call(&self, access_token: &str, operation: &str, arguments: Value) -> anyhow::Result<Value> {
        let request = http()?.post(format!("{}/v1/ops/{operation}", self.url)).bearer_auth(access_token);
        let answer = request.json(&arguments).send().await.context("Ricrac couldn't be reached.")?;
        let status = answer.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Ended.into());
        }
        let said: Value = answer.json().await.unwrap_or_default();
        if let Some(message) = said["error"].as_str().filter(|_| !status.is_success()) {
            bail!("Ricrac said: {message}");
        }
        if !status.is_success() {
            bail!("Ricrac answered {status}.");
        }
        Ok(said)
    }
}

fn grants(store: &Store) -> Vec<Grant> {
    store.setting(GRANTS).and_then(|kept| serde_json::from_str(&kept).ok()).unwrap_or_default()
}

fn keep(store: &Store, grants: &[Grant]) -> anyhow::Result<()> {
    Ok(store.set_setting(GRANTS, Some(&serde_json::to_string(grants)?))?)
}

fn http() -> anyhow::Result<reqwest::Client> {
    motile_protocol::tls::install();
    Ok(reqwest::Client::builder().timeout(TIMEOUT).build()?)
}

/// Why Ricrac refused, in its words where it gave any.
async fn refusal(answer: reqwest::Response) -> String {
    let status = answer.status();
    let said: Value = answer.json().await.unwrap_or_default();
    let words = said["error_description"].as_str().or(said["error"].as_str());
    words.map(str::to_string).unwrap_or_else(|| status.to_string())
}

fn nodes(list: &Value) -> &[Value] {
    list["nodes"].as_array().map(Vec::as_slice).unwrap_or_default()
}

fn text(value: &Value) -> Option<String> {
    value.as_str().map(str::to_string)
}

fn color(value: &Value) -> String {
    value.as_str().unwrap_or_default().trim_start_matches('#').to_string()
}

fn read_team(team: &Value, states: &[Value]) -> Option<RicracTeam> {
    let mut states: Vec<RicracState> =
        states.iter().filter(|state| state["archivedAt"].is_null()).filter_map(read_state).collect();
    states.sort_by(|a, b| (workflow(a.kind), a.position).partial_cmp(&(workflow(b.kind), b.position)).unwrap());
    Some(RicracTeam { id: text(&team["id"])?, key: text(&team["key"])?, name: text(&team["name"])?, states })
}

/// Where a kind of status comes as an issue goes from reported to closed.
fn workflow(kind: RicracStateKind) -> u8 {
    match kind {
        RicracStateKind::Triage => 0,
        RicracStateKind::Backlog => 1,
        RicracStateKind::Unstarted => 2,
        RicracStateKind::Started => 3,
        RicracStateKind::Completed => 4,
        RicracStateKind::Canceled => 5,
    }
}

fn read_state(state: &Value) -> Option<RicracState> {
    let kind = match state["type"].as_str()? {
        "triage" => RicracStateKind::Triage,
        "unstarted" => RicracStateKind::Unstarted,
        "started" => RicracStateKind::Started,
        "completed" => RicracStateKind::Completed,
        "canceled" => RicracStateKind::Canceled,
        _ => RicracStateKind::Backlog,
    };
    Some(RicracState {
        id: text(&state["id"])?,
        name: text(&state["name"])?,
        kind,
        color: color(&state["color"]),
        position: state["position"].as_f64().unwrap_or(0.0),
    })
}

fn read_user(user: &Value) -> Option<RicracUser> {
    Some(RicracUser { id: text(&user["id"])?, name: text(&user["name"])?, me: user["isMe"] == true })
}

/// The comments of the issue's threads, each thread's replies with it, the oldest first.
fn read_detail(issue: &Value) -> Option<RicracIssueDetail> {
    let comment = |comment: &Value| {
        Some(RicracComment {
            id: text(&comment["id"])?,
            author: text(&comment["user"]["name"]).unwrap_or_else(|| "Ricrac".to_string()),
            body: text(&comment["body"])?,
            created_at: time(&comment["createdAt"]).unwrap_or(0.0),
        })
    };
    let threads = issue["comments"].as_array().map(Vec::as_slice).unwrap_or_default();
    let replies = threads.iter().flat_map(|thread| thread["replies"].as_array().map(Vec::as_slice).unwrap_or_default());
    let mut comments: Vec<RicracComment> = threads.iter().chain(replies).filter_map(comment).collect();
    comments.sort_by(|a, b| a.created_at.total_cmp(&b.created_at));
    Some(RicracIssueDetail {
        issue: read_issue(issue)?,
        description: text(&issue["description"]).unwrap_or_default(),
        comments,
    })
}

fn read_issue(issue: &Value) -> Option<RicracIssue> {
    let label = |label: &Value| Some(Label { name: text(&label["name"])?, color: color(&label["color"]) });
    let labels = issue["labels"].as_array().map(Vec::as_slice).unwrap_or_default();
    Some(RicracIssue {
        id: text(&issue["id"])?,
        identifier: text(&issue["identifier"])?,
        title: text(&issue["title"])?,
        url: text(&issue["url"])?,
        priority: issue["priority"].as_f64().unwrap_or(0.0) as u8,
        state: read_state(&issue["state"])?,
        team: text(&issue["team"]["id"])?,
        assignee: read_user(&issue["assignee"]),
        labels: labels.iter().filter_map(label).take(LABELS).collect(),
        branch_name: text(&issue["branchName"]).unwrap_or_default(),
        updated_at: time(&issue["updatedAt"]).unwrap_or(0.0),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    /// What the stand-in was asked: the path, the body and the token it came with.
    type Asked = Arc<Mutex<Vec<(String, String, String)>>>;

    fn issue(state: &str) -> Value {
        json!({
            "id": "issue", "identifier": "ENG-7", "title": "Greet by name", "url": "https://ricrac.app/engines/issue/ENG-7",
            "priority": 2, "updatedAt": "2026-10-05T10:00:00.000Z", "team": { "id": "team", "key": "ENG", "name": "Engines" },
            "branchName": "eng-7-greet-by-name",
            "assignee": { "id": "ada", "name": "Ada", "displayName": "ada", "email": "ada@engines.dev" },
            "description": "Say **hello** to whoever runs it.",
            "comments": [
                { "id": "first", "body": "Which name?", "createdAt": "2026-10-05T11:00:00.000Z", "user": null, "replies": [
                    { "id": "third", "body": "The one it's given.", "createdAt": "2026-10-05T13:00:00.000Z", "user": { "name": "Grace" } },
                ] },
                { "id": "second", "body": "Done in a branch.", "createdAt": "2026-10-05T12:00:00.000Z", "user": { "name": "Ada" }, "replies": [] },
            ],
            "state": { "id": state, "name": "Todo", "type": "unstarted", "color": "#e2e2e2" },
            "labels": [{ "id": "bug", "name": "Bug", "color": "#eb5757", "parent": null }],
        })
    }

    fn page(nodes: Value) -> Value {
        json!({ "nodes": nodes, "pageInfo": { "hasNextPage": false, "endCursor": null } })
    }

    /// Stands in for Ricrac: grants a token for the code `good`, renews one for a refresh token
    /// it gave, and answers the operations with one team and one issue.
    async fn fake_ricrac() -> (Ricrac, Asked) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let asked = Asked::default();
        let heard = asked.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let (path, body, token) = read_request(&mut stream).await;
                let refused = (
                    "400 Bad Request",
                    json!({ "error": "invalid_grant", "error_description": "Unknown or used code" }),
                );
                let (status, answer) = match path.as_str() {
                    "/oauth/token" if body.contains("code=good&") => {
                        ("200 OK", json!({ "access_token": "access", "refresh_token": "refresh", "expires_in": 86400 }))
                    }
                    "/oauth/token" if body.contains("refresh_token=refresh&") => (
                        "200 OK",
                        json!({ "access_token": "renewed", "refresh_token": "refresh2", "expires_in": 86400 }),
                    ),
                    "/oauth/token" => refused,
                    _ if path.starts_with("/v1/") && token == "revoked" => {
                        ("401 Unauthorized", json!({ "error": "Not signed in" }))
                    }
                    "/v1/ops/get_workspace" => (
                        "200 OK",
                        json!({ "id": "acme", "name": "Engines", "teams": [{ "id": "team", "key": "ENG", "name": "Engines", "private": false }] }),
                    ),
                    "/v1/ops/get_user" => ("200 OK", json!({ "id": "ada", "name": "Ada", "isMe": true })),
                    "/v1/ops/list_issue_statuses" => (
                        "200 OK",
                        page(json!([
                            { "id": "done", "name": "Done", "type": "completed", "color": "#5e6ad2", "position": 3, "archivedAt": null },
                            { "id": "review", "name": "In Review", "type": "started", "color": "#0f783c", "position": 3, "archivedAt": null },
                            { "id": "old", "name": "Old", "type": "started", "color": "#0f783c", "position": 9, "archivedAt": "2026-10-01T00:00:00Z" },
                            { "id": "progress", "name": "In Progress", "type": "started", "color": "#f2c94c", "position": 2, "archivedAt": null },
                            { "id": "todo", "name": "Todo", "type": "unstarted", "color": "#e2e2e2", "position": 1, "archivedAt": null },
                        ])),
                    ),
                    "/v1/ops/list_users" => (
                        "200 OK",
                        page(json!([
                            { "id": "grace", "name": "Grace", "isMe": false, "active": true },
                            { "id": "gone", "name": "Charles", "isMe": false, "active": false },
                            { "id": "ada", "name": "Ada", "isMe": true, "active": true },
                        ])),
                    ),
                    "/v1/ops/list_issues" if body.contains("\"team\":\"paged\"") && !body.contains("cursor") => (
                        "200 OK",
                        json!({ "nodes": [issue("done")], "pageInfo": { "hasNextPage": true, "endCursor": "next" } }),
                    ),
                    "/v1/ops/list_issues" => ("200 OK", page(json!([issue("todo")]))),
                    "/v1/ops/get_issue" => ("200 OK", issue("todo")),
                    "/v1/ops/save_issue" if body.contains("\"id\"") => ("200 OK", issue("done")),
                    "/v1/ops/save_issue" if body.contains("\"title\":\"Fail\"") => {
                        ("400 Bad Request", json!({ "error": "No team \"NOPE\"; teams are ENG" }))
                    }
                    "/v1/ops/save_issue" => ("200 OK", issue("todo")),
                    "/v1/ops/save_comment" => ("200 OK", json!({ "id": "fourth" })),
                    _ => ("200 OK", json!({})),
                };
                heard.lock().unwrap().push((path, body, token));
                let answer = answer.to_string();
                let reply = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
                    answer.len()
                );
                let _ = stream.write_all(reply.as_bytes()).await;
            }
        });
        (Ricrac::new("motile".to_string(), url), asked)
    }

    async fn read_request(stream: &mut tokio::net::TcpStream) -> (String, String, String) {
        let mut read = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let count = stream.read(&mut buffer).await.unwrap();
            read.extend_from_slice(&buffer[..count]);
            let text = String::from_utf8_lossy(&read).to_string();
            let Some((head, body)) = text.split_once("\r\n\r\n") else { continue };
            let header = |name: &str| {
                let found = head.lines().find(|line| line.to_lowercase().starts_with(name))?;
                Some(found[name.len()..].trim().to_string())
            };
            let length = header("content-length:").and_then(|length| length.parse().ok()).unwrap_or(0);
            if count == 0 || body.len() >= length {
                let path = head.split(' ').nth(1).unwrap_or_default().to_string();
                let token = header("authorization: bearer").unwrap_or_default();
                return (path, body.to_string(), token);
            }
        }
    }

    fn store(dir: &tempfile::TempDir) -> Store {
        Store::open(&dir.path().join("motile.sqlite")).unwrap()
    }

    fn parameters(url: &str) -> HashMap<String, String> {
        reqwest::Url::parse(url).unwrap().query_pairs().into_owned().collect()
    }

    fn ada() -> RicracConnection {
        RicracConnection { id: "acme".to_string(), workspace: "Engines".to_string(), user: "Ada".to_string() }
    }

    async fn connected(ricrac: &Ricrac, store: &Store) {
        let page = parameters(&ricrac.connect().unwrap());
        ricrac.finish(store, "good", &page["state"]).await.unwrap();
    }

    /// Changes what is kept of the one grant, as time or Ricrac would.
    fn change_grant(store: &Store, name: &str, value: Value) {
        let mut kept: Value = serde_json::from_str(&store.setting(GRANTS).unwrap()).unwrap();
        kept[0][name] = value;
        store.set_setting(GRANTS, Some(&kept.to_string())).unwrap();
    }

    /// The arguments of the operation the stand-in was last asked.
    fn last_arguments(asked: &Asked) -> (String, Value) {
        let (path, body, _) = asked.lock().unwrap().last().unwrap().clone();
        (path, serde_json::from_str(&body).unwrap())
    }

    #[tokio::test]
    async fn an_approved_connection_is_exchanged_with_its_verifier_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let (ricrac, asked) = fake_ricrac().await;
        let page = parameters(&ricrac.connect().unwrap());
        assert_eq!(page["client_id"], "motile");
        assert_eq!(page["redirect_uri"], "motile://ricrac");
        assert_eq!(page["scope"], "read write");

        let connections = ricrac.finish(&store(&dir), "good", &page["state"]).await.unwrap();
        assert_eq!(connections, [ada()]);

        let (_, exchange, _) = asked.lock().unwrap()[0].clone();
        let sent = parameters(&format!("http://ricrac/?{exchange}"));
        assert_eq!(URL_SAFE_NO_PAD.encode(Sha256::digest(&sent["code_verifier"])), page["code_challenge"]);
        assert!(!sent.contains_key("client_secret"));
        assert_eq!(ricrac.connections(&store(&dir)), [ada()]);
    }

    #[tokio::test]
    async fn a_code_is_refused_without_the_state_it_was_asked_with_or_when_ricrac_refuses_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (ricrac, asked) = fake_ricrac().await;
        let page = parameters(&ricrac.connect().unwrap());

        assert!(ricrac.finish(&store, "good", "another").await.is_err());
        assert!(asked.lock().unwrap().is_empty());

        let refused = ricrac.finish(&store, "bad", &page["state"]).await.unwrap_err();
        assert_eq!(refused.to_string(), "Ricrac refused the connection: Unknown or used code");
        assert!(ricrac.finish(&store, "good", &page["state"]).await.is_err(), "a state works once");
        assert!(ricrac.connections(&store).is_empty());
    }

    #[tokio::test]
    async fn a_workspace_connected_again_is_kept_once_and_disconnecting_takes_the_grant_back() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (ricrac, asked) = fake_ricrac().await;
        connected(&ricrac, &store).await;
        connected(&ricrac, &store).await;
        assert_eq!(ricrac.connections(&store), [ada()]);

        assert!(ricrac.disconnect(&store, "acme").await.unwrap().is_empty());
        let (path, body, _) = asked.lock().unwrap().last().unwrap().clone();
        let revoked = ("/oauth/revoke", "token=refresh&token_type_hint=refresh_token&client_id=motile");
        assert_eq!((path.as_str(), body.as_str()), revoked);
        assert!(ricrac.connections(&store).is_empty());
    }

    fn todo() -> RicracState {
        RicracState {
            id: "todo".to_string(),
            name: "Todo".to_string(),
            kind: RicracStateKind::Unstarted,
            color: "e2e2e2".to_string(),
            position: 0.0,
        }
    }

    fn greet() -> RicracIssue {
        RicracIssue {
            id: "issue".to_string(),
            identifier: "ENG-7".to_string(),
            title: "Greet by name".to_string(),
            url: "https://ricrac.app/engines/issue/ENG-7".to_string(),
            priority: 2,
            state: todo(),
            team: "team".to_string(),
            assignee: Some(RicracUser { id: "ada".to_string(), name: "Ada".to_string(), me: false }),
            labels: vec![Label { name: "Bug".to_string(), color: "eb5757".to_string() }],
            branch_name: "eng-7-greet-by-name".to_string(),
            updated_at: 1_791_194_400.0,
        }
    }

    fn wanted(team: Option<&str>, mine: bool, closed: bool, search: Option<&str>) -> Wanted {
        Wanted { team: team.map(str::to_string), mine, closed, states: Vec::new(), search: search.map(str::to_string) }
    }

    #[tokio::test]
    async fn the_issues_asked_for_are_listed_with_the_arguments_that_choose_them() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (ricrac, asked) = fake_ricrac().await;
        connected(&ricrac, &store).await;

        let issues = ricrac.issues(&store, "acme", &wanted(Some("team"), true, false, None)).await.unwrap();
        assert_eq!(issues, std::slice::from_ref(&greet()));
        let arguments = json!({
            "limit": 200, "orderBy": "updatedAt", "team": "team", "assignee": "me",
            "filter": { "state": { "type": { "nin": ["completed", "canceled"] } } },
        });
        assert_eq!(last_arguments(&asked), ("/v1/ops/list_issues".to_string(), arguments));
        assert_eq!(asked.lock().unwrap().last().unwrap().2, "access");

        ricrac.issues(&store, "acme", &wanted(None, false, true, None)).await.unwrap();
        assert_eq!(last_arguments(&asked).1, json!({ "limit": 200, "orderBy": "updatedAt" }));

        let kinds = vec![RicracStateKind::Unstarted, RicracStateKind::Started];
        ricrac.issues(&store, "acme", &Wanted { states: kinds, ..wanted(None, false, false, None) }).await.unwrap();
        let filter = json!({ "state": { "type": { "in": ["unstarted", "started"] } } });
        assert_eq!(last_arguments(&asked).1["filter"], filter);

        ricrac.issues(&store, "acme", &wanted(Some("team"), true, false, Some(" eng-7 "))).await.unwrap();
        let searched = json!({ "limit": 200, "orderBy": "updatedAt", "team": "team", "query": "eng-7" });
        assert_eq!(last_arguments(&asked).1, searched);
    }

    #[tokio::test]
    async fn the_issues_of_every_page_are_listed() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (ricrac, asked) = fake_ricrac().await;
        connected(&ricrac, &store).await;

        let issues = ricrac.issues(&store, "acme", &wanted(Some("paged"), false, true, None)).await.unwrap();
        let done = RicracIssue { state: RicracState { id: "done".to_string(), ..todo() }, ..greet() };
        assert_eq!(issues, vec![done, greet()]);
        let next = json!({ "limit": 200, "orderBy": "updatedAt", "team": "paged", "cursor": "next" });
        assert_eq!(last_arguments(&asked).1, next);
    }

    #[tokio::test]
    async fn an_issue_is_read_with_its_comments_changed_commented_on_and_filed() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (ricrac, asked) = fake_ricrac().await;
        connected(&ricrac, &store).await;

        let detail = ricrac.issue(&store, "acme", "issue").await.unwrap();
        let authors: Vec<(&str, &str)> =
            detail.comments.iter().map(|comment| (comment.author.as_str(), comment.body.as_str())).collect();
        assert_eq!(detail.issue, greet());
        assert_eq!(detail.description, "Say **hello** to whoever runs it.");
        let said = [("Ricrac", "Which name?"), ("Ada", "Done in a branch."), ("Grace", "The one it's given.")];
        assert_eq!(authors, said);
        assert_eq!(last_arguments(&asked).1, json!({ "id": "issue", "includeComments": true }));

        let change = RicracChange { state: Some("done".to_string()), assignee: Some(String::new()), priority: Some(1) };
        let changed = ricrac.update(&store, "acme", "issue", &change).await.unwrap();
        assert_eq!(changed, RicracIssue { state: RicracState { id: "done".to_string(), ..todo() }, ..greet() });
        let arguments = json!({ "id": "issue", "state": "done", "assignee": null, "priority": 1 });
        assert_eq!(last_arguments(&asked), ("/v1/ops/save_issue".to_string(), arguments));

        ricrac
            .update(&store, "acme", "issue", &RicracChange { priority: Some(0), ..Default::default() })
            .await
            .unwrap();
        assert_eq!(last_arguments(&asked).1, json!({ "id": "issue", "priority": 0 }));

        ricrac.comment(&store, "acme", "issue", "On it.").await.unwrap();
        let (path, commented, _) = asked.lock().unwrap().iter().rev().nth(1).unwrap().clone();
        let commented: Value = serde_json::from_str(&commented).unwrap();
        assert_eq!(
            (path.as_str(), commented),
            ("/v1/ops/save_comment", json!({ "issueId": "issue", "body": "On it." }))
        );

        let new = NewRicracIssue {
            team: "team".to_string(),
            title: " Greet by name ".to_string(),
            description: String::new(),
            state: Some("todo".to_string()),
            assignee: None,
            priority: 2,
        };
        assert_eq!(ricrac.create(&store, "acme", &new).await.unwrap(), greet());
        let filed = json!({ "team": "team", "title": "Greet by name", "priority": 2, "state": "todo" });
        assert_eq!(last_arguments(&asked).1, filed);
        assert!(
            ricrac.create(&store, "acme", &NewRicracIssue { title: " ".to_string(), ..new.clone() }).await.is_err()
        );

        let refused = ricrac.create(&store, "acme", &NewRicracIssue { title: "Fail".to_string(), ..new }).await;
        assert_eq!(refused.unwrap_err().to_string(), "Ricrac said: No team \"NOPE\"; teams are ENG");
    }

    #[tokio::test]
    async fn a_teams_statuses_come_in_the_order_an_issue_goes_through_them_with_the_users_to_assign() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (ricrac, _) = fake_ricrac().await;
        connected(&ricrac, &store).await;

        let (teams, users) = ricrac.teams(&store, "acme").await.unwrap();
        let names: Vec<&str> = teams[0].states.iter().map(|state| state.name.as_str()).collect();
        assert_eq!((teams[0].key.as_str(), names), ("ENG", vec!["Todo", "In Progress", "In Review", "Done"]));
        let users: Vec<(&str, bool)> = users.iter().map(|user| (user.name.as_str(), user.me)).collect();
        assert_eq!(users, [("Ada", true), ("Grace", false)]);
    }

    #[tokio::test]
    async fn a_token_about_to_run_out_is_renewed_and_a_connection_ricrac_ended_is_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (ricrac, asked) = fake_ricrac().await;
        connected(&ricrac, &store).await;

        change_grant(&store, "expires_at", json!(now() + 60.0));
        ricrac.issues(&store, "acme", &wanted(None, true, false, None)).await.unwrap();
        assert_eq!(asked.lock().unwrap().last().unwrap().2, "renewed");
        ricrac.issues(&store, "acme", &wanted(None, true, false, None)).await.unwrap();
        let renewals =
            asked.lock().unwrap().iter().filter(|(_, body, _)| body.contains("grant_type=refresh_token")).count();
        assert_eq!(renewals, 1, "the renewed token is kept");

        change_grant(&store, "access_token", json!("revoked"));
        let ended = ricrac.issues(&store, "acme", &wanted(None, true, false, None)).await.unwrap_err();
        assert_eq!(ended.to_string(), "Ricrac ended this connection. Connect the workspace again.");
        assert!(ricrac.connections(&store).is_empty());

        connected(&ricrac, &store).await;
        change_grant(&store, "expires_at", json!(0));
        change_grant(&store, "refresh_token", json!("taken back"));
        assert!(ricrac.issues(&store, "acme", &wanted(None, true, false, None)).await.is_err());
        assert!(ricrac.connections(&store).is_empty());
    }
}
