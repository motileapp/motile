//! The proxy's state: its accounts, where its threads are, and what it keeps on the
//! volume. Everything that reads or changes an account goes through here.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::Duration;

use anyhow::Context;
use motile_protocol::now;
use motile_protocol::proxy_wire::{AccountChange, AccountStatus, Capture, ProxyAccount, ProxyState};
use motile_protocol::wire::{Agent, AgentLimits};

use crate::accounts::{Account, Tokens};
use crate::config::Upstreams;
use crate::limits::{self, Reading};
use crate::oauth::{self, RenewError, SignIns, Signed};
use crate::pricing::{self, Prices};
use crate::router::{self, Evidence, Facts, Moved, Placing, Router, ThreadKey};
use crate::seal::Seal;
use crate::store::{Captured, Record, Store};

/// An account's tokens are renewed when they expire within this.
const RENEW_BEFORE: f64 = 10.0 * 60.0;
const RENEW_CHECK: Duration = Duration::from_secs(60);
/// An account nothing was read from for this long is asked for its limits.
const POLL_AFTER: f64 = 60.0;
const POLL_CHECK: Duration = Duration::from_secs(30);
const PRICES_EVERY: Duration = Duration::from_secs(86400);
const TIDY_EVERY: Duration = Duration::from_secs(600);
/// A reading of an account whose windows didn't change is kept at most this often.
const READING_EVERY: f64 = 300.0;
const CAPTURE_LASTS: f64 = 24.0 * 3600.0;
/// Codex's prompt cache outlives a pause this long, by OpenAI's account of it.
pub const CODEX_CACHE: f64 = 30.0 * 60.0;
/// Claude's, unless a request asks for an hour.
pub const CLAUDE_CACHE: f64 = 5.0 * 60.0;
pub const CLAUDE_LONG_CACHE: f64 = 3600.0;
const CAPTURE: &str = "capture";
const ACCOUNT_NAME: &str = "Proxy";

/// What a thread's request asks of `Proxy::choose`.
pub struct Asked<'a> {
    pub agent: Agent,
    pub model: Option<&'a str>,
    pub cache_lifetime: f64,
    /// The account the thread is pinned to.
    pub pin: Option<&'a str>,
}

pub struct Proxy {
    pub store: Store,
    seal: Seal,
    pub upstreams: Upstreams,
    pub http: reqwest::Client,
    accounts: RwLock<Vec<Account>>,
    /// One renewal at a time: a refresh token works once.
    renewing: tokio::sync::Mutex<()>,
    router: Mutex<Router>,
    pub sign_ins: SignIns,
    prices: RwLock<Prices>,
    capture: Mutex<(Capture, Option<f64>)>,
    data_dir: PathBuf,
}

impl Proxy {
    pub fn open(data_dir: PathBuf, device_key: &[u8; 32], upstreams: Upstreams) -> anyhow::Result<Arc<Self>> {
        std::fs::create_dir_all(&data_dir)?;
        let store = Store::open(&data_dir.join("proxy.sqlite")).context("The proxy's database couldn't be opened.")?;
        let seal = Seal::new(device_key);
        let accounts = store.accounts(&seal)?;
        let capture = store.setting(CAPTURE).unwrap_or((Capture::Off, None));
        let prices = pricing::read_kept(&data_dir.join("prices.json")).unwrap_or_default();
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(Duration::from_secs(600))
            .build()?;
        let proxy = Self {
            store,
            seal,
            upstreams,
            http,
            accounts: RwLock::new(accounts),
            renewing: tokio::sync::Mutex::new(()),
            router: Mutex::default(),
            sign_ins: SignIns::default(),
            prices: RwLock::new(prices),
            capture: Mutex::new(capture),
            data_dir,
        };
        proxy.pick_threads_up(now());
        Ok(Arc::new(proxy))
    }

    /// Puts the threads of the last days back where the log says they were.
    fn pick_threads_up(&self, now: f64) {
        let replayed = match self.store.replay(now - 7.0 * 86400.0) {
            Ok(replayed) => replayed,
            Err(error) => return tracing::warn!("couldn't read the log back: {error:#}"),
        };
        let mut router = self.router();
        for request in replayed {
            let key = ThreadKey { server: request.server, thread: request.thread };
            let lifetime = match request.agent {
                Agent::Claude => CLAUDE_LONG_CACHE,
                Agent::Codex => CODEX_CACHE,
            };
            router.touch(&key, request.agent, request.model.as_deref(), lifetime, request.at);
            if let Some(account) = &request.account {
                router.assign(&key, account, 0.0, request.at);
            }
            router.mark(&key, request.at);
            router.spent(&key, request.cost, request.finished_at);
            router.tidy(request.at);
        }
        router.tidy(now);
    }

    fn router(&self) -> MutexGuard<'_, Router> {
        self.router.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn set_prices(&self, prices: Prices) {
        *self.prices.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = prices;
    }

    pub fn cost(&self, spent: &crate::usage::Spent) -> f64 {
        self.prices.read().unwrap_or_else(|poisoned| poisoned.into_inner()).cost(spent)
    }

    pub fn accounts(&self) -> Vec<Account> {
        self.accounts.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    pub fn account(&self, id: &str) -> Option<Account> {
        self.accounts().into_iter().find(|account| account.id == id)
    }

    /// Changes an account and keeps the change.
    fn change(&self, id: &str, change: impl FnOnce(&mut Account)) -> Option<Account> {
        let mut accounts = self.accounts.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        let account = accounts.iter_mut().find(|account| account.id == id)?;
        change(account);
        if let Err(error) = self.store.save_account(account, &self.seal) {
            tracing::warn!("couldn't keep a change to an account: {error:#}");
        }
        Some(account.clone())
    }

    /// Adds an account that signed in, or gives one already there its new tokens.
    pub fn signed_in(&self, signed: Signed) -> Account {
        let now = now();
        let mut accounts = self.accounts.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        let index = accounts.iter().position(|account| account.id == signed.id && account.agent == signed.agent);
        let account = match index {
            Some(index) => {
                let account = &mut accounts[index];
                account.tokens = signed.tokens;
                account.email = signed.email.or(account.email.take());
                account.plan = signed.plan.or(account.plan.take());
                account.status = AccountStatus::Ready;
                account.clone()
            }
            None => {
                let name = signed.email.clone().unwrap_or_else(|| agent_title(signed.agent).to_string());
                let account = Account {
                    id: signed.id,
                    agent: signed.agent,
                    name,
                    email: signed.email,
                    plan: signed.plan,
                    priority: 0,
                    enabled: true,
                    status: AccountStatus::Ready,
                    tokens: signed.tokens,
                    windows: vec![],
                    read_at: None,
                    reset_credits: 0,
                    created_at: now,
                };
                accounts.push(account.clone());
                account
            }
        };
        if let Err(error) = self.store.save_account(&account, &self.seal) {
            tracing::warn!("couldn't keep an account: {error:#}");
        }
        account
    }

    pub fn update_account(&self, id: &str, change: AccountChange) -> anyhow::Result<()> {
        let changed = self.change(id, |account| match change {
            AccountChange::Name { name } => account.name = name.trim().to_string(),
            AccountChange::Priority { priority } => account.priority = priority,
            AccountChange::Enabled { enabled } => account.enabled = enabled,
        });
        changed.map(|_| ()).context("There is no such account.")
    }

    pub fn remove_account(&self, id: &str) -> anyhow::Result<()> {
        self.accounts.write().unwrap_or_else(|poisoned| poisoned.into_inner()).retain(|account| account.id != id);
        self.store.delete_account(id)
    }

    pub fn set_capture(&self, capture: Capture) -> anyhow::Result<()> {
        let until = (capture != Capture::Off).then(|| now() + CAPTURE_LASTS);
        self.store.set_setting(CAPTURE, &(capture.clone(), until))?;
        *self.capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = (capture, until);
        Ok(())
    }

    /// Whether a request of the thread is captured in full; counts it against `Next`.
    pub fn capturing(&self, server: &str, thread: &str) -> bool {
        let mut capture = self.capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if capture.1.is_some_and(|until| until < now()) {
            *capture = (Capture::Off, None);
        }
        let captured = match &mut capture.0 {
            Capture::Off => false,
            Capture::Thread { server: of, thread: which } => of == server && which == thread,
            Capture::Next { left } => {
                *left = left.saturating_sub(1);
                true
            }
        };
        if capture.0 == (Capture::Next { left: 0 }) {
            *capture = (Capture::Off, None);
        }
        if let Capture::Next { .. } | Capture::Off = capture.0 {
            let _ = self.store.set_setting(CAPTURE, &*capture);
        }
        captured
    }

    /// The account the thread's request goes to: the one it is pinned to, or its own while it can take
    /// it, else the first by the rank, leaving out those `tried` already.
    pub fn choose(&self, key: &ThreadKey, request: Asked, tried: &[String], now: f64) -> Option<Account> {
        let Asked { agent, model, cache_lifetime, pin } = request;
        let accounts = self.accounts();
        let open =
            |account: &&Account| account.agent == agent && account.available(now) && !tried.contains(&account.id);
        if let Some(pin) = pin {
            return self.pinned(
                accounts.iter().filter(open).find(|account| account.id == pin)?,
                key,
                model,
                cache_lifetime,
                now,
            );
        }
        let current = {
            let mut router = self.router();
            router.touch(key, agent, model, cache_lifetime, now);
            let thread = router.thread(key)?;
            let current = thread.account.clone();
            let stays = current.as_ref().filter(|_| !thread.cache_lapsed(now));
            if let Some(account) = stays.and_then(|id| accounts.iter().filter(open).find(|account| &account.id == id)) {
                router.mark(key, now);
                return Some(account.clone());
            }
            current
        };
        let order = self.rank(&accounts, agent, model, current.as_deref(), tried, now);
        let account = accounts.into_iter().find(|account| Some(&account.id) == order.first())?;
        let mut router = self.router();
        let expected = router.expected(agent, model);
        router.assign(key, &account.id, expected, now);
        router.mark(key, now);
        let moved = current.is_some_and(|current| current != account.id);
        if moved && let Some(thread) = router.thread_mut(key) {
            thread.moved = Moved::Pending;
        }
        Some(account)
    }

    /// Keeps a pinned thread on its account, which counts what it is expected to spend there.
    fn pinned(
        &self,
        account: &Account,
        key: &ThreadKey,
        model: Option<&str>,
        cache_lifetime: f64,
        now: f64,
    ) -> Option<Account> {
        let mut router = self.router();
        router.touch(key, account.agent, model, cache_lifetime, now);
        let current = router.thread(key).and_then(|thread| thread.account.clone());
        if current.as_deref() != Some(account.id.as_str()) {
            let expected = router.expected(account.agent, model);
            router.assign(key, &account.id, expected, now);
        }
        router.mark(key, now);
        Some(account.clone())
    }

    /// The available accounts of the agent, the one a thread should start on first.
    fn rank(
        &self,
        accounts: &[Account],
        agent: Agent,
        model: Option<&str>,
        current: Option<&str>,
        tried: &[String],
        now: f64,
    ) -> Vec<String> {
        let of_agent: Vec<&Account> = accounts.iter().filter(|account| account.agent == agent).collect();
        let facts: Vec<Facts> = of_agent.iter().map(|account| self.facts(account, now)).collect();
        let open: Vec<usize> = (0..of_agent.len())
            .filter(|index| of_agent[*index].available(now) && !tried.contains(&of_agent[*index].id))
            .collect();
        let router = self.router();
        let placing = Placing { agent, model, current, expected: router.expected(agent, model), now };
        let candidates = router::candidates(
            &router,
            &open.iter().map(|index| facts[*index].clone()).collect::<Vec<_>>(),
            &facts,
            &placing,
        );
        let horizon = router.horizon();
        motile_protocol::select::rank(&candidates, now, horizon)
            .into_iter()
            .map(|index| of_agent[open[index]].id.clone())
            .collect()
    }

    fn facts<'a>(&self, account: &'a Account, now: f64) -> Facts<'a> {
        let since = now - router::EVIDENCE;
        let readings = self.store.readings(&account.id, since).unwrap_or_default();
        let costs = self.store.costs(&account.id, since).unwrap_or_default();
        let evidence: HashMap<String, Evidence> = account
            .windows
            .iter()
            .map(|window| (window.label.clone(), Evidence::read(&readings, &costs, &window.label)))
            .collect();
        Facts {
            id: &account.id,
            plan: account.plan.as_deref(),
            priority: account.priority,
            windows: &account.windows,
            read_at: account.read_at,
            reset_credits: account.reset_credits,
            evidence,
        }
    }

    /// Whether another account than `except` could take the thread's next request.
    pub fn has_other(&self, agent: Agent, except: &str, now: f64) -> bool {
        self.accounts().iter().any(|account| account.agent == agent && account.id != except && account.available(now))
    }

    /// How many of the request's input items came from another account than the thread's own: all of
    /// them on its first request after a move, as many as that one had after it.
    pub fn moved_items(&self, key: &ThreadKey, items: usize) -> Option<usize> {
        let mut router = self.router();
        let thread = router.thread_mut(key)?;
        match thread.moved {
            Moved::Never => None,
            Moved::Pending => {
                thread.moved = Moved::Items(items);
                Some(items)
            }
            Moved::Items(before) => Some(before),
        }
    }

    pub fn observe(&self, id: &str, reading: &Reading, now: f64) {
        let mut stored = None;
        self.change(id, |account| {
            let before = account.windows.clone();
            let stale = account.read_at.is_none_or(|at| now - at >= READING_EVERY);
            account.read(reading, now);
            if stale || !same_use(&before, &account.windows) {
                stored = Some(account.windows.clone());
            }
        });
        if let Some(windows) = stored
            && let Err(error) = self.store.add_reading(id, now, &windows)
        {
            tracing::warn!("couldn't keep a reading: {error:#}");
        }
    }

    pub fn rest(&self, id: &str, until: f64) {
        tracing::info!(account = id, until, "an account reached its limit and rests");
        self.change(id, |account| account.status = AccountStatus::Resting { until: Some(until) });
    }

    pub fn sign_in_again(&self, id: &str) {
        tracing::warn!(account = id, "an account's tokens stopped working; it has to be signed in again");
        self.change(id, |account| account.status = AccountStatus::SignInAgain);
    }

    /// The account's tokens, renewed first if they are about to expire.
    pub async fn tokens(&self, id: &str) -> Option<Tokens> {
        let account = self.account(id)?;
        if account.tokens.expires_at - now() > RENEW_BEFORE {
            return Some(account.tokens);
        }
        self.renew(id, &account.tokens.access).await
    }

    /// Renews the account's tokens, unless another renewal already replaced `stale`. An account whose
    /// refresh token was refused has to be signed in again.
    pub async fn renew(&self, id: &str, stale: &str) -> Option<Tokens> {
        let _renewing = self.renewing.lock().await;
        let account = self.account(id)?;
        if account.tokens.access != stale {
            return Some(account.tokens);
        }
        match oauth::renew(&self.http, &self.upstreams, account.agent, &account.tokens).await {
            Ok(renewed) => {
                let tokens = renewed.tokens.clone();
                self.change(id, |account| {
                    account.tokens = renewed.tokens;
                    account.plan = renewed.plan.or(account.plan.take());
                    if account.status == AccountStatus::SignInAgain {
                        account.status = AccountStatus::Ready;
                    }
                });
                tracing::info!(account = id, "renewed an account's tokens");
                Some(tokens)
            }
            Err(RenewError::Dead) => {
                self.sign_in_again(id);
                None
            }
            Err(RenewError::Failed(error)) => {
                tracing::warn!(account = id, "couldn't renew an account's tokens: {error:#}");
                None
            }
        }
    }

    pub fn record(&self, key: &ThreadKey, record: Record, captured: Option<Captured>) {
        self.router().spent(key, record.cost, record.finished_at);
        if let Err(error) = self.store.record(&record, captured.as_ref()) {
            tracing::warn!("couldn't log a request: {error:#}");
        }
    }

    pub fn state(&self) -> ProxyState {
        let now = now();
        let accounts = self.accounts();
        let next: Vec<String> = [Agent::Claude, Agent::Codex]
            .into_iter()
            .filter_map(|agent| self.rank(&accounts, agent, None, None, &[], now).into_iter().next())
            .collect();
        let router = self.router();
        let accounts = accounts
            .into_iter()
            .map(|account| ProxyAccount {
                threads: router.working(&account.id, now),
                next: next.contains(&account.id),
                status: account.status(now),
                id: account.id,
                agent: account.agent,
                name: account.name,
                email: account.email,
                plan: account.plan,
                priority: account.priority,
                enabled: account.enabled,
                windows: account.windows,
                reset_credits: account.reset_credits,
            })
            .collect();
        let (capture, capture_until) = self.capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        let log_head = self.store.log_head().unwrap_or_default();
        ProxyState { version: env!("CARGO_PKG_VERSION").to_string(), accounts, capture, capture_until, log_head }
    }

    /// The limits of the account a new thread of the agent starts on, or of the account that rests the
    /// shortest while when none can take one.
    pub fn pool(&self, agent: Agent) -> Option<AgentLimits> {
        let now = now();
        let accounts = self.accounts();
        let first = self.rank(&accounts, agent, None, None, &[], now).into_iter().next();
        let (account, error) = match first.and_then(|id| accounts.iter().find(|account| account.id == id)) {
            Some(account) => (account, None),
            None => {
                let usable = accounts.iter().filter(|account| account.agent == agent && account.enabled);
                let resting = usable.min_by(|a, b| rest_end(a).total_cmp(&rest_end(b)))?;
                (
                    resting,
                    Some(format!("Every {} account of your proxy is at its limit or signed out.", agent_title(agent))),
                )
            }
        };
        Some(AgentLimits {
            agent,
            account_name: ACCOUNT_NAME.to_string(),
            account: account.email.clone(),
            plan: account.plan.clone(),
            windows: account.windows.clone(),
            reset_credits: account.reset_credits,
            error,
        })
    }

    pub fn start_background(self: &Arc<Self>, fetch_prices: bool) {
        let proxy = self.clone();
        tokio::spawn(async move {
            loop {
                proxy.renew_expiring().await;
                tokio::time::sleep(RENEW_CHECK).await;
            }
        });
        let proxy = self.clone();
        tokio::spawn(async move {
            loop {
                proxy.poll_idle().await;
                tokio::time::sleep(POLL_CHECK).await;
            }
        });
        let proxy = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(TIDY_EVERY).await;
                let now = now();
                proxy.router().tidy(now);
                if let Err(error) = proxy.store.prune(now) {
                    tracing::warn!("couldn't prune the log: {error:#}");
                }
            }
        });
        if !fetch_prices {
            return;
        }
        let proxy = self.clone();
        tokio::spawn(async move {
            loop {
                match pricing::fetch(&proxy.http, &proxy.upstreams.prices).await {
                    Ok(prices) => {
                        pricing::keep(&proxy.data_dir.join("prices.json"), &prices);
                        proxy.set_prices(prices);
                    }
                    Err(error) => tracing::warn!("couldn't fetch the price list: {error:#}"),
                }
                tokio::time::sleep(PRICES_EVERY).await;
            }
        });
    }

    async fn renew_expiring(&self) {
        let now = now();
        let expiring = self.accounts().into_iter().filter(|account| {
            account.status != AccountStatus::SignInAgain && account.tokens.expires_at - now < RENEW_BEFORE
        });
        for account in expiring {
            let _ = self.renew(&account.id, &account.tokens.access).await;
        }
    }

    /// Asks the accounts nothing was read from lately for their limits.
    async fn poll_idle(&self) {
        let now = now();
        let idle = self.accounts().into_iter().filter(|account| {
            account.enabled
                && account.status != AccountStatus::SignInAgain
                && account.read_at.is_none_or(|at| now - at > POLL_AFTER)
        });
        for account in idle {
            if let Err(error) = self.poll(&account).await {
                tracing::debug!(account = account.id, "couldn't read an account's limits: {error:#}");
            }
        }
    }

    /// Asks the agent's usage page what the account has used.
    pub async fn poll(&self, account: &Account) -> anyhow::Result<()> {
        let Some(tokens) = self.tokens(&account.id).await else { return Ok(()) };
        let request = match account.agent {
            Agent::Claude => self
                .http
                .get(format!("{}/api/oauth/usage", self.upstreams.anthropic))
                .header("anthropic-beta", oauth::CLAUDE_OAUTH_BETA),
            Agent::Codex => self
                .http
                .get(format!("{}/wham/usage", self.upstreams.chatgpt))
                .header("chatgpt-account-id", tokens.chatgpt_account.clone().unwrap_or_default()),
        };
        let response = request.bearer_auth(&tokens.access).timeout(Duration::from_secs(15)).send().await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            let _ = self.renew(&account.id, &tokens.access).await;
            anyhow::bail!("its token was refused");
        }
        let usage: serde_json::Value = response.error_for_status()?.json().await?;
        let now = now();
        match account.agent {
            Agent::Claude => self.observe(&account.id, &limits::from_claude_usage(&usage), now),
            Agent::Codex => {
                let (reading, plan) = limits::from_codex_usage(&usage);
                self.observe(&account.id, &reading, now);
                if plan.is_some() {
                    self.change(&account.id, |account| account.plan = plan);
                }
            }
        }
        Ok(())
    }
}

fn same_use(before: &[motile_protocol::wire::LimitWindow], after: &[motile_protocol::wire::LimitWindow]) -> bool {
    before.len() == after.len()
        && before
            .iter()
            .zip(after)
            .all(|(a, b)| a.label == b.label && a.used_percent == b.used_percent && a.resets_at == b.resets_at)
}

fn rest_end(account: &Account) -> f64 {
    match account.status {
        AccountStatus::Resting { until } => until.unwrap_or(0.0),
        AccountStatus::SignInAgain => f64::INFINITY,
        AccountStatus::Ready => 0.0,
    }
}

pub fn agent_title(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "Claude",
        Agent::Codex => "Codex",
    }
}
