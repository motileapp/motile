//! The host and an app's connection talking over real iroh connections on this machine, with
//! `scripts/fake-agent` standing in for Claude Code and Codex.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use motile_core::connection::{Connection, Follow, HostAddr, bind};
use motile_protocol::identity::DeviceKey;
use motile_protocol::wire::{
    Access as AgentAccess, Agent, Approval, Item, ItemKind, Message, NewThread, Project, Request, Thread, ThreadChange,
    ToolStatus, TurnSummary,
};
use motile_server::access::Access;
use motile_server::agents::environment::Environment;
use motile_server::hub::Hub;
use motile_server::serve::{BindOptions, Server};
use motile_server::store::Store;

const TIMEOUT: Duration = Duration::from_secs(30);

fn repo_file(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path).canonicalize().unwrap()
}

struct Harness {
    dir: tempfile::TempDir,
    host_key: DeviceKey,
    app_key: DeviceKey,
    address: HostAddr,
    endpoint: Option<iroh::Endpoint>,
    variables: HashMap<String, String>,
}

impl Harness {
    /// A host whose agents replay `fixture`, pausing `delay` seconds between lines.
    async fn start(fixture: &str, delay: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let arguments_file = dir.path().join("arguments.txt");
        let variables = HashMap::from([
            ("PATH".to_string(), std::env::var("PATH").unwrap_or_default()),
            ("HOME".to_string(), dir.path().to_string_lossy().into_owned()),
            ("FAKE_AGENT_FIXTURE".to_string(), repo_file(fixture).to_string_lossy().into_owned()),
            ("FAKE_AGENT_DELAY".to_string(), delay.to_string()),
            ("FAKE_AGENT_ARGUMENTS_FILE".to_string(), arguments_file.to_string_lossy().into_owned()),
        ]);
        let host_key = DeviceKey::generate();
        let app_key = DeviceKey::generate();
        let (endpoint, address) = serve(&dir, &host_key, &app_key, &variables, None).await;
        Self { dir, host_key, app_key, address, endpoint: Some(endpoint), variables }
    }

    /// Stops the host and starts it again on the same data and the same address.
    async fn restart(&mut self) {
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close().await;
        }
        let port = self.address.direct.map(|address| address.port());
        let (endpoint, address) = serve(&self.dir, &self.host_key, &self.app_key, &self.variables, port).await;
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

    /// A project to start threads in, added the way the app adds one.
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
            model: None,
            effort: None,
            access: AgentAccess::Supervised,
            plan: false,
        })
    }

    /// The arguments the agent got for each turn, one per line, leaving out the calls that only
    /// asked for a title.
    fn recorded_turns(&self) -> Vec<String> {
        let recorded = std::fs::read_to_string(self.dir.path().join("arguments.txt")).unwrap_or_default();
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
    host_key: &DeviceKey,
    app_key: &DeviceKey,
    variables: &HashMap<String, String>,
    port: Option<u16>,
) -> (iroh::Endpoint, HostAddr) {
    let fake_agent = repo_file("scripts/fake-agent");
    let executables = HashMap::from([(Agent::Claude, fake_agent.clone()), (Agent::Codex, fake_agent)]);
    let environment = Environment::fixed(variables.clone(), executables);
    let store = Store::open(&dir.path().join("motile.sqlite")).unwrap();
    let hub = Hub::new(store, dir.path().join("media"), environment).unwrap();

    // After a restart the old endpoint may take a moment to let go of the port.
    let options = BindOptions { local_only: true, port };
    let mut endpoint = motile_server::serve::bind(host_key, &options).await;
    for _ in 0..50 {
        if endpoint.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        endpoint = motile_server::serve::bind(host_key, &options).await;
    }
    let endpoint = endpoint.unwrap();
    let port = endpoint.bound_sockets().iter().find(|address| address.is_ipv4()).unwrap().port();
    let address = format!("{}@127.0.0.1:{port}", host_key.public()).parse().unwrap();

    let access = Access::new(vec![app_key.public()], None);
    let server = Server { hub, access, attachments: dir.path().join("attachments") };
    tokio::spawn(server.run(endpoint.clone()));
    (endpoint, address)
}

async fn next(follow: &mut Follow) -> Message {
    let message = tokio::time::timeout(TIMEOUT, follow.next()).await.expect("the host went quiet");
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
    let request = Request::Send { thread_id, new_thread, text: text.to_string(), attachments: Vec::new() };
    match connection.request(&request).await.unwrap() {
        Message::Sent { thread_id } => thread_id,
        other => panic!("unexpected answer to send: {other:?}"),
    }
}

async fn update(connection: &Connection, thread_id: &str, change: ThreadChange) -> Message {
    connection.request(&Request::Update { thread_id: thread_id.to_string(), change }).await.unwrap()
}

/// Applies a thread's updates the way the app does.
#[derive(Default)]
struct Transcript {
    items: Vec<Item>,
    running: bool,
    approvals: Vec<Approval>,
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
    let new_thread = harness.new_thread(&connection, Agent::Codex).await;
    let thread_id = send(&connection, None, new_thread, "Use an f-string in greet.py").await;

    let transcript = finished_transcript(&connection, &thread_id).await;

    assert_eq!(
        transcript.texts().last().unwrap(),
        &"Updated greet.py to use an f-string; running `python3 greet.py` outputs `Hello world`."
    );
    assert_eq!(
        transcript.tools(),
        vec![
            ("Bash", ToolStatus::Succeeded),
            ("Bash", ToolStatus::Succeeded),
            ("Edit", ToolStatus::Succeeded),
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

    // The next message resumes Codex's own session and reads the prompt from stdin.
    send(&connection, Some(thread_id.clone()), None, "Thanks").await;
    finished_transcript(&connection, &thread_id).await;
    let turns = harness.recorded_turns();
    assert!(turns[1].ends_with("resume\n01a0f557-4a9b-74a0-b330-f6c4d13b2880\n-"), "{}", turns[1]);
    assert!(
        turns[1].contains("-c\ndeveloper_instructions=\"You can show the user an image or a video"),
        "{}",
        turns[1]
    );
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

    // The fake agent writes its long reply 24 characters at a time. What reaches the app are
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
async fn an_app_that_reconnects_mid_turn_is_sent_only_what_it_missed() {
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

    // The turn keeps running without the app. Back again, it asks for what came after its revision.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let second = harness.connect().await;
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
async fn an_app_ahead_of_the_host_starts_over() {
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

#[tokio::test]
async fn the_thread_list_follows_new_retitled_and_deleted_threads() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { host, threads, .. } = next(&mut list).await else {
        panic!("the list starts with a welcome")
    };
    assert!(threads.is_empty());
    assert_eq!(host.agents.len(), 2);

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
async fn a_tool_call_that_needs_approval_waits_for_the_answer() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
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
async fn a_question_the_agent_asks_is_answered_by_the_user() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
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

#[tokio::test]
async fn a_message_sent_while_a_turn_runs_starts_the_next_turn() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0.05").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let thread_id = send(&connection, None, new_thread, "What does the note say?").await;
    send(&connection, Some(thread_id.clone()), None, "And then run it again").await;

    let mut transcript = Transcript::default();
    let mut follow = open(&connection, &thread_id, 0).await;
    while transcript.turn_ends().len() < 2 {
        transcript.apply(next(&mut follow).await);
    }

    assert_eq!(transcript.user_texts(), vec!["What does the note say?", "And then run it again"]);
    let turns = harness.recorded_turns();
    assert_eq!(turns.len(), 2);
    assert!(turns[1].contains("--resume\ne8c19686-947e-4f00-9129-eb7bde33809a"), "{}", turns[1]);
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
    assert!(matches!(refused, Message::Error { message } if message.contains("while it is monitoring")));

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
    assert!(matches!(refused, Message::Error { message } if message.contains("while it is working")));

    finished_transcript(&connection, &thread_id).await;
    assert_eq!(update(&connection, &thread_id, done).await, Message::Ok);
    let marked = thread_where(&mut list, |thread| thread.done_at.is_some()).await;
    assert_eq!(marked.undone_at, None);

    let undone = ThreadChange { done: Some(false), ..Default::default() };
    assert_eq!(update(&connection, &thread_id, undone).await, Message::Ok);
    let back = thread_where(&mut list, |thread| thread.done_at.is_none()).await;
    assert!(back.undone_at.is_some());

    let done = ThreadChange { done: Some(true), ..Default::default() };
    assert_eq!(update(&connection, &thread_id, done).await, Message::Ok);
    thread_where(&mut list, |thread| thread.done_at.is_some()).await;
    send(&connection, Some(thread_id.clone()), None, "One more thing").await;
    let reopened = thread_where(&mut list, |thread| thread.running).await;
    assert_eq!(reopened.done_at, None);
}

#[tokio::test]
async fn a_device_outside_the_account_is_refused() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let stranger = harness.connect_as(&DeviceKey::generate()).await;

    let closed = tokio::time::timeout(TIMEOUT, stranger.closed()).await.unwrap();
    assert!(closed.refused && closed.reason.contains("isn't linked"), "{}", closed.reason);
    assert!(stranger.request(&Request::ListDir { path: None, icons: false }).await.is_err());
}

#[tokio::test]
async fn the_model_effort_and_access_chosen_for_a_thread_reach_the_agent() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let project = harness.project(&connection).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { host, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
    let opus = host.models.iter().find(|model| model.id == "claude-opus-5-5").expect("Claude's models are offered");
    assert_eq!(opus.agent, Agent::Claude);
    assert!(opus.efforts.contains(&"xhigh".to_string()));

    let new_thread = NewThread {
        project_id: project.id,
        agent: Agent::Claude,
        model: Some(opus.id.clone()),
        effort: Some("xhigh".to_string()),
        access: AgentAccess::Full,
        plan: false,
    };
    let thread_id = send(&connection, None, Some(new_thread), "What is in README.md?").await;
    finished_transcript(&connection, &thread_id).await;
    let first = &harness.recorded_turns()[0];
    assert!(
        first.contains("--permission-mode\nbypassPermissions\n--model\nclaude-opus-5-5\n--effort\nxhigh"),
        "{first}"
    );
    assert!(first.contains("--append-system-prompt\nYou can show the user an image or a video"), "{first}");

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
async fn projects_are_added_and_removed_and_show_their_branch() {
    let harness = Harness::start("fixtures/read-and-bash.jsonl", "0").await;
    let connection = harness.connect().await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { projects, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
    assert!(projects.is_empty());

    let repository = harness.dir.path().join("repository");
    std::fs::create_dir_all(repository.join(".git")).unwrap();
    std::fs::write(repository.join(".git/HEAD"), "ref: refs/heads/feature/login\n").unwrap();
    let path = repository.to_string_lossy().into_owned();
    assert_eq!(connection.request(&Request::AddProject { path: format!("{path}/") }).await.unwrap(), Message::Ok);
    let Message::Projects { projects } = next(&mut list).await else { panic!("expected the projects") };
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
            connection.request(&Request::ListDir { path: here.clone(), icons }).await.unwrap()
        else {
            panic!("expected the folder's contents")
        };
        assert_eq!(folders, vec!["repository"]);
        assert_eq!(files, images);
    }

    let remove = Request::RemoveProject { project_id: projects[0].id.clone() };
    assert_eq!(connection.request(&remove).await.unwrap(), Message::Ok);
    assert_eq!(next(&mut list).await, Message::Projects { projects: Vec::new() });
}

async fn projects_now(connection: &Connection) -> Vec<Project> {
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    let Message::Welcome { projects, .. } = next(&mut list).await else { panic!("the list starts with a welcome") };
    projects
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

    // Another image on the host takes its place.
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
    assert_eq!((media.src.as_str(), media.video), (screenshot.to_str().unwrap(), false));
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
