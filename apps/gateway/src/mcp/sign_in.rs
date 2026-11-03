//! Signing in to an MCP server as MCP's authorization spec says: the protected resource's metadata
//! names its authorization server, the gateway registers itself there as a client, and the user
//! approves it in the browser, which comes back to `motile://mcp` with a code that only the
//! gateway, which holds the PKCE verifier, can exchange.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

pub const REDIRECT: &str = "motile://mcp";
const SIGN_IN_WAITS: Duration = Duration::from_secs(15 * 60);
const ASK_WITHIN: Duration = Duration::from_secs(20);

/// An MCP server's OAuth login, sealed in the store.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OAuth {
    pub token_endpoint: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub resource: String,
    pub access: String,
    pub refresh: Option<String>,
    pub expires_at: Option<f64>,
}

/// Where an MCP server signs in.
#[derive(Clone, Debug, PartialEq)]
pub struct Discovered {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: Option<String>,
    pub scope: Option<String>,
    pub resource: String,
}

/// What a sign-in remembers until its code comes back.
pub struct Pending {
    /// The MCP server, once it exists: one signing in again.
    pub mcp_server: Option<String>,
    pub url: String,
    pub name: Option<String>,
    verifier: String,
    token_endpoint: String,
    client_id: String,
    client_secret: Option<String>,
    resource: String,
}

#[derive(Default)]
pub struct SignIns(Mutex<HashMap<String, (Instant, Pending)>>);

impl SignIns {
    /// Registers with the MCP server's authorization server and returns the state and the page that
    /// signs in; none when the MCP server doesn't let apps register, and takes a key instead.
    pub async fn start(
        &self,
        http: &reqwest::Client,
        url: &str,
        name: Option<String>,
        mcp_server: Option<String>,
        challenge: Option<&str>,
    ) -> anyhow::Result<Option<(String, String)>> {
        let Some(found) = discover(http, url, challenge).await? else { return Ok(None) };
        let Some(registration) = found.registration_endpoint.as_deref() else { return Ok(None) };
        let (client_id, client_secret) = register(http, registration).await?;
        let verifier = random_url_safe();
        let state = random_url_safe();
        let mut query = vec![
            ("response_type", "code".to_string()),
            ("client_id", client_id.clone()),
            ("redirect_uri", REDIRECT.to_string()),
            ("code_challenge", URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))),
            ("code_challenge_method", "S256".to_string()),
            ("state", state.clone()),
            ("resource", found.resource.clone()),
        ];
        if let Some(scope) = &found.scope {
            query.push(("scope", scope.clone()));
        }
        let page = Url::parse_with_params(&found.authorization_endpoint, &query)
            .context("The MCP server's sign-in page isn't an address.")?;
        let pending = Pending {
            mcp_server,
            url: url.to_string(),
            name,
            verifier,
            token_endpoint: found.token_endpoint,
            client_id,
            client_secret,
            resource: found.resource,
        };
        let mut all = self.0.lock().await;
        all.retain(|_, (started, _)| started.elapsed() < SIGN_IN_WAITS);
        all.insert(state.clone(), (Instant::now(), pending));
        Ok(Some((state, page.to_string())))
    }

    /// Exchanges the code the sign-in's page sent back.
    pub async fn finish(&self, http: &reqwest::Client, state: &str, code: &str) -> anyhow::Result<(Pending, OAuth)> {
        let (_, pending) = self.0.lock().await.remove(state).context("This sign-in ended. Start it again.")?;
        let mut form = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT),
            ("client_id", &pending.client_id),
            ("code_verifier", &pending.verifier),
            ("resource", &pending.resource),
        ];
        if let Some(secret) = &pending.client_secret {
            form.push(("client_secret", secret));
        }
        let answer = token_request(http, &pending.token_endpoint, &form).await?;
        let login = oauth_from(&answer, &pending, None)?;
        Ok((pending, login))
    }
}

pub enum RenewError {
    /// The refresh token was refused or there is none: sign in again.
    Dead,
    Failed(anyhow::Error),
}

pub async fn renew(http: &reqwest::Client, login: &OAuth) -> Result<OAuth, RenewError> {
    let Some(refresh) = &login.refresh else { return Err(RenewError::Dead) };
    let mut form = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh.as_str()),
        ("client_id", &login.client_id),
        ("resource", &login.resource),
    ];
    if let Some(secret) = &login.client_secret {
        form.push(("client_secret", secret));
    }
    let response = http
        .post(&login.token_endpoint)
        .form(&form)
        .timeout(ASK_WITHIN)
        .send()
        .await
        .map_err(|error| RenewError::Failed(error.into()))?;
    let status = response.status();
    let answer: Value = response.json().await.unwrap_or_default();
    if status.as_u16() == 400 || status.as_u16() == 401 {
        return Err(RenewError::Dead);
    }
    if !status.is_success() {
        return Err(RenewError::Failed(anyhow::anyhow!("The login couldn't be renewed ({status}).")));
    }
    let mut renewed = login.clone();
    renewed.access = answer["access_token"].as_str().map(String::from).ok_or(RenewError::Dead)?;
    if let Some(refresh) = answer["refresh_token"].as_str() {
        renewed.refresh = Some(refresh.to_string());
    }
    renewed.expires_at = answer["expires_in"].as_f64().map(|seconds| motile_protocol::now() + seconds);
    Ok(renewed)
}

/// Finds where the MCP server at `url` signs in, from what its refusal said (`challenge`), its
/// protected resource metadata, and its authorization server's metadata; none when it has neither,
/// as one that takes a key.
pub async fn discover(
    http: &reqwest::Client,
    url: &str,
    challenge: Option<&str>,
) -> anyhow::Result<Option<Discovered>> {
    let address = Url::parse(url).context("That isn't an address.")?;
    let origin = address.origin().ascii_serialization();
    let path = address.path().trim_end_matches('/');
    let said = challenge.map(challenge_params).unwrap_or_default();
    let mut candidates = Vec::new();
    if let Some(metadata) = said.get("resource_metadata") {
        candidates.push(metadata.clone());
    }
    if !path.is_empty() {
        candidates.push(format!("{origin}/.well-known/oauth-protected-resource{path}"));
    }
    candidates.push(format!("{origin}/.well-known/oauth-protected-resource"));
    let mut resource_metadata = None;
    for candidate in candidates {
        if let Some(found) = fetch_json(http, &candidate).await {
            resource_metadata = Some(found);
            break;
        }
    }
    let issuer = resource_metadata
        .as_ref()
        .and_then(|metadata| metadata["authorization_servers"][0].as_str())
        .map(String::from)
        .unwrap_or_else(|| origin.clone());
    let resource = resource_metadata
        .as_ref()
        .and_then(|metadata| metadata["resource"].as_str())
        .map(String::from)
        .unwrap_or_else(|| url.to_string());
    let scope = said.get("scope").cloned().or_else(|| {
        let scopes = resource_metadata.as_ref()?["scopes_supported"].as_array()?;
        let scopes: Vec<&str> = scopes.iter().filter_map(Value::as_str).collect();
        (!scopes.is_empty()).then(|| scopes.join(" "))
    });
    let Some(server) = authorization_metadata(http, &issuer).await else { return Ok(None) };
    let endpoint = |name: &str, fallback: &str| {
        server[name].as_str().map(String::from).unwrap_or_else(|| format!("{}{fallback}", issuer.trim_end_matches('/')))
    };
    Ok(Some(Discovered {
        authorization_endpoint: endpoint("authorization_endpoint", "/authorize"),
        token_endpoint: endpoint("token_endpoint", "/token"),
        registration_endpoint: server["registration_endpoint"].as_str().map(String::from),
        scope,
        resource,
    }))
}

async fn authorization_metadata(http: &reqwest::Client, issuer: &str) -> Option<Value> {
    let address = Url::parse(issuer).ok()?;
    let origin = address.origin().ascii_serialization();
    let path = address.path().trim_end_matches('/');
    let candidates = match path.is_empty() {
        true => vec![
            format!("{origin}/.well-known/oauth-authorization-server"),
            format!("{origin}/.well-known/openid-configuration"),
        ],
        false => vec![
            format!("{origin}/.well-known/oauth-authorization-server{path}"),
            format!("{origin}/.well-known/openid-configuration{path}"),
            format!("{origin}{path}/.well-known/openid-configuration"),
        ],
    };
    for candidate in candidates {
        if let Some(found) = fetch_json(http, &candidate).await {
            return Some(found);
        }
    }
    None
}

async fn fetch_json(http: &reqwest::Client, url: &str) -> Option<Value> {
    let response = http.get(url).header("accept", "application/json").timeout(ASK_WITHIN).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<Value>().await.ok().filter(Value::is_object)
}

async fn register(http: &reqwest::Client, endpoint: &str) -> anyhow::Result<(String, Option<String>)> {
    let response = http
        .post(endpoint)
        .json(&json!({
            "client_name": "Motile",
            "client_uri": "https://motile.app",
            "redirect_uris": [REDIRECT],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        }))
        .timeout(ASK_WITHIN)
        .send()
        .await
        .context("The MCP server's sign-in couldn't be reached.")?;
    let status = response.status();
    let answer: Value = response.json().await.unwrap_or_default();
    if !status.is_success() {
        let said = answer["error_description"].as_str().or(answer["error"].as_str()).unwrap_or_default();
        bail!("The MCP server refused to register Motile ({status}). {said}");
    }
    let client_id = answer["client_id"].as_str().context("The MCP server registered Motile without an id.")?;
    Ok((client_id.to_string(), answer["client_secret"].as_str().map(String::from)))
}

async fn token_request(http: &reqwest::Client, endpoint: &str, form: &[(&str, &str)]) -> anyhow::Result<Value> {
    let response = http
        .post(endpoint)
        .header("accept", "application/json")
        .form(form)
        .timeout(ASK_WITHIN)
        .send()
        .await
        .context("The MCP server's sign-in couldn't be reached.")?;
    let status = response.status();
    let answer: Value = response.json().await.unwrap_or_default();
    if !status.is_success() {
        let said = answer["error_description"].as_str().or(answer["error"].as_str()).unwrap_or_default();
        bail!("The sign-in was refused ({status}). {said}");
    }
    Ok(answer)
}

fn oauth_from(answer: &Value, pending: &Pending, before: Option<&OAuth>) -> anyhow::Result<OAuth> {
    let access = answer["access_token"].as_str().context("The sign-in gave no token.")?;
    Ok(OAuth {
        token_endpoint: pending.token_endpoint.clone(),
        client_id: pending.client_id.clone(),
        client_secret: pending.client_secret.clone(),
        resource: pending.resource.clone(),
        access: access.to_string(),
        refresh: answer["refresh_token"].as_str().map(String::from).or(before.and_then(|login| login.refresh.clone())),
        expires_at: answer["expires_in"].as_f64().map(|seconds| motile_protocol::now() + seconds),
    })
}

/// The parameters of a `WWW-Authenticate: Bearer …` challenge.
fn challenge_params(challenge: &str) -> HashMap<String, String> {
    let rest = challenge.trim().strip_prefix("Bearer").unwrap_or(challenge);
    let mut params = HashMap::new();
    let mut rest = rest.trim_start();
    while let Some(equals) = rest.find('=') {
        let name = rest[..equals].trim().trim_start_matches(',').trim().to_ascii_lowercase();
        let after = &rest[equals + 1..];
        let (value, next) = match after.strip_prefix('"') {
            Some(quoted) => match quoted.find('"') {
                Some(end) => (&quoted[..end], &quoted[end + 1..]),
                None => (quoted, ""),
            },
            None => match after.find(',') {
                Some(end) => (&after[..end], &after[end..]),
                None => (after, ""),
            },
        };
        params.insert(name, value.trim().to_string());
        rest = next.trim_start_matches([',', ' ']);
    }
    params
}

fn random_url_safe() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS provides randomness");
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_challenge_names_its_metadata_and_scope() {
        let params = challenge_params(
            r#"Bearer error="invalid_token", resource_metadata="https://mcp.example.com/.well-known/oauth-protected-resource", scope="read write""#,
        );
        assert_eq!(params["resource_metadata"], "https://mcp.example.com/.well-known/oauth-protected-resource");
        assert_eq!(params["scope"], "read write");
        assert_eq!(params["error"], "invalid_token");
        assert_eq!(challenge_params("Bearer realm=mcp, scope=read")["scope"], "read");
    }
}
