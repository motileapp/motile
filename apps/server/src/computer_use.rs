//! Computer use: Cua Driver (github.com/trycua/cua, MIT) lets the agents see and use the apps on the
//! server's screen. While it is on, the server runs one driver under its own program, so that macOS
//! asks for the server's permissions, and every agent started gets an MCP server that talks to it.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
mod release;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, bail};
use motile_protocol::wire::{ComputerUse as ComputerUseInfo, ComputerUseState};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::watch;

/// What the agents know the driver's MCP server by.
pub const SERVER_NAME: &str = "cua-driver";
/// Names a driver to use in place of the release, for the tests and the dev apps. With it there is
/// no download, no permission to check and no screen to find. Empty is the same as unset.
const STAND_IN: &str = "MOTILE_CUA_DRIVER";
/// The label the driver gives the program it runs under.
const HOST_ID: &str = "app.motile.server";
/// Tools the agents don't get: those that change the driver itself, and those going away.
pub const HIDDEN_TOOLS: [&str; 15] = [
    "set_config",
    "get_config",
    "install_extension",
    "install_ffmpeg",
    "check_for_update",
    "start_recording",
    "stop_recording",
    "get_recording_state",
    "replay_trajectory",
    "set_agent_cursor_theme",
    "set_agent_cursor_motion",
    "set_agent_cursor_enabled",
    "get_agent_cursor_state",
    "get_accessibility_tree",
    "parse_visual_regions",
];
/// Tools that only look, which every access lets the agents use without asking.
pub const LOOKING_TOOLS: [&str; 15] = [
    "get_window_state",
    "get_desktop_state",
    "zoom",
    "list_apps",
    "list_windows",
    "verify_state",
    "get_screen_size",
    "get_cursor_position",
    "get_browser_state",
    "check_permissions",
    "health_report",
    "get_session",
    "list_sessions",
    "get_guide",
    "start_session",
];
/// The driver's state, kept apart from anything the user's own driver keeps.
const STATE_FOLDER: &str = "state";
const STARTS_WITHIN: Duration = Duration::from_secs(30);
const TRIES_AGAIN_AFTER: Duration = Duration::from_secs(30);
/// How soon granting the permissions is noticed, and taking them away while the driver runs.
#[cfg(target_os = "macos")]
const PERMISSIONS_CHECKED_EVERY: Duration = Duration::from_secs(5);
#[cfg(target_os = "macos")]
const PERMISSIONS_RECHECKED_EVERY: Duration = Duration::from_secs(30);
/// What the driver last wrote to its error output, to say why it stopped.
const STDERR_KEPT: usize = 2000;
/// Numbers each run of the driver for the agents' MCP servers, which stop with the run they reach:
/// a new run changes what they are given, so they are started again.
const RUN_VARIABLE: &str = "MOTILE_DRIVER_RUN";

/// What an agent's process is given to reach the driver.
#[derive(Clone, Debug, PartialEq)]
pub struct DriverAccess {
    /// The MCP server's command, arguments and environment.
    pub command: PathBuf,
    pub arguments: Vec<String>,
    pub variables: Vec<(String, String)>,
    /// What the agent's own process is given besides: the screen the apps it starts show on.
    pub screen: Vec<(String, String)>,
    /// The server's own screen, as `:N`, when it has no desktop the agents use.
    pub own_display: Option<String>,
}

/// What reaches the screen the driver uses, which stays up as long as this is kept.
#[derive(Default)]
struct Screen {
    variables: Vec<(String, String)>,
    own_display: Option<String>,
    #[cfg(target_os = "linux")]
    _kept: Option<linux::Screen>,
}

pub struct ComputerUse {
    folder: PathBuf,
    /// The variables of the user's login shell that the driver runs with.
    login: Vec<(String, String)>,
    enabled: watch::Sender<bool>,
    state: watch::Sender<ComputerUseState>,
    access: Mutex<Option<DriverAccess>>,
    runs: AtomicU64,
}

impl ComputerUse {
    pub fn new(folder: PathBuf, login: &HashMap<String, String>, enabled: bool) -> Arc<Self> {
        let kept = ["PATH", "HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "TMPDIR", "SHELL"];
        let login = kept.iter().filter_map(|name| Some((name.to_string(), login.get(*name)?.clone()))).collect();
        Arc::new(Self {
            folder,
            login,
            enabled: watch::Sender::new(enabled),
            state: watch::Sender::new(ComputerUseState::Off),
            access: Mutex::new(None),
            runs: AtomicU64::new(0),
        })
    }

    pub fn info(&self) -> ComputerUseInfo {
        ComputerUseInfo { enabled: *self.enabled.borrow(), state: self.state.borrow().clone() }
    }

    pub fn set_enabled(&self, enabled: bool) {
        let changed = self.enabled.send_if_modified(|on| std::mem::replace(on, enabled) != enabled);
        if !changed {
            return;
        }
        if !enabled {
            self.set_access(None);
        }
        self.state.send_replace(if enabled { ComputerUseState::Starting } else { ComputerUseState::Off });
    }

    /// Changes whenever `info` would say something else.
    pub fn changes(&self) -> watch::Receiver<ComputerUseState> {
        self.state.subscribe()
    }

    /// What the agents started now are given, while the driver runs.
    pub fn access(&self) -> Option<DriverAccess> {
        self.access.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    /// Runs the driver while computer use is on, and starts it again when it stops.
    pub fn keep_running(self: &Arc<Self>) {
        let this = self.clone();
        tokio::spawn(async move {
            let mut enabled = this.enabled.subscribe();
            loop {
                if !*enabled.borrow_and_update() {
                    this.state.send_replace(ComputerUseState::Off);
                    if enabled.changed().await.is_err() {
                        return;
                    }
                    continue;
                }
                this.state.send_replace(ComputerUseState::Starting);
                // Turned off, or off and on again before this saw it: the driver starts over or stops.
                let ran = tokio::select! {
                    ran = this.run() => ran,
                    _ = enabled.changed() => Ok(()),
                };
                this.set_access(None);
                let Err(error) = ran else { continue };
                tracing::warn!("computer use stopped: {error:#}");
                this.state.send_replace(ComputerUseState::Failed { message: error.to_string() });
                tokio::select! {
                    _ = tokio::time::sleep(TRIES_AGAIN_AFTER) => {}
                    _ = enabled.changed() => {}
                }
            }
        });
    }

    fn set_access(&self, access: Option<DriverAccess>) {
        *self.access.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = access;
    }

    /// Starts the driver and gives it to the agents until it stops. `Ok` when it should start again
    /// at once, as when the permissions it ran with were taken away.
    async fn run(&self) -> anyhow::Result<()> {
        let stand_in = std::env::var_os(STAND_IN).filter(|path| !path.is_empty()).map(PathBuf::from);
        let program = match &stand_in {
            Some(program) => program.clone(),
            None => release::install(&self.folder).await?,
        };
        #[cfg(target_os = "macos")]
        if stand_in.is_none() {
            self.wait_for_permissions().await?;
        }
        let kept = Runtime(runtime_folder(&self.folder)?);
        let runtime = &kept.0;
        let screen = self.screen(runtime, stand_in.is_some()).await?;
        let (screen_variables, own_display) = (screen.variables.clone(), screen.own_display.clone());

        let socket = runtime.join("driver.sock");
        let _ = std::fs::remove_file(&socket);
        let driver_variables = self.driver_variables();
        let mut driver = Command::new(&program);
        driver
            .args(serve_arguments(&socket))
            .env_clear()
            .envs(self.login.iter().cloned())
            .envs(screen_variables.iter().cloned())
            .envs(driver_variables.iter().cloned())
            // It stops when its input ends: when the server does, however it stops.
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut driver = driver.spawn().context("The computer use driver couldn't be started.")?;
        // Held apart, since waiting for the driver would close it.
        let _input = driver.stdin.take();
        let stderr = keep_stderr(&mut driver);
        if let Err(error) = wait_for_socket(&socket, &mut driver).await {
            let said = stderr.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
            return Err(error.context(said.trim().to_string()));
        }
        let mut variables = self.login.clone();
        variables.extend(screen_variables.iter().cloned());
        variables.extend(driver_variables);
        let run = self.runs.fetch_add(1, Ordering::Relaxed) + 1;
        variables.push((RUN_VARIABLE.to_string(), run.to_string()));
        self.set_access(Some(DriverAccess {
            command: program,
            arguments: mcp_arguments(&socket),
            variables,
            screen: screen_variables,
            own_display: own_display.clone(),
        }));
        self.state.send_replace(ComputerUseState::Ready { virtual_screen: own_display.is_some() });

        #[cfg(target_os = "macos")]
        let taken_away = async {
            if stand_in.is_some() {
                return std::future::pending().await;
            }
            loop {
                tokio::time::sleep(PERMISSIONS_RECHECKED_EVERY).await;
                if macos::check().await.is_ok_and(|permissions| !permissions.granted()) {
                    return;
                }
            }
        };
        #[cfg(not(target_os = "macos"))]
        let taken_away = std::future::pending::<()>();
        tokio::select! {
            exited = driver.wait() => {
                let said = stderr.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
                let status = exited.map_or_else(|error| error.to_string(), |status| status.to_string());
                bail!("The computer use driver stopped ({status}). {}", said.trim())
            }
            _ = taken_away => Ok(()),
        }
    }

    /// The screen the driver uses on Linux. On macOS the driver sees the apps as they are.
    #[cfg(target_os = "linux")]
    async fn screen(&self, runtime: &Path, stand_in: bool) -> anyhow::Result<Screen> {
        if stand_in {
            return Ok(Screen::default());
        }
        let path = self.login.iter().find(|(name, _)| name == "PATH").map_or("/usr/bin:/bin", |(_, path)| path);
        let screen = linux::screen(runtime, path).await?;
        Ok(Screen { variables: screen.variables.clone(), own_display: screen.own_display.clone(), _kept: Some(screen) })
    }

    #[cfg(not(target_os = "linux"))]
    async fn screen(&self, _runtime: &Path, _stand_in: bool) -> anyhow::Result<Screen> {
        Ok(Screen::default())
    }

    /// Waits until the server has the macOS permissions, saying which it lacks meanwhile.
    #[cfg(target_os = "macos")]
    async fn wait_for_permissions(&self) -> anyhow::Result<()> {
        loop {
            let permissions = macos::check().await?;
            if permissions.granted() {
                return Ok(());
            }
            self.state.send_replace(ComputerUseState::NeedsPermissions {
                accessibility: !permissions.accessibility,
                screen_recording: !permissions.screen_recording,
            });
            tokio::time::sleep(PERMISSIONS_CHECKED_EVERY).await;
        }
    }

    /// Keeps the driver's state in the server's folder and its telemetry and update checks off:
    /// the server pins its version.
    fn driver_variables(&self) -> Vec<(String, String)> {
        let state = self.folder.join(STATE_FOLDER);
        [
            ("CUA_DRIVER_RS_HOME", state.display().to_string()),
            ("CUA_DRIVER_RS_TELEMETRY_ENABLED", "false".to_string()),
            ("DO_NOT_TRACK", "1".to_string()),
            ("CUA_DRIVER_RS_UPDATE_CHECK", "false".to_string()),
        ]
        .map(|(name, value)| (name.to_string(), value))
        .into()
    }
}

fn serve_arguments(socket: &Path) -> Vec<String> {
    let socket = socket.display().to_string();
    let arguments = ["serve", "--embedded", "--parent-liveness-stdio", "--no-permissions-gate", "--socket", &socket];
    let rest = ["--host-bundle-id", HOST_ID, "--permission-mode", "standard"];
    arguments.into_iter().chain(rest).map(String::from).collect()
}

fn mcp_arguments(socket: &Path) -> Vec<String> {
    let socket = socket.display().to_string();
    ["mcp", "--embedded", "--socket", &socket, "--host-bundle-id", HOST_ID].map(String::from).into()
}

/// The folder of the driver's socket and the screen's session bus, removed once they stop.
struct Runtime(PathBuf);

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A folder for the driver's socket and the screen's session bus, whose paths can't be long: the
/// server's own folder can be too deep.
fn runtime_folder(folder: &Path) -> anyhow::Result<PathBuf> {
    use sha2::Digest;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let named = hex::encode(&sha2::Sha256::digest(folder.as_os_str().as_encoded_bytes())[..6]);
    let runtime = std::env::temp_dir().join(format!("motile-cua-{named}"));
    let _ = std::fs::DirBuilder::new().mode(0o700).create(&runtime);
    let metadata = std::fs::metadata(&runtime).context("The computer use driver has no folder for its socket.")?;
    if metadata.uid() != unsafe { libc::getuid() } || metadata.mode() & 0o077 != 0 {
        bail!("{} belongs to someone else.", runtime.display());
    }
    Ok(runtime)
}

fn keep_stderr(driver: &mut Child) -> Arc<Mutex<String>> {
    let kept = Arc::new(Mutex::new(String::new()));
    let Some(mut stderr) = driver.stderr.take() else { return kept };
    let writer = kept.clone();
    tokio::spawn(async move {
        let mut buffer = [0u8; 4096];
        while let Ok(read) = stderr.read(&mut buffer).await {
            if read == 0 {
                break;
            }
            let mut kept = writer.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            kept.push_str(&String::from_utf8_lossy(&buffer[..read]));
            if kept.len() > STDERR_KEPT {
                let cut = kept.len() - STDERR_KEPT;
                let cut = (cut..kept.len()).find(|index| kept.is_char_boundary(*index)).unwrap_or(kept.len());
                kept.replace_range(..cut, "");
            }
        }
    });
    kept
}

async fn wait_for_socket(socket: &Path, driver: &mut Child) -> anyhow::Result<()> {
    let started = tokio::time::Instant::now();
    while !socket.exists() {
        if let Ok(Some(status)) = driver.try_wait() {
            bail!("The computer use driver stopped as it started ({status}).");
        }
        if started.elapsed() > STARTS_WITHIN {
            bail!("The computer use driver didn't start.");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(())
}

/// What the agents are told while they have the driver.
pub fn instructions(access: &DriverAccess) -> String {
    let mut text = String::from(
        "\n\nThe cua-driver tools see and use the apps on the server's screen: their windows, buttons, menus and \
         text, read from each app's accessibility tree. Use them whenever a task needs an app's interface, instead of \
         scripting clicks and keys yourself (osascript, cliclick, xdotool) or any other computer use tool you have. \
         Shell commands and your other tools stay better for whatever doesn't need the interface. Their input goes to \
         the target window in the background, without raising it or taking focus: keep it that way, and pass \
         delivery_mode \"foreground\" only after a background action didn't land. Target windows by pid and window id, \
         never by what is frontmost. Other threads may use the same screen, and someone may be at it: leave it as you \
         found it, closing the windows you opened and no others.",
    );
    if let Some(display) = &access.own_display {
        text += &format!(
            " This server has no desktop to use, so it has a screen of its own, display {display}: apps you start \
             from a shell show there, where the cua-driver tools see them."
        );
    }
    text
}

/// What a process that was started before it got the driver is told with its next prompt: what
/// it would have been told as it started, and the screen its shell doesn't know of.
pub fn note(access: &DriverAccess) -> String {
    let mut text = format!("Computer use is on now. {}", instructions(access).trim_start());
    if !access.screen.is_empty() {
        let screen: Vec<String> = access.screen.iter().map(|(name, value)| format!("{name}={value}")).collect();
        text += &format!(" Your shell started without that screen: start apps with {}.", screen.join(" "));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_driver_runs_under_the_server_and_each_agent_reaches_it_through_its_socket() {
        let socket = Path::new("/tmp/motile-cua-1/driver.sock");
        let serve = serve_arguments(socket);
        assert_eq!(serve[..3], ["serve", "--embedded", "--parent-liveness-stdio"]);
        assert!(serve.windows(2).any(|pair| pair == ["--socket", "/tmp/motile-cua-1/driver.sock"]));
        assert!(serve.windows(2).any(|pair| pair == ["--permission-mode", "standard"]));
        let mcp = mcp_arguments(socket);
        assert_eq!(
            mcp,
            ["mcp", "--embedded", "--socket", "/tmp/motile-cua-1/driver.sock", "--host-bundle-id", HOST_ID]
        );
    }

    #[test]
    fn the_driver_keeps_its_state_apart_and_reports_nothing() {
        let computer_use = ComputerUse::new(PathBuf::from("/data/computer-use"), &HashMap::new(), true);
        let variables: HashMap<String, String> = computer_use.driver_variables().into_iter().collect();
        assert_eq!(variables["CUA_DRIVER_RS_HOME"], "/data/computer-use/state");
        assert_eq!(variables["CUA_DRIVER_RS_TELEMETRY_ENABLED"], "false");
        assert_eq!(variables["CUA_DRIVER_RS_UPDATE_CHECK"], "false");
    }

    #[test]
    fn the_socket_folder_is_short_and_the_same_for_the_same_server() {
        let deep =
            Path::new("/Users/someone/.local/share/motile/worktrees/motile/0123abcd/apps/macos/build/dev/server");
        let runtime = Runtime(runtime_folder(&deep.join("computer-use")).unwrap());
        assert_eq!(runtime.0, runtime_folder(&deep.join("computer-use")).unwrap());
        let elsewhere = Runtime(runtime_folder(Path::new("/elsewhere/computer-use")).unwrap());
        assert_ne!(runtime.0, elsewhere.0);
        assert!(runtime.0.join("driver.sock").as_os_str().len() < 104, "{}", runtime.0.display());
    }

    #[test]
    fn agents_on_a_server_without_a_desktop_are_told_where_their_apps_show() {
        let access = DriverAccess {
            command: PathBuf::from("/data/cua-driver"),
            arguments: Vec::new(),
            variables: Vec::new(),
            screen: Vec::new(),
            own_display: None,
        };
        assert!(instructions(&access).contains("instead of scripting clicks and keys yourself"));
        assert!(!instructions(&access).contains("display"));
        let own = DriverAccess { own_display: Some(":90".to_string()), ..access };
        assert!(instructions(&own).contains("display :90"));
    }

    #[tokio::test]
    async fn a_driver_that_starts_is_given_to_the_agents_until_computer_use_is_turned_off() {
        let folder = tempfile::tempdir().unwrap();
        let driver = folder.path().join("driver");
        std::fs::write(
            &driver,
            "#!/bin/sh\nwhile [ $# -gt 0 ]; do [ \"$1\" = --socket ] && socket=$2; shift; done\ntouch \"$socket\"\ncat > /dev/null\n",
        )
        .unwrap();
        std::fs::set_permissions(&driver, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let computer_use = ComputerUse::new(folder.path().join("computer-use"), &HashMap::new(), false);
        let mut changes = computer_use.changes();
        // SAFETY: no other test reads the stand-in.
        unsafe { std::env::set_var(STAND_IN, &driver) };
        computer_use.keep_running();
        assert_eq!(computer_use.access(), None);

        computer_use.set_enabled(true);
        let ready = changes.wait_for(|state| matches!(state, ComputerUseState::Ready { .. })).await.unwrap().clone();
        assert_eq!(ready, ComputerUseState::Ready { virtual_screen: false });
        let access = computer_use.access().unwrap();
        assert_eq!(access.command, driver);
        assert_eq!(access.arguments[..3], ["mcp", "--embedded", "--socket"]);

        computer_use.set_enabled(false);
        changes.wait_for(|state| *state == ComputerUseState::Off).await.unwrap();
        assert_eq!(computer_use.access(), None);
        assert_eq!(computer_use.info(), ComputerUseInfo { enabled: false, state: ComputerUseState::Off });

        computer_use.set_enabled(true);
        changes.wait_for(|state| matches!(state, ComputerUseState::Ready { .. })).await.unwrap();
        let again = computer_use.access().unwrap();
        assert_eq!((&again.command, &again.arguments), (&access.command, &access.arguments));
        assert_ne!(again, access, "the agents' servers reached the run that stopped, so they start again");
    }
}
