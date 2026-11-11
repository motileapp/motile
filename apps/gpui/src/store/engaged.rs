//! What the core needs to count the user's time in the client: whether a window of it is in
//! front, the project on screen, and how long ago the user last used it.

use std::cell::Cell;
use std::time::{Duration, Instant};

use gpui_kit::*;
use motile_core::api::Command;

use super::Store;

/// How often the latest input is told, while focused.
const INPUT_EVERY: Duration = Duration::from_secs(5);

thread_local! {
    static LAST_INPUT: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// The user moved the pointer, clicked, scrolled or pressed a key in a window.
pub fn touched() {
    LAST_INPUT.set(Some(Instant::now()));
}

#[derive(Clone, PartialEq, Default)]
struct Focus {
    focused: bool,
    server_id: Option<String>,
    project_id: Option<String>,
}

#[derive(Default)]
pub struct Engaged {
    sent: Option<Focus>,
    /// The input last told to the core.
    told: Option<Instant>,
    ticks: Option<Task<()>>,
}

impl Store {
    /// Tells the core when whether the client is focused, or the project it shows, changed.
    pub(super) fn follow_focus(&mut self, cx: &mut Context<Self>) {
        let focus = self.focus();
        if self.engaged.sent.as_ref() == Some(&focus) {
            return;
        }
        let command = Command::Focus {
            focused: focus.focused,
            server_id: focus.server_id.clone(),
            project_id: focus.project_id.clone(),
        };
        if self.send(command) == 0 {
            return;
        }
        let gained = focus.focused && !self.engaged.sent.as_ref().is_some_and(|sent| sent.focused);
        let focused = focus.focused;
        self.engaged.sent = Some(focus);
        if !focused {
            self.engaged.ticks = None;
            return;
        }
        if !gained {
            return;
        }
        self.engaged.told = LAST_INPUT.get();
        self.engaged.ticks = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(INPUT_EVERY).await;
                if this.update(cx, |store, _| store.tell_input()).is_err() {
                    break;
                }
            }
        }));
    }

    fn focus(&self) -> Focus {
        let focused = self.app_active;
        let routed = self.settings.section.is_some() || self.shows_usage;
        let shown = if routed {
            None
        } else if let Some(thread) = self.selected_thread() {
            Some((Some(thread.server_id.clone()), Some(thread.project_id.clone())))
        } else {
            self.selected_draft()
                .map(|draft| (self.server_of(draft).map(|server| server.id.clone()), draft.project_id.clone()))
        };
        let (server_id, project_id) = shown.unwrap_or_default();
        Focus { focused, server_id, project_id }
    }

    fn tell_input(&mut self) {
        let Some(last) = LAST_INPUT.get() else { return };
        if self.engaged.told == Some(last) {
            return;
        }
        self.engaged.told = Some(last);
        self.send(Command::Input { idle_secs: last.elapsed().as_secs_f64() });
    }
}
