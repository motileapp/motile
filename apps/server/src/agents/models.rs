//! The models each agent's accounts can run, and the effort each runs at when it isn't given one.
//! Claude Code is asked for what its model picker lists, which depends on who is signed in, and
//! for the settings it would apply to each; `codex app-server` for its list and its config.

use motile_protocol::wire::{Agent, ModelInfo};
use serde::Deserialize;
use serde_json::{Value, json};

use super::environment::Environment;
use super::{ask, claude};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeModel {
    /// What its picker sends: an alias like `opus`, or an id.
    value: String,
    resolved_model: Option<String>,
    #[serde(default)]
    supported_effort_levels: Vec<String>,
}

/// What Claude Code lists under the account of `environment`, asked through the control requests
/// the Agent SDK uses. `None` when it doesn't answer, as one too old to does.
pub async fn claude_models(environment: &Environment) -> Option<Vec<ModelInfo>> {
    let mut answers = ask::claude(environment, &["--bare"], &["list_models"]).await.ok()?;
    let listed = answers.remove("list_models")?.ok()?;
    let mut models = claude_list(serde_json::from_value(listed["models"].clone()).ok()?);
    for model in &mut models {
        model.default_effort = claude_default_effort(environment, model).await;
    }
    Some(models)
}

/// The effort Claude Code runs `model` at when it isn't given one: the model's own, or what the
/// account's settings say. Only a process started with the model reports it.
async fn claude_default_effort(environment: &Environment, model: &ModelInfo) -> Option<String> {
    if model.efforts.is_empty() {
        return None;
    }
    let arguments = ["--bare", "--model", model.id.as_str()];
    let mut answers = ask::claude(environment, &arguments, &["get_settings"]).await.ok()?;
    let settings = answers.remove("get_settings")?.ok()?;
    applied_effort(&settings["applied"], &model.efforts)
}

fn applied_effort(applied: &Value, efforts: &[String]) -> Option<String> {
    let ultracode = (applied["ultracode"] == true).then_some(claude::ULTRACODE);
    let mut wanted = [ultracode, applied["effort"].as_str()].into_iter().flatten();
    wanted.find(|effort| efforts.iter().any(|offered| offered == effort)).map(String::from)
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
#[serde(rename_all = "camelCase")]
struct CodexModel {
    model: String,
    display_name: String,
    #[serde(default)]
    hidden: bool,
    default_reasoning_effort: Option<String>,
    #[serde(default)]
    supported_reasoning_efforts: Vec<CodexEffort>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexEffort {
    reasoning_effort: String,
}

/// What Codex lists in its own picker under the account of `environment`. `None` when it doesn't
/// answer.
pub async fn codex_models(environment: &Environment) -> Option<Vec<ModelInfo>> {
    let requests = [("model/list", json!({})), ("config/read", json!({}))];
    let mut answers = ask::codex(environment, &requests).await.ok()?;
    let listed = answers.remove("model/list")?.ok()?;
    let config = answers.remove("config/read").and_then(Result::ok).unwrap_or_default();
    let models = serde_json::from_value(listed["data"].clone()).ok()?;
    Some(codex_list(models, config["config"]["model_reasoning_effort"].as_str()))
}

/// The models to offer, each defaulting to the effort the account's config sets where it takes
/// that one, and to its own otherwise.
fn codex_list(models: Vec<CodexModel>, configured: Option<&str>) -> Vec<ModelInfo> {
    let shown = models.into_iter().filter(|model| !model.hidden);
    let models = shown.map(|model| {
        let efforts: Vec<String> =
            model.supported_reasoning_efforts.into_iter().map(|level| level.reasoning_effort).collect();
        let configured = configured.filter(|effort| efforts.iter().any(|offered| offered == effort));
        ModelInfo {
            id: model.model,
            name: model.display_name,
            agent: Agent::Codex,
            account: String::new(),
            default_effort: configured.map(String::from).or(model.default_reasoning_effort),
            efforts,
        }
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

    #[test]
    fn claude_code_s_default_effort_is_the_one_it_applies_where_the_model_takes_it() {
        let efforts: Vec<String> = ["low", "medium", "xhigh", "ultracode"].map(String::from).into();
        let applied = |value: Value| applied_effort(&value, &efforts);
        assert_eq!(applied(json!({"effort": "medium", "ultracode": false})).as_deref(), Some("medium"));
        assert_eq!(applied(json!({"effort": "xhigh", "ultracode": true})).as_deref(), Some("ultracode"));
        assert_eq!(applied(json!({"effort": "max", "ultracode": false})), None, "not one it offers");
        assert_eq!(
            applied_effort(&json!({"effort": "xhigh", "ultracode": true}), &efforts[..3]).as_deref(),
            Some("xhigh")
        );
    }

    #[test]
    fn codex_defaults_to_the_effort_its_config_sets_where_the_model_takes_it() {
        let listed = json!([
            {"id": "gpt-6.1-sol", "model": "gpt-6.1-sol", "displayName": "GPT-6.1-Sol", "hidden": false,
             "defaultReasoningEffort": "low",
             "supportedReasoningEfforts": [{"reasoningEffort": "low"}, {"reasoningEffort": "xhigh"}]},
            {"id": "gpt-6-luna", "model": "gpt-6-luna", "displayName": "GPT-6-Luna", "hidden": false,
             "defaultReasoningEffort": "medium",
             "supportedReasoningEfforts": [{"reasoningEffort": "medium"}, {"reasoningEffort": "high"}]},
            {"id": "codex-auto-review", "model": "codex-auto-review", "displayName": "Codex Auto Review", "hidden": true},
        ]);
        let models = |configured| codex_list(serde_json::from_value(listed.clone()).unwrap(), configured);
        let defaults = |configured| {
            let models = models(configured).into_iter();
            models.map(|model| format!("{} {}", model.id, model.default_effort.unwrap_or_default())).collect::<Vec<_>>()
        };
        assert_eq!(defaults(None), ["gpt-6.1-sol low", "gpt-6-luna medium"]);
        assert_eq!(defaults(Some("xhigh")), ["gpt-6.1-sol xhigh", "gpt-6-luna medium"], "where the model takes it");
        let first = &models(None)[0];
        assert_eq!((first.name.as_str(), first.efforts.join(" ")), ("GPT-6.1-Sol", "low xhigh".to_string()));
    }
}
