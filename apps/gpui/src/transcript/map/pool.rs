//! The maps that run live: at most three. A map is shown each frame it is drawn; when a fourth
//! is shown, the one shown longest ago that is off screen gives its live state up: its tiles,
//! its paths and its animations.

use std::time::{Duration, Instant};

use gpui_kit::*;

use super::view::MapView;

pub const LIVE: usize = 3;
/// A map not drawn for this long is off screen.
const OFF_SCREEN: Duration = Duration::from_millis(250);

#[derive(Default)]
struct Pool {
    maps: Vec<(u64, WeakEntity<MapView>, Instant)>,
}

impl Global for Pool {}

/// The map was drawn: it is live, and the least recently shown off-screen ones beyond three
/// are not.
pub fn shown(id: u64, map: WeakEntity<MapView>, cx: &mut App) {
    let pool = cx.default_global::<Pool>();
    pool.maps.retain(|(other, _, _)| *other != id);
    pool.maps.push((id, map, Instant::now()));
    let mut evicted = Vec::new();
    while pool.maps.len() > LIVE {
        let Some(oldest) = pool.maps.iter().position(|(_, _, at)| at.elapsed() >= OFF_SCREEN) else { break };
        evicted.push(pool.maps.remove(oldest).1);
    }
    for map in evicted {
        let _ = map.update(cx, |map, _| map.give_up());
    }
}

/// The map is gone.
pub fn forget(id: u64, cx: &mut App) {
    if let Some(pool) = cx.try_global::<Pool>()
        && pool.maps.iter().any(|(other, _, _)| *other == id)
    {
        cx.global_mut::<Pool>().maps.retain(|(other, _, _)| *other != id);
    }
}
