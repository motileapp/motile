//! `motile-proxy`: links itself to the account with `MOTILE_ENROLL_CODE` on its first start, then
//! serves the account's servers and clients.

use std::path::{Path, PathBuf};

use anyhow::Context;
use motile_protocol::auth_api::DeviceKind;
use motile_protocol::auth_client::{AuthClient, DeviceDescription};
use motile_protocol::identity::{DeviceKey, is_public_key};
use motile_protocol::{DEFAULT_AUTH_URL, tls};
use motile_proxy::access::{Access, AccountSource};
use motile_proxy::config::Upstreams;
use motile_proxy::serve::BindOptions;
use motile_proxy::{Options, start};
use serde::{Deserialize, Serialize};
use tracing_subscriber::EnvFilter;

const DEVICE_NAME: &str = "Proxy";

#[derive(Serialize, Deserialize)]
struct Linked {
    auth_url: String,
    email: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    tls::install();

    let data_dir = PathBuf::from(variable("DATA_DIR").unwrap_or_else(|| "data".into()));
    let auth_url = variable("MOTILE_AUTH_URL").unwrap_or_else(|| DEFAULT_AUTH_URL.into());
    std::fs::create_dir_all(&data_dir).context("The data folder couldn't be made.")?;
    let key = DeviceKey::load_or_create(&data_dir.join("device.key")).context("The device key couldn't be read.")?;

    let servers = keys("MOTILE_ALLOW_SERVERS")?;
    let clients = keys("MOTILE_ALLOW_CLIENTS")?;
    let access = match servers.is_empty() && clients.is_empty() {
        true => {
            link(&data_dir, &auth_url, &key).await;
            let key = DeviceKey::from_bytes(key.to_bytes());
            Access::account(AccountSource { auth_url, key, cache: data_dir.join("devices.json") })
        }
        false => Access::fixed(&servers, &clients),
    };

    let options = Options {
        data_dir,
        key: DeviceKey::from_bytes(key.to_bytes()),
        access,
        upstreams: Upstreams::from_env(),
        bind: BindOptions {
            local_only: variable("MOTILE_LOCAL").is_some(),
            port: variable("MOTILE_PROXY_PORT").and_then(|port| port.parse().ok()),
        },
        prices: None,
    };
    let (_proxy, endpoint) = start(options).await?;
    tracing::info!(key = key.public(), "the proxy is serving its account");
    shutdown().await;
    endpoint.close().await;
    Ok(())
}

/// Links the proxy to the account the enroll code belongs to, once.
async fn link(data_dir: &Path, auth_url: &str, key: &DeviceKey) {
    let file = data_dir.join("account.json");
    if let Some(linked) =
        std::fs::read_to_string(&file).ok().and_then(|text| serde_json::from_str::<Linked>(&text).ok())
    {
        tracing::info!("linked to {}", linked.email);
        return;
    }
    let Some(code) = variable("MOTILE_ENROLL_CODE") else {
        tracing::error!(
            "This proxy isn't linked to an account yet. Add a proxy in a client's settings and start this one with \
             MOTILE_ENROLL_CODE set to the code it shows."
        );
        return;
    };
    let auth = AuthClient::new(auth_url);
    let device = DeviceDescription { name: DEVICE_NAME, platform: std::env::consts::OS };
    match auth.enroll_as(key, code.trim(), &device, DeviceKind::Proxy).await {
        Ok(enrolled) => {
            let linked = Linked { auth_url: auth.base_url().to_string(), email: enrolled.email.clone() };
            if let Err(error) = std::fs::write(&file, serde_json::to_string(&linked).unwrap_or_default()) {
                tracing::warn!("couldn't keep the account: {error}");
            }
            tracing::info!("linked to {}", enrolled.email);
        }
        Err(error) => tracing::error!(
            "The enroll code didn't link this proxy: {error:#}. Make a new code in a client and set MOTILE_ENROLL_CODE to it."
        ),
    }
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
