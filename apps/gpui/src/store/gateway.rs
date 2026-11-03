//! The account's gateway as Settings → Agent Gateway, Settings → MCP Gateway and the Limits tab show
//! it: what the core last said of it, the gateway being added, the sign-in of an account, a page of its
//! request log and the request opened from it.

use std::sync::Arc;

use motile_core::api::Command;
use motile_core::gateway::View;
use motile_protocol::gateway_wire::{AccountChange, Capture};
use motile_protocol::wire::Agent;
use serde::Deserialize;
use serde_json::Value;

use super::Store;
use super::settings::SettingsSection;
use super::usage::UsageTab;
use crate::panel::state::{CodeDocument, CodeFile, Loaded};

/// How a gateway is added: `motile_core::gateway::Enroll`, read back.
#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct Enroll {
    pub code: String,
    pub expires_at: f64,
    pub command: String,
    #[serde(default)]
    pub spans: Vec<u32>,
    pub steps: Vec<String>,
}

/// A request of the gateway's log: `motile_core::gateway::LogRow`, read back.
#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct LogRow {
    pub id: i64,
    pub time: String,
    pub server: String,
    pub thread: String,
    pub account: Option<String>,
    pub model: Option<String>,
    pub path: String,
    pub status: u16,
    pub ok: bool,
    pub latency: String,
    pub tokens: String,
    pub retries: Option<String>,
    pub size: String,
    pub error: Option<String>,
    pub captured: bool,
}

#[derive(Deserialize)]
struct LogPage {
    rows: Vec<LogRow>,
    more: bool,
}

/// A captured request in full, its request and its response each as a document of its headers
/// and its body.
pub struct Captured {
    pub row: LogRow,
    pub request: Arc<CodeDocument>,
    pub response: Arc<CodeDocument>,
}

#[derive(Deserialize)]
struct CapturedView {
    row: LogRow,
    request_headers: Vec<(String, String)>,
    request_body: String,
    response_headers: Vec<(String, String)>,
    response_body: String,
}

/// Lays a captured request out as two documents, which takes a while for a long conversation.
fn read_captured(value: Value, entry: i64) -> Result<Option<Captured>, String> {
    let Some(request) = value.get("request").filter(|request| !request.is_null()) else { return Ok(None) };
    let view: CapturedView = serde_json::from_value(request.clone()).map_err(|error| error.to_string())?;
    let document = |part: &str, headers: &[(String, String)], body: &str| {
        let mut lines: Vec<String> = headers.iter().map(|(name, value)| format!("{name}: {value}")).collect();
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.extend(body.lines().map(str::to_string));
        let file = CodeFile::whole(part.to_string(), lines);
        Arc::new(CodeDocument::new(format!("gateway-captured-{entry}-{part}"), vec![file], false, false))
    };
    Ok(Some(Captured {
        request: document("request", &view.request_headers, &view.request_body),
        response: document("response", &view.response_headers, &view.response_body),
        row: view.row,
    }))
}

/// The pages of the request log read so far, newest first.
#[derive(Default)]
pub struct Log {
    pub rows: Vec<LogRow>,
    pub more: bool,
    pub errors_only: bool,
    pub loading: bool,
    pub error: Option<String>,
    /// Counts what was asked, so that only the last answer is kept.
    asked: u64,
}

/// The gateway being added: the code it joins with, and the gateway it replaces.
pub struct Adding {
    pub enroll: Loaded<Enroll>,
    replaces: Option<String>,
}

/// The sheet that adds an account, from the moment it is asked for until the account is added.
#[derive(Clone, PartialEq, Debug)]
pub struct SigningIn {
    pub agent: Agent,
    /// Why starting it or finishing it failed, when the core didn't say so in the view.
    pub error: Option<String>,
    /// The view has shown its sign-in, whose end means the account was added.
    started: bool,
}

#[derive(Default)]
pub struct Gateway {
    /// What the core last said; None until it has.
    pub view: Option<View>,
    pub adding: Option<Adding>,
    pub signing_in: Option<SigningIn>,
    pub log: Log,
    /// The captured request opened from the log, by its entry.
    pub captured: Option<(i64, Loaded<Option<Captured>>)>,
    /// What the core was last told about whether a view shows the gateway.
    watching: bool,
    page_open: bool,
}

impl Gateway {
    pub fn exists(&self) -> bool {
        self.view.as_ref().is_some_and(|view| view.exists)
    }
}

impl Store {
    pub(super) fn apply_gateway(&mut self, view: View) {
        if let Some(adding) = &self.gateway.adding
            && view.exists
            && view.key != adding.replaces
        {
            self.close_add_gateway();
        }
        if let Some(signing_in) = &mut self.gateway.signing_in {
            match &view.sign_in {
                Some(_) => signing_in.started = true,
                None if signing_in.started => self.gateway.signing_in = None,
                None => {}
            }
        }
        let came = self.gateway.view.as_ref().is_some_and(|shown| shown.log_head != view.log_head);
        let called = self.gateway.view.as_ref().is_some_and(|shown| shown.calls_head != view.calls_head);
        self.gateway.view = Some(view);
        if came && self.gateway.page_open {
            self.refresh_gateway_log();
        }
        if called {
            self.refresh_mcp_server_calls();
        }
    }

    /// Puts the requests that came since the newest row shown on top, or starts over from the
    /// newest page when more came than a page holds.
    fn refresh_gateway_log(&mut self) {
        if self.gateway.log.loading {
            return;
        }
        let (asked, errors_only) = (self.gateway.log.asked, self.gateway.log.errors_only);
        self.ask(Command::GatewayLog { before: None, errors_only }, move |store, result, _| {
            let log = &mut store.gateway.log;
            let page = result.ok().and_then(|value| serde_json::from_value::<LogPage>(value).ok());
            let Some(page) = page.filter(|_| log.asked == asked) else { return };
            let newest = log.rows.first().map_or(0, |row| row.id);
            let fresh: Vec<_> = page.rows.iter().filter(|row| row.id > newest).cloned().collect();
            if fresh.len() == page.rows.len() && !log.rows.is_empty() {
                log.rows = page.rows;
                log.more = page.more;
                return;
            }
            log.rows.splice(0..0, fresh);
        });
    }

    /// Tells the core whether the Agent Gateway page, the MCP Gateway page or the Limits tab shows,
    /// and reads the log when the Agent Gateway page opens.
    pub(super) fn follow_gateway_views(&mut self) {
        let page_open = self.settings.section == Some(SettingsSection::AgentGateway);
        let mcp_servers_open = self.settings.section == Some(SettingsSection::McpGateway);
        let limits_open = self.shows_usage && self.usage_tab() == UsageTab::Limits;
        let watching = page_open || mcp_servers_open || limits_open;
        if watching != self.gateway.watching {
            self.gateway.watching = watching;
            self.send(Command::WatchGateway { on: watching });
        }
        if !page_open && !mcp_servers_open && self.gateway.adding.is_some() {
            self.close_add_gateway();
        }
        self.follow_mcp_servers_page(mcp_servers_open);
        if page_open == self.gateway.page_open {
            return;
        }
        self.gateway.page_open = page_open;
        if page_open {
            self.load_gateway_log(true);
            return;
        }
        self.gateway.captured = None;
    }

    /// Asks for the code a new gateway joins with, and watches the account for it to join.
    pub fn open_add_gateway(&mut self) {
        let replaces = self.gateway.view.as_ref().filter(|view| view.exists).and_then(|view| view.key.clone());
        self.gateway.adding = Some(Adding { enroll: Loaded::Loading, replaces });
        self.send(Command::WatchServers { on: true });
        self.ask(Command::AddGateway, |store, result, _| {
            let Some(adding) = &mut store.gateway.adding else { return };
            adding.enroll = match result {
                Ok(value) => {
                    serde_json::from_value(value).map_or_else(|error| Loaded::Failed(error.to_string()), Loaded::Ready)
                }
                Err(error) => Loaded::Failed(error.message),
            };
        });
    }

    pub fn close_add_gateway(&mut self) {
        if self.gateway.adding.take().is_some() {
            self.send(Command::WatchServers { on: false });
        }
    }

    pub fn remove_gateway(&mut self) {
        self.ask(Command::RemoveGateway, |store, result, _| {
            if let Err(error) = result {
                store.fail(error);
            }
        });
    }

    /// Starts signing an account of the agent in. Its sheet shows the page to open.
    pub fn start_gateway_sign_in(&mut self, agent: Agent) {
        self.gateway.signing_in = Some(SigningIn { agent, error: None, started: false });
        self.ask(Command::GatewaySignIn { agent }, move |store, result, _| {
            let Some(signing_in) = &mut store.gateway.signing_in else { return };
            if signing_in.agent != agent {
                return;
            }
            if let Err(error) = result {
                signing_in.error = Some(error.message);
            }
        });
    }

    /// Hands the gateway the code Claude's page showed.
    pub fn finish_gateway_sign_in(&mut self, code: String) {
        if let Some(signing_in) = &mut self.gateway.signing_in {
            signing_in.error = None;
        }
        self.ask(Command::GatewayFinishSignIn { code }, |store, result, _| {
            let Err(error) = result else { return };
            let told = store.gateway_sign_in().is_some_and(|sign_in| sign_in.error.is_some());
            if let Some(signing_in) = store.gateway.signing_in.as_mut().filter(|_| !told) {
                signing_in.error = Some(error.message);
            }
        });
    }

    pub fn cancel_gateway_sign_in(&mut self) {
        self.gateway.signing_in = None;
        self.send(Command::GatewayCancelSignIn);
    }

    /// The sign-in under way, as the core says.
    pub fn gateway_sign_in(&self) -> Option<&motile_core::gateway::SignInView> {
        self.gateway.view.as_ref()?.sign_in.as_ref()
    }

    pub fn update_gateway_account(&mut self, account: String, change: AccountChange) {
        self.gateway_command(Command::GatewayUpdateAccount { account, change });
    }

    pub fn remove_gateway_account(&mut self, account: String) {
        self.gateway_command(Command::GatewayRemoveAccount { account });
    }

    pub fn capture_gateway(&mut self, capture: Capture) {
        self.gateway_command(Command::GatewayCapture { capture });
    }

    pub(super) fn gateway_command(&mut self, command: Command) {
        self.ask(command, |store, result, _| {
            if let Err(error) = result {
                store.fail(error);
            }
        });
    }

    /// Reads the log's first page again, or the page after the rows read.
    pub fn load_gateway_log(&mut self, first: bool) {
        let log = &mut self.gateway.log;
        if !first && (log.loading || !log.more) {
            return;
        }
        log.asked += 1;
        log.loading = true;
        let (asked, errors_only) = (log.asked, log.errors_only);
        let before = if first { None } else { log.rows.last().map(|row| row.id) };
        self.ask(Command::GatewayLog { before, errors_only }, move |store, result, _| {
            let log = &mut store.gateway.log;
            if log.asked != asked {
                return;
            }
            log.loading = false;
            let page = result
                .map_err(|error| error.message)
                .and_then(|value| serde_json::from_value::<LogPage>(value).map_err(|error| error.to_string()));
            match page {
                Ok(page) => {
                    if first {
                        log.rows.clear();
                    }
                    log.rows.extend(page.rows);
                    log.more = page.more;
                    log.error = None;
                }
                Err(error) => log.error = Some(error),
            }
        });
    }

    pub fn show_gateway_errors_only(&mut self, errors_only: bool) {
        if self.gateway.log.errors_only == errors_only {
            return;
        }
        self.gateway.log.errors_only = errors_only;
        self.gateway.log.rows.clear();
        self.gateway.log.more = false;
        self.load_gateway_log(true);
    }

    /// Opens a captured request, laid out off the main thread.
    pub fn open_gateway_captured(&mut self, entry: i64) {
        self.gateway.captured = Some((entry, Loaded::Loading));
        let read = move |value: Value| read_captured(value, entry);
        self.ask_read(Command::GatewayCaptured { entry }, read, move |store, result, _| {
            let Some((shown, captured)) = &mut store.gateway.captured else { return };
            if *shown != entry {
                return;
            }
            *captured = match result.map_err(|error| error.message).and_then(|read| read) {
                Ok(read) => Loaded::Ready(read),
                Err(error) => Loaded::Failed(error),
            };
        });
    }

    pub fn close_gateway_captured(&mut self) {
        self.gateway.captured = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_captured_request_is_its_headers_then_its_body() {
        let value = serde_json::json!({"request": {
            "row": {"id": 7, "at": 0.0, "time": "14:32:05", "server_id": "k", "server": "studio", "thread_id": "t",
                "thread": "Fix the build", "account": "Work", "model": "claude", "path": "/v1/messages", "status": 200,
                "ok": true, "latency": "340 ms", "tokens": "No tokens", "retries": null, "size": "1 KB sent",
                "error": null, "captured": true},
            "request_headers": [["x-api-key", "Hidden"]],
            "request_body": "{\n  \"a\": 1\n}",
            "response_headers": [],
            "response_body": "ok"
        }});
        let captured = read_captured(value, 7).unwrap().unwrap();
        let lines: Vec<&str> = captured.request.files[0].lines.iter().map(|line| line.as_ref()).collect();
        assert_eq!(lines, vec!["x-api-key: Hidden", "", "{", "  \"a\": 1", "}"]);
        let lines: Vec<&str> = captured.response.files[0].lines.iter().map(|line| line.as_ref()).collect();
        assert_eq!(lines, vec!["ok"]);
        assert_eq!(captured.row.id, 7);
        assert!(read_captured(serde_json::json!({"request": null}), 7).unwrap().is_none());
    }
}
