//! The models each agent can run. Claude Code has no way to list its models, so they are written
//! down here; Codex keeps its own list on disk, which is read instead.

use std::path::Path;

use motile_protocol::wire::{Agent, ModelInfo};
use serde::Deserialize;

const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

fn claude(id: &str, name: &str, default_effort: Option<&str>) -> ModelInfo {
    ModelInfo {
        id: id.to_string(),
        name: name.to_string(),
        agent: Agent::Claude,
        efforts: default_effort.map(|_| EFFORTS.map(String::from).to_vec()).unwrap_or_default(),
        default_effort: default_effort.map(String::from),
    }
}

pub fn claude_models() -> Vec<ModelInfo> {
    vec![
        claude("claude-opus-5-5", "Claude Opus 5.5", Some("high")),
        claude("claude-sonnet-5-5", "Claude Sonnet 5.5", Some("high")),
        claude("claude-fable-5-1", "Claude Fable 5.1", Some("medium")),
        claude("claude-haiku-4-5", "Claude Haiku 4.5", None),
    ]
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

/// The models Codex lists in its own picker, from `<home>/.codex/models_cache.json`.
pub fn codex_models(home: &Path) -> Vec<ModelInfo> {
    let text = std::fs::read_to_string(home.join(".codex/models_cache.json")).unwrap_or_default();
    let Ok(mut cache) = serde_json::from_str::<CodexCache>(&text) else { return Vec::new() };
    cache.models.retain(|model| model.visibility == "list");
    cache.models.sort_by_key(|model| model.priority);
    let models = cache.models.into_iter().map(|model| ModelInfo {
        id: model.slug,
        name: model.display_name,
        agent: Agent::Codex,
        efforts: model.supported_reasoning_levels.into_iter().map(|level| level.effort).collect(),
        default_effort: model.default_reasoning_level,
    });
    models.collect()
}
