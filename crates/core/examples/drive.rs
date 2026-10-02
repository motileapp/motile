//! Drives the core from a terminal, the way an app does: one JSON command per line on stdin,
//! one JSON event per line on stdout.
//!
//!     cargo run -p motile-core --example drive -- /tmp/motile-app http://localhost:3000
//!     {"id": 1, "type": "dev_sign_in", "email": "you@example.com"}
//!     {"id": 2, "type": "create_enroll_token"}
//!     {"id": 3, "type": "request", "server_id": "…", "request": {"type": "branches", "project_id": "…"}}
//!     {"id": 4, "type": "request", "server_id": "…", "request": {"type": "switch_branch", "project_id": "…", "branch": "main"}}

use std::io::BufRead;
use std::sync::Arc;

use motile_core::api::{Config, Envelope, Event};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut arguments = std::env::args().skip(1);
    let data_dir = arguments.next().unwrap_or_else(|| "/tmp/motile-app".to_string());
    let auth_url = arguments.next().unwrap_or_else(|| motile_protocol::DEFAULT_AUTH_URL.to_string());
    let config = Config {
        data_dir: data_dir.into(),
        auth_url,
        device_name: "Terminal".to_string(),
        platform: std::env::consts::OS.to_string(),
        local_only: std::env::var("MOTILE_LOCAL").is_ok(),
        direct_addr: std::env::var("MOTILE_SERVER_ADDR").ok(),
    };
    let sink = Arc::new(|event: Event| println!("{}", serde_json::to_string(&event).unwrap_or_default()));
    let handle = motile_core::core::start(config, sink)?;

    let lines = tokio::task::spawn_blocking(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            match serde_json::from_str::<Envelope>(&line) {
                Ok(envelope) => handle.send(envelope.id, envelope.command),
                Err(error) => eprintln!("not a command: {error}"),
            }
        }
        handle.stop();
    });
    lines.await?;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    Ok(())
}
