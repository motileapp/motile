//! The gateway's MCP servers as Settings → MCP Gateway shows them: the calls agents made with them, the
//! call opened from them, the tools folded out, and what the page and its sheets ask the gateway.

use std::collections::HashSet;
use std::sync::Arc;

use gpui_kit::App;
use motile_core::api::Command;
use motile_protocol::gateway_wire::McpChange;
use serde::Deserialize;
use serde_json::Value;

use super::Store;
use crate::panel::state::{CodeDocument, CodeFile, Loaded};
use crate::sign_in::Session;

/// Where an MCP server's sign-in in the browser ends.
pub const MCP_REDIRECT: &str = "motile://mcp";

/// A call of the activity: `motile_core::mcp_servers::CallRow`, read back.
#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct CallRow {
    pub id: i64,
    pub time: String,
    pub mcp_server: String,
    pub monogram: String,
    pub tint: String,
    pub tool: String,
    pub source: String,
    pub latency: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct CallPage {
    rows: Vec<CallRow>,
    more: bool,
}

/// A call in full, its arguments and its result each as a document.
pub struct Call {
    pub row: CallRow,
    pub arguments: Arc<CodeDocument>,
    pub result: Arc<CodeDocument>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct CallView {
    row: CallRow,
    arguments: String,
    result: String,
    error: Option<String>,
}

/// A field of the form a tool is tried with: `motile_core::mcp_servers::Field`, read back.
#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct Field {
    pub name: String,
    pub description: Option<String>,
    pub kind: FieldKind,
    pub required: bool,
    pub choices: Vec<String>,
    pub value: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Number,
    Switch,
    Choice,
    Json,
}

/// What a tool answered, as its text and as JSON.
pub struct Answer {
    pub text: Arc<CodeDocument>,
    pub json: Arc<CodeDocument>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct Answered {
    text: String,
    json: String,
    error: Option<String>,
}

fn document(id: String, text: &str) -> Arc<CodeDocument> {
    let file = CodeFile::whole(id.clone(), text.lines().map(str::to_string).collect());
    Arc::new(CodeDocument::new(id, vec![file], false, false))
}

fn read_call(value: Value, call: i64) -> Result<Option<Call>, String> {
    let Some(view) = value.get("call").filter(|view| !view.is_null()) else { return Ok(None) };
    let view: CallView = serde_json::from_value(view.clone()).map_err(|error| error.to_string())?;
    let result = view.error.as_deref().unwrap_or(&view.result);
    Ok(Some(Call {
        arguments: document(format!("mcp-server-call-{call}-arguments"), &view.arguments),
        result: document(format!("mcp-server-call-{call}-result"), result),
        error: view.error,
        row: view.row,
    }))
}

fn read_answer(value: Value, run: u64) -> Result<Answer, String> {
    let answered: Answered = serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(Answer {
        text: document(format!("mcp-server-try-{run}-text"), &answered.text),
        json: document(format!("mcp-server-try-{run}-json"), &answered.json),
        error: answered.error,
    })
}

/// The pages of the activity read so far, newest first.
#[derive(Default)]
pub struct Calls {
    pub rows: Vec<CallRow>,
    pub more: bool,
    pub errors_only: bool,
    pub loading: bool,
    pub error: Option<String>,
    /// Counts what was asked, so that only the last answer is kept.
    asked: u64,
}

#[derive(Default)]
pub struct McpServers {
    pub calls: Calls,
    /// The call opened from the activity.
    pub call: Option<(i64, Loaded<Option<Call>>)>,
    /// The MCP servers whose tools are folded out, while the page shows.
    pub unfolded: HashSet<String>,
    /// The MCP server whose sign-in is starting.
    pub starting_sign_in: Option<String>,
    session: Option<Session>,
    page_open: bool,
}

impl Store {
    /// Reads the activity when the page opens, and lets go of what it showed when it closes.
    pub(super) fn follow_mcp_servers_page(&mut self, open: bool) {
        if open == self.mcp_servers.page_open {
            return;
        }
        self.mcp_servers.page_open = open;
        if open {
            self.load_mcp_server_calls(true);
            return;
        }
        self.mcp_servers.call = None;
        self.mcp_servers.unfolded.clear();
    }

    /// Puts the calls that came since the newest row shown on top, or starts over from the newest
    /// page when more came than a page holds.
    pub(super) fn refresh_mcp_server_calls(&mut self) {
        if !self.mcp_servers.page_open || self.mcp_servers.calls.loading {
            return;
        }
        let calls = &self.mcp_servers.calls;
        let (asked, errors_only) = (calls.asked, calls.errors_only);
        self.ask(Command::McpServerCalls { before: None, errors_only }, move |store, result, _| {
            let calls = &mut store.mcp_servers.calls;
            let page = result.ok().and_then(|value| serde_json::from_value::<CallPage>(value).ok());
            let Some(page) = page.filter(|_| calls.asked == asked) else { return };
            let newest = calls.rows.first().map_or(0, |row| row.id);
            let fresh: Vec<_> = page.rows.iter().filter(|row| row.id > newest).cloned().collect();
            if fresh.len() == page.rows.len() && !calls.rows.is_empty() {
                calls.rows = page.rows;
                calls.more = page.more;
                return;
            }
            calls.rows.splice(0..0, fresh);
        });
    }

    /// Reads the activity's first page again, or the page after the rows read.
    pub fn load_mcp_server_calls(&mut self, first: bool) {
        let calls = &mut self.mcp_servers.calls;
        if !first && (calls.loading || !calls.more) {
            return;
        }
        calls.asked += 1;
        calls.loading = true;
        let (asked, errors_only) = (calls.asked, calls.errors_only);
        let before = if first { None } else { calls.rows.last().map(|row| row.id) };
        self.ask(Command::McpServerCalls { before, errors_only }, move |store, result, _| {
            let calls = &mut store.mcp_servers.calls;
            if calls.asked != asked {
                return;
            }
            calls.loading = false;
            let page = result
                .map_err(|error| error.message)
                .and_then(|value| serde_json::from_value::<CallPage>(value).map_err(|error| error.to_string()));
            match page {
                Ok(page) => {
                    if first {
                        calls.rows.clear();
                    }
                    calls.rows.extend(page.rows);
                    calls.more = page.more;
                    calls.error = None;
                }
                Err(error) => calls.error = Some(error),
            }
        });
    }

    pub fn show_mcp_server_errors_only(&mut self, errors_only: bool) {
        let calls = &mut self.mcp_servers.calls;
        if calls.errors_only == errors_only {
            return;
        }
        calls.errors_only = errors_only;
        calls.rows.clear();
        calls.more = false;
        self.load_mcp_server_calls(true);
    }

    /// Opens a call with its arguments and result, laid out off the main thread.
    pub fn open_mcp_server_call(&mut self, call: i64) {
        self.mcp_servers.call = Some((call, Loaded::Loading));
        let read = move |value: Value| read_call(value, call);
        self.ask_read(Command::McpServerCall { call }, read, move |store, result, _| {
            let Some((shown, loaded)) = &mut store.mcp_servers.call else { return };
            if *shown != call {
                return;
            }
            *loaded = match result.map_err(|error| error.message).and_then(|read| read) {
                Ok(read) => Loaded::Ready(read),
                Err(error) => Loaded::Failed(error),
            };
        });
    }

    pub fn close_mcp_server_call(&mut self) {
        self.mcp_servers.call = None;
    }

    pub fn toggle_mcp_server_tools(&mut self, mcp_server: &str) {
        let unfolded = &mut self.mcp_servers.unfolded;
        if !unfolded.remove(mcp_server) {
            unfolded.insert(mcp_server.to_string());
        }
    }

    pub fn update_mcp_server(&mut self, mcp_server: String, change: McpChange) {
        self.gateway_command(Command::McpServerUpdate { mcp_server, change });
    }

    pub fn refresh_mcp_server(&mut self, mcp_server: String) {
        self.gateway_command(Command::McpServerRefresh { mcp_server });
    }

    pub fn remove_mcp_server(&mut self, mcp_server: String) {
        self.mcp_servers.unfolded.remove(&mcp_server);
        self.gateway_command(Command::McpServerRemove { mcp_server });
    }

    /// Signs an MCP server whose login stopped working in again, in the browser.
    pub fn sign_mcp_server_in_again(&mut self, mcp_server: String) {
        if self.mcp_servers.starting_sign_in.is_some() {
            return;
        }
        self.mcp_servers.starting_sign_in = Some(mcp_server.clone());
        self.ask(Command::McpServerSignIn { mcp_server }, |store, result, cx| {
            store.mcp_servers.starting_sign_in = None;
            let answer = match result {
                Ok(answer) => answer,
                Err(error) => return store.fail(error),
            };
            let Some(url) = answer["sign_in"]["url"].as_str() else { return };
            let Some((session, sent_back)) = crate::sign_in::start(url, MCP_REDIRECT, cx) else {
                cx.open_url(url);
                return;
            };
            store.mcp_servers.session = Some(session);
            cx.spawn(async move |this, cx| {
                let address = sent_back.await.ok().flatten();
                let _ = this.update(cx, |store, cx| {
                    store.mcp_servers.session = None;
                    if let Some(address) = address {
                        store.gateway_command(Command::McpServerFinishSignIn { address });
                    }
                    cx.notify();
                });
            })
            .detach();
        });
    }

    /// Sends one of the sheets' commands; `done` hears the answer or what went wrong.
    pub fn ask_mcp_servers(&mut self, command: Command, done: impl FnOnce(Result<Value, String>, &mut App) + 'static) {
        self.ask(command, move |_, result, cx| done(result.map_err(|error| error.message), cx));
    }

    /// Calls a tool with what its form holds; `done` hears its answer, laid out off the main thread.
    pub fn try_mcp_server_tool(
        &mut self,
        command: Command,
        run: u64,
        done: impl FnOnce(Result<Answer, String>, &mut App) + 'static,
    ) {
        let read = move |value: Value| read_answer(value, run);
        self.ask_read(command, read, move |_, result, cx| {
            done(result.map_err(|error| error.message).and_then(|read| read), cx)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_call_shows_its_error_where_its_result_would_be() {
        let row = serde_json::json!({"id": 3, "at": 0.0, "time": "14:32", "mcp_server_id": "c1",
            "mcp_server": "Linear", "monogram": "L", "tint": "chart-1", "tool": "list_issues",
            "server_id": null, "thread_id": null, "source": "Tried in Settings", "latency": "40 ms",
            "ok": false, "error": "Refused"});
        let value = serde_json::json!({"call": {"row": row, "arguments": "{\n  \"a\": 1\n}", "result": "",
            "error": "Refused\nmore"}});
        let call = read_call(value, 3).unwrap().unwrap();
        let lines = |document: &CodeDocument| -> Vec<String> {
            document.files[0].lines.iter().map(|line| line.to_string()).collect()
        };
        assert_eq!(lines(&call.arguments), vec!["{", "  \"a\": 1", "}"]);
        assert_eq!(lines(&call.result), vec!["Refused", "more"]);
        assert_eq!(call.row.tool, "list_issues");
        assert!(read_call(serde_json::json!({"call": null}), 3).unwrap().is_none());
    }
}
