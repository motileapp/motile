//! What the usage route shows: the tab, the period and the servers picked, which it keeps for
//! next time, and what was last read for them. It shows what the core kept at once, and reads it
//! again.

use motile_core::api::Command;
use motile_core::limits;
use motile_protocol::wire::Agent;
use serde::{Deserialize, Serialize};

use super::Store;
use crate::bridge::CoreError;
use crate::models::Server;
use crate::panel::state::Loaded;

/// What the agents spent, as the core adds it up: `motile_core::usage::View`, read back.
#[derive(Clone, PartialEq, Debug, Deserialize, Default)]
pub struct UsageReport {
    pub cost_usd: f64,
    pub tokens: u64,
    pub unpriced_tokens: u64,
    pub cache_savings_usd: f64,
    pub writing_cost_usd: f64,
    pub writing_tokens: u64,
    pub starts: Vec<f64>,
    pub agents: Vec<UsageSeries>,
    pub kinds: Vec<UsageKind>,
    pub models: Vec<UsageLine>,
    pub projects: Vec<UsageLine>,
    #[serde(default)]
    pub servers: Vec<UsageLine>,
    #[serde(default)]
    pub accounts: Vec<UsageLine>,
}

#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct UsageSeries {
    pub agent: Agent,
    pub cost_usd: f64,
    pub tokens: u64,
    pub cost_points: Vec<f64>,
    pub token_points: Vec<u64>,
}

#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct UsageKind {
    pub name: String,
    pub tokens: u64,
    pub cost_usd: f64,
}

/// A model, a project, a server, an account or a kind of token, and its part of the whole.
#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct UsageLine {
    pub name: String,
    #[serde(default)]
    pub agent: Option<Agent>,
    /// The server it was spent on, when more than one spent.
    #[serde(default)]
    pub server: Option<String>,
    pub tokens: u64,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    pub share: f64,
    /// An account that runs through the gateway, which its symbol after the name says.
    #[serde(default)]
    pub gateway: bool,
}

impl UsageReport {
    /// The kinds of token as lines, so they are listed like the models and the projects.
    pub fn kind_lines(&self) -> Vec<UsageLine> {
        self.kinds
            .iter()
            .map(|kind| {
                let share = if self.cost_usd > 0. {
                    kind.cost_usd / self.cost_usd
                } else {
                    kind.tokens as f64 / self.tokens.max(1) as f64
                };
                UsageLine {
                    name: kind.name.clone(),
                    agent: None,
                    server: None,
                    tokens: kind.tokens,
                    cost_usd: Some(kind.cost_usd),
                    share,
                    gateway: false,
                }
            })
            .collect()
    }
}

/// What the user and the agents did: `motile_core::activity::View`, read back.
#[derive(Clone, PartialEq, Debug, Deserialize, Default)]
pub struct ActivityReport {
    pub empty: bool,
    pub starts: Vec<f64>,
    pub tiles: Vec<ActivityFact>,
    pub sources: Vec<ActivitySource>,
    pub clients: Vec<ActivityClient>,
    pub hours: Vec<ActivityHour>,
    pub busiest: Option<String>,
    pub outcomes: Vec<ActivityLine>,
    pub agents: Vec<ActivityLine>,
    pub models: Vec<ActivityLine>,
    pub projects: Vec<ActivityLine>,
    pub starters: Vec<ActivityLine>,
    pub facts: Vec<ActivityFact>,
    pub features: Vec<FeatureGroup>,
    pub agent_features: Vec<FeatureLine>,
}

/// A headline number: what it counts, how much, and what it says under it.
#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct ActivityFact {
    pub label: String,
    pub value: String,
    pub detail: String,
}

/// Where work came from: a kind of client of the user's, the agents or the servers.
#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct ActivitySource {
    pub id: String,
    pub label: String,
    /// The symbol's name in `Symbol.swift`.
    pub symbol: String,
    /// Which of the six chart colours, from 1.
    pub color: u8,
    pub messages: u32,
    pub turns: u32,
    pub seconds: f64,
    pub message_points: Vec<u32>,
    pub turn_points: Vec<u32>,
    pub second_points: Vec<f64>,
}

#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct ActivityClient {
    pub device: String,
    pub name: String,
    pub client: String,
    pub detail: String,
    pub symbol: String,
    pub color: u8,
    pub messages: u32,
    pub share: f64,
    pub last_seen: Option<f64>,
    pub this_device: bool,
    pub removed: bool,
}

#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct ActivityHour {
    pub weekday: u8,
    pub hour: u8,
    pub messages: u32,
    pub turns: u32,
    /// From 0, nothing, to 4, as busy as the busiest hour.
    pub level: u8,
}

/// An outcome, an agent, a model, a project or a starter of turns and its part of them.
#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct ActivityLine {
    pub name: String,
    pub agent: Option<Agent>,
    pub server: Option<String>,
    pub turns: u32,
    pub detail: String,
    pub share: f64,
}

#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct FeatureGroup {
    pub name: String,
    pub count: u32,
    pub features: Vec<FeatureLine>,
}

#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct FeatureLine {
    pub id: String,
    pub label: String,
    pub count: u32,
    pub share: f64,
    pub detail: String,
}

/// How much of their plans the agents' accounts have used: `motile_core::limits`, read back.
#[derive(Clone, PartialEq, Debug, Deserialize, Default)]
pub struct LimitsReport {
    pub sections: Vec<LimitsSection>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct LimitsSection {
    pub agent: Agent,
    /// The account's name on the server.
    pub name: Option<String>,
    /// Who is signed in.
    pub account: Option<String>,
    pub plan: Option<String>,
    #[serde(default)]
    pub servers: Vec<String>,
    #[serde(default)]
    pub windows: Vec<LimitWindow>,
    /// Why there are no windows.
    pub note: Option<String>,
    /// One of the gateway's accounts.
    #[serde(default)]
    pub gateway: bool,
}

#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct LimitWindow {
    pub label: String,
    /// Between 0 and 100.
    pub used_percent: f64,
    pub used: String,
    pub resets_in: Option<String>,
    pub pace: Option<Pace>,
    pub tone: Tone,
    #[serde(default)]
    pub reset_credits: u32,
}

impl From<&limits::Row> for LimitWindow {
    fn from(row: &limits::Row) -> Self {
        Self {
            label: row.label.clone(),
            used_percent: row.used_percent,
            used: row.used.clone(),
            resets_in: row.resets_in.clone(),
            pace: row.pace.map(|pace| match pace {
                limits::Pace::Limited => Pace::Limited,
                limits::Pace::Ahead => Pace::Ahead,
                limits::Pace::On => Pace::On,
                limits::Pace::Under => Pace::Under,
            }),
            tone: match row.tone {
                limits::Tone::Success => Tone::Success,
                limits::Tone::Pending => Tone::Pending,
                limits::Tone::Warning => Tone::Warning,
            },
            reset_credits: row.reset_credits,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pace {
    Limited,
    Ahead,
    On,
    Under,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Success,
    Pending,
    Warning,
}

/// What the usage shows: what was done, what the agents cost, the tokens they spent, or what is
/// left of their plans.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageTab {
    #[default]
    Activity,
    Cost,
    Tokens,
    Limits,
}

impl UsageTab {
    pub const ALL: [UsageTab; 4] = [UsageTab::Activity, UsageTab::Cost, UsageTab::Tokens, UsageTab::Limits];

    pub fn label(self) -> &'static str {
        match self {
            UsageTab::Activity => "Activity",
            UsageTab::Cost => "Cost",
            UsageTab::Tokens => "Tokens",
            UsageTab::Limits => "Limits",
        }
    }

    /// What the tab asks the servers for; the cost and the tokens are read together.
    fn reads(self) -> u8 {
        match self {
            UsageTab::Activity => 0,
            UsageTab::Cost | UsageTab::Tokens => 1,
            UsageTab::Limits => 2,
        }
    }
}

/// The stretch of time the activity, the cost and the tokens are shown for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsagePeriod {
    Day,
    Week,
    #[default]
    Month,
    Quarter,
}

impl UsagePeriod {
    pub const ALL: [UsagePeriod; 4] = [UsagePeriod::Day, UsagePeriod::Week, UsagePeriod::Month, UsagePeriod::Quarter];

    pub fn label(self) -> &'static str {
        match self {
            UsagePeriod::Day => "24h",
            UsagePeriod::Week => "7d",
            UsagePeriod::Month => "30d",
            UsagePeriod::Quarter => "90d",
        }
    }

    pub fn bucket_secs(self) -> u32 {
        if self == UsagePeriod::Day { 3600 } else { 86400 }
    }

    pub fn buckets(self) -> u32 {
        match self {
            UsagePeriod::Day => 24,
            UsagePeriod::Week => 7,
            UsagePeriod::Month => 30,
            UsagePeriod::Quarter => 90,
        }
    }
}

const TAB_KEY: &str = "usage.tab";
const PERIOD_KEY: &str = "usage.period";
const SERVERS_KEY: &str = "usage.servers";

/// What the usage route has read, and which answer it waits for.
pub struct Usage {
    pub limits: Loaded<LimitsReport>,
    pub spending: Loaded<UsageReport>,
    pub activity: Loaded<ActivityReport>,
    pub refreshing: bool,
    /// Counts what was asked, so that only the last answer is shown.
    asked: u64,
}

impl Default for Usage {
    fn default() -> Self {
        Self {
            limits: Loaded::Loading,
            spending: Loaded::Loading,
            activity: Loaded::Loading,
            refreshing: false,
            asked: 0,
        }
    }
}

/// What was read, or what is shown when reading it failed.
fn loaded<T>(result: Result<T, String>, shown: Loaded<T>) -> Loaded<T> {
    match result {
        Ok(value) => Loaded::Ready(value),
        Err(_) if shown.value().is_some() => shown,
        Err(error) => Loaded::Failed(error),
    }
}

fn read<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| error.to_string())
}

fn flatten<T>(result: Result<Result<T, String>, CoreError>) -> Result<T, String> {
    result.map_err(|error| error.message).and_then(|read| read)
}

impl Store {
    pub fn usage_tab(&self) -> UsageTab {
        self.prefs.get(TAB_KEY).unwrap_or_default()
    }

    pub fn usage_period(&self) -> UsagePeriod {
        self.prefs.get(PERIOD_KEY).unwrap_or_default()
    }

    /// The servers picked. None picked counts all of them, as does every one.
    fn usage_servers(&self) -> Vec<String> {
        self.prefs.get(SERVERS_KEY).unwrap_or_default()
    }

    /// Opens the usage, which reads what it shows as it appears.
    pub fn open_usage(&mut self) {
        if !self.account.signed_in {
            return;
        }
        self.close_settings();
        if self.shows_usage {
            return;
        }
        self.shows_usage = true;
        self.load_usage();
    }

    pub fn set_usage_tab(&mut self, tab: UsageTab) {
        let before = self.usage_tab();
        self.prefs.set(TAB_KEY, tab);
        if before.reads() != tab.reads() {
            self.load_usage();
        }
    }

    pub fn set_usage_period(&mut self, period: UsagePeriod) {
        self.prefs.set(PERIOD_KEY, period);
        self.load_usage();
    }

    /// Counts the server, or stops counting it.
    pub fn toggle_usage_server(&mut self, server: &str) {
        let mut picked: Vec<String> =
            self.usage_servers().into_iter().filter(|id| self.servers.iter().any(|server| &server.id == id)).collect();
        match picked.iter().position(|id| id == server) {
            Some(index) => {
                picked.remove(index);
            }
            None => picked.push(server.to_string()),
        }
        self.prefs.set(SERVERS_KEY, picked);
        self.load_usage();
    }

    pub fn usage_server_picked(&self, server: &str) -> bool {
        self.usage_servers().iter().any(|id| id == server)
    }

    /// The servers counted among the account's, or nothing for all of them.
    fn counted_servers(&self) -> Option<Vec<String>> {
        let picked: Vec<String> = self
            .servers
            .iter()
            .filter(|server| self.usage_server_picked(&server.id))
            .map(|server| server.id.clone())
            .collect();
        (!picked.is_empty() && picked.len() < self.servers.len()).then_some(picked)
    }

    /// "All servers", the one server's name, or how many.
    pub fn usage_servers_label(&self) -> String {
        let Some(picked) = self.counted_servers() else { return "All servers".into() };
        let named: Vec<&Server> = self.servers.iter().filter(|server| picked.contains(&server.id)).collect();
        match named.as_slice() {
            [one] => one.name.clone(),
            _ => format!("{} servers", named.len()),
        }
    }

    /// Asks the servers again for what the tab shows.
    pub fn refresh_usage(&mut self) {
        self.load_usage();
    }

    /// Shows what the core kept for the tab at once, then asks the servers.
    fn load_usage(&mut self) {
        self.follow_gateway_views();
        self.usage.asked += 1;
        let number = self.usage.asked;
        self.usage.refreshing = true;
        let servers = self.counted_servers();
        if self.usage_tab() == UsageTab::Limits {
            self.load_limits(true, servers.clone(), move |store, kept| {
                if store.usage.asked != number {
                    return;
                }
                if let Ok(report) = kept
                    && !report.sections.is_empty()
                {
                    store.usage.limits = Loaded::Ready(report);
                }
                store.load_limits(false, servers, move |store, result| {
                    if store.usage.asked != number {
                        return;
                    }
                    store.usage.refreshing = false;
                    let shown = std::mem::replace(&mut store.usage.limits, Loaded::Loading);
                    store.usage.limits = loaded(result, shown);
                });
            });
            return;
        }
        let period = self.usage_period();
        if self.usage_tab() == UsageTab::Activity {
            self.load_activity(period, true, servers.clone(), move |store, kept| {
                if store.usage.asked != number {
                    return;
                }
                if let Ok(report) = kept {
                    store.usage.activity = Loaded::Ready(report);
                }
                store.load_activity(period, false, servers, move |store, result| {
                    if store.usage.asked != number {
                        return;
                    }
                    store.usage.refreshing = false;
                    let shown = std::mem::replace(&mut store.usage.activity, Loaded::Loading);
                    store.usage.activity = loaded(result, shown);
                });
            });
            return;
        }
        self.load_spending(period, true, servers.clone(), move |store, kept| {
            if store.usage.asked != number {
                return;
            }
            if let Ok(report) = kept {
                store.usage.spending = Loaded::Ready(report);
            }
            store.load_spending(period, false, servers, move |store, result| {
                if store.usage.asked != number {
                    return;
                }
                store.usage.refreshing = false;
                let shown = std::mem::replace(&mut store.usage.spending, Loaded::Loading);
                store.usage.spending = loaded(result, shown);
            });
        });
    }

    /// Asks the servers what their agents spent in the period, on this device's clock. `kept`
    /// answers at once with what the core kept.
    fn load_spending(
        &mut self,
        period: UsagePeriod,
        kept: bool,
        servers: Option<Vec<String>>,
        then: impl FnOnce(&mut Store, Result<UsageReport, String>) + 'static,
    ) {
        let command = Command::Usage {
            bucket_secs: period.bucket_secs(),
            buckets: period.buckets(),
            utc_offset_secs: chrono::Local::now().offset().local_minus_utc(),
            servers,
            kept,
        };
        self.ask_read(command, read::<UsageReport>, move |store, result, _| then(store, flatten(result)));
    }

    /// Asks the servers what was done on them in the period, on this device's clock. `kept`
    /// answers at once with what the core kept.
    fn load_activity(
        &mut self,
        period: UsagePeriod,
        kept: bool,
        servers: Option<Vec<String>>,
        then: impl FnOnce(&mut Store, Result<ActivityReport, String>) + 'static,
    ) {
        let command = Command::Activity {
            bucket_secs: period.bucket_secs(),
            buckets: period.buckets(),
            utc_offset_secs: chrono::Local::now().offset().local_minus_utc(),
            servers,
            kept,
        };
        self.ask_read(command, read::<ActivityReport>, move |store, result, _| then(store, flatten(result)));
    }

    /// How much of their plans the agents' accounts have used, as the servers read it now. `kept`
    /// answers at once with what was last read.
    fn load_limits(
        &mut self,
        kept: bool,
        servers: Option<Vec<String>>,
        then: impl FnOnce(&mut Store, Result<LimitsReport, String>) + 'static,
    ) {
        let command = Command::Limits { kept, servers };
        self.ask_read(command, read::<LimitsReport>, move |store, result, _| then(store, flatten(result)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_keeps_what_is_shown() {
        assert_eq!(loaded(Err::<u8, _>("gone".into()), Loaded::Ready(3)).value(), Some(&3));
        assert!(
            matches!(loaded(Err::<u8, _>("gone".into()), Loaded::Loading), Loaded::Failed(error) if error == "gone")
        );
        assert_eq!(loaded(Ok(4), Loaded::Ready(3)).value(), Some(&4));
    }

    #[test]
    fn activity_is_read_from_the_core() {
        use motile_core::activity::{ClientRow, HourCell, Line, View};
        let view = View {
            starts: vec![0., 3600.],
            clients: vec![ClientRow {
                device: "key".into(),
                name: "Studio".into(),
                client: "GPUI app · Linux".into(),
                detail: String::new(),
                symbol: "monitor",
                color: 2,
                messages: 3,
                turns: 1,
                share: 1.,
                last_seen: Some(5.),
                this_device: true,
                removed: false,
            }],
            hours: vec![HourCell { weekday: 1, hour: 15, messages: 3, turns: 1, level: 4 }],
            busiest: Some("Tuesdays".into()),
            outcomes: vec![Line {
                name: "Finished".into(),
                agent: Some(Agent::Codex),
                server: None,
                turns: 1,
                detail: "2m on average".into(),
                share: 1.,
            }],
            ..View::default()
        };
        let report: ActivityReport = read(serde_json::to_value(view).unwrap()).unwrap();
        let client = &report.clients[0];
        assert_eq!(
            (client.symbol.as_str(), client.color, client.last_seen, client.this_device),
            ("monitor", 2, Some(5.), true)
        );
        assert_eq!((report.hours[0].weekday, report.hours[0].level), (1, 4));
        assert_eq!(report.outcomes[0].agent, Some(Agent::Codex));
    }

    #[test]
    fn limits_are_read_from_the_core() {
        let value = serde_json::json!({
            "sections": [{
                "agent": "claude", "name": null, "account": "a@b.c", "plan": "Max", "servers": ["studio"],
                "windows": [{"label": "5 hours", "used_percent": 34.0, "used": "34%", "resets_in": "3h",
                    "pace": "ahead", "tone": "pending", "reset_credits": 2}],
                "note": null
            }],
            "notes": []
        });
        let report: LimitsReport = read(value).unwrap();
        let window = &report.sections[0].windows[0];
        assert_eq!((window.pace, window.tone, window.reset_credits), (Some(Pace::Ahead), Tone::Pending, 2));
    }
}
