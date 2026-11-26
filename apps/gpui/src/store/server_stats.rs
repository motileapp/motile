//! What a server's machine does, for the dropdown its line in the sidebar opens.

use std::collections::HashMap;

use motile_core::api::Command;
use motile_core::server_stats::StatsView;

use super::Store;

#[derive(Clone, PartialEq, Debug)]
pub enum ShownStats {
    Loading,
    Shown(StatsView),
    Failed(String),
}

impl ShownStats {
    /// How long the machine has been up, "2d 10h", once the server has said.
    pub fn uptime(&self) -> Option<&str> {
        match self {
            ShownStats::Shown(stats) => Some(&stats.uptime),
            ShownStats::Loading | ShownStats::Failed(_) => None,
        }
    }
}

static LOADING: ShownStats = ShownStats::Loading;

/// The server whose dropdown is open, and what each server's dropdown last showed.
#[derive(Default)]
pub struct ServerStats {
    open: Option<String>,
    shown: HashMap<String, ShownStats>,
}

impl ServerStats {
    pub fn open_server(&self) -> Option<&str> {
        self.open.as_deref()
    }

    pub fn of(&self, server_id: &str) -> &ShownStats {
        self.shown.get(server_id).unwrap_or(&LOADING)
    }

    /// Opens the server's dropdown, which shows what it last showed until the server answers.
    /// Answers the server whose dropdown it closed for it.
    fn open(&mut self, server_id: &str) -> Option<String> {
        let closed = self.open.replace(server_id.to_string()).filter(|closed| closed != server_id);
        if !matches!(self.shown.get(server_id), Some(ShownStats::Shown(_))) {
            self.shown.insert(server_id.to_string(), ShownStats::Loading);
        }
        closed
    }

    fn close(&mut self) -> Option<String> {
        self.open.take()
    }

    /// Takes what a server's machine does, or why it can't be shown, while its dropdown is open.
    fn apply(&mut self, server_id: String, stats: Option<StatsView>, error: Option<String>) {
        if self.open.as_deref() != Some(server_id.as_str()) {
            return;
        }
        let shown = match stats {
            Some(stats) => ShownStats::Shown(stats),
            None => ShownStats::Failed(error.unwrap_or_default()),
        };
        self.shown.insert(server_id, shown);
    }
}

impl Store {
    /// Opens the server's dropdown and follows its machine while it is open.
    pub fn open_stats(&mut self, server_id: &str) {
        if self.server_stats.open_server() == Some(server_id) {
            return;
        }
        if let Some(closed) = self.server_stats.open(server_id) {
            self.send(Command::WatchServerStats { server_id: closed, on: false });
        }
        self.send(Command::WatchServerStats { server_id: server_id.to_string(), on: true });
    }

    pub fn close_stats(&mut self) {
        let Some(closed) = self.server_stats.close() else { return };
        self.send(Command::WatchServerStats { server_id: closed, on: false });
    }

    pub(super) fn apply_server_stats(&mut self, server_id: String, stats: Option<StatsView>, error: Option<String>) {
        self.server_stats.apply(server_id, stats, error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(uptime: &str) -> StatsView {
        StatsView { uptime: uptime.into(), rows: Vec::new() }
    }

    #[test]
    fn a_dropdown_opened_again_shows_its_last_stats_until_new_ones_come() {
        let mut watched = ServerStats::default();
        watched.open("a");
        assert_eq!(watched.of("a"), &ShownStats::Loading);
        watched.apply("a".into(), Some(stats("1h")), None);
        watched.close();
        watched.apply("a".into(), Some(stats("2h")), None);
        assert_eq!(watched.of("a").uptime(), Some("1h"));
        watched.open("a");
        assert_eq!(watched.of("a").uptime(), Some("1h"));
        watched.apply("a".into(), Some(stats("3h")), None);
        assert_eq!(watched.of("a").uptime(), Some("3h"));
    }

    #[test]
    fn a_failed_dropdown_loads_again_and_another_server_closes_it() {
        let mut watched = ServerStats::default();
        watched.open("a");
        watched.apply("a".into(), None, Some("Update studio to see this".into()));
        assert_eq!(watched.of("a"), &ShownStats::Failed("Update studio to see this".into()));
        assert_eq!(watched.open("b"), Some("a".into()));
        assert_eq!(watched.open("a"), Some("b".into()));
        assert_eq!(watched.of("a"), &ShownStats::Loading);
        assert_eq!(watched.open("a"), None);
        assert_eq!(watched.close(), Some("a".into()));
    }
}
