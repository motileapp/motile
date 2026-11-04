//! The account's MCP servers: MCPs the gateway holds the logins of, which every agent of every server
//! is given, each by its slug. An agent's MCP requests pass through to the MCP server with its login
//! (`gateway.rs`); the gateway itself only asks one for its tools and for a tool the user tries.

pub mod client;
pub mod passthrough;
pub mod sign_in;

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Instant;

use anyhow::{Context, bail};
use motile_protocol::gateway_wire::mcp_slug;
use motile_protocol::gateway_wire::{
    McpAdding, McpAuth, McpChange, McpKey, McpServer, McpStatus, McpTool, ToolKind, ToolPermission, ToolResult,
};
use motile_protocol::now;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::gateway::Gateway;
use crate::store::CallRecord;
use client::{Failure, Session};
use sign_in::{OAuth, RenewError, SignIns};

/// A login is renewed when it expires within this.
const RENEW_BEFORE: f64 = 5.0 * 60.0;
/// What a call keeps of its arguments and its result.
pub const CALL_KEPT: usize = 64 * 1024;
/// Never an MCP server's slug: the agents' own MCP is called that.
const RESERVED: &str = "motile";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Login {
    None,
    Key { header: String, value: String },
    OAuth(OAuth),
}

#[derive(Clone, Debug)]
pub struct McpEntry {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub login: Login,
    pub status: McpStatus,
    pub tools: Vec<McpTool>,
    pub permissions: Permissions,
    pub icon: Option<String>,
    pub created_at: f64,
}

/// What agents may do with an MCP server's tools: a tool's own permission, or else its kind's, so that
/// a tool the MCP server adds later takes its kind's. A read-only tool with none is allowed, any other
/// needs approval.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Permissions {
    #[serde(default)]
    pub groups: HashMap<ToolKind, ToolPermission>,
    #[serde(default)]
    pub tools: HashMap<String, ToolPermission>,
}

impl Permissions {
    pub fn of(&self, name: &str, kind: ToolKind) -> ToolPermission {
        let own = self.tools.get(name);
        let fallback = match kind {
            ToolKind::Read => ToolPermission::Allow,
            ToolKind::Write | ToolKind::Other => ToolPermission::Ask,
        };
        own.or_else(|| self.groups.get(&kind)).copied().unwrap_or(fallback)
    }
}

impl McpEntry {
    /// What agents may do with the tool `name`; one the MCP server hasn't listed is of no kind.
    pub fn permission(&self, name: &str) -> ToolPermission {
        let kind = self.tools.iter().find(|tool| tool.name == name).map_or(ToolKind::Other, McpTool::kind);
        self.permissions.of(name, kind)
    }

    /// The headers its login goes in.
    pub fn login_headers(&self) -> Vec<(String, String)> {
        match &self.login {
            Login::None => vec![],
            Login::Key { header, value } => vec![(header.clone(), value.clone())],
            Login::OAuth(login) => vec![("authorization".into(), format!("Bearer {}", login.access))],
        }
    }

    fn view(&self) -> McpServer {
        let tools = self.tools.iter().map(|tool| {
            let permission = self.permissions.of(&tool.name, tool.kind());
            McpTool { enabled: permission != ToolPermission::Block, permission: Some(permission), ..tool.clone() }
        });
        McpServer {
            id: self.id.clone(),
            slug: self.slug.clone(),
            name: self.name.clone(),
            url: self.url.clone(),
            enabled: self.enabled,
            auth: match self.login {
                Login::None => McpAuth::None,
                Login::Key { .. } => McpAuth::Key,
                Login::OAuth(_) => McpAuth::SignIn,
            },
            status: self.status.clone(),
            tools: tools.collect(),
            icon: self.icon.clone(),
        }
    }
}

#[derive(Default)]
pub struct McpServers {
    list: RwLock<Vec<McpEntry>>,
    pub sign_ins: SignIns,
    /// One renewal at a time: a refresh token may work only once.
    renewing: tokio::sync::Mutex<()>,
}

impl McpServers {
    pub fn new(list: Vec<McpEntry>) -> Self {
        Self { list: RwLock::new(list), ..Self::default() }
    }

    pub fn all(&self) -> Vec<McpEntry> {
        self.list.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    pub fn get(&self, id: &str) -> Option<McpEntry> {
        self.all().into_iter().find(|mcp_server| mcp_server.id == id)
    }

    pub fn by_slug(&self, slug: &str) -> Option<McpEntry> {
        self.all().into_iter().find(|mcp_server| mcp_server.slug == slug)
    }

    pub fn views(&self) -> Vec<McpServer> {
        self.all().iter().map(McpEntry::view).collect()
    }

    /// The slugs the agents are given.
    pub fn given(&self) -> Vec<String> {
        self.all().into_iter().filter(|mcp_server| mcp_server.enabled).map(|mcp_server| mcp_server.slug).collect()
    }
}

impl Gateway {
    fn change_mcp_server(&self, id: &str, change: impl FnOnce(&mut McpEntry)) -> Option<McpEntry> {
        let mut list = self.mcp_servers.list.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mcp_server = list.iter_mut().find(|mcp_server| mcp_server.id == id)?;
        change(mcp_server);
        if let Err(error) = self.store.save_mcp_server(mcp_server, &self.seal) {
            tracing::warn!("couldn't keep a change to an MCP server: {error:#}");
        }
        Some(mcp_server.clone())
    }

    pub fn mcp_server_status(&self, id: &str, status: McpStatus) {
        let Some(mcp_server) = self.mcp_servers.get(id) else { return };
        if mcp_server.status != status {
            self.change_mcp_server(id, |mcp_server| mcp_server.status = status);
        }
    }

    /// The MCP server with a login that isn't about to expire.
    pub async fn fresh_mcp_server(&self, id: &str) -> Option<McpEntry> {
        let mcp_server = self.mcp_servers.get(id)?;
        let Login::OAuth(login) = &mcp_server.login else { return Some(mcp_server) };
        if login.expires_at.is_none_or(|expires_at| expires_at - now() > RENEW_BEFORE) {
            return Some(mcp_server);
        }
        self.renew_mcp_server(id, &login.access).await
    }

    /// Renews the MCP server's login, unless another renewal already replaced `stale`. One whose
    /// refresh was refused has to be signed in again.
    pub async fn renew_mcp_server(&self, id: &str, stale: &str) -> Option<McpEntry> {
        let _renewing = self.mcp_servers.renewing.lock().await;
        let mcp_server = self.mcp_servers.get(id)?;
        let Login::OAuth(login) = &mcp_server.login else { return None };
        if login.access != stale {
            return Some(mcp_server);
        }
        match sign_in::renew(&self.http, login).await {
            Ok(renewed) => self.change_mcp_server(id, |mcp_server| {
                mcp_server.login = Login::OAuth(renewed);
                if mcp_server.status == McpStatus::SignInAgain {
                    mcp_server.status = McpStatus::Ready;
                }
            }),
            Err(RenewError::Dead) => {
                tracing::warn!(mcp_server = mcp_server.slug, "an MCP server's login stopped working");
                self.mcp_server_status(id, McpStatus::SignInAgain);
                None
            }
            Err(RenewError::Failed(error)) => {
                tracing::warn!(mcp_server = mcp_server.slug, "couldn't renew an MCP server's login: {error:#}");
                None
            }
        }
    }

    pub async fn add_mcp_server(
        &self,
        url: &str,
        name: Option<String>,
        key: Option<McpKey>,
    ) -> anyhow::Result<McpAdding> {
        let url = url.trim();
        let parsed = reqwest::Url::parse(url).context("That isn't an address.")?;
        if !["http", "https"].contains(&parsed.scheme()) {
            bail!("An MCP server's address starts with https://.");
        }
        let login = match key {
            Some(key) => Login::Key { header: key.header.trim().to_string(), value: key.value.trim().to_string() },
            None => Login::None,
        };
        let headers = match &login {
            Login::Key { header, value } => vec![(header.clone(), value.clone())],
            _ => vec![],
        };
        match Session::open(&self.http, url, headers).await {
            Ok(session) => {
                let id = self.create_mcp_server(url, name, login, session).await?;
                Ok(McpAdding::Added { id })
            }
            Err(Failure::Unauthorized(_)) if matches!(login, Login::Key { .. }) => {
                bail!("The MCP server refused the key.")
            }
            Err(Failure::Unauthorized(challenge)) => {
                match self
                    .mcp_servers
                    .sign_ins
                    .start(&self.http, &self.upstreams.mcp_redirect, url, name, None, challenge.as_deref())
                    .await?
                {
                    Some((state, url)) => Ok(McpAdding::SignIn { state, url }),
                    None => Ok(McpAdding::NeedsKey),
                }
            }
            Err(Failure::Failed(error)) => Err(error),
        }
    }

    pub async fn sign_mcp_server_in(&self, id: &str) -> anyhow::Result<McpAdding> {
        let mcp_server = self.mcp_servers.get(id).context("There is no such MCP server.")?;
        let started = self
            .mcp_servers
            .sign_ins
            .start(&self.http, &self.upstreams.mcp_redirect, &mcp_server.url, None, Some(mcp_server.id), None)
            .await?;
        let (state, url) = started.context("This MCP server doesn't sign in. Give it a key instead.")?;
        Ok(McpAdding::SignIn { state, url })
    }

    pub async fn finish_mcp_server_sign_in(&self, state: &str, code: &str) -> anyhow::Result<McpAdding> {
        let (pending, login) = self.mcp_servers.sign_ins.finish(&self.http, state, code).await?;
        if let Some(id) = pending.mcp_server {
            self.change_mcp_server(&id, |mcp_server| {
                mcp_server.login = Login::OAuth(login);
                mcp_server.status = McpStatus::Ready;
            })
            .context("The MCP server was removed while it signed in.")?;
            self.refresh_mcp_server(&id).await;
            return Ok(McpAdding::Added { id });
        }
        let headers = vec![("authorization".to_string(), format!("Bearer {}", login.access))];
        let session = match Session::open(&self.http, &pending.url, headers).await {
            Ok(session) => session,
            Err(Failure::Unauthorized(_)) => bail!("The MCP server refused the login it just gave."),
            Err(Failure::Failed(error)) => return Err(error),
        };
        let id = self.create_mcp_server(&pending.url, pending.name, Login::OAuth(login), session).await?;
        Ok(McpAdding::Added { id })
    }

    async fn create_mcp_server(
        &self,
        url: &str,
        name: Option<String>,
        login: Login,
        mut session: Session<'_>,
    ) -> anyhow::Result<String> {
        let listed = session.tools().await;
        let info = session.server_info.clone();
        session.close().await;
        let tools = match listed {
            Ok(tools) => tools.iter().filter_map(tool_of).collect(),
            Err(Failure::Unauthorized(_)) => bail!("The MCP server refused to list its tools."),
            Err(Failure::Failed(error)) => return Err(error),
        };
        let name = name
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .or_else(|| info["title"].as_str().map(String::from))
            .unwrap_or_else(|| name_of_host(url));
        let mut list = self.mcp_servers.list.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        let taken: Vec<String> = list.iter().map(|mcp_server| mcp_server.slug.clone()).collect();
        let mcp_server = McpEntry {
            id: uuid::Uuid::new_v4().to_string(),
            slug: free_slug(&mcp_slug(&name), &taken),
            name,
            url: url.to_string(),
            enabled: true,
            login,
            status: McpStatus::Ready,
            tools,
            permissions: Permissions::default(),
            icon: icon_of(&info),
            created_at: now(),
        };
        self.store.save_mcp_server(&mcp_server, &self.seal)?;
        list.push(mcp_server.clone());
        Ok(mcp_server.id)
    }

    /// Asks the MCP server for its tools again, and says how it answered.
    pub async fn refresh_mcp_server(&self, id: &str) {
        let mut renewed = false;
        loop {
            let Some(mcp_server) = self.fresh_mcp_server(id).await else { return };
            let opened = Session::open(&self.http, &mcp_server.url, mcp_server.login_headers()).await;
            let listed = match opened {
                Ok(mut session) => {
                    let listed = session.tools().await;
                    let info = session.server_info.clone();
                    session.close().await;
                    listed.map(|tools| (tools, info))
                }
                Err(failure) => Err(failure),
            };
            match listed {
                Ok((tools, info)) => {
                    let tools: Vec<McpTool> = tools.iter().filter_map(tool_of).collect();
                    self.change_mcp_server(id, |mcp_server| {
                        mcp_server.tools = tools;
                        mcp_server.icon = icon_of(&info).or(mcp_server.icon.take());
                        mcp_server.status = McpStatus::Ready;
                    });
                    return;
                }
                Err(Failure::Unauthorized(_)) => match &mcp_server.login {
                    Login::OAuth(login) if !renewed => {
                        renewed = true;
                        if self.renew_mcp_server(id, &login.access).await.is_none() {
                            return;
                        }
                    }
                    Login::OAuth(_) => return self.mcp_server_status(id, McpStatus::SignInAgain),
                    _ => {
                        let error = "It refused its key. Give it a new one.".to_string();
                        return self.mcp_server_status(id, McpStatus::Failing { error });
                    }
                },
                Err(Failure::Failed(error)) => {
                    let error = motile_protocol::error_text(&error);
                    return self.mcp_server_status(id, McpStatus::Failing { error });
                }
            }
        }
    }

    /// Renaming an MCP server names it anew for agents too.
    pub fn update_mcp_server(&self, id: &str, change: McpChange) -> anyhow::Result<()> {
        if let McpChange::Name { name } = &change
            && name.trim().is_empty()
        {
            bail!("An MCP server needs a name.");
        }
        if let McpChange::Slug { slug } = &change {
            let slug = mcp_slug(slug);
            let taken = self
                .mcp_servers
                .all()
                .iter()
                .any(|mcp_server| mcp_server.slug.eq_ignore_ascii_case(&slug) && mcp_server.id != id);
            if taken || slug.eq_ignore_ascii_case(RESERVED) {
                bail!("Another MCP server is called {slug}.");
            }
        }
        let others: Vec<String> = self
            .mcp_servers
            .all()
            .into_iter()
            .filter(|mcp_server| mcp_server.id != id)
            .map(|mcp_server| mcp_server.slug)
            .collect();
        let changed = self.change_mcp_server(id, |mcp_server| match change {
            McpChange::Name { name } => {
                mcp_server.name = name.trim().to_string();
                mcp_server.slug = free_slug(&mcp_slug(&name), &others);
            }
            McpChange::Slug { slug } => mcp_server.slug = mcp_slug(&slug),
            McpChange::Enabled { enabled } => mcp_server.enabled = enabled,
            McpChange::ToolPermission { name, permission } => {
                mcp_server.permissions.tools.insert(name, permission);
            }
            McpChange::GroupPermission { kind, permission } => {
                let McpEntry { tools, permissions, .. } = mcp_server;
                permissions.groups.insert(kind, permission);
                for tool in tools.iter().filter(|tool| tool.kind() == kind) {
                    permissions.tools.remove(&tool.name);
                }
            }
            McpChange::Key { key } => {
                mcp_server.login =
                    Login::Key { header: key.header.trim().to_string(), value: key.value.trim().to_string() };
                mcp_server.status = McpStatus::Ready;
            }
        });
        changed.map(|_| ()).context("There is no such MCP server.")
    }

    pub fn remove_mcp_server(&self, id: &str) -> anyhow::Result<()> {
        self.mcp_servers.list.write().unwrap_or_else(|poisoned| poisoned.into_inner()).retain(|kept| kept.id != id);
        self.store.delete_mcp_server(id)
    }

    /// Calls the tool as an agent would, and logs it as tried from Settings.
    pub async fn try_tool(&self, id: &str, tool: &str, arguments: Value) -> anyhow::Result<ToolResult> {
        let started = Instant::now();
        let mut renewed = false;
        let answered = loop {
            let mcp_server = self.fresh_mcp_server(id).await.context("There is no such MCP server.")?;
            let called = match Session::open(&self.http, &mcp_server.url, mcp_server.login_headers()).await {
                Ok(mut session) => {
                    let called = session.call(tool, arguments.clone()).await;
                    session.close().await;
                    called
                }
                Err(failure) => Err(failure),
            };
            match called {
                Err(Failure::Unauthorized(_)) if !renewed => {
                    renewed = true;
                    let Login::OAuth(login) = &mcp_server.login else {
                        break Err("The MCP server refused its key.".into());
                    };
                    if self.renew_mcp_server(id, &login.access).await.is_none() {
                        break Err("The MCP server's login stopped working. Sign in again.".into());
                    }
                }
                Err(Failure::Unauthorized(_)) => {
                    self.mcp_server_status(id, McpStatus::SignInAgain);
                    break Err("The MCP server's login stopped working. Sign in again.".to_string());
                }
                Err(Failure::Failed(error)) => break Err(motile_protocol::error_text(&error)),
                Ok(result) => break Ok(result),
            }
        };
        let (result, error) = match answered {
            Ok(result) => {
                let error = result_error(&result);
                (result, error)
            }
            Err(error) => (Value::Null, Some(error)),
        };
        self.log_call(CallRecord {
            at: now(),
            server: None,
            thread: None,
            mcp_server: id.to_string(),
            tool: tool.to_string(),
            latency_ms: started.elapsed().as_millis() as u64,
            error: error.clone(),
            arguments: cut(&arguments.to_string()),
            result: cut(&result.to_string()),
        });
        Ok(ToolResult { result, error })
    }

    pub fn log_call(&self, call: CallRecord) {
        if let Err(error) = self.store.add_call(&call) {
            tracing::warn!("couldn't log a tool call: {error:#}");
        }
    }

    /// Takes the tools a `tools/list` that passed through gave, so that Settings shows what the
    /// agents see.
    pub fn saw_tools(&self, id: &str, tools: &[Value]) {
        let tools: Vec<McpTool> = tools.iter().filter_map(tool_of).collect();
        let Some(mcp_server) = self.mcp_servers.get(id) else { return };
        if mcp_server.tools != tools || mcp_server.status != McpStatus::Ready {
            self.change_mcp_server(id, |mcp_server| {
                mcp_server.tools = tools;
                mcp_server.status = McpStatus::Ready;
            });
        }
    }
}

/// A tool as `tools/list` gives it.
pub fn tool_of(tool: &Value) -> Option<McpTool> {
    let name = tool["name"].as_str()?;
    let title = tool["title"].as_str().or(tool["annotations"]["title"].as_str());
    let hints = &tool["annotations"];
    let kind = match (hints["readOnlyHint"].as_bool(), hints["destructiveHint"].as_bool()) {
        (Some(true), _) => ToolKind::Read,
        (Some(false), _) | (_, Some(true)) => ToolKind::Write,
        _ => ToolKind::Other,
    };
    Some(McpTool {
        name: name.to_string(),
        title: title.map(String::from),
        description: tool["description"].as_str().map(|text| text.trim().to_string()),
        input_schema: tool["inputSchema"].clone(),
        writes: kind != ToolKind::Read,
        enabled: true,
        kind: Some(kind),
        permission: None,
    })
}

/// What went wrong in a tool's result, by its text.
pub fn result_error(result: &Value) -> Option<String> {
    if result["isError"].as_bool() != Some(true) {
        return None;
    }
    let texts: Vec<&str> =
        result["content"].as_array().into_iter().flatten().filter_map(|part| part["text"].as_str()).collect();
    Some(if texts.is_empty() { "The tool failed.".to_string() } else { texts.join("\n") })
}

pub fn cut(text: &str) -> String {
    if text.len() <= CALL_KEPT {
        return text.to_string();
    }
    let mut end = CALL_KEPT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

fn icon_of(info: &Value) -> Option<String> {
    let icons = info["icons"].as_array()?;
    icons.iter().find_map(|icon| icon["src"].as_str()).map(String::from)
}

/// A name from the address: `mcp.linear.app` is Linear.
fn name_of_host(url: &str) -> String {
    let host = reqwest::Url::parse(url).ok().and_then(|url| url.host_str().map(String::from)).unwrap_or_default();
    let labels: Vec<&str> = host.split('.').collect();
    let skipped = ["mcp", "api", "www", "server", "app"];
    let label = labels
        .iter()
        .take(labels.len().saturating_sub(1).max(1))
        .find(|label| !skipped.contains(label))
        .or(labels.first())
        .copied()
        .unwrap_or("McpEntry");
    let mut characters = label.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => "McpEntry".to_string(),
    }
}

fn free_slug(slug: &str, taken: &[String]) -> String {
    let free = |candidate: &String| {
        [RESERVED.to_string()].iter().chain(taken).all(|taken| !taken.eq_ignore_ascii_case(candidate))
    };
    let first = slug.to_string();
    if free(&first) {
        return first;
    }
    (2..).map(|number| format!("{slug}-{number}")).find(free).expect("some number is free")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_slug_is_never_taken_in_any_case() {
        assert_eq!(free_slug("Linear", &["linear".into(), "Linear-2".into()]), "Linear-3");
        assert_eq!(free_slug("Motile", &[]), "Motile-2");
    }

    #[test]
    fn a_name_comes_from_the_host_without_its_mcp_or_api_part() {
        assert_eq!(name_of_host("https://mcp.linear.app/mcp"), "Linear");
        assert_eq!(name_of_host("https://api.githubcopilot.com/mcp/"), "Githubcopilot");
        assert_eq!(name_of_host("https://sentry.dev/mcp"), "Sentry");
        assert_eq!(name_of_host("http://localhost:4000/mcp"), "Localhost");
    }

    #[test]
    fn a_tools_kind_is_what_its_annotations_say() {
        let kind = |annotations: Value| tool_of(&json!({"name": "a", "annotations": annotations})).unwrap().kind();
        assert_eq!(kind(json!({"readOnlyHint": true})), ToolKind::Read);
        assert_eq!(kind(json!({"readOnlyHint": false})), ToolKind::Write);
        assert_eq!(kind(json!({"destructiveHint": true})), ToolKind::Write);
        assert_eq!(kind(json!({})), ToolKind::Other);
        let bare = tool_of(&json!({"name": "create", "description": " Files one. ", "inputSchema": {}})).unwrap();
        assert!(bare.writes);
        assert_eq!(bare.description.as_deref(), Some("Files one."));
    }

    #[test]
    fn a_tool_takes_its_own_permission_or_its_kinds() {
        let mut permissions = Permissions::default();
        assert_eq!(permissions.of("list", ToolKind::Read), ToolPermission::Allow);
        assert_eq!(permissions.of("create", ToolKind::Write), ToolPermission::Ask);
        assert_eq!(permissions.of("comment", ToolKind::Other), ToolPermission::Ask);
        permissions.groups.insert(ToolKind::Write, ToolPermission::Allow);
        permissions.tools.insert("delete".into(), ToolPermission::Block);
        assert_eq!(permissions.of("create", ToolKind::Write), ToolPermission::Allow);
        assert_eq!(permissions.of("delete", ToolKind::Write), ToolPermission::Block);
        assert_eq!(permissions.of("list", ToolKind::Read), ToolPermission::Allow);
    }

    #[test]
    fn a_failed_result_says_its_text() {
        let failed = json!({"isError": true, "content": [{"type": "text", "text": "No such issue"}]});
        assert_eq!(result_error(&failed).as_deref(), Some("No such issue"));
        assert_eq!(result_error(&json!({"content": []})), None);
    }
}
