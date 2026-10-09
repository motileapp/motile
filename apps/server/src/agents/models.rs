//! The models each agent's accounts can run. Claude Code is asked for what its model picker
//! lists, which depends on who is signed in; Codex keeps its own list on disk, which is read.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use motile_protocol::wire::{Agent, ModelInfo};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use super::environment::Environment;

const LIST_WITHIN: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeModel {
    /// What its picker sends: an alias like `opus`, or an id.
    value: String,
    resolved_model: Option<String>,
    #[serde(default)]
    supported_effort_levels: Vec<String>,
}

/// What Claude Code lists under the account of `environment`, asked through the control request
/// the Agent SDK uses. `None` when it doesn't answer, as one too old to does.
pub async fn claude_models(environment: &Environment) -> Option<Vec<ModelInfo>> {
    let executable = environment.executable(Agent::Claude)?;
    let mut command = Command::new(executable);
    command.args(["-p", "--bare", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"]);
    command.env_clear().envs(&environment.variables);
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    let mut child = command.spawn().ok()?;
    let request = json!({"type": "control_request", "request_id": "models", "request": {"subtype": "list_models"}});
    let mut stdin = child.stdin.take()?;
    stdin.write_all(format!("{request}\n").as_bytes()).await.ok()?;
    drop(stdin);
    let mut lines = BufReader::new(child.stdout.take()?).lines();
    let answer = tokio::time::timeout(LIST_WITHIN, async {
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(mut event) = serde_json::from_str::<Value>(&line) else { continue };
            if event["type"] == "control_response" {
                return Some(event["response"]["response"]["models"].take());
            }
        }
        None
    });
    let listed = answer.await.ok()??;
    let models: Vec<ClaudeModel> = serde_json::from_value(listed).ok()?;
    Some(claude_list(models))
}

fn claude_list(models: Vec<ClaudeModel>) -> Vec<ModelInfo> {
    let mut listed: Vec<ModelInfo> = Vec::new();
    for model in models {
        if model.value == "default" {
            continue;
        }
        let id = model.resolved_model.unwrap_or(model.value);
        if listed.iter().any(|known| known.id == id) {
            continue;
        }
        let mut efforts = model.supported_effort_levels;
        if efforts.iter().any(|effort| effort == "xhigh") {
            efforts.push(super::claude::ULTRACODE.to_string());
        }
        listed.push(ModelInfo {
            name: claude_name(&id),
            id,
            agent: Agent::Claude,
            account: String::new(),
            efforts,
            default_effort: None,
        });
    }
    listed
}

/// "Claude Opus 5.5" for `claude-opus-5-5`, with a date like `20251001` at the end left off.
fn claude_name(id: &str) -> String {
    let mut parts = id.strip_prefix("claude-").unwrap_or(id).split('-');
    let family = parts.next().unwrap_or_default();
    let mut name = String::from("Claude ");
    name.extend(family.chars().next().map(|first| first.to_ascii_uppercase()));
    name.extend(family.chars().skip(1));
    let version: Vec<&str> =
        parts.filter(|part| part.len() < 8 && part.bytes().all(|byte| byte.is_ascii_digit())).collect();
    if !version.is_empty() {
        name.push(' ');
        name.push_str(&version.join("."));
    }
    name
}

#[derive(Deserialize)]
struct CodexCache {
    models: Vec<CodexModel>,
}

#[derive(Deserialize)]
struct CodexModel {
    slug: String,
    display_name: String,
    #[serde(default)]
    visibility: String,
    #[serde(default)]
    priority: i64,
    default_reasoning_level: Option<String>,
    #[serde(default)]
    supported_reasoning_levels: Vec<CodexEffort>,
}

#[derive(Deserialize)]
struct CodexEffort {
    effort: String,
}

/// The folder Codex keeps the account of `environment` in: `CODEX_HOME`, or `~/.codex`.
pub fn codex_home(environment: &Environment) -> PathBuf {
    if let Some(home) = environment.variables.get("CODEX_HOME").filter(|home| !home.is_empty()) {
        return PathBuf::from(home);
    }
    Path::new(environment.variables.get("HOME").map(String::as_str).unwrap_or_default()).join(".codex")
}

/// The models Codex lists in its own picker, from `models_cache.json` in its folder.
pub fn codex_models(codex_home: &Path) -> Vec<ModelInfo> {
    let text = std::fs::read_to_string(codex_home.join("models_cache.json")).unwrap_or_default();
    let Ok(mut cache) = serde_json::from_str::<CodexCache>(&text) else { return Vec::new() };
    cache.models.retain(|model| model.visibility == "list");
    cache.models.sort_by_key(|model| model.priority);
    let models = cache.models.into_iter().map(|model| ModelInfo {
        id: model.slug,
        name: model.display_name,
        agent: Agent::Codex,
        account: String::new(),
        efforts: model.supported_reasoning_levels.into_iter().map(|level| level.effort).collect(),
        default_effort: model.default_reasoning_level,
    });
    models.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_models_are_named_after_their_ids() {
        assert_eq!(claude_name("claude-opus-5-5"), "Claude Opus 5.5");
        assert_eq!(claude_name("claude-fable-5-1"), "Claude Fable 5.1");
        assert_eq!(claude_name("claude-haiku-4-5-20251001"), "Claude Haiku 4.5");
        assert_eq!(claude_name("claude-opus-4-1"), "Claude Opus 4.1");
    }

    #[test]
    fn the_picker_list_leaves_out_the_default_row_and_repeats() {
        let listed = serde_json::from_value(json!([
            {"value": "default", "resolvedModel": "claude-opus-5-5", "displayName": "Default (recommended)",
             "supportedEffortLevels": ["low", "high"]},
            {"value": "opus", "resolvedModel": "claude-opus-5-5", "displayName": "Opus",
             "supportedEffortLevels": ["low", "high"]},
            {"value": "claude-fable-5-1[1m]", "resolvedModel": "claude-fable-5-1", "displayName": "Fable",
             "supportedEffortLevels": ["low", "xhigh", "max"]},
            {"value": "haiku", "resolvedModel": "claude-haiku-4-5-20251001", "displayName": "Haiku"},
        ]))
        .unwrap();
        let models = claude_list(listed);
        let ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
        assert_eq!(ids, ["claude-opus-5-5", "claude-fable-5-1", "claude-haiku-4-5-20251001"]);
        assert_eq!(models[0].name, "Claude Opus 5.5");
        assert_eq!(models[0].efforts, ["low", "high"], "ultracode needs xhigh");
        assert_eq!(models[1].efforts, ["low", "xhigh", "max", "ultracode"]);
        assert!(models[2].efforts.is_empty());
        assert!(models.iter().all(|model| model.default_effort.is_none() && model.agent == Agent::Claude));
    }
}
