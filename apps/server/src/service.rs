//! Keeps the server running across reboots and crashes: a systemd unit on Linux, a launchd agent
//! in the user's desktop session on a Mac.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};

use crate::agents::environment::{home_dir, is_root, user_name};

const UNIT_NAME: &str = "motile.service";
const UNIT_FILE: &str = "/etc/systemd/system/motile.service";
const LINUX_BINARY: &str = "/usr/local/bin/motile";

const AGENT_LABEL: &str = "app.motile.server";
/// Claude Code refuses full access as root unless it is told the machine is a sandbox.
pub const SANDBOX_VARIABLE: &str = "IS_SANDBOX";

pub fn is_mac() -> bool {
    cfg!(target_os = "macos")
}

/// Says whether the server is running afterwards.
pub fn install(data_dir: &Path) -> anyhow::Result<bool> {
    if is_mac() {
        return install_agent(data_dir);
    }
    if !has_systemd() {
        println!("systemd isn't running here, so there is no service. Start the server with: motile run");
        return Ok(false);
    }
    if is_root() {
        install_unit(&std::env::current_exe()?, data_dir, &user_name())?;
        return Ok(true);
    }
    println!("Installing the service needs administrator rights.");
    let status = Command::new("sudo")
        .arg(std::env::current_exe()?)
        .args(["service", "install", "--user", &user_name(), "--data-dir"])
        .arg(data_dir)
        .status()
        .context("sudo isn't available. Run the install command as root.")?;
    if !status.success() {
        bail!("The service couldn't be installed.");
    }
    Ok(true)
}

pub fn uninstall() -> anyhow::Result<()> {
    if is_mac() {
        return uninstall_agent();
    }
    if !is_root() {
        bail!("Removing the service needs root. Run: sudo motile uninstall");
    }
    let _ = systemctl(&["disable", "--now", UNIT_NAME]);
    let _ = std::fs::remove_file(UNIT_FILE);
    systemctl(&["daemon-reload"])?;
    println!("Removed {UNIT_NAME}. Threads and the device key are still in the data folder.");
    Ok(())
}

/// What `motile status` says about the service.
pub fn state() -> String {
    if is_mac() {
        return agent_state();
    }
    if !has_systemd() {
        return "no systemd".to_string();
    }
    let Ok(output) = Command::new("systemctl").args(["is-active", UNIT_NAME]).output() else {
        return "unknown".to_string();
    };
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Follows the service's log in this process's place.
pub fn logs() -> anyhow::Result<()> {
    use std::os::unix::process::CommandExt;
    if is_mac() {
        let log = agent_log()?;
        let error = Command::new("tail").args(["-n", "100", "-f"]).arg(&log).exec();
        return Err(error).with_context(|| format!("{} can't be followed.", log.display()));
    }
    let error = Command::new("journalctl").args(["-u", "motile", "-f", "-n", "100"]).exec();
    Err(error).context("journalctl isn't available.")
}

fn has_systemd() -> bool {
    Path::new("/run/systemd/system").is_dir()
}

/// The unit is a system unit so it survives reboots and logouts, but it runs as the user who
/// installed it, with their home folder, where the agents keep their sign-in.
pub fn unit(binary: &Path, data_dir: &Path, user: &str) -> String {
    let sandbox = if user == "root" { format!("Environment={SANDBOX_VARIABLE}=1\n") } else { String::new() };
    format!(
        "[Unit]\n\
         Description=Motile server\n\
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

/// Copies this binary to a stable place and starts the unit. Needs root.
pub fn install_unit(current_binary: &Path, data_dir: &Path, user: &str) -> anyhow::Result<()> {
    if !is_root() {
        bail!("Installing the service needs root.");
    }
    let binary = place_binary(current_binary, Path::new(LINUX_BINARY))?;
    std::fs::write(UNIT_FILE, unit(&binary, data_dir, user))?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", UNIT_NAME])?;
    systemctl(&["restart", UNIT_NAME])?;
    println!("Installed {} and started {UNIT_NAME}.", binary.display());
    Ok(())
}

fn place_binary(current: &Path, target: &Path) -> anyhow::Result<PathBuf> {
    if current == target {
        return Ok(target.to_path_buf());
    }
    if let Some(folder) = target.parent() {
        std::fs::create_dir_all(folder).with_context(|| format!("{} can't be created.", folder.display()))?;
    }
    // A running binary can't be overwritten, but it can be replaced.
    let staged = target.with_file_name(".motile.new");
    std::fs::copy(current, &staged).with_context(|| format!("{} can't be written.", staged.display()))?;
    std::fs::rename(&staged, target)?;
    Ok(target.to_path_buf())
}

fn systemctl(arguments: &[&str]) -> anyhow::Result<()> {
    let status = Command::new("systemctl").args(arguments).status().context("systemctl isn't available.")?;
    if !status.success() {
        bail!("systemctl {} failed.", arguments.join(" "));
    }
    Ok(())
}

/// The agent runs in the user's desktop session, where the agents find the Keychain, the
/// simulators and code signing. It starts when the user logs in, so with automatic login it
/// survives a reboot too.
pub fn agent_plist(binary: &Path, data_dir: &Path, log: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key>\n\
         \t<string>{AGENT_LABEL}</string>\n\
         \t<key>ProgramArguments</key>\n\
         \t<array>\n\
         \t\t<string>{binary}</string>\n\
         \t\t<string>run</string>\n\
         \t\t<string>--data-dir</string>\n\
         \t\t<string>{data_dir}</string>\n\
         \t</array>\n\
         \t<key>RunAtLoad</key>\n\
         \t<true/>\n\
         \t<key>KeepAlive</key>\n\
         \t<true/>\n\
         \t<key>StandardOutPath</key>\n\
         \t<string>{log}</string>\n\
         \t<key>StandardErrorPath</key>\n\
         \t<string>{log}</string>\n\
         </dict>\n\
         </plist>\n",
        binary = xml_text(binary),
        data_dir = xml_text(data_dir),
        log = xml_text(log),
    )
}

fn xml_text(path: &Path) -> String {
    path.display().to_string().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn install_agent(data_dir: &Path) -> anyhow::Result<bool> {
    if is_root() {
        bail!("On a Mac, run the install command as the user whose desktop the agents should use, not as root.");
    }
    let home = home_dir()?;
    let binary = place_binary(&std::env::current_exe()?, &home.join(".local/bin/motile"))?;
    let log = agent_log()?;
    let plist = agent_plist_path()?;
    std::fs::create_dir_all(plist.parent().unwrap_or(&home))?;
    std::fs::write(&plist, agent_plist(&binary, data_dir, &log))?;

    let domain = gui_domain();
    let _ = launchctl(&["bootout", &format!("{domain}/{AGENT_LABEL}")]);
    if launchctl(&["print", &domain]).is_err() {
        println!("Installed the service. It starts when you log in on this Mac's desktop.");
        note_about_automatic_login();
        return Ok(false);
    }
    launchctl(&["bootstrap", &domain, &plist.to_string_lossy()])?;
    println!("Installed {} and started {AGENT_LABEL}.", binary.display());
    note_about_automatic_login();
    Ok(true)
}

fn uninstall_agent() -> anyhow::Result<()> {
    let _ = launchctl(&["bootout", &format!("{}/{AGENT_LABEL}", gui_domain())]);
    let _ = std::fs::remove_file(agent_plist_path()?);
    println!("Removed {AGENT_LABEL}. Threads and the device key are still in the data folder.");
    Ok(())
}

fn agent_state() -> String {
    let Ok(output) = Command::new("launchctl").args(["print", &format!("{}/{AGENT_LABEL}", gui_domain())]).output()
    else {
        return "unknown".to_string();
    };
    if !output.status.success() {
        return "not installed".to_string();
    }
    match String::from_utf8_lossy(&output.stdout).contains("state = running") {
        true => "running".to_string(),
        false => "not running".to_string(),
    }
}

/// Without automatic login, the agent only starts once someone logs in after a reboot.
fn note_about_automatic_login() {
    let output =
        Command::new("defaults").args(["read", "/Library/Preferences/com.apple.loginwindow", "autoLoginUser"]).output();
    let automatic = output.map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string()).unwrap_or_default();
    if automatic == user_name() {
        return;
    }
    println!(
        "After a restart, your server starts once you log in on this Mac. To have it start by itself, turn on \
         automatic login in System Settings > Users & Groups."
    );
}

fn gui_domain() -> String {
    format!("gui/{}", unsafe { libc::getuid() })
}

fn agent_plist_path() -> anyhow::Result<PathBuf> {
    Ok(home_dir()?.join("Library/LaunchAgents").join(format!("{AGENT_LABEL}.plist")))
}

fn agent_log() -> anyhow::Result<PathBuf> {
    Ok(home_dir()?.join("Library/Logs/motile.log"))
}

fn launchctl(arguments: &[&str]) -> anyhow::Result<()> {
    let output = Command::new("launchctl").args(arguments).output().context("launchctl isn't available.")?;
    if !output.status.success() {
        bail!("launchctl {} failed: {}", arguments.join(" "), String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(())
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

    #[test]
    fn the_agent_runs_the_server_from_the_data_folder_and_keeps_it_alive() {
        let plist = agent_plist(
            Path::new("/Users/ann/.local/bin/motile"),
            Path::new("/Users/ann/.local/share/motile"),
            Path::new("/Users/ann/Library/Logs/motile.log"),
        );
        assert!(plist.contains("<string>app.motile.server</string>"));
        assert!(plist.contains(
            "\t\t<string>/Users/ann/.local/bin/motile</string>\n\
             \t\t<string>run</string>\n\
             \t\t<string>--data-dir</string>\n\
             \t\t<string>/Users/ann/.local/share/motile</string>\n"
        ));
        assert!(plist.contains("<key>KeepAlive</key>\n\t<true/>"));
        assert!(plist.contains("<key>StandardErrorPath</key>\n\t<string>/Users/ann/Library/Logs/motile.log</string>"));
    }

    #[test]
    fn paths_are_escaped_for_xml() {
        let plist = agent_plist(Path::new("/Users/a&b/motile"), Path::new("/data"), Path::new("/log"));
        assert!(plist.contains("<string>/Users/a&amp;b/motile</string>"));
    }
}
