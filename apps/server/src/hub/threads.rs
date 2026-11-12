//! What an agent does through the `motile` MCP server to your server's threads, projects and pull
//! requests besides following the agents it started: it starts threads of their own, lists and
//! changes them, and reads and links their pull requests. A thread it changes has no more access
//! than it has.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, bail};
use motile_protocol::wire::{Ask, Interruption, Message, NewThread, NewWorktree, Project, Thread, ThreadChange};
use serde_json::{Value, json};

use super::children::{agent_key, parse_agent, pick_access};
use super::{Hub, NewMessage};
use crate::git;
use crate::mcp::{
    AddProject, CreateThread, LinkPullRequest, ListThreads, QueueAction, ReadPullRequest, ThreadQueue, UpdateThread,
};

const LISTED: usize = 30;
const MOST_LISTED: usize = 100;
/// How many messages a search looks through for the threads it lists.
const SEARCHED: u32 = 200;
const QUEUED_CHARS: usize = 2000;
const BODY_CHARS: usize = 6000;
const PULL_REQUEST_EVENTS: usize = 20;
const CHANGED_FILES: usize = 200;

impl Hub {
    /// Starts a thread of its own, which the sidebar lists, as the user starts one.
    pub async fn create_thread(self: &Arc<Self>, caller: &str, create: CreateThread) -> anyhow::Result<Value> {
        let text = create.prompt.trim().to_string();
        if text.is_empty() {
            bail!("Give the thread a prompt. Its agent knows nothing of your conversation.");
        }
        let request = create.request_id.clone().filter(|id| !id.is_empty()).map(|id| (caller.to_string(), id));
        if let Some(created) = request.as_ref().and_then(|request| self.lock_started().get(request).cloned()) {
            return self.thread_json_of(&created).await;
        }
        let own = self.thread(caller).await.context("Your thread is no longer on your server.")?;
        let project = match create.project.as_deref() {
            Some(named) => self.project_named(named).await?,
            None => self.project_named(&own.project_id).await?,
        };
        let agent = create.agent.as_deref().map(parse_agent).transpose()?;
        let account = self.pick_account(&own, create.account.as_deref(), agent)?;
        let (model, effort) = self.pick_model(&own, &account, create.model, create.effort)?;
        let access = pick_access(own.access, create.access.as_deref())?;
        let worktree = match create.worktree {
            true => {
                let folder = if project.id == own.project_id { &own.cwd } else { &project.path };
                let base = create.base.filter(|base| !base.trim().is_empty()).or_else(|| git::current_branch(folder));
                Some(NewWorktree {
                    base: base.context("The project has no branch for a worktree to start from.")?,
                    branch: None,
                })
            }
            false => None,
        };
        let new_thread = NewThread {
            id: None,
            project_id: project.id,
            agent: account.agent,
            agent_account: Some(account.id),
            model,
            effort,
            access,
            worktree,
            from: None,
        };
        let message = NewMessage::text(text, self.agent_actor(caller).await);
        let thread_id = self.send_started_by(None, Some(new_thread), message, Some(caller.to_string())).await?;
        if let Some(request) = request {
            self.lock_started().insert(request, thread_id.clone());
        }
        if let Some(title) = create.title.filter(|title| !title.trim().is_empty()) {
            let change = ThreadChange { title: Some(title), ..ThreadChange::default() };
            self.update(&thread_id, change, self.agent_actor(caller).await).await?;
        }
        self.thread_json_of(&thread_id).await
    }

    /// Your server's threads, the last active first, or those whose messages have the words, the
    /// best first.
    pub async fn list_threads(self: &Arc<Self>, list: ListThreads) -> anyhow::Result<Value> {
        let project_id = match list.project.as_deref() {
            Some(named) => Some(self.project_named(named).await?.id),
            None => None,
        };
        let mut threads: Vec<Thread> = self.each_thread(|live| Some(live.stored.thread.clone())).await;
        threads.sort_by(|a, b| b.updated_at.total_cmp(&a.updated_at));
        let mut excerpts = HashMap::new();
        if let Some(query) = list.query.filter(|query| !query.trim().is_empty()) {
            let hub = self.clone();
            let found =
                tokio::task::spawn_blocking(move || hub.store.search_threads_with(&query, SEARCHED, &[])).await??;
            let order: Vec<String> = found.iter().map(|found| found.thread_id.clone()).collect();
            excerpts = found.into_iter().map(|found| (found.thread_id, found.excerpt)).collect();
            threads.retain(|thread| excerpts.contains_key(&thread.id));
            threads.sort_by_key(|thread| order.iter().position(|id| *id == thread.id));
        }
        threads.retain(|thread| project_id.as_ref().is_none_or(|id| *id == thread.project_id));
        threads.retain(|thread| list.archived || thread.archived_at.is_none());
        threads.retain(|thread| list.agents || thread.parent.is_none());
        let total = threads.len();
        let offset = list.offset.unwrap_or(0);
        let limit = list.limit.unwrap_or(LISTED).clamp(1, MOST_LISTED);
        let projects = self.project_names().await;
        let listed: Vec<Value> = threads
            .iter()
            .skip(offset)
            .take(limit)
            .map(|thread| {
                let mut listed = self.thread_json(thread, &projects);
                if let Some(excerpt) = excerpts.get(&thread.id) {
                    listed["excerpt"] = json!(excerpt);
                }
                listed
            })
            .collect();
        let next = (offset + listed.len() < total).then_some(offset + listed.len());
        Ok(json!({"threads": listed, "total": total, "next_offset": next}))
    }

    /// Changes a thread's title, model, account, access, archive or snooze: the caller's own, or one
    /// it may drive.
    pub async fn update_thread(&self, caller: &str, update: UpdateThread) -> anyhow::Result<Value> {
        let thread_id = self.own_or_reached(caller, update.thread_id).await?;
        let own = self.thread(caller).await.context("Your thread is no longer on your server.")?;
        let thread = self.thread(&thread_id).await.context("That thread is no longer on your server.")?;
        let mut change = ThreadChange { title: update.title, archived: update.archived, ..ThreadChange::default() };
        if let Some(access) = update.access.as_deref() {
            change.access = Some(pick_access(own.access, Some(access))?);
        }
        if let Some(at) = update.snoozed_until.as_deref() {
            let at = chrono::DateTime::parse_from_rfc3339(at).context("snoozed_until is an RFC 3339 time.")?;
            change.snoozed_until = Some(at.timestamp() as f64);
        }
        if update.account.is_some() || update.model.is_some() || update.effort.is_some() {
            let account = match update.account.as_deref() {
                Some(named) => self.pick_account(&thread, Some(named), None)?,
                None => self.account_of(&thread),
            };
            let moves = account.id != thread.agent_account;
            let given = (update.model.is_some(), update.effort.is_some());
            let (model, effort) = self.pick_model(&thread, &account, update.model, update.effort)?;
            if moves || given.0 {
                change.model = Some(model.unwrap_or_default());
            }
            if moves || given.1 {
                change.effort = Some(effort.unwrap_or_default());
            }
            change.agent_account = moves.then_some(account.id);
        }
        self.update(&thread_id, change, self.agent_actor(caller).await).await?;
        self.thread_json_of(&thread_id).await
    }

    /// The messages that wait for a thread's agent, after sending one now or taking one back.
    pub async fn thread_queue(self: &Arc<Self>, caller: &str, queue: ThreadQueue) -> anyhow::Result<Value> {
        if queue.action != QueueAction::List {
            self.reach(caller, &queue.thread_id).await?;
            let message_id = queue.message_id.as_deref().filter(|id| !id.is_empty());
            let message_id = message_id.context("Name the message by its message_id.")?;
            match queue.action {
                QueueAction::SendNow => self.send_queued(&queue.thread_id, message_id).await?,
                _ => self.cancel_queued(&queue.thread_id, message_id).await?,
            }
        }
        let live = self.live_or(&queue.thread_id, "Your server has no thread with that id.").await?;
        let queued: Vec<Value> = live
            .queued
            .iter()
            .filter(|queued| queued.notice.is_none())
            .map(
                |queued| json!({"message_id": queued.id, "text": cut(&queued.text, QUEUED_CHARS), "held": queued.held}),
            )
            .collect();
        Ok(json!({"queued": queued}))
    }

    pub async fn projects_json(&self) -> Value {
        let projects = self.projects_of(&self.projects.lock().await);
        json!({"projects": projects.iter().map(project_json).collect::<Vec<_>>()})
    }

    /// Adds a folder of your server as a project, makes a new one, or clones one from GitHub.
    pub async fn add_project_for(&self, add: AddProject) -> anyhow::Result<Value> {
        let given = |value: Option<String>| value.filter(|value| !value.trim().is_empty());
        let project_id = match (given(add.path), given(add.name), given(add.repo)) {
            (Some(path), None, None) => self.add_project(&path).await?,
            (None, Some(name), None) => self.new_project(&name).await?,
            (None, None, Some(repo)) => self.clone_repo(&repo).await?,
            _ => bail!("Give one of path, name or repo."),
        };
        Ok(project_json(&self.project_named(&project_id).await?))
    }

    /// What GitHub says of a thread's pull request, or of another of its repository's.
    pub async fn pull_request_for(&self, caller: &str, read: ReadPullRequest) -> anyhow::Result<Value> {
        let thread_id = read.thread_id.filter(|id| !id.is_empty()).unwrap_or_else(|| caller.to_string());
        let thread = self.thread(&thread_id).await.context("Your server has no thread with that id.")?;
        let linked = thread.pull_request.as_ref().map(|pull_request| pull_request.number);
        let number = read.number.or(linked).context("That thread has no pull request. Give its number.")?;
        let Message::PullRequest { pull_request } =
            self.pull_request(&thread.project_id, Some(&thread_id), number).await?
        else {
            bail!("GitHub didn't say what pull request #{number} is.");
        };
        let mut read = serde_json::to_value(&*pull_request)?;
        let Some(fields) = read.as_object_mut() else { return Ok(read) };
        for unwanted in ["id", "viewer", "merge_methods", "auto_merge_allowed", "repository_labels"] {
            fields.remove(unwanted);
        }
        if let Some(body) = fields.get("body").and_then(Value::as_str) {
            fields.insert("body".into(), json!(cut(body, BODY_CHARS)));
        }
        if let Some(Value::Array(events)) = fields.get_mut("activity") {
            let shown = events.len().saturating_sub(PULL_REQUEST_EVENTS);
            events.drain(..shown);
        }
        Ok(read)
    }

    /// Links a pull request to a thread or unlinks its own, and has its agent told what happens
    /// on it, or no longer.
    pub async fn link_pull_request_for(&self, caller: &str, link: LinkPullRequest) -> anyhow::Result<Value> {
        let thread_id = self.own_or_reached(caller, link.thread_id).await?;
        if link.unlink {
            self.link_pull_request(&thread_id, None).await?;
        } else if let Some(number) = link.number {
            self.link_pull_request(&thread_id, Some(number)).await?;
        }
        if let Some(watch) = link.watch {
            self.watch_pull_request(&thread_id, watch).await?;
        }
        self.thread_json_of(&thread_id).await
    }

    /// What git says about the folder a thread works in.
    pub async fn git_status_for(&self, caller: &str, thread_id: Option<String>, fetch: bool) -> anyhow::Result<Value> {
        let thread_id = thread_id.filter(|id| !id.is_empty()).unwrap_or_else(|| caller.to_string());
        let thread = self.thread(&thread_id).await.context("Your server has no thread with that id.")?;
        let Message::GitStatus { status, mut files, problem } =
            self.git_status(&thread.project_id, Some(&thread_id), fetch).await?
        else {
            bail!("Git didn't say how the folder is.");
        };
        let more = files.len().saturating_sub(CHANGED_FILES);
        files.truncate(CHANGED_FILES);
        let mut read = json!({"folder": self.git_folder(&thread.project_id, Some(&thread_id)).await?,
                              "status": status, "files": files});
        if more > 0 {
            read["more_files"] = json!(more);
        }
        if let Some(problem) = problem {
            read["problem"] = json!(problem);
        }
        Ok(read)
    }

    /// The thread to change: the caller's own when none is named, or another it may drive.
    async fn own_or_reached(&self, caller: &str, thread_id: Option<String>) -> anyhow::Result<String> {
        let Some(thread_id) = thread_id.filter(|id| !id.is_empty() && id != caller) else {
            return Ok(caller.to_string());
        };
        self.reach(caller, &thread_id).await?;
        Ok(thread_id)
    }

    /// A project by its id, its name or its folder.
    async fn project_named(&self, named: &str) -> anyhow::Result<Project> {
        let named = named.trim();
        let projects = self.projects_of(&self.projects.lock().await);
        let by_id = projects.iter().find(|project| project.id == named || project.path == named);
        let found = by_id.or_else(|| projects.iter().find(|project| project.name.eq_ignore_ascii_case(named)));
        found.cloned().with_context(|| format!("Your server has no project {named}. list_projects lists them."))
    }

    async fn thread_json_of(&self, thread_id: &str) -> anyhow::Result<Value> {
        let thread = self.thread(thread_id).await.context("That thread is no longer on your server.")?;
        Ok(self.thread_json(&thread, &self.project_names().await))
    }

    fn thread_json(&self, thread: &Thread, projects: &HashMap<String, String>) -> Value {
        let mut read = json!({
            "id": thread.id,
            "title": thread.title,
            "project": projects.get(&thread.project_id),
            "folder": thread.cwd,
            "agent": agent_key(thread.agent),
            "account": self.account_of(thread).name,
            "model": thread.model,
            "effort": thread.effort,
            "access": thread.access,
            "status": status_of(thread),
            "created_at": time_text(thread.created_at),
            "updated_at": time_text(thread.updated_at),
        });
        if thread.archived_at.is_some() {
            read["archived"] = json!(true);
        }
        if let Some(at) = thread.snoozed_until {
            read["snoozed_until"] = json!(time_text(at));
        }
        if let Some(pull_request) = &thread.pull_request {
            read["pull_request"] = json!({"number": pull_request.number, "title": pull_request.title,
                                         "url": pull_request.url, "merged": pull_request.merged,
                                         "closed": pull_request.closed, "watching": thread.watching});
        }
        if let Some(parent) = &thread.parent {
            read["started_by"] = json!(parent.thread_id);
        }
        read
    }
}

fn status_of(thread: &Thread) -> &'static str {
    match (thread.needs_approval, thread.asking) {
        (true, Some(Ask::Question)) => "asking",
        (true, _) => "needs_approval",
        _ if thread.running => "running",
        _ if thread.monitoring => "monitoring",
        _ if matches!(thread.interruption, Some(Interruption::Limit { .. })) => "waiting_for_usage_limit",
        _ => "idle",
    }
}

fn project_json(project: &Project) -> Value {
    json!({"id": project.id, "name": project.name, "folder": project.path, "branch": project.branch})
}

fn time_text(at: f64) -> String {
    chrono::DateTime::from_timestamp(at as i64, 0).unwrap_or_default().to_rfc3339()
}

fn cut(text: &str, chars: usize) -> String {
    match text.char_indices().nth(chars) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}
