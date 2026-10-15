//! The skills an agent's account can run in a project, as its CLI lists them, which spends no
//! tokens: Claude Code answers `initialize` with its commands, where its own are `builtin`, and
//! `codex app-server` answers `skills/list`. A message starts one by its `$name`; Codex reads
//! that itself, and Claude Code only runs a command that starts the last part of the message.

use std::path::Path;

use motile_protocol::skills::mentions;
use motile_protocol::wire::{Skill, SkillMention, SkillSource};
use serde::Deserialize;
use serde_json::{Value, json};

use super::ask;
use super::environment::Environment;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeCommand {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    builtin: bool,
}

const WORKFLOW: &str = "(dynamic workflow)";

/// What Claude Code lists in `folder` besides its own commands. `None` when it doesn't answer.
pub async fn claude_skills(environment: &Environment, folder: &Path) -> Option<Vec<Skill>> {
    let arguments = ["--strict-mcp-config"];
    let mut answers = ask::claude_in(environment, folder, &arguments, &["initialize"]).await.ok()?;
    let commands = answers.remove("initialize")?.ok()?["commands"].take();
    let commands: Vec<ClaudeCommand> = serde_json::from_value(commands).ok()?;
    Some(commands.into_iter().filter_map(claude_skill).collect())
}

/// A command as the composer offers it, by the shortest name it answers to, unless Claude Code
/// has it built in or it is a plugin's workflow, which the plugin's skills start.
fn claude_skill(command: ClaudeCommand) -> Option<Skill> {
    let plugin_workflow = command.name.contains(':') && command.description.trim_end().ends_with(WORKFLOW);
    if command.builtin || plugin_workflow {
        return None;
    }
    let (description, source) = claude_source(&command.description);
    let description = description.to_string();
    let name = [command.name].into_iter().chain(command.aliases).min_by_key(String::len)?;
    Some(Skill { label: name.clone(), description, source, name })
}

/// Claude Code ends a description with where the command comes from: "(user)", "(project)" or
/// the plugin's name, or starts it with the plugin's name.
fn claude_source(description: &str) -> (&str, SkillSource) {
    let trimmed = description.trim();
    let Some((description, from)) = trimmed.strip_suffix(')').and_then(|rest| rest.rsplit_once('(')) else {
        let unprefixed = trimmed.strip_prefix('(').and_then(|rest| rest.split_once(") ")).map(|(_, rest)| rest);
        return (unprefixed.unwrap_or(trimmed), SkillSource::Plugin);
    };
    let source = match from {
        "user" => SkillSource::Personal,
        "project" => SkillSource::Project,
        _ => SkillSource::Plugin,
    };
    (description.trim_end(), source)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexSkill {
    name: String,
    #[serde(default)]
    description: String,
    short_description: Option<String>,
    interface: Option<CodexInterface>,
    scope: Option<String>,
    #[serde(default = "enabled")]
    enabled: bool,
    plugin_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexInterface {
    display_name: Option<String>,
    short_description: Option<String>,
}

fn enabled() -> bool {
    true
}

/// What Codex offers in `folder`, leaving out what its config turned off. `None` when it doesn't
/// answer.
pub async fn codex_skills(environment: &Environment, folder: &Path) -> Option<Vec<Skill>> {
    let requests = [("skills/list", json!({"cwds": [folder]}))];
    let mut answers = ask::codex(environment, &requests).await.ok()?;
    let listed = answers.remove("skills/list")?.ok()?;
    let skills: Vec<CodexSkill> = serde_json::from_value(listed["data"][0]["skills"].clone()).ok()?;
    Some(skills.into_iter().filter(|skill| skill.enabled).map(codex_skill).collect())
}

fn codex_skill(skill: CodexSkill) -> Skill {
    let source = match skill.scope.as_deref() {
        _ if skill.plugin_id.is_some() => SkillSource::Plugin,
        Some("repo") => SkillSource::Project,
        Some("user") => SkillSource::Personal,
        _ => SkillSource::System,
    };
    let (display_name, interface_description) =
        skill.interface.map(|interface| (interface.display_name, interface.short_description)).unwrap_or_default();
    let short = skill.short_description.or(interface_description);
    Skill {
        label: display_name.unwrap_or_else(|| skill.name.clone()),
        description: short.unwrap_or(skill.description),
        name: skill.name,
        source,
    }
}

/// The skills of `skills` that `text` starts, in the order it names them, each once.
pub fn mentioned(text: &str, skills: &[Skill]) -> Vec<SkillMention> {
    let mut found: Vec<SkillMention> = Vec::new();
    for (_, name) in mentions(text) {
        let Some(skill) = skills.iter().find(|skill| skill.name == name) else { continue };
        if found.iter().all(|mention| mention.name != name) {
            found.push(SkillMention { name: skill.name.clone(), label: skill.label.clone() });
        }
    }
    found
}

/// What Claude Code is given for `prompt`: the last skill it names starts the last part, as the
/// command Claude Code runs with the rest as its arguments, and the ones before are named as
/// commands for the model to run.
pub fn claude_content(prompt: &str, skills: &[SkillMention]) -> Value {
    let named: Vec<_> =
        mentions(prompt).into_iter().filter(|(_, name)| skills.iter().any(|skill| skill.name == *name)).collect();
    let Some((last, _)) = named.last() else { return json!(prompt) };
    let mut before = prompt[..last.start].to_string();
    for (range, _) in named.iter().rev().skip(1) {
        before.replace_range(range.start..range.start + 1, "/");
    }
    let command = format!("/{}", &prompt[last.start + 1..]);
    let before = before.trim();
    let blocks = [before, command.trim()].into_iter().filter(|text| !text.is_empty());
    Value::Array(blocks.map(|text| json!({"type": "text", "text": text})).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(name: &str) -> Skill {
        Skill { name: name.into(), label: name.into(), description: String::new(), source: SkillSource::Project }
    }

    fn mention(name: &str) -> SkillMention {
        SkillMention { name: name.into(), label: name.into() }
    }

    #[test]
    fn a_message_starts_the_skills_it_names_that_the_agent_has() {
        let skills = [skill("ship-it"), skill("review")];
        let found = mentioned("$review then $ship-it, not $HOME, and $review again", &skills);
        assert_eq!(found.iter().map(|mention| mention.name.as_str()).collect::<Vec<_>>(), ["review", "ship-it"]);
    }

    #[test]
    fn claude_code_runs_the_last_skill_with_the_rest_and_is_told_of_the_others() {
        let skills = [mention("review"), mention("ship-it")];
        assert_eq!(
            claude_content("first $review this, then $ship-it all of it", &skills),
            json!([{"type": "text", "text": "first /review this, then"}, {"type": "text", "text": "/ship-it all of it"}])
        );
        assert_eq!(claude_content("$ship-it", &skills), json!([{"type": "text", "text": "/ship-it"}]));
        assert_eq!(claude_content("costs $HOME", &skills), json!("costs $HOME"));
    }

    #[test]
    fn claude_code_says_where_a_skill_comes_from_at_the_end_of_its_description() {
        assert_eq!(claude_source("Commit and push (project)"), ("Commit and push", SkillSource::Project));
        assert_eq!(claude_source("Mine (user)"), ("Mine", SkillSource::Personal));
        assert_eq!(claude_source("Edit documents (claude.ai sync)"), ("Edit documents", SkillSource::Plugin));
        assert_eq!(claude_source("Plain"), ("Plain", SkillSource::Plugin));
        assert_eq!(claude_source("(claude-security) Scan code"), ("Scan code", SkillSource::Plugin));
    }

    #[test]
    fn claude_code_s_commands_are_offered_by_their_shortest_name_without_plugins_workflows() {
        let listed = json!([
            {"name": "claude-security:claude-security", "description": "(claude-security) Scan code", "aliases": ["claude-security"]},
            {"name": "claude-security:scan", "description": "Scan pipeline (dynamic workflow)"},
            {"name": "mine", "description": "My own workflow (dynamic workflow)"},
            {"name": "docs", "description": "Edit docs (claude.ai sync)", "aliases": ["anthropic-skills:docs"]},
            {"name": "clear", "description": "Start over", "aliases": ["new"], "builtin": true},
        ]);
        let commands: Vec<ClaudeCommand> = serde_json::from_value(listed).unwrap();
        let offered: Vec<_> = commands
            .into_iter()
            .filter_map(claude_skill)
            .map(|skill| (skill.name, skill.label, skill.description))
            .collect();
        let offer = |name: &str, description: &str| (name.to_string(), name.to_string(), description.to_string());
        assert_eq!(
            offered,
            [offer("claude-security", "Scan code"), offer("mine", "My own workflow"), offer("docs", "Edit docs")]
        );
    }

    #[test]
    fn codex_s_skills_are_named_and_placed_as_it_shows_them() {
        let listed = json!([
            {"name": "imagegen", "description": "Long", "interface": {"displayName": "Image Gen", "shortDescription": "Make images"},
             "scope": "system", "enabled": true, "pluginId": null},
            {"name": "deploy", "description": "Deploys", "interface": null, "scope": "repo", "enabled": true},
            {"name": "off", "description": "Off", "scope": "user", "enabled": false},
            {"name": "browser", "description": "Browses", "scope": "user", "enabled": true, "pluginId": "browser@openai"},
        ]);
        let skills: Vec<CodexSkill> = serde_json::from_value(listed).unwrap();
        let skills: Vec<Skill> = skills.into_iter().filter(|skill| skill.enabled).map(codex_skill).collect();
        let shown: Vec<_> =
            skills.iter().map(|skill| (skill.label.as_str(), skill.description.as_str(), skill.source)).collect();
        assert_eq!(
            shown,
            [
                ("Image Gen", "Make images", SkillSource::System),
                ("deploy", "Deploys", SkillSource::Project),
                ("browser", "Browses", SkillSource::Plugin),
            ]
        );
    }
}
