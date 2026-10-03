//! The server's GitHub login, through GitHub's `gh` program: whether it is there, the
//! repositories it reaches, and cloning one of them.

use std::time::Duration;

use anyhow::bail;
use motile_protocol::wire::{GitHubState, Message, Repo};
use serde::Deserialize;

use crate::agents::environment::Environment;
use crate::git::{command, on_path, run};

const QUICK: Duration = Duration::from_secs(30);
const CLONE_TIMEOUT: Duration = Duration::from_secs(1800);

/// Read from what `gh` has stored, without asking GitHub.
pub async fn state(environment: &Environment) -> GitHubState {
    if !on_path("gh", environment) {
        return GitHubState::Missing;
    }
    match run(command("gh", home(environment), environment, &["auth", "token"]), None, QUICK).await {
        Ok(_) => GitHubState::Ready,
        Err(_) => GitHubState::SignedOut,
    }
}

/// The repositories of the login, of its organizations and the ones it was invited to.
pub async fn repos(environment: &Environment) -> anyhow::Result<Message> {
    let state = state(environment).await;
    if state != GitHubState::Ready {
        return Ok(Message::Github { state });
    }
    let list = command("gh", home(environment), environment, &["api", "user/repos?sort=pushed&per_page=100"]);
    match run(list, None, QUICK).await {
        Ok(listing) => Ok(Message::Repos { repos: listed(&listing)? }),
        Err(error) if login_refused(&error.to_string()) => Ok(Message::Github { state: GitHubState::SignedOut }),
        Err(error) => Err(error),
    }
}

fn login_refused(said: &str) -> bool {
    said.contains("HTTP 401") || said.contains("gh auth login")
}

fn listed(listing: &str) -> anyhow::Result<Vec<Repo>> {
    #[derive(Deserialize)]
    struct Listed {
        full_name: String,
        description: Option<String>,
        private: bool,
    }
    let listed: Vec<Listed> = serde_json::from_str(listing)?;
    let repo = |listed: Listed| Repo {
        name: listed.full_name,
        description: listed.description.filter(|description| !description.trim().is_empty()),
        private: listed.private,
    };
    Ok(listed.into_iter().map(repo).collect())
}

/// The name of the repository in `owner/name`, when that is all `repo` is.
pub fn repo_name(repo: &str) -> anyhow::Result<&str> {
    let part = |part: &str| {
        !part.is_empty()
            && !part.starts_with(['-', '.'])
            && part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    match repo.split_once('/') {
        Some((owner, name)) if part(owner) && part(name) => Ok(name),
        _ => bail!("{repo} isn't a repository on GitHub. Write it as owner/name."),
    }
}

pub async fn clone(repo: &str, into: &str, environment: &Environment) -> anyhow::Result<()> {
    run(command("gh", home(environment), environment, &["repo", "clone", repo, into]), None, CLONE_TIMEOUT).await?;
    Ok(())
}

fn home(environment: &Environment) -> &str {
    environment.variables.get("HOME").map(String::as_str).unwrap_or("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repositories_are_read_from_what_github_lists() {
        let listing = r#"[
            {"full_name": "acme/app", "description": "The app", "private": true, "fork": false},
            {"full_name": "me/notes", "description": "", "private": false}
        ]"#;
        let repos = listed(listing).unwrap();

        assert_eq!(repos[0], Repo { name: "acme/app".into(), description: Some("The app".into()), private: true });
        assert_eq!(repos[1], Repo { name: "me/notes".into(), description: None, private: false });
    }

    #[test]
    fn only_an_owner_and_a_name_are_cloned() {
        assert_eq!(repo_name("acme/app.js").unwrap(), "app.js");
        for refused in ["app", "acme/", "acme/app/more", "--upload-pack=x/app", "acme/..", "acme/a b"] {
            assert!(repo_name(refused).is_err(), "{refused}");
        }
    }
}
