//! Firebase Cloud Messaging's HTTP v1 API, with an access token the service account signs for.

use std::time::{Duration, Instant};

use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Sent, jwt, pem_body};
use crate::config::PushConfig;

const URL: &str = "https://fcm.googleapis.com";
const SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";
const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";
const ASSERTION_LIFETIME_SECS: i64 = 60 * 60;
const TOKEN_MARGIN: Duration = Duration::from_secs(5 * 60);

#[derive(Deserialize)]
struct ServiceAccount {
    project_id: String,
    client_email: String,
    private_key: String,
    token_uri: String,
}

#[derive(Deserialize)]
struct AccessToken {
    access_token: String,
    expires_in: u64,
}

pub struct Fcm {
    http: reqwest::Client,
    key: RsaKeyPair,
    project_id: String,
    client_email: String,
    /// The token endpoint the assertion is for, which is where it is sent unless the tests say another.
    audience: String,
    token_url: String,
    url: String,
    token: tokio::sync::Mutex<Option<(String, Instant)>>,
}

impl Fcm {
    pub fn new(config: &PushConfig) -> Option<Self> {
        let account = config.fcm_service_account.as_deref()?;
        let Ok(account) = serde_json::from_str::<ServiceAccount>(account) else {
            tracing::error!("FCM_SERVICE_ACCOUNT isn't a service account's JSON");
            return None;
        };
        let Some(key) = pem_body(&account.private_key).and_then(|der| RsaKeyPair::from_pkcs8(&der).ok()) else {
            tracing::error!("FCM_SERVICE_ACCOUNT's private key isn't an RSA key");
            return None;
        };
        let http = reqwest::Client::builder().timeout(Duration::from_secs(15)).build().expect("HTTP client");
        Some(Self {
            http,
            key,
            project_id: account.project_id,
            client_email: account.client_email,
            token_url: config.fcm_token_url.clone().unwrap_or_else(|| account.token_uri.clone()),
            audience: account.token_uri,
            url: config.fcm_url.clone().unwrap_or_else(|| URL.to_string()),
            token: tokio::sync::Mutex::new(None),
        })
    }

    fn sign(&self, message: &[u8]) -> Option<Vec<u8>> {
        let mut signature = vec![0; self.key.public().modulus_len()];
        self.key.sign(&RSA_PKCS1_SHA256, &SystemRandom::new(), message, &mut signature).ok()?;
        Some(signature)
    }

    async fn access_token(&self) -> Option<String> {
        let mut cached = self.token.lock().await;
        if let Some((token, until)) = &*cached
            && Instant::now() < *until
        {
            return Some(token.clone());
        }
        let now = chrono::Utc::now().timestamp();
        let header = json!({ "alg": "RS256", "typ": "JWT" });
        let claims = json!({
            "iss": self.client_email,
            "scope": SCOPE,
            "aud": self.audience,
            "iat": now,
            "exp": now + ASSERTION_LIFETIME_SECS,
        });
        let assertion = jwt(&header, &claims, |message| self.sign(message))?;
        let response = self.http.post(&self.token_url).form(&[("grant_type", GRANT_TYPE), ("assertion", &assertion)]);
        let response = match response.send().await {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => {
                tracing::warn!("Google's token endpoint answered {}", response.status());
                return None;
            }
            Err(error) => {
                tracing::warn!("Google's token endpoint failed: {error}");
                return None;
            }
        };
        let token: AccessToken = response.json().await.ok()?;
        let until = Instant::now() + Duration::from_secs(token.expires_in).saturating_sub(TOKEN_MARGIN);
        *cached = Some((token.access_token.clone(), until));
        Some(token.access_token)
    }

    pub async fn send(&self, message: &Value) -> Sent {
        let Some(token) = self.access_token().await else { return Sent::Failed };
        let url = format!("{}/v1/projects/{}/messages:send", self.url, self.project_id);
        let request = self.http.post(url).bearer_auth(token).json(&json!({ "message": message }));
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!("FCM push failed: {error}");
                return Sent::Failed;
            }
        };
        let status = response.status();
        if status.is_success() {
            return Sent::Delivered;
        }
        let body = response.text().await.unwrap_or_default();
        if status == 404 || body.contains("UNREGISTERED") {
            return Sent::Gone;
        }
        if status == 401 {
            *self.token.lock().await = None;
        }
        tracing::warn!("FCM answered {status}: {body}");
        Sent::Failed
    }
}
