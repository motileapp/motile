//! Thread titles. A new thread is titled with the start of its first message, and the thread's
//! agent is asked for a better one. When the first message doesn't say what the thread is about,
//! the title is generated again from the transcript once the first turn has ended.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use motile_protocol::wire::{Agent, Item, ItemKind};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::agents::environment::Environment;
use crate::hub::file_name;

pub const UNTITLED: &str = "New thread";
const PLACEHOLDER_CHARS: usize = 50;
const MAX_TITLE_CHARS: usize = 120;
const MAX_PROMPT_CHARS: usize = 8000;
const TIMEOUT: Duration = Duration::from_secs(180);
const RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(4)];
const CLAUDE_MODEL: &str = "claude-haiku-4-5";

const INITIAL_PROMPT: &str = r#"Generate a title that will help the user recognize this Motile thread weeks later.
Return JSON with keys title and needsRefinement.
Set needsRefinement to true only if the subject is still unknown, such as an unresolved link, "fix this", or an unexplained attachment. Otherwise set it to false.

Before answering, silently reduce the request to:
- Subject: What system, feature, or problem is this really about?
- Outcome: What does the user ultimately want to understand or change?
- Incidental instructions: What only describes how the agent should do the work?

Title the subject and outcome. Discard incidental instructions.

Editorial rules:
- 3-8 words, fewer than 40 characters.
- Use a compact noun phrase or clear action phrase.
- Capture the umbrella goal when the request lists several symptoms or steps.
- Name the product change, not the mock, plan, report, branch, or PR used to produce it.
- Models, subagents, tools, output formats, and monitoring instructions do not belong in the title unless they are themselves the topic.
- For reviews, name what is being reviewed and the relevant concern.
- For research, name the question domain rather than the requested research process.
- Do not claim the work is complete.
- Do not copy and truncate the user's message.
- Avoid project names already visible in the UI, quotes, labels, filler, and trailing punctuation."#;

const REGENERATE_RULES: &str = r#"Return JSON with keys title and needsRefinement. Set needsRefinement to false.

Determine the title in this order:
1. Read the USER messages first. Identify the latest explicit durable goal. The original subject remains the subject until the user clearly changes what the thread is about.
2. Use ASSISTANT messages to resolve vague links, unnamed code, and discovered product nouns. Do not promote one assistant finding into the thread subject unless the user adopts it as a new goal.
3. Compare that subject with the previous title. Replace the previous title when it is generic, artifact-based, a completion update, or contradicted by the thread.
4. Title the durable subject and desired outcome, not the current workflow state.

Editorial rules:
- 3-8 words, fewer than 40 characters.
- Use a compact noun phrase or clear action phrase.
- Ignore deliverables and operations such as mocks, plans, branches, PRs, tests, CI, commits, merging, and monitoring unless they are the actual topic.
- Models, subagents, tools, output formats, and monitoring instructions do not belong in the title unless they are themselves the topic.
- Treat final operational follow-ups and assistant completion summaries as weak evidence of subject.
- Do not claim the work is complete.
- Do not copy and truncate a thread message.
- Avoid project names already visible in the UI, PR numbers, quotes, labels, filler, and trailing punctuation.
- Keep the previous title unchanged if it is already accurate."#;

pub struct Generated {
    pub title: String,
    /// The message didn't say what the thread is about; try again once there is a transcript.
    pub needs_refinement: bool,
}

/// The title a thread has until a better one is generated: the start of its first message.
pub fn placeholder(text: &str, attachments: &[String]) -> String {
    let seed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if seed.is_empty() {
        return attachments.first().map_or(UNTITLED.to_string(), |path| format!("File: {}", file_name(path)));
    }
    if seed.chars().count() <= PLACEHOLDER_CHARS {
        return seed;
    }
    let shortened: String = seed.chars().take(PLACEHOLDER_CHARS).collect();
    format!("{}...", shortened.trim_end())
}

pub async fn from_first_message(environment: &Environment, agent: Agent, message: &str) -> Option<Generated> {
    let prompt = format!("{INITIAL_PROMPT}\n\nUser message:\n{}", keep_ends(message));
    generate(environment, agent, &prompt).await
}

pub async fn from_transcript(
    environment: &Environment,
    agent: Agent,
    previous_title: &str,
    items: &[Item],
) -> Option<Generated> {
    let previous = serde_json::to_string(previous_title).unwrap_or_default();
    let prompt = format!(
        "Regenerate the title for an existing Motile thread so the user can recognize it weeks later.\n\
         The previous title was {previous}.\n{REGENERATE_RULES}\n\nThread contents:\n{}",
        keep_end(&transcript(items))
    );
    generate(environment, agent, &prompt).await
}

async fn generate(environment: &Environment, agent: Agent, prompt: &str) -> Option<Generated> {
    for attempt in 0..=RETRY_DELAYS.len() {
        let output = match agent {
            Agent::Claude => ask_claude(environment, prompt).await,
            Agent::Codex => ask_codex(environment, prompt).await,
        };
        match output {
            Ok(generated) => return Some(generated),
            Err(error) => tracing::warn!("couldn't generate a thread title: {error:#}"),
        }
        if let Some(delay) = RETRY_DELAYS.get(attempt) {
            tokio::time::sleep(*delay).await;
        }
    }
    None
}

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": { "title": { "type": "string" }, "needsRefinement": { "type": "boolean" } },
        "required": ["title", "needsRefinement"],
        "additionalProperties": false,
    })
}

async fn ask_claude(environment: &Environment, prompt: &str) -> anyhow::Result<Generated> {
    let executable = environment.executable(Agent::Claude).ok_or_else(|| anyhow::anyhow!("claude isn't installed"))?;
    // The title needs only the prompt, not configuration from a checkout.
    let folder = TempFolder::new("motile-title-")?;
    let mut command = Command::new(executable);
    command.args([
        "-p",
        "--output-format",
        "json",
        "--json-schema",
        &schema().to_string(),
        "--model",
        CLAUDE_MODEL,
        "--settings",
        r#"{"disableAllHooks":true}"#,
        "--tools",
        "",
        "--disable-slash-commands",
        "--strict-mcp-config",
        "--permission-mode",
        "dontAsk",
    ]);
    let output = run(command, environment, folder.path(), prompt).await?;
    let answer: Value = serde_json::from_str(output.trim())?;
    parse(&answer["structured_output"])
        .or_else(|| parse_text(answer["result"].as_str().unwrap_or_default()))
        .ok_or_else(|| anyhow::anyhow!("claude answered without a title"))
}

async fn ask_codex(environment: &Environment, prompt: &str) -> anyhow::Result<Generated> {
    let executable = environment.executable(Agent::Codex).ok_or_else(|| anyhow::anyhow!("codex isn't installed"))?;
    let folder = TempFolder::new("motile-title-")?;
    let schema_file = folder.path().join("schema.json");
    let answer_file = folder.path().join("answer.json");
    std::fs::write(&schema_file, schema().to_string())?;

    let mut command = Command::new(executable);
    command.args(["exec", "--ephemeral", "--skip-git-repo-check", "-s", "read-only"]);
    command.args(["--config", "model_reasoning_effort=\"low\""]);
    if let Some(model) = small_codex_model(environment) {
        command.args(["--model", &model]);
    }
    command.arg("--output-schema").arg(&schema_file).arg("--output-last-message").arg(&answer_file).arg("-");
    run(command, environment, folder.path(), prompt).await?;
    let answer = std::fs::read_to_string(&answer_file)?;
    parse_text(&answer).ok_or_else(|| anyhow::anyhow!("codex answered without a title"))
}

/// The lightest model Codex lists, which is plenty for a title.
fn small_codex_model(environment: &Environment) -> Option<String> {
    let models = environment.models().iter().filter(|model| model.agent == Agent::Codex);
    let small = models.filter(|model| ["luna", "mini", "nano"].iter().any(|hint| model.id.contains(hint)));
    small.map(|model| model.id.clone()).next()
}

async fn run(mut command: Command, environment: &Environment, cwd: &Path, prompt: &str) -> anyhow::Result<String> {
    command
        .current_dir(cwd)
        .env_clear()
        .envs(&environment.variables)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes()).await?;
    }
    let output = tokio::time::timeout(TIMEOUT, child.wait_with_output()).await??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("exited with {}: {}", output.status, stderr.trim().chars().take(400).collect::<String>());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse(answer: &Value) -> Option<Generated> {
    let title = sanitize(answer["title"].as_str()?)?;
    Some(Generated { title, needs_refinement: answer["needsRefinement"].as_bool().unwrap_or(false) })
}

fn parse_text(text: &str) -> Option<Generated> {
    let text = text.trim();
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    parse(&serde_json::from_str(text.get(start..=end)?).ok()?)
}

/// One line, without wrapping quotes, short enough for a sidebar. `None` when nothing is left.
fn sanitize(raw: &str) -> Option<String> {
    let line = raw.trim().lines().next()?.trim();
    let unquoted = line.trim_matches(|character| matches!(character, '\'' | '"' | '`')).trim();
    let title = unquoted.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() || title == UNTITLED {
        return None;
    }
    if title.chars().count() <= MAX_TITLE_CHARS {
        return Some(title);
    }
    let shortened: String = title.chars().take(MAX_TITLE_CHARS - 3).collect();
    Some(format!("{}...", shortened.trim_end()))
}

fn transcript(items: &[Item]) -> String {
    let sections = items.iter().filter_map(|item| match &item.kind {
        ItemKind::User { text, .. } if !text.is_empty() => Some(format!("USER:\n{text}")),
        ItemKind::Assistant { text } if !text.is_empty() => Some(format!("ASSISTANT:\n{text}")),
        _ => None,
    });
    sections.collect::<Vec<_>>().join("\n\n")
}

/// A long message keeps its start and its end, where the request usually is.
fn keep_ends(message: &str) -> String {
    let characters: Vec<char> = message.chars().collect();
    if characters.len() <= MAX_PROMPT_CHARS {
        return message.to_string();
    }
    let half = MAX_PROMPT_CHARS / 2;
    let head: String = characters[..half].iter().collect();
    let tail: String = characters[characters.len() - half..].iter().collect();
    format!("{head}\n[Content truncated]\n{tail}")
}

fn keep_end(contents: &str) -> String {
    let count = contents.chars().count();
    if count <= MAX_PROMPT_CHARS {
        return contents.to_string();
    }
    let tail: String = contents.chars().skip(count - MAX_PROMPT_CHARS).collect();
    format!("[Earlier content truncated]\n{tail}")
}

struct TempFolder(std::path::PathBuf);

impl TempFolder {
    fn new(prefix: &str) -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!("{prefix}{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_is_the_start_of_the_message() {
        assert_eq!(placeholder("\n  Fix the build \nplease", &[]), "Fix the build please");
        assert_eq!(placeholder(&"a".repeat(80), &[]), format!("{}...", "a".repeat(50)));
        assert_eq!(placeholder("", &["/tmp/notes.txt".to_string()]), "File: notes.txt");
        assert_eq!(placeholder("", &[]), "New thread");
    }

    #[test]
    fn titles_are_cleaned_up() {
        assert_eq!(sanitize("  \"Fix Lazy Feed Test\"\nmore").as_deref(), Some("Fix Lazy Feed Test"));
        assert_eq!(sanitize("`Tidy   the  sidebar`").as_deref(), Some("Tidy the sidebar"));
        assert_eq!(sanitize("New thread"), None);
        assert_eq!(sanitize("   "), None);
        assert_eq!(sanitize(&"x".repeat(200)).map(|title| title.chars().count()), Some(120));
    }

    #[test]
    fn title_is_read_from_text_around_the_json() {
        let generated = parse_text("Sure:\n{\"title\": \"Speed Up Sync\", \"needsRefinement\": true}\n").unwrap();
        assert_eq!(generated.title, "Speed Up Sync");
        assert!(generated.needs_refinement);
    }

    #[test]
    fn long_messages_keep_both_ends() {
        let message = format!("{}{}", "a".repeat(6000), "b".repeat(6000));
        let kept = keep_ends(&message);
        assert!(kept.starts_with("aaaa") && kept.ends_with("bbbb") && kept.contains("[Content truncated]"));
        assert_eq!(kept.chars().count(), 8000 + "\n[Content truncated]\n".len());
    }
}
