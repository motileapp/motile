//! What the search found, on two lines that are as tall whatever was found: its project's icon,
//! its title and what it is up to, then the message it was found by, or else its project:
//! `SearchResults.swift`.

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::search::MatchView;
use motile_protocol::wire::Speaker;

use super::rows::{quieted, thread_status};
use super::{BOTTOM_PADDING, FIRST_LINE_HEIGHT, SIDE_PADDING, Sidebar, TOP_PADDING, row_margin};
use crate::models::{ago, last_component};
use crate::store::SearchResult;
use crate::theme::{Radius, colors};
use crate::ui::logos::{self, PullRequestLabel};
use crate::ui::{Highlight, KeyCaps, icons};

/// How tall a result's row is, without the gap around it.
pub const RESULT_ROW_HEIGHT: f32 = TOP_PADDING + FIRST_LINE_HEIGHT + 18. + BOTTOM_PADDING;

/// Who wrote the message, then the message with what was searched for set off.
fn excerpt(found: &MatchView, cx: &App) -> StyledText {
    let c = colors(cx);
    let speaker = if found.speaker == Speaker::User { "You: " } else { "Agent: " };
    let mut text = speaker.to_string();
    let mut runs = vec![(
        0..text.len(),
        HighlightStyle { color: Some(c.muted_foreground), font_weight: Some(FontWeight::MEDIUM), ..Default::default() },
    )];
    for part in &found.parts {
        let start = text.len();
        text.push_str(&part.text);
        let style = if part.matched {
            HighlightStyle { color: Some(c.foreground), font_weight: Some(FontWeight::SEMIBOLD), ..Default::default() }
        } else {
            HighlightStyle { color: Some(c.muted_stronger_foreground), ..Default::default() }
        };
        runs.push((start..text.len(), style));
    }
    StyledText::new(text).with_highlights(runs)
}

impl Sidebar {
    /// A result, lit while it is the highlighted one, which the pointer moves as the arrow keys do.
    pub(super) fn result_row(
        &self,
        result: &SearchResult,
        index: usize,
        highlighted: bool,
        jump: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let (project, title) = match result {
            SearchResult::Draft(listed) => {
                (store.project(listed.draft.project_id.as_deref()).cloned(), listed.preview.clone())
            }
            SearchResult::Thread(thread, _) => {
                (store.project(Some(&thread.project_id)).map(|project| project.seen(thread)), thread.title.clone())
            }
        };
        let done = result.thread().is_some_and(|thread| thread.is_done());
        let title_color = if highlighted {
            c.foreground
        } else if done {
            c.muted_stronger_foreground
        } else {
            c.muted_foreground
        };
        let status = match result {
            SearchResult::Draft(_) => div()
                .flex()
                .items_center()
                .gap(px(3.))
                .text_color(c.muted_foreground)
                .child(icons::symbol("file", 11.))
                .child(div().text_size(px(11.)).font_weight(FontWeight::MEDIUM).child("Draft")),
            SearchResult::Thread(thread, _) => {
                let read_only = store.on_removed_server(thread);
                let pull_request =
                    project.as_ref().and_then(|project| project.pull_request(thread)).filter(|_| !read_only);
                let state = if let Some(jump) = jump {
                    div().text_color(c.foreground).child(KeyCaps::new([jump])).into_any_element()
                } else if read_only {
                    thread_status(thread, true, cx)
                } else if done {
                    div()
                        .text_size(px(11.))
                        .text_color(c.muted_stronger_foreground)
                        .child(ago(thread.done_at.unwrap_or(thread.updated_at)))
                        .into_any_element()
                } else {
                    thread_status(thread, false, cx)
                };
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .when_some(pull_request, |status, pull_request| {
                        status.child(
                            PullRequestLabel::new(SharedString::from(format!("found-pr-{}", thread.id)), &pull_request)
                                .colored(!done),
                        )
                    })
                    .child(state)
            }
        };
        let detail = match result {
            SearchResult::Thread(_, Some(found)) => div().child(excerpt(found, cx)),
            _ => {
                let name = project
                    .as_ref()
                    .map(|project| project.name.clone())
                    .or_else(|| result.thread().map(|thread| last_component(&thread.cwd)));
                div().text_color(c.muted_stronger_foreground).child(name.unwrap_or_else(|| "No project".into()))
            }
        };
        let key = format!("result-{}", result.id());
        let margin = row_margin();
        let (opened, menu_thread) = (result.clone(), result.thread().map(|thread| thread.id.clone()));
        div()
            .id(SharedString::from(key.clone()))
            .group(SharedString::from(key.clone()))
            .relative()
            .size_full()
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .px(px(margin.left))
            .child(
                Highlight::new()
                    .radius(Radius::MD)
                    .inset(margin)
                    .hovered(highlighted)
                    .opaque(true)
                    .render(key.clone(), cx),
            )
            .child(
                div()
                    .relative()
                    .size_full()
                    .px(px(SIDE_PADDING))
                    .pt(px(TOP_PADDING))
                    .pb(px(BOTTOM_PADDING))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(FIRST_LINE_HEIGHT))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(quieted(logos::project_icon(project.as_ref(), 14., cx), done && !highlighted, cx))
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(title_color)
                                    .truncate()
                                    .child(title),
                            )
                            .child(div().flex_1().min_w(px(6.)))
                            .child(div().flex_shrink_0().child(status)),
                    )
                    .child(detail.h(px(18.)).line_height(px(18.)).min_w_0().text_size(px(12.)).truncate()),
            )
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && this.highlighted != index {
                    this.highlighted = index;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| this.open_result(&opened, window, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    let Some(thread_id) = menu_thread.clone() else { return };
                    let Some(menu) = this.thread_menu(thread_id, cx) else { return };
                    menu.show(event.position, window, cx);
                }),
            )
            .into_any_element()
    }

    /// Opens what the search found and ends the search: the field is emptied at once, so that
    /// nothing it says after still searches.
    pub(super) fn open_result(&mut self, result: &SearchResult, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |input, cx| input.set_value("", window, cx));
        self.store.update(cx, |store, cx| {
            store.open_search_result(result, cx);
            cx.notify();
        });
    }

    /// The arrow keys move the highlight through what the search found, round from either end.
    pub(super) fn steer(&mut self, step: isize, cx: &mut Context<Self>) -> bool {
        let count = self.result_ids.len();
        if count == 0 {
            return false;
        }
        self.highlighted = (self.highlighted as isize + step).rem_euclid(count as isize) as usize;
        cx.notify();
        true
    }

    /// Return opens the highlighted result, as the results are now.
    pub(super) fn open_highlighted(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let plan = self.plan(cx);
        let Some(result) = plan.results.get(self.highlighted) else { return };
        self.open_result(result, window, cx);
    }

    /// The highlight stays on its row while what is found later comes in below; otherwise it
    /// goes back to the first.
    pub(super) fn keep_highlight(&mut self, ids: Vec<String>) {
        if ids == self.result_ids {
            return;
        }
        let kept = &self.result_ids[..self.result_ids.len().min(self.highlighted + 1)];
        let stays = ids.len() >= kept.len() && ids[..kept.len()] == *kept;
        if !stays {
            self.highlighted = 0;
        }
        self.result_ids = ids;
    }
}
