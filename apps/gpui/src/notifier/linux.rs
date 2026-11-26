//! The desktop's notifications (org.freedesktop.Notifications) and the count on the app's icon
//! in the dock (com.canonical.Unity.LauncherEntry), over the session bus, on threads of their own.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use futures::channel::mpsc::UnboundedSender;
use zbus::blocking::{Connection, MessageIterator};
use zbus::zvariant::Value;

use super::APP_ID;

const NOTIFICATIONS: &str = "org.freedesktop.Notifications";
const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";
/// The action of a click on the notification itself.
const OPEN: &str = "default";
/// The sound of the freedesktop sound theme for a message, as GNOME and KDE play.
const SOUND: &str = "message-new-instant";

enum Op {
    Show { thread_id: String, title: String, body: String, sound: bool },
    Close(String),
    Badge(usize),
}

/// The thread each notification on screen is of, by the id the desktop gave it.
type Threads = Arc<Mutex<HashMap<u32, String>>>;

pub struct Desktop {
    ops: Sender<Op>,
}

impl Desktop {
    /// `clicked` hears the thread of each notification that is clicked.
    pub fn start(clicked: UnboundedSender<String>) -> Self {
        let (ops, received) = channel();
        let started = std::thread::Builder::new().name("notifications".into()).spawn(move || run(received, clicked));
        if let Err(error) = started {
            tracing::warn!("notifications can't be shown: {error}");
        }
        Self { ops }
    }

    pub fn show(&self, thread_id: String, title: String, body: String, sound: bool) {
        let _ = self.ops.send(Op::Show { thread_id, title, body, sound });
    }

    pub fn close(&self, thread_id: String) {
        let _ = self.ops.send(Op::Close(thread_id));
    }

    pub fn badge(&self, count: usize) {
        let _ = self.ops.send(Op::Badge(count));
    }
}

fn run(ops: Receiver<Op>, clicked: UnboundedSender<String>) {
    let bus = match Connection::session() {
        Ok(bus) => bus,
        Err(error) => return tracing::warn!("notifications can't be shown without the session bus: {error}"),
    };
    let threads = Threads::default();
    listen(&bus, threads.clone(), clicked);
    let mut shown: HashMap<String, u32> = HashMap::new();
    for op in ops {
        let done = match op {
            Op::Show { thread_id, title, body, sound } => {
                let replaced = shown.get(&thread_id).copied().unwrap_or(0);
                notify(&bus, replaced, &title, &body, sound).map(|id| {
                    let mut threads = threads.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    threads.remove(&replaced);
                    threads.insert(id, thread_id.clone());
                    shown.insert(thread_id, id);
                })
            }
            Op::Close(thread_id) => {
                let Some(id) = shown.remove(&thread_id) else { continue };
                threads.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&id);
                bus.call_method(Some(NOTIFICATIONS), NOTIFICATIONS_PATH, Some(NOTIFICATIONS), "CloseNotification", &id)
                    .map(drop)
            }
            Op::Badge(count) => badge(&bus, count),
        };
        if let Err(error) = done {
            tracing::warn!("the desktop refused a notification: {error}");
        }
    }
}

fn notify(bus: &Connection, replaced: u32, title: &str, body: &str, sound: bool) -> zbus::Result<u32> {
    let mut hints: HashMap<&str, Value> = HashMap::from([("desktop-entry", Value::from(APP_ID))]);
    match sound {
        true => hints.insert("sound-name", Value::from(SOUND)),
        false => hints.insert("suppress-sound", Value::from(true)),
    };
    let actions = vec![OPEN, "Open"];
    let body = ("Motile", replaced, APP_ID, title, body, actions, hints, -1i32);
    let reply = bus.call_method(Some(NOTIFICATIONS), NOTIFICATIONS_PATH, Some(NOTIFICATIONS), "Notify", &body)?;
    reply.body().deserialize::<u32>()
}

/// Hears the clicks on the notifications, on a thread of its own.
fn listen(bus: &Connection, threads: Threads, clicked: UnboundedSender<String>) {
    let rule = format!("type='signal',interface='{NOTIFICATIONS}',member='ActionInvoked'");
    let messages = match MessageIterator::for_match_rule(rule.as_str(), bus, None) {
        Ok(messages) => messages,
        Err(error) => return tracing::warn!("clicks on notifications won't open their threads: {error}"),
    };
    let _ = std::thread::Builder::new().name("notification-clicks".into()).spawn(move || {
        for message in messages.flatten() {
            let Ok((id, action)) = message.body().deserialize::<(u32, String)>() else { continue };
            if action != OPEN {
                continue;
            }
            let thread_id = threads.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&id).cloned();
            let Some(thread_id) = thread_id else { continue };
            if clicked.unbounded_send(thread_id).is_err() {
                break;
            }
        }
    });
}

/// The count docks show on the app's icon, which they find by its desktop entry.
fn badge(bus: &Connection, count: usize) -> zbus::Result<()> {
    let properties: HashMap<&str, Value> =
        HashMap::from([("count", Value::from(count as i64)), ("count-visible", Value::from(count > 0))]);
    let app = format!("application://{APP_ID}.desktop");
    bus.emit_signal(
        None::<&str>,
        "/app/motile/motile",
        "com.canonical.Unity.LauncherEntry",
        "Update",
        &(app, properties),
    )
}

/// The freedesktop sound theme's message sound, through libcanberra where the desktop has it.
pub fn play_sound() {
    let _ = std::thread::Builder::new().name("alert-sound".into()).spawn(|| {
        let _ = std::process::Command::new("canberra-gtk-play").args(["-i", SOUND]).status();
    });
}
