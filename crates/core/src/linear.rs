//! The Linear tabs: a workspace's issues under their statuses, as Linear lists them, and one
//! issue with its description and comments set, and the prompt that hands it to an agent.

use motile_protocol::wire::{Label, LinearIssue, LinearIssueDetail, LinearState, LinearStateKind};
use serde::Serialize;

use crate::pull_request::{self, Text};

/// The issues of one status.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Group {
    pub state: LinearState,
    pub rows: Vec<Row>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Row {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub url: String,
    pub priority: u8,
    pub priority_label: &'static str,
    pub team: String,
    pub assignee: Option<String>,
    pub assignee_id: Option<String>,
    /// The assignee's first letters, for where the name has no room.
    pub initials: Option<String>,
    pub labels: Vec<Label>,
    pub updated_at: f64,
}

/// One issue as its tab shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Page {
    pub row: Row,
    pub state: LinearState,
    /// What Linear would call a branch for it.
    pub branch: String,
    pub description: Vec<Text>,
    /// The oldest first.
    pub comments: Vec<Comment>,
    /// What hands the issue to an agent.
    pub prompt: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub initials: String,
    pub at: f64,
    pub body: Vec<Text>,
}

pub fn page(detail: &LinearIssueDetail) -> Page {
    let comment = |comment: &motile_protocol::wire::LinearComment| Comment {
        id: comment.id.clone(),
        author: comment.author.clone(),
        initials: initials(&comment.author),
        at: comment.created_at,
        body: pull_request::text(&comment.body),
    };
    Page {
        row: row(&detail.issue),
        state: detail.issue.state.clone(),
        branch: detail.issue.branch_name.clone(),
        description: pull_request::text(&detail.description),
        comments: detail.comments.iter().map(comment).collect(),
        prompt: prompt(detail),
    }
}

/// The issue in words for an agent: what it is, what it asks for and what was said of it.
fn prompt(detail: &LinearIssueDetail) -> String {
    let issue = &detail.issue;
    let mut prompt = format!("Work on the Linear issue {}, \"{}\": {}", issue.identifier, issue.title, issue.url);
    let description = detail.description.trim();
    if !description.is_empty() {
        prompt.push_str(&format!("\n\nIts description:\n\n{description}"));
    }
    if !detail.comments.is_empty() {
        prompt.push_str("\n\nThe comments on it, the oldest first:");
    }
    for comment in &detail.comments {
        prompt.push_str(&format!("\n\n{}: {}", comment.author, comment.body.trim()));
    }
    prompt
}

/// What is worked on first, then what waits and what is done. In each status the most urgent
/// issues come first, and of those the last updated.
pub fn groups(issues: &[LinearIssue]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for issue in issues {
        let row = row(issue);
        match groups.iter_mut().find(|group| group.state.id == issue.state.id) {
            Some(group) => group.rows.push(row),
            None => groups.push(Group { state: issue.state.clone(), rows: vec![row] }),
        }
    }
    groups.sort_by(|a, b| place(&a.state).partial_cmp(&place(&b.state)).unwrap());
    for group in &mut groups {
        group.rows.sort_by(|a, b| (urgency(a), b.updated_at).partial_cmp(&(urgency(b), a.updated_at)).unwrap());
    }
    groups
}

/// Linear lists the statuses an issue is worked on in from the last to the first, as "In Review"
/// over "In Progress".
fn place(state: &LinearState) -> (u8, f64) {
    match state.kind {
        LinearStateKind::Triage => (0, state.position),
        LinearStateKind::Started => (1, -state.position),
        LinearStateKind::Unstarted => (2, state.position),
        LinearStateKind::Backlog => (3, state.position),
        LinearStateKind::Completed => (4, state.position),
        LinearStateKind::Canceled => (5, state.position),
    }
}

/// Linear's priorities run from 1, urgent, to 4, low, and 0 is none.
fn urgency(row: &Row) -> u8 {
    if row.priority == 0 { u8::MAX } else { row.priority }
}

fn row(issue: &LinearIssue) -> Row {
    Row {
        id: issue.id.clone(),
        identifier: issue.identifier.clone(),
        title: issue.title.clone(),
        url: issue.url.clone(),
        priority: issue.priority,
        priority_label: match issue.priority {
            1 => "Urgent",
            2 => "High priority",
            3 => "Medium priority",
            4 => "Low priority",
            _ => "No priority",
        },
        team: issue.team.clone(),
        assignee: issue.assignee.as_ref().map(|user| user.name.clone()),
        assignee_id: issue.assignee.as_ref().map(|user| user.id.clone()),
        initials: issue.assignee.as_ref().map(|user| initials(&user.name)),
        labels: issue.labels.clone(),
        updated_at: issue.updated_at,
    }
}

fn initials(name: &str) -> String {
    let words = name.split(|character: char| !character.is_alphanumeric()).filter(|word| !word.is_empty());
    words.take(2).filter_map(|word| word.chars().next()).flat_map(char::to_uppercase).collect()
}

#[cfg(test)]
mod tests {
    use motile_protocol::wire::{LinearComment, LinearUser};

    use super::*;

    fn state(name: &str, kind: LinearStateKind, position: f64) -> LinearState {
        LinearState { id: name.to_string(), name: name.to_string(), kind, color: "888888".to_string(), position }
    }

    fn issue(
        identifier: &str,
        state: LinearState,
        priority: u8,
        updated_at: f64,
        assignee: Option<&str>,
    ) -> LinearIssue {
        LinearIssue {
            id: identifier.to_string(),
            identifier: identifier.to_string(),
            title: identifier.to_string(),
            url: String::new(),
            priority,
            state,
            team: "team".to_string(),
            assignee: assignee.map(|name| LinearUser { id: name.to_string(), name: name.to_string(), me: false }),
            labels: Vec::new(),
            branch_name: format!("ada/{}", identifier.to_lowercase()),
            updated_at,
        }
    }

    #[test]
    fn issues_are_listed_under_their_statuses_the_most_urgent_first() {
        let todo = state("Todo", LinearStateKind::Unstarted, 1.0);
        let progress = state("In Progress", LinearStateKind::Started, 2.0);
        let review = state("In Review", LinearStateKind::Started, 3.0);
        let done = state("Done", LinearStateKind::Completed, 4.0);
        let issues = [
            issue("ENG-1", done, 0, 9.0, None),
            issue("ENG-2", todo.clone(), 0, 8.0, None),
            issue("ENG-3", todo.clone(), 3, 1.0, None),
            issue("ENG-4", todo.clone(), 3, 5.0, None),
            issue("ENG-5", todo, 1, 2.0, None),
            issue("ENG-6", progress, 2, 3.0, Some("ada lovelace-king")),
            issue("ENG-7", review, 4, 4.0, Some("ngyekta@gmail.com")),
        ];

        let listed: Vec<(String, Vec<String>)> = groups(&issues)
            .into_iter()
            .map(|group| (group.state.name, group.rows.into_iter().map(|row| row.identifier).collect()))
            .collect();
        let expected = [
            ("In Review", vec!["ENG-7"]),
            ("In Progress", vec!["ENG-6"]),
            ("Todo", vec!["ENG-5", "ENG-4", "ENG-3", "ENG-2"]),
            ("Done", vec!["ENG-1"]),
        ];
        let expected: Vec<(String, Vec<String>)> = expected
            .into_iter()
            .map(|(name, rows)| (name.to_string(), rows.into_iter().map(str::to_string).collect()))
            .collect();
        assert_eq!(listed, expected);

        let rows = groups(&issues);
        assert_eq!(rows[1].rows[0].initials.as_deref(), Some("AL"));
        assert_eq!(rows[0].rows[0].initials.as_deref(), Some("NG"));
        assert_eq!(rows[1].rows[0].priority_label, "High priority");
    }

    #[test]
    fn an_issue_is_handed_to_an_agent_with_its_description_and_comments() {
        let todo = state("Todo", LinearStateKind::Unstarted, 1.0);
        let comment = |author: &str, body: &str| LinearComment {
            id: author.to_string(),
            author: author.to_string(),
            body: body.to_string(),
            created_at: 1.0,
        };
        let mut detail = LinearIssueDetail {
            issue: issue("ENG-7", todo, 2, 1.0, Some("Ada")),
            description: "Say **hello** to whoever runs it.\n".to_string(),
            comments: vec![comment("Grace Hopper", "Which name?"), comment("Ada", "The one given.")],
        };
        detail.issue.url = "https://linear.app/engines/issue/ENG-7".to_string();

        let page = page(&detail);
        let expected = "Work on the Linear issue ENG-7, \"ENG-7\": https://linear.app/engines/issue/ENG-7\n\n\
            Its description:\n\nSay **hello** to whoever runs it.\n\n\
            The comments on it, the oldest first:\n\nGrace Hopper: Which name?\n\nAda: The one given.";
        assert_eq!(page.prompt, expected);
        assert_eq!((page.branch.as_str(), page.comments[0].initials.as_str()), ("ada/eng-7", "GH"));
        assert_eq!(page.description.len(), 1);

        detail.description.clear();
        detail.comments.clear();
        let bare = "Work on the Linear issue ENG-7, \"ENG-7\": https://linear.app/engines/issue/ENG-7";
        assert_eq!(super::page(&detail).prompt, bare);
    }
}
