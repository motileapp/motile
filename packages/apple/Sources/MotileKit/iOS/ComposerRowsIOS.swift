#if os(iOS)
import SwiftUI

/// Places the composer's text and the row of its buttons. Collapsed, they are one line: the text
/// between the button that attaches and the one that sends. Otherwise the text is above the row.
struct ComposerRows: Layout {
    let collapsed: Bool

    private static let sideInset: CGFloat = 14
    private static let topInset: CGFloat = 4
    /// The room the attach button takes at the row's start, and the send button at its end.
    private static let buttonRoom: CGFloat = 46

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? 0
        guard subviews.count == 2 else { return CGSize(width: width, height: 0) }
        let text = subviews[0].sizeThatFits(ProposedViewSize(width: textWidth(in: width), height: nil))
        let row = subviews[1].sizeThatFits(ProposedViewSize(width: width, height: nil))
        return CGSize(width: width, height: collapsed ? max(text.height, row.height) : Self.topInset + text.height + row.height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard subviews.count == 2 else { return }
        let textProposal = ProposedViewSize(width: textWidth(in: bounds.width), height: nil)
        let text = subviews[0].sizeThatFits(textProposal)
        let row = subviews[1].sizeThatFits(ProposedViewSize(width: bounds.width, height: nil))
        subviews[1].place(
            at: CGPoint(x: bounds.minX, y: bounds.maxY - row.height), proposal: ProposedViewSize(width: bounds.width, height: row.height))
        let origin = collapsed
            ? CGPoint(x: bounds.minX + Self.buttonRoom, y: bounds.midY - text.height / 2)
            : CGPoint(x: bounds.minX + Self.sideInset, y: bounds.minY + Self.topInset)
        subviews[0].place(at: origin, proposal: textProposal)
    }

    private func textWidth(in width: CGFloat) -> CGFloat {
        max(0, width - 2 * (collapsed ? Self.buttonRoom : Self.sideInset))
    }
}

/// The row of the composer's buttons: the one that attaches, the model's name, which opens the
/// thread's settings, and the ones that stop and send. Collapsed, the text is in the middle of
/// the row instead of the model's name.
struct ComposerTouchControls: View {
    @Environment(AppStore.self) private var store
    let collapsed: Bool

    var body: some View {
        HStack(spacing: 0) {
            AttachMenu()
                .padding(.leading, 4)
            Spacer(minLength: 8)
            if !collapsed {
                settingsButton
                    .transition(.opacity)
            }
            ComposerSendButtons()
        }
    }

    private var settingsButton: some View {
        let model = store.composerModel
        return Button {
            store.showsThreadSettings = true
        } label: {
            HStack(spacing: 6) {
                if let model {
                    AgentIcon(agent: model.agent, size: 15)
                }
                Text(model?.name ?? "No agent")
                    .font(.ui(size: 13, weight: .medium))
                    .lineLimit(1)
                Image(systemName: "chevron.down")
                    .font(.ui(size: 9, weight: .bold))
                    .foregroundStyle(Color.themeTertiary)
            }
            .foregroundStyle(Color.themeSecondary)
            .padding(.horizontal, 10)
            .frame(height: 34)
            .padding(.vertical, 6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.highlight(radius: 10, inset: EdgeInsets(top: 6, leading: 0, bottom: 6, trailing: 0)))
        .accessibilityLabel("Thread settings")
    }
}
#endif
