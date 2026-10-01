use std::collections::HashMap;

use motile_protocol::identity::{DeviceKey, random_token, sha256_hex};
use reqwest::StatusCode;
use reqwest::header::LOCATION;
use sqlx::PgPool;

use super::{ANN, Auth, BOB, GOOGLE_CLIENT_ID, GoogleAccount, MAC, emails, names};

#[sqlx::test]
async fn signing_in_with_google_links_the_app_that_started_it(db: PgPool) {
    let auth = Auth::start(db).await;
    let key = DeviceKey::generate();
    let verifier = random_token();

    let google = auth.open_sign_in(&verifier, "app-state").await;
    let query: HashMap<String, String> = google.query_pairs().into_owned().collect();
    assert_eq!(google.host_str(), Some("accounts.google.com"));
    assert_eq!(query["client_id"], GOOGLE_CLIENT_ID);
    assert_eq!(query["redirect_uri"], format!("{}/auth/google/callback", auth.base));
    assert_eq!(query["code_challenge_method"], "S256");

    let code = auth.sign_in_code(&verifier, &ANN).await;
    let me = auth.client.exchange(&key, &code, &verifier, &MAC).await.unwrap();

    assert_eq!(emails(&me), Some("ann@example.com"));
    assert_eq!(names(&me), vec!["Ann's Mac"]);
    assert_eq!(me.devices[0].public_key, key.public());
    assert_eq!(auth.client.me(&key).await.unwrap(), me);
}

#[sqlx::test]
async fn a_code_works_once_and_only_with_the_secret_that_started_the_sign_in(db: PgPool) {
    let auth = Auth::start(db).await;
    let key = DeviceKey::generate();
    let verifier = random_token();

    let code = auth.sign_in_code(&verifier, &ANN).await;
    let stolen = auth.client.exchange(&DeviceKey::generate(), &code, "a-guess", &MAC).await;
    assert_eq!(stolen.unwrap_err().to_string(), "This sign-in was started by another app.");

    // The failed attempt used the code up.
    let late = auth.client.exchange(&key, &code, &verifier, &MAC).await;
    assert_eq!(late.unwrap_err().to_string(), "This sign-in has expired. Sign in again.");
    assert_eq!(auth.client.me(&key).await.unwrap().user, None);

    let code = auth.sign_in_code(&verifier, &ANN).await;
    auth.client.exchange(&key, &code, &verifier, &MAC).await.unwrap();
    let again = auth.client.exchange(&key, &code, &verifier, &MAC).await;
    assert!(again.is_err(), "a used code doesn't work again");
}

#[sqlx::test]
async fn the_code_is_only_ever_sent_to_the_app(db: PgPool) {
    let auth = Auth::start(db).await;
    let challenge = sha256_hex(b"verifier");

    for redirect in ["https://evil.example.com", "motile://auth.evil", ""] {
        let url = format!("{}/auth/start?challenge={challenge}&state=s&redirect={redirect}", auth.base);
        assert_eq!(auth.get(&url).await.status(), StatusCode::BAD_REQUEST, "{redirect}");
    }
    let malformed = format!("{}/auth/start?challenge=short&state=s&redirect=motile://auth", auth.base);
    assert_eq!(auth.get(&malformed).await.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn a_sign_in_google_did_not_complete_goes_back_to_the_app_without_a_code(db: PgPool) {
    let auth = Auth::start(db).await;
    let google = auth.open_sign_in(&random_token(), "app-state").await;
    let query: HashMap<String, String> = google.query_pairs().into_owned().collect();

    let denied =
        auth.get(&format!("{}/auth/google/callback?error=access_denied&state={}", auth.base, query["state"])).await;
    assert_eq!(denied.headers()[LOCATION], "motile://auth?error=access_denied&state=app-state");

    let unknown = auth.get(&format!("{}/auth/google/callback?code=x&state=made-up", auth.base)).await;
    assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn an_unverified_google_address_is_turned_away(db: PgPool) {
    let auth = Auth::start(db).await;
    let unverified = GoogleAccount { subject: "google-eve", email: "eve@example.com", email_verified: false };

    let response = auth.through_google(&random_token(), "app-state", &unverified).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(response.text().await.unwrap().contains("verified this email address"));
}

#[sqlx::test]
async fn signing_in_again_keeps_the_account_and_another_person_gets_their_own(db: PgPool) {
    let auth = Auth::start(db).await;
    let (first, _) = auth.app(&ANN).await;
    let (_, second) = auth.app(&ANN).await;
    let (_, other) = auth.app(&BOB).await;

    assert_eq!(second.devices.len(), 2);
    assert!(second.devices.iter().any(|device| device.public_key == first.public()));
    assert_eq!(emails(&other), Some("bob@example.com"));
    assert_eq!(other.devices.len(), 1);
}

#[sqlx::test]
async fn the_dev_login_only_exists_when_it_is_switched_on(db: PgPool) {
    let auth = Auth::start_with(db, false).await;
    let refused = auth.client.dev_login(&random_token(), "dev@example.com").await;
    assert!(refused.is_err());
}

#[sqlx::test]
async fn the_dev_login_signs_in_without_google(db: PgPool) {
    let auth = Auth::start(db).await;
    let key = DeviceKey::generate();
    let verifier = random_token();

    let code = auth.client.dev_login(&verifier, "Dev@Example.com").await.unwrap();
    let me = auth.client.exchange(&key, &code, &verifier, &MAC).await.unwrap();

    assert_eq!(emails(&me), Some("dev@example.com"));
}

#[sqlx::test]
async fn the_installer_downloads_from_this_server_and_downloads_lead_to_the_release(db: PgPool) {
    let auth = Auth::start(db).await;

    let script = auth.get(&format!("{}/install", auth.base)).await.text().await.unwrap();
    assert!(script.starts_with("#!/bin/sh"));
    assert!(script.contains(&format!("MOTILE_URL=\"${{MOTILE_URL:-{}}}\"", auth.base)));

    let download = auth.get(&format!("{}/download/motile-x86_64-unknown-linux-musl.tar.gz", auth.base)).await;
    assert_eq!(
        download.headers()[LOCATION],
        "https://releases.example.com/latest/motile-x86_64-unknown-linux-musl.tar.gz"
    );
    let odd = auth.get(&format!("{}/download/..%2Fsecret", auth.base)).await;
    assert_eq!(odd.status(), StatusCode::NOT_FOUND);
}
