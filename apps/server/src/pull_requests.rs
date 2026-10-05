//! A pull request as GitHub has it, read and acted on with GitHub's `gh` in a folder of its
//! repository: what stops it from merging, what was said on it, and merging, closing,
//! reviewing and the rest.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, bail};
use motile_protocol::wire::{
    Check, CheckStatus, EventKind, FileViewed, Label, MergeMethod, Mergeable, PullRequest, PullRequestAction,
    PullRequestDetail, PullRequestEdit, PullRequestEvent, PullRequestState, PullRequestSummary, Reaction, ReactionKind,
    ReviewDecision, ReviewThread, ReviewVerdict, Reviewer, Side, Stack, StackLayer, ThreadComment, Verdict, Viewer,
};
use serde_json::Value;

use crate::agents::environment::Environment;
use crate::git::{MAX_PATCH_BYTES, command, run};

const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// A merge waits for GitHub to make the commit.
const ACTION_TIMEOUT: Duration = Duration::from_secs(120);

const QUERY: &str = r#"
query($owner: String!, $repo: String!, $number: Int!, $head: String!) {
  viewer { login }
  repository(owner: $owner, name: $repo) {
    mergeCommitAllowed squashMergeAllowed rebaseMergeAllowed autoMergeAllowed viewerPermission viewerDefaultMergeMethod
    defaultBranchRef { name }
    labels(first: 100) { nodes { name color } }
    assignableUsers(first: 100) { nodes { login } }
    pullRequest(number: $number) {
      id number title url body state isDraft mergeable reviewDecision
      additions deletions changedFiles createdAt mergedAt closedAt baseRefName headRefName
      author { login }
      mergedBy { login }
      autoMergeRequest { mergeMethod }
      viewerCanUpdate viewerDidAuthor viewerCanUpdateBranch
      baseRef { compare(headRef: $head) { behindBy } }
      labels(first: 50) { nodes { name color } }
      reviewRequests(first: 50) { nodes { requestedReviewer { __typename ... on User { login } ... on Team { name } } } }
      latestReviews(first: 50) { nodes { author { login } state } }
      files(first: 100) { nodes { path viewerViewedState } }
      commits(last: 100) { totalCount nodes { commit { oid abbreviatedOid messageHeadline committedDate author { name user { login } } } } }
      lastCommit: commits(last: 1) { nodes { commit { statusCheckRollup { contexts(first: 100) { nodes {
        __typename
        ... on CheckRun { name status conclusion detailsUrl startedAt checkSuite { workflowRun { workflow { name } } } }
        ... on StatusContext { context state targetUrl description createdAt }
      } } } } } }
      comments(last: 100) { nodes { id author { login } body createdAt url ...Reactions } }
      reviews(last: 100) { nodes { id author { login } body state submittedAt url ...Reactions } }
      reviewThreads(first: 100) { nodes { id isResolved isOutdated path line diffSide comments(first: 50) { nodes {
        id author { login } body createdAt url diffHunk ...Reactions
      } } } }
    }
  }
}
fragment Reactions on Reactable { reactionGroups { content viewerHasReacted reactors { totalCount } } }
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
    let mut detail =
        read_detail(&answer).with_context(|| format!("GitHub has no pull request #{number} in this repository."))?;
    detail.stack = stack(folder, environment, number).await;
    if detail.default_branch.as_ref().is_some_and(|default| *default != detail.base) {
        detail.stacked_on = pull_request_of(folder, environment, &detail.base).await;
    }
    Ok(detail)
}

/// The stack GitHub keeps the pull request in. A host without stacks has none.
async fn stack(folder: &str, environment: &Environment, number: u64) -> Option<Stack> {
    let path = format!("repos/{{owner}}/{{repo}}/stacks?pull_request={number}");
    let listed = run(command("gh", folder, environment, &["api", &path]), None, READ_TIMEOUT).await.ok()?;
    read_stack(&serde_json::from_str(&listed).ok()?)
}

fn read_stack(listed: &Value) -> Option<Stack> {
    let stack = listed.as_array()?.first()?;
    let base = stack["base"]["ref"].as_str().or(stack["base"].as_str())?;
    let layers = stack["pull_requests"].as_array()?.iter().map(|layer| {
        let merged = !layer["merged_at"].is_null() && layer["merged_at"].is_string();
        StackLayer {
            number: layer["number"].as_u64().unwrap_or(0),
            title: text(&layer["title"]),
            head: text(&layer["head"]["ref"]),
            draft: layer["draft"].as_bool().unwrap_or(false),
            merged,
            closed: !merged && layer["state"].as_str() == Some("closed"),
        }
    });
    Some(Stack {
        number: stack["number"].as_u64()?,
        url: stack["html_url"].as_str().or(stack["url"].as_str()).unwrap_or_default().to_string(),
        base: base.to_string(),
        layers: layers.collect(),
    })
}

/// The latest pull request whose branch is `branch`.
async fn pull_request_of(folder: &str, environment: &Environment, branch: &str) -> Option<PullRequest> {
    let arguments = ["pr", "list", "--head", branch, "--state", "all", "--limit", "1", "--json", SUMMARY_FIELDS];
    let listed = run(command("gh", folder, environment, &arguments), None, READ_TIMEOUT).await.ok()?;
    let listed: Value = serde_json::from_str(&listed).ok()?;
    Some(read_summary(listed.as_array()?.first()?)?.pull_request)
}

const SUMMARY_FIELDS: &str = "number,title,url,state,isDraft,author,headRefName,baseRefName,updatedAt,reviewDecision,statusCheckRollup,additions,deletions";

/// The repository's pull requests in that state, the last updated first.
pub async fn list(
    folder: &str,
    environment: &Environment,
    state: PullRequestState,
) -> anyhow::Result<Vec<PullRequestSummary>> {
    let state = match state {
        PullRequestState::Open => "open",
        PullRequestState::Closed => "closed",
        PullRequestState::Merged => "merged",
        PullRequestState::All => "all",
    };
    let arguments = ["pr", "list", "--state", state, "--limit", "50", "--json", SUMMARY_FIELDS];
    let listed = run(command("gh", folder, environment, &arguments), None, READ_TIMEOUT).await?;
    let listed: Value = serde_json::from_str(&listed).context("GitHub's answer couldn't be read.")?;
    Ok(listed.as_array().into_iter().flatten().filter_map(read_summary).collect())
}

fn read_summary(found: &Value) -> Option<PullRequestSummary> {
    let state = found["state"].as_str()?;
    let checks: Vec<CheckStatus> = found["statusCheckRollup"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|context| match (context["status"].as_str(), context["conclusion"].as_str(), context["state"].as_str()) {
            (Some(status), _, _) if status != "COMPLETED" => CheckStatus::Pending,
            (_, Some(conclusion), _) if !conclusion.is_empty() => conclusion_status(Some(conclusion)),
            (_, _, state) => conclusion_status(state),
        })
        .collect();
    Some(PullRequestSummary {
        pull_request: PullRequest {
            number: found["number"].as_u64()?,
            title: text(&found["title"]),
            url: text(&found["url"]),
            draft: found["isDraft"].as_bool().unwrap_or(false),
            merged: state == "MERGED",
            closed: state == "CLOSED",
        },
        author: login(&found["author"]),
        head: text(&found["headRefName"]),
        base: text(&found["baseRefName"]),
        updated_at: time(&found["updatedAt"]).unwrap_or(0.0),
        review: decision(&found["reviewDecision"]),
        checks: rollup(&checks),
        additions: count(&found["additions"]),
        deletions: count(&found["deletions"]),
    })
}

/// The checks together: a failure outweighs one that runs, which outweighs a success.
fn rollup(checks: &[CheckStatus]) -> Option<CheckStatus> {
    use CheckStatus::*;
    [Failure, Cancelled, ActionRequired, Pending, Success]
        .into_iter()
        .find(|status| checks.contains(status))
        .or_else(|| checks.first().map(|_| Neutral))
}

/// What one commit changed, as GitHub has it.
pub async fn commit_diff(folder: &str, environment: &Environment, sha: &str) -> anyhow::Result<(String, bool)> {
    if sha.is_empty() || !sha.chars().all(|letter| letter.is_ascii_hexdigit()) {
        bail!("That isn't a commit.");
    }
    let path = format!("repos/{{owner}}/{{repo}}/commits/{sha}");
    let arguments = ["api", &path, "-H", "Accept: application/vnd.github.diff"];
    let patch = run(command("gh", folder, environment, &arguments), None, READ_TIMEOUT).await?;
    Ok(cut(patch))
}

/// Changes the pull request as the edit says, and says what was done. Reactions and viewed
/// files go without a word.
pub async fn edit(
    folder: &str,
    environment: &Environment,
    number: u64,
    edit: &PullRequestEdit,
) -> anyhow::Result<String> {
    let numbered = number.to_string();
    let gh = async |arguments: &[&str], input: Option<&str>| {
        run(command("gh", folder, environment, arguments), input, ACTION_TIMEOUT).await
    };
    match edit {
        PullRequestEdit::Title { title } => {
            let title = title.trim();
            if title.is_empty() {
                bail!("A pull request needs a title.");
            }
            gh(&["pr", "edit", &numbered, "--title", title], None).await?;
            Ok(format!("Renamed PR #{number}"))
        }
        PullRequestEdit::Body { body } => {
            gh(&["pr", "edit", &numbered, "--body-file", "-"], Some(body)).await?;
            Ok(format!("Updated the description of PR #{number}"))
        }
        PullRequestEdit::Labels { add, remove } => {
            let arguments = changes(&["pr", "edit", &numbered], "--add-label", add, "--remove-label", remove);
            gh(&arguments.iter().map(String::as_str).collect::<Vec<_>>(), None).await?;
            Ok(format!("Updated the labels of PR #{number}"))
        }
        PullRequestEdit::Reviewers { add, remove } => {
            let arguments = changes(&["pr", "edit", &numbered], "--add-reviewer", add, "--remove-reviewer", remove);
            gh(&arguments.iter().map(String::as_str).collect::<Vec<_>>(), None).await?;
            Ok(format!("Updated the reviewers of PR #{number}"))
        }
        PullRequestEdit::React { subject, reaction, on } => {
            let mutation = if *on { "addReaction" } else { "removeReaction" };
            let query = format!(
                "mutation($subject: ID!, $content: ReactionContent!) {{ {mutation}(input: {{subjectId: $subject, content: $content}}) {{ clientMutationId }} }}"
            );
            graphql(folder, environment, &query, &[("subject", subject), ("content", reaction_name(*reaction))])
                .await?;
            Ok(String::new())
        }
        PullRequestEdit::Reply { thread, body } => {
            if body.trim().is_empty() {
                bail!("Write a reply first.");
            }
            let query = "mutation($thread: ID!, $body: String!) { addPullRequestReviewThreadReply(input: {pullRequestReviewThreadId: $thread, body: $body}) { clientMutationId } }";
            graphql(folder, environment, query, &[("thread", thread), ("body", body.trim())]).await?;
            Ok(format!("Replied on PR #{number}"))
        }
        PullRequestEdit::Resolve { thread, resolved } => {
            let mutation = if *resolved { "resolveReviewThread" } else { "unresolveReviewThread" };
            let query =
                format!("mutation($thread: ID!) {{ {mutation}(input: {{threadId: $thread}}) {{ clientMutationId }} }}");
            graphql(folder, environment, &query, &[("thread", thread)]).await?;
            Ok(if *resolved { "Resolved the conversation" } else { "Opened the conversation again" }.to_string())
        }
        PullRequestEdit::Viewed { path, viewed } => {
            let id = node_id(folder, environment, number).await?;
            let mutation = if *viewed { "markFileAsViewed" } else { "unmarkFileAsViewed" };
            let query = format!(
                "mutation($id: ID!, $path: String!) {{ {mutation}(input: {{pullRequestId: $id, path: $path}}) {{ clientMutationId }} }}"
            );
            graphql(folder, environment, &query, &[("id", &id), ("path", path)]).await?;
            Ok(String::new())
        }
        PullRequestEdit::Review { verdict, body, comments } => {
            let (event, said) = match verdict {
                ReviewVerdict::Comment => ("COMMENT", "Reviewed"),
                ReviewVerdict::Approve => ("APPROVE", "Approved"),
                ReviewVerdict::RequestChanges => ("REQUEST_CHANGES", "Asked for changes on"),
            };
            if *verdict != ReviewVerdict::Approve && body.trim().is_empty() && comments.is_empty() {
                bail!("Write something for the review first.");
            }
            let lines: Vec<Value> = comments
                .iter()
                .map(|comment| {
                    let side = if comment.side == Side::Left { "LEFT" } else { "RIGHT" };
                    serde_json::json!({"path": comment.path, "line": comment.line, "side": side, "body": comment.body})
                })
                .collect();
            let review = serde_json::json!({"event": event, "body": body.trim(), "comments": lines}).to_string();
            let path = format!("repos/{{owner}}/{{repo}}/pulls/{number}/reviews");
            gh(&["api", &path, "--method", "POST", "--input", "-"], Some(&review)).await?;
            let count = match comments.len() {
                0 => String::new(),
                1 => " with a comment".to_string(),
                count => format!(" with {count} comments"),
            };
            Ok(format!("{said} PR #{number}{count}"))
        }
    }
}

/// `start` with a flag for each name to add and each to take away.
fn changes(start: &[&str], add_flag: &str, add: &[String], remove_flag: &str, remove: &[String]) -> Vec<String> {
    let mut arguments: Vec<String> = start.iter().map(|word| word.to_string()).collect();
    for name in add {
        arguments.extend([add_flag.to_string(), name.clone()]);
    }
    for name in remove {
        arguments.extend([remove_flag.to_string(), name.clone()]);
    }
    arguments
}

/// The name GitHub's API knows the pull request by.
async fn node_id(folder: &str, environment: &Environment, number: u64) -> anyhow::Result<String> {
    let query = "query($owner: String!, $repo: String!, $number: Int!) { repository(owner: $owner, name: $repo) { pullRequest(number: $number) { id } } }";
    let arguments = [
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
        "-F".to_string(),
        "owner={owner}".to_string(),
        "-F".to_string(),
        "repo={repo}".to_string(),
        "-F".to_string(),
        format!("number={number}"),
    ];
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let answer: Value =
        serde_json::from_str(&run(command("gh", folder, environment, &arguments), None, READ_TIMEOUT).await?)?;
    Ok(answer["data"]["repository"]["pullRequest"]["id"]
        .as_str()
        .context("GitHub didn't name the pull request.")?
        .to_string())
}

/// Runs a GraphQL mutation with string variables.
async fn graphql(
    folder: &str,
    environment: &Environment,
    query: &str,
    variables: &[(&str, &str)],
) -> anyhow::Result<()> {
    let mut arguments = vec!["api".to_string(), "graphql".to_string(), "-f".to_string(), format!("query={query}")];
    for (name, value) in variables {
        arguments.extend(["-f".to_string(), format!("{name}={value}")]);
    }
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    run(command("gh", folder, environment, &arguments), None, ACTION_TIMEOUT).await?;
    Ok(())
}

fn reaction_name(reaction: ReactionKind) -> &'static str {
    match reaction {
        ReactionKind::ThumbsUp => "THUMBS_UP",
        ReactionKind::ThumbsDown => "THUMBS_DOWN",
        ReactionKind::Laugh => "LAUGH",
        ReactionKind::Hooray => "HOORAY",
        ReactionKind::Confused => "CONFUSED",
        ReactionKind::Heart => "HEART",
        ReactionKind::Rocket => "ROCKET",
        ReactionKind::Eyes => "EYES",
    }
}

fn reactions(subject: &Value) -> Vec<Reaction> {
    let kinds = [
        ReactionKind::ThumbsUp,
        ReactionKind::ThumbsDown,
        ReactionKind::Laugh,
        ReactionKind::Hooray,
        ReactionKind::Confused,
        ReactionKind::Heart,
        ReactionKind::Rocket,
        ReactionKind::Eyes,
    ];
    let groups = subject["reactionGroups"].as_array().cloned().unwrap_or_default();
    let found = groups.iter().filter_map(|group| {
        let kind = kinds.into_iter().find(|kind| group["content"].as_str() == Some(reaction_name(*kind)))?;
        let count = count(&group["reactors"]["totalCount"]);
        let mine = group["viewerHasReacted"].as_bool().unwrap_or(false);
        (count > 0).then_some(Reaction { kind, count, mine })
    });
    found.collect()
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
    let patch = run(command("gh", folder, environment, &arguments), None, READ_TIMEOUT).await?;
    Ok(cut(patch))
}

/// The patch, cut at a line when it is too long, and whether it was.
fn cut(mut patch: String) -> (String, bool) {
    if patch.len() <= MAX_PATCH_BYTES {
        return (patch, false);
    }
    let whole_lines =
        patch.as_bytes()[..MAX_PATCH_BYTES].iter().rposition(|byte| *byte == b'\n').map_or(0, |end| end + 1);
    patch.truncate(whole_lines);
    (patch, true)
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
        review: decision(&found["reviewDecision"]),
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
            can_triage: matches!(
                repository["viewerPermission"].as_str(),
                Some("ADMIN" | "MAINTAIN" | "WRITE" | "TRIAGE")
            ),
            login: text(&answer["data"]["viewer"]["login"]),
        },
        activity: activity(found),
        id: text(&found["id"]),
        default_branch: repository["defaultBranchRef"]["name"].as_str().map(str::to_string),
        labels: labels(&found["labels"]),
        repository_labels: labels(&repository["labels"]),
        reviewers: reviewers(found),
        assignable: nodes(&repository["assignableUsers"]).iter().map(|user| text(&user["login"])).collect(),
        files: nodes(&found["files"])
            .iter()
            .map(|file| FileViewed {
                path: text(&file["path"]),
                viewed: file["viewerViewedState"].as_str() == Some("VIEWED"),
            })
            .collect(),
        threads: threads(found),
        stack: None,
        stacked_on: None,
        pull_request,
    })
}

fn decision(value: &Value) -> Option<ReviewDecision> {
    match value.as_str() {
        Some("APPROVED") => Some(ReviewDecision::Approved),
        Some("CHANGES_REQUESTED") => Some(ReviewDecision::ChangesRequested),
        Some("REVIEW_REQUIRED") => Some(ReviewDecision::ReviewRequired),
        _ => None,
    }
}

fn nodes(list: &Value) -> Vec<Value> {
    list["nodes"].as_array().cloned().unwrap_or_default()
}

fn labels(list: &Value) -> Vec<Label> {
    nodes(list).iter().map(|label| Label { name: text(&label["name"]), color: text(&label["color"]) }).collect()
}

/// Who was asked to review and who reviewed, each once, with their latest verdict. The author's
/// own replies aren't a review.
fn reviewers(found: &Value) -> Vec<Reviewer> {
    let author = login(&found["author"]);
    let mut reviewers: Vec<Reviewer> = Vec::new();
    for review in nodes(&found["latestReviews"]) {
        if login(&review["author"]) == author {
            continue;
        }
        let verdict = match review["state"].as_str() {
            Some("APPROVED") => Verdict::Approved,
            Some("CHANGES_REQUESTED") => Verdict::ChangesRequested,
            Some("DISMISSED") => Verdict::Dismissed,
            Some("COMMENTED") => Verdict::Commented,
            _ => continue,
        };
        reviewers.push(Reviewer { name: login(&review["author"]), requested: false, verdict: Some(verdict) });
    }
    for request in nodes(&found["reviewRequests"]) {
        let asked = &request["requestedReviewer"];
        let Some(name) = asked["login"].as_str().or(asked["name"].as_str()) else { continue };
        match reviewers.iter_mut().find(|reviewer| reviewer.name == name) {
            Some(reviewer) => reviewer.requested = true,
            None => reviewers.push(Reviewer { name: name.to_string(), requested: true, verdict: None }),
        }
    }
    reviewers
}

fn threads(found: &Value) -> Vec<ReviewThread> {
    let threads = nodes(&found["reviewThreads"]).into_iter().map(|thread| ReviewThread {
        id: text(&thread["id"]),
        path: text(&thread["path"]),
        line: thread["line"].as_u64().map(|line| line as u32),
        side: if thread["diffSide"].as_str() == Some("LEFT") { Side::Left } else { Side::Right },
        resolved: thread["isResolved"].as_bool().unwrap_or(false),
        outdated: thread["isOutdated"].as_bool().unwrap_or(false),
        comments: nodes(&thread["comments"])
            .iter()
            .map(|comment| ThreadComment {
                id: text(&comment["id"]),
                author: login(&comment["author"]),
                body: text(&comment["body"]),
                at: time(&comment["createdAt"]).unwrap_or(0.0),
                url: comment["url"].as_str().map(str::to_string),
                hunk: comment["diffHunk"].as_str().map(str::to_string),
                reactions: reactions(comment),
            })
            .collect(),
    });
    threads.filter(|thread| !thread.comments.is_empty()).collect()
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
                sha: text(&commit["oid"]),
            },
        });
    }
    for comment in nodes(&found["comments"]) {
        events.push(PullRequestEvent {
            at: time(&comment["createdAt"]).unwrap_or(0.0),
            author: login(&comment["author"]),
            kind: EventKind::Comment {
                body: text(&comment["body"]),
                url: comment["url"].as_str().map(str::to_string),
                id: text(&comment["id"]),
                reactions: reactions(&comment),
            },
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
                id: text(&review["id"]),
                reactions: reactions(&review),
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

pub(crate) fn time(value: &Value) -> Option<f64> {
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

    #[test]
    fn reviewers_conversations_and_reactions_are_read() {
        let read = read_detail(&answer(json!({
            "number": 7, "title": "T", "url": "u", "state": "OPEN", "id": "PR_7",
            "reviewRequests": {"nodes": [
                {"requestedReviewer": {"__typename": "User", "login": "bo"}},
                {"requestedReviewer": {"__typename": "Team", "name": "core"}},
            ]},
            "latestReviews": {"nodes": [
                {"author": {"login": "ana"}, "state": "APPROVED"},
                {"author": {"login": "bo"}, "state": "COMMENTED"},
            ]},
            "files": {"nodes": [{"path": "a.py", "viewerViewedState": "VIEWED"}, {"path": "b.py", "viewerViewedState": "UNVIEWED"}]},
            "comments": {"nodes": [{"id": "C1", "author": {"login": "ana"}, "body": "Nice", "createdAt": "2026-10-05T04:00:00Z",
                "reactionGroups": [
                    {"content": "THUMBS_UP", "viewerHasReacted": true, "reactors": {"totalCount": 2}},
                    {"content": "HEART", "viewerHasReacted": false, "reactors": {"totalCount": 0}},
                ]}]},
            "reviewThreads": {"nodes": [
                {"id": "T1", "isResolved": false, "isOutdated": false, "path": "a.py", "line": 3, "diffSide": "LEFT",
                 "comments": {"nodes": [{"id": "RC1", "author": {"login": "ana"}, "body": "Why?", "createdAt": "2026-10-05T04:00:00Z",
                    "diffHunk": "@@ -1 +1 @@", "reactionGroups": []}]}},
                {"id": "T2", "isResolved": true, "isOutdated": true, "path": "b.py", "line": null, "diffSide": "RIGHT",
                 "comments": {"nodes": []}},
            ]},
        })))
        .unwrap();

        let reviewers: Vec<(&str, bool, Option<Verdict>)> = read
            .reviewers
            .iter()
            .map(|reviewer| (reviewer.name.as_str(), reviewer.requested, reviewer.verdict))
            .collect();
        assert_eq!(
            reviewers,
            [("ana", false, Some(Verdict::Approved)), ("bo", true, Some(Verdict::Commented)), ("core", true, None)]
        );
        assert_eq!(read.files.iter().filter(|file| file.viewed).count(), 1);
        let EventKind::Comment { id, reactions, .. } = &read.activity[0].kind else { panic!("expected the comment") };
        assert_eq!(
            (id.as_str(), reactions.as_slice()),
            ("C1", &[Reaction { kind: ReactionKind::ThumbsUp, count: 2, mine: true }][..])
        );
        // A conversation without comments left is no conversation.
        assert_eq!(read.threads.len(), 1);
        assert_eq!((read.threads[0].line, read.threads[0].side), (Some(3), Side::Left));
    }

    #[test]
    fn a_stack_is_read_bottom_first() {
        let listed = json!([{"number": 3, "url": "api", "html_url": "https://github.com/acme/app/stacks/3", "base": {"ref": "main"},
        "pull_requests": [
            {"number": 7, "title": "Bottom", "head": {"ref": "a"}, "state": "closed", "merged_at": "2026-10-05T04:00:00Z"},
            {"number": 8, "title": "Top", "head": {"ref": "b"}, "state": "open", "draft": true, "merged_at": null},
        ]}]);
        let stack = read_stack(&listed).unwrap();
        assert_eq!(
            (stack.number, stack.base.as_str(), stack.url.as_str()),
            (3, "main", "https://github.com/acme/app/stacks/3")
        );
        let layers: Vec<(u64, bool, bool)> =
            stack.layers.iter().map(|layer| (layer.number, layer.merged, layer.draft)).collect();
        assert_eq!(layers, [(7, true, false), (8, false, true)]);
        assert!(read_stack(&json!([])).is_none());
    }

    #[test]
    fn a_listed_pull_request_says_how_its_checks_went_together() {
        let listed = json!({"number": 9, "title": "T", "url": "u", "state": "OPEN", "isDraft": false,
        "author": {"login": "ana"}, "headRefName": "b", "baseRefName": "main", "updatedAt": "2026-10-05T04:00:00Z",
        "reviewDecision": "REVIEW_REQUIRED", "additions": 3, "deletions": 1,
        "statusCheckRollup": [
            {"__typename": "CheckRun", "status": "COMPLETED", "conclusion": "SUCCESS"},
            {"__typename": "CheckRun", "status": "IN_PROGRESS", "conclusion": ""},
            {"__typename": "StatusContext", "state": "FAILURE"},
        ]});
        let summary = read_summary(&listed).unwrap();
        assert_eq!(
            (summary.checks, summary.review),
            (Some(CheckStatus::Failure), Some(ReviewDecision::ReviewRequired))
        );
        assert_eq!(rollup(&[CheckStatus::Success, CheckStatus::Pending]), Some(CheckStatus::Pending));
        assert_eq!(rollup(&[]), None);
    }
}
