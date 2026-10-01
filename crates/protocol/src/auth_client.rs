//! Calls to the auth server. A linked device signs its requests with its key.

use std::time::Duration;

use anyhow::bail;
use reqwest::Method;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::auth_api::{
    ApiError, DevLoginRequest, DevLoginResponse, EnrollRequest, EnrollResponse, EnrollToken, ExchangeRequest, Me,
    enroll_message, link_message,
};
use crate::identity::{DeviceKey, sha256_hex, sign_request};
use crate::{APP_REDIRECT, now};

#[derive(Clone)]
pub struct AuthClient {
    base_url: String,
    http: reqwest::Client,
}

pub struct DeviceDescription<'a> {
    pub name: &'a str,
    pub platform: &'a str,
}

impl AuthClient {
    pub fn new(base_url: &str) -> Self {
        crate::tls::install();
        let http =
            reqwest::Client::builder().timeout(Duration::from_secs(15)).build().expect("a TLS backend is built in");
        Self { base_url: base_url.trim_end_matches('/').to_string(), http }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The page the browser opens to sign in. It ends at `motile://auth?code=…&state=…`.
    pub fn sign_in_url(&self, verifier: &str, state: &str) -> String {
        let base = format!("{}/auth/start", self.base_url);
        let parameters = [
            ("challenge", sha256_hex(verifier.as_bytes())),
            ("state", state.to_string()),
            ("redirect", APP_REDIRECT.to_string()),
        ];
        reqwest::Url::parse_with_params(&base, parameters).map(String::from).unwrap_or(base)
    }

    async fn send<T: DeserializeOwned>(&self, request: reqwest::RequestBuilder) -> anyhow::Result<T> {
        let Ok(response) = request.send().await else {
            bail!("{} can't be reached.", self.base_url);
        };
        let status = response.status();
        if status.is_success() {
            return Ok(response.json().await?);
        }
        match response.json::<ApiError>().await {
            Ok(body) => bail!("{}", body.error),
            Err(_) => bail!("{} answered {status}.", self.base_url),
        }
    }

    fn signed(&self, key: &DeviceKey, method: Method, path: &str, body: &[u8]) -> reqwest::RequestBuilder {
        let authorization = sign_request(key, method.as_str(), path, body, now());
        let request =
            self.http.request(method, format!("{}{path}", self.base_url)).header("authorization", authorization);
        if body.is_empty() {
            return request;
        }
        request.header("content-type", "application/json").body(body.to_vec())
    }

    fn post<B: Serialize>(&self, path: &str, body: &B) -> reqwest::RequestBuilder {
        self.http.post(format!("{}{path}", self.base_url)).json(body)
    }

    pub async fn me(&self, key: &DeviceKey) -> anyhow::Result<Me> {
        self.send(self.signed(key, Method::GET, "/api/me", b"")).await
    }

    /// Links this device to the account that signed in.
    pub async fn exchange(
        &self,
        key: &DeviceKey,
        code: &str,
        verifier: &str,
        device: &DeviceDescription<'_>,
    ) -> anyhow::Result<Me> {
        let request = ExchangeRequest {
            code: code.to_string(),
            verifier: verifier.to_string(),
            public_key: key.public(),
            name: device.name.to_string(),
            platform: device.platform.to_string(),
            signature: key.sign(&link_message(code)),
        };
        self.send(self.post("/api/auth/exchange", &request)).await
    }

    /// Signs in without Google. Only answered by an auth server started with `DEV_LOGIN=1`.
    pub async fn dev_login(&self, verifier: &str, email: &str) -> anyhow::Result<String> {
        let request = DevLoginRequest { challenge: sha256_hex(verifier.as_bytes()), email: email.to_string() };
        let response: DevLoginResponse = self.send(self.post("/api/dev/login", &request)).await?;
        Ok(response.code)
    }

    pub async fn create_enroll_token(&self, key: &DeviceKey) -> anyhow::Result<EnrollToken> {
        self.send(self.signed(key, Method::POST, "/api/enroll-tokens", b"")).await
    }

    pub async fn enroll(
        &self,
        key: &DeviceKey,
        token: &str,
        device: &DeviceDescription<'_>,
    ) -> anyhow::Result<EnrollResponse> {
        let request = EnrollRequest {
            token: token.to_string(),
            public_key: key.public(),
            name: device.name.to_string(),
            platform: device.platform.to_string(),
            signature: key.sign(&enroll_message(token)),
        };
        self.send(self.post("/api/enroll", &request)).await
    }

    /// Removes a device from the account; `public_key` may be the caller's own.
    pub async fn remove_device(&self, key: &DeviceKey, public_key: &str) -> anyhow::Result<()> {
        let path = format!("/api/devices/{public_key}");
        let _: serde_json::Value = self.send(self.signed(key, Method::DELETE, &path, b"")).await?;
        Ok(())
    }
}
