import SwiftUI

/// The questions an agent asks with a tool call, one at a time. Each takes one of its options,
/// several of them when it allows that, or an answer typed in place of them.
struct QuestionsView: View {
    @Environment(AppStore.self) private var store
    let approval: Approval
    @State private var index = 0
    @State private var chosen: [String: Set<String>] = [:]
    @State private var typed: [String: String] = [:]

    /// How far past its text an option's light and the room to press it reach.
    private static let optionReach: CGFloat = 8

    private var question: Question { approval.questions[min(index, approval.questions.count - 1)] }
    private var isLast: Bool { index >= approval.questions.count - 1 }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            WaitingTitle(
                approval.title, symbol: .messageCircleQuestionMark,
                place: approval.questions.count > 1 ? "\(index + 1) of \(approval.questions.count)" : nil
            )
            .foregroundStyle(Color.themeMutedForeground)
            VStack(alignment: .leading, spacing: 2) {
                Text(question.text)
                    .font(.ui(size: 13, weight: .medium))
                    .fixedSize(horizontal: false, vertical: true)
                if question.multiple {
                    Text("Choose any that apply")
                        .foregroundStyle(Color.themeMutedForeground)
                }
            }
            VStack(spacing: 2) {
                ForEach(question.options) { option in
                    row(option)
                }
            }
            .padding(.horizontal, -Self.optionReach)
            InputField("Something else", text: typedAnswer, size: .small)
            HStack(spacing: 8) {
                if index > 0 {
                    ActionButton("Back", icon: .chevronLeft, variant: .ghost, size: .small) { index -= 1 }
                }
                Spacer()
                ActionButton(approval.refuseLabel, size: .small) { store.answer(approval, allow: false) }
                if isLast {
                    ActionButton(approval.allowLabel, variant: .primary, size: .small) { store.answer(approval, allow: true, answers: answers) }
                        .disabled(answers.count < approval.questions.count)
                } else {
                    ActionButton("Next", variant: .primary, size: .small) { index += 1 }
                        .disabled(answers[question.text] == nil)
                }
            }
        }
    }

    private func row(_ option: Question.Choice) -> some View {
        let picked = isChosen(option)
        return Button {
            choose(option)
        } label: {
            HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 1) {
                    Text(option.label)
                        .fontWeight(.medium)
                    if !option.detail.isEmpty {
                        Text(option.detail)
                            .font(.ui(size: 11.5))
                            .foregroundStyle(Color.themeMutedForeground)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 8)
                Image(.check, size: 13)
                    .foregroundStyle(Color.themePrimary)
                    .opacity(picked ? 1 : 0)
            }
            .multilineTextAlignment(.leading)
            .padding(.vertical, 6)
            .padding(.horizontal, Self.optionReach)
        }
        .buttonStyle(.highlight(radius: Radius.md, lit: picked))
    }

    /// What was typed for a question, or else the options chosen for it, in their order.
    private var answers: [String: String] {
        var answers: [String: String] = [:]
        for question in approval.questions {
            let written = (typed[question.id] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
            let picked = question.options.map(\.label).filter { chosen[question.id]?.contains($0) == true }
            let answer = written.isEmpty ? picked.joined(separator: ", ") : written
            if !answer.isEmpty { answers[question.text] = answer }
        }
        return answers
    }

    /// What is typed counts in place of the options, so none shows as chosen beside it.
    private func isChosen(_ option: Question.Choice) -> Bool {
        guard (typed[question.id] ?? "").isEmpty else { return false }
        return chosen[question.id]?.contains(option.label) == true
    }

    private func choose(_ option: Question.Choice) {
        typed[question.id] = nil
        guard question.multiple else {
            chosen[question.id] = [option.label]
            showNext(after: index)
            return
        }
        var picked = chosen[question.id] ?? []
        if !picked.insert(option.label).inserted { picked.remove(option.label) }
        chosen[question.id] = picked
    }

    /// Goes on to the next question once the choice has been seen.
    private func showNext(after asked: Int) {
        guard !isLast else { return }
        Task {
            try? await Task.sleep(for: .milliseconds(200))
            guard index == asked else { return }
            index += 1
        }
    }

    private var typedAnswer: Binding<String> {
        let id = question.id
        return Binding(get: { typed[id] ?? "" }, set: { typed[id] = $0 })
    }
}
