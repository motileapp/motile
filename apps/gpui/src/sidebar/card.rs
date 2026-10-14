//! What a thread is, beside its row in the sidebar: where it works, the model and the account as
//! the composer shows them, and its pull request. It comes up once the pointer rests on a row,
//! then goes at once to every row the pointer moves to, and shortly after it went away:
//! `ThreadCard.swift`.

use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{ROW_INSET, Sidebar, row_margin};
use crate::models::{ThreadInfo, last_component, shortened};
use crate::theme::{Surface, colors};
use crate::ui::control::dotted;
use crate::ui::logos::{self, pull_request_color, pull_request_symbol};
use crate::ui::{divider_on, dropdown_card, icons, tabular_numbers};

const MAX_WIDTH: f32 = 320.;
/// How long the pointer rests on a row before the first card comes up.
const REST: Duration = Duration::from_millis(300);
/// How soon after a card went the next one comes up at once.
const LINGER: Duration = Duration::from_millis(400);
const FADE: Duration = Duration::from_millis(100);
/// Between the row's light and the card.
const GAP: f32 = 4.;
/// What the card keeps from the window's edges, with its shadow.
const MARGIN: f32 = 16.;
/// Above the rows, under the menus and the modals.
const PRIORITY: usize = 10;

/// The card shown, beside the row it is for.
struct Shown {
    thread_id: String,
    row: Bounds<Pixels>,
    fading: bool,
    serial: usize,
}

#[derive(Default)]
pub struct Peek {
    /// The thread whose row the pointer is on.
    pointed: Option<String>,
    shown: Option<Shown>,
    hidden_at: Option<Instant>,
    waiting: Option<Task<()>>,
    serial: usize,
}

impl Peek {
    fn show(&mut self, thread_id: String, row: Bounds<Pixels>, fading: bool) {
        self.waiting = None;
        self.serial += 1;
        self.shown = Some(Shown { thread_id, row, fading, serial: self.serial });
    }

    pub fn points_at(&self, thread_id: &str) -> bool {
        self.pointed.as_deref() == Some(thread_id)
    }

    pub fn hide(&mut self) {
        self.pointed = None;
        self.waiting = None;
        if self.shown.take().is_some() {
            self.hidden_at = Some(Instant::now());
        }
    }
}

impl Sidebar {
    /// The pointer is on the thread's row, at `row` in the window, or on no thread's.
    pub(super) fn point(&mut self, target: Option<(String, Bounds<Pixels>)>, cx: &mut Context<Self>) {
        self.peek.waiting = None;
        let behind_modal = crate::ui::modal::is_open(cx);
        let Some((thread_id, row)) = target.filter(|_| !behind_modal && self.press.is_none()) else {
            self.peek.hide();
            return;
        };
        self.peek.pointed = Some(thread_id.clone());
        let lingers = self.peek.hidden_at.is_some_and(|at| at.elapsed() < LINGER);
        if self.peek.shown.is_some() || lingers {
            self.peek.show(thread_id, row, false);
            return;
        }
        self.peek.waiting = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REST).await;
            let _ = this.update(cx, |this, cx| {
                this.peek.show(thread_id, row, true);
                cx.notify();
            });
        }));
    }

    /// The card over the window, beside its row.
    pub(super) fn card_layer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let shown = self.peek.shown.as_ref()?;
        let store = self.store.read(cx);
        if !store.app_active || crate::ui::modal::is_open(cx) || crate::ui::menu::is_open(cx) {
            return None;
        }
        let thread = store.threads.get(&shown.thread_id)?.clone();
        let card = self.card(&thread, cx);
        let card = if shown.fading {
            card.with_animation(("thread-card", shown.serial), Animation::new(FADE), |card, delta| card.opacity(delta))
                .into_any_element()
        } else {
            card.into_any_element()
        };
        let origin = point(shown.row.right() - px(ROW_INSET) + px(GAP), shown.row.top() + px(row_margin().top));
        Some(
            deferred(anchored().position(origin).snap_to_window_with_margin(px(MARGIN)).child(card))
                .with_priority(PRIORITY)
                .into_any_element(),
        )
    }

    fn card(&self, thread: &ThreadInfo, cx: &App) -> Div {
        let c = colors(cx);
        let store = self.store.read(cx);
        let project = store.project(Some(&thread.project_id)).map(|project| project.seen(thread));
        let server = store.server(Some(&thread.server_id));
        let line = |text: String, middle: bool, icon: AnyElement| {
            let shown = div().min_w_0().text_size(px(12.)).whitespace_nowrap().overflow_hidden();
            let shown = if middle { shown.text_ellipsis_middle() } else { shown.text_ellipsis() };
            div()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_color(c.muted_foreground)
                .child(div().size(px(14.)).flex_shrink_0().flex().items_center().justify_center().child(icon))
                .child(shown.child(dotted(text, c)))
                .into_any_element()
        };
        let name = project.as_ref().map(|project| project.name.clone()).unwrap_or_else(|| last_component(&thread.cwd));
        let checkout = match project.as_ref().and_then(|project| project.branch.clone().map(|branch| (project, branch)))
        {
            Some((project, branch)) => {
                line(branch, true, icons::symbol(project.checkout_symbol(), 13.).into_any_element())
            }
            None => {
                let home = server.map(|server| server.home.as_str()).unwrap_or_default();
                line(shortened(&thread.cwd, home), true, icons::symbol("folder", 13.).into_any_element())
            }
        };
        let pull_request = project.as_ref().and_then(|project| project.pull_request(thread));
        dropdown_card(div(), cx)
            .max_w(px(MAX_WIDTH))
            .p(px(12.))
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c.foreground)
                    .truncate()
                    .child(thread.title.clone()),
            )
            .child(
                div()
                    .pt(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(line(name, false, logos::project_icon(project.as_ref(), 14., cx)))
                    .child(checkout)
                    .child(line(
                        store.model_label(thread),
                        false,
                        logos::agent_icon(thread.agent, 13., cx).into_any_element(),
                    ))
                    .when_some(server, |lines, server| {
                        lines.child(line(server.name.clone(), false, icons::symbol("server", 12.).into_any_element()))
                    }),
            )
            .when_some(pull_request, |card, pull_request| {
                card.child(div().py(px(10.)).child(divider_on(Surface::Popover, cx))).child(
                    div()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .w(px(14.))
                                .flex_shrink_0()
                                .flex()
                                .justify_center()
                                .text_color(pull_request_color(&pull_request, c))
                                .child(icons::symbol(pull_request_symbol(&pull_request), 13.)),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(12.))
                                .font_weight(FontWeight::MEDIUM)
                                .font_features(tabular_numbers())
                                .text_color(c.muted_stronger_foreground)
                                .child(format!("#{}", pull_request.number)),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .text_size(px(12.))
                                .text_color(c.muted_foreground)
                                .truncate()
                                .child(pull_request.title.clone()),
                        ),
                )
            })
    }
}
