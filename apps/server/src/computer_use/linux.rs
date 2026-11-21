//! The screen the driver uses on Linux: the X11 desktop of the server's user when one is signed in,
//! or else one of the server's own. A Wayland desktop gets one of the server's own too: its
//! screenshot prompt asks a service like the server on every capture, at a screen nobody watches.

use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, bail};
use tokio::process::{Child, Command};

/// Set on whatever runs on the server's own screen, so it is never taken for a desktop.
const OWN_SCREEN: &str = "MOTILE_VIRTUAL_SCREEN";
/// What a desktop's processes say that the driver and the agents' apps need.
const DESKTOP_VARIABLES: [&str; 8] = [
    "DISPLAY",
    "XAUTHORITY",
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_RUNTIME_DIR",
    "XDG_SESSION_TYPE",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_DESKTOP",
    "DESKTOP_SESSION",
];
const DISPLAYS: std::ops::RangeInclusive<u32> = 90..=99;
const SIZE: &str = "1920x1080x24";
const STARTS_WITHIN: Duration = Duration::from_secs(10);
pub const PACKAGES: &str = "Xvfb, Openbox, D-Bus and AT-SPI";
const APT_PACKAGES: &str = "xvfb openbox dbus at-spi2-core";
/// Where at-spi2-core lets the session bus start the accessibility bus.
const ACCESSIBILITY_BUS: &str = "/usr/share/dbus-1/services/org.a11y.Bus.service";

pub struct Screen {
    /// What the driver and the agents' processes are given to reach it.
    pub variables: Vec<(String, String)>,
    /// `:N` when it is the server's own.
    pub own_display: Option<String>,
    /// The display server, its session bus and its window manager, which stop with it.
    _processes: Vec<Child>,
}

pub async fn screen(runtime: &Path, path: &str) -> anyhow::Result<Screen> {
    match desktop() {
        Some(variables) => Ok(Screen { variables, own_display: None, _processes: Vec::new() }),
        None => own_screen(runtime, path).await,
    }
}

/// The variables of a signed-in X11 desktop of this user: the server's own, when it was started
/// from one, or those of a process of the desktop's.
fn desktop() -> Option<Vec<(String, String)>> {
    let own: HashMap<String, String> = std::env::vars().collect();
    std::iter::once(own).chain(processes_of_this_user()).find_map(|variables| desktop_of(&variables, x11_display_runs))
}

fn desktop_of(variables: &HashMap<String, String>, runs: impl Fn(&str) -> bool) -> Option<Vec<(String, String)>> {
    if variables.contains_key(OWN_SCREEN) || variables.contains_key("WAYLAND_DISPLAY") {
        return None;
    }
    if variables.get("XDG_SESSION_TYPE").is_some_and(|kind| kind == "wayland") {
        return None;
    }
    let display = variables.get("DISPLAY")?;
    if !runs(display) {
        return None;
    }
    let taken = DESKTOP_VARIABLES.iter().filter_map(|name| Some((name.to_string(), variables.get(*name)?.clone())));
    Some(taken.collect())
}

/// Whether `display` is a local X server that is up: `:0` has its socket at `/tmp/.X11-unix/X0`.
fn x11_display_runs(display: &str) -> bool {
    let Some(number) = display.strip_prefix(':').and_then(|rest| rest.split('.').next()) else { return false };
    Path::new(&format!("/tmp/.X11-unix/X{number}")).exists()
}

fn processes_of_this_user() -> impl Iterator<Item = HashMap<String, String>> {
    let uid = unsafe { libc::getuid() };
    let entries = std::fs::read_dir("/proc").into_iter().flatten().flatten();
    entries.filter(move |entry| entry.metadata().is_ok_and(|metadata| metadata.uid() == uid)).filter_map(|entry| {
        let environ = std::fs::read(entry.path().join("environ")).ok()?;
        Some(parse_environ(&environ))
    })
}

fn parse_environ(bytes: &[u8]) -> HashMap<String, String> {
    let pairs = bytes.split(|byte| *byte == 0).filter_map(|pair| {
        let (name, value) = std::str::from_utf8(pair).ok()?.split_once('=')?;
        Some((name.to_string(), value.to_string()))
    });
    pairs.collect()
}

/// An X server of the server's own, with a session bus for the accessibility bus to start on and a
/// window manager to raise and focus windows.
async fn own_screen(runtime: &Path, path: &str) -> anyhow::Result<Screen> {
    let missing: Vec<&str> =
        ["Xvfb", "openbox", "dbus-daemon"].into_iter().filter(|program| !on_path(program, path)).collect();
    if !missing.is_empty() || !Path::new(ACCESSIBILITY_BUS).exists() {
        bail!(
            "This server has no desktop to use, so it needs {PACKAGES} for a screen of its own. \
             On Debian and Ubuntu: sudo apt install {APT_PACKAGES}"
        );
    }
    let number = DISPLAYS.into_iter().find(|number| display_is_free(*number)).context("No display is free.")?;
    let display = format!(":{number}");
    let bus = runtime.join("bus");
    let _ = std::fs::remove_file(&bus);
    let address = format!("unix:path={}", bus.display());
    let mut variables = vec![
        ("DISPLAY".to_string(), display.clone()),
        ("DBUS_SESSION_BUS_ADDRESS".to_string(), address.clone()),
        ("XDG_SESSION_TYPE".to_string(), "x11".to_string()),
        // Qt apps only say what they show to an accessibility bus they were told about.
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON".to_string(), "1".to_string()),
        (OWN_SCREEN.to_string(), "1".to_string()),
    ];
    let mut processes = Vec::new();
    let server = ["-screen", "0", SIZE, "-nolisten", "tcp"];
    processes.push(start("Xvfb", std::iter::once(display.as_str()).chain(server), path, &[])?);
    wait_for(&PathBuf::from(format!("/tmp/.X11-unix/X{number}")), &mut processes[0]).await?;
    let session = ["--session", "--nofork", "--nopidfile"];
    let bus_argument = format!("--address={address}");
    processes.push(start("dbus-daemon", session.into_iter().chain([bus_argument.as_str()]), path, &[])?);
    wait_for(&bus, &mut processes[1]).await?;
    processes.push(start("openbox", std::iter::empty(), path, &variables)?);
    let accessible = ["set", "org.gnome.desktop.interface", "toolkit-accessibility", "true"];
    let set = Command::new("gsettings").args(accessible).envs(variables.clone()).env("PATH", path).status().await;
    if !set.is_ok_and(|status| status.success()) {
        // GTK apps then need to be told by their environment.
        variables.push(("GTK_MODULES".to_string(), "gail:atk-bridge".to_string()));
    }
    Ok(Screen { variables, own_display: Some(display), _processes: processes })
}

fn display_is_free(number: u32) -> bool {
    !Path::new(&format!("/tmp/.X11-unix/X{number}")).exists() && !Path::new(&format!("/tmp/.X{number}-lock")).exists()
}

fn on_path(program: &str, path: &str) -> bool {
    std::env::split_paths(path).any(|folder| folder.join(program).is_file())
}

/// Starts `program`, which stops when the server does, however it stops.
fn start<'a>(
    program: &str,
    arguments: impl IntoIterator<Item = &'a str>,
    path: &str,
    variables: &[(String, String)],
) -> anyhow::Result<Child> {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .env("PATH", path)
        .envs(variables.iter().cloned())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    unsafe {
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    command.spawn().with_context(|| format!("{program} couldn't be started."))
}

async fn wait_for(socket: &Path, process: &mut Child) -> anyhow::Result<()> {
    let started = tokio::time::Instant::now();
    while !socket.exists() {
        if let Ok(Some(status)) = process.try_wait() {
            bail!("The server's screen stopped as it started ({status}).");
        }
        if started.elapsed() > STARTS_WITHIN {
            bail!("The server's screen didn't start.");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variables(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect()
    }

    #[test]
    fn an_x11_desktop_that_runs_is_used_with_what_its_apps_need() {
        let session = variables(&[
            ("DISPLAY", ":0"),
            ("XAUTHORITY", "/run/user/1000/gdm/Xauthority"),
            ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
            ("XDG_SESSION_TYPE", "x11"),
            ("HOME", "/home/ann"),
        ]);
        let found = desktop_of(&session, |display| display == ":0").unwrap();
        assert!(found.contains(&("DISPLAY".to_string(), ":0".to_string())));
        assert!(found.contains(&("DBUS_SESSION_BUS_ADDRESS".to_string(), "unix:path=/run/user/1000/bus".to_string())));
        assert!(!found.iter().any(|(name, _)| name == "HOME"));
        assert_eq!(desktop_of(&session, |_| false), None);
    }

    #[test]
    fn wayland_desktops_and_the_servers_own_screen_are_not_used_as_desktops() {
        let wayland = variables(&[("DISPLAY", ":0"), ("WAYLAND_DISPLAY", "wayland-0")]);
        let said_wayland = variables(&[("DISPLAY", ":0"), ("XDG_SESSION_TYPE", "wayland")]);
        let own = variables(&[("DISPLAY", ":90"), (OWN_SCREEN, "1")]);
        for variables in [wayland, said_wayland, own] {
            assert_eq!(desktop_of(&variables, |_| true), None);
        }
        assert_eq!(desktop_of(&variables(&[("HOME", "/home/ann")]), |_| true), None);
    }

    #[test]
    fn a_processs_environment_is_read_as_its_variables() {
        let read = parse_environ(b"DISPLAY=:1\0EMPTY=\0A=b=c\0broken\0");
        assert_eq!(read.get("DISPLAY").map(String::as_str), Some(":1"));
        assert_eq!(read.get("EMPTY").map(String::as_str), Some(""));
        assert_eq!(read.get("A").map(String::as_str), Some("b=c"));
        assert_eq!(read.len(), 3);
    }

    #[test]
    fn a_display_runs_when_its_socket_is_there() {
        assert!(!x11_display_runs("localhost:10.0"));
        assert!(!x11_display_runs(":4242"));
    }
}
