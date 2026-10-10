import SwiftUI

/// The questions an agent asks with a tool call, one at a time. Each takes one of its options,
/// several of them when it allows that, or an answer typed in place of them.
struct QuestionsView: View {
    @Environment(AppStore.self) private var store
    let approval: Approval
    @State private var index = 0
    @State private var chosen: [String: Set<String>] = [:]
    @State private var typed: [String: String] = [:]
    @FocusState private var typing: Bool

    /// How far past its text an option's light and the room to press it reach.
    private static let optionReach: CGFloat = 8
    private static let markGap: CGFloat = 10

    private var question: Question { approval.questions[min(index, approval.questions.count - 1)] }
    private var isLast: Bool { index >= approval.questions.count - 1 }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 6) {
                WaitingTitle(
                    question.header.isEmpty ? approval.title : question.header, symbol: .messageCircleQuestionMark,
                    place: approval.questions.count > 1 ? "\(index + 1) of \(approval.questions.count)" : nil
                )
                .foregroundStyle(Color.themeWarning)
                Text(question.text)
                    .font(.ui(size: 13.5, weight: .medium))
                    .lineSpacing(2)
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
                somethingElse
            }
            .padding(.horizontal, -Self.optionReach)
            HStack(spacing: 8) {
                if index > 0 {
                    ActionButton("Back", icon: .chevronLeft, variant: .ghost, size: .small) { index -= 1 }
                }
                Spacer()
                ActionButton(approval.refuseLabel, size: .small) { store.answer(approval, allow: false) }
                ActionButton(isLast ? approval.allowLabel : "Next", variant: .primary, size: .small, action: goOn)
                    .disabled(!canGoOn)
            }
        }
    }

    private func row(_ option: Question.Choice) -> some View {
        let picked = isChosen(option)
        return Button {
            choose(option)
        } label: {
            HStack(alignment: .firstTextBaseline, spacing: Self.markGap) {
                PickMark(picked: picked, multiple: question.multiple)
                    .alignmentGuide(.firstTextBaseline) { $0[VerticalAlignment.center] + 4.5 }
                VStack(alignment: .leading, spacing: 2) {
                    Text(option.label)
                        .font(.ui(size: 13, weight: .medium))
                        .foregroundStyle(Color.themeForeground)
                    if !option.detail.isEmpty {
                        Text(option.detail)
                            .font(.ui(size: 12))
                            .foregroundStyle(Color.themeMutedForeground)
                            .lineSpacing(1.5)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 0)
            }
            .multilineTextAlignment(.leading)
            .padding(.vertical, 8)
            .padding(.horizontal, Self.optionReach)
        }
        .buttonStyle(.highlight(radius: Radius.md, lit: picked, fill: .themeBorderComposer))
    }

    /// The last row, which takes an answer typed in place of the options.
    private var somethingElse: some View {
        let written = !(typed[question.id] ?? "").isEmpty
        return HStack(spacing: Self.markGap) {
            PickMark(picked: written, multiple: question.multiple)
            InputField("Something else", text: typedAnswer, variant: .bare, size: .large, focus: $typing)
                .padding(.leading, -(ControlSize.large.padding - 2))
                .fixedSize(horizontal: false, vertical: true)
                .onSubmit(goOn)
        }
        .padding(.vertical, 8)
        .padding(.horizontal, Self.optionReach)
        .contentShape(Rectangle())
        .onTapGesture { typing = true }
        .hoverHighlight(radius: Radius.md, lit: written || typing, fill: .themeBorderComposer)
    }

    private var canGoOn: Bool {
        isLast ? answers.count == approval.questions.count : answers[question.text] != nil
    }

    /// Answers the questions after the last, or else goes on to the next.
    private func goOn() {
        guard canGoOn else { return }
        guard isLast else {
            index += 1
            return
        }
        store.answer(approval, allow: true, answers: answers)
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
        typing = false
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
