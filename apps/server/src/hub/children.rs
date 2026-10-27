//! The agents an agent starts through the `motile` MCP server, on any of your server's accounts.
//! Each works in a thread of its own, which knows the thread and the tool call that started it.
//! That tool call follows how the agent is doing, and the agent that started it is told when the
//! turn it gave the thread ends, or when the thread asks a question, unless a call of its own was
//! waiting for just that.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use motile_protocol::now;
use motile_protocol::wire::{
    Access, Agent, AgentAccount, AgentNotice, Approval, Ask, Interruption, ItemKind, Message, NewThread, NewWorktree,
    NoticeEvent, Queued, RunsOn, Subagent, Thread, ThreadParent, ToolCall, ToolStatus,
};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::broadcast;

use super::{Hub, Live, Slot, TitleSource, agent_name, new_id, plain_message, thread_worktree};
use crate::git;
use crate::mcp::StartAgent;

/// The tool call that starts an agent, whatever the agent calls the server.
const START_TOOL: &str = "__start_agent";
pub const WAIT: Duration = Duration::from_secs(600);
pub const LONGEST_WAIT: Duration = Duration::from_secs(3600);
/// How long a call that starts an agent waits for its own tool call to show in the transcript.
const LINK_WITHIN: Duration = Duration::from_secs(3);
const LINK_EVERY: Duration = Duration::from_millis(50);
/// How much of a reply a notice or an answer carries; `read_thread` reads the rest.
const NOTICE_REPLY_CHARS: usize = 6000;
const STATE_REPLY_CHARS: usize = 20_000;
const PROGRESS_CHARS: usize = 120;

#[derive(Serialize, Clone)]
pub struct ChildState {
    pub id: String,
    pub title: String,
    pub agent: &'static str,
    /// The account's name, when its agent has more than one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    pub model: Option<String>,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// What it asks, while it does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub questions: Option<Value>,
    /// When its usage limit resets, while it waits for that.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_resets: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub wait_timed_out: bool,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Running,
    NeedsApproval,
    Asking,
    WaitingForUsageLimit,
    Finished,
    Failed,
    Stopped,
}

impl Status {
    fn settled(self) -> bool {
        matches!(self, Self::Finished | Self::Failed | Self::Stopped)
    }
}

fn descendants_in(threads: &HashMap<String, Slot>, thread_id: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (id, slot) in threads {
        if slot.parent.as_ref().is_some_and(|parent| parent.thread_id == thread_id) {
            found.extend(descendants_in(threads, id));
            found.push(id.clone());
        }
    }
    found
}

/// Counts a call that waits for agents while it does; the agents are looked at again once it no
/// longer waits, for what it didn't take.
struct Waiting {
    hub: Arc<Hub>,
    ids: Vec<String>,
}

impl Waiting {
    fn new(hub: &Arc<Hub>, ids: &[String]) -> Self {
        let mut waiting = hub.lock_waiting();
        for id in ids {
            *waiting.entry(id.clone()).or_default() += 1;
        }
        Self { hub: hub.clone(), ids: ids.to_vec() }
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        let mut waiting = self.hub.lock_waiting();
        for id in &self.ids {
            if let Some(count) = waiting.get_mut(id) {
                *count -= 1;
                if *count == 0 {
                    waiting.remove(id);
                }
            }
        }
        drop(waiting);
        for id in &self.ids {
            let _ = self.hub.children_changed.send(id.clone());
        }
    }
}

impl Hub {
    /// The threads the thread's agent started, theirs before them, deepest first.
    pub(super) fn descendants(&self, thread_id: &str) -> Vec<String> {
        descendants_in(&self.lock_threads(), thread_id)
    }

    fn descends_from(&self, thread_id: &str, ancestor: &str) -> bool {
        let threads = self.lock_threads();
        let mut seen = HashSet::new();
        let mut parent = threads.get(thread_id).and_then(|slot| slot.parent.as_ref());
        while let Some(found) = parent {
            if found.thread_id == ancestor {
                return true;
            }
            if !seen.insert(found.thread_id.clone()) {
                return false;
            }
            parent = threads.get(&found.thread_id).and_then(|slot| slot.parent.as_ref());
        }
        false
    }

    fn lock_waiting(&self) -> std::sync::MutexGuard<'_, HashMap<String, usize>> {
        self.waiting_on.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The account's name, when its agent has more than one.
    pub(super) fn account_name(&self, account: &AgentAccount) -> Option<String> {
        let accounts = self.agent_accounts();
        let several = accounts.iter().filter(|kept| kept.agent == account.agent).count() > 1;
        several.then(|| account.name.clone())
    }

    fn lock_started(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), String>> {
        self.started.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Your server's accounts an agent can start agents on, with their models and how much of
    /// their plans they have used.
    pub async fn accounts_for(&self, caller: &str) -> anyhow::Result<Value> {
        let own = self.thread(caller).await.context("Your thread is no longer on your server.")?;
        let own = self.account_of(&own).id;
        let limits = match self.limits().await {
            Message::Limits { agents } => agents,
            _ => Vec::new(),
        };
        let models = self.models();
        let accounts: Vec<Value> = self
            .agent_accounts()
            .into_iter()
            .map(|account| {
                let models: Vec<Value> = models
                    .iter()
                    .filter(|model| model.account == account.id)
                    .map(|model| {
                        json!({"id": model.id, "name": model.name, "efforts": model.efforts,
                               "default_effort": model.default_effort})
                    })
                    .collect();
                let read = limits.iter().find(|read| read.agent == account.agent && read.account_name == account.name);
                let windows = read.map(|read| {
                    let window = |window: &motile_protocol::wire::LimitWindow| {
                        let resets_at = window.resets_at.and_then(|at| chrono::DateTime::from_timestamp(at as i64, 0));
                        json!({"window": window.label, "used_percent": window.used_percent.round(),
                               "resets_at": resets_at.map(|at| at.to_rfc3339())})
                    };
                    read.windows.iter().map(window).collect::<Vec<_>>()
                });
                json!({
                    "id": account.id,
                    "name": account.name,
                    "agent": agent_key(account.agent),
                    "yours": account.id == own,
                    "signed_in_as": account.email,
                    "plan": account.plan,
                    "installed": self.environment.executable(account.agent).is_some(),
                    "models": models,
                    "limits": windows,
                })
            })
            .collect();
        Ok(json!({ "accounts": accounts }))
    }

    /// Starts an agent in a thread of its own for the caller's agent, which goes on meanwhile
    /// unless it waits.
    pub async fn start_child(self: &Arc<Self>, caller: &str, start: StartAgent) -> anyhow::Result<ChildState> {
        let text = start.prompt.trim().to_string();
        if text.is_empty() {
            bail!("Give the agent a prompt. It knows nothing of your conversation.");
        }
        let request = start.request_id.clone().filter(|id| !id.is_empty()).map(|id| (caller.to_string(), id));
        let started = request.as_ref().and_then(|request| self.lock_started().get(request).cloned());
        let wait = start.wait.then(|| wait_for(start.timeout_seconds));
        if let Some(child) = started {
            return Ok(self.child_states(caller, &[child], wait).await?.remove(0));
        }
        let agent = start.agent.as_deref().map(parse_agent).transpose()?;
        self.wait_for_start_call(caller, &text).await;
        let own = self.thread(caller).await.context("Your thread is no longer on your server.")?;
        let account = self.pick_account(&own, start.account.as_deref(), agent)?;
        let (model, effort) = self.pick_model(&own, &account, start.model, start.effort)?;
        let access = pick_access(own.access, start.access.as_deref())?;
        let worktree = match start.worktree {
            true => {
                let base =
                    git::current_branch(&own.cwd).context("Your folder has no branch for a worktree to start from.")?;
                Some(NewWorktree { base, branch: None })
            }
            false => None,
        };
        let role = start.role.map(|role| role.trim().to_string()).filter(|role| !role.is_empty());
        let prompt = match &role {
            Some(role) => format!("Act as the {role} agent for this task.\n\n{text}"),
            None => text.clone(),
        };
        let new_thread = NewThread {
            project_id: own.project_id.clone(),
            agent: account.agent,
            agent_account: Some(account.id.clone()),
            model,
            effort,
            access,
            worktree,
            from: None,
        };
        let folder = (!start.worktree).then(|| own.cwd.clone());
        let mut stored = self.new_thread(new_thread, &prompt, &[], folder).await?;
        let child_id = stored.thread.id.clone();
        let model = stored.thread.model.clone().map(|model| self.model_name(account.agent, &model).unwrap_or(model));
        let runs_on = RunsOn {
            agent: account.agent,
            model,
            account: self.account_name(&account),
            thread_id: Some(child_id.clone()),
        };
        let tool_id = self.show_started(caller, &text, role, runs_on).await?;
        stored.thread.parent = Some(ThreadParent { thread_id: caller.to_string(), tool_id });
        stored.reports = true;
        let title = start.title.map(|title| title.trim().to_string()).filter(|title| !title.is_empty());
        if let Some(title) = &title {
            stored.thread.title = title.clone();
            stored.title_source = TitleSource::Generated;
        }
        self.store.save_thread(&stored)?;
        if let Some((thread_id, worktree)) = thread_worktree(&stored) {
            self.lock_worktrees().insert(thread_id, worktree);
        }
        self.add_thread(Live::new(stored, self.media.clone()));
        let mut live = self.live_or(&child_id, "The agent's thread is gone.").await?;
        live.append_queued(&self.store, plain_message(prompt.clone()))?;
        self.start_turn(&mut live, prompt)?;
        drop(live);
        if let Some(request) = request {
            self.lock_started().insert(request, child_id.clone());
        }
        if title.is_none() {
            tokio::spawn(self.clone().title_from_first_message(child_id.clone(), text));
        }
        Ok(self.child_states(caller, &[child_id], wait).await?.remove(0))
    }

    /// Gives the agent's own tool call a moment to show in its transcript, so that the agent it
    /// starts is shown there.
    async fn wait_for_start_call(&self, caller: &str, prompt: &str) {
        let started = Instant::now();
        while started.elapsed() < LINK_WITHIN {
            if self.live(caller).await.is_none_or(|live| start_call(&live, prompt).is_some()) {
                return;
            }
            tokio::time::sleep(LINK_EVERY).await;
        }
    }

    /// Shows the agent in the caller's tool call that started it, which no other agent can take
    /// then, and answers with that call's id.
    async fn show_started(
        &self,
        caller: &str,
        prompt: &str,
        kind: Option<String>,
        runs_on: RunsOn,
    ) -> anyhow::Result<Option<String>> {
        let mut parent = self.live_or(caller, "Your thread is no longer on your server.").await?;
        let Some(tool_id) = start_call(&parent, prompt) else { return Ok(None) };
        let agent = Subagent {
            kind,
            status: ToolStatus::Running,
            progress: None,
            result: None,
            tokens: None,
            tool_uses: None,
            duration_ms: None,
            workflow: None,
            runs_on: Some(Box::new(runs_on)),
            waits: None,
            stopped: false,
            limited: false,
        };
        parent.change_tool(&self.store, &tool_id, |call| {
            call.agent = Some(agent);
            true
        })?;
        self.count_agents(&mut parent);
        Ok(Some(tool_id))
    }

    fn pick_account(&self, own: &Thread, named: Option<&str>, agent: Option<Agent>) -> anyhow::Result<AgentAccount> {
        let accounts = self.agent_accounts();
        let own_account = self.account_of(own);
        let account = match named.map(str::trim).filter(|named| !named.is_empty()) {
            Some(named) => {
                let by_id = accounts.iter().find(|account| account.id == named);
                let by_name = || accounts.iter().find(|account| account.name.eq_ignore_ascii_case(named));
                let found = by_id.or_else(by_name).cloned();
                found
                    .with_context(|| format!("Your server has no account called {named}. list_accounts lists them."))?
            }
            None => match agent {
                Some(agent) if agent != own_account.agent => self.default_account(agent),
                _ => own_account,
            },
        };
        if let Some(agent) = agent.filter(|agent| *agent != account.agent) {
            bail!("{} is a {} account, not a {} one.", account.name, agent_name(account.agent), agent_name(agent));
        }
        if self.environment.executable(account.agent).is_none() {
            bail!("{} isn't installed on your server.", agent_name(account.agent));
        }
        Ok(account)
    }

    /// The model and effort the agent runs: the ones asked for, which the account must list, or
    /// the caller's own when it runs the same agent, or the account's defaults.
    fn pick_model(
        &self,
        own: &Thread,
        account: &AgentAccount,
        model: Option<String>,
        effort: Option<String>,
    ) -> anyhow::Result<(Option<String>, Option<String>)> {
        let listed: Vec<_> = self.models().into_iter().filter(|model| model.account == account.id).collect();
        let model = match model.map(|model| model.trim().to_string()).filter(|model| !model.is_empty()) {
            Some(wanted) => {
                match listed.iter().find(|model| model.id == wanted || model.name.eq_ignore_ascii_case(&wanted)) {
                    Some(found) => Some(found.id.clone()),
                    None if listed.is_empty() => Some(wanted),
                    None => {
                        let offered: Vec<&str> = listed.iter().map(|model| model.id.as_str()).collect();
                        bail!("{} doesn't offer {wanted}. It offers {}.", account.name, offered.join(", "))
                    }
                }
            }
            None if account.agent == own.agent => own.model.clone(),
            None => None,
        };
        let known = model.as_ref().and_then(|id| listed.iter().find(|listed| &listed.id == id));
        let effort = match effort.map(|effort| effort.trim().to_string()).filter(|effort| !effort.is_empty()) {
            Some(wanted) => {
                if let Some(known) = known.filter(|known| !known.efforts.is_empty() && !known.efforts.contains(&wanted))
                {
                    bail!("{} runs at {}, not {wanted}.", known.name, known.efforts.join(", "));
                }
                Some(wanted)
            }
            None if account.agent == own.agent && model == own.model => own.effort.clone(),
            None => None,
        };
        Ok((model, effort))
    }

    /// How the agents the caller started are doing. With `wait`, once all of them have ended, one
    /// asks a question, or `wait` is over. What it is answered here, it isn't told again.
    pub async fn child_states(
        self: &Arc<Self>,
        caller: &str,
        ids: &[String],
        wait: Option<Duration>,
    ) -> anyhow::Result<Vec<ChildState>> {
        if ids.is_empty() {
            bail!("Name the agents by the ids start_agent gave you.");
        }
        let deadline = wait.map(|wait| Instant::now() + wait);
        let _waiting = Waiting::new(self, ids);
        let mut told = self.children_told.subscribe();
        loop {
            let timed_out = deadline.is_some_and(|deadline| Instant::now() >= deadline);
            let mut seen = Vec::new();
            for id in ids {
                if !self.descends_from(id, caller) {
                    bail!("No agent you started has the id {id}.");
                }
                let live = self.live_or(id, "That agent's thread is gone.").await?;
                let question = live.activity.approvals.first().map(|approval| approval.id.clone());
                seen.push((self.state_of(&live)?, question, live.told_limit));
            }
            let settled = seen.iter().all(|(state, ..)| state.status.settled());
            let asking = seen.iter().any(|(state, ..)| state.status == Status::Asking);
            // An agent that reached its usage limit is worth hearing of once, not waited for.
            let limited =
                seen.iter().any(|(state, _, told_limit)| state.status == Status::WaitingForUsageLimit && !told_limit);
            if deadline.is_none() || timed_out || settled || asking || limited {
                let mut states = Vec::new();
                for (mut state, question, _) in seen {
                    state.wait_timed_out = timed_out && !settled && !asking && !limited;
                    if let Some(mut live) = self.live(&state.id).await {
                        self.taken(&mut live, state.status, question)?;
                    }
                    states.push(state);
                }
                return Ok(states);
            }
            let Some(deadline) = deadline else { unreachable!("a call that doesn't wait has returned") };
            tokio::select! {
                _ = told_of(&mut told, ids) => {}
                _ = tokio::time::sleep_until(deadline.into()) => {}
            }
        }
    }

    /// The caller's agent was told what this state says: it needn't be told again.
    fn taken(&self, live: &mut Live, status: Status, question: Option<String>) -> anyhow::Result<()> {
        if status.settled() && live.stored.reports {
            live.stored.reports = false;
            self.store.save_thread(&live.stored)?;
        }
        if status == Status::Asking {
            live.told_question = question;
        }
        live.told_limit = status == Status::WaitingForUsageLimit;
        Ok(())
    }

    /// Gives an agent the caller started another message, which it is told the end of.
    pub async fn message_child(self: &Arc<Self>, caller: &str, id: &str, text: String) -> anyhow::Result<ChildState> {
        {
            if !self.descends_from(id, caller) {
                bail!("No agent you started has the id {id}.");
            }
            let mut live = self.live_or(id, "That agent's thread is gone.").await?;
            live.stored.reports = true;
            self.store.save_thread(&live.stored)?;
        }
        self.send(Some(id.to_string()), None, text, Vec::new(), true, None).await?;
        Ok(self.child_states(caller, &[id.to_string()], None).await?.remove(0))
    }

    /// Answers the question an agent the caller started asks, by question.
    pub async fn answer_child(
        self: &Arc<Self>,
        caller: &str,
        id: &str,
        answers: HashMap<String, String>,
    ) -> anyhow::Result<ChildState> {
        let approval_id = {
            if !self.descends_from(id, caller) {
                bail!("No agent you started has the id {id}.");
            }
            let live = self.live_or(id, "That agent's thread is gone.").await?;
            let asking = live.activity.approvals.iter().find(|approval| approval.ask() == Ask::Question);
            asking.context("That agent isn't asking anything now.")?.id.clone()
        };
        self.answer(id, &approval_id, true, answers).await?;
        Ok(self.child_states(caller, &[id.to_string()], None).await?.remove(0))
    }

    /// Stops an agent the caller started, and the agents it started.
    pub async fn stop_child(self: &Arc<Self>, caller: &str, id: &str) -> anyhow::Result<ChildState> {
        {
            if !self.descends_from(id, caller) {
                bail!("No agent you started has the id {id}.");
            }
            let mut live = self.live_or(id, "That agent's thread is gone.").await?;
            live.stored.reports = false;
            self.store.save_thread(&live.stored)?;
        }
        self.stop(id).await;
        Ok(self.child_states(caller, &[id.to_string()], None).await?.remove(0))
    }

    fn state_of(&self, live: &Live) -> anyhow::Result<ChildState> {
        let thread = &live.stored.thread;
        let account = self.account_of(thread);
        let approval = live.activity.approvals.first();
        let mut state = ChildState {
            id: thread.id.clone(),
            title: thread.title.clone(),
            agent: agent_name(thread.agent),
            account: self.account_name(&account),
            model: thread.model.clone().or_else(|| live.session_model.clone()),
            status: Status::Running,
            reply: None,
            error: None,
            questions: None,
            limit_resets: None,
            wait_timed_out: false,
        };
        state.status = match approval.map(Approval::ask) {
            Some(Ask::Question) => Status::Asking,
            Some(_) => Status::NeedsApproval,
            None if live.busy() || thread.monitoring => Status::Running,
            None if live.limited() => {
                state.limit_resets = limit_resets(thread);
                Status::WaitingForUsageLimit
            }
            None => {
                let items = self.store.last_turn(&thread.id)?;
                let ended = items.iter().rev().find_map(|item| match &item.kind {
                    ItemKind::TurnEnd { summary } => Some(summary.clone()),
                    _ => None,
                });
                state.reply = items.iter().rev().find_map(|item| match &item.kind {
                    ItemKind::Assistant { text } => Some(text.clone()),
                    _ => None,
                });
                state.error = items.iter().rev().find_map(|item| match &item.kind {
                    ItemKind::Error { message } => Some(message.clone()),
                    _ => None,
                });
                match ended {
                    Some(summary) if summary.stopped => Status::Stopped,
                    Some(summary) if summary.is_error => Status::Failed,
                    _ if state.error.is_some() => Status::Failed,
                    Some(_) => Status::Finished,
                    None => Status::Stopped,
                }
            }
        };
        if state.status == Status::Asking {
            let input: Value =
                approval.and_then(|approval| serde_json::from_str(&approval.input).ok()).unwrap_or_default();
            state.questions = Some(input["questions"].clone());
        }
        state.reply = state.reply.map(|reply| cut(&reply, STATE_REPLY_CHARS, &thread.id));
        Ok(state)
    }

    /// Has the agents that started others see how those are doing the moment they change, and hear
    /// what they are to hear. What changed while one was being told is told next, once.
    pub fn keep_parents_told(self: &Arc<Self>) {
        let changes = self.children_changes.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
        let Some(mut changes) = changes else { return };
        let hub = self.clone();
        tokio::spawn(async move {
            while let Some(first) = changes.recv().await {
                let mut changed = vec![first];
                while let Ok(more) = changes.try_recv() {
                    if !changed.contains(&more) {
                        changed.push(more);
                    }
                }
                for child in changed {
                    if let Err(error) = hub.tell_parent(&child).await {
                        tracing::warn!(child, "couldn't tell an agent of the agent it started: {error:#}");
                    }
                    let _ = hub.children_told.send(child);
                }
            }
        });
    }

    /// Shows how the agent is doing in the tool call that started it, and tells the agent that
    /// started it what it is to hear.
    async fn tell_parent(self: &Arc<Self>, child_id: &str) -> anyhow::Result<()> {
        let Some(parent) = self.slot(child_id).and_then(|slot| slot.parent) else { return Ok(()) };
        let (agent, notice) = {
            let Some(mut child) = self.live(child_id).await else { return Ok(()) };
            let state = self.state_of(&child)?;
            let agent = self.subagent_of(&mut child, &state)?;
            if state.status != Status::WaitingForUsageLimit {
                child.told_limit = false;
            }
            let waited_on = self.lock_waiting().contains_key(child_id);
            let notice = if waited_on { None } else { self.notice_due(&mut child, &state)? };
            (agent, notice)
        };
        let Some(mut live) = self.live(&parent.thread_id).await else { return Ok(()) };
        if let Some(tool_id) = &parent.tool_id {
            live.change_tool(&self.store, tool_id, |call| shows(call, agent))?;
            self.count_agents(&mut live);
        }
        match notice {
            Some((notice, text)) => self.tell(&mut live, notice, text),
            None => Ok(()),
        }
    }

    fn subagent_of(&self, live: &mut Live, state: &ChildState) -> anyhow::Result<Subagent> {
        let tool_uses = live.tool_count(&self.store)?;
        let thread = &live.stored.thread;
        let tokens =
            self.store.session(&thread.id, &self.continuation(thread))?.and_then(|session| session.context_used);
        let status = match state.status {
            Status::Finished => ToolStatus::Succeeded,
            Status::Failed | Status::Stopped => ToolStatus::Failed,
            _ => ToolStatus::Running,
        };
        // What it waits for the clients read from its thread; this says what it is doing.
        let progress = match state.status {
            Status::WaitingForUsageLimit if limit_continues(thread) => {
                Some("Waiting for its usage limit to reset".to_string())
            }
            Status::WaitingForUsageLimit => Some("Reached its usage limit".to_string()),
            Status::Running | Status::NeedsApproval | Status::Asking => {
                live.open.values().filter(|item| item.parent.is_none()).max_by_key(|item| item.seq).and_then(|item| {
                    match &item.kind {
                        ItemKind::Tool { call } => Some(doing(call, &thread.cwd)),
                        ItemKind::Thinking { .. } => Some("Thinking".to_string()),
                        ItemKind::Assistant { .. } => Some("Writing its reply".to_string()),
                        _ => None,
                    }
                })
            }
            _ => None,
        };
        let ended = thread.turn_ended_at.unwrap_or_else(now);
        Ok(Subagent {
            kind: None,
            status,
            progress,
            result: state.reply.clone().or_else(|| state.error.clone()).filter(|_| state.status.settled()),
            tokens,
            tool_uses: Some(tool_uses),
            duration_ms: (state.status.settled() || state.status == Status::WaitingForUsageLimit)
                .then(|| ((ended - thread.created_at).max(0.0) * 1000.0) as u64),
            workflow: None,
            runs_on: Some(Box::new(RunsOn {
                agent: thread.agent,
                model: state.model.clone().map(|model| self.model_name(thread.agent, &model).unwrap_or(model)),
                account: state.account.clone(),
                thread_id: Some(thread.id.clone()),
            })),
            waits: match state.status {
                Status::NeedsApproval => live.activity.approvals.first().map(Approval::ask),
                Status::Asking => Some(Ask::Question),
                _ => None,
            },
            stopped: state.status == Status::Stopped,
            limited: state.status == Status::WaitingForUsageLimit,
        })
    }

    /// What the agent that started the thread is to be told of it now, once.
    fn notice_due(&self, live: &mut Live, state: &ChildState) -> anyhow::Result<Option<(AgentNotice, String)>> {
        if !live.stored.reports {
            return Ok(None);
        }
        let thread = &live.stored.thread;
        let on = match &state.account {
            Some(account) => format!("{} ({account})", state.agent),
            None => state.agent.to_string(),
        };
        let named = format!("Agent \"{}\" ({}, on {on})", thread.title, thread.id);
        let (event, text) = match state.status {
            Status::Asking => {
                let Some(approval) = live.activity.approvals.first() else { return Ok(None) };
                if live.told_question.as_ref() == Some(&approval.id) {
                    return Ok(None);
                }
                live.told_question = Some(approval.id.clone());
                let questions = questions_text(state.questions.as_ref());
                let text = format!(
                    "{named} asks:\n{questions}\nAnswer it with answer_agent when the answer follows from your task or what the \
                     user told you. Otherwise ask the user, then answer it."
                );
                (NoticeEvent::Asked, text)
            }
            Status::Finished => {
                let reply = state.reply.as_deref().unwrap_or("It finished without a reply.");
                (
                    NoticeEvent::Finished,
                    format!("{named} finished. Its reply:\n\n{}", cut(reply, NOTICE_REPLY_CHARS, &thread.id)),
                )
            }
            Status::Failed => {
                let error = state.error.as_deref().unwrap_or("It reported an error.");
                (NoticeEvent::Failed, format!("{named} failed: {error}"))
            }
            Status::Stopped => (NoticeEvent::Stopped, format!("{named} was stopped before it finished.")),
            Status::WaitingForUsageLimit => {
                if live.told_limit {
                    return Ok(None);
                }
                live.told_limit = true;
                let resets =
                    state.limit_resets.as_deref().map(|at| format!(" The limit resets at {at}.")).unwrap_or_default();
                let continues = if limit_continues(thread) {
                    " It goes on by itself once the limit resets."
                } else {
                    " It won't go on by itself: message it with message_agent once the limit resets."
                };
                let text = format!(
                    "{named} reached its account's usage limit and waits.{resets}{continues} To go on sooner, stop it with \
                     stop_agent and start the work again with start_agent on an account that has room left: list_accounts \
                     says how much of its limits each account has used."
                );
                (NoticeEvent::Limited, text)
            }
            _ => return Ok(None),
        };
        if state.status.settled() {
            live.stored.reports = false;
            self.store.save_thread(&live.stored)?;
        }
        let summary = match state.status {
            Status::Finished => state.reply.as_deref().map(one_line),
            Status::Failed => state.error.as_deref().map(one_line),
            Status::Asking => {
                state.questions.as_ref().and_then(|questions| questions[0]["question"].as_str()).map(one_line)
            }
            _ => None,
        };
        let notice = AgentNotice {
            thread_id: thread.id.clone(),
            title: thread.title.clone(),
            event,
            agent: thread.agent,
            summary,
            repair: false,
        };
        Ok(Some((notice, text)))
    }

    /// Tells the thread's agent what the notice is about: at once while it works, in a turn of its
    /// own when it doesn't, and after the turn it is about to start otherwise.
    pub(super) fn tell(self: &Arc<Self>, live: &mut Live, notice: AgentNotice, text: String) -> anyhow::Result<()> {
        let queued = Queued {
            id: new_id(),
            text,
            attachments: Vec::new(),
            media: Vec::new(),
            skills: Vec::new(),
            held: false,
            sending: false,
            notice: Some(notice),
            context: None,
            programs: Vec::new(),
        };
        let waits = live.preparing.is_some() || live.limited();
        let Some(run) = live.run.as_ref().filter(|run| run.resting.is_none()).filter(|_| !waits) else {
            if waits {
                live.queued.push(queued);
                live.save_queued(&self.store)?;
                live.send_activity();
                return Ok(());
            }
            return self.start_next_turn(live, queued);
        };
        let idle = run.received_result;
        live.queued.push(queued);
        let index = live.queued.len() - 1;
        if !idle && live.steer(&self.store, index)? {
            return Ok(());
        }
        if idle && live.give(&self.store, index)? {
            return self.resume(live);
        }
        live.save_queued(&self.store)?;
        live.send_activity();
        Ok(())
    }
}

/// The agent's tool call shows the agent as it is now. `false` when it did already.
fn shows(call: &mut ToolCall, agent: Subagent) -> bool {
    let kind = call.agent.as_ref().and_then(|known| known.kind.clone());
    let agent = Some(Subagent { kind, ..agent });
    if call.agent == agent {
        return false;
    }
    call.agent = agent;
    true
}

/// Waits until the agent that started one of the agents was told how it is doing.
async fn told_of(told: &mut broadcast::Receiver<String>, ids: &[String]) {
    while let Ok(id) = told.recv().await {
        if ids.contains(&id) {
            return;
        }
    }
}

/// The tool call of the thread's turn that started an agent with `prompt`, and hasn't been
/// given that agent yet.
fn start_call(live: &Live, prompt: &str) -> Option<String> {
    let mut calls: Vec<_> = live
        .open
        .values()
        .filter(|item| item.parent.is_none())
        .filter_map(|item| match &item.kind {
            ItemKind::Tool { call } if call.name.ends_with(START_TOOL) && call.agent.is_none() => {
                Some((item.seq, call))
            }
            _ => None,
        })
        .filter(|(_, call)| {
            let input: Value = serde_json::from_str(&call.input).unwrap_or_default();
            input["prompt"].as_str().is_some_and(|given| given.trim() == prompt)
        })
        .collect();
    calls.sort_by_key(|(seq, _)| *seq);
    calls.first().map(|(_, call)| call.id.clone())
}

/// What a tool call does, on one line, as the transcript words it: "Running cargo test", with a
/// path in the thread's folder from there.
fn doing(call: &ToolCall, cwd: &str) -> String {
    let input: Value = serde_json::from_str(&call.input).unwrap_or_default();
    let name = call.name.rsplit("__").next().unwrap_or(&call.name);
    let (verb, keys): (&str, &[&str]) = match name {
        "Bash" => ("Running", &["command"]),
        "Read" => ("Reading", &["file_path"]),
        "Write" => ("Writing", &["file_path"]),
        "Edit" | "MultiEdit" | "NotebookEdit" => ("Editing", &["file_path"]),
        "Grep" | "Glob" => ("Searching for", &["pattern"]),
        "WebSearch" => ("Searching the web for", &["query"]),
        "WebFetch" => ("Fetching", &["url"]),
        "Task" | "Agent" => ("Running an agent:", &["description"]),
        _ => ("Using", &[]),
    };
    let folder = format!("{}/", cwd.trim_end_matches('/'));
    let target =
        keys.iter().find_map(|key| input[key].as_str()).map(|target| target.strip_prefix(&folder).unwrap_or(target));
    let line = match target {
        Some(target) => format!("{verb} {target}"),
        None if verb == "Using" => format!("Using {name}"),
        None => verb.to_string(),
    };
    let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
    match line.char_indices().nth(PROGRESS_CHARS) {
        Some((end, _)) => format!("{}…", &line[..end]),
        None => line,
    }
}

/// The start of a text, on one line, without Markdown's marks of code and emphasis.
fn one_line(text: &str) -> String {
    let plain = text.replace("**", "").replace('`', "");
    let line = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    match line.char_indices().nth(PROGRESS_CHARS * 2) {
        Some((end, _)) => format!("{}…", &line[..end]),
        None => line,
    }
}

fn questions_text(questions: Option<&Value>) -> String {
    let Some(questions) = questions.and_then(Value::as_array) else { return String::new() };
    let question = |question: &Value| {
        let text = question["question"].as_str().unwrap_or_default();
        let options: Vec<&str> = question["options"]
            .as_array()
            .map(|options| options.iter().filter_map(|option| option["label"].as_str()).collect())
            .unwrap_or_default();
        match options.is_empty() {
            true => format!("- {text}"),
            false => format!("- {text} Options: {}.", options.join(", ")),
        }
    };
    questions.iter().map(question).collect::<Vec<_>>().join("\n")
}

/// The reply, cut to `chars` with where to read the rest.
fn cut(reply: &str, chars: usize, thread_id: &str) -> String {
    match reply.char_indices().nth(chars) {
        Some((end, _)) => {
            format!("{}\n\n[Cut short. read_thread with thread_id {thread_id} reads all of it.]", &reply[..end])
        }
        None => reply.to_string(),
    }
}

/// When the thread's usage limit resets, in UTC, when its agent said.
fn limit_resets(thread: &Thread) -> Option<String> {
    let Some(Interruption::Limit { resets_at: Some(at), .. }) = thread.interruption else { return None };
    let at = chrono::DateTime::from_timestamp(at as i64, 0)?;
    Some(at.format("%Y-%m-%d %H:%M UTC").to_string())
}

/// Whether the thread goes on by itself once its usage limit resets.
fn limit_continues(thread: &Thread) -> bool {
    matches!(thread.interruption, Some(Interruption::Limit { continues: true, .. }))
}

fn wait_for(seconds: Option<u64>) -> Duration {
    seconds.map(Duration::from_secs).unwrap_or(WAIT).min(LONGEST_WAIT)
}

fn parse_agent(name: &str) -> anyhow::Result<Agent> {
    match name.trim().to_ascii_lowercase().as_str() {
        "claude" | "claude code" | "claude_code" => Ok(Agent::Claude),
        "codex" => Ok(Agent::Codex),
        _ => bail!("agent is claude or codex."),
    }
}

fn agent_key(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude",
        Agent::Codex => "codex",
    }
}

fn rank(access: Access) -> u8 {
    match access {
        Access::Supervised => 0,
        Access::AcceptEdits => 1,
        Access::Auto => 2,
        Access::Full => 3,
    }
}

/// The access an agent starts with: what was asked, never more than the caller's own.
fn pick_access(own: Access, wanted: Option<&str>) -> anyhow::Result<Access> {
    let Some(wanted) = wanted.map(str::trim).filter(|wanted| !wanted.is_empty()) else { return Ok(own) };
    let access: Access =
        serde_json::from_value(json!(wanted)).ok().context("access is supervised, accept_edits, auto or full.")?;
    if rank(access) > rank(own) {
        let named = serde_json::to_value(own).ok().and_then(|own| own.as_str().map(String::from)).unwrap_or_default();
        bail!("An agent you start can't have more access than you, and you have {named}.");
    }
    Ok(access)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agent_gets_at_most_the_access_of_the_one_that_starts_it() {
        assert_eq!(pick_access(Access::Auto, None).unwrap(), Access::Auto);
        assert_eq!(pick_access(Access::Auto, Some("supervised")).unwrap(), Access::Supervised);
        assert!(pick_access(Access::Auto, Some("full")).is_err());
        assert!(pick_access(Access::Full, Some("everything")).is_err());
    }

    #[test]
    fn a_long_reply_is_cut_with_where_to_read_the_rest() {
        assert_eq!(cut("short", 10, "t"), "short");
        assert!(cut(&"x".repeat(20), 10, "t").starts_with(&format!("{}\n\n[Cut short. read_thread", "x".repeat(10))));
    }

    #[test]
    fn a_tool_call_is_told_on_one_short_line() {
        let call = |name: &str, input: Value| ToolCall {
            id: String::new(),
            name: name.into(),
            input: input.to_string(),
            output: None,
            status: ToolStatus::Running,
            agent: None,
        };
        assert_eq!(doing(&call("Bash", json!({"command": "cargo\n  test"})), "/srv"), "Running cargo test");
        assert_eq!(doing(&call("mcp__motile__read_thread", json!({"thread_id": "t"})), "/srv"), "Using read_thread");
        assert!(doing(&call("Grep", json!({"pattern": "x".repeat(200)})), "/srv").ends_with('…'));
        assert_eq!(doing(&call("Read", json!({"file_path": "/srv/api/a.rs"})), "/srv/"), "Reading api/a.rs");
    }
}
