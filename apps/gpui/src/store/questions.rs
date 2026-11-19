//! How far the questions of a tool call are answered: the one that shows, the options chosen for
//! each, what was written for each in the composer, and the questions taking one answer whose
//! chosen answer is what was written. What was written stays when an option is chosen instead.

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use gpui_kit::prelude::*;
use motile_core::render::rows::{Question, Waiting};

use super::Store;

#[derive(Clone, Default)]
pub struct QuestionAnswers {
    index: usize,
    chosen: HashMap<String, BTreeSet<String>>,
    typed: HashMap<String, String>,
    written_chosen: BTreeSet<String>,
}

impl Store {
    /// The tool call with questions the open thread's agent waits on. While it shows, the composer
    /// writes the answer to its question in place of a message.
    pub fn asking(&self) -> Option<&Waiting> {
        self.selected_thread()?;
        self.activity.approvals.first().filter(|approval| !approval.questions.is_empty())
    }

    pub fn question_index(&self, approval: &Waiting) -> usize {
        let index = self.question_answers.get(&approval.id).map_or(0, |answers| answers.index);
        index.min(approval.questions.len().saturating_sub(1))
    }

    pub fn question<'a>(&self, approval: &'a Waiting) -> &'a Question {
        &approval.questions[self.question_index(approval)]
    }

    pub fn is_last_question(&self, approval: &Waiting) -> bool {
        self.question_index(approval) + 1 >= approval.questions.len()
    }

    /// What was written for the question that shows.
    pub fn typed_answer(&self, approval: &Waiting) -> String {
        let question = &self.question(approval).text;
        let answers = self.question_answers.get(&approval.id);
        answers.and_then(|answers| answers.typed.get(question)).cloned().unwrap_or_default()
    }

    /// Writing chooses what is written as the answer.
    pub(super) fn write_answer(&mut self, text: String) {
        let Some(approval) = self.asking() else { return };
        if text == self.typed_answer(approval) {
            return;
        }
        let (id, question) = (approval.id.clone(), self.question(approval).text.clone());
        let answers = self.question_answers.entry(id).or_default();
        answers.written_chosen.insert(question.clone());
        if text.is_empty() {
            answers.typed.remove(&question);
        } else {
            answers.typed.insert(question, text);
        }
    }

    /// Whether what is written counts: beside the options of a question that takes several, or as
    /// the one answer when it was written or chosen last.
    pub fn is_written_chosen(&self, approval: &Waiting) -> bool {
        if self.typed_answer(approval).trim().is_empty() {
            return false;
        }
        let question = self.question(approval);
        let answers = self.question_answers.get(&approval.id);
        question.multiple || answers.is_some_and(|answers| answers.written_chosen.contains(&question.text))
    }

    pub fn choose_written(&mut self) {
        let Some(approval) = self.asking() else { return };
        let (id, question) = (approval.id.clone(), self.question(approval).text.clone());
        self.question_answers.entry(id).or_default().written_chosen.insert(question);
    }

    pub fn is_chosen(&self, approval: &Waiting, label: &str) -> bool {
        if !self.question(approval).multiple && self.is_written_chosen(approval) {
            return false;
        }
        let question = &self.question(approval).text;
        let answers = self.question_answers.get(&approval.id);
        answers.and_then(|answers| answers.chosen.get(question)).is_some_and(|chosen| chosen.contains(label))
    }

    /// Chooses an option, in place of what was written when it is the only answer, which goes on to
    /// the next question once it has been seen.
    pub fn choose(&mut self, label: String, cx: &mut Context<Self>) {
        let Some(approval) = self.asking() else { return };
        let (id, question, index) =
            (approval.id.clone(), self.question(approval).clone(), self.question_index(approval));
        let advances = !question.multiple && !self.is_last_question(approval);
        let answers = self.question_answers.entry(id.clone()).or_default();
        if !question.multiple {
            answers.written_chosen.remove(&question.text);
        }
        let picked = answers.chosen.entry(question.text).or_default();
        if !question.multiple {
            picked.clear();
            picked.insert(label);
        } else if !picked.remove(&label) {
            picked.insert(label);
        }
        if !advances {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(200)).await;
            let _ = this.update(cx, |store, cx| {
                let Some(answers) = store.question_answers.get_mut(&id).filter(|answers| answers.index == index) else {
                    return;
                };
                answers.index = index + 1;
                cx.notify();
            });
        })
        .detach();
    }

    /// The options chosen for each question, in their order, and what was written: after them, or in
    /// their place where a question takes one.
    pub fn answers(&self, approval: &Waiting) -> HashMap<String, String> {
        let state = self.question_answers.get(&approval.id).cloned().unwrap_or_default();
        let mut answers = HashMap::new();
        for question in &approval.questions {
            let typed = state.typed.get(&question.text).map(|text| text.trim()).unwrap_or_default();
            let written = if question.multiple || state.written_chosen.contains(&question.text) { typed } else { "" };
            let chosen = state.chosen.get(&question.text);
            let picked: Vec<&str> = question
                .options
                .iter()
                .map(|option| option.label.as_str())
                .filter(|label| chosen.is_some_and(|chosen| chosen.contains(*label)))
                .collect();
            let parts = if question.multiple {
                [picked, vec![written]].concat()
            } else if written.is_empty() {
                picked.into_iter().take(1).collect()
            } else {
                vec![written]
            };
            let answer = parts.into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join(", ");
            if !answer.is_empty() {
                answers.insert(question.text.clone(), answer);
            }
        }
        answers
    }

    pub fn can_go_on(&self, approval: &Waiting) -> bool {
        let answers = self.answers(approval);
        if !self.is_last_question(approval) {
            return answers.contains_key(&self.question(approval).text);
        }
        answers.len() == approval.questions.len()
    }

    /// Goes on to the next question, or answers them all after the last.
    pub fn go_on(&mut self) {
        let Some(approval) = self.asking().filter(|approval| self.can_go_on(approval)) else { return };
        if !self.is_last_question(approval) {
            let (id, index) = (approval.id.clone(), self.question_index(approval));
            self.question_answers.entry(id).or_default().index = index + 1;
            return;
        }
        let (id, answers) = (approval.id.clone(), self.answers(approval));
        self.answer(&id, true, answers);
    }

    pub fn go_back(&mut self) {
        let Some(approval) = self.asking() else { return };
        let (id, index) = (approval.id.clone(), self.question_index(approval));
        if index == 0 {
            return;
        }
        self.question_answers.entry(id).or_default().index = index - 1;
    }
}
