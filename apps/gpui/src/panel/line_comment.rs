//! The sheet that comments on a line of a pull request's diff: what was said there already, to
//! reply to, and a comment to keep for the review or to hand to the agent.

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Command;
use motile_protocol::wire::{PullRequestEdit, Side};

use super::pull_request::{label, styled_line, text_blocks};
use super::state::file_symbol;
use crate::models::ago;
use crate::store::Store;
pub use crate::store::pull_requests::CommentedLine;
use crate::store::pull_requests::Thread;
use crate::theme::{self, Layer, Radius, Surface, colors};
use crate::ui::icons;
use crate::ui::modal::{modal, present_sheet};
use crate::ui::{ActionButton, TextArea, text_area_state};

impl From<super::code_view::CommentedLine> for CommentedLine {
    fn from(line: super::code_view::CommentedLine) -> Self {
        let side = if line.side == "left" { Side::Left } else { Side::Right };
        Self { path: line.path, line: line.line, side, code: line.code }
    }
}

pub struct LineCommentSheet {
    store: Entity<Store>,
    number: u64,
    commented: CommentedLine,
    text: Entity<TextareaState>,
    focus: FocusHandle,
    /// It has closed and fades out.
    pub leaving: bool,
}

impl LineCommentSheet {
    pub fn new(
        store: Entity<Store>,
        number: u64,
        commented: CommentedLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let has_threads = !Self::threads_of(&store, number, &commented, cx).is_empty();
        let placeholder = if has_threads { "Reply, or say something new" } else { "Comment on this line" };
        let text = text_area_state(placeholder, 5, 12, window, cx);
        cx.subscribe(&text, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        text.update(cx, |text, cx| text.focus(window, cx));
        Self { store, number, commented, text, focus: cx.focus_handle(), leaving: false }
    }

    /// The conversations already on the line.
    fn threads_of(store: &Entity<Store>, number: u64, commented: &CommentedLine, cx: &App) -> Vec<Thread> {
        let Some(page) = store.read(cx).pull_requests.page_of(number) else { return Vec::new() };
        page.threads
            .iter()
            .filter(|thread| {
                thread.path == commented.path && thread.line == Some(commented.line) && thread.side == commented.side
            })
            .cloned()
            .collect()
    }

    fn written(&self, cx: &App) -> String {
        self.text.read(cx).value().trim().to_string()
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.comment_on_line(None);
            cx.notify();
        });
    }

    fn add_to_review(&mut self, cx: &mut Context<Self>) {
        let written = self.written(cx);
        let (number, line) = (self.number, self.commented.clone());
        self.store.update(cx, |store, cx| {
            store.add_pending_comment(number, line.path, line.line, line.side, written);
            cx.notify();
        });
        self.dismiss(cx);
    }

    fn reply_to(&mut self, thread: String, cx: &mut Context<Self>) {
        let body = self.written(cx);
        let number = self.number;
        self.store.update(cx, |store, cx| {
            let Some(target) = store.panel_target() else { return };
            store.pull_request_edit(PullRequestEdit::Reply { thread, body }, None, &target, number);
            cx.notify();
        });
        self.dismiss(cx);
    }

    fn ask_agent(&mut self, cx: &mut Context<Self>) {
        let note = self.written(cx);
        let line = self.commented.clone();
        let number = self.number;
        self.store.update(cx, |store, _| {
            let page = store.pull_requests.page_of(number);
            let (url, head) = page.map(|page| (page.url.clone(), page.head.clone())).unwrap_or_default();
            let command =
                Command::LinePrompt { number, url, head, path: line.path, line: line.line, code: line.code, note };
            store.ask(command, |store, reply, cx| {
                let Ok(answer) = reply else { return };
                let Some(prompt) = answer["prompt"].as_str() else { return };
                store.hand_off(prompt.to_string());
                cx.notify();
            });
        });
        self.dismiss(cx);
    }
}

impl Render for LineCommentSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let threads = Self::threads_of(&self.store, self.number, &self.commented, cx);
        let written = self.written(cx);
        let empty = written.is_empty();
        let last = threads.last().map(|thread| thread.id.clone());
        let code = if self.commented.code.is_empty() { " ".to_string() } else { self.commented.code.clone() };
        let conversation = (!threads.is_empty()).then(|| {
            div()
                .id("line-comment-threads")
                .w_full()
                .max_h(px(220.))
                .overflow_y_scroll()
                .bg(Surface::Background.layer(Layer::Box, c))
                .rounded(px(Radius::LG))
                .child(div().w_full().p(px(12.)).flex().flex_col().items_start().gap(px(10.)).children(
                    threads.iter().flat_map(|thread| thread.comments.iter()).enumerate().map(|(index, comment)| {
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(4.))
                            .child(div().text_size(px(12.5)).child(styled_line(vec![
                                (comment.author.clone(), c.foreground, FontWeight::SEMIBOLD),
                                (format!(" · {}", ago(comment.at)), c.muted_stronger_foreground, FontWeight::NORMAL),
                            ])))
                            .child(text_blocks(&format!("line-comment-{index}"), &comment.body, Surface::Secondary, cx))
                    }),
                ))
        });
        let content = div()
            .id("line-comment")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let modified = event.keystroke.modifiers.modified();
                match event.keystroke.key.as_str() {
                    "escape" => this.dismiss(cx),
                    // Return is a line break while the comment is written.
                    "enter"
                        if !modified
                            && !this.text.focus_handle(cx).is_focused(window)
                            && !this.written(cx).is_empty() =>
                    {
                        this.add_to_review(cx)
                    }
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .p(px(20.))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(12.))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(6.))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(icons::symbol(file_symbol(&self.commented.path), 12.).text_color(c.muted_foreground))
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(c.foreground)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis_start()
                                    .child(self.commented.path.clone()),
                            )
                            .child(div().flex_shrink_0().child(label(
                                format!("line {}", self.commented.line),
                                12.5,
                                c.muted_stronger_foreground,
                            ))),
                    )
                    .child(
                        div()
                            .w_full()
                            .px(px(10.))
                            .py(px(7.))
                            .bg(Surface::Background.layer(Layer::Box, c))
                            .rounded(px(Radius::MD))
                            .font_family(theme::MONO_FONT)
                            .text_size(px(12.))
                            .text_color(c.foreground)
                            .line_clamp(3)
                            .child(code),
                    ),
            )
            .children(conversation)
            .child(TextArea::new(&self.text))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        ActionButton::new("line-comment-ask", "Ask the Agent")
                            .help("Put this line and what you wrote in the composer")
                            .disabled(empty)
                            .on_click(cx.listener(|this, _, _, cx| this.ask_agent(cx))),
                    )
                    .child(div().flex_1())
                    .child(
                        ActionButton::new("line-comment-cancel", "Cancel")
                            .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))),
                    )
                    .when_some(last, |row, thread| {
                        row.child(
                            ActionButton::new("line-comment-reply", "Reply")
                                .disabled(empty)
                                .on_click(cx.listener(move |this, _, _, cx| this.reply_to(thread.clone(), cx))),
                        )
                    })
                    .child(
                        ActionButton::new("line-comment-add", "Add to Review")
                            .primary()
                            .disabled(empty)
                            .on_click(cx.listener(|this, _, _, cx| this.add_to_review(cx))),
                    ),
            );
        let close = cx.entity().downgrade();
        modal("line-comment-sheet", content, move |_, cx| {
            close.update(cx, |this, cx| this.dismiss(cx)).ok();
        })
        .width(500.)
        .leaving(self.leaving)
    }
}

/// Keeps the sheet up while the store says which line is being commented on, and gives it to
/// draw, and for a moment after while it fades out. Whoever draws the panel calls this from its
/// render with a slot of its own.
pub fn follow(
    slot: &mut Option<Entity<LineCommentSheet>>,
    store: &Entity<Store>,
    window: &mut Window,
    cx: &mut App,
) -> Option<Entity<LineCommentSheet>> {
    let state = store.read(cx);
    let commenting = state.pull_requests.commenting.clone();
    let number = state.panel_target().and_then(|target| target.pull_request);
    let shown = state.pull_requests.page.value().map(|page| page.number).or(number);
    match commenting.zip(shown) {
        None => *slot = None,
        Some((line, number)) if slot.is_none() => {
            let store = store.clone();
            *slot = Some(cx.new(|cx| LineCommentSheet::new(store, number, line, window, cx)));
        }
        Some(_) => {}
    }
    present_sheet("line-comment-sheet", slot.clone(), |sheet, leaving| sheet.leaving = leaving, window, cx)
}
