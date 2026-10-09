import SwiftUI

/// The tool call the agent waits with, above the composer: it is allowed or refused, and one
/// that asks questions is answered. The calls behind it wait their turn.
struct WaitingStrip: View {
    @Environment(AppStore.self) private var store
    let approval: Approval
    let count: Int

    static let padding: CGFloat = 12
    private static let targetFont = PlatformFont.uiMono(12)
    /// Eight lines of a command and the room under them, after which it scrolls to the strip's edge.
    private static let targetHeight = targetFont.textLineHeight * 8 + padding

    var body: some View {
        content
            .font(.ui(size: 12.5))
            .frame(maxWidth: .infinity, alignment: .leading)
            .modifier(ComposerStrip(edge: .top, height: nil))
    }

    @ViewBuilder private var content: some View {
        if approval.questions.isEmpty {
            call
        } else {
            QuestionsView(approval: approval)
                .id(approval.id)
                .padding(Self.padding)
        }
    }

    private var call: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                WaitingTitle(approval.title, symbol: approval.symbol, place: count > 1 ? "1 of \(count)" : nil)
                    .foregroundStyle(Color.themeWarning)
                Spacer(minLength: 8)
                ActionButton(approval.refuseLabel, size: .small) { store.answer(approval, allow: false) }
                ActionButton(approval.allowLabel, variant: .warning, size: .small) { store.answer(approval, allow: true) }
            }
            .padding([.top, .horizontal], Self.padding)
            .padding(.bottom, approval.target.isEmpty ? Self.padding : 8)
            if !approval.target.isEmpty {
                FadingScroll(maxHeight: Self.targetHeight) {
                    Text(approval.target)
                        .font(.ui(size: 12, design: .monospaced))
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding([.bottom, .horizontal], Self.padding)
                }
            }
        }
    }
}

/// What a waiting strip is about, and which of several it is.
struct WaitingTitle: View {
    let title: String
    let symbol: Symbol
    let place: String?

    init(_ title: String, symbol: Symbol, place: String?) {
        self.title = title
        self.symbol = symbol
        self.place = place
    }

    var body: some View {
        HStack(spacing: 6) {
            Image(symbol, size: 13)
            Text(title)
                .font(.ui(size: 12, weight: .semibold))
                .lineLimit(1)
            if let place {
                Text(place)
                    .font(.ui(size: 12))
                    .monospacedDigit()
                    .foregroundStyle(Color.themeSecondary)
                    .fixedSize()
            }
        }
    }
}
