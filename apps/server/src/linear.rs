//! The server's connection to Linear: the user approves Motile's application in a browser, and
//! the token Linear grants for it is kept here, never anywhere else.

use std::time::Duration;

use anyhow::{Context, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use motile_protocol::wire::LinearConnection;
use motile_protocol::{LINEAR_REDIRECT, now};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::store::Store;

/// Motile's application at Linear. It has no secret: a code is exchanged with the verifier that
/// its sign-in page was opened with.
const CLIENT_ID: &str = "8f0db699b33a443cbf216ca079d32fe9";
const AUTHORIZE_URL: &str = "https://linear.app";
const API_URL: &str = "https://api.linear.app";
const SCOPE: &str = "read,write";
const GRANT: &str = "linear";
const TIMEOUT: Duration = Duration::from_secs(20);

pub struct Linear {
    client_id: String,
    authorize_url: String,
    api_url: String,
    /// The connection that waits for the user to approve it.
    pending: std::sync::Mutex<Option<Pending>>,
}

struct Pending {
    state: String,
    verifier: String,
}

/// What Linear granted, as it is kept in the settings.
#[derive(Serialize, Deserialize)]
struct Grant {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: f64,
    connection: LinearConnection,
}

#[derive(Deserialize)]
struct Granted {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: f64,
}

impl Linear {
    /// `MOTILE_LINEAR_URL` is a stand-in for Linear, and `MOTILE_LINEAR_CLIENT_ID` another
    /// application than Motile's.
    pub fn from_environment() -> Self {
        let stand_in = std::env::var("MOTILE_LINEAR_URL").ok();
        let client_id = std::env::var("MOTILE_LINEAR_CLIENT_ID").unwrap_or_else(|_| CLIENT_ID.to_string());
        let authorize_url = stand_in.clone().unwrap_or_else(|| AUTHORIZE_URL.to_string());
        let api_url = stand_in.unwrap_or_else(|| API_URL.to_string());
        Self { client_id, authorize_url, api_url, pending: std::sync::Mutex::default() }
    }

    pub fn connection(&self, store: &Store) -> Option<LinearConnection> {
        grant(store).map(|grant| grant.connection)
    }

    /// The page where the user approves the connection. Only the code it ends with can finish it.
    pub fn connect(&self) -> anyhow::Result<String> {
        let state = uuid::Uuid::new_v4().simple().to_string();
        let verifier = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(&verifier));
        let parameters = [
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", LINEAR_REDIRECT),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("state", &state),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ];
        let url = reqwest::Url::parse_with_params(&format!("{}/oauth/authorize", self.authorize_url), parameters)?;
        *self.pending.lock().unwrap() = Some(Pending { state, verifier });
        Ok(url.into())
    }

    pub async fn finish(&self, store: &Store, code: &str, state: &str) -> anyhow::Result<LinearConnection> {
        let pending = self.pending.lock().unwrap().take_if(|pending| pending.state == state);
        let Some(pending) = pending else {
            bail!("This isn't the connection that was started here. Connect Linear again.");
        };
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", LINEAR_REDIRECT),
            ("client_id", &self.client_id),
            ("code_verifier", &pending.verifier),
        ];
        let answer = http()?.post(format!("{}/oauth/token", self.api_url)).form(&form).send().await;
        let answer = answer.context("Linear couldn't be reached.")?;
        if !answer.status().is_success() {
            bail!("Linear refused the connection: {}", refusal(answer).await);
        }
        let granted: Granted = answer.json().await.context("Linear's answer couldn't be read.")?;
        let connection = self.who(&granted.access_token).await?;
        let grant = Grant {
            access_token: granted.access_token,
            refresh_token: granted.refresh_token,
            expires_at: now() + granted.expires_in,
            connection: connection.clone(),
        };
        store.set_setting(GRANT, Some(&serde_json::to_string(&grant)?))?;
        Ok(connection)
    }

    /// Forgets the grant even when Linear can't be told, so that a server can always be
    /// disconnected.
    pub async fn disconnect(&self, store: &Store) -> anyhow::Result<()> {
        let Some(grant) = grant(store) else { return Ok(()) };
        let (token, hint) = match &grant.refresh_token {
            Some(token) => (token, "refresh_token"),
            None => (&grant.access_token, "access_token"),
        };
        let form = [("token", token.as_str()), ("token_type_hint", hint)];
        let revoked = http()?.post(format!("{}/oauth/revoke", self.api_url)).form(&form).send().await;
        if let Err(error) = revoked.and_then(|answer| answer.error_for_status()) {
            tracing::warn!("Linear didn't take the grant back: {error}");
        }
        store.set_setting(GRANT, None)?;
        Ok(())
    }

    async fn who(&self, access_token: &str) -> anyhow::Result<LinearConnection> {
        let query = json!({ "query": "{ viewer { name } organization { name } }" });
        let asked = http()?.post(format!("{}/graphql", self.api_url)).bearer_auth(access_token).json(&query);
        let answer = asked.send().await.context("Linear couldn't be reached.")?;
        if !answer.status().is_success() {
            bail!("Linear didn't say whose workspace this is: {}", refusal(answer).await);
        }
        let answer: Value = answer.json().await.context("Linear's answer couldn't be read.")?;
        let name = |of: &str| answer["data"][of]["name"].as_str().map(str::to_string);
        let (Some(workspace), Some(user)) = (name("organization"), name("viewer")) else {
            bail!("Linear didn't say whose workspace this is.");
        };
        Ok(LinearConnection { workspace, user })
    }
}

fn grant(store: &Store) -> Option<Grant> {
    serde_json::from_str(&store.setting(GRANT)?).ok()
}

fn http() -> anyhow::Result<reqwest::Client> {
    motile_protocol::tls::install();
    Ok(reqwest::Client::builder().timeout(TIMEOUT).build()?)
}

/// Why Linear refused, in its words where it gave any.
async fn refusal(answer: reqwest::Response) -> String {
    let status = answer.status();
    let said: Value = answer.json().await.unwrap_or_default();
    let words = said["error_description"].as_str().or(said["error"].as_str());
    words.map(str::to_string).unwrap_or_else(|| status.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    type Asked = Arc<Mutex<Vec<(String, String)>>>;

    /// Stands in for Linear: grants a token for the code `good`, and keeps the path and the body
    /// of what it was asked.
    async fn fake_linear() -> (Linear, Asked) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let asked = Asked::default();
        let heard = asked.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let (path, body) = read_request(&mut stream).await;
                let (status, answer) = match path.as_str() {
                    "/oauth/token" if body.contains("code=good&") => {
                        ("200 OK", json!({ "access_token": "access", "refresh_token": "refresh", "expires_in": 86399 }))
                    }
                    "/oauth/token" => (
                        "400 Bad Request",
                        json!({ "error": "invalid_grant", "error_description": "authorization code is invalid" }),
                    ),
                    "/graphql" => (
                        "200 OK",
                        json!({ "data": { "viewer": { "name": "Ada" }, "organization": { "name": "Engines" } } }),
                    ),
                    _ => ("200 OK", json!({})),
                };
                heard.lock().unwrap().push((path, body));
                let answer = answer.to_string();
                let reply = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
                    answer.len()
                );
                let _ = stream.write_all(reply.as_bytes()).await;
            }
        });
        let linear = Linear {
            client_id: "motile".to_string(),
            authorize_url: url.clone(),
            api_url: url,
            pending: std::sync::Mutex::default(),
        };
        (linear, asked)
    }

    async fn read_request(stream: &mut tokio::net::TcpStream) -> (String, String) {
        let mut read = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let count = stream.read(&mut buffer).await.unwrap();
            read.extend_from_slice(&buffer[..count]);
            let text = String::from_utf8_lossy(&read).to_string();
            let Some((head, body)) = text.split_once("\r\n\r\n") else { continue };
            let length =
                head.lines().find_map(|line| line.to_lowercase().strip_prefix("content-length: ")?.parse().ok());
            if count == 0 || body.len() >= length.unwrap_or(0) {
                let path = head.split(' ').nth(1).unwrap_or_default().to_string();
                return (path, body.to_string());
            }
        }
    }

    fn store(dir: &tempfile::TempDir) -> Store {
        Store::open(&dir.path().join("motile.sqlite")).unwrap()
    }

    fn parameters(url: &str) -> HashMap<String, String> {
        reqwest::Url::parse(url).unwrap().query_pairs().into_owned().collect()
    }

    fn ada() -> LinearConnection {
        LinearConnection { workspace: "Engines".to_string(), user: "Ada".to_string() }
    }

    #[tokio::test]
    async fn an_approved_connection_is_exchanged_with_its_verifier_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let (linear, asked) = fake_linear().await;
        let page = parameters(&linear.connect().unwrap());
        assert_eq!(page["client_id"], "motile");
        assert_eq!(page["redirect_uri"], "motile://linear");

        let connection = linear.finish(&store(&dir), "good", &page["state"]).await.unwrap();
        assert_eq!(connection, ada());

        let (_, exchange) = asked.lock().unwrap()[0].clone();
        let sent: HashMap<String, String> =
            reqwest::Url::parse(&format!("http://linear/?{exchange}")).unwrap().query_pairs().into_owned().collect();
        assert_eq!(URL_SAFE_NO_PAD.encode(Sha256::digest(&sent["code_verifier"])), page["code_challenge"]);
        assert!(!sent.contains_key("client_secret"));
        assert_eq!(linear.connection(&store(&dir)), Some(ada()));
    }

    #[tokio::test]
    async fn a_code_is_refused_without_the_state_it_was_asked_with_or_when_linear_refuses_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (linear, asked) = fake_linear().await;
        let page = parameters(&linear.connect().unwrap());

        assert!(linear.finish(&store, "good", "another").await.is_err());
        assert!(asked.lock().unwrap().is_empty());

        let refused = linear.finish(&store, "bad", &page["state"]).await.unwrap_err();
        assert_eq!(refused.to_string(), "Linear refused the connection: authorization code is invalid");
        assert!(linear.finish(&store, "good", &page["state"]).await.is_err(), "a state works once");
        assert_eq!(linear.connection(&store), None);
    }

    #[tokio::test]
    async fn disconnecting_takes_the_grant_back_at_linear_and_forgets_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let (linear, asked) = fake_linear().await;
        let page = parameters(&linear.connect().unwrap());
        linear.finish(&store, "good", &page["state"]).await.unwrap();

        linear.disconnect(&store).await.unwrap();

        let (path, body) = asked.lock().unwrap().last().unwrap().clone();
        assert_eq!((path.as_str(), body.as_str()), ("/oauth/revoke", "token=refresh&token_type_hint=refresh_token"));
        assert_eq!(linear.connection(&store), None);
    }
}
