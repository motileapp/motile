//! What the composer shows of the agents the thread's agent started: the strip over it while
//! some wait for the user, the bar in its place while one of them is shown, and the crumb before
//! the title that leads back to the thread they belong to.

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::WaitingChild;
use motile_core::render::agents::AgentView;

use super::RADIUS;
use super::strips::{HEIGHT, margin, strip};
use crate::models::{ask_label, ask_phrase};
use crate::panel::view::agent_time;
use crate::store::Store;
use crate::theme::{self, ControlSize, Surface, colors};
use crate::ui::{ActionButton, AgentAvatar, icons, text_width};

const TEXT_SIZE: f32 = 12.5;

/// Above the composer of a thread whose agent started agents that wait for the user: which, and
/// the way to the first of them, where it is answered.
pub fn children_strip(
    store: &Entity<Store>,
    text: String,
    first: &WaitingChild,
    count: usize,
    cx: &App,
) -> impl IntoElement {
    let c = colors(cx);
    let (word, symbol) = ask_label(first.ask);
    // A narrow composer says what is asked without naming the agent.
    let short = if count == 1 { format!("Agent {}", ask_phrase(first.ask)) } else { text.clone() };
    let (store, thread_id) = (store.clone(), first.thread_id.clone());
    strip(true, Some(HEIGHT), cx)
        .child(div().ml(px(14.)).mr(px(7.)).flex_shrink_0().child(icons::symbol(symbol, 13.).text_color(c.warning)))
        .child(
            div()
                .id("children-waiting")
                .min_w_0()
                .text_size(px(TEXT_SIZE))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.warning)
                .tooltip(crate::ui::tooltip(text.clone()))
                .child(ShortWhenNarrow { full: text.into(), short: short.into() }),
        )
        .child(div().flex_1().min_w(px(8.)))
        .child(
            ActionButton::new("open-waiting-child", format!("Open {word}"))
                .help(format!("Open {}", first.title))
                .ghost()
                .surface(Surface::Composer)
                .margin(margin())
                .on_click(move |_, _, cx| {
                    store.update(cx, |store, cx| {
                        store.open_started_thread(&thread_id, cx);
                        cx.notify();
                    })
                }),
        )
}

/// The whole text where it fits, else the short one, cut at its end.
#[derive(IntoElement)]
struct ShortWhenNarrow {
    full: SharedString,
    short: SharedString,
}

impl RenderOnce for ShortWhenNarrow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let width = window.use_state(cx, |_, _| std::rc::Rc::new(std::cell::Cell::new(f32::MAX)));
        let room = width.read(cx).clone();
        let fits = text_width(&self.full, TEXT_SIZE, FontWeight::MEDIUM, window) <= room.get();
        div()
            .relative()
            .w_full()
            .min_w_0()
            .child(
                canvas(
                    move |bounds, window, _| {
                        let width = f32::from(bounds.size.width);
                        if (room.get() - width).abs() > 0.5 {
                            room.set(width);
                            window.refresh();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .child(if fits { self.full } else { self.short })
    }
}

/// In place of the composer of an agent the thread's agent started, which runs on its own and
/// can't be written to: how it is doing, and the way back to the thread.
pub fn agent_bar(store: &Entity<Store>, agent: &AgentView, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let height = ControlSize::Large.height() + 8.;
    let store = store.clone();
    div()
        .w_full()
        .max_w(px(theme::COMPOSER_WIDTH))
        .h(px(height))
        .pl(px(16.))
        .pr(px((height - ControlSize::Regular.height()) / 2.))
        .flex()
        .items_center()
        .gap(px(8.))
        .bg(c.composer)
        .rounded(px(RADIUS))
        .border_1()
        .border_color(c.border_composer)
        .shadow(c.shadow(theme::ShadowSize::Lg, theme::Opacity::Shadow))
        .child(AgentAvatar::new(agent.agent).state(agent.state))
        .child(
            div()
                .min_w_0()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.foreground)
                .truncate()
                .child(agent.title.clone()),
        )
        .child(agent_time(agent, cx))
        .child(div().flex_1().min_w(px(8.)))
        .child(
            div().min_w_0().text_size(px(TEXT_SIZE)).text_color(c.muted_foreground).truncate().child("Runs on its own"),
        )
        .child(
            ActionButton::new("open-parent", "Open Parent")
                .symbol("corner-left-up")
                .ghost()
                .surface(Surface::Composer)
                .on_click(move |_, _, cx| {
                    store.update(cx, |store, cx| {
                        store.show_agents(cx);
                        cx.notify();
                    })
                }),
        )
}

/// The thread before a thread's title, as a button that opens it: the thread whose agent started
/// the open one, or the thread of the agent shown.
pub fn title_crumb(
    store: &Entity<Store>,
    title: &str,
    action: impl Fn(&mut Store, &mut Context<Store>) + 'static,
) -> ActionButton {
    let store = store.clone();
    ActionButton::new("title-crumb", title.to_string())
        .symbol("corner-left-up")
        .help(format!("Open {title}"))
        .ghost()
        .small()
        .truncates(true)
        .on_click(move |_, _, cx| {
            store.update(cx, |store, cx| {
                action(store, cx);
                cx.notify();
            })
        })
}
