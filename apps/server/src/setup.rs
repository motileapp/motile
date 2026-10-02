//! What the one-line installer runs after downloading the binary: makes sure there is an agent to
//! drive, links the server to the account behind the token, and installs it as a service.

use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::process::Command;

use anyhow::{Context, bail};
use motile_protocol::auth_client::{AuthClient, DeviceDescription};
use motile_protocol::wire::Agent;

use crate::agents::environment::Environment;
use crate::config::{Account, DataDir};
use crate::service;

const CLAUDE_INSTALL: &str = "curl -fsSL https://claude.ai/install.sh | bash";
const CODEX_INSTALL: &str = "curl -fsSL https://chatgpt.com/codex/install.sh | sh";

pub struct Options {
    pub token: Option<String>,
    pub auth_url: String,
    pub name: Option<String>,
    /// Don't ask anything; install nothing that wasn't asked for.
    pub assume_defaults: bool,
    pub skip_service: bool,
}

pub async fn setup(data_dir: &DataDir, options: Options) -> anyhow::Result<()> {
    ensure_agents(options.assume_defaults).await?;
    enroll(data_dir, &options).await?;
    if options.skip_service {
        println!("Skipped the service. Start the server with: motile run");
        return Ok(());
    }
    if service::install(data_dir.path())? {
        println!("\nMotile is running on this machine. It shows up in the app in a moment.");
    }
    Ok(())
}

async fn ensure_agents(assume_defaults: bool) -> anyhow::Result<()> {
    let environment = Environment::resolve().await;
    let installed: Vec<String> = environment
        .agents()
        .into_iter()
        .filter_map(|info| info.version.map(|version| format!("{} {version}", agent_name(info.agent))))
        .collect();
    if !installed.is_empty() {
        println!("Found {}.", installed.join(" and "));
        return Ok(());
    }

    println!("Neither Claude Code nor Codex is installed on this machine. Motile needs at least one.");
    if assume_defaults {
        println!("Install one, then sign in to it:\n  {CLAUDE_INSTALL}\n  {CODEX_INSTALL}");
        return Ok(());
    }
    let choice = ask("Install [1] Claude Code, [2] Codex, [3] both, or [s]kip? [1] ")?;
    let agents: &[Agent] = match choice.trim().to_lowercase().as_str() {
        "" | "1" => &[Agent::Claude],
        "2" => &[Agent::Codex],
        "3" => &[Agent::Claude, Agent::Codex],
        _ => &[],
    };
    for agent in agents {
        install_agent(*agent)?;
    }
    if agents.is_empty() {
        println!("Skipped. Threads will fail until an agent is installed.");
    }
    Ok(())
}

fn install_agent(agent: Agent) -> anyhow::Result<()> {
    let (script, sign_in) = match agent {
        Agent::Claude => (CLAUDE_INSTALL, "claude"),
        Agent::Codex => (CODEX_INSTALL, "codex login"),
    };
    println!("\nInstalling {}…", agent_name(agent));
    let status = Command::new("sh").args(["-c", script]).status().context("sh isn't available.")?;
    if !status.success() {
        bail!("{} couldn't be installed. Install it by hand and run the command again.", agent_name(agent));
    }
    println!("{} is installed. Sign in to it by running `{sign_in}` once.", agent_name(agent));
    Ok(())
}

/// Reads an answer from the terminal even when stdin is the piped installer script.
fn ask(question: &str) -> anyhow::Result<String> {
    print!("{question}");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    if std::io::stdin().is_terminal() {
        std::io::stdin().read_line(&mut answer)?;
        return Ok(answer);
    }
    let Ok(terminal) = std::fs::File::open("/dev/tty") else {
        println!();
        return Ok("s".to_string());
    };
    BufReader::new(terminal).read_line(&mut answer)?;
    Ok(answer)
}

async fn enroll(data_dir: &DataDir, options: &Options) -> anyhow::Result<()> {
    let Some(token) = &options.token else {
        return match data_dir.account() {
            Some(account) => {
                println!("Already linked to {}.", account.email);
                Ok(())
            }
            None => bail!("This server isn't linked to an account yet. Copy the install command from the Motile app."),
        };
    };
    let key = data_dir.device_key()?;
    let auth = AuthClient::new(&options.auth_url);
    let name = options.name.clone().unwrap_or_else(Environment::hostname);
    let device = DeviceDescription { name: &name, platform: std::env::consts::OS };
    let enrolled = auth.enroll(&key, token, &device).await?;
    data_dir.save_account(&Account { auth_url: auth.base_url().to_string(), email: enrolled.email.clone() })?;
    println!("Linked this machine to {} as \"{name}\".", enrolled.email);
    Ok(())
}

fn agent_name(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "Claude Code",
        Agent::Codex => "Codex",
    }
}
