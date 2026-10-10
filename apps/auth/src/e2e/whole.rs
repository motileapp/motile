//! The three programs together: a client's core signs in, the install command's token links a
//! server, the server accepts the client because the auth server says they share an account, and a
//! thread runs with `scripts/fake-agent` as its agent.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use motile_core::api::{AccountView, Command, Config, Event, ProjectView, ServerView, ThreadView};
use motile_core::core::{Handle, start};
use motile_core::link::State;
use motile_core::render::rows::{Row, RowKind};
use motile_protocol::identity::DeviceKey;
use motile_protocol::wire::{Access as AgentAccess, Agent, NewThread, Request, ToolStatus};
use motile_server::access::{Access, AccountSource};
use motile_server::agents::environment::Environment;
use motile_server::config::DataDir;
use motile_server::hub::Hub;
use motile_server::serve::{BindOptions, Server, bind};
use motile_server::setup;
use motile_server::store::Store;
use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::mpsc;

use super::Auth;

const TIMEOUT: Duration = Duration::from_secs(30);

fn repo_file(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path).canonicalize().unwrap()
}

/// A client: the core, and what the client would be showing from the events it has been sent.
struct Client {
    handle: Handle,
    events: mpsc::UnboundedReceiver<Event>,
    next_id: u64,
    account: AccountView,
    servers: Vec<ServerView>,
    threads: HashMap<String, ThreadView>,
    projects: Vec<ProjectView>,
    rows: Vec<Row>,
    running: bool,
    live: bool,
}

impl Client {
    fn start(data_dir: &Path, auth_url: &str, server_port: u16) -> Self {
        let config = Config {
            data_dir: data_dir.to_path_buf(),
            auth_url: auth_url.to_string(),
            device_name: "Ann's Mac".into(),
            platform: "macos".into(),
            local_only: true,
            direct_addr: Some(format!("127.0.0.1:{server_port}")),
            media_limit: None,
        };
        let (sender, events) = mpsc::unbounded_channel();
        let sink = Arc::new(move |event: Event| {
            let _ = sender.send(event);
        });
        Self {
            handle: start(config, sink).unwrap(),
            events,
            next_id: 0,
            account: AccountView::default(),
            servers: Vec::new(),
            threads: HashMap::new(),
            projects: Vec::new(),
            rows: Vec::new(),
            running: false,
            live: false,
        }
    }

    fn apply(&mut self, event: &Event) {
        match event.clone() {
            Event::Account { account } => self.account = account,
            Event::Servers { servers } => self.servers = servers,
            Event::Threads { server_id, threads } => {
                self.threads.retain(|_, thread| thread.server_id != server_id);
                self.threads.extend(threads.into_iter().map(|thread| (thread.thread.id.clone(), thread)));
            }
            Event::ThreadUpsert { thread } => {
                self.threads.insert(thread.thread.id.clone(), *thread);
            }
            Event::ThreadDeleted { thread_id } => {
                self.threads.remove(&thread_id);
            }
            Event::Projects { projects, .. } => {
                self.projects = projects.into_iter().filter(|view| !view.project.no_project).collect();
            }
            Event::Rows { reset, start, remove, rows, .. } => {
                if reset {
                    self.rows.clear();
                }
                self.rows.splice(start..start + remove, rows);
            }
            Event::Spans { row_id, spans, .. } => {
                let row = self.rows.iter_mut().find(|row| row.id == row_id);
                if let Some(Row { kind: RowKind::Code { spans: slot, .. }, .. }) = row {
                    *slot = Some(spans);
                }
            }
            Event::Activity { activity, .. } => self.running = activity.running,
            Event::Live { live, .. } => self.live = live,
            Event::ThreadError { message, .. } => panic!("a thread couldn't be opened: {message}"),
            Event::Restored
            | Event::Reply { .. }
            | Event::ServerUpdate { .. }
            | Event::GitProgress { .. }
            | Event::MediaProgress { .. }
            | Event::CodeSpans { .. }
            | Event::Agents { .. }
            | Event::AgentRows { .. }
            | Event::UploadProgress { .. } => {}
        }
    }

    /// Takes events until the client's state is as wanted.
    async fn until(&mut self, what: &str, wanted: impl Fn(&Client) -> bool) {
        while !wanted(self) {
            let event = tokio::time::timeout(TIMEOUT, self.events.recv()).await;
            let event = event.unwrap_or_else(|_| panic!("timed out waiting until {what}")).expect("the core stopped");
            self.apply(&event);
        }
    }

    /// Sends a command and returns its answer, applying the events that arrive before it.
    async fn ask(&mut self, command: Command) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.handle.send(id, command);
        loop {
            let event =
                tokio::time::timeout(TIMEOUT, self.events.recv()).await.expect("no answer").expect("the core stopped");
            self.apply(&event);
            match event {
                Event::Reply { id: answered, ok: true, value } if answered == id => return Ok(value),
                Event::Reply { id: answered, value, .. } if answered == id => {
                    return Err(value["error"].as_str().unwrap_or_default().to_string());
                }
                _ => {}
            }
        }
    }

    fn connected(&self) -> bool {
        self.servers.first().is_some_and(|server| server.state == State::Connected)
    }

    fn turn_ended(&self) -> bool {
        !self.running && matches!(self.rows.last(), Some(Row { kind: RowKind::TurnEnd { .. }, .. }))
    }
}

/// A server that has run the install command's `motile setup <token>` and is now serving.
async fn run_server(data: &DataDir, endpoint: iroh::Endpoint, key: DeviceKey) {
    let fake_agent = repo_file("scripts/fake-agent");
    let variables = HashMap::from([
        ("PATH".to_string(), std::env::var("PATH").unwrap_or_default()),
        ("HOME".to_string(), data.path().to_string_lossy().into_owned()),
    ]);
    let executables = HashMap::from([(Agent::Claude, fake_agent.clone()), (Agent::Codex, fake_agent)]);
    let environment = Environment::fixed(variables, executables);
    let store = Store::open(&data.database()).unwrap();
    let hub =
        Hub::new(store, data.media(), data.attachments(), data.worktrees(), data.no_project(), environment).unwrap();
    hub.refresh_models().await;

    let account = data.account().expect("setup linked the server");
    let access =
        Access::new(Vec::new(), Some(AccountSource { auth_url: account.auth_url, key, cache: data.account_keys() }));
    access.keep_fresh();
    let server = Server { hub, access, attachments: data.attachments() };
    tokio::spawn(server.run(endpoint));
}

fn kinds(client: &Client) -> Vec<&'static str> {
    let kinds = client.rows.iter().map(|row| match &row.kind {
        RowKind::User { .. } => "user",
        RowKind::Prose { .. } => "prose",
        RowKind::Code { .. } => "code",
        RowKind::Tool { .. } => "tool",
        RowKind::Thinking { .. } => "thinking",
        RowKind::Media { .. } => "media",
        RowKind::File { .. } => "file",
        RowKind::Group { .. } => "group",
        RowKind::Fold { .. } => "fold",
        RowKind::Error { .. } => "error",
        RowKind::Changes { .. } => "changes",
        RowKind::TurnEnd { .. } => "turn_end",
        RowKind::Queued { .. } => "queued",
        RowKind::Handoff { .. } => "handoff",
    });
    kinds.collect()
}

#[sqlx::test]
async fn a_client_signs_in_links_a_server_and_runs_a_thread_it_still_has_after_a_restart(db: PgPool) {
    let auth = Auth::start(db).await;
    let folder = tempfile::tempdir().unwrap();
    let server_data = DataDir::new(Some(folder.path().join("server"))).unwrap();
    let server_key = server_data.device_key().unwrap();
    let options = BindOptions { local_only: true, port: None };
    let server_endpoint = bind(&server_key, &options).await.unwrap();
    let server_port = server_endpoint.bound_sockets().iter().find(|address| address.is_ipv4()).unwrap().port();

    // Signed out at first, then signed in, with no servers yet.
    let client_data = folder.path().join("app");
    let mut client = Client::start(&client_data, &auth.base, server_port);
    client.until("the account is known", |client| !client.account.device_key.is_empty()).await;
    assert!(!client.account.signed_in);
    client.ask(Command::DevSignIn { email: "ann@example.com".into() }).await.unwrap();
    assert_eq!(client.account.user.as_ref().unwrap().email, "ann@example.com");
    assert!(client.servers.is_empty());

    // The install command's token links the server, and the client notices.
    client.ask(Command::WatchServers { on: true }).await.unwrap();
    let token = client.ask(Command::CreateEnrollToken).await.unwrap();
    assert!(token["command"].as_str().unwrap().ends_with(token["token"].as_str().unwrap()));
    let setup_options = setup::Options {
        token: Some(token["token"].as_str().unwrap().to_string()),
        auth_url: auth.base.clone(),
        name: Some("build-box".into()),
        assume_defaults: true,
        skip_service: true,
    };
    setup::setup(&server_data, setup_options).await.unwrap();
    run_server(&server_data, server_endpoint.clone(), server_data.device_key().unwrap()).await;

    client.until("the server is connected", Client::connected).await;
    let server = client.servers[0].clone();
    assert_eq!((server.name.as_str(), server.platform.as_str()), ("build-box", std::env::consts::OS));
    assert_eq!(server.id, server_key.public());
    assert!(server.info.as_ref().unwrap().models.iter().any(|model| model.agent == Agent::Claude));

    // A project, and a thread in it.
    let project_folder = folder.path().join("api");
    std::fs::create_dir_all(&project_folder).unwrap();
    std::fs::write(project_folder.join("favicon.svg"), "<svg>api</svg>").unwrap();
    let add = Request::AddProject { path: project_folder.to_string_lossy().into_owned() };
    client.ask(Command::Request { server_id: server.id.clone(), request: add }).await.unwrap();
    client.until("the project is listed", |client| client.projects.len() == 1).await;
    assert_eq!(client.projects[0].project.name, "api");

    // Its icon is fetched from the server into a file, and so is another image picked on the server.
    client.until("the project's icon arrives", |client| client.projects[0].icon_path.is_some()).await;
    assert_eq!(std::fs::read_to_string(client.projects[0].icon_path.as_ref().unwrap()).unwrap(), "<svg>api</svg>");
    let picked = folder.path().join("picked.png");
    std::fs::write(&picked, "picked").unwrap();
    let project_id = client.projects[0].project.id.clone();
    let pick = Command::SetProjectIcon {
        server_id: server.id.clone(),
        project_id,
        path: Some(picked.to_string_lossy().into_owned()),
    };
    client.ask(pick).await.unwrap();
    client
        .until("the picked icon arrives", |client| {
            client.projects[0].icon_path.as_ref().is_some_and(|path| path.ends_with(".png"))
        })
        .await;
    assert_eq!(std::fs::read_to_string(client.projects[0].icon_path.as_ref().unwrap()).unwrap(), "picked");

    let new_thread = NewThread {
        project_id: client.projects[0].project.id.clone(),
        agent: Agent::Claude,
        agent_account: None,
        model: None,
        effort: None,
        access: AgentAccess::Full,
        plan: false,
        worktree: None,
    };
    let send = Command::Send {
        server_id: server.id.clone(),
        thread_id: None,
        new_thread: Some(new_thread.clone()),
        text: "Add a rate limiter to the API".into(),
        attachments: Vec::new(),
        now: false,
    };
    let sent = client.ask(send).await.unwrap();
    let thread_id = sent["thread_id"].as_str().unwrap().to_string();
    client.ask(Command::OpenThread { server_id: server.id.clone(), thread_id: thread_id.clone() }).await.unwrap();
    client.ask(Command::MarkSeen { thread_id: thread_id.clone() }).await.unwrap();
    client.until("the turn has ended", Client::turn_ended).await;
    client.until("the thread has caught up with its server", |client| client.live).await;

    // The finished turn shows its last message; what led to it is behind the fold.
    assert_eq!(kinds(&client), ["user", "fold", "prose", "code", "prose", "code", "prose", "turn_end"]);
    for (open, len) in [("fold", 12), ("group", 14), ("group", 17)] {
        let closed = |row: &&Row| match &row.kind {
            RowKind::Fold { open, .. } | RowKind::Group { open, .. } => !open,
            _ => false,
        };
        let row_id = client.rows.iter().find(closed).unwrap().id.clone();
        client.ask(Command::ToggleRow { thread_id: thread_id.clone(), row_id }).await.unwrap();
        client.until(&format!("the {open} has opened"), |client| client.rows.len() == len).await;
    }
    assert_eq!(
        kinds(&client),
        [
            "user", "fold", "prose", "group", "tool", "tool", "prose", "group", "tool", "tool", "tool", "prose",
            "code", "prose", "code", "prose", "turn_end"
        ]
    );
    let tools: Vec<(&str, &str, ToolStatus)> = client
        .rows
        .iter()
        .filter_map(|row| match &row.kind {
            RowKind::Tool { tool } => Some((tool.verb.as_str(), tool.target.as_str(), tool.status)),
            _ => None,
        })
        .collect();
    assert_eq!(tools[1], ("Read", "/srv/api/src/middleware.rs", ToolStatus::Succeeded));
    assert_eq!(tools[4], ("Ran", "cargo test --quiet", ToolStatus::Succeeded));
    let highlighted = client
        .rows
        .iter()
        .filter(|row| matches!(&row.kind, RowKind::Code { spans: Some(spans), .. } if !spans.is_empty()));
    assert_eq!(highlighted.count(), 2, "code that streamed in arrives highlighted");

    client
        .until("the thread has its generated title", |client| {
            client.threads[&thread_id].thread.title == "Add API Rate Limiting"
        })
        .await;
    client.until("the thread is at rest", |client| !client.threads[&thread_id].thread.running).await;
    assert!(client.threads[&thread_id].thread.turn_ended_at.is_some());

    // Closed and opened again with the server gone: everything is there from the cache, folded as
    // a finished turn is.
    let fold = client.rows.iter().find(|row| matches!(row.kind, RowKind::Fold { .. })).unwrap().id.clone();
    client.ask(Command::ToggleRow { thread_id: thread_id.clone(), row_id: fold }).await.unwrap();
    client.until("the fold has closed", |client| client.rows.len() == 8).await;
    let before = client.rows.clone();

    // An image the agent shows has its size before the file is here. The file is fetched from
    // the server when it is asked for, and kept.
    let send = Command::Send {
        server_id: server.id.clone(),
        thread_id: None,
        new_thread: Some(new_thread.clone()),
        text: "Show the screenshot".into(),
        attachments: Vec::new(),
        now: false,
    };
    let showing = client.ask(send).await.unwrap()["thread_id"].as_str().unwrap().to_string();
    client.ask(Command::OpenThread { server_id: server.id.clone(), thread_id: showing }).await.unwrap();
    client.until("the image is shown", |client| client.turn_ended() && kinds(client).contains(&"media")).await;
    assert_eq!(kinds(&client), ["user", "fold", "prose", "media", "prose", "turn_end"]);
    let RowKind::Media { media, width, height, size, alt, name, .. } = client.rows[3].kind.clone() else {
        panic!("the reply shows an image")
    };
    assert_eq!((width, height), (Some(960), Some(600)));
    assert_eq!((alt.as_str(), name.as_str()), ("The landing page", "screenshot.png"));
    let screenshot = std::fs::read(project_folder.join("screenshot.png")).unwrap();
    let find = || Command::Media { server_id: server.id.clone(), media_id: media.clone() };

    // A fetch that is stopped answers with an error and leaves nothing behind.
    let (fetch, cancel) = (client.next_id + 1, client.next_id + 2);
    client.next_id = cancel;
    client.handle.send(fetch, find());
    client.handle.send(cancel, Command::CancelMedia { media_id: media.clone() });
    let mut answers = HashMap::new();
    while answers.len() < 2 {
        let event =
            tokio::time::timeout(TIMEOUT, client.events.recv()).await.expect("no answer").expect("the core stopped");
        client.apply(&event);
        if let Event::Reply { id, ok, value } = event {
            answers.insert(id, (ok, value["error"].as_str().map(str::to_string)));
        }
    }
    assert_eq!(answers[&fetch], (false, Some("The download was stopped.".to_string())));
    assert_eq!(answers[&cancel], (true, None));
    let left = std::fs::read_dir(client_data.join("media")).map(|entries| entries.count()).unwrap_or(0);
    assert_eq!(left, 0);

    let fetched = client.ask(find()).await.unwrap()["path"].as_str().unwrap().to_string();
    assert!(Path::new(&fetched).starts_with(&client_data));
    assert_eq!(std::fs::read(&fetched).unwrap(), screenshot);
    let storage = client.ask(Command::Storage).await.unwrap();
    assert_eq!((storage["media_bytes"].as_u64(), storage["media_limit"].as_u64()), (Some(size), Some(2_000_000_000)));

    // A thread that is never opened here is followed while its agent works.
    let send = Command::Send {
        server_id: server.id.clone(),
        thread_id: None,
        new_thread: Some(new_thread),
        text: "Add a rate limiter to the API".into(),
        attachments: Vec::new(),
        now: false,
    };
    let unopened = client.ask(send).await.unwrap()["thread_id"].as_str().unwrap().to_string();
    client
        .until("the unopened thread is at rest with its title", |client| {
            let thread = client.threads.get(&unopened).map(|view| &view.thread);
            thread.is_some_and(|thread| !thread.running && thread.title == "Add API Rate Limiting")
        })
        .await;

    client.handle.stop();
    server_endpoint.close().await;
    let mut reopened = Client::start(&client_data, &auth.base, server_port);
    reopened.until("the cached thread is listed", |client| client.threads.contains_key(&thread_id)).await;
    assert!(reopened.account.signed_in);
    assert_eq!(reopened.servers[0].name, "build-box");
    assert_eq!(reopened.projects.len(), 1);
    assert!(reopened.projects[0].icon_path.is_some(), "the icon is shown from the cache, without the server");
    reopened.ask(Command::OpenThread { server_id: server.id.clone(), thread_id: thread_id.clone() }).await.unwrap();
    reopened.until("the cached rows are shown", |client| client.rows.len() == before.len()).await;
    assert!(!reopened.live, "rows from the cache are not news");
    reopened
        .ask(Command::Highlight {
            thread_id: thread_id.clone(),
            row_ids: before.iter().map(|row| row.id.clone()).collect(),
        })
        .await
        .unwrap();
    reopened.until("the code is highlighted again", |client| client.rows == before).await;
    reopened.ask(Command::OpenThread { server_id: server.id.clone(), thread_id: unopened }).await.unwrap();
    assert_eq!(kinds(&reopened), ["user", "fold", "prose", "code", "prose", "code", "prose", "turn_end"]);

    // The image is here without the server, until the copies on this device are cleared.
    assert_eq!(reopened.ask(find()).await.unwrap()["path"], fetched.as_str());
    reopened.ask(Command::ClearMedia).await.unwrap();
    assert_eq!(reopened.ask(Command::Storage).await.unwrap()["media_bytes"], 0);
    assert!(reopened.ask(find()).await.is_err());

    // Signing out makes the device a stranger and empties the client.
    let old_key = reopened.account.device_key.clone();
    reopened.ask(Command::SignOut).await.unwrap();
    reopened.until("the client is signed out", |client| !client.account.signed_in && client.threads.is_empty()).await;
    assert_ne!(reopened.account.device_key, old_key);
    assert!(reopened.servers.is_empty());
}
