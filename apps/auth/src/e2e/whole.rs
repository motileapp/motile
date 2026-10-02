//! The three programs together: an app's core signs in, the install command's token links a
//! host, the host accepts the app because the auth server says they share an account, and a
//! thread runs with `scripts/fake-agent` as its agent.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use motile_core::api::{AccountView, Command, Config, Event, HostView, ProjectView, ThreadView};
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

/// An app: the core, and what the app would be showing from the events it has been sent.
struct App {
    handle: Handle,
    events: mpsc::UnboundedReceiver<Event>,
    next_id: u64,
    account: AccountView,
    hosts: Vec<HostView>,
    threads: HashMap<String, ThreadView>,
    projects: Vec<ProjectView>,
    rows: Vec<Row>,
    running: bool,
}

impl App {
    fn start(data_dir: &Path, auth_url: &str, host_port: u16) -> Self {
        let config = Config {
            data_dir: data_dir.to_path_buf(),
            auth_url: auth_url.to_string(),
            device_name: "Ann's Mac".into(),
            platform: "macos".into(),
            local_only: true,
            direct_addr: Some(format!("127.0.0.1:{host_port}")),
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
            hosts: Vec::new(),
            threads: HashMap::new(),
            projects: Vec::new(),
            rows: Vec::new(),
            running: false,
        }
    }

    fn apply(&mut self, event: &Event) {
        match event.clone() {
            Event::Account { account } => self.account = account,
            Event::Hosts { hosts } => self.hosts = hosts,
            Event::Threads { host_id, threads } => {
                self.threads.retain(|_, thread| thread.host_id != host_id);
                self.threads.extend(threads.into_iter().map(|thread| (thread.thread.id.clone(), thread)));
            }
            Event::ThreadUpsert { thread } => {
                self.threads.insert(thread.thread.id.clone(), thread);
            }
            Event::ThreadDeleted { thread_id } => {
                self.threads.remove(&thread_id);
            }
            Event::Projects { projects, .. } => self.projects = projects,
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
            Event::ThreadError { message, .. } => panic!("a thread couldn't be opened: {message}"),
            Event::Reply { .. } | Event::HostUpdate { .. } => {}
        }
    }

    /// Takes events until the app's state is as wanted.
    async fn until(&mut self, what: &str, wanted: impl Fn(&App) -> bool) {
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
        self.hosts.first().is_some_and(|host| host.state == State::Connected)
    }

    fn turn_ended(&self) -> bool {
        !self.running && matches!(self.rows.last(), Some(Row { kind: RowKind::TurnEnd { .. }, .. }))
    }
}

/// A host that has run the install command's `motile setup <token>` and is now serving.
async fn run_host(data: &DataDir, endpoint: iroh::Endpoint, key: DeviceKey) {
    let fake_agent = repo_file("scripts/fake-agent");
    let variables = HashMap::from([
        ("PATH".to_string(), std::env::var("PATH").unwrap_or_default()),
        ("HOME".to_string(), data.path().to_string_lossy().into_owned()),
    ]);
    let executables = HashMap::from([(Agent::Claude, fake_agent.clone()), (Agent::Codex, fake_agent)]);
    let environment = Environment::fixed(variables, executables);
    let hub = Hub::new(Store::open(&data.database()).unwrap(), environment).unwrap();

    let account = data.account().expect("setup linked the host");
    let access =
        Access::new(Vec::new(), Some(AccountSource { auth_url: account.auth_url, key, cache: data.account_keys() }));
    access.keep_fresh();
    let server = Server { hub, access, attachments: data.attachments() };
    tokio::spawn(server.run(endpoint));
}

#[sqlx::test]
async fn an_app_signs_in_links_a_host_and_runs_a_thread_it_still_has_after_a_restart(db: PgPool) {
    let auth = Auth::start(db).await;
    let folder = tempfile::tempdir().unwrap();
    let host_data = DataDir::new(Some(folder.path().join("host"))).unwrap();
    let host_key = host_data.device_key().unwrap();
    let options = BindOptions { local_only: true, port: None };
    let host_endpoint = bind(&host_key, &options).await.unwrap();
    let host_port = host_endpoint.bound_sockets().iter().find(|address| address.is_ipv4()).unwrap().port();

    // Signed out at first, then signed in, with no hosts yet.
    let app_data = folder.path().join("app");
    let mut app = App::start(&app_data, &auth.base, host_port);
    app.until("the account is known", |app| !app.account.device_key.is_empty()).await;
    assert!(!app.account.signed_in);
    app.ask(Command::DevSignIn { email: "ann@example.com".into() }).await.unwrap();
    assert_eq!(app.account.user.as_ref().unwrap().email, "ann@example.com");
    assert!(app.hosts.is_empty());

    // The install command's token links the host, and the app notices.
    app.ask(Command::WatchHosts { on: true }).await.unwrap();
    let token = app.ask(Command::CreateEnrollToken).await.unwrap();
    assert!(token["command"].as_str().unwrap().ends_with(token["token"].as_str().unwrap()));
    let setup_options = setup::Options {
        token: Some(token["token"].as_str().unwrap().to_string()),
        auth_url: auth.base.clone(),
        name: Some("build-box".into()),
        assume_defaults: true,
        skip_service: true,
    };
    setup::setup(&host_data, setup_options).await.unwrap();
    run_host(&host_data, host_endpoint.clone(), host_data.device_key().unwrap()).await;

    app.until("the host is connected", App::connected).await;
    let host = app.hosts[0].clone();
    assert_eq!((host.name.as_str(), host.platform.as_str()), ("build-box", std::env::consts::OS));
    assert_eq!(host.id, host_key.public());
    assert!(host.info.as_ref().unwrap().models.iter().any(|model| model.agent == Agent::Claude));

    // A project, and a thread in it.
    let project_folder = folder.path().join("api");
    std::fs::create_dir_all(&project_folder).unwrap();
    std::fs::write(project_folder.join("favicon.svg"), "<svg>api</svg>").unwrap();
    let add = Request::AddProject { path: project_folder.to_string_lossy().into_owned() };
    app.ask(Command::Request { host_id: host.id.clone(), request: add }).await.unwrap();
    app.until("the project is listed", |app| app.projects.len() == 1).await;
    assert_eq!(app.projects[0].project.name, "api");

    // Its icon is fetched from the host into a file, and so is another image picked on the host.
    app.until("the project's icon arrives", |app| app.projects[0].icon_path.is_some()).await;
    assert_eq!(std::fs::read_to_string(app.projects[0].icon_path.as_ref().unwrap()).unwrap(), "<svg>api</svg>");
    let picked = folder.path().join("picked.png");
    std::fs::write(&picked, "picked").unwrap();
    let project_id = app.projects[0].project.id.clone();
    let pick = Command::SetProjectIcon {
        host_id: host.id.clone(),
        project_id,
        path: Some(picked.to_string_lossy().into_owned()),
    };
    app.ask(pick).await.unwrap();
    app.until("the picked icon arrives", |app| {
        app.projects[0].icon_path.as_ref().is_some_and(|path| path.ends_with(".png"))
    })
    .await;
    assert_eq!(std::fs::read_to_string(app.projects[0].icon_path.as_ref().unwrap()).unwrap(), "picked");

    let new_thread = NewThread {
        project_id: app.projects[0].project.id.clone(),
        agent: Agent::Claude,
        model: None,
        effort: None,
        access: AgentAccess::Full,
        plan: false,
    };
    let send = Command::Send {
        host_id: host.id.clone(),
        thread_id: None,
        new_thread: Some(new_thread),
        text: "Add a rate limiter to the API".into(),
        files: Vec::new(),
    };
    let sent = app.ask(send).await.unwrap();
    let thread_id = sent["thread_id"].as_str().unwrap().to_string();
    app.ask(Command::OpenThread { host_id: host.id.clone(), thread_id: thread_id.clone() }).await.unwrap();
    app.ask(Command::MarkSeen { thread_id: thread_id.clone() }).await.unwrap();
    app.until("the turn has ended", App::turn_ended).await;

    let kinds: Vec<&str> = app
        .rows
        .iter()
        .map(|row| match &row.kind {
            RowKind::User { .. } => "user",
            RowKind::Prose { .. } => "prose",
            RowKind::Code { .. } => "code",
            RowKind::Tool { .. } => "tool",
            RowKind::Thinking { .. } => "thinking",
            RowKind::Error { .. } => "error",
            RowKind::TurnEnd { .. } => "turn_end",
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            "user", "prose", "tool", "tool", "prose", "tool", "tool", "tool", "prose", "code", "prose", "code",
            "prose", "turn_end"
        ]
    );
    let tools: Vec<(&str, &str, ToolStatus)> = app
        .rows
        .iter()
        .filter_map(|row| match &row.kind {
            RowKind::Tool { tool } => Some((tool.verb.as_str(), tool.target.as_str(), tool.status)),
            _ => None,
        })
        .collect();
    assert_eq!(tools[1], ("Read", "/srv/api/src/middleware.rs", ToolStatus::Succeeded));
    assert_eq!(tools[4], ("Ran", "cargo test --quiet", ToolStatus::Succeeded));
    let highlighted = app
        .rows
        .iter()
        .filter(|row| matches!(&row.kind, RowKind::Code { spans: Some(spans), .. } if !spans.is_empty()));
    assert_eq!(highlighted.count(), 2, "code that streamed in arrives highlighted");

    app.until("the thread has its generated title", |app| {
        app.threads[&thread_id].thread.title == "Add API Rate Limiting"
    })
    .await;
    app.until("the thread is at rest", |app| !app.threads[&thread_id].thread.running).await;
    assert!(app.threads[&thread_id].thread.turn_ended_at.is_some());

    // Closed and opened again with the host gone: everything is there from the cache.
    let before = app.rows.clone();
    app.handle.stop();
    host_endpoint.close().await;
    let mut reopened = App::start(&app_data, &auth.base, host_port);
    reopened.until("the cached thread is listed", |app| app.threads.contains_key(&thread_id)).await;
    assert!(reopened.account.signed_in);
    assert_eq!(reopened.hosts[0].name, "build-box");
    assert_eq!(reopened.projects.len(), 1);
    assert!(reopened.projects[0].icon_path.is_some(), "the icon is shown from the cache, without the host");
    reopened.ask(Command::OpenThread { host_id: host.id.clone(), thread_id: thread_id.clone() }).await.unwrap();
    reopened.until("the cached rows are shown", |app| app.rows.len() == before.len()).await;
    reopened
        .ask(Command::Highlight {
            thread_id: thread_id.clone(),
            row_ids: before.iter().map(|row| row.id.clone()).collect(),
        })
        .await
        .unwrap();
    reopened.until("the code is highlighted again", |app| app.rows == before).await;

    // Signing out makes the device a stranger and empties the app.
    let old_key = reopened.account.device_key.clone();
    reopened.ask(Command::SignOut).await.unwrap();
    reopened.until("the app is signed out", |app| !app.account.signed_in && app.threads.is_empty()).await;
    assert_ne!(reopened.account.device_key, old_key);
    assert!(reopened.hosts.is_empty());
}
