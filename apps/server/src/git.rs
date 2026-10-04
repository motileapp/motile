//! A project's repository, read and changed with the `git` program: its branches, what isn't
//! committed or pushed, the commits, pushes and pull requests an app asks for, and the worktrees
//! of the threads that work in one of their own. Pull requests are GitHub's, through its `gh`
//! program.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, bail};
use motile_protocol::wire::{Branch, Change, ChangedFile, GitStatus, PullRequest};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use crate::agents::environment::Environment;

const QUICK: Duration = Duration::from_secs(30);
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);
/// A commit runs the repository's hooks, which may run its tests.
const COMMIT_TIMEOUT: Duration = Duration::from_secs(600);
/// A worktree of a large repository takes a while to check out.
const WORKTREE_TIMEOUT: Duration = Duration::from_secs(300);
/// Staging a folder reads every file in it that git hasn't seen.
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(120);
const REFUSAL_CHARS: usize = 2000;
/// The refs that keep the snapshots of the threads, each named after its thread.
const SNAPSHOTS: &str = "refs/motile/threads";
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
/// A patch beyond this is cut; nobody reads more of it in an app.
const MAX_PATCH_BYTES: usize = 4 * 1024 * 1024;

/// Whether the folder is inside a git repository, read from the folders themselves.
pub fn in_repository(folder: &str) -> bool {
    Path::new(folder).ancestors().any(|folder| folder.join(".git").exists())
}

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
        default_branch: match &default {
            Some(default) => Some(default.clone()),
            None => usual_branch(folder, environment).await,
        },
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
    let mut entries = counts.split('\0');
    while let Some(entry) = entries.next() {
        let mut fields = entry.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (fields.next(), fields.next(), fields.next()) else { continue };
        // A renamed file has no path there: where it was and where it is follow.
        let path = if path.is_empty() { entries.nth(1).unwrap_or_default() } else { path };
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

/// `main` or `master`, when the repository has one.
async fn usual_branch(folder: &str, environment: &Environment) -> Option<String> {
    for usual in ["main", "master"] {
        let name = format!("refs/heads/{usual}");
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

/// What GitHub's `gh` says of a pull request, and the commit at its head: the one with that
/// number, or the one of the branch that is checked out.
async fn viewed_pull_request(
    folder: &str,
    environment: &Environment,
    number: Option<u64>,
) -> Option<(PullRequest, String)> {
    let number = number.map(|number| number.to_string());
    let mut arguments = vec!["pr", "view"];
    arguments.extend(number.as_deref());
    arguments.extend(["--json", "number,title,url,state,isDraft,headRefOid"]);
    let view = command("gh", folder, environment, &arguments);
    let answer: serde_json::Value = serde_json::from_str(&run(view, None, QUICK).await.ok()?).ok()?;
    let state = answer["state"].as_str()?;
    let (merged, closed) = (state == "MERGED", state == "CLOSED");
    if !merged && !closed && state != "OPEN" {
        return None;
    }
    let found = PullRequest {
        number: answer["number"].as_u64()?,
        title: answer["title"].as_str()?.to_string(),
        url: answer["url"].as_str()?.to_string(),
        draft: answer["isDraft"].as_bool().unwrap_or(false),
        merged,
        closed,
    };
    Some((found, answer["headRefOid"].as_str().unwrap_or_default().to_string()))
}

/// The pull request of the branch that is checked out: the open one, or the merged or closed one
/// while the branch has no commit since.
pub async fn pull_request(folder: &str, environment: &Environment) -> Option<PullRequest> {
    let (found, head) = viewed_pull_request(folder, environment, None).await?;
    if !found.is_open() {
        git(folder, environment, &["merge-base", "--is-ancestor", "HEAD", &head]).await.ok()?;
    }
    Some(found)
}

/// The pull request with that number, whatever became of it.
pub async fn pull_request_numbered(folder: &str, environment: &Environment, number: u64) -> Option<PullRequest> {
    Some(viewed_pull_request(folder, environment, Some(number)).await?.0)
}

/// The names and the patch of the changes at `paths` that aren't committed, all of them when it
/// is empty. They are staged in a copy of the index, so the repository's own is left alone.
pub async fn pending_changes(
    folder: &str,
    environment: &Environment,
    paths: &[String],
) -> anyhow::Result<(String, String)> {
    let index = IndexCopy::of(folder, environment).await?;
    let on_copy = |arguments: &[&str]| run(index.git(folder, environment, arguments), None, QUICK);
    on_copy(&add_arguments(paths)).await?;
    let names = on_copy(&["diff", "--cached", "--name-status"]).await?;
    let patch = on_copy(&["diff", "--cached", "--no-ext-diff", "--patch", "--minimal"]).await?;
    Ok((names, patch))
}

/// A copy of the repository's index to stage changes in, so the repository's own is left alone.
/// The file goes when this does.
struct IndexCopy(PathBuf);

impl IndexCopy {
    async fn of(folder: &str, environment: &Environment) -> anyhow::Result<Self> {
        let index = git(folder, environment, &["rev-parse", "--git-path", "index"]).await?;
        let copy = Self(std::env::temp_dir().join(format!("motile-index-{}", uuid::Uuid::new_v4().simple())));
        let _ = std::fs::copy(Path::new(folder).join(index.trim()), &copy.0);
        Ok(copy)
    }

    fn git(&self, folder: &str, environment: &Environment, arguments: &[&str]) -> Command {
        let mut git = command("git", folder, environment, arguments);
        git.env("GIT_INDEX_FILE", &self.0);
        git
    }
}

impl Drop for IndexCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.0.with_extension("lock"));
    }
}

/// The folder as it is now, with the files git doesn't track yet, as a tree in the repository.
async fn tree_of_folder(folder: &str, environment: &Environment) -> anyhow::Result<String> {
    let index = IndexCopy::of(folder, environment).await?;
    run(index.git(folder, environment, &["add", "-A"]), None, SNAPSHOT_TIMEOUT).await?;
    let tree = run(index.git(folder, environment, &["write-tree"]), None, QUICK).await?;
    Ok(tree.trim().to_string())
}

async fn commit_named(folder: &str, environment: &Environment, name: &str) -> Option<String> {
    let commit = format!("{name}^{{commit}}");
    let found = git(folder, environment, &["rev-parse", "--verify", "--quiet", &commit]).await.ok()?;
    Some(found.trim().to_string())
}

/// Keeps the folder as it is now as the next of the snapshots named `name`, each a commit on
/// the one before it, under a ref of their own. Answers with the snapshot and the one before
/// it, which are the same when nothing changed in between.
pub async fn snapshot(folder: &str, environment: &Environment, name: &str) -> anyhow::Result<(String, Option<String>)> {
    let tree = tree_of_folder(folder, environment).await?;
    let reference = format!("{SNAPSHOTS}/{name}");
    let before = commit_named(folder, environment, &reference).await;
    if let Some(before) = &before {
        let kept = git(folder, environment, &["rev-parse", &format!("{before}^{{tree}}")]).await?;
        if kept.trim() == tree {
            return Ok((before.clone(), Some(before.clone())));
        }
    }
    let mut arguments = vec!["-c", "commit.gpgsign=false", "commit-tree", &tree, "-m", "Motile snapshot"];
    if let Some(before) = &before {
        arguments.extend(["-p", before]);
    }
    let mut commit = command("git", folder, environment, &arguments);
    for who in ["AUTHOR", "COMMITTER"] {
        commit.env(format!("GIT_{who}_NAME"), "Motile").env(format!("GIT_{who}_EMAIL"), "snapshots@motile.app");
    }
    let commit = run(commit, None, QUICK).await?.trim().to_string();
    git(folder, environment, &["update-ref", &reference, &commit]).await?;
    Ok((commit, before))
}

/// Lets go of the snapshots named `name`.
pub async fn forget_snapshots(folder: &str, environment: &Environment, name: &str) {
    let _ = git(folder, environment, &["update-ref", "-d", &format!("{SNAPSHOTS}/{name}")]).await;
}

/// The commit a snapshot was taken on, to compare the snapshot with.
pub fn before_snapshot(snapshot: &str) -> String {
    format!("{snapshot}^")
}

/// What isn't committed: from the commit that is checked out to the folder as it is.
pub async fn uncommitted(folder: &str, environment: &Environment) -> anyhow::Result<(String, String)> {
    let head = commit_named(folder, environment, "HEAD").await.unwrap_or_else(|| EMPTY_TREE.to_string());
    Ok((head, tree_of_folder(folder, environment).await?))
}

/// Everything since the branch left the one it started from, committed or not: `base` for a
/// worktree's branch, and the remote's default branch otherwise.
pub async fn since_branching(
    folder: &str,
    environment: &Environment,
    base: Option<&str>,
) -> anyhow::Result<(String, String)> {
    let remote = remote(folder, environment).await;
    let branch = match (base, &remote) {
        (Some(base), _) => Some(base.to_string()),
        (None, Some(remote)) => default_branch(folder, environment, remote).await,
        (None, None) => None,
    };
    let on_remote = remote.zip(branch.clone()).map(|(remote, branch)| format!("refs/remotes/{remote}/{branch}"));
    let local = match &branch {
        Some(branch) => vec![format!("refs/heads/{branch}")],
        None => vec!["refs/heads/main".to_string(), "refs/heads/master".to_string()],
    };
    let mut start = None;
    for candidate in on_remote.into_iter().chain(local) {
        if commit_named(folder, environment, &candidate).await.is_some() {
            start = Some(candidate);
            break;
        }
    }
    let start = start.context("Git doesn't know which branch this one started from.")?;
    let fork = git(folder, environment, &["merge-base", &start, "HEAD"]).await?;
    Ok((fork.trim().to_string(), tree_of_folder(folder, environment).await?))
}

/// The files that differ between two commits or trees, with the lines added and removed.
pub async fn changed_between(
    folder: &str,
    environment: &Environment,
    from: &str,
    to: &str,
) -> anyhow::Result<Vec<ChangedFile>> {
    let names = git(folder, environment, &["diff", "--name-status", "-M", "-z", from, to]).await?;
    let counts = git(folder, environment, &["diff", "--numstat", "-M", "-z", from, to]).await?;
    let mut files = read_names(&names);
    count_lines(&mut files, &counts);
    Ok(files)
}

/// Reads `git diff --name-status -z`.
fn read_names(names: &str) -> Vec<ChangedFile> {
    let mut files = Vec::new();
    let mut entries = names.split('\0');
    while let Some(status) = entries.next() {
        let renamed = status.starts_with('R') || status.starts_with('C');
        let from = if renamed { entries.next().map(str::to_string) } else { None };
        let Some(path) = entries.next() else { break };
        let change = match status.chars().next() {
            Some('A') => Change::Added,
            Some('D') => Change::Deleted,
            Some('R') => Change::Renamed,
            _ => Change::Modified,
        };
        files.push(ChangedFile { path: path.to_string(), from, change, added: 0, removed: 0 });
    }
    files
}

/// The patch between two commits or trees, and whether it was cut for being too long.
pub async fn patch_between(
    folder: &str,
    environment: &Environment,
    from: &str,
    to: &str,
) -> anyhow::Result<(String, bool)> {
    let arguments = [
        "-c",
        "core.quotePath=false",
        "diff",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "-M",
        "--patch",
        from,
        to,
    ];
    let mut diff = command("git", folder, environment, &arguments);
    diff.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = diff.spawn().context("Git isn't installed on your server.")?;
    let mut stdout = child.stdout.take().context("Git's answer can't be read.")?;
    let mut patch = Vec::new();
    let mut start = (&mut stdout).take(MAX_PATCH_BYTES as u64 + 1);
    let Ok(read) = tokio::time::timeout(QUICK, start.read_to_end(&mut patch)).await else {
        bail!("git took too long and was stopped.");
    };
    read?;
    if patch.len() > MAX_PATCH_BYTES {
        let _ = child.start_kill();
        let whole_lines = patch[..MAX_PATCH_BYTES].iter().rposition(|byte| *byte == b'\n').map_or(0, |end| end + 1);
        patch.truncate(whole_lines);
        return Ok((String::from_utf8_lossy(&patch).into_owned(), true));
    }
    drop(stdout);
    let output = child.wait_with_output().await?;
    if !output.status.success() {
        bail!("{}", refusal(&String::from_utf8_lossy(&output.stderr)));
    }
    Ok((String::from_utf8_lossy(&patch).into_owned(), false))
}

/// Which of the paths git ignores.
pub async fn ignored(folder: &str, environment: &Environment, paths: &[String]) -> HashSet<String> {
    if paths.is_empty() {
        return HashSet::new();
    }
    let check = command("git", folder, environment, &["check-ignore", "-z", "--stdin"]);
    // Git fails when it ignores none of them, and when the folder is no repository.
    let found = run(check, Some(&paths.join("\0")), QUICK).await.unwrap_or_default();
    found.split('\0').filter(|path| !path.is_empty()).map(str::to_string).collect()
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

/// Makes the worktree at `path` on `branch`. A branch that isn't there yet is made from `base`,
/// which the remote is asked for first. `true` when the branch was made.
pub async fn add_worktree(
    repository: &str,
    environment: &Environment,
    path: &str,
    branch: &str,
    base: &str,
) -> anyhow::Result<bool> {
    prune_worktrees(repository, environment).await;
    let exists = async |name: &str| {
        git(repository, environment, &["rev-parse", "--verify", "--quiet", &format!("{name}^{{commit}}")]).await.is_ok()
    };
    if exists(&format!("refs/heads/{branch}")).await {
        let add = command("git", repository, environment, &["worktree", "add", "--quiet", path, branch]);
        run(add, None, WORKTREE_TIMEOUT).await?;
        return Ok(false);
    }
    let start = latest(repository, environment, base).await;
    if !exists(&start).await {
        bail!("{base} isn't a branch with a commit to start from.");
    }
    let arguments = ["worktree", "add", "--quiet", "--no-track", "-b", branch, path, &start];
    run(command("git", repository, environment, &arguments), None, WORKTREE_TIMEOUT).await?;
    // Where GitHub's gh opens the branch's pull request into.
    let _ = git(repository, environment, &["config", &format!("branch.{branch}.gh-merge-base"), base]).await;
    Ok(true)
}

/// The branch as the remote has it now, unless the local one has everything the remote has.
async fn latest(repository: &str, environment: &Environment, branch: &str) -> String {
    let Some(remote) = remote(repository, environment).await else { return branch.to_string() };
    let on_remote = format!("refs/remotes/{remote}/{branch}");
    let refspec = format!("+refs/heads/{branch}:{on_remote}");
    let fetch = command("git", repository, environment, &["fetch", "--quiet", "--no-tags", &remote, &refspec]);
    if let Err(error) = run(fetch, None, FETCH_TIMEOUT).await {
        tracing::debug!(repository, "couldn't fetch {branch}: {error:#}");
    }
    let local = format!("refs/heads/{branch}");
    let ahead = git(repository, environment, &["merge-base", "--is-ancestor", &on_remote, &local]).await.is_ok();
    let known = git(repository, environment, &["rev-parse", "--verify", "--quiet", &on_remote]).await.is_ok();
    if ahead || !known { branch.to_string() } else { on_remote }
}

/// Forgets the worktrees whose folders have gone.
pub async fn prune_worktrees(repository: &str, environment: &Environment) {
    let _ = git(repository, environment, &["worktree", "prune"]).await;
}

pub async fn rename_branch(repository: &str, environment: &Environment, from: &str, to: &str) -> anyhow::Result<()> {
    git(repository, environment, &["branch", "-m", "--", from, to]).await.map(|_| ())
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
    fn the_files_between_two_commits_are_read_with_their_counts_and_renames() {
        let mut files = read_names("M\0src/main.rs\0A\0notes with space.md\0D\0gone.rs\0R087\0old.rs\0new.rs\0");
        count_lines(
            &mut files,
            "3\t1\tsrc/main.rs\x002\t0\tnotes with space.md\x000\t9\tgone.rs\x004\t2\t\0old.rs\0new.rs\0",
        );

        let read: Vec<_> =
            files.iter().map(|file| (file.path.as_str(), file.change, file.added, file.removed)).collect();
        assert_eq!(
            read,
            [
                ("src/main.rs", Change::Modified, 3, 1),
                ("notes with space.md", Change::Added, 2, 0),
                ("gone.rs", Change::Deleted, 0, 9),
                ("new.rs", Change::Renamed, 4, 2)
            ]
        );
        assert_eq!(files[3].from.as_deref(), Some("old.rs"));
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
