//! The host and an app's connection talking over real iroh connections on this machine, with
//! `scripts/fake-agent` standing in for Claude Code and Codex.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use motile_core::connection::{Connection, Follow, HostAddr, bind};
use motile_protocol::identity::DeviceKey;
use motile_protocol::wire::{
    Access as AgentAccess, Agent, Item, ItemKind, Message, NewThread, Project, Request, Thread, ThreadChange,
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
    let hub = Hub::new(store, environment).unwrap();

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
    synced: Option<u64>,
    /// The latest revision seen once live.
    rev: u64,
    deltas: usize,
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
            }
            Message::Items { items } => {
                for item in items {
                    if self.synced.is_some() {
                        self.rev = self.rev.max(item.rev);
                    }
                    match self.items.iter().position(|existing| existing.id == item.id) {
                        Some(position) => self.items[position] = item,
                        None => self.items.push(item),
                    }
                }
                self.items.sort_by_key(|item| item.seq);
            }
            Message::Synced { rev } => {
                self.synced = Some(rev);
                self.rev = rev;
            }
            Message::TextDelta { id, text, rev } => {
                self.deltas += 1;
                self.rev = rev;
                let item = self.items.iter_mut().find(|item| item.id == id).expect("a delta for an unknown item");
                let ItemKind::Assistant { text: current } = &mut item.kind else {
                    panic!("a delta for a non-text item")
                };
                current.push_str(&text);
                item.rev = rev;
            }
            Message::Activity { activity } => self.running = activity.running,
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
    let summary = live.turn_ends().pop().expect("the turn ends with its summary");
    assert_eq!(summary.denials.len(), 1);
    assert_eq!(summary.denials[0].tool_name, "Bash");
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
}

#[tokio::test]
async fn an_app_that_reconnects_mid_turn_is_sent_only_what_it_missed() {
    let harness = Harness::start("fixtures/edit-and-run.jsonl", "0.02").await;
    let first = harness.connect().await;
    let new_thread = harness.new_thread(&first, Agent::Claude).await;
    let thread_id = send(&first, None, new_thread, "Use an f-string in greet.py").await;

    let mut transcript = Transcript::default();
    let mut follow = open(&first, &thread_id, 0).await;
    while transcript.deltas < 3 {
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
async fn allowing_a_denied_tool_resumes_the_session_with_that_tool_allowed() {
    let harness = Harness::start("fixtures/edit-and-run.jsonl", "0").await;
    let connection = harness.connect().await;
    let new_thread = harness.new_thread(&connection, Agent::Claude).await;
    let mut list = connection.follow(&Request::Subscribe).await.unwrap();
    next(&mut list).await;
    let thread_id = send(&connection, None, new_thread, "Use an f-string in greet.py").await;
    let transcript = finished_transcript(&connection, &thread_id).await;
    let denials = transcript.turn_ends().pop().unwrap().denials.clone();
    let waiting = thread_where(&mut list, |thread| !thread.running && thread.turn_ended_at.is_some()).await;
    assert!(waiting.needs_approval);

    let allow = Request::Allow { thread_id: thread_id.clone(), denials };
    assert_eq!(connection.request(&allow).await.unwrap(), Message::Ok);
    let after = finished_transcript(&connection, &thread_id).await;

    assert_eq!(after.user_texts().last().unwrap(), &"I've allowed Bash. Please continue.");
    let turns = harness.recorded_turns();
    assert!(turns[1].contains("--resume\n7827b0d8-4806-41c7-812e-540c46fcb36b"), "{}", turns[1]);
    assert!(turns[1].ends_with("--allowedTools\nBash(python3 greet.py)"), "{}", turns[1]);
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
    assert!(stranger.request(&Request::ListDir { path: None }).await.is_err());
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

    let Message::Dir { folders, .. } = connection
        .request(&Request::ListDir { path: Some(harness.dir.path().to_string_lossy().into_owned()) })
        .await
        .unwrap()
    else {
        panic!("expected the folder's contents")
    };
    assert_eq!(folders, vec!["repository"]);

    let remove = Request::RemoveProject { project_id: projects[0].id.clone() };
    assert_eq!(connection.request(&remove).await.unwrap(), Message::Ok);
    assert_eq!(next(&mut list).await, Message::Projects { projects: Vec::new() });
}
