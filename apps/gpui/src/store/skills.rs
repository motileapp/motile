//! The skills that fit the word typed before the composer's cursor, as the Mac app's
//! `Core/Skills.swift` keeps them: the menu, the row lit in it, and the skill picked from it.

use std::ops::Range;
use std::time::Duration;

use gpui_kit::*;
use motile_core::api::Command;
use serde::Deserialize;

use super::Store;

/// How long the menu's skills may take before the menu says they are loading.
const SKILL_MENU_PATIENCE: Duration = Duration::from_millis(150);

/// The skills that fit the word being typed, as the core ranks them, and that word.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SkillMenu {
    pub typed: String,
    pub rows: Vec<SkillRow>,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct SkillRow {
    pub name: String,
    pub label: String,
    pub description: String,
    /// Where it comes from, and Lucide's name for the symbol that says so.
    pub source: String,
    pub symbol: String,
}

/// A skill a text names by its `$name`, in UTF-16 units of the text.
#[derive(Deserialize, Clone, Debug)]
pub struct SkillSpan {
    pub start: usize,
    pub len: usize,
    pub name: String,
    pub label: String,
}

#[derive(Deserialize, Default)]
struct Chips {
    chips: Vec<SkillSpan>,
}

/// The skill the composer puts in place of the word typed before its cursor, once.
#[derive(Clone, Debug, PartialEq)]
pub struct SkillPick {
    pub id: u64,
    pub name: String,
    pub label: String,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SkillKey {
    Up,
    Down,
    Pick,
    Dismiss,
}

impl SkillKey {
    pub fn of(keystroke: &Keystroke) -> Option<Self> {
        if keystroke.modifiers.modified() {
            return None;
        }
        Some(match keystroke.key.as_str() {
            "up" => Self::Up,
            "down" => Self::Down,
            "enter" | "tab" => Self::Pick,
            "escape" => Self::Dismiss,
            _ => return None,
        })
    }
}

#[derive(Default)]
pub struct Skills {
    pub menu: Option<SkillMenu>,
    pub highlight: usize,
    pub pick: Option<SkillPick>,
    /// The menu's skills are asked for and haven't come in a moment, the first time on a server.
    pub loading: bool,
    asked: u64,
    answered: u64,
    loading_soon: Option<Task<()>>,
    /// The word whose menu was put away, which stays away until another is typed.
    dismissed: Option<String>,
}

impl Store {
    /// The composer's project on its server while it is connected, whose skills its menu offers.
    pub fn composer_skills_key(&self) -> Option<(String, String)> {
        let project = self.composer_project()?;
        self.server(Some(&project.server_id)).filter(|server| server.connected())?;
        Some((project.server_id, project.id))
    }

    /// Has the composer's skills listed before its menu opens, so that it opens with them.
    pub fn prefetch_skills(&mut self) {
        let Some((server_id, project_id)) = self.composer_skills_key() else { return };
        self.send(Command::PrefetchSkills { server_id, project_id });
    }

    /// Asks the core which skills fit the word that ends `before_cursor`, the composer's text up
    /// to its cursor. Only the last answer counts.
    pub fn read_skill_menu(&mut self, before_cursor: &str, cx: &mut Context<Self>) {
        if !before_cursor.contains(['$', '/']) {
            self.skills.dismissed = None;
            return self.close_skill_menu(cx);
        }
        let (Some(project), Some(account)) = (self.composer_project(), self.composer_account()) else {
            return self.close_skill_menu(cx);
        };
        self.skills.asked += 1;
        let asked = self.skills.asked;
        let start = before_cursor.char_indices().rev().nth(199).map_or(0, |(at, _)| at);
        let command = Command::SkillMenu {
            server_id: project.server_id,
            project_id: project.id,
            account: account.id,
            before_cursor: before_cursor[start..].to_string(),
        };
        self.ask(command, move |store, result, cx| {
            if asked != store.skills.asked {
                return;
            }
            store.skills.answered = asked;
            store.skills.loading = false;
            let Ok(answer) = result else { return cx.notify() };
            let menu: SkillMenu = serde_json::from_value(answer).unwrap_or_default();
            let skills = &mut store.skills;
            if skills.dismissed.as_ref() != Some(&menu.typed) {
                skills.dismissed = None;
            }
            if menu.rows.is_empty() || skills.dismissed.is_some() {
                skills.menu = None;
                return cx.notify();
            }
            let names = |menu: &SkillMenu| menu.rows.iter().map(|row| row.name.clone()).collect::<Vec<_>>();
            if skills.menu.as_ref().map(names) != Some(names(&menu)) {
                skills.highlight = 0;
            }
            skills.menu = Some(menu);
            cx.notify();
        });
        self.skills.loading_soon = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SKILL_MENU_PATIENCE).await;
            let _ = this.update(cx, |store, cx| {
                let skills = &mut store.skills;
                if asked != skills.asked
                    || skills.answered >= asked
                    || skills.menu.is_some()
                    || skills.dismissed.is_some()
                {
                    return;
                }
                skills.loading = true;
                cx.notify();
            });
        }));
    }

    pub fn close_skill_menu(&mut self, cx: &mut Context<Self>) {
        self.skills.asked += 1;
        let loading = std::mem::take(&mut self.skills.loading);
        if self.skills.menu.take().is_some() || loading {
            cx.notify();
        }
    }

    /// What a key does while the menu is open.
    pub fn skill_key(&mut self, key: SkillKey, cx: &mut Context<Self>) {
        match key {
            SkillKey::Up => self.move_skill_highlight(-1),
            SkillKey::Down => self.move_skill_highlight(1),
            SkillKey::Pick => self.pick_skill(None, cx),
            SkillKey::Dismiss => {
                self.skills.dismissed = self.skills.menu.as_ref().map(|menu| menu.typed.clone());
                self.close_skill_menu(cx);
            }
        }
        cx.notify();
    }

    fn move_skill_highlight(&mut self, step: isize) {
        let Some(count) = self.skills.menu.as_ref().map(|menu| menu.rows.len()).filter(|count| *count > 0) else {
            return;
        };
        let count = count as isize;
        self.skills.highlight = ((self.skills.highlight as isize + step + count) % count) as usize;
    }

    /// Puts the skill at `index`, or the lit one, in place of the word typed.
    pub fn pick_skill(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        let Some(menu) = &self.skills.menu else { return };
        let Some(row) = menu.rows.get(index.unwrap_or(self.skills.highlight)) else { return };
        let id = self.skills.pick.as_ref().map_or(0, |pick| pick.id) + 1;
        self.skills.pick = Some(SkillPick { id, name: row.name.clone(), label: row.label.clone() });
        self.close_skill_menu(cx);
        cx.notify();
    }

    /// The skills of the composer's agent that `text` names by their `$name`s, as byte ranges of
    /// it with their names and labels.
    pub fn read_skill_spans(
        &mut self,
        text: String,
        then: impl FnOnce(Vec<(Range<usize>, SkillSpan)>, &mut Context<Store>) + 'static,
    ) {
        if !text.contains('$') {
            return;
        }
        let (Some(project), Some(account)) = (self.composer_project(), self.composer_account()) else { return };
        let command = Command::SkillChips {
            server_id: project.server_id,
            project_id: project.id,
            account: account.id,
            text: text.clone(),
        };
        self.ask(command, move |_, result, cx| {
            let Ok(answer) = result else { return };
            let chips: Chips = serde_json::from_value(answer).unwrap_or_default();
            let spans = chips
                .chips
                .into_iter()
                .filter_map(|span| Some((utf16_range(&text, span.start, span.len)?, span)))
                .collect();
            then(spans, cx);
        });
    }
}

/// The bytes of `text` that `len` UTF-16 units from `start` cover.
pub fn utf16_range(text: &str, start: usize, len: usize) -> Option<Range<usize>> {
    let mut units = 0;
    let mut from = None;
    for (at, character) in text.char_indices().chain(std::iter::once((text.len(), '\0'))) {
        if units == start {
            from = Some(at);
        }
        if units == start + len {
            return Some(from?..at);
        }
        units += character.len_utf16();
    }
    None
}

/// The word that ends at `cursor` when it starts with `$` or `/`, which a skill picked from the
/// menu replaces: what is typed now, which the menu may not have caught up with.
pub fn skill_word(text: &str, cursor: usize) -> Option<Range<usize>> {
    let before = text.get(..cursor)?;
    let start = before
        .char_indices()
        .rfind(|(_, character)| character.is_whitespace())
        .map_or(0, |(at, space)| at + space.len_utf8());
    before[start..].starts_with(['$', '/']).then_some(start..cursor)
}

#[cfg(test)]
mod tests {
    use super::{skill_word, utf16_range};

    #[test]
    fn the_word_a_pick_replaces_starts_with_a_dollar_or_a_slash() {
        assert_eq!(skill_word("use $re", 7), Some(4..7));
        assert_eq!(skill_word("/re", 3), Some(0..3));
        assert_eq!(skill_word("é\n$r", 5), Some(3..5));
        assert_eq!(skill_word("use $re", 3), None);
        assert_eq!(skill_word("use $re ", 8), None, "the word has ended");
    }

    #[test]
    fn utf16_units_are_bytes_of_the_text() {
        assert_eq!(utf16_range("run $pdf", 4, 4), Some(4..8));
        assert_eq!(utf16_range("😀 $pdf", 3, 4), Some(5..9));
        assert_eq!(utf16_range("$pdf", 2, 4), None);
    }
}
