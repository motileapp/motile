//! `motile-gateway`: serves the account's servers and clients once it is linked to the account, by
//! `MOTILE_ENROLL_CODE` on its first start or by `motile-gateway setup <code>` at any time, which links
//! it again over any link it has.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use motile_gateway::access::{Access, AccountSource};
use motile_gateway::config::{self, Upstreams};
use motile_gateway::serve::BindOptions;
use motile_gateway::{Options, start};
use motile_protocol::auth_api::DeviceKind;
use motile_protocol::auth_client::{AuthClient, DeviceDescription};
use motile_protocol::identity::{DeviceKey, is_public_key};
use motile_protocol::{DEFAULT_AUTH_URL, tls};
use serde::{Deserialize, Serialize};
use tracing_subscriber::EnvFilter;

const DEVICE_NAME: &str = "Gateway";
const LINKED_FILE: &str = "account.json";
/// How often a gateway that isn't linked looks whether `setup` linked it.
const LINK_CHECK_EVERY: Duration = Duration::from_secs(2);

#[derive(Serialize, Deserialize)]
struct Linked {
    auth_url: String,
    email: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tls::install();
    let data_dir = PathBuf::from(variable("DATA_DIR").unwrap_or_else(|| "data".into()));
    let auth_url = variable("MOTILE_AUTH_URL").unwrap_or_else(|| DEFAULT_AUTH_URL.into());
    std::fs::create_dir_all(&data_dir).context("The data folder couldn't be made.")?;
    let key = DeviceKey::load_or_create(&data_dir.join("device.key")).context("The device key couldn't be read.")?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [command, code] = args.as_slice()
        && command == "setup"
    {
        let email = enroll(&data_dir, &auth_url, &key, code).await?;
        println!("Linked to {email}. Your gateway joins your account now.");
        return Ok(());
    }
    if !args.is_empty() {
        anyhow::bail!(
            "Run motile-gateway with no arguments, or `motile-gateway setup <code>` to link it to your account."
        );
    }

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let servers = keys("MOTILE_ALLOW_SERVERS")?;
    let clients = keys("MOTILE_ALLOW_CLIENTS")?;
    let mut upstreams = Upstreams::from_env();
    let access = match servers.is_empty() && clients.is_empty() {
        true => {
            let auth_url = linked_auth_url(&data_dir, &auth_url, &key).await;
            upstreams.mcp_redirect = config::mcp_redirect(&auth_url);
            let key = DeviceKey::from_bytes(key.to_bytes());
            Access::account(AccountSource { auth_url, key, cache: data_dir.join("devices.json") })
        }
        false => Access::fixed(&servers, &clients),
    };

    let options = Options {
        data_dir,
        key: DeviceKey::from_bytes(key.to_bytes()),
        access,
        upstreams,
        bind: BindOptions {
            local_only: variable("MOTILE_LOCAL").is_some(),
            port: variable("MOTILE_GATEWAY_PORT").and_then(|port| port.parse().ok()),
            external_addr: parsed("MOTILE_GATEWAY_EXTERNAL_ADDR")?,
        },
        prices: None,
    };
    let (_gateway, endpoint) = start(options).await?;
    tracing::info!(key = key.public(), "the gateway is serving its account");
    shutdown().await;
    endpoint.close().await;
    Ok(())
}

/// The auth server of the account the gateway is linked to. Without a link, `MOTILE_ENROLL_CODE`
/// makes one; without that either, it waits until `motile-gateway setup <code>` does.
async fn linked_auth_url(data_dir: &Path, auth_url: &str, key: &DeviceKey) -> String {
    if let Some(linked) = linked(data_dir) {
        tracing::info!("linked to {}", linked.email);
        return linked.auth_url;
    }
    if let Some(code) = variable("MOTILE_ENROLL_CODE") {
        match enroll(data_dir, auth_url, key, &code).await {
            Ok(email) => tracing::info!("linked to {email}"),
            Err(error) => tracing::error!("MOTILE_ENROLL_CODE didn't link this gateway: {error:#}"),
        }
    }
    let mut asked = false;
    loop {
        if let Some(linked) = linked(data_dir) {
            tracing::info!("linked to {}", linked.email);
            return linked.auth_url;
        }
        if !asked {
            tracing::warn!(
                "Not linked to an account yet. Add a gateway in a client's Settings → Gateway and run the command it shows \
                 in a shell here: motile-gateway setup <code>"
            );
            asked = true;
        }
        tokio::time::sleep(LINK_CHECK_EVERY).await;
    }
}

fn linked(data_dir: &Path) -> Option<Linked> {
    let text = std::fs::read_to_string(data_dir.join(LINKED_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Links the gateway to the account the code belongs to, over any link it has, and says to whom.
async fn enroll(data_dir: &Path, auth_url: &str, key: &DeviceKey, code: &str) -> anyhow::Result<String> {
    let auth = AuthClient::new(auth_url);
    let device = DeviceDescription { name: DEVICE_NAME, platform: std::env::consts::OS };
    let enrolled = auth
        .enroll_as(key, code.trim(), &device, DeviceKind::Gateway)
        .await
        .context("That code didn't link this gateway. Make a new one in Settings → Gateway.")?;
    let linked = Linked { auth_url: auth.base_url().to_string(), email: enrolled.email.clone() };
    std::fs::write(data_dir.join(LINKED_FILE), serde_json::to_string(&linked)?)
        .context("The link couldn't be kept in the data folder.")?;
    Ok(enrolled.email)
}

async fn shutdown() {
    #[cfg(unix)]
    {
        let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("signals");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

fn variable(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn parsed<T: std::str::FromStr>(name: &str) -> anyhow::Result<Option<T>> {
    let Some(value) = variable(name) else { return Ok(None) };
    let parsed = value.trim().parse().map_err(|_| anyhow::anyhow!("{name} isn't valid: {value}"))?;
    Ok(Some(parsed))
}

/// Device keys allowed without the auth server, for tests and local runs.
fn keys(name: &str) -> anyhow::Result<Vec<String>> {
    let keys: Vec<String> = variable(name)
        .unwrap_or_default()
        .split(',')
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .collect();
    if let Some(key) = keys.iter().find(|key| !is_public_key(key)) {
        anyhow::bail!("{key} in {name} isn't a device key.");
    }
    Ok(keys)
}
