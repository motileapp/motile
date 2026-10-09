//! The agents' accounts on this server. An agent's CLI keeps its sign-in in a folder, so each
//! account is a folder: `CLAUDE_CONFIG_DIR` for Claude Code, `CODEX_HOME` for Codex. Every agent
//! has a default account, which uses the CLI's usual folder.

use std::path::{Path, PathBuf};

use anyhow::bail;
use motile_protocol::wire::{Agent, AgentAccount, Variable};

use crate::agents::environment::Environment;

const MAX_NAME_CHARS: usize = 40;
/// What a Codex account that shares sessions keeps to itself: its sign-in, and what the models
/// it may use are.
const PRIVATE: [&str; 2] = ["auth.json", "models_cache.json"];
/// What Codex writes as it runs, which a shared folder would mix up.
const LOCAL: [&str; 3] = ["log", "memories", "tmp"];
/// Made in the shared folder first, so that a new account shares them from the start.
const SHARED: [&str; 9] =
    ["sessions", "archived_sessions", "sqlite", "shell_snapshots", "worktrees", "skills", "plugins", "cache", "logs"];

pub fn default_id(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude",
        Agent::Codex => "codex",
    }
}

pub fn is_default(account: &AgentAccount) -> bool {
    account.id == default_id(account.agent)
}

/// The account the agent has before any was added.
pub fn default_account(agent: Agent) -> AgentAccount {
    AgentAccount {
        id: default_id(agent).to_string(),
        agent,
        name: "Default".to_string(),
        folder: String::new(),
        shares_sessions: false,
        variables: vec![],
        email: None,
        plan: None,
    }
}

/// The accounts kept in the settings, with the default ones first whatever was kept.
pub fn with_defaults(kept: Vec<AgentAccount>) -> Vec<AgentAccount> {
    let defaults = [Agent::Claude, Agent::Codex].map(|agent| {
        let kept = kept.iter().find(|account| account.id == default_id(agent) && account.agent == agent);
        kept.cloned().unwrap_or_else(|| default_account(agent))
    });
    let others = kept.into_iter().filter(|account| account.id != default_id(account.agent));
    defaults.into_iter().chain(others).collect()
}

/// The account as it is to be kept: checked against the others, with a new one given its id.
pub fn checked(mut account: AgentAccount, accounts: &[AgentAccount]) -> anyhow::Result<AgentAccount> {
    account.name = account.name.trim().to_string();
    account.folder = account.folder.trim().trim_end_matches('/').to_string();
    account.variables.retain(|variable| !variable.name.trim().is_empty());
    for variable in &mut account.variables {
        variable.name = variable.name.trim().to_string();
    }
    let existing = accounts.iter().find(|kept| kept.id == account.id && !account.id.is_empty());
    if !account.id.is_empty() && existing.is_none() {
        bail!("That account is no longer on your server.");
    }
    for variable in account.variables.iter_mut().filter(|variable| variable.sensitive && variable.value.is_empty()) {
        let kept = existing.and_then(|existing| existing.variables.iter().find(|kept| kept.name == variable.name));
        let Some(kept) = kept else { bail!("{} needs a value.", variable.name) };
        variable.value = kept.value.clone();
    }
    if existing.is_some_and(|existing| existing.agent != account.agent) {
        bail!("An account stays with its agent. Add a new one for the other agent.");
    }
    if account.name.is_empty() {
        bail!("An account needs a name.");
    }
    if account.name.chars().count() > MAX_NAME_CHARS {
        bail!("An account's name can be at most {MAX_NAME_CHARS} characters.");
    }
    let others: Vec<&AgentAccount> =
        accounts.iter().filter(|other| other.id != account.id && other.agent == account.agent).collect();
    if others.iter().any(|other| other.name.eq_ignore_ascii_case(&account.name)) {
        bail!("Another {} account is called {}.", agent_name(account.agent), account.name);
    }
    if is_default(&account) {
        if !account.folder.is_empty() || account.shares_sessions {
            bail!("The default account keeps the agent's usual folder.");
        }
    } else {
        if account.folder.is_empty() {
            bail!("An account needs a folder for its sign-in.");
        }
        if !account.folder.starts_with('/') && !account.folder.starts_with("~/") {
            bail!("The folder has to be a full path, like ~/.claude-personal.");
        }
        if others.iter().any(|other| other.folder == account.folder) {
            bail!("Another {} account keeps its sign-in in {}.", agent_name(account.agent), account.folder);
        }
    }
    if account.shares_sessions && account.agent != Agent::Codex {
        bail!("Only Codex accounts can share sessions.");
    }
    for variable in &account.variables {
        let mut characters = variable.name.chars();
        let starts = characters.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_');
        if !starts || !characters.all(|character| character.is_ascii_alphanumeric() || character == '_') {
            bail!("{} isn't a variable's name.", variable.name);
        }
        if variable.name == folder_variable(account.agent) {
            bail!("The account's folder sets {}.", variable.name);
        }
    }
    if account.id.is_empty() {
        account.id = uuid::Uuid::new_v4().to_string();
    }
    Ok(account)
}

fn agent_name(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "Claude Code",
        Agent::Codex => "Codex",
    }
}

fn folder_variable(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "CLAUDE_CONFIG_DIR",
        Agent::Codex => "CODEX_HOME",
    }
}

/// The account as a client sees it, without the values of its sensitive variables.
pub fn redacted(mut account: AgentAccount) -> AgentAccount {
    for variable in account.variables.iter_mut().filter(|variable| variable.sensitive) {
        variable.value.clear();
    }
    account
}

/// The environment to run the account's agent in.
pub fn environment(base: &Environment, account: &AgentAccount) -> Environment {
    let variables = account.variables.iter().map(|Variable { name, value, .. }| (name.clone(), value.clone()));
    let folder = (!account.folder.is_empty()).then(|| {
        (folder_variable(account.agent).to_string(), expanded(&account.folder, base).to_string_lossy().into_owned())
    });
    base.with(variables.chain(folder))
}

/// The folder the account's agent keeps its sessions in. Accounts with the same one can take a
/// thread over from each other and resume its session.
pub fn sessions_folder(account: &AgentAccount, base: &Environment) -> PathBuf {
    match account.folder.is_empty() || account.shares_sessions {
        true => usual_folder(account.agent, base),
        false => expanded(&account.folder, base),
    }
}

/// Where the agent's CLI keeps its things without an account's folder.
fn usual_folder(agent: Agent, base: &Environment) -> PathBuf {
    if let Some(folder) = base.variables.get(folder_variable(agent)).filter(|folder| !folder.is_empty()) {
        return expanded(folder, base);
    }
    let home = Path::new(base.variables.get("HOME").map(String::as_str).unwrap_or_default());
    match agent {
        Agent::Claude => home.join(".claude"),
        Agent::Codex => home.join(".codex"),
    }
}

fn expanded(folder: &str, base: &Environment) -> PathBuf {
    match folder.strip_prefix("~/") {
        Some(rest) => Path::new(base.variables.get("HOME").map(String::as_str).unwrap_or_default()).join(rest),
        None => PathBuf::from(folder),
    }
}

/// Readies the account's folder before its agent runs: a Codex account that shares sessions
/// gets links to everything in the default account's folder but what is its own.
pub fn prepare(account: &AgentAccount, base: &Environment) -> anyhow::Result<()> {
    if !account.shares_sessions {
        return Ok(());
    }
    let shared = usual_folder(account.agent, base);
    let own = expanded(&account.folder, base);
    link_shared(&shared, &own)
}

fn link_shared(shared: &Path, own: &Path) -> anyhow::Result<()> {
    if shared == own {
        bail!("An account that shares sessions needs a folder of its own, not {}.", own.display());
    }
    for name in SHARED {
        std::fs::create_dir_all(shared.join(name))?;
    }
    std::fs::create_dir_all(own)?;
    for name in PRIVATE {
        let path = own.join(name);
        if path.symlink_metadata().is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            bail!("{} has to be the account's own file, not a link.", path.display());
        }
    }
    for entry in std::fs::read_dir(shared)? {
        let name = entry?.file_name();
        let Some(text) = name.to_str() else { continue };
        if PRIVATE.contains(&text) || LOCAL.contains(&text) {
            continue;
        }
        let (target, link) = (shared.join(&name), own.join(&name));
        match link.symlink_metadata() {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                if std::fs::read_link(&link)? == target {
                    continue;
                }
                std::fs::remove_file(&link)?;
            }
            Ok(_) => {
                bail!("{} is the account's own, so it can't be shared. Move it away and try again.", link.display())
            }
            Err(_) => {}
        }
        std::os::unix::fs::symlink(&target, &link)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn account(id: &str, agent: Agent, name: &str, folder: &str) -> AgentAccount {
        AgentAccount {
            id: id.to_string(),
            agent,
            name: name.to_string(),
            folder: folder.to_string(),
            shares_sessions: false,
            variables: vec![],
            email: None,
            plan: None,
        }
    }

    fn base(home: &str) -> Environment {
        let variables =
            HashMap::from([("HOME".to_string(), home.to_string()), ("PATH".to_string(), "/bin".to_string())]);
        Environment::fixed(variables, HashMap::new())
    }

    #[test]
    fn every_agent_keeps_a_default_account_first() {
        let personal = account("claude-personal", Agent::Claude, "Personal", "~/.claude-personal");
        let accounts = with_defaults(vec![personal.clone()]);
        let ids: Vec<&str> = accounts.iter().map(|account| account.id.as_str()).collect();
        assert_eq!(ids, ["claude", "codex", "claude-personal"]);
    }

    #[test]
    fn a_new_account_gets_an_id_of_its_own_and_a_kept_one_keeps_it() {
        let accounts = with_defaults(vec![account("claude-personal", Agent::Claude, "Personal", "~/.a")]);
        let new = checked(account("", Agent::Claude, "  Work Plan ", "~/.claude-work/"), &accounts).unwrap();
        assert_eq!((new.name.as_str(), new.folder.as_str()), ("Work Plan", "~/.claude-work"));
        assert!(uuid::Uuid::parse_str(&new.id).is_ok(), "{}", new.id);
        let again = checked(account("", Agent::Codex, "Personal", "~/.codex-personal"), &accounts).unwrap();
        assert_ne!(again.id, new.id);
        let kept = checked(account("claude-personal", Agent::Claude, "Renamed", "~/.a"), &accounts).unwrap();
        assert_eq!(kept.id, "claude-personal");
    }

    #[test]
    fn an_account_needs_a_full_folder_no_other_account_has() {
        let accounts = with_defaults(vec![account("claude-personal", Agent::Claude, "Personal", "~/.claude-personal")]);
        let refused = |new: AgentAccount| checked(new, &accounts).unwrap_err().to_string();
        assert!(refused(account("", Agent::Claude, "Work", "")).contains("needs a folder"));
        assert!(refused(account("", Agent::Claude, "Work", "claude-work")).contains("full path"));
        assert!(refused(account("", Agent::Claude, "Work", "~/.claude-personal")).contains("keeps its sign-in"));
        assert!(refused(account("", Agent::Claude, "personal", "~/.claude-work")).contains("is called"));
        assert!(refused(account("claude", Agent::Claude, "Default", "~/.elsewhere")).contains("usual folder"));
        let mut sharing = account("", Agent::Claude, "Work", "~/.claude-work");
        sharing.shares_sessions = true;
        assert!(refused(sharing).contains("Only Codex"));
    }

    #[test]
    fn a_sensitive_value_never_reaches_a_client_and_is_kept_when_saved_without_one() {
        let key = |value: &str| Variable { name: "ANTHROPIC_API_KEY".into(), value: value.into(), sensitive: true };
        let mut router = account("claude-router", Agent::Claude, "Router", "~/.claude-router");
        router.variables = vec![key("sk-secret")];
        let accounts = with_defaults(vec![router.clone()]);

        let sent = redacted(router.clone());
        assert_eq!(sent.variables, [key("")]);
        assert_eq!(checked(sent, &accounts).unwrap().variables, [key("sk-secret")]);

        let mut replaced = router.clone();
        replaced.variables = vec![key("sk-new")];
        assert_eq!(checked(replaced, &accounts).unwrap().variables, [key("sk-new")]);

        let mut new = account("", Agent::Claude, "Work", "~/.claude-work");
        new.variables = vec![key("")];
        assert!(checked(new, &accounts).unwrap_err().to_string().contains("needs a value"));
    }

    #[test]
    fn the_environment_points_the_agent_at_the_accounts_folder() {
        let mut personal = account("claude-personal", Agent::Claude, "Personal", "~/.claude-personal");
        personal.variables =
            vec![Variable { name: "ANTHROPIC_BASE_URL".into(), value: "https://router".into(), sensitive: false }];
        let environment = environment(&base("/home/me"), &personal);
        assert_eq!(environment.variables["CLAUDE_CONFIG_DIR"], "/home/me/.claude-personal");
        assert_eq!(environment.variables["ANTHROPIC_BASE_URL"], "https://router");
        assert_eq!(environment.variables["HOME"], "/home/me");
        let default = with_defaults(vec![]).remove(0);
        assert!(!super::environment(&base("/home/me"), &default).variables.contains_key("CLAUDE_CONFIG_DIR"));
    }

    #[test]
    fn a_codex_account_that_shares_sessions_resumes_the_default_accounts_threads() {
        let base = base("/home/me");
        let mut work = account("codex-work", Agent::Codex, "Work", "~/.codex-work");
        let default = with_defaults(vec![]).remove(1);
        assert_ne!(sessions_folder(&work, &base), sessions_folder(&default, &base));
        work.shares_sessions = true;
        assert_eq!(sessions_folder(&work, &base), PathBuf::from("/home/me/.codex"));
        assert_eq!(sessions_folder(&work, &base), sessions_folder(&default, &base));
    }

    #[test]
    fn a_shared_folder_is_linked_but_the_sign_in_and_logs_stay_the_accounts_own() {
        let root = tempfile::tempdir().unwrap();
        let (shared, own) = (root.path().join("codex"), root.path().join("codex-work"));
        std::fs::create_dir_all(shared.join("log")).unwrap();
        for file in ["config.toml", "auth.json", "models_cache.json"] {
            std::fs::write(shared.join(file), "").unwrap();
        }
        link_shared(&shared, &own).unwrap();
        link_shared(&shared, &own).unwrap();
        let linked =
            |name: &str| own.join(name).symlink_metadata().is_ok_and(|metadata| metadata.file_type().is_symlink());
        assert!(linked("config.toml") && linked("sessions") && linked("skills"));
        assert!(
            !own.join("auth.json").exists() && !own.join("models_cache.json").exists() && !own.join("log").exists()
        );
        assert_eq!(std::fs::read_link(own.join("sessions")).unwrap(), shared.join("sessions"));
    }

    #[test]
    fn a_folder_of_the_accounts_own_is_not_replaced_by_a_link() {
        let root = tempfile::tempdir().unwrap();
        let (shared, own) = (root.path().join("codex"), root.path().join("codex-work"));
        std::fs::create_dir_all(own.join("sessions")).unwrap();
        let refused = link_shared(&shared, &own).unwrap_err().to_string();
        assert!(refused.contains("Move it away"));
        assert!(link_shared(&shared, &shared).is_err());
    }
}
