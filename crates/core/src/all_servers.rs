//! The settings every server of the account has on, those linked later too. Each server keeps a
//! copy, the newest is the account's, and the client turns them on wherever they are off.

use motile_protocol::wire::{AllServers, ComputerUse, ComputerUseState, Request, ServerInfo};
use serde::{Deserialize, Serialize};

/// A server from this protocol on can be told to go on after limits and restarts.
const CONTINUES_FROM: u32 = 14;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub struct AllServersView {
    /// Some server can keep them; servers too old to don't.
    pub available: bool,
    pub continue_after_limits: bool,
    pub continue_after_restarts: bool,
    pub computer_use: bool,
}

/// The account's settings: the copy changed last.
pub fn newest<'a>(infos: impl Iterator<Item = &'a ServerInfo>) -> Option<AllServers> {
    infos.filter_map(|info| info.all_servers).max_by(|a, b| a.changed_at.total_cmp(&b.changed_at))
}

pub fn view<'a>(infos: impl Iterator<Item = &'a ServerInfo>) -> AllServersView {
    let Some(all) = newest(infos) else { return AllServersView::default() };
    AllServersView {
        available: true,
        continue_after_limits: all.continue_after_limits,
        continue_after_restarts: all.continue_after_restarts,
        computer_use: all.computer_use,
    }
}

/// Brings the server's settings in line with the account's, as the requests that do it will.
pub fn catch_up(info: &mut ServerInfo, all: &AllServers) -> Vec<Request> {
    let mut requests = Vec::new();
    let continues = &mut info.continue_settings;
    let after_limits = all.continue_after_limits && !continues.after_limits;
    let after_restarts = all.continue_after_restarts && !continues.after_restarts;
    if info.protocol >= CONTINUES_FROM && (after_limits || after_restarts) {
        continues.after_limits |= after_limits;
        continues.after_restarts |= after_restarts;
        requests.push(Request::SetContinueSettings {
            after_limits: after_limits.then_some(true),
            after_restarts: after_restarts.then_some(true),
        });
    }
    if let Some(computer_use) =
        info.computer_use.as_mut().filter(|computer_use| all.computer_use && !computer_use.enabled)
    {
        *computer_use = ComputerUse { enabled: true, state: ComputerUseState::Starting };
        requests.push(Request::SetComputerUse { enabled: true });
    }
    if let Some(kept) = info.all_servers.as_mut().filter(|kept| kept.changed_at < all.changed_at) {
        *kept = *all;
        requests.push(Request::SetAllServers { settings: *all });
    }
    requests
}

#[cfg(test)]
mod tests {
    use super::*;
    use motile_protocol::wire::ContinueSettings;

    fn info(protocol: u32, all_servers: Option<AllServers>) -> ServerInfo {
        serde_json::from_value(serde_json::json!({
            "version": "0.1.0",
            "protocol": protocol,
            "hostname": "vibe",
            "home": "/home/vibe",
            "agents": [],
            "models": [],
            "continue_settings": { "after_limits": false, "after_restarts": false },
            "computer_use": { "enabled": false, "state": { "state": "off" } },
            "all_servers": all_servers,
        }))
        .unwrap()
    }

    fn all(changed_at: f64) -> AllServers {
        AllServers { continue_after_limits: true, continue_after_restarts: false, computer_use: true, changed_at }
    }

    #[test]
    fn the_copy_changed_last_is_the_accounts() {
        let infos = [info(42, Some(all(5.0))), info(42, Some(all(9.0))), info(41, None)];
        assert_eq!(newest(infos.iter()).unwrap().changed_at, 9.0);
        assert_eq!(newest([info(41, None)].iter()), None);
        assert!(!view([info(41, None)].iter()).available);
    }

    #[test]
    fn a_server_with_the_settings_off_has_them_turned_on_and_keeps_the_newer_copy() {
        let mut server = info(42, Some(AllServers::default()));
        let requests = catch_up(&mut server, &all(9.0));
        assert_eq!(requests.len(), 3);
        assert!(matches!(requests[0], Request::SetContinueSettings { after_limits: Some(true), after_restarts: None }));
        assert!(matches!(requests[1], Request::SetComputerUse { enabled: true }));
        assert!(matches!(requests[2], Request::SetAllServers { settings } if settings == all(9.0)));
        assert_eq!(server.continue_settings, ContinueSettings { after_limits: true, after_restarts: false });
        assert!(server.computer_use.as_ref().unwrap().enabled);
        assert!(catch_up(&mut server, &all(9.0)).is_empty());
    }

    #[test]
    fn a_server_too_old_to_keep_the_copy_still_has_the_settings_turned_on() {
        let mut server = info(41, None);
        let requests = catch_up(&mut server, &all(9.0));
        assert_eq!(requests.len(), 2);
        assert!(server.all_servers.is_none());
    }

    #[test]
    fn settings_turned_off_for_all_servers_are_left_as_each_server_has_them() {
        let mut server = info(42, Some(all(5.0)));
        server.continue_settings.after_limits = true;
        let off = AllServers { changed_at: 9.0, ..AllServers::default() };
        let requests = catch_up(&mut server, &off);
        assert!(matches!(requests[..], [Request::SetAllServers { .. }]));
        assert!(server.continue_settings.after_limits);
    }
}
