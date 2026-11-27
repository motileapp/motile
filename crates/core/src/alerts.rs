//! The alerts the servers send, as this device shows them: its notification settings, how each
//! alert shows given where the user is, the badge, and the phone's push registration.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

use motile_protocol::alerts::{
    AlertChoices, AlertKind, ENDED_WORK_STAYS_SECS, PushKey, Pushed, ServerWork, Started, WorkItem, WorkState,
};
use motile_protocol::auth_api::{PushRegistration, PushService};
use motile_protocol::identity::DeviceKey;
use motile_protocol::wire::Thread;
use serde::{Deserialize, Serialize};

const FILE: &str = "notifications.json";
const REMEMBERED_ALERTS: usize = 200;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Off,
    #[default]
    Notifications,
    Sound,
    NotificationsAndSound,
}

impl Mode {
    fn notifies(self) -> bool {
        matches!(self, Self::Notifications | Self::NotificationsAndSound)
    }

    fn sounds(self) -> bool {
        matches!(self, Self::Sound | Self::NotificationsAndSound)
    }
}

/// This device's notification settings.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct Settings {
    /// On a phone, the system shows its pushes: `off` stops them and the others keep them.
    pub mode: Mode,
    /// While the client is in front and shows another thread, an alert shows inside it. Off by
    /// default, as it interrupts.
    pub toasts: bool,
    pub choices: AlertChoices,
    /// A phone shows what the agents do while they work: a Live Activity, or an ongoing notification.
    pub live: bool,
}

impl Settings {
    pub fn load(data_dir: &Path) -> Self {
        std::fs::read(data_dir.join(FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) {
        let Ok(bytes) = serde_json::to_vec_pretty(self) else { return };
        if let Err(error) = std::fs::write(data_dir.join(FILE), bytes) {
            tracing::warn!("couldn't keep the notification settings: {error}");
        }
    }
}

/// How an alert shows.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shown {
    /// A notification of the system.
    pub system: bool,
    pub sound: bool,
    /// A toast inside the client, which opens the thread.
    pub toast: bool,
}

/// Where the user is when an alert comes.
pub struct Where {
    /// The client is focused: in front, with a window of it the user can see.
    pub focused: bool,
    /// The alert's thread is on screen.
    pub on_thread: bool,
    /// A phone or a tablet, whose system shows the pushes while the client isn't in front.
    pub pushed: bool,
}

pub fn shown(settings: &Settings, kind: AlertKind, at: &Where) -> Option<Shown> {
    if !settings.choices.allows(kind) || (at.focused && at.on_thread) || (at.pushed && !at.focused) {
        return None;
    }
    let sound = settings.mode.sounds();
    let shown = match (at.focused, at.pushed) {
        (true, true) => Shown { system: false, sound: false, toast: settings.toasts },
        (true, false) => Shown { system: false, sound, toast: settings.toasts },
        (false, _) => Shown { system: settings.mode.notifies(), sound, toast: false },
    };
    (shown.system || shown.sound || shown.toast).then_some(shown)
}

/// The alerts already shown, so that one heard twice shows once.
#[derive(Default)]
pub struct Heard {
    order: VecDeque<String>,
    ids: HashSet<String>,
}

impl Heard {
    /// Whether the alert is new.
    pub fn first_time(&mut self, id: &str) -> bool {
        if !self.ids.insert(id.to_string()) {
            return false;
        }
        self.order.push_back(id.to_string());
        if self.order.len() > REMEMBERED_ALERTS
            && let Some(oldest) = self.order.pop_front()
        {
            self.ids.remove(&oldest);
        }
        true
    }
}

/// The threads alerted while the user was away from the client, which its icon counts until the
/// client is in front again.
#[derive(Default)]
pub struct Badge {
    alerted: HashSet<String>,
}

impl Badge {
    pub fn count(&self) -> usize {
        self.alerted.len()
    }

    /// Whether the count changed.
    pub fn alerted(&mut self, thread_id: &str) -> bool {
        self.alerted.insert(thread_id.to_string())
    }

    /// Whether the count changed.
    pub fn clear(&mut self) -> bool {
        let counted = !self.alerted.is_empty();
        self.alerted.clear();
        counted
    }
}

const WORK_TITLE_CHARS: usize = 60;

/// What a server's agents are doing, for the widgets: the top-level threads that wait for the
/// user, then those that work or whose agents do, then those whose turn ended in the last 15
/// minutes, the latest first.
pub fn work_of<'a>(
    server_id: &str,
    server_name: &str,
    threads: impl Iterator<Item = &'a Thread> + Clone,
    project_name: impl Fn(&str) -> String,
    now: f64,
) -> ServerWork {
    let started = Started::of(threads.clone());
    let items: Vec<WorkItem> = threads
        .filter(|thread| thread.parent.is_none() && thread.archived_at.is_none())
        .filter_map(|thread| {
            let (state, since) = match () {
                _ if thread.needs_approval => (WorkState::Waiting, thread.updated_at),
                _ if thread.running || thread.monitoring || started.agents_work(thread) => {
                    (WorkState::Working, thread.updated_at)
                }
                _ => match thread.turn_ended_at {
                    Some(ended) if now - ended < ENDED_WORK_STAYS_SECS => (WorkState::Finished, ended),
                    _ => return None,
                },
            };
            let title = thread.title.chars().take(WORK_TITLE_CHARS).collect();
            Some(WorkItem {
                thread_id: thread.id.clone(),
                title,
                project: project_name(&thread.project_id),
                state,
                since,
            })
        })
        .collect();
    ServerWork::new(server_id.to_string(), server_name.to_string(), items, now)
}

/// What the phone gave for its pushes, kept to send again whenever the settings change.
#[derive(Deserialize, Clone, PartialEq, Debug)]
pub struct PushDevice {
    pub service: PushService,
    pub token: String,
    #[serde(default)]
    pub live_start_token: Option<String>,
    #[serde(default)]
    pub live_token: Option<String>,
}

pub fn registration(device: &PushDevice, settings: &Settings) -> PushRegistration {
    let notifies = settings.mode != Mode::Off;
    PushRegistration {
        service: device.service.clone(),
        token: device.token.clone(),
        choices: match notifies {
            true => AlertChoices { sound: settings.mode.sounds(), ..settings.choices },
            false => AlertChoices {
                finished: false,
                failed: false,
                approval: false,
                question: false,
                limit: false,
                sound: false,
            },
        },
        live: settings.live,
        live_start_token: device.live_start_token.clone(),
        live_token: device.live_token.clone(),
    }
}

/// Opens what was pushed to this device, with the device key in `data_dir`, without the core
/// running: for a phone's push handler, which may run before the app does.
pub fn open_push(data_dir: &Path, sealed: &str) -> Option<Pushed> {
    let key = DeviceKey::load_or_create(&key_path(data_dir)).ok()?;
    PushKey::of_device(&key).open_pushed(sealed)
}

/// The device key as an X25519 secret, in base64.
pub fn push_secret(key: &DeviceKey) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(PushKey::of_device(key).to_bytes())
}

pub fn key_path(data_dir: &Path) -> PathBuf {
    data_dir.join("device.key")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(focused: bool, on_thread: bool) -> Where {
        Where { focused, on_thread, pushed: false }
    }

    #[test]
    fn an_alert_shows_by_where_the_user_is() {
        let settings = Settings { mode: Mode::NotificationsAndSound, toasts: true, ..Settings::default() };
        let away = shown(&settings, AlertKind::Finished, &at(false, false)).unwrap();
        assert_eq!(away, Shown { system: true, sound: true, toast: false });
        let elsewhere = shown(&settings, AlertKind::Finished, &at(true, false)).unwrap();
        assert_eq!(elsewhere, Shown { system: false, sound: true, toast: true });
        let by_default = shown(&Settings::default(), AlertKind::Finished, &at(false, false));
        assert_eq!(by_default, Some(Shown { system: true, sound: false, toast: false }));
        assert_eq!(shown(&settings, AlertKind::Finished, &at(true, true)), None);
        let unfocused_on_thread = shown(&settings, AlertKind::Approval, &at(false, true)).unwrap();
        assert!(unfocused_on_thread.system);
    }

    #[test]
    fn settings_and_phones_silence_alerts() {
        let toasts = Settings { toasts: true, ..Settings::default() };
        let phone_away = Where { focused: false, on_thread: false, pushed: true };
        assert_eq!(shown(&toasts, AlertKind::Approval, &phone_away), None);
        let phone_in_front = Where { focused: true, on_thread: false, pushed: true };
        assert_eq!(
            shown(&toasts, AlertKind::Approval, &phone_in_front),
            Some(Shown { system: false, sound: false, toast: true })
        );
        assert_eq!(shown(&Settings::default(), AlertKind::Approval, &phone_in_front), None);
        let phone_on_thread = Where { focused: true, on_thread: true, pushed: true };
        assert_eq!(shown(&toasts, AlertKind::Approval, &phone_on_thread), None);
        let off = Settings { mode: Mode::Off, toasts: false, ..Settings::default() };
        assert_eq!(shown(&off, AlertKind::Approval, &at(false, false)), None);
        assert_eq!(shown(&off, AlertKind::Approval, &at(true, false)), None);
        let sound_only = Settings { mode: Mode::Sound, ..Settings::default() };
        assert_eq!(
            shown(&sound_only, AlertKind::Failed, &at(false, false)),
            Some(Shown { system: false, sound: true, toast: false })
        );
        let no_finished =
            Settings { choices: AlertChoices { finished: false, ..AlertChoices::default() }, ..Settings::default() };
        assert_eq!(shown(&no_finished, AlertKind::Finished, &at(false, false)), None);
    }

    #[test]
    fn the_badge_counts_the_threads_alerted_while_away_until_the_client_is_in_front() {
        let mut badge = Badge::default();
        assert!(badge.alerted("a"));
        assert!(!badge.alerted("a"), "a thread counts once");
        assert!(badge.alerted("b"));
        assert_eq!(badge.count(), 2);
        assert!(badge.clear());
        assert_eq!(badge.count(), 0);
        assert!(!badge.clear());
    }

    #[test]
    fn an_alert_heard_twice_shows_once() {
        let mut heard = Heard::default();
        assert!(heard.first_time("a"));
        assert!(!heard.first_time("a"));
        for index in 0..REMEMBERED_ALERTS {
            heard.first_time(&index.to_string());
        }
        assert!(heard.first_time("a"));
    }

    #[test]
    fn turning_notifications_off_on_a_phone_asks_for_no_alerts() {
        let device =
            PushDevice { service: PushService::Fcm, token: "t".into(), live_start_token: None, live_token: None };
        let off = registration(&device, &Settings { mode: Mode::Off, live: true, ..Settings::default() });
        assert!(!off.choices.allows(AlertKind::Approval));
        assert!(off.live);
        let on = registration(&device, &Settings::default());
        assert!(on.choices.allows(AlertKind::Finished));
        assert!(!on.choices.sound);
        let loud = registration(&device, &Settings { mode: Mode::NotificationsAndSound, ..Settings::default() });
        assert!(loud.choices.sound);
    }

    #[test]
    fn a_push_opens_with_the_device_key_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let key = DeviceKey::load_or_create(&key_path(dir.path())).unwrap();
        let pushed = Pushed::Work(Default::default());
        let sealed = motile_protocol::alerts::seal_pushed(&key.public(), &pushed).unwrap();
        assert_eq!(open_push(dir.path(), &sealed), Some(pushed));
    }

    fn thread(id: &str, title: &str) -> Thread {
        serde_json::from_value(serde_json::json!({
            "id": id, "title": title, "project_id": "p", "cwd": "/", "agent": "claude", "model": null,
            "effort": null, "access": "full", "created_at": 0.0, "updated_at": 100.0, "done_at": null,
            "running": false, "needs_approval": false, "turn_ended_at": null, "rev": 0
        }))
        .unwrap()
    }

    #[test]
    fn work_lists_what_waits_then_works_then_ended_lately() {
        let waiting = Thread { needs_approval: true, running: true, ..thread("w", "Waits") };
        let working = Thread { running: true, ..thread("r", "Works") };
        let ended = Thread { turn_ended_at: Some(950.0), ..thread("e", "Ended") };
        let long_ago = Thread { turn_ended_at: Some(1.0), ..thread("o", "Long ago") };
        let child = Thread {
            running: true,
            parent: Some(motile_protocol::wire::ThreadParent { thread_id: "r".into(), tool_id: None }),
            ..thread("c", "Child")
        };
        let archived = Thread { running: true, archived_at: Some(5.0), ..thread("a", "Archived") };
        let delegated = Thread { turn_ended_at: Some(950.0), updated_at: 2.0, ..thread("d", "Delegates") };
        let delegate = Thread {
            running: true,
            parent: Some(motile_protocol::wire::ThreadParent { thread_id: "d".into(), tool_id: None }),
            ..thread("dc", "Delegate")
        };
        let threads = [ended, long_ago, child, archived, working, waiting, delegated, delegate];
        let work = work_of("s", "studio", threads.iter(), |_| "motile".into(), 1000.0);
        let listed: Vec<(&str, WorkState)> =
            work.items.iter().map(|item| (item.thread_id.as_str(), item.state)).collect();
        assert_eq!(
            listed,
            [
                ("w", WorkState::Waiting),
                ("r", WorkState::Working),
                ("d", WorkState::Working),
                ("e", WorkState::Finished)
            ]
        );
        assert_eq!((work.working, work.waiting), (2, 1));
        assert_eq!(work.items[0].project, "motile");
    }

    #[test]
    fn settings_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(dir.path()), Settings::default());
        let settings = Settings { mode: Mode::Sound, toasts: false, live: false, ..Settings::default() };
        settings.save(dir.path());
        assert_eq!(Settings::load(dir.path()), settings);
    }
}
