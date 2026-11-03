//! Which account a new thread starts on. Of the accounts that can carry it through the next hours
//! without running dry, the one whose spare quota lapses soonest: first to expire, first spent. The
//! gateway ranks its accounts with it, from what it measures; the clients rank a server's own accounts.

use crate::wire::{Agent, LimitWindow};

/// How far ahead a thread is expected to keep working, until its own length was measured.
pub const HORIZON: f64 = 2.0 * 3600.0;
/// How much more an account must be able to carry than it is asked to, when what it spends was
/// measured, and when it is still a guess.
const MARGIN_MEASURED: f64 = 1.2;
const MARGIN_PRIOR: f64 = 1.5;
/// Spare quota that lapses within this much of another's counts as lapsing together.
const SAME_LAPSE: f64 = 30.0 * 60.0;
/// A reading older than this is moved on by what the account spends meanwhile.
const PROJECT_AFTER: f64 = 120.0;
/// A reading older than this says nothing.
const STALE_AFTER: f64 = 15.0 * 60.0;
const SESSION: f64 = 5.0 * 3600.0;
const WEEK: f64 = 7.0 * 86400.0;

/// One of an account's windows that applies to the thread's model.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    /// Percent.
    pub used: f64,
    pub resets_at: Option<f64>,
    /// Seconds.
    pub length: f64,
    /// Points per hour what already runs on the account spends in it: its threads and anything
    /// outside the gateway.
    pub spend: f64,
    /// Points per hour the new thread would spend in it.
    pub thread: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub windows: Vec<Window>,
    /// When the windows were read. `None` when they never were.
    pub read_at: Option<f64>,
    /// What it spends comes from measurements rather than guesses.
    pub measured: bool,
    /// Lower first.
    pub priority: u32,
    /// The thread is on it already.
    pub current: bool,
    pub reset_credits: u32,
}

/// The candidates' indices, the one to start on first.
pub fn rank(candidates: &[Candidate], now: f64, horizon: f64) -> Vec<usize> {
    let mut keyed: Vec<(Vec<f64>, usize)> =
        candidates.iter().enumerate().map(|(index, candidate)| (key(candidate, index, now, horizon), index)).collect();
    keyed.sort_by(|(a, _), (b, _)| {
        a.iter().zip(b).map(|(a, b)| a.total_cmp(b)).find(|order| order.is_ne()).unwrap_or(std::cmp::Ordering::Equal)
    });
    keyed.into_iter().map(|(_, index)| index).collect()
}

/// Feasible accounts by tier, lapse and fit; then those never read; then those a reset credit
/// would save; then the rest by how long they would stall the thread.
fn key(candidate: &Candidate, index: usize, now: f64, horizon: f64) -> Vec<f64> {
    let order = index as f64;
    let priority = candidate.priority as f64;
    let Some(read_at) = candidate.read_at.filter(|at| now - at <= STALE_AFTER) else {
        return vec![1.0, priority, order];
    };
    let windows: Vec<Live> = candidate.windows.iter().map(|window| Live::new(window, now - read_at, now)).collect();
    let fit = fit(&windows, horizon);
    let margin = if candidate.measured { MARGIN_MEASURED } else { MARGIN_PRIOR };
    if fit >= margin {
        let lapse = (lapse(&windows) / SAME_LAPSE).floor();
        return vec![0.0, priority, lapse, if candidate.current { 0.0 } else { 1.0 }, -fit, order];
    }
    if candidate.reset_credits > 0 {
        return vec![2.0, priority, -fit, order];
    }
    vec![3.0, stall(&windows), order]
}

/// A window as it is now: its room in points, the seconds until it resets, and the points per
/// hour it would be spent at with the thread on it.
struct Live {
    room: f64,
    left: f64,
    rate: f64,
}

impl Live {
    fn new(window: &Window, age: f64, now: f64) -> Self {
        let rate = (window.spend + window.thread).max(0.0);
        let Some(at) = window.resets_at.filter(|at| *at > now) else {
            return Self { room: 100.0, left: window.length, rate };
        };
        let projected = if age > PROJECT_AFTER { window.spend.max(0.0) * age / 3600.0 } else { 0.0 };
        Self { room: (100.0 - window.used - projected).clamp(0.0, 100.0), left: at - now, rate }
    }

    /// How many times over the room covers what the next `seconds` take.
    fn covers(&self, room: f64, seconds: f64) -> f64 {
        let need = self.rate * seconds / 3600.0;
        match (room, need) {
            (room, _) if room <= 0.0 => 0.0,
            (_, need) if need <= 0.0 => f64::INFINITY,
            (room, need) => room / need,
        }
    }
}

/// How many times over the account can carry what runs on it and the thread through the horizon:
/// each window until it resets, and the fresh window after, if it resets within the horizon.
fn fit(windows: &[Live], horizon: f64) -> f64 {
    windows
        .iter()
        .map(|window| {
            let before = window.covers(window.room, window.left.min(horizon));
            let after = if window.left < horizon { window.covers(100.0, horizon - window.left) } else { f64::INFINITY };
            before.min(after)
        })
        .fold(f64::INFINITY, f64::min)
}

/// Seconds until the first of the account's quota that nothing would spend lapses. A window's room is
/// only as large as the tightest window's.
fn lapse(windows: &[Live]) -> f64 {
    let cap = windows.iter().map(|window| window.room).fold(f64::INFINITY, f64::min);
    windows
        .iter()
        .filter(|window| cap - window.rate * window.left / 3600.0 > 0.0)
        .map(|window| window.left)
        .fold(f64::INFINITY, f64::min)
}

/// Seconds the thread would wait between the account running dry and its reset.
fn stall(windows: &[Live]) -> f64 {
    windows
        .iter()
        .map(|window| match window.rate {
            _ if window.room <= 0.0 => window.left,
            rate if rate <= 0.0 => 0.0,
            rate => (window.left - window.room / rate * 3600.0).max(0.0),
        })
        .fold(0.0, f64::max)
}

/// Points per hour a typical thread spends in a window of `length` seconds on `plan`, until what
/// it spends was measured.
pub fn prior_thread_spend(agent: Agent, plan: Option<&str>, length: f64) -> f64 {
    let plan = plan.unwrap_or_default().to_lowercase();
    let scale = match agent {
        Agent::Claude if plan.contains("20x") => 0.3,
        Agent::Claude if plan.contains("max") => 1.0,
        Agent::Claude if plan.contains("pro") => 3.0,
        Agent::Codex if plan.contains("pro") => 0.25,
        _ => 1.0,
    };
    let session = 15.0 * scale;
    if length <= 2.0 * SESSION { session } else { session / 10.0 }
}

/// Points per hour the window was spent at since it started, once it has run long enough to say.
pub fn average_spend(window: &LimitWindow, now: f64) -> Option<f64> {
    let at = window.resets_at.filter(|at| *at > now)?;
    let elapsed = length(window) - (at - now);
    if elapsed < 30.0 * 60.0 {
        return None;
    }
    Some(window.used_percent.max(0.0) / (elapsed / 3600.0))
}

/// The window's length in seconds: what the agent said, or what its label means.
pub fn length(window: &LimitWindow) -> f64 {
    let fallback = if window.label.starts_with("Weekly") { WEEK } else { SESSION };
    window.window_secs.map(|secs| secs as f64).unwrap_or(fallback)
}

/// "Weekly · Opus" only counts what an Opus model spends.
pub fn applies(window: &LimitWindow, model_id: &str, model_name: &str) -> bool {
    let Some((_, only)) = window.label.split_once(" · ") else { return true };
    let only = only.to_lowercase();
    model_name.to_lowercase().contains(&only) || model_id.to_lowercase().contains(&only)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: f64 = 1_000_000.0;
    const HOUR: f64 = 3600.0;

    fn session(used: f64, resets_in: f64, spend: f64) -> Window {
        Window { used, resets_at: Some(NOW + resets_in), length: SESSION, spend, thread: 15.0 }
    }

    fn weekly(used: f64, resets_in: f64) -> Window {
        Window { used, resets_at: Some(NOW + resets_in), length: WEEK, spend: 0.0, thread: 1.5 }
    }

    fn account(windows: Vec<Window>) -> Candidate {
        Candidate { windows, read_at: Some(NOW), measured: true, priority: 0, current: false, reset_credits: 0 }
    }

    fn first(candidates: &[Candidate]) -> usize {
        rank(candidates, NOW, HORIZON)[0]
    }

    #[test]
    fn a_nearly_spent_account_that_resets_in_an_hour_is_skipped_but_one_that_resets_in_minutes_is_drained() {
        let roomy = account(vec![session(30.0, 4.0 * HOUR, 0.0)]);
        let strands = account(vec![session(95.0, HOUR, 0.0)]);
        assert_eq!(first(&[strands.clone(), roomy.clone()]), 1);

        let lapses = account(vec![session(95.0, 0.25 * HOUR, 0.0)]);
        assert_eq!(first(&[roomy, lapses]), 1);
    }

    #[test]
    fn the_quota_that_lapses_soonest_is_spent_first() {
        let late = account(vec![session(30.0, 4.5 * HOUR, 0.0)]);
        let soon = account(vec![session(60.0, 1.5 * HOUR, 0.0)]);
        assert_eq!(first(&[late, soon]), 1);
    }

    #[test]
    fn a_nearly_spent_week_rules_out_a_roomy_session() {
        let week_spent = account(vec![session(10.0, 0.5 * HOUR, 0.0), weekly(98.0, 3.0 * 86400.0)]);
        let other = account(vec![session(40.0, 4.0 * HOUR, 0.0), weekly(30.0, 5.0 * 86400.0)]);
        assert_eq!(first(&[week_spent, other]), 1);
    }

    #[test]
    fn threads_that_start_together_are_spread_by_what_each_adds() {
        let resetting = |spend| account(vec![session(80.0, 0.2 * HOUR, spend)]);
        let fresh = account(vec![session(0.0, 5.0 * HOUR, 0.0)]);
        assert_eq!(first(&[resetting(0.0), fresh.clone()]), 0);
        assert_eq!(first(&[resetting(45.0), fresh]), 1);
    }

    #[test]
    fn the_current_account_keeps_the_thread_among_equals() {
        let mut current = account(vec![session(50.0, 3.0 * HOUR, 0.0)]);
        current.current = true;
        let other = account(vec![session(20.0, 3.1 * HOUR, 0.0)]);
        assert_eq!(first(&[other, current]), 1);
    }

    #[test]
    fn a_lower_tier_only_takes_the_thread_when_no_higher_one_can() {
        let mut backup = account(vec![session(0.0, 0.5 * HOUR, 0.0)]);
        backup.priority = 1;
        let main = account(vec![session(40.0, 4.0 * HOUR, 0.0)]);
        assert_eq!(first(&[backup.clone(), main]), 1);
        assert_eq!(first(&[backup, account(vec![session(99.0, 4.0 * HOUR, 0.0)])]), 0);
    }

    #[test]
    fn accounts_never_read_come_after_feasible_ones_and_before_spent_ones() {
        let unread = Candidate { read_at: None, ..account(vec![]) };
        let spent = account(vec![session(100.0, 2.0 * HOUR, 0.0)]);
        let roomy = account(vec![session(10.0, 4.0 * HOUR, 0.0)]);
        assert_eq!(rank(&[spent.clone(), unread.clone(), roomy], NOW, HORIZON), [2, 1, 0]);

        let stale = Candidate { read_at: Some(NOW - 3600.0), ..account(vec![session(10.0, 4.0 * HOUR, 0.0)]) };
        assert_eq!(rank(&[spent, stale], NOW, HORIZON), [1, 0]);
    }

    #[test]
    fn when_every_account_is_spent_the_one_that_stalls_least_goes_first_after_those_a_credit_saves() {
        let long = account(vec![session(100.0, 3.0 * HOUR, 0.0)]);
        let short = account(vec![session(100.0, 0.5 * HOUR, 0.0)]);
        assert_eq!(first(&[long.clone(), short.clone()]), 1);

        let credited = Candidate { reset_credits: 1, ..long };
        assert_eq!(first(&[short, credited]), 1);
    }

    #[test]
    fn a_reading_that_aged_is_moved_on_by_what_the_account_spends() {
        let read = |age| Candidate { read_at: Some(NOW - age), ..account(vec![session(70.0, 4.0 * HOUR, 20.0)]) };
        assert_eq!(first(&[read(0.0), account(vec![session(50.0, 4.5 * HOUR, 0.0)])]), 1);
        assert!(
            fit(&[Live::new(&read(600.0).windows[0], 600.0, NOW)], HORIZON)
                < fit(&[Live::new(&read(0.0).windows[0], 0.0, NOW)], HORIZON)
        );
    }

    #[test]
    fn a_window_past_its_reset_or_not_started_is_whole() {
        let past = Window { resets_at: Some(NOW - 10.0), ..session(100.0, 0.0, 0.0) };
        let unstarted = Window { resets_at: None, ..session(100.0, 0.0, 0.0) };
        assert_eq!(first(&[account(vec![session(100.0, HOUR, 0.0)]), account(vec![past])]), 1);
        assert_eq!(first(&[account(vec![session(100.0, HOUR, 0.0)]), account(vec![unstarted])]), 1);
    }

    #[test]
    fn a_guess_needs_more_room_than_a_measurement() {
        let tight = vec![session(60.0, 4.0 * HOUR, 0.0)];
        let spent = account(vec![session(100.0, 0.5 * HOUR, 0.0)]);
        assert_eq!(first(&[spent.clone(), account(tight.clone())]), 1);
        assert_eq!(first(&[spent, Candidate { measured: false, ..account(tight) }]), 0);
    }

    #[test]
    fn priors_scale_with_the_plan_and_the_window() {
        assert!(
            prior_thread_spend(Agent::Claude, Some("Pro"), SESSION)
                > prior_thread_spend(Agent::Claude, Some("Max"), SESSION)
        );
        assert!(
            prior_thread_spend(Agent::Claude, Some("Max 20x"), SESSION)
                < prior_thread_spend(Agent::Claude, Some("Max"), SESSION)
        );
        assert!(prior_thread_spend(Agent::Codex, None, WEEK) < prior_thread_spend(Agent::Codex, None, SESSION));
    }

    #[test]
    fn the_average_spend_waits_for_half_an_hour_of_the_window() {
        let window = |resets_in: f64| LimitWindow {
            label: "Session".into(),
            used_percent: 30.0,
            resets_at: Some(NOW + resets_in),
            window_secs: Some(18000),
            warning: false,
        };
        assert_eq!(average_spend(&window(4.8 * HOUR), NOW), None);
        assert_eq!(average_spend(&window(3.0 * HOUR), NOW), Some(15.0));
    }
}
