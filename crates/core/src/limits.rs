//! How much of their plans the agents' accounts have used, from every server, for the Limits tab
//! of the usage view: a section for each account, with a row for each of its windows.

use motile_protocol::wire::{Agent, AgentLimits, LimitWindow};
use serde::Serialize;

/// How far spending may stray from the time that has passed and still be on pace, in points.
const ON_PACE: f64 = 5.0;

/// What a server said of its agents' logins.
pub struct Read {
    pub server: String,
    pub limits: AgentLimits,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Section {
    pub agent: Agent,
    /// The account's name on the server, when it has one.
    pub name: Option<String>,
    /// Who is signed in.
    pub account: Option<String>,
    pub plan: Option<String>,
    /// The servers with this login.
    pub servers: Vec<String>,
    pub windows: Vec<Row>,
    /// Why there are no windows.
    pub note: Option<String>,
}

#[derive(Serialize, Debug, PartialEq)]
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
    /// Resets the login may use to start its windows over.
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

/// The sections, Claude Code's first, an account several servers share once.
pub fn sections(reads: Vec<Read>, now: f64) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    let mut reads = reads;
    reads.sort_by_key(|read| (read.limits.agent != Agent::Claude, read.limits.error.is_some()));
    for Read { server, limits } in reads {
        let known = limits.account.as_ref().and_then(|account| {
            sections
                .iter_mut()
                .find(|section| section.agent == limits.agent && section.account.as_ref() == Some(account))
        });
        if let Some(section) = known {
            section.servers.push(server);
            continue;
        }
        sections.push(section(server, limits, now));
    }
    sections
}

fn section(server: String, limits: AgentLimits, now: f64) -> Section {
    let note = match (&limits.error, limits.windows.is_empty()) {
        (Some(error), _) => Some(format!("Couldn't read the limits on {server}. {error}")),
        (None, true) => Some("Signed in with an API key, which has no plan limits.".to_string()),
        (None, false) => None,
    };
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
    }
}

fn row(window: &LimitWindow, reset_credits: u32, now: f64) -> Row {
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
fn duration(seconds: f64) -> String {
    let minutes = (seconds.max(0.0) / 60.0).ceil() as u64;
    let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
    match (days, hours) {
        (0, 0) if minutes == 0 => "now".to_string(),
        (0, 0) => format!("{minutes}m"),
        (0, _) => format!("{hours}h {minutes}m"),
        _ => format!("{days}d {hours}h"),
    }
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
        Read { server: server.into(), limits }
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
        let [section] = &sections(vec![read("studio", Agent::Codex, Some("a"), windows)], 1000.0)[..] else {
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
    fn a_login_on_several_servers_is_shown_once_and_named() {
        let reads = vec![
            read("laptop", Agent::Codex, Some("a"), vec![window("Session", 1.0, 60.0, 18000)]),
            read("studio", Agent::Claude, Some("a"), vec![window("Session", 1.0, 60.0, 18000)]),
            read("studio", Agent::Codex, Some("a"), vec![window("Session", 1.0, 60.0, 18000)]),
            read("box", Agent::Codex, Some("b"), vec![window("Session", 1.0, 60.0, 18000)]),
        ];
        let shown: Vec<_> = sections(reads, 1000.0)
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
            sections(reads, 1000.0).into_iter().map(|section| (section.name, section.account)).collect();
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
    fn a_login_without_windows_says_why() {
        let mut failed = read("studio", Agent::Codex, None, vec![]);
        failed.limits.error = Some("It didn't answer in time.".into());
        let shown = sections(vec![failed, read("studio", Agent::Claude, Some("a"), vec![])], 1000.0);
        let notes: Vec<_> = shown.iter().map(|section| section.note.as_deref().unwrap()).collect();
        assert_eq!(
            notes,
            [
                "Signed in with an API key, which has no plan limits.",
                "Couldn't read the limits on studio. It didn't answer in time.",
            ]
        );
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
