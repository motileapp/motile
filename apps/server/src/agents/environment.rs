//! Where the agent CLIs are and the environment to run them in. A service starts with a bare
//! `PATH`, so the user's login shell is asked for its environment.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use motile_protocol::wire::{Agent, AgentInfo};
use tokio::process::Command;

use super::executable_name;

#[derive(Clone)]
pub struct Environment {
    pub variables: HashMap<String, String>,
    executables: HashMap<Agent, PathBuf>,
    versions: HashMap<Agent, String>,
    /// A stand-in for GitHub's `gh` from `MOTILE_GH_PATH`, for the dev apps.
    gh: Option<PathBuf>,
}

impl Environment {
    pub async fn resolve() -> Self {
        let variables = shell_environment().await;
        let mut executables = HashMap::new();
        let mut versions = HashMap::new();
        for agent in [Agent::Claude, Agent::Codex] {
            let Some(path) = locate(agent, &variables) else { continue };
            let Some(version) = version(&path, &variables).await else { continue };
            executables.insert(agent, path);
            versions.insert(agent, version);
        }
        let gh = std::env::var_os("MOTILE_GH_PATH").map(PathBuf::from);
        Self { variables, executables, versions, gh }
    }

    /// Skips the lookup, for tests that stand in for the agents.
    pub fn fixed(variables: HashMap<String, String>, executables: HashMap<Agent, PathBuf>) -> Self {
        let versions = executables.keys().map(|agent| (*agent, "test".to_string())).collect();
        Self { variables, executables, versions, gh: None }
    }

    /// The same, with these variables set besides.
    pub fn with(&self, variables: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut environment = self.clone();
        environment.variables.extend(variables);
        environment
    }

    /// The program to run for `name`: the stand-in for `gh` when there is one.
    pub fn program(&self, name: &str) -> PathBuf {
        match &self.gh {
            Some(gh) if name == "gh" => gh.clone(),
            _ => PathBuf::from(name),
        }
    }

    pub fn executable(&self, agent: Agent) -> Option<&Path> {
        self.executables.get(&agent).map(PathBuf::as_path)
    }

    pub fn hostname() -> String {
        let mut buffer = [0u8; 256];
        let result = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
        if result != 0 {
            return "server".to_string();
        }
        let length = buffer.iter().position(|byte| *byte == 0).unwrap_or(buffer.len());
        String::from_utf8_lossy(&buffer[..length]).into_owned()
    }

    pub fn agents(&self) -> Vec<AgentInfo> {
        [Agent::Claude, Agent::Codex]
            .into_iter()
            .map(|agent| AgentInfo { agent, version: self.versions.get(&agent).cloned() })
            .collect()
    }
}

fn override_variable(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "MOTILE_CLAUDE_PATH",
        Agent::Codex => "MOTILE_CODEX_PATH",
    }
}

fn locate(agent: Agent, variables: &HashMap<String, String>) -> Option<PathBuf> {
    if let Ok(path) = std::env::var(override_variable(agent)) {
        return is_executable(Path::new(&path)).then(|| PathBuf::from(path));
    }
    let name = executable_name(agent);
    let home = variables.get("HOME").cloned().unwrap_or_default();
    let on_path = variables.get("PATH").map(String::as_str).unwrap_or_default().split(':').map(PathBuf::from);
    let usual = [format!("{home}/.local/bin"), "/usr/local/bin".to_string(), format!("{home}/.bun/bin")];
    on_path.chain(usual.into_iter().map(PathBuf::from)).map(|folder| folder.join(name)).find(|path| is_executable(path))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

async fn version(executable: &Path, variables: &HashMap<String, String>) -> Option<String> {
    let mut command = Command::new(executable);
    command.arg("--version").env_clear().envs(variables);
    let output = run(command, Duration::from_secs(10)).await?;
    output.lines().next().map(|line| line.trim().to_string())
}

async fn shell_environment() -> HashMap<String, String> {
    const START: &str = "__MOTILE_ENV_START__";
    const END: &str = "__MOTILE_ENV_END__";

    let mut variables: HashMap<String, String> = std::env::vars().collect();
    let shell = variables.get("SHELL").filter(|shell| !shell.is_empty()).cloned().unwrap_or_else(login_shell);
    let mut command = Command::new(shell);
    command.args(["-ilc", &format!("printf '{START}'; env; printf '{END}'")]);
    let Some(output) = run(command, Duration::from_secs(5)).await else { return variables };
    let Some(listing) = output.split_once(START).and_then(|(_, rest)| rest.split_once(END)).map(|(listing, _)| listing)
    else {
        return variables;
    };
    for line in listing.lines() {
        let Some((name, value)) = line.split_once('=') else { continue };
        variables.insert(name.to_string(), value.to_string());
    }
    variables
}

fn login_shell() -> String {
    passwd_field(|entry| entry.pw_shell).filter(|shell| !shell.is_empty()).unwrap_or_else(|| "/bin/sh".to_string())
}

pub fn is_root() -> bool {
    unsafe { libc::getuid() == 0 }
}

/// The name of the user running this, from the user database rather than the environment, which
/// isn't always set.
pub fn user_name() -> String {
    passwd_field(|entry| entry.pw_name).unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "root".to_string()))
}

pub fn home_dir() -> anyhow::Result<PathBuf> {
    if let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) {
        return Ok(PathBuf::from(home));
    }
    passwd_field(|entry| entry.pw_dir).map(PathBuf::from).ok_or_else(|| anyhow::anyhow!("HOME isn't set."))
}

/// A field of this user's entry in the user database, which on a Mac isn't `/etc/passwd`.
fn passwd_field(field: impl FnOnce(&libc::passwd) -> *const libc::c_char) -> Option<String> {
    let entry = unsafe { libc::getpwuid(libc::getuid()) };
    if entry.is_null() {
        return None;
    }
    let value = field(unsafe { &*entry });
    if value.is_null() {
        return None;
    }
    Some(unsafe { std::ffi::CStr::from_ptr(value) }.to_string_lossy().into_owned())
}

/// Stdout of a short-lived process, or `None` if it failed or took too long.
async fn run(mut command: Command, timeout: Duration) -> Option<String> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    let child = command.spawn().ok()?;
    let output = tokio::time::timeout(timeout, child.wait_with_output()).await.ok()?.ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}
