//! Signs an account of an agent in for a client, with the account's folder. Claude Code's
//! `claude auth login` links to a page whose code comes back to it; Codex's
//! `codex login --device-auth` links to a page to enter its own code on, and finishes by itself.

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
/// How long finishing a Codex sign-in waits for the code to be entered on its page.
const ENTERED_WITHIN: Duration = Duration::from_secs(15);
/// How long a sign-in waits for its code.
const WAITS_FOR: Duration = Duration::from_secs(15 * 60);

#[derive(Default)]
pub struct SignIns {
    running: Mutex<HashMap<String, Running>>,
}

struct Running {
    agent: Agent,
    child: Child,
    stdin: ChildStdin,
    /// What the CLI says after the link, a line at a time.
    said: mpsc::UnboundedReceiver<String>,
    environment: Environment,
    /// The account's folder, as the account was when it started.
    folder: String,
}

/// The page a sign-in links to, and the code to enter there when the agent has one.
pub struct Started {
    pub id: String,
    pub url: String,
    pub code: Option<String>,
}

impl SignIns {
    /// Starts signing `agent` in to `folder` in `environment`, and answers with the page to sign
    /// in on.
    pub async fn start(
        self: &std::sync::Arc<Self>,
        agent: Agent,
        environment: Environment,
        folder: String,
    ) -> anyhow::Result<Started> {
        let name = crate::agent_accounts::agent_name(agent);
        let executable = environment.executable(agent).with_context(|| format!("{name} isn't installed."))?;
        let arguments = match agent {
            Agent::Claude => ["auth", "login"],
            Agent::Codex => ["login", "--device-auth"],
        };
        let mut child = Command::new(executable)
            .args(arguments)
            .current_dir(std::env::temp_dir())
            .env_clear()
            .envs(&environment.variables)
            .env("BROWSER", "true")
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let stdin = child.stdin.take().context("no stdin")?;
        let (lines, mut said) = mpsc::unbounded_channel();
        forward(child.stdout.take().context("no stdout")?, lines.clone());
        forward(child.stderr.take().context("no stderr")?, lines);
        let shown = tokio::time::timeout(LINK_WITHIN, async {
            let mut link = None;
            while let Some(line) = said.recv().await {
                let line = plain(&line);
                if let Some(found) = line.split_whitespace().find(|word| word.starts_with("https://")) {
                    link = Some(found.to_string());
                    if agent == Agent::Claude {
                        return link.map(|link| (link, None));
                    }
                }
                if let (Some(link), Some(code)) = (&link, device_code(&line)) {
                    return Some((link.clone(), Some(code)));
                }
            }
            None
        })
        .await
        .ok()
        .flatten()
        .with_context(|| format!("{name} didn't give a page to sign in on."))?;
        let (url, code) = shown;
        let id = uuid::Uuid::new_v4().to_string();
        let running = Running { agent, child, stdin, said, environment, folder };
        self.running.lock().await.insert(id.clone(), running);
        let sign_ins = self.clone();
        let waiting = id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(WAITS_FOR).await;
            sign_ins.cancel(&waiting).await;
        });
        Ok(Started { id, url, code })
    }

    /// Finishes the sign-in to `folder`: Claude Code's with the code its page showed, Codex's once
    /// its code was entered on its page. Answers with the agent and the environment it signed in,
    /// once the CLI kept the sign-in.
    pub async fn finish(&self, id: &str, code: &str, folder: &str) -> anyhow::Result<(Agent, Environment)> {
        let running = self.running.lock().await.remove(id);
        let mut running = running.context("That sign-in stopped. Sign in again.")?;
        if running.folder != folder {
            anyhow::bail!("The folder changed since the sign-in started. Sign in again.");
        }
        let status = match running.agent {
            Agent::Claude => {
                running.stdin.write_all(format!("{}\n", code.trim()).as_bytes()).await?;
                let waited = tokio::time::timeout(KEPT_WITHIN, running.child.wait()).await;
                waited.map_err(|_| anyhow::anyhow!("Claude Code didn't finish signing in. Sign in again."))??
            }
            Agent::Codex => match tokio::time::timeout(ENTERED_WITHIN, running.child.wait()).await {
                Ok(status) => status?,
                Err(_) => {
                    self.running.lock().await.insert(id.to_string(), running);
                    anyhow::bail!("Codex is still waiting. Enter the code on the page, then sign in.");
                }
            },
        };
        let Running { agent, mut said, environment, .. } = running;
        if status.success() {
            return Ok((agent, environment));
        }
        let mut output = Vec::new();
        while let Ok(line) = said.try_recv() {
            output.push(plain(&line));
        }
        let name = crate::agent_accounts::agent_name(agent);
        tracing::warn!("{name} didn't sign in: {}", output.join(" / "));
        match agent {
            Agent::Claude => anyhow::bail!("That code didn't work. Sign in again."),
            Agent::Codex => anyhow::bail!("Codex didn't sign in. Sign in again."),
        }
    }

    pub async fn cancel(&self, id: &str) {
        self.running.lock().await.remove(id);
    }
}

/// The line without the colours a terminal would show.
fn plain(line: &str) -> String {
    let mut plain = String::with_capacity(line.len());
    let mut characters = line.chars();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            plain.push(character);
            continue;
        }
        for following in characters.by_ref() {
            if following.is_ascii_alphabetic() {
                break;
            }
        }
    }
    plain
}

/// The one-time code Codex shows to enter on its page: letters and digits around a dash.
fn device_code(line: &str) -> Option<String> {
    let word = line.trim();
    let (before, after) = word.split_once('-')?;
    let part = |part: &str| (4..=6).contains(&part.len()) && part.chars().all(|c| c.is_ascii_alphanumeric());
    (part(before) && part(after) && !word.contains(' ')).then(|| word.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_s_code_is_found_in_what_it_prints() {
        let printed = [
            "Welcome to Codex [v\u{1b}[90m0.167.1\u{1b}[0m]",
            "   \u{1b}[94m1Z22-2JRJI\u{1b}[0m",
            "2. Enter this one-time code",
        ];
        let found: Vec<Option<String>> = printed.iter().map(|line| device_code(&plain(line))).collect();
        assert_eq!(found, [None, Some("1Z22-2JRJI".to_string()), None]);
        assert_eq!(
            plain("   \u{1b}[94mhttps://auth.openai.com/codex/device\u{1b}[0m").trim(),
            "https://auth.openai.com/codex/device"
        );
    }
}
