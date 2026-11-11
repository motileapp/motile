import Foundation

/// How far the questions of a tool call are answered: the one that shows, the options chosen for
/// each, and what was written for each in the composer: in place of the one option a question
/// takes, or beside those of one that takes several.
struct QuestionAnswers: Equatable {
    var index = 0
    var chosen: [String: Set<String>] = [:]
    var typed: [String: String] = [:]
}

extension AppStore {
    /// The tool call with questions the open thread's agent waits on. While it shows, the composer
    /// writes the answer to its question in place of a message.
    var asking: Approval? {
        guard selectedThread != nil, let approval = activity.approvals.first, !approval.questions.isEmpty else { return nil }
        return approval
    }

    func questionIndex(_ approval: Approval) -> Int {
        min(questionAnswers[approval.id]?.index ?? 0, approval.questions.count - 1)
    }

    func question(_ approval: Approval) -> Question {
        approval.questions[questionIndex(approval)]
    }

    func isLastQuestion(_ approval: Approval) -> Bool {
        questionIndex(approval) >= approval.questions.count - 1
    }

    /// What was written for the question that shows.
    func typedAnswer(_ approval: Approval) -> String {
        questionAnswers[approval.id]?.typed[question(approval).id] ?? ""
    }

    func writeAnswer(_ text: String, to approval: Approval) {
        let id = question(approval).id
        questionAnswers[approval.id, default: QuestionAnswers()].typed[id] = text.isEmpty ? nil : text
    }

    /// What is written counts in place of the one option a question takes, so none shows as chosen beside it.
    func isChosen(_ option: Question.Choice, in approval: Approval) -> Bool {
        let written = !typedAnswer(approval).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        guard question(approval).multiple || !written else { return false }
        return questionAnswers[approval.id]?.chosen[question(approval).id]?.contains(option.label) == true
    }

    /// Chooses an option, in place of what was written when it is the only answer, which goes on to
    /// the next question once it has been seen.
    func choose(_ option: Question.Choice, in approval: Approval) {
        let question = question(approval)
        let index = questionIndex(approval)
        var answers = questionAnswers[approval.id] ?? QuestionAnswers()
        if !question.multiple { answers.typed[question.id] = nil }
        var picked = answers.chosen[question.id] ?? []
        if question.multiple {
            if !picked.insert(option.label).inserted { picked.remove(option.label) }
        } else {
            picked = [option.label]
        }
        answers.chosen[question.id] = picked
        questionAnswers[approval.id] = answers
        guard !question.multiple, !isLastQuestion(approval) else { return }
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(200))
            guard let self, questionAnswers[approval.id]?.index ?? 0 == index else { return }
            questionAnswers[approval.id]?.index = index + 1
        }
    }

    /// The options chosen for each question, in their order, and what was written: after them, or in
    /// their place where a question takes one.
    func answers(_ approval: Approval) -> [String: String] {
        let state = questionAnswers[approval.id] ?? QuestionAnswers()
        var answers: [String: String] = [:]
        for question in approval.questions {
            let written = (state.typed[question.id] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
            let picked = question.options.map(\.label).filter { state.chosen[question.id]?.contains($0) == true }
            let parts = question.multiple ? picked + [written] : [written.isEmpty ? picked.first ?? "" : written]
            let answer = parts.filter { !$0.isEmpty }.joined(separator: ", ")
            if !answer.isEmpty { answers[question.text] = answer }
        }
        return answers
    }

    func canGoOn(_ approval: Approval) -> Bool {
        let answers = answers(approval)
        guard isLastQuestion(approval) else { return answers[question(approval).text] != nil }
        return answers.count == approval.questions.count
    }

    /// Goes on to the next question, or answers them all after the last.
    func goOn(_ approval: Approval) {
        guard canGoOn(approval) else { return }
        guard isLastQuestion(approval) else {
            questionAnswers[approval.id, default: QuestionAnswers()].index = questionIndex(approval) + 1
            return
        }
        answer(approval, allow: true, answers: answers(approval))
    }

    func goBack(_ approval: Approval) {
        let index = questionIndex(approval)
        guard index > 0 else { return }
        questionAnswers[approval.id]?.index = index - 1
    }
}
