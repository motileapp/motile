//! The settings open over the window: which section, the group a search picked, and what the
//! settings change on the servers.

use motile_core::api::Command;
use motile_protocol::wire::{AgentAccount, ContinueSettings, PullRequestSettings, Request};

use super::Store;
use super::shortcuts::Shortcuts;
use crate::models::Server;

/// A page of the settings, listed in the settings' sidebar.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum SettingsSection {
    #[default]
    General,
    Keyboard,
    Servers,
    Agents,
    Projects,
    TextGeneration,
    PullRequests,
    Integrations,
}

impl SettingsSection {
    pub const ALL: [SettingsSection; 8] = [
        SettingsSection::General,
        SettingsSection::Keyboard,
        SettingsSection::Servers,
        SettingsSection::Agents,
        SettingsSection::Projects,
        SettingsSection::TextGeneration,
        SettingsSection::PullRequests,
        SettingsSection::Integrations,
    ];

    pub fn title(self) -> &'static str {
        match self {
            SettingsSection::General => "General",
            SettingsSection::Keyboard => "Keybindings",
            SettingsSection::Servers => "Servers",
            SettingsSection::Agents => "Agents",
            SettingsSection::Projects => "Projects",
            SettingsSection::TextGeneration => "Text Generation",
            SettingsSection::PullRequests => "Pull Requests",
            SettingsSection::Integrations => "Integrations",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            SettingsSection::General => "sliders-horizontal",
            SettingsSection::Keyboard => "keyboard",
            SettingsSection::Servers => "server",
            SettingsSection::Agents => "bot",
            SettingsSection::Projects => "folder",
            SettingsSection::TextGeneration => "pencil-line",
            SettingsSection::PullRequests => "git-pull-request",
            SettingsSection::Integrations => "blocks",
        }
    }
}

/// What automatically pulling does, said under its switch.
pub const PULL_DEFAULT_BRANCH_CAPTION: &str =
    "Keeps each project's folder current while it is on its default branch with no changes or commits of its own.";

/// One group of settings, as the settings' search finds it. Its `id` is the group's on its page.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SettingsEntry {
    pub id: String,
    pub title: String,
    pub section: SettingsSection,
    /// Words the group is also found by, beyond its title and its section's.
    pub keywords: &'static str,
}

const ENTRIES: [(&str, &str, SettingsSection, &str); 18] = [
    ("account", "Account", SettingsSection::General, "signed in email sign out"),
    ("updates", "Updates", SettingsSection::General, "version check for updates release"),
    ("appearance", "Theme", SettingsSection::General, "appearance light dark system mode"),
    ("messages", "Sent while the agent works", SettingsSection::General, "messages queue steer interrupt"),
    ("storage", "Images and videos", SettingsSection::General, "storage cache clear media disk"),
    (
        "shortcuts",
        "Keybindings",
        SettingsSection::Keyboard,
        "keys keyboard shortcuts hotkeys commands record keybindings.json",
    ),
    (
        "servers",
        "Servers",
        SettingsSection::Servers,
        "add remove unlink unlinked delete copies machine agents connected",
    ),
    ("continue-limits", "Continue after usage limits", SettingsSection::Servers, "rate limit reset resume quota wait"),
    (
        "continue-restarts",
        "Continue after restarts",
        SettingsSection::Servers,
        "restart update crash resume interrupted",
    ),
    (
        "agent-accounts",
        "Accounts",
        SettingsSection::Agents,
        "claude codex sign in login folder subscription plan api key router",
    ),
    ("projects", "Projects", SettingsSection::Projects, "add remove folder icon setup worktree script"),
    (
        "text-model",
        "Model",
        SettingsSection::TextGeneration,
        "titles branch names commit messages pull requests writer",
    ),
    ("branch-names", "Branch names", SettingsSection::TextGeneration, "instructions prefix naming"),
    ("merged", "Mark the thread done after merge or close", SettingsSection::PullRequests, "merge close finished"),
    (
        "worktrees",
        "Remove the thread's worktree after merge",
        SettingsSection::PullRequests,
        "merge pushed branch clean up",
    ),
    ("pull", "Automatically pull", SettingsSection::PullRequests, "default branch main merge sync fetch fast-forward"),
    ("integration-linear", "Linear", SettingsSection::Integrations, "issues tracker tab connect"),
    ("integration-ricrac", "Ricrac", SettingsSection::Integrations, "issues tracker tab connect"),
];

impl SettingsEntry {
    /// The groups every word of the query is found in, by title, section or keywords, and the
    /// shortcuts' commands it finds by name.
    pub fn matching(query: &str, shortcuts: &Shortcuts) -> Vec<SettingsEntry> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let groups = ENTRIES.iter().map(|(id, title, section, keywords)| SettingsEntry {
            id: id.to_string(),
            title: title.to_string(),
            section: *section,
            keywords,
        });
        let commands = shortcuts.commands.iter().map(|command| SettingsEntry {
            id: format!("shortcut-{}", command.id),
            title: command.label.clone(),
            section: SettingsSection::Keyboard,
            keywords: "shortcut keys",
        });
        groups
            .chain(commands)
            .filter(|entry| {
                let text = format!("{} {} {}", entry.title, entry.section.title(), entry.keywords).to_lowercase();
                words.iter().all(|word| text.contains(word))
            })
            .collect()
    }
}

#[derive(Default)]
pub struct SettingsState {
    /// The section open over the window, while the settings are.
    pub section: Option<SettingsSection>,
    /// The group of settings a search picked, until its page has scrolled to it.
    pub target: Option<String>,
}

impl Store {
    pub fn open_settings(&mut self, section: SettingsSection, target: Option<String>) {
        if !self.account.signed_in {
            return;
        }
        self.shows_usage = false;
        self.settings.section = Some(section);
        self.settings.target = target;
    }

    pub fn close_settings(&mut self) {
        self.settings.section = None;
        self.settings.target = None;
        self.recording_shortcut = None;
    }

    /// What the server does with pull requests by itself.
    pub fn set_pull_request_settings(&mut self, settings: PullRequestSettings, server: &Server) {
        let command = Command::SetPullRequestSettings { server_id: server.id.clone(), settings };
        self.ask(command, |store, result, _| {
            if let Err(error) = result {
                store.fail(error);
            }
        });
    }

    /// What the server goes on with by itself: threads whose usage limit reset, and threads
    /// whose agents a restart cut off.
    pub fn set_continue_settings(&mut self, settings: ContinueSettings, server: &Server) {
        let command = Command::SetContinueSettings { server_id: server.id.clone(), settings };
        self.ask(command, |store, result, _| {
            if let Err(error) = result {
                store.fail(error);
            }
        });
    }

    /// Adds or changes an account of an agent on the server. `done` hears whether it was kept.
    pub fn save_agent_account(
        &mut self,
        account: AgentAccount,
        server: &Server,
        done: impl FnOnce(bool, &mut gpui_kit::App) + 'static,
    ) {
        self.request_then(&server.id, Request::SaveAgentAccount { account }, move |store, result, cx| {
            let kept = result.is_ok();
            if let Err(error) = result {
                store.fail(error);
            }
            done(kept, cx);
        });
    }

    /// Removes an account. Its threads go on with the agent's default account.
    pub fn remove_agent_account(&mut self, account: &AgentAccount, server: &Server) {
        self.request(&server.id, Request::RemoveAgentAccount { id: account.id.clone() });
    }

    /// Has the server start signing a Claude Code account in. `done` hears the sign-in's id and
    /// the page to sign in on, or why it couldn't start.
    pub fn start_agent_sign_in(
        &mut self,
        account: AgentAccount,
        server: &Server,
        done: impl FnOnce(Result<(String, String), String>, &mut gpui_kit::App) + 'static,
    ) {
        self.request_then(&server.id, Request::StartSignIn { account }, move |_, result, cx| {
            let started = result.map_err(|error| error.message).and_then(|answer| {
                let url = answer["url"].as_str().filter(|url| !url.is_empty());
                let Some(url) = url else { return Err("Claude Code didn't give a page to sign in on.".to_string()) };
                Ok((answer["id"].as_str().unwrap_or_default().to_string(), url.to_string()))
            });
            done(started, cx);
        });
    }

    /// Hands the server the code the sign-in page showed, which saves `account` with it. `done`
    /// hears the saved account's id and who signed in, or why not.
    pub fn finish_agent_sign_in(
        &mut self,
        server_id: &str,
        id: String,
        code: String,
        account: AgentAccount,
        done: impl FnOnce(Result<(String, String), String>, &mut gpui_kit::App) + 'static,
    ) {
        self.request_then(server_id, Request::FinishSignIn { id, code, account }, move |_, result, cx| {
            let signed_in = result.map_err(|error| error.message).map(|answer| {
                let saved = answer["account"].as_str().unwrap_or_default().to_string();
                let who = match (answer["email"].as_str(), answer["plan"].as_str()) {
                    (Some(email), Some(plan)) => format!("{email} · {plan}"),
                    (Some(email), None) => email.to_string(),
                    (None, _) => "Claude Code".to_string(),
                };
                (saved, who)
            });
            done(signed_in, cx);
        });
    }

    /// Stops a sign-in that waits for its code.
    pub fn cancel_agent_sign_in(&mut self, server_id: &str, id: String) {
        self.send(Command::Request { server_id: server_id.to_string(), request: Request::CancelSignIn { id } });
    }
}

#[cfg(test)]
mod tests {
    use motile_core::keybindings::CommandInfo;

    use super::*;

    #[test]
    fn a_search_finds_groups_and_commands_by_every_word() {
        let shortcuts = Shortcuts {
            commands: vec![CommandInfo { id: "chat.new".into(), label: "New Thread".into() }],
            ..Default::default()
        };
        let ids = |query: &str| -> Vec<String> {
            SettingsEntry::matching(query, &shortcuts).into_iter().map(|entry| entry.id).collect()
        };
        assert_eq!(ids("THEME"), vec!["appearance"]);
        assert_eq!(ids("new thread"), vec!["shortcut-chat.new"]);
        assert_eq!(ids("continue restart"), vec!["continue-restarts"]);
        assert!(ids("  ").is_empty());
        assert!(ids("usage tokens cost").is_empty());
    }
}
