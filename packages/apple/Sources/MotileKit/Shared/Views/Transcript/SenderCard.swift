import SwiftUI

/// What the avatar beside a message the user didn't write opens: who sent it and what it did, in
/// the look of the sidebar's thread card.
struct SenderCard: View {
    @Environment(AppStore.self) private var store
    let avatar: MessageAvatar
    let sender: MessageSender
    /// The thread the message is in.
    let threadID: String

    var body: some View {
        Hugging {
            VStack(alignment: .leading, spacing: 0) {
                switch avatar.agent {
                case .some(let agent): agentLines(agent)
                case .none: serverLines
                }
            }
            .padding(12)
            .dropdownCard()
        }
    }

    @ViewBuilder private func agentLines(_ agent: Agent) -> some View {
        let sending = store.anyThread(sender.threadID)
        title(sending?.title ?? agent.name)
        VStack(alignment: .leading, spacing: 10) {
            let server = store.anyThread(threadID).flatMap { store.server($0.serverID) }
            let proxied = server?.runsThroughGateway(sender.account, of: agent) ?? false
            let pinned = server?.runsPinned(sender.account, of: agent) ?? false
            CardLine(text: accountLabel(agent, server: server), proxied: proxied, pinned: pinned, wraps: true) {
                AgentIcon(agent: agent, size: ThreadCard.iconSize)
            }
            CardLine(text: sender.note, wraps: true) { Image(.messageSquareText, size: ThreadCard.iconSize) }
            if let sending {
                let project = store.project(sending.projectID)?.seen(from: sending)
                CardLine(text: project?.name ?? URL(fileURLWithPath: sending.cwd).lastPathComponent, wraps: true) {
                    ProjectIcon(project: project, size: ThreadCard.iconSize)
                }
            } else {
                CardLine(text: "Its thread was deleted", wraps: true) { Image(.trash2, size: ThreadCard.iconSize) }
            }
        }
        .padding(.top, 10)
    }

    @ViewBuilder private var serverLines: some View {
        let thread = store.anyThread(threadID)
        let server = thread.flatMap { store.server($0.serverID) }
        HStack(alignment: .firstTextBaseline, spacing: Self.iconGap) {
            Image(.server, size: ThreadCard.iconSize)
                .foregroundStyle(Color.themeForeground)
                .levelWithFirstLine(.sm)
            title(server?.name ?? "The server")
        }
        Text(sender.note)
            .font(.ui(.xs))
            .foregroundStyle(Color.themeMutedForeground)
            .padding(.top, 4)
        if sender.pullRequest, let thread,
            let pullRequest = store.project(thread.projectID)?.seen(from: thread).pullRequest(of: thread)
        {
            ThemeDivider()
                .padding(.vertical, 10)
            PullRequestLine(pullRequest: pullRequest, wraps: true)
        }
    }

    /// Between a line's icon and its words, as the thread card's lines have it.
    private static let iconGap: CGFloat = 8

    private func title(_ text: String) -> some View {
        Text(text)
            .font(.ui(.sm, weight: .medium))
            .foregroundStyle(Color.themeForeground)
    }

    /// The agent, with the account it ran on when its agent has more than one, as the composer
    /// names it. The Gateway account that places its threads has no name of its own to show.
    private func accountLabel(_ agent: Agent, server: Server?) -> String {
        guard let server, server.accounts(of: agent).count > 1, let account = server.account(sender.account, of: agent),
            !account.gateway || account.pin != nil
        else { return agent.name }
        return "\(agent.name) · \(account.name)"
    }
}

/// A pull request's state, number and title on one line, as the thread card shows it.
struct PullRequestLine: View {
    let pullRequest: PullRequest
    /// Its title goes on to more lines rather than being cut.
    var wraps = false

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Image(pullRequest.state.symbol, size: ThreadCard.iconSize)
                .foregroundStyle(pullRequest.state.color)
                .levelWithFirstLine(.xs)
            Text(verbatim: "#\(pullRequest.number)")
                .font(.ui(.xs, weight: .medium))
                .monospacedDigit()
                .foregroundStyle(Color.themeMutedStrongerForeground)
            Text(pullRequest.title)
                .font(.ui(.xs))
                .foregroundStyle(Color.themeMutedForeground)
                .lineLimit(wraps ? nil : 1)
        }
    }
}

/// Lays the card out as wide as what it holds, up to the thread card's widest.
private struct Hugging: Layout {
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard let card = subviews.first else { return .zero }
        let width = min(card.sizeThatFits(.unspecified).width, ThreadCard.maxWidth)
        return card.sizeThatFits(ProposedViewSize(width: width, height: nil))
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        subviews.first?.place(at: bounds.origin, proposal: ProposedViewSize(bounds.size))
    }
}
