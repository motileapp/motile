//! A small MCP client over streamable HTTP, for what the gateway asks an MCP server itself: its tools,
//! and a tool the user tries from Settings.

use std::time::Duration;

use anyhow::{Context, anyhow};
use futures_util::StreamExt;
use serde_json::{Value, json};

pub const PROTOCOL_VERSION: &str = "2025-06-18";
const ANSWER_WITHIN: Duration = Duration::from_secs(60);
const MAX_ANSWER: usize = 16 * 1024 * 1024;
const SESSION_HEADER: &str = "mcp-session-id";

pub enum Failure {
    /// The MCP server wants a login, or a better one; what it said in `WWW-Authenticate`.
    Unauthorized(Option<String>),
    Failed(anyhow::Error),
}

impl From<anyhow::Error> for Failure {
    fn from(error: anyhow::Error) -> Self {
        Self::Failed(error)
    }
}

impl From<reqwest::Error> for Failure {
    fn from(error: reqwest::Error) -> Self {
        Self::Failed(error.into())
    }
}

pub struct Session<'a> {
    http: &'a reqwest::Client,
    url: &'a str,
    headers: Vec<(String, String)>,
    session: Option<String>,
    next_id: u64,
    pub server_info: Value,
}

impl<'a> Session<'a> {
    /// Opens a session: `initialize`, then `notifications/initialized`.
    pub async fn open(
        http: &'a reqwest::Client,
        url: &'a str,
        headers: Vec<(String, String)>,
    ) -> Result<Session<'a>, Failure> {
        let mut session = Self { http, url, headers, session: None, next_id: 1, server_info: Value::Null };
        let params = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "motile-gateway", "version": env!("CARGO_PKG_VERSION")},
        });
        let initialized = session.request("initialize", params).await?;
        session.server_info = initialized["serverInfo"].clone();
        session.notify("notifications/initialized").await?;
        Ok(session)
    }

    /// Every tool, over as many pages as it takes.
    pub async fn tools(&mut self) -> Result<Vec<Value>, Failure> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = cursor.as_ref().map(|cursor| json!({"cursor": cursor})).unwrap_or_else(|| json!({}));
            let page = self.request("tools/list", params).await?;
            tools.extend(page["tools"].as_array().cloned().unwrap_or_default());
            match page["nextCursor"].as_str() {
                Some(next) if !next.is_empty() && cursor.as_deref() != Some(next) => cursor = Some(next.to_string()),
                _ => return Ok(tools),
            }
        }
    }

    pub async fn call(&mut self, tool: &str, arguments: Value) -> Result<Value, Failure> {
        self.request("tools/call", json!({"name": tool, "arguments": arguments})).await
    }

    /// Ends the session, as a client should; what it answers doesn't matter.
    pub async fn close(self) {
        let Some(session) = &self.session else { return };
        let request = self.http.delete(self.url).header(SESSION_HEADER, session).timeout(Duration::from_secs(5));
        let _ = self.with_headers(request).send().await;
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value, Failure> {
        let id = self.next_id;
        self.next_id += 1;
        let response = self.post(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).await?;
        if let Some(session) = response.headers().get(SESSION_HEADER).and_then(|value| value.to_str().ok()) {
            self.session = Some(session.to_string());
        }
        let answer = answer_of(response, id).await?;
        if let Some(error) = answer.get("error") {
            let message = error["message"].as_str().unwrap_or("The MCP server refused it.");
            return Err(Failure::Failed(anyhow!("{message}")));
        }
        Ok(answer["result"].clone())
    }

    async fn notify(&mut self, method: &str) -> Result<(), Failure> {
        self.post(json!({"jsonrpc": "2.0", "method": method})).await?;
        Ok(())
    }

    async fn post(&self, message: Value) -> Result<reqwest::Response, Failure> {
        let mut request = self
            .http
            .post(self.url)
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .timeout(ANSWER_WITHIN)
            .body(message.to_string());
        if let Some(session) = &self.session {
            request = request.header(SESSION_HEADER, session).header("mcp-protocol-version", PROTOCOL_VERSION);
        }
        let response = self.with_headers(request).send().await.context("The MCP server couldn't be reached.")?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            let challenge = response.headers().get("www-authenticate").and_then(|value| value.to_str().ok());
            return Err(Failure::Unauthorized(challenge.map(String::from)));
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let said = body.trim().chars().take(300).collect::<String>();
            return Err(Failure::Failed(anyhow!("The MCP server answered {status}. {said}")));
        }
        Ok(response)
    }

    fn with_headers(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        self.headers.iter().fold(request, |request, (name, value)| request.header(name, value))
    }
}

/// The answer to request `id`, whether it came as JSON or in an event stream.
async fn answer_of(response: reqwest::Response, id: u64) -> Result<Value, Failure> {
    let streams = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|kind| kind.contains("event-stream"));
    if !streams {
        let body = response.bytes().await?;
        let answer: Value = serde_json::from_slice(&body).context("The MCP server's answer isn't JSON.")?;
        return Ok(answer);
    }
    let mut events = Events::default();
    let mut stream = response.bytes_stream();
    let mut read = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        read += chunk.len();
        if read > MAX_ANSWER {
            break;
        }
        for data in events.feed(&chunk) {
            let Ok(message) = serde_json::from_str::<Value>(&data) else { continue };
            if message["id"].as_u64() == Some(id) && (message.get("result").is_some() || message.get("error").is_some())
            {
                return Ok(message);
            }
        }
    }
    Err(Failure::Failed(anyhow!("The MCP server ended its answer without one.")))
}

/// The `data` of each server-sent event, as the chunks of a stream complete them.
#[derive(Default)]
pub struct Events {
    pending: Vec<u8>,
    data: Vec<String>,
}

impl Events {
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(chunk);
        let mut done = Vec::new();
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if !self.data.is_empty() {
                    done.push(std::mem::take(&mut self.data).join("\n"));
                }
                continue;
            }
            if let Some(data) = line.strip_prefix("data:") {
                self.data.push(data.strip_prefix(' ').unwrap_or(data).to_string());
            }
        }
        done
    }

    /// What is left once the stream ended.
    pub fn finish(&mut self) -> Vec<String> {
        let mut done = self.feed(b"\n\n");
        if !self.data.is_empty() {
            done.push(std::mem::take(&mut self.data).join("\n"));
        }
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_read_whole_across_chunks() {
        let mut events = Events::default();
        assert!(events.feed(b"event: message\ndata: {\"id\"").is_empty());
        assert_eq!(events.feed(b": 1}\n\ndata: a\r\ndata: b\r\n\r\n"), ["{\"id\": 1}", "a\nb"]);
        assert!(events.feed(b"data: tail").is_empty());
        assert_eq!(events.finish(), ["tail"]);
    }
}
