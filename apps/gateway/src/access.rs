//! Which devices may connect: the servers and the clients of the gateway's account, as the auth server
//! lists them. Servers send their agents' traffic, clients manage the gateway. The list is kept on disk
//! so the gateway still serves its account while the auth server is unreachable.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use motile_protocol::auth_api::DeviceKind;
use motile_protocol::auth_client::AuthClient;
use motile_protocol::identity::DeviceKey;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock, watch};

const REFRESH_EVERY: Duration = Duration::from_secs(60);
const MIN_REFRESH_GAP: Duration = Duration::from_secs(5);

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Server,
    Client,
}

pub struct Access {
    fixed: HashMap<String, Role>,
    linked: RwLock<HashMap<String, Role>>,
    /// The account's own relay, while it has one.
    relay: watch::Sender<Option<String>>,
    account: Option<AccountAccess>,
}

struct AccountAccess {
    auth: AuthClient,
    key: DeviceKey,
    cache: PathBuf,
    last_refresh: Mutex<Option<Instant>>,
}

pub struct AccountSource {
    pub auth_url: String,
    pub key: DeviceKey,
    /// Where the account's list is kept between runs.
    pub cache: PathBuf,
}

impl Access {
    /// Only the devices given, for tests.
    pub fn fixed(servers: &[String], clients: &[String]) -> Arc<Self> {
        let servers = servers.iter().map(|key| (key.clone(), Role::Server));
        let clients = clients.iter().map(|key| (key.clone(), Role::Client));
        Arc::new(Self {
            fixed: servers.chain(clients).collect(),
            linked: RwLock::default(),
            relay: watch::Sender::new(None),
            account: None,
        })
    }

    pub fn account(source: AccountSource) -> Arc<Self> {
        let text = std::fs::read_to_string(&source.cache).unwrap_or_default();
        let cached = serde_json::from_str(&text).unwrap_or_default();
        let account = AccountAccess {
            auth: AuthClient::new(&source.auth_url),
            key: source.key,
            cache: source.cache,
            last_refresh: Mutex::default(),
        };
        Arc::new(Self {
            fixed: HashMap::new(),
            linked: RwLock::new(cached),
            relay: watch::Sender::new(None),
            account: Some(account),
        })
    }

    /// What the device may do, asking the auth server again if it isn't listed yet.
    pub async fn role(&self, public_key: &str) -> Option<Role> {
        if let Some(role) = self.listed(public_key).await {
            return Some(role);
        }
        if self.refreshed_recently().await {
            return None;
        }
        self.refresh().await;
        self.listed(public_key).await
    }

    pub async fn listed(&self, public_key: &str) -> Option<Role> {
        if let Some(role) = self.fixed.get(public_key) {
            return Some(*role);
        }
        self.linked.read().await.get(public_key).copied()
    }

    async fn refreshed_recently(&self) -> bool {
        let Some(account) = &self.account else { return true };
        account.last_refresh.lock().await.is_some_and(|at| at.elapsed() < MIN_REFRESH_GAP)
    }

    pub async fn refresh(&self) {
        let Some(account) = &self.account else { return };
        *account.last_refresh.lock().await = Some(Instant::now());
        let me = match account.auth.me(&account.key).await {
            Ok(me) => me,
            Err(error) => {
                tracing::warn!("couldn't refresh the account's devices: {error:#}");
                return;
            }
        };
        if me.user.is_none() {
            tracing::warn!("this gateway is no longer linked to an account; no device may connect");
        }
        self.relay.send_if_modified(|kept| {
            let changed = *kept != me.relay;
            kept.clone_from(&me.relay);
            changed
        });
        let roles: HashMap<String, Role> = me
            .devices
            .into_iter()
            .filter_map(|device| match device.kind {
                DeviceKind::Server => Some((device.public_key, Role::Server)),
                DeviceKind::Client => Some((device.public_key, Role::Client)),
                DeviceKind::Gateway => None,
            })
            .collect();
        if let Ok(text) = serde_json::to_string(&roles) {
            let _ = std::fs::write(&account.cache, text);
        }
        *self.linked.write().await = roles;
    }

    /// Follows the account's own relay.
    pub fn watch_relay(&self) -> watch::Receiver<Option<String>> {
        self.relay.subscribe()
    }

    pub fn keep_fresh(self: &Arc<Self>) {
        if self.account.is_none() {
            return;
        }
        let access = self.clone();
        tokio::spawn(async move {
            loop {
                access.refresh().await;
                tokio::time::sleep(REFRESH_EVERY).await;
            }
        });
    }
}
