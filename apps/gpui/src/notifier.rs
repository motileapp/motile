//! What the system shows of an alert: a notification, one per thread, that opens the thread; the
//! system's alert sound; and the count on the app's icon. The Mac and Windows post through GPUI.
//! On Linux the app talks to the desktop over D-Bus itself, as GPUI there neither replaces a
//! thread's notification nor closes one.

#[cfg(target_os = "linux")]
mod linux;

use std::collections::HashSet;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::alerts::Alert;

use crate::store::Store;

/// The id the system knows the app by: its desktop entry on Linux, its AppUserModelID on Windows.
pub const APP_ID: &str = "app.motile.motile";

#[derive(Default)]
pub struct Notifier {
    #[cfg(target_os = "linux")]
    desktop: Option<linux::Desktop>,
    /// The threads whose notifications may still be on screen.
    shown: HashSet<String>,
}

impl Notifier {
    /// Opens the thread of a notification that was clicked.
    pub fn start(cx: &mut Context<Store>) -> Self {
        let store = cx.weak_entity();
        #[cfg(target_os = "linux")]
        {
            let (clicked, mut clicks) = futures::channel::mpsc::unbounded::<String>();
            cx.spawn(async move |_, cx| {
                use futures::StreamExt;
                while let Some(thread_id) = clicks.next().await {
                    cx.update(|cx| open(&store, &thread_id, cx));
                }
            })
            .detach();
            Self { desktop: Some(linux::Desktop::start(clicked)), shown: HashSet::new() }
        }
        #[cfg(not(target_os = "linux"))]
        {
            cx.on_system_notification_response(move |response, cx| open(&store, &response.tag, cx));
            Self::default()
        }
    }

    /// The alert's notification, in place of the one its thread had.
    pub fn show(&mut self, alert: &Alert, sound: bool, cx: &App) {
        self.shown.insert(alert.thread_id.clone());
        let (title, body) = text(alert);
        #[cfg(target_os = "linux")]
        {
            let _ = cx;
            let Some(desktop) = &self.desktop else { return };
            desktop.show(alert.thread_id.clone(), title, body, sound);
        }
        #[cfg(not(target_os = "linux"))]
        {
            cx.show_system_notification(SystemNotification {
                tag: alert.thread_id.clone().into(),
                title: title.into(),
                body: body.into(),
                actions: Vec::new(),
            });
            // GPUI's notifications are silent.
            if sound {
                Self::play_sound();
            }
        }
    }

    pub fn dismiss(&mut self, thread_id: &str, cx: &App) {
        self.shown.remove(thread_id);
        #[cfg(target_os = "linux")]
        {
            let _ = cx;
            let Some(desktop) = &self.desktop else { return };
            desktop.close(thread_id.to_string());
        }
        #[cfg(not(target_os = "linux"))]
        cx.dismiss_system_notification(thread_id);
    }

    /// In front, what came while the app was away has been seen.
    pub fn dismiss_all(&mut self, cx: &App) {
        for thread_id in std::mem::take(&mut self.shown) {
            self.dismiss(&thread_id, cx);
        }
    }

    /// The system's alert sound, once.
    pub fn play_sound() {
        #[cfg(target_os = "linux")]
        linux::play_sound();
        #[cfg(target_os = "macos")]
        objc2_app_kit::NSBeep();
        #[cfg(windows)]
        {
            use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_NODEFAULT};
            let alias: Vec<u16> = "Notification.Default".encode_utf16().chain([0]).collect();
            // SAFETY: the alias is a null-terminated UTF-16 string that outlives the call.
            unsafe { PlaySoundW(alias.as_ptr(), std::ptr::null_mut(), SND_ALIAS | SND_ASYNC | SND_NODEFAULT) };
        }
    }

    /// The count on the app's icon: the Dock's on the Mac, the dock's or the taskbar's on Linux
    /// where it reads Unity's launcher entries, as GNOME's and KDE's do. Windows has none.
    pub fn badge(&mut self, count: usize) {
        #[cfg(target_os = "linux")]
        if let Some(desktop) = &self.desktop {
            desktop.badge(count);
        }
        #[cfg(target_os = "macos")]
        {
            use objc2_app_kit::NSApplication;
            use objc2_foundation::NSString;
            let Some(main) = objc2::MainThreadMarker::new() else { return };
            let label = (count > 0).then(|| NSString::from_str(&count.to_string()));
            NSApplication::sharedApplication(main).dockTile().setBadgeLabel(label.as_deref());
        }
        #[cfg(windows)]
        let _ = count;
    }
}

/// The notification's title and body: the thread's title, and under it its project, then what
/// happened, as the Mac shows its subtitle above its body.
fn text(alert: &Alert) -> (String, String) {
    let lines: Vec<&str> =
        [alert.project.as_str(), alert.body.as_str()].into_iter().filter(|line| !line.trim().is_empty()).collect();
    (alert.thread_title.clone(), lines.join("\n"))
}

/// Brings the app forward with the thread open.
fn open(store: &WeakEntity<Store>, thread_id: &str, cx: &mut App) {
    crate::app_window::bring_forward(cx);
    let _ = store.update(cx, |store, cx| {
        store.open_alerted_thread(thread_id, cx);
        cx.notify();
    });
}

#[cfg(test)]
mod tests {
    use motile_protocol::alerts::{Alert, AlertKind, NewAlert};

    use super::text;

    #[test]
    fn a_notification_names_the_thread_its_project_and_what_happened() {
        let new = |project: &str| NewAlert {
            server_id: "s".into(),
            thread_id: "t".into(),
            kind: AlertKind::Approval,
            thread_title: "Fix the login".into(),
            project: project.into(),
            detail: Some("Run greet.py".into()),
        };
        let alert = Alert::new(new("motile"), "a".into(), 0.);
        assert_eq!(text(&alert), ("Fix the login".into(), "motile\nNeeds your approval · Run greet.py".into()));
        let alert = Alert::new(new(""), "a".into(), 0.);
        assert_eq!(text(&alert).1, "Needs your approval · Run greet.py");
    }
}
