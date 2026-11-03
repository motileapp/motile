//! An agent's MCP request, passed on to its MCP server with the MCP server's login in place of the
//! agent's token, and the answer passed back as it streams. The gateway reads only what it must: a
//! call to a tool that is off is answered here, a `tools/list` loses the tools that are off, and
//! each `tools/call` is logged with its result.

use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use futures_util::StreamExt;
use iroh::endpoint::{RecvStream, SendStream};
use motile_protocol::frame::write_frame;
use motile_protocol::gateway_wire::{GatewayReply, McpStatus};
use motile_protocol::now;
use serde_json::{Value, json};

use super::client::Events;
use super::{Login, McpEntry, cut, result_error};
use crate::gateway::Gateway;
use crate::store::CallRecord;
use crate::upstream::{NOT_PASSED, header_pairs, passed_back};

const MAX_REQUEST: usize = 16 * 1024 * 1024;
/// How much of a `tools/list` answer is read to filter it.
const MAX_LIST: usize = 16 * 1024 * 1024;

pub struct McpIncoming {
    pub server: String,
    pub thread: String,
    pub mcp_server: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
}

/// What the agent's message asks, as much as the gateway reads of it.
struct Asked {
    method: Option<String>,
    id: Option<Value>,
    tool: Option<String>,
    arguments: Value,
}

impl Asked {
    fn of(body: &[u8]) -> Self {
        let message: Value = serde_json::from_slice(body).unwrap_or_default();
        Self {
            method: message["method"].as_str().map(String::from),
            id: message.get("id").cloned(),
            tool: message["params"]["name"].as_str().map(String::from),
            arguments: message["params"]["arguments"].clone(),
        }
    }

    fn is(&self, method: &str) -> bool {
        self.method.as_deref() == Some(method)
    }
}

pub async fn serve(
    gateway: Arc<Gateway>,
    incoming: McpIncoming,
    mut send: SendStream,
    mut recv: RecvStream,
) -> anyhow::Result<()> {
    let body = recv.read_to_end(MAX_REQUEST).await.context("The request's body didn't arrive whole.")?;
    let asked = Asked::of(&body);
    let Some(mcp_server) = gateway.mcp_servers.by_slug(&incoming.mcp_server).filter(|mcp_server| mcp_server.enabled)
    else {
        let message = format!("Your gateway has no MCP server {} that is on.", incoming.mcp_server);
        return answer(&mut send, 404, &asked, &message).await;
    };
    let started = Instant::now();
    let calling = asked.is("tools/call").then(|| asked.tool.clone()).flatten();
    if let Some(tool) = &calling
        && !mcp_server.tool_on(tool)
    {
        let error = format!("{tool} is turned off for agents in Motile's settings.");
        log(&gateway, &incoming, &mcp_server, tool, &asked, started, Some(error.clone()), Value::Null);
        return answer(&mut send, 200, &asked, &error).await;
    }

    let mut renewed = false;
    let response = loop {
        let Some(mcp_server) = gateway.fresh_mcp_server(&mcp_server.id).await else {
            let error = sign_in_again(&mcp_server);
            return answer(&mut send, 502, &asked, &error).await;
        };
        let request = gateway
            .http
            .request(reqwest::Method::from_bytes(incoming.method.as_bytes())?, &mcp_server.url)
            .body(body.clone());
        let headers = incoming
            .headers
            .iter()
            .filter(|(name, _)| !NOT_PASSED.contains(&name.to_ascii_lowercase().as_str()))
            .cloned()
            .chain(mcp_server.login_headers());
        let request = headers.fold(request, |request, (name, value)| request.header(name, value));
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                let error = format!("{} couldn't be reached: {error}", mcp_server.name);
                gateway.mcp_server_status(&mcp_server.id, McpStatus::Failing { error: error.clone() });
                if let Some(tool) = &calling {
                    log(&gateway, &incoming, &mcp_server, tool, &asked, started, Some(error.clone()), Value::Null);
                }
                return answer(&mut send, 502, &asked, &error).await;
            }
        };
        let refused = matches!(response.status().as_u16(), 401 | 403);
        if refused
            && let Login::OAuth(login) = &mcp_server.login
            && !renewed
        {
            renewed = true;
            if gateway.renew_mcp_server(&mcp_server.id, &login.access).await.is_some() {
                continue;
            }
        }
        if refused {
            let error = match mcp_server.login {
                Login::OAuth(_) => {
                    gateway.mcp_server_status(&mcp_server.id, McpStatus::SignInAgain);
                    sign_in_again(&mcp_server)
                }
                _ => {
                    let error = format!("{} refused its key. Give it a new one in Motile's settings.", mcp_server.name);
                    gateway.mcp_server_status(&mcp_server.id, McpStatus::Failing { error: error.clone() });
                    error
                }
            };
            if let Some(tool) = &calling {
                log(&gateway, &incoming, &mcp_server, tool, &asked, started, Some(error.clone()), Value::Null);
            }
            return answer(&mut send, 502, &asked, &error).await;
        }
        break response;
    };

    let status = response.status().as_u16();
    let headers = header_pairs(response.headers());
    let streams =
        headers.iter().any(|(name, value)| name.eq_ignore_ascii_case("content-type") && value.contains("event-stream"));
    let passed: Vec<(String, String)> =
        passed_back(&headers).into_iter().filter(|(name, _)| !name.eq_ignore_ascii_case("content-length")).collect();

    if asked.is("tools/list") && status < 300 {
        let listed = read_all(response, MAX_LIST).await;
        let filtered = filtered_list(&gateway, &mcp_server, &listed, streams);
        write_frame(&mut send, &GatewayReply::HttpHead { status, headers: passed, account: None }).await?;
        send.write_all(&filtered).await?;
        send.finish()?;
        return Ok(());
    }

    write_frame(&mut send, &GatewayReply::HttpHead { status, headers: passed, account: None }).await?;
    let mut kept = Vec::new();
    let mut failed = None;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => {
                failed = Some(format!("The answer broke off: {error}"));
                break;
            }
        };
        if calling.is_some() {
            let room = MAX_LIST.saturating_sub(kept.len());
            kept.extend_from_slice(&chunk[..room.min(chunk.len())]);
        }
        if send.write_all(&chunk).await.is_err() {
            break;
        }
    }
    let _ = send.finish();
    if status < 300 && mcp_server.status != McpStatus::Ready {
        gateway.mcp_server_status(&mcp_server.id, McpStatus::Ready);
    }
    let Some(tool) = calling else { return Ok(()) };
    let message = answer_in(&kept, streams, asked.id.as_ref());
    let result = message.as_ref().map(|message| message["result"].clone()).unwrap_or(Value::Null);
    let error = failed
        .or_else(|| message.as_ref().and_then(|message| message["error"]["message"].as_str().map(String::from)))
        .or_else(|| result_error(&result))
        .or_else(|| (status >= 300).then(|| format!("{} answered {status}.", mcp_server.name)));
    log(&gateway, &incoming, &mcp_server, &tool, &asked, started, error, result);
    Ok(())
}

/// Answers the agent from the gateway: a JSON-RPC error to a request, `status` with the text to
/// anything else.
async fn answer(send: &mut SendStream, status: u16, asked: &Asked, message: &str) -> anyhow::Result<()> {
    let json_header = vec![("content-type".to_string(), "application/json".to_string())];
    let (status, body) = match (&asked.id, asked.is("tools/call")) {
        (Some(id), true) => (
            200,
            json!({"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": message}], "isError": true}}),
        ),
        (Some(id), false) => (200, json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": message}})),
        (None, _) => (status, json!({"error": message})),
    };
    write_frame(send, &GatewayReply::HttpHead { status, headers: json_header, account: None }).await?;
    send.write_all(body.to_string().as_bytes()).await?;
    send.finish()?;
    Ok(())
}

fn sign_in_again(mcp_server: &McpEntry) -> String {
    format!("{}'s login stopped working. Sign in again in Motile's settings, under McpServers.", mcp_server.name)
}

#[allow(clippy::too_many_arguments)]
fn log(
    gateway: &Gateway,
    incoming: &McpIncoming,
    mcp_server: &McpEntry,
    tool: &str,
    asked: &Asked,
    started: Instant,
    error: Option<String>,
    result: Value,
) {
    gateway.log_call(CallRecord {
        at: now(),
        server: Some(incoming.server.clone()),
        thread: Some(incoming.thread.clone()),
        mcp_server: mcp_server.id.clone(),
        tool: tool.to_string(),
        latency_ms: started.elapsed().as_millis() as u64,
        error,
        arguments: cut(&asked.arguments.to_string()),
        result: cut(&result.to_string()),
    });
}

async fn read_all(response: reqwest::Response, limit: usize) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(Ok(chunk)) = stream.next().await {
        kept.extend_from_slice(&chunk);
        if kept.len() > limit {
            break;
        }
    }
    kept
}

/// The answer with the tools that are off left out, as JSON or as the events it came in.
fn filtered_list(gateway: &Gateway, mcp_server: &McpEntry, body: &[u8], streams: bool) -> Vec<u8> {
    let filter = |message: &mut Value| {
        let first_page = message["result"]["nextCursor"].as_str().is_none_or(str::is_empty);
        let Some(tools) = message["result"]["tools"].as_array_mut() else { return };
        if first_page {
            gateway.saw_tools(&mcp_server.id, tools);
        }
        tools.retain(|tool| tool["name"].as_str().is_none_or(|name| mcp_server.tool_on(name)));
    };
    if !streams {
        let Ok(mut message) = serde_json::from_slice::<Value>(body) else { return body.to_vec() };
        filter(&mut message);
        return message.to_string().into_bytes();
    }
    let mut events = Events::default();
    let mut data = events.feed(body);
    data.extend(events.finish());
    let mut out = String::new();
    for data in data {
        let data = match serde_json::from_str::<Value>(&data) {
            Ok(mut message) => {
                filter(&mut message);
                message.to_string()
            }
            Err(_) => data,
        };
        out.push_str("event: message\n");
        for line in data.lines() {
            out.push_str("data: ");
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }
    out.into_bytes()
}

/// The answer to request `id` in what came back.
fn answer_in(body: &[u8], streams: bool, id: Option<&Value>) -> Option<Value> {
    if !streams {
        return serde_json::from_slice(body).ok();
    }
    let mut events = Events::default();
    let mut data = events.feed(body);
    data.extend(events.finish());
    data.iter()
        .filter_map(|data| serde_json::from_str::<Value>(data).ok())
        .find(|message| message.get("id") == id && (message.get("result").is_some() || message.get("error").is_some()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_to_a_call_is_found_among_the_events() {
        let body = b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\n\
                     event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{\"content\":[]}}\n\n";
        let found = answer_in(body, true, Some(&json!(7))).unwrap();
        assert_eq!(found["result"], json!({"content": []}));
        assert_eq!(answer_in(body, true, Some(&json!(8))), None);
        assert_eq!(answer_in(br#"{"id":1,"result":{}}"#, false, Some(&json!(1))).unwrap()["result"], json!({}));
    }
}
