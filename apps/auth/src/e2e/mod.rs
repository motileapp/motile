//! The auth server's real router on a fresh database per test. Google is played by a small
//! server that answers the token request; everything else is the real thing.

mod devices;
mod sign_in;
mod web;
mod whole;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::routing::post;
use axum::{Json, Router};
use motile_protocol::auth_api::Me;
use motile_protocol::auth_client::{AuthClient, DeviceDescription};
use motile_protocol::identity::{DeviceKey, random_token};
use reqwest::Url;
use reqwest::header::LOCATION;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::config::Config;
use crate::{Inner, google, router};

const GOOGLE_CLIENT_ID: &str = "google-client";
const WEB_URL: &str = "https://app.example.com";
const INSTALL_URL: &str = "https://example.com/install.sh";
const MAC: DeviceDescription<'static> = DeviceDescription { name: "Ann's Mac", platform: "macos" };
const LINUX: DeviceDescription<'static> = DeviceDescription { name: "build-box", platform: "linux" };

/// What the fake Google says about whoever signs in next.
#[derive(Clone)]
struct GoogleAccount {
    subject: &'static str,
    email: &'static str,
    email_verified: bool,
}

const ANN: GoogleAccount = GoogleAccount { subject: "google-ann", email: "ann@example.com", email_verified: true };
const BOB: GoogleAccount = GoogleAccount { subject: "google-bob", email: "bob@example.com", email_verified: true };

pub struct Auth {
    pub base: String,
    pub db: PgPool,
    pub client: AuthClient,
    /// Follows no redirects, to look at where the browser would be sent.
    http: reqwest::Client,
    google_account: Arc<Mutex<GoogleAccount>>,
    /// The nonce of the sign-in in progress, which Google would echo in the ID token.
    google_nonce: Arc<Mutex<String>>,
}

impl Auth {
    pub async fn start(db: PgPool) -> Self {
        Self::start_with(db, |_| {}).await
    }

    pub async fn start_with(db: PgPool, configure: impl FnOnce(&mut Config)) -> Self {
        motile_protocol::tls::install();
        let google_account = Arc::new(Mutex::new(ANN));
        let google_nonce = Arc::new(Mutex::new(String::new()));
        let google_base = fake_google(google_account.clone(), google_nonce.clone()).await;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let mut config = Config {
            public_url: base.clone(),
            database_url: String::new(),
            port: 0,
            google_client_id: GOOGLE_CLIENT_ID.into(),
            google_client_secret: "google-secret".into(),
            google_authorize_url: google::AUTHORIZE_URL.into(),
            google_token_url: format!("{google_base}/token"),
            dev_login: true,
            install_url: INSTALL_URL.into(),
            web_url: Some(WEB_URL.into()),
        };
        configure(&mut config);
        let state = Arc::new(Inner { config, db: db.clone(), http: reqwest::Client::new() });
        tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });

        let http = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap();
        Self { client: AuthClient::new(&base), base, db, http, google_account, google_nonce }
    }

    async fn get(&self, url: &str) -> reqwest::Response {
        self.http.get(url).send().await.unwrap()
    }

    /// Opens the sign-in page as the browser would and returns where Google is asked to sign in.
    async fn open_sign_in(&self, verifier: &str, app_state: &str) -> Url {
        self.open(&self.client.sign_in_url(verifier, app_state)).await
    }

    async fn open(&self, sign_in_url: &str) -> Url {
        let response = self.get(sign_in_url).await;
        assert!(response.status().is_redirection(), "{}", response.status());
        Url::parse(response.headers()[LOCATION].to_str().unwrap()).unwrap()
    }

    /// Goes through Google as `account` and returns where the browser ends up.
    async fn through_google(&self, verifier: &str, app_state: &str, account: &GoogleAccount) -> reqwest::Response {
        self.through_google_from(&self.client.sign_in_url(verifier, app_state), account).await
    }

    async fn through_google_from(&self, sign_in_url: &str, account: &GoogleAccount) -> reqwest::Response {
        let google = self.open(sign_in_url).await;
        let query: HashMap<String, String> = google.query_pairs().into_owned().collect();
        *self.google_account.lock().unwrap() = account.clone();
        *self.google_nonce.lock().unwrap() = query["nonce"].clone();
        self.get(&format!("{}/auth/google/callback?code=google-code&state={}", self.base, query["state"])).await
    }

    /// The code the client is handed back after `account` signed in.
    async fn sign_in_code(&self, verifier: &str, account: &GoogleAccount) -> String {
        let response = self.through_google(verifier, "app-state", account).await;
        let back = Url::parse(response.headers()[LOCATION].to_str().unwrap()).unwrap();
        assert_eq!((back.scheme(), back.host_str()), ("motile", Some("auth")));
        let query: HashMap<String, String> = back.query_pairs().into_owned().collect();
        assert_eq!(query["state"], "app-state");
        query["code"].clone()
    }

    /// A new client signed in as `account`.
    async fn new_client(&self, account: &GoogleAccount) -> (DeviceKey, Me) {
        let key = DeviceKey::generate();
        let verifier = random_token();
        let code = self.sign_in_code(&verifier, account).await;
        let me = self.client.exchange(&key, &code, &verifier, &MAC).await.unwrap();
        (key, me)
    }
}

async fn fake_google(account: Arc<Mutex<GoogleAccount>>, nonce: Arc<Mutex<String>>) -> String {
    let token = move || {
        let account = account.lock().unwrap().clone();
        let claims = json!({
            "iss": "https://accounts.google.com",
            "aud": GOOGLE_CLIENT_ID,
            "exp": chrono::Utc::now().timestamp() + 60,
            "nonce": *nonce.lock().unwrap(),
            "sub": account.subject,
            "email": account.email,
            "email_verified": account.email_verified,
            "name": "Ann Example",
        });
        async move { Json::<Value>(json!({ "id_token": google::id_token(&claims) })) }
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, Router::new().route("/token", post(token))).await.unwrap() });
    base
}

fn emails(me: &Me) -> Option<&str> {
    me.user.as_ref().map(|user| user.email.as_str())
}

fn names(me: &Me) -> Vec<&str> {
    me.devices.iter().map(|device| device.name.as_str()).collect()
}
