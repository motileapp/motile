//! The macOS permissions the driver needs. It runs under the server's program, so they are the
//! program's: what the agents' scripts already use, which survive its updates since they belong to
//! its signature. macOS keeps a process's answer once it has asked, so a new process asks each time.

use anyhow::Context;
use serde::{Deserialize, Serialize};

pub const SUBCOMMAND: &str = "computer-use-permissions";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Permissions {
    pub accessibility: bool,
    pub screen_recording: bool,
}

impl Permissions {
    pub fn granted(&self) -> bool {
        self.accessibility && self.screen_recording
    }
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
}

/// What `SUBCOMMAND` prints: this process's permissions, which are those of the program that
/// started it when that is the server's.
pub fn print() {
    let permissions = unsafe {
        Permissions { accessibility: AXIsProcessTrusted(), screen_recording: CGPreflightScreenCaptureAccess() }
    };
    println!("{}", serde_json::to_string(&permissions).unwrap_or_default());
}

/// The server's permissions now, asked by a process of its own program.
pub async fn check() -> anyhow::Result<Permissions> {
    let program = std::env::current_exe()?;
    let output = tokio::process::Command::new(program).arg(SUBCOMMAND).kill_on_drop(true).output().await?;
    serde_json::from_slice(&output.stdout).context("The server couldn't tell which permissions it has.")
}
