//! The limit resets of the server's own accounts. Codex's app-server lists and uses its account's.
//! Claude Code does neither, so Claude is asked directly with the token Claude Code keeps: in
//! `.credentials.json` in its folder, or on a Mac in the Keychain. A Gateway account's resets are
//! the gateway's.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use motile_protocol::resets;
use motile_protocol::wire::{Agent, AgentAccount, LimitReset};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::agents::ask;
use crate::agents::environment::Environment;
use crate::limits::CLAUDE_ARGUMENTS;

const CLAUDE_URL: &str = "https://api.anthropic.com";
const CLAUDE_OAUTH_BETA: &str = "oauth-2025-04-20";
const TIMEOUT: Duration = Duration::from_secs(25);
/// A token closer than this to expiring is renewed by Claude Code first.
const FRESH_FOR: f64 = 60.0;

/// The resets of the Claude login in `environment`, none when its token can't be read.
pub async fn claude(environment: &Environment) -> Vec<LimitReset> {
    let Some(token) = claude_token(environment).await.filter(Token::fresh) else { return Vec::new() };
    let path = format!("/api/oauth/usage?{}", resets::CLAUDE_USAGE_QUERY);
    match claude_request(&token, reqwest::Method::GET, &path, None).await {
        Ok(usage) => resets::claude(&usage["cedar_ember"], motile_protocol::now()),
        Err(error) => {
            tracing::debug!("couldn't read Claude's resets: {error:#}");
            Vec::new()
        }
    }
}

/// Spends one of the resets of the account's login, which isn't a Gateway account.
pub async fn use_own(account: &AgentAccount, environment: &Environment, reset: &str) -> anyhow::Result<()> {
    match account.agent {
        Agent::Claude => use_claude(environment, reset).await,
        Agent::Codex => use_codex(environment, reset).await,
    }
}

async fn use_claude(environment: &Environment, reset: &str) -> anyhow::Result<()> {
    let token = match claude_token(environment).await.filter(Token::fresh) {
        Some(token) => token,
        None => {
            // Asking Claude Code what was used has it renew its token.
            ask::claude(environment, &CLAUDE_ARGUMENTS, &["get_usage"]).await?;
            let token = claude_token(environment).await.filter(Token::fresh);
            token.context("Claude Code's sign-in couldn't be read. Sign in to Claude Code again.")?
        }
    };
    let profile = claude_request(&token, reqwest::Method::GET, "/api/oauth/profile", None).await?;
    let organization = profile["organization"]["uuid"].as_str().context("Claude didn't say which organisation.")?;
    let path = format!("/api/organizations/{organization}/reset_rate_limits");
    let claim = resets::claude_claim(reset, &uuid::Uuid::new_v4().to_string());
    let answer = claude_request(&token, reqwest::Method::POST, &path, Some(claim)).await?;
    resets::claude_claimed(&answer).map_err(anyhow::Error::msg)
}

async fn use_codex(environment: &Environment, reset: &str) -> anyhow::Result<()> {
    const CONSUME: &str = "account/rateLimitResetCredit/consume";
    let credit = (!reset.is_empty()).then_some(reset);
    let params = json!({"idempotencyKey": uuid::Uuid::new_v4().to_string(), "creditId": credit});
    let mut answers = ask::codex(environment, &[(CONSUME, params)]).await?;
    let answer = answers.remove(CONSUME).context("Codex didn't answer.")?.map_err(anyhow::Error::msg)?;
    resets::codex_consumed(answer["outcome"].as_str().unwrap_or_default()).map_err(anyhow::Error::msg)
}

struct Token {
    access: String,
    expires_at: Option<f64>,
}

impl Token {
    fn fresh(&self) -> bool {
        self.expires_at.is_none_or(|at| at - motile_protocol::now() > FRESH_FOR)
    }
}

/// The token of the Claude Code login in `environment`, where Claude Code reads it: the Keychain
/// first on a Mac, then `.credentials.json` in its folder. A stand-in for Claude never gets the
/// Keychain's.
async fn claude_token(environment: &Environment) -> Option<Token> {
    let folder = environment.variables.get("CLAUDE_CONFIG_DIR").filter(|folder| !folder.trim().is_empty());
    if cfg!(target_os = "macos")
        && stand_in().is_none()
        && let Some(token) = keychain_token(environment, folder.map(String::as_str)).await
    {
        return Some(token);
    }
    let home = Path::new(environment.variables.get("HOME").map(String::as_str).unwrap_or_default());
    let folder = folder.map(PathBuf::from).unwrap_or_else(|| home.join(".claude"));
    let kept = tokio::fs::read(folder.join(".credentials.json")).await.ok()?;
    token_of(&serde_json::from_slice(&kept).ok()?)
}

async fn keychain_token(environment: &Environment, folder: Option<&str>) -> Option<Token> {
    let user = environment.variables.get("USER").map(String::as_str).unwrap_or_default();
    let user = match !user.is_empty() && user.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
        true => user,
        false => "claude-code-user",
    };
    let found = tokio::process::Command::new("security")
        .args(["find-generic-password", "-a", user, "-w", "-s", &keychain_service(folder)])
        .output();
    let found = tokio::time::timeout(Duration::from_secs(5), found).await.ok()?.ok()?;
    let text = String::from_utf8(found.stdout).ok().filter(|_| found.status.success())?;
    let text = text.trim();
    let kept = serde_json::from_str(text).ok().or_else(|| serde_json::from_slice(&hex::decode(text).ok()?).ok())?;
    token_of(&kept)
}

/// Claude Code's Keychain item: one of its own for a folder other than the usual one.
fn keychain_service(folder: Option<&str>) -> String {
    match folder {
        None => "Claude Code-credentials".to_string(),
        Some(folder) => {
            let hash = hex::encode(Sha256::digest(folder.nfc().collect::<String>().as_bytes()));
            format!("Claude Code-credentials-{}", &hash[..8])
        }
    }
}

fn token_of(kept: &Value) -> Option<Token> {
    let oauth = &kept["claudeAiOauth"];
    let access = oauth["accessToken"].as_str().map(str::trim).filter(|token| !token.is_empty())?;
    Some(Token { access: access.to_string(), expires_at: oauth["expiresAt"].as_f64().map(|at| at / 1000.0) })
}

async fn claude_request(
    token: &Token,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> anyhow::Result<Value> {
    motile_protocol::tls::install();
    let url = format!("{}{path}", stand_in().as_deref().unwrap_or(CLAUDE_URL));
    let request = reqwest::Client::builder().timeout(TIMEOUT).build()?.request(method, url);
    let request = request.bearer_auth(&token.access).header("anthropic-beta", CLAUDE_OAUTH_BETA);
    let request = match body {
        Some(body) => request.json(&body),
        None => request,
    };
    let answer = request.send().await.context("Claude couldn't be reached.")?;
    match answer.status() {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            anyhow::bail!("Claude refused Claude Code's sign-in. Sign in to Claude Code again.")
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => {
            anyhow::bail!("Claude is turning requests away for now. Try again in a moment.")
        }
        _ => Ok(answer.error_for_status()?.json().await?),
    }
}

/// `MOTILE_ANTHROPIC_URL`, a stand-in for Claude.
fn stand_in() -> Option<String> {
    std::env::var("MOTILE_ANTHROPIC_URL").ok().filter(|url| !url.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_of_its_own_has_a_keychain_item_of_its_own() {
        assert_eq!(keychain_service(None), "Claude Code-credentials");
        let hash = hex::encode(Sha256::digest(b"/Users/ada/.claude-work"));
        assert_eq!(
            keychain_service(Some("/Users/ada/.claude-work")),
            format!("Claude Code-credentials-{}", &hash[..8])
        );
    }

    #[test]
    fn the_token_and_when_it_expires_are_read_from_claude_codes_login() {
        let kept = json!({"claudeAiOauth": {"accessToken": "sk-ant-oat", "refreshToken": "r", "expiresAt": 1_800_000_000_000_u64}});
        let token = token_of(&kept).unwrap();
        assert_eq!((token.access.as_str(), token.expires_at), ("sk-ant-oat", Some(1_800_000_000.0)));
        assert!(token_of(&json!({"claudeAiOauth": {"accessToken": " "}})).is_none());
    }
}
