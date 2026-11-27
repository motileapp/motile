//! Pushes to phones, through fake APNs and FCM servers that record what they are sent.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Version};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Form, Json, Router};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use motile_protocol::alerts::{
    Alert, AlertChoices, AlertKind, PushKey, Pushed, ServerWork, WorkItem, WorkState, seal_pushed,
};
use motile_protocol::auth_api::{
    DeviceKind, PushRegistration, PushService, PushTarget, PushWork, SealedAlert, SealedFor,
};
use motile_protocol::auth_client::DeviceDescription;
use motile_protocol::identity::{DeviceKey, random_token};
use motile_protocol::now;
use reqwest::Method;
use ring::rand::SystemRandom;
use ring::signature::{
    ECDSA_P256_SHA256_FIXED, ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair, RSA_PKCS1_2048_8192_SHA256,
    RsaKeyPair, UnparsedPublicKey,
};
use serde_json::{Value, json};
use sqlx::PgPool;

use super::web::{call, session};
use super::{ANN, Auth, BOB, LINUX, MAC};
use crate::config::PushConfig;
use crate::push::pem_body;

const TOPIC: &str = "app.motile.ios";
const FCM_PROJECT: &str = "motile-test";
const FCM_ACCESS_TOKEN: &str = "fcm-access-token";
const GOOGLE_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const FCM_KEY: &str = include_str!("fcm-test-key.pem");
const WAIT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug)]
struct Received {
    path: String,
    http2: bool,
    headers: HeaderMap,
    body: Value,
}

impl Received {
    fn header(&self, name: &str) -> &str {
        self.headers.get(name).map(|value| value.to_str().unwrap()).unwrap_or_default()
    }
}

/// APNs and FCM in one: what they were sent, and the tokens they say are gone.
#[derive(Clone, Default)]
struct Fake {
    received: Arc<Mutex<Vec<Received>>>,
    gone: Arc<Mutex<HashSet<String>>>,
}

impl Fake {
    fn record(&self, path: String, version: Version, headers: HeaderMap, body: Value) {
        let http2 = version == Version::HTTP_2;
        self.received.lock().unwrap().push(Received { path, http2, headers, body });
    }

    fn is_gone(&self, token: &str) -> bool {
        self.gone.lock().unwrap().contains(token)
    }

    fn received(&self) -> Vec<Received> {
        self.received.lock().unwrap().clone()
    }

    /// The pushes, past Google's token requests, once there are `count` of them.
    async fn pushes(&self, count: usize) -> Vec<Received> {
        let pushes = || self.received().into_iter().filter(|received| received.path != "/token").collect::<Vec<_>>();
        let deadline = tokio::time::Instant::now() + WAIT;
        while pushes().len() < count {
            assert!(tokio::time::Instant::now() < deadline, "{} pushes, not {count}", pushes().len());
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        let pushes = pushes();
        assert_eq!(pushes.len(), count, "{pushes:#?}");
        pushes
    }
}

async fn apns(
    State(fake): State<Fake>,
    Path(token): Path<String>,
    version: Version,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    fake.record(format!("/3/device/{token}"), version, headers, serde_json::from_slice(&body).unwrap());
    if fake.is_gone(&token) {
        return (StatusCode::GONE, Json(json!({ "reason": "Unregistered" })));
    }
    (StatusCode::OK, Json(json!({})))
}

async fn google_token(
    State(fake): State<Fake>,
    version: Version,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Json<Value> {
    fake.record("/token".into(), version, headers, serde_json::to_value(form).unwrap());
    Json(json!({ "access_token": FCM_ACCESS_TOKEN, "expires_in": 3600, "token_type": "Bearer" }))
}

async fn fcm(
    State(fake): State<Fake>,
    Path(project): Path<String>,
    version: Version,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let body: Value = serde_json::from_slice(&body).unwrap();
    let token = body["message"]["token"].as_str().unwrap_or_default().to_string();
    fake.record(format!("/v1/projects/{project}/messages:send"), version, headers, body);
    if fake.is_gone(&token) {
        let error = json!({ "error": { "status": "NOT_FOUND", "details": [{ "errorCode": "UNREGISTERED" }] } });
        return (StatusCode::NOT_FOUND, Json(error));
    }
    (StatusCode::OK, Json(json!({ "name": "projects/motile-test/messages/1" })))
}

async fn fake_push_services() -> (String, Fake) {
    let fake = Fake::default();
    let router = Router::new()
        .route("/3/device/{token}", post(apns))
        .route("/token", post(google_token))
        .route("/v1/projects/{project}/messages:send", post(fcm))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (base, fake)
}

struct Push {
    auth: Auth,
    fake: Fake,
    apns_public: Vec<u8>,
}

impl Push {
    async fn start(db: PgPool) -> Self {
        let (url, fake) = fake_push_services().await;
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new()).unwrap();
        let apns_key =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &SystemRandom::new()).unwrap();
        let p8 = format!("-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n", STANDARD.encode(pkcs8));
        let service_account = json!({
            "type": "service_account",
            "project_id": FCM_PROJECT,
            "client_email": "push@motile-test.iam.gserviceaccount.com",
            "private_key": FCM_KEY,
            "token_uri": GOOGLE_TOKEN_URI,
        });
        let push = PushConfig {
            apns_key: Some(p8),
            apns_key_id: Some("KEY0123456".into()),
            apns_team_id: Some("TEAM012345".into()),
            apns_url: Some(url.clone()),
            fcm_service_account: Some(service_account.to_string()),
            fcm_url: Some(url.clone()),
            fcm_token_url: Some(format!("{url}/token")),
        };
        let auth = Auth::start_with(db, |config| config.push = push).await;
        Self { auth, fake, apns_public: apns_key.public_key().as_ref().to_vec() }
    }

    async fn server_of(&self, client: &DeviceKey) -> DeviceKey {
        let server = DeviceKey::generate();
        let token = self.auth.client.create_enroll_token(client).await.unwrap();
        self.auth.client.enroll(&server, &token.token, &LINUX).await.unwrap();
        server
    }

    async fn count(&self, table: &str) -> i64 {
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
            .fetch_one(&self.auth.db)
            .await
            .unwrap()
    }

    async fn live_token(&self, phone: &DeviceKey) -> Option<String> {
        sqlx::query_scalar("SELECT live_token FROM push_registrations WHERE device = $1")
            .bind(phone.public())
            .fetch_one(&self.auth.db)
            .await
            .unwrap()
    }

    async fn until_registrations(&self, count: i64) {
        let deadline = tokio::time::Instant::now() + WAIT;
        while self.count("push_registrations").await != count {
            assert!(tokio::time::Instant::now() < deadline, "the registrations never came to {count}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

fn iphone(token: &str) -> PushRegistration {
    PushRegistration {
        service: PushService::Apns { topic: TOPIC.into(), sandbox: true },
        token: token.into(),
        choices: AlertChoices::default(),
        live: false,
        live_start_token: None,
        live_token: None,
    }
}

fn android(token: &str) -> PushRegistration {
    PushRegistration { service: PushService::Fcm, ..iphone(token) }
}

fn alert(kind: AlertKind) -> Alert {
    Alert {
        id: random_token(),
        server_id: "server".into(),
        thread_id: "thread-1".into(),
        kind,
        thread_title: "Fix the login".into(),
        project: "motile".into(),
        detail: Some("Run greet.py".into()),
        body: format!("{} · Run greet.py", kind.headline()),
        at: whole_now(),
    }
}

fn sealed(phone: &DeviceKey, alert: &Alert) -> SealedAlert {
    SealedAlert {
        device: phone.public(),
        thread_id: alert.thread_id.clone(),
        kind: alert.kind,
        sealed: seal_pushed(&phone.public(), &Pushed::Alert(alert.clone())).unwrap(),
    }
}

fn work(server: &DeviceKey, working: usize, waiting: usize) -> ServerWork {
    let at = whole_now();
    let item = |index: usize, state| WorkItem {
        thread_id: format!("thread-{index}"),
        title: format!("Thread {index}"),
        project: "motile".into(),
        state,
        since: at,
    };
    let items = (0..working).map(|index| item(index, WorkState::Working));
    let items = items.chain((working..working + waiting).map(|index| item(index, WorkState::Waiting)));
    ServerWork::new(server.public(), "build-box".into(), items.collect(), at)
}

/// Now in whole seconds, which come back from JSON exactly, as a fraction may not.
fn whole_now() -> f64 {
    now().floor()
}

fn work_for(phones: &[&DeviceKey], work: &ServerWork) -> PushWork {
    let sealed = phones.iter().map(|phone| SealedFor {
        device: phone.public(),
        sealed: seal_pushed(&phone.public(), &Pushed::Work(work.clone())).unwrap(),
    });
    PushWork { working: work.working, waiting: work.waiting, sealed: sealed.collect() }
}

fn parts(push: &Received) -> Vec<String> {
    serde_json::from_value(push.body["aps"]["content-state"]["parts"].clone()).unwrap()
}

fn counts(push: &Received) -> (i64, i64) {
    let content = &push.body["aps"]["content-state"];
    (content["working"].as_i64().unwrap(), content["waiting"].as_i64().unwrap())
}

fn opened_work(phone: &DeviceKey, sealed: &str) -> ServerWork {
    match PushKey::of_device(phone).open_pushed(sealed) {
        Some(Pushed::Work(work)) => work,
        other => panic!("not work: {other:?}"),
    }
}

/// The claims of a JWT whose signature `public` verifies.
fn verified_claims(
    token: &str,
    algorithm: &'static dyn ring::signature::VerificationAlgorithm,
    public: &[u8],
) -> (Value, Value) {
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3);
    let signature = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
    let message = format!("{}.{}", parts[0], parts[1]);
    UnparsedPublicKey::new(algorithm, public).verify(message.as_bytes(), &signature).expect("a good signature");
    let decode = |part: &str| serde_json::from_slice::<Value>(&URL_SAFE_NO_PAD.decode(part).unwrap()).unwrap();
    (decode(parts[0]), decode(parts[1]))
}

#[sqlx::test]
async fn a_phone_hears_an_alert_a_server_of_its_account_sealed_for_it(db: PgPool) {
    let push = Push::start(db).await;
    let (phone, _) = push.auth.new_client(&ANN).await;
    let server = push.server_of(&phone).await;
    push.auth.client.set_push(&phone, &iphone("aaaa1111")).await.unwrap();

    let targets = push.auth.client.push_targets(&server).await.unwrap();
    assert_eq!(targets, vec![PushTarget { device: phone.public(), choices: AlertChoices::default(), live: false }]);

    let approval = alert(AlertKind::Approval);
    push.auth.client.push_alerts(&server, &[sealed(&phone, &approval)]).await.unwrap();
    let pushes = push.fake.pushes(1).await;
    let sent = &pushes[0];
    assert_eq!(sent.path, "/3/device/aaaa1111");
    assert!(sent.http2);
    assert_eq!(sent.header("apns-push-type"), "alert");
    assert_eq!(sent.header("apns-topic"), TOPIC);
    assert_eq!(sent.header("apns-priority"), "10");
    assert_eq!(sent.header("apns-collapse-id"), "thread-1");
    let expiration: f64 = sent.header("apns-expiration").parse().unwrap();
    assert!((expiration - (now() + 86400.0)).abs() < 60.0);

    let token = sent.header("authorization").strip_prefix("bearer ").unwrap();
    let (header, claims) = verified_claims(token, &ECDSA_P256_SHA256_FIXED, &push.apns_public);
    assert_eq!(header, json!({ "alg": "ES256", "kid": "KEY0123456" }));
    assert_eq!(claims["iss"], "TEAM012345");
    assert!((claims["iat"].as_f64().unwrap() - now()).abs() < 60.0);

    let aps = &sent.body["aps"];
    assert_eq!(aps["alert"], json!({ "title": "Motile", "body": "An agent needs you" }));
    assert_eq!(aps["mutable-content"], 1);
    assert_eq!(aps.get("sound"), None, "quiet unless the phone asked for sound");
    assert_eq!(aps["thread-id"], "thread-1");
    assert_eq!(aps["interruption-level"], "time-sensitive");
    assert_eq!(sent.body["motile"], "alert");
    assert_eq!(sent.body["thread"], "thread-1");
    let opened = PushKey::of_device(&phone).open_pushed(sent.body["sealed"].as_str().unwrap());
    assert_eq!(opened, Some(Pushed::Alert(approval)));

    // A finished turn doesn't break through a Focus, and the token is signed once.
    push.auth.client.push_alerts(&server, &[sealed(&phone, &alert(AlertKind::Finished))]).await.unwrap();
    let pushes = push.fake.pushes(2).await;
    assert_eq!(pushes[1].body["aps"]["interruption-level"], "active");
    assert_eq!(pushes[1].body["aps"]["alert"]["body"], "An agent finished");
    assert_eq!(pushes[1].header("authorization"), pushes[0].header("authorization"));

    let loud =
        PushRegistration { choices: AlertChoices { sound: true, ..AlertChoices::default() }, ..iphone("aaaa1111") };
    push.auth.client.set_push(&phone, &loud).await.unwrap();
    push.auth.client.push_alerts(&server, &[sealed(&phone, &alert(AlertKind::Finished))]).await.unwrap();
    assert_eq!(push.fake.pushes(3).await[2].body["aps"]["sound"], "default");
}

#[sqlx::test]
async fn an_android_phone_hears_an_alert_through_fcm(db: PgPool) {
    let push = Push::start(db).await;
    let (phone, _) = push.auth.new_client(&ANN).await;
    let server = push.server_of(&phone).await;
    push.auth.client.set_push(&phone, &android("fcm-token:abc_123")).await.unwrap();

    let finished = alert(AlertKind::Finished);
    push.auth.client.push_alerts(&server, &[sealed(&phone, &finished)]).await.unwrap();
    let pushes = push.fake.pushes(1).await;
    let sent = &pushes[0];
    assert_eq!(sent.path, format!("/v1/projects/{FCM_PROJECT}/messages:send"));
    assert_eq!(sent.header("authorization"), format!("Bearer {FCM_ACCESS_TOKEN}"));
    let message = &sent.body["message"];
    assert_eq!(message["token"], "fcm-token:abc_123");
    assert_eq!(message.get("notification"), None);
    assert_eq!(message["data"]["motile"], "alert");
    assert_eq!(message["data"]["thread"], "thread-1");
    assert_eq!(message["data"]["kind"], "finished");
    assert_eq!(message["android"], json!({ "priority": "HIGH", "ttl": "86400s", "collapse_key": "thread-1" }));
    let opened = PushKey::of_device(&phone).open_pushed(message["data"]["sealed"].as_str().unwrap());
    assert_eq!(opened, Some(Pushed::Alert(finished)));

    let grants: Vec<Received> = push.fake.received().into_iter().filter(|received| received.path == "/token").collect();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].body["grant_type"], "urn:ietf:params:oauth:grant-type:jwt-bearer");
    let rsa = RsaKeyPair::from_pkcs8(&pem_body(FCM_KEY).unwrap()).unwrap();
    let assertion = grants[0].body["assertion"].as_str().unwrap();
    let (header, claims) = verified_claims(assertion, &RSA_PKCS1_2048_8192_SHA256, rsa.public().as_ref());
    assert_eq!(header["alg"], "RS256");
    assert_eq!(claims["iss"], "push@motile-test.iam.gserviceaccount.com");
    assert_eq!(claims["scope"], "https://www.googleapis.com/auth/firebase.messaging");
    assert_eq!(claims["aud"], GOOGLE_TOKEN_URI);

    // The access token is used again until it is about to expire.
    push.auth.client.push_alerts(&server, &[sealed(&phone, &alert(AlertKind::Failed))]).await.unwrap();
    push.fake.pushes(2).await;
    assert_eq!(push.fake.received().iter().filter(|received| received.path == "/token").count(), 1);
}

#[sqlx::test]
async fn a_phone_hears_only_the_alerts_it_chose(db: PgPool) {
    let push = Push::start(db).await;
    let (phone, _) = push.auth.new_client(&ANN).await;
    let (other, _) = push.auth.new_client(&ANN).await;
    let server = push.server_of(&phone).await;
    let choices = AlertChoices { finished: false, question: false, ..AlertChoices::default() };
    push.auth.client.set_push(&phone, &PushRegistration { choices, ..iphone("aaaa1111") }).await.unwrap();

    let alerts = [
        sealed(&phone, &alert(AlertKind::Finished)),
        sealed(&phone, &alert(AlertKind::Plan)),
        // A client that takes no pushes, and the server itself, are no phones.
        sealed(&other, &alert(AlertKind::Approval)),
        sealed(&server, &alert(AlertKind::Approval)),
        sealed(&phone, &alert(AlertKind::Limit)),
    ];
    push.auth.client.push_alerts(&server, &alerts).await.unwrap();
    let pushes = push.fake.pushes(1).await;
    assert_eq!(pushes[0].body["aps"]["alert"]["body"], "An agent reached its usage limit");
    assert_eq!(push.auth.client.push_targets(&server).await.unwrap()[0].choices, choices);
}

#[sqlx::test]
async fn a_server_of_another_account_neither_lists_nor_pushes_to_a_phone(db: PgPool) {
    let push = Push::start(db).await;
    let (phone, _) = push.auth.new_client(&ANN).await;
    let (bob, _) = push.auth.new_client(&BOB).await;
    let (anns, bobs) = (push.server_of(&phone).await, push.server_of(&bob).await);
    push.auth.client.set_push(&phone, &PushRegistration { live: true, ..iphone("aaaa1111") }).await.unwrap();

    assert!(push.auth.client.push_targets(&bobs).await.unwrap().is_empty());
    push.auth.client.push_alerts(&bobs, &[sealed(&phone, &alert(AlertKind::Approval))]).await.unwrap();
    push.auth.client.push_work(&bobs, &work_for(&[&phone], &work(&bobs, 1, 0))).await.unwrap();
    assert_eq!(push.count("push_work").await, 0);

    push.auth.client.push_alerts(&anns, &[sealed(&phone, &alert(AlertKind::Failed))]).await.unwrap();
    let pushes = push.fake.pushes(1).await;
    assert_eq!(pushes[0].body["aps"]["alert"]["body"], "An agent failed");
}

fn error<T: std::fmt::Debug, E: std::fmt::Display>(result: Result<T, E>) -> String {
    match result {
        Ok(value) => panic!("it went through: {value:?}"),
        Err(error) => error.to_string(),
    }
}

#[sqlx::test]
async fn only_a_client_registers_and_only_a_server_pushes(db: PgPool) {
    let push = Push::start(db).await;
    let (phone, _) = push.auth.new_client(&ANN).await;
    let server = push.server_of(&phone).await;
    let gateway = DeviceKey::generate();
    let token = push.auth.client.create_enroll_token(&phone).await.unwrap();
    let description = DeviceDescription { name: "motile-gateway", platform: "linux" };
    push.auth.client.enroll_as(&gateway, &token.token, &description, DeviceKind::Gateway).await.unwrap();
    let stranger = DeviceKey::generate();

    assert_eq!(error(push.auth.client.set_push(&server, &iphone("aaaa1111")).await), "Only a client takes pushes.");
    assert_eq!(error(push.auth.client.set_push(&gateway, &iphone("aaaa1111")).await), "Only a client takes pushes.");
    assert_eq!(
        error(push.auth.client.set_push(&stranger, &iphone("aaaa1111")).await),
        "This device isn't linked to an account."
    );
    for pusher in [&phone, &gateway] {
        let targets = push.auth.client.push_targets(pusher).await;
        assert_eq!(error(targets), "Only a server pushes.");
        let alerts = push.auth.client.push_alerts(pusher, &[sealed(&phone, &alert(AlertKind::Approval))]).await;
        assert_eq!(error(alerts), "Only a server pushes.");
        assert_eq!(
            error(push.auth.client.push_work(pusher, &work_for(&[&phone], &work(&server, 1, 0))).await),
            "Only a server pushes."
        );
    }
    assert_eq!(error(push.auth.client.push_targets(&stranger).await), "This device isn't linked to an account.");

    // The web app holds a session, which is no device.
    let web = session(&push.auth, &ANN).await;
    assert_eq!(call(&push.auth, Method::GET, "/api/push/targets", &web).await.status(), StatusCode::FORBIDDEN);
    assert_eq!(call(&push.auth, Method::DELETE, "/api/push", &web).await.status(), StatusCode::FORBIDDEN);

    for token in ["", "aaaa/../1111", &"a".repeat(513)] {
        let result = push.auth.client.set_push(&phone, &iphone(token)).await;
        assert_eq!(error(result), "That isn't a push token.");
    }
    let live = PushRegistration { live_token: Some("live?token".into()), ..iphone("aaaa1111") };
    assert_eq!(error(push.auth.client.set_push(&phone, &live).await), "That isn't a push token.");
    let topic = PushRegistration {
        service: PushService::Apns { topic: "app/motile".into(), sandbox: false },
        ..iphone("aaaa1111")
    };
    assert_eq!(error(push.auth.client.set_push(&phone, &topic).await), "That isn't an app's bundle identifier.");
    assert_eq!(push.count("push_registrations").await, 0);

    push.auth.client.set_push(&phone, &android("fcm-token:abc_123")).await.unwrap();
    assert_eq!(push.auth.client.push_targets(&server).await.unwrap().len(), 1);
}

#[sqlx::test]
async fn a_phone_that_stops_is_removed_or_moves_takes_no_pushes(db: PgPool) {
    let push = Push::start(db).await;
    let (phone, _) = push.auth.new_client(&ANN).await;
    let server = push.server_of(&phone).await;
    let registration = PushRegistration { live: true, ..iphone("aaaa1111") };

    push.auth.client.set_push(&phone, &registration).await.unwrap();
    push.auth.client.push_work(&server, &work_for(&[&phone], &work(&server, 1, 0))).await.unwrap();
    assert_eq!(push.count("push_work").await, 1);
    push.auth.client.remove_push(&phone).await.unwrap();
    assert_eq!((push.count("push_registrations").await, push.count("push_work").await), (0, 0));
    assert!(push.auth.client.push_targets(&server).await.unwrap().is_empty());

    // Signed in to another account, it registers again there.
    push.auth.client.set_push(&phone, &registration).await.unwrap();
    let verifier = random_token();
    let code = push.auth.sign_in_code(&verifier, &BOB).await;
    push.auth.client.exchange(&phone, &code, &verifier, &MAC).await.unwrap();
    assert_eq!(push.count("push_registrations").await, 0);

    // Removed from the account, it is forgotten with what was kept for it.
    let (phone, _) = push.auth.new_client(&ANN).await;
    push.auth.client.set_push(&phone, &registration).await.unwrap();
    push.auth.client.push_work(&server, &work_for(&[&phone], &work(&server, 1, 0))).await.unwrap();
    push.auth.client.remove_device(&phone, "self").await.unwrap();
    assert_eq!((push.count("push_registrations").await, push.count("push_work").await), (0, 0));
    assert!(push.auth.client.push_targets(&server).await.unwrap().is_empty());
}

#[sqlx::test]
async fn a_new_device_of_the_same_install_takes_its_push_token(db: PgPool) {
    let push = Push::start(db).await;
    let (old, _) = push.auth.new_client(&ANN).await;
    let (new, _) = push.auth.new_client(&ANN).await;
    let server = push.server_of(&new).await;
    let registration = PushRegistration { live: true, live_token: Some("live0000".into()), ..iphone("aaaa1111") };
    push.auth.client.set_push(&old, &registration).await.unwrap();
    push.auth.client.push_work(&server, &work_for(&[&old], &work(&server, 1, 0))).await.unwrap();
    push.fake.pushes(1).await;

    push.auth.client.set_push(&new, &registration).await.unwrap();
    assert_eq!((push.count("push_registrations").await, push.count("push_work").await), (1, 0));
    let targets = push.auth.client.push_targets(&server).await.unwrap();
    assert_eq!(targets.iter().map(|target| target.device.clone()).collect::<Vec<_>>(), vec![new.public()]);

    push.auth.client.push_work(&server, &work_for(&[&old, &new], &work(&server, 1, 0))).await.unwrap();
    let pushes = push.fake.pushes(2).await;
    assert_eq!(opened_work(&new, &parts(&pushes[1])[0]).working, 1, "only what the phone's key opens");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(push.fake.received().iter().filter(|received| received.path.starts_with("/3/")).count(), 2);
}

#[sqlx::test]
async fn a_token_the_push_service_no_longer_takes_is_dropped(db: PgPool) {
    let push = Push::start(db).await;
    let (iphone_key, _) = push.auth.new_client(&ANN).await;
    let (android_key, _) = push.auth.new_client(&ANN).await;
    let server = push.server_of(&iphone_key).await;
    push.auth.client.set_push(&iphone_key, &iphone("aaaa1111")).await.unwrap();
    push.auth.client.set_push(&android_key, &android("fcm-token:gone")).await.unwrap();
    push.fake.gone.lock().unwrap().extend(["aaaa1111".to_string(), "fcm-token:gone".to_string()]);

    push.auth.client.push_alerts(&server, &[sealed(&iphone_key, &alert(AlertKind::Finished))]).await.unwrap();
    push.until_registrations(1).await;
    push.auth.client.push_alerts(&server, &[sealed(&android_key, &alert(AlertKind::Finished))]).await.unwrap();
    push.until_registrations(0).await;

    // A Live Activity that is gone only loses its token.
    let live = PushRegistration { live: true, live_token: Some("dead0000".into()), ..iphone("bbbb2222") };
    push.auth.client.set_push(&iphone_key, &live).await.unwrap();
    push.fake.gone.lock().unwrap().insert("dead0000".into());
    push.auth.client.push_work(&server, &work_for(&[&iphone_key], &work(&server, 1, 0))).await.unwrap();
    push.fake.pushes(3).await;
    assert_eq!(push.live_token(&iphone_key).await, None);
    assert_eq!(push.count("push_registrations").await, 1);
}

#[sqlx::test]
async fn work_starts_updates_and_ends_a_live_activity(db: PgPool) {
    let push = Push::start(db).await;
    let (phone, _) = push.auth.new_client(&ANN).await;
    let (first, second) = (push.server_of(&phone).await, push.server_of(&phone).await);
    let registration = PushRegistration { live: true, live_start_token: Some("start000".into()), ..iphone("aaaa1111") };
    push.auth.client.set_push(&phone, &registration).await.unwrap();
    assert!(push.auth.client.push_targets(&first).await.unwrap()[0].live);

    let started = work(&first, 1, 0);
    push.auth.client.push_work(&first, &work_for(&[&phone], &started)).await.unwrap();
    let pushes = push.fake.pushes(1).await;
    let start = &pushes[0];
    assert_eq!(start.path, "/3/device/start000");
    assert_eq!(start.header("apns-push-type"), "liveactivity");
    assert_eq!(start.header("apns-topic"), "app.motile.ios.push-type.liveactivity");
    assert_eq!(start.header("apns-priority"), "10");
    assert_eq!(start.body["aps"]["event"], "start");
    assert_eq!(start.body["aps"]["attributes-type"], "AgentsAttributes");
    assert_eq!(start.body["aps"]["attributes"], json!({}));
    assert_eq!(start.body["aps"]["alert"], json!({ "title": "Motile", "body": "Agents are working" }));
    assert!((start.body["aps"]["timestamp"].as_f64().unwrap() - now()).abs() < 60.0);
    let sealed = parts(start);
    assert_eq!(sealed.len(), 1);
    assert_eq!(opened_work(&phone, &sealed[0]), started);
    assert_eq!(counts(start), (1, 0));

    // It starts once: until the phone sends the Live Activity's token, more work starts nothing.
    push.auth.client.push_work(&first, &work_for(&[&phone], &work(&first, 2, 0))).await.unwrap();
    push.auth
        .client
        .set_push(&phone, &PushRegistration { live_token: Some("live0000".into()), ..registration })
        .await
        .unwrap();
    let waiting = work(&second, 0, 1);
    push.auth.client.push_work(&second, &work_for(&[&phone], &waiting)).await.unwrap();
    let pushes = push.fake.pushes(2).await;
    let update = &pushes[1];
    assert_eq!(update.path, "/3/device/live0000");
    assert_eq!(update.header("apns-priority"), "5");
    assert_eq!(update.body["aps"]["event"], "update");
    let mut servers = vec![first.public(), second.public()];
    servers.sort();
    let shown: Vec<String> = parts(update).iter().map(|sealed| opened_work(&phone, sealed).server_id).collect();
    assert_eq!(shown, servers);
    assert_eq!(counts(update), (2, 1), "every server's counts, as the parts may not open");

    // While the second still waits, the first stopping updates it.
    push.auth.client.push_work(&first, &work_for(&[&phone], &work(&first, 0, 0))).await.unwrap();
    let pushes = push.fake.pushes(3).await;
    assert_eq!(pushes[2].body["aps"]["event"], "update");
    assert_eq!(parts(&pushes[2]).len(), 2);

    // A part two hours old is stale.
    sqlx::query("UPDATE push_work SET at = now() - interval '3 hours' WHERE server = $1")
        .bind(first.public())
        .execute(&push.auth.db)
        .await
        .unwrap();
    push.auth.client.push_work(&second, &work_for(&[&phone], &waiting)).await.unwrap();
    let pushes = push.fake.pushes(4).await;
    assert_eq!(parts(&pushes[3]).len(), 1);

    push.auth.client.push_work(&second, &work_for(&[&phone], &work(&second, 0, 0))).await.unwrap();
    let pushes = push.fake.pushes(5).await;
    let end = &pushes[4];
    assert_eq!(end.path, "/3/device/live0000");
    assert_eq!(end.header("apns-priority"), "10");
    assert_eq!(end.body["aps"]["event"], "end");
    let dismissal = end.body["aps"]["dismissal-date"].as_f64().unwrap();
    assert!((dismissal - (now() + 15.0 * 60.0)).abs() < 60.0);
    assert_eq!(push.live_token(&phone).await, None);
}

#[sqlx::test]
async fn an_android_phone_hears_work_urgently_only_when_it_starts_or_stops(db: PgPool) {
    let push = Push::start(db).await;
    let (phone, _) = push.auth.new_client(&ANN).await;
    let server = push.server_of(&phone).await;
    push.auth.client.set_push(&phone, &PushRegistration { live: true, ..android("fcm-token:abc_123") }).await.unwrap();

    for (index, (working, priority)) in [(1, "HIGH"), (2, "NORMAL"), (0, "HIGH")].into_iter().enumerate() {
        let now_doing = work(&server, working, 0);
        push.auth.client.push_work(&server, &work_for(&[&phone], &now_doing)).await.unwrap();
        let pushes = push.fake.pushes(index + 1).await;
        let message = &pushes[index].body["message"];
        assert_eq!(message["android"]["priority"], priority);
        assert_eq!(message["data"]["motile"], "work");
        let sealed: Vec<String> = serde_json::from_str(message["data"]["parts"].as_str().unwrap()).unwrap();
        assert_eq!(sealed.iter().map(|sealed| opened_work(&phone, sealed)).collect::<Vec<_>>(), vec![now_doing]);
    }
}

#[sqlx::test]
async fn without_the_push_services_keys_registrations_are_still_kept(db: PgPool) {
    let auth = Auth::start(db).await;
    let (phone, _) = auth.new_client(&ANN).await;
    let token = auth.client.create_enroll_token(&phone).await.unwrap();
    let server = DeviceKey::generate();
    auth.client.enroll(&server, &token.token, &LINUX).await.unwrap();

    auth.client.set_push(&phone, &PushRegistration { live: true, ..iphone("aaaa1111") }).await.unwrap();
    assert_eq!(auth.client.push_targets(&server).await.unwrap().len(), 1);
    auth.client.push_alerts(&server, &[sealed(&phone, &alert(AlertKind::Approval))]).await.unwrap();
    auth.client.push_work(&server, &work_for(&[&phone], &work(&server, 1, 0))).await.unwrap();
}
