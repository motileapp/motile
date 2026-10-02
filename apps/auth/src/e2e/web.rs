//! The web app's side: its server starts a sign-in that ends at its own callback, opens a
//! session with the code, and calls the API with the session's token.

use std::collections::HashMap;

use motile_protocol::auth_api::{EnrollToken, Me, Session};
use motile_protocol::identity::{DeviceKey, random_token, sha256_hex};
use reqwest::header::LOCATION;
use reqwest::{Method, StatusCode, Url};
use serde_json::json;
use sqlx::PgPool;

use super::{ANN, Auth, BOB, GoogleAccount, LINUX, MAC, WEB_URL, emails, names};

fn web_sign_in_url(auth: &Auth, verifier: &str) -> String {
    let challenge = sha256_hex(verifier.as_bytes());
    format!("{}/auth/start?challenge={challenge}&state=web-state&redirect={WEB_URL}/auth/callback", auth.base)
}

/// The code the web app's callback is handed after `account` signed in.
async fn web_code(auth: &Auth, verifier: &str, account: &GoogleAccount) -> String {
    let response = auth.through_google_from(&web_sign_in_url(auth, verifier), account).await;
    let back = Url::parse(response.headers()[LOCATION].to_str().unwrap()).unwrap();
    assert_eq!(format!("{}{}", back.origin().ascii_serialization(), back.path()), format!("{WEB_URL}/auth/callback"));
    let query: HashMap<String, String> = back.query_pairs().into_owned().collect();
    assert_eq!(query["state"], "web-state");
    query["code"].clone()
}

async fn open_session(auth: &Auth, code: &str, verifier: &str) -> reqwest::Response {
    let body = json!({ "code": code, "verifier": verifier });
    auth.http.post(format!("{}/api/sessions", auth.base)).json(&body).send().await.unwrap()
}

/// The token of a new session of `account`.
async fn session(auth: &Auth, account: &GoogleAccount) -> String {
    let verifier = random_token();
    let code = web_code(auth, &verifier, account).await;
    let response = open_session(auth, &code, &verifier).await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Session>().await.unwrap().token
}

async fn call(auth: &Auth, method: Method, path: &str, token: &str) -> reqwest::Response {
    auth.http.request(method, format!("{}{path}", auth.base)).bearer_auth(token).send().await.unwrap()
}

async fn me(auth: &Auth, token: &str) -> Me {
    call(auth, Method::GET, "/api/me", token).await.json().await.unwrap()
}

#[sqlx::test]
async fn signing_in_on_the_web_opens_a_session_that_sees_the_account(db: PgPool) {
    let auth = Auth::start(db).await;
    auth.app(&ANN).await;

    let token = session(&auth, &ANN).await;
    let account = me(&auth, &token).await;

    assert_eq!(emails(&account), Some("ann@example.com"));
    assert_eq!(names(&account), vec!["Ann's Mac"]);
}

#[sqlx::test]
async fn a_web_code_works_once_and_only_with_the_secret_that_started_the_sign_in(db: PgPool) {
    let auth = Auth::start(db).await;
    let verifier = random_token();

    let code = web_code(&auth, &verifier, &ANN).await;
    assert_eq!(open_session(&auth, &code, "a-guess").await.status(), StatusCode::UNAUTHORIZED);
    // The failed attempt used the code up.
    assert_eq!(open_session(&auth, &code, &verifier).await.status(), StatusCode::BAD_REQUEST);

    let code = web_code(&auth, &verifier, &ANN).await;
    assert_eq!(open_session(&auth, &code, &verifier).await.status(), StatusCode::OK);
    assert_eq!(open_session(&auth, &code, &verifier).await.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn a_web_sign_in_never_links_a_device_and_an_apps_never_opens_a_session(db: PgPool) {
    let auth = Auth::start(db).await;
    let key = DeviceKey::generate();
    let verifier = random_token();

    let web = web_code(&auth, &verifier, &ANN).await;
    assert!(auth.client.exchange(&key, &web, &verifier, &MAC).await.is_err());
    assert_eq!(auth.client.me(&key).await.unwrap().user, None);

    let app = auth.sign_in_code(&verifier, &ANN).await;
    assert_eq!(open_session(&auth, &app, &verifier).await.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn the_web_code_is_only_sent_to_the_web_app_this_server_was_given(db: PgPool) {
    let auth = Auth::start_with(db, |config| config.web_url = None).await;

    let refused = auth.get(&web_sign_in_url(&auth, "verifier")).await;

    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn a_web_sign_in_google_did_not_complete_goes_back_to_the_web_app_without_a_code(db: PgPool) {
    let auth = Auth::start(db).await;
    let google = auth.open(&web_sign_in_url(&auth, &random_token())).await;
    let query: HashMap<String, String> = google.query_pairs().into_owned().collect();

    let denied =
        auth.get(&format!("{}/auth/google/callback?error=access_denied&state={}", auth.base, query["state"])).await;

    assert_eq!(denied.headers()[LOCATION], format!("{WEB_URL}/auth/callback?error=access_denied&state=web-state"));
}

#[sqlx::test]
async fn a_session_token_is_stored_hashed_and_stops_working_once_expired_or_signed_out(db: PgPool) {
    let auth = Auth::start(db).await;
    let token = session(&auth, &ANN).await;
    let stored: Vec<String> = sqlx::query_scalar("SELECT token_hash FROM sessions").fetch_all(&auth.db).await.unwrap();
    assert_eq!(stored, vec![sha256_hex(token.as_bytes())]);

    let signed_out = call(&auth, Method::DELETE, "/api/sessions/current", &token).await;
    assert_eq!(signed_out.status(), StatusCode::OK);
    assert_eq!(call(&auth, Method::GET, "/api/me", &token).await.status(), StatusCode::UNAUTHORIZED);

    let token = session(&auth, &ANN).await;
    sqlx::query("UPDATE sessions SET expires_at = now() - interval '1 second'").execute(&auth.db).await.unwrap();
    assert_eq!(call(&auth, Method::GET, "/api/me", &token).await.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(call(&auth, Method::GET, "/api/me", "made-up").await.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn a_session_adds_hosts_and_removes_devices_of_its_own_account_only(db: PgPool) {
    let auth = Auth::start(db).await;
    let ann = session(&auth, &ANN).await;
    let bob = session(&auth, &BOB).await;
    let host = DeviceKey::generate();

    let token: EnrollToken = call(&auth, Method::POST, "/api/enroll-tokens", &ann).await.json().await.unwrap();
    assert_eq!(token.command, format!("curl -fsSL {}/install | sh -s -- {}", auth.base, token.token));
    auth.client.enroll(&host, &token.token, &LINUX).await.unwrap();
    assert_eq!(names(&me(&auth, &ann).await), vec!["build-box"]);

    let path = format!("/api/devices/{}", host.public());
    assert_eq!(call(&auth, Method::DELETE, &path, &bob).await.status(), StatusCode::NOT_FOUND);
    assert_eq!(me(&auth, &ann).await.devices.len(), 1);

    assert_eq!(call(&auth, Method::DELETE, &path, &ann).await.status(), StatusCode::OK);
    assert_eq!(me(&auth, &ann).await.devices, vec![]);
    assert_eq!(auth.client.me(&host).await.unwrap().user, None);
}

#[sqlx::test]
async fn the_dev_login_opens_a_web_session_without_google(db: PgPool) {
    let auth = Auth::start(db).await;
    let verifier = random_token();
    let body = json!({ "challenge": sha256_hex(verifier.as_bytes()), "email": "dev@example.com", "web": true });

    let login = auth.http.post(format!("{}/api/dev/login", auth.base)).json(&body).send().await.unwrap();
    let code = login.json::<serde_json::Value>().await.unwrap()["code"].as_str().unwrap().to_string();
    let opened: Session = open_session(&auth, &code, &verifier).await.json().await.unwrap();

    assert_eq!(emails(&me(&auth, &opened.token).await), Some("dev@example.com"));
}
