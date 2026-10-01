use motile_protocol::auth_api::DeviceKind;
use motile_protocol::identity::{DeviceKey, sign_request};
use motile_protocol::now;
use reqwest::StatusCode;
use sqlx::PgPool;

use super::{ANN, Auth, BOB, LINUX, emails, names};

#[sqlx::test]
async fn the_install_command_links_one_host_to_the_account(db: PgPool) {
    let auth = Auth::start(db).await;
    let (app, _) = auth.app(&ANN).await;
    let host = DeviceKey::generate();

    let token = auth.client.create_enroll_token(&app).await.unwrap();
    assert_eq!(token.command, format!("curl -fsSL {}/install | sh -s -- {}", auth.base, token.token));
    assert!(token.expires_at > now());
    let enrolled = auth.client.enroll(&host, &token.token, &LINUX).await.unwrap();
    assert_eq!(enrolled.email, "ann@example.com");

    let me = auth.client.me(&app).await.unwrap();
    assert_eq!(names(&me), vec!["Ann's Mac", "build-box"]);
    assert_eq!(me.devices[1].kind, DeviceKind::Host);
    // The host sees the same account, which is how it learns which apps to accept.
    assert_eq!(auth.client.me(&host).await.unwrap(), me);

    // Running the same command again on that machine is fine; on another machine it isn't.
    auth.client.enroll(&host, &token.token, &LINUX).await.unwrap();
    let other = auth.client.enroll(&DeviceKey::generate(), &token.token, &LINUX).await;
    assert!(other.unwrap_err().to_string().contains("expired or was used on another machine"));
}

#[sqlx::test]
async fn an_expired_or_made_up_token_links_nothing(db: PgPool) {
    let auth = Auth::start(db).await;
    let (app, _) = auth.app(&ANN).await;
    let token = auth.client.create_enroll_token(&app).await.unwrap();
    sqlx::query("UPDATE enroll_tokens SET expires_at = now() - interval '1 second'").execute(&auth.db).await.unwrap();

    assert!(auth.client.enroll(&DeviceKey::generate(), &token.token, &LINUX).await.is_err());
    assert!(auth.client.enroll(&DeviceKey::generate(), "made-up", &LINUX).await.is_err());
    assert_eq!(auth.client.me(&app).await.unwrap().devices.len(), 1);
}

#[sqlx::test]
async fn only_a_signed_in_app_can_add_hosts(db: PgPool) {
    let auth = Auth::start(db).await;
    let (app, _) = auth.app(&ANN).await;
    let host = DeviceKey::generate();
    let token = auth.client.create_enroll_token(&app).await.unwrap();
    auth.client.enroll(&host, &token.token, &LINUX).await.unwrap();

    let from_host = auth.client.create_enroll_token(&host).await;
    assert_eq!(from_host.unwrap_err().to_string(), "Hosts are added from the app.");
    let from_stranger = auth.client.create_enroll_token(&DeviceKey::generate()).await;
    assert_eq!(from_stranger.unwrap_err().to_string(), "This device isn't linked to an account.");
}

#[sqlx::test]
async fn devices_are_removed_by_their_own_account_only(db: PgPool) {
    let auth = Auth::start(db).await;
    let (ann, _) = auth.app(&ANN).await;
    let (bob, _) = auth.app(&BOB).await;
    let host = DeviceKey::generate();
    let token = auth.client.create_enroll_token(&ann).await.unwrap();
    auth.client.enroll(&host, &token.token, &LINUX).await.unwrap();

    assert!(auth.client.remove_device(&bob, &host.public()).await.is_err());
    assert_eq!(auth.client.me(&ann).await.unwrap().devices.len(), 2);

    auth.client.remove_device(&ann, &host.public()).await.unwrap();
    assert_eq!(names(&auth.client.me(&ann).await.unwrap()), vec!["Ann's Mac"]);
    assert_eq!(auth.client.me(&host).await.unwrap().user, None);

    // Signing out removes the app itself.
    auth.client.remove_device(&ann, "self").await.unwrap();
    assert_eq!(emails(&auth.client.me(&ann).await.unwrap()), None);
}

#[sqlx::test]
async fn a_request_is_accepted_only_as_its_device_signed_it(db: PgPool) {
    let auth = Auth::start(db).await;
    let (app, _) = auth.app(&ANN).await;
    let http = reqwest::Client::new();
    let get_me =
        |authorization: String| http.get(format!("{}/api/me", auth.base)).header("authorization", authorization).send();

    let good = sign_request(&app, "GET", "/api/me", b"", now());
    assert_eq!(get_me(good).await.unwrap().status(), StatusCode::OK);

    let for_another_path = sign_request(&app, "GET", "/api/enroll-tokens", b"", now());
    assert_eq!(get_me(for_another_path).await.unwrap().status(), StatusCode::UNAUTHORIZED);
    let stale = sign_request(&app, "GET", "/api/me", b"", now() - 3600.0);
    assert_eq!(get_me(stale).await.unwrap().status(), StatusCode::UNAUTHORIZED);
    assert_eq!(get_me("Motile nonsense".to_string()).await.unwrap().status(), StatusCode::UNAUTHORIZED);
}
