//! What the one-line installer runs after downloading the binary: makes sure there is an agent to
//! drive, links the host to the account behind the token, and runs it as a systemd service.

use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
use motile_protocol::auth_client::{AuthClient, DeviceDescription};
use motile_protocol::wire::Agent;

use crate::agents::environment::Environment;
use crate::config::{Account, DataDir};

const UNIT_NAME: &str = "motile.service";
const UNIT_FILE: &str = "/etc/systemd/system/motile.service";
const BINARY: &str = "/usr/local/bin/motile";
/// Claude Code refuses full access as root unless it is told the machine is a sandbox.
pub const SANDBOX_VARIABLE: &str = "IS_SANDBOX";

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
        println!("Skipped the service. Start the host with: motile run");
        return Ok(());
    }
    install_service(data_dir)?;
    println!("\nMotile is running on this machine. It shows up in the app in a moment.");
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
            None => bail!("This host isn't linked to an account yet. Copy the install command from the Motile app."),
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

fn is_root() -> bool {
    unsafe { libc::getuid() == 0 }
}

fn user_name() -> String {
    std::env::var("USER").or_else(|_| std::env::var("LOGNAME")).unwrap_or_else(|_| "root".to_string())
}

fn has_systemd() -> bool {
    Path::new("/run/systemd/system").is_dir()
}

/// The service is a system unit so it survives reboots and logouts, but it runs as the user who
/// installed it, with their home folder, where the agents keep their sign-in.
pub fn unit(binary: &Path, data_dir: &Path, user: &str) -> String {
    let sandbox = if user == "root" { format!("Environment={SANDBOX_VARIABLE}=1\n") } else { String::new() };
    format!(
        "[Unit]\n\
         Description=Motile host\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         User={user}\n\
         {sandbox}\
         ExecStart={binary} run --data-dir {data_dir}\n\
         Restart=always\n\
         RestartSec=2\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        binary = binary.display(),
        data_dir = data_dir.display(),
    )
}

fn install_service(data_dir: &DataDir) -> anyhow::Result<()> {
    if !has_systemd() {
        println!("systemd isn't running here, so there is no service. Start the host with: motile run");
        return Ok(());
    }
    if is_root() {
        return write_service(&std::env::current_exe()?, data_dir.path(), &user_name());
    }
    println!("Installing the service needs administrator rights.");
    let status = Command::new("sudo")
        .arg(std::env::current_exe()?)
        .args(["service", "install", "--user", &user_name(), "--data-dir"])
        .arg(data_dir.path())
        .status()
        .context("sudo isn't available. Run the install command as root.")?;
    if !status.success() {
        bail!("The service couldn't be installed.");
    }
    Ok(())
}

/// Copies this binary to a stable place and starts the unit. Needs root.
pub fn write_service(current_binary: &Path, data_dir: &Path, user: &str) -> anyhow::Result<()> {
    if !is_root() {
        bail!("Installing the service needs root.");
    }
    let binary = install_binary(current_binary)?;
    std::fs::write(UNIT_FILE, unit(&binary, data_dir, user))?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", UNIT_NAME])?;
    systemctl(&["restart", UNIT_NAME])?;
    println!("Installed {} and started {UNIT_NAME}.", binary.display());
    Ok(())
}

fn install_binary(current: &Path) -> anyhow::Result<PathBuf> {
    let target = PathBuf::from(BINARY);
    if current == target {
        return Ok(target);
    }
    // A running binary can't be overwritten, but it can be replaced.
    let staged = target.with_file_name(".motile.new");
    std::fs::copy(current, &staged).with_context(|| format!("{} can't be written.", staged.display()))?;
    std::fs::rename(&staged, &target)?;
    Ok(target)
}

fn systemctl(arguments: &[&str]) -> anyhow::Result<()> {
    let status = Command::new("systemctl").args(arguments).status().context("systemctl isn't available.")?;
    if !status.success() {
        bail!("systemctl {} failed.", arguments.join(" "));
    }
    Ok(())
}

pub fn uninstall() -> anyhow::Result<()> {
    if !is_root() {
        bail!("Removing the service needs root. Run: sudo motile uninstall");
    }
    let _ = systemctl(&["disable", "--now", UNIT_NAME]);
    let _ = std::fs::remove_file(UNIT_FILE);
    systemctl(&["daemon-reload"])?;
    println!("Removed {UNIT_NAME}. Threads and the device key are still in the data folder.");
    Ok(())
}

pub fn service_state() -> Option<String> {
    if !has_systemd() {
        return None;
    }
    let output = Command::new("systemctl").args(["is-active", UNIT_NAME]).output().ok()?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn agent_name(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "Claude Code",
        Agent::Codex => "Codex",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_service_tells_claude_code_it_is_sandboxed() {
        let root = unit(Path::new("/usr/local/bin/motile"), Path::new("/root/.local/share/motile"), "root");
        assert!(root.contains("User=root\nEnvironment=IS_SANDBOX=1\n"));
        assert!(root.contains("ExecStart=/usr/local/bin/motile run --data-dir /root/.local/share/motile\n"));

        let user = unit(Path::new("/usr/local/bin/motile"), Path::new("/home/ann/.local/share/motile"), "ann");
        assert!(user.contains("User=ann\n"));
        assert!(!user.contains(SANDBOX_VARIABLE));
    }
}
