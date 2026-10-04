//! The browser's side of a sign-in. A client or the web app opens `/auth/start`, Google signs the
//! person in, and the browser is sent back to where it came from with a one-time code. Only
//! whoever started the sign-in can use the code: it must present the secret behind the challenge
//! it started with.

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use motile_protocol::APP_REDIRECT;
use motile_protocol::identity::{random_token, sha256_hex};
use reqwest::Url;

use crate::config::Config;
use crate::db::NewSignIn;
use crate::{AppState, db, google, pages};

fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64 && text.chars().all(|character| character.is_ascii_hexdigit())
}

/// Sends the browser back to whoever started the sign-in: the client, or the web app.
fn back(config: &Config, web: bool, parameters: &[(&str, &str)]) -> Response {
    let target = match web {
        true => config.web_redirect(),
        false => Some(APP_REDIRECT.to_string()),
    };
    let Some(url) = target.and_then(|target| Url::parse_with_params(&target, parameters).ok()) else {
        return failed("This sign-in can't be finished. Start again.");
    };
    Redirect::to(url.as_str()).into_response()
}

fn failed(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, pages::message("Sign-in failed", message)).into_response()
}

pub async fn start(State(state): State<AppState>, Query(query): Query<HashMap<String, String>>) -> Response {
    let text = |key: &str| query.get(key).map(String::as_str).unwrap_or_default();
    let (challenge, app_state, redirect) = (text("challenge"), text("state"), text("redirect"));
    // The code is only ever sent to the client's own address or to the web app.
    let web = state.config.web_redirect().is_some_and(|web_redirect| web_redirect == redirect);
    if redirect != APP_REDIRECT && !web {
        return failed("This sign-in link doesn't lead back to Motile.");
    }
    if !is_sha256_hex(challenge) || app_state.is_empty() || app_state.len() > 200 {
        return failed("This sign-in link is malformed. Start again.");
    }

    let id = random_token();
    let google_verifier = random_token();
    let nonce = random_token();
    let sign_in = NewSignIn { id: &id, challenge, app_state, google_verifier: &google_verifier, nonce: &nonce, web };
    if let Err(error) = db::create_sign_in(&state.db, &sign_in).await {
        tracing::error!("couldn't save a sign-in: {error}");
        return failed("Something went wrong. Try again in a moment.");
    }
    Redirect::to(&google::authorize_url(&state.config, &id, &nonce, &google_verifier)).into_response()
}

pub async fn google_callback(State(state): State<AppState>, Query(query): Query<HashMap<String, String>>) -> Response {
    let text = |key: &str| query.get(key).map(String::as_str).unwrap_or_default();
    let sign_in = match db::pending_sign_in(&state.db, text("state")).await {
        Ok(Some(sign_in)) => sign_in,
        Ok(None) => return failed("This sign-in has expired. Start again."),
        Err(error) => {
            tracing::error!("couldn't load a sign-in: {error}");
            return failed("Something went wrong. Try again in a moment.");
        }
    };
    if !text("error").is_empty() || text("code").is_empty() {
        return back(&state.config, sign_in.web, &[("error", "access_denied"), ("state", &sign_in.app_state)]);
    }

    let exchanged =
        google::exchange(&state.http, &state.config, text("code"), &sign_in.google_verifier, &sign_in.nonce).await;
    let identity = match exchanged {
        Ok(identity) if identity.email_verified => identity,
        Ok(_) => return failed("Google hasn't verified this email address."),
        Err(error) => {
            tracing::warn!("Google sign-in failed: {error}");
            return failed("Google couldn't confirm the sign-in. Start again.");
        }
    };

    let code = random_token();
    let saved = async {
        let user = db::upsert_user(&state.db, &identity).await?;
        db::complete_sign_in(&state.db, &sign_in.id, user.id, &sha256_hex(code.as_bytes())).await
    };
    if let Err(error) = saved.await {
        tracing::error!("couldn't complete a sign-in: {error}");
        return failed("Something went wrong. Try again in a moment.");
    }
    back(&state.config, sign_in.web, &[("code", &code), ("state", &sign_in.app_state)])
}
