//! What the composer's text does with skills, as the Mac app's `ComposerTextMac.swift` does: it
//! says what is typed before its cursor, puts in the skill picked from the menu, and makes the
//! `$name`s of skills chips. The menu hangs over the transcript from the composer's top, as one
//! of its strips.

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::input::{InlineToken, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{Composer, strips};
use crate::store::skills::{SkillPick, SkillRow, SkillSpan, skill_word};
use crate::theme::{self, Radius, colors};
use crate::ui::{Chip, icons, skill_chip};

const ROW_HEIGHT: f32 = 32.;
const INSET: f32 = 4.;
/// As many rows as show before the rest scroll, the last cut in half to say there are more.
const SHOWN_ROWS: f32 = 7.5;

#[derive(Default)]
pub struct SkillTyping {
    /// What was last said to be typed before the cursor.
    reported: Option<String>,
    /// The last skill picked from the menu that the text has put in.
    picked: u64,
    /// Text was just pasted, whose last `$name` becomes a chip too.
    pub pasted: Rc<Cell<bool>>,
    scroll: UniformListScrollHandle,
    lit: usize,
}

impl SkillTyping {
    /// Before any skill is put in but the ones picked up to `picked`.
    pub fn after(picked: u64) -> Self {
        Self { picked, ..Self::default() }
    }
}

impl Composer {
    /// Tells the store what is typed before the cursor, for the skills that fit it. While an
    /// input method composes, nothing is, so its keys stay its own.
    pub(super) fn report(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let before = if self.composing(window, cx) { String::new() } else { self.typed(window, cx) };
        if self.skills.reported.as_ref() == Some(&before) {
            return;
        }
        self.skills.reported = Some(before.clone());
        self.store.update(cx, |store, cx| store.read_skill_menu(&before, cx));
    }

    /// The text before the cursor, while it is typed in.
    fn typed(&self, window: &Window, cx: &App) -> String {
        let text = self.text.read(cx);
        let selection = text.selected_range();
        if !text.focus_handle(cx).is_focused(window) || !selection.is_empty() {
            return String::new();
        }
        text.value().get(..selection.start).unwrap_or_default().to_string()
    }

    /// Another text was put in the composer: its menu closes, and its `$name`s become chips.
    pub(super) fn loaded_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.skills.reported = Some(self.typed(window, cx));
        self.store.update(cx, |store, cx| store.close_skill_menu(cx));
        self.chipify(true, window, cx);
    }

    /// After an edit, the `$name`s of skills that a space or a line follows become chips, and
    /// after a paste the one that ends the text too.
    pub(super) fn chipify_typed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pasted = self.skills.pasted.replace(false);
        let text = self.text.read(cx);
        let value = text.value();
        let spaced =
            value.get(..text.cursor()).and_then(|before| before.chars().next_back()).is_some_and(char::is_whitespace);
        if pasted || spaced {
            self.chipify(pasted, window, cx);
        }
    }

    /// Makes the text's `$name`s of skills chips: those a space or a line follows, and with
    /// `at_end` the one that ends the text too.
    fn chipify(&mut self, at_end: bool, window: &mut Window, cx: &mut Context<Self>) {
        let asked = self.text.read(cx).value().to_string();
        let (text, handle) = (self.text.downgrade(), window.window_handle());
        self.store.update(cx, |store, _| {
            store.read_skill_spans(asked.clone(), move |spans, cx| {
                let Some(text) = text.upgrade() else { return };
                let _ = handle.update(cx, |_, window, cx| put_chips(&text, &asked, spans, at_end, window, cx));
            })
        });
    }

    /// Puts what the store says in the text: the skill picked, and keeps the lit row in sight.
    pub(super) fn follow_skills(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let skills = &self.store.read(cx).skills;
        let (pick, highlight) = (skills.pick.clone(), skills.highlight);
        if highlight != self.skills.lit {
            self.skills.lit = highlight;
            self.skills.scroll.scroll_to_item(highlight, ScrollStrategy::Nearest);
        }
        let Some(pick) = pick.filter(|pick| pick.id != self.skills.picked) else { return };
        self.skills.picked = pick.id;
        self.put(pick, window, cx);
    }

    /// Puts the picked skill in place of the word typed before the cursor, with a space after.
    fn put(&mut self, pick: SkillPick, window: &mut Window, cx: &mut Context<Self>) {
        self.text.update(cx, |text, cx| {
            let Some(typed) = skill_word(&text.value(), text.cursor()) else { return };
            if text.replace_range_with_token(typed, token(&pick.name, pick.label), window, cx).is_ok() {
                text.insert(" ", window, cx);
            }
        });
    }

    /// The skills that fit the word typed, on top of the composer: the best first, each with
    /// what it does and where it comes from.
    pub(super) fn skill_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let c = colors(cx);
        let skills = &self.store.read(cx).skills;
        let rows = Rc::new(skills.menu.as_ref()?.rows.clone());
        let highlight = skills.highlight;
        let height = (rows.len() as f32).min(SHOWN_ROWS) * ROW_HEIGHT + INSET * 2.;
        let list = uniform_list(
            "skill-menu",
            rows.len(),
            cx.processor(move |this, range: Range<usize>, _, cx| {
                range.map(|index| this.skill_row(index, &rows[index], index == highlight, cx)).collect()
            }),
        )
        .track_scroll(&self.skills.scroll)
        .w_full()
        .h(px(height))
        .py(px(INSET));
        let strip = strips::strip(true, None, cx)
            .id("skill-menu-strip")
            .occlude()
            .shadow(c.shadow(theme::ShadowSize::Lg, theme::Opacity::Shadow))
            .child(div().flex_1().min_w_0().child(list));
        let hanging = div().absolute().bottom_0().left_0().right_0().child(strip);
        Some(div().absolute().top_0().left_0().right_0().h(px(0.)).child(hanging).into_any_element())
    }

    fn skill_row(&self, index: usize, row: &SkillRow, lit: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let c = colors(cx);
        let (hover, click) = (self.store.clone(), self.store.clone());
        div()
            .id(("skill", index))
            .w_full()
            .h(px(ROW_HEIGHT))
            .px(px(INSET))
            .flex()
            .on_hover(move |hovered, _, cx| {
                if !*hovered {
                    return;
                }
                hover.update(cx, |store, cx| {
                    if store.skills.highlight != index {
                        store.skills.highlight = index;
                        cx.notify();
                    }
                });
            })
            .on_click(move |_, _, cx| click.update(cx, |store, cx| store.pick_skill(Some(index), cx)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(Radius::MD))
                    .when(lit, |row| row.bg(c.border_composer))
                    .text_size(px(skill_chip::TEXT_SIZE))
                    .child(icons::symbol(skill_chip::SYMBOL, skill_chip::TEXT_SIZE).text_color(c.skill))
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c.foreground)
                            .whitespace_nowrap()
                            .child(row.label.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(c.muted_foreground)
                            .truncate()
                            .child(row.description.clone()),
                    )
                    .child(div().w(px(8.)).flex_none())
                    .child(Chip::new(row.source.clone()).icon(row.symbol.clone())),
            )
    }
}

fn token(name: &str, label: impl Into<SharedString>) -> InlineToken {
    InlineToken::new(name.to_string(), format!("${name}")).with_label(label)
}

/// Makes chips of the `$name`s of `spans` that a space or a line follows, and with `at_end` the
/// one that ends the text, while the text is still what they were found in.
fn put_chips(
    text: &Entity<TextareaState>,
    asked: &str,
    spans: Vec<(Range<usize>, SkillSpan)>,
    at_end: bool,
    window: &mut Window,
    cx: &mut App,
) {
    text.update(cx, |text, cx| {
        if text.value().as_ref() != asked {
            return;
        }
        let selection = text.selected_range();
        let ended = spans.into_iter().filter(|(range, _)| {
            asked.get(range.end..).and_then(|after| after.chars().next()).map_or(at_end, char::is_whitespace)
        });
        let mut changed = false;
        for (range, span) in ended.rev() {
            let chip = token(&span.name, span.label);
            if text.tokens().iter().any(|kept| kept.range() == range && kept.token() == &chip) {
                continue;
            }
            changed |= text.replace_range_with_token(range, chip, window, cx).is_ok();
        }
        if changed {
            text.set_selected_range(selection, cx);
        }
    });
}
