//! How much of their plans the agents' accounts have used, from every server, for the Limits tab
//! of the usage view: a section for each account and each of the proxy's accounts, with a row
//! for each of its windows. And which account a new thread is better off starting on.

use std::collections::HashMap;

use motile_protocol::proxy_wire::{AccountStatus, ProxyAccount};
use motile_protocol::select::{self, Candidate, HORIZON, Window};
use motile_protocol::wire::{Agent, AgentAccount, AgentLimits, LimitWindow, ModelInfo};
use serde::Serialize;

/// How far spending may stray from the time that has passed and still be on pace, in points.
const ON_PACE: f64 = 5.0;

/// What a server said of its agents' accounts.
pub struct Read {
    pub server: String,
    pub limits: AgentLimits,
    /// The server's Proxy account: the proxy's account a new thread would start on.
    pub proxy: bool,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Section {
    pub agent: Agent,
    /// The account's name on the server, when it has one.
    pub name: Option<String>,
    /// Who is signed in.
    pub account: Option<String>,
    pub plan: Option<String>,
    /// The servers with this account.
    pub servers: Vec<String>,
    pub windows: Vec<Row>,
    /// Why there are no windows.
    pub note: Option<String>,
    /// One of the proxy's accounts.
    pub proxy: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Row {
    pub label: String,
    /// Between 0 and 100.
    pub used_percent: f64,
    /// "34%".
    pub used: String,
    /// "3h 30m", "5d 11h".
    pub resets_in: Option<String>,
    pub pace: Option<Pace>,
    pub tone: Tone,
    /// Resets the account may use to start its windows over.
    pub reset_credits: u32,
}

/// The colour a window's bar takes: calm until half is used, then pending, warning from three
/// quarters or when the agent says so.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Success,
    Pending,
    Warning,
}

/// How what was used compares with how much of the window has passed.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Pace {
    Ahead,
    On,
    Under,
}

/// The sections, Claude Code's first, an account several servers share once. The proxy's `accounts`
/// come after the servers' of their agent, each once: one a server has too is that server's section,
/// marked as the proxy's. While they are known, what the servers' Proxy accounts said is left out.
pub fn sections(reads: Vec<Read>, accounts: &[ProxyAccount], now: f64) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    let mut reads: Vec<Read> = reads.into_iter().filter(|read| !read.proxy || accounts.is_empty()).collect();
    reads.sort_by_key(|read| {
        (read.limits.agent != Agent::Claude, read.limits.error.is_some(), read.limits.windows.is_empty())
    });
    for Read { server, limits, proxy } in reads {
        let known = limits.account.as_ref().and_then(|account| {
            sections
                .iter_mut()
                .find(|section| section.agent == limits.agent && section.account.as_ref() == Some(account))
        });
        if let Some(section) = known {
            section.servers.push(server);
            continue;
        }
        sections.push(Section { proxy, ..section(server, limits, now) });
    }
    for account in accounts {
        let windows: Vec<Row> = account.windows.iter().map(|window| row(window, account.reset_credits, now)).collect();
        let shared = account.email.as_ref().and_then(|email| {
            sections
                .iter_mut()
                .find(|section| section.agent == account.agent && section.account.as_ref() == Some(email))
        });
        if let Some(section) = shared {
            section.proxy = true;
            if !windows.is_empty() {
                section.windows = windows;
                section.note = None;
            }
            continue;
        }
        sections.push(Section {
            agent: account.agent,
            name: Some(account.name.clone()).filter(|name| account.email.as_ref() != Some(name)),
            account: account.email.clone(),
            plan: account.plan.clone(),
            servers: Vec::new(),
            note: account_note(account, &windows),
            windows,
            proxy: true,
        });
    }
    sections.sort_by_key(|section| section.agent != Agent::Claude);
    sections
}

fn section(server: String, limits: AgentLimits, now: f64) -> Section {
    let note = note(&server, &limits);
    let windows = limits.windows.iter().map(|window| row(window, limits.reset_credits, now)).collect();
    let name = Some(limits.account_name).filter(|name| !name.is_empty());
    Section {
        agent: limits.agent,
        name,
        account: limits.account,
        plan: limits.plan,
        servers: vec![server],
        windows,
        note,
        proxy: false,
    }
}

/// Why an account of the proxy shows no windows.
pub(crate) fn account_note(account: &ProxyAccount, windows: &[Row]) -> Option<String> {
    if !windows.is_empty() {
        return None;
    }
    Some(match account.status {
        AccountStatus::SignInAgain => "Sign in again to see its limits.".to_string(),
        _ => "No limits read yet.".to_string(),
    })
}

fn note(server: &str, limits: &AgentLimits) -> Option<String> {
    if let Some(error) = &limits.error {
        return Some(format!("Couldn't read the limits on {server}. {error}"));
    }
    if !limits.windows.is_empty() {
        return None;
    }
    match limits.plan {
        Some(_) => Some(format!("Couldn't read the limits on {server}.")),
        None => Some("Not signed in to a plan.".to_string()),
    }
}

pub(crate) fn row(window: &LimitWindow, reset_credits: u32, now: f64) -> Row {
    let used_percent = window.used_percent.clamp(0.0, 100.0);
    Row {
        label: window.label.clone(),
        used_percent,
        used: format!("{}%", used_percent.round()),
        resets_in: window.resets_at.map(|at| duration(at - now)),
        pace: pace(used_percent, window, now),
        tone: tone(used_percent, window.warning),
        reset_credits,
    }
}

fn tone(used_percent: f64, warning: bool) -> Tone {
    match used_percent {
        _ if warning || used_percent >= 75.0 => Tone::Warning,
        _ if used_percent >= 50.0 => Tone::Pending,
        _ => Tone::Success,
    }
}

fn pace(used_percent: f64, window: &LimitWindow, now: f64) -> Option<Pace> {
    if used_percent == 0.0 {
        return Some(Pace::Under);
    }
    let length = window.window_secs? as f64;
    let left = (window.resets_at? - now).clamp(0.0, length);
    let passed = (length - left) / length * 100.0;
    Some(match used_percent - passed {
        gap if gap > ON_PACE => Pace::Ahead,
        gap if gap < -ON_PACE => Pace::Under,
        _ => Pace::On,
    })
}

/// "5d 11h", "3h 30m", "12m", the two largest units.
pub(crate) fn duration(seconds: f64) -> String {
    let minutes = (seconds.max(0.0) / 60.0).ceil() as u64;
    let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
    match (days, hours) {
        (0, 0) if minutes == 0 => "now".to_string(),
        (0, 0) => format!("{minutes}m"),
        (0, _) => format!("{hours}h {minutes}m"),
        _ => format!("{days}d {hours}h"),
    }
}

/// A server's agents' accounts and what is known of their limits, for choosing the account a new
/// thread starts on with `motile_protocol::select`.
pub struct Accounts<'a> {
    pub accounts: &'a [AgentAccount],
    pub limits: &'a [AgentLimits],
    /// When the server read the limits.
    pub read_at: Option<f64>,
    /// How many threads work on each account now, by account id.
    pub working: &'a HashMap<String, u32>,
}

impl Accounts<'_> {
    /// For each account and model, the account of the same agent that runs the model too and that
    /// a new thread is better off on, when it isn't the account itself: by account id, then model id.
    pub fn roomier(&self, models: &[ModelInfo], now: f64) -> HashMap<String, HashMap<String, String>> {
        let mut roomier: HashMap<String, HashMap<String, String>> = HashMap::new();
        for account in self.accounts {
            for model in models.iter().filter(|model| runs(account, model) && self.counts(account, Some(model))) {
                let others = self.accounts.iter().filter(|other| other.id != account.id).filter_map(|other| {
                    let model = models.iter().find(|listed| listed.id == model.id && runs(other, listed))?;
                    self.counts(other, Some(model)).then_some((other, model))
                });
                let candidates: Vec<_> = std::iter::once((account, model)).chain(others).collect();
                let ranked: Vec<Candidate> = candidates
                    .iter()
                    .enumerate()
                    .filter_map(|(index, (account, model))| {
                        Some(self.candidate(account, self.read(account)?, Some(model), index == 0, now))
                    })
                    .collect();
                let Some(&first) = select::rank(&ranked, now, HORIZON).first() else { continue };
                if first != 0 {
                    roomier
                        .entry(account.id.clone())
                        .or_default()
                        .insert(model.id.clone(), candidates[first].0.id.clone());
                }
            }
        }
        roomier
    }

    /// The accounts' ids, each agent's in the order a new thread would choose them, counting the
    /// windows of every model. Those whose limits weren't read keep their places after the others.
    pub fn by_room(&self, now: f64) -> Vec<String> {
        let mut agents: Vec<Agent> = Vec::new();
        for account in self.accounts {
            if !agents.contains(&account.agent) {
                agents.push(account.agent);
            }
        }
        let mut ordered = Vec::new();
        for agent in agents {
            let of_agent = self.accounts.iter().filter(|account| account.agent == agent);
            let (read, unread): (Vec<_>, Vec<_>) = of_agent.partition(|account| self.counts(account, None));
            let ranked: Vec<Candidate> = read
                .iter()
                .filter_map(|account| Some(self.candidate(account, self.read(account)?, None, false, now)))
                .collect();
            ordered.extend(select::rank(&ranked, now, HORIZON).into_iter().map(|index| read[index].id.clone()));
            ordered.extend(unread.into_iter().map(|account| account.id.clone()));
        }
        ordered
    }

    /// Whether the account's limits were read and have a window that counts for the model.
    fn counts(&self, account: &AgentAccount, model: Option<&ModelInfo>) -> bool {
        let (id, name) = model.map_or(("", ""), |model| (model.id.as_str(), model.name.as_str()));
        self.read(account).is_some_and(|limits| limits.windows.iter().any(|window| select::applies(window, id, name)))
    }

    /// The account as the rule sees it. Each window is spent at the account's own rate since it
    /// started, which its working threads share; until that says something, a typical thread's.
    fn candidate(
        &self,
        account: &AgentAccount,
        limits: &AgentLimits,
        model: Option<&ModelInfo>,
        current: bool,
        now: f64,
    ) -> Candidate {
        let (id, name) = model.map_or(("", ""), |model| (model.id.as_str(), model.name.as_str()));
        let working = self.working.get(&account.id).copied().unwrap_or_default().max(1) as f64;
        let plan = limits.plan.as_deref().or(account.plan.as_deref());
        let mut measured = true;
        let windows = limits
            .windows
            .iter()
            .filter(|window| select::applies(window, id, name))
            .map(|window| {
                let length = select::length(window);
                let average = select::average_spend(window, now).filter(|spend| *spend > 0.0);
                measured &= average.is_some();
                let thread = average
                    .map_or_else(|| select::prior_thread_spend(account.agent, plan, length), |spend| spend / working);
                Window {
                    used: window.used_percent.clamp(0.0, 100.0),
                    resets_at: window.resets_at,
                    length,
                    spend: average.unwrap_or_default(),
                    thread,
                }
            })
            .collect();
        Candidate {
            windows,
            read_at: self.read_at,
            measured,
            priority: 0,
            current,
            reset_credits: limits.reset_credits,
        }
    }

    fn read(&self, account: &AgentAccount) -> Option<&AgentLimits> {
        read(account, self.limits)
    }
}

/// The windows of each account of a server, by account id, for the model picker.
pub fn by_account(accounts: &[AgentAccount], limits: &[AgentLimits], now: f64) -> HashMap<String, Vec<Row>> {
    accounts
        .iter()
        .filter_map(|account| {
            let limits = read(account, limits)?;
            let windows = limits.windows.iter().map(|window| row(window, limits.reset_credits, now)).collect();
            Some((account.id.clone(), windows))
        })
        .collect()
}

fn read<'a>(account: &AgentAccount, limits: &'a [AgentLimits]) -> Option<&'a AgentLimits> {
    limits
        .iter()
        .find(|limits| limits.agent == account.agent && limits.account_name == account.name && limits.error.is_none())
}

fn runs(account: &AgentAccount, model: &ModelInfo) -> bool {
    model.agent == account.agent && (model.account.is_empty() || model.account == account.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(label: &str, used_percent: f64, resets_in: f64, window_secs: u64) -> LimitWindow {
        LimitWindow {
            label: label.into(),
            used_percent,
            resets_at: Some(1000.0 + resets_in),
            window_secs: Some(window_secs),
            warning: false,
        }
    }

    fn read(server: &str, agent: Agent, account: Option<&str>, windows: Vec<LimitWindow>) -> Read {
        let limits = AgentLimits {
            agent,
            account_name: String::new(),
            account: account.map(String::from),
            plan: Some("Max".into()),
            windows,
            reset_credits: 2,
            error: None,
        };
        Read { server: server.into(), limits, proxy: false }
    }

    #[test]
    fn a_window_says_what_was_used_when_it_resets_and_how_that_compares_with_the_time_passed() {
        let session = window("Session", 80.0, 3600.0 + 1800.0, 18000);
        let weekly = window("Weekly", 4.6, 5.0 * 86400.0 + 11.0 * 3600.0, 604800);
        let even = window("Weekly", 50.0, 302400.0, 604800);
        let mut flagged = window("Weekly · Fable", 30.0, 302400.0, 604800);
        flagged.warning = true;
        let mut unused = window("Weekly · Opus", 0.0, 0.0, 604800);
        unused.resets_at = None;
        let windows = vec![session, weekly, even, flagged, unused];
        let [section] = &sections(vec![read("studio", Agent::Codex, Some("a"), windows)], &[], 1000.0)[..] else {
            panic!("one section")
        };
        let rows: Vec<_> = section
            .windows
            .iter()
            .map(|row| (row.used.as_str(), row.resets_in.as_deref(), row.pace, row.tone))
            .collect();
        assert_eq!(
            rows,
            [
                ("80%", Some("1h 30m"), Some(Pace::Ahead), Tone::Warning),
                ("5%", Some("5d 11h"), Some(Pace::Under), Tone::Success),
                ("50%", Some("3d 12h"), Some(Pace::On), Tone::Pending),
                ("30%", Some("3d 12h"), Some(Pace::Under), Tone::Warning),
                ("0%", None, Some(Pace::Under), Tone::Success),
            ]
        );
        assert_eq!(section.windows[0].reset_credits, 2);
    }

    #[test]
    fn a_account_on_several_servers_is_shown_once_and_named() {
        let reads = vec![
            read("laptop", Agent::Codex, Some("a"), vec![window("Session", 1.0, 60.0, 18000)]),
            read("studio", Agent::Claude, Some("a"), vec![window("Session", 1.0, 60.0, 18000)]),
            read("studio", Agent::Codex, Some("a"), vec![window("Session", 1.0, 60.0, 18000)]),
            read("box", Agent::Codex, Some("b"), vec![window("Session", 1.0, 60.0, 18000)]),
        ];
        let shown: Vec<_> = sections(reads, &[], 1000.0)
            .into_iter()
            .map(|section| (section.agent, section.account, section.servers))
            .collect();
        assert_eq!(
            shown,
            [
                (Agent::Claude, Some("a".to_string()), vec!["studio".to_string()]),
                (Agent::Codex, Some("a".to_string()), vec!["laptop".to_string(), "studio".to_string()]),
                (Agent::Codex, Some("b".to_string()), vec!["box".to_string()]),
            ]
        );
    }

    #[test]
    fn a_section_names_the_account_and_who_is_signed_in_to_it() {
        let mut named = read("studio", Agent::Claude, Some("a@b.c"), vec![]);
        named.limits.account_name = "Work".into();
        let mut router = read("studio", Agent::Claude, None, vec![]);
        router.limits.account_name = "Router".into();
        let reads = vec![named, router, read("box", Agent::Claude, Some("c@d.e"), vec![])];
        let shown: Vec<_> =
            sections(reads, &[], 1000.0).into_iter().map(|section| (section.name, section.account)).collect();
        assert_eq!(
            shown,
            [
                (Some("Work".to_string()), Some("a@b.c".to_string())),
                (Some("Router".to_string()), None),
                (None, Some("c@d.e".to_string())),
            ]
        );
    }

    #[test]
    fn a_account_without_windows_says_why() {
        let mut failed = read("studio", Agent::Codex, None, vec![]);
        failed.limits.error = Some("It didn't answer in time.".into());
        let mut no_plan = read("box", Agent::Claude, Some("b"), vec![]);
        no_plan.limits.plan = None;
        let reads = vec![failed, read("studio", Agent::Claude, Some("a"), vec![]), no_plan];
        let shown = sections(reads, &[], 1000.0);
        let notes: Vec<_> = shown.iter().map(|section| section.note.as_deref().unwrap()).collect();
        assert_eq!(
            notes,
            [
                "Couldn't read the limits on studio.",
                "Not signed in to a plan.",
                "Couldn't read the limits on studio. It didn't answer in time.",
            ]
        );
    }

    #[test]
    fn a_account_on_several_servers_shows_the_windows_one_of_them_read() {
        let reads = vec![
            read("laptop", Agent::Claude, Some("a"), vec![]),
            read("studio", Agent::Claude, Some("a"), vec![window("Session", 24.0, 60.0, 18000)]),
        ];
        let [section] = &sections(reads, &[], 1000.0)[..] else { panic!("one section") };
        assert_eq!((section.windows.len(), section.note.as_deref()), (1, None));
        assert_eq!(section.servers, ["studio", "laptop"]);
    }

    fn account(id: &str, agent: Agent) -> AgentAccount {
        AgentAccount {
            id: id.into(),
            agent,
            name: id.into(),
            folder: String::new(),
            shares_sessions: false,
            variables: vec![],
            email: None,
            plan: None,
            proxy: false,
        }
    }

    fn model(id: &str, name: &str, agent: Agent, account: &str) -> ModelInfo {
        ModelInfo {
            id: id.into(),
            name: name.into(),
            agent,
            account: account.into(),
            efforts: vec![],
            default_effort: None,
            contexts: vec![],
        }
    }

    fn used_by(name: &str, agent: Agent, windows: Vec<LimitWindow>) -> AgentLimits {
        AgentLimits { account_name: name.into(), ..read("studio", agent, Some(name), windows).limits }
    }

    #[test]
    fn each_account_has_the_windows_its_account_read_and_none_when_it_couldnt() {
        let accounts =
            [account("yekta", Agent::Claude), account("work", Agent::Claude), account("codex", Agent::Codex)];
        let mut failed = used_by("work", Agent::Claude, vec![window("Session", 10.0, 60.0, 18000)]);
        failed.error = Some("timed out".into());
        let limits = [
            used_by(
                "yekta",
                Agent::Claude,
                vec![window("Session", 80.0, 60.0, 18000), window("Weekly", 20.0, 60.0, 604800)],
            ),
            failed,
        ];
        let windows = by_account(&accounts, &limits, 1000.0);
        let yekta: Vec<_> =
            windows["yekta"].iter().map(|row| (row.label.as_str(), row.used.as_str(), row.tone)).collect();
        assert_eq!(yekta, [("Session", "80%", Tone::Warning), ("Weekly", "20%", Tone::Success)]);
        assert!(!windows.contains_key("work") && !windows.contains_key("codex"));
    }

    const HOUR: f64 = 3600.0;
    const DAY: f64 = 86400.0;

    fn session(used: f64, resets_in: f64) -> LimitWindow {
        window("Session", used, resets_in, 18000)
    }

    fn weekly(used: f64, resets_in: f64) -> LimitWindow {
        window("Weekly", used, resets_in, 604800)
    }

    fn roomier(accounts: &[AgentAccount], models: &[ModelInfo], limits: &[AgentLimits], read_at: f64) -> Moves {
        let working = HashMap::new();
        Accounts { accounts, limits, read_at: Some(read_at), working: &working }.roomier(models, 1000.0)
    }

    type Moves = HashMap<String, HashMap<String, String>>;

    fn claude(ids: &[&str]) -> Vec<AgentAccount> {
        ids.iter().map(|id| account(id, Agent::Claude)).collect()
    }

    fn opus(accounts: &[&str]) -> Vec<ModelInfo> {
        accounts.iter().map(|account| model("opus", "Opus 5.5", Agent::Claude, account)).collect()
    }

    #[test]
    fn a_new_thread_starts_where_spare_quota_lapses_soonest_rather_than_where_most_is_left() {
        let limits = [
            used_by("yekta", Agent::Claude, vec![session(10.0, 3.0 * HOUR), weekly(10.0, 5.0 * DAY)]),
            used_by("work", Agent::Claude, vec![session(40.0, HOUR), weekly(10.0, 5.0 * DAY)]),
        ];
        let moves = roomier(&claude(&["yekta", "work"]), &opus(&["yekta", "work"]), &limits, 1000.0);
        assert_eq!(moves["yekta"]["opus"], "work");
        assert!(!moves.contains_key("work"));
    }

    #[test]
    fn a_nearly_spent_week_rules_out_a_session_that_lapses_sooner() {
        let limits = [
            used_by("yekta", Agent::Claude, vec![session(10.0, 3.0 * HOUR), weekly(10.0, 5.0 * DAY)]),
            used_by("capped", Agent::Claude, vec![session(10.0, 0.5 * HOUR), weekly(97.0, 3.0 * DAY)]),
        ];
        let moves = roomier(&claude(&["yekta", "capped"]), &opus(&["yekta", "capped"]), &limits, 1000.0);
        assert_eq!(moves["capped"]["opus"], "yekta");
        assert!(!moves.contains_key("yekta"));
    }

    #[test]
    fn a_window_of_another_model_doesnt_count_and_one_that_reset_is_whole() {
        let accounts = claude(&["yekta", "work"]);
        let models = [model("opus", "Opus 5.5", Agent::Claude, ""), model("fable", "Fable 5.1", Agent::Claude, "")];
        let mut reset = session(95.0, 0.0);
        reset.resets_at = Some(900.0);
        let limits = [
            used_by("yekta", Agent::Claude, vec![session(20.0, HOUR), weekly(20.0, 5.0 * DAY)]),
            used_by(
                "work",
                Agent::Claude,
                vec![reset, weekly(10.0, 0.25 * HOUR), window("Weekly · Fable", 99.0, 3.0 * DAY, 604800)],
            ),
        ];
        let moves = roomier(&accounts, &models, &limits, 1000.0);
        assert_eq!(moves["yekta"]["opus"], "work");
        assert!(!moves["yekta"].contains_key("fable"));
        assert_eq!(moves["work"]["fable"], "yekta");
        assert!(!moves["work"].contains_key("opus"));
    }

    #[test]
    fn a_new_thread_stays_among_accounts_whose_quota_lapses_together_and_never_moves_to_one_unread() {
        let accounts = [
            account("yekta", Agent::Claude),
            account("close", Agent::Claude),
            account("unknown", Agent::Claude),
            account("other-model", Agent::Claude),
            account("codex", Agent::Codex),
        ];
        let models = [
            model("opus", "Opus 5.5", Agent::Claude, "yekta"),
            model("opus", "Opus 5.5", Agent::Claude, "close"),
            model("opus", "Opus 5.5", Agent::Claude, "unknown"),
            model("sonnet", "Sonnet 5.5", Agent::Claude, "other-model"),
            model("gpt", "GPT", Agent::Codex, "codex"),
        ];
        let mut failed = used_by("unknown", Agent::Claude, vec![session(0.0, 4.9 * HOUR)]);
        failed.error = Some("It didn't answer in time.".into());
        let limits = [
            used_by("yekta", Agent::Claude, vec![session(50.0, HOUR)]),
            used_by("close", Agent::Claude, vec![session(20.0, 1.2 * HOUR)]),
            failed,
            used_by("other-model", Agent::Claude, vec![session(0.0, 4.9 * HOUR)]),
            used_by("codex", Agent::Codex, vec![session(0.0, 4.9 * HOUR)]),
        ];
        assert!(roomier(&accounts, &models, &limits, 1000.0).is_empty());
    }

    #[test]
    fn limits_read_too_long_ago_move_nothing() {
        let limits = [
            used_by("yekta", Agent::Claude, vec![session(10.0, 3.0 * HOUR)]),
            used_by("work", Agent::Claude, vec![session(40.0, HOUR)]),
        ];
        let accounts = claude(&["yekta", "work"]);
        let models = opus(&["yekta", "work"]);
        assert!(!roomier(&accounts, &models, &limits, 1000.0).is_empty());
        assert!(roomier(&accounts, &models, &limits, 1000.0 - HOUR).is_empty());
    }

    #[test]
    fn accounts_are_ordered_as_a_new_thread_would_choose_them_and_unread_ones_last() {
        let mut accounts = claude(&["yekta", "unread", "work", "spare", "spent"]);
        accounts.insert(1, account("codex", Agent::Codex));
        let limits = [
            used_by("yekta", Agent::Claude, vec![session(10.0, 3.0 * HOUR), weekly(10.0, 5.0 * DAY)]),
            used_by("work", Agent::Claude, vec![session(40.0, HOUR)]),
            used_by("spare", Agent::Claude, vec![window("Weekly · Fable", 10.0, DAY, 604800)]),
            used_by("spent", Agent::Claude, vec![session(100.0, 2.0 * HOUR)]),
            used_by("codex", Agent::Codex, vec![session(10.0, HOUR)]),
        ];
        let working = HashMap::new();
        let ordered =
            Accounts { accounts: &accounts, limits: &limits, read_at: Some(1000.0), working: &working }.by_room(1000.0);
        assert_eq!(ordered, ["work", "yekta", "spent", "unread", "spare", "codex"]);
    }

    fn proxy_account(name: &str, agent: Agent, email: &str, windows: Vec<LimitWindow>) -> ProxyAccount {
        ProxyAccount {
            id: name.into(),
            agent,
            name: name.into(),
            email: Some(email.into()),
            plan: Some("Max".into()),
            priority: 0,
            enabled: true,
            status: AccountStatus::Ready,
            windows,
            reset_credits: 0,
            threads: 0,
            next: false,
        }
    }

    #[test]
    fn the_proxys_accounts_are_sections_of_their_own_or_mark_a_servers_that_shares_the_account() {
        let reads = || {
            let mut through_proxy = read("studio", Agent::Claude, Some("main@b.c"), vec![session(50.0, HOUR)]);
            through_proxy.limits.account_name = "Proxy".into();
            through_proxy.proxy = true;
            vec![
                read("studio", Agent::Claude, Some("a@b.c"), vec![session(20.0, HOUR)]),
                read("studio", Agent::Codex, Some("x@b.c"), vec![session(20.0, HOUR)]),
                through_proxy,
            ]
        };
        let accounts = [
            proxy_account("Codex", Agent::Codex, "c@b.c", vec![session(5.0, HOUR)]),
            proxy_account("Shared", Agent::Claude, "a@b.c", vec![session(70.0, HOUR)]),
            proxy_account("Main", Agent::Claude, "main@b.c", vec![]),
            proxy_account("solo@b.c", Agent::Claude, "solo@b.c", vec![]),
        ];
        let shown: Vec<_> = sections(reads(), &accounts, 1000.0)
            .into_iter()
            .map(|section| {
                let used = section.windows.first().map(|row| row.used.clone());
                (section.name, section.account.unwrap(), section.servers.len(), section.proxy, used, section.note)
            })
            .collect();
        let section = |name: Option<&str>, account: &str, servers, proxy, used: Option<&str>, note: Option<&str>| {
            (
                name.map(String::from),
                account.to_string(),
                servers,
                proxy,
                used.map(String::from),
                note.map(String::from),
            )
        };
        assert_eq!(
            shown,
            [
                section(None, "a@b.c", 1, true, Some("70%"), None),
                section(Some("Main"), "main@b.c", 0, true, None, Some("No limits read yet.")),
                section(None, "solo@b.c", 0, true, None, Some("No limits read yet.")),
                section(None, "x@b.c", 1, false, Some("20%"), None),
                section(Some("Codex"), "c@b.c", 0, true, Some("5%"), None),
            ]
        );

        let unknown = sections(reads(), &[], 1000.0);
        let proxy: Vec<_> = unknown.iter().map(|section| (section.name.as_deref(), section.proxy)).collect();
        assert_eq!(proxy, [(None, false), (Some("Proxy"), true), (None, false)]);
    }

    #[test]
    fn durations_say_their_two_largest_units() {
        assert_eq!(duration(-5.0), "now");
        assert_eq!(duration(30.0), "1m");
        assert_eq!(duration(12.0 * 60.0), "12m");
        assert_eq!(duration(2.0 * 3600.0 + 13.0 * 60.0), "2h 13m");
        assert_eq!(duration(3.0 * 86400.0 + 4.0 * 3600.0 + 59.0), "3d 4h");
    }
}
