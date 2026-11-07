//! Which devices may connect: the clients linked to the server's account, as listed by the auth
//! server, and the account's gateway, which the server's agents may reach. The list is kept on disk so
//! the server still accepts its clients while the auth server is unreachable.

use std::collections::{HashMap, HashSet};
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

pub struct Access {
    /// Keys given on the command line.
    fixed: HashSet<String>,
    linked: RwLock<HashSet<String>>,
    /// The servers the account deleted completely, whose copies this server drops.
    deleted: RwLock<HashSet<String>>,
    /// The account's servers, this one too, and the ones it unlinked, by key.
    servers: RwLock<HashMap<String, Named>>,
    /// The key of the account's gateway, while it has one.
    gateway: watch::Sender<Option<String>>,
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
    pub fn new(fixed: Vec<String>, account: Option<AccountSource>) -> Arc<Self> {
        let cached = account.as_ref().map(|account| read_listed(&account.cache)).unwrap_or_default();
        let account = account.map(|account| AccountAccess {
            auth: AuthClient::new(&account.auth_url),
            key: account.key,
            cache: account.cache,
            last_refresh: Mutex::default(),
        });
        Arc::new(Self {
            fixed: fixed.into_iter().collect(),
            linked: RwLock::new(cached.clients),
            deleted: RwLock::default(),
            servers: RwLock::default(),
            gateway: watch::Sender::new(cached.gateway),
            account,
        })
    }

    pub async fn allows(&self, public_key: &str) -> bool {
        if self.is_listed(public_key).await {
            return true;
        }
        // It may have been linked a moment ago. Unknown devices can't make the server ask the auth
        // server more than once every few seconds.
        if self.refreshed_recently().await {
            return false;
        }
        self.refresh().await;
        self.is_listed(public_key).await
    }

    /// Whether the device is allowed as far as the server knows right now.
    pub async fn is_listed(&self, public_key: &str) -> bool {
        self.fixed.contains(public_key) || self.linked.read().await.contains(public_key)
    }

    pub async fn deleted(&self) -> Vec<String> {
        self.deleted.read().await.iter().cloned().collect()
    }

    /// The account's servers and the ones it unlinked, as the auth server last listed them; empty
    /// until it answered.
    pub async fn servers(&self) -> HashMap<String, Named> {
        self.servers.read().await.clone()
    }

    pub async fn is_deleted(&self, public_key: &str) -> bool {
        self.deleted.read().await.contains(public_key)
    }

    pub fn gateway(&self) -> Option<String> {
        self.gateway.borrow().clone()
    }

    /// Follows the key of the account's gateway.
    pub fn watch_gateway(&self) -> watch::Receiver<Option<String>> {
        self.gateway.subscribe()
    }

    pub fn set_gateway(&self, key: Option<String>) {
        self.gateway.send_if_modified(|kept| {
            let changed = *kept != key;
            *kept = key;
            changed
        });
    }

    async fn refreshed_recently(&self) -> bool {
        let Some(account) = &self.account else { return true };
        account.last_refresh.lock().await.is_some_and(|at| at.elapsed() < MIN_REFRESH_GAP)
    }

    /// Asks the auth server which clients and which gateway belong to the account.
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
            tracing::warn!("this server is no longer linked to an account; no client may connect");
        }
        *self.deleted.write().await = me.deleted.into_iter().collect();
        let servers = me.devices.iter().filter(|device| device.kind == DeviceKind::Server);
        let mut named: HashMap<String, Named> = servers
            .map(|device| {
                (device.public_key.clone(), Named { name: device.name.clone(), platform: device.platform.clone() })
            })
            .collect();
        for removed in &me.removed {
            named.insert(
                removed.public_key.clone(),
                Named { name: removed.name.clone(), platform: removed.platform.clone() },
            );
        }
        *self.servers.write().await = named;
        let keys_of = |kind: DeviceKind| {
            let devices = me.devices.iter().filter(move |device| device.kind == kind);
            devices.map(|device| device.public_key.clone())
        };
        let listed =
            Listed { clients: keys_of(DeviceKind::Client).collect(), gateway: keys_of(DeviceKind::Gateway).next() };
        if let Ok(text) = serde_json::to_string(&listed) {
            let _ = std::fs::write(&account.cache, text);
        }
        *self.linked.write().await = listed.clients;
        self.set_gateway(listed.gateway);
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

/// A server as the account names it.
#[derive(Clone, Debug, PartialEq)]
pub struct Named {
    pub name: String,
    pub platform: String,
}

/// The account's devices as the auth server last listed them.
#[derive(Serialize, Deserialize, Default, PartialEq, Debug)]
struct Listed {
    clients: HashSet<String>,
    gateway: Option<String>,
}

fn read_listed(file: &std::path::Path) -> Listed {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    if let Ok(listed) = serde_json::from_str(&text) {
        return listed;
    }
    // Up to 0.1.267 the file only listed the clients.
    Listed { clients: serde_json::from_str(&text).unwrap_or_default(), gateway: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kept_list_is_read_with_its_gateway_and_from_before_it_had_one() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("account-keys.json");
        std::fs::write(&file, r#"["client-a"]"#).unwrap();
        assert_eq!(read_listed(&file), Listed { clients: HashSet::from(["client-a".into()]), gateway: None });
        std::fs::write(&file, r#"{"clients":["client-a"],"gateway":"gateway-key"}"#).unwrap();
        assert_eq!(read_listed(&file).gateway.as_deref(), Some("gateway-key"));
        assert_eq!(read_listed(&folder.path().join("missing.json")), Listed::default());
    }
}
