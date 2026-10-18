//! Signs an account of Claude Code in for a client: `claude auth login` runs with the account's
//! folder, the page it links to opens on the client, and the code that page shows comes back to it.

use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;

use anyhow::Context;
use motile_protocol::wire::Agent;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, mpsc};

use crate::agents::environment::Environment;

const LINK_WITHIN: Duration = Duration::from_secs(30);
const KEPT_WITHIN: Duration = Duration::from_secs(60);
/// How long a sign-in waits for its code.
const WAITS_FOR: Duration = Duration::from_secs(15 * 60);

#[derive(Default)]
pub struct SignIns {
    running: Mutex<HashMap<String, Running>>,
}

struct Running {
    child: Child,
    stdin: ChildStdin,
    /// What the CLI says after the link, a line at a time.
    said: mpsc::UnboundedReceiver<String>,
    environment: Environment,
    /// The account's folder, as the account was when it started.
    folder: String,
}

impl SignIns {
    /// Starts a sign-in to `folder` in `environment` and answers with its id and the page to sign
    /// in on.
    pub async fn start(
        self: &std::sync::Arc<Self>,
        environment: Environment,
        folder: String,
    ) -> anyhow::Result<(String, String)> {
        let executable = environment.executable(Agent::Claude).context("Claude Code isn't installed.")?;
        let mut child = Command::new(executable)
            .args(["auth", "login"])
            .current_dir(std::env::temp_dir())
            .env_clear()
            .envs(&environment.variables)
            .env("BROWSER", "true")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let stdin = child.stdin.take().context("no stdin")?;
        let (lines, mut said) = mpsc::unbounded_channel();
        forward(child.stdout.take().context("no stdout")?, lines.clone());
        forward(child.stderr.take().context("no stderr")?, lines);
        let link = tokio::time::timeout(LINK_WITHIN, async {
            while let Some(line) = said.recv().await {
                if let Some(link) = line.split_whitespace().find(|word| word.starts_with("https://")) {
                    return Some(link.to_string());
                }
            }
            None
        })
        .await
        .ok()
        .flatten()
        .context("Claude Code didn't give a page to sign in on.")?;
        let id = uuid::Uuid::new_v4().to_string();
        self.running.lock().await.insert(id.clone(), Running { child, stdin, said, environment, folder });
        let sign_ins = self.clone();
        let waiting = id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(WAITS_FOR).await;
            sign_ins.cancel(&waiting).await;
        });
        Ok((id, link))
    }

    /// Hands the code to the sign-in to `folder`, and answers with the environment it signed in,
    /// once the CLI kept the sign-in.
    pub async fn finish(&self, id: &str, code: &str, folder: &str) -> anyhow::Result<Environment> {
        let running = self.running.lock().await.remove(id);
        let Running { mut child, mut stdin, mut said, environment, folder: started } =
            running.context("That sign-in stopped. Sign in again.")?;
        if started != folder {
            anyhow::bail!("The folder changed since the sign-in started. Sign in again.");
        }
        stdin.write_all(format!("{}\n", code.trim()).as_bytes()).await?;
        drop(stdin);
        let status = tokio::time::timeout(KEPT_WITHIN, child.wait())
            .await
            .map_err(|_| anyhow::anyhow!("Claude Code didn't finish signing in. Sign in again."))??;
        if status.success() {
            return Ok(environment);
        }
        let mut output = Vec::new();
        while let Ok(line) = said.try_recv() {
            output.push(line);
        }
        tracing::warn!("Claude Code didn't sign in: {}", output.join(" / "));
        anyhow::bail!("That code didn't work. Sign in again.")
    }

    pub async fn cancel(&self, id: &str) {
        self.running.lock().await.remove(id);
    }
}

fn forward(output: impl AsyncRead + Unpin + Send + 'static, lines: mpsc::UnboundedSender<String>) {
    tokio::spawn(async move {
        let mut output = BufReader::new(output).lines();
        while let Ok(Some(line)) = output.next_line().await {
            if lines.send(line).is_err() {
                break;
            }
        }
    });
}
