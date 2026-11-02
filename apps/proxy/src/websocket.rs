//! Codex's WebSocket, bridged to the ChatGPT backend's with an account's token. A socket stays on the
//! account it opened on. When the thread's account changes, the socket is closed before the next
//! `response.create` goes out: Codex opens a new one and sends its whole input, since a
//! `previous_response_id` only means something to the socket that answered it. A limit error in place
//! of a response does the same when another account can take the thread, and goes back to Codex when
//! none can.

use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures_util::{SinkExt, StreamExt};
use iroh::endpoint::{RecvStream, SendStream};
use motile_protocol::frame::{read_frame, write_frame};
use motile_protocol::now;
use motile_protocol::proxy_wire::{LogEntry, ProxyReply, WsMessage};
use motile_protocol::wire::Agent;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::{self, Message};

use crate::accounts::Account;
use crate::limits;
use crate::proxy::{Asked, CODEX_CACHE, Proxy};
use crate::router::ThreadKey;
use crate::store::{Captured, Record};
use crate::upstream::{
    Incoming, drop_foreign_reasoning, header_pairs, limit_reply, no_account, passed_back, upstream_headers,
    upstream_url,
};
use crate::usage::Spent;

/// Closes Codex's socket so that it opens another, on the thread's new account.
const MOVED: u16 = 1012;
const ERROR_KEPT: usize = 4096;

type Upstream = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub async fn serve(
    proxy: Arc<Proxy>,
    incoming: Incoming,
    mut send: SendStream,
    mut recv: RecvStream,
) -> anyhow::Result<()> {
    if incoming.agent != Agent::Codex {
        write_frame(&mut send, &ProxyReply::HttpHead { status: 426, headers: vec![], account: None }).await?;
        let _ = send.finish();
        return Ok(());
    }
    let key = incoming.key();
    let Some((account, upstream)) = open(&proxy, &incoming, &key, &mut send).await? else { return Ok(()) };

    let (frames, mut from_codex) = mpsc::channel::<WsMessage>(16);
    tokio::spawn(async move {
        while let Ok(Some(message)) = read_frame::<_, WsMessage>(&mut recv).await {
            if frames.send(message).await.is_err() {
                break;
            }
        }
    });
    let (mut to_openai, mut from_openai) = upstream.split();
    let mut turn: Option<Turn> = None;
    loop {
        tokio::select! {
            message = from_codex.recv() => {
                let Some(message) = message else { break };
                let message = match message {
                    WsMessage::Text { text } => {
                        let Some(text) = begin_turn(&proxy, &incoming, &key, &account, &text, &mut turn) else {
                            close(&mut send, MOVED, "The thread moved to another account.").await;
                            break;
                        };
                        Message::text(text)
                    }
                    WsMessage::Binary { data } => Message::binary(STANDARD.decode(data).unwrap_or_default()),
                    WsMessage::Close { code, reason } => {
                        let frame = CloseFrame { code: code.into(), reason: reason.into() };
                        let _ = to_openai.send(Message::Close(Some(frame))).await;
                        break;
                    }
                };
                if to_openai.send(message).await.is_err() {
                    close(&mut send, 1011, "OpenAI closed the connection.").await;
                    break;
                }
            }
            message = from_openai.next() => {
                let message = match message {
                    Some(Ok(message)) => message,
                    Some(Err(error)) => {
                        close(&mut send, 1011, &format!("OpenAI's connection broke: {error}")).await;
                        break;
                    }
                    None => {
                        close(&mut send, 1000, "").await;
                        break;
                    }
                };
                let forward = match message {
                    Message::Text(text) => match answer(&proxy, &key, &account, text.as_str(), &mut turn) {
                        Answer::Forward => WsMessage::Text { text: text.to_string() },
                        Answer::Move => {
                            close(&mut send, MOVED, "The account reached its limit.").await;
                            break;
                        }
                    },
                    Message::Binary(data) => WsMessage::Binary { data: STANDARD.encode(data) },
                    Message::Close(frame) => {
                        let (code, reason) = frame.map(|frame| (frame.code.into(), frame.reason.to_string())).unwrap_or((1000, String::new()));
                        close(&mut send, code, &reason).await;
                        break;
                    }
                    _ => continue,
                };
                if write_frame(&mut send, &forward).await.is_err() {
                    break;
                }
            }
        }
    }
    if let Some(turn) = turn.take() {
        turn.finish(&proxy, &key, &account, 0, "The connection closed before the response finished.");
    }
    Ok(())
}

/// Opens the backend's socket on the thread's account, trying the next on a limit. Answers Codex with
/// `WebSocketOpen`, or with the backend's refusal as an HTTP response.
async fn open(
    proxy: &Arc<Proxy>,
    incoming: &Incoming,
    key: &ThreadKey,
    send: &mut SendStream,
) -> anyhow::Result<Option<(Account, Upstream)>> {
    let mut tried: Vec<String> = Vec::new();
    let mut renewed: Vec<String> = Vec::new();
    loop {
        let pin = incoming.pin.as_deref();
        let asked = Asked { agent: incoming.agent, model: None, cache_lifetime: CODEX_CACHE, pin };
        let Some(account) = proxy.choose(key, asked, &tried, now()) else {
            match limit_reply(proxy, incoming.agent, pin) {
                Some(made) => {
                    write_frame(send, &ProxyReply::HttpHead { status: 429, headers: made.headers, account: None })
                        .await?;
                    send.write_all(&made.body).await?;
                }
                None => {
                    write_frame(send, &ProxyReply::Error { message: no_account(proxy, incoming.agent, pin) }).await?
                }
            }
            let _ = send.finish();
            return Ok(None);
        };
        let Some(tokens) = proxy.tokens(&account.id).await else {
            tried.push(account.id);
            continue;
        };
        let url = upstream_url(proxy, incoming.agent, &incoming.path).replacen("http", "ws", 1);
        let mut request = url.into_client_request().context("The backend's address isn't valid.")?;
        for (name, value) in upstream_headers(incoming.agent, &incoming.headers, &tokens) {
            let (Ok(name), Ok(value)) = (tungstenite::http::HeaderName::try_from(name), value.parse()) else {
                continue;
            };
            request.headers_mut().insert(name, value);
        }
        match tokio_tungstenite::connect_async(request).await {
            Ok((upstream, response)) => {
                let headers = header_pairs(response.headers());
                if let Some(reading) = limits::from_headers(incoming.agent, &headers) {
                    proxy.observe(&account.id, &reading, now());
                }
                let open =
                    ProxyReply::WebSocketOpen { headers: passed_back(&headers), account: Some(account.id.clone()) };
                write_frame(send, &open).await?;
                return Ok(Some((account, upstream)));
            }
            Err(tungstenite::Error::Http(response)) => {
                let status = response.status().as_u16();
                let headers = header_pairs(response.headers());
                let body = response.body().clone().unwrap_or_default();
                if status == 401
                    && !renewed.contains(&account.id)
                    && proxy.renew(&account.id, &tokens.access).await.is_some()
                {
                    renewed.push(account.id);
                    continue;
                }
                if status == 401 {
                    proxy.sign_in_again(&account.id);
                    tried.push(account.id);
                    continue;
                }
                if let Some(until) = limits::limit_reached(incoming.agent, status, &headers, &body, now()) {
                    proxy.rest(&account.id, until);
                    tried.push(account.id);
                    continue;
                }
                write_frame(
                    send,
                    &ProxyReply::HttpHead { status, headers: passed_back(&headers), account: Some(account.id.clone()) },
                )
                .await?;
                send.write_all(&body).await?;
                let _ = send.finish();
                return Ok(None);
            }
            Err(error) => {
                write_frame(send, &ProxyReply::Error { message: format!("OpenAI couldn't be reached: {error}") })
                    .await?;
                let _ = send.finish();
                return Ok(None);
            }
        }
    }
}

/// A `response.create` and what came back for it.
struct Turn {
    path: String,
    at: f64,
    started: Instant,
    request_bytes: u64,
    response_bytes: u64,
    /// Nothing came back for it yet.
    answered: bool,
    spent: Spent,
    captured: Option<(String, Vec<u8>)>,
}

impl Turn {
    fn finish(self, proxy: &Proxy, key: &ThreadKey, account: &Account, status: u16, error: &str) {
        let error = (!error.is_empty()).then(|| error[..error.floor_char_boundary(ERROR_KEPT)].to_string());
        let entry = LogEntry {
            id: 0,
            at: self.at,
            server: key.server.clone(),
            thread: key.thread.clone(),
            account: Some(account.id.clone()),
            model: self.spent.model.clone(),
            path: self.path,
            status,
            latency_ms: self.started.elapsed().as_millis() as u64,
            input_tokens: self.spent.input,
            output_tokens: self.spent.output,
            cache_read_tokens: self.spent.cache_read,
            cache_write_tokens: self.spent.cache_write,
            retries: 0,
            request_bytes: self.request_bytes,
            response_bytes: self.response_bytes,
            error,
            captured: self.captured.is_some(),
        };
        let captured = self.captured.map(|(request, response)| Captured {
            request_headers: vec![],
            request_body: request.into_bytes(),
            response_headers: vec![],
            response_body: response,
        });
        let cost = proxy.cost(&self.spent);
        proxy.record(key, Record { entry, agent: Agent::Codex, finished_at: now(), cost, spent: self.spent }, captured);
    }
}

/// A message from Codex, as it goes on: `None` when the thread's account is no longer this socket's.
fn begin_turn(
    proxy: &Proxy,
    incoming: &Incoming,
    key: &ThreadKey,
    account: &Account,
    text: &str,
    turn: &mut Option<Turn>,
) -> Option<String> {
    let Ok(mut request) = serde_json::from_str::<Value>(text) else { return Some(text.to_string()) };
    if request["type"] != "response.create" {
        return Some(text.to_string());
    }
    let model = request["model"].as_str().map(String::from);
    let asked = Asked {
        agent: incoming.agent,
        model: model.as_deref(),
        cache_lifetime: CODEX_CACHE,
        pin: incoming.pin.as_deref(),
    };
    let chosen = proxy.choose(key, asked, &[], now())?;
    if chosen.id != account.id {
        return None;
    }
    let items = request["input"].as_array().map(Vec::len).unwrap_or(0);
    let text = match proxy.moved_items(key, items) {
        Some(before) if drop_foreign_reasoning(&mut request, before) => request.to_string(),
        _ => text.to_string(),
    };
    let captured = proxy.capturing(&incoming.server, &incoming.thread).then(|| (text.clone(), Vec::new()));
    *turn = Some(Turn {
        path: incoming.path.clone(),
        at: now(),
        started: Instant::now(),
        request_bytes: text.len() as u64,
        response_bytes: 0,
        answered: false,
        spent: Spent { model, ..Spent::default() },
        captured,
    });
    Some(text)
}

enum Answer {
    Forward,
    /// The account reached its limit before answering, and another can take the thread.
    Move,
}

fn answer(proxy: &Proxy, key: &ThreadKey, account: &Account, text: &str, turn: &mut Option<Turn>) -> Answer {
    let Ok(event) = serde_json::from_str::<Value>(text) else { return Answer::Forward };
    let now = now();
    if let Some(reading) = limits::from_codex_event(&event) {
        proxy.observe(&account.id, &reading, now);
        return Answer::Forward;
    }
    let Some(current) = turn.as_mut() else { return Answer::Forward };
    current.response_bytes += text.len() as u64;
    if let Some((_, response)) = &mut current.captured {
        response.extend_from_slice(text.as_bytes());
        response.push(b'\n');
    }
    let first = !current.answered;
    current.answered = true;
    if let Some(until) = limits::limit_reached_event(&event, now) {
        proxy.rest(&account.id, until);
        let moves = first && proxy.has_other(Agent::Codex, &account.id, now);
        let finished = turn.take().expect("a turn is under way");
        finished.finish(proxy, key, account, 429, text);
        return if moves { Answer::Move } else { Answer::Forward };
    }
    current.spent.read_event(Agent::Codex, &event);
    let status = match event["type"].as_str() {
        Some("response.completed" | "response.done") => 200,
        Some("response.failed" | "response.incomplete") => 500,
        Some("error") => event["status"].as_u64().unwrap_or(500) as u16,
        _ => return Answer::Forward,
    };
    let finished = turn.take().expect("a turn is under way");
    finished.finish(proxy, key, account, status, if status == 200 { "" } else { text });
    Answer::Forward
}

async fn close(send: &mut SendStream, code: u16, reason: &str) {
    let _ = write_frame(send, &WsMessage::Close { code, reason: reason.to_string() }).await;
    let _ = send.finish();
}
