//! A project's repository, read and changed with the `git` program: its branches, what isn't
//! committed or pushed, and the commits, pushes and pull requests an app asks for. Pull requests
//! are GitHub's, through its `gh` program.

use std::collections::HashSet;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, bail};
use motile_protocol::wire::{Branch, Change, ChangedFile, GitStatus, PullRequest};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::agents::environment::Environment;

const QUICK: Duration = Duration::from_secs(30);
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);
/// A commit runs the repository's hooks, which may run its tests.
const COMMIT_TIMEOUT: Duration = Duration::from_secs(600);
const REFUSAL_CHARS: usize = 2000;

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

/// What git says about the folder, the files with changes that aren't committed, and the commit
/// that is checked out. `None` when the folder is no repository.
pub async fn status(folder: &str, environment: &Environment) -> Option<(GitStatus, Vec<ChangedFile>, String)> {
    let listing = git(folder, environment, &["status", "--porcelain=2", "--branch", "-z"]).await.ok()?;
    let mut read = read_status(&listing);
    // A repository without a commit has nothing to compare with.
    let counts = git(folder, environment, &["diff", "HEAD", "--numstat", "--no-renames", "-z"]).await;
    count_lines(&mut read.files, &counts.unwrap_or_default());

    let remote = remote(folder, environment).await;
    let default = match &remote {
        Some(remote) => default_branch(folder, environment, remote).await,
        None => None,
    };
    let mut status = GitStatus {
        default: match &default {
            Some(default) => read.branch.as_deref() == Some(default.as_str()),
            None => matches!(read.branch.as_deref(), Some("main" | "master")),
        },
        remote: remote.is_some(),
        upstream: read.upstream,
        ahead: read.ahead,
        behind: read.behind,
        changed: read.files.len() as u32,
        added: read.files.iter().map(|file| file.added).sum(),
        removed: read.files.iter().map(|file| file.removed).sum(),
        pull_requests: remote.is_some() && on_path("gh", environment),
        branch: read.branch,
        ..GitStatus::default()
    };
    let committed = !read.head.is_empty();
    if committed && remote.is_some() && !status.upstream {
        status.ahead = count(folder, environment, &["HEAD", "--not", "--remotes"]).await;
    }
    if let (true, false, Some(remote), Some(default)) = (committed, status.default, &remote, &default) {
        status.ahead_of_default = count(folder, environment, &[&format!("{remote}/{default}..HEAD")]).await;
    }
    Some((status, read.files, read.head))
}

#[derive(Default)]
struct ReadStatus {
    /// Empty before the first commit.
    head: String,
    branch: Option<String>,
    upstream: bool,
    ahead: u32,
    behind: u32,
    files: Vec<ChangedFile>,
}

/// Reads `git status --porcelain=2 --branch -z`.
fn read_status(listing: &str) -> ReadStatus {
    let mut read = ReadStatus::default();
    let mut entries = listing.split('\0');
    while let Some(entry) = entries.next() {
        let (kind, rest) = entry.split_once(' ').unwrap_or((entry, ""));
        let file = |fields: usize, change: fn(&str) -> Change| {
            let mut parts = rest.splitn(fields, ' ');
            let state = parts.next().unwrap_or_default();
            let path = parts.last().unwrap_or_default().to_string();
            ChangedFile { path, from: None, change: change(state), added: 0, removed: 0 }
        };
        match kind {
            "#" => match rest.split_once(' ') {
                Some(("branch.oid", oid)) if oid != "(initial)" => read.head = oid.to_string(),
                Some(("branch.head", name)) if name != "(detached)" => read.branch = Some(name.to_string()),
                Some(("branch.upstream", _)) => read.upstream = true,
                Some(("branch.ab", counts)) => {
                    let mut counts =
                        counts.split(' ').map(|count| count.get(1..).and_then(|count| count.parse().ok()).unwrap_or(0));
                    read.ahead = counts.next().unwrap_or(0);
                    read.behind = counts.next().unwrap_or(0);
                }
                _ => {}
            },
            "1" => read.files.push(file(8, change_of)),
            "2" => {
                let from = entries.next().map(str::to_string);
                read.files.push(ChangedFile { from, ..file(9, |_| Change::Renamed) });
            }
            "u" => read.files.push(file(10, |_| Change::Modified)),
            "?" => read.files.push(ChangedFile { path: rest.to_string(), ..file(1, |_| Change::Added) }),
            _ => {}
        }
    }
    read
}

/// The change two letters of `git status` stand for: the index's and the folder's.
fn change_of(state: &str) -> Change {
    if state.contains('D') {
        return Change::Deleted;
    }
    if state.starts_with('A') { Change::Added } else { Change::Modified }
}

/// Fills in the lines added and removed, from `git diff --numstat -z`.
fn count_lines(files: &mut [ChangedFile], counts: &str) {
    for entry in counts.split('\0') {
        let mut fields = entry.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (fields.next(), fields.next(), fields.next()) else { continue };
        let Some(file) = files.iter_mut().find(|file| file.path == path) else { continue };
        file.added = added.parse().unwrap_or(0);
        file.removed = removed.parse().unwrap_or(0);
    }
}

async fn count(folder: &str, environment: &Environment, range: &[&str]) -> u32 {
    let arguments: Vec<&str> = ["rev-list", "--count"].into_iter().chain(range.iter().copied()).collect();
    git(folder, environment, &arguments).await.ok().and_then(|count| count.trim().parse().ok()).unwrap_or(0)
}

/// The remote the repository works with: `origin`, or the first one.
async fn remote(folder: &str, environment: &Environment) -> Option<String> {
    let remotes = git(folder, environment, &["remote"]).await.ok()?;
    let first = remotes.lines().next()?;
    Some(remotes.lines().find(|remote| *remote == "origin").unwrap_or(first).to_string())
}

/// The branch the remote starts new work from.
async fn default_branch(folder: &str, environment: &Environment, remote: &str) -> Option<String> {
    let head = format!("refs/remotes/{remote}/HEAD");
    if let Ok(default) = git(folder, environment, &["symbolic-ref", "--quiet", "--short", &head]).await {
        return default.trim().strip_prefix(&format!("{remote}/")).map(str::to_string);
    }
    for usual in ["main", "master"] {
        let name = format!("refs/remotes/{remote}/{usual}");
        if git(folder, environment, &["rev-parse", "--verify", "--quiet", &name]).await.is_ok() {
            return Some(usual.to_string());
        }
    }
    None
}

pub(crate) fn on_path(program: &str, environment: &Environment) -> bool {
    let path = environment.variables.get("PATH").map(String::as_str).unwrap_or_default();
    path.split(':').any(|folder| Path::new(folder).join(program).is_file())
}

/// Asks the remote for what is new. A remote that can't be reached leaves what was known.
pub async fn fetch(folder: &str, environment: &Environment) {
    let fetch = command("git", folder, environment, &["fetch", "--quiet", "--no-tags"]);
    if let Err(error) = run(fetch, None, FETCH_TIMEOUT).await {
        tracing::debug!(folder, "couldn't fetch: {error:#}");
    }
}

/// The open pull request of the branch that is checked out, from GitHub's `gh`.
pub async fn pull_request(folder: &str, environment: &Environment) -> Option<PullRequest> {
    let view = command("gh", folder, environment, &["pr", "view", "--json", "number,title,url,state,isDraft"]);
    let answer: serde_json::Value = serde_json::from_str(&run(view, None, QUICK).await.ok()?).ok()?;
    if answer["state"] != "OPEN" {
        return None;
    }
    Some(PullRequest {
        number: answer["number"].as_u64()?,
        title: answer["title"].as_str()?.to_string(),
        url: answer["url"].as_str()?.to_string(),
        draft: answer["isDraft"].as_bool().unwrap_or(false),
    })
}

/// The names and the patch of the changes at `paths` that aren't committed, all of them when it
/// is empty. They are staged in a copy of the index, so the repository's own is left alone.
pub async fn pending_changes(
    folder: &str,
    environment: &Environment,
    paths: &[String],
) -> anyhow::Result<(String, String)> {
    let index = git(folder, environment, &["rev-parse", "--git-path", "index"]).await?;
    let copy = std::env::temp_dir().join(format!("motile-index-{}", uuid::Uuid::new_v4().simple()));
    let _ = std::fs::copy(Path::new(folder).join(index.trim()), &copy);
    let on_copy = |arguments: &[&str]| {
        let mut git = command("git", folder, environment, arguments);
        git.env("GIT_INDEX_FILE", &copy);
        run(git, None, QUICK)
    };
    let read = async {
        on_copy(&add_arguments(paths)).await?;
        let names = on_copy(&["diff", "--cached", "--name-status"]).await?;
        let patch = on_copy(&["diff", "--cached", "--no-ext-diff", "--patch", "--minimal"]).await?;
        Ok((names, patch))
    };
    let read = read.await;
    let _ = std::fs::remove_file(&copy);
    read
}

fn add_arguments(paths: &[String]) -> Vec<&str> {
    if paths.is_empty() {
        return vec!["add", "-A"];
    }
    ["--literal-pathspecs", "add", "-A", "--"].into_iter().chain(paths.iter().map(String::as_str)).collect()
}

/// The subjects of the latest commits, which show how the repository words them.
pub async fn recent_subjects(folder: &str, environment: &Environment) -> String {
    git(folder, environment, &["log", "-n", "20", "--no-merges", "--pretty=format:%s"]).await.unwrap_or_default()
}

/// What the branch adds to the remote's default branch: its commits, the files they touch and
/// their patch.
pub async fn branch_changes(folder: &str, environment: &Environment) -> anyhow::Result<(String, String, String)> {
    let remote = remote(folder, environment).await.context("This repository has no remote.")?;
    let default = default_branch(folder, environment, &remote).await;
    let default = default.context("Git doesn't know which branch the remote starts new work from.")?;
    let commits = format!("{remote}/{default}..HEAD");
    let changes = format!("{remote}/{default}...HEAD");
    let log = git(folder, environment, &["log", "--no-merges", "--pretty=format:%s%n%b", &commits]).await?;
    let files = git(folder, environment, &["diff", "--stat", &changes]).await?;
    let patch = git(folder, environment, &["diff", "--no-ext-diff", "--patch", "--minimal", &changes]).await?;
    Ok((log, files, patch))
}

/// Commits the changes at `paths`, or all of them when it is empty. What else is staged stays so.
pub async fn commit(folder: &str, environment: &Environment, message: &str, paths: &[String]) -> anyhow::Result<()> {
    if message.trim().is_empty() {
        bail!("A commit needs a message.");
    }
    if paths.is_empty() {
        git(folder, environment, &["add", "-A"]).await?;
        let commit = command("git", folder, environment, &["commit", "--quiet", "-F", "-"]);
        run(commit, Some(message), COMMIT_TIMEOUT).await?;
        return Ok(());
    }
    // A renamed file is committed with the place it left.
    let listing = git(folder, environment, &["status", "--porcelain=2", "-z"]).await?;
    let left =
        read_status(&listing).files.into_iter().filter(|file| paths.contains(&file.path)).filter_map(|file| file.from);
    let paths: Vec<String> = paths.iter().cloned().chain(left).collect();
    git(folder, environment, &add_arguments(&paths)).await?;
    let arguments = ["--literal-pathspecs", "commit", "--quiet", "-F", "-", "--"];
    let arguments: Vec<&str> = arguments.into_iter().chain(paths.iter().map(String::as_str)).collect();
    run(command("git", folder, environment, &arguments), Some(message), COMMIT_TIMEOUT).await?;
    Ok(())
}

pub async fn pull(folder: &str, environment: &Environment) -> anyhow::Result<()> {
    run(command("git", folder, environment, &["pull", "--ff-only", "--quiet"]), None, NETWORK_TIMEOUT).await?;
    Ok(())
}

/// Pushes the branch to the branch it follows. One that follows none, or one of another name,
/// is pushed under its own name and follows that from then on.
pub async fn push(folder: &str, environment: &Environment) -> anyhow::Result<()> {
    let branch = git(folder, environment, &["symbolic-ref", "--quiet", "--short", "HEAD"]).await;
    let branch = branch.ok().context("Check out a branch before pushing.")?;
    let branch = branch.trim();
    let setting = async |name: &str| {
        let value = git(folder, environment, &["config", "--get", name]).await.ok()?;
        Some(value.trim().to_string()).filter(|value| !value.is_empty())
    };
    let target = format!("HEAD:refs/heads/{branch}");
    let followed = setting(&format!("branch.{branch}.remote")).await.filter(|remote| remote != ".");
    let follows_itself = setting(&format!("branch.{branch}.merge")).await == Some(format!("refs/heads/{branch}"));
    let arguments = match followed {
        Some(remote) if follows_itself => vec!["push".to_string(), remote, target],
        _ => {
            let chosen = match setting(&format!("branch.{branch}.pushRemote")).await {
                Some(remote) => Some(remote),
                None => setting("remote.pushDefault").await,
            };
            let remote = match chosen {
                Some(remote) => remote,
                None => remote(folder, environment).await.context("Add a remote before pushing.")?,
            };
            vec!["push".to_string(), "-u".to_string(), remote, target]
        }
    };
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    run(command("git", folder, environment, &arguments), None, NETWORK_TIMEOUT).await?;
    Ok(())
}

/// Opens a pull request for the branch with GitHub's `gh`, into the repository's default branch,
/// and answers with its address.
pub async fn open_pull_request(
    folder: &str,
    environment: &Environment,
    title: &str,
    body: &str,
) -> anyhow::Result<String> {
    let arguments = ["pr", "create", "--title", title, "--body-file", "-"];
    let said = run(command("gh", folder, environment, &arguments), Some(body), NETWORK_TIMEOUT).await?;
    let url = said
        .lines()
        .rev()
        .find(|line| line.starts_with("http"))
        .context("GitHub didn't say where the pull request is.")?;
    Ok(url.trim().to_string())
}

/// The commit that is checked out, as its short name and its subject.
pub async fn head(folder: &str, environment: &Environment) -> anyhow::Result<(String, String)> {
    let line = git(folder, environment, &["log", "-1", "--format=%h%x09%s"]).await?;
    let (name, subject) = line.trim().split_once('\t').unwrap_or((line.trim(), ""));
    Ok((name.to_string(), subject.to_string()))
}

/// The branch the checked-out one follows, like `origin/main`.
pub async fn upstream(folder: &str, environment: &Environment) -> Option<String> {
    let name = git(folder, environment, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"]).await;
    name.ok().map(|name| name.trim().to_string())
}

/// `name`, or `name-2`, `name-3` and so on when a branch of that name is there already.
pub async fn free_branch_name(folder: &str, environment: &Environment, name: &str) -> String {
    let mut candidate = name.to_string();
    for count in 2.. {
        let taken =
            git(folder, environment, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{candidate}")]).await;
        if taken.is_err() {
            break;
        }
        candidate = format!("{name}-{count}");
    }
    candidate
}

/// Starts a repository in the folder.
pub async fn init(folder: &str, environment: &Environment) -> anyhow::Result<()> {
    git(folder, environment, &["init", "--quiet"]).await.map(|_| ())
}

/// Where the folder's repository was cloned from.
pub async fn origin(folder: &str, environment: &Environment) -> Option<String> {
    git(folder, environment, &["remote", "get-url", "origin"]).await.ok().map(|url| url.trim().to_string())
}

async fn git(folder: &str, environment: &Environment, arguments: &[&str]) -> anyhow::Result<String> {
    run(command("git", folder, environment, arguments), None, QUICK).await
}

pub(crate) fn command(program: &str, folder: &str, environment: &Environment, arguments: &[&str]) -> Command {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .current_dir(folder)
        .env_clear()
        .envs(&environment.variables)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1");
    command
}

/// What the program printed. `input` is what it reads.
pub(crate) async fn run(mut command: Command, input: Option<&str>, timeout: Duration) -> anyhow::Result<String> {
    let program = command.as_std().get_program().to_string_lossy().into_owned();
    command
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().with_context(|| match program.as_str() {
        "gh" => "GitHub's gh isn't installed on your server.".to_string(),
        _ => "Git isn't installed on your server.".to_string(),
    })?;
    if let (Some(mut stdin), Some(input)) = (child.stdin.take(), input) {
        stdin.write_all(input.as_bytes()).await?;
    }
    let Ok(output) = tokio::time::timeout(timeout, child.wait_with_output()).await else {
        bail!("{program} took too long and was stopped.");
    };
    let output = output?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!("{}", refusal(if stderr.trim().is_empty() { &stdout } else { &stderr }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The end of what the program said, without the word git starts every complaint with.
fn refusal(said: &str) -> String {
    let said = said.trim();
    let start = said.char_indices().rev().nth(REFUSAL_CHARS - 1).map_or(0, |(start, _)| start);
    let said = &said[start..];
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
    fn status_is_read_with_its_branch_and_files() {
        let listing = "# branch.oid 1234abcd\0# branch.head feature/login\0# branch.upstream origin/feature/login\0\
                       # branch.ab +2 -1\0\
                       1 .M N... 100644 100644 100644 aaa bbb src/main with space.rs\0\
                       1 D. N... 100644 000000 000000 aaa bbb gone.rs\0\
                       2 R. N... 100644 100644 100644 aaa bbb R100 new name.rs\0old name.rs\0\
                       ? notes/\0";

        let mut read = read_status(listing);
        count_lines(&mut read.files, "3\t1\tsrc/main with space.rs\0-\t-\timage.png\0");

        assert_eq!((read.head.as_str(), read.branch.as_deref()), ("1234abcd", Some("feature/login")));
        assert_eq!((read.upstream, read.ahead, read.behind), (true, 2, 1));
        let files: Vec<_> = read.files.iter().map(|file| (file.path.as_str(), file.change, file.added)).collect();
        assert_eq!(
            files,
            [
                ("src/main with space.rs", Change::Modified, 3),
                ("gone.rs", Change::Deleted, 0),
                ("new name.rs", Change::Renamed, 0),
                ("notes/", Change::Added, 0)
            ]
        );
        assert_eq!(read.files[2].from.as_deref(), Some("old name.rs"));
    }

    #[test]
    fn a_new_repository_has_a_branch_and_no_commit() {
        let read = read_status("# branch.oid (initial)\0# branch.head main\0? README\0");

        assert_eq!((read.head.as_str(), read.branch.as_deref(), read.upstream), ("", Some("main"), false));
    }

    #[test]
    fn a_refusal_reads_as_a_sentence() {
        assert_eq!(refusal("fatal: invalid reference: nope\n"), "Invalid reference: nope");
        assert_eq!(refusal(""), "Git failed without saying why.");
    }
}
