import Foundation

/// How far the questions of a tool call are answered: the one that shows, the options chosen for
/// each, what was written for each in the composer, and the questions taking one answer whose
/// chosen answer is what was written. What was written stays when an option is chosen instead.
struct QuestionAnswers: Equatable {
    var index = 0
    var chosen: [String: Set<String>] = [:]
    var typed: [String: String] = [:]
    var writtenChosen: Set<String> = []
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

    /// Writing chooses what is written as the answer.
    func writeAnswer(_ text: String, to approval: Approval) {
        guard text != typedAnswer(approval) else { return }
        let id = question(approval).id
        var answers = questionAnswers[approval.id] ?? QuestionAnswers()
        answers.typed[id] = text.isEmpty ? nil : text
        answers.writtenChosen.insert(id)
        questionAnswers[approval.id] = answers
    }

    /// Whether what is written counts: beside the options of a question that takes several, or as
    /// the one answer when it was written or chosen last.
    func isWrittenChosen(_ approval: Approval) -> Bool {
        guard !typedAnswer(approval).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return false }
        let question = question(approval)
        return question.multiple || questionAnswers[approval.id]?.writtenChosen.contains(question.id) == true
    }

    func chooseWritten(in approval: Approval) {
        questionAnswers[approval.id, default: QuestionAnswers()].writtenChosen.insert(question(approval).id)
    }

    func isChosen(_ option: Question.Choice, in approval: Approval) -> Bool {
        guard question(approval).multiple || !isWrittenChosen(approval) else { return false }
        return questionAnswers[approval.id]?.chosen[question(approval).id]?.contains(option.label) == true
    }

    /// Chooses an option, in place of what was written when it is the only answer, which goes on to
    /// the next question once it has been seen.
    func choose(_ option: Question.Choice, in approval: Approval) {
        let question = question(approval)
        let index = questionIndex(approval)
        var answers = questionAnswers[approval.id] ?? QuestionAnswers()
        if !question.multiple { answers.writtenChosen.remove(question.id) }
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
            let typed = (state.typed[question.id] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
            let written = question.multiple || state.writtenChosen.contains(question.id) ? typed : ""
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
