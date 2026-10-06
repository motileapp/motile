//! The settings open over the window: which section, the group a search picked, and what the
//! usage page has loaded.

use motile_core::api::Command;
use motile_protocol::wire::{Agent, PullRequestSettings};
use serde::Deserialize;

use super::Store;
use crate::models::Server;

/// A page of the settings, listed in the settings' sidebar.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum SettingsSection {
    #[default]
    General,
    Servers,
    Projects,
    TextGeneration,
    PullRequests,
    Usage,
}

impl SettingsSection {
    pub const ALL: [SettingsSection; 6] = [
        SettingsSection::General,
        SettingsSection::Servers,
        SettingsSection::Projects,
        SettingsSection::TextGeneration,
        SettingsSection::PullRequests,
        SettingsSection::Usage,
    ];

    pub fn title(self) -> &'static str {
        match self {
            SettingsSection::General => "General",
            SettingsSection::Servers => "Servers",
            SettingsSection::Projects => "Projects",
            SettingsSection::TextGeneration => "Text generation",
            SettingsSection::PullRequests => "Pull requests",
            SettingsSection::Usage => "Usage",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            SettingsSection::General => "sliders-horizontal",
            SettingsSection::Servers => "server",
            SettingsSection::Projects => "folder",
            SettingsSection::TextGeneration => "pencil-line",
            SettingsSection::PullRequests => "git-pull-request",
            SettingsSection::Usage => "chart-column",
        }
    }
}

/// One group of settings, as the settings' search finds it. Its `id` is the group's on its page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SettingsEntry {
    pub id: &'static str,
    pub title: &'static str,
    pub section: SettingsSection,
    /// Words the group is also found by, beyond its title and its section's.
    pub keywords: &'static str,
}

impl SettingsEntry {
    const fn new(id: &'static str, title: &'static str, section: SettingsSection, keywords: &'static str) -> Self {
        Self { id, title, section, keywords }
    }

    pub const ALL: [SettingsEntry; 12] = [
        Self::new("account", "Account", SettingsSection::General, "signed in email sign out"),
        Self::new("updates", "Updates", SettingsSection::General, "version check for updates release"),
        Self::new("appearance", "Theme", SettingsSection::General, "appearance light dark system mode"),
        Self::new("messages", "Sent while the agent works", SettingsSection::General, "messages queue steer interrupt"),
        Self::new("storage", "Images and videos", SettingsSection::General, "storage cache clear media disk"),
        Self::new("servers", "Servers", SettingsSection::Servers, "add remove machine agents connected"),
        Self::new("projects", "Projects", SettingsSection::Projects, "add remove folder icon setup worktree script"),
        Self::new(
            "text-model",
            "Model",
            SettingsSection::TextGeneration,
            "titles branch names commit messages pull requests writer",
        ),
        Self::new("branch-names", "Branch names", SettingsSection::TextGeneration, "instructions prefix naming"),
        Self::new("merged", "Mark the thread done", SettingsSection::PullRequests, "merge close finished"),
        Self::new(
            "worktrees",
            "Remove the thread's worktree",
            SettingsSection::PullRequests,
            "merge pushed branch clean up",
        ),
        Self::new("usage", "Tokens and cost", SettingsSection::Usage, "spent price models chart"),
    ];

    /// The groups every word of the query is found in, by title, section or keywords.
    pub fn matching(query: &str) -> Vec<SettingsEntry> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return Vec::new();
        }
        Self::ALL
            .into_iter()
            .filter(|entry| {
                let text = format!("{} {} {}", entry.title, entry.section.title(), entry.keywords).to_lowercase();
                words.iter().all(|word| text.contains(word))
            })
            .collect()
    }
}

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

/// A model, a project or a kind of token, and its part of the whole.
#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct UsageLine {
    pub name: String,
    #[serde(default)]
    pub agent: Option<Agent>,
    pub tokens: u64,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    pub share: f64,
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
                    tokens: kind.tokens,
                    cost_usd: Some(kind.cost_usd),
                    share,
                }
            })
            .collect()
    }
}

/// The stretch of time the usage page shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsagePeriod {
    Day,
    #[default]
    Week,
    Month,
    Quarter,
}

impl UsagePeriod {
    pub const ALL: [UsagePeriod; 4] = [UsagePeriod::Day, UsagePeriod::Week, UsagePeriod::Month, UsagePeriod::Quarter];

    pub fn label(self) -> &'static str {
        match self {
            UsagePeriod::Day => "24 hours",
            UsagePeriod::Week => "7 days",
            UsagePeriod::Month => "30 days",
            UsagePeriod::Quarter => "90 days",
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

/// What the usage page has: nothing yet, what went wrong, or the report of the period asked for.
#[derive(Clone, PartialEq, Debug, Default)]
pub enum UsageState {
    #[default]
    Loading,
    Failed(String),
    Ready(UsageReport),
}

#[derive(Default)]
pub struct SettingsState {
    /// The section open over the window, while the settings are.
    pub section: Option<SettingsSection>,
    /// The group of settings a search picked, until its page has scrolled to it.
    pub target: Option<String>,
    pub usage: UsageState,
    /// The period the usage was last asked for; an older answer is dropped.
    pub usage_period: Option<UsagePeriod>,
}

impl Store {
    pub fn open_settings(&mut self, section: SettingsSection, target: Option<String>) {
        if !self.account.signed_in {
            return;
        }
        self.settings.section = Some(section);
        self.settings.target = target;
    }

    pub fn close_settings(&mut self) {
        self.settings.section = None;
        self.settings.target = None;
    }

    /// What the server does with pull requests by itself.
    pub fn set_pull_request_settings(&mut self, settings: PullRequestSettings, server: &Server) {
        let command = Command::SetPullRequestSettings { server_id: server.id.clone(), settings };
        self.ask(command, |store, result, _| {
            if let Err(error) = result {
                store.error_message = Some(error);
            }
        });
    }

    /// Asks every connected server what its agents spent in the period, on this device's clock.
    pub fn load_usage(&mut self, period: UsagePeriod) {
        self.settings.usage = UsageState::Loading;
        self.settings.usage_period = Some(period);
        let command = Command::Usage {
            bucket_secs: period.bucket_secs(),
            buckets: period.buckets(),
            utc_offset_secs: chrono::Local::now().offset().local_minus_utc(),
        };
        self.ask_read(
            command,
            |value| serde_json::from_value::<UsageReport>(value).map_err(|error| error.to_string()),
            move |store, result, _| {
                if store.settings.usage_period != Some(period) {
                    return;
                }
                store.settings.usage = match result {
                    Ok(Ok(report)) => UsageState::Ready(report),
                    Ok(Err(error)) | Err(error) => UsageState::Failed(error),
                };
            },
        );
    }
}
