//! Commit messages, pull request texts and branch names, written from the changes by an agent.

use std::path::Path;

use anyhow::Context;
use serde_json::{Value, json};

use crate::agents::environment::Environment;
use crate::generate::{self, Writer, capped};
use crate::git;

const MAX_LIST_CHARS: usize = 6000;
const MAX_PATCH_CHARS: usize = 40_000;
const MAX_TEMPLATE_CHARS: usize = 8000;
const TEMPLATES: [&str; 4] = [
    ".github/pull_request_template.md",
    ".github/PULL_REQUEST_TEMPLATE.md",
    "docs/pull_request_template.md",
    "pull_request_template.md",
];

const COMMIT_PROMPT: &str = r#"Write the git commit message for the changes below.
Return JSON with keys subject, body and branch.

- subject: what the change does, in the imperative, at most 72 characters, without a trailing period. Word it the way the repository's recent commits are worded, including any prefix they share.
- body: an empty string unless the change needs explaining. Then a few short plain sentences or bullet points about what changed and why, not a list of files.
- branch: a git branch name for this work, named the way it is said below.
- Do not mention yourself, an AI or the tools used, and add no trailers."#;

const PULL_REQUEST_PROMPT: &str = r#"Write the title and the description of a pull request for the branch below.
Return JSON with keys title and body.

- title: what the branch changes, at most 72 characters, without a trailing period. Word it the way the repository's recent commits are worded.
- body: Markdown. Say what changes and why in a few short sentences or bullet points, then how it was tested if the commits say so. If a template is given, fill it in instead.
- Do not mention yourself, an AI or the tools used."#;

const BRANCH_PROMPT: &str = "Name the git branch for the work the message below asks for.
Return JSON with key branch.";

/// How the writer names a branch until the user says otherwise.
pub const BRANCH_INSTRUCTIONS: &str = "Name the branch after the work, in 2 to 5 lowercase words joined by hyphens.
Start it with motile/, as in motile/fix-login-redirect.";

const MAX_MESSAGE_CHARS: usize = 8000;
const MAX_BRANCH_CHARS: usize = 80;

/// What the thread the work was done in is about, to say why the change was made.
#[derive(Default)]
pub struct Thread {
    pub title: String,
    pub messages: String,
}

pub struct CommitDraft {
    pub subject: String,
    pub body: String,
    /// A name for a branch for the work.
    pub branch: Option<String>,
}

impl CommitDraft {
    pub fn message(&self) -> String {
        if self.body.is_empty() {
            return self.subject.clone();
        }
        format!("{}\n\n{}", self.subject, self.body)
    }
}

pub async fn commit_message(
    folder: &str,
    environment: &Environment,
    writer: &Writer,
    thread: &Thread,
    paths: &[String],
    branch_instructions: &str,
) -> anyhow::Result<CommitDraft> {
    let (names, patch) = git::pending_changes(folder, environment, paths).await?;
    if names.trim().is_empty() {
        anyhow::bail!("There is nothing to commit.");
    }
    let prompt = format!(
        "{COMMIT_PROMPT}{}{}{}\n\nChanged files:\n{}\n\nPatch:\n{}",
        section("How to name the branch", branch_instructions),
        section("Recent commits", &git::recent_subjects(folder, environment).await),
        about(thread),
        capped(&names, MAX_LIST_CHARS),
        capped(&patch, MAX_PATCH_CHARS)
    );
    let schema = schema(&["subject", "body", "branch"]);
    let answer = generate::ask(writer, &prompt, &schema).await.context("Couldn't write the commit message.")?;
    let subject = first_line(answer["subject"].as_str().unwrap_or_default());
    if subject.is_empty() {
        anyhow::bail!("The commit message came back empty.");
    }
    let body = answer["body"].as_str().unwrap_or_default().trim().to_string();
    Ok(CommitDraft { subject, body, branch: branch_name(answer["branch"].as_str().unwrap_or_default()) })
}

/// A name for the branch of the work the thread's first message asks for.
pub async fn branch_for(writer: &Writer, instructions: &str, message: &str) -> anyhow::Result<String> {
    let prompt = format!("{BRANCH_PROMPT}\n\n{instructions}\n\nUser message:\n{}", capped(message, MAX_MESSAGE_CHARS));
    let answer = generate::ask(writer, &prompt, &schema(&["branch"])).await?;
    branch_name(answer["branch"].as_str().unwrap_or_default()).context("The branch's name came back empty.")
}

/// The title and the text of a pull request for the branch.
pub async fn pull_request(
    folder: &str,
    environment: &Environment,
    writer: &Writer,
    thread: &Thread,
) -> anyhow::Result<(String, String)> {
    let (commits, files, patch) = git::branch_changes(folder, environment).await?;
    if commits.trim().is_empty() {
        anyhow::bail!("This branch has no commits to open a pull request for.");
    }
    let template = TEMPLATES.iter().find_map(|name| std::fs::read_to_string(Path::new(folder).join(name)).ok());
    let prompt = format!(
        "{PULL_REQUEST_PROMPT}{}{}{}\n\nCommits:\n{}\n\nChanged files:\n{}\n\nPatch:\n{}",
        section("Recent commits", &git::recent_subjects(folder, environment).await),
        about(thread),
        section("Template", &capped(&template.unwrap_or_default(), MAX_TEMPLATE_CHARS)),
        capped(&commits, MAX_LIST_CHARS),
        capped(&files, MAX_LIST_CHARS),
        capped(&patch, MAX_PATCH_CHARS)
    );
    let answer = generate::ask(writer, &prompt, &schema(&["title", "body"])).await;
    let answer = answer.context("Couldn't write the pull request.")?;
    let title = first_line(answer["title"].as_str().unwrap_or_default());
    if title.is_empty() {
        anyhow::bail!("The pull request's title came back empty.");
    }
    Ok((title, answer["body"].as_str().unwrap_or_default().trim().to_string()))
}

fn schema(keys: &[&str]) -> Value {
    let properties: serde_json::Map<String, Value> =
        keys.iter().map(|key| (key.to_string(), json!({ "type": "string" }))).collect();
    json!({ "type": "object", "properties": properties, "required": keys, "additionalProperties": false })
}

fn section(name: &str, contents: &str) -> String {
    if contents.trim().is_empty() {
        return String::new();
    }
    format!("\n\n{name}:\n{}", contents.trim())
}

fn about(thread: &Thread) -> String {
    if thread.messages.is_empty() {
        return String::new();
    }
    let request = format!("Title: {}\n{}", thread.title, capped(&thread.messages, MAX_LIST_CHARS));
    section("The thread the work was asked for in", &request)
}

fn first_line(text: &str) -> String {
    text.trim().lines().next().unwrap_or_default().trim().trim_end_matches('.').to_string()
}

/// A name git accepts for a branch, from what the agent suggested. `None` when nothing is left.
pub fn branch_name(suggested: &str) -> Option<String> {
    let allowed = |character: char| character.is_ascii_alphanumeric() || "-_/.".contains(character);
    let kept: String = suggested.trim().to_lowercase().replace(' ', "-").chars().filter(|c| allowed(*c)).collect();
    let kept: String = kept.chars().take(MAX_BRANCH_CHARS).collect();
    let name = kept.trim_matches(|character| "-/.".contains(character)).replace("..", ".").replace("//", "/");
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_suggested_branch_becomes_a_name_git_accepts() {
        assert_eq!(branch_name(" Fix Login Redirect ").as_deref(), Some("fix-login-redirect"));
        assert_eq!(branch_name("-feature/pay~ments.").as_deref(), Some("feature/payments"));
        assert_eq!(branch_name("?!"), None);
        assert_eq!(branch_name(&"a".repeat(200)).map(|name| name.len()), Some(MAX_BRANCH_CHARS));
    }

    #[test]
    fn a_subject_is_one_line_without_its_period() {
        assert_eq!(first_line("  Stream replies in blocks.\nmore"), "Stream replies in blocks");
        assert_eq!(first_line(""), "");
    }
}
