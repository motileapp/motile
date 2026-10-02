//! The branches of a project's repository, read and switched with the `git` program.

use std::collections::HashSet;
use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, bail};
use motile_protocol::wire::Branch;
use tokio::process::Command;

use crate::agents::environment::Environment;

/// The branch checked out in the folder, read from git's own files.
pub fn current_branch(path: &str) -> Option<String> {
    let dot_git = Path::new(path).join(".git");
    // In a worktree `.git` is a file that points at the real folder.
    let git_dir = match std::fs::read_to_string(&dot_git) {
        Ok(pointer) => Path::new(path).join(pointer.trim().strip_prefix("gitdir:")?.trim()),
        Err(_) => dot_git,
    };
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref: refs/heads/") {
        Some(branch) => Some(branch.to_string()),
        None => Some(head.chars().take(7).collect()),
    }
}

/// Local branches first, then the ones only on a remote, each with the latest commit first. The
/// branch that is checked out leads, and the default one follows it.
pub async fn branches(folder: &str, environment: &Environment) -> anyhow::Result<Vec<Branch>> {
    let format = "--format=%(refname)%09%(HEAD)%09%(symref)";
    let refs =
        git(folder, environment, &["for-each-ref", "--sort=-committerdate", format, "refs/heads", "refs/remotes"])
            .await?;
    let mut branches = listed(&refs);
    if !branches.iter().any(|branch| branch.current) {
        // A repository without a commit has a branch that no ref names yet.
        let unborn = git(folder, environment, &["symbolic-ref", "--quiet", "--short", "HEAD"]).await;
        if let Ok(name) = unborn {
            branches.push(Branch { name: name.trim().to_string(), current: true, default: false, remote: false });
        }
    }
    branches.sort_by_key(|branch| (branch.remote, !branch.current, !branch.default));
    Ok(branches)
}

fn listed(refs: &str) -> Vec<Branch> {
    let lines = || refs.lines().map(|line| line.split('\t').collect::<Vec<_>>()).filter(|fields| fields.len() == 3);
    let local = || lines().filter_map(|fields| Some((fields[0].strip_prefix("refs/heads/")?, fields[1] == "*")));
    // `refs/remotes/origin/feature/login` is `feature/login` on `origin`.
    let on_remote = |name: &str| Some(name.strip_prefix("refs/remotes/")?.split_once('/')?.1.to_string());
    let remote_default =
        lines().find(|fields| fields[2].starts_with("refs/remotes/")).and_then(|fields| on_remote(fields[2]));
    let default = remote_default.or_else(|| {
        let usual = ["main", "master"].into_iter().find(|usual| local().any(|(name, _)| name == *usual));
        usual.map(str::to_string)
    });

    let mut known = HashSet::new();
    let mut branches = Vec::new();
    for (name, current) in local() {
        known.insert(name.to_string());
        branches.push(Branch { name: name.to_string(), current, default: false, remote: false });
    }
    for fields in lines().filter(|fields| fields[2].is_empty()) {
        let Some(name) = on_remote(fields[0]) else { continue };
        if known.insert(name.clone()) {
            branches.push(Branch { name, current: false, default: false, remote: true });
        }
    }
    for branch in &mut branches {
        branch.default = default.as_deref() == Some(branch.name.as_str());
    }
    branches
}

/// Checks the branch out. With `create` it is made first, from what is checked out, and changes
/// that aren't committed come along.
pub async fn switch(folder: &str, environment: &Environment, branch: &str, create: bool) -> anyhow::Result<()> {
    if branch.is_empty() || branch.starts_with('-') {
        bail!("{branch} isn't a valid branch name.");
    }
    let arguments: &[&str] = if create { &["switch", "-c", branch] } else { &["switch", branch] };
    git(folder, environment, arguments).await?;
    Ok(())
}

async fn git(folder: &str, environment: &Environment, arguments: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(folder)
        .env_clear()
        .envs(&environment.variables)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .context("Git isn't installed on your server.")?;
    if !output.status.success() {
        bail!("{}", refusal(&String::from_utf8_lossy(&output.stderr)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// What git said, without the word it starts every complaint with.
fn refusal(stderr: &str) -> String {
    let said = stderr.trim();
    let said = said.strip_prefix("fatal: ").or_else(|| said.strip_prefix("error: ")).unwrap_or(said);
    let mut letters = said.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => "Git failed without saying why.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(branches: &[Branch]) -> Vec<&str> {
        branches.iter().map(|branch| branch.name.as_str()).collect()
    }

    #[test]
    fn a_branch_on_the_remote_is_listed_once_and_only_when_it_is_not_local() {
        let refs = "refs/heads/feature/login\t*\t\n\
                    refs/heads/main\t \t\n\
                    refs/remotes/origin/HEAD\t \trefs/remotes/origin/main\n\
                    refs/remotes/origin/main\t \t\n\
                    refs/remotes/origin/fix/typo\t \t\n\
                    refs/remotes/fork/fix/typo\t \t\n";

        let branches = listed(refs);

        assert_eq!(names(&branches), ["feature/login", "main", "fix/typo"]);
        assert_eq!(branches.iter().map(|branch| branch.remote).collect::<Vec<_>>(), [false, false, true]);
        assert_eq!(branches.iter().map(|branch| branch.current).collect::<Vec<_>>(), [true, false, false]);
        assert_eq!(branches.iter().map(|branch| branch.default).collect::<Vec<_>>(), [false, true, false]);
    }

    #[test]
    fn without_a_remote_the_default_is_the_usual_name() {
        let branches = listed("refs/heads/topic\t*\t\nrefs/heads/master\t \t\n");

        assert_eq!(branches.iter().find(|branch| branch.default).map(|branch| branch.name.as_str()), Some("master"));
    }

    #[test]
    fn a_refusal_reads_as_a_sentence() {
        assert_eq!(refusal("fatal: invalid reference: nope\n"), "Invalid reference: nope");
        assert_eq!(refusal(""), "Git failed without saying why.");
    }
}
