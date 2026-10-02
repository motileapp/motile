//! The relays an app and a host meet on when they can't reach each other directly.

use std::sync::Arc;

use iroh::{RelayConfig, RelayMode, RelayUrl};

pub const RELAY_URL: &str = "https://relay.motile.app";

/// Motile's relay next to iroh's public ones. Only iroh's tell a device its public address,
/// which it needs to be reached directly from behind a router.
pub fn relay_mode() -> RelayMode {
    let relays = RelayMode::Default.relay_map();
    let url: RelayUrl = RELAY_URL.parse().expect("a valid relay URL");
    relays.insert(url.clone(), Arc::new(RelayConfig::new(url, None)));
    RelayMode::Custom(relays)
}
