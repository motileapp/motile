//! What the apps and the hosts call. A linked device signs its requests with its key; the
//! requests that link one prove the key with a signature over what they redeem.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use motile_protocol::auth_api::{
    DevLoginRequest, DevLoginResponse, DeviceKind, EnrollRequest, EnrollResponse, EnrollToken, ExchangeRequest, Me,
    enroll_message, link_message,
};
use motile_protocol::identity::{is_public_key, random_token, sha256_hex, verify, verify_request};
use motile_protocol::now;
use serde_json::{Value, json};

use crate::db::{NewSignIn, UserRow};
use crate::error::{AppError, AppResult};
use crate::google::Identity;
use crate::{AppState, db};

const MAX_NAME_CHARS: usize = 100;

fn clean(text: &str, fallback: &str) -> String {
    let cleaned: String =
        text.trim().chars().filter(|character| !character.is_control()).take(MAX_NAME_CHARS).collect();
    if cleaned.is_empty() { fallback.to_string() } else { cleaned }
}

/// The public key that signed the request.
fn signer(method: &Method, uri: &Uri, headers: &HeaderMap, body: &[u8]) -> AppResult<String> {
    let header = headers.get("authorization").and_then(|value| value.to_str().ok()).unwrap_or_default();
    verify_request(header, method.as_str(), uri.path(), body, now())
        .map_err(|error| AppError::unauthorized(error.to_string()))
}

async fn linked_user(state: &AppState, public_key: &str) -> AppResult<UserRow> {
    let user = db::user_of_device(&state.db, public_key).await?;
    user.ok_or_else(|| AppError::unauthorized("This device isn't linked to an account."))
}

pub async fn exchange(State(state): State<AppState>, Json(request): Json<ExchangeRequest>) -> AppResult<Json<Me>> {
    if !is_public_key(&request.public_key) {
        return Err(AppError::bad_request("That isn't a device key."));
    }
    if !verify(&request.public_key, &link_message(&request.code), &request.signature) {
        return Err(AppError::unauthorized("The device's signature doesn't match."));
    }
    let expired = || AppError::bad_request("This sign-in has expired. Sign in again.");
    let sign_in = db::take_sign_in(&state.db, &sha256_hex(request.code.as_bytes())).await?.ok_or_else(expired)?;
    if sha256_hex(request.verifier.as_bytes()) != sign_in.challenge {
        return Err(AppError::unauthorized("This sign-in was started by another app."));
    }
    let user_id = sign_in.user_id.ok_or_else(expired)?;
    let user = db::user_by_id(&state.db, user_id).await?.ok_or_else(expired)?;

    let name = clean(&request.name, "Motile app");
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
    let sign_in =
        NewSignIn { id: &id, challenge: &request.challenge, app_state: "dev", google_verifier: "", nonce: "" };
    db::create_sign_in(&state.db, &sign_in).await?;
    db::complete_sign_in(&state.db, &id, user.id, &sha256_hex(code.as_bytes())).await?;
    Ok(Json(DevLoginResponse { code }))
}

pub async fn me(State(state): State<AppState>, method: Method, uri: Uri, headers: HeaderMap) -> AppResult<Json<Me>> {
    let public_key = signer(&method, &uri, &headers, b"")?;
    let Some(user) = db::user_of_device(&state.db, &public_key).await? else {
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
    let public_key = signer(&method, &uri, &headers, &body)?;
    let user = linked_user(&state, &public_key).await?;
    // A host can't add more hosts; only an app someone signed in to can.
    if db::device_kind(&state.db, &public_key).await?.as_deref() != Some(db::kind_text(DeviceKind::Client)) {
        return Err(AppError::new(StatusCode::FORBIDDEN, "Hosts are added from the app."));
    }
    let token = random_token();
    let expires_at = db::create_enroll_token(&state.db, user.id, &sha256_hex(token.as_bytes())).await?;
    Ok(Json(EnrollToken {
        command: state.config.install_command(&token),
        token,
        expires_at: expires_at.timestamp_millis() as f64 / 1000.0,
    }))
}

pub async fn enroll(
    State(state): State<AppState>,
    Json(request): Json<EnrollRequest>,
) -> AppResult<Json<EnrollResponse>> {
    if !is_public_key(&request.public_key) {
        return Err(AppError::bad_request("That isn't a device key."));
    }
    if !verify(&request.public_key, &enroll_message(&request.token), &request.signature) {
        return Err(AppError::unauthorized("The device's signature doesn't match."));
    }
    let token_hash = sha256_hex(request.token.as_bytes());
    let expired = || {
        AppError::bad_request(
            "This install command has expired or was used on another machine. Copy a new one from the app.",
        )
    };
    let user_id = db::use_enroll_token(&state.db, &token_hash, &request.public_key).await?.ok_or_else(expired)?;
    let user = db::user_by_id(&state.db, user_id).await?.ok_or_else(expired)?;

    let name = clean(&request.name, "Host");
    let platform = clean(&request.platform, "unknown");
    if !db::link_device(&state.db, user.id, &request.public_key, DeviceKind::Host, &name, &platform).await? {
        return Err(AppError::new(StatusCode::CONFLICT, "This machine can't be linked to the account."));
    }
    Ok(Json(EnrollResponse { email: user.email }))
}

/// Removes one of the account's devices; `self` is the caller.
pub async fn remove_device(
    State(state): State<AppState>,
    Path(target): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    let public_key = signer(&method, &uri, &headers, b"")?;
    let user = linked_user(&state, &public_key).await?;
    let target = if target == "self" { public_key } else { target };
    if !db::remove_device(&state.db, user.id, &target).await? {
        return Err(AppError::not_found());
    }
    Ok(Json(json!({ "ok": true })))
}
