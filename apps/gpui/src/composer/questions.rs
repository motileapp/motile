//! The questions an agent asks with a tool call, one at a time. Each takes one of its options,
//! several of them when it allows that, or an answer typed in place of them.

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::rows::{Question, Waiting};

use crate::composer::strips::waiting_title;
use crate::store::Store;
use crate::theme::{ControlSize, Surface, colors};
use crate::ui::{ActionButton, InputField, Variant, icons};

/// How far past its text an option's light and the room to press it reach.
const OPTION_REACH: f32 = 8.;

pub struct Questions {
    store: Entity<Store>,
    approval: Waiting,
    index: usize,
    chosen: HashMap<String, BTreeSet<String>>,
    typed: Vec<Entity<InputState>>,
    _subscriptions: Vec<Subscription>,
}

impl Questions {
    pub fn new(store: Entity<Store>, approval: Waiting, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let typed: Vec<Entity<InputState>> = approval
            .questions
            .iter()
            .map(|_| cx.new(|cx| InputState::new(window, cx).placeholder("Something else")))
            .collect();
        let subscriptions =
            typed.iter().map(|input| cx.subscribe(input, |_, _, _: &InputEvent, cx| cx.notify())).collect();
        Self { store, approval, index: 0, chosen: HashMap::new(), typed, _subscriptions: subscriptions }
    }

    fn question(&self) -> &Question {
        &self.approval.questions[self.index.min(self.approval.questions.len() - 1)]
    }

    fn is_last(&self) -> bool {
        self.index + 1 >= self.approval.questions.len()
    }

    fn written(&self, index: usize, cx: &App) -> String {
        self.typed[index].read(cx).value().trim().to_string()
    }

    /// What was typed for a question, or else the options chosen for it, in their order.
    fn answers(&self, cx: &App) -> HashMap<String, String> {
        let mut answers = HashMap::new();
        for (index, question) in self.approval.questions.iter().enumerate() {
            let written = self.written(index, cx);
            let picked: Vec<String> = question
                .options
                .iter()
                .map(|option| option.label.clone())
                .filter(|label| self.chosen.get(&question.text).is_some_and(|chosen| chosen.contains(label)))
                .collect();
            let answer = if written.is_empty() { picked.join(", ") } else { written };
            if !answer.is_empty() {
                answers.insert(question.text.clone(), answer);
            }
        }
        answers
    }

    /// What is typed counts in place of the options, so none shows as chosen beside it.
    fn is_chosen(&self, label: &str, cx: &App) -> bool {
        if !self.written(self.index, cx).is_empty() {
            return false;
        }
        self.chosen.get(&self.question().text).is_some_and(|chosen| chosen.contains(label))
    }

    fn choose(&mut self, label: String, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.index;
        let question = self.question().clone();
        self.typed[index].update(cx, |typed, cx| typed.set_value("", window, cx));
        let picked = self.chosen.entry(question.text.clone()).or_default();
        if !question.multiple {
            picked.clear();
            picked.insert(label);
            self.show_next(index, cx);
        } else if !picked.remove(&label) {
            picked.insert(label);
        }
        cx.notify();
    }

    /// Goes on to the next question once the choice has been seen.
    fn show_next(&mut self, asked: usize, cx: &mut Context<Self>) {
        if self.is_last() {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(200)).await;
            let _ = this.update(cx, |this, cx| {
                if this.index != asked {
                    return;
                }
                this.index += 1;
                cx.notify();
            });
        })
        .detach();
    }

    fn option_row(&self, option_index: usize, label: String, detail: String, cx: &mut Context<Self>) -> Stateful<Div> {
        let c = colors(cx);
        let picked = self.is_chosen(&label, cx);
        let light = Surface::Composer.next().color(c);
        div()
            .id(("option", option_index))
            .w_full()
            .py(px(6.))
            .px(px(OPTION_REACH))
            .rounded(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .when(picked, |row| row.bg(light))
            .when(!picked, |row| row.hover(move |row| row.bg(light)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .child(div().font_weight(FontWeight::MEDIUM).child(label.clone()))
                    .when(!detail.is_empty(), |text| {
                        text.child(div().text_size(px(11.5)).text_color(c.secondary).child(detail))
                    }),
            )
            .child(icons::symbol("check", 13.).text_color(c.primary.opacity(if picked { 1. } else { 0. })))
            .on_click(cx.listener(move |this, _, window, cx| this.choose(label.clone(), window, cx)))
    }
}

impl Render for Questions {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let count = self.approval.questions.len();
        let question = self.question().clone();
        let answers = self.answers(cx);
        let complete = answers.len() >= count;
        let answered = answers.contains_key(&question.text);
        let place = (count > 1).then(|| format!("{} of {}", self.index + 1, count));
        let (refuse, allow) = (self.store.clone(), self.store.clone());
        let (refuse_id, allow_id) = (self.approval.id.clone(), self.approval.id.clone());
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                waiting_title(self.approval.title.clone(), "message-circle-question-mark", place, cx)
                    .text_color(c.secondary),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(question.text.clone()))
                    .when(question.multiple, |text| {
                        text.child(div().text_color(c.secondary).child("Choose any that apply"))
                    }),
            )
            .child(
                div().mx(px(-OPTION_REACH)).flex().flex_col().gap(px(2.)).children(
                    question
                        .options
                        .iter()
                        .enumerate()
                        .map(|(index, option)| self.option_row(index, option.label.clone(), option.detail.clone(), cx)),
                ),
            )
            .child(InputField::new(&self.typed[self.index]).size(ControlSize::Small).surface(Surface::Composer))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when(self.index > 0, |row| {
                        row.child(
                            ActionButton::new("back", "Back")
                                .symbol("chevron-left")
                                .ghost()
                                .small()
                                .surface(Surface::Composer)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.index -= 1;
                                    cx.notify();
                                })),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        ActionButton::new("refuse-questions", self.approval.refuse)
                            .small()
                            .surface(Surface::Composer)
                            .on_click(move |_, _, cx| {
                                refuse.update(cx, |store, cx| {
                                    store.answer(&refuse_id, false, HashMap::new());
                                    cx.notify();
                                })
                            }),
                    )
                    .when(self.is_last(), |row| {
                        row.child(
                            ActionButton::new("allow-questions", self.approval.allow)
                                .variant(Variant::Primary)
                                .small()
                                .surface(Surface::Composer)
                                .disabled(!complete)
                                .on_click(move |_, _, cx| {
                                    let answers = answers.clone();
                                    allow.update(cx, |store, cx| {
                                        store.answer(&allow_id, true, answers);
                                        cx.notify();
                                    })
                                }),
                        )
                    })
                    .when(!self.is_last(), |row| {
                        row.child(
                            ActionButton::new("next-question", "Next")
                                .variant(Variant::Primary)
                                .small()
                                .surface(Surface::Composer)
                                .disabled(!answered)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.index += 1;
                                    cx.notify();
                                })),
                        )
                    }),
            )
    }
}
