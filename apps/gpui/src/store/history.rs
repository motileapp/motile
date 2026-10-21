use gpui_kit::{Context, Subscription};

use super::Selection;
use super::settings::SettingsSection;
use crate::store::Store;

const LIMIT: usize = 50;

/// What fills the window in place of the others: a thread or a draft, an agent shown in its
/// thread's place, a section of the settings or the usage. The panels beside them aren't pages.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Page {
    Selection(Selection),
    Agent { thread_id: String, agent_id: String },
    Settings(SettingsSection),
    Usage,
}

/// The pages the client went through, for Go Back and Go Forward.
#[derive(Default)]
pub struct PageHistory {
    back: Vec<Page>,
    forward: Vec<Page>,
    current: Option<Page>,
}

impl PageHistory {
    /// The client is on `page`; the one it left goes behind it, and nothing is ahead any more.
    fn visit(&mut self, page: Page) {
        let Some(current) = self.current.replace(page.clone()) else { return };
        if current == page {
            return;
        }
        self.back.push(current);
        if self.back.len() > LIMIT {
            self.back.remove(0);
        }
        self.forward.clear();
    }

    fn can_step(&self, back: bool, exists: impl Fn(&Page) -> bool) -> bool {
        let pages = if back { &self.back } else { &self.forward };
        pages.iter().any(exists)
    }

    /// The nearest page behind or ahead that still exists, which becomes the current one. The
    /// ones on the way that don't are dropped.
    fn step(&mut self, back: bool, exists: impl Fn(&Page) -> bool) -> Option<Page> {
        loop {
            let page = if back { self.back.pop() } else { self.forward.pop() }?;
            if !exists(&page) {
                continue;
            }
            if let Some(current) = self.current.replace(page.clone()) {
                if back { self.forward.push(current) } else { self.back.push(current) }
            }
            return Some(page);
        }
    }
}

impl Store {
    pub fn page(&self) -> Page {
        if let Some(section) = self.settings.section {
            return Page::Settings(section);
        }
        if self.shows_usage {
            return Page::Usage;
        }
        if let (Some(agent_id), Selection::Thread(thread_id)) = (&self.right_panel.shown_agent, &self.selection) {
            return Page::Agent { thread_id: thread_id.clone(), agent_id: agent_id.clone() };
        }
        Page::Selection(self.selection.clone())
    }

    /// Records the page once an update is done, so that the steps on the way aren't pages.
    pub(super) fn watch_page(cx: &mut Context<Self>) -> Subscription {
        cx.observe_self(|store, cx| {
            if store.record_page() {
                cx.notify();
            }
        })
    }

    pub fn can_go(&self, back: bool) -> bool {
        self.history.can_step(back, |page| self.page_exists(page))
    }

    /// Goes back or forward to the nearest page that is still there. Whether there was one.
    pub fn go(&mut self, back: bool, cx: &mut Context<Self>) -> bool {
        self.record_page();
        let mut history = std::mem::take(&mut self.history);
        let step = history.step(back, |page| self.page_exists(page));
        self.history = history;
        let Some(page) = step else { return false };
        self.show_page(&page, cx);
        self.history.current = Some(self.page());
        cx.notify();
        true
    }

    /// Whether the page changed since it was last recorded.
    fn record_page(&mut self) -> bool {
        let page = self.page();
        if self.history.current.as_ref() == Some(&page) {
            return false;
        }
        self.history.visit(page);
        true
    }

    fn page_exists(&self, page: &Page) -> bool {
        match page {
            Page::Selection(Selection::Thread(id)) | Page::Agent { thread_id: id, .. } => self.threads.contains_key(id),
            Page::Selection(Selection::Draft(id)) => self.thread_drafts.iter().any(|draft| &draft.id == id),
            Page::Settings(_) | Page::Usage => self.account.signed_in,
        }
    }

    fn show_page(&mut self, page: &Page, cx: &mut Context<Self>) {
        match page {
            Page::Settings(section) => self.open_settings(*section, None),
            Page::Usage => self.open_usage(),
            Page::Selection(selection) => {
                self.close_route();
                self.select(selection.clone(), cx);
                self.show_agents(cx);
            }
            Page::Agent { thread_id, agent_id } => {
                self.close_route();
                self.select(Selection::Thread(thread_id.clone()), cx);
                self.show_agent(agent_id.clone(), cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(id: &str) -> Page {
        Page::Selection(Selection::Thread(id.into()))
    }

    fn visited(pages: &[Page]) -> PageHistory {
        let mut history = PageHistory::default();
        for page in pages {
            history.visit(page.clone());
        }
        history
    }

    #[test]
    fn goes_back_and_forward_through_the_pages_visited() {
        let mut history = visited(&[thread("a"), Page::Usage, thread("b")]);
        assert_eq!(history.step(true, |_| true), Some(Page::Usage));
        assert_eq!(history.step(true, |_| true), Some(thread("a")));
        assert_eq!(history.step(true, |_| true), None);
        assert_eq!(history.step(false, |_| true), Some(Page::Usage));
        assert_eq!(history.step(false, |_| true), Some(thread("b")));
        assert!(!history.can_step(false, |_| true));
    }

    #[test]
    fn a_new_page_drops_the_ones_ahead() {
        let mut history = visited(&[thread("a"), thread("b")]);
        history.step(true, |_| true);
        history.visit(Page::Settings(SettingsSection::default()));
        assert!(!history.can_step(false, |_| true));
        assert_eq!(history.step(true, |_| true), Some(thread("a")));
    }

    #[test]
    fn skips_the_pages_that_are_gone() {
        let mut history = visited(&[thread("a"), thread("gone"), thread("b")]);
        let exists = |page: &Page| page != &thread("gone");
        assert_eq!(history.step(true, exists), Some(thread("a")));
        assert_eq!(history.step(false, exists), Some(thread("b")));
    }

    #[test]
    fn keeps_only_the_latest_pages() {
        let pages: Vec<Page> = (0..LIMIT + 10).map(|index| thread(&index.to_string())).collect();
        let mut history = visited(&pages);
        let mut steps = 0;
        while history.step(true, |_| true).is_some() {
            steps += 1;
        }
        assert_eq!(steps, LIMIT);
    }
}
