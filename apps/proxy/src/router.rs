//! Which account a thread's requests go to. A thread stays on its account until the account can't take
//! it, is turned off, or the thread was idle longer than its cache lasts; then it is placed again by
//! `select::rank`, from what the proxy measures: how many points of a window a dollar of the account's
//! spending takes (`k`), what is spent on it outside the proxy (`g`), what each thread spends an
//! hour (`d_t`) and what a new one is expected to (`d_x`).

use std::collections::{HashMap, VecDeque};

use motile_protocol::select::{self, Candidate, HORIZON, Window};
use motile_protocol::wire::{Agent, LimitWindow};

/// Dollars a thread spends an hour, until measured.
pub const D0: f64 = 8.0;
/// Dollars of evidence an account's `k` needs to be measured rather than guessed.
const TAU: f64 = 5.0;
/// How far back an account's readings and spending are compared.
pub const EVIDENCE: f64 = 6.0 * 3600.0;
const HALF_LIFE: f64 = 30.0 * 60.0;
/// A thread with no request for this long isn't working.
pub const ACTIVE_WITHIN: f64 = 10.0 * 60.0;
const FIRST_HOUR: f64 = 3600.0;
const HISTORY: f64 = 7.0 * 86400.0;
const SAMPLES: usize = 3;
/// Readings of a window that resets within this much of each other read the same window.
const SAME_RESET: f64 = 600.0;
/// Idle hours an account needs before what it spends outside the proxy counts.
const IDLE_EVIDENCE: f64 = 0.5;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ThreadKey {
    pub server: String,
    pub thread: String,
}

/// Where a thread's input stands with the account it is on. Codex's encrypted reasoning is only
/// read by the account that wrote it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Moved {
    Never,
    /// It moved, and its next request will be its first on the new account.
    Pending,
    /// Its first request on its account had this many input items, all from another account.
    Items(usize),
}

#[derive(Clone, Debug)]
pub struct Thread {
    pub agent: Agent,
    pub model: Option<String>,
    pub account: Option<String>,
    pub last_at: f64,
    /// How long its prompt cache lasts after a request.
    pub cache_lifetime: f64,
    pub moved: Moved,
    started_at: f64,
    session_start: f64,
    /// Dollars an hour, as of `rate_at`.
    rate: f64,
    rate_at: f64,
    first_hour: f64,
    first_hour_kept: bool,
}

impl Thread {
    fn rate(&self, now: f64) -> f64 {
        self.rate * 0.5f64.powf((now - self.rate_at).max(0.0) / HALF_LIFE)
    }

    fn active(&self, now: f64) -> bool {
        now - self.last_at < ACTIVE_WITHIN
    }

    /// Idle long enough that its cache is gone, so moving it costs nothing.
    pub fn cache_lapsed(&self, now: f64) -> bool {
        now - self.last_at > self.cache_lifetime
    }
}

#[derive(Default)]
pub struct Router {
    threads: HashMap<ThreadKey, Thread>,
    /// What new threads spent in their first hour.
    first_hours: VecDeque<(f64, Agent, Option<String>, f64)>,
    /// How long threads kept working without a break.
    sessions: VecDeque<(f64, f64)>,
}

impl Router {
    pub fn thread(&self, key: &ThreadKey) -> Option<&Thread> {
        self.threads.get(key)
    }

    pub fn thread_mut(&mut self, key: &ThreadKey) -> Option<&mut Thread> {
        self.threads.get_mut(key)
    }

    /// A request of the thread starts. Answers whether its cache lapsed since its last one.
    pub fn touch(&mut self, key: &ThreadKey, agent: Agent, model: Option<&str>, cache_lifetime: f64, now: f64) {
        let thread = self.threads.entry(key.clone()).or_insert_with(|| Thread {
            agent,
            model: model.map(String::from),
            account: None,
            last_at: now,
            cache_lifetime,
            moved: Moved::Never,
            started_at: now,
            session_start: now,
            rate: 0.0,
            rate_at: now,
            first_hour: 0.0,
            first_hour_kept: false,
        });
        if !thread.active(now) {
            self.sessions.push_back((now, thread.last_at - thread.session_start));
            thread.session_start = now;
        }
        thread.cache_lifetime = cache_lifetime;
    }

    /// Marks the request as the thread's latest, once its account was chosen.
    pub fn mark(&mut self, key: &ThreadKey, now: f64) {
        if let Some(thread) = self.threads.get_mut(key) {
            thread.last_at = now;
        }
    }

    /// Puts the thread on `account`. A new thread reserves what one is expected to spend there at once.
    pub fn assign(&mut self, key: &ThreadKey, account: &str, expected: f64, now: f64) {
        let Some(thread) = self.threads.get_mut(key) else { return };
        if thread.account.is_none() && thread.rate(now) < expected {
            thread.rate = expected;
            thread.rate_at = now;
        }
        thread.account = Some(account.to_string());
    }

    /// The thread spent `cost` dollars, in a request that finished `at`.
    pub fn spent(&mut self, key: &ThreadKey, cost: f64, at: f64) {
        let Some(thread) = self.threads.get_mut(key) else { return };
        thread.rate = thread.rate(at) + cost * std::f64::consts::LN_2 / (HALF_LIFE / 3600.0);
        thread.rate_at = at;
        if at - thread.started_at <= FIRST_HOUR {
            thread.first_hour += cost;
        }
    }

    /// Dollars an hour the threads working on the account spend.
    pub fn load(&self, account: &str, now: f64) -> f64 {
        self.working_on(account, now).map(|thread| thread.rate(now)).sum()
    }

    pub fn working(&self, account: &str, now: f64) -> u32 {
        self.working_on(account, now).count() as u32
    }

    fn working_on<'a>(&'a self, account: &'a str, now: f64) -> impl Iterator<Item = &'a Thread> + 'a {
        self.threads.values().filter(move |thread| thread.account.as_deref() == Some(account) && thread.active(now))
    }

    /// What a new thread of the agent and model is expected to spend an hour: the median of what
    /// recent new ones spent in their first hour.
    pub fn expected(&self, agent: Agent, model: Option<&str>) -> f64 {
        let spent: Vec<f64> = self
            .first_hours
            .iter()
            .filter(|(_, of, thread_model, _)| *of == agent && (model.is_none() || thread_model.as_deref() == model))
            .map(|(.., spent)| *spent)
            .collect();
        median(spent).unwrap_or(D0)
    }

    /// How long a thread is expected to keep working: the median of how long they did.
    pub fn horizon(&self) -> f64 {
        let lengths: Vec<f64> = self.sessions.iter().map(|(_, length)| *length).collect();
        median(lengths).map(|length| length.clamp(3600.0, 4.0 * 3600.0)).unwrap_or(HORIZON)
    }

    /// Keeps what threads that finished their first hour spent, and forgets old threads and samples.
    pub fn tidy(&mut self, now: f64) {
        for thread in self.threads.values_mut() {
            if !thread.first_hour_kept && now - thread.started_at > FIRST_HOUR {
                thread.first_hour_kept = true;
                self.first_hours.push_back((thread.started_at, thread.agent, thread.model.clone(), thread.first_hour));
            }
        }
        let sessions = &mut self.sessions;
        self.threads.retain(|_, thread| {
            let keep = now - thread.last_at < 86400.0;
            if !keep {
                sessions.push_back((now, thread.last_at - thread.session_start));
            }
            keep
        });
        self.first_hours.retain(|(at, ..)| now - at < HISTORY);
        self.sessions.retain(|(at, length)| now - at < HISTORY && *length > 0.0);
    }
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.len() < SAMPLES {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    Some(if values.len().is_multiple_of(2) { (values[middle - 1] + values[middle]) / 2.0 } else { values[middle] })
}

/// What a window's readings and the proxy's spending on the account say over the last hours.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Evidence {
    /// Points the window rose while the proxy spent, and over how many hours.
    pub busy_points: f64,
    pub busy_hours: f64,
    /// Dollars the proxy spent on the account.
    pub dollars: f64,
    /// Points the window rose while the proxy spent nothing, and over how many hours.
    pub idle_points: f64,
    pub idle_hours: f64,
}

impl Evidence {
    /// Compares each reading of the window with the one before, while it is the same window.
    pub fn read(readings: &[(f64, Vec<LimitWindow>)], costs: &[(f64, f64)], label: &str) -> Self {
        let mut evidence = Self::default();
        let window = |windows: &[LimitWindow]| windows.iter().find(|window| window.label == label).cloned();
        let read: Vec<(f64, LimitWindow)> =
            readings.iter().filter_map(|(at, windows)| Some((*at, window(windows)?))).collect();
        for pair in read.windows(2) {
            let ((from, before), (to, after)) = (&pair[0], &pair[1]);
            let same = match (before.resets_at, after.resets_at) {
                (Some(a), Some(b)) => (a - b).abs() < SAME_RESET,
                (a, b) => a.is_none() && b.is_none(),
            };
            let points = after.used_percent - before.used_percent;
            if !same || points < 0.0 {
                continue;
            }
            let hours = (to - from) / 3600.0;
            let dollars: f64 = costs.iter().filter(|(at, _)| at > from && at <= to).map(|(_, cost)| cost).sum();
            if dollars > 0.0 {
                evidence.busy_points += points;
                evidence.busy_hours += hours;
                evidence.dollars += dollars;
            } else {
                evidence.idle_points += points;
                evidence.idle_hours += hours;
            }
        }
        evidence
    }

    /// Points an hour spent outside the proxy.
    pub fn outside(&self) -> f64 {
        if self.idle_hours < IDLE_EVIDENCE {
            return 0.0;
        }
        (self.idle_points / self.idle_hours).max(0.0)
    }

    fn explained(&self) -> f64 {
        (self.busy_points - self.outside() * self.busy_hours).max(0.0)
    }

    pub fn measured(&self) -> bool {
        self.dollars >= TAU
    }

    /// Points a dollar, from this window's evidence alone, once there is enough of it.
    pub fn points_per_dollar(&self) -> Option<f64> {
        self.measured().then(|| self.explained() / self.dollars)
    }

    /// Points a dollar, drawn from `prior` as long as the evidence is thin.
    pub fn k(&self, prior: f64) -> f64 {
        (self.explained() + prior * TAU) / (self.dollars + TAU)
    }
}

/// An account as the rank weighs it.
#[derive(Clone)]
pub struct Facts<'a> {
    pub id: &'a str,
    pub plan: Option<&'a str>,
    pub priority: u32,
    pub windows: &'a [LimitWindow],
    pub read_at: Option<f64>,
    pub reset_credits: u32,
    /// By window label.
    pub evidence: HashMap<String, Evidence>,
}

/// What ranking a thread needs to know about it.
pub struct Placing<'a> {
    pub agent: Agent,
    /// `None` for a thread whose model isn't known yet: only the windows of every model count.
    pub model: Option<&'a str>,
    pub current: Option<&'a str>,
    pub expected: f64,
    pub now: f64,
}

/// The candidates for `select::rank`, in the order of `accounts`.
pub fn candidates(router: &Router, accounts: &[Facts], all: &[Facts], placing: &Placing) -> Vec<Candidate> {
    accounts
        .iter()
        .map(|account| {
            let load = router.load(account.id, placing.now);
            let windows = account
                .windows
                .iter()
                .filter(|window| match placing.model {
                    Some(model) => select::applies(window, model, model),
                    None => !window.label.contains(" · "),
                })
                .map(|window| {
                    let evidence = account.evidence.get(&window.label).cloned().unwrap_or_default();
                    let prior = prior_k(account, all, window, placing.agent);
                    let k = evidence.k(prior);
                    Window {
                        used: window.used_percent,
                        resets_at: window.resets_at,
                        length: select::length(window),
                        spend: evidence.outside() + k * load,
                        thread: k * placing.expected,
                    }
                })
                .collect();
            Candidate {
                windows,
                read_at: account.read_at,
                measured: account.evidence.values().any(Evidence::measured),
                priority: account.priority,
                current: placing.current == Some(account.id),
                reset_credits: account.reset_credits,
            }
        })
        .collect()
}

/// Points a dollar an account is assumed to take until measured: what accounts of its plan show, or the
/// plan's typical thread over the typical thread's dollars.
fn prior_k(account: &Facts, all: &[Facts], window: &LimitWindow, agent: Agent) -> f64 {
    let shown: Vec<f64> = all
        .iter()
        .filter(|other| other.id != account.id && other.plan == account.plan)
        .filter_map(|other| other.evidence.get(&window.label)?.points_per_dollar())
        .collect();
    if !shown.is_empty() {
        return shown.iter().sum::<f64>() / shown.len() as f64;
    }
    select::prior_thread_spend(agent, account.plan, select::length(window)) / D0
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: f64 = 1_000_000.0;
    const HOUR: f64 = 3600.0;

    fn key(thread: &str) -> ThreadKey {
        ThreadKey { server: "s".into(), thread: thread.into() }
    }

    fn session(used: f64) -> Vec<LimitWindow> {
        vec![LimitWindow {
            label: "Session".into(),
            used_percent: used,
            resets_at: Some(NOW + 4.0 * HOUR),
            window_secs: Some(18000),
            warning: false,
        }]
    }

    #[test]
    fn k_is_points_over_dollars_once_measured_and_the_prior_before() {
        let readings = vec![(NOW, session(10.0)), (NOW + 600.0, session(14.0)), (NOW + 1200.0, session(18.0))];
        let costs = vec![(NOW + 300.0, 2.0), (NOW + 900.0, 6.0)];
        let evidence = Evidence::read(&readings, &costs, "Session");
        assert_eq!((evidence.busy_points, evidence.dollars), (8.0, 8.0));
        assert_eq!(evidence.points_per_dollar(), Some(1.0));
        assert!((evidence.k(3.0) - (8.0 + 15.0) / 13.0).abs() < 1e-9);

        let thin = Evidence::read(&readings[..2], &costs[..1], "Session");
        assert_eq!(thin.points_per_dollar(), None);
        assert!((thin.k(3.0) - (4.0 + 15.0) / 7.0).abs() < 1e-9);
    }

    #[test]
    fn what_rises_while_the_proxy_spends_nothing_is_spent_outside_it() {
        let readings = vec![(NOW, session(10.0)), (NOW + HOUR, session(16.0)), (NOW + 2.0 * HOUR, session(30.0))];
        let costs = vec![(NOW + 1.5 * HOUR, 8.0)];
        let evidence = Evidence::read(&readings, &costs, "Session");
        assert_eq!(evidence.outside(), 6.0);
        assert_eq!(evidence.points_per_dollar(), Some((14.0 - 6.0) / 8.0));
    }

    #[test]
    fn a_window_that_reset_between_readings_is_not_compared() {
        let mut reset = session(2.0);
        reset[0].resets_at = Some(NOW + 9.0 * HOUR);
        let readings = vec![(NOW, session(90.0)), (NOW + 600.0, reset)];
        assert_eq!(Evidence::read(&readings, &[(NOW + 300.0, 10.0)], "Session"), Evidence::default());
    }

    #[test]
    fn a_threads_rate_halves_every_half_hour_and_a_new_thread_reserves_the_expected() {
        let mut router = Router::default();
        router.touch(&key("t"), Agent::Claude, Some("m"), HOUR, NOW);
        router.assign(&key("t"), "a", D0, NOW);
        router.mark(&key("t"), NOW);
        assert_eq!((router.load("a", NOW), router.working("a", NOW)), (D0, 1));
        assert_eq!(router.working("a", NOW + ACTIVE_WITHIN + 1.0), 0);

        router.mark(&key("t"), NOW + HALF_LIFE);
        assert!((router.load("a", NOW + HALF_LIFE) - D0 / 2.0).abs() < 1e-9);
        router.spent(&key("t"), 1.0, NOW + HALF_LIFE);
        let added = 2.0 * std::f64::consts::LN_2;
        assert!((router.load("a", NOW + HALF_LIFE) - (D0 / 2.0 + added)).abs() < 1e-9);
    }

    #[test]
    fn a_new_thread_is_expected_to_spend_what_recent_ones_did_in_their_first_hour() {
        let mut router = Router::default();
        assert_eq!(router.expected(Agent::Codex, Some("gpt")), D0);
        for (index, spent) in [2.0, 4.0, 30.0].iter().enumerate() {
            let thread = key(&index.to_string());
            router.touch(&thread, Agent::Codex, Some("gpt"), HOUR, NOW);
            router.spent(&thread, *spent, NOW + 60.0);
            router.spent(&thread, 100.0, NOW + 2.0 * HOUR);
        }
        router.tidy(NOW + 2.0 * HOUR);
        assert_eq!(router.expected(Agent::Codex, Some("gpt")), 4.0);
        assert_eq!(router.expected(Agent::Codex, Some("other")), D0);
        assert_eq!(router.expected(Agent::Claude, None), D0);
    }

    #[test]
    fn the_horizon_is_the_median_working_stretch_kept_between_one_and_four_hours() {
        let mut router = Router::default();
        assert_eq!(router.horizon(), HORIZON);
        for (index, hours) in [0.2, 0.5, 3.0].iter().enumerate() {
            let thread = key(&index.to_string());
            router.touch(&thread, Agent::Claude, None, HOUR, NOW);
            router.mark(&thread, NOW + hours * HOUR);
            router.touch(&thread, Agent::Claude, None, HOUR, NOW + 5.0 * HOUR);
        }
        assert_eq!(router.horizon(), HOUR);
    }

    #[test]
    fn a_account_of_the_same_plan_lends_its_measured_k() {
        let windows = session(10.0);
        let measured = Evidence { busy_points: 20.0, busy_hours: 1.0, dollars: 10.0, ..Evidence::default() };
        let facts = |id, plan, evidence: Option<Evidence>| Facts {
            id,
            plan,
            priority: 0,
            windows: &windows,
            read_at: Some(NOW),
            reset_credits: 0,
            evidence: evidence.into_iter().map(|evidence| ("Session".to_string(), evidence)).collect(),
        };
        let all =
            [facts("a", Some("Max"), Some(measured)), facts("b", Some("Max"), None), facts("c", Some("Pro"), None)];
        assert_eq!(prior_k(&all[1], &all, &windows[0], Agent::Claude), 2.0);
        let pro = select::prior_thread_spend(Agent::Claude, Some("Pro"), 18000.0) / D0;
        assert_eq!(prior_k(&all[2], &all, &windows[0], Agent::Claude), pro);
    }
}
