//! Whether the client was away: in the background, or paused by the system, asleep or suspended.
//! What was open or dialing then can't be trusted, and its end isn't an outage: coming back, the
//! links dial again, and only what is dialed once the client is back says whether a server is
//! reachable. Otherwise every return after a while would flash the connection that died meanwhile.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// A gap this long between two runs says the system had the client paused.
pub const STALE_AFTER: Duration = Duration::from_secs(10);
/// How often the client is seen running while nothing else asks, so that only a pause leaves a gap.
const BEAT: Duration = Duration::from_secs(1);
/// How long after the client is back a connection or a dial from before still ends quietly. The
/// one that lived through the pause ends at once, as its timers all fire on waking.
const SETTLING: Duration = Duration::from_secs(15);

/// Shared by the core, which says when the client goes to the background and comes back, and the
/// links, which ask it whether a connection that ended is worth saying so.
#[derive(Clone)]
pub struct Away(Arc<Mutex<Inner>>);

struct Inner {
    /// When the client was last seen running.
    ran_at: SystemTime,
    background_since: Option<SystemTime>,
    /// When the client last came back from a pause or the background.
    back_at: Option<SystemTime>,
    /// The longest pause or stay in the background the core hasn't taken yet.
    untaken: Duration,
}

impl Default for Away {
    fn default() -> Self {
        Self::new()
    }
}

impl Away {
    pub fn new() -> Self {
        let away = Self(Arc::new(Mutex::new(Inner::new(SystemTime::now()))));
        // A thread of its own, which nothing else keeps busy and which stops when the system
        // pauses the client, as everything does.
        let beating = Arc::downgrade(&away.0);
        let _ = std::thread::Builder::new().name("away".to_string()).spawn(move || {
            loop {
                std::thread::sleep(BEAT);
                let Some(inner) = beating.upgrade() else { break };
                Self(inner).with(|_, _| ());
            }
        });
        away
    }

    fn with<T>(&self, act: impl FnOnce(&mut Inner, SystemTime) -> T) -> T {
        let mut inner = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = SystemTime::now();
        inner.ran(now);
        act(&mut inner, now)
    }

    pub fn backgrounded(&self) {
        self.with(|inner, now| inner.backgrounded(now));
    }

    pub fn foregrounded(&self) {
        self.with(|inner, now| inner.foregrounded(now));
    }

    /// How long the client was away since this was last asked: the longest pause or stay in the
    /// background. Zero when it didn't go anywhere.
    pub fn take(&self) -> Duration {
        self.with(|inner, _| std::mem::take(&mut inner.untaken))
    }

    /// Whether a connection or a dial that started at `started` and has just ended went because
    /// the client was away, which isn't worth saying.
    pub fn excuses(&self, started: SystemTime) -> bool {
        self.with(|inner, now| inner.excuses(started, now))
    }
}

impl Inner {
    fn new(now: SystemTime) -> Self {
        Self { ran_at: now, background_since: None, back_at: None, untaken: Duration::ZERO }
    }

    fn ran(&mut self, now: SystemTime) {
        let gap = now.duration_since(self.ran_at).unwrap_or_default();
        self.ran_at = now;
        if gap >= STALE_AFTER {
            self.back_at = Some(now);
            self.untaken = self.untaken.max(gap);
        }
    }

    fn backgrounded(&mut self, now: SystemTime) {
        self.background_since.get_or_insert(now);
    }

    fn foregrounded(&mut self, now: SystemTime) {
        let Some(since) = self.background_since.take() else { return };
        self.back_at = Some(now);
        self.untaken = self.untaken.max(now.duration_since(since).unwrap_or_default());
    }

    fn excuses(&self, started: SystemTime, now: SystemTime) -> bool {
        self.background_since.is_some()
            || self
                .back_at
                .is_some_and(|back| started < back && now.duration_since(back).unwrap_or_default() < SETTLING)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    /// Runs `inner` every two seconds, as the core's tick does, from `from` to `to`.
    fn tick(inner: &mut Inner, from: u64, to: u64) {
        for secs in (from..=to).step_by(2) {
            inner.ran(at(secs));
        }
    }

    #[test]
    fn a_connection_that_lived_through_a_pause_ends_quietly_once_back() {
        let mut inner = Inner::new(at(0));
        tick(&mut inner, 0, 100);
        // Suspended from 100 to 400; the connection's timers fire the moment it runs again.
        inner.ran(at(400));
        assert!(inner.excuses(at(50), at(400)));
        assert_eq!(std::mem::take(&mut inner.untaken), Duration::from_secs(300));
    }

    #[test]
    fn a_connection_that_ends_without_a_pause_says_why() {
        let mut inner = Inner::new(at(0));
        tick(&mut inner, 0, 100);
        assert!(!inner.excuses(at(50), at(100)));
        assert_eq!(inner.untaken, Duration::ZERO);
    }

    #[test]
    fn a_dial_made_once_back_says_why() {
        let mut inner = Inner::new(at(0));
        tick(&mut inner, 0, 100);
        inner.ran(at(400));
        tick(&mut inner, 400, 404);
        assert!(!inner.excuses(at(401), at(404)));
    }

    #[test]
    fn a_connection_from_before_a_pause_that_ends_long_after_it_says_why() {
        let mut inner = Inner::new(at(0));
        tick(&mut inner, 0, 100);
        inner.ran(at(400));
        tick(&mut inner, 400, 600);
        assert!(!inner.excuses(at(50), at(600)));
    }

    #[test]
    fn nothing_that_ends_in_the_background_is_worth_saying() {
        let mut inner = Inner::new(at(0));
        tick(&mut inner, 0, 100);
        inner.backgrounded(at(100));
        tick(&mut inner, 100, 200);
        assert!(inner.excuses(at(150), at(200)));
        inner.foregrounded(at(200));
        assert!(inner.excuses(at(150), at(201)));
        assert!(!inner.excuses(at(201), at(203)));
        assert_eq!(inner.untaken, Duration::from_secs(100));
    }
}
