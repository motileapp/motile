import SwiftUI

/// The questions an agent asks with a tool call, one at a time. Each takes one of its options,
/// several of them when it allows that, or an answer written in the composer in place of them.
struct QuestionsView: View {
    @Environment(AppStore.self) private var store
    let approval: Approval

    /// How far past its text an option's light and the room to press it reach.
    private static let optionReach: CGFloat = 8
    private static let optionGap: CGFloat = 2
    private static let markGap: CGFloat = 10

    var body: some View {
        let question = store.question(approval)
        let index = store.questionIndex(approval)
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 6) {
                WaitingTitle(
                    question.header.isEmpty ? approval.title : question.header, symbol: .messageCircleQuestionMark,
                    place: approval.questions.count > 1 ? "\(index + 1) of \(approval.questions.count)" : nil
                )
                .foregroundStyle(Color.themeWarning)
                Text(question.text)
                    .font(.ui(.sm, weight: .medium))
                    .lineSpacing(2)
                    .fixedSize(horizontal: false, vertical: true)
                if question.multiple {
                    Text("Choose any that apply")
                        .foregroundStyle(Color.themeMutedForeground)
                }
            }
            VStack(spacing: Self.optionGap) {
                ForEach(question.options) { option in
                    row(option, multiple: question.multiple)
                }
                ownAnswer(multiple: question.multiple)
            }
            .padding(.horizontal, -Self.optionReach)
            HStack(spacing: 0) {
                if index > 0 {
                    ActionButton("Back", icon: .chevronLeft, variant: .ghost, margin: .beside(trailing: 8)) { store.goBack(approval) }
                }
                Spacer()
                ActionButton(approval.refuseLabel, variant: .ghost, margin: .beside(leading: 8, trailing: 4)) {
                    store.answer(approval, allow: false)
                }
                ActionButton(store.isLastQuestion(approval) ? approval.allowLabel : "Next", variant: .primary, margin: .beside(leading: 4)) {
                    store.goOn(approval)
                }
                .disabled(!store.canGoOn(approval))
            }
        }
    }

    private func row(_ option: Question.Choice, multiple: Bool) -> some View {
        let picked = store.isChosen(option, in: approval)
        return Button {
            store.choose(option, in: approval)
        } label: {
            HStack(alignment: .firstTextBaseline, spacing: Self.markGap) {
                PickMark(picked: picked, multiple: multiple)
                    .alignmentGuide(.firstTextBaseline) { $0[VerticalAlignment.center] + 4.5 }
                VStack(alignment: .leading, spacing: 2) {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text(option.label)
                            .font(.ui(.sm, weight: .medium))
                            .foregroundStyle(Color.themeForeground)
                        if option.recommended {
                            Chip("Recommended", tone: .themeSuccess, compact: true)
                                .fixedSize()
                        }
                    }
                    if !option.detail.isEmpty {
                        Text(option.detail)
                            .font(.ui(.xs))
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
            .picked(picked)
        }
        .buttonStyle(.highlight(radius: Radius.md))
    }

    /// The last row, which shows the answer written in the composer, or says to write one there.
    private func ownAnswer(multiple: Bool) -> some View {
        let written = store.typedAnswer(approval).trimmingCharacters(in: .whitespacesAndNewlines)
        return Button {
            store.focusComposer()
        } label: {
            HStack(alignment: .firstTextBaseline, spacing: Self.markGap) {
                Group {
                    if written.isEmpty {
                        Image(.pencilLine, size: .sm)
                            .foregroundStyle(Color.themeMutedForeground)
                            .frame(width: PickMark.size)
                    } else {
                        PickMark(picked: true, multiple: multiple)
                    }
                }
                .alignmentGuide(.firstTextBaseline) { $0[VerticalAlignment.center] + 4.5 }
                Text(written.isEmpty ? (multiple ? "Add your own answer below" : "Or type your own answer below") : written)
                    .font(.ui(.sm, weight: .medium))
                    .foregroundStyle(written.isEmpty ? Color.themeMutedForeground : Color.themeForeground)
                    .lineLimit(3)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
            }
            .multilineTextAlignment(.leading)
            .padding(.vertical, 8)
            .padding(.horizontal, Self.optionReach)
            .picked(!written.isEmpty)
        }
        .buttonStyle(.highlight(radius: Radius.md))
    }
}

private extension View {
    /// The option's look once it is the answer: the primary colour's wash inside a line of its
    /// stronger wash, apart from the plain light of the pointer.
    func picked(_ picked: Bool) -> some View {
        let shape = RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
        return background {
            if picked {
                shape
                    .fill(Color.themePrimary.wash())
                    .overlay { shape.strokeBorder(Color.themePrimary.wash(lit: true), lineWidth: 1) }
            }
        }
    }
}
