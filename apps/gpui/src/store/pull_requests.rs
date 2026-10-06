//! The pull request tabs' state: the pull request a tab shows as the core worked it out, the
//! repository's list, the action that runs, the comments on lines kept for a review, and what
//! the last action said. The pages are read off the main thread.

use std::collections::HashMap;
use std::ops::Range;
use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Command;
use motile_core::render::markdown;
use motile_protocol::wire::{
    FileViewed, Label, LineComment, MergeMethod, PullRequestAction, PullRequestEdit, PullRequestState, Reaction,
    ReactionKind, Request, ReviewVerdict, Side,
};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::panel::state::{Loaded, PanelTarget};
use crate::store::Store;
use crate::theme::Colors;
use crate::transcript::prose::{self, PreparedProse};

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Open,
    Draft,
    Merged,
    Closed,
}

impl State {
    pub fn title(self) -> &'static str {
        match self {
            State::Open => "Open",
            State::Draft => "Draft",
            State::Merged => "Merged",
            State::Closed => "Closed",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            State::Open => "git-pull-request",
            State::Draft => "git-pull-request-draft",
            State::Merged => "git-merge",
            State::Closed => "git-pull-request-closed",
        }
    }

    pub fn color(self, c: &Colors) -> Hsla {
        match self {
            State::Open => c.success,
            State::Draft => c.secondary,
            State::Merged => c.merged,
            State::Closed => c.danger,
        }
    }
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Success,
    Danger,
    Warning,
    Pending,
    Neutral,
    Merged,
}

impl Tone {
    pub fn color(self, c: &Colors) -> Hsla {
        match self {
            Tone::Success => c.success,
            Tone::Danger => c.danger,
            Tone::Warning => c.warning,
            Tone::Pending => c.working,
            Tone::Neutral => c.secondary,
            Tone::Merged => c.merged,
        }
    }
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StatusKind {
    Review,
    Checks,
    Behind,
    Conflicts,
    AutoMerge,
    Draft,
    Merged,
    Closed,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    Primary,
    Danger,
    #[default]
    Plain,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Opened,
    Commits,
    Comment,
    Review,
    Merged,
    Closed,
    Thread,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Status {
    pub kind: StatusKind,
    pub tone: Tone,
    pub title: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub at: Option<f64>,
    #[serde(default)]
    pub buttons: Vec<Button>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Check {
    pub name: String,
    #[serde(default)]
    pub workflow: Option<String>,
    pub label: String,
    pub tone: Tone,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    /// The prompt that has the agent fix it.
    #[serde(default)]
    pub fix: Option<String>,
}

/// Something the tab offers: an action on the pull request, or a prompt for the thread's agent.
#[derive(Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Button {
    pub label: String,
    #[serde(default)]
    pub pending_label: Option<String>,
    #[serde(default)]
    pub action: Option<PullRequestAction>,
    #[serde(default)]
    pub method: Option<MergeMethod>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub confirm: Option<Confirm>,
    /// It was chosen from the menu, so what it does is said in the bar rather than on it.
    #[serde(skip)]
    pub from_menu: bool,
}

impl Button {
    /// Names it while its action runs.
    pub fn key(&self) -> String {
        if self.from_menu { format!("menu:{}", self.label) } else { self.label.clone() }
    }

    /// The same button as the menu offers it.
    pub fn in_menu(&self) -> Button {
        Button { from_menu: true, ..self.clone() }
    }
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Confirm {
    pub title: String,
    pub message: String,
    pub button: String,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Choice {
    pub label: String,
    pub action: PullRequestAction,
    #[serde(default)]
    pub method: Option<MergeMethod>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Entry {
    pub kind: EntryKind,
    pub author: String,
    pub said: String,
    pub tone: Tone,
    pub at: f64,
    #[serde(default, deserialize_with = "read_text")]
    pub body: Vec<Text>,
    #[serde(default)]
    pub commits: Vec<Commit>,
    #[serde(default)]
    pub url: Option<String>,
    /// Names the comment or the review, to react to it.
    #[serde(default, rename = "id")]
    pub subject: Option<String>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub thread: Option<Thread>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Commit {
    pub oid: String,
    pub headline: String,
    pub sha: String,
}

/// A conversation on a line.
#[derive(Deserialize, Clone, Debug)]
pub struct Thread {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub line: Option<u32>,
    pub side: Side,
    pub resolved: bool,
    pub outdated: bool,
    /// The last lines of the diff it was written under, the line itself last.
    #[serde(default)]
    pub hunk: Vec<String>,
    #[serde(default)]
    pub comments: Vec<Comment>,
    #[serde(default)]
    pub fix: Option<String>,
    #[serde(default)]
    pub can_resolve: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub at: f64,
    #[serde(default, deserialize_with = "read_text")]
    pub body: Vec<Text>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Toggle {
    pub name: String,
    pub on: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Reviewer {
    pub name: String,
    pub label: String,
    pub tone: Tone,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Stack {
    pub url: String,
    pub base: String,
    /// Bottom first.
    pub layers: Vec<StackLayer>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct StackLayer {
    pub number: u64,
    pub title: String,
    pub state: State,
    pub current: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Linked {
    pub number: u64,
    pub title: String,
    pub state: State,
}

/// The pull request tab as the core worked it out: `pull_request::View`, its Markdown prepared.
#[derive(Deserialize, Clone, Debug)]
pub struct Page {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: State,
    pub byline: String,
    pub base: String,
    pub head: String,
    pub files: u32,
    pub additions: u32,
    pub deletions: u32,
    #[serde(default)]
    pub statuses: Vec<Status>,
    #[serde(default)]
    pub checks: Vec<Check>,
    #[serde(default)]
    pub primary: Option<Button>,
    #[serde(default)]
    pub method: Option<MergeMethod>,
    #[serde(default)]
    pub methods: Vec<Choice>,
    #[serde(default)]
    pub menu: Vec<Button>,
    #[serde(default)]
    pub activity: Vec<Entry>,
    #[serde(default)]
    pub verdicts: Vec<Choice>,
    #[serde(default)]
    pub with_comment: Option<Choice>,
    /// Something is still being worked out, so it is read again in a while.
    #[serde(default)]
    pub settling: bool,
    /// The description as it was written, for editing it.
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub can_edit: bool,
    #[serde(default)]
    pub labels: Vec<Label>,
    #[serde(default)]
    pub label_choices: Vec<Toggle>,
    #[serde(default)]
    pub reviewers: Vec<Reviewer>,
    #[serde(default)]
    pub reviewer_choices: Vec<Toggle>,
    #[serde(default)]
    pub viewed: Vec<FileViewed>,
    #[serde(default)]
    pub can_review_lines: bool,
    #[serde(default)]
    pub threads: Vec<Thread>,
    #[serde(default)]
    pub stack: Option<Stack>,
    #[serde(default)]
    pub stacked_on: Option<Linked>,
    #[serde(default)]
    pub watchable: bool,
}

/// A pull request in the repository's list.
#[derive(Deserialize, Clone, Debug)]
pub struct Row {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: State,
    pub author: String,
    pub head: String,
    pub base: String,
    pub updated_at: f64,
    #[serde(default)]
    pub checks: Option<Tone>,
    #[serde(default)]
    pub checks_label: Option<String>,
    #[serde(default)]
    pub review: Option<(Tone, String)>,
    pub additions: u32,
    pub deletions: u32,
}

/// A stretch of Markdown from the pull request, ready to draw as the transcript draws a reply.
#[derive(Clone, Debug)]
pub enum Text {
    Prose(PreparedProse),
    Code { code: SharedString, spans: Vec<(Range<usize>, u32)> },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RawText {
    Prose {
        text: String,
        #[serde(default)]
        runs: Vec<[u32; 3]>,
        #[serde(default)]
        links: Vec<RawLink>,
        #[serde(default)]
        paras: Vec<RawPara>,
    },
    Code {
        code: String,
        #[serde(default)]
        spans: Vec<u32>,
    },
}

#[derive(Deserialize)]
struct RawLink {
    start: u32,
    len: u32,
    url: String,
}

#[derive(Deserialize)]
struct RawPara {
    start: u32,
    len: u32,
    #[serde(flatten)]
    kind: RawParaKind,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RawParaKind {
    Body,
    Heading {
        level: u8,
    },
    ListItem {
        depth: u8,
        marker: bool,
        quote: u8,
    },
    Quote {
        depth: u8,
    },
    Pre {
        depth: u8,
        #[serde(default)]
        language: String,
        #[serde(default)]
        spans: Option<Vec<u32>>,
    },
    Rule,
    Cell {
        table: u16,
        row: u16,
        column: u16,
        columns: u16,
        header: bool,
        align: u8,
    },
}

impl RawParaKind {
    fn into_core(self) -> markdown::ParaKind {
        match self {
            RawParaKind::Body => markdown::ParaKind::Body,
            RawParaKind::Heading { level } => markdown::ParaKind::Heading { level },
            RawParaKind::ListItem { depth, marker, quote } => markdown::ParaKind::ListItem { depth, marker, quote },
            RawParaKind::Quote { depth } => markdown::ParaKind::Quote { depth },
            RawParaKind::Pre { depth, language, spans } => {
                markdown::ParaKind::Pre { depth, language, code: String::new(), spans: spans.map(std::sync::Arc::new) }
            }
            RawParaKind::Rule => markdown::ParaKind::Rule,
            RawParaKind::Cell { table, row, column, columns, header, align } => {
                markdown::ParaKind::Cell { table, row, column, columns, header, align }
            }
        }
    }
}

impl RawText {
    fn prepare(self) -> Text {
        match self {
            RawText::Prose { text, runs, links, paras } => {
                let prose = markdown::Prose {
                    text,
                    runs,
                    links: links
                        .into_iter()
                        .map(|link| markdown::Link { start: link.start, len: link.len, url: link.url })
                        .collect(),
                    paras: paras
                        .into_iter()
                        .map(|para| markdown::Para { start: para.start, len: para.len, kind: para.kind.into_core() })
                        .collect(),
                };
                Text::Prose(prose::prepare(&prose))
            }
            RawText::Code { code, spans } => {
                let spans = prose::code_spans(&code, &spans);
                Text::Code { code: code.into(), spans }
            }
        }
    }
}

/// Markdown blocks as the core sends them, prepared for drawing.
pub fn read_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Text>, D::Error> {
    let raw: Vec<RawText> = Vec::deserialize(deserializer)?;
    Ok(raw.into_iter().map(RawText::prepare).collect())
}

/// The same from an answer's `blocks`.
pub fn read_blocks(value: &Value) -> Vec<Text> {
    serde_json::from_value::<Vec<RawText>>(value.clone())
        .map(|raw| raw.into_iter().map(RawText::prepare).collect())
        .unwrap_or_default()
}

/// GitHub's reactions, in the order it offers them.
pub const REACTIONS: [(ReactionKind, &str); 8] = [
    (ReactionKind::ThumbsUp, "👍"),
    (ReactionKind::ThumbsDown, "👎"),
    (ReactionKind::Laugh, "😄"),
    (ReactionKind::Hooray, "🎉"),
    (ReactionKind::Confused, "😕"),
    (ReactionKind::Heart, "❤️"),
    (ReactionKind::Rocket, "🚀"),
    (ReactionKind::Eyes, "👀"),
];

pub fn emoji(kind: ReactionKind) -> &'static str {
    REACTIONS.iter().find(|(reaction, _)| *reaction == kind).map_or("", |(_, emoji)| emoji)
}

/// What an action on a pull request did, or why it couldn't.
#[derive(Clone, PartialEq, Debug)]
pub struct Notice {
    id: u64,
    pub text: String,
    pub failed: bool,
    /// The pull request the action opened.
    pub url: Option<String>,
}

/// A comment on a line, kept for the next review.
#[derive(Clone, PartialEq, Debug)]
pub struct PendingLineComment {
    pub id: u64,
    pub path: String,
    pub line: u32,
    pub side: Side,
    pub body: String,
}

/// A line of a pull request's diff that a comment is being written on.
#[derive(Clone, PartialEq, Debug)]
pub struct CommentedLine {
    pub path: String,
    /// As GitHub counts it, in the file as it was or as it is.
    pub line: u32,
    pub side: Side,
    pub code: String,
}

pub struct PullRequests {
    pub page: Loaded<Page>,
    /// The key of the button whose action runs, which is pending meanwhile.
    pub working: Option<String>,
    pub list: Loaded<Vec<Row>>,
    /// The comments on lines kept for the next review, by pull request.
    pub pending: HashMap<u64, Vec<PendingLineComment>>,
    pub notice: Option<Notice>,
    /// Counts up with every answer that set the page.
    pub reads: u64,
    /// The key of the action that last worked, with a count that goes up each time, so the
    /// control that started it can finish with it.
    pub finished: Option<(String, u64)>,
    /// The line the comment sheet is open on.
    pub commenting: Option<CommentedLine>,
    shown: Option<PanelTarget>,
    shown_number: Option<u64>,
    /// How each project's pull requests merge, by project.
    methods: Option<HashMap<String, MergeMethod>>,
    next_id: u64,
    notice_task: Option<Task<()>>,
}

impl Default for PullRequests {
    fn default() -> Self {
        Self {
            page: Loaded::Loading,
            working: None,
            list: Loaded::Loading,
            pending: HashMap::new(),
            notice: None,
            reads: 0,
            finished: None,
            commenting: None,
            shown: None,
            shown_number: None,
            methods: None,
            next_id: 0,
            notice_task: None,
        }
    }
}

impl PullRequests {
    /// The page, when it is the pull request's.
    pub fn page_of(&self, number: u64) -> Option<&Page> {
        self.page.value().filter(|page| page.number == number)
    }

    pub fn pending_for(&self, number: u64) -> Vec<PendingLineComment> {
        self.pending.get(&number).cloned().unwrap_or_default()
    }

    fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
}

fn target_request(target: &PanelTarget) -> (String, String, Option<String>) {
    (target.server_id.clone(), target.project_id.clone(), target.thread_id.clone())
}

fn read_page(value: Value) -> Result<Page, String> {
    serde_json::from_value(value).map_err(|error| format!("The pull request couldn't be read: {error}"))
}

fn action_key(action: PullRequestAction) -> String {
    serde_json::to_value(action).ok().and_then(|value| value.as_str().map(str::to_string)).unwrap_or_default()
}

impl Store {
    /// Forgets what was shown when the folder is another one than before.
    fn look_at_pull_requests(&mut self, target: &PanelTarget) {
        let state = &mut self.pull_requests;
        if state.shown.as_ref() == Some(target) {
            return;
        }
        let same_folder = state.shown.as_ref().is_some_and(|shown| shown.key == target.key);
        state.shown = Some(target.clone());
        if same_folder {
            return;
        }
        state.shown_number = None;
        state.page = Loaded::Loading;
        state.notice = None;
        state.list = Loaded::Loading;
        state.reads += 1;
    }

    fn merge_method(&mut self, project_id: &str) -> Option<MergeMethod> {
        if self.pull_requests.methods.is_none() {
            self.pull_requests.methods = Some(self.prefs.get("pullRequest.methods").unwrap_or_default());
        }
        self.pull_requests.methods.as_ref()?.get(project_id).copied()
    }

    fn remember_merge_method(&mut self, method: MergeMethod, project_id: &str) {
        self.merge_method(project_id);
        let Some(methods) = self.pull_requests.methods.as_mut() else { return };
        methods.insert(project_id.to_string(), method);
        let saved = methods.clone();
        self.prefs.set("pullRequest.methods", saved);
    }

    /// Asks the server for the pull request. What is shown of it stays until the answer is
    /// there, unless it is another one.
    pub fn load_pull_request(&mut self, target: &PanelTarget, number: u64) {
        self.look_at_pull_requests(target);
        if self.pull_requests.shown_number != Some(number) {
            self.pull_requests.shown_number = Some(number);
            self.pull_requests.page = Loaded::Loading;
            self.pull_requests.notice = None;
            self.pull_requests.reads += 1;
        }
        let (server_id, project_id, thread_id) = target_request(target);
        let method = self.merge_method(&project_id);
        let command = Command::PullRequest { server_id, project_id, thread_id, number, method };
        let target = target.clone();
        self.ask_read(command, read_page, move |store, result: Result<Result<Page, String>, String>, _| {
            let state = &mut store.pull_requests;
            if state.shown.as_ref() != Some(&target) || state.shown_number != Some(number) {
                return;
            }
            match result.and_then(|read| read) {
                Ok(page) => state.page = Loaded::Ready(page),
                // A pull request that was read stays; only its notice says what went wrong.
                Err(error) if state.page.value().is_some() => {
                    state.notice = Some(Notice { id: state.next_id(), text: error, failed: true, url: None })
                }
                Err(error) => state.page = Loaded::Failed(error),
            }
            state.reads += 1;
        });
    }

    /// Does something to the pull request: an action of a button or a choice, with what the
    /// comment box says. `key` names the button it came from, which is pending meanwhile.
    pub fn pull_request_act(
        &mut self,
        action: PullRequestAction,
        method: Option<MergeMethod>,
        text: Option<String>,
        key: String,
        target: &PanelTarget,
        number: u64,
    ) {
        if self.pull_requests.working.is_some() {
            return;
        }
        self.pull_requests.working = Some(key.clone());
        self.pull_requests.notice = None;
        if let (PullRequestAction::Merge | PullRequestAction::EnableAutoMerge, Some(method)) = (action, method) {
            self.remember_merge_method(method, &target.project_id);
        }
        let (server_id, project_id, thread_id) = target_request(target);
        let command = Command::PullRequestAction {
            server_id,
            project_id,
            thread_id,
            number,
            action,
            method,
            text: text.filter(|text| !text.is_empty()),
        };
        let target = target.clone();
        let read = |answer: Value| {
            let title = answer["title"].as_str().unwrap_or_default().to_string();
            let url = answer["url"].as_str().map(str::to_string);
            read_page(answer["view"].clone()).map(|page| (title, url, page))
        };
        self.ask_read(command, read, move |store, result, cx| {
            store.pull_requests.working = None;
            let state = &store.pull_requests;
            if state.shown.as_ref() != Some(&target) || state.shown_number != Some(number) {
                return;
            }
            match result.and_then(|read| read) {
                Ok((title, url, page)) => {
                    store.pull_requests.page = Loaded::Ready(page);
                    store.pull_requests.reads += 1;
                    store.finish_pull_request_work(key);
                    if url.is_some() {
                        store.say_pull_request(title, false, url, cx);
                    }
                }
                Err(error) => store.say_pull_request(error, true, None, cx),
            }
        });
    }

    /// Changes the pull request as `edit` says. A `key` names the control that waits for it;
    /// without one it goes like a reaction.
    pub fn pull_request_edit(&mut self, edit: PullRequestEdit, key: Option<String>, target: &PanelTarget, number: u64) {
        if key.is_some() {
            if self.pull_requests.working.is_some() {
                return;
            }
            self.pull_requests.working = key.clone();
            self.pull_requests.notice = None;
        }
        let (server_id, project_id, thread_id) = target_request(target);
        let method = self.merge_method(&project_id);
        let command = Command::PullRequestEdit { server_id, project_id, thread_id, number, edit, method };
        let target = target.clone();
        let read = |answer: Value| read_page(answer["view"].clone());
        self.ask_read(command, read, move |store, result, cx| {
            if key.is_some() {
                store.pull_requests.working = None;
            }
            let state = &store.pull_requests;
            if state.shown.as_ref() != Some(&target) || state.shown_number != Some(number) {
                return;
            }
            match result.and_then(|read| read) {
                Ok(page) => {
                    store.pull_requests.page = Loaded::Ready(page);
                    store.pull_requests.reads += 1;
                    if let Some(key) = key {
                        store.finish_pull_request_work(key);
                    }
                }
                Err(error) => store.say_pull_request(error, true, None, cx),
            }
        });
    }

    fn finish_pull_request_work(&mut self, key: String) {
        let count = self.pull_requests.finished.as_ref().map_or(0, |finished| finished.1) + 1;
        self.pull_requests.finished = Some((key, count));
    }

    /// Keeps a comment on a line for the next review of the pull request.
    pub fn add_pending_comment(&mut self, number: u64, path: String, line: u32, side: Side, body: String) {
        let id = self.pull_requests.next_id();
        let comment = PendingLineComment { id, path, line, side, body };
        self.pull_requests.pending.entry(number).or_default().push(comment);
    }

    pub fn remove_pending_comment(&mut self, number: u64, id: u64) {
        if let Some(pending) = self.pull_requests.pending.get_mut(&number) {
            pending.retain(|comment| comment.id != id);
        }
    }

    /// Sends a review with the comments kept for it, and forgets them once it is there.
    pub fn pull_request_review(
        &mut self,
        verdict: ReviewVerdict,
        body: String,
        key: String,
        target: &PanelTarget,
        number: u64,
    ) {
        let comments = self
            .pull_requests
            .pending_for(number)
            .into_iter()
            .map(|comment| LineComment {
                path: comment.path,
                line: comment.line,
                side: comment.side,
                body: comment.body,
            })
            .collect();
        let edit = PullRequestEdit::Review { verdict, body, comments };
        self.pull_requests.pending.remove(&number);
        self.pull_request_edit(edit, Some(key), target, number);
    }

    /// Asks the server for the repository's pull requests in that state.
    pub fn load_pull_requests(&mut self, target: &PanelTarget, state: PullRequestState) {
        self.look_at_pull_requests(target);
        let (server_id, project_id, thread_id) = target_request(target);
        let command = Command::PullRequests { server_id, project_id, thread_id, state };
        let target = target.clone();
        let read = |answer: Value| {
            serde_json::from_value::<Vec<Row>>(answer["rows"].clone())
                .map_err(|error| format!("The pull requests couldn't be read: {error}"))
        };
        self.ask_read(command, read, move |store, result, cx| {
            if store.pull_requests.shown.as_ref() != Some(&target) {
                return;
            }
            match result.and_then(|read| read) {
                Ok(rows) => store.pull_requests.list = Loaded::Ready(rows),
                Err(error) if store.pull_requests.list.value().is_none() => {
                    store.pull_requests.list = Loaded::Failed(error)
                }
                Err(error) => store.say_pull_request(error, true, None, cx),
            }
        });
    }

    /// Makes the pull request the thread's own, or with `None` takes its own away.
    pub fn link_pull_request(&mut self, number: Option<u64>, thread_id: String, server_id: &str) {
        let request = Request::LinkPullRequest { thread_id, number };
        self.request_then(server_id, request, move |store, result, cx| match result {
            Ok(_) => {
                let said = match number {
                    Some(number) => format!("Linked PR #{number} to this thread"),
                    None => "Unlinked the pull request from this thread".to_string(),
                };
                store.say_pull_request(said, false, None, cx);
            }
            Err(error) => store.say_pull_request(error, true, None, cx),
        });
    }

    /// Has the thread's agent told what happens on its pull request, or stops it.
    pub fn watch_pull_request(&mut self, on: bool, thread_id: String, server_id: &str) {
        let request = Request::WatchPullRequest { thread_id, watch: on };
        self.request_then(server_id, request, move |store, result, cx| match result {
            Ok(_) => {
                let said = if on {
                    "Watching: the agent hears when its checks finish, someone comments or it conflicts"
                } else {
                    "No longer watching"
                };
                store.say_pull_request(said.to_string(), false, None, cx);
            }
            Err(error) => store.say_pull_request(error, true, None, cx),
        });
    }

    /// How the project's pull requests merge from now on.
    pub fn choose_merge_method(&mut self, method: MergeMethod, target: &PanelTarget, number: u64) {
        self.remember_merge_method(method, &target.project_id);
        self.load_pull_request(target, number);
    }

    /// What worked goes away by itself; what failed stays until it is closed.
    fn say_pull_request(&mut self, text: String, failed: bool, url: Option<String>, cx: &mut Context<Self>) {
        let id = self.pull_requests.next_id();
        self.pull_requests.notice = Some(Notice { id, text, failed, url });
        if failed {
            return;
        }
        self.pull_requests.notice_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(8)).await;
            let _ = this.update(cx, |store, cx| {
                if store.pull_requests.notice.as_ref().is_some_and(|notice| notice.id == id) {
                    store.pull_requests.notice = None;
                    cx.notify();
                }
            });
        }));
    }

    pub fn dismiss_pull_request_notice(&mut self) {
        self.pull_requests.notice = None;
    }

    /// Opens the sheet that comments on the line, or with `None` closes it.
    pub fn comment_on_line(&mut self, line: Option<CommentedLine>) {
        self.pull_requests.commenting = line;
    }

    /// Puts a prompt that a pull request's tab wrote in the composer, under what is there, for
    /// the user to read and send.
    pub fn hand_off(&mut self, prompt: String) {
        let written = self.draft().trim().to_string();
        let text = if written.is_empty() { prompt } else { format!("{written}\n\n{prompt}") };
        self.set_draft(text);
        if self.panel_maximized() {
            self.toggle_panel_maximized();
        }
        self.draft_version += 1;
        self.composer_focus += 1;
    }

    /// The key a verdict's button is pending under: "approve", "request_changes".
    pub fn verdict_key(action: PullRequestAction) -> String {
        action_key(action)
    }
}
