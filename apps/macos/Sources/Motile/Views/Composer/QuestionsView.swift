import SwiftUI

/// The questions an agent asks with a tool call. Each takes one of its options, several of them
/// when it allows that, or an answer typed in place of them.
struct QuestionsView: View {
    @Environment(AppStore.self) private var store
    let approval: Approval
    @State private var chosen: [String: Set<String>] = [:]
    @State private var typed: [String: String] = [:]

    private static let optionGap: CGFloat = 6

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            ForEach(approval.questions) { question in
                VStack(alignment: .leading, spacing: Self.optionGap) {
                    Text(question.text)
                        .fontWeight(.medium)
                    ForEach(question.options) { option in
                        Button {
                            choose(option, for: question)
                        } label: {
                            HStack(alignment: .firstTextBaseline, spacing: 6) {
                                Image(systemName: symbol(option, for: question))
                                    .foregroundStyle(isChosen(option, for: question) ? Color.themePrimary : Color.themeSecondary)
                                Text(option.label)
                                Text(option.detail)
                                    .foregroundStyle(Color.themeSecondary)
                                    .lineLimit(1)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.vertical, Self.optionGap / 2)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .padding(.vertical, -Self.optionGap / 2)
                    }
                    TextField("Something else", text: typedAnswer(for: question))
                        .textFieldStyle(.roundedBorder)
                }
            }
            HStack(spacing: 8) {
                Spacer()
                Button(approval.refuseLabel) { store.answer(approval, allow: false) }
                    .buttonStyle(.bordered)
                Button(approval.allowLabel) { store.answer(approval, allow: true, answers: answers) }
                    .buttonStyle(.borderedProminent)
                    .disabled(answers.count < approval.questions.count)
            }
            .controlSize(.small)
        }
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

    private func isChosen(_ option: Question.Choice, for question: Question) -> Bool {
        chosen[question.id]?.contains(option.label) == true
    }

    private func choose(_ option: Question.Choice, for question: Question) {
        guard question.multiple else {
            chosen[question.id] = [option.label]
            return
        }
        var picked = chosen[question.id] ?? []
        if !picked.insert(option.label).inserted { picked.remove(option.label) }
        chosen[question.id] = picked
    }

    private func symbol(_ option: Question.Choice, for question: Question) -> String {
        let on = isChosen(option, for: question)
        if question.multiple { return on ? "checkmark.square.fill" : "square" }
        return on ? "largecircle.fill.circle" : "circle"
    }

    private func typedAnswer(for question: Question) -> Binding<String> {
        Binding(get: { typed[question.id] ?? "" }, set: { typed[question.id] = $0 })
    }
}
