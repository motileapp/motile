//! A pull request as GitHub has it, read and acted on with GitHub's `gh` in a folder of its
//! repository: what stops it from merging, what was said on it, and merging, closing,
//! reviewing and the rest.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, bail};
use motile_protocol::wire::{
    Check, CheckStatus, EventKind, MergeMethod, Mergeable, PullRequest, PullRequestAction, PullRequestDetail,
    PullRequestEvent, ReviewDecision, Verdict, Viewer,
};
use serde_json::Value;

use crate::agents::environment::Environment;
use crate::git::{MAX_PATCH_BYTES, command, run};

const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// A merge waits for GitHub to make the commit.
const ACTION_TIMEOUT: Duration = Duration::from_secs(120);

const QUERY: &str = r#"
query($owner: String!, $repo: String!, $number: Int!, $head: String!) {
  repository(owner: $owner, name: $repo) {
    mergeCommitAllowed squashMergeAllowed rebaseMergeAllowed autoMergeAllowed viewerPermission viewerDefaultMergeMethod
    pullRequest(number: $number) {
      number title url body state isDraft mergeable reviewDecision
      additions deletions changedFiles createdAt mergedAt closedAt baseRefName headRefName
      author { login }
      mergedBy { login }
      autoMergeRequest { mergeMethod }
      viewerCanUpdate viewerDidAuthor viewerCanUpdateBranch
      baseRef { compare(headRef: $head) { behindBy } }
      commits(last: 100) { totalCount nodes { commit { abbreviatedOid messageHeadline committedDate author { name user { login } } } } }
      lastCommit: commits(last: 1) { nodes { commit { statusCheckRollup { contexts(first: 100) { nodes {
        __typename
        ... on CheckRun { name status conclusion detailsUrl startedAt checkSuite { workflowRun { workflow { name } } } }
        ... on StatusContext { context state targetUrl description createdAt }
      } } } } } }
      comments(last: 100) { nodes { author { login } body createdAt url } }
      reviews(last: 100) { nodes { author { login } body state submittedAt url } }
    }
  }
}
"#;

/// What GitHub says of the pull request with that number.
pub async fn detail(folder: &str, environment: &Environment, number: u64) -> anyhow::Result<PullRequestDetail> {
    let arguments = [
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={QUERY}"),
        "-F".to_string(),
        "owner={owner}".to_string(),
        "-F".to_string(),
        "repo={repo}".to_string(),
        "-F".to_string(),
        format!("number={number}"),
        "-f".to_string(),
        format!("head=refs/pull/{number}/head"),
    ];
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let answer = run(command("gh", folder, environment, &arguments), None, READ_TIMEOUT).await?;
    let answer: Value = serde_json::from_str(&answer).context("GitHub's answer couldn't be read.")?;
    read_detail(&answer).with_context(|| format!("GitHub has no pull request #{number} in this repository."))
}

/// Does `action` to the pull request and says what was done, with the address of a pull request
/// it opened.
pub async fn act(
    folder: &str,
    environment: &Environment,
    number: u64,
    action: PullRequestAction,
    method: Option<MergeMethod>,
    text: Option<&str>,
) -> anyhow::Result<(String, Option<String>)> {
    let text = text.map(str::trim).filter(|text| !text.is_empty());
    let (arguments, input) = arguments(number, action, method, text)?;
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let said = run(command("gh", folder, environment, &arguments), input, ACTION_TIMEOUT).await?;
    if action == PullRequestAction::Revert {
        let url = said.lines().rev().find(|line| line.starts_with("http")).map(|line| line.trim().to_string());
        let opened = url.as_deref().and_then(|url| url.rsplit('/').next()).unwrap_or_default();
        return Ok((format!("Opened PR #{opened} to revert PR #{number}"), url));
    }
    Ok((done(number, action, method), None))
}

/// The patch of what the pull request changes, and whether it was cut for being too long.
pub async fn diff(folder: &str, environment: &Environment, number: u64) -> anyhow::Result<(String, bool)> {
    let number = number.to_string();
    let arguments = ["pr", "diff", &number, "--color", "never"];
    let mut patch = run(command("gh", folder, environment, &arguments), None, READ_TIMEOUT).await?;
    if patch.len() <= MAX_PATCH_BYTES {
        return Ok((patch, false));
    }
    let whole_lines =
        patch.as_bytes()[..MAX_PATCH_BYTES].iter().rposition(|byte| *byte == b'\n').map_or(0, |end| end + 1);
    patch.truncate(whole_lines);
    Ok((patch, true))
}

/// The arguments of `gh` for the action, and what it reads.
fn arguments(
    number: u64,
    action: PullRequestAction,
    method: Option<MergeMethod>,
    text: Option<&str>,
) -> anyhow::Result<(Vec<String>, Option<&str>)> {
    use PullRequestAction::*;
    let number = number.to_string();
    let how = || format!("--{}", flag(method.unwrap_or(MergeMethod::Merge)));
    let with_comment = |mut arguments: Vec<String>| {
        if let Some(text) = text {
            arguments.extend(["--comment".to_string(), text.to_string()]);
        }
        arguments
    };
    let list = |words: &[&str]| words.iter().map(|word| word.to_string()).collect::<Vec<_>>();
    Ok(match action {
        Merge => (list(&["pr", "merge", &number, &how()]), None),
        EnableAutoMerge => (list(&["pr", "merge", &number, "--auto", &how()]), None),
        DisableAutoMerge => (list(&["pr", "merge", &number, "--disable-auto"]), None),
        Ready => (list(&["pr", "ready", &number]), None),
        Draft => (list(&["pr", "ready", &number, "--undo"]), None),
        Close => (with_comment(list(&["pr", "close", &number])), None),
        Reopen => (with_comment(list(&["pr", "reopen", &number])), None),
        UpdateBranch if method == Some(MergeMethod::Rebase) => {
            (list(&["pr", "update-branch", &number, "--rebase"]), None)
        }
        UpdateBranch => (list(&["pr", "update-branch", &number]), None),
        Revert => (list(&["pr", "revert", &number]), None),
        Comment => {
            let text = text.context("Write a comment first.")?;
            (list(&["pr", "comment", &number, "--body-file", "-"]), Some(text))
        }
        Approve => match text {
            Some(text) => (list(&["pr", "review", &number, "--approve", "--body-file", "-"]), Some(text)),
            None => (list(&["pr", "review", &number, "--approve"]), None),
        },
        RequestChanges => {
            let Some(text) = text else { bail!("Say what should change first.") };
            (list(&["pr", "review", &number, "--request-changes", "--body-file", "-"]), Some(text))
        }
    })
}

fn flag(method: MergeMethod) -> &'static str {
    match method {
        MergeMethod::Merge => "merge",
        MergeMethod::Squash => "squash",
        MergeMethod::Rebase => "rebase",
    }
}

/// What a client says once the action is done.
fn done(number: u64, action: PullRequestAction, method: Option<MergeMethod>) -> String {
    use PullRequestAction::*;
    match action {
        Merge => match method {
            Some(MergeMethod::Squash) => format!("Squashed and merged PR #{number}"),
            Some(MergeMethod::Rebase) => format!("Rebased and merged PR #{number}"),
            _ => format!("Merged PR #{number}"),
        },
        EnableAutoMerge => format!("PR #{number} merges once it can"),
        DisableAutoMerge => format!("PR #{number} no longer merges by itself"),
        Ready => format!("PR #{number} is ready for review"),
        Draft => format!("PR #{number} is a draft again"),
        Close => format!("Closed PR #{number}"),
        Reopen => format!("Reopened PR #{number}"),
        UpdateBranch => format!("Brought PR #{number} up to date"),
        Revert => format!("Reverted PR #{number}"),
        Comment => format!("Commented on PR #{number}"),
        Approve => format!("Approved PR #{number}"),
        RequestChanges => format!("Asked for changes on PR #{number}"),
    }
}

/// Reads the answer to `QUERY`.
fn read_detail(answer: &Value) -> Option<PullRequestDetail> {
    let repository = &answer["data"]["repository"];
    let found = &repository["pullRequest"];
    let state = found["state"].as_str()?;
    let merged_at = time(&found["mergedAt"]);
    let merged = state == "MERGED" || merged_at.is_some();
    let pull_request = PullRequest {
        number: found["number"].as_u64()?,
        title: found["title"].as_str()?.to_string(),
        url: found["url"].as_str()?.to_string(),
        draft: found["isDraft"].as_bool().unwrap_or(false),
        merged,
        closed: !merged && state == "CLOSED",
    };
    let methods = [
        (MergeMethod::Merge, "mergeCommitAllowed", "MERGE"),
        (MergeMethod::Squash, "squashMergeAllowed", "SQUASH"),
        (MergeMethod::Rebase, "rebaseMergeAllowed", "REBASE"),
    ];
    let preferred = repository["viewerDefaultMergeMethod"].as_str();
    let mut merge_methods: Vec<(bool, MergeMethod)> = methods
        .iter()
        .filter(|(_, allowed, _)| repository[*allowed].as_bool().unwrap_or(false))
        .map(|(method, _, name)| (preferred != Some(name), *method))
        .collect();
    merge_methods.sort_by_key(|(later, _)| *later);

    Some(PullRequestDetail {
        body: text(&found["body"]),
        author: login(&found["author"]),
        base: text(&found["baseRefName"]),
        head: text(&found["headRefName"]),
        additions: count(&found["additions"]),
        deletions: count(&found["deletions"]),
        changed_files: count(&found["changedFiles"]),
        commits: count(&found["commits"]["totalCount"]),
        created_at: time(&found["createdAt"]).unwrap_or(0.0),
        merged_at,
        merged_by: found["mergedBy"]["login"].as_str().map(str::to_string),
        closed_at: time(&found["closedAt"]),
        mergeable: match found["mergeable"].as_str() {
            Some("MERGEABLE") => Mergeable::Mergeable,
            Some("CONFLICTING") => Mergeable::Conflicting,
            _ => Mergeable::Unknown,
        },
        behind_by: found["baseRef"]["compare"]["behindBy"].as_u64().map(|behind| behind as u32),
        review: match found["reviewDecision"].as_str() {
            Some("APPROVED") => Some(ReviewDecision::Approved),
            Some("CHANGES_REQUESTED") => Some(ReviewDecision::ChangesRequested),
            Some("REVIEW_REQUIRED") => Some(ReviewDecision::ReviewRequired),
            _ => None,
        },
        checks: checks(&found["lastCommit"]["nodes"][0]["commit"]["statusCheckRollup"]["contexts"]["nodes"]),
        auto_merge: match found["autoMergeRequest"]["mergeMethod"].as_str() {
            Some("MERGE") => Some(MergeMethod::Merge),
            Some("SQUASH") => Some(MergeMethod::Squash),
            Some("REBASE") => Some(MergeMethod::Rebase),
            _ => None,
        },
        merge_methods: merge_methods.into_iter().map(|(_, method)| method).collect(),
        auto_merge_allowed: repository["autoMergeAllowed"].as_bool().unwrap_or(false),
        viewer: Viewer {
            can_write: matches!(repository["viewerPermission"].as_str(), Some("ADMIN" | "MAINTAIN" | "WRITE")),
            can_update: found["viewerCanUpdate"].as_bool().unwrap_or(false),
            can_update_branch: found["viewerCanUpdateBranch"].as_bool().unwrap_or(false),
            authored: found["viewerDidAuthor"].as_bool().unwrap_or(false),
        },
        activity: activity(found),
        pull_request,
    })
}

/// The checks of the last commit, one per name: a check that ran again is the newest run.
fn checks(contexts: &Value) -> Vec<Check> {
    let mut checks: Vec<(String, Check)> = Vec::new();
    let mut started: HashMap<String, String> = HashMap::new();
    for context in contexts.as_array().into_iter().flatten() {
        let (check, at) = match context["__typename"].as_str() {
            Some("CheckRun") => {
                let status = match (context["status"].as_str(), context["conclusion"].as_str()) {
                    (Some("COMPLETED"), conclusion) => conclusion_status(conclusion),
                    _ => CheckStatus::Pending,
                };
                let check = Check {
                    name: text(&context["name"]),
                    workflow: context["checkSuite"]["workflowRun"]["workflow"]["name"].as_str().map(str::to_string),
                    status,
                    description: None,
                    url: context["detailsUrl"].as_str().map(str::to_string),
                };
                (check, text(&context["startedAt"]))
            }
            Some("StatusContext") => {
                let check = Check {
                    name: text(&context["context"]),
                    workflow: None,
                    status: conclusion_status(context["state"].as_str()),
                    description: context["description"].as_str().filter(|said| !said.is_empty()).map(str::to_string),
                    url: context["targetUrl"].as_str().map(str::to_string),
                };
                (check, text(&context["createdAt"]))
            }
            _ => continue,
        };
        let key = format!("{}/{}", check.workflow.as_deref().unwrap_or_default(), check.name);
        match checks.iter().position(|(seen, _)| *seen == key) {
            Some(index) if started.get(&key).is_some_and(|before| *before <= at) => checks[index].1 = check,
            Some(_) => continue,
            None => checks.push((key.clone(), check)),
        }
        started.insert(key, at);
    }
    checks.into_iter().map(|(_, check)| check).collect()
}

fn conclusion_status(conclusion: Option<&str>) -> CheckStatus {
    match conclusion {
        Some("SUCCESS") => CheckStatus::Success,
        Some("ACTION_REQUIRED") => CheckStatus::ActionRequired,
        Some("FAILURE" | "ERROR" | "TIMED_OUT" | "STARTUP_FAILURE") => CheckStatus::Failure,
        Some("CANCELLED") => CheckStatus::Cancelled,
        Some("SKIPPED") => CheckStatus::Skipped,
        Some("PENDING" | "EXPECTED") | None => CheckStatus::Pending,
        Some(_) => CheckStatus::Neutral,
    }
}

/// The commits, comments and reviews, the oldest first.
fn activity(found: &Value) -> Vec<PullRequestEvent> {
    let nodes = |list: &Value| list["nodes"].as_array().cloned().unwrap_or_default();
    let mut events = Vec::new();
    for node in nodes(&found["commits"]) {
        let commit = &node["commit"];
        let author = commit["author"]["user"]["login"].as_str().or(commit["author"]["name"].as_str());
        events.push(PullRequestEvent {
            at: time(&commit["committedDate"]).unwrap_or(0.0),
            author: author.unwrap_or("ghost").to_string(),
            kind: EventKind::Commit {
                oid: text(&commit["abbreviatedOid"]),
                headline: text(&commit["messageHeadline"]),
            },
        });
    }
    for comment in nodes(&found["comments"]) {
        events.push(PullRequestEvent {
            at: time(&comment["createdAt"]).unwrap_or(0.0),
            author: login(&comment["author"]),
            kind: EventKind::Comment { body: text(&comment["body"]), url: comment["url"].as_str().map(str::to_string) },
        });
    }
    for review in nodes(&found["reviews"]) {
        let verdict = match review["state"].as_str() {
            Some("APPROVED") => Verdict::Approved,
            Some("CHANGES_REQUESTED") => Verdict::ChangesRequested,
            Some("DISMISSED") => Verdict::Dismissed,
            Some("COMMENTED") => Verdict::Commented,
            // A review that is still being written isn't anybody else's to see.
            _ => continue,
        };
        events.push(PullRequestEvent {
            at: time(&review["submittedAt"]).unwrap_or(0.0),
            author: login(&review["author"]),
            kind: EventKind::Review {
                verdict,
                body: text(&review["body"]),
                url: review["url"].as_str().map(str::to_string),
            },
        });
    }
    events.sort_by(|a, b| a.at.total_cmp(&b.at));
    events
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_string()
}

fn count(value: &Value) -> u32 {
    value.as_u64().unwrap_or(0) as u32
}

/// Someone who left GitHub is its ghost.
fn login(author: &Value) -> String {
    author["login"].as_str().unwrap_or("ghost").to_string()
}

fn time(value: &Value) -> Option<f64> {
    let at = chrono::DateTime::parse_from_rfc3339(value.as_str()?).ok()?;
    Some(at.timestamp_millis() as f64 / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn answer(pull_request: Value) -> Value {
        json!({"data": {"repository": {
            "mergeCommitAllowed": true, "squashMergeAllowed": true, "rebaseMergeAllowed": false,
            "autoMergeAllowed": true, "viewerPermission": "WRITE", "viewerDefaultMergeMethod": "SQUASH",
            "pullRequest": pull_request,
        }}})
    }

    #[test]
    fn a_pull_request_is_read_with_where_it_stands() {
        let read = read_detail(&answer(json!({
            "number": 7, "title": "Greet by name", "url": "https://github.com/acme/app/pull/7", "body": "Greets.",
            "state": "OPEN", "isDraft": false, "mergeable": "CONFLICTING", "reviewDecision": "CHANGES_REQUESTED",
            "additions": 4, "deletions": 1, "changedFiles": 2, "createdAt": "2026-10-05T03:43:11Z",
            "mergedAt": null, "closedAt": null, "baseRefName": "main", "headRefName": "greet",
            "author": {"login": "yekta"}, "mergedBy": null, "autoMergeRequest": {"mergeMethod": "SQUASH"},
            "viewerCanUpdate": true, "viewerDidAuthor": true, "viewerCanUpdateBranch": true,
            "baseRef": {"compare": {"behindBy": 3}},
            "commits": {"totalCount": 1, "nodes": [{"commit": {"abbreviatedOid": "1a2b3c4",
                "messageHeadline": "Greet by name", "committedDate": "2026-10-05T03:40:00Z",
                "author": {"name": "Yekta", "user": null}}}]},
            "comments": {"nodes": [{"author": null, "body": "Nice", "createdAt": "2026-10-05T04:00:00Z", "url": "u"}]},
            "reviews": {"nodes": [
                {"author": {"login": "ana"}, "body": "Rename it", "state": "CHANGES_REQUESTED", "submittedAt": "2026-10-05T03:50:00Z", "url": "r"},
                {"author": {"login": "ana"}, "body": "", "state": "PENDING", "submittedAt": null, "url": "p"},
            ]},
        })))
        .unwrap();

        assert_eq!((read.pull_request.number, read.pull_request.is_open()), (7, true));
        assert_eq!(
            (read.mergeable, read.behind_by, read.review),
            (Mergeable::Conflicting, Some(3), Some(ReviewDecision::ChangesRequested))
        );
        assert_eq!(read.merge_methods, [MergeMethod::Squash, MergeMethod::Merge]);
        assert_eq!(read.auto_merge, Some(MergeMethod::Squash));
        assert!(read.viewer.can_write && read.viewer.authored);
        assert_eq!(read.created_at, 1_791_171_791.0);
        let said: Vec<(&str, &str)> = read
            .activity
            .iter()
            .map(|event| match &event.kind {
                EventKind::Commit { headline, .. } => (event.author.as_str(), headline.as_str()),
                EventKind::Comment { body, .. } | EventKind::Review { body, .. } => {
                    (event.author.as_str(), body.as_str())
                }
            })
            .collect();
        assert_eq!(said, [("Yekta", "Greet by name"), ("ana", "Rename it"), ("ghost", "Nice")]);
    }

    #[test]
    fn a_check_that_ran_again_is_its_newest_run() {
        let contexts = json!([
            {"__typename": "CheckRun", "name": "rust", "status": "COMPLETED", "conclusion": "FAILURE",
             "detailsUrl": "a", "startedAt": "2026-10-05T03:00:00Z", "checkSuite": {"workflowRun": {"workflow": {"name": "CI"}}}},
            {"__typename": "CheckRun", "name": "rust", "status": "IN_PROGRESS", "conclusion": null,
             "detailsUrl": "b", "startedAt": "2026-10-05T03:10:00Z", "checkSuite": {"workflowRun": {"workflow": {"name": "CI"}}}},
            {"__typename": "StatusContext", "context": "deploy", "state": "ERROR", "targetUrl": "c",
             "description": "Build failed", "createdAt": "2026-10-05T03:05:00Z"},
        ]);
        let read = checks(&contexts);

        let statuses: Vec<(&str, CheckStatus, Option<&str>)> =
            read.iter().map(|check| (check.name.as_str(), check.status, check.url.as_deref())).collect();
        assert_eq!(statuses, [("rust", CheckStatus::Pending, Some("b")), ("deploy", CheckStatus::Failure, Some("c"))]);
        assert_eq!(read[1].description.as_deref(), Some("Build failed"));
    }

    #[test]
    fn a_merged_pull_request_is_merged_whatever_its_state_says() {
        let read = read_detail(&answer(json!({
            "number": 8, "title": "T", "url": "u", "state": "CLOSED", "mergedAt": "2026-10-05T03:43:11Z",
            "mergedBy": {"login": "yekta"},
        })))
        .unwrap();
        assert_eq!((read.pull_request.merged, read.pull_request.closed), (true, false));
        assert_eq!(read.merged_by.as_deref(), Some("yekta"));
        assert!(read_detail(&answer(Value::Null)).is_none());
    }

    #[test]
    fn each_action_is_the_gh_command_that_does_it() {
        let words = |action, method, text| {
            let (arguments, input) = arguments(7, action, method, text).unwrap();
            (arguments.join(" "), input)
        };
        use PullRequestAction::*;
        assert_eq!(words(Merge, Some(MergeMethod::Squash), None), ("pr merge 7 --squash".into(), None));
        assert_eq!(
            words(EnableAutoMerge, Some(MergeMethod::Rebase), None),
            ("pr merge 7 --auto --rebase".into(), None)
        );
        assert_eq!(words(Draft, None, None), ("pr ready 7 --undo".into(), None));
        assert_eq!(words(Close, None, Some("Not needed")), ("pr close 7 --comment Not needed".into(), None));
        assert_eq!(words(UpdateBranch, Some(MergeMethod::Rebase), None), ("pr update-branch 7 --rebase".into(), None));
        assert_eq!(words(Approve, None, None), ("pr review 7 --approve".into(), None));
        assert_eq!(words(Comment, None, Some("Looks good")), ("pr comment 7 --body-file -".into(), Some("Looks good")));
        assert!(arguments(7, RequestChanges, None, None).is_err());
    }
}
