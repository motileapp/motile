//! Motile Proxy: the account's Claude and Codex accounts, served to its servers over iroh. Agents'
//! requests pass through to Anthropic and OpenAI with an account's token, each thread kept on one
//! account while it can take it.

pub mod access;
pub mod accounts;
pub mod config;
pub mod limits;
pub mod manage;
pub mod oauth;
pub mod pricing;
pub mod proxy;
pub mod router;
pub mod seal;
pub mod serve;
pub mod store;
pub mod upstream;
pub mod usage;
pub mod websocket;

use std::path::PathBuf;
use std::sync::Arc;

use iroh::Endpoint;
use motile_protocol::identity::DeviceKey;

use crate::access::Access;
use crate::config::Upstreams;
use crate::pricing::Prices;
use crate::proxy::Proxy;
use crate::serve::BindOptions;

pub struct Options {
    pub data_dir: PathBuf,
    pub key: DeviceKey,
    pub access: Arc<Access>,
    pub upstreams: Upstreams,
    pub bind: BindOptions,
    /// Prices to use rather than fetching LiteLLM's list.
    pub prices: Option<Prices>,
}

/// Opens the proxy's data and starts serving its account on iroh.
pub async fn start(options: Options) -> anyhow::Result<(Arc<Proxy>, Endpoint)> {
    motile_protocol::tls::install();
    let proxy = Proxy::open(options.data_dir, &options.key.to_bytes(), options.upstreams)?;
    let fetch_prices = options.prices.is_none();
    if let Some(prices) = options.prices {
        proxy.set_prices(prices);
    }
    proxy.start_background(fetch_prices);
    let endpoint = serve::bind(&options.key, &options.bind).await?;
    options.access.keep_fresh();
    tokio::spawn(serve::run(proxy.clone(), options.access, endpoint.clone()));
    Ok((proxy, endpoint))
}
