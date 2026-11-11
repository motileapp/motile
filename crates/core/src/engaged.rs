//! The time the user spends in the client: while it is focused, and they used it in the last five
//! minutes. A pause shorter than that counts, as reading or thinking does; after a longer one the
//! stretch ends at their last input, so time away is never counted. Leaving the client ends it at
//! once. Each stretch is on the project on screen, and is kept until that project's server has it.

use motile_protocol::wire::EngagedRecord;
use serde::{Deserialize, Serialize};

use crate::cache::Cache;

/// The longest pause that still counts.
pub const IDLE: f64 = 300.0;
/// A stretch shorter than this isn't kept.
const SHORTEST: f64 = 1.0;
/// The first protocol whose servers keep stretches.
pub const ENGAGED_PROTOCOL: u32 = 41;
/// The most stretches kept for servers that don't take them; the oldest go first.
const MOST_WAITING: usize = 5_000;
/// The most stretches sent in one request.
pub const BATCH: usize = 500;
const OPEN_KEY: &str = "engaged.open";
const WAITING_KEY: &str = "engaged.waiting";

/// What is on screen: a project of a server, or nothing of one (both empty).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Place {
    pub server_id: String,
    pub project_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Stretch {
    pub place: Place,
    pub started_at: f64,
    pub ended_at: f64,
}

impl Stretch {
    pub fn record(&self, device: &str) -> EngagedRecord {
        EngagedRecord {
            device: device.to_string(),
            project_id: self.place.project_id.clone(),
            started_at: self.started_at,
            ended_at: self.ended_at,
        }
    }
}

#[derive(Default)]
pub struct Engagement {
    focused: bool,
    place: Place,
    last_input: f64,
    open: Option<Stretch>,
    /// The stretches that ended and their servers don't have yet.
    waiting: Vec<Stretch>,
    /// The open stretch changed since it was last kept.
    unsaved: bool,
}

impl Engagement {
    /// What the cache kept. A stretch that was open when the client stopped ends where it was
    /// last counted to.
    pub fn load(cache: &Cache) -> Self {
        let mut engagement = Self { waiting: cache.kept(WAITING_KEY).unwrap_or_default(), ..Self::default() };
        engagement.open = cache.kept::<Option<Stretch>>(OPEN_KEY).flatten();
        engagement.close();
        engagement.save(cache);
        engagement
    }

    /// Whether the client is focused, and what it shows.
    pub fn focus(&mut self, now: f64, focused: bool, place: Place) {
        self.settle(now);
        let gained = focused && !self.focused;
        let moved = place != self.place;
        let was_open = self.open.is_some();
        if !focused || moved {
            self.close();
        }
        self.focused = focused;
        self.place = place;
        if gained {
            self.last_input = now;
            self.start(now);
        } else if focused && moved && was_open {
            self.start(now);
        }
    }

    /// The user used the client `idle_secs` before `now`.
    pub fn input(&mut self, now: f64, idle_secs: f64) {
        let at = now - idle_secs.clamp(0.0, IDLE * 2.0);
        if !self.focused || at <= self.last_input {
            return;
        }
        match self.open.as_mut() {
            Some(open) if at - self.last_input <= IDLE => {
                open.ended_at = open.ended_at.max(at);
                self.unsaved = true;
            }
            _ => {
                self.close();
                self.start(at);
            }
        }
        self.last_input = at;
    }

    /// Ends the open stretch once the user has been away longer than a pause.
    pub fn tick(&mut self, now: f64) {
        if self.open.is_some() && now - self.last_input > IDLE {
            self.close();
        }
    }

    /// The open stretch, counted up to `now` if the user is still there.
    pub fn open(&self, now: f64) -> Option<Stretch> {
        let mut open = self.open.clone()?;
        if self.focused && now - self.last_input <= IDLE {
            open.ended_at = open.ended_at.max(now);
        }
        Some(open)
    }

    pub fn waiting(&self) -> &[Stretch] {
        &self.waiting
    }

    /// Takes the stretches out that a server now has, by when they started.
    pub fn delivered(&mut self, cache: &Cache, started: &[f64]) {
        self.waiting.retain(|stretch| !started.contains(&stretch.started_at));
        cache.keep(WAITING_KEY, &self.waiting);
    }

    /// Drops the stretches no server will take: those of servers that are no longer the account's.
    pub fn forget(&mut self, cache: &Cache, keeps: impl Fn(&str) -> bool) {
        let before = self.waiting.len();
        self.waiting.retain(|stretch| stretch.place.server_id.is_empty() || keeps(&stretch.place.server_id));
        if self.waiting.len() != before {
            cache.keep(WAITING_KEY, &self.waiting);
        }
    }

    /// Keeps what changed. True when a stretch ended since, for its server to be sent it.
    pub fn save_if_changed(&mut self, cache: &Cache, ended: usize) -> bool {
        let ended = self.waiting.len() != ended;
        if ended {
            cache.keep(WAITING_KEY, &self.waiting);
        }
        if ended || self.unsaved {
            cache.keep(OPEN_KEY, &self.open);
            self.unsaved = false;
        }
        ended
    }

    fn save(&self, cache: &Cache) {
        cache.keep(WAITING_KEY, &self.waiting);
        cache.keep(OPEN_KEY, &self.open);
    }

    fn settle(&mut self, now: f64) {
        if !self.focused || self.open.is_none() {
            return;
        }
        if now - self.last_input > IDLE {
            self.close();
            return;
        }
        if let Some(open) = self.open.as_mut() {
            open.ended_at = open.ended_at.max(now);
            self.unsaved = true;
        }
    }

    fn start(&mut self, at: f64) {
        self.open = Some(Stretch { place: self.place.clone(), started_at: at, ended_at: at });
        self.unsaved = true;
    }

    fn close(&mut self) {
        let Some(open) = self.open.take() else { return };
        self.unsaved = true;
        if open.ended_at - open.started_at >= SHORTEST {
            self.waiting.push(open);
        }
        if self.waiting.len() > MOST_WAITING {
            self.waiting.drain(..self.waiting.len() - MOST_WAITING);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(project: &str) -> Place {
        Place { server_id: "studio".into(), project_id: project.into() }
    }

    fn kept(engagement: &Engagement) -> Vec<(&str, f64, f64)> {
        engagement
            .waiting
            .iter()
            .map(|stretch| (stretch.place.project_id.as_str(), stretch.started_at, stretch.ended_at))
            .collect()
    }

    #[test]
    fn a_short_pause_counts_and_leaving_ends_the_stretch_at_once() {
        let mut engagement = Engagement::default();
        engagement.focus(100.0, true, place("p"));
        engagement.input(110.0, 0.0);
        engagement.input(290.0, 0.0);
        engagement.focus(400.0, false, place("p"));

        assert_eq!(kept(&engagement), [("p", 100.0, 400.0)]);
    }

    #[test]
    fn a_long_pause_ends_the_stretch_at_the_last_input_and_the_next_input_starts_another() {
        let mut engagement = Engagement::default();
        engagement.focus(100.0, true, place("p"));
        engagement.input(160.0, 10.0);
        engagement.tick(400.0);
        assert!(engagement.open(400.0).is_some());
        engagement.tick(460.0);
        assert_eq!(kept(&engagement), [("p", 100.0, 150.0)]);

        engagement.input(1_000.0, 0.0);
        engagement.input(1_030.0, 0.0);
        engagement.focus(1_040.0, false, place("p"));
        assert_eq!(kept(&engagement), [("p", 100.0, 150.0), ("p", 1_000.0, 1_040.0)]);
    }

    #[test]
    fn coming_back_after_a_sleep_counts_nothing_of_it() {
        let mut engagement = Engagement::default();
        engagement.focus(100.0, true, place("p"));
        engagement.input(200.0, 0.0);
        engagement.input(5_000.0, 0.0);
        engagement.input(5_010.0, 0.0);

        assert_eq!(kept(&engagement), [("p", 100.0, 200.0)]);
        assert_eq!(engagement.open(5_020.0).map(|open| (open.started_at, open.ended_at)), Some((5_000.0, 5_020.0)));
    }

    #[test]
    fn another_project_on_screen_starts_another_stretch() {
        let mut engagement = Engagement::default();
        engagement.focus(100.0, true, place("p"));
        engagement.input(150.0, 0.0);
        engagement.focus(160.0, true, place("q"));
        engagement.input(200.0, 0.0);
        engagement.focus(210.0, true, Place::default());
        engagement.focus(260.0, false, Place::default());

        assert_eq!(kept(&engagement), [("p", 100.0, 160.0), ("q", 160.0, 210.0), ("", 210.0, 260.0)]);
    }

    #[test]
    fn nothing_counts_while_the_client_isnt_focused_or_the_clock_goes_back() {
        let mut engagement = Engagement::default();
        engagement.input(100.0, 0.0);
        engagement.focus(200.0, true, place("p"));
        engagement.input(150.0, 0.0);
        engagement.focus(200.5, false, place("p"));

        assert!(kept(&engagement).is_empty());
        assert!(engagement.open(300.0).is_none());
    }
}
