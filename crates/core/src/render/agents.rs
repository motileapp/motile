//! The agents a thread's agent started, as the list a client shows. Each has a transcript of its
//! own: the items whose `parent` is the tool call that started it. A workflow's is its agents,
//! under the phases they run in.

use std::collections::HashMap;

use motile_protocol::wire::{
    Agent, AgentNotice, Ask, Interruption, Item, ItemKind, NoticeEvent, Subagent, Thread, ToolCall, ToolStatus,
    Workflow, WorkflowAgent,
};
use serde::Serialize;
use serde_json::{Value, json};

use super::rows::short_model_name;

/// What is shown of what an agent is doing or reported, at most.
const MAX_DETAIL_CHARS: usize = 280;
/// The tool calls that start agents, before they have said anything of them.
const STARTING: [&str; 4] = ["Task", "Agent", "Workflow", "mcp__motile__start_agent"];
/// What an older server says an agent does while it waits for the user.
const WAITING_PROGRESS: [&str; 2] = ["Waiting for your approval", "Asking a question"];

/// How the thread an agent works in is doing, which it says before the agent's tool call does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThreadState {
    Working,
    Waiting(Ask),
    Limited,
}

impl ThreadState {
    pub fn of(thread: &Thread) -> Self {
        if thread.needs_approval {
            return Self::Waiting(thread.asking.unwrap_or(Ask::Approval));
        }
        if thread.running || thread.monitoring {
            return Self::Working;
        }
        match thread.interruption {
            Some(Interruption::Limit { .. }) => Self::Limited,
            _ => Self::Working,
        }
    }

    /// The states of the threads the agents of the thread `thread_id` work in, by their ids.
    pub fn of_children(threads: &HashMap<String, Thread>, thread_id: &str) -> HashMap<String, Self> {
        let children =
            threads.values().filter(|thread| thread.parent.as_ref().is_some_and(|of| of.thread_id == thread_id));
        children.map(|thread| (thread.id.clone(), Self::of(thread))).collect()
    }
}

/// How an agent is doing, which its avatar's dot shows.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Working,
    /// It waits for the user to answer it.
    Waiting,
    /// It reached its account's usage limit, and waits for the limit to reset.
    Limited,
    Done,
    Failed,
    Stopped,
}

impl AgentState {
    fn of(status: ToolStatus, agent: Option<&Subagent>) -> Self {
        match status {
            ToolStatus::Running if agent.is_some_and(|agent| agent.waits.is_some()) => Self::Waiting,
            ToolStatus::Running if agent.is_some_and(|agent| agent.limited) => Self::Limited,
            ToolStatus::Running => Self::Working,
            ToolStatus::Succeeded => Self::Done,
            ToolStatus::Failed if agent.is_some_and(|agent| agent.stopped) => Self::Stopped,
            ToolStatus::Failed => Self::Failed,
        }
    }

    fn of_notice(event: NoticeEvent) -> Self {
        match event {
            NoticeEvent::Finished | NoticeEvent::Unknown => Self::Done,
            NoticeEvent::Failed => Self::Failed,
            NoticeEvent::Stopped => Self::Stopped,
            NoticeEvent::Asked => Self::Waiting,
            NoticeEvent::Limited => Self::Limited,
        }
    }

    /// The word for it, "Needs approval" for what it waits for.
    fn label(self, waits: Option<Ask>) -> &'static str {
        match (self, waits) {
            (Self::Working, _) => "Working",
            (Self::Waiting, Some(Ask::Question)) => "Has a question",
            (Self::Waiting, Some(Ask::Plan)) => "Has a plan",
            (Self::Waiting, _) => "Needs approval",
            (Self::Limited, _) => "Usage limit",
            (Self::Done, _) => "Done",
            (Self::Failed, _) => "Failed",
            (Self::Stopped, _) => "Stopped",
        }
    }

    /// Its word is shown beside its title: the user is to look at it.
    pub fn flagged(self) -> bool {
        matches!(self, Self::Waiting | Self::Limited | Self::Failed | Self::Stopped)
    }
}

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
    pub state: AgentState,
    /// Whether it is listed under the previous agents: it and every agent of its family ended.
    pub previous: bool,
    /// "Working", "Needs approval", "Failed".
    pub label: String,
    /// What it is doing now, or what it reported once it has ended, on one line.
    pub detail: String,
    /// What it was asked to do.
    pub prompt: String,
    pub started_at: f64,
    /// How long it worked, once it has ended.
    pub duration_ms: Option<u64>,
    pub tokens: Option<u64>,
    pub tool_uses: Option<u64>,
    /// The thread it works in, when it was started through your server: opening it opens that.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    /// The agent it runs as, whose logo it shows, when that is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<Agent>,
    /// The model's short name, "Opus 5.5", or else its id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The account's name, when its agent has more than one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

/// How many of `count` agents stand side by side as avatars: all of four or fewer, else three
/// beside a count of the rest, which is never one, since that one's avatar takes the same room.
pub fn avatars_shown(count: usize) -> usize {
    if count <= 4 { count } else { 3 }
}

/// The agent the item's tool call started, once the agent has said something of it. `threads` are
/// the states of the threads agents work in, by their ids.
pub fn view(item: &Item, threads: &HashMap<String, ThreadState>) -> Option<AgentView> {
    let ItemKind::Tool { call } = &item.kind else { return None };
    call.agent.as_ref()?;
    starting(item, call, threads)
}

/// Whether the tool call starts an agent.
pub fn starts_agent(call: &ToolCall) -> bool {
    call.agent.is_some() || STARTING.contains(&call.name.as_str())
}

/// The agent the tool call starts, as far as it is known. While it works in a thread of its own,
/// that thread says whether it waits.
pub fn starting(item: &Item, call: &ToolCall, threads: &HashMap<String, ThreadState>) -> Option<AgentView> {
    if !starts_agent(call) {
        return None;
    }
    let agent = call.agent.as_ref();
    let input: Value = serde_json::from_str(&call.input).unwrap_or_default();
    let text = |key: &str| input[key].as_str().unwrap_or_default().trim().to_string();
    let prompt = text("prompt");
    let status = agent.map_or(call.status, |agent| agent.status);
    let working = status == ToolStatus::Running;
    let runs_on = agent.and_then(|agent| agent.runs_on.as_deref());
    let thread_id = runs_on.and_then(|on| on.thread_id.clone());
    let thread = thread_id.as_ref().and_then(|id| threads.get(id)).copied().filter(|_| working);
    let workflow = agent.and_then(|agent| agent.workflow.as_ref());
    let (title, kind, detail) = match workflow {
        Some(workflow) => {
            (workflow_name(call, workflow), Some("Workflow".to_string()), workflow_progress(workflow, working))
        }
        None => {
            let named = ["description", "title", "name"].iter().map(|key| text(key)).find(|title| !title.is_empty());
            let title = named.unwrap_or_else(|| prompt.lines().next().unwrap_or("Agent").to_string());
            // An agent in a thread of its own reports in it; its call's reply only says it started.
            let own_thread = agent.and_then(|agent| agent.runs_on.as_ref()).is_some_and(|on| on.thread_id.is_some());
            let reply = call.output.as_deref().filter(|_| !own_thread);
            let reported = agent.and_then(|agent| agent.result.as_deref()).or(reply);
            let progress = agent.and_then(|agent| agent.progress.as_deref());
            let progress = match thread {
                Some(ThreadState::Waiting(Ask::Question)) => Some("Asking a question"),
                Some(ThreadState::Waiting(_)) => Some("Waiting for your approval"),
                Some(_) => progress.filter(|progress| !WAITING_PROGRESS.contains(progress)),
                None => progress,
            };
            let detail = match working {
                true => progress.or(reported),
                false => reported.or(progress),
            };
            (title, agent.and_then(|agent| agent.kind.clone()), detail.unwrap_or_default().to_string())
        }
    };
    let (state, waits) = match thread {
        Some(ThreadState::Working) => (AgentState::Working, None),
        Some(ThreadState::Waiting(ask)) => (AgentState::Waiting, Some(ask)),
        Some(ThreadState::Limited) => (AgentState::Limited, None),
        None => (AgentState::of(status, agent), agent.and_then(|agent| agent.waits)),
    };
    let label = state.label(waits).to_string();
    let detail = if detail.trim().is_empty() && working { label.clone() } else { detail };
    Some(AgentView {
        id: item.id.clone(),
        parent: item.parent.clone(),
        title,
        kind,
        status,
        state,
        previous: !working,
        label,
        detail: one_line(&detail),
        prompt,
        started_at: item.created_at,
        // One at its usage limit has stopped working for now, and says how long it worked.
        duration_ms: agent.and_then(|agent| agent.duration_ms).filter(|_| !working || state == AgentState::Limited),
        tokens: agent.and_then(|agent| agent.tokens),
        tool_uses: agent.and_then(|agent| agent.tool_uses),
        thread_id,
        agent: runs_on.map(|on| on.agent),
        model: runs_on
            .and_then(|on| on.model.as_deref())
            .map(|model| short_model_name(model).unwrap_or(model).to_string()),
        account: runs_on.and_then(|on| on.account.clone()),
    })
}

/// The agents as the agents tab lists them: the newest first, each followed by the ones it
/// started, and a family under the previous agents only once all of it ended.
pub fn listed(agents: impl DoubleEndedIterator<Item = AgentView>) -> Vec<AgentView> {
    let newest: Vec<AgentView> = agents.rev().collect();
    let known = |id: &String| newest.iter().any(|agent| &agent.id == id);
    let mut listed = Vec::with_capacity(newest.len());
    for root in newest.iter().filter(|agent| !agent.parent.as_ref().is_some_and(known)) {
        let mut family = Vec::new();
        add_family(root, &newest, &mut family);
        let previous = family.iter().all(|agent| agent.status != ToolStatus::Running);
        listed.extend(family.into_iter().map(|agent| AgentView { previous, ..agent }));
    }
    listed
}

fn add_family(agent: &AgentView, all: &[AgentView], family: &mut Vec<AgentView>) {
    family.push(agent.clone());
    for child in all.iter().filter(|child| child.parent.as_ref() == Some(&agent.id)) {
        add_family(child, all, family);
    }
}

/// Whether the agent has worked since `at`, when the thread's agent was told of it, so that what
/// it was told no longer says how it is.
pub fn worked_since(agent: &AgentView, at: f64) -> bool {
    let ended_at = agent.duration_ms.map(|duration| agent.started_at + duration as f64 / 1000.0);
    agent.status == ToolStatus::Running || ended_at.is_some_and(|ended_at| ended_at > at)
}

/// What the thread's agent was told of an agent it started, as that agent: what happened to it,
/// and what it said.
pub fn noticed(item: &Item, notice: &AgentNotice) -> AgentView {
    let state = AgentState::of_notice(notice.event);
    let label = match notice.event {
        _ if notice.repair => "Asked to fix",
        NoticeEvent::Finished => "Finished",
        NoticeEvent::Failed => "Failed",
        NoticeEvent::Stopped => "Stopped",
        NoticeEvent::Asked => "Asks a question",
        NoticeEvent::Limited => "Reached its usage limit",
        NoticeEvent::Unknown => "Told",
    };
    AgentView {
        id: item.id.clone(),
        parent: None,
        title: notice.title.clone(),
        kind: None,
        status: match state {
            AgentState::Done => ToolStatus::Succeeded,
            AgentState::Waiting | AgentState::Limited => ToolStatus::Running,
            _ => ToolStatus::Failed,
        },
        state,
        previous: !matches!(state, AgentState::Waiting | AgentState::Limited),
        label: label.to_string(),
        detail: notice.summary.as_deref().map(one_line).unwrap_or_default(),
        prompt: String::new(),
        started_at: item.created_at,
        duration_ms: None,
        tokens: None,
        tool_uses: None,
        thread_id: Some(notice.thread_id.clone()),
        agent: Some(notice.agent),
        model: None,
        account: None,
    }
}

/// The workflow's name as the call gave it, or what its script says it does.
pub fn workflow_name(call: &ToolCall, workflow: &Workflow) -> String {
    let input: Value = serde_json::from_str(&call.input).unwrap_or_default();
    let named = input["name"].as_str().filter(|name| !name.is_empty());
    let described = Some(workflow.title.as_str()).filter(|title| !title.is_empty());
    named.or(described).unwrap_or("Workflow").to_string()
}

/// How far the workflow is: the phase it works in and how many of its agents are done, or once
/// it has ended, how many it ran and how many of them failed.
pub fn workflow_progress(workflow: &Workflow, working: bool) -> String {
    let agents = &workflow.agents;
    let done = agents.iter().filter(|agent| agent.status != ToolStatus::Running).count();
    let failed = agents.iter().filter(|agent| agent.status == ToolStatus::Failed).count();
    let of = |count: usize| format!("{count} {}", if count == 1 { "agent" } else { "agents" });
    if !working {
        return match failed {
            0 => format!("Ran {}", of(agents.len())),
            _ => format!("Ran {}, {failed} failed", of(agents.len())),
        };
    }
    let latest = agents.iter().rev().find(|agent| agent.status == ToolStatus::Running).or(agents.last());
    let counted = format!("{done} of {} done", of(agents.len()));
    match latest.and_then(|agent| agent.phase.as_deref()) {
        Some(phase) => format!("{phase} · {counted}"),
        None if agents.is_empty() => "Starting".to_string(),
        None => counted,
    }
}

/// What the workflow the item's tool call started has done, as a transcript: each phase, then the
/// agents that ran in it. Empty when the call started no workflow.
pub fn workflow_items(item: &Item) -> Vec<Item> {
    let ItemKind::Tool { call } = &item.kind else { return Vec::new() };
    let Some(agent) = &call.agent else { return Vec::new() };
    let Some(workflow) = &agent.workflow else { return Vec::new() };
    let ended = agent.status != ToolStatus::Running;
    let phase_of = |agent: &WorkflowAgent| {
        let known = agent.phase.as_ref().and_then(|phase| workflow.phases.iter().position(|known| known == phase));
        known.unwrap_or(workflow.phases.len())
    };
    let mut agents: Vec<(usize, &WorkflowAgent)> = workflow.agents.iter().enumerate().collect();
    agents.sort_by_key(|(_, agent)| phase_of(agent));
    let mut items: Vec<Item> = Vec::new();
    let mut phase = None;
    for (index, agent) in agents {
        let child = |id: String, created_at: f64, kind: ItemKind| Item {
            id: format!("{}/{id}", item.id),
            seq: 0,
            rev: item.rev,
            created_at,
            media: Vec::new(),
            parent: Some(item.id.clone()),
            ui: Default::default(),
            kind,
        };
        if agent.phase.is_some() && agent.phase != phase {
            let title = agent.phase.clone().unwrap_or_default();
            items.push(child(
                format!("phase/{title}"),
                item.created_at,
                ItemKind::Assistant { text: format!("**{title}**") },
            ));
        }
        phase = agent.phase.clone();
        let status = match agent.status {
            ToolStatus::Running if ended => ToolStatus::Failed,
            status => status,
        };
        let call = ToolCall {
            id: format!("{}/agent/{index}", item.id),
            name: "Agent".to_string(),
            input: json!({"description": agent.label, "prompt": agent.prompt}).to_string(),
            output: agent.result.clone(),
            status,
            agent: None,
        };
        let started_at = agent.started_at.unwrap_or(item.created_at);
        items.push(child(format!("agent/{index}"), started_at, ItemKind::Tool { call }));
    }
    for (seq, item) in items.iter_mut().enumerate() {
        item.seq = seq as u64;
    }
    items
}

/// The text on one line, without the marks of Markdown's emphasis and code.
fn one_line(text: &str) -> String {
    let plain = text.replace("**", "").replace('`', "");
    let line = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= MAX_DETAIL_CHARS {
        return line;
    }
    let kept: String = line.chars().take(MAX_DETAIL_CHARS).collect();
    format!("{}…", kept.trim_end())
}

#[cfg(test)]
mod tests {
    use motile_protocol::wire::{Agent, RunsOn, Subagent};

    use super::*;

    /// The agent as its tool call alone says.
    fn alone(item: &Item) -> Option<AgentView> {
        view(item, &HashMap::new())
    }

    fn started(status: ToolStatus, progress: Option<&str>, result: Option<&str>) -> Item {
        let agent = Subagent {
            kind: Some("Explore".into()),
            status,
            progress: progress.map(String::from),
            result: result.map(String::from),
            tokens: Some(2100),
            tool_uses: Some(3),
            duration_ms: Some(30_000),
            workflow: None,
            runs_on: None,
            waits: None,
            stopped: false,
            limited: false,
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
        Item {
            id: "t".into(),
            seq: 0,
            rev: 1,
            created_at: 5.0,
            media: Vec::new(),
            parent: None,
            ui: Default::default(),
            kind,
        }
    }

    #[test]
    fn an_agent_that_works_says_what_it_does_and_one_that_ended_what_it_reported() {
        let working = alone(&started(ToolStatus::Running, Some("Reading a.txt"), None)).unwrap();
        assert_eq!((working.title.as_str(), working.detail.as_str()), ("Count the files", "Reading a.txt"));
        assert_eq!((working.duration_ms, working.tool_uses), (None, Some(3)));

        let ended = alone(&started(ToolStatus::Succeeded, Some("Reading a.txt"), Some("Three\n\nfiles"))).unwrap();
        assert_eq!((ended.detail.as_str(), ended.duration_ms), ("Three files", Some(30_000)));
    }

    #[test]
    fn agents_are_listed_newest_first_each_with_the_ones_it_started_under_it() {
        let agent = |id: &str, parent: Option<&str>, status: ToolStatus| AgentView {
            id: id.into(),
            parent: parent.map(String::from),
            previous: status != ToolStatus::Running,
            ..alone(&started(status, None, None)).unwrap()
        };
        let started = [
            agent("spec", None, ToolStatus::Running),
            agent("build", None, ToolStatus::Succeeded),
            agent("sketch", Some("spec"), ToolStatus::Succeeded),
            agent("solver", Some("build"), ToolStatus::Succeeded),
            agent("drawings", Some("spec"), ToolStatus::Succeeded),
        ];
        let listed: Vec<(String, bool)> =
            listed(started.into_iter()).into_iter().map(|agent| (agent.id, agent.previous)).collect();
        let expected = [("build", true), ("solver", true), ("spec", false), ("drawings", false), ("sketch", false)];
        assert_eq!(listed, expected.map(|(id, previous)| (id.to_string(), previous)));
    }

    #[test]
    fn an_agent_at_its_usage_limit_says_how_long_it_worked() {
        let mut item = started(ToolStatus::Running, Some("Reached its usage limit"), None);
        let ItemKind::Tool { call } = &mut item.kind else { unreachable!() };
        call.agent.as_mut().unwrap().limited = true;
        let limited = alone(&item).unwrap();
        assert_eq!((limited.state, limited.duration_ms), (AgentState::Limited, Some(30_000)));
    }

    #[test]
    fn an_agent_in_a_thread_of_its_own_never_shows_the_reply_that_started_it() {
        let mut item = started(ToolStatus::Running, None, None);
        let ItemKind::Tool { call } = &mut item.kind else { unreachable!() };
        call.name = "mcp__motile__start_agent".into();
        call.output = Some(r#"{"id":"c1","title":"Count the files"}"#.into());
        let agent = call.agent.as_mut().unwrap();
        agent.runs_on =
            Some(Box::new(RunsOn { agent: Agent::Codex, model: None, account: None, thread_id: Some("c1".into()) }));
        assert_eq!(alone(&item).unwrap().detail, "Working");
    }

    fn workflow_call(status: ToolStatus, agents: &[(&str, &str, ToolStatus)]) -> Item {
        let mut item = started(status, None, None);
        let ItemKind::Tool { call } = &mut item.kind else { unreachable!() };
        call.name = "Workflow".into();
        call.input = r#"{"name":"colours"}"#.into();
        let agents = agents.iter().map(|(label, phase, status)| WorkflowAgent {
            label: label.to_string(),
            phase: Some(phase.to_string()),
            status: *status,
            model: None,
            prompt: Some(format!("Say {label}")),
            result: None,
            started_at: None,
            tokens: None,
            tool_uses: None,
            duration_ms: None,
        });
        let workflow = Workflow {
            title: "Say colours".into(),
            phases: vec!["Ask".into(), "Join".into()],
            agents: agents.collect(),
        };
        call.agent.as_mut().unwrap().workflow = Some(workflow);
        item
    }

    #[test]
    fn a_workflow_says_its_phase_and_how_many_of_its_agents_are_done() {
        let running = [("red", "Ask", ToolStatus::Succeeded), ("blue", "Ask", ToolStatus::Running)];
        let working = alone(&workflow_call(ToolStatus::Running, &running)).unwrap();
        assert_eq!((working.title.as_str(), working.kind.as_deref()), ("colours", Some("Workflow")));
        assert_eq!(working.detail, "Ask · 1 of 2 agents done");

        let ran = [("red", "Ask", ToolStatus::Succeeded), ("blue", "Ask", ToolStatus::Failed)];
        assert_eq!(alone(&workflow_call(ToolStatus::Succeeded, &ran)).unwrap().detail, "Ran 2 agents, 1 failed");
    }

    #[test]
    fn a_workflow_s_transcript_is_its_agents_under_their_phases() {
        let agents = [
            ("join", "Join", ToolStatus::Running),
            ("red", "Ask", ToolStatus::Succeeded),
            ("blue", "Ask", ToolStatus::Running),
        ];
        let shown = |item: &Item| match &item.kind {
            ItemKind::Assistant { text } => text.clone(),
            ItemKind::Tool { call } => {
                format!("{} {:?}", serde_json::from_str::<Value>(&call.input).unwrap()["description"], call.status)
            }
            _ => String::new(),
        };
        let working: Vec<String> =
            workflow_items(&workflow_call(ToolStatus::Running, &agents)).iter().map(shown).collect();
        assert_eq!(working, ["**Ask**", "\"red\" Succeeded", "\"blue\" Running", "**Join**", "\"join\" Running"]);

        let stopped = workflow_items(&workflow_call(ToolStatus::Failed, &agents));
        assert_eq!(shown(&stopped[2]), "\"blue\" Failed", "an agent the workflow left behind no longer runs");
        assert!(stopped.iter().all(|item| item.parent.as_deref() == Some("t")));
        assert!(workflow_items(&started(ToolStatus::Running, None, None)).is_empty());
    }

    #[test]
    fn a_tool_call_that_started_no_agent_is_none() {
        let mut item = started(ToolStatus::Running, None, None);
        let ItemKind::Tool { call } = &mut item.kind else { unreachable!() };
        call.agent = None;
        assert_eq!(alone(&item), None);
    }

    #[test]
    fn a_repair_asks_to_fix_and_a_notice_of_a_newer_server_is_only_told() {
        let item = started(ToolStatus::Succeeded, None, None);
        let notice = |event, repair| AgentNotice {
            thread_id: "t".into(),
            title: "Program errors".into(),
            event,
            agent: Agent::Claude,
            summary: None,
            repair,
        };

        let repair = noticed(&item, &notice(NoticeEvent::Failed, true));
        let failed = noticed(&item, &notice(NoticeEvent::Failed, false));
        let newer = noticed(&item, &notice(NoticeEvent::Unknown, false));

        assert_eq!((repair.label.as_str(), repair.state), ("Asked to fix", AgentState::Failed));
        assert_eq!(failed.label, "Failed");
        assert_eq!(
            (newer.label.as_str(), newer.state, newer.status),
            ("Told", AgentState::Done, ToolStatus::Succeeded)
        );
    }

    fn in_its_own_thread(status: ToolStatus, waits: Option<Ask>, progress: &str) -> Item {
        let result = (status != ToolStatus::Running).then_some("Counted three");
        let mut item = started(status, Some(progress), result);
        let ItemKind::Tool { call } = &mut item.kind else { unreachable!() };
        call.name = "mcp__motile__start_agent".into();
        let agent = call.agent.as_mut().unwrap();
        agent.waits = waits;
        agent.runs_on =
            Some(Box::new(RunsOn { agent: Agent::Codex, model: None, account: None, thread_id: Some("c1".into()) }));
        item
    }

    fn shown(item: &Item, thread: ThreadState) -> (AgentState, String, String) {
        let agent = view(item, &HashMap::from([("c1".to_string(), thread)])).unwrap();
        (agent.state, agent.label, agent.detail)
    }

    fn states(state: AgentState, label: &str, detail: &str) -> (AgentState, String, String) {
        (state, label.to_string(), detail.to_string())
    }

    #[test]
    fn an_agent_whose_thread_no_longer_waits_works_whatever_its_tool_call_still_says() {
        let item = in_its_own_thread(ToolStatus::Running, Some(Ask::Approval), "Waiting for your approval");
        assert_eq!(shown(&item, ThreadState::Working), states(AgentState::Working, "Working", "Working"));

        let item = in_its_own_thread(ToolStatus::Running, None, "Reading a.txt");
        assert_eq!(shown(&item, ThreadState::Working), states(AgentState::Working, "Working", "Reading a.txt"));
    }

    #[test]
    fn an_agent_whose_thread_waits_says_what_for_before_its_tool_call_does() {
        let item = in_its_own_thread(ToolStatus::Running, None, "Running the tests");
        assert_eq!(
            shown(&item, ThreadState::Waiting(Ask::Approval)),
            states(AgentState::Waiting, "Needs approval", "Waiting for your approval")
        );
        assert_eq!(
            shown(&item, ThreadState::Waiting(Ask::Question)),
            states(AgentState::Waiting, "Has a question", "Asking a question")
        );
        assert_eq!(
            shown(&item, ThreadState::Waiting(Ask::Plan)),
            states(AgentState::Waiting, "Has a plan", "Waiting for your approval")
        );
    }

    #[test]
    fn an_agent_whose_thread_reached_its_limit_keeps_what_its_tool_call_says_of_it() {
        let item = in_its_own_thread(ToolStatus::Running, None, "Waiting for its usage limit to reset");
        assert_eq!(
            shown(&item, ThreadState::Limited),
            states(AgentState::Limited, "Usage limit", "Waiting for its usage limit to reset")
        );
    }

    #[test]
    fn an_agent_that_ended_is_as_its_tool_call_says() {
        let item = in_its_own_thread(ToolStatus::Succeeded, None, "Reading a.txt");
        assert_eq!(
            shown(&item, ThreadState::Waiting(Ask::Approval)),
            states(AgentState::Done, "Done", "Counted three")
        );
    }

    #[test]
    fn a_thread_s_state_comes_from_what_it_waits_for_then_whether_it_works() {
        let thread: Thread = serde_json::from_value(json!({
            "id": "c1", "title": "Count", "project_id": "p", "cwd": "/", "agent": "codex", "model": null,
            "effort": null, "access": "full", "created_at": 0.0, "updated_at": 0.0, "done_at": null,
            "running": false, "needs_approval": false, "turn_ended_at": null, "rev": 0,
            "parent": {"thread_id": "t", "tool_id": "call"},
        }))
        .unwrap();
        let limited =
            Thread { interruption: Some(Interruption::Limit { resets_at: None, continues: true }), ..thread.clone() };
        let asking = Thread { needs_approval: true, asking: Some(Ask::Question), running: true, ..limited.clone() };
        let other = Thread { id: "c2".into(), parent: None, needs_approval: true, ..thread.clone() };
        assert_eq!(ThreadState::of(&thread), ThreadState::Working);
        assert_eq!(ThreadState::of(&limited), ThreadState::Limited);
        assert_eq!(ThreadState::of(&Thread { running: true, ..limited.clone() }), ThreadState::Working);
        assert_eq!(ThreadState::of(&asking), ThreadState::Waiting(Ask::Question));

        let threads = HashMap::from([("c1".to_string(), asking), ("c2".to_string(), other)]);
        let children = ThreadState::of_children(&threads, "t");
        assert_eq!(children, HashMap::from([("c1".to_string(), ThreadState::Waiting(Ask::Question))]));
    }
}
