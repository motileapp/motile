//! What the app keeps of what the core sends: its views of servers, projects and threads, with
//! the lookups the screens need, and what is read out of the answers to commands.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use motile_core::api::{ProjectView, ServerView, ThreadView};
use motile_core::connection::PathKind;
use motile_core::git::Control;
use motile_core::link::State;
use motile_core::render::agents::AgentView;
use motile_core::render::rows::Waiting;
use motile_protocol::wire::{Access, Agent, GitStatus, ModelInfo, PullRequestSettings, Queued, Thread};
use serde::Deserialize;

pub fn agent_name(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "Claude Code",
        Agent::Codex => "Codex",
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub state: State,
    pub error: Option<String>,
    /// "relay" or "direct".
    pub path: Option<String>,
    pub rtt_ms: Option<u64>,
    pub home: String,
    /// The version of the server's program.
    pub version: String,
    pub protocol_version: u32,
    pub models: Vec<ModelInfo>,
    /// The model picked to write titles, commit messages and pull requests there.
    pub text_model: Option<String>,
    /// How the writer there is told to name branches, and what that is until it is changed.
    pub branch_instructions: String,
    pub default_branch_instructions: String,
    pub pull_request_settings: PullRequestSettings,
    /// The agents installed on the server, with their versions.
    pub agents: HashMap<Agent, String>,
    /// Whether the server has ever told us about itself.
    pub known: bool,
}

impl From<ServerView> for Server {
    fn from(view: ServerView) -> Self {
        let info = view.info;
        let path = view.path.map(|path| match path {
            PathKind::Relay => "relay".to_string(),
            PathKind::Direct => "direct".to_string(),
        });
        Self {
            id: view.id,
            name: view.name,
            platform: view.platform,
            state: view.state,
            error: view.error,
            path,
            rtt_ms: view.rtt_ms,
            known: info.is_some(),
            home: info.as_ref().map(|info| info.home.clone()).unwrap_or_default(),
            version: info.as_ref().map(|info| info.version.clone()).unwrap_or_default(),
            protocol_version: info.as_ref().map_or(0, |info| info.protocol),
            models: info.as_ref().map(|info| info.models.clone()).unwrap_or_default(),
            text_model: info.as_ref().and_then(|info| info.text_model.clone()),
            branch_instructions: info.as_ref().map(|info| info.branch_instructions.text.clone()).unwrap_or_default(),
            default_branch_instructions: info
                .as_ref()
                .map(|info| info.branch_instructions.default.clone())
                .unwrap_or_default(),
            pull_request_settings: info.as_ref().map(|info| info.pull_request_settings).unwrap_or_default(),
            agents: info
                .map(|info| info.agents.into_iter().filter_map(|agent| Some((agent.agent, agent.version?))).collect())
                .unwrap_or_default(),
        }
    }
}

impl Server {
    pub fn connected(&self) -> bool {
        self.state == State::Connected
    }
}

/// A git worktree of a project, made for the thread whose `cwd` it is.
#[derive(Clone, PartialEq, Debug)]
pub struct Worktree {
    pub path: String,
    pub branch: Option<String>,
    pub git: Option<GitStatus>,
    pub git_control: Option<Control>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Project {
    pub id: String,
    pub server_id: String,
    pub path: String,
    pub name: String,
    pub branch: Option<String>,
    /// What its server last read from git there.
    pub git: Option<GitStatus>,
    /// The git button for its repository.
    pub git_control: Option<Control>,
    /// The worktree of the thread the project is seen from, when it works in one.
    pub worktree: Option<Worktree>,
    pub worktrees: Vec<Worktree>,
    /// The shell script that runs in every new worktree.
    pub setup: Option<String>,
    /// The icon as a file on this device, once the core has fetched it.
    pub icon_path: Option<String>,
    pub created_at: f64,
}

impl Project {
    pub fn new(view: ProjectView, server_id: &str) -> Self {
        let mut controls = view.worktree_controls;
        let project = view.project;
        Self {
            worktrees: project
                .worktrees
                .into_iter()
                .map(|worktree| Worktree {
                    git_control: controls.remove(&worktree.path),
                    path: worktree.path,
                    branch: worktree.branch,
                    git: worktree.git,
                })
                .collect(),
            id: project.id,
            server_id: server_id.to_string(),
            path: project.path,
            name: project.name,
            branch: project.branch,
            git: project.git,
            git_control: view.git_control,
            worktree: None,
            setup: project.setup,
            icon_path: view.icon_path,
            created_at: project.created_at,
        }
    }

    /// Names the checkout git runs in from here: the project's folder or the thread's worktree.
    pub fn checkout_id(&self) -> String {
        format!("{}:{}", self.id, self.worktree.as_ref().map(|worktree| worktree.path.as_str()).unwrap_or(&self.path))
    }

    /// The project as the thread works in it: with the branch and the git state of its worktree,
    /// when it has one.
    pub fn seen(&self, thread: &ThreadInfo) -> Project {
        let Some(worktree) = self.worktrees.iter().find(|worktree| worktree.path == thread.cwd) else {
            return self.clone();
        };
        let mut seen = self.clone();
        seen.branch = worktree.branch.clone();
        seen.git = worktree.git.clone();
        seen.git_control = worktree.git_control.clone();
        seen.worktree = Some(worktree.clone());
        seen
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct ThreadInfo {
    pub thread: Thread,
    pub server_id: String,
    pub unread: bool,
}

impl From<ThreadView> for ThreadInfo {
    fn from(view: ThreadView) -> Self {
        Self { thread: view.thread, server_id: view.server_id, unread: view.unread }
    }
}

impl std::ops::Deref for ThreadInfo {
    type Target = Thread;

    fn deref(&self) -> &Thread {
        &self.thread
    }
}

impl ThreadInfo {
    pub fn is_done(&self) -> bool {
        self.done_at.is_some()
    }

    /// The agent's process is still there, working or monitoring.
    pub fn busy(&self) -> bool {
        self.running || self.monitoring
    }

    /// Active threads keep their place when something happens in them; only coming back from
    /// done moves one to the top.
    pub fn active_order(&self) -> f64 {
        self.created_at.max(self.undone_at.unwrap_or(0.))
    }
}

pub fn access_label(access: Access) -> &'static str {
    match access {
        Access::Supervised => "Supervised",
        Access::AcceptEdits => "Auto-accept edits",
        Access::Auto => "Auto",
        Access::Full => "Full access",
    }
}

pub fn access_detail(access: Access) -> &'static str {
    match access {
        Access::Supervised => "Ask before commands and file changes.",
        Access::AcceptEdits => "Auto-approve edits, ask before other actions.",
        Access::Auto => "The agent approves routine actions itself.",
        Access::Full => "Allow commands and edits without prompts.",
    }
}

pub const ACCESSES: [Access; 4] = [Access::Supervised, Access::AcceptEdits, Access::Auto, Access::Full];

/// What the open thread's agent is doing, and what waits for the user.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Activity {
    pub running: bool,
    pub monitoring: bool,
    pub thinking: bool,
    /// The agent is making its conversation shorter to go on with it.
    pub compacting: bool,
    /// How many agents it has started that still work.
    pub agents: u32,
    pub started_at: Option<f64>,
    /// The tool calls the running turn waits with until they are allowed or refused.
    pub approvals: Vec<Waiting>,
    /// The messages that wait for the agent to take them.
    pub queued: Vec<Queued>,
}

impl Activity {
    pub fn new(activity: motile_protocol::wire::Activity, waiting: Vec<Waiting>) -> Self {
        Self {
            running: activity.running,
            monitoring: activity.monitoring,
            thinking: activity.thinking,
            compacting: activity.compacting,
            agents: activity.agents,
            started_at: activity.started_at,
            approvals: waiting,
            queued: activity.queued,
        }
    }

    pub fn busy(&self) -> bool {
        self.running || self.monitoring
    }

    /// What is shown between sending a first message and the server saying the turn runs.
    pub fn starting() -> Self {
        Self { running: true, started_at: Some(now()), ..Self::default() }
    }
}

pub trait AgentViewExt {
    fn working(&self) -> bool;
    fn usage(&self) -> Option<String>;
}

impl AgentViewExt for AgentView {
    fn working(&self) -> bool {
        self.status == motile_protocol::wire::ToolStatus::Running
    }

    /// "3 tools · 21k tokens", as far as either is known.
    fn usage(&self) -> Option<String> {
        let tools = self.tool_uses.map(|count| format!("{count} {}", if count == 1 { "tool" } else { "tools" }));
        let spent = self.tokens.map(|tokens| {
            if tokens < 1000 { format!("{tokens} tokens") } else { format!("{}k tokens", tokens / 1000) }
        });
        let parts: Vec<String> = [tools, spent].into_iter().flatten().collect();
        (!parts.is_empty()).then(|| parts.join(" · "))
    }
}

/// A file attached to a message, as the transcript shows it. An image or a video names the file
/// the core has it under, and a video the image that stands for it until it plays.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct AttachedFile {
    pub name: String,
    pub media: Option<String>,
    pub video: bool,
    pub poster: Option<String>,
}

impl AttachedFile {
    /// The image a tile of it shows.
    pub fn picture(&self) -> Option<&String> {
        if self.video { self.poster.as_ref() } else { self.media.as_ref() }
    }
}

impl From<&motile_core::render::rows::Attached> for AttachedFile {
    fn from(attached: &motile_core::render::rows::Attached) -> Self {
        Self {
            name: attached.name.clone(),
            media: attached.media.clone(),
            video: attached.video,
            poster: attached.poster.clone(),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum UploadState {
    Uploading(f64),
    Ready,
    Failed(String),
}

/// A file in the composer: on its way to the server, or there and ready to be sent.
#[derive(Clone, PartialEq, Debug)]
pub struct Attachment {
    pub id: String,
    /// Where it is on this device. A file that came back from a queued message is only on its
    /// server.
    pub file: Option<PathBuf>,
    pub name: String,
    pub bytes: Option<u64>,
    pub video: bool,
    /// Shown as a tile with its picture, not by its name.
    pub pictured: bool,
    pub server_id: String,
    pub state: UploadState,
    /// Where it is on the server, once it is there.
    pub path: Option<String>,
    pub media: Option<String>,
    pub poster: Option<String>,
}

pub fn is_video(path: &Path) -> bool {
    let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or_default().to_lowercase();
    matches!(extension.as_str(), "mov" | "mp4" | "m4v" | "avi" | "mkv" | "webm" | "mpg" | "mpeg" | "3gp")
}

pub fn is_image(path: &Path) -> bool {
    let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or_default().to_lowercase();
    matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" | "bmp" | "tiff" | "tif" | "ico" | "svg"
    )
}

impl Attachment {
    pub fn from_file(file: PathBuf, server_id: &str) -> Self {
        let video = is_video(&file);
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: file.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default(),
            bytes: std::fs::metadata(&file).ok().map(|metadata| metadata.len()),
            video,
            pictured: video || is_image(&file),
            file: Some(file),
            server_id: server_id.to_string(),
            state: UploadState::Uploading(0.),
            path: None,
            media: None,
            poster: None,
        }
    }

    pub fn from_server(path: &str, shown: Option<&AttachedFile>, server_id: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            file: None,
            name: Path::new(path).file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default(),
            bytes: None,
            video: shown.is_some_and(|shown| shown.video),
            pictured: shown.is_some(),
            server_id: server_id.to_string(),
            state: UploadState::Ready,
            path: Some(path.to_string()),
            media: shown.and_then(|shown| shown.media.clone()),
            poster: shown.and_then(|shown| shown.poster.clone()),
        }
    }

    pub fn attached(&self) -> AttachedFile {
        AttachedFile {
            name: self.name.clone(),
            media: self.media.clone(),
            video: self.media.is_some() && self.video,
            poster: self.poster.clone(),
        }
    }

    pub fn viewed(&self) -> Option<ViewedMedia> {
        if let Some(file) = &self.file {
            return Some(ViewedMedia {
                name: self.name.clone(),
                video: self.video,
                source: MediaSource::File(file.clone()),
            });
        }
        self.media.as_ref().map(|media| ViewedMedia {
            name: self.name.clone(),
            video: self.video,
            source: MediaSource::Media(media.clone()),
        })
    }
}

/// An image or a video the viewer shows: a file on this device, or one the core has or fetches.
#[derive(Clone, PartialEq, Debug)]
pub enum MediaSource {
    File(PathBuf),
    Media(String),
}

#[derive(Clone, PartialEq, Debug)]
pub struct ViewedMedia {
    pub name: String,
    pub video: bool,
    pub source: MediaSource,
}

/// What the viewer has open: the images and videos of one message or of the composer, and
/// which of them is shown.
#[derive(Clone, PartialEq, Debug)]
pub struct Viewing {
    pub items: Vec<ViewedMedia>,
    pub index: usize,
}

impl Viewing {
    pub fn item(&self) -> &ViewedMedia {
        &self.items[self.index]
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct EnrollToken {
    pub command: String,
    pub expires_at: f64,
}

/// The folders of a server under the path typed in the panel.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct FolderListing {
    pub path: String,
    pub typed: String,
    pub parent: Option<String>,
    pub folders: Vec<ListedFolder>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ListedFolder {
    pub name: String,
    pub path: String,
    /// What to type to look inside it.
    pub typed: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct RemoteFolder {
    pub path: String,
    pub parent: Option<String>,
    #[serde(default)]
    pub folders: Vec<String>,
    /// The images in the folder, when an icon is being chosen.
    #[serde(default)]
    pub files: Vec<String>,
}

pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        .unwrap_or(0.)
}

/// "now", "5m", "3h", "2d": how long ago, as short as a sidebar needs.
pub fn ago(timestamp: f64) -> String {
    let seconds = (now() - timestamp).max(0.);
    if seconds < 60. {
        return "now".into();
    }
    if seconds < 3600. {
        return format!("{}m", (seconds / 60.) as u64);
    }
    if seconds < 86400. {
        return format!("{}h", (seconds / 3600.) as u64);
    }
    format!("{}d", (seconds / 86400.) as u64)
}

/// "850ms", "12s", "3m 5s", "1h 2m".
pub fn duration(milliseconds: u64) -> String {
    if milliseconds < 1000 {
        return format!("{milliseconds}ms");
    }
    let seconds = milliseconds / 1000;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    if seconds < 3600 {
        return format!("{}m {}s", seconds / 60, seconds % 60);
    }
    format!("{}h {}m", seconds / 3600, seconds % 3600 / 60)
}

/// A running timer: "5s", "12m 3s", "1h 3m".
pub fn elapsed(since: f64) -> String {
    let seconds = (now() - since).max(0.) as u64;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    if seconds < 3600 {
        return format!("{}m {}s", seconds / 60, seconds % 60);
    }
    format!("{}h {}m", seconds / 3600, seconds % 3600 / 60)
}

/// Whether `version` comes before `other`, comparing their numbers from the left.
pub fn is_older(version: &str, other: Option<&str>) -> bool {
    let Some(other) = other else { return false };
    if version.is_empty() || other.is_empty() {
        return false;
    }
    let numbers = |text: &str| text.split('.').map(|part| part.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (ours, theirs) = (numbers(version), numbers(other));
    for index in 0..ours.len().max(theirs.len()) {
        let left = ours.get(index).copied().unwrap_or(0);
        let right = theirs.get(index).copied().unwrap_or(0);
        if left != right {
            return left < right;
        }
    }
    false
}

pub fn last_component(path: &str) -> String {
    Path::new(path).file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_their_numbers() {
        assert!(is_older("0.1.9", Some("0.1.12")));
        assert!(!is_older("0.1.12", Some("0.1.12")));
        assert!(!is_older("0.2", Some("0.1.12")));
        assert!(is_older("0.1", Some("0.1.1")));
        assert!(!is_older("", Some("0.1.1")));
        assert!(!is_older("0.1", None));
    }

    #[test]
    fn durations_read_as_short_as_they_can() {
        assert_eq!(duration(850), "850ms");
        assert_eq!(duration(12_000), "12s");
        assert_eq!(duration(185_000), "3m 5s");
        assert_eq!(duration(3_720_000), "1h 2m");
    }
}
