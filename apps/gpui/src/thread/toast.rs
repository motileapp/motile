//! What the last git action or project add did, what was refused, or an alert of another thread,
//! at the top of the thread.

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::alerts::AlertKind;

use crate::store::{AlertToast, Store, Toast};
use crate::theme::{Colors, ControlSize, IconSize, Layer, Radius, Surface, TextSize, colors};
use crate::ui::rich_text::RichText;
use crate::ui::{ActionButton, FadingScroll, Spinner, Variant, icons, sharing};

const NOTICE_RADIUS: f32 = Radius::LG;
const NOTICE_PADDING: f32 = 14.;
const TITLE_HEIGHT: f32 = 16.;
const CLOSE_SIZE: f32 = ControlSize::Regular.height();
/// Tucks the close button into the corner, its own corner running along the toast's.
const CLOSE_MARGIN: f32 = NOTICE_RADIUS - ControlSize::Regular.radius();
const TRAILING: f32 = CLOSE_MARGIN + CLOSE_SIZE + 4.;
/// How many lines of what a failed action printed show before they scroll.
const OUTPUT_LINES: f32 = 8.;
const OUTPUT_LINE_HEIGHT: f32 = 16.;
const OUTPUT_PADDING: f32 = 8.;
/// The room between the toast's buttons' lights, which their clicks share.
const BUTTON_GAP: f32 = 8.;

/// What the thread did, as its sidebar row says it: its word and its symbol.
fn alert_label(kind: AlertKind) -> (&'static str, &'static str) {
    match kind {
        AlertKind::Finished => ("Finished", "flag"),
        AlertKind::Failed => ("Failed", "circle-alert"),
        AlertKind::Approval => ("Needs approval", "shield"),
        AlertKind::Question => ("Has a question", "message-circle-question-mark"),
        AlertKind::Plan => ("Has a plan", "clipboard-list"),
        AlertKind::Limit => ("Usage limit reached", "clock"),
        AlertKind::Other => ("Needs you", "bell"),
    }
}

fn alert_color(kind: AlertKind, c: &Colors) -> Hsla {
    match kind {
        AlertKind::Finished => c.success,
        AlertKind::Failed => c.destructive,
        _ => c.warning,
    }
}

/// The toast's card, which its close button goes last in.
fn card(c: &Colors) -> Div {
    div()
        .relative()
        .max_w(px(320.))
        .py(px(NOTICE_PADDING - 1.))
        .pl(px(NOTICE_PADDING - 1.))
        .pr(px(TRAILING - 1.))
        .bg(c.popover)
        .rounded(px(NOTICE_RADIUS))
        .border_1()
        .border_color(c.border_popover)
        .block_mouse_except_scroll()
}

fn close_button(store: &Entity<Store>) -> impl IntoElement {
    let close = store.clone();
    div().absolute().top(px(CLOSE_MARGIN - 1.)).right(px(CLOSE_MARGIN - 1.)).child(
        ActionButton::icon("toast-close", "x", "Close")
            .symbol_size(ControlSize::Regular.small_symbol())
            .surface(Surface::Popover)
            .on_click(move |_, _, cx| {
                close.update(cx, |store, cx| {
                    store.dismiss_toast();
                    cx.notify();
                })
            }),
    )
}

/// What the thread did, as its sidebar row says it, then the thread and where it is.
fn alert_view(store: &Entity<Store>, toast: &Toast, alert: &AlertToast, c: &Colors) -> Div {
    let open = store.clone();
    let thread_id = alert.thread_id.clone();
    let (label, symbol) = alert_label(alert.kind);
    let color = alert_color(alert.kind, c);
    let place: Vec<String> =
        [&alert.project, &alert.server_name].into_iter().filter(|part| !part.is_empty()).cloned().collect();
    let mut place_line =
        div().pt(px(2.)).flex().text_size(TextSize::Xs).text_color(c.muted_foreground).whitespace_nowrap();
    for (index, part) in place.iter().enumerate() {
        if index > 0 {
            place_line = place_line.child(div().text_color(c.muted_stronger_foreground).child("\u{a0}·\u{a0}"));
        }
        place_line = place_line.child(part.clone());
    }
    card(c)
        .child(
            div()
                .flex()
                .flex_col()
                .items_start()
                .child(
                    div()
                        .h(px(TITLE_HEIGHT))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(TextSize::Xs)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color)
                        .child(icons::symbol(symbol, IconSize::Sm).text_color(color))
                        .child(label),
                )
                .child(
                    div()
                        .pt(px(8.))
                        .text_size(TextSize::Sm)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c.foreground)
                        .line_clamp(2)
                        .child(toast.title.clone()),
                )
                .when(!place.is_empty(), |column| column.child(place_line))
                .child(div().pt(px(10.)).child(
                    ActionButton::new("toast-thread", "Open").primary().surface(Surface::Popover).on_click(
                        move |_, _, cx| {
                            open.update(cx, |store, cx| {
                                store.open_alerted_thread(&thread_id, cx);
                                cx.notify();
                            })
                        },
                    ),
                )),
        )
        .child(close_button(store))
}

/// What follows is one click away: the push after a commit, the pull request after a push, the
/// thread in a project that was cloned, the thread an alert is of.
pub fn toast_view(store: &Entity<Store>, toast: &Toast, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    if let Some(alert) = &toast.alert {
        return alert_view(store, toast, alert, c);
    }
    let next = store.clone();
    let view = store.clone();
    let open = store.clone();
    let url = toast.url.clone();
    let next_label = toast.next_label();
    let opens_project = toast.project_id.is_some();
    let count = [url.is_some(), next_label.is_some(), opens_project].into_iter().filter(|shown| *shown).count();
    let next_index = usize::from(url.is_some());
    let project_index = next_index + usize::from(next_label.is_some());
    let (symbol, symbol_color) =
        if toast.failed { ("circle-alert", c.destructive) } else { ("circle-check", c.success) };
    let description = toast.description.clone().map(|description| {
        let text = RichText::new(SharedString::from(format!("toast-{}", toast.serial)), description);
        if !toast.failed {
            return div().text_size(TextSize::Xs).text_color(c.muted_foreground).line_clamp(2).child(text);
        }
        // The end is where a hook says what it found.
        let output = FadingScroll::new("toast-output", OUTPUT_LINES * OUTPUT_LINE_HEIGHT + 2. * OUTPUT_PADDING)
            .padding(OUTPUT_PADDING)
            .starts_at_end(true)
            .fill(Surface::Popover.layer(Layer::Box, c))
            .child(
                div()
                    .text_size(TextSize::Xs)
                    .line_height(px(OUTPUT_LINE_HEIGHT))
                    .font_family(crate::theme::MONO_FONT)
                    .text_color(c.muted_foreground)
                    .child(text),
            );
        div()
            .self_stretch()
            .mt(px(4.))
            .mr(px(-(TRAILING - NOTICE_PADDING)))
            .rounded(px(Radius::MD))
            .overflow_hidden()
            .bg(Surface::Popover.layer(Layer::Box, c))
            .border_1()
            .border_color(Surface::Popover.border(c))
            .child(output)
    });
    card(c)
        .flex()
        .items_start()
        .gap(px(10.))
        .child(div().h(px(TITLE_HEIGHT)).flex_shrink_0().flex().items_center().map(|icon| {
            if toast.pending {
                return icon.child(Spinner::new(IconSize::Md).render(cx));
            }
            icon.child(icons::symbol(symbol, IconSize::Md).text_color(symbol_color))
        }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(4.))
                .child(
                    div()
                        .min_h(px(TITLE_HEIGHT))
                        .flex()
                        .items_center()
                        .text_size(TextSize::Sm)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c.foreground)
                        .child(toast.title.clone()),
                )
                .children(description)
                .when(count > 0, |column| {
                    column.child(
                        div()
                            .pt(px(6.))
                            .flex()
                            .when_some(url, |row, url| {
                                let variant = if next_label.is_none() { Variant::Primary } else { Variant::Secondary };
                                row.child(
                                    ActionButton::new("toast-pr", "View PR")
                                        .variant(variant)
                                        .surface(Surface::Popover)
                                        .margin(sharing(BUTTON_GAP, 0, count))
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |store, cx| {
                                                store.show_pull_request(&url, cx);
                                                cx.notify();
                                            })
                                        }),
                                )
                            })
                            .when_some(next_label, |row, label| {
                                row.child(
                                    ActionButton::new("toast-next", label)
                                        .primary()
                                        .surface(Surface::Popover)
                                        .margin(sharing(BUTTON_GAP, next_index, count))
                                        .on_click(move |_, _, cx| {
                                            next.update(cx, |store, cx| {
                                                store.run_next_git(cx);
                                                cx.notify();
                                            })
                                        }),
                                )
                            })
                            .when(opens_project, |row| {
                                row.child(
                                    ActionButton::new("toast-project", "Open project")
                                        .primary()
                                        .surface(Surface::Popover)
                                        .margin(sharing(BUTTON_GAP, project_index, count))
                                        .on_click(move |_, _, cx| {
                                            open.update(cx, |store, cx| {
                                                store.open_toast_project(cx);
                                                cx.notify();
                                            })
                                        }),
                                )
                            }),
                    )
                }),
        )
        .child(close_button(store))
}
