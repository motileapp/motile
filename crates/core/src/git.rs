//! The git button of a project: the one action its status calls for, the menu behind it, and
//! what each says when it can't run. Every client shows the same.

use motile_protocol::wire::{GitAction, GitStatus, PullRequest};
use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Control {
    pub quick: Quick,
    pub menu: Vec<Item>,
    /// Said under the menu.
    pub warning: Option<String>,
}

/// What the button does when clicked: an action, or opening the pull request at `url`. With
/// neither, the button is off and `hint` says why.
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct Quick {
    pub label: String,
    /// Said before the label, and colors the button: "Merged", "Closed" or "Draft".
    pub state: Option<String>,
    pub action: Option<GitAction>,
    pub url: Option<String>,
    pub hint: Option<String>,
    pub confirm: Option<Confirm>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Item {
    pub label: String,
    pub action: GitAction,
    /// Why it can't run now. Missing when it can.
    pub reason: Option<String>,
    pub confirm: Option<Confirm>,
}

/// Asked before an action that pushes from the default branch.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Confirm {
    pub title: String,
    pub description: String,
    /// The button that goes on, on the default branch.
    pub proceed: String,
    /// The button that makes a branch for the work first.
    pub branch_off: String,
}

const UP_TO_DATE: &str = "The branch is up to date. Nothing to do.";

pub fn control(status: &GitStatus) -> Control {
    let warning = match (&status.branch, status.behind > 0) {
        (None, _) => Some("No branch is checked out. Check one out to push or open a pull request."),
        (Some(_), true) => Some("Behind the remote. Pull first."),
        (Some(_), false) => None,
    };
    Control { quick: quick(status), menu: menu(status), warning: warning.map(str::to_string) }
}

fn quick(status: &GitStatus) -> Quick {
    let run = |label: &str, action| Quick {
        label: label.to_string(),
        action: Some(action),
        confirm: confirm(status, action),
        ..Quick::default()
    };
    let off =
        |label: &str, hint: &str| Quick { label: label.to_string(), hint: Some(hint.to_string()), ..Quick::default() };
    // Without `gh` a pull request can't be opened, so the branch is only pushed.
    let pushes_only = has_open_pull_request(status) || status.default || !status.pull_requests;

    if status.branch.is_none() {
        return off("Commit", "Check out a branch before pushing or opening a pull request.");
    }
    if status.changed > 0 {
        if !status.remote {
            return run("Commit", GitAction::Commit);
        }
        if pushes_only {
            return run("Commit & Push", GitAction::CommitPush);
        }
        return run("Commit, Push & PR", GitAction::CommitPushPr);
    }
    if !status.remote {
        return off("Commit", UP_TO_DATE);
    }
    if status.upstream && status.ahead > 0 && status.behind > 0 {
        return off("Sync", "The branch and the remote have both moved on. Rebase or merge first.");
    }
    if status.behind > 0 {
        return run("Pull", GitAction::Pull);
    }
    if status.ahead > 0 {
        if pushes_only {
            return run("Push", GitAction::Push);
        }
        return run("Push & Create PR", GitAction::CreatePr);
    }
    if let Some(pull_request) = &status.pull_request {
        return Quick {
            label: format!("PR #{}", pull_request.number),
            state: state(pull_request).map(str::to_string),
            url: Some(pull_request.url.clone()),
            ..Quick::default()
        };
    }
    if status.ahead_of_default > 0 && !status.default && status.pull_requests {
        return run("Create PR", GitAction::CreatePr);
    }
    if !status.upstream {
        return off("Push", "No local commits to push.");
    }
    off("Commit", UP_TO_DATE)
}

fn has_open_pull_request(status: &GitStatus) -> bool {
    status.pull_request.as_ref().is_some_and(PullRequest::is_open)
}

fn state(pull_request: &PullRequest) -> Option<&'static str> {
    if pull_request.merged {
        return Some("Merged");
    }
    if pull_request.closed {
        return Some("Closed");
    }
    pull_request.draft.then_some("Draft")
}

fn menu(status: &GitStatus) -> Vec<Item> {
    let item = |label: &str, action, reason: Option<&str>| Item {
        label: label.to_string(),
        action,
        reason: reason.map(str::to_string),
        confirm: confirm(status, action),
    };
    let nothing_changed = (status.changed == 0).then_some("Nothing has changed. Make changes before committing.");
    let commit = item("Commit", GitAction::Commit, nothing_changed);
    if !status.remote {
        return vec![commit];
    }

    let push_reason = if status.branch.is_none() {
        Some("Check out a branch before pushing.")
    } else if status.changed > 0 {
        Some("Commit your changes before pushing.")
    } else if status.behind > 0 {
        Some("The branch is behind the remote. Pull before pushing.")
    } else if status.ahead == 0 {
        Some("No local commits to push.")
    } else {
        None
    };
    let push = item("Push", GitAction::Push, push_reason);
    if has_open_pull_request(status) {
        return vec![commit, push];
    }
    let merged = status
        .pull_request
        .as_ref()
        .filter(|found| found.merged)
        .map(|merged| format!("PR #{} is merged. Commit new work before creating another.", merged.number));

    let pull_request_reason = if !status.pull_requests {
        Some("Install GitHub's gh on your server to open pull requests.")
    } else if status.branch.is_none() {
        Some("Check out a branch before creating a pull request.")
    } else if status.changed > 0 {
        Some("Commit your changes before creating a pull request.")
    } else if merged.is_some() {
        merged.as_deref()
    } else if status.behind > 0 {
        Some("The branch is behind the remote. Pull before creating a pull request.")
    } else if status.ahead_of_default.max(status.ahead) == 0 {
        Some("No commits to include in a pull request.")
    } else {
        None
    };
    vec![commit, push, item("Create PR", GitAction::CreatePr, pull_request_reason)]
}

fn confirm(status: &GitStatus, action: GitAction) -> Option<Confirm> {
    let branch = status.branch.as_deref().filter(|_| status.default)?;
    let commits = status.changed > 0 && matches!(action, GitAction::CommitPush | GitAction::CommitPushPr);
    let (title, does, proceed) = match (action, commits) {
        (GitAction::Commit | GitAction::Pull, _) => return None,
        (GitAction::Push | GitAction::CommitPush, true) => (
            format!("Commit & push to {branch}?"),
            "commit and push your changes",
            format!("Commit & push to {branch}"),
        ),
        (GitAction::Push | GitAction::CommitPush, false) => {
            (format!("Push to {branch}?"), "push your commits", format!("Push to {branch}"))
        }
        (_, true) => (
            format!("Commit, push & create a PR from {branch}?"),
            "commit, push and open a pull request",
            "Commit, push & create PR".to_string(),
        ),
        (_, false) => (
            format!("Push & create a PR from {branch}?"),
            "push your commits and open a pull request",
            "Push & create PR".to_string(),
        ),
    };
    Some(Confirm {
        title,
        description: format!("{branch} is the default branch. You can {does} there, or on a new branch."),
        proceed,
        branch_off: "Use a new branch".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch() -> GitStatus {
        GitStatus {
            branch: Some("feature/login".to_string()),
            remote: true,
            upstream: true,
            pull_requests: true,
            ..GitStatus::default()
        }
    }

    fn main() -> GitStatus {
        GitStatus { branch: Some("main".to_string()), default: true, ..branch() }
    }

    fn open() -> Option<PullRequest> {
        Some(PullRequest {
            number: 12,
            title: "Log in".to_string(),
            url: "https://x/12".to_string(),
            draft: false,
            merged: false,
            closed: false,
        })
    }

    fn merged() -> Option<PullRequest> {
        open().map(|open| PullRequest { merged: true, ..open })
    }

    fn closed() -> Option<PullRequest> {
        open().map(|open| PullRequest { closed: true, ..open })
    }

    fn quick_of(status: GitStatus) -> (String, Option<GitAction>) {
        let quick = control(&status).quick;
        (quick.label, quick.action)
    }

    fn runs(label: &str, action: GitAction) -> (String, Option<GitAction>) {
        (label.to_string(), Some(action))
    }

    fn off(label: &str) -> (String, Option<GitAction>) {
        (label.to_string(), None)
    }

    #[test]
    fn changes_are_committed_as_far_as_the_branch_can_go() {
        let local = GitStatus { remote: false, upstream: false, pull_requests: false, ..branch() };
        assert_eq!(quick_of(GitStatus { changed: 1, ..local }), runs("Commit", GitAction::Commit));
        assert_eq!(quick_of(GitStatus { changed: 1, ..branch() }), runs("Commit, Push & PR", GitAction::CommitPushPr));
        assert_eq!(quick_of(GitStatus { changed: 1, ..main() }), runs("Commit & Push", GitAction::CommitPush));
        let with_pull_request = GitStatus { changed: 1, pull_request: open(), ..branch() };
        assert_eq!(quick_of(with_pull_request), runs("Commit & Push", GitAction::CommitPush));
        let without_gh = GitStatus { changed: 1, pull_requests: false, ..branch() };
        assert_eq!(quick_of(without_gh), runs("Commit & Push", GitAction::CommitPush));
    }

    #[test]
    fn a_clean_branch_is_pulled_pushed_or_opened_as_a_pull_request() {
        assert_eq!(quick_of(GitStatus { behind: 2, ..branch() }), runs("Pull", GitAction::Pull));
        assert_eq!(quick_of(GitStatus { behind: 2, ahead: 1, ..branch() }), off("Sync"));
        assert_eq!(quick_of(GitStatus { ahead: 1, ..branch() }), runs("Push & Create PR", GitAction::CreatePr));
        assert_eq!(quick_of(GitStatus { ahead: 1, ..main() }), runs("Push", GitAction::Push));
        assert_eq!(quick_of(GitStatus { ahead: 1, pull_request: open(), ..branch() }), runs("Push", GitAction::Push));
        assert_eq!(quick_of(GitStatus { ahead_of_default: 2, ..branch() }), runs("Create PR", GitAction::CreatePr));
        assert_eq!(quick_of(branch()), off("Commit"));
        assert_eq!(quick_of(GitStatus { upstream: false, ..branch() }), off("Push"));
        assert_eq!(quick_of(GitStatus { branch: None, changed: 1, ..branch() }), off("Commit"));

        let opened = control(&GitStatus { ahead_of_default: 2, pull_request: open(), ..branch() }).quick;
        assert_eq!(
            (opened.label.as_str(), opened.url.as_deref(), opened.action),
            ("PR #12", Some("https://x/12"), None)
        );
        assert_eq!(opened.state, None);
    }

    #[test]
    fn a_merged_pull_request_is_shown_instead_of_creating_another() {
        // A squash merge leaves the branch ahead of the default one.
        let status = GitStatus { ahead_of_default: 2, pull_request: merged(), ..branch() };
        let control = control(&status);
        assert_eq!(
            (control.quick.label.as_str(), control.quick.url.as_deref(), control.quick.action),
            ("PR #12", Some("https://x/12"), None)
        );
        assert_eq!(control.quick.state.as_deref(), Some("Merged"));
        assert_eq!(control.menu[2].label, "Create PR");
        assert_eq!(
            control.menu[2].reason.as_deref(),
            Some("PR #12 is merged. Commit new work before creating another.")
        );

        let changed = GitStatus { changed: 1, ..status };
        assert_eq!(quick_of(changed), runs("Commit, Push & PR", GitAction::CommitPushPr));
    }

    #[test]
    fn a_closed_pull_request_is_shown_and_another_can_be_created() {
        let status = GitStatus { ahead_of_default: 2, pull_request: closed(), ..branch() };
        let control = control(&status);
        assert_eq!((control.quick.label.as_str(), control.quick.url.as_deref()), ("PR #12", Some("https://x/12")));
        assert_eq!(control.quick.state.as_deref(), Some("Closed"));
        assert_eq!((control.menu[2].label.as_str(), control.menu[2].reason.as_deref()), ("Create PR", None));

        let ahead = GitStatus { ahead: 1, ..status };
        assert_eq!(quick_of(ahead), runs("Push & Create PR", GitAction::CreatePr));
    }

    #[test]
    fn a_draft_pull_request_says_so() {
        let draft = open().map(|open| PullRequest { draft: true, ..open });
        let quick = control(&GitStatus { pull_request: draft, ..branch() }).quick;
        assert_eq!((quick.label.as_str(), quick.state.as_deref()), ("PR #12", Some("Draft")));
    }

    #[test]
    fn the_menu_says_why_an_item_cannot_run() {
        let reasons = |status: GitStatus| -> Vec<(String, bool)> {
            control(&status).menu.into_iter().map(|item| (item.label, item.reason.is_none())).collect()
        };
        let item = |label: &str, runs: bool| (label.to_string(), runs);

        let dirty = GitStatus { changed: 1, ahead: 1, ..branch() };
        assert_eq!(reasons(dirty), [item("Commit", true), item("Push", false), item("Create PR", false)]);
        let ahead = GitStatus { ahead: 1, ahead_of_default: 1, ..branch() };
        assert_eq!(reasons(ahead), [item("Commit", false), item("Push", true), item("Create PR", true)]);
        let opened = GitStatus { ahead: 1, pull_request: open(), ..branch() };
        assert_eq!(reasons(opened), [item("Commit", false), item("Push", true)]);
        let local = GitStatus { remote: false, changed: 1, ..branch() };
        assert_eq!(reasons(local), [item("Commit", true)]);

        let behind = control(&GitStatus { behind: 1, ahead_of_default: 1, ..branch() });
        assert_eq!(behind.warning.as_deref(), Some("Behind the remote. Pull first."));
        assert_eq!(
            behind.menu[2].reason.as_deref(),
            Some("The branch is behind the remote. Pull before creating a pull request.")
        );
    }

    #[test]
    fn pushing_from_the_default_branch_is_asked_about_first() {
        let quick = control(&GitStatus { changed: 1, ..main() }).quick;
        let confirm = quick.confirm.expect("a push from the default branch is confirmed");
        assert_eq!(confirm.title, "Commit & push to main?");
        assert_eq!(confirm.proceed, "Commit & push to main");

        let push = control(&GitStatus { ahead: 1, ..main() }).quick.confirm.unwrap();
        assert_eq!(push.proceed, "Push to main");
        assert_eq!(control(&GitStatus { changed: 1, ..branch() }).quick.confirm, None);
        assert_eq!(control(&GitStatus { changed: 1, ..main() }).menu[0].confirm, None);
    }
}
