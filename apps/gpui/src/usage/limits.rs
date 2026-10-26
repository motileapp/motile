//! How much of their plans the agents' logins have used: a section for each login, its windows in
//! one card, each with a bar of what was used.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::models::agent_name;
use crate::store::usage::{LimitWindow, LimitsReport, LimitsSection, Pace, Tone};
use crate::theme::{IconSize, Layer, Opacity, Radius, Surface, colors};
use crate::ui::{Chip, card_default, divider_on, icons, logos, tabular_numbers, tooltip};

/// Narrower than this, a window puts its bar under its numbers.
const BESIDE_WIDTH: f32 = 520.;
const BAR_HEIGHT: f32 = 28.;
const NUMBERS_WIDTH: f32 = 168.;

pub fn limits(report: &LimitsReport, width: f32, cx: &App) -> AnyElement {
    let c = colors(cx);
    let beside = width >= BESIDE_WIDTH;
    div()
        .flex()
        .flex_col()
        .gap(px(40.))
        .when(report.sections.is_empty() && report.notes.is_empty(), |view| {
            view.child(super::note("No agent is installed on these servers.", cx))
        })
        .children(report.sections.iter().enumerate().map(|(index, section)| {
            let windows = (!section.windows.is_empty()).then(|| {
                let last = section.windows.len() - 1;
                card_default(div().flex().flex_col(), cx).children(section.windows.iter().enumerate().flat_map(
                    |(place, window)| {
                        let row = limit_row((index, place), window, beside, cx);
                        let divider = (place < last).then(|| divider_on(Surface::Card, cx).into_any_element());
                        std::iter::once(row).chain(divider)
                    },
                ))
            });
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(header(section, cx))
                .when_some(section.note.clone(), |section, note| {
                    section.child(card_default(
                        div().w_full().p(px(16.)).text_size(px(12.)).text_color(c.muted_foreground).child(note),
                        cx,
                    ))
                })
                .children(windows)
        }))
        .children(
            report.notes.iter().map(|note| div().text_size(px(10.)).text_color(c.muted_foreground).child(note.clone())),
        )
        .into_any_element()
}

fn header(section: &LimitsSection, cx: &App) -> Div {
    let c = colors(cx);
    let login = match (&section.name, &section.account) {
        (Some(name), Some(account)) => Some(format!("{name} · {account}")),
        (Some(name), None) => Some(name.clone()),
        (None, Some(account)) => Some(account.clone()),
        (None, None) => None,
    };
    div()
        .px(px(4.))
        .pb(px(2.))
        .flex()
        .items_center()
        .gap(px(8.))
        .child(logos::agent_icon(section.agent, IconSize::Md, cx))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(14.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.foreground)
                .child(agent_name(section.agent)),
        )
        .when_some(section.plan.clone(), |header, plan| header.child(Chip::new(plan)))
        .when_some(login, |header, login| {
            header.child(
                div()
                    .min_w_0()
                    .text_size(px(12.))
                    .text_color(c.muted_foreground)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis_middle()
                    .child(crate::ui::control::dotted(login, c)),
            )
        })
}

/// One window of a plan: what was used of it, how that compares with the time passed, and a bar
/// with when it starts over.
fn limit_row(id: (usize, usize), window: &LimitWindow, beside: bool, cx: &App) -> AnyElement {
    let c = colors(cx);
    let tone = match window.tone {
        Tone::Success => c.success,
        Tone::Pending => c.pending,
        Tone::Warning => c.warning,
    };
    let numbers = div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.foreground)
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(window.label.clone()),
        )
        .child(
            div()
                .flex()
                .items_baseline()
                .gap(px(5.))
                .child(
                    div()
                        .text_size(px(28.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .font_features(tabular_numbers())
                        .text_color(c.foreground)
                        .child(window.used.clone()),
                )
                .child(div().text_size(px(13.)).text_color(c.muted_foreground).child("used")),
        )
        .when_some(window.pace, |numbers, pace| {
            let (symbol, label, help) = match pace {
                Pace::Ahead => ("trending-up", "Ahead of pace", "Used faster than the window passes"),
                Pace::On => ("gauge", "On pace", "Used as fast as the window passes"),
                Pace::Under => ("trending-down", "Under pace", "Room left for the rest of the window"),
            };
            numbers.child(
                div()
                    .id(("pace", id.0 * 100 + id.1))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .text_size(px(12.))
                    .text_color(c.muted_stronger_foreground)
                    .child(icons::symbol(symbol, IconSize::Sm))
                    .child(label)
                    .tooltip(tooltip(help)),
            )
        });
    let bar = limit_bar(id, window, tone, cx);
    let row = div().px(px(16.)).py(px(14.)).flex();
    let row = if beside {
        row.items_center().gap(px(24.)).child(numbers.w(px(NUMBERS_WIDTH)).flex_shrink_0()).child(bar)
    } else {
        row.flex_col().gap(px(12.)).child(numbers).child(bar)
    };
    row.into_any_element()
}

/// What was used of a window as the part of a bar it fills, and at its end when the window starts
/// over and the resets the login may use.
fn limit_bar(id: (usize, usize), window: &LimitWindow, tone: Hsla, cx: &App) -> Div {
    let c = colors(cx);
    let used = (window.used_percent / 100.).clamp(0., 1.) as f32;
    let credits = window.reset_credits;
    let end = div()
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .when_some(window.resets_in.clone(), |end, resets_in| {
            end.child(
                div()
                    .id(("resets", id.0 * 100 + id.1))
                    .child(resets_in.clone())
                    .tooltip(tooltip(format!("Starts over in {resets_in}"))),
            )
        })
        .when(window.resets_in.is_some() && credits > 0, |end| end.child("·"))
        .when(credits > 0, |end| {
            let help = if credits == 1 { "1 reset to use".to_string() } else { format!("{credits} resets to use") };
            end.child(
                div()
                    .id(("credits", id.0 * 100 + id.1))
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .child(icons::symbol("ticket", IconSize::Sm))
                    .child(credits.to_string())
                    .tooltip(tooltip(help)),
            )
        });
    div()
        .relative()
        .flex_1()
        .min_w_0()
        .w_full()
        .h(px(BAR_HEIGHT))
        .rounded(px(Radius::MD))
        .bg(Surface::Card.layer(Layer::Control, c))
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w(relative(used))
                .rounded(px(Radius::MD))
                .bg(c.at(tone, Opacity::ColorTintChart)),
        )
        .child(
            div()
                .relative()
                .size_full()
                .px(px(9.))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_color(c.foreground)
                .font_features(tabular_numbers())
                .child(div().text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).child(window.used.clone()))
                .child(div().flex_1())
                .child(end),
        )
}
