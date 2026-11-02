//! Signing accounts in and keeping their tokens fresh, as the agents' own CLIs do. Claude: OAuth with
//! PKCE, the user pastes back the code its page shows (`claude auth login`). Codex: the device code
//! flow (`codex login --device-auth`), approved in the browser while the proxy waits.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use motile_protocol::proxy_wire::ProxySignIn;
use motile_protocol::wire::Agent;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::accounts::Tokens;
use crate::config::Upstreams;
use crate::limits::{capitalized, codex_plan};

const CLAUDE_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const CLAUDE_REDIRECT: &str = "https://platform.claude.com/oauth/code/callback";
const CLAUDE_SCOPES: &str =
    "user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload user:plugins";
pub const CLAUDE_OAUTH_BETA: &str = "oauth-2025-04-20";
const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
/// How long a sign-in waits to be finished, and a device code to be approved.
const SIGN_IN_WAITS: Duration = Duration::from_secs(15 * 60);
/// When the token's lifetime isn't said: Codex refreshes its tokens every eight days.
const UNSAID_LIFETIME: f64 = 8.0 * 86400.0;

/// An account's identity and tokens, as a sign-in or a renewal gives them.
#[derive(Debug)]
pub struct Signed {
    pub id: String,
    pub agent: Agent,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub tokens: Tokens,
}

#[derive(Debug)]
pub enum RenewError {
    /// The refresh token stopped working; the account has to be signed in again.
    Dead,
    Failed(anyhow::Error),
}

enum Pending {
    Claude { verifier: String, state: String },
    Codex { device_auth_id: String, user_code: String, interval: Duration },
}

#[derive(Default)]
pub struct SignIns {
    pending: Mutex<HashMap<String, (Instant, Pending)>>,
}

impl SignIns {
    pub async fn start(
        &self,
        http: &reqwest::Client,
        upstreams: &Upstreams,
        agent: Agent,
    ) -> anyhow::Result<ProxySignIn> {
        let id = uuid::Uuid::new_v4().to_string();
        let (sign_in, pending) = match agent {
            Agent::Claude => {
                let verifier = random_url_safe();
                let state = random_url_safe();
                let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
                let query = [
                    ("code", "true"),
                    ("client_id", CLAUDE_CLIENT_ID),
                    ("response_type", "code"),
                    ("redirect_uri", CLAUDE_REDIRECT),
                    ("scope", CLAUDE_SCOPES),
                    ("code_challenge", &challenge),
                    ("code_challenge_method", "S256"),
                    ("state", &state),
                ];
                let url = reqwest::Url::parse_with_params(&upstreams.claude_authorize, query)?;
                let sign_in = ProxySignIn { id: id.clone(), agent, url: url.to_string(), user_code: None };
                (sign_in, Pending::Claude { verifier, state })
            }
            Agent::Codex => {
                let response = http
                    .post(format!("{}/api/accounts/deviceauth/usercode", upstreams.openai_auth))
                    .json(&json!({"client_id": CODEX_CLIENT_ID}))
                    .send()
                    .await
                    .context("OpenAI's sign-in couldn't be reached.")?;
                if !response.status().is_success() {
                    bail!("OpenAI didn't give a code to sign in with ({}).", response.status());
                }
                let answer: Value = response.json().await?;
                let device_auth_id = answer["device_auth_id"].as_str().context("OpenAI gave no device code.")?;
                let user_code =
                    answer["user_code"].as_str().or(answer["usercode"].as_str()).context("No user code.")?;
                let interval = text_or_number(&answer["interval"]).unwrap_or(5).max(1);
                let sign_in = ProxySignIn {
                    id: id.clone(),
                    agent,
                    url: format!("{}/codex/device", upstreams.openai_auth),
                    user_code: Some(user_code.to_string()),
                };
                let pending = Pending::Codex {
                    device_auth_id: device_auth_id.to_string(),
                    user_code: user_code.to_string(),
                    interval: Duration::from_secs(interval),
                };
                (sign_in, pending)
            }
        };
        let mut all = self.pending.lock().await;
        all.retain(|_, (started, _)| started.elapsed() < SIGN_IN_WAITS);
        all.insert(id, (Instant::now(), pending));
        Ok(sign_in)
    }

    /// Finishes a sign-in: Claude's with the code its page showed, Codex's once its code is approved.
    pub async fn finish(
        &self,
        http: &reqwest::Client,
        upstreams: &Upstreams,
        id: &str,
        code: Option<&str>,
    ) -> anyhow::Result<Signed> {
        let started = self.pending.lock().await.remove(id);
        let Some((started, pending)) = started.filter(|(started, _)| started.elapsed() < SIGN_IN_WAITS) else {
            bail!("That sign-in stopped. Sign in again.");
        };
        match pending {
            Pending::Claude { verifier, state } => {
                let answer = match claude_exchange(http, upstreams, code, &verifier, &state).await {
                    Ok(answer) => answer,
                    Err(error) => {
                        self.pending
                            .lock()
                            .await
                            .insert(id.to_string(), (started, Pending::Claude { verifier, state }));
                        return Err(error);
                    }
                };
                let tokens = claude_tokens(&answer, None)?;
                let id = answer["organization"]["uuid"].as_str().context("Claude didn't say which organisation.")?;
                let (email, plan) = claude_profile(http, upstreams, &tokens.access).await.unwrap_or_else(|error| {
                    tracing::warn!("couldn't read the Claude account's profile: {error:#}");
                    (None, None)
                });
                let email = email.or_else(|| answer["account"]["email_address"].as_str().map(String::from));
                Ok(Signed { id: id.to_string(), agent: Agent::Claude, email, plan, tokens })
            }
            Pending::Codex { device_auth_id, user_code, interval } => {
                let deadline = started + SIGN_IN_WAITS;
                let approved = loop {
                    if Instant::now() > deadline {
                        bail!("The code wasn't approved in time. Sign in again.");
                    }
                    let response = http
                        .post(format!("{}/api/accounts/deviceauth/token", upstreams.openai_auth))
                        .json(&json!({"device_auth_id": device_auth_id, "user_code": user_code}))
                        .send()
                        .await?;
                    match response.status().as_u16() {
                        200..=299 => break response.json::<Value>().await?,
                        403 | 404 => tokio::time::sleep(interval).await,
                        status => bail!("OpenAI refused the sign-in ({status}). Sign in again."),
                    }
                };
                let code = approved["authorization_code"].as_str().context("OpenAI gave no code.")?;
                let verifier = approved["code_verifier"].as_str().context("OpenAI gave no verifier.")?;
                let form = [
                    ("grant_type", "authorization_code"),
                    ("client_id", CODEX_CLIENT_ID),
                    ("code", code),
                    ("redirect_uri", &format!("{}/deviceauth/callback", upstreams.openai_auth)),
                    ("code_verifier", verifier),
                ];
                let response = http.post(format!("{}/oauth/token", upstreams.openai_auth)).form(&form).send().await?;
                if !response.status().is_success() {
                    tracing::warn!("OpenAI refused the code: {}", response.text().await.unwrap_or_default());
                    bail!("OpenAI didn't sign the account in. Sign in again.");
                }
                codex_signed(&response.json().await?, None)
            }
        }
    }
}

/// New tokens for an account, from its refresh token.
pub async fn renew(
    http: &reqwest::Client,
    upstreams: &Upstreams,
    agent: Agent,
    tokens: &Tokens,
) -> Result<Signed, RenewError> {
    let request = match agent {
        Agent::Claude => http.post(&upstreams.claude_token).json(&json!({
            "grant_type": "refresh_token",
            "refresh_token": tokens.refresh,
            "client_id": CLAUDE_CLIENT_ID,
            "scope": CLAUDE_SCOPES,
        })),
        Agent::Codex => http.post(format!("{}/oauth/token", upstreams.openai_auth)).json(&json!({
            "client_id": CODEX_CLIENT_ID,
            "grant_type": "refresh_token",
            "refresh_token": tokens.refresh,
        })),
    };
    let failed = |error: anyhow::Error| RenewError::Failed(error);
    let response = request.send().await.map_err(|error| failed(error.into()))?;
    let status = response.status().as_u16();
    let answer: Value = response.json().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        if is_dead(agent, status, &answer) {
            return Err(RenewError::Dead);
        }
        return Err(failed(anyhow::anyhow!("The tokens couldn't be renewed ({status}).")));
    }
    let renewed = match agent {
        Agent::Claude => claude_tokens(&answer, Some(tokens)).map(|renewed| Signed {
            id: String::new(),
            agent,
            email: None,
            plan: None,
            tokens: renewed,
        }),
        Agent::Codex => codex_signed(&answer, Some(tokens)),
    };
    renewed.map_err(failed)
}

fn is_dead(agent: Agent, status: u16, answer: &Value) -> bool {
    let error = &answer["error"];
    let code = error.as_str().or(error["code"].as_str()).or(error["type"].as_str()).or(answer["code"].as_str());
    match agent {
        Agent::Claude => matches!(status, 400 | 401) && code == Some("invalid_grant"),
        Agent::Codex => {
            status == 401
                || matches!(
                    code,
                    Some(
                        "refresh_token_expired"
                            | "refresh_token_reused"
                            | "refresh_token_invalidated"
                            | "invalid_grant"
                    )
                )
        }
    }
}

/// Trades the code Claude's page showed for the account's tokens. The sign-in stays open when it
/// fails, so that the code can be pasted again.
async fn claude_exchange(
    http: &reqwest::Client,
    upstreams: &Upstreams,
    code: Option<&str>,
    verifier: &str,
    state: &str,
) -> anyhow::Result<Value> {
    let code = code.map(str::trim).filter(|code| !code.is_empty()).context("Paste the code the page shows.")?;
    let code = code.split('#').next().unwrap_or(code);
    let body = json!({
        "grant_type": "authorization_code",
        "code": code,
        "redirect_uri": CLAUDE_REDIRECT,
        "client_id": CLAUDE_CLIENT_ID,
        "code_verifier": verifier,
        "state": state,
    });
    let response =
        http.post(&upstreams.claude_token).json(&body).send().await.context("Claude couldn't be reached.")?;
    if !response.status().is_success() {
        tracing::warn!("Claude's sign-in refused its code: {}", response.text().await.unwrap_or_default());
        bail!("That code didn't work. Paste it again.");
    }
    Ok(response.json().await?)
}

fn claude_tokens(answer: &Value, before: Option<&Tokens>) -> anyhow::Result<Tokens> {
    let access = answer["access_token"].as_str().context("Claude gave no token.")?;
    let refresh =
        answer["refresh_token"].as_str().map(String::from).or_else(|| before.map(|before| before.refresh.clone()));
    let lifetime = answer["expires_in"].as_f64().unwrap_or(3600.0);
    Ok(Tokens {
        access: access.to_string(),
        refresh: refresh.context("Claude gave no refresh token.")?,
        expires_at: motile_protocol::now() + lifetime,
        chatgpt_account: None,
    })
}

/// Who is signed in to a Claude account and its plan, as Claude's profile says.
pub async fn claude_profile(
    http: &reqwest::Client,
    upstreams: &Upstreams,
    access: &str,
) -> anyhow::Result<(Option<String>, Option<String>)> {
    let response = http
        .get(format!("{}/api/oauth/profile", upstreams.anthropic))
        .bearer_auth(access)
        .header("anthropic-beta", CLAUDE_OAUTH_BETA)
        .send()
        .await?
        .error_for_status()?;
    let profile: Value = response.json().await?;
    let email = profile["account"]["email"].as_str().map(String::from);
    Ok((email, claude_plan(&profile["organization"])))
}

fn claude_plan(organization: &Value) -> Option<String> {
    let kind = organization["organization_type"].as_str()?;
    let tier = organization["rate_limit_tier"].as_str().unwrap_or_default();
    Some(match kind.trim_start_matches("claude_") {
        "max" if tier.contains("20x") => "Max 20x".to_string(),
        "max" => "Max".to_string(),
        plan => capitalized(plan),
    })
}

fn codex_signed(answer: &Value, before: Option<&Tokens>) -> anyhow::Result<Signed> {
    let kept = |key: &str, old: Option<&String>| answer[key].as_str().map(String::from).or_else(|| old.cloned());
    let access = kept("access_token", before.map(|before| &before.access)).context("OpenAI gave no token.")?;
    let refresh =
        kept("refresh_token", before.map(|before| &before.refresh)).context("OpenAI gave no refresh token.")?;
    let claims = answer["id_token"].as_str().and_then(jwt_claims).unwrap_or_default();
    let auth = &claims["https://api.openai.com/auth"];
    let chatgpt_account = auth["chatgpt_account_id"]
        .as_str()
        .map(String::from)
        .or_else(|| before.and_then(|before| before.chatgpt_account.clone()))
        .context("OpenAI didn't say which ChatGPT account signed in.")?;
    let email = claims["email"].as_str().or(claims["https://api.openai.com/profile"]["email"].as_str());
    let expires_at = jwt_claims(&access).and_then(|claims| claims["exp"].as_f64());
    Ok(Signed {
        id: chatgpt_account.clone(),
        agent: Agent::Codex,
        email: email.map(String::from),
        plan: auth["chatgpt_plan_type"].as_str().map(codex_plan),
        tokens: Tokens {
            access,
            refresh,
            expires_at: expires_at.unwrap_or_else(|| motile_protocol::now() + UNSAID_LIFETIME),
            chatgpt_account: Some(chatgpt_account),
        },
    })
}

/// A JWT's claims, unverified: the token came straight from OpenAI over TLS.
fn jwt_claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?).ok()
}

fn text_or_number(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.trim().parse().ok())
}

fn random_url_safe() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS provides randomness");
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt(claims: Value) -> String {
        format!("e30.{}.sig", URL_SAFE_NO_PAD.encode(claims.to_string()))
    }

    #[test]
    fn a_codex_account_is_its_chatgpt_account_with_the_email_and_plan_its_id_token_names() {
        let id_token = jwt(json!({"email": "a@b.c", "https://api.openai.com/auth": {
            "chatgpt_account_id": "acct-1", "chatgpt_plan_type": "pro"}}));
        let answer =
            json!({"id_token": id_token, "access_token": jwt(json!({"exp": 2000000000})), "refresh_token": "r"});
        let signed = codex_signed(&answer, None).unwrap();
        assert_eq!(
            (signed.id.as_str(), signed.email.as_deref(), signed.plan.as_deref()),
            ("acct-1", Some("a@b.c"), Some("Pro"))
        );
        assert_eq!(
            (signed.tokens.expires_at, signed.tokens.chatgpt_account.as_deref()),
            (2000000000.0, Some("acct-1"))
        );

        let renewed = codex_signed(&json!({"access_token": "x.e30.y"}), Some(&signed.tokens)).unwrap();
        assert_eq!((renewed.tokens.refresh.as_str(), renewed.id.as_str()), ("r", "acct-1"));
    }

    #[test]
    fn a_claude_plan_is_its_organisations_kind_and_tier() {
        let plan = |kind: &str, tier: &str| claude_plan(&json!({"organization_type": kind, "rate_limit_tier": tier}));
        assert_eq!(plan("claude_max", "default_claude_max_20x").as_deref(), Some("Max 20x"));
        assert_eq!(plan("claude_max", "default_claude_max_5x").as_deref(), Some("Max"));
        assert_eq!(plan("claude_pro", "").as_deref(), Some("Pro"));
    }

    #[test]
    fn only_a_refused_refresh_token_makes_a_account_sign_in_again() {
        assert!(is_dead(
            Agent::Claude,
            400,
            &json!({"error": "invalid_grant", "error_description": "Refresh token expired"})
        ));
        assert!(!is_dead(Agent::Claude, 500, &json!({"error": "invalid_grant"})));
        assert!(!is_dead(Agent::Claude, 429, &json!({})));
        assert!(is_dead(Agent::Codex, 400, &json!({"error": {"code": "refresh_token_reused"}})));
        assert!(is_dead(Agent::Codex, 401, &json!({})));
        assert!(!is_dead(Agent::Codex, 503, &json!({})));
    }
}
