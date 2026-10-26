//! What the keyboard shortcuts' commands do, what their conditions ask about the app, the
//! threads' keys in the sidebar, and what Esc does when nothing focused claims it.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::integrations::Integration;
use super::shortcuts::ShortcutContext;
use super::usage::{UsagePeriod, UsageTab};
use super::{PanelPage, Selection, Store};
use crate::models::{Project, ThreadInfo};
use crate::panel::state::PanelTab;
use crate::theme::Appearance;

/// How long the modifiers of the threads' keys are held before the keys show, so that a quick
/// ⌘C doesn't flash them.
const HINT_DELAY: Duration = Duration::from_millis(200);

impl Store {
    /// The names that hold now for the rules' conditions, with what has the keys: the composer,
    /// or another field to type in.
    pub fn shortcut_context(&self, composer_focus: bool, editable_focus: bool) -> ShortcutContext {
        let mut context = Vec::new();
        if composer_focus {
            context.push("composerFocus");
        }
        if editable_focus {
            context.push("editableFocus");
        }
        if self.selected_thread().is_some() {
            context.push("threadOpen");
            if self.activity.running {
                context.push("turnRunning");
            }
        }
        if self.right_panel.is_open {
            context.push("rightPanelOpen");
        }
        if self.settings.section.is_some() {
            context.push("settingsOpen");
        }
        if self.shows_usage {
            context.push("usagePageOpen");
        }
        context
    }

    /// The threads ⌘1 to ⌘9 and the next and previous thread go through: the sidebar's, as it
    /// lists them, or what its search found.
    pub fn jump_threads(&self) -> Vec<ThreadInfo> {
        if !self.is_searching_threads() {
            return self.active_threads().to_vec();
        }
        self.search_results().into_iter().filter_map(|result| result.thread().cloned()).collect()
    }

    /// Whether there is any thread for the jump keys, without listing them.
    pub fn has_jump_threads(&self) -> bool {
        if !self.is_searching_threads() {
            return !self.active_threads().is_empty();
        }
        let search = self.sidebar.search.trim();
        let found = &self.sidebar.message_search.matches;
        self.active_threads()
            .iter()
            .chain(self.done_threads())
            .any(|thread| found.contains_key(&thread.id) || self.matches(thread, search))
    }

    /// The keys that open the thread at `index` among `jump_threads`, drawn on its row while
    /// they are held.
    pub fn jump_hint(&self, index: usize) -> Option<String> {
        if !self.shows_jump_hints || index >= 9 {
            return None;
        }
        self.shortcuts.label(&format!("thread.jump.{}", index + 1))
    }

    /// The modifiers held changed: once those of the threads' keys are held alone for a moment,
    /// the sidebar shows the keys.
    pub fn hold_modifiers(&mut self, held: Modifiers, cx: &mut Context<Self>) {
        self.jump_hint_task = None;
        let held = Modifiers { function: false, ..held };
        let route_open = self.panel.is_some() || self.settings.section.is_some();
        if !held.modified() || route_open || self.shortcuts.modifiers("thread.jump.1") != Some(held) {
            self.shows_jump_hints = false;
            return;
        }
        self.jump_hint_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HINT_DELAY).await;
            let _ = this.update(cx, |store, cx| {
                store.shows_jump_hints = true;
                cx.notify();
            });
        }));
    }

    /// Runs the command. Whether there was anything for it to do; without, the keys go on to
    /// whatever else takes them. What the window itself does is `keyboard::perform`'s.
    pub fn perform(&mut self, command: &str, cx: &mut Context<Self>) -> bool {
        if let Some(place) = command.strip_prefix("thread.jump.").and_then(|place| place.parse::<usize>().ok()) {
            return (1..=9).contains(&place) && self.show_thread_at(place - 1, cx);
        }
        if let Some(change) = usage_command(command) {
            if !self.shows_usage {
                return false;
            }
            match change {
                UsageChange::Tab(tab) => self.set_usage_tab(tab),
                UsageChange::Period(period) => self.set_usage_period(period),
            }
            return true;
        }
        match command {
            "commandPalette.toggle" => return self.toggle_panel(PanelPage::Commands),
            "threadPicker.toggle" => return self.toggle_panel(PanelPage::Threads),
            "settings.open" => {
                if !self.account.signed_in {
                    return false;
                }
                self.open_settings(Default::default(), None);
            }
            "usage.open" => {
                if !self.account.signed_in {
                    return false;
                }
                self.open_usage();
            }
            "appearance.cycle" => {
                let all = Appearance::ALL;
                let current = self.prefs.get::<Appearance>("appearance").unwrap_or_default();
                let index = all.iter().position(|appearance| *appearance == current).unwrap_or(0);
                self.prefs.set("appearance", all[(index + 1) % all.len()]);
            }
            "sidebar.toggle" => {
                let hidden = !self.prefs.bool("sidebar.hidden");
                self.prefs.set("sidebar.hidden", hidden);
            }
            "chat.new" => {
                if !self.account.signed_in || !self.has_live_servers() {
                    return false;
                }
                self.new_thread(cx);
            }
            "chat.newLocal" => {
                let Some(key) = self.composer_project().map(|project| project.key) else { return false };
                let Some(project) = self.project_to_open(&key).map(|project| project.id.clone()) else { return false };
                self.start_new_thread(Some(project), cx);
            }
            "chat.newWithoutProject" => {
                let Some(project) = self.project_to_open(Project::NO_PROJECT_KEY).map(|project| project.id.clone())
                else {
                    return false;
                };
                self.start_new_thread(Some(project), cx);
            }
            "navigation.back" => return self.go(true, cx),
            "navigation.forward" => return self.go(false, cx),
            "thread.previous" => return self.show_thread_offset(-1, cx),
            "thread.next" => return self.show_thread_offset(1, cx),
            "thread.done" => {
                if self.changeable_thread().is_none() {
                    return false;
                }
                self.toggle_done(cx);
            }
            "thread.undo" => {
                if self.undo.is_none() {
                    return false;
                }
                self.perform_undo(cx);
            }
            "thread.stop" => {
                if !self.activity.busy() || self.changeable_thread().is_none() {
                    return false;
                }
                self.stop_thread(cx);
            }
            "thread.steerQueuedMessage" => {
                let queued = self.changeable_thread().and(self.activity.queued.first());
                let Some(queued) = queued.map(|queued| queued.id.clone()) else { return false };
                self.send_now(&queued);
            }
            "thread.editQueuedMessage" => {
                let queued = self.changeable_thread().and(self.activity.queued.last());
                let Some(queued) = queued.map(|queued| queued.id.clone()) else { return false };
                self.take_back(&queued);
            }
            "pullRequest.copyLink" => {
                let Some(url) = self.shown_pull_request().map(|page| page.url.clone()) else { return false };
                cx.write_to_clipboard(ClipboardItem::new_string(url));
            }
            "pullRequest.copyNumber" => {
                let Some(number) = self.shown_pull_request().map(|page| page.number) else { return false };
                cx.write_to_clipboard(ClipboardItem::new_string(format!("#{number}")));
            }
            "composer.sendAlternate" => {
                if self.changeable_thread().is_none() || !self.activity.running || !self.can_send() {
                    return false;
                }
                self.send_message(true, cx);
            }
            "composer.server" => {
                let copies = self.draft_project_copies();
                let current = self.selected_draft().and_then(|draft| draft.project_id.as_deref());
                let Some(index) = copies.iter().position(|copy| Some(copy.id.as_str()) == current) else {
                    return false;
                };
                let next = copies[(index + 1) % copies.len()].id.clone();
                self.set_new_thread_project(Some(next), cx);
            }
            "composer.branch" => {
                let Some(project) = self.composer_project().filter(|project| self.can_switch_branches(project)) else {
                    return false;
                };
                self.show_branches(&project);
            }
            "rightPanel.toggle" => {
                let open = !self.right_panel.is_open;
                self.set_panel_open(open);
            }
            "rightPanel.toggleMaximized" => {
                if !self.can_maximize_panel() {
                    return false;
                }
                self.toggle_panel_maximized();
            }
            "rightPanel.new" => {
                if self.panel_unavailable().is_some() {
                    return false;
                }
                self.open_blank_tab();
            }
            "rightPanel.nextTab" | "rightPanel.previousTab" => {
                if !self.right_panel.is_open || self.panel_tabs().tabs.len() < 2 {
                    return false;
                }
                self.activate_tab_offset(if command == "rightPanel.nextTab" { 1 } else { -1 });
            }
            "rightPanel.diff" => {
                if self.panel_unavailable().is_some() || !self.panel_target().is_some_and(|target| target.repository) {
                    return false;
                }
                self.show_diff(None, None);
            }
            "rightPanel.files" | "rightPanel.agents" => {
                if self.panel_unavailable().is_some() {
                    return false;
                }
                self.open_tab(if command == "rightPanel.files" { PanelTab::Files } else { PanelTab::Agents });
            }
            "rightPanel.pullRequest" | "rightPanel.pullRequests" => {
                let all = command == "rightPanel.pullRequests";
                if self.panel_unavailable().is_some()
                    || self.pull_requests_unavailable().is_some()
                    || (all && !self.pull_requests_extended())
                {
                    return false;
                }
                self.open_tab(if all { PanelTab::PullRequests } else { PanelTab::PullRequest });
            }
            "rightPanel.linear" => {
                if self.panel_unavailable().is_some()
                    || !self.is_on(Integration::Linear)
                    || self.linear_unavailable().is_some()
                {
                    return false;
                }
                self.open_tab(PanelTab::Linear);
            }
            "rightPanel.ricrac" => {
                if self.panel_unavailable().is_some()
                    || !self.is_on(Integration::Ricrac)
                    || self.ricrac_unavailable().is_some()
                {
                    return false;
                }
                self.open_tab(PanelTab::Ricrac);
            }
            _ => return false,
        }
        true
    }

    /// Opens the command panel on the page, or closes it when it is already there.
    fn toggle_panel(&mut self, page: PanelPage) -> bool {
        if self.panel.as_ref() == Some(&page) {
            self.close_panel();
            return true;
        }
        if !self.account.signed_in || self.servers.is_empty() {
            return false;
        }
        self.open_panel(page);
        true
    }

    fn show_thread_at(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.jump_threads().get(index).map(|thread| thread.id.clone()) else { return false };
        self.select(Selection::Thread(id), cx);
        true
    }

    /// Opens the thread `offset` places from the open one in the sidebar. From a draft, the
    /// next is the first and the previous the last.
    fn show_thread_offset(&mut self, offset: isize, cx: &mut Context<Self>) -> bool {
        let threads = self.jump_threads();
        if threads.is_empty() {
            return false;
        }
        let last = threads.len() - 1;
        let open = self.selected_thread().and_then(|open| threads.iter().position(|thread| thread.id == open.id));
        let index = match open {
            Some(index) => index.saturating_add_signed(offset).min(last),
            None if offset > 0 => 0,
            None => last,
        };
        self.show_thread_at(index, cx)
    }

    /// The pull request the right panel shows, while its tab is the one in front.
    fn shown_pull_request(&self) -> Option<&super::pull_requests::Page> {
        if !self.right_panel.is_open {
            return None;
        }
        match self.panel_tabs().active {
            Some(PanelTab::PullRequest | PanelTab::PullRequestNumber(_)) => self.pull_requests.page.value(),
            _ => None,
        }
    }

    /// Whether the composer's controls are out of sight, behind a route or the maximized panel,
    /// so that their keys don't press them.
    pub fn composer_away(&self) -> bool {
        self.settings.section.is_some() || self.shows_usage || self.panel_maximized()
    }

    /// What Esc does when the focused view claims nothing: leaves a route, restores the
    /// maximized right panel, or hides a right panel that only has its blank tab. Nothing while
    /// the command panel, the viewer or the branches have it. Whether it did anything.
    pub fn escape(&mut self) -> bool {
        if self.panel.is_some() || self.viewing.is_some() || self.shows_branches {
            return false;
        }
        if self.settings.section.is_some() || self.shows_usage {
            self.close_route();
        } else if self.panel_maximized() {
            self.toggle_panel_maximized();
        } else if self.right_panel.is_open && self.panel_tabs().is_blank() {
            self.set_panel_open(false);
        } else {
            return false;
        }
        true
    }

    /// Leaves the settings or the usage for the threads.
    pub fn close_route(&mut self) {
        self.close_settings();
        self.shows_usage = false;
    }
}

enum UsageChange {
    Tab(UsageTab),
    Period(UsagePeriod),
}

fn usage_command(command: &str) -> Option<UsageChange> {
    Some(match command {
        "usage.cost" => UsageChange::Tab(UsageTab::Cost),
        "usage.tokens" => UsageChange::Tab(UsageTab::Tokens),
        "usage.limits" => UsageChange::Tab(UsageTab::Limits),
        "usage.period.day" => UsageChange::Period(UsagePeriod::Day),
        "usage.period.week" => UsageChange::Period(UsagePeriod::Week),
        "usage.period.month" => UsageChange::Period(UsagePeriod::Month),
        "usage.period.quarter" => UsageChange::Period(UsagePeriod::Quarter),
        _ => return None,
    })
}
