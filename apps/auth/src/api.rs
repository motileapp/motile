//! What the clients, the servers and the web app call. A linked device signs its requests with its
//! key; the requests that link one prove the key with a signature over what they redeem. The web
//! client sends the token of the session a person signed in to.

use std::collections::HashMap;
use std::net::SocketAddr;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use motile_protocol::auth_api::{
    DevLoginRequest, DevLoginResponse, DeviceKind, EnrollRequest, EnrollResponse, EnrollToken, ExchangeRequest, Me,
    PushRegistration, PushService, PushTarget, PushWork, RelayRequest, SealedAlert, Session, SessionRequest,
    enroll_message, link_message,
};
use motile_protocol::identity::{is_public_key, random_install_code, random_token, sha256_hex, verify, verify_request};
use motile_protocol::now;
use motile_protocol::relay::checked_url;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::db::{NewSignIn, Phone, UserRow, WorkPart};
use crate::error::{AppError, AppResult};
use crate::google::Identity;
use crate::{AppState, db, limits};

const MAX_NAME_CHARS: usize = 100;

fn clean(text: &str, fallback: &str) -> String {
    let cleaned: String =
        text.trim().chars().filter(|character| !character.is_control()).take(MAX_NAME_CHARS).collect();
    if cleaned.is_empty() { fallback.to_string() } else { cleaned }
}

/// Who is asking: a device that signed the request, or a person signed in to the web app.
enum Caller {
    Device(String),
    Web(UserRow),
}

fn session_token(headers: &HeaderMap) -> Option<&str> {
    headers.get("authorization")?.to_str().ok()?.strip_prefix("Bearer ")
}

async fn caller(state: &AppState, method: &Method, uri: &Uri, headers: &HeaderMap, body: &[u8]) -> AppResult<Caller> {
    let Some(token) = session_token(headers) else {
        let header = headers.get("authorization").and_then(|value| value.to_str().ok()).unwrap_or_default();
        let public_key = verify_request(header, method.as_str(), uri.path(), body, now())
            .map_err(|error| AppError::unauthorized(error.to_string()))?;
        return Ok(Caller::Device(public_key));
    };
    let user = db::user_of_session(&state.db, &sha256_hex(token.as_bytes())).await?;
    user.map(Caller::Web).ok_or_else(|| AppError::unauthorized("This session has ended. Sign in again."))
}

async fn linked_user(state: &AppState, public_key: &str) -> AppResult<UserRow> {
    let user = db::user_of_device(&state.db, public_key).await?;
    user.ok_or_else(|| AppError::unauthorized("This device isn't linked to an account."))
}

fn sign_in_expired() -> AppError {
    AppError::bad_request("This sign-in has expired. Sign in again.")
}

/// Uses up a sign-in's code and returns who signed in. The code of a sign-in the web app started
/// only opens a session, and a client's only links that client.
async fn redeem(state: &AppState, code: &str, verifier: &str, web: bool) -> AppResult<Uuid> {
    let taken = db::take_sign_in(&state.db, &sha256_hex(code.as_bytes()), web).await?;
    let sign_in = taken.ok_or_else(sign_in_expired)?;
    if sha256_hex(verifier.as_bytes()) != sign_in.challenge {
        return Err(AppError::unauthorized("This sign-in was started by another client."));
    }
    sign_in.user_id.ok_or_else(sign_in_expired)
}

pub async fn exchange(State(state): State<AppState>, Json(request): Json<ExchangeRequest>) -> AppResult<Json<Me>> {
    if !is_public_key(&request.public_key) {
        return Err(AppError::bad_request("That isn't a device key."));
    }
    if !verify(&request.public_key, &link_message(&request.code), &request.signature) {
        return Err(AppError::unauthorized("The device's signature doesn't match."));
    }
    let user_id = redeem(&state, &request.code, &request.verifier, false).await?;
    let user = db::user_by_id(&state.db, user_id).await?.ok_or_else(sign_in_expired)?;

    let name = clean(&request.name, "Motile client");
    let platform = clean(&request.platform, "unknown");
    if !db::link_device(&state.db, user.id, &request.public_key, DeviceKind::Client, &name, &platform).await? {
        return Err(AppError::new(StatusCode::CONFLICT, "This device can't be linked to the account."));
    }
    Ok(Json(db::me(&state.db, &user).await?))
}

pub async fn dev_login(
    State(state): State<AppState>,
    Json(request): Json<DevLoginRequest>,
) -> AppResult<Json<DevLoginResponse>> {
    if !state.config.dev_login {
        return Err(AppError::not_found());
    }
    let email = request.email.trim().to_lowercase();
    if email.is_empty() {
        return Err(AppError::bad_request("An email address is needed."));
    }
    let identity = Identity {
        subject: format!("dev:{email}"),
        name: email.split('@').next().map(String::from),
        email,
        email_verified: true,
        picture: None,
    };
    let user = db::upsert_user(&state.db, &identity).await?;
    let (id, code) = (random_token(), random_token());
    let sign_in = NewSignIn {
        id: &id,
        challenge: &request.challenge,
        app_state: "dev",
        google_verifier: "",
        nonce: "",
        web: request.web,
    };
    db::create_sign_in(&state.db, &sign_in).await?;
    db::complete_sign_in(&state.db, &id, user.id, &sha256_hex(code.as_bytes())).await?;
    Ok(Json(DevLoginResponse { code }))
}

pub async fn create_session(
    State(state): State<AppState>,
    Json(request): Json<SessionRequest>,
) -> AppResult<Json<Session>> {
    let user_id = redeem(&state, &request.code, &request.verifier, true).await?;
    let token = random_token();
    let expires_at = db::create_session(&state.db, user_id, &sha256_hex(token.as_bytes())).await?;
    Ok(Json(Session { token, expires_at: expires_at.timestamp_millis() as f64 / 1000.0 }))
}

pub async fn end_session(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<Value>> {
    let Some(token) = session_token(&headers) else {
        return Err(AppError::unauthorized("There is no session to end."));
    };
    db::end_session(&state.db, &sha256_hex(token.as_bytes())).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn me(State(state): State<AppState>, method: Method, uri: Uri, headers: HeaderMap) -> AppResult<Json<Me>> {
    let user = match caller(&state, &method, &uri, &headers, b"").await? {
        Caller::Web(user) => Some(user),
        Caller::Device(public_key) => db::user_of_device(&state.db, &public_key).await?,
    };
    let Some(user) = user else {
        return Ok(Json(Me::default()));
    };
    Ok(Json(db::me(&state.db, &user).await?))
}

pub async fn create_enroll_token(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Json<EnrollToken>> {
    let user = match caller(&state, &method, &uri, &headers, &body).await? {
        Caller::Web(user) => user,
        Caller::Device(public_key) => {
            let user = linked_user(&state, &public_key).await?;
            // A server or the gateway can't add devices; only someone signed in can, in a client or on the web.
            if db::device_kind(&state.db, &public_key).await? != Some(DeviceKind::Client) {
                return Err(AppError::new(StatusCode::FORBIDDEN, "Servers are added from a client."));
            }
            user
        }
    };
    let (token, expires_at) = loop {
        let token = random_install_code();
        match db::create_enroll_token(&state.db, user.id, &sha256_hex(token.as_bytes())).await {
            Ok(expires_at) => break (token, expires_at),
            Err(sqlx::Error::Database(error)) if error.is_unique_violation() => continue,
            Err(error) => return Err(error.into()),
        }
    };
    Ok(Json(EnrollToken {
        command: state.config.install_command(&token),
        token,
        expires_at: expires_at.timestamp_millis() as f64 / 1000.0,
    }))
}

pub async fn enroll(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<EnrollRequest>,
) -> AppResult<Json<EnrollResponse>> {
    let address = limits::caller_address(&headers, connection);
    if state.enroll_failures.blocked(address) {
        return Err(AppError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many install commands failed from this address. Try again in 15 minutes.",
        ));
    }
    if request.kind == DeviceKind::Client {
        return Err(AppError::bad_request("A client links by signing in, not with an install command."));
    }
    if !is_public_key(&request.public_key) {
        return Err(AppError::bad_request("That isn't a device key."));
    }
    if !verify(&request.public_key, &enroll_message(&request.token), &request.signature) {
        return Err(AppError::unauthorized("The device's signature doesn't match."));
    }
    // The code is shown in capitals but may have been typed in small letters.
    let token_hash = sha256_hex(request.token.trim().to_ascii_uppercase().as_bytes());
    let expired = || {
        AppError::bad_request(
            "This install command has expired or was used on another machine. Copy a new one from a client.",
        )
    };
    let Some(user_id) = db::use_enroll_token(&state.db, &token_hash, &request.public_key).await? else {
        state.enroll_failures.record(address);
        return Err(expired());
    };
    let user = db::user_by_id(&state.db, user_id).await?.ok_or_else(expired)?;

    let name = clean(&request.name, if request.kind == DeviceKind::Gateway { "Gateway" } else { "Server" });
    let platform = clean(&request.platform, "unknown");
    if !db::link_device(&state.db, user.id, &request.public_key, request.kind, &name, &platform).await? {
        return Err(AppError::new(StatusCode::CONFLICT, "This machine can't be linked to the account."));
    }
    Ok(Json(EnrollResponse { email: user.email }))
}

/// Removes one of the account's devices; `self` is the device that asks.
pub async fn remove_device(
    State(state): State<AppState>,
    Path(target): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    let (user, target) = match caller(&state, &method, &uri, &headers, b"").await? {
        Caller::Web(user) => (user, target),
        Caller::Device(public_key) => {
            let user = linked_user(&state, &public_key).await?;
            (user, if target == "self" { public_key } else { target })
        }
    };
    if !db::remove_device(&state.db, user.id, &target).await? {
        return Err(AppError::not_found());
    }
    Ok(Json(json!({ "ok": true })))
}

/// Sets the account's relay, or goes back to iroh's public relays. Only someone signed in can, in a
/// client or on the web.
pub async fn set_relay(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Json<Value>> {
    let user = match caller(&state, &method, &uri, &headers, &body).await? {
        Caller::Web(user) => user,
        Caller::Device(public_key) => {
            let user = linked_user(&state, &public_key).await?;
            if db::device_kind(&state.db, &public_key).await? != Some(DeviceKind::Client) {
                return Err(AppError::new(StatusCode::FORBIDDEN, "The relay is set from a client."));
            }
            user
        }
    };
    let request: RelayRequest =
        serde_json::from_slice(&body).map_err(|_| AppError::bad_request("That isn't a relay request."))?;
    let url = match request.url.as_deref().map(str::trim).filter(|url| !url.is_empty()) {
        Some(url) => Some(checked_url(url).map_err(AppError::bad_request)?),
        None => None,
    };
    db::set_relay(&state.db, user.id, url.as_deref()).await?;
    Ok(Json(json!({ "ok": true })))
}

/// Deletes one of the account's servers completely: it is removed if it is still linked, and every
/// copy of its threads is dropped. Only someone signed in can, in a client or on the web.
pub async fn delete_server_completely(
    State(state): State<AppState>,
    Path(target): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    let user = match caller(&state, &method, &uri, &headers, b"").await? {
        Caller::Web(user) => user,
        Caller::Device(public_key) => {
            let user = linked_user(&state, &public_key).await?;
            if db::device_kind(&state.db, &public_key).await? != Some(DeviceKind::Client) {
                return Err(AppError::new(StatusCode::FORBIDDEN, "Servers are deleted from a client."));
            }
            user
        }
    };
    if !db::delete_server_completely(&state.db, user.id, &target).await? {
        return Err(AppError::not_found());
    }
    Ok(Json(json!({ "ok": true })))
}

const MAX_TOKEN_CHARS: usize = 512;
const MAX_TOPIC_CHARS: usize = 200;
const MAX_SEALED_CHARS: usize = 4096;
const MAX_THREAD_ID_CHARS: usize = 200;

/// The device that signed the request, if it is one of an account's devices of `kind`.
async fn device_of_kind(
    state: &AppState,
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
    body: &[u8],
    kind: DeviceKind,
    refusal: &str,
) -> AppResult<(String, UserRow)> {
    let Caller::Device(public_key) = caller(state, method, uri, headers, body).await? else {
        return Err(AppError::new(StatusCode::FORBIDDEN, refusal));
    };
    let user = linked_user(state, &public_key).await?;
    if db::device_kind(&state.db, &public_key).await? != Some(kind) {
        return Err(AppError::new(StatusCode::FORBIDDEN, refusal));
    }
    Ok((public_key, user))
}

async fn phone(state: &AppState, method: &Method, uri: &Uri, headers: &HeaderMap, body: &[u8]) -> AppResult<String> {
    let (device, _) =
        device_of_kind(state, method, uri, headers, body, DeviceKind::Client, "Only a client takes pushes.").await?;
    Ok(device)
}

async fn pushing_server(
    state: &AppState,
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
    body: &[u8],
) -> AppResult<(String, UserRow)> {
    device_of_kind(state, method, uri, headers, body, DeviceKind::Server, "Only a server pushes.").await
}

fn parse<T: DeserializeOwned>(body: &[u8], what: &str) -> AppResult<T> {
    serde_json::from_slice(body).map_err(|_| AppError::bad_request(format!("That isn't {what}.")))
}

/// A token APNs or FCM gave, which goes into a URL path and a JSON body.
fn is_push_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= MAX_TOKEN_CHARS
        && token.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ':'))
}

fn is_bundle_id(topic: &str) -> bool {
    !topic.is_empty()
        && topic.len() <= MAX_TOPIC_CHARS
        && topic.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
}

async fn phones_by_device(state: &AppState, user: &UserRow) -> AppResult<HashMap<String, Phone>> {
    let phones = db::phones_of(&state.db, user.id).await?;
    Ok(phones.into_iter().map(|phone| (phone.device.clone(), phone)).collect())
}

/// Has the auth server push to the client that asks, in place of where it pushed before.
pub async fn set_push(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Json<Value>> {
    let device = phone(&state, &method, &uri, &headers, &body).await?;
    let mut registration: PushRegistration = parse(&body, "a push registration")?;
    registration.live_start_token = registration.live_start_token.filter(|token| !token.is_empty());
    registration.live_token = registration.live_token.filter(|token| !token.is_empty());
    let tokens = [Some(&registration.token), registration.live_start_token.as_ref(), registration.live_token.as_ref()];
    if !tokens.into_iter().flatten().all(|token| is_push_token(token)) {
        return Err(AppError::bad_request("That isn't a push token."));
    }
    if let PushService::Apns { topic, .. } = &registration.service
        && !is_bundle_id(topic)
    {
        return Err(AppError::bad_request("That isn't an app's bundle identifier."));
    }
    db::set_push(&state.db, &device, &registration).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove_push(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    let device = phone(&state, &method, &uri, &headers, b"").await?;
    db::remove_push(&state.db, &device).await?;
    Ok(Json(json!({ "ok": true })))
}

/// The phones a server seals what it pushes for: those of its own account.
pub async fn push_targets(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> AppResult<Json<Vec<PushTarget>>> {
    let (_, user) = pushing_server(&state, &method, &uri, &headers, b"").await?;
    let phones = db::phones_of(&state.db, user.id).await?;
    let targets = phones.into_iter().map(|phone| PushTarget {
        device: phone.device,
        choices: phone.registration.choices,
        live: phone.registration.live,
    });
    Ok(Json(targets.collect()))
}

/// Pushes each alert to its phone if the phone is the server's account's and wants that kind;
/// skips the others.
pub async fn push_alerts(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Json<Value>> {
    let (_, user) = pushing_server(&state, &method, &uri, &headers, &body).await?;
    let alerts: Vec<SealedAlert> = parse(&body, "a list of alerts")?;
    let phones = phones_by_device(&state, &user).await?;
    let deliveries: Vec<(Phone, SealedAlert)> = alerts
        .into_iter()
        .filter(|alert| alert.sealed.len() <= MAX_SEALED_CHARS && alert.thread_id.len() <= MAX_THREAD_ID_CHARS)
        .filter_map(|alert| {
            let phone = phones.get(&alert.device)?;
            phone.registration.choices.allows(alert.kind).then(|| (phone.clone(), alert))
        })
        .collect();
    if !deliveries.is_empty() {
        let state = state.clone();
        tokio::spawn(async move {
            for (phone, alert) in deliveries {
                state.push.alert(&state.db, &phone, &alert).await;
            }
        });
    }
    Ok(Json(json!({ "ok": true })))
}

/// Keeps what the server's agents are doing for each of the account's phones it was sealed for,
/// and pushes each phone that shows it what every server of the account last said.
pub async fn push_work(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Json<Value>> {
    let (server, user) = pushing_server(&state, &method, &uri, &headers, &body).await?;
    let work: PushWork = parse(&body, "what the agents are doing")?;
    let phones = phones_by_device(&state, &user).await?;
    let mut deliveries = Vec::new();
    for part in work.sealed {
        let Some(phone) = phones.get(&part.device) else { continue };
        if part.sealed.len() > MAX_SEALED_CHARS {
            continue;
        }
        let before = db::fresh_work(&state.db, &phone.device, user.id).await?;
        db::store_work(&state.db, &phone.device, &server, work.working, work.waiting, &part.sealed).await?;
        if !phone.registration.live {
            continue;
        }
        let parts = db::fresh_work(&state.db, &phone.device, user.id).await?;
        deliveries.push((phone.clone(), parts, before.iter().map(WorkPart::active).sum::<i64>()));
    }
    if !deliveries.is_empty() {
        let state = state.clone();
        tokio::spawn(async move {
            for (phone, parts, before) in deliveries {
                state.push.work(&state.db, &phone, &parts, before).await;
            }
        });
    }
    Ok(Json(json!({ "ok": true })))
}
