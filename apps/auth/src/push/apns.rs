//! Apple's push service, over HTTP/2 with a token signed by the team's APNs key.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair};
use serde_json::{Value, json};

use super::{Sent, jwt, pem_body};
use crate::config::PushConfig;

const PRODUCTION_URL: &str = "https://api.push.apple.com";
const SANDBOX_URL: &str = "https://api.sandbox.push.apple.com";
const TOKEN_LIFETIME: Duration = Duration::from_secs(50 * 60);
const GONE_REASONS: [&str; 3] = ["BadDeviceToken", "Unregistered", "DeviceTokenNotForTopic"];

pub struct Apns {
    http: reqwest::Client,
    key: EcdsaKeyPair,
    key_id: String,
    team_id: String,
    url: Option<String>,
    token: Mutex<Option<(String, Instant)>>,
}

pub struct ApnsPush<'a> {
    pub token: &'a str,
    pub topic: String,
    pub push_type: &'static str,
    pub priority: u8,
    pub collapse_id: Option<&'a str>,
    /// When APNs gives up delivering it, in seconds since 1970.
    pub expiration: Option<i64>,
    pub sandbox: bool,
    pub payload: Value,
}

impl Apns {
    pub fn new(config: &PushConfig) -> Option<Self> {
        let (Some(key), Some(key_id), Some(team_id)) = (&config.apns_key, &config.apns_key_id, &config.apns_team_id)
        else {
            return None;
        };
        let key = pem_body(key).and_then(|der| {
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &der, &SystemRandom::new()).ok()
        });
        let Some(key) = key else {
            tracing::error!("APNS_KEY isn't the text of an APNs key's .p8 file");
            return None;
        };
        let http = reqwest::Client::builder()
            .http2_prior_knowledge()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("HTTP/2 client");
        Some(Self {
            http,
            key,
            key_id: key_id.clone(),
            team_id: team_id.clone(),
            url: config.apns_url.clone(),
            token: Mutex::new(None),
        })
    }

    fn token(&self) -> Option<String> {
        let mut cached = self.token.lock().unwrap();
        if let Some((token, made)) = &*cached
            && made.elapsed() < TOKEN_LIFETIME
        {
            return Some(token.clone());
        }
        let header = json!({ "alg": "ES256", "kid": self.key_id });
        let claims = json!({ "iss": self.team_id, "iat": chrono::Utc::now().timestamp() });
        let token = jwt(&header, &claims, |message| {
            Some(self.key.sign(&SystemRandom::new(), message).ok()?.as_ref().to_vec())
        })?;
        *cached = Some((token.clone(), Instant::now()));
        Some(token)
    }

    pub async fn send(&self, push: &ApnsPush<'_>) -> Sent {
        let Some(token) = self.token() else {
            tracing::warn!("couldn't sign an APNs token");
            return Sent::Failed;
        };
        let base = self.url.as_deref().unwrap_or(if push.sandbox { SANDBOX_URL } else { PRODUCTION_URL });
        let mut request = self
            .http
            .post(format!("{base}/3/device/{}", push.token))
            .header("authorization", format!("bearer {token}"))
            .header("apns-push-type", push.push_type)
            .header("apns-topic", &push.topic)
            .header("apns-priority", push.priority.to_string())
            .json(&push.payload);
        if let Some(collapse_id) = push.collapse_id {
            request = request.header("apns-collapse-id", collapse_id);
        }
        if let Some(expiration) = push.expiration {
            request = request.header("apns-expiration", expiration.to_string());
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!("APNs push failed: {error}");
                return Sent::Failed;
            }
        };
        let status = response.status();
        if status.is_success() {
            return Sent::Delivered;
        }
        let body: Value = response.json().await.unwrap_or_default();
        let reason = body["reason"].as_str().unwrap_or_default();
        if status == 410 || (status == 400 && GONE_REASONS.contains(&reason)) {
            return Sent::Gone;
        }
        if reason == "ExpiredProviderToken" {
            *self.token.lock().unwrap() = None;
        }
        tracing::warn!("APNs answered {status}: {reason}");
        Sent::Failed
    }
}
