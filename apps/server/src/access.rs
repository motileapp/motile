//! Which devices may connect: the apps linked to the host's account, as listed by the auth
//! server. The list is kept on disk so the host still accepts its apps while the auth server is
//! unreachable.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use motile_protocol::auth_api::DeviceKind;
use motile_protocol::auth_client::AuthClient;
use motile_protocol::identity::DeviceKey;
use tokio::sync::{Mutex, RwLock};

const REFRESH_EVERY: Duration = Duration::from_secs(60);
const MIN_REFRESH_GAP: Duration = Duration::from_secs(5);

pub struct Access {
    /// Keys given on the command line.
    fixed: HashSet<String>,
    linked: RwLock<HashSet<String>>,
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
        let cached = account.as_ref().map(|account| read_keys(&account.cache)).unwrap_or_default();
        let account = account.map(|account| AccountAccess {
            auth: AuthClient::new(&account.auth_url),
            key: account.key,
            cache: account.cache,
            last_refresh: Mutex::default(),
        });
        Arc::new(Self { fixed: fixed.into_iter().collect(), linked: RwLock::new(cached), account })
    }

    pub async fn allows(&self, public_key: &str) -> bool {
        if self.is_listed(public_key).await {
            return true;
        }
        // It may have been linked a moment ago. Unknown devices can't make the host ask the auth
        // server more than once every few seconds.
        if self.refreshed_recently().await {
            return false;
        }
        self.refresh().await;
        self.is_listed(public_key).await
    }

    /// Whether the device is allowed as far as the host knows right now.
    pub async fn is_listed(&self, public_key: &str) -> bool {
        self.fixed.contains(public_key) || self.linked.read().await.contains(public_key)
    }

    async fn refreshed_recently(&self) -> bool {
        let Some(account) = &self.account else { return true };
        account.last_refresh.lock().await.is_some_and(|at| at.elapsed() < MIN_REFRESH_GAP)
    }

    /// Asks the auth server which apps belong to the account.
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
            tracing::warn!("this host is no longer linked to an account; no app may connect");
        }
        let apps = me.devices.into_iter().filter(|device| device.kind == DeviceKind::Client);
        let keys: HashSet<String> = apps.map(|device| device.public_key).collect();
        if let Ok(text) = serde_json::to_string(&keys) {
            let _ = std::fs::write(&account.cache, text);
        }
        *self.linked.write().await = keys;
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

fn read_keys(file: &std::path::Path) -> HashSet<String> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    serde_json::from_str(&text).unwrap_or_default()
}
