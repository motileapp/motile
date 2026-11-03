use motile_protocol::auth_api::{DeviceKind, enroll_message};
use motile_protocol::auth_client::DeviceDescription;
use motile_protocol::identity::{DeviceKey, sign_request};
use motile_protocol::now;
use reqwest::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use super::{ANN, Auth, BOB, INSTALL_URL, LINUX, emails, names};
use crate::limits::MAX_ENROLL_FAILURES;

#[sqlx::test]
async fn the_install_command_links_one_server_to_the_account(db: PgPool) {
    let auth = Auth::start(db).await;
    let (client, _) = auth.new_client(&ANN).await;
    let server = DeviceKey::generate();

    let token = auth.client.create_enroll_token(&client).await.unwrap();
    let command = format!("curl -fsSL {INSTALL_URL} | MOTILE_AUTH_URL={} sh -s -- {}", auth.base, token.token);
    assert_eq!(token.command, command);
    assert_eq!(token.token.len(), 8);
    assert!(token.token.chars().all(|character| character.is_ascii_uppercase() || character.is_ascii_digit()));
    assert!(token.expires_at > now() + 14.0 * 60.0 && token.expires_at <= now() + 15.0 * 60.0);
    // Typed by hand, the code may come in small letters.
    let enrolled = auth.client.enroll(&server, &token.token.to_lowercase(), &LINUX).await.unwrap();
    assert_eq!(enrolled.email, "ann@example.com");

    let me = auth.client.me(&client).await.unwrap();
    assert_eq!(names(&me), vec!["Ann's Mac", "build-box"]);
    assert_eq!(me.devices[1].kind, DeviceKind::Server);
    // The server sees the same account, which is how it learns which clients to accept.
    assert_eq!(auth.client.me(&server).await.unwrap(), me);

    // Running the same command again on that machine is fine; on another machine it isn't.
    auth.client.enroll(&server, &token.token, &LINUX).await.unwrap();
    let other = auth.client.enroll(&DeviceKey::generate(), &token.token, &LINUX).await;
    assert!(other.unwrap_err().to_string().contains("expired or was used on another machine"));
}

#[sqlx::test]
async fn an_expired_or_made_up_token_links_nothing(db: PgPool) {
    let auth = Auth::start(db).await;
    let (client, _) = auth.new_client(&ANN).await;
    let token = auth.client.create_enroll_token(&client).await.unwrap();
    sqlx::query("UPDATE enroll_tokens SET expires_at = now() - interval '1 second'").execute(&auth.db).await.unwrap();

    assert!(auth.client.enroll(&DeviceKey::generate(), &token.token, &LINUX).await.is_err());
    assert!(auth.client.enroll(&DeviceKey::generate(), "made-up", &LINUX).await.is_err());
    assert_eq!(auth.client.me(&client).await.unwrap().devices.len(), 1);
}

#[sqlx::test]
async fn an_address_that_guesses_wrong_too_often_is_refused(db: PgPool) {
    let auth = Auth::start(db).await;
    let (client, _) = auth.new_client(&ANN).await;
    let token = auth.client.create_enroll_token(&client).await.unwrap();

    for _ in 0..MAX_ENROLL_FAILURES {
        let wrong = auth.client.enroll(&DeviceKey::generate(), "WRONG000", &LINUX).await;
        assert!(wrong.unwrap_err().to_string().contains("expired or was used on another machine"));
    }
    let right = auth.client.enroll(&DeviceKey::generate(), &token.token, &LINUX).await;
    assert!(right.unwrap_err().to_string().contains("Too many install commands failed"));
    assert_eq!(auth.client.me(&client).await.unwrap().devices.len(), 1);
}

#[sqlx::test]
async fn only_a_signed_in_client_can_add_servers(db: PgPool) {
    let auth = Auth::start(db).await;
    let (client, _) = auth.new_client(&ANN).await;
    let server = DeviceKey::generate();
    let token = auth.client.create_enroll_token(&client).await.unwrap();
    auth.client.enroll(&server, &token.token, &LINUX).await.unwrap();

    let from_server = auth.client.create_enroll_token(&server).await;
    assert_eq!(from_server.unwrap_err().to_string(), "Servers are added from a client.");
    let from_stranger = auth.client.create_enroll_token(&DeviceKey::generate()).await;
    assert_eq!(from_stranger.unwrap_err().to_string(), "This device isn't linked to an account.");
}

#[sqlx::test]
async fn devices_are_removed_by_their_own_account_only(db: PgPool) {
    let auth = Auth::start(db).await;
    let (ann, _) = auth.new_client(&ANN).await;
    let (bob, _) = auth.new_client(&BOB).await;
    let server = DeviceKey::generate();
    let token = auth.client.create_enroll_token(&ann).await.unwrap();
    auth.client.enroll(&server, &token.token, &LINUX).await.unwrap();

    assert!(auth.client.remove_device(&bob, &server.public()).await.is_err());
    assert_eq!(auth.client.me(&ann).await.unwrap().devices.len(), 2);

    auth.client.remove_device(&ann, &server.public()).await.unwrap();
    assert_eq!(names(&auth.client.me(&ann).await.unwrap()), vec!["Ann's Mac"]);
    assert_eq!(auth.client.me(&server).await.unwrap().user, None);

    // Signing out removes the client itself.
    auth.client.remove_device(&ann, "self").await.unwrap();
    assert_eq!(emails(&auth.client.me(&ann).await.unwrap()), None);
}

fn removed_names(me: &motile_protocol::auth_api::Me) -> Vec<&str> {
    me.removed.iter().map(|server| server.name.as_str()).collect()
}

#[sqlx::test]
async fn a_removed_server_is_remembered_by_name_until_it_links_again(db: PgPool) {
    let auth = Auth::start(db).await;
    let (ann, _) = auth.new_client(&ANN).await;
    let server = DeviceKey::generate();
    let token = auth.client.create_enroll_token(&ann).await.unwrap();
    auth.client.enroll(&server, &token.token, &LINUX).await.unwrap();

    auth.client.remove_device(&ann, &server.public()).await.unwrap();
    let me = auth.client.me(&ann).await.unwrap();
    assert_eq!(names(&me), vec!["Ann's Mac"]);
    assert_eq!(removed_names(&me), vec!["build-box"]);
    assert_eq!(me.removed[0].public_key, server.public());
    assert!(me.deleted.is_empty());
    // A removed server is no device of the account: it can't see it, nor be counted as linked.
    assert_eq!(auth.client.me(&server).await.unwrap().user, None);

    auth.client.enroll(&server, &token.token, &LINUX).await.unwrap();
    let me = auth.client.me(&ann).await.unwrap();
    assert_eq!(names(&me), vec!["Ann's Mac", "build-box"]);
    assert!(me.removed.is_empty());
}

#[sqlx::test]
async fn a_server_deleted_completely_is_listed_as_deleted_and_can_link_again(db: PgPool) {
    let auth = Auth::start(db).await;
    let (ann, _) = auth.new_client(&ANN).await;
    let (linked, removed) = (DeviceKey::generate(), DeviceKey::generate());
    for server in [&linked, &removed] {
        let token = auth.client.create_enroll_token(&ann).await.unwrap();
        auth.client.enroll(server, &token.token, &LINUX).await.unwrap();
    }
    auth.client.remove_device(&ann, &removed.public()).await.unwrap();

    auth.client.delete_server_completely(&ann, &linked.public()).await.unwrap();
    auth.client.delete_server_completely(&ann, &removed.public()).await.unwrap();
    let me = auth.client.me(&ann).await.unwrap();
    assert_eq!(names(&me), vec!["Ann's Mac"]);
    assert!(me.removed.is_empty());
    let mut deleted = me.deleted.clone();
    deleted.sort();
    let mut expected = vec![linked.public(), removed.public()];
    expected.sort();
    assert_eq!(deleted, expected);
    // Deleting it again changes nothing.
    auth.client.delete_server_completely(&ann, &linked.public()).await.unwrap();

    let token = auth.client.create_enroll_token(&ann).await.unwrap();
    auth.client.enroll(&linked, &token.token, &LINUX).await.unwrap();
    let me = auth.client.me(&ann).await.unwrap();
    assert_eq!(names(&me), vec!["Ann's Mac", "build-box"]);
    assert_eq!(me.deleted, vec![removed.public()]);
}

#[sqlx::test]
async fn only_a_client_of_the_account_deletes_its_servers_completely(db: PgPool) {
    let auth = Auth::start(db).await;
    let (ann, _) = auth.new_client(&ANN).await;
    let (bob, _) = auth.new_client(&BOB).await;
    let (server, other) = (DeviceKey::generate(), DeviceKey::generate());
    for key in [&server, &other] {
        let token = auth.client.create_enroll_token(&ann).await.unwrap();
        auth.client.enroll(key, &token.token, &LINUX).await.unwrap();
    }

    assert!(auth.client.delete_server_completely(&bob, &server.public()).await.is_err());
    let from_server = auth.client.delete_server_completely(&other, &server.public()).await;
    assert_eq!(from_server.unwrap_err().to_string(), "Servers are deleted from a client.");
    // A client is removed, never deleted completely.
    let (second, _) = auth.new_client(&ANN).await;
    assert!(auth.client.delete_server_completely(&ann, &second.public()).await.is_err());
    assert!(auth.client.delete_server_completely(&ann, &DeviceKey::generate().public()).await.is_err());

    let me = auth.client.me(&ann).await.unwrap();
    assert_eq!(me.devices.len(), 4);
    assert!(me.deleted.is_empty());
    assert!(auth.client.me(&bob).await.unwrap().deleted.is_empty());
}

#[sqlx::test]
async fn a_request_is_accepted_only_as_its_device_signed_it(db: PgPool) {
    let auth = Auth::start(db).await;
    let (client, _) = auth.new_client(&ANN).await;
    let http = reqwest::Client::new();
    let get_me =
        |authorization: String| http.get(format!("{}/api/me", auth.base)).header("authorization", authorization).send();

    let good = sign_request(&client, "GET", "/api/me", b"", now());
    assert_eq!(get_me(good).await.unwrap().status(), StatusCode::OK);

    let for_another_path = sign_request(&client, "GET", "/api/enroll-tokens", b"", now());
    assert_eq!(get_me(for_another_path).await.unwrap().status(), StatusCode::UNAUTHORIZED);
    let stale = sign_request(&client, "GET", "/api/me", b"", now() - 3600.0);
    assert_eq!(get_me(stale).await.unwrap().status(), StatusCode::UNAUTHORIZED);
    assert_eq!(get_me("Motile nonsense".to_string()).await.unwrap().status(), StatusCode::UNAUTHORIZED);
}

const GATEWAY: DeviceDescription<'static> = DeviceDescription { name: "motile-gateway", platform: "linux" };

fn kinds(me: &motile_protocol::auth_api::Me) -> Vec<DeviceKind> {
    me.devices.iter().map(|device| device.kind).collect()
}

#[sqlx::test]
async fn the_install_command_links_the_accounts_gateway(db: PgPool) {
    let auth = Auth::start(db).await;
    let (client, _) = auth.new_client(&ANN).await;
    let server = DeviceKey::generate();
    let token = auth.client.create_enroll_token(&client).await.unwrap();
    auth.client.enroll(&server, &token.token, &LINUX).await.unwrap();
    let gateway = DeviceKey::generate();

    let token = auth.client.create_enroll_token(&client).await.unwrap();
    let enrolled = auth.client.enroll_as(&gateway, &token.token, &GATEWAY, DeviceKind::Gateway).await.unwrap();
    assert_eq!(enrolled.email, "ann@example.com");

    let me = auth.client.me(&client).await.unwrap();
    assert_eq!(names(&me), vec!["Ann's Mac", "build-box", "motile-gateway"]);
    assert_eq!(kinds(&me), vec![DeviceKind::Client, DeviceKind::Server, DeviceKind::Gateway]);
    // Servers and the gateway see the same account, which is how they find and accept each other.
    assert_eq!(auth.client.me(&server).await.unwrap(), me);
    assert_eq!(auth.client.me(&gateway).await.unwrap(), me);

    // The code works for one machine, as a server's does.
    let other = auth.client.enroll_as(&DeviceKey::generate(), &token.token, &GATEWAY, DeviceKind::Gateway).await;
    assert!(other.unwrap_err().to_string().contains("expired or was used on another machine"));
}

#[sqlx::test]
async fn a_new_gateway_replaces_the_accounts_previous_one(db: PgPool) {
    let auth = Auth::start(db).await;
    let (ann, _) = auth.new_client(&ANN).await;
    let (bob, _) = auth.new_client(&BOB).await;
    let (first, second, bobs) = (DeviceKey::generate(), DeviceKey::generate(), DeviceKey::generate());
    for (owner, gateway) in [(&ann, &first), (&bob, &bobs), (&ann, &second)] {
        let token = auth.client.create_enroll_token(owner).await.unwrap();
        auth.client.enroll_as(gateway, &token.token, &GATEWAY, DeviceKind::Gateway).await.unwrap();
    }

    let me = auth.client.me(&ann).await.unwrap();
    assert_eq!(kinds(&me), vec![DeviceKind::Client, DeviceKind::Gateway]);
    assert_eq!(me.devices[1].public_key, second.public());
    assert_eq!(auth.client.me(&first).await.unwrap().user, None);
    // A gateway replaced or removed is gone, never kept as a removed server.
    assert!(me.removed.is_empty());
    auth.client.remove_device(&ann, &second.public()).await.unwrap();
    let me = auth.client.me(&ann).await.unwrap();
    assert_eq!(kinds(&me), vec![DeviceKind::Client]);
    assert!(me.removed.is_empty() && me.deleted.is_empty());

    assert_eq!(kinds(&auth.client.me(&bob).await.unwrap()), vec![DeviceKind::Client, DeviceKind::Gateway]);
}

#[sqlx::test]
async fn a_gateway_adds_and_deletes_no_servers(db: PgPool) {
    let auth = Auth::start(db).await;
    let (client, _) = auth.new_client(&ANN).await;
    let (server, gateway) = (DeviceKey::generate(), DeviceKey::generate());
    let token = auth.client.create_enroll_token(&client).await.unwrap();
    auth.client.enroll(&server, &token.token, &LINUX).await.unwrap();
    let token = auth.client.create_enroll_token(&client).await.unwrap();
    auth.client.enroll_as(&gateway, &token.token, &GATEWAY, DeviceKind::Gateway).await.unwrap();

    let from_gateway = auth.client.create_enroll_token(&gateway).await;
    assert_eq!(from_gateway.unwrap_err().to_string(), "Servers are added from a client.");
    let deleting = auth.client.delete_server_completely(&gateway, &server.public()).await;
    assert_eq!(deleting.unwrap_err().to_string(), "Servers are deleted from a client.");
    // Nor is the gateway a server to delete completely.
    assert!(auth.client.delete_server_completely(&client, &gateway.public()).await.is_err());
    assert_eq!(auth.client.me(&client).await.unwrap().devices.len(), 3);
}

#[sqlx::test]
async fn the_install_command_links_no_client(db: PgPool) {
    let auth = Auth::start(db).await;
    let (client, _) = auth.new_client(&ANN).await;
    let token = auth.client.create_enroll_token(&client).await.unwrap();

    let as_client = auth.client.enroll_as(&DeviceKey::generate(), &token.token, &LINUX, DeviceKind::Client).await;
    assert_eq!(as_client.unwrap_err().to_string(), "A client links by signing in, not with an install command.");
    // The code is still good for a server.
    auth.client.enroll(&DeviceKey::generate(), &token.token, &LINUX).await.unwrap();
}

#[sqlx::test]
async fn a_server_that_sends_no_kind_links_as_a_server(db: PgPool) {
    let auth = Auth::start(db).await;
    let (client, _) = auth.new_client(&ANN).await;
    let token = auth.client.create_enroll_token(&client).await.unwrap();
    let server = DeviceKey::generate();

    let request = json!({
        "token": token.token,
        "public_key": server.public(),
        "name": "old-box",
        "platform": "linux",
        "signature": server.sign(&enroll_message(&token.token)),
    });
    let response =
        reqwest::Client::new().post(format!("{}/api/enroll", auth.base)).json(&request).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let me = auth.client.me(&client).await.unwrap();
    assert_eq!(names(&me), vec!["Ann's Mac", "old-box"]);
    assert_eq!(me.devices[1].kind, DeviceKind::Server);
}
