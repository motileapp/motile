//! Asks an agent's CLI for a short piece of text as JSON: a thread's title, a commit message.
//! The lightest model answers, without tools and outside any checkout.

use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use motile_protocol::wire::{Agent, AgentAccount, ModelInfo, Tokens};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::agents::environment::Environment;
use crate::agents::{ModelUsage, claude};

const TIMEOUT: Duration = Duration::from_secs(180);
const CLAUDE_MODEL: &str = "claude-haiku-4-5";

/// Who writes: an agent's CLI under one of its accounts, with the model the user picked or its
/// lightest one.
#[derive(Clone)]
pub struct Writer {
    pub agent: Agent,
    pub agent_account: String,
    pub model: Option<String>,
    /// The account's environment.
    environment: Environment,
    /// What its answers took, until whoever asked takes it to keep.
    spent: Arc<Mutex<Vec<ModelUsage>>>,
}

impl Writer {
    pub fn new(account: &AgentAccount, model: Option<String>, environment: Environment) -> Self {
        Self { agent: account.agent, agent_account: account.id.clone(), model, environment, spent: Arc::default() }
    }

    pub fn take_spent(&self) -> Vec<ModelUsage> {
        std::mem::take(&mut self.spent.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))
    }

    fn spend(&self, spent: impl IntoIterator<Item = ModelUsage>) {
        self.spent.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).extend(spent);
    }
}

/// The writer's answer to the prompt, in the shape of the JSON schema.
pub async fn ask(writer: &Writer, prompt: &str, schema: &Value) -> anyhow::Result<Value> {
    match writer.agent {
        Agent::Claude => ask_claude(writer, prompt, schema).await,
        Agent::Codex => ask_codex(writer, prompt, schema).await,
    }
}

async fn ask_claude(writer: &Writer, prompt: &str, schema: &Value) -> anyhow::Result<Value> {
    let environment = &writer.environment;
    let executable = environment.executable(Agent::Claude).ok_or_else(|| anyhow::anyhow!("claude isn't installed"))?;
    let folder = TempFolder::new("motile-ask-")?;
    let mut command = Command::new(executable);
    command.args([
        "-p",
        "--output-format",
        "json",
        "--json-schema",
        &schema.to_string(),
        "--model",
        writer.model.as_deref().unwrap_or(CLAUDE_MODEL),
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
    writer.spend(claude::model_usage(&answer["modelUsage"]));
    if answer["structured_output"].is_object() {
        return Ok(answer["structured_output"].clone());
    }
    json_in(answer["result"].as_str().unwrap_or_default())
        .ok_or_else(|| anyhow::anyhow!("claude answered without JSON"))
}

async fn ask_codex(writer: &Writer, prompt: &str, schema: &Value) -> anyhow::Result<Value> {
    let environment = &writer.environment;
    let executable = environment.executable(Agent::Codex).ok_or_else(|| anyhow::anyhow!("codex isn't installed"))?;
    let folder = TempFolder::new("motile-ask-")?;
    let schema_file = folder.path().join("schema.json");
    let answer_file = folder.path().join("answer.json");
    std::fs::write(&schema_file, schema.to_string())?;

    let mut command = Command::new(executable);
    command.args(["exec", "--ephemeral", "--skip-git-repo-check", "-s", "read-only", "--json"]);
    command.args(["--config", "model_reasoning_effort=\"low\""]);
    if let Some(model) = &writer.model {
        command.args(["--model", model]);
    }
    command.arg("--output-schema").arg(&schema_file).arg("--output-last-message").arg(&answer_file).arg("-");
    let events = run(command, environment, folder.path(), prompt).await?;
    writer.spend(codex_usage(&events, writer.model.clone().unwrap_or_default()));
    let answer = std::fs::read_to_string(&answer_file)?;
    json_in(&answer).ok_or_else(|| anyhow::anyhow!("codex answered without JSON"))
}

/// What `codex exec --json` says its turn took. It counts what it read from the cache and wrote
/// to it as input too.
fn codex_usage(events: &str, model: String) -> Option<ModelUsage> {
    let mut events = events.lines().filter_map(|line| serde_json::from_str::<Value>(line).ok());
    let usage = events.rfind(|event| event["type"] == "turn.completed")?["usage"].clone();
    let count = |name: &str| usage[name].as_u64().unwrap_or_default();
    let (cache_read, cache_write) = (count("cached_input_tokens"), count("cache_write_input_tokens"));
    let tokens = Tokens {
        input: count("input_tokens").saturating_sub(cache_read + cache_write),
        cache_read,
        cache_write,
        output: count("output_tokens"),
    };
    Some(ModelUsage { model, tokens, cost_usd: None })
}

/// The lightest model Codex lists for the account, which is plenty for a title.
pub fn small_codex_model(models: &[ModelInfo], account: &AgentAccount) -> Option<String> {
    let listed = models.iter().filter(|model| model.agent == Agent::Codex && model.account == account.id);
    let small = listed.filter(|model| ["luna", "mini", "nano"].iter().any(|hint| model.id.contains(hint)));
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

/// The JSON object in an answer that may have words around it.
fn json_in(text: &str) -> Option<Value> {
    let text = text.trim();
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str(text.get(start..=end)?).ok()
}

/// `text` cut to at most `limit` characters, saying so when it was.
pub fn capped(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((end, _)) => format!("{}\n[Truncated]", &text[..end]),
        None => text.to_string(),
    }
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
    fn what_codex_says_a_turn_took_is_read_without_the_cache_counted_twice() {
        let events = r#"{"type":"thread.started","thread_id":"t"}
not json
{"type":"turn.completed","usage":{"input_tokens":14211,"cached_input_tokens":12000,"cache_write_input_tokens":11,"output_tokens":19,"reasoning_output_tokens":0}}"#;
        let spent = codex_usage(events, "gpt-6-luna".to_string()).unwrap();
        assert_eq!(spent.model, "gpt-6-luna");
        assert_eq!(spent.tokens, Tokens { input: 2200, cache_read: 12000, cache_write: 11, output: 19 });
        assert_eq!(codex_usage(r#"{"type":"turn.started"}"#, String::new()), None);
    }

    #[test]
    fn json_is_read_from_text_around_it() {
        let answer = json_in("Sure:\n{\"title\": \"Speed Up Sync\", \"needsRefinement\": true}\n").unwrap();
        assert_eq!(answer["title"], "Speed Up Sync");
        assert_eq!(answer["needsRefinement"], true);
        assert_eq!(json_in("no json here"), None);
    }

    #[test]
    fn long_text_is_cut_and_says_so() {
        assert_eq!(capped("héllo", 5), "héllo");
        assert_eq!(capped("héllo", 2), "hé\n[Truncated]");
    }
}
