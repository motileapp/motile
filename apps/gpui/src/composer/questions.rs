//! The questions an agent asks with a tool call. Each takes one of its options, several of them
//! when it allows that, or an answer typed in place of them.

use std::collections::{BTreeSet, HashMap};

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::rows::Waiting;

use crate::store::Store;
use crate::theme::colors;
use crate::ui::button::Button;
use crate::ui::icons;

const OPTION_GAP: f32 = 6.;

pub struct Questions {
    store: Entity<Store>,
    approval: Waiting,
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
        Self { store, approval, chosen: HashMap::new(), typed, _subscriptions: subscriptions }
    }

    /// What was typed for a question, or else the options chosen for it, in their order.
    fn answers(&self, cx: &App) -> HashMap<String, String> {
        let mut answers = HashMap::new();
        for (index, question) in self.approval.questions.iter().enumerate() {
            let written = self.typed[index].read(cx).value().trim().to_string();
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

    fn choose(&mut self, question: usize, label: String, cx: &mut Context<Self>) {
        let question = &self.approval.questions[question];
        let picked = self.chosen.entry(question.text.clone()).or_default();
        if !question.multiple {
            picked.clear();
            picked.insert(label);
        } else if !picked.remove(&label) {
            picked.insert(label);
        }
        cx.notify();
    }
}

impl Render for Questions {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let answers = self.answers(cx);
        let complete = answers.len() >= self.approval.questions.len();
        let (refuse, allow) = (self.store.clone(), self.store.clone());
        let (refuse_id, allow_id) = (self.approval.id.clone(), self.approval.id.clone());
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .children(self.approval.questions.iter().enumerate().map(|(index, question)| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(OPTION_GAP))
                    .child(div().font_weight(FontWeight::MEDIUM).child(question.text.clone()))
                    .children(question.options.iter().enumerate().map(|(option_index, option)| {
                        let on = self.chosen.get(&question.text).is_some_and(|chosen| chosen.contains(&option.label));
                        let symbol = match (question.multiple, on) {
                            (true, true) => "checkmark.square.fill",
                            (true, false) => "square",
                            (false, true) => "largecircle.fill.circle",
                            (false, false) => "circle",
                        };
                        let label = option.label.clone();
                        div()
                            .id(SharedString::from(format!("option-{index}-{option_index}")))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(icons::symbol(symbol, 12.).text_color(if on { c.primary } else { c.secondary }))
                            .child(div().flex_shrink_0().child(option.label.clone()))
                            .child(div().min_w_0().truncate().text_color(c.secondary).child(option.detail.clone()))
                            .on_click(cx.listener(move |this, _, _, cx| this.choose(index, label.clone(), cx)))
                    }))
                    .child(crate::ui::text_field(&self.typed[index], cx))
            }))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(div().flex_1())
                    .child(Button::new("refuse-questions", self.approval.refuse).small().on_click(move |_, _, cx| {
                        refuse.update(cx, |store, cx| {
                            store.answer(&refuse_id, false, HashMap::new());
                            cx.notify();
                        })
                    }))
                    .child(
                        Button::new("allow-questions", self.approval.allow)
                            .small()
                            .prominent()
                            .disabled(!complete)
                            .on_click(move |_, _, cx| {
                                let answers = answers.clone();
                                allow.update(cx, |store, cx| {
                                    store.answer(&allow_id, true, answers);
                                    cx.notify();
                                })
                            }),
                    ),
            )
    }
}
