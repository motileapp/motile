//! Where the server keeps its files, and which account it belongs to.

use std::path::{Path, PathBuf};

use anyhow::Context;
use motile_protocol::identity::DeviceKey;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct DataDir(PathBuf);

/// Written by `setup`: the auth server this server asks for its account's devices.
#[derive(Serialize, Deserialize, Clone)]
pub struct Account {
    pub auth_url: String,
    pub email: String,
}

impl DataDir {
    pub fn new(explicit: Option<PathBuf>) -> anyhow::Result<Self> {
        let path = match explicit {
            Some(path) => path,
            None => default_data_dir().context("HOME isn't set; pass --data-dir.")?,
        };
        std::fs::create_dir_all(&path).with_context(|| format!("{} can't be created.", path.display()))?;
        Ok(Self(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn database(&self) -> PathBuf {
        self.0.join("motile.sqlite")
    }

    pub fn attachments(&self) -> PathBuf {
        self.0.join("attachments")
    }

    /// The git worktrees of the threads that work in one of their own.
    pub fn worktrees(&self) -> PathBuf {
        self.0.join("worktrees")
    }

    /// The copies of the images and videos that threads show.
    pub fn media(&self) -> PathBuf {
        self.0.join("media")
    }

    /// The account's clients, as the auth server last listed them.
    pub fn account_keys(&self) -> PathBuf {
        self.0.join("account-keys.json")
    }

    pub fn device_key(&self) -> anyhow::Result<DeviceKey> {
        DeviceKey::load_or_create(&self.0.join("device.key")).context("The device key can't be read.")
    }

    pub fn account(&self) -> Option<Account> {
        let text = std::fs::read_to_string(self.0.join("account.json")).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save_account(&self, account: &Account) -> anyhow::Result<()> {
        std::fs::write(self.0.join("account.json"), serde_json::to_string_pretty(account)?)?;
        Ok(())
    }
}

fn default_data_dir() -> Option<PathBuf> {
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(data_home).join("motile"));
    }
    let home = std::env::var_os("HOME").filter(|value| !value.is_empty())?;
    Some(PathBuf::from(home).join(".local/share/motile"))
}
