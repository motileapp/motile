//! Which devices may connect: the clients linked to the server's account, as listed by the auth
//! server, and the account's proxy, which the server's agents may reach. The list is kept on disk so
//! the server still accepts its clients while the auth server is unreachable.

use std::collections::HashSet;
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
    /// The key of the account's proxy, while it has one.
    proxy: watch::Sender<Option<String>>,
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
            proxy: watch::Sender::new(cached.proxy),
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

    pub async fn is_deleted(&self, public_key: &str) -> bool {
        self.deleted.read().await.contains(public_key)
    }

    pub fn proxy(&self) -> Option<String> {
        self.proxy.borrow().clone()
    }

    /// Follows the key of the account's proxy.
    pub fn watch_proxy(&self) -> watch::Receiver<Option<String>> {
        self.proxy.subscribe()
    }

    pub fn set_proxy(&self, key: Option<String>) {
        self.proxy.send_if_modified(|kept| {
            let changed = *kept != key;
            *kept = key;
            changed
        });
    }

    async fn refreshed_recently(&self) -> bool {
        let Some(account) = &self.account else { return true };
        account.last_refresh.lock().await.is_some_and(|at| at.elapsed() < MIN_REFRESH_GAP)
    }

    /// Asks the auth server which clients and which proxy belong to the account.
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
        let keys_of = |kind: DeviceKind| {
            let devices = me.devices.iter().filter(move |device| device.kind == kind);
            devices.map(|device| device.public_key.clone())
        };
        let listed =
            Listed { clients: keys_of(DeviceKind::Client).collect(), proxy: keys_of(DeviceKind::Proxy).next() };
        if let Ok(text) = serde_json::to_string(&listed) {
            let _ = std::fs::write(&account.cache, text);
        }
        *self.linked.write().await = listed.clients;
        self.set_proxy(listed.proxy);
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

/// The account's devices as the auth server last listed them.
#[derive(Serialize, Deserialize, Default, PartialEq, Debug)]
struct Listed {
    clients: HashSet<String>,
    proxy: Option<String>,
}

fn read_listed(file: &std::path::Path) -> Listed {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    if let Ok(listed) = serde_json::from_str(&text) {
        return listed;
    }
    // Up to 0.1.267 the file only listed the clients.
    Listed { clients: serde_json::from_str(&text).unwrap_or_default(), proxy: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kept_list_is_read_with_its_proxy_and_from_before_it_had_one() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("account-keys.json");
        std::fs::write(&file, r#"["client-a"]"#).unwrap();
        assert_eq!(read_listed(&file), Listed { clients: HashSet::from(["client-a".into()]), proxy: None });
        std::fs::write(&file, r#"{"clients":["client-a"],"proxy":"proxy-key"}"#).unwrap();
        assert_eq!(read_listed(&file).proxy.as_deref(), Some("proxy-key"));
        assert_eq!(read_listed(&folder.path().join("missing.json")), Listed::default());
    }
}
