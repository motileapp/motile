//! The servers' alerts as this client shows them, as the core decided: a notification of the
//! system, a sound, a toast that opens the thread, and the badge.

use std::time::Duration;

use gpui_kit::prelude::*;
use motile_core::alerts::{Settings, Shown};
use motile_core::api::Command;
use motile_protocol::alerts::{Alert, AlertKind};

use super::{Store, Toast};
use crate::notifier::Notifier;

const TOAST_LASTS: Duration = Duration::from_secs(6);

/// The alert a toast tells of, whose thread its Open button opens, and where that thread is.
#[derive(Clone, PartialEq, Debug)]
pub struct AlertToast {
    pub thread_id: String,
    pub kind: AlertKind,
    pub project: String,
    pub server_name: String,
}

impl Toast {
    fn alert(alert: &Alert, server_name: String) -> Self {
        let told = AlertToast {
            thread_id: alert.thread_id.clone(),
            kind: alert.kind,
            project: alert.project.clone(),
            server_name,
        };
        Self { alert: Some(told), ..Self::new(alert.thread_title.clone(), None) }
    }
}

impl Store {
    pub fn set_notification_settings(&mut self, settings: Settings) {
        self.notifications = settings;
        self.send(Command::SetNotifications { settings });
    }

    /// What failed or is still under way keeps its toast.
    pub(super) fn present_alert(&mut self, alert: Alert, server_name: String, shown: Shown, cx: &mut Context<Self>) {
        if shown.system {
            self.notifier.show(&alert, shown.sound, cx);
        } else if shown.sound {
            Notifier::play_sound();
        }
        let busy = self.toast.as_ref().is_some_and(|toast| toast.failed || toast.pending);
        if !shown.toast || busy {
            return;
        }
        self.show_toast_for(Toast::alert(&alert, server_name), TOAST_LASTS, cx);
    }

    /// Opens the thread an alert is of, over the settings or the usage, or once it is known.
    pub fn open_alerted_thread(&mut self, thread_id: &str, cx: &mut Context<Self>) {
        self.close_route();
        if !self.threads.contains_key(thread_id) {
            self.last_selection = Some(thread_id.to_string());
            return;
        }
        self.reveal_thread(thread_id.to_string(), cx);
        self.forget_alerts_of(thread_id, cx);
    }

    /// What told of the thread goes once it is open.
    pub(super) fn forget_alerts_of(&mut self, thread_id: &str, cx: &mut Context<Self>) {
        self.notifier.dismiss(thread_id, cx);
        if self.toast.as_ref().and_then(|toast| toast.alert.as_ref()).is_some_and(|alert| alert.thread_id == thread_id)
        {
            self.toast = None;
        }
    }
}
