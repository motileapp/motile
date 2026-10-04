//! The server's GitHub login, through GitHub's `gh` program: whether it is there, the
//! repositories it reaches, and cloning one of them.

use std::collections::HashSet;
use std::time::Duration;

use anyhow::bail;
use motile_protocol::wire::{GitHubState, Message, Repo};
use serde::Deserialize;
use tokio::task::JoinSet;

use crate::agents::environment::Environment;
use crate::git::{command, on_path, run};

const QUICK: Duration = Duration::from_secs(30);
const PER_PAGE: usize = 100;
const PAGES_AT_ONCE: usize = 4;
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

/// Every repository of the login, of its organizations and the ones it was invited to, the
/// last pushed first. A missing login shows in how `gh` refuses.
pub async fn repos(environment: &Environment) -> anyhow::Result<Message> {
    if !on_path("gh", environment) {
        return Ok(Message::Github { state: GitHubState::Missing });
    }
    let mut listing = String::new();
    for first in (1..).step_by(PAGES_AT_ONCE) {
        let pages = match pages(first, environment).await {
            Ok(pages) => pages,
            Err(error) if login_refused(&error.to_string()) => {
                return Ok(Message::Github { state: GitHubState::SignedOut });
            }
            Err(error) => return Err(error),
        };
        let last = pages.iter().any(|page| page.lines().count() < PER_PAGE);
        listing.extend(pages);
        if last {
            break;
        }
    }
    Ok(Message::Repos { repos: listed(&listing)? })
}

/// `PAGES_AT_ONCE` pages from `first` on, asked for together since GitHub takes about a second
/// for each.
async fn pages(first: usize, environment: &Environment) -> anyhow::Result<Vec<String>> {
    let mut asked = JoinSet::new();
    for page in first..first + PAGES_AT_ONCE {
        let path = format!("user/repos?sort=pushed&per_page={PER_PAGE}&page={page}");
        let jq = ".[] | {full_name, description, private}";
        let list = command("gh", home(environment), environment, &["api", &path, "--jq", jq]);
        asked.spawn(async move { (page, run(list, None, QUICK).await) });
    }
    let mut pages = Vec::new();
    while let Some(answer) = asked.join_next().await {
        let (page, listing) = answer?;
        pages.push((page, listing?));
    }
    pages.sort_by_key(|(page, _)| *page);
    Ok(pages.into_iter().map(|(_, listing)| listing).collect())
}

fn login_refused(said: &str) -> bool {
    said.contains("HTTP 401") || said.contains("gh auth login")
}

/// One repository a line. A push while the pages are asked for can list one twice.
fn listed(listing: &str) -> anyhow::Result<Vec<Repo>> {
    #[derive(Deserialize)]
    struct Listed {
        full_name: String,
        description: Option<String>,
        private: bool,
    }
    let mut seen = HashSet::new();
    let mut repos = Vec::new();
    for line in listing.lines().filter(|line| !line.trim().is_empty()) {
        let listed: Listed = serde_json::from_str(line)?;
        if !seen.insert(listed.full_name.clone()) {
            continue;
        }
        repos.push(Repo {
            name: listed.full_name,
            description: listed.description.filter(|description| !description.trim().is_empty()),
            private: listed.private,
        });
    }
    Ok(repos)
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
        let listing = concat!(
            r#"{"full_name":"acme/app","description":"The app","private":true}"#,
            "\n",
            r#"{"full_name":"me/notes","description":"","private":false}"#,
            "\n",
            r#"{"full_name":"acme/app","description":"The app","private":true}"#,
            "\n",
        );
        let repos = listed(listing).unwrap();

        assert_eq!(
            repos,
            [
                Repo { name: "acme/app".into(), description: Some("The app".into()), private: true },
                Repo { name: "me/notes".into(), description: None, private: false },
            ]
        );
    }

    #[test]
    fn only_an_owner_and_a_name_are_cloned() {
        assert_eq!(repo_name("acme/app.js").unwrap(), "app.js");
        for refused in ["app", "acme/", "acme/app/more", "--upload-pack=x/app", "acme/..", "acme/a b"] {
            assert!(repo_name(refused).is_err(), "{refused}");
        }
    }
}
