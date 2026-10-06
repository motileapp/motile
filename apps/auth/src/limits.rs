//! Slows the guessing of install codes: an address that fails to enroll too often is refused for
//! a while.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::HeaderMap;

const WINDOW: Duration = Duration::from_secs(15 * 60);
pub const MAX_ENROLL_FAILURES: u32 = 10;

#[derive(Default)]
pub struct EnrollFailures {
    by_address: Mutex<HashMap<IpAddr, (Instant, u32)>>,
}

impl EnrollFailures {
    pub fn blocked(&self, address: IpAddr) -> bool {
        let mut failures = self.by_address.lock().unwrap();
        let Some((since, count)) = failures.get(&address) else { return false };
        if since.elapsed() > WINDOW {
            failures.remove(&address);
            return false;
        }
        *count >= MAX_ENROLL_FAILURES
    }

    pub fn record(&self, address: IpAddr) {
        let mut failures = self.by_address.lock().unwrap();
        failures.retain(|_, (since, _)| since.elapsed() <= WINDOW);
        failures.entry(address).or_insert((Instant::now(), 0)).1 += 1;
    }
}

/// Where a request came from: the last address the proxy in front wrote, else the connection's.
pub fn caller_address(headers: &HeaderMap, connection: SocketAddr) -> IpAddr {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|addresses| addresses.rsplit(',').next())
        .and_then(|address| address.trim().parse().ok())
        .unwrap_or(connection.ip())
}
