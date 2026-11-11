//! The features the activity counts: what a request of a client or a motile tool call of an agent
//! uses, and what each is called where it is shown.

use crate::wire::{NewThread, Request, ThreadChange};

/// A feature: its id as kept, what it is called, and the group it is shown in.
pub struct Feature {
    pub id: &'static str,
    pub label: &'static str,
    pub group: Group,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Threads,
    Git,
    PullRequests,
    Programs,
    Projects,
    Integrations,
    Settings,
    Agents,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Threads => "Threads",
            Group::Git => "Git",
            Group::PullRequests => "Pull requests",
            Group::Programs => "Programs",
            Group::Projects => "Projects",
            Group::Integrations => "Issue trackers",
            Group::Settings => "Settings",
            Group::Agents => "Agents",
        }
    }
}

const fn feature(id: &'static str, label: &'static str, group: Group) -> Feature {
    Feature { id, label, group }
}

pub const MESSAGE: &str = "message";
/// What an agent's call of a motile tool is kept as: this, then the tool's name.
pub const AGENT_TOOL: &str = "agent.";

pub const FEATURES: &[Feature] = &[
    feature(MESSAGE, "Messages sent", Group::Threads),
    feature("thread.new", "Threads started", Group::Threads),
    feature("thread.archive", "Threads archived", Group::Threads),
    feature("thread.unarchive", "Threads brought back", Group::Threads),
    feature("thread.snooze", "Threads snoozed", Group::Threads),
    feature("thread.rename", "Threads renamed", Group::Threads),
    feature("thread.move", "Threads reordered", Group::Threads),
    feature("thread.model", "Models changed", Group::Threads),
    feature("thread.effort", "Efforts changed", Group::Threads),
    feature("thread.access", "Access changed", Group::Threads),
    feature("thread.account", "Accounts switched", Group::Threads),
    feature("thread.continue", "Threads continued", Group::Threads),
    feature("thread.delete", "Threads deleted", Group::Threads),
    feature("turn.stop", "Turns stopped", Group::Threads),
    feature("queue.send_now", "Queued messages sent now", Group::Threads),
    feature("queue.cancel", "Queued messages removed", Group::Threads),
    feature("approval", "Approvals answered", Group::Threads),
    feature("search", "Searches", Group::Threads),
    feature("file.upload", "Files attached", Group::Threads),
    feature("git.commit", "Commits", Group::Git),
    feature("git.push", "Pushes", Group::Git),
    feature("git.commit_push", "Commits and pushes", Group::Git),
    feature("git.commit_push_pr", "Commits, pushes and pull requests", Group::Git),
    feature("git.create_pr", "Pull requests opened", Group::Git),
    feature("git.pull", "Pulls", Group::Git),
    feature("git.branch", "Branches switched", Group::Git),
    feature("git.worktree", "Worktrees started", Group::Git),
    feature("git.init", "Repositories made", Group::Git),
    feature("pull_request.merge", "Pull requests merged", Group::PullRequests),
    feature("pull_request.enable_auto_merge", "Auto-merges turned on", Group::PullRequests),
    feature("pull_request.disable_auto_merge", "Auto-merges turned off", Group::PullRequests),
    feature("pull_request.ready", "Marked ready for review", Group::PullRequests),
    feature("pull_request.draft", "Turned into drafts", Group::PullRequests),
    feature("pull_request.close", "Pull requests closed", Group::PullRequests),
    feature("pull_request.reopen", "Pull requests reopened", Group::PullRequests),
    feature("pull_request.update_branch", "Branches updated", Group::PullRequests),
    feature("pull_request.revert", "Pull requests reverted", Group::PullRequests),
    feature("pull_request.comment", "Comments", Group::PullRequests),
    feature("pull_request.approve", "Approvals", Group::PullRequests),
    feature("pull_request.request_changes", "Changes requested", Group::PullRequests),
    feature("pull_request.edit", "Pull requests edited", Group::PullRequests),
    feature("pull_request.link", "Pull requests linked", Group::PullRequests),
    feature("pull_request.watch", "Pull requests watched", Group::PullRequests),
    feature("program.message", "Messages from programs", Group::Programs),
    feature("program.tool", "Program tools run", Group::Programs),
    feature("project.add", "Projects added", Group::Projects),
    feature("project.remove", "Projects removed", Group::Projects),
    feature("project.setup", "Setup scripts changed", Group::Projects),
    feature("linear.update", "Linear issues changed", Group::Integrations),
    feature("linear.comment", "Linear comments", Group::Integrations),
    feature("linear.create", "Linear issues filed", Group::Integrations),
    feature("ricrac.update", "Ricrac issues changed", Group::Integrations),
    feature("ricrac.comment", "Ricrac comments", Group::Integrations),
    feature("ricrac.create", "Ricrac issues filed", Group::Integrations),
    feature("account.save", "Agent accounts added or changed", Group::Settings),
    feature("account.sign_in", "Agent accounts signed in", Group::Settings),
    feature("backup.set", "Backups set up", Group::Settings),
    feature("server.update", "Servers updated", Group::Settings),
    feature("agent.start_agent", "Agents started", Group::Agents),
    feature("agent.agent_status", "Agents checked on", Group::Agents),
    feature("agent.message_agent", "Agents messaged", Group::Agents),
    feature("agent.answer_agent", "Agents answered", Group::Agents),
    feature("agent.approve_agent", "Agents' tool calls allowed", Group::Agents),
    feature("agent.stop_agent", "Agents stopped", Group::Agents),
    feature("agent.list_accounts", "Accounts listed", Group::Agents),
    feature("agent.create_thread", "Threads created", Group::Agents),
    feature("agent.list_threads", "Threads listed", Group::Agents),
    feature("agent.update_thread", "Threads updated", Group::Agents),
    feature("agent.thread_queue", "Queues managed", Group::Agents),
    feature("agent.read_thread", "Threads read", Group::Agents),
    feature("agent.list_projects", "Projects listed", Group::Agents),
    feature("agent.add_project", "Projects added", Group::Agents),
    feature("agent.read_pull_request", "Pull requests read", Group::Agents),
    feature("agent.link_pull_request", "Pull requests linked", Group::Agents),
    feature("agent.git_status", "Git statuses read", Group::Agents),
    feature("agent.openui_guide", "OpenUI guide read", Group::Agents),
    feature("agent.check_ui", "Programs checked", Group::Agents),
];

pub fn find(id: &str) -> Option<&'static Feature> {
    FEATURES.iter().find(|feature| feature.id == id)
}

/// A use of a feature: its id, how it was used, and the thread it was used in.
#[derive(Clone, PartialEq, Debug)]
pub struct Use {
    pub feature: &'static str,
    pub variant: String,
    pub thread_id: Option<String>,
    pub project_id: Option<String>,
}

impl Use {
    fn of(feature: &'static str) -> Self {
        Self { feature, variant: String::new(), thread_id: None, project_id: None }
    }

    fn variant(mut self, variant: impl Into<String>) -> Self {
        self.variant = variant.into();
        self
    }

    fn thread(mut self, thread_id: Option<&String>) -> Self {
        self.thread_id = thread_id.cloned();
        self
    }

    fn project(mut self, project_id: &str) -> Self {
        self.project_id = Some(project_id.to_string());
        self
    }
}

/// The features a client's request uses, once it succeeded. Reading, following and housekeeping
/// use none.
pub fn uses(request: &Request) -> Vec<Use> {
    let one = |used: Use| vec![used];
    match request {
        Request::Send { thread_id, new_thread, now, context, .. } => {
            let variant = match (new_thread, now, context) {
                (Some(_), _, _) => "new",
                (None, _, Some(_)) => "program",
                (None, true, None) => "now",
                (None, false, None) => "reply",
            };
            let mut used = vec![Use::of(MESSAGE).variant(variant).thread(thread_id.as_ref())];
            if context.is_some() {
                used.push(Use::of("program.message").thread(thread_id.as_ref()));
            }
            if let Some(new_thread) = new_thread {
                used.push(Use::of("thread.new").variant(new_thread_kind(new_thread)).project(&new_thread.project_id));
            }
            used
        }
        Request::Update { thread_id, change } => {
            changes(change).into_iter().map(|feature| Use::of(feature).thread(Some(thread_id))).collect()
        }
        Request::Answer { thread_id, allow, answers, .. } => {
            let variant = match (!answers.is_empty(), allow) {
                (true, _) => "answer",
                (false, true) => "allow",
                (false, false) => "refuse",
            };
            one(Use::of("approval").variant(variant).thread(Some(thread_id)))
        }
        Request::Stop { thread_id } => one(Use::of("turn.stop").thread(Some(thread_id))),
        Request::Continue { thread_id } => one(Use::of("thread.continue").thread(Some(thread_id))),
        Request::Delete { thread_id } => one(Use::of("thread.delete").thread(Some(thread_id))),
        Request::SendQueued { thread_id, .. } => one(Use::of("queue.send_now").thread(Some(thread_id))),
        Request::CancelQueued { thread_id, .. } => one(Use::of("queue.cancel").thread(Some(thread_id))),
        Request::SearchThreads { .. } => one(Use::of("search")),
        Request::Upload { .. } => one(Use::of("file.upload")),
        Request::UiTool { thread_id, tool, .. } => {
            one(Use::of("program.tool").variant(tool.as_str()).thread(Some(thread_id)))
        }
        Request::GitRun { project_id, action, thread_id, .. } => {
            let feature = match action {
                crate::wire::GitAction::Commit => "git.commit",
                crate::wire::GitAction::Push => "git.push",
                crate::wire::GitAction::CommitPush => "git.commit_push",
                crate::wire::GitAction::CommitPushPr => "git.commit_push_pr",
                crate::wire::GitAction::CreatePr => "git.create_pr",
                crate::wire::GitAction::Pull => "git.pull",
            };
            one(Use::of(feature).thread(thread_id.as_ref()).project(project_id))
        }
        Request::SwitchBranch { project_id, .. } => one(Use::of("git.branch").project(project_id)),
        Request::WorktreeStart { project_id, .. } => one(Use::of("git.worktree").project(project_id)),
        Request::InitRepository { project_id } => one(Use::of("git.init").project(project_id)),
        Request::PullRequestAction { project_id, thread_id, action, .. } => {
            let feature = match action {
                crate::wire::PullRequestAction::Merge => "pull_request.merge",
                crate::wire::PullRequestAction::EnableAutoMerge => "pull_request.enable_auto_merge",
                crate::wire::PullRequestAction::DisableAutoMerge => "pull_request.disable_auto_merge",
                crate::wire::PullRequestAction::Ready => "pull_request.ready",
                crate::wire::PullRequestAction::Draft => "pull_request.draft",
                crate::wire::PullRequestAction::Close => "pull_request.close",
                crate::wire::PullRequestAction::Reopen => "pull_request.reopen",
                crate::wire::PullRequestAction::UpdateBranch => "pull_request.update_branch",
                crate::wire::PullRequestAction::Revert => "pull_request.revert",
                crate::wire::PullRequestAction::Comment => "pull_request.comment",
                crate::wire::PullRequestAction::Approve => "pull_request.approve",
                crate::wire::PullRequestAction::RequestChanges => "pull_request.request_changes",
            };
            one(Use::of(feature).thread(thread_id.as_ref()).project(project_id))
        }
        Request::PullRequestEdit { project_id, .. } => one(Use::of("pull_request.edit").project(project_id)),
        Request::LinkPullRequest { thread_id, .. } => one(Use::of("pull_request.link").thread(Some(thread_id))),
        Request::WatchPullRequest { thread_id, watch } => {
            one(Use::of("pull_request.watch").variant(if *watch { "on" } else { "off" }).thread(Some(thread_id)))
        }
        Request::AddProject { .. } => one(Use::of("project.add").variant("folder")),
        Request::NewProject { .. } => one(Use::of("project.add").variant("new")),
        Request::CloneRepo { .. } => one(Use::of("project.add").variant("clone")),
        Request::RemoveProject { project_id } => one(Use::of("project.remove").project(project_id)),
        Request::SetProjectSetup { project_id, .. } => one(Use::of("project.setup").project(project_id)),
        Request::LinearUpdate { .. } => one(Use::of("linear.update")),
        Request::LinearComment { .. } => one(Use::of("linear.comment")),
        Request::LinearCreate { .. } => one(Use::of("linear.create")),
        Request::RicracUpdate { .. } => one(Use::of("ricrac.update")),
        Request::RicracComment { .. } => one(Use::of("ricrac.comment")),
        Request::RicracCreate { .. } => one(Use::of("ricrac.create")),
        Request::SaveAgentAccount { account } => {
            one(Use::of("account.save").variant(if account.id.is_empty() { "new" } else { "change" }))
        }
        Request::StartSignIn { .. } => one(Use::of("account.sign_in")),
        Request::SetBackup { bucket } => {
            one(Use::of("backup.set").variant(if bucket.is_some() { "on" } else { "off" }))
        }
        Request::UpdateServer { .. } => one(Use::of("server.update")),
        _ => vec![],
    }
}

/// The feature an agent's call of a motile tool uses, by the tool's name as the agent calls it.
pub fn agent_tool(name: &str) -> Option<&'static str> {
    let tool = name.strip_prefix("mcp__motile__")?;
    FEATURES.iter().map(|feature| feature.id).find(|id| id.strip_prefix(AGENT_TOOL) == Some(tool))
}

fn new_thread_kind(new_thread: &NewThread) -> &'static str {
    match (&new_thread.from, &new_thread.worktree) {
        (Some(_), _) => "copy",
        (None, Some(_)) => "worktree",
        (None, None) => "local",
    }
}

fn changes(change: &ThreadChange) -> Vec<&'static str> {
    let mut features = Vec::new();
    let mut add = |set: bool, feature| {
        if set {
            features.push(feature);
        }
    };
    add(change.title.is_some(), "thread.rename");
    add(change.model.is_some(), "thread.model");
    add(change.effort.is_some(), "thread.effort");
    add(change.access.is_some(), "thread.access");
    add(change.archived == Some(true), "thread.archive");
    add(change.archived == Some(false), "thread.unarchive");
    add(change.snoozed_until.is_some(), "thread.snooze");
    add(change.position.is_some(), "thread.move");
    add(change.agent_account.is_some(), "thread.account");
    features
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_use_names_a_known_feature() {
        let send = |new_thread: bool, now: bool| Request::Send {
            thread_id: (!new_thread).then(|| "t".to_string()),
            new_thread: new_thread.then(|| {
                serde_json::from_value(serde_json::json!({
                    "project_id": "p", "agent": "claude", "model": null, "effort": null, "access": "full"
                }))
                .unwrap()
            }),
            text: "hi".into(),
            attachments: vec![],
            now,
            context: None,
            message_id: None,
        };
        let new = uses(&send(true, false));
        assert_eq!(
            new.iter().map(|used| (used.feature, used.variant.as_str())).collect::<Vec<_>>(),
            [("message", "new"), ("thread.new", "local")]
        );
        assert_eq!(uses(&send(false, true))[0].variant, "now");
        assert!(uses(&Request::Ping).is_empty());
        let archive = Request::Update {
            thread_id: "t".into(),
            change: serde_json::from_value(serde_json::json!({"done": true, "position": 3.0})).unwrap(),
        };
        let features: Vec<_> = uses(&archive).iter().map(|used| used.feature).collect();
        assert_eq!(features, ["thread.archive", "thread.move"]);
        for used in [new, uses(&archive)].concat() {
            assert!(find(used.feature).is_some(), "{} isn't listed", used.feature);
        }
    }

    #[test]
    fn an_agents_motile_tools_are_features_and_other_tools_are_not() {
        assert_eq!(agent_tool("mcp__motile__start_agent"), Some("agent.start_agent"));
        assert_eq!(agent_tool("mcp__linear__list_issues"), None);
        assert_eq!(agent_tool("Bash"), None);
    }
}
