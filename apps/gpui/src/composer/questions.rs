//! The questions an agent asks with a tool call, one at a time. Each takes one of its options,
//! several of them when it allows that, or an answer written in the composer in place of them.

use std::collections::HashMap;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::rows::{Choice, Waiting};

use crate::composer::strips::waiting_title;
use crate::store::Store;
use crate::theme::{IconSize, Layer, Radius, Surface, TextSize, colors};
use crate::ui::{ActionButton, Chip, PickMark, Variant, beside, icons};

/// How far past its text an option's light and the room to press it reach.
const OPTION_REACH: f32 = 8.;
const OPTION_GAP: f32 = 2.;
const MARK_GAP: f32 = 10.;
const OWN_ANSWER_LINES: usize = 3;

pub fn questions(store: &Entity<Store>, approval: &Waiting, cx: &App) -> Div {
    let c = colors(cx);
    let state = store.read(cx);
    let count = approval.questions.len();
    let index = state.question_index(approval);
    let question = state.question(approval);
    let can_go_on = state.can_go_on(approval);
    let place = (count > 1).then(|| format!("{} of {}", index + 1, count));
    let title = if question.header.is_empty() { approval.title.clone() } else { question.header.clone() };
    let go_on = if state.is_last_question(approval) { approval.allow } else { "Next" };
    let options = question.options.iter().enumerate().map(|(position, option)| {
        let picked = state.is_chosen(approval, &option.label);
        option_row(store, index, position, option, picked, question.multiple, cx)
    });
    let typed = state.typed_answer(approval).trim().to_string();
    let (back, refuse, next) = (store.clone(), store.clone(), store.clone());
    let refuse_id = approval.id.clone();
    div()
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(waiting_title(title, "message-circle-question-mark", place, cx).text_color(c.warning))
                .child(
                    div()
                        .text_size(TextSize::Sm)
                        .line_height(px(19.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c.foreground)
                        .child(question.text.clone()),
                )
                .when(question.multiple, |text| {
                    text.child(div().text_color(c.muted_foreground).child("Choose any that apply"))
                }),
        )
        .child(div().mx(px(-OPTION_REACH)).flex().flex_col().gap(px(OPTION_GAP)).children(options).child(own_answer(
            store,
            index,
            typed,
            question.multiple,
            cx,
        )))
        .child(
            div()
                .flex()
                .items_center()
                .when(index > 0, |row| {
                    row.child(
                        ActionButton::new("back", "Back")
                            .symbol("chevron-left")
                            .ghost()
                            .surface(Surface::Composer)
                            .margin(beside(0., 8.))
                            .on_click(move |_, _, cx| {
                                back.update(cx, |store, cx| {
                                    store.go_back();
                                    cx.notify();
                                })
                            }),
                    )
                })
                .child(div().flex_1())
                .child(
                    ActionButton::new("refuse-questions", approval.refuse)
                        .ghost()
                        .surface(Surface::Composer)
                        .margin(beside(8., 4.))
                        .on_click(move |_, _, cx| {
                            refuse.update(cx, |store, cx| {
                                store.answer(&refuse_id, false, HashMap::new());
                                cx.notify();
                            })
                        }),
                )
                .child(
                    ActionButton::new("go-on", go_on)
                        .variant(Variant::Primary)
                        .surface(Surface::Composer)
                        .disabled(!can_go_on)
                        .margin(beside(4., 0.))
                        .on_click(move |_, _, cx| {
                            next.update(cx, |store, cx| {
                                store.go_on();
                                cx.notify();
                            })
                        }),
                ),
        )
}

/// A row that lights under the pointer, and once it is the answer takes the primary colour's wash
/// inside a line of its stronger wash.
fn row(id: SharedString, picked: bool, cx: &App) -> (Stateful<Div>, Div) {
    let c = colors(cx);
    let light = Surface::Composer.layer(Layer::Row, c);
    let pressed = div().id(id).w_full().rounded(px(Radius::MD)).hover(move |row| row.bg(light));
    let face = div()
        .w_full()
        .py(px(8.))
        .px(px(OPTION_REACH))
        .rounded(px(Radius::MD))
        .border_1()
        .border_color(if picked { c.wash(c.primary, true) } else { transparent_black() })
        .when(picked, |face| face.bg(c.wash(c.primary, false)))
        .flex()
        .items_start()
        .gap(px(MARK_GAP));
    (pressed, face)
}

/// The mark's place, on the first line of the words beside it.
fn mark_place(mark: impl IntoElement) -> Div {
    div().h(px(18.)).flex().items_center().child(mark)
}

fn option_row(
    store: &Entity<Store>,
    index: usize,
    position: usize,
    option: &Choice,
    picked: bool,
    multiple: bool,
    cx: &App,
) -> Stateful<Div> {
    let c = colors(cx);
    let (pressed, face) = row(SharedString::from(format!("option-{position}")), picked, cx);
    let label = option.label.clone();
    let store = store.clone();
    let mark = PickMark::new(SharedString::from(format!("pick-{index}-{position}")), picked, multiple);
    let face = face.child(mark_place(mark)).child(
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .child(
                        div()
                            .min_w_0()
                            .text_size(TextSize::Sm)
                            .line_height(px(18.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c.foreground)
                            .child(option.label.clone()),
                    )
                    .when(option.recommended, |line| {
                        line.child(Chip::new("Recommended").tone(c.success).compact(true))
                    }),
            )
            .when(!option.detail.is_empty(), |text| {
                text.child(
                    div()
                        .text_size(TextSize::Xs)
                        .line_height(px(16.))
                        .text_color(c.muted_foreground)
                        .child(option.detail.clone()),
                )
            }),
    );
    pressed.child(face).on_click(move |_, _, cx| {
        store.update(cx, |store, cx| {
            store.choose(label.clone(), cx);
            cx.notify();
        })
    })
}

/// The last row, which shows the answer written in the composer, or says to write one there.
fn own_answer(store: &Entity<Store>, index: usize, typed: String, multiple: bool, cx: &App) -> Stateful<Div> {
    let c = colors(cx);
    let written = !typed.is_empty();
    let (pressed, face) = row(SharedString::from("own-answer"), written, cx);
    let store = store.clone();
    let mark = if written {
        PickMark::new(SharedString::from(format!("pick-{index}-own")), true, multiple).into_any_element()
    } else {
        div()
            .w(px(PickMark::SIZE))
            .flex()
            .justify_center()
            .child(icons::symbol("pencil-line", IconSize::Sm).text_color(c.muted_foreground))
            .into_any_element()
    };
    let hint = if multiple { "Add your own answer below" } else { "Or type your own answer below" };
    let words = if written { typed } else { hint.to_string() };
    let face = face.child(mark_place(mark)).child(
        div()
            .flex_1()
            .min_w_0()
            .text_size(TextSize::Sm)
            .line_height(px(18.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(if written { c.foreground } else { c.muted_foreground })
            .line_clamp(OWN_ANSWER_LINES)
            .child(words),
    );
    pressed.child(face).on_click(move |_, _, cx| {
        store.update(cx, |store, cx| {
            store.composer_focus += 1;
            cx.notify();
        })
    })
}
