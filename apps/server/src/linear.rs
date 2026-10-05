//! The server's connections to Linear workspaces: the user approves Motile's application in a
//! browser, and the token Linear grants for it is kept here, never anywhere else. With it the
//! server reads a workspace's teams and issues and moves an issue to another status.

use std::time::Duration;

use anyhow::{Context, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use motile_protocol::wire::{
    Label, LinearChange, LinearComment, LinearConnection, LinearIssue, LinearIssueDetail, LinearState, LinearStateKind,
    LinearTeam, LinearUser, NewLinearIssue,
};
use motile_protocol::{LINEAR_REDIRECT, now};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::pull_requests::time;
use crate::store::Store;

/// Motile's application at Linear. It has no secret: a code is exchanged with the verifier that
/// its sign-in page was opened with.
const CLIENT_ID: &str = "8f0db699b33a443cbf216ca079d32fe9";
const AUTHORIZE_URL: &str = "https://linear.app";
const API_URL: &str = "https://api.linear.app";
const SCOPE: &str = "read,write";
const GRANTS: &str = "linear";
const TIMEOUT: Duration = Duration::from_secs(20);
/// A token this close to its end is renewed before it is used.
const RENEW_AHEAD: f64 = 300.0;
const ISSUE: &str = "id identifier title url priority branchName updatedAt team { id } \
    assignee { id name isMe } state { id name type color position } labels(first: 5) { nodes { name color } }";

/// Which issues a list is of.
pub struct Wanted {
    pub team: Option<String>,
    pub mine: bool,
    pub closed: bool,
    pub search: Option<String>,
}

pub struct Linear {
    client_id: String,
    authorize_url: String,
    api_url: String,
    /// The connection that waits for the user to approve it.
    pending: std::sync::Mutex<Option<Pending>>,
    /// Held while the grants are changed, so that a token is renewed once.
    changing: tokio::sync::Mutex<()>,
}

struct Pending {
    state: String,
    verifier: String,
}

/// What Linear granted for a workspace, as it is kept in the settings.
#[derive(Serialize, Deserialize)]
struct Grant {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: f64,
    connection: LinearConnection,
}

#[derive(Deserialize)]
struct Granted {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: f64,
}

/// Linear no longer takes the token: the user took the grant back, or it ran out.
#[derive(Debug)]
struct Ended;

impl std::fmt::Display for Ended {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Linear ended this connection. Connect the workspace again.")
    }
}

impl std::error::Error for Ended {}

impl Linear {
    /// `MOTILE_LINEAR_URL` is a stand-in for Linear, and `MOTILE_LINEAR_CLIENT_ID` another
    /// application than Motile's.
    pub fn from_environment() -> Self {
        let stand_in = std::env::var("MOTILE_LINEAR_URL").ok().filter(|url| !url.is_empty());
        let client_id = std::env::var("MOTILE_LINEAR_CLIENT_ID").unwrap_or_else(|_| CLIENT_ID.to_string());
        let authorize_url = stand_in.clone().unwrap_or_else(|| AUTHORIZE_URL.to_string());
        let api_url = stand_in.unwrap_or_else(|| API_URL.to_string());
        Self::new(client_id, authorize_url, api_url)
    }

    fn new(client_id: String, authorize_url: String, api_url: String) -> Self {
        Self { client_id, authorize_url, api_url, pending: std::sync::Mutex::default(), changing: Default::default() }
    }

    pub fn connections(&self, store: &Store) -> Vec<LinearConnection> {
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
            ("redirect_uri", LINEAR_REDIRECT),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("state", &state),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            // Linear's page is where the workspace is chosen, and without this Linear skips it for
            // someone who approved before.
            ("prompt", "consent"),
        ];
        let url = reqwest::Url::parse_with_params(&format!("{}/oauth/authorize", self.authorize_url), parameters)?;
        *self.pending.lock().unwrap() = Some(Pending { state, verifier });
        Ok(url.into())
    }

    /// Keeps what Linear grants for the code beside the other workspaces', in place of what was
    /// kept for the same workspace.
    pub async fn finish(&self, store: &Store, code: &str, state: &str) -> anyhow::Result<Vec<LinearConnection>> {
        let pending = self.pending.lock().unwrap().take_if(|pending| pending.state == state);
        let Some(pending) = pending else {
            bail!("This isn't the connection that was started here. Connect Linear again.");
        };
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", LINEAR_REDIRECT),
            ("client_id", &self.client_id),
            ("code_verifier", &pending.verifier),
        ];
        let answer = http()?.post(format!("{}/oauth/token", self.api_url)).form(&form).send().await;
        let answer = answer.context("Linear couldn't be reached.")?;
        if !answer.status().is_success() {
            bail!("Linear refused the connection: {}", refusal(answer).await);
        }
        let granted: Granted = answer.json().await.context("Linear's answer couldn't be read.")?;
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

    /// Forgets the grant even when Linear can't be told, so that a workspace can always be
    /// disconnected. Answers with the workspaces that stay.
    pub async fn disconnect(&self, store: &Store, workspace: &str) -> anyhow::Result<Vec<LinearConnection>> {
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
        let form = [("token", token.as_str()), ("token_type_hint", hint)];
        let revoked = http()?.post(format!("{}/oauth/revoke", self.api_url)).form(&form).send().await;
        if let Err(error) = revoked.and_then(|answer| answer.error_for_status()) {
            tracing::warn!("Linear didn't take the grant back: {error}");
        }
        keep(store, &grants)?;
        Ok(grants.into_iter().map(|grant| grant.connection).collect())
    }

    /// The teams by name, each with its statuses in the order an issue goes through them, and
    /// the users who can be assigned, by name.
    pub async fn teams(&self, store: &Store, workspace: &str) -> anyhow::Result<(Vec<LinearTeam>, Vec<LinearUser>)> {
        let query = "{ teams(first: 50) { nodes { id key name \
            states(first: 30) { nodes { id name type color position } } } } \
            users(first: 100) { nodes { id name isMe active } } }";
        let found = self.ask(store, workspace, query, json!({})).await?;
        let mut teams: Vec<LinearTeam> = nodes(&found["teams"]).iter().filter_map(read_team).collect();
        teams.sort_by_key(|team| team.name.to_lowercase());
        let active = nodes(&found["users"]).iter().filter(|user| user["active"] != false);
        let mut users: Vec<LinearUser> = active.filter_map(read_user).collect();
        users.sort_by_key(|user| user.name.to_lowercase());
        Ok((teams, users))
    }

    pub async fn issues(&self, store: &Store, workspace: &str, wanted: &Wanted) -> anyhow::Result<Vec<LinearIssue>> {
        let mut filter = json!({});
        if let Some(team) = &wanted.team {
            filter["team"] = json!({ "id": { "eq": team } });
        }
        match wanted.search.as_deref().map(str::trim).filter(|words| !words.is_empty()) {
            Some(words) => filter["or"] = searched(words),
            None => {
                if wanted.mine {
                    filter["assignee"] = json!({ "isMe": { "eq": true } });
                }
                if !wanted.closed {
                    filter["state"] = json!({ "type": { "nin": ["completed", "canceled"] } });
                }
            }
        }
        let query = format!(
            "query($filter: IssueFilter) {{ issues(filter: $filter, first: 100, orderBy: updatedAt) {{ nodes {{ {ISSUE} }} }} }}"
        );
        let found = self.ask(store, workspace, &query, json!({ "filter": filter })).await?;
        Ok(nodes(&found["issues"]).iter().filter_map(read_issue).collect())
    }

    pub async fn issue(&self, store: &Store, workspace: &str, issue: &str) -> anyhow::Result<LinearIssueDetail> {
        let query = format!(
            "query($id: String!) {{ issue(id: $id) {{ {ISSUE} description \
             comments(first: 100) {{ nodes {{ id body createdAt user {{ name }} }} }} }} }}"
        );
        let found = self.ask(store, workspace, &query, json!({ "id": issue })).await?;
        read_detail(&found["issue"]).context("Linear doesn't have this issue.")
    }

    pub async fn update(
        &self,
        store: &Store,
        workspace: &str,
        issue: &str,
        change: &LinearChange,
    ) -> anyhow::Result<LinearIssue> {
        let mut input = json!({});
        if let Some(state) = &change.state {
            input["stateId"] = json!(state);
        }
        if let Some(assignee) = &change.assignee {
            input["assigneeId"] = if assignee.is_empty() { Value::Null } else { json!(assignee) };
        }
        if let Some(priority) = change.priority {
            input["priority"] = json!(priority);
        }
        let query = format!(
            "mutation($id: String!, $input: IssueUpdateInput!) {{ issueUpdate(id: $id, input: $input) {{ issue {{ {ISSUE} }} }} }}"
        );
        let changed = self.ask(store, workspace, &query, json!({ "id": issue, "input": input })).await?;
        read_issue(&changed["issueUpdate"]["issue"]).context("Linear didn't say what became of the issue.")
    }

    /// Answers with the issue as it is with the comment.
    pub async fn comment(
        &self,
        store: &Store,
        workspace: &str,
        issue: &str,
        body: &str,
    ) -> anyhow::Result<LinearIssueDetail> {
        let query = "mutation($input: CommentCreateInput!) { commentCreate(input: $input) { success } }";
        self.ask(store, workspace, query, json!({ "input": { "issueId": issue, "body": body } })).await?;
        self.issue(store, workspace, issue).await
    }

    pub async fn create(&self, store: &Store, workspace: &str, new: &NewLinearIssue) -> anyhow::Result<LinearIssue> {
        if new.title.trim().is_empty() {
            bail!("An issue needs a title.");
        }
        let mut input = json!({ "teamId": new.team, "title": new.title.trim(), "priority": new.priority });
        if !new.description.trim().is_empty() {
            input["description"] = json!(new.description);
        }
        if let Some(state) = &new.state {
            input["stateId"] = json!(state);
        }
        if let Some(assignee) = &new.assignee {
            input["assigneeId"] = json!(assignee);
        }
        let query =
            format!("mutation($input: IssueCreateInput!) {{ issueCreate(input: $input) {{ issue {{ {ISSUE} }} }} }}");
        let filed = self.ask(store, workspace, &query, json!({ "input": input })).await?;
        read_issue(&filed["issueCreate"]["issue"]).context("Linear didn't say what became of the issue.")
    }

    /// What Linear answers the workspace's user. A connection Linear has ended is forgotten.
    async fn ask(&self, store: &Store, workspace: &str, query: &str, variables: Value) -> anyhow::Result<Value> {
        let token = self.token(store, workspace).await?;
        let answer = self.graphql(&token, query, variables).await;
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
            bail!("Your server isn't connected to this Linear workspace.");
        };
        let fresh = grant.expires_at - now() > RENEW_AHEAD;
        let Some(refresh_token) = grant.refresh_token.clone().filter(|_| !fresh) else {
            return Ok(grant.access_token.clone());
        };
        let form = [("grant_type", "refresh_token"), ("refresh_token", &refresh_token), ("client_id", &self.client_id)];
        let answer = http()?.post(format!("{}/oauth/token", self.api_url)).form(&form).send().await;
        let answer = answer.context("Linear couldn't be reached.")?;
        if answer.status().is_client_error() {
            grants.retain(|grant| grant.connection.id != workspace);
            keep(store, &grants)?;
            return Err(Ended.into());
        }
        if !answer.status().is_success() {
            bail!("Linear didn't renew the connection: {}", refusal(answer).await);
        }
        let granted: Granted = answer.json().await.context("Linear's answer couldn't be read.")?;
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

    async fn who(&self, access_token: &str) -> anyhow::Result<LinearConnection> {
        let query = "{ viewer { name } organization { id name } }";
        let answer = self.graphql(access_token, query, json!({})).await?;
        let said = |of: &str, what: &str| answer[of][what].as_str().map(str::to_string);
        let (Some(id), Some(workspace), Some(user)) =
            (said("organization", "id"), said("organization", "name"), said("viewer", "name"))
        else {
            bail!("Linear didn't say whose workspace this is.");
        };
        Ok(LinearConnection { id, workspace, user })
    }

    /// The `data` of Linear's answer, or what it said went wrong.
    async fn graphql(&self, access_token: &str, query: &str, variables: Value) -> anyhow::Result<Value> {
        let asked = json!({ "query": query, "variables": variables });
        let request = http()?.post(format!("{}/graphql", self.api_url)).bearer_auth(access_token).json(&asked);
        let answer = request.send().await.context("Linear couldn't be reached.")?;
        let status = answer.status();
        let mut said: Value = answer.json().await.unwrap_or_default();
        let error = &said["errors"][0];
        if status == reqwest::StatusCode::UNAUTHORIZED || error["extensions"]["code"] == "AUTHENTICATION_ERROR" {
            return Err(Ended.into());
        }
        if let Some(message) = error["extensions"]["userPresentableMessage"].as_str().or(error["message"].as_str()) {
            bail!("Linear said: {message}");
        }
        if !status.is_success() {
            bail!("Linear answered {status}.");
        }
        Ok(said["data"].take())
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

/// Why Linear refused, in its words where it gave any.
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

fn read_team(team: &Value) -> Option<LinearTeam> {
    let mut states: Vec<LinearState> = nodes(&team["states"]).iter().filter_map(read_state).collect();
    states.sort_by(|a, b| (workflow(a.kind), a.position).partial_cmp(&(workflow(b.kind), b.position)).unwrap());
    Some(LinearTeam { id: text(&team["id"])?, key: text(&team["key"])?, name: text(&team["name"])?, states })
}

/// Where a kind of status comes as an issue goes from reported to closed.
fn workflow(kind: LinearStateKind) -> u8 {
    match kind {
        LinearStateKind::Triage => 0,
        LinearStateKind::Backlog => 1,
        LinearStateKind::Unstarted => 2,
        LinearStateKind::Started => 3,
        LinearStateKind::Completed => 4,
        LinearStateKind::Canceled => 5,
    }
}

fn read_state(state: &Value) -> Option<LinearState> {
    let kind = match state["type"].as_str()? {
        "triage" => LinearStateKind::Triage,
        "unstarted" => LinearStateKind::Unstarted,
        "started" => LinearStateKind::Started,
        "completed" => LinearStateKind::Completed,
        "canceled" => LinearStateKind::Canceled,
        _ => LinearStateKind::Backlog,
    };
    Some(LinearState {
        id: text(&state["id"])?,
        name: text(&state["name"])?,
        kind,
        color: color(&state["color"]),
        position: state["position"].as_f64().unwrap_or(0.0),
    })
}

/// The issues with the words in their title or description, or with that number.
fn searched(words: &str) -> Value {
    let mut either = vec![
        json!({ "title": { "containsIgnoreCase": words } }),
        json!({ "description": { "containsIgnoreCase": words } }),
    ];
    let number = words.rsplit('-').next().and_then(|number| number.parse::<u32>().ok());
    if let Some(number) = number {
        either.push(json!({ "number": { "eq": number } }));
    }
    Value::Array(either)
}

fn read_user(user: &Value) -> Option<LinearUser> {
    Some(LinearUser { id: text(&user["id"])?, name: text(&user["name"])?, me: user["isMe"] == true })
}

fn read_detail(issue: &Value) -> Option<LinearIssueDetail> {
    let comment = |comment: &Value| {
        Some(LinearComment {
            id: text(&comment["id"])?,
            author: text(&comment["user"]["name"]).unwrap_or_else(|| "Linear".to_string()),
            body: text(&comment["body"])?,
            created_at: time(&comment["createdAt"]).unwrap_or(0.0),
        })
    };
    let mut comments: Vec<LinearComment> = nodes(&issue["comments"]).iter().filter_map(comment).collect();
    comments.sort_by(|a, b| a.created_at.total_cmp(&b.created_at));
    Some(LinearIssueDetail {
        issue: read_issue(issue)?,
        description: text(&issue["description"]).unwrap_or_default(),
        comments,
    })
}

fn read_issue(issue: &Value) -> Option<LinearIssue> {
    let label = |label: &Value| Some(Label { name: text(&label["name"])?, color: color(&label["color"]) });
    Some(LinearIssue {
        id: text(&issue["id"])?,
        identifier: text(&issue["identifier"])?,
        title: text(&issue["title"])?,
        url: text(&issue["url"])?,
        priority: issue["priority"].as_f64().unwrap_or(0.0) as u8,
        state: read_state(&issue["state"])?,
        team: text(&issue["team"]["id"])?,
        assignee: read_user(&issue["assignee"]),
        labels: nodes(&issue["labels"]).iter().filter_map(label).collect(),
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
            "id": "issue", "identifier": "ENG-7", "title": "Greet by name", "url": "https://linear.app/engines/issue/ENG-7",
            "priority": 2, "updatedAt": "2026-10-05T10:00:00.000Z", "team": { "id": "team" }, "branchName": "ada/eng-7-greet-by-name",
            "assignee": { "id": "ada", "name": "Ada", "isMe": true }, "description": "Say **hello** to whoever runs it.",
            "comments": { "nodes": [
                { "id": "second", "body": "Done in a branch.", "createdAt": "2026-10-05T12:00:00.000Z", "user": { "name": "Ada" } },
                { "id": "first", "body": "Which name?", "createdAt": "2026-10-05T11:00:00.000Z", "user": null },
            ] },
            "state": { "id": state, "name": "Todo", "type": "unstarted", "color": "#e2e2e2", "position": 1 },
            "labels": { "nodes": [{ "name": "Bug", "color": "#eb5757" }] },
        })
    }

    /// Stands in for Linear: grants a token for the code `good`, renews one for a refresh token
    /// it gave, and answers the queries with one team and one issue.
    async fn fake_linear() -> (Linear, Asked) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let asked = Asked::default();
        let heard = asked.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let (path, body, token) = read_request(&mut stream).await;
                let refused = ("400 Bad Request", json!({ "error_description": "authorization code is invalid" }));
                let (status, answer) = match path.as_str() {
                    "/oauth/token" if body.contains("code=good&") => {
                        ("200 OK", json!({ "access_token": "access", "refresh_token": "refresh", "expires_in": 86399 }))
                    }
                    "/oauth/token" if body.contains("refresh_token=refresh&") => (
                        "200 OK",
                        json!({ "access_token": "renewed", "refresh_token": "refresh2", "expires_in": 86399 }),
                    ),
                    "/oauth/token" => refused,
                    "/graphql" if token == "revoked" => {
                        ("400 Bad Request", json!({ "errors": [{ "extensions": { "code": "AUTHENTICATION_ERROR" } }] }))
                    }
                    "/graphql" if body.contains("issueUpdate") => {
                        ("200 OK", json!({ "data": { "issueUpdate": { "issue": issue("done") } } }))
                    }
                    "/graphql" if body.contains("issueCreate") => {
                        ("200 OK", json!({ "data": { "issueCreate": { "issue": issue("todo") } } }))
                    }
                    "/graphql" if body.contains("commentCreate") => {
                        ("200 OK", json!({ "data": { "commentCreate": { "success": true } } }))
                    }
                    "/graphql" if body.contains("issue(id") => {
                        ("200 OK", json!({ "data": { "issue": issue("todo") } }))
                    }
                    "/graphql" if body.contains("issues(") => {
                        ("200 OK", json!({ "data": { "issues": { "nodes": [issue("todo")] } } }))
                    }
                    "/graphql" if body.contains("teams(") => (
                        "200 OK",
                        json!({ "data": { "teams": { "nodes": [{ "id": "team", "key": "ENG", "name": "Engines", "states": { "nodes": [
                            { "id": "done", "name": "Done", "type": "completed", "color": "#5e6ad2", "position": 3 },
                            { "id": "review", "name": "In Review", "type": "started", "color": "#0f783c", "position": 3 },
                            { "id": "progress", "name": "In Progress", "type": "started", "color": "#f2c94c", "position": 2 },
                            { "id": "todo", "name": "Todo", "type": "unstarted", "color": "#e2e2e2", "position": 1 },
                        ] } }] }, "users": { "nodes": [
                            { "id": "grace", "name": "Grace", "isMe": false, "active": true },
                            { "id": "gone", "name": "Charles", "isMe": false, "active": false },
                            { "id": "ada", "name": "Ada", "isMe": true, "active": true },
                        ] } } }),
                    ),
                    "/graphql" => (
                        "200 OK",
                        json!({ "data": { "viewer": { "name": "Ada" }, "organization": { "id": "org", "name": "Engines" } } }),
                    ),
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
        (Linear::new("motile".to_string(), url.clone(), url), asked)
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

    fn ada() -> LinearConnection {
        LinearConnection { id: "org".to_string(), workspace: "Engines".to_string(), user: "Ada".to_string() }
    }

    async fn connected(linear: &Linear, store: &Store) {
        let page = parameters(&linear.connect().unwrap());
        linear.finish(store, "good", &page["state"]).await.unwrap();
    }

    /// Changes what is kept of the one grant, as time or Linear would.
    fn change_grant(store: &Store, name: &str, value: Value) {
        let mut kept: Value = serde_json::from_str(&store.setting(GRANTS).unwrap()).unwrap();
        kept[0][name] = value;
        store.set_setting(GRANTS, Some(&kept.to_string())).unwrap();
    }

    #[tokio::test]
    async fn an_approved_connection_is_exchanged_with_its_verifier_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let (linear, asked) = fake_linear().await;
        let page = parameters(&linear.connect().unwrap());
        assert_eq!(page["client_id"], "motile");
        assert_eq!(page["redirect_uri"], "motile://linear");

        let connections = linear.finish(&store(&dir), "good", &page["state"]).await.unwrap();
        assert_eq!(connections, [ada()]);

        let (_, exchange, _) = asked.lock().unwrap()[0].clone();
        let sent = parameters(&format!("http://linear/?{exchange}"));
        assert_eq!(URL_SAFE_NO_PAD.encode(Sha256::digest(&sent["code_verifier"])), page["code_challenge"]);
        assert!(!sent.contains_key("client_secret"));
        assert_eq!(linear.connections(&store(&dir)), [ada()]);
    }

    #[tokio::test]
    async fn a_workspace_connected_again_is_kept_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (linear, _) = fake_linear().await;
        connected(&linear, &store).await;
        connected(&linear, &store).await;
        assert_eq!(linear.connections(&store), [ada()]);
    }

    #[tokio::test]
    async fn a_code_is_refused_without_the_state_it_was_asked_with_or_when_linear_refuses_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (linear, asked) = fake_linear().await;
        let page = parameters(&linear.connect().unwrap());

        assert!(linear.finish(&store, "good", "another").await.is_err());
        assert!(asked.lock().unwrap().is_empty());

        let refused = linear.finish(&store, "bad", &page["state"]).await.unwrap_err();
        assert_eq!(refused.to_string(), "Linear refused the connection: authorization code is invalid");
        assert!(linear.finish(&store, "good", &page["state"]).await.is_err(), "a state works once");
        assert!(linear.connections(&store).is_empty());
    }

    #[tokio::test]
    async fn disconnecting_takes_the_grant_back_at_linear_and_forgets_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (linear, asked) = fake_linear().await;
        connected(&linear, &store).await;

        assert!(linear.disconnect(&store, "org").await.unwrap().is_empty());

        let (path, body, _) = asked.lock().unwrap().last().unwrap().clone();
        assert_eq!((path.as_str(), body.as_str()), ("/oauth/revoke", "token=refresh&token_type_hint=refresh_token"));
        assert!(linear.connections(&store).is_empty());
    }

    fn todo() -> LinearState {
        LinearState {
            id: "todo".to_string(),
            name: "Todo".to_string(),
            kind: LinearStateKind::Unstarted,
            color: "e2e2e2".to_string(),
            position: 1.0,
        }
    }

    fn greet() -> LinearIssue {
        LinearIssue {
            id: "issue".to_string(),
            identifier: "ENG-7".to_string(),
            title: "Greet by name".to_string(),
            url: "https://linear.app/engines/issue/ENG-7".to_string(),
            priority: 2,
            state: todo(),
            team: "team".to_string(),
            assignee: Some(LinearUser { id: "ada".to_string(), name: "Ada".to_string(), me: true }),
            labels: vec![Label { name: "Bug".to_string(), color: "eb5757".to_string() }],
            branch_name: "ada/eng-7-greet-by-name".to_string(),
            updated_at: 1_791_194_400.0,
        }
    }

    fn wanted(team: Option<&str>, mine: bool, closed: bool, search: Option<&str>) -> Wanted {
        Wanted { team: team.map(str::to_string), mine, closed, search: search.map(str::to_string) }
    }

    /// The variables of the query the stand-in was last asked.
    fn last_variables(asked: &Asked) -> Value {
        let (_, body, _) = asked.lock().unwrap().last().unwrap().clone();
        serde_json::from_str::<Value>(&body).unwrap()["variables"].take()
    }

    #[tokio::test]
    async fn the_issues_asked_for_are_read_with_the_filter_that_chooses_them() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (linear, asked) = fake_linear().await;
        connected(&linear, &store).await;

        let issues = linear.issues(&store, "org", &wanted(Some("team"), true, false, None)).await.unwrap();
        assert_eq!(issues, std::slice::from_ref(&greet()));
        let filter = json!({
            "team": { "id": { "eq": "team" } },
            "assignee": { "isMe": { "eq": true } },
            "state": { "type": { "nin": ["completed", "canceled"] } },
        });
        assert_eq!(last_variables(&asked)["filter"], filter);
        assert_eq!(asked.lock().unwrap().last().unwrap().2, "access");

        linear.issues(&store, "org", &wanted(None, false, true, None)).await.unwrap();
        assert_eq!(last_variables(&asked)["filter"], json!({}));

        linear.issues(&store, "org", &wanted(Some("team"), true, false, Some(" eng-7 "))).await.unwrap();
        let searched = json!({
            "team": { "id": { "eq": "team" } },
            "or": [
                { "title": { "containsIgnoreCase": "eng-7" } },
                { "description": { "containsIgnoreCase": "eng-7" } },
                { "number": { "eq": 7 } },
            ],
        });
        assert_eq!(last_variables(&asked)["filter"], searched);
    }

    #[tokio::test]
    async fn an_issue_is_read_with_its_comments_changed_commented_on_and_filed() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (linear, asked) = fake_linear().await;
        connected(&linear, &store).await;

        let detail = linear.issue(&store, "org", "issue").await.unwrap();
        let authors: Vec<(&str, &str)> =
            detail.comments.iter().map(|comment| (comment.author.as_str(), comment.body.as_str())).collect();
        assert_eq!(detail.issue, greet());
        assert_eq!(detail.description, "Say **hello** to whoever runs it.");
        assert_eq!(authors, [("Linear", "Which name?"), ("Ada", "Done in a branch.")]);

        let change = LinearChange { state: Some("done".to_string()), assignee: Some(String::new()), priority: Some(1) };
        let changed = linear.update(&store, "org", "issue", &change).await.unwrap();
        assert_eq!(changed, LinearIssue { state: LinearState { id: "done".to_string(), ..todo() }, ..greet() });
        let input = json!({ "stateId": "done", "assigneeId": null, "priority": 1 });
        assert_eq!(last_variables(&asked), json!({ "id": "issue", "input": input }));

        linear.update(&store, "org", "issue", &LinearChange { priority: Some(0), ..Default::default() }).await.unwrap();
        assert_eq!(last_variables(&asked)["input"], json!({ "priority": 0 }));

        linear.comment(&store, "org", "issue", "On it.").await.unwrap();
        let commented = asked.lock().unwrap().iter().rev().nth(1).unwrap().1.clone();
        let commented: Value = serde_json::from_str(&commented).unwrap();
        assert_eq!(commented["variables"]["input"], json!({ "issueId": "issue", "body": "On it." }));

        let new = NewLinearIssue {
            team: "team".to_string(),
            title: " Greet by name ".to_string(),
            description: String::new(),
            state: Some("todo".to_string()),
            assignee: None,
            priority: 2,
        };
        assert_eq!(linear.create(&store, "org", &new).await.unwrap(), greet());
        let filed = json!({ "teamId": "team", "title": "Greet by name", "priority": 2, "stateId": "todo" });
        assert_eq!(last_variables(&asked)["input"], filed);
        assert!(linear.create(&store, "org", &NewLinearIssue { title: " ".to_string(), ..new }).await.is_err());
    }

    #[tokio::test]
    async fn a_teams_statuses_come_in_the_order_an_issue_goes_through_them_with_the_users_to_assign() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (linear, _) = fake_linear().await;
        connected(&linear, &store).await;

        let (teams, users) = linear.teams(&store, "org").await.unwrap();
        let names: Vec<&str> = teams[0].states.iter().map(|state| state.name.as_str()).collect();
        assert_eq!((teams[0].key.as_str(), names), ("ENG", vec!["Todo", "In Progress", "In Review", "Done"]));
        let users: Vec<(&str, bool)> = users.iter().map(|user| (user.name.as_str(), user.me)).collect();
        assert_eq!(users, [("Ada", true), ("Grace", false)]);
    }

    #[tokio::test]
    async fn a_token_about_to_run_out_is_renewed_and_a_connection_linear_ended_is_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (linear, asked) = fake_linear().await;
        connected(&linear, &store).await;

        change_grant(&store, "expires_at", json!(now() + 60.0));
        linear.issues(&store, "org", &wanted(None, true, false, None)).await.unwrap();
        assert_eq!(asked.lock().unwrap().last().unwrap().2, "renewed");
        linear.issues(&store, "org", &wanted(None, true, false, None)).await.unwrap();
        let renewals =
            asked.lock().unwrap().iter().filter(|(_, body, _)| body.contains("grant_type=refresh_token")).count();
        assert_eq!(renewals, 1, "the renewed token is kept");

        change_grant(&store, "access_token", json!("revoked"));
        let ended = linear.issues(&store, "org", &wanted(None, true, false, None)).await.unwrap_err();
        assert_eq!(ended.to_string(), "Linear ended this connection. Connect the workspace again.");
        assert!(linear.connections(&store).is_empty());

        connected(&linear, &store).await;
        change_grant(&store, "expires_at", json!(0));
        change_grant(&store, "refresh_token", json!("taken back"));
        assert!(linear.issues(&store, "org", &wanted(None, true, false, None)).await.is_err());
        assert!(linear.connections(&store).is_empty());
    }
}
