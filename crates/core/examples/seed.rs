//! Signs a data folder in with the dev login and fills its account's server with a project and a
//! few finished threads. `apps/macos/scripts/dev-app.sh` runs it once; the app it then opens on
//! the same data folder is signed in.
//!
//!     cargo run -p motile-core --example seed -- <data dir> <auth url> <project folder>

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use motile_core::api::{Command, Config, Event};
use motile_core::core::Handle;
use motile_core::link::State;
use motile_protocol::wire::{Access, Agent, NewThread, Request};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

const EMAIL: &str = "demo@motile.app";
const PROMPTS: [&str; 3] = [
    "Add a rate limiter to the API",
    "Show the screenshot of the new dashboard",
    "Change greet.py to use an f-string, then run greet.py.",
];

struct Seed {
    handle: Handle,
    events: UnboundedReceiver<Event>,
    next_id: u64,
}

impl Seed {
    /// The first event the closure makes something of.
    async fn wait<T>(&mut self, what: &str, mut pick: impl FnMut(&Event) -> Option<T>) -> anyhow::Result<T> {
        let found = async {
            loop {
                let event = self.events.recv().await.context("The core stopped.")?;
                if let Some(value) = pick(&event) {
                    return anyhow::Ok(value);
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(60), found)
            .await
            .with_context(|| format!("Timed out waiting for {what}."))?
    }

    async fn command(&mut self, command: Command) -> anyhow::Result<serde_json::Value> {
        self.next_id += 1;
        let sent = self.next_id;
        self.handle.send(sent, command);
        let (ok, value) = self
            .wait("an answer", |event| match event {
                Event::Reply { id, ok, value } if *id == sent => Some((*ok, value.clone())),
                _ => None,
            })
            .await?;
        if !ok {
            bail!("{value}");
        }
        Ok(value)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [data_dir, auth_url, folder] = arguments.as_slice() else {
        bail!("usage: seed <data dir> <auth url> <project folder>");
    };
    let config = Config {
        data_dir: data_dir.into(),
        auth_url: auth_url.clone(),
        device_name: "Dev Mac".to_string(),
        platform: std::env::consts::OS.to_string(),
        local_only: std::env::var("MOTILE_LOCAL").is_ok(),
        direct_addr: std::env::var("MOTILE_SERVER_ADDR").ok(),
    };
    let (sender, events) = unbounded_channel();
    let handle = motile_core::core::start(
        config,
        Arc::new(move |event| {
            let _ = sender.send(event);
        }),
    )?;
    let mut seed = Seed { handle, events, next_id: 0 };

    seed.command(Command::DevSignIn { email: EMAIL.to_string() }).await?;
    let server_id = seed
        .wait("the server to connect", |event| match event {
            Event::Servers { servers } => {
                servers.iter().find(|server| server.state == State::Connected).map(|server| server.id.clone())
            }
            _ => None,
        })
        .await?;

    let request = Request::AddProject { path: folder.clone() };
    seed.command(Command::Request { server_id: server_id.clone(), request }).await?;
    let project_id = seed
        .wait("the project", |event| match event {
            Event::Projects { projects, .. } => projects
                .iter()
                .find(|project| &project.project.path == folder)
                .map(|project| project.project.id.clone()),
            _ => None,
        })
        .await?;

    for prompt in PROMPTS {
        let new_thread = NewThread {
            project_id: project_id.clone(),
            agent: Agent::Claude,
            model: None,
            effort: None,
            access: Access::Full,
            plan: false,
            worktree: None,
        };
        let sent = seed
            .command(Command::Send {
                server_id: server_id.clone(),
                thread_id: None,
                new_thread: Some(new_thread),
                text: prompt.to_string(),
                files: Vec::new(),
                attachments: Vec::new(),
            })
            .await?;
        let thread_id = sent["thread_id"].as_str().unwrap_or_default().to_string();
        seed.wait("the turn to end", |event| match event {
            Event::ThreadUpsert { thread } if thread.thread.id == thread_id => {
                (!thread.thread.running && thread.thread.turn_ended_at.is_some()).then_some(())
            }
            _ => None,
        })
        .await?;
        println!("seeded: {prompt}");
    }

    seed.handle.stop();
    tokio::time::sleep(Duration::from_millis(300)).await;
    Ok(())
}
