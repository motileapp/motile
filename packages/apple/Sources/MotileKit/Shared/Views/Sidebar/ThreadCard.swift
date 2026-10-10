import SwiftUI

/// What a thread is, beside its row in the sidebar: where it works, the model and the account as
/// the composer shows them, and its pull request.
struct ThreadCard: View {
    static let maxWidth: CGFloat = 320
    @Environment(AppStore.self) private var store
    let threadID: String

    var body: some View {
        if let thread = store.threads[threadID] {
            card(thread, project: store.project(thread.projectID)?.seen(from: thread))
        }
    }

    private func card(_ thread: ThreadInfo, project: Project?) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(thread.title)
                .font(.ui(size: 13, weight: .medium))
                .foregroundStyle(Color.themeForeground)
                .lineLimit(1)
            VStack(alignment: .leading, spacing: 7) {
                CardLine(text: project?.name ?? URL(fileURLWithPath: thread.cwd).lastPathComponent) {
                    ProjectIcon(project: project, size: 14)
                }
                if let server = store.server(thread.serverID) {
                    CardLine(text: server.name) { Image(.server, size: 12) }
                }
                if let project, let branch = project.branch {
                    CardLine(text: branch, middle: true) { Image(project.checkoutSymbol, size: 13) }
                } else {
                    let home = store.server(thread.serverID)?.home ?? ""
                    CardLine(text: ThreadFolderLabel.shortened(thread.cwd, home: home), middle: true) { Image(.folder, size: 13) }
                }
                CardLine(text: store.modelLabel(of: thread)) { AgentIcon(agent: thread.agent, size: 13) }
            }
            .padding(.top, 10)
            if let pullRequest = project?.pullRequest(of: thread) {
                ThemeDivider()
                    .padding(.vertical, 10)
                HStack(spacing: 8) {
                    Image(pullRequest.state.symbol, size: 13)
                        .foregroundStyle(pullRequest.state.color)
                        .frame(width: 14)
                    Text(verbatim: "#\(pullRequest.number)")
                        .font(.ui(size: 12, weight: .medium))
                        .monospacedDigit()
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                    Text(pullRequest.title)
                        .font(.ui(size: 12))
                        .foregroundStyle(Color.themeMutedForeground)
                        .lineLimit(1)
                }
            }
        }
        .padding(12)
        .dropdownCard()
    }
}

private struct CardLine<Icon: View>: View {
    let text: String
    /// Cut in the middle, as a path or a branch is.
    var middle = false
    @ViewBuilder let icon: () -> Icon

    var body: some View {
        HStack(spacing: 8) {
            icon()
                .frame(width: 14, height: 14)
            Text(dotted: text)
                .font(.ui(size: 12))
                .lineLimit(1)
                .truncationMode(middle ? .middle : .tail)
        }
        .foregroundStyle(Color.themeMutedForeground)
    }
}

/// Brings up the card of the thread whose row the pointer rests on. It waits a moment the first
/// time, then goes at once to every row the pointer moves to, and shortly after it went away.
final class ThreadPeek {
    static let shared = ThreadPeek()
    private static let rest: TimeInterval = 0.15
    private static let linger: TimeInterval = 0.4
    private let card = ThreadCardWindow()
    private var waiting: Timer?
    private var hiddenAt = Date.distantPast

    private init() {}

    /// The pointer is on the thread's row, `row` in `view`, or on no thread's.
    func point(at thread: ThreadInfo?, row: CGRect, in view: PlatformView, store: AppStore) {
        waiting?.invalidate()
        guard let thread else {
            hide()
            return
        }
        let content = AnyView(ThreadCard(threadID: thread.id).environment(store))
        guard card.isShown || Date().timeIntervalSince(hiddenAt) < Self.linger else {
            waiting = Timer.scheduledTimer(withTimeInterval: Self.rest, repeats: false) { [weak self, weak view] _ in
                guard let self, let view else { return }
                card.show(content, beside: row, in: view, fading: true)
            }
            return
        }
        card.show(content, beside: row, in: view, fading: false)
    }

    func hide() {
        waiting?.invalidate()
        guard card.isShown else { return }
        card.hide()
        hiddenAt = Date()
    }
}

/// The card in a layer as tall as the window, beside its row: level with the row's top, lifted
/// where it would pass the window's bottom. The layer's margin leaves room for the card's shadow.
struct ThreadCardLayer: View {
    static let margin: CGFloat = 16
    static let width = ThreadCard.maxWidth + 2 * margin
    /// Between the row's light and the card.
    private static let gap: CGFloat = 4
    let card: AnyView
    /// How far below the layer's top the row's top is.
    let rowTop: CGFloat

    /// Where the layer starts beside a row whose side is at `rowEnd`.
    static func left(besideRowEndingAt rowEnd: CGFloat) -> CGFloat {
        rowEnd - rowMargin.trailing + gap - margin
    }

    var body: some View {
        Beside(top: rowTop + rowMargin.top - Self.margin) {
            card.padding(Self.margin)
        }
    }
}

private struct Beside: Layout {
    let top: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        proposal.replacingUnspecifiedDimensions()
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard let card = subviews.first else { return }
        let width = min(card.sizeThatFits(.unspecified).width, bounds.width)
        let size = card.sizeThatFits(ProposedViewSize(width: width, height: nil))
        let y = max(bounds.minY, min(bounds.minY + top, bounds.maxY - size.height))
        card.place(at: CGPoint(x: bounds.minX, y: y), proposal: ProposedViewSize(size))
    }
}
