//! A server's HTTP request, passed on to Anthropic or OpenAI with an account's token in place of the
//! agent's, and the response passed back as it streams. Nothing is translated. A limit error before
//! anything streamed rests the account and tries the next; once a response streams it is never
//! replayed.

use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use futures_util::StreamExt;
use iroh::endpoint::{RecvStream, SendStream};
use motile_protocol::frame::write_frame;
use motile_protocol::gateway_wire::{AccountStatus, GatewayReply, LogEntry};
use motile_protocol::now;
use motile_protocol::wire::Agent;
use serde_json::{Value, json};

use crate::accounts::Tokens;
use crate::gateway::{Asked, CLAUDE_CACHE, CLAUDE_LONG_CACHE, CODEX_CACHE, Gateway, agent_title};
use crate::limits::{self, header};
use crate::oauth::CLAUDE_OAUTH_BETA;
use crate::router::ThreadKey;
use crate::store::{Captured, Record};
use crate::usage::{EventScanner, Spent, read_body};

const MAX_REQUEST: usize = 64 * 1024 * 1024;
/// How much of a failed response's body the log keeps.
const ERROR_KEPT: usize = 4096;
/// How much of a captured response is kept.
const CAPTURE_KEPT: usize = 32 * 1024 * 1024;
/// How much of a response that isn't a stream is kept to read its usage from.
const BODY_KEPT: usize = 16 * 1024 * 1024;
/// Headers that belong to one hop, or carry the agent's own credentials, and never go on.
pub const NOT_PASSED: [&str; 18] = [
    "host",
    "content-length",
    "connection",
    "keep-alive",
    "transfer-encoding",
    "te",
    "trailer",
    "upgrade",
    "proxy-authorization",
    "proxy-connection",
    "authorization",
    "x-openai-actor-authorization",
    "x-api-key",
    "accept-encoding",
    "sec-websocket-key",
    "sec-websocket-version",
    "sec-websocket-extensions",
    "sec-websocket-accept",
];

pub struct Incoming {
    pub server: String,
    pub thread: String,
    pub agent: Agent,
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    /// The account the thread is pinned to, which it never leaves.
    pub pin: Option<String>,
    /// The body comes compressed with zstd.
    pub compressed: bool,
}

fn decompressed(body: &[u8]) -> anyhow::Result<Vec<u8>> {
    use std::io::Read;
    let mut decoded = Vec::new();
    let decoder = zstd::stream::read::Decoder::new(body).context("The request's body isn't zstd.")?;
    decoder.take(MAX_REQUEST as u64 + 1).read_to_end(&mut decoded).context("The request's body isn't zstd.")?;
    anyhow::ensure!(decoded.len() <= MAX_REQUEST, "The request's body is too large.");
    Ok(decoded)
}

impl Incoming {
    pub fn key(&self) -> ThreadKey {
        ThreadKey { server: self.server.clone(), thread: self.thread.clone() }
    }
}

/// What came back from the accounts tried.
enum Outcome {
    Response(String, Box<reqwest::Response>, Vec<(String, String)>),
    /// Every account reached its limit; the last one's answer.
    Limited(u16, Vec<(String, String)>, Vec<u8>),
    NoAccount,
    Failed(anyhow::Error),
}

pub async fn serve(
    gateway: Arc<Gateway>,
    incoming: Incoming,
    mut send: SendStream,
    mut recv: RecvStream,
) -> anyhow::Result<()> {
    let started = Instant::now();
    let at = now();
    let body = recv.read_to_end(MAX_REQUEST).await.context("The request's body didn't arrive whole.")?;
    let body = match incoming.compressed {
        true => decompressed(&body)?,
        false => body,
    };
    let parsed: Option<Value> = serde_json::from_slice(&body).ok();
    let model = parsed.as_ref().and_then(|body| body["model"].as_str()).map(String::from);
    let lifetime = cache_lifetime(incoming.agent, &body);
    let key = incoming.key();
    let captured = gateway.capturing(&incoming.server, &incoming.thread);

    let mut tried: Vec<String> = Vec::new();
    let mut renewed: Vec<String> = Vec::new();
    let mut limited = None;
    let mut sent_body = body.clone();
    let outcome = loop {
        let asked = Asked {
            agent: incoming.agent,
            model: model.as_deref(),
            cache_lifetime: lifetime,
            pin: incoming.pin.as_deref(),
        };
        let Some(account) = gateway.choose(&key, asked, &tried, now()) else {
            break match limited.take() {
                Some((status, headers, body)) => Outcome::Limited(status, headers, body),
                None => Outcome::NoAccount,
            };
        };
        let Some(tokens) = gateway.tokens(&account.id).await else {
            tried.push(account.id);
            continue;
        };
        sent_body = without_foreign_reasoning(&gateway, &key, incoming.agent, &body, parsed.as_ref());
        let request = gateway
            .http
            .request(
                reqwest::Method::from_bytes(incoming.method.as_bytes())?,
                upstream_url(&gateway, incoming.agent, &incoming.path),
            )
            .body(sent_body.clone());
        let request = upstream_headers(incoming.agent, &incoming.headers, &tokens)
            .into_iter()
            .fold(request, |request, (name, value)| request.header(name, value));
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => break Outcome::Failed(error.into()),
        };
        let headers = header_pairs(response.headers());
        let status = response.status().as_u16();
        if let Some(reading) = limits::from_headers(incoming.agent, &headers) {
            gateway.observe(&account.id, &reading, now());
        }
        if status == 401 {
            if !renewed.contains(&account.id) && gateway.renew(&account.id, &tokens.access).await.is_some() {
                renewed.push(account.id);
                continue;
            }
            gateway.sign_in_again(&account.id);
            tried.push(account.id);
            continue;
        }
        if status == 429 {
            let error = response.bytes().await.unwrap_or_default().to_vec();
            let until = limits::limit_reached(incoming.agent, status, &headers, &error, now()).unwrap_or(now());
            gateway.rest(&account.id, until);
            tried.push(account.id);
            limited = Some((status, headers, error));
            continue;
        }
        break Outcome::Response(account.id, Box::new(response), headers);
    };

    let mut entry = LogEntry {
        id: 0,
        at,
        server: incoming.server.clone(),
        thread: incoming.thread.clone(),
        account: None,
        model: model.clone(),
        path: incoming.path.clone(),
        status: 0,
        latency_ms: 0,
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        retries: tried.len() as u32,
        request_bytes: body.len() as u64,
        response_bytes: 0,
        error: None,
        captured,
    };
    let mut spent = Spent { model: model.clone(), ..Spent::default() };
    let mut kept_response = Vec::new();
    let mut response_headers = Vec::new();
    match outcome {
        Outcome::Response(account, response, headers) => {
            entry.status = response.status().as_u16();
            entry.latency_ms = started.elapsed().as_millis() as u64;
            response_headers = passed_back(&headers);
            let head = GatewayReply::HttpHead {
                status: entry.status,
                headers: response_headers.clone(),
                account: Some(account.clone()),
            };
            entry.account = Some(account);
            write_frame(&mut send, &head).await?;
            let streams = header(&headers, "content-type").is_some_and(|kind| kind.contains("event-stream"));
            let keep = if captured {
                CAPTURE_KEPT
            } else if entry.status >= 400 {
                ERROR_KEPT
            } else if streams {
                0
            } else {
                BODY_KEPT
            };
            let mut scanner = EventScanner::default();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        entry.error = Some(format!("The response broke off: {error}"));
                        break;
                    }
                };
                entry.response_bytes += chunk.len() as u64;
                if streams {
                    for event in scanner.feed(&chunk) {
                        spent.read_event(incoming.agent, &event);
                    }
                }
                let room = keep.saturating_sub(kept_response.len());
                kept_response.extend_from_slice(&chunk[..room.min(chunk.len())]);
                if send.write_all(&chunk).await.is_err() {
                    entry.error.get_or_insert_with(|| "The server stopped reading the response.".to_string());
                    break;
                }
            }
            if !streams {
                let read = read_body(incoming.agent, &kept_response);
                spent = Spent { model: read.model.or(spent.model), ..read };
            }
            if entry.status >= 400 {
                entry.error =
                    Some(String::from_utf8_lossy(&kept_response[..kept_response.len().min(ERROR_KEPT)]).into_owned());
            }
            let _ = send.finish();
        }
        Outcome::Limited(status, headers, error) => {
            entry.status = status;
            entry.error = Some(String::from_utf8_lossy(&error[..error.len().min(ERROR_KEPT)]).into_owned());
            response_headers = passed_back(&headers);
            write_frame(
                &mut send,
                &GatewayReply::HttpHead { status, headers: response_headers.clone(), account: None },
            )
            .await?;
            send.write_all(&error).await?;
            entry.response_bytes = error.len() as u64;
            kept_response = error;
            let _ = send.finish();
        }
        Outcome::NoAccount => {
            let message = no_account(&gateway, incoming.agent, incoming.pin.as_deref());
            entry.error = Some(message.clone());
            match limit_reply(&gateway, incoming.agent, incoming.pin.as_deref()) {
                Some(made) => {
                    entry.status = 429;
                    write_frame(
                        &mut send,
                        &GatewayReply::HttpHead { status: 429, headers: made.headers.clone(), account: None },
                    )
                    .await?;
                    send.write_all(&made.body).await?;
                    response_headers = made.headers;
                    kept_response = made.body;
                }
                None => write_frame(&mut send, &GatewayReply::Error { message }).await?,
            }
            let _ = send.finish();
        }
        Outcome::Failed(error) => {
            let message = format!("{} couldn't be reached: {error}", upstream_name(incoming.agent));
            entry.status = 502;
            entry.error = Some(message.clone());
            write_frame(&mut send, &GatewayReply::Error { message }).await?;
            let _ = send.finish();
        }
    }
    entry.input_tokens = spent.input;
    entry.output_tokens = spent.output;
    entry.cache_read_tokens = spent.cache_read;
    entry.cache_write_tokens = spent.cache_write;
    entry.model = spent.model.clone().or(entry.model);
    let captured = captured.then(|| Captured {
        request_headers: incoming.headers.iter().filter(|(name, _)| !is_secret(name)).cloned().collect(),
        request_body: sent_body,
        response_headers,
        response_body: kept_response,
    });
    let cost = gateway.cost(&spent);
    gateway.record(&key, Record { entry, agent: incoming.agent, finished_at: now(), cost, spent }, captured);
    Ok(())
}

/// How long the request's prompt cache lasts: an hour when Claude Code asks for it.
pub fn cache_lifetime(agent: Agent, body: &[u8]) -> f64 {
    match agent {
        Agent::Codex => CODEX_CACHE,
        Agent::Claude if contains(body, br#""ttl":"1h""#) => CLAUDE_LONG_CACHE,
        Agent::Claude => CLAUDE_CACHE,
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

/// The body as it goes to the thread's account: a Codex request that came from another account leaves
/// out the encrypted reasoning that account wrote.
pub fn without_foreign_reasoning(
    gateway: &Gateway,
    key: &ThreadKey,
    agent: Agent,
    body: &[u8],
    parsed: Option<&Value>,
) -> Vec<u8> {
    let Some(parsed) = parsed.filter(|_| agent == Agent::Codex) else { return body.to_vec() };
    let Some(items) = parsed["input"].as_array().map(Vec::len) else { return body.to_vec() };
    let Some(before) = gateway.moved_items(key, items) else { return body.to_vec() };
    let mut parsed = parsed.clone();
    match drop_foreign_reasoning(&mut parsed, before) {
        true => serde_json::to_vec(&parsed).unwrap_or_else(|_| body.to_vec()),
        false => body.to_vec(),
    }
}

/// Drops the reasoning items with encrypted content among the first `before` input items.
pub fn drop_foreign_reasoning(request: &mut Value, before: usize) -> bool {
    let Some(input) = request["input"].as_array_mut() else { return false };
    let length = input.len();
    let mut index = 0;
    input.retain(|item| {
        let foreign = index < before && item["type"] == "reasoning" && !item["encrypted_content"].is_null();
        index += 1;
        !foreign
    });
    input.len() != length
}

pub fn upstream_url(gateway: &Gateway, agent: Agent, path: &str) -> String {
    match agent {
        Agent::Claude => format!("{}{path}", gateway.upstreams.anthropic),
        Agent::Codex => format!("{}/codex{path}", gateway.upstreams.chatgpt),
    }
}

/// The agent's headers with the account's credentials in place of its own.
pub fn upstream_headers(agent: Agent, incoming: &[(String, String)], tokens: &Tokens) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = incoming
        .iter()
        .filter(|(name, _)| !NOT_PASSED.contains(&name.to_ascii_lowercase().as_str()))
        .cloned()
        .collect();
    headers.push(("authorization".into(), format!("Bearer {}", tokens.access)));
    match agent {
        Agent::Claude => {
            let betas = header(&headers, "anthropic-beta").unwrap_or_default().to_string();
            headers.retain(|(name, _)| !name.eq_ignore_ascii_case("anthropic-beta"));
            let mut betas: Vec<&str> = betas.split(',').map(str::trim).filter(|beta| !beta.is_empty()).collect();
            if !betas.contains(&CLAUDE_OAUTH_BETA) {
                betas.push(CLAUDE_OAUTH_BETA);
            }
            headers.push(("anthropic-beta".into(), betas.join(",")));
        }
        Agent::Codex => {
            headers.retain(|(name, _)| !name.eq_ignore_ascii_case("chatgpt-account-id"));
            if let Some(chatgpt_account) = &tokens.chatgpt_account {
                headers.push(("chatgpt-account-id".into(), chatgpt_account.clone()));
            }
            if header(&headers, "originator").is_none() {
                headers.push(("originator".into(), "codex_cli_rs".into()));
            }
        }
    }
    headers
}

pub fn header_pairs(headers: &reqwest::header::HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| Some((name.as_str().to_string(), value.to_str().ok()?.to_string())))
        .collect()
}

/// The upstream's response headers that go back to the server.
pub fn passed_back(headers: &[(String, String)]) -> Vec<(String, String)> {
    headers
        .iter()
        .filter(|(name, _)| !NOT_PASSED.contains(&name.as_str()) && !name.starts_with("sec-websocket"))
        .cloned()
        .collect()
}

fn is_secret(name: &str) -> bool {
    ["authorization", "x-api-key", "cookie"].contains(&name.to_ascii_lowercase().as_str())
}

fn upstream_name(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "Anthropic",
        Agent::Codex => "OpenAI",
    }
}

pub fn no_account(gateway: &Gateway, agent: Agent, pin: Option<&str>) -> String {
    let title = agent_title(agent);
    if let Some(pin) = pin {
        let Some(account) = gateway.accounts().into_iter().find(|account| account.id == pin) else {
            return "The account this thread is pinned to is no longer on your gateway.".to_string();
        };
        return match account.status(now()) {
            _ if !account.enabled => format!("{} is turned off on your gateway.", account.name),
            AccountStatus::SignInAgain => format!("{} needs to be signed in again on your gateway.", account.name),
            _ => format!("{} can't take requests right now.", account.name),
        };
    }
    match gateway.accounts().iter().any(|account| account.agent == agent) {
        true => format!("Every {title} account of your gateway is at its limit, turned off or signed out."),
        false => format!("Your gateway has no {title} account. Add one in Settings → Gateway."),
    }
}

/// An HTTP response the gateway makes itself.
pub struct Made {
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// The limit error the agent knows, when its accounts all rest, or the one it is pinned to does: it
/// waits for the soonest reset.
pub fn limit_reply(gateway: &Gateway, agent: Agent, pin: Option<&str>) -> Option<Made> {
    let now = now();
    let resting = gateway
        .accounts()
        .into_iter()
        .filter(|account| account.agent == agent && account.enabled && pin.is_none_or(|pin| account.id == pin));
    let until = resting
        .filter_map(|account| match account.status(now) {
            AccountStatus::Resting { until } => until,
            _ => None,
        })
        .fold(f64::INFINITY, f64::min);
    if !until.is_finite() {
        return None;
    }
    let message = match pin.and_then(|pin| gateway.accounts().into_iter().find(|account| account.id == pin)) {
        Some(account) => format!("{} is at its limit.", account.name),
        None => format!("Every {} account of your gateway is at its limit.", agent_title(agent)),
    };
    let retry_after = ((until - now).max(1.0) as u64).to_string();
    let mut headers =
        vec![("content-type".to_string(), "application/json".to_string()), ("retry-after".to_string(), retry_after)];
    let body = match agent {
        Agent::Claude => {
            headers.push(("anthropic-ratelimit-unified-status".into(), "rejected".into()));
            headers.push(("anthropic-ratelimit-unified-reset".into(), (until as u64).to_string()));
            headers.push(("anthropic-ratelimit-unified-representative-claim".into(), claude_claim(until - now).into()));
            headers.push(("anthropic-ratelimit-unified-overage-status".into(), "rejected".into()));
            json!({"type": "error", "error": {"type": "rate_limit_error", "message": message}})
        }
        Agent::Codex => {
            json!({"error": {"type": "usage_limit_reached", "message": message, "resets_at": until as u64}})
        }
    };
    Some(Made { headers, body: body.to_string().into_bytes() })
}

/// The window Claude Code is told ran out, which it needs to read the reset at all.
fn claude_claim(resets_in: f64) -> &'static str {
    match resets_in > 5.0 * 3600.0 {
        true => "seven_day",
        false => "five_hour",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect()
    }

    fn tokens(chatgpt_account: Option<&str>) -> Tokens {
        Tokens {
            access: "account-token".into(),
            refresh: "r".into(),
            expires_at: 0.0,
            chatgpt_account: chatgpt_account.map(String::from),
        }
    }

    #[test]
    fn claude_codes_key_gives_way_to_the_accounts_token_and_the_oauth_beta_joins_its_own() {
        let incoming = pairs(&[
            ("Authorization", "Bearer thread-token"),
            ("x-api-key", "sk-ant"),
            ("anthropic-beta", "claude-code-20250219, interleaved-thinking-2025-05-14"),
            ("anthropic-version", "2023-06-01"),
            ("accept-encoding", "gzip"),
            ("Host", "127.0.0.1"),
        ]);
        let headers = upstream_headers(Agent::Claude, &incoming, &tokens(None));
        assert_eq!(header(&headers, "authorization"), Some("Bearer account-token"));
        assert_eq!(header(&headers, "x-api-key"), None);
        assert_eq!(
            header(&headers, "anthropic-beta"),
            Some("claude-code-20250219,interleaved-thinking-2025-05-14,oauth-2025-04-20")
        );
        assert_eq!(header(&headers, "anthropic-version"), Some("2023-06-01"));
        assert_eq!((header(&headers, "accept-encoding"), header(&headers, "host")), (None, None));
    }

    #[test]
    fn codex_is_given_the_accounts_chatgpt_account() {
        let incoming = pairs(&[
            ("authorization", "Bearer thread-token"),
            ("X-OpenAI-Actor-Authorization", "Bearer thread-token"),
            ("session-id", "s"),
            ("originator", "codex_exec"),
        ]);
        let headers = upstream_headers(Agent::Codex, &incoming, &tokens(Some("acct-1")));
        assert_eq!(header(&headers, "authorization"), Some("Bearer account-token"));
        assert_eq!(header(&headers, "chatgpt-account-id"), Some("acct-1"));
        assert_eq!(header(&headers, "x-openai-actor-authorization"), None);
        assert_eq!((header(&headers, "session-id"), header(&headers, "originator")), (Some("s"), Some("codex_exec")));
    }

    #[test]
    fn only_reasoning_another_account_encrypted_is_dropped() {
        let mut request = json!({"input": [
            {"type": "message", "role": "user"},
            {"type": "reasoning", "encrypted_content": "from-a"},
            {"type": "reasoning", "summary": []},
            {"type": "function_call"},
            {"type": "reasoning", "encrypted_content": "from-b"},
        ]});
        assert!(drop_foreign_reasoning(&mut request, 4));
        let kinds: Vec<_> =
            request["input"].as_array().unwrap().iter().map(|item| item["encrypted_content"].as_str()).collect();
        assert_eq!(kinds, [None, None, None, Some("from-b")]);
        assert!(!drop_foreign_reasoning(&mut request, 1));
    }

    #[test]
    fn claudes_cache_lasts_an_hour_only_when_the_request_asks() {
        assert_eq!(
            cache_lifetime(Agent::Claude, br#"{"cache_control":{"type":"ephemeral","ttl":"1h"}}"#),
            CLAUDE_LONG_CACHE
        );
        assert_eq!(cache_lifetime(Agent::Claude, br#"{"cache_control":{"type":"ephemeral"}}"#), CLAUDE_CACHE);
        assert_eq!(cache_lifetime(Agent::Codex, b"{}"), CODEX_CACHE);
    }
}
