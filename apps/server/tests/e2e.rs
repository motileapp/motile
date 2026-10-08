//! The server and a client's connection talking over real iroh connections on this machine, with
//! `scripts/fake-agent` standing in for Claude Code and Codex.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use motile_core::connection::{Connection, Follow, ServerAddr, bind};
use motile_core::link::{Link, LinkEvent, State};
use motile_protocol::identity::DeviceKey;
use motile_protocol::wire::{
    Access as AgentAccess, Agent, AgentAccount, AgentLimits, Approval, CONTINUE_PROMPT, Change, CheckStatus, DiffScope,
    EventKind, FileKind, GitAction, GitHubState, GitStage, GitStatus, Interruption, Item, ItemKind, LineComment,
    MergeMethod, Mergeable, Message, NewThread, NewWorktree, Project, PullRequestAction, PullRequestDetail,
    PullRequestEdit, PullRequestState, Queued, ReactionKind, Request, RestartWhen, ReviewVerdict, ServerUpdate, Side,
    Thread, ThreadChange, Tokens, ToolCall, ToolStatus, TurnChanges, TurnSummary, UsageBucket, Variable,
};
use motile_server::access::Access;
use motile_server::agents::environment::Environment;
use motile_server::hub::Hub;
use motile_server::serve::{BindOptions, Server};
use motile_server::store::Store;
use serde_json::json;
use tokio::sync::mpsc;

const TIMEOUT: Duration = Duration::from_secs(30);

fn repo_file(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path).canonicalize().unwrap()
}

struct Harness {
    dir: tempfile::TempDir,
    server_key: DeviceKey,
    app_key: DeviceKey,
    address: ServerAddr,
    endpoint: Option<iroh::Endpoint>,
    variables: HashMap<String, String>,
}

impl Harness {
    /// A server whose agents replay `fixture`, pausing `delay` seconds between lines.
    async fn start(fixture: &str, delay: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let arguments_file = dir.path().join("arguments.txt");
        let variables = HashMap::from([
            // Programs a test stands in for, like GitHub's `gh`, are found in `bin` first.
            ("PATH".to_string(), format!("{}/bin:{}", dir.path().display(), std::env::var("PATH").unwrap_or_default())),
            ("HOME".to_string(), dir.path().to_string_lossy().into_owned()),
            ("FAKE_AGENT_FIXTURE".to_string(), repo_file(fixture).to_string_lossy().into_owned()),
            ("FAKE_AGENT_DELAY".to_string(), delay.to_string()),
            ("FAKE_AGENT_RESET".to_string(), "2".to_string()),
            ("FAKE_AGENT_ARGUMENTS_FILE".to_string(), arguments_file.to_string_lossy().into_owned()),
        ]);
        let server_key = DeviceKey::generate();
        let app_key = DeviceKey::generate();
        let (endpoint, address) = serve(&dir, &server_key, &app_key, &variables, None).await;
        Self { dir, server_key, app_key, address, endpoint: Some(endpoint), variables }
    }

    /// Stops the server and starts it again on the same data and the same address.
    async fn restart(&mut self) {
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close().await;
        }
        let port = self.address.direct.map(|address| address.port());
        let (endpoint, address) = serve(&self.dir, &self.server_key, &self.app_key, &self.variables, port).await;
        self.endpoint = Some(endpoint);
        self.address = address;
    }

    async fn connect(&self) -> Connection {
        self.connect_as(&self.app_key).await
    }

    async fn connect_as(&self, key: &DeviceKey) -> Connection {
        let endpoint = bind(key, true).await.unwrap();
        Connection::dial(&endpoint, &self.address).await.unwrap()
    }

    fn folder(&self, name: &str) -> String {
        let folder = self.dir.path().join(name);
        std::fs::create_dir_all(&folder).unwrap();
        folder.to_string_lossy().into_owned()
    }

    /// A project to start threads in, added the way the client adds one.
    async fn project(&self, connection: &Connection) -> Project {
        let path = self.folder("project");
        assert_eq!(connection.request(&Request::AddProject { path: path.clone() }).await.unwrap(), Message::Ok);
        let mut list = connection.follow(&Request::Subscribe).await.unwrap();
        let Message::Welcome { projects, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
        projects.into_iter().find(|project| project.path == path).expect("the project was added")
    }

    async fn new_thread(&self, connection: &Connection, agent: Agent) -> Option<NewThread> {
        let project = self.project(connection).await;
        Some(NewThread {
            project_id: project.id,
            agent,
            agent_account: None,
            model: None,
            effort: None,
            access: AgentAccess::Supervised,
            plan: false,
            worktree: None,
        })
    }

    /// The arguments the agent got for each turn, one per line, leaving out the calls that only
    /// asked for a title.
    fn recorded_turns(&self) -> Vec<String> {
        self.recorded_turns_in("arguments.txt")
    }

    fn recorded_turns_in(&self, file: &str) -> Vec<String> {
        let recorded = std::fs::read_to_string(self.dir.path().join(file)).unwrap_or_default();
        let calls = recorded.lines().filter_map(|line| serde_json::from_str::<Vec<String>>(line).ok());
        let is_title = |arguments: &Vec<String>| {
            arguments.iter().any(|argument| argument == "--json-schema" || argument == "--output-last-message")
        };
        calls.filter(|arguments| !is_title(arguments)).map(|arguments| arguments.join("\n")).collect()
    }

    /// The settings a running agent was told to change.
    fn recorded_changes(&self) -> Vec<serde_json::Value> {
        let recorded = std::fs::read_to_string(self.dir.path().join("arguments.txt")).unwrap_or_default();
        let lines = recorded.lines().filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok());
        lines.filter(|line| line.is_object()).collect()
    }
}

async fn serve(
    dir: &tempfile::TempDir,
    server_key: &DeviceKey,
    app_key: &DeviceKey,
    variables: &HashMap<String, String>,
    port: Option<u16>,
) -> (iroh::Endpoint, ServerAddr) {
    let fake_agent = repo_file("scripts/fake-agent");
    let executables = HashMap::from([(Agent::Claude, fake_agent.clone()), (Agent::Codex, fake_agent)]);
    let environment = Environment::fixed(variables.clone(), executables);
    let store = Store::open(&dir.path().join("motile.sqlite")).unwrap();
    let hub = Hub::new(
        store,
        dir.path().join("media"),
        dir.path().join("attachments"),
        dir.path().join("worktrees"),
        dir.path().join("no-project"),
        environment,
    )
    .unwrap();
    hub.keep_pull_requests_current(Duration::from_millis(200));
    hub.watch_pull_requests(Duration::from_millis(300));
    hub.keep_limits_continued(Duration::from_millis(200));
    hub.continue_interrupted();

    // After a restart the old endpoint may take a moment to let go of the port.
    let options = BindOptions { local_only: true, port };
    let mut endpoint = motile_server::serve::bind(server_key, &options).await;
    for _ in 0..50 {
        if endpoint.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        endpoint = motile_server::serve::bind(server_key, &options).await;
    }
    let endpoint = endpoint.unwrap();
    let port = endpoint.bound_sockets().iter().find(|address| address.is_ipv4()).unwrap().port();
    let address = format!("{}@127.0.0.1:{port}", server_key.public()).parse().unwrap();

    let access = Access::new(vec![app_key.public()], None);
    let server = Server { hub, access, attachments: dir.path().join("attachments") };
    tokio::spawn(server.run(endpoint.clone()));
    (endpoint, address)
}

/// What the agent replays for a prompt it has no script for.
fn fixture(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "fixtures/read-and-bash.jsonl",
        Agent::Codex => "fixtures/codex-edit-and-run.jsonl",
    }
}

async fn next(follow: &mut Follow) -> Message {
    let message = tokio::time::timeout(TIMEOUT, follow.next()).await.expect("the server went quiet");
    message.unwrap().expect("the stream ended")
}

/// The next change to a thread on the list, skipping the project updates in between.
async fn next_thread(list: &mut Follow) -> Thread {
    loop {
        match next(list).await {
            Message::ThreadUpsert { thread } => return thread,
            Message::Projects { .. } => {}
            other => panic!("expected a thread to be announced, got {other:?}"),
        }
    }
}

async fn thread_where(list: &mut Follow, mut wanted: impl FnMut(&Thread) -> bool) -> Thread {
    loop {
        let thread = next_thread(list).await;
        if wanted(&thread) {
            return thread;
        }
    }
}

async fn send(connection: &Connection, thread_id: Option<String>, new_thread: Option<NewThread>, text: &str) -> String {
    let request = Request::Send { thread_id, new_thread, text: text.to_string(), attachments: Vec::new(), now: false };
    match connection.request(&request).await.unwrap() {
        Message::Sent { thread_id } => thread_id,
        other => panic!("unexpected answer to send: {other:?}"),
    }
}

async fn update(connection: &Connection, thread_id: &str, change: ThreadChange) -> Message {
    connection.request(&Request::Update { thread_id: thread_id.to_string(), change }).await.unwrap()
}

/// Applies a thread's updates the way the client does.
#[derive(Default)]
struct Transcript {
    items: Vec<Item>,
    running: bool,
    approvals: Vec<Approval>,
    queued: Vec<Queued>,
    synced: Option<u64>,
    /// The latest revision seen once live.
    rev: u64,
    /// The streamed pieces of text, as they arrived.
    pieces: Vec<String>,
    resets: usize,
}

impl Transcript {
    fn apply(&mut self, message: Message) {
        match message {
            Message::Opened { reset, activity } => {
                if reset {
                    self.items.clear();
                    self.resets += 1;
                }
                self.running = activity.running;
                self.approvals = activity.approvals;
                self.queued = activity.queued;
            }
            Message::Items { items } => {
                for item in items {
                    if self.synced.is_some() {
                        self.rev = self.rev.max(item.rev);
                    }
                    match self.items.iter().position(|existing| existing.id == item.id) {
                        Some(position) => self.items[position] = item,
                        None => {
                            if let ItemKind::Assistant { text } = &item.kind {
                                self.pieces.push(text.clone());
                            }
                            self.items.push(item)
                        }
                    }
                }
                self.items.sort_by_key(|item| item.seq);
            }
            Message::Synced { rev } => {
                self.synced = Some(rev);
                self.rev = rev;
            }
            Message::TextDelta { id, text, rev } => {
                self.pieces.push(text.clone());
                self.rev = rev;
                let item = self.items.iter_mut().find(|item| item.id == id).expect("a delta for an unknown item");
                let ItemKind::Assistant { text: current } = &mut item.kind else {
                    panic!("a delta for a non-text item")
                };
                current.push_str(&text);
                item.rev = rev;
            }
            Message::Activity { activity } => {
                self.running = activity.running;
                self.approvals = activity.approvals;
                self.queued = activity.queued;
            }
            other => panic!("unexpected thread update: {other:?}"),
        }
    }

    async fn follow_until_idle(&mut self, follow: &mut Follow) {
        while self.synced.is_none() || self.running {
            self.apply(next(follow).await);
        }
    }

    fn texts(&self) -> Vec<&str> {
        let texts = self.items.iter().filter_map(|item| match &item.kind {
            ItemKind::Assistant { text } => Some(text.as_str()),
            _ => None,
        });
        texts.collect()
    }

    fn user_texts(&self) -> Vec<&str> {
        let texts = self.items.iter().filter_map(|item| match &item.kind {
            ItemKind::User { text, .. } => Some(text.as_str()),
            _ => None,
        });
        texts.collect()
    }

    fn tools(&self) -> Vec<(&str, ToolStatus)> {
        let tools = self.items.iter().filter_map(|item| match &item.kind {
            ItemKind::Tool { call } => Some((call.name.as_str(), call.status)),
            _ => None,
        });
        tools.collect()
    }

    fn errors(&self) -> Vec<&str> {
        let errors = self.items.iter().filter_map(|item| match &item.kind {
            ItemKind::Error { message } => Some(message.as_str()),
            _ => None,
        });
        errors.collect()
    }

    fn turn_ends(&self) -> Vec<&TurnSummary> {
        let summaries = self.items.iter().filter_map(|item| match &item.kind {
            ItemKind::TurnEnd { summary } => Some(summary),
            _ => None,
        });
        summaries.collect()
    }
}

async fn open(connection: &Connection, thread_id: &str, since: u64) -> Follow {
    connection.follow(&Request::Open { thread_id: thread_id.to_string(), since }).await.unwrap()
}

async fn finished_transcript(connection: &Connection, thread_id: &str) -> Transcript {
    let mut transcript = Transcript::default();
    transcript.follow_until_idle(&mut open(connection, thread_id, 0).await).await;
    transcript
}

#[tokio::test]
async fn a_claude_turn_streams_into_the_transcript_and_survives_a_restart() {
    let mut harness = Harness::start("fixtures/edit-and-run.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let thread_id = send(&connection, None, new_thread, "Use an f-string in greet.py").await;

    let live = finished_transcript(&connection, &thread_id).await;

    assert_eq!(live.user_texts(), vec!["Use an f-string in greet.py"]);
    assert!(live.texts().last().unwrap().contains("| Change | Why |"));
    assert!(live.tools().contains(&("Edit", ToolStatus::Succeeded)));
    assert_eq!(live.turn_ends().len(), 1);
    let seqs: Vec<u64> = live.items.iter().map(|item| item.seq).collect();
    assert_eq!(seqs, (0..live.items.len() as u64).collect::<Vec<_>>());

    harness.restart().await;
    let reopened = finished_transcript(&harness.connect().await, &thread_id).await;
    assert_eq!(reopened.items, live.items);
    assert_eq!(reopened.synced, live.synced.map(|_| live.rev));
}

#[tokio::test]
async fn a_codex_turn_shows_its_commands_and_edits() {
    let harness = Harness::start("fixtures/codex-edit-and-run.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Codex).await.unwrap();
    let new_thread = NewThread { effort: Some("high".into()), access: AgentAccess::AcceptEdits, ..new_thread };
    let thread_id = send(&connection, None, Some(new_thread), "Use an f-string in greet.py").await;

    let transcript = finished_transcript(&connection, &thread_id).await;

    assert_eq!(transcript.user_texts(), vec!["Use an f-string in greet.py"]);
    assert_eq!(
        transcript.texts().last().unwrap(),
        &"Updated [greet.py](/tmp/cctest/greet.py:2) to use `f\"Hello {name}\"`. Verified it prints `Hello world`."
    );
    assert_eq!(
        transcript.tools(),
        vec![
            ("Bash", ToolStatus::Succeeded),
            ("Bash", ToolStatus::Succeeded),
            ("Edit", ToolStatus::Succeeded),
            ("Bash", ToolStatus::Failed),
            ("Bash", ToolStatus::Succeeded)
        ]
    );
    let ran = transcript.items.iter().find_map(|item| match &item.kind {
        ItemKind::Tool { call } if call.input.contains("python3 greet.py") => call.output.clone(),
        _ => None,
    });
    assert_eq!(ran.as_deref(), Some("Hello world\n"));
    assert!(matches!(transcript.items.last().unwrap().kind, ItemKind::TurnEnd { .. }));
    assert!(transcript.errors().is_empty());

    // The next message resumes Codex's own thread in a new process, with the thread's settings.
    let low = ThreadChange { effort: Some("low".into()), access: Some(AgentAccess::Full), ..Default::default() };
    assert_eq!(update(&connection, &thread_id, low).await, Message::Ok);
    send(&connection, Some(thread_id.clone()), None, "Thanks").await;
    let resumed = finished_transcript(&connection, &thread_id).await;
    assert_eq!(resumed.turn_ends().len(), 2);
    assert_eq!(resumed.tools().len(), 10, "the second turn's items don't replace the first's");

    assert_eq!(harness.recorded_turns(), vec!["app-server", "app-server"]);
    let asked = harness.recorded_changes();
    let started = &asked[0]["thread/start"];
    assert_eq!((&started["approvalPolicy"], &started["sandbox"]), (&json!("on-request"), &json!("workspace-write")));
    assert!(started["developerInstructions"].as_str().unwrap().starts_with(
        "In case you're asked: you are running in Motile through the Codex harness with high reasoning effort. \
         No need to mention this otherwise. You can show the user an image or a video"
    ));
    let first_turn = &asked[1]["turn/start"];
    assert_eq!(
        (&first_turn["effort"], &first_turn["input"][0]["text"]),
        (&json!("high"), &json!("Use an f-string in greet.py"))
    );
    let resumed = &asked[2]["thread/resume"];
    assert_eq!(resumed["threadId"], "01a0f557-4a9b-74a0-b330-f6c4d13b2880");
    assert_eq!((&resumed["approvalPolicy"], &resumed["sandbox"]), (&json!("never"), &json!("danger-full-access")));
    assert_eq!(asked[3]["turn/start"]["effort"], "low");
}

#[tokio::test]
async fn a_streamed_reply_arrives_in_finished_blocks() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let thread_id = send(&connection, None, new_thread, "Add a rate limiter to the API").await;

    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &thread_id, 0).await;
    transcript.follow_until_idle(&mut follow).await;

    // The fake agent writes its long reply 24 characters at a time. What reaches the client are
    // whole lines that end a paragraph, a list item or a line of code, a few times a second.
    let reply = *transcript.texts().last().unwrap();
    assert!(reply.starts_with("The API now limits each client") && reply.ends_with("in total.\n"), "{reply}");
    let start = transcript.pieces.iter().position(|piece| piece.starts_with("The API now limits")).unwrap();
    let pieces = &transcript.pieces[start..];
    assert!(pieces.len() > 1 && pieces.len() < 12, "{} pieces", pieces.len());
    assert!(pieces.iter().all(|piece| piece.ends_with('\n')), "{pieces:?}");
    assert!(reply.starts_with(&pieces.concat()));
}

#[tokio::test]
async fn a_client_that_reconnects_mid_turn_is_sent_only_what_it_missed() {
    let harness = Harness::start("fixtures/edit-and-run.jsonl", "0.02").await;
    let first = harness.connect().await;
    let new_thread = harness.new_thread(&first, Agent::Claude).await;
    let thread_id = send(&first, None, new_thread, "Use an f-string in greet.py").await;

    let mut transcript = Transcript::default();
    let mut follow = open(&first, &thread_id, 0).await;
    while transcript.items.len() < 3 {
        transcript.apply(next(&mut follow).await);
    }
    first.close();
    let had = transcript.items.len();

    // The turn keeps running without the client. Back again, it asks for what came after its revision.
    let second = harness.connect().await;
    let mut watching = Transcript::default();
    let mut watch = open(&second, &thread_id, 0).await;
    while watching.synced.is_none() || watching.rev <= transcript.rev {
        watching.apply(next(&mut watch).await);
    }
    let mut follow = open(&second, &thread_id, transcript.rev).await;
    transcript.synced = None;
    let mut resent = 0;
    while transcript.synced.is_none() {
        let message = next(&mut follow).await;
        if let Message::Items { items } = &message {
            resent += items.len();
        }
        transcript.apply(message);
    }
    assert!(transcript.running, "the turn is still running");
    assert!(resent > 0 && resent <= transcript.items.len() - had + 1, "resent {resent} of {}", transcript.items.len());
    assert_eq!(transcript.resets, 0);

    transcript.follow_until_idle(&mut follow).await;
    let whole = finished_transcript(&second, &thread_id).await;
    assert_eq!(transcript.items, whole.items, "catching up gives the same transcript as reading it whole");

    // Up to date, there is nothing to send.
    let mut follow = open(&second, &thread_id, whole.rev).await;
    assert!(matches!(next(&mut follow).await, Message::Opened { reset: false, .. }));
    assert_eq!(next(&mut follow).await, Message::Synced { rev: whole.rev });
}

#[tokio::test]
async fn a_client_ahead_of_the_server_starts_over() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let thread_id = send(&connection, None, new_thread, "What does the note say?").await;
    let whole = finished_transcript(&connection, &thread_id).await;

    let mut stale = Transcript::default();
    stale.follow_until_idle(&mut open(&connection, &thread_id, whole.rev + 1000).await).await;

    assert_eq!(stale.resets, 1);
    assert_eq!(stale.items, whole.items);
}

/// The states the link goes through until it is connected.
async fn states_until_connected(events: &mut mpsc::UnboundedReceiver<(String, LinkEvent)>) -> Vec<State> {
    let mut states = Vec::new();
    while states.last() != Some(&State::Connected) {
        let (_, event) = tokio::time::timeout(TIMEOUT, events.recv()).await.expect("the link didn't connect").unwrap();
        let LinkEvent::Status(status) = event else { continue };
        if states.last() != Some(&status.state) {
            states.push(status.state);
        }
    }
    states
}

#[tokio::test]
async fn a_link_given_a_new_endpoint_leaves_the_old_one_and_connects_again() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let (events, mut received) = mpsc::unbounded_channel();
    let stale = bind(&harness.app_key, true).await.unwrap();
    let link = Link::connect(stale.clone(), harness.address.clone(), events);
    assert_eq!(states_until_connected(&mut received).await, [State::Connected]);

    link.redial_on(bind(&harness.app_key, true).await.unwrap());
    assert_eq!(states_until_connected(&mut received).await, [State::Connecting, State::Connected]);

    stale.close().await;
    let add = Request::AddProject { path: harness.folder("project") };
    assert_eq!(link.request(&add).await.unwrap(), Message::Ok);
    link.shutdown();
}

#[tokio::test]
async fn the_thread_list_follows_new_retitled_and_deleted_threads() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { server, threads, .. } = next(&mut list).await else {
        panic!("the list starts with a welcome")
    };
    assert!(threads.is_empty());
    assert_eq!(server.agents.len(), 2);

    let text = "Please look at README.md and tell me what this project is actually for.\nBe brief.";
    let thread_id = send(&connection, None, new_thread, text).await;
    let first = next_thread(&mut list).await;
    assert_eq!(first.id, thread_id);
    assert_eq!(first.title, "Please look at README.md and tell me what this pro...");
    assert!(first.running);

    // The agent's title replaces the placeholder, and the turn ends.
    let titled = thread_where(&mut list, |thread| thread.title == "Use an F-String in Greet").await;
    assert_eq!(titled.id, thread_id);
    let ended = if titled.running { thread_where(&mut list, |thread| !thread.running).await } else { titled };
    assert!(ended.turn_ended_at.is_some());
    assert!(ended.rev > 0);

    let renamed = ThreadChange { title: Some(" Readme ".to_string()), ..Default::default() };
    assert_eq!(update(&connection, &thread_id, renamed).await, Message::Ok);
    assert_eq!(next_thread(&mut list).await.title, "Readme");

    // A thread starts out listed by when it was created, until the user moves it.
    assert_eq!(ended.position, ended.created_at);
    let moved = ThreadChange { position: Some(1234.5), ..Default::default() };
    assert_eq!(update(&connection, &thread_id, moved).await, Message::Ok);
    assert_eq!(next_thread(&mut list).await.position, 1234.5);

    connection.request(&Request::Delete { thread_id: thread_id.clone() }).await.unwrap();
    let mut deleted = next(&mut list).await;
    while matches!(deleted, Message::Projects { .. }) {
        deleted = next(&mut list).await;
    }
    assert_eq!(deleted, Message::ThreadDeleted { thread_id: thread_id.clone() });
    let Message::Error { .. } = next(&mut open(&connection, &thread_id, 0).await).await else {
        panic!("a deleted thread can't be opened");
    };
}

#[tokio::test]
async fn a_vague_first_message_is_titled_from_the_transcript_once_the_turn_ends() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0.01").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;

    send(&connection, None, new_thread, "fix this").await;
    let mut seen = Vec::new();
    let refined = thread_where(&mut list, |thread| {
        seen.push((thread.title.clone(), thread.running));
        thread.title == "Refined From Transcript"
    })
    .await;

    assert!(!refined.running);
    assert!(seen.iter().all(|(title, _)| title == "fix this" || title == "Refined From Transcript"), "{seen:?}");
    assert!(
        seen.contains(&("fix this".to_string(), false)),
        "the placeholder stays until the turn has ended: {seen:?}"
    );
}

#[tokio::test]
async fn a_tool_call_of_claude_that_needs_approval_waits_for_the_answer() {
    a_tool_call_that_needs_approval_waits_for_the_answer(Agent::Claude).await;
}

#[tokio::test]
async fn a_tool_call_of_codex_that_needs_approval_waits_for_the_answer() {
    a_tool_call_that_needs_approval_waits_for_the_answer(Agent::Codex).await;
}

async fn a_tool_call_that_needs_approval_waits_for_the_answer(agent: Agent) {
    let harness = Harness::start(fixture(agent), "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, agent).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;
    let thread_id = send(&connection, None, new_thread, "Use an f-string and run greet.py").await;
    let answer = |approval_id: String, allow| {
        let request = Request::Answer { thread_id: thread_id.clone(), approval_id, allow, answers: HashMap::new() };
        let connection = &connection;
        async move { connection.request(&request).await }
    };

    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &thread_id, 0).await;
    let edit = first_approval(&mut transcript, &mut follow).await;
    assert_eq!(edit.tool_name, "Edit");
    assert!(edit.input.contains("greet.py"));
    let waiting = thread_where(&mut list, |thread| thread.needs_approval).await;
    assert!(waiting.running, "the turn stands still, it has not ended");

    assert_eq!(answer(edit.id.clone(), true).await.unwrap(), Message::Ok);
    while transcript.approvals.first().is_none_or(|approval| approval.id == edit.id) {
        transcript.apply(next(&mut follow).await);
    }
    let bash = transcript.approvals[0].clone();
    assert_eq!(bash.tool_name, "Bash");
    let twice = answer(edit.id, true).await.unwrap();
    assert!(matches!(twice, Message::Error { message } if message.contains("no longer waits")));

    assert_eq!(answer(bash.id, false).await.unwrap(), Message::Ok);
    transcript.follow_until_idle(&mut follow).await;
    assert_eq!(transcript.tools(), vec![("Edit", ToolStatus::Succeeded), ("Bash", ToolStatus::Failed)]);
    assert_eq!(transcript.texts().last().unwrap(), &"`greet` uses an f-string now. I didn't run it.");
    assert!(transcript.approvals.is_empty());
    let ended = thread_where(&mut list, |thread| !thread.running).await;
    assert!(!ended.needs_approval);
    assert_eq!(harness.recorded_turns().len(), 1, "the answers reach the process that asked");
}

/// Follows the thread until the turn waits with a tool call.
async fn first_approval(transcript: &mut Transcript, follow: &mut Follow) -> Approval {
    while transcript.approvals.is_empty() {
        transcript.apply(next(follow).await);
    }
    transcript.approvals[0].clone()
}

#[tokio::test]
async fn a_question_claude_asks_is_answered_by_the_user() {
    a_question_the_agent_asks_is_answered_by_the_user(Agent::Claude).await;
}

#[tokio::test]
async fn a_question_codex_asks_is_answered_by_the_user() {
    a_question_the_agent_asks_is_answered_by_the_user(Agent::Codex).await;
}

async fn a_question_the_agent_asks_is_answered_by_the_user(agent: Agent) {
    let harness = Harness::start(fixture(agent), "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, agent).await;
    let thread_id = send(&connection, None, new_thread, "Which color should the button be? Ask me.").await;

    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &thread_id, 0).await;
    let asked = first_approval(&mut transcript, &mut follow).await;
    assert_eq!(asked.tool_name, "AskUserQuestion");
    let answers = HashMap::from([("Which color should the button be?".to_string(), "Blue".to_string())]);
    let answer = Request::Answer { thread_id: thread_id.clone(), approval_id: asked.id, allow: true, answers };
    assert_eq!(connection.request(&answer).await.unwrap(), Message::Ok);

    transcript.follow_until_idle(&mut follow).await;
    assert_eq!(transcript.texts(), vec!["The button will be blue."]);
}

#[tokio::test]
async fn an_approved_plan_is_carried_out_with_the_threads_access() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await.unwrap();
    let new_thread = NewThread { plan: true, access: AgentAccess::AcceptEdits, ..new_thread };
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;
    let thread_id = send(&connection, None, Some(new_thread), "Plan the hello function").await;

    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &thread_id, 0).await;
    let plan = first_approval(&mut transcript, &mut follow).await;
    assert_eq!(plan.tool_name, "ExitPlanMode");
    assert!(thread_where(&mut list, |thread| thread.needs_approval).await.plan);
    let approve =
        Request::Answer { thread_id: thread_id.clone(), approval_id: plan.id, allow: true, answers: HashMap::new() };
    assert_eq!(connection.request(&approve).await.unwrap(), Message::Ok);

    transcript.follow_until_idle(&mut follow).await;
    assert_eq!(transcript.texts(), vec!["`hello()` is in place. I worked in acceptEdits mode."]);
    assert!(!thread_where(&mut list, |thread| !thread.running).await.plan, "the thread has left plan mode");
}

#[tokio::test]
async fn a_plan_codex_presents_is_carried_out_in_a_turn_of_its_own_or_left() {
    let harness = Harness::start(fixture(Agent::Codex), "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Codex).await.unwrap();
    let new_thread = NewThread { plan: true, access: AgentAccess::AcceptEdits, ..new_thread };
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;
    let answer = |thread_id: &str, approval_id, allow| {
        let request = Request::Answer { thread_id: thread_id.to_string(), approval_id, allow, answers: HashMap::new() };
        let connection = &connection;
        async move { connection.request(&request).await.unwrap() }
    };

    let left = send(&connection, None, Some(new_thread.clone()), "Plan the hello function").await;
    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &left, 0).await;
    let plan = first_approval(&mut transcript, &mut follow).await;
    assert_eq!(plan.tool_name, "ExitPlanMode");
    assert_eq!(transcript.turn_ends().len(), 1, "Codex presents its plan when the turn has ended");
    assert_eq!(answer(&left, plan.id, false).await, Message::Ok);
    transcript.follow_until_idle(&mut follow).await;
    assert!(thread_where(&mut list, |thread| thread.id == left && !thread.running).await.plan);
    assert_eq!(transcript.turn_ends().len(), 1);
    assert!(transcript.errors().is_empty());

    let carried_out = send(&connection, None, Some(new_thread), "Plan the hello function").await;
    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &carried_out, 0).await;
    let plan = first_approval(&mut transcript, &mut follow).await;
    assert_eq!(answer(&carried_out, plan.id, true).await, Message::Ok);
    while transcript.turn_ends().len() < 2 {
        transcript.apply(next(&mut follow).await);
    }
    assert_eq!(transcript.texts(), vec!["`hello()` is in place. I worked in default mode."]);
    assert_eq!(transcript.user_texts(), vec!["Plan the hello function"]);
    let ended = thread_where(&mut list, |thread| thread.id == carried_out && !thread.running).await;
    assert!(!ended.plan, "the thread has left plan mode");
    assert_eq!(harness.recorded_turns().len(), 2, "the process that planned carries the plan out");
}

#[tokio::test]
async fn settings_changed_while_the_agent_is_there_reach_its_process() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;
    let thread_id = send(&connection, None, new_thread, "Watch the deploy").await;
    thread_where(&mut list, |thread| thread.monitoring).await;

    let model = ThreadChange { model: Some("sonnet".into()), ..Default::default() };
    assert_eq!(update(&connection, &thread_id, model).await, Message::Ok);
    let effort = ThreadChange { effort: Some("high".into()), ..Default::default() };
    assert_eq!(update(&connection, &thread_id, effort).await, Message::Ok);
    let access = ThreadChange { access: Some(AgentAccess::Full), ..Default::default() };
    assert_eq!(update(&connection, &thread_id, access).await, Message::Ok);
    thread_where(&mut list, |thread| !thread.monitoring && !thread.running).await;

    assert_eq!(
        harness.recorded_changes(),
        vec![
            serde_json::json!({"subtype": "set_model", "model": "sonnet"}),
            serde_json::json!({"subtype": "apply_flag_settings", "settings": {"effortLevel": "high"}}),
            serde_json::json!({"subtype": "set_permission_mode", "mode": "bypassPermissions"}),
        ]
    );
    assert_eq!(harness.recorded_turns().len(), 1);
}

#[tokio::test]
async fn stopping_a_turn_ends_it_and_says_so() {
    let harness = Harness::start("fixtures/edit-and-run.jsonl", "0.2").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let thread_id = send(&connection, None, new_thread, "Use an f-string in greet.py").await;

    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &thread_id, 0).await;
    transcript.apply(next(&mut follow).await);
    assert!(transcript.running);
    connection.request(&Request::Stop { thread_id: thread_id.clone() }).await.unwrap();
    transcript.follow_until_idle(&mut follow).await;

    let summary = transcript.turn_ends().pop().expect("a stopped turn still ends with a summary");
    assert!(summary.stopped && !summary.is_error);
    assert!(transcript.errors().is_empty());
    assert!(transcript.tools().iter().all(|(_, status)| *status != ToolStatus::Running));
}

/// Where the user's messages and the turns' ends are in the transcript, in order.
fn messages_and_turn_ends(transcript: &Transcript) -> Vec<&str> {
    let marks = transcript.items.iter().filter_map(|item| match &item.kind {
        ItemKind::User { text, .. } => Some(text.as_str()),
        ItemKind::TurnEnd { .. } => Some("(turn end)"),
        _ => None,
    });
    marks.collect()
}

#[tokio::test]
async fn a_thread_the_usage_limit_stopped_continues_once_the_limit_resets() {
    let harness = Harness::start(fixture(Agent::Claude), "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;
    let thread_id = send(&connection, None, new_thread, "Hit the limit").await;

    let limited = thread_where(&mut list, |thread| thread.interruption.is_some()).await;
    assert!(
        matches!(limited.interruption, Some(Interruption::Limit { resets_at: Some(_), continues: true })),
        "{:?}",
        limited.interruption
    );
    let continued = thread_where(&mut list, |thread| thread.running && thread.interruption.is_none()).await;
    assert_eq!(continued.id, thread_id);

    let transcript = finished_transcript(&connection, &thread_id).await;
    assert_eq!(transcript.user_texts(), vec!["Hit the limit", CONTINUE_PROMPT]);
    let errors = transcript.errors();
    assert!(matches!(&errors[..], [limit] if limit.starts_with("You've hit your limit")), "{errors:?}");
}

#[tokio::test]
async fn a_thread_the_usage_limit_stopped_waits_when_it_is_not_to_continue() {
    let harness = Harness::start(fixture(Agent::Codex), "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Codex).await.unwrap();
    let new_thread = NewThread { access: AgentAccess::Full, ..new_thread };
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;
    let thread_id = send(&connection, None, Some(new_thread), "Hit the limit").await;

    thread_where(&mut list, |thread| thread.interruption.is_some()).await;
    let change = ThreadChange { continues: Some(false), ..ThreadChange::default() };
    assert_eq!(update(&connection, &thread_id, change).await, Message::Ok);
    thread_where(&mut list, |thread| {
        !thread.running && matches!(thread.interruption, Some(Interruption::Limit { continues: false, .. }))
    })
    .await;

    tokio::time::sleep(Duration::from_millis(2500)).await;
    let transcript = finished_transcript(&connection, &thread_id).await;
    assert_eq!(transcript.user_texts(), vec!["Hit the limit"], "the limit has reset, and the thread still waits");
    assert_eq!(transcript.errors(), vec!["You've hit your usage limit."]);
}

#[tokio::test]
async fn a_turn_a_restart_cuts_off_says_so_and_continues_once_the_server_is_back() {
    let mut harness = Harness::start(fixture(Agent::Claude), "0").await;
    let connection = harness.connect().await;
    let set = Request::SetContinueSettings { after_limits: None, after_restarts: Some(true) };
    assert_eq!(connection.request(&set).await.unwrap(), Message::Ok);
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let thread_id = send(&connection, None, new_thread, "Run greet.py").await;
    let mut transcript = Transcript::default();
    first_approval(&mut transcript, &mut open(&connection, &thread_id, 0).await).await;

    harness.restart().await;
    let connection = harness.connect().await;
    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &thread_id, 0).await;
    while !transcript.user_texts().contains(&CONTINUE_PROMPT) {
        transcript.apply(next(&mut follow).await);
    }
    transcript.follow_until_idle(&mut follow).await;

    assert_eq!(transcript.errors(), vec!["Your server restarted before the agent finished."]);
    assert_eq!(transcript.tools()[0], ("Edit", ToolStatus::Failed), "the call the restart cut off failed");
    assert_eq!(messages_and_turn_ends(&transcript), vec!["Run greet.py", "(turn end)", CONTINUE_PROMPT, "(turn end)"]);
}

#[tokio::test]
async fn every_client_hears_where_the_update_of_the_server_is() {
    // Nothing listens there, so the download fails at once and the test's program stays as it is.
    unsafe { std::env::set_var("MOTILE_DOWNLOAD_URL", "http://127.0.0.1:9") };
    let harness = Harness::start(fixture(Agent::Claude), "0").await;
    let connection = harness.connect().await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { server, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
    assert_eq!(server.update, None);

    let other = harness.connect().await;
    let reply = other.request(&Request::UpdateServer { when: Some(RestartWhen::Idle) }).await.unwrap();
    assert!(matches!(reply, Message::Error { .. }), "the download failed: {reply:?}");

    let Message::Server { server } = next(&mut list).await else { panic!("the list says the update began") };
    assert_eq!(server.update, Some(ServerUpdate::Installing { percent: None }));
    let Message::Server { server } = next(&mut list).await else { panic!("the list says the update ended") };
    assert_eq!(server.update, None);
}

#[tokio::test]
async fn a_message_sent_while_claude_works_starts_the_next_turn_when_this_one_ends() {
    a_message_sent_while_a_turn_runs_starts_the_next_turn_when_it_ends(Agent::Claude).await;
}

#[tokio::test]
async fn a_message_sent_while_codex_works_starts_the_next_turn_when_this_one_ends() {
    a_message_sent_while_a_turn_runs_starts_the_next_turn_when_it_ends(Agent::Codex).await;
}

async fn a_message_sent_while_a_turn_runs_starts_the_next_turn_when_it_ends(agent: Agent) {
    let harness = Harness::start(fixture(agent), "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, agent).await.unwrap();
    let new_thread = NewThread { access: AgentAccess::Full, ..new_thread };
    let thread_id = send(&connection, None, Some(new_thread), "Run greet.py").await;
    send(&connection, Some(thread_id.clone()), None, "Use single quotes").await;

    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &thread_id, 0).await;
    transcript.apply(next(&mut follow).await);
    let waiting: Vec<&str> = transcript.queued.iter().map(|queued| queued.text.as_str()).collect();
    assert_eq!(waiting, vec!["Use single quotes"], "the message waits outside the transcript");
    while transcript.turn_ends().len() < 2 {
        transcript.apply(next(&mut follow).await);
    }

    assert!(transcript.queued.is_empty());
    assert_eq!(
        messages_and_turn_ends(&transcript),
        vec!["Run greet.py", "(turn end)", "Use single quotes", "(turn end)"]
    );
    assert!(
        !transcript.texts().iter().any(|text| text.contains("You also said")),
        "the turn it waited for never took it"
    );
    assert_eq!(harness.recorded_turns().len(), 1, "the process that worked takes the message");
}

#[tokio::test]
async fn a_message_sent_now_while_claude_writes_stops_the_reply_and_is_answered_next() {
    let harness = Harness::start(fixture(Agent::Claude), "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let thread_id = send(&connection, None, new_thread, "Give me a long reply with a lot of code").await;
    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &thread_id, 0).await;
    let queued = queue_while_waiting(&connection, &thread_id, "Just say pineapple", &mut transcript, &mut follow).await;

    let send_now = Request::SendQueued { thread_id: thread_id.clone(), message_id: queued.id };
    assert_eq!(connection.request(&send_now).await.unwrap(), Message::Ok);
    transcript.follow_until_idle(&mut follow).await;

    assert_eq!(
        messages_and_turn_ends(&transcript),
        vec!["Give me a long reply with a lot of code", "Just say pineapple", "(turn end)"],
        "the stopped reply and its answer are one turn"
    );
    assert!(transcript.texts().last().unwrap().ends_with("You also said: Just say pineapple"));
    assert!(transcript.errors().is_empty());
    assert!(transcript.queued.is_empty());
    assert_eq!(harness.recorded_turns().len(), 1);
}

#[tokio::test]
async fn a_message_sent_to_steer_is_taken_by_the_turn_that_runs() {
    let harness = Harness::start(fixture(Agent::Claude), "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let thread_id = send(&connection, None, new_thread, "Give me a long reply with a lot of code").await;
    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &thread_id, 0).await;

    let steer = Request::Send {
        thread_id: Some(thread_id.clone()),
        new_thread: None,
        text: "Just say pineapple".to_string(),
        attachments: Vec::new(),
        now: true,
    };
    assert_eq!(connection.request(&steer).await.unwrap(), Message::Sent { thread_id: thread_id.clone() });
    transcript.follow_until_idle(&mut follow).await;

    assert_eq!(
        messages_and_turn_ends(&transcript),
        vec!["Give me a long reply with a lot of code", "Just say pineapple", "(turn end)"],
        "the message never waited for the turn to end"
    );
    assert!(transcript.texts().last().unwrap().ends_with("You also said: Just say pineapple"));
    assert!(transcript.queued.is_empty());
    assert_eq!(harness.recorded_turns().len(), 1);
}

/// Sends a message to a thread whose turn waits for an approval, and follows the thread until
/// the message is queued.
async fn queue_while_waiting(
    connection: &Connection,
    thread_id: &str,
    text: &str,
    transcript: &mut Transcript,
    follow: &mut Follow,
) -> Queued {
    send(connection, Some(thread_id.to_string()), None, text).await;
    loop {
        if let Some(queued) = transcript.queued.iter().find(|queued| queued.text == text) {
            return queued.clone();
        }
        transcript.apply(next(follow).await);
    }
}

#[tokio::test]
async fn a_queued_message_survives_a_restart_and_waits_to_be_sent() {
    let mut harness = Harness::start(fixture(Agent::Claude), "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let thread_id = send(&connection, None, new_thread, "Run greet.py").await;
    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &thread_id, 0).await;
    first_approval(&mut transcript, &mut follow).await;
    let queued = queue_while_waiting(&connection, &thread_id, "Use single quotes", &mut transcript, &mut follow).await;

    harness.restart().await;
    let mut reopened = Transcript::default();
    reopened.apply(next(&mut open(&harness.connect().await, &thread_id, 0).await).await);
    assert_eq!(reopened.queued, vec![Queued { held: true, ..queued }]);
}

#[tokio::test]
async fn a_message_queued_for_claude_is_sent_now_or_taken_back() {
    a_queued_message_is_sent_now_or_taken_back(Agent::Claude).await;
}

#[tokio::test]
async fn a_message_queued_for_codex_is_sent_now_or_taken_back() {
    a_queued_message_is_sent_now_or_taken_back(Agent::Codex).await;
}

async fn a_queued_message_is_sent_now_or_taken_back(agent: Agent) {
    let harness = Harness::start(fixture(agent), "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, agent).await;
    let thread_id = send(&connection, None, new_thread, "Run greet.py").await;
    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &thread_id, 0).await;
    let edit = first_approval(&mut transcript, &mut follow).await;

    let dropped = queue_while_waiting(&connection, &thread_id, "Never mind", &mut transcript, &mut follow).await;
    let kept = queue_while_waiting(&connection, &thread_id, "Use single quotes", &mut transcript, &mut follow).await;

    let cancel = Request::CancelQueued { thread_id: thread_id.clone(), message_id: dropped.id.clone() };
    assert_eq!(connection.request(&cancel).await.unwrap(), Message::Ok);
    let again = connection.request(&cancel).await.unwrap();
    assert!(matches!(again, Message::Error { message } if message.contains("no longer waiting")));
    let send_now = Request::SendQueued { thread_id: thread_id.clone(), message_id: kept.id.clone() };
    assert_eq!(connection.request(&send_now).await.unwrap(), Message::Ok);
    while !transcript.queued.is_empty() || !messages_and_turn_ends(&transcript).contains(&"Use single quotes") {
        transcript.apply(next(&mut follow).await);
    }
    let too_late = Request::CancelQueued { thread_id: thread_id.clone(), message_id: kept.id };
    let refused = connection.request(&too_late).await.unwrap();
    assert!(matches!(refused, Message::Error { message } if message.contains("no longer waiting")));

    let allow = |approval_id| Request::Answer {
        thread_id: thread_id.clone(),
        approval_id,
        allow: true,
        answers: HashMap::new(),
    };
    assert_eq!(connection.request(&allow(edit.id.clone())).await.unwrap(), Message::Ok);
    while transcript.approvals.first().is_none_or(|approval| approval.id == edit.id) {
        transcript.apply(next(&mut follow).await);
    }
    assert_eq!(connection.request(&allow(transcript.approvals[0].id.clone())).await.unwrap(), Message::Ok);
    transcript.follow_until_idle(&mut follow).await;

    assert_eq!(messages_and_turn_ends(&transcript), vec!["Run greet.py", "Use single quotes", "(turn end)"]);
    assert!(transcript.texts().last().unwrap().ends_with("You also said: Use single quotes"));
    assert!(transcript.queued.is_empty());
}

#[tokio::test]
async fn stopping_claude_keeps_its_queued_messages_until_they_are_sent() {
    stopping_a_turn_keeps_its_queued_messages_until_they_are_sent(Agent::Claude).await;
}

#[tokio::test]
async fn stopping_codex_keeps_its_queued_messages_until_they_are_sent() {
    stopping_a_turn_keeps_its_queued_messages_until_they_are_sent(Agent::Codex).await;
}

async fn stopping_a_turn_keeps_its_queued_messages_until_they_are_sent(agent: Agent) {
    let harness = Harness::start(fixture(agent), "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, agent).await;
    let thread_id = send(&connection, None, new_thread, "Run greet.py").await;
    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &thread_id, 0).await;
    first_approval(&mut transcript, &mut follow).await;
    let queued =
        queue_while_waiting(&connection, &thread_id, "What does the note say?", &mut transcript, &mut follow).await;

    connection.request(&Request::Stop { thread_id: thread_id.clone() }).await.unwrap();
    transcript.follow_until_idle(&mut follow).await;
    assert_eq!(messages_and_turn_ends(&transcript), vec!["Run greet.py", "(turn end)"]);
    assert!(transcript.queued.len() == 1 && transcript.queued[0].held, "the message stays, and waits to be sent");
    assert_eq!(harness.recorded_turns().len(), 1);

    let send_now = Request::SendQueued { thread_id: thread_id.clone(), message_id: queued.id };
    assert_eq!(connection.request(&send_now).await.unwrap(), Message::Ok);
    while transcript.turn_ends().len() < 2 {
        transcript.apply(next(&mut follow).await);
    }
    assert_eq!(
        messages_and_turn_ends(&transcript),
        vec!["Run greet.py", "(turn end)", "What does the note say?", "(turn end)"]
    );
    assert!(transcript.queued.is_empty());
    assert_eq!(harness.recorded_turns().len(), 2);
}

#[tokio::test]
async fn an_agent_that_monitors_takes_messages_and_wakes_by_itself() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;
    let thread_id = send(&connection, None, new_thread, "Watch the deploy").await;

    let monitoring = thread_where(&mut list, |thread| thread.monitoring).await;
    assert!(!monitoring.running && monitoring.turn_ended_at.is_some());
    let done = ThreadChange { done: Some(true), ..Default::default() };
    let refused = update(&connection, &thread_id, done).await;
    assert!(matches!(refused, Message::Error { message } if message.contains("still monitoring")));

    send(&connection, Some(thread_id.clone()), None, "How far is it?").await;
    thread_where(&mut list, |thread| thread.running).await;
    thread_where(&mut list, |thread| thread.monitoring).await;
    let woken = thread_where(&mut list, |thread| thread.running).await;
    assert!(!woken.monitoring);
    let ended = thread_where(&mut list, |thread| !thread.running).await;
    assert!(!ended.monitoring);

    let transcript = finished_transcript(&connection, &thread_id).await;
    assert_eq!(
        transcript.texts(),
        vec![
            "The deploy is rolling out. I'm watching it and will tell you when it is healthy.",
            "Still rolling out; 2 of 3 services are up. You said: How far is it?",
            "The deploy finished: all 3 services are healthy.",
        ]
    );
    assert_eq!(transcript.turn_ends().len(), 3);
    assert_eq!(transcript.tools(), vec![("Monitor", ToolStatus::Succeeded)]);
    assert!(transcript.errors().is_empty());
    assert_eq!(harness.recorded_turns().len(), 1, "the process that monitors takes the message");
}

#[tokio::test]
async fn what_the_agents_claude_starts_do_is_kept_apart_under_the_calls_that_started_them() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;
    let thread_id = send(&connection, None, new_thread, "Ask two agents").await;

    let working = thread_where(&mut list, |thread| thread.agents == 2).await;
    assert!(working.running);
    let transcript = finished_transcript(&connection, &thread_id).await;
    assert_eq!(thread_where(&mut list, |thread| !thread.running).await.agents, 0);

    let own: Vec<&Item> = transcript.items.iter().filter(|item| item.parent.is_none()).collect();
    let started: Vec<(&Item, &ToolCall)> = own
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Tool { call } => Some((*item, call)),
            _ => None,
        })
        .collect();
    let [(reader, read), (_, tested)] = &started[..] else { panic!("expected two calls, got {started:?}") };
    let agent = read.agent.as_ref().expect("the call started an agent");
    assert_eq!((agent.kind.as_deref(), agent.status), (Some("Explore"), ToolStatus::Succeeded));
    assert_eq!((agent.progress.as_deref(), agent.tool_uses), (Some("Searching for greet("), Some(2)));
    assert_eq!(tested.agent.as_ref().and_then(|agent| agent.result.as_deref()), Some("Both tests pass."));

    let did = |parent: &str| -> Vec<String> {
        let of_agent = transcript.items.iter().filter(|item| item.parent.as_deref() == Some(parent));
        let said = of_agent.map(|item| match &item.kind {
            ItemKind::Tool { call } => format!("{} {:?}", call.name, call.status),
            ItemKind::Assistant { text } => text.clone(),
            other => panic!("unexpected item of an agent: {other:?}"),
        });
        said.collect()
    };
    assert_eq!(
        did(&reader.id),
        [
            "Read Succeeded",
            "Grep Succeeded",
            "`greet.py` defines `greet(name)`, which returns a greeting, and prints one."
        ]
    );
    assert_eq!(messages_and_turn_ends(&transcript), ["Ask two agents", "(turn end)"]);
}

#[tokio::test]
async fn what_an_agent_codex_starts_does_neither_joins_its_turn_nor_ends_it() {
    let harness = Harness::start(fixture(Agent::Codex), "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Codex).await;
    let thread_id = send(&connection, None, new_thread, "Ask an agent").await;
    let transcript = finished_transcript(&connection, &thread_id).await;

    let tools = |of_agent: bool| -> Vec<&ToolCall> {
        let items = transcript.items.iter().filter(|item| item.parent.is_some() == of_agent);
        let calls = items.filter_map(|item| match &item.kind {
            ItemKind::Tool { call } => Some(call),
            _ => None,
        });
        calls.collect()
    };
    let [started] = &tools(false)[..] else { panic!("expected the call that started the agent") };
    let agent = started.agent.as_ref().expect("the call started an agent");
    assert_eq!((started.input.as_str(), agent.status), (r#"{"description":"Read greet"}"#, ToolStatus::Succeeded));
    assert_eq!(agent.result.as_deref(), Some("`greet.py` defines `greet(name)`."));
    assert_eq!(tools(true).iter().map(|call| call.name.as_str()).collect::<Vec<_>>(), ["Bash"]);
    assert_eq!(messages_and_turn_ends(&transcript), ["Ask an agent", "(turn end)"]);
    let last = transcript.items.iter().rfind(|item| matches!(item.kind, ItemKind::Assistant { .. })).unwrap();
    assert_eq!(last.parent, None);
}

#[tokio::test]
async fn stopping_an_agent_that_monitors_ends_the_watch() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;
    let thread_id = send(&connection, None, new_thread, "Watch the deploy").await;
    let monitoring = thread_where(&mut list, |thread| thread.monitoring).await;

    connection.request(&Request::Stop { thread_id: thread_id.clone() }).await.unwrap();
    let stopped = thread_where(&mut list, |thread| !thread.monitoring).await;
    assert!(!stopped.running);
    assert_eq!(stopped.turn_ended_at, monitoring.turn_ended_at);

    let transcript = finished_transcript(&connection, &thread_id).await;
    assert_eq!(transcript.texts().len(), 1, "the watch ended before it saw anything");
    assert_eq!(transcript.turn_ends().len(), 1);
    assert!(transcript.errors().is_empty());

    send(&connection, Some(thread_id.clone()), None, "What does the note say?").await;
    finished_transcript(&connection, &thread_id).await;
    let turns = harness.recorded_turns();
    assert_eq!(turns.len(), 2);
    assert!(turns[1].contains("--resume\n00000000-0000-4000-8000-00000000d00d"), "{}", turns[1]);
}

#[tokio::test]
async fn a_thread_is_marked_done_and_comes_back_with_new_activity() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0.1").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;
    let thread_id = send(&connection, None, new_thread, "What does the note say?").await;

    let done = ThreadChange { done: Some(true), ..Default::default() };
    let refused = update(&connection, &thread_id, done.clone()).await;
    assert!(matches!(refused, Message::Error { message } if message.contains("still working")));

    finished_transcript(&connection, &thread_id).await;
    assert_eq!(update(&connection, &thread_id, done).await, Message::Ok);
    let marked = thread_where(&mut list, |thread| thread.done_at.is_some()).await;
    assert_eq!(marked.position, marked.created_at);

    // Coming back from done puts the thread at the top of the active ones.
    let undone = ThreadChange { done: Some(false), ..Default::default() };
    assert_eq!(update(&connection, &thread_id, undone).await, Message::Ok);
    let back = thread_where(&mut list, |thread| thread.done_at.is_none()).await;
    assert!(back.position > back.created_at);

    let done = ThreadChange { done: Some(true), ..Default::default() };
    assert_eq!(update(&connection, &thread_id, done).await, Message::Ok);
    thread_where(&mut list, |thread| thread.done_at.is_some()).await;
    send(&connection, Some(thread_id.clone()), None, "One more thing").await;
    let reopened = thread_where(&mut list, |thread| thread.running).await;
    assert_eq!(reopened.done_at, None);
    assert!(reopened.position > back.position);
}

#[tokio::test]
async fn a_device_outside_the_account_is_refused() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let stranger = harness.connect_as(&DeviceKey::generate()).await;

    let closed = tokio::time::timeout(TIMEOUT, stranger.closed()).await.unwrap();
    assert!(closed.refused && closed.reason.contains("isn't linked"), "{}", closed.reason);
    assert!(stranger.request(&Request::ListDir { path: None, icons: false, hidden: false }).await.is_err());
}

#[tokio::test]
async fn the_model_effort_and_access_chosen_for_a_thread_reach_the_agent() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let project = harness.project(&connection).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { server, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
    let opus = server.models.iter().find(|model| model.id == "claude-opus-5-5").expect("Claude's models are offered");
    assert_eq!(opus.agent, Agent::Claude);
    assert!(opus.efforts.contains(&"xhigh".to_string()));

    let new_thread = NewThread {
        project_id: project.id,
        agent: Agent::Claude,
        agent_account: None,
        model: Some(opus.id.clone()),
        effort: Some("xhigh".to_string()),
        access: AgentAccess::Full,
        plan: false,
        worktree: None,
    };
    let thread_id = send(&connection, None, Some(new_thread), "What is in README.md?").await;
    finished_transcript(&connection, &thread_id).await;
    let first = &harness.recorded_turns()[0];
    assert!(
        first.contains("--permission-mode\nbypassPermissions\n--model\nclaude-opus-5-5\n--effort\nxhigh"),
        "{first}"
    );
    assert!(
        first.contains(
            "--append-system-prompt\nIn case you're asked: you are running in Motile through the Claude Code harness. \
             No need to mention this otherwise. You can show the user an image or a video"
        ),
        "{first}"
    );

    let change = ThreadChange { effort: Some("low".to_string()), plan: Some(true), ..Default::default() };
    assert_eq!(update(&connection, &thread_id, change).await, Message::Ok);
    send(&connection, Some(thread_id.clone()), None, "And in LICENSE?").await;
    finished_transcript(&connection, &thread_id).await;
    let second = &harness.recorded_turns()[1];
    assert!(second.contains("--permission-mode\nplan\n--model\nclaude-opus-5-5\n--effort\nlow"), "{second}");

    let odd = ThreadChange { effort: Some("low\" sandbox".to_string()), ..Default::default() };
    assert!(matches!(update(&connection, &thread_id, odd).await, Message::Error { .. }));
}

#[tokio::test]
async fn a_thread_works_with_the_account_it_was_started_with_and_moves_to_another() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let project = harness.project(&connection).await;
    let recorded = harness.dir.path().join("personal.txt").to_string_lossy().into_owned();
    let personal = AgentAccount {
        id: String::new(),
        agent: Agent::Claude,
        name: "Personal".into(),
        folder: "~/.claude-personal".into(),
        shares_sessions: false,
        variables: vec![Variable { name: "FAKE_AGENT_ARGUMENTS_FILE".into(), value: recorded, sensitive: true }],
        email: None,
        plan: None,
    };
    let saved = connection.request(&Request::SaveAgentAccount { account: personal }).await.unwrap();
    assert_eq!(saved, Message::Ok);
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { server, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
    let ids: Vec<&str> = server.agent_accounts.iter().map(|account| account.id.as_str()).collect();
    assert_eq!(ids, ["claude", "codex", "claude-personal"]);
    assert_eq!(server.agent_accounts[2].variables[0].value, "", "a sensitive value stays on the server");

    let new_thread = NewThread {
        project_id: project.id,
        agent: Agent::Claude,
        agent_account: Some("claude-personal".into()),
        model: None,
        effort: None,
        access: AgentAccess::Full,
        plan: false,
        worktree: None,
    };
    let thread_id = send(&connection, None, Some(new_thread), "What is in README.md?").await;
    finished_transcript(&connection, &thread_id).await;
    send(&connection, Some(thread_id.clone()), None, "And in LICENSE?").await;
    finished_transcript(&connection, &thread_id).await;
    // Reading what the accounts have used, which saving one does, starts their agents too.
    let turns_in = |file: &str| -> Vec<String> {
        let turns = harness.recorded_turns_in(file).into_iter();
        turns.filter(|arguments| arguments.contains("--append-system-prompt")).collect()
    };
    let turns = turns_in("personal.txt");
    assert_eq!(turns.len(), 2, "{turns:?}");
    assert!(!turns[0].contains("--resume") && turns[1].contains("--resume"), "{turns:?}");
    assert!(turns_in("arguments.txt").is_empty());

    let change = ThreadChange { agent_account: Some("claude".into()), ..Default::default() };
    assert_eq!(update(&connection, &thread_id, change).await, Message::Ok);
    send(&connection, Some(thread_id.clone()), None, "And in Cargo.toml?").await;
    finished_transcript(&connection, &thread_id).await;
    let moved = turns_in("arguments.txt");
    assert_eq!(moved.len(), 1, "{moved:?}");
    assert!(!moved[0].contains("--resume"), "a new session starts where the old one isn't: {moved:?}");

    let removed = connection.request(&Request::RemoveAgentAccount { id: "claude".into() }).await.unwrap();
    assert!(matches!(removed, Message::Error { .. }), "the default account stays");
    let removed = connection.request(&Request::RemoveAgentAccount { id: "claude-personal".into() }).await.unwrap();
    assert_eq!(removed, Message::Ok);
}

#[tokio::test]
async fn projects_are_added_and_removed_and_show_their_branch() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { projects, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
    assert!(own(projects).is_empty());

    let repository = harness.dir.path().join("repository");
    std::fs::create_dir_all(repository.join(".git")).unwrap();
    std::fs::write(repository.join(".git/HEAD"), "ref: refs/heads/feature/login\n").unwrap();
    let path = repository.to_string_lossy().into_owned();
    assert_eq!(connection.request(&Request::AddProject { path: format!("{path}/") }).await.unwrap(), Message::Ok);
    let Message::Projects { projects } = next(&mut list).await else { panic!("expected the projects") };
    let projects = own(projects);
    assert_eq!(projects.len(), 1);
    assert_eq!((projects[0].name.as_str(), projects[0].branch.as_deref()), ("repository", Some("feature/login")));

    // Adding the same folder again changes nothing.
    assert_eq!(connection.request(&Request::AddProject { path }).await.unwrap(), Message::Ok);
    let missing = Request::AddProject { path: "/no/such/folder".to_string() };
    assert!(matches!(connection.request(&missing).await.unwrap(), Message::Error { .. }));

    // Only folders are listed, and the images among the files when an icon is being chosen.
    std::fs::write(harness.dir.path().join("logo.png"), "png").unwrap();
    std::fs::write(harness.dir.path().join("notes.txt"), "notes").unwrap();
    let here = Some(harness.dir.path().to_string_lossy().into_owned());
    for (icons, images) in [(false, Vec::new()), (true, vec!["logo.png".to_string()])] {
        let Message::Dir { folders, files, .. } =
            connection.request(&Request::ListDir { path: here.clone(), icons, hidden: false }).await.unwrap()
        else {
            panic!("expected the folder's contents")
        };
        assert_eq!(folders, vec!["no-project", "repository"]);
        assert_eq!(files, images);
    }

    let remove = Request::RemoveProject { project_id: projects[0].id.clone() };
    assert_eq!(connection.request(&remove).await.unwrap(), Message::Ok);
    let Message::Projects { projects } = next(&mut list).await else { panic!("expected the projects") };
    assert!(own(projects).is_empty());
}

/// Stands in for a GitHub login with two repositories, once `signed-in` is next to it.
const FAKE_GH_LOGIN: &str = r#"#!/bin/sh
case "$1 $2" in
"auth token")
    [ -f "$(dirname "$0")/signed-in" ] || { echo "no oauth token found for github.com" >&2; exit 1; }
    echo token ;;
"api user/repos"*)
    [ -f "$(dirname "$0")/signed-in" ] || { echo "To get started with GitHub CLI, please run:  gh auth login" >&2; exit 4; }
    case "$2" in
    *"&page=1")
        echo '{"full_name":"acme/app","description":"The app","private":true}'
        echo '{"full_name":"me/notes","description":null,"private":false}' ;;
    esac ;;
"repo clone")
    git init --quiet "$4" && git -C "$4" remote add origin "https://github.com/$3.git" ;;
esac
"#;

#[tokio::test]
async fn a_project_is_started_from_a_name_or_cloned_from_github() {
    use std::os::unix::fs::PermissionsExt;
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let bin = harness.dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("gh"), FAKE_GH_LOGIN).unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let added = |answer: Message| match answer {
        Message::ProjectAdded { project_id } => project_id,
        other => panic!("expected the added project, got {other:?}"),
    };
    let path_of =
        |projects: Vec<Project>, id: &str| projects.into_iter().find(|project| project.id == id).unwrap().path;
    let projects = harness.dir.path().join("projects");

    let named = added(connection.request(&Request::NewProject { name: "My App!".to_string() }).await.unwrap());
    let folder = projects.join("my-app");
    assert_eq!(path_of(projects_now(&connection).await, &named), folder.to_string_lossy());
    assert!(folder.join(".git").is_dir());
    let again = connection.request(&Request::NewProject { name: "my app".to_string() }).await.unwrap();
    assert!(matches!(again, Message::Error { message } if message.contains("already exists")));

    // Nobody is signed in to gh yet.
    let signed_out = Message::Github { state: GitHubState::SignedOut };
    assert_eq!(connection.request(&Request::GithubStatus).await.unwrap(), signed_out);
    assert_eq!(connection.request(&Request::GithubRepos).await.unwrap(), signed_out);

    std::fs::write(bin.join("signed-in"), "").unwrap();
    let ready = Message::Github { state: GitHubState::Ready };
    assert_eq!(connection.request(&Request::GithubStatus).await.unwrap(), ready);
    let Message::Repos { repos } = connection.request(&Request::GithubRepos).await.unwrap() else {
        panic!("expected the repositories")
    };
    let listed: Vec<_> = repos.iter().map(|repo| (repo.name.as_str(), repo.private)).collect();
    assert_eq!(listed, vec![("acme/app", true), ("me/notes", false)]);

    let clone = Request::CloneRepo { repo: "acme/app".to_string() };
    let cloned = added(connection.request(&clone).await.unwrap());
    assert_eq!(path_of(projects_now(&connection).await, &cloned), projects.join("app").to_string_lossy());
    // A folder that is the repository already is the same project.
    assert_eq!(added(connection.request(&clone).await.unwrap()), cloned);
    // One that is something else is left alone.
    let other = connection.request(&Request::CloneRepo { repo: "acme/my-app".to_string() }).await.unwrap();
    assert!(matches!(other, Message::Error { message } if message.contains("isn't acme/my-app")));
}

fn git(folder: &Path, arguments: &[&str]) {
    let output = std::process::Command::new("git")
        .args(arguments)
        .current_dir(folder)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {arguments:?}: {}", String::from_utf8_lossy(&output.stderr));
}

async fn branches(connection: &Connection, project_id: &str) -> Vec<(String, bool, bool, bool)> {
    let request = Request::Branches { project_id: project_id.to_string() };
    let Message::Branches { branches } = connection.request(&request).await.unwrap() else {
        panic!("expected branches")
    };
    branches.into_iter().map(|branch| (branch.name, branch.current, branch.default, branch.remote)).collect()
}

async fn switch(connection: &Connection, project_id: &str, branch: &str, create: bool) -> Message {
    let request = Request::SwitchBranch { project_id: project_id.to_string(), branch: branch.to_string(), create };
    connection.request(&request).await.unwrap()
}

#[tokio::test]
async fn a_projects_branches_are_listed_switched_and_created() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0.2").await;
    let connection = harness.connect().await;
    let origin = harness.dir.path().join("origin");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "-q", "-b", "main"]);
    std::fs::write(origin.join("README"), "one").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "one"]);
    git(&origin, &["switch", "-q", "-c", "fix/typo"]);
    std::fs::write(origin.join("README"), "two").unwrap();
    git(&origin, &["commit", "-q", "-am", "two"]);
    git(&origin, &["switch", "-q", "main"]);
    let repository = harness.dir.path().join("repository");
    git(harness.dir.path(), &["clone", "-q", "origin", "repository"]);
    git(&repository, &["switch", "-q", "-c", "feature/login"]);

    let path = repository.to_string_lossy().into_owned();
    assert_eq!(connection.request(&Request::AddProject { path }).await.unwrap(), Message::Ok);
    let project = projects_now(&connection).await.remove(0);
    assert_eq!(project.branch.as_deref(), Some("feature/login"));

    // The checked-out branch leads, the default follows, and a branch only on the remote is last.
    let name = |name: &str, current, default, remote| (name.to_string(), current, default, remote);
    assert_eq!(
        branches(&connection, &project.id).await,
        [
            name("feature/login", true, false, false),
            name("main", false, true, false),
            name("fix/typo", false, false, true)
        ]
    );

    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;
    assert_eq!(switch(&connection, &project.id, "main", false).await, Message::Ok);
    let Message::Projects { projects } = next(&mut list).await else { panic!("expected the projects") };
    assert_eq!(own(projects)[0].branch.as_deref(), Some("main"));

    // Switching to a remote's branch makes the local one.
    assert_eq!(switch(&connection, &project.id, "fix/typo", false).await, Message::Ok);
    assert_eq!(std::fs::read_to_string(repository.join("README")).unwrap(), "two");
    assert!(branches(&connection, &project.id).await.contains(&name("fix/typo", true, false, false)));

    assert_eq!(switch(&connection, &project.id, "feature/pay", true).await, Message::Ok);
    assert_eq!(branches(&connection, &project.id).await[0], name("feature/pay", true, false, false));
    assert!(matches!(switch(&connection, &project.id, "feature/pay", true).await, Message::Error { .. }));
    assert!(matches!(switch(&connection, &project.id, "no such", true).await, Message::Error { .. }));

    // A change that the switch would lose stops it, in git's words.
    std::fs::write(repository.join("README"), "three").unwrap();
    let Message::Error { message } = switch(&connection, &project.id, "main", false).await else {
        panic!("a switch that loses a change is refused")
    };
    assert!(message.contains("README") && message.contains("overwritten"), "{message}");
    git(&repository, &["checkout", "-q", "README"]);

    // Not while an agent works in the project.
    let new_thread = NewThread {
        project_id: project.id.clone(),
        agent: Agent::Claude,
        agent_account: None,
        model: None,
        effort: None,
        access: AgentAccess::Supervised,
        plan: false,
        worktree: None,
    };
    let thread_id = send(&connection, None, Some(new_thread), "Look at the README").await;
    let Message::Error { message } = switch(&connection, &project.id, "main", false).await else {
        panic!("a switch while an agent works is refused")
    };
    assert!(message.contains("working"), "{message}");
    connection.request(&Request::Stop { thread_id }).await.unwrap();

    let plain = harness.folder("plain");
    assert_eq!(connection.request(&Request::AddProject { path: plain }).await.unwrap(), Message::Ok);
    let plain = projects_now(&connection).await.into_iter().find(|project| project.branch.is_none()).unwrap();
    let request = Request::Branches { project_id: plain.id };
    assert!(matches!(connection.request(&request).await.unwrap(), Message::Error { .. }));
}

async fn pull_request_action(
    connection: &Connection,
    project_id: &str,
    action: PullRequestAction,
    method: Option<MergeMethod>,
    text: Option<&str>,
) -> Result<(String, Option<String>, PullRequestDetail), String> {
    let request = Request::PullRequestAction {
        project_id: project_id.to_string(),
        thread_id: None,
        number: 7,
        action,
        method,
        text: text.map(str::to_string),
    };
    match connection.request(&request).await.unwrap() {
        Message::PullRequestDone { title, url, pull_request } => Ok((title, url, *pull_request)),
        Message::Error { message } => Err(message),
        other => panic!("expected what the action did: {other:?}"),
    }
}

#[tokio::test]
async fn a_folder_without_git_is_never_fetched_and_can_be_made_a_repository() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0.2").await;
    let connection = harness.connect().await;
    let folder = harness.dir.path().join("notes");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.to_string_lossy().into_owned();
    assert_eq!(connection.request(&Request::AddProject { path }).await.unwrap(), Message::Ok);
    let project = projects_now(&connection).await.remove(0);
    assert_eq!(project.branch, None);

    let status = Request::GitStatus { project_id: project.id.clone(), thread_id: None, fetch: true };
    assert!(matches!(
        connection.request(&status).await.unwrap(),
        Message::GitStatus { status: None, problem: None, .. }
    ));

    let init = Request::InitRepository { project_id: project.id.clone() };
    assert_eq!(connection.request(&init).await.unwrap(), Message::Ok);
    let project = projects_now(&connection).await.remove(0);
    assert!(project.branch.is_some() && project.git.is_some());
    assert!(matches!(connection.request(&init).await.unwrap(), Message::Error { .. }));
}

#[tokio::test]
async fn a_pull_request_is_read_reviewed_and_merged_from_its_folder() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0.2").await;
    let connection = harness.connect().await;
    let root = harness.dir.path();
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(repo_file("scripts/fake-gh"), bin.join("gh")).unwrap();

    git(root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
    git(root, &["clone", "-q", "origin.git", "repository"]);
    let repository = root.join("repository");
    std::fs::write(repository.join("greet.py"), "print('hello')\n").unwrap();
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "-q", "-m", "Add the greeting"]);
    git(&repository, &["push", "-q", "-u", "origin", "main"]);
    git(&repository, &["switch", "-q", "-c", "greet"]);
    std::fs::write(repository.join("greet.py"), "print('hello you')\n").unwrap();
    git(&repository, &["commit", "-q", "-am", "Greet you"]);

    let path = repository.to_string_lossy().into_owned();
    assert_eq!(connection.request(&Request::AddProject { path }).await.unwrap(), Message::Ok);
    let project = projects_now(&connection).await.remove(0);
    let (_, end) = git_run(&connection, &project.id, run(GitAction::CreatePr)).await;
    assert_eq!(done(end).0, "Created PR #7");

    // The tab reads where it stands, and what it changes.
    let request = Request::PullRequest { project_id: project.id.clone(), thread_id: None, number: 7 };
    let Message::PullRequest { pull_request } = connection.request(&request).await.unwrap() else {
        panic!("expected the pull request")
    };
    assert_eq!((pull_request.base.as_str(), pull_request.head.as_str()), ("main", "greet"));
    assert_eq!(
        (pull_request.commits, pull_request.changed_files, pull_request.mergeable),
        (1, 1, Mergeable::Mergeable)
    );
    assert_eq!(pull_request.merge_methods, [MergeMethod::Squash, MergeMethod::Merge, MergeMethod::Rebase]);
    assert!(pull_request.checks.iter().all(|check| check.status == CheckStatus::Success));
    assert!(matches!(&pull_request.activity[0].kind, EventKind::Commit { headline, .. } if headline == "Greet you"));
    let diff =
        Request::Diff { project_id: project.id.clone(), thread_id: None, scope: DiffScope::PullRequest { number: 7 } };
    let Message::Diff { patch, truncated: false } = connection.request(&diff).await.unwrap() else {
        panic!("expected the pull request's patch")
    };
    assert!(patch.contains("+print('hello you')"), "{patch}");

    // A comment joins its activity.
    let (title, _, commented) =
        pull_request_action(&connection, &project.id, PullRequestAction::Comment, None, Some("Looks good"))
            .await
            .unwrap();
    assert_eq!(title, "Commented on PR #7");
    assert!(
        commented
            .activity
            .iter()
            .any(|event| matches!(&event.kind, EventKind::Comment { body, .. } if body == "Looks good"))
    );

    // GitHub's refusal is said as it is, and nothing changes.
    change_pull_request(&bin, json!({"mergeable": "CONFLICTING"}));
    let refused =
        pull_request_action(&connection, &project.id, PullRequestAction::Merge, Some(MergeMethod::Squash), None).await;
    assert!(refused.unwrap_err().contains("is not mergeable"));
    change_pull_request(&bin, json!({"mergeable": "MERGEABLE"}));

    // Merged, it is merged for the git button too, and can be reverted.
    let (title, _, merged) =
        pull_request_action(&connection, &project.id, PullRequestAction::Merge, Some(MergeMethod::Squash), None)
            .await
            .unwrap();
    assert_eq!(title, "Squashed and merged PR #7");
    assert!(merged.pull_request.merged && merged.merged_at.is_some());
    let (status, _) = git_status(&connection, &project.id, false).await;
    assert_eq!(status.pull_request.map(|found| found.merged), Some(true));
    let (title, url, _) =
        pull_request_action(&connection, &project.id, PullRequestAction::Revert, None, None).await.unwrap();
    assert_eq!(
        (title.as_str(), url.as_deref()),
        ("Opened PR #8 to revert PR #7", Some("https://github.com/acme/app/pull/8"))
    );
    let refused = pull_request_action(&connection, &project.id, PullRequestAction::Close, None, None).await;
    assert!(refused.unwrap_err().contains("is not open"));
}

async fn git_status(connection: &Connection, project_id: &str, fetch: bool) -> (GitStatus, Vec<String>) {
    let request = Request::GitStatus { project_id: project_id.to_string(), thread_id: None, fetch };
    let Message::GitStatus { status, files, .. } = connection.request(&request).await.unwrap() else {
        panic!("expected the status")
    };
    (status.expect("the folder is a repository"), files.into_iter().map(|file| file.path).collect())
}

struct GitRun<'a> {
    action: GitAction,
    message: Option<&'a str>,
    paths: &'a [&'a str],
    new_branch: bool,
    thread_id: Option<&'a str>,
}

fn run(action: GitAction) -> GitRun<'static> {
    GitRun { action, message: None, paths: &[], new_branch: false, thread_id: None }
}

/// The stages the server went through, and how the run ended.
async fn git_run(connection: &Connection, project_id: &str, run: GitRun<'_>) -> (Vec<GitStage>, Message) {
    let request = Request::GitRun {
        project_id: project_id.to_string(),
        action: run.action,
        thread_id: run.thread_id.map(str::to_string),
        message: run.message.map(str::to_string),
        paths: run.paths.iter().map(|path| path.to_string()).collect(),
        new_branch: run.new_branch,
    };
    let mut follow = connection.follow(&request).await.unwrap();
    let mut stages = Vec::new();
    loop {
        match next(&mut follow).await {
            Message::GitProgress { stage } => stages.push(stage),
            end => return (stages, end),
        }
    }
}

/// What a run that worked said it did: its title, its description and what follows.
fn done(end: Message) -> (String, String, Option<GitAction>) {
    let Message::GitDone { title, description, next, .. } = end else { panic!("the run failed: {end:?}") };
    (title, description.unwrap_or_default(), next)
}

#[tokio::test]
async fn changes_are_committed_pushed_and_opened_as_a_pull_request() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0.2").await;
    let connection = harness.connect().await;
    let root = harness.dir.path();
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(repo_file("scripts/fake-gh"), bin.join("gh")).unwrap();

    git(root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
    git(root, &["clone", "-q", "origin.git", "repository"]);
    let repository = root.join("repository");
    git(&repository, &["config", "user.name", "Test"]);
    git(&repository, &["config", "user.email", "test@example.com"]);
    std::fs::write(repository.join("greet.py"), "print('hello')\n").unwrap();
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "-q", "-m", "Add the greeting"]);
    git(&repository, &["push", "-q", "-u", "origin", "main"]);

    let path = repository.to_string_lossy().into_owned();
    assert_eq!(connection.request(&Request::AddProject { path }).await.unwrap(), Message::Ok);
    let project = projects_now(&connection).await.remove(0);
    let (status, files) = git_status(&connection, &project.id, false).await;
    assert_eq!(
        (status.branch.as_deref(), status.default, status.upstream, status.changed),
        (Some("main"), true, true, 0)
    );
    assert!(status.pull_requests && files.is_empty());

    std::fs::write(repository.join("greet.py"), "name = 'you'\nprint(f'hello {name}')\n").unwrap();
    std::fs::write(repository.join("notes.txt"), "later\n").unwrap();
    let (status, files) = git_status(&connection, &project.id, false).await;
    assert_eq!((status.changed, status.added, status.removed), (2, 2, 1));
    assert_eq!(files, ["greet.py", "notes.txt"]);

    // One run takes the picked file from the default branch to a pull request: the agent names
    // the branch and writes the commit message and the pull request.
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;
    let whole = GitRun { paths: &["greet.py"], new_branch: true, ..run(GitAction::CommitPushPr) };
    let (stages, end) = git_run(&connection, &project.id, whole).await;
    use GitStage::{Commit, Message as Written, Pull, PullRequest, PullRequestText, Push};
    assert_eq!(stages, [Written, Commit, Push, PullRequestText, PullRequest]);
    let Message::GitDone { title, description, url, next: None } = end else { panic!("the run failed: {end:?}") };
    assert_eq!((title.as_str(), description.as_deref()), ("Created PR #7", Some("Greet with an f-string")));
    assert_eq!(url.as_deref(), Some("https://github.com/acme/app/pull/7"));
    git(&root.join("origin.git"), &["rev-parse", "--verify", "-q", "refs/heads/greet-f-string"]);
    let state: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(bin.join("fake-gh.json")).unwrap()).unwrap();
    assert_eq!(state["pulls"][0]["body"].as_str().unwrap().trim(), "Greets by name.\n\nGreet with an f-string");

    let (status, files) = git_status(&connection, &project.id, false).await;
    assert_eq!((status.branch.as_deref(), status.default, status.upstream), (Some("greet-f-string"), false, true));
    assert_eq!((status.ahead, status.ahead_of_default, files), (0, 1, vec!["notes.txt".to_string()]));
    assert_eq!(status.default_branch.as_deref(), Some("main"));
    assert_eq!(
        status.pull_request.map(|opened| (opened.number, opened.title)),
        Some((7, "Greet with an f-string".to_string()))
    );
    let Message::Projects { projects } = next(&mut list).await else { panic!("expected the projects") };
    assert_eq!(own(projects)[0].git.as_ref().map(|git| git.ahead_of_default), Some(1));

    // A message that is given is used as it is, and a commit says that a push follows.
    let (stages, end) =
        git_run(&connection, &project.id, GitRun { message: Some("Add notes"), ..run(GitAction::Commit) }).await;
    let (title, description, follows) = done(end);
    assert_eq!((stages, description.as_str(), follows), (vec![Commit], "Add notes", Some(GitAction::Push)));
    assert!(title.starts_with("Committed "), "{title}");
    let (stages, end) = git_run(&connection, &project.id, run(GitAction::Push)).await;
    let (title, description, follows) = done(end);
    assert_eq!((stages, description.as_str(), follows), (vec![Push], "Add notes", None));
    assert!(title.starts_with("Pushed ") && title.ends_with(" to origin/greet-f-string"), "{title}");

    // A commit on the remote is seen once it is fetched, and pulled.
    git(root, &["clone", "-q", "-b", "greet-f-string", "origin.git", "other"]);
    let other = root.join("other");
    std::fs::write(other.join("greet.py"), "print('hi')\n").unwrap();
    git(&other, &["commit", "-q", "-am", "Say hi"]);
    git(&other, &["push", "-q"]);
    assert_eq!(git_status(&connection, &project.id, false).await.0.behind, 0);
    assert_eq!(git_status(&connection, &project.id, true).await.0.behind, 1);
    let (stages, end) = git_run(&connection, &project.id, run(GitAction::Pull)).await;
    assert_eq!((stages, done(end).0.as_str()), (vec![Pull], "Pulled"));
    assert_eq!(std::fs::read_to_string(repository.join("greet.py")).unwrap(), "print('hi')\n");
    assert_eq!(done(git_run(&connection, &project.id, run(GitAction::Pull)).await.1).0, "Already up to date");

    let (_, end) = git_run(&connection, &project.id, run(GitAction::Commit)).await;
    assert_eq!(end, Message::Error { message: "There is nothing to commit.".to_string() });

    // The model picked for the server writes the next message.
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { server, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
    let model = server.models.iter().find(|model| model.agent == Agent::Claude).unwrap().id.clone();
    assert_eq!(server.text_model, None);
    let unknown = Request::SetTextModel { model: Some("no-such-model".to_string()) };
    assert!(matches!(connection.request(&unknown).await.unwrap(), Message::Error { .. }));
    let pick = Request::SetTextModel { model: Some(model.clone()) };
    assert_eq!(connection.request(&pick).await.unwrap(), Message::Ok);
    std::fs::write(repository.join("notes.txt"), "sooner\n").unwrap();
    let (stages, end) = git_run(&connection, &project.id, run(GitAction::Commit)).await;
    assert_eq!((stages, done(end).1.as_str()), (vec![Written, Commit], "Greet with an f-string"));
    let recorded = std::fs::read_to_string(root.join("arguments.txt")).unwrap();
    let written_by: Vec<String> = recorded.lines().last().map(|line| serde_json::from_str(line).unwrap()).unwrap();
    assert!(written_by.windows(2).any(|pair| pair[0] == "--model" && pair[1] == model), "{written_by:?}");

    // A merged pull request stays the branch's until the branch has a commit it doesn't.
    let head = git_says(&repository, &["rev-parse", "HEAD"]);
    change_pull_request(&bin, json!({"state": "MERGED", "headOid": head}));
    let (status, _) = git_status(&connection, &project.id, true).await;
    assert_eq!(status.pull_request.map(|merged| (merged.number, merged.merged)), Some((7, true)));
    std::fs::write(repository.join("notes.txt"), "now\n").unwrap();
    git(&repository, &["commit", "-q", "-am", "Note it now"]);
    assert_eq!(git_status(&connection, &project.id, false).await.0.pull_request, None);

    // A pull request opened for a thread is that thread's own: it stays with it whatever the
    // folder's branch does, and follows what GitHub says of it, done or not.
    change_pull_request(&bin, json!({"state": "OPEN"}));
    let new_thread =
        NewThread { project_id: project.id.clone(), ..harness.new_thread(&connection, Agent::Claude).await.unwrap() };
    let thread_id = send(&connection, None, Some(new_thread), "Greet by an f-string").await;
    finished_transcript(&connection, &thread_id).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;
    let (_, end) =
        git_run(&connection, &project.id, GitRun { thread_id: Some(&thread_id), ..run(GitAction::CreatePr) }).await;
    assert_eq!(done(end).0, "PR #7 is already open");
    let linked = thread_where(&mut list, |thread| thread.pull_request.is_some()).await;
    assert_eq!(linked.pull_request.map(|opened| (opened.number, opened.is_open())), Some((7, true)));
    let done = ThreadChange { done: Some(true), ..Default::default() };
    assert_eq!(update(&connection, &thread_id, done).await, Message::Ok);
    let branch = git_says(&repository, &["rev-parse", "--abbrev-ref", "HEAD"]);
    git(&repository, &["checkout", "-q", "main"]);
    change_pull_request(&bin, json!({"state": "CLOSED"}));
    let closed = thread_where(&mut list, |thread| thread.pull_request.as_ref().is_some_and(|found| found.closed)).await;
    assert_eq!(closed.id, thread_id);

    // A thread at work whose pull request is over goes on with the one its branch gets, however
    // it was opened; a done thread keeps its own.
    git(&repository, &["checkout", "-q", &branch]);
    std::fs::write(repository.join("notes.txt"), "again\n").unwrap();
    git(&repository, &["commit", "-q", "-am", "Note it again"]);
    let created = std::process::Command::new(bin.join("gh"))
        .args(["pr", "create", "--title", "Note it again", "--body", "Again"])
        .current_dir(&repository)
        .output()
        .unwrap();
    assert!(created.status.success(), "{}", String::from_utf8_lossy(&created.stderr));
    let (status, _) = git_status(&connection, &project.id, false).await;
    assert_eq!(status.pull_request.as_ref().map(|found| found.number), Some(8));
    assert_eq!(thread_now(&connection, &thread_id).await.pull_request.map(|found| found.number), Some(7));
    let undone = ThreadChange { done: Some(false), ..Default::default() };
    assert_eq!(update(&connection, &thread_id, undone).await, Message::Ok);
    git_status(&connection, &project.id, false).await;
    let moved_on =
        thread_where(&mut list, |thread| thread.pull_request.as_ref().is_some_and(|found| found.number == 8)).await;
    assert_eq!((moved_on.id, moved_on.pull_request.map(|found| found.is_open())), (thread_id, Some(true)));
}

/// Edits the pull request with the number 7 through the server, as the tab does.
async fn pull_request_edit(
    connection: &Connection,
    project_id: &str,
    edit: PullRequestEdit,
) -> Result<(String, PullRequestDetail), String> {
    let request = Request::PullRequestEdit { project_id: project_id.to_string(), thread_id: None, number: 7, edit };
    match connection.request(&request).await.unwrap() {
        Message::PullRequestDone { title, pull_request, .. } => Ok((title, *pull_request)),
        Message::Error { message } => Err(message),
        other => panic!("expected what the edit did: {other:?}"),
    }
}

/// A repository with a remote, `greet.py` on `main` and a branch `greet` that changes it, added
/// as a project, with `scripts/fake-gh` as GitHub's `gh`.
async fn project_with_a_branch(harness: &Harness, connection: &Connection) -> (Project, PathBuf) {
    let root = harness.dir.path();
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(repo_file("scripts/fake-gh"), bin.join("gh")).unwrap();
    git(root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
    git(root, &["clone", "-q", "origin.git", "repository"]);
    let repository = root.join("repository");
    std::fs::write(repository.join("greet.py"), "print('hello')\n").unwrap();
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "-q", "-m", "Add the greeting"]);
    git(&repository, &["push", "-q", "-u", "origin", "main"]);
    git(&repository, &["switch", "-q", "-c", "greet"]);
    std::fs::write(repository.join("greet.py"), "print('hello you')\n").unwrap();
    git(&repository, &["commit", "-q", "-am", "Greet you"]);
    let path = repository.to_string_lossy().into_owned();
    assert_eq!(connection.request(&Request::AddProject { path }).await.unwrap(), Message::Ok);
    (projects_now(connection).await.remove(0), bin)
}

/// The text of the first message the thread's agent was given that has `wanted` in it.
async fn told(connection: &Connection, thread_id: &str, wanted: &str) -> String {
    let mut follow = open(connection, thread_id, 0).await;
    loop {
        let Message::Items { items } = next(&mut follow).await else { continue };
        for item in items {
            if let ItemKind::User { text, .. } = item.kind
                && text.contains(wanted)
            {
                return text;
            }
        }
    }
}

#[tokio::test]
async fn a_pull_request_is_listed_edited_reviewed_on_its_lines_and_reacted_to() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let (project, bin) = project_with_a_branch(&harness, &connection).await;
    let (_, end) = git_run(&connection, &project.id, run(GitAction::CreatePr)).await;
    assert_eq!(done(end).0, "Created PR #7");

    let list = Request::PullRequests { project_id: project.id.clone(), thread_id: None, state: PullRequestState::Open };
    let Message::PullRequests { pull_requests } = connection.request(&list).await.unwrap() else {
        panic!("expected the list")
    };
    let listed: Vec<(u64, &str, Option<CheckStatus>)> =
        pull_requests.iter().map(|found| (found.pull_request.number, found.head.as_str(), found.checks)).collect();
    assert_eq!(listed, [(7, "greet", Some(CheckStatus::Success))]);

    let (title, edited) =
        pull_request_edit(&connection, &project.id, PullRequestEdit::Title { title: "Greet whoever".into() })
            .await
            .unwrap();
    assert_eq!((title.as_str(), edited.pull_request.title.as_str()), ("Renamed PR #7", "Greet whoever"));
    let body = PullRequestEdit::Body { body: "Says hello **to you**.".into() };
    assert_eq!(pull_request_edit(&connection, &project.id, body).await.unwrap().1.body, "Says hello **to you**.");
    let labels = PullRequestEdit::Labels { add: vec!["bug".into()], remove: Vec::new() };
    let labelled = pull_request_edit(&connection, &project.id, labels).await.unwrap().1;
    assert_eq!(labelled.labels.iter().map(|label| label.name.as_str()).collect::<Vec<_>>(), ["bug"]);
    assert_eq!(labelled.repository_labels.len(), 3);
    let unknown = PullRequestEdit::Labels { add: vec!["nope".into()], remove: Vec::new() };
    assert!(pull_request_edit(&connection, &project.id, unknown).await.unwrap_err().contains("'nope' not found"));
    let reviewers = PullRequestEdit::Reviewers { add: vec!["ana".into()], remove: Vec::new() };
    let asked = pull_request_edit(&connection, &project.id, reviewers).await.unwrap().1;
    assert_eq!(
        asked.reviewers.iter().map(|reviewer| (reviewer.name.as_str(), reviewer.requested)).collect::<Vec<_>>(),
        [("ana", true)]
    );

    // A review with a comment on a line opens a conversation there, which is answered and resolved.
    let comment = LineComment { path: "greet.py".into(), line: 1, side: Side::Right, body: "Who is you?".into() };
    let review =
        PullRequestEdit::Review { verdict: ReviewVerdict::Comment, body: String::new(), comments: vec![comment] };
    let (title, reviewed) = pull_request_edit(&connection, &project.id, review).await.unwrap();
    assert_eq!(title, "Reviewed PR #7 with a comment");
    let thread = reviewed.threads[0].clone();
    assert_eq!((thread.path.as_str(), thread.line, thread.side), ("greet.py", Some(1), Side::Right));
    let reply = PullRequestEdit::Reply { thread: thread.id.clone(), body: "The reader".into() };
    let replied = pull_request_edit(&connection, &project.id, reply).await.unwrap().1;
    assert_eq!(replied.threads[0].comments.len(), 2);
    let resolve = PullRequestEdit::Resolve { thread: thread.id.clone(), resolved: true };
    assert!(pull_request_edit(&connection, &project.id, resolve).await.unwrap().1.threads[0].resolved);

    let subject = thread.comments[0].id.clone();
    let react = PullRequestEdit::React { subject: subject.clone(), reaction: ReactionKind::Heart, on: true };
    let (title, reacted) = pull_request_edit(&connection, &project.id, react).await.unwrap();
    assert!(title.is_empty());
    let reactions = &reacted.threads[0].comments[0].reactions;
    assert_eq!(
        reactions.iter().map(|reaction| (reaction.kind, reaction.count, reaction.mine)).collect::<Vec<_>>(),
        [(ReactionKind::Heart, 1, true)]
    );
    let unreact = PullRequestEdit::React { subject, reaction: ReactionKind::Heart, on: false };
    assert!(
        pull_request_edit(&connection, &project.id, unreact).await.unwrap().1.threads[0].comments[0]
            .reactions
            .is_empty()
    );

    let viewed = PullRequestEdit::Viewed { path: "greet.py".into(), viewed: true };
    let files = pull_request_edit(&connection, &project.id, viewed).await.unwrap().1.files;
    assert_eq!(files.iter().map(|file| (file.path.as_str(), file.viewed)).collect::<Vec<_>>(), [("greet.py", true)]);

    // A commit's changes are read by its whole name.
    let sha = reviewed
        .activity
        .iter()
        .find_map(|event| match &event.kind {
            EventKind::Commit { sha, .. } => Some(sha.clone()),
            _ => None,
        })
        .unwrap();
    let diff = Request::Diff { project_id: project.id.clone(), thread_id: None, scope: DiffScope::Commit { sha } };
    let Message::Diff { patch, .. } = connection.request(&diff).await.unwrap() else { panic!("expected the patch") };
    assert!(patch.contains("+print('hello you')"), "{patch}");
    let wrong = Request::Diff {
        project_id: project.id.clone(),
        thread_id: None,
        scope: DiffScope::Commit { sha: "x; rm".into() },
    };
    assert_eq!(connection.request(&wrong).await.unwrap(), Message::Error { message: "That isn't a commit.".into() });

    // A stack GitHub keeps it in is read with it.
    let file = bin.join("fake-gh.json");
    let mut state: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    state["stacks"] = json!([{"number": 2, "url": "api", "html_url": "https://github.com/acme/app/stacks/2", "base": {"ref": "main"},
        "pull_requests": [{"number": 7, "title": "Greet whoever", "head": {"ref": "greet"}, "state": "open", "merged_at": null}]}]);
    std::fs::write(&file, state.to_string()).unwrap();
    let read = Request::PullRequest { project_id: project.id.clone(), thread_id: None, number: 7 };
    let Message::PullRequest { pull_request } = connection.request(&read).await.unwrap() else { panic!("expected it") };
    assert_eq!(pull_request.stack.map(|stack| (stack.number, stack.layers.len())), Some((2, 1)));
}

#[tokio::test]
async fn a_linked_pull_request_is_watched_for_the_agent_until_it_merges_and_settles_the_thread() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let (project, bin) = project_with_a_branch(&harness, &connection).await;
    let (_, end) = git_run(&connection, &project.id, run(GitAction::CreatePr)).await;
    assert_eq!(done(end).0, "Created PR #7");
    let new_thread =
        NewThread { project_id: project.id.clone(), ..harness.new_thread(&connection, Agent::Claude).await.unwrap() };
    let thread_id = send(&connection, None, Some(new_thread), "Look around").await;
    finished_transcript(&connection, &thread_id).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;

    let settings = Request::SetPullRequestSettings { done_on_merge: Some(true), remove_merged_worktrees: None };
    assert_eq!(connection.request(&settings).await.unwrap(), Message::Ok);
    let refused = Request::WatchPullRequest { thread_id: thread_id.clone(), watch: true };
    assert!(matches!(connection.request(&refused).await.unwrap(), Message::Error { .. }));
    let link = Request::LinkPullRequest { thread_id: thread_id.clone(), number: Some(7) };
    assert_eq!(connection.request(&link).await.unwrap(), Message::Ok);
    let linked = thread_where(&mut list, |thread| thread.pull_request.is_some()).await;
    assert_eq!(linked.pull_request.map(|found| found.number), Some(7));
    let watch = Request::WatchPullRequest { thread_id: thread_id.clone(), watch: true };
    assert_eq!(connection.request(&watch).await.unwrap(), Message::Ok);
    assert!(thread_where(&mut list, |thread| thread.watching).await.watching);

    // Once the server has looked, a comment and checks that finish are told to the agent.
    change_pull_request(&bin, json!({"checks": [["build", "pending"]]}));
    tokio::time::sleep(Duration::from_millis(900)).await;
    change_pull_request(
        &bin,
        json!({"checks": [["build", "failure"]], "comments": [{"id": "IC_99", "author": {"login": "ana"},
            "body": "Please greet by name", "createdAt": "2026-10-05T04:00:00Z", "url": "u"}]}),
    );
    let text = told(&connection, &thread_id, "changed on GitHub").await;
    assert!(text.starts_with("PR #7 (https://github.com/acme/app/pull/7) changed on GitHub:"), "{text}");
    assert!(text.contains("- Its checks finished: 1 of 1 failed: CI / build."), "{text}");
    assert!(text.contains("- ana commented: \"Please greet by name\""), "{text}");
    finished_transcript(&connection, &thread_id).await;

    // Merged, the thread is done and no longer watches.
    let merge = Request::PullRequestAction {
        project_id: project.id.clone(),
        thread_id: None,
        number: 7,
        action: PullRequestAction::Merge,
        method: Some(MergeMethod::Squash),
        text: None,
    };
    assert!(matches!(connection.request(&merge).await.unwrap(), Message::PullRequestDone { .. }));
    let merging = thread_where(&mut list, |thread| thread.git_stage.is_some()).await;
    assert_eq!(merging.git_stage, Some(GitStage::Merge));
    let settled = thread_where(&mut list, |thread| thread.done_at.is_some()).await;
    assert!(!settled.watching && settled.pull_request.is_some_and(|found| found.merged));

    let unlink = Request::LinkPullRequest { thread_id: thread_id.clone(), number: None };
    assert_eq!(connection.request(&unlink).await.unwrap(), Message::Ok);
    assert!(thread_where(&mut list, |thread| thread.pull_request.is_none()).await.pull_request.is_none());

    let settings = Request::SetPullRequestSettings { done_on_merge: Some(false), remove_merged_worktrees: Some(true) };
    assert_eq!(connection.request(&settings).await.unwrap(), Message::Ok);
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { server, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
    assert!(!server.pull_request_settings.done_on_merge && server.pull_request_settings.remove_merged_worktrees);
}

#[tokio::test]
async fn a_worktree_asked_for_with_a_branch_is_made_on_it() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let (project, _) = project_with_a_branch(&harness, &connection).await;
    let new_thread = NewThread {
        project_id: project.id.clone(),
        agent: Agent::Claude,
        agent_account: None,
        model: None,
        effort: None,
        access: AgentAccess::Full,
        plan: false,
        worktree: Some(NewWorktree { base: "main".to_string(), branch: Some("ada/eng-7-greet-by-name".to_string()) }),
    };
    let thread_id = send(&connection, None, Some(new_thread), "Greet by name").await;
    finished_transcript(&connection, &thread_id).await;

    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { threads, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
    let worktree = &threads.iter().find(|thread| thread.id == thread_id).unwrap().cwd;
    let branch = std::process::Command::new("git").args(["-C", worktree, "branch", "--show-current"]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&branch.stdout).trim(), "ada/eng-7-greet-by-name");
}

#[tokio::test]
async fn a_worktree_whose_pull_request_merged_is_removed_when_nothing_in_it_is_lost() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let (project, _) = project_with_a_branch(&harness, &connection).await;
    let repository = harness.dir.path().join("repository");
    git(&repository, &["config", "user.name", "Test"]);
    git(&repository, &["config", "user.email", "test@example.com"]);
    git(&repository, &["switch", "-q", "main"]);
    let settings = Request::SetPullRequestSettings { done_on_merge: None, remove_merged_worktrees: Some(true) };
    assert_eq!(connection.request(&settings).await.unwrap(), Message::Ok);

    let new_thread = NewThread {
        project_id: project.id.clone(),
        agent: Agent::Claude,
        agent_account: None,
        model: None,
        effort: None,
        access: AgentAccess::Full,
        plan: false,
        worktree: Some(NewWorktree { base: "main".to_string(), branch: None }),
    };
    let thread_id = send(&connection, None, Some(new_thread), "Greet by name").await;
    finished_transcript(&connection, &thread_id).await;
    let mut announced = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut announced).await;
    let opened = GitRun { thread_id: Some(&thread_id), ..run(GitAction::CommitPushPr) };
    let (_, end) = git_run(&connection, &project.id, opened).await;
    assert!(done(end).0.starts_with("Created PR #"));
    // The thread says what its run is at to every client, and nothing once it is over.
    let mut stages = Vec::new();
    loop {
        match next_thread(&mut announced).await.git_stage {
            Some(stage) if stages.last() != Some(&stage) => stages.push(stage),
            None if !stages.is_empty() => break,
            _ => {}
        }
    }
    use GitStage::{Commit, PullRequest, PullRequestText, Push};
    assert_eq!(stages, [GitStage::Message, Commit, Push, PullRequestText, PullRequest]);
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { threads, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
    let worktree = PathBuf::from(&threads.iter().find(|thread| thread.id == thread_id).unwrap().cwd);
    assert!(worktree.is_dir());

    let merge = Request::PullRequestAction {
        project_id: project.id.clone(),
        thread_id: Some(thread_id.clone()),
        number: 7,
        action: PullRequestAction::Merge,
        method: Some(MergeMethod::Squash),
        text: None,
    };
    assert!(matches!(connection.request(&merge).await.unwrap(), Message::PullRequestDone { .. }));
    let merged = thread_where(&mut list, |thread| thread.pull_request.as_ref().is_some_and(|found| found.merged)).await;
    assert_eq!((merged.id, merged.done_at), (thread_id, None));
    for _ in 0..50 {
        if !worktree.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(!worktree.exists(), "the merged worktree is still there");
    // Its branch stays, for the thread to work on again.
    let branch = git_says(&repository, &["branch", "--list", "--format=%(refname:short)"]);
    assert!(branch.lines().count() > 2, "{branch}");
}

/// Changes what `scripts/fake-gh` in `bin` says of the first pull request it opened.
fn change_pull_request(bin: &Path, change: serde_json::Value) {
    let file = bin.join("fake-gh.json");
    let mut state: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    for (key, value) in change.as_object().unwrap() {
        state["pulls"][0][key] = value.clone();
    }
    std::fs::write(file, state.to_string()).unwrap();
}

fn git_says(folder: &Path, arguments: &[&str]) -> String {
    let output = std::process::Command::new("git").args(arguments).current_dir(folder).output().unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[tokio::test]
async fn where_a_new_worktree_starts_is_said_and_so_is_a_remote_that_cannot_be_fetched_from() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let (project, _) = project_with_a_branch(&harness, &connection).await;
    let root = harness.dir.path();
    let repository = root.join("repository");
    let start = async |fetch| {
        let request = Request::WorktreeStart { project_id: project.id.clone(), base: "main".to_string(), fetch };
        let Message::WorktreeStart { start, problem } = connection.request(&request).await.unwrap() else {
            panic!("expected where the worktree starts")
        };
        (start, problem)
    };
    assert_eq!(start(true).await, ("main".to_string(), None));
    let status = Request::GitStatus { project_id: project.id.clone(), thread_id: None, fetch: true };
    assert!(matches!(connection.request(&status).await.unwrap(), Message::GitStatus { problem: None, .. }));

    // A commit on the remote is only known once fetched, and pulling it makes the local base
    // the start again.
    git(root, &["clone", "-q", "origin.git", "other"]);
    std::fs::write(root.join("other/greet.py"), "print('hi')\n").unwrap();
    git(&root.join("other"), &["commit", "-q", "-am", "Say hi"]);
    git(&root.join("other"), &["push", "-q"]);
    assert_eq!(start(false).await, ("main".to_string(), None));
    assert_eq!(start(true).await, ("origin/main".to_string(), None));
    let update = Request::UpdateBase { project_id: project.id.clone(), base: "main".to_string() };
    let Message::WorktreeStart { start: updated, .. } = connection.request(&update).await.unwrap() else {
        panic!("expected where the worktree starts")
    };
    assert_eq!(updated, "main");
    assert_eq!(git_says(&repository, &["log", "-1", "--format=%s", "main"]), "Say hi");
    assert_eq!(start(false).await, ("main".to_string(), None));

    // A remote that is gone is said, and the thread shows it before it starts from what was
    // fetched last.
    std::fs::rename(root.join("origin.git"), root.join("gone.git")).unwrap();
    let (said, problem) = start(true).await;
    assert_eq!(said, "main");
    assert!(problem.is_some_and(|problem| problem.contains("origin.git")));
    let status = Request::GitStatus { project_id: project.id.clone(), thread_id: None, fetch: true };
    let Message::GitStatus { problem, .. } = connection.request(&status).await.unwrap() else {
        panic!("expected the status")
    };
    assert!(problem.is_some_and(|problem| problem.contains("origin.git")));
    let new_thread = NewThread {
        project_id: project.id.clone(),
        agent: Agent::Claude,
        agent_account: None,
        model: None,
        effort: None,
        access: AgentAccess::Full,
        plan: false,
        worktree: Some(NewWorktree { base: "main".to_string(), branch: None }),
    };
    let thread_id = send(&connection, None, Some(new_thread), "Show the screenshot").await;
    let transcript = finished_transcript(&connection, &thread_id).await;
    let ItemKind::Tool { call } = &transcript.items[1].kind else { panic!("the failed fetch is shown first") };
    assert_eq!(call.input, json!({ "command": "git fetch origin main" }).to_string());
    assert_eq!(call.status, ToolStatus::Failed);
}

#[tokio::test]
async fn a_thread_works_in_a_worktree_of_its_own_on_a_branch_named_for_it() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let root = harness.dir.path();
    git(root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
    git(root, &["clone", "-q", "origin.git", "repository"]);
    let repository = root.join("repository");
    git(&repository, &["config", "user.name", "Test"]);
    git(&repository, &["config", "user.email", "test@example.com"]);
    std::fs::write(repository.join("greet.py"), "print('hello')\n").unwrap();
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "-q", "-m", "Add the greeting"]);
    git(&repository, &["push", "-q", "-u", "origin", "main"]);
    // The remote is a commit ahead of the project's folder.
    git(root, &["clone", "-q", "origin.git", "other"]);
    std::fs::write(root.join("other/greet.py"), "print('hi')\n").unwrap();
    git(&root.join("other"), &["commit", "-q", "-am", "Say hi"]);
    git(&root.join("other"), &["push", "-q"]);
    let latest = git_says(&root.join("other"), &["rev-parse", "HEAD"]);

    let path = repository.to_string_lossy().into_owned();
    assert_eq!(connection.request(&Request::AddProject { path: path.clone() }).await.unwrap(), Message::Ok);
    let project = projects_now(&connection).await.remove(0);
    let script = "echo \"$MOTILE_PROJECT\" > setup.txt; echo ready".to_string();
    let setup = Request::SetProjectSetup { project_id: project.id.clone(), script: Some(script.clone()) };
    assert_eq!(connection.request(&setup).await.unwrap(), Message::Ok);
    let instructions = Some("Start it with team/ and say what the work is.".to_string());
    let named = Request::SetBranchInstructions { instructions: instructions.clone() };
    assert_eq!(connection.request(&named).await.unwrap(), Message::Ok);

    let new_thread = NewThread {
        project_id: project.id.clone(),
        agent: Agent::Claude,
        agent_account: None,
        model: None,
        effort: None,
        access: AgentAccess::Full,
        plan: false,
        worktree: Some(NewWorktree { base: "main".to_string(), branch: None }),
    };
    let thread_id = send(&connection, None, Some(new_thread), "Show the screenshot").await;
    let transcript = finished_transcript(&connection, &thread_id).await;

    // The agent worked in the worktree, which started from what the remote has, after the
    // project's setup script ran there. The project's folder is as it was.
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { server, threads, projects } = next(&mut list).await else {
        panic!("the list starts with a welcome")
    };
    let worktree = PathBuf::from(&threads[0].cwd);
    assert!(worktree.starts_with(root.join("worktrees/repository")), "{worktree:?}");
    assert!(worktree.join("screenshot.png").is_file() && !repository.join("screenshot.png").exists());
    assert_eq!(std::fs::read_to_string(worktree.join("greet.py")).unwrap(), "print('hi')\n");
    assert_eq!(std::fs::read_to_string(worktree.join("setup.txt")).unwrap().trim(), path);
    let ItemKind::Tool { call } = &transcript.items[1].kind else { panic!("the setup script is shown first") };
    assert_eq!(
        (call.input.as_str(), call.output.as_deref()),
        (json!({ "command": script }).to_string().as_str(), Some("ready"))
    );
    assert_eq!(call.status, ToolStatus::Succeeded);
    assert_eq!(git_says(&repository, &["status", "--porcelain", "--branch"]), "## main...origin/main [behind 1]");
    assert_eq!(server.branch_instructions.text, instructions.clone().unwrap());

    // The writer named the branch the way the instructions say.
    let branch = "team/greet-f-string";
    let mut worktrees = own(projects)[0].worktrees.clone();
    while worktrees[0].branch.as_deref() != Some(branch) {
        let Message::Projects { projects } = next(&mut list).await else { continue };
        worktrees = own(projects)[0].worktrees.clone();
    }
    assert_eq!((worktrees.len(), worktrees[0].path.as_str()), (1, threads[0].cwd.as_str()));
    assert_eq!(git_says(&worktree, &["rev-parse", "--abbrev-ref", "HEAD"]), branch);
    assert_eq!(
        git_says(&repository, &["rev-parse", "main", branch]),
        format!("{}\n{latest}", git_says(&repository, &["rev-parse", "main"]))
    );

    // Git works in the thread's worktree.
    let status =
        Request::GitStatus { project_id: project.id.clone(), thread_id: Some(thread_id.clone()), fetch: false };
    let Message::GitStatus { status: Some(status), files, .. } = connection.request(&status).await.unwrap() else {
        panic!("expected the status")
    };
    assert_eq!((status.branch.as_deref(), status.default, status.changed), (Some(branch), false, files.len() as u32));
    let commit = Request::GitRun {
        project_id: project.id.clone(),
        action: GitAction::Commit,
        thread_id: Some(thread_id.clone()),
        message: Some("Take a screenshot".to_string()),
        paths: Vec::new(),
        new_branch: false,
    };
    let mut follow = connection.follow(&commit).await.unwrap();
    assert_eq!(next(&mut follow).await, Message::GitProgress { stage: GitStage::Commit });
    assert!(matches!(next(&mut follow).await, Message::GitDone { .. }));
    assert_eq!(git_says(&repository, &["log", "-1", "--format=%s", branch]), "Take a screenshot");
    assert_eq!(git_says(&repository, &["log", "-1", "--format=%s", "main"]), "Add the greeting");

    // A worktree that has gone is made again on its branch for the next turn.
    std::fs::remove_dir_all(&worktree).unwrap();
    send(&connection, Some(thread_id.clone()), None, "What is in README.md?").await;
    finished_transcript(&connection, &thread_id).await;
    assert_eq!(git_says(&worktree, &["log", "-1", "--format=%s"]), "Take a screenshot");

    // Taking the instructions back names branches the server's own way again.
    let reset = Request::SetBranchInstructions { instructions: None };
    assert_eq!(connection.request(&reset).await.unwrap(), Message::Ok);
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { server, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
    assert_eq!(server.branch_instructions.text, server.branch_instructions.default);
    assert!(server.branch_instructions.default.contains("motile/"));

    // The worktree goes with its thread, and the branch stays.
    connection.request(&Request::Delete { thread_id }).await.unwrap();
    assert!(!worktree.exists());
    assert!(projects_now(&connection).await[0].worktrees.is_empty());
    assert_eq!(git_says(&repository, &["log", "-1", "--format=%s", branch]), "Take a screenshot");
}

/// The item that ends the thread's last turn and what that turn changed, once the server has read it.
async fn last_turns_changes(connection: &Connection, thread_id: &str) -> (String, TurnChanges) {
    let mut follow = open(connection, thread_id, 0).await;
    let mut transcript = Transcript::default();
    loop {
        transcript.apply(next(&mut follow).await);
        let Some(Item { id, kind: ItemKind::TurnEnd { summary }, .. }) = transcript.items.last() else { continue };
        if let Some(changes) = &summary.changes {
            return (id.clone(), changes.clone());
        }
    }
}

#[tokio::test]
async fn what_a_turn_changed_is_listed_with_the_turn_and_shown_as_a_diff() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let repository = PathBuf::from(harness.folder("repository"));
    git(&repository, &["init", "-q", "-b", "main"]);
    git(&repository, &["config", "user.name", "Test"]);
    git(&repository, &["config", "user.email", "test@example.com"]);
    std::fs::write(repository.join("greet.py"), "print('hello')\n").unwrap();
    std::fs::write(repository.join(".gitignore"), "build/\n").unwrap();
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "-q", "-m", "Add the greeting"]);
    // What the user left there before the turn isn't the turn's.
    std::fs::write(repository.join("mine.txt"), "mine\n").unwrap();
    std::fs::create_dir(repository.join("build")).unwrap();
    std::fs::write(repository.join("build/out.bin"), [0, 1, 2]).unwrap();

    let path = repository.to_string_lossy().into_owned();
    assert_eq!(connection.request(&Request::AddProject { path }).await.unwrap(), Message::Ok);
    let project = projects_now(&connection).await.remove(0);
    let new_thread = NewThread {
        project_id: project.id.clone(),
        agent: Agent::Claude,
        agent_account: None,
        model: None,
        effort: None,
        access: AgentAccess::Full,
        plan: false,
        worktree: None,
    };
    let thread_id = send(&connection, None, Some(new_thread), "Greet by name").await;
    let (turn, changes) = last_turns_changes(&connection, &thread_id).await;
    let files: Vec<_> =
        changes.files.iter().map(|file| (file.path.as_str(), file.change, file.added, file.removed)).collect();
    assert_eq!(files, [("docs/greeting.md", Change::Added, 3, 0), ("greet.py", Change::Modified, 5, 1)]);

    let diff = async |scope: DiffScope| {
        let request = Request::Diff { project_id: project.id.clone(), thread_id: Some(thread_id.clone()), scope };
        match connection.request(&request).await.unwrap() {
            Message::Diff { patch, truncated: false } => patch,
            other => panic!("expected a diff, got {other:?}"),
        }
    };
    let patch = diff(DiffScope::Turn { item_id: turn.clone() }).await;
    assert!(patch.contains("+++ b/docs/greeting.md") && patch.contains("-print('hello')"), "{patch}");
    assert!(patch.contains("+def greet(name):") && !patch.contains("mine.txt"), "{patch}");
    // What isn't committed is the turn's work and the user's, without what git ignores.
    for scope in [DiffScope::Uncommitted, DiffScope::Branch] {
        let patch = diff(scope).await;
        assert!(patch.contains("+++ b/mine.txt") && patch.contains("+++ b/greet.py"), "{patch}");
        assert!(patch.contains("+++ b/docs/greeting.md") && !patch.contains("out.bin"), "{patch}");
    }
    // Nothing of it was staged in the repository.
    assert_eq!(git_says(&repository, &["status", "--porcelain"]), "M greet.py\n?? docs/\n?? mine.txt");

    // A turn that changes nothing has no changes, whatever the user did before it.
    std::fs::write(repository.join("mine.txt"), "mine, changed\n").unwrap();
    send(&connection, Some(thread_id.clone()), None, "What is in README.md?").await;
    let transcript = finished_transcript(&connection, &thread_id).await;
    assert_eq!(transcript.turn_ends().len(), 2);
    send(&connection, Some(thread_id.clone()), None, "Show the screenshot").await;
    let (shot_turn, changes) = last_turns_changes(&connection, &thread_id).await;
    assert_eq!(changes.files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(), ["screenshot.png"]);
    let shot_patch = diff(DiffScope::Turn { item_id: shot_turn }).await;
    let blob =
        shot_patch.lines().find_map(|line| line.strip_prefix("index 0000000000000000000000000000000000000000.."));
    let shot_blob = blob.map(|blob| blob.split(' ').next().unwrap().to_string()).expect(&shot_patch);
    let transcript = finished_transcript(&connection, &thread_id).await;
    assert_eq!(transcript.turn_ends()[1].changes, None);
    // The first turn's diff is still what it was.
    assert_eq!(diff(DiffScope::Turn { item_id: turn }).await, patch);

    // The folder's files are listed and read, and nothing outside it.
    let list = async |path: &str| {
        let request = Request::ListFiles {
            project_id: project.id.clone(),
            thread_id: Some(thread_id.clone()),
            path: path.into(),
        };
        connection.request(&request).await.unwrap()
    };
    let Message::Files { entries } = list("").await else { panic!("expected the files") };
    let listed: Vec<_> = entries.iter().map(|entry| (entry.name.as_str(), entry.folder, entry.ignored)).collect();
    assert_eq!(
        listed,
        [
            ("build", true, true),
            ("docs", true, false),
            (".gitignore", false, false),
            ("greet.py", false, false),
            ("mine.txt", false, false),
            ("screenshot.png", false, false)
        ]
    );
    let Message::Files { entries } = list("docs").await else { panic!("expected the files") };
    assert_eq!(entries.iter().map(|entry| entry.name.as_str()).collect::<Vec<_>>(), ["greeting.md"]);
    let shown = harness.dir.path().join("shown.png");
    let read_blob = async |path: &str, blob: Option<&str>| {
        let request = Request::ReadFile {
            project_id: project.id.clone(),
            thread_id: Some(thread_id.clone()),
            path: path.into(),
            blob: blob.map(str::to_string),
        };
        connection.file(&request, &shown).await
    };
    let read = async |path: &str| read_blob(path, None).await;
    let (kind, size, bytes) = read("docs/greeting.md").await.unwrap();
    assert_eq!((kind, size as usize), (FileKind::Text, bytes.len()));
    assert!(String::from_utf8(bytes).unwrap().starts_with("# Greeting"));
    let (kind, size, _) = read("screenshot.png").await.unwrap();
    let screenshot = std::fs::read(repository.join("screenshot.png")).unwrap();
    assert_eq!(
        (kind, size as usize, std::fs::read(&shown).unwrap()),
        (FileKind::Image, screenshot.len(), screenshot.clone())
    );
    let (kind, _, bytes) = read("build/out.bin").await.unwrap();
    assert_eq!((kind, bytes.len()), (FileKind::Binary, 0));
    // An image a diff shows is read as git keeps it.
    std::fs::remove_file(&shown).unwrap();
    let (kind, _, _) = read_blob("screenshot.png", Some(&shot_blob)).await.unwrap();
    assert_eq!((kind, std::fs::read(&shown).unwrap()), (FileKind::Image, screenshot));
    assert!(read_blob("screenshot.png", Some("--output=x")).await.is_err());
    for refused in ["../motile.sqlite", ".git/config", "/etc/hosts", "docs"] {
        assert!(read(refused).await.is_err(), "{refused}");
        assert!(matches!(list(&format!("{refused}/..")).await, Message::Error { .. }), "{refused}");
    }

    // The snapshots go with their thread.
    assert!(git_says(&repository, &["for-each-ref", "refs/motile"]).contains(&thread_id));
    connection.request(&Request::Delete { thread_id }).await.unwrap();
    for _ in 0..50 {
        if git_says(&repository, &["for-each-ref", "refs/motile"]).is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(git_says(&repository, &["for-each-ref", "refs/motile"]), "");

    // A folder that is no repository has neither.
    let new_thread = harness.new_thread(&connection, Agent::Claude).await.unwrap();
    let project_id = new_thread.project_id.clone();
    let thread_id = send(&connection, None, Some(new_thread), "Greet by name").await;
    let transcript = finished_transcript(&connection, &thread_id).await;
    assert_eq!(transcript.turn_ends()[0].changes, None);
    let request = Request::Diff { project_id, thread_id: Some(thread_id), scope: DiffScope::Uncommitted };
    assert!(matches!(connection.request(&request).await.unwrap(), Message::Error { .. }));
}

async fn thread_now(connection: &Connection, thread_id: &str) -> Thread {
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { threads, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
    threads.into_iter().find(|thread| thread.id == thread_id).expect("the thread is listed")
}

async fn projects_now(connection: &Connection) -> Vec<Project> {
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { projects, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
    own(projects)
}

/// The projects the user added, without the server's "No project".
fn own(projects: Vec<Project>) -> Vec<Project> {
    projects.into_iter().filter(|project| !project.no_project).collect()
}

async fn icon_bytes(connection: &Connection, project_id: &str) -> Vec<u8> {
    use base64::Engine;
    let request = Request::ProjectIcon { project_id: project_id.to_string() };
    let Message::Icon { data } = connection.request(&request).await.unwrap() else { panic!("expected an icon") };
    base64::engine::general_purpose::STANDARD.decode(data).unwrap()
}

#[tokio::test]
async fn a_project_shows_the_icon_in_its_folder_until_another_is_chosen() {
    let mut harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let folder = harness.folder("project/public");
    std::fs::write(format!("{folder}/favicon.svg"), "<svg>found</svg>").unwrap();

    let project = harness.project(&connection).await;
    assert!(project.icon.as_deref().is_some_and(|icon| icon.ends_with(".svg")), "{:?}", project.icon);
    assert_eq!(icon_bytes(&connection, &project.id).await, b"<svg>found</svg>");

    // Another image on the server takes its place.
    let image = format!("{folder}/chosen.png");
    std::fs::write(&image, "png bytes").unwrap();
    let choose = Request::SetProjectIcon { project_id: project.id.clone(), path: Some(image) };
    assert_eq!(connection.request(&choose).await.unwrap(), Message::Ok);
    let chosen = projects_now(&connection).await.remove(0).icon.unwrap();
    assert!(chosen.ends_with(".png"), "{chosen}");
    assert_eq!(icon_bytes(&connection, &project.id).await, b"png bytes");

    let not_an_image = Request::SetProjectIcon { project_id: project.id.clone(), path: Some("/etc/hostname".into()) };
    assert!(matches!(connection.request(&not_an_image).await.unwrap(), Message::Error { .. }));

    // The choice is kept across a restart.
    harness.restart().await;
    let connection = harness.connect().await;
    assert_eq!(projects_now(&connection).await.remove(0).icon, Some(chosen));

    let reset = Request::SetProjectIcon { project_id: project.id.clone(), path: None };
    assert_eq!(connection.request(&reset).await.unwrap(), Message::Ok);
    assert_eq!(projects_now(&connection).await.remove(0).icon, project.icon);
}

#[tokio::test]
async fn a_project_without_an_icon_has_none_until_one_appears_in_its_folder() {
    let mut harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let project = harness.project(&connection).await;
    assert_eq!(project.icon, None);
    let request = Request::ProjectIcon { project_id: project.id.clone() };
    assert!(matches!(connection.request(&request).await.unwrap(), Message::Error { .. }));

    std::fs::write(format!("{}/icon.png", project.path), "png").unwrap();
    harness.restart().await;
    let connection = harness.connect().await;
    assert!(projects_now(&connection).await.remove(0).icon.is_some());
}

#[tokio::test]
async fn an_image_the_agent_shows_is_kept_as_it_was_and_goes_with_its_thread() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let thread_id = send(&connection, None, new_thread, "Show the screenshot").await;

    // The image arrives with the block that shows it, while the reply is still streaming.
    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &thread_id, 0).await;
    while transcript.items.iter().all(|item| item.media.is_empty()) {
        transcript.apply(next(&mut follow).await);
    }
    assert!(transcript.running);
    transcript.follow_until_idle(&mut follow).await;

    let screenshot = PathBuf::from(harness.folder("project")).join("screenshot.png");
    let shown = std::fs::read(&screenshot).unwrap();
    let reply = transcript.items.iter().find(|item| !item.media.is_empty()).unwrap();
    assert!(matches!(&reply.kind, ItemKind::Assistant { text } if text.ends_with("The header is in place.")));
    let [media] = &reply.media[..] else { panic!("the reply shows one image: {:?}", reply.media) };
    // On a Mac the temporary folder is reached through a link, which the agent's paths have resolved.
    assert_eq!(
        (std::fs::canonicalize(&media.src).unwrap(), media.video),
        (std::fs::canonicalize(&screenshot).unwrap(), false)
    );
    assert_eq!((media.width, media.height, media.size), (Some(960), Some(600), shown.len() as u64));

    // The agent's file changes; the thread still shows what it showed.
    std::fs::write(&screenshot, "something else").unwrap();
    let fetched = harness.dir.path().join("fetched.png");
    let mut progress = Vec::new();
    connection.media(&media.id, &fetched, |received, size| progress.push((received, size))).await.unwrap();
    assert_eq!(std::fs::read(&fetched).unwrap(), shown);
    assert_eq!(progress.last(), Some(&(media.size, media.size)));
    assert_eq!(finished_transcript(&connection, &thread_id).await.items, transcript.items);

    let nowhere = connection.media("../motile.sqlite", &fetched, |_, _| {}).await;
    assert!(nowhere.is_err());

    let kept = harness.dir.path().join("media").join(&media.id);
    assert!(kept.is_file());
    let delete = Request::Delete { thread_id: thread_id.clone() };
    assert_eq!(connection.request(&delete).await.unwrap(), Message::Ok);
    assert!(!kept.exists());
}

#[tokio::test]
async fn attached_images_and_videos_are_shown_in_the_message_and_go_with_its_thread() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let local = harness.dir.path().join("local");
    std::fs::create_dir_all(&local).unwrap();
    // The start of a PNG that is 640 by 400; enough to read its size from.
    let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR".to_vec();
    png.extend([0, 0, 2, 128, 0, 0, 1, 144, 8, 6, 0, 0, 0, 0, 0, 0, 0]);
    for (name, bytes) in [("shot.png", &png[..]), ("demo.mov", b"a video"), ("notes.txt", b"notes")] {
        std::fs::write(local.join(name), bytes).unwrap();
    }

    let mut progress = Vec::new();
    let (shot, shot_id) =
        connection.upload(&local.join("shot.png"), None, |sent, size| progress.push((sent, size))).await.unwrap();
    assert_eq!(progress.last(), Some(&(png.len() as u64, png.len() as u64)));
    let (video, video_id) = connection.upload(&local.join("demo.mov"), None, |_, _| {}).await.unwrap();
    let (poster, poster_id) = connection.upload(&local.join("shot.png"), Some(video.clone()), |_, _| {}).await.unwrap();
    let (notes, notes_id) = connection.upload(&local.join("notes.txt"), None, |_, _| {}).await.unwrap();
    let (unsent, _) = connection.upload(&local.join("notes.txt"), None, |_, _| {}).await.unwrap();
    assert_eq!(notes_id, None);

    let attachments = vec![shot.clone(), video.clone(), notes.clone()];
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let request = Request::Send { thread_id: None, new_thread, text: "Look".to_string(), attachments, now: false };
    let Message::Sent { thread_id } = connection.request(&request).await.unwrap() else { panic!("not sent") };
    let transcript = finished_transcript(&connection, &thread_id).await;

    // The client that sent the files named their contents the way the server did.
    let message = &transcript.items[0];
    let shown: Vec<_> =
        message.media.iter().map(|media| (media.src.as_str(), Some(&media.id), media.poster.as_ref())).collect();
    assert_eq!(
        shown,
        vec![(shot.as_str(), shot_id.as_ref(), None), (video.as_str(), video_id.as_ref(), poster_id.as_ref())]
    );
    assert_eq!((message.media[1].width, message.media[1].height), (Some(640), Some(400)));

    let gone = Request::Send {
        thread_id: None,
        new_thread: None,
        text: String::new(),
        attachments: vec![poster.clone() + "x"],
        now: false,
    };
    assert!(matches!(connection.request(&gone).await.unwrap(), Message::Error { .. }));

    let kept = harness.dir.path().join("media").join(shot_id.unwrap());
    assert!(kept.is_file());
    let delete = Request::Delete { thread_id: thread_id.clone() };
    assert_eq!(connection.request(&delete).await.unwrap(), Message::Ok);
    assert!(!kept.exists());
    for path in [&shot, &video, &poster, &notes] {
        assert!(!Path::new(path).exists(), "{path} went with its thread");
    }
    assert!(Path::new(&unsent).exists(), "a file that was never sent waits for its message");
}

async fn usage(connection: &Connection, agent: Agent, writing: bool) -> Vec<UsageBucket> {
    let request = Request::Usage { since: 0.0, until: f64::MAX, bucket_secs: 86400, utc_offset_secs: 3600 };
    let Message::Usage { buckets } = connection.request(&request).await.unwrap() else { panic!("no usage came") };
    buckets.into_iter().filter(|bucket| bucket.agent == agent && bucket.writing == writing).collect()
}

/// What the agent's turns spent.
async fn spent(connection: &Connection, agent: Agent) -> Vec<UsageBucket> {
    usage(connection, agent, false).await
}

/// What writing titles, branch names, commit messages and pull requests took, once some did.
async fn written(connection: &Connection, agent: Agent) -> Vec<UsageBucket> {
    for _ in 0..100 {
        let written = usage(connection, agent, true).await;
        if !written.is_empty() {
            return written;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("nothing was written")
}

#[tokio::test]
async fn the_agents_say_what_their_logins_have_used_of_their_plans() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let Message::Limits { agents } = connection.request(&Request::Limits { refresh: true }).await.unwrap() else {
        panic!("no limits came")
    };
    let [claude, codex] = &agents[..] else { panic!("both agents answered") };
    let windows = |limits: &AgentLimits| -> Vec<(String, f64, bool)> {
        limits.windows.iter().map(|window| (window.label.clone(), window.used_percent, window.warning)).collect()
    };
    assert_eq!(
        (claude.agent, claude.account.as_deref(), claude.plan.as_deref()),
        (Agent::Claude, Some("demo@motile.app"), Some("Max"))
    );
    let expected = [("Session", 34.0, false), ("Weekly", 61.0, false), ("Weekly · Fable", 82.0, true)];
    assert_eq!(windows(claude), expected.map(|(label, used, warning)| (label.to_string(), used, warning)));
    assert_eq!((codex.agent, codex.plan.as_deref(), codex.reset_credits), (Agent::Codex, Some("Pro"), 1));
    assert_eq!(windows(codex), [("Session".to_string(), 12.0, false), ("Weekly".to_string(), 48.0, false)]);
    assert!(codex.windows.iter().all(|window| window.resets_at.is_some_and(|at| at > motile_protocol::now())));
}

#[tokio::test]
async fn what_the_agents_spend_is_kept_by_model_and_counted_once() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await.unwrap();
    let project_id = new_thread.project_id.clone();
    let thread_id = send(&connection, None, Some(new_thread), "Read greet.py and run it").await;
    let first = finished_transcript(&connection, &thread_id).await;

    let [claude] = &spent(&connection, Agent::Claude).await[..] else { panic!("one model in one project spent") };
    assert_eq!((claude.model.as_str(), &claude.project_id), ("claude-haiku-4-5-20251001", &project_id));
    assert_eq!(claude.tokens, Tokens { input: 932, cache_read: 35573, cache_write: 8487, output: 261 });
    assert_eq!((claude.cost_usd, claude.costs), (Some(0.022768299999999995), None));
    assert_eq!(claude.start % 86400.0, 82800.0, "a day starts at the midnight of the client's clock");
    assert_eq!(first.turn_ends()[0].cost_usd, claude.cost_usd);

    // The session reports what it has spent since it began, which the replay never adds to.
    send(&connection, Some(thread_id.clone()), None, "Thanks").await;
    let second = finished_transcript(&connection, &thread_id).await;
    assert_eq!(second.turn_ends()[1].cost_usd, None);
    assert_eq!(spent(&connection, Agent::Claude).await, vec![claude.clone()]);

    let new_thread = NewThread { agent: Agent::Codex, ..harness.new_thread(&connection, Agent::Claude).await.unwrap() };
    let codex_thread = send(&connection, None, Some(new_thread), "Long reply with a lot of code").await;
    finished_transcript(&connection, &codex_thread).await;
    send(&connection, Some(codex_thread.clone()), None, "Long reply with a lot of code").await;
    finished_transcript(&connection, &codex_thread).await;

    let [codex] = &spent(&connection, Agent::Codex).await[..] else { panic!("one model in one project spent") };
    let twice = Tokens { input: 2 * 2512, cache_read: 2 * 12288, cache_write: 0, output: 2 * 240 };
    assert_eq!((codex.model.as_str(), codex.tokens, codex.cost_usd), ("gpt-fake", twice, None));

    // Each thread's title was written by its agent's lightest model, which is kept apart.
    let [title] = &written(&connection, Agent::Claude).await[..] else { panic!("one model wrote for Claude") };
    assert_eq!((title.model.as_str(), &title.project_id), ("claude-haiku-4-5", &project_id));
    assert_eq!(title.tokens, Tokens { input: 19, cache_read: 7279, cache_write: 0, output: 120 });
    assert_eq!(title.cost_usd, Some(0.0013));
    let [title] = &written(&connection, Agent::Codex).await[..] else { panic!("one model wrote for Codex") };
    assert_eq!(title.tokens, Tokens { input: 2211, cache_read: 12000, cache_write: 0, output: 19 });

    assert_eq!(connection.request(&Request::Delete { thread_id }).await.unwrap(), Message::Ok);
    assert_eq!(spent(&connection, Agent::Claude).await, vec![claude.clone()], "it was spent all the same");
}

/// Stands in for GitHub: it knows one pull request.
const FAKE_GH_SUBJECT: &str = r#"#!/bin/sh
case "$1 $2" in
"api repos/acme/app/issues/7") echo '{"title":"Stream Replies in Finished Blocks","body":"Passes a reply on in blocks."}' ;;
*) echo "gh: Not Found (HTTP 404)" >&2; exit 1 ;;
esac
"#;

#[tokio::test]
async fn a_thread_about_a_linked_pull_request_is_titled_after_what_that_is_about() {
    use std::os::unix::fs::PermissionsExt;
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let bin = harness.dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("gh"), FAKE_GH_SUBJECT).unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;

    let message = "Take over https://github.com/acme/app/pull/7 and https://github.com/acme/app/issues/9.";
    send(&connection, None, new_thread, message).await;

    thread_where(&mut list, |thread| thread.title == "Stream Replies in Finished Blocks").await;
}

#[tokio::test]
async fn a_thread_without_a_project_works_in_a_folder_of_its_own() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let in_git = std::process::Command::new("git").arg("init").arg("-q").arg(harness.dir.path()).status().unwrap();
    assert!(in_git.success());
    let connection = harness.connect().await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { projects, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
    let no_project = projects.into_iter().find(|project| project.no_project).expect("every server has No project");
    assert_eq!(no_project.name, "No project");

    let new_thread = NewThread {
        project_id: no_project.id.clone(),
        agent: Agent::Claude,
        agent_account: None,
        model: None,
        effort: None,
        access: AgentAccess::Supervised,
        plan: false,
        worktree: None,
    };
    let first_id = send(&connection, None, Some(new_thread.clone()), "Sketch a parser, for JSON").await;
    let first = thread_where(&mut list, |thread| thread.id == first_id).await;
    let second_id = send(&connection, None, Some(new_thread), "Sketch a parser, for JSON").await;
    let second = thread_where(&mut list, |thread| thread.id == second_id).await;
    assert_ne!(first.cwd, second.cwd);
    for thread in [&first, &second] {
        assert_eq!(Path::new(&thread.cwd).parent().unwrap(), Path::new(&no_project.path));
        assert!(Path::new(&thread.cwd).is_dir());
        let name = Path::new(&thread.cwd).file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.ends_with(&format!("-sketch-a-parser-for-json-{}", &thread.id[..8])), "{name}");
    }

    // The repository the server's folder happens to be in isn't the thread's.
    let status = Request::GitStatus { project_id: no_project.id.clone(), thread_id: Some(first_id), fetch: false };
    let Message::GitStatus { status, .. } = connection.request(&status).await.unwrap() else { panic!("a git status") };
    assert_eq!(status, None);

    let init = Request::InitRepository { project_id: no_project.id.clone() };
    assert!(matches!(connection.request(&init).await.unwrap(), Message::Error { .. }));
    assert!(!Path::new(&no_project.path).join(".git").exists());

    let remove = Request::RemoveProject { project_id: no_project.id };
    assert!(matches!(connection.request(&remove).await.unwrap(), Message::Error { .. }));
}
