//! The tools a program's `Query` and `Mutation` call, run on your server for the program's thread:
//! inside the folder the thread works in, with the thread's access. Each answers plain JSON shaped
//! for a program to read, rows of flat objects where it can.

use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, bail};
use chrono::{DateTime, SecondsFormat};
use motile_protocol::wire::{
    Access, LinearIssue, Message, PullRequest, PullRequestDetail, PullRequestState, PullRequestSummary, RicracIssue,
    UsageBucket,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::agents::environment::Environment;
use crate::hub::Hub;
use crate::{files, git, linear, pull_requests, ricrac};

/// The most of a file, a patch or a command's output a program is given.
const MAX_TEXT: usize = 64 * 1024;
const MAX_COMMITS: u32 = 200;
const DEFAULT_COMMITS: u32 = 20;
const MAX_DAYS: u32 = 365;
const DEFAULT_DAYS: u32 = 30;
const RUN_TIMEOUT: Duration = Duration::from_secs(60);
const DAY: u32 = 24 * 3600;

/// A tool as the agent's prompt describes it.
#[derive(serde::Serialize, Clone, Debug)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    /// Only a click runs it, through `Mutation` and `@Run`.
    pub mutation: bool,
    pub input_schema: Value,
    pub output_schema: Value,
}

/// Why a tool gave no result, with OpenUI's code for it.
#[derive(Debug, PartialEq)]
pub struct Failure {
    pub code: &'static str,
    pub message: String,
}

impl Failure {
    fn not_found(tool: &str) -> Self {
        Self { code: "tool-not-found", message: format!("Your server has no tool called {tool}.") }
    }
}

impl From<anyhow::Error> for Failure {
    fn from(error: anyhow::Error) -> Self {
        Self { code: "tool-error", message: motile_protocol::error_text(&error) }
    }
}

/// The folder a thread works in, and how much it may do there.
pub struct Folder<'a> {
    pub cwd: &'a str,
    pub access: Access,
    pub environment: &'a Environment,
}

pub fn definitions() -> Vec<ToolDefinition> {
    let string = json!({ "type": "string" });
    let number = json!({ "type": "number" });
    let integer = json!({ "type": "integer" });
    let boolean = json!({ "type": "boolean" });
    let nullable = |kind: &str| json!({ "type": [kind, "null"] });
    let object = |properties: Value| json!({ "type": "object", "properties": properties });
    let rows = |properties: Value| json!({ "type": "array", "items": { "type": "object", "properties": properties } });
    let change = json!({ "type": "string", "enum": ["added", "modified", "deleted", "renamed"] });
    let file_change = json!({ "path": string, "change": change, "added": integer, "removed": integer });
    let pull_request = json!({
        "number": integer, "title": string, "url": string, "state": { "type": "string", "enum": ["open", "draft", "merged", "closed"] },
        "author": string, "branch": string, "base": string, "updated_at": string,
        "review": nullable("string"), "checks": nullable("string"), "additions": integer, "deletions": integer,
    });
    let issue = json!({
        "workspace": string, "identifier": string, "title": string, "url": string, "status": string,
        "status_type": { "type": "string", "enum": ["triage", "backlog", "unstarted", "started", "completed", "canceled"] },
        "priority": { "type": "integer", "description": "0 none, 1 urgent, 2 high, 3 medium, 4 low" },
        "assignee": nullable("string"), "labels": { "type": "array", "items": string }, "updated_at": string,
    });
    let query = object(json!({ "query": { "type": "string", "description": "Words the issues have." } }));
    let mut detail = pull_request.clone();
    let more = json!({
        "body": string, "changed_files": integer, "commits": integer, "created_at": string,
        "merged_at": nullable("string"), "labels": { "type": "array", "items": string },
        "checks": rows(json!({ "name": string, "status": string })),
    });
    if let (Some(fields), Value::Object(more)) = (detail.as_object_mut(), more) {
        fields.extend(more);
    }
    vec![
        ToolDefinition {
            name: "list_files",
            description: "What is in a folder of the thread's folder, folders first.",
            mutation: false,
            input_schema: object(
                json!({ "path": { "type": "string", "description": "Inside the thread's folder; empty for the folder itself." } }),
            ),
            output_schema: rows(json!({ "name": string, "path": string, "folder": boolean, "ignored": boolean })),
        },
        ToolDefinition {
            name: "read_file",
            description: "A text file of the thread's folder, its first 64 KB.",
            mutation: false,
            input_schema: json!({ "type": "object", "properties": { "path": string }, "required": ["path"] }),
            output_schema: object(json!({ "path": string, "text": string, "size": integer, "truncated": boolean })),
        },
        ToolDefinition {
            name: "git_status",
            description: "The branch, how far it is from its upstream, and the files with changes that aren't committed.",
            mutation: false,
            input_schema: object(json!({})),
            output_schema: object(json!({
                "branch": nullable("string"), "default_branch": nullable("string"), "ahead": integer, "behind": integer,
                "changed": integer, "added": integer, "removed": integer, "files": rows(file_change.clone()),
            })),
        },
        ToolDefinition {
            name: "git_log",
            description: "The latest commits of the branch, the newest first.",
            mutation: false,
            input_schema: object(
                json!({ "limit": { "type": "integer", "description": "At most 200; 20 without it." } }),
            ),
            output_schema: rows(
                json!({ "hash": string, "short": string, "subject": string, "author": string, "date": string }),
            ),
        },
        ToolDefinition {
            name: "git_diff",
            description: "What isn't committed, of one path or of all: the files and the patch, its first 64 KB.",
            mutation: false,
            input_schema: object(json!({ "path": string })),
            output_schema: object(json!({ "files": rows(file_change), "patch": string, "truncated": boolean })),
        },
        ToolDefinition {
            name: "pull_requests",
            description: "The repository's open pull requests on GitHub, the last updated first.",
            mutation: false,
            input_schema: object(json!({ "state": { "type": "string", "enum": ["open", "closed", "merged", "all"] } })),
            output_schema: rows(pull_request),
        },
        ToolDefinition {
            name: "pull_request",
            description: "One pull request with its text, checks and labels.",
            mutation: false,
            input_schema: json!({ "type": "object", "properties": { "number": integer }, "required": ["number"] }),
            output_schema: object(detail),
        },
        ToolDefinition {
            name: "usage",
            description: "What the agents on your server spent, by day, agent, model and project, the oldest day first.",
            mutation: false,
            input_schema: object(
                json!({ "days": { "type": "integer", "description": "At most 365; 30 without it." } }),
            ),
            output_schema: rows(json!({
                "date": string, "agent": string, "model": string, "project": string, "tokens": integer, "input": integer,
                "output": integer, "cache_read": integer, "cache_write": integer, "cost_usd": number,
            })),
        },
        ToolDefinition {
            name: "linear_issues",
            description: "The open issues of the Linear workspaces your server is connected to, or with query the ones that have the words.",
            mutation: false,
            input_schema: query.clone(),
            output_schema: rows(issue.clone()),
        },
        ToolDefinition {
            name: "ricrac_issues",
            description: "The open issues of the Ricrac workspaces your server is connected to, or with query the ones that have the words.",
            mutation: false,
            input_schema: query,
            output_schema: rows(issue),
        },
        ToolDefinition {
            name: "run",
            description: "Runs a shell command in the thread's folder for up to 60 seconds. The user confirms it first unless the thread has full access.",
            mutation: true,
            input_schema: json!({ "type": "object", "properties": { "command": string }, "required": ["command"] }),
            output_schema: object(
                json!({ "exit_code": nullable("integer"), "stdout": string, "stderr": string, "truncated": boolean }),
            ),
        },
    ]
}

/// Runs the tool for the thread. `confirmed` says the user allowed this very `run`.
pub async fn call(hub: &Hub, thread_id: &str, tool: &str, args: Value, confirmed: bool) -> Result<Value, Failure> {
    let thread = hub.thread(thread_id).await.context("That thread isn't one of your server's own.")?;
    let folder = Folder { cwd: &thread.cwd, access: thread.access, environment: hub.environment() };
    let args = if args.is_null() { json!({}) } else { args };
    match tool {
        "list_files" => Ok(list_files(&folder, &path_of(args)?).await?),
        "read_file" => Ok(read_file(&folder, &arguments::<Path>(args)?.path)?),
        "git_status" => Ok(git_status(&folder).await?),
        "git_log" => Ok(git_log(&folder, arguments::<Limit>(args)?.limit).await?),
        "git_diff" => Ok(git_diff(&folder, &path_of(args)?).await?),
        "pull_requests" => {
            let state = arguments::<PullRequestsArgs>(args)?.state.unwrap_or(PullRequestState::Open);
            let found = pull_requests::list(folder.cwd, folder.environment, state).await?;
            Ok(Value::Array(found.iter().map(summary_row).collect()))
        }
        "pull_request" => {
            let number = arguments::<Number>(args)?.number;
            Ok(detail_row(&pull_requests::detail(folder.cwd, folder.environment, number).await?))
        }
        "usage" => {
            let days = arguments::<Days>(args)?.days.unwrap_or(DEFAULT_DAYS).clamp(1, MAX_DAYS);
            let until = motile_protocol::now();
            let since = (until / f64::from(DAY)).floor() * f64::from(DAY) - f64::from((days - 1) * DAY);
            let Message::Usage { buckets, .. } = hub.usage(since, until, DAY, 0, &[])? else {
                return Err(anyhow::anyhow!("Your server couldn't add up what was spent.").into());
            };
            Ok(usage_rows(&buckets, &hub.project_names().await))
        }
        "linear_issues" => Ok(linear_issues(hub, arguments::<Query>(args)?.query).await?),
        "ricrac_issues" => Ok(ricrac_issues(hub, arguments::<Query>(args)?.query).await?),
        "run" => Ok(run(&folder, &arguments::<Command>(args)?.command, confirmed).await?),
        _ => Err(Failure::not_found(tool)),
    }
}

#[derive(Deserialize)]
struct Path {
    path: String,
}

#[derive(Deserialize)]
struct OptionalPath {
    path: Option<String>,
}

#[derive(Deserialize)]
struct Limit {
    limit: Option<u32>,
}

#[derive(Deserialize)]
struct PullRequestsArgs {
    state: Option<PullRequestState>,
}

#[derive(Deserialize)]
struct Number {
    number: u64,
}

#[derive(Deserialize)]
struct Days {
    days: Option<u32>,
}

#[derive(Deserialize)]
struct Query {
    query: Option<String>,
}

#[derive(Deserialize)]
struct Command {
    command: String,
}

fn arguments<T: DeserializeOwned>(args: Value) -> anyhow::Result<T> {
    serde_json::from_value(args).map_err(|error| anyhow::anyhow!("The tool's arguments are wrong: {error}."))
}

fn path_of(args: Value) -> anyhow::Result<String> {
    Ok(arguments::<OptionalPath>(args)?.path.unwrap_or_default())
}

pub async fn list_files(folder: &Folder<'_>, path: &str) -> anyhow::Result<Value> {
    let Message::Files { entries } = files::list_files(folder.cwd, path, folder.environment).await? else {
        bail!("{path} can't be listed.");
    };
    let rows = entries.iter().map(|entry| {
        let path = std::path::Path::new(path).join(&entry.name);
        json!({ "name": entry.name, "path": path, "folder": entry.folder, "ignored": entry.ignored })
    });
    Ok(Value::Array(rows.collect()))
}

pub fn read_file(folder: &Folder<'_>, path: &str) -> anyhow::Result<Value> {
    let found = files::inside(folder.cwd, path)?;
    if !found.is_file() {
        bail!("{path} isn't a file.");
    }
    let size = found.metadata()?.len();
    let mut start = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(std::fs::File::open(&found)?, MAX_TEXT as u64), &mut start)?;
    if start.contains(&0) {
        bail!("{path} isn't text.");
    }
    let text = String::from_utf8_lossy(&start);
    Ok(json!({ "path": path, "text": text, "size": size, "truncated": size > MAX_TEXT as u64 }))
}

pub async fn git_status(folder: &Folder<'_>) -> anyhow::Result<Value> {
    let (status, files, _) =
        git::status(folder.cwd, folder.environment).await.context("This folder isn't a git repository.")?;
    Ok(json!({
        "branch": status.branch, "default_branch": status.default_branch, "ahead": status.ahead, "behind": status.behind,
        "changed": status.changed, "added": status.added, "removed": status.removed, "files": files,
    }))
}

pub async fn git_log(folder: &Folder<'_>, limit: Option<u32>) -> anyhow::Result<Value> {
    let limit = limit.unwrap_or(DEFAULT_COMMITS).clamp(1, MAX_COMMITS);
    let commits = git::log(folder.cwd, folder.environment, limit).await?;
    let rows = commits.into_iter().map(|commit| {
        json!({ "hash": commit.hash, "short": commit.short, "subject": commit.subject, "author": commit.author, "date": commit.date })
    });
    Ok(Value::Array(rows.collect()))
}

pub async fn git_diff(folder: &Folder<'_>, path: &str) -> anyhow::Result<Value> {
    let paths: Vec<&str> = Some(path).filter(|path| !path.is_empty()).into_iter().collect();
    if let Some(path) = paths.first() {
        files::plain(path)?;
    }
    let (from, to) = git::uncommitted(folder.cwd, folder.environment).await?;
    let changed = git::changed_between(folder.cwd, folder.environment, &from, &to).await?;
    let changed: Vec<_> =
        changed.into_iter().filter(|file| paths.iter().all(|path| covers(path, &file.path))).collect();
    let (mut patch, mut truncated) = git::patch_of(folder.cwd, folder.environment, &from, &to, &paths).await?;
    if patch.len() > MAX_TEXT {
        let end = patch.floor_char_boundary(MAX_TEXT);
        patch.truncate(patch[..end].rfind('\n').map_or(0, |end| end + 1));
        truncated = true;
    }
    Ok(json!({ "files": changed, "patch": patch, "truncated": truncated }))
}

/// Whether `path`, a file or a folder, is or holds `file`.
fn covers(path: &str, file: &str) -> bool {
    let path = path.trim_end_matches('/');
    file == path || file.strip_prefix(path).is_some_and(|rest| rest.starts_with('/'))
}

fn state_of(pull_request: &PullRequest) -> &'static str {
    match pull_request {
        PullRequest { merged: true, .. } => "merged",
        PullRequest { closed: true, .. } => "closed",
        PullRequest { draft: true, .. } => "draft",
        _ => "open",
    }
}

fn date(at: f64) -> String {
    DateTime::from_timestamp(at as i64, 0).map(|at| at.to_rfc3339_opts(SecondsFormat::Secs, true)).unwrap_or_default()
}

pub fn summary_row(found: &PullRequestSummary) -> Value {
    let pull_request = &found.pull_request;
    json!({
        "number": pull_request.number, "title": pull_request.title, "url": pull_request.url, "state": state_of(pull_request),
        "author": found.author, "branch": found.head, "base": found.base, "updated_at": date(found.updated_at),
        "review": found.review, "checks": found.checks, "additions": found.additions, "deletions": found.deletions,
    })
}

pub fn detail_row(found: &PullRequestDetail) -> Value {
    let pull_request = &found.pull_request;
    let checks: Vec<Value> =
        found.checks.iter().map(|check| json!({ "name": check.name, "status": check.status })).collect();
    let labels: Vec<&str> = found.labels.iter().map(|label| label.name.as_str()).collect();
    json!({
        "number": pull_request.number, "title": pull_request.title, "url": pull_request.url, "state": state_of(pull_request),
        "author": found.author, "branch": found.head, "base": found.base, "body": found.body,
        "additions": found.additions, "deletions": found.deletions, "changed_files": found.changed_files,
        "commits": found.commits, "created_at": date(found.created_at), "merged_at": found.merged_at.map(date),
        "review": found.review, "checks": checks, "labels": labels,
    })
}

/// A row for each day, agent, model and project, the oldest day first.
pub fn usage_rows(buckets: &[UsageBucket], projects: &HashMap<String, String>) -> Value {
    let mut rows: Vec<(String, Value)> = Vec::new();
    for bucket in buckets {
        let day = date(bucket.start).get(..10).unwrap_or_default().to_string();
        let project = projects.get(&bucket.project_id).cloned().unwrap_or_default();
        let key = format!("{day}\0{:?}\0{}\0{}", bucket.agent, bucket.model, project);
        let tokens = &bucket.tokens;
        let row = match rows.iter_mut().find(|(found, _)| *found == key) {
            Some((_, row)) => row,
            None => {
                let row = json!({
                    "date": day, "agent": bucket.agent, "model": bucket.model, "project": project, "tokens": 0,
                    "input": 0, "output": 0, "cache_read": 0, "cache_write": 0, "cost_usd": 0.0,
                });
                rows.push((key, row));
                &mut rows.last_mut().expect("just pushed").1
            }
        };
        let total = tokens.input + tokens.output + tokens.cache_read + tokens.cache_write;
        let spent = [
            ("tokens", total),
            ("input", tokens.input),
            ("output", tokens.output),
            ("cache_read", tokens.cache_read),
            ("cache_write", tokens.cache_write),
        ];
        for (name, count) in spent {
            row[name] = json!(row[name].as_u64().unwrap_or(0) + count);
        }
        row["cost_usd"] = json!(row["cost_usd"].as_f64().unwrap_or(0.0) + bucket.cost_usd.unwrap_or(0.0));
    }
    Value::Array(rows.into_iter().map(|(_, row)| row).collect())
}

pub fn linear_row(workspace: &str, issue: &LinearIssue) -> Value {
    let labels: Vec<&str> = issue.labels.iter().map(|label| label.name.as_str()).collect();
    json!({
        "workspace": workspace, "identifier": issue.identifier, "title": issue.title, "url": issue.url,
        "status": issue.state.name, "status_type": issue.state.kind, "priority": issue.priority,
        "assignee": issue.assignee.as_ref().map(|user| &user.name), "labels": labels, "updated_at": date(issue.updated_at),
    })
}

pub fn ricrac_row(workspace: &str, issue: &RicracIssue) -> Value {
    let labels: Vec<&str> = issue.labels.iter().map(|label| label.name.as_str()).collect();
    json!({
        "workspace": workspace, "identifier": issue.identifier, "title": issue.title, "url": issue.url,
        "status": issue.state.name, "status_type": issue.state.kind, "priority": issue.priority,
        "assignee": issue.assignee.as_ref().map(|user| &user.name), "labels": labels, "updated_at": date(issue.updated_at),
    })
}

async fn linear_issues(hub: &Hub, query: Option<String>) -> anyhow::Result<Value> {
    let mut rows = Vec::new();
    for connection in hub.linear.connections(hub.store()) {
        let wanted =
            linear::Wanted { team: None, mine: false, closed: false, states: Vec::new(), search: query.clone() };
        let issues = hub.linear.issues(hub.store(), &connection.id, &wanted).await?;
        rows.extend(issues.iter().map(|issue| linear_row(&connection.workspace, issue)));
    }
    Ok(Value::Array(rows))
}

async fn ricrac_issues(hub: &Hub, query: Option<String>) -> anyhow::Result<Value> {
    let mut rows = Vec::new();
    for connection in hub.ricrac.connections(hub.store()) {
        let wanted =
            ricrac::Wanted { team: None, mine: false, closed: false, states: Vec::new(), search: query.clone() };
        let issues = hub.ricrac.issues(hub.store(), &connection.id, &wanted).await?;
        rows.extend(issues.iter().map(|issue| ricrac_row(&connection.workspace, issue)));
    }
    Ok(Value::Array(rows))
}

/// Runs the command in the thread's folder. Only a thread with full access runs one the user
/// hasn't confirmed.
pub async fn run(folder: &Folder<'_>, command: &str, confirmed: bool) -> anyhow::Result<Value> {
    if command.trim().is_empty() {
        bail!("There is no command to run.");
    }
    if folder.access != Access::Full && !confirmed {
        bail!("This thread runs a command only once you have confirmed it.");
    }
    let mut shell = tokio::process::Command::new("/bin/sh");
    shell
        .args(["-c", command])
        .current_dir(folder.cwd)
        .env_clear()
        .envs(&folder.environment.variables)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .process_group(0);
    let mut child = shell.spawn().context("The command couldn't be started.")?;
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
    let ran = tokio::time::timeout(RUN_TIMEOUT, async {
        let (stdout, stderr, status) = tokio::join!(capped(stdout), capped(stderr), child.wait());
        (stdout, stderr, status)
    })
    .await;
    let Ok(((stdout, stdout_cut), (mut stderr, stderr_cut), status)) = ran else {
        if let Some(id) = child.id() {
            unsafe { libc::kill(-(id as i32), libc::SIGKILL) };
        }
        let _ = child.wait().await;
        bail!("The command took longer than {} seconds and was stopped.", RUN_TIMEOUT.as_secs());
    };
    let status = status?;
    if status.code().is_none() {
        stderr.push_str("The command was stopped by a signal.");
    }
    Ok(json!({ "exit_code": status.code(), "stdout": stdout, "stderr": stderr, "truncated": stdout_cut || stderr_cut }))
}

/// What a stream says, up to `MAX_TEXT` bytes, and whether it said more. The rest is read and
/// dropped so that the program doesn't wait on a full pipe.
async fn capped(stream: Option<impl AsyncRead + Unpin>) -> (String, bool) {
    let Some(mut stream) = stream else { return (String::new(), false) };
    let (mut kept, mut buffer, mut cut) = (Vec::new(), vec![0u8; 8192], false);
    while let Ok(read) = stream.read(&mut buffer).await {
        if read == 0 {
            break;
        }
        let room = MAX_TEXT - kept.len();
        kept.extend_from_slice(&buffer[..read.min(room)]);
        cut |= read > room;
    }
    (String::from_utf8_lossy(&kept).into_owned(), cut)
}

#[cfg(test)]
mod tests {
    use motile_protocol::wire::{Agent, CheckStatus, Label, LinearState, LinearStateKind, LinearUser, Tokens};

    use super::*;

    struct Repository {
        _dir: tempfile::TempDir,
        path: String,
        environment: Environment,
    }

    impl Repository {
        fn folder(&self, access: Access) -> Folder<'_> {
            Folder { cwd: &self.path, access, environment: &self.environment }
        }
    }

    fn git_in(path: &std::path::Path, arguments: &[&str]) {
        let ran = std::process::Command::new("git").args(arguments).current_dir(path).output().unwrap();
        assert!(ran.status.success(), "{}", String::from_utf8_lossy(&ran.stderr));
    }

    fn repository() -> Repository {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("project");
        std::fs::create_dir_all(path.join("src")).unwrap();
        std::fs::write(path.join("src/greet.py"), "print('hi')\n").unwrap();
        std::fs::write(path.join("README.md"), "# Greet\n").unwrap();
        git_in(&path, &["init", "-q", "-b", "main"]);
        for (subject, file) in [("Add greet", "src/greet.py"), ("Add the readme", "README.md")] {
            git_in(&path, &["add", file]);
            git_in(&path, &["-c", "user.name=Ada", "-c", "user.email=ada@motile.app", "commit", "-q", "-m", subject]);
        }
        let variables = std::env::vars().collect();
        let environment = Environment::fixed(variables, HashMap::new());
        Repository { path: path.to_string_lossy().into_owned(), _dir: dir, environment }
    }

    #[tokio::test]
    async fn the_log_lists_commits_newest_first_as_rows() {
        let repository = repository();

        let log = git_log(&repository.folder(Access::Full), Some(5)).await.unwrap();

        let subjects: Vec<&str> = log.as_array().unwrap().iter().map(|row| row["subject"].as_str().unwrap()).collect();
        assert_eq!(subjects, ["Add the readme", "Add greet"]);
        assert_eq!(log[0]["author"], "Ada");
        assert_eq!(log[0]["hash"].as_str().unwrap().len(), 40);
        assert!(log[0]["date"].as_str().unwrap().contains('T'));
        let one = git_log(&repository.folder(Access::Full), Some(1)).await.unwrap();
        assert_eq!(one.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn status_and_diff_tell_what_isnt_committed() {
        let repository = repository();
        std::fs::write(format!("{}/src/greet.py", repository.path), "print('hello')\n").unwrap();
        std::fs::write(format!("{}/notes.txt", repository.path), "new\n").unwrap();
        let folder = repository.folder(Access::Full);

        let status = git_status(&folder).await.unwrap();
        let everything = git_diff(&folder, "").await.unwrap();
        let one = git_diff(&folder, "src").await.unwrap();

        assert_eq!(status["branch"], "main");
        assert_eq!(status["changed"], 2);
        let changed = |diff: &Value| -> Vec<String> {
            diff["files"].as_array().unwrap().iter().map(|file| file["path"].as_str().unwrap().to_string()).collect()
        };
        assert_eq!(changed(&everything), ["notes.txt", "src/greet.py"]);
        assert_eq!(changed(&one), ["src/greet.py"]);
        assert!(one["patch"].as_str().unwrap().contains("+print('hello')"));
        assert!(!one["patch"].as_str().unwrap().contains("notes.txt"));
        assert_eq!(everything["files"][1]["change"], "modified");
    }

    #[tokio::test]
    async fn files_are_read_only_inside_the_folder() {
        let repository = repository();
        let folder = repository.folder(Access::Full);

        let listed = list_files(&folder, "").await.unwrap();
        let read = read_file(&folder, "src/greet.py").unwrap();

        let names: Vec<&str> = listed.as_array().unwrap().iter().map(|row| row["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["src", "README.md"]);
        assert_eq!(listed[0]["folder"], true);
        assert_eq!(read, json!({ "path": "src/greet.py", "text": "print('hi')\n", "size": 12, "truncated": false }));
        for outside in ["../project/../secret", "/etc/hosts", ".git/config"] {
            assert!(read_file(&folder, outside).is_err(), "{outside}");
        }
        assert!(list_files(&folder, "..").await.is_err());
    }

    #[tokio::test]
    async fn a_command_runs_in_the_folder_once_its_access_allows() {
        let repository = repository();

        let ran = run(&repository.folder(Access::Full), "ls src; echo oops >&2; exit 3", false).await.unwrap();
        let refused = run(&repository.folder(Access::Supervised), "touch made", false).await;
        let confirmed = run(&repository.folder(Access::Supervised), "echo yes", true).await.unwrap();

        assert_eq!(ran, json!({ "exit_code": 3, "stdout": "greet.py\n", "stderr": "oops\n", "truncated": false }));
        assert!(refused.unwrap_err().to_string().contains("confirmed"));
        assert!(!std::path::Path::new(&repository.path).join("made").exists());
        assert_eq!(confirmed["stdout"], "yes\n");
        let long = run(&repository.folder(Access::Full), "head -c 100000 /dev/zero | tr '\\0' a", false).await.unwrap();
        assert_eq!((long["stdout"].as_str().unwrap().len(), &long["truncated"]), (MAX_TEXT, &json!(true)));
    }

    #[test]
    fn usage_is_a_row_for_each_day_agent_model_and_project() {
        let bucket = |start: f64, project: &str, input: u64, cost: Option<f64>| UsageBucket {
            start,
            agent: Agent::Claude,
            account_name: "Personal".into(),
            model: "claude-opus-5-5".into(),
            project_id: project.into(),
            tokens: Tokens { input, cache_read: 10, cache_write: 0, output: 5 },
            cost_usd: cost,
            costs: None,
            cache_savings_usd: 0.0,
            writing: false,
        };
        let day = 1_790_000_000.0 - 1_790_000_000.0 % 86_400.0;
        let buckets =
            [bucket(day, "p", 100, Some(1.5)), bucket(day, "p", 50, Some(0.5)), bucket(day + 86_400.0, "q", 1, None)];
        let projects = HashMap::from([("p".to_string(), "api".to_string())]);

        let rows = usage_rows(&buckets, &projects);

        assert_eq!(
            rows,
            json!([
                { "date": "2026-09-21", "agent": "claude", "model": "claude-opus-5-5", "project": "api", "tokens": 180,
                  "input": 150, "output": 10, "cache_read": 20, "cache_write": 0, "cost_usd": 2.0 },
                { "date": "2026-09-22", "agent": "claude", "model": "claude-opus-5-5", "project": "", "tokens": 16,
                  "input": 1, "output": 5, "cache_read": 10, "cache_write": 0, "cost_usd": 0.0 },
            ])
        );
    }

    #[test]
    fn pull_requests_and_issues_are_flat_rows() {
        let summary = PullRequestSummary {
            pull_request: PullRequest {
                number: 7,
                title: "Greet by name".into(),
                url: "https://github.com/a/b/pull/7".into(),
                draft: true,
                merged: false,
                closed: false,
            },
            author: "ada".into(),
            head: "greet".into(),
            base: "main".into(),
            updated_at: 0.0,
            review: None,
            checks: Some(CheckStatus::Failure),
            additions: 3,
            deletions: 1,
        };
        let issue = LinearIssue {
            id: "i".into(),
            identifier: "ENG-7".into(),
            title: "Greet".into(),
            url: "https://linear.app/i".into(),
            priority: 2,
            state: LinearState {
                id: "s".into(),
                name: "Todo".into(),
                kind: LinearStateKind::Unstarted,
                color: "e2e2e2".into(),
                position: 1.0,
            },
            team: "t".into(),
            assignee: Some(LinearUser { id: "u".into(), name: "Ada".into(), me: true }),
            labels: vec![Label { name: "Bug".into(), color: "eb5757".into() }],
            branch_name: String::new(),
            updated_at: 0.0,
        };

        let row = summary_row(&summary);
        let issue = linear_row("Engines", &issue);

        assert_eq!(
            (&row["state"], &row["checks"], &row["branch"]),
            (&json!("draft"), &json!("failure"), &json!("greet"))
        );
        assert_eq!(row["updated_at"], "1970-01-01T00:00:00Z");
        assert_eq!(
            issue,
            json!({ "workspace": "Engines", "identifier": "ENG-7", "title": "Greet", "url": "https://linear.app/i",
                    "status": "Todo", "status_type": "unstarted", "priority": 2, "assignee": "Ada", "labels": ["Bug"],
                    "updated_at": "1970-01-01T00:00:00Z" })
        );
    }

    #[test]
    fn every_tool_has_a_name_and_schemas_and_only_run_changes_anything() {
        let tools = definitions();

        let names: Vec<&str> = tools.iter().map(|tool| tool.name).collect();
        assert_eq!(
            names,
            [
                "list_files",
                "read_file",
                "git_status",
                "git_log",
                "git_diff",
                "pull_requests",
                "pull_request",
                "usage",
                "linear_issues",
                "ricrac_issues",
                "run"
            ]
        );
        assert!(tools.iter().all(|tool| tool.input_schema["type"] == "object" && tool.output_schema.is_object()));
        assert_eq!(tools.iter().filter(|tool| tool.mutation).map(|tool| tool.name).collect::<Vec<_>>(), ["run"]);
    }
}
