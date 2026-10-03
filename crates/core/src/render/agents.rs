//! The agents a thread's agent started, as the list an app shows. Each has a transcript of its
//! own: the items whose `parent` is the tool call that started it.

use motile_protocol::wire::{Item, ItemKind, ToolStatus};
use serde::Serialize;
use serde_json::Value;

/// What is shown of what an agent is doing or reported, at most.
const MAX_DETAIL_CHARS: usize = 280;

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct AgentView {
    /// The tool call that started it, which names its transcript.
    pub id: String,
    /// The agent that started it, when the thread's own didn't.
    pub parent: Option<String>,
    pub title: String,
    /// The kind of agent it was started as: "Explore".
    pub kind: Option<String>,
    pub status: ToolStatus,
    /// What it is doing now, or what it reported once it has ended, on one line.
    pub detail: String,
    /// What it was asked to do.
    pub prompt: String,
    pub started_at: f64,
    /// How long it worked, once it has ended.
    pub duration_ms: Option<u64>,
    pub tokens: Option<u64>,
    pub tool_uses: Option<u64>,
}

/// The agent the item's tool call started, if it started one.
pub fn view(item: &Item) -> Option<AgentView> {
    let ItemKind::Tool { call } = &item.kind else { return None };
    let agent = call.agent.as_ref()?;
    let input: Value = serde_json::from_str(&call.input).unwrap_or_default();
    let text = |key: &str| input[key].as_str().unwrap_or_default().trim().to_string();
    let prompt = text("prompt");
    let named = Some(text("description")).filter(|title| !title.is_empty());
    let title = named.unwrap_or_else(|| prompt.lines().next().unwrap_or("Agent").to_string());
    let working = agent.status == ToolStatus::Running;
    let reported = agent.result.as_deref().or(call.output.as_deref());
    let detail = match working {
        true => agent.progress.as_deref().or(reported),
        false => reported.or(agent.progress.as_deref()),
    };
    Some(AgentView {
        id: item.id.clone(),
        parent: item.parent.clone(),
        title,
        kind: agent.kind.clone(),
        status: agent.status,
        detail: one_line(detail.unwrap_or_default()),
        prompt,
        started_at: item.created_at,
        duration_ms: agent.duration_ms.filter(|_| !working),
        tokens: agent.tokens,
        tool_uses: agent.tool_uses,
    })
}

fn one_line(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= MAX_DETAIL_CHARS {
        return line;
    }
    let kept: String = line.chars().take(MAX_DETAIL_CHARS).collect();
    format!("{}…", kept.trim_end())
}

#[cfg(test)]
mod tests {
    use motile_protocol::wire::{Subagent, ToolCall};

    use super::*;

    fn started(status: ToolStatus, progress: Option<&str>, result: Option<&str>) -> Item {
        let agent = Subagent {
            kind: Some("Explore".into()),
            status,
            progress: progress.map(String::from),
            result: result.map(String::from),
            tokens: Some(2100),
            tool_uses: Some(3),
            duration_ms: Some(30_000),
        };
        let call = ToolCall {
            id: "t".into(),
            name: "Agent".into(),
            input: r#"{"description":"Count the files","prompt":"Count them.\nThen report."}"#.into(),
            output: None,
            status: ToolStatus::Succeeded,
            agent: Some(agent),
        };
        let kind = ItemKind::Tool { call };
        Item { id: "t".into(), seq: 0, rev: 1, created_at: 5.0, media: Vec::new(), parent: None, kind }
    }

    #[test]
    fn an_agent_that_works_says_what_it_does_and_one_that_ended_what_it_reported() {
        let working = view(&started(ToolStatus::Running, Some("Reading a.txt"), None)).unwrap();
        assert_eq!((working.title.as_str(), working.detail.as_str()), ("Count the files", "Reading a.txt"));
        assert_eq!((working.duration_ms, working.tool_uses), (None, Some(3)));

        let ended = view(&started(ToolStatus::Succeeded, Some("Reading a.txt"), Some("Three\n\nfiles"))).unwrap();
        assert_eq!((ended.detail.as_str(), ended.duration_ms), ("Three files", Some(30_000)));
    }

    #[test]
    fn a_tool_call_that_started_no_agent_is_none() {
        let mut item = started(ToolStatus::Running, None, None);
        let ItemKind::Tool { call } = &mut item.kind else { unreachable!() };
        call.agent = None;
        assert_eq!(view(&item), None);
    }
}
