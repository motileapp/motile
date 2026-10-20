import SwiftUI

/// Above the composer of a thread whose agent started agents that wait for the user: which, and
/// the way to the first of them, where it is answered.
struct ChildrenStrip: View {
    @Environment(AppStore.self) private var store
    let text: String
    let first: ThreadChildren.Waiting
    /// How many wait.
    let count: Int

    var body: some View {
        HStack(spacing: 0) {
            Image(first.ask.symbol, size: 13)
                .foregroundStyle(Color.themeWarning)
                .padding(.leading, 14)
                .padding(.trailing, 7)
            // A narrow composer says what is asked without naming the agent.
            ViewThatFits(in: .horizontal) {
                Text(text)
                Text(count == 1 ? "Agent \(first.ask.phrase)" : text)
                    .truncationMode(.tail)
            }
            .font(.ui(size: 12.5, weight: .medium))
            .foregroundStyle(Color.themeWarning)
            .lineLimit(1)
            .help(text)
            Spacer(minLength: 8)
            ActionButton("Open \(first.ask.word)", help: "Open \(first.title)", variant: .ghost, margin: ComposerStrip.margin) {
                store.openStartedThread(first.threadID)
            }
        }
        .modifier(ComposerStrip(edge: .top))
    }
}

/// In place of the composer of an agent the thread's agent started, which runs on its own and
/// can't be written to: how it is doing, and the way back to the thread.
struct AgentBar: View {
    @Environment(AppStore.self) private var store
    let agent: AgentInfo

    private static let height = ControlSize.large.height + 8
    /// The way back's arrow stands close to its words.
    private static let backGap: CGFloat = 4
    /// What it says stands twice the bar's gap from the way back.
    private static let backApart: CGFloat = 8

    var body: some View {
        // A narrow bar leaves out what it says of the agent rather than cut it.
        ViewThatFits(in: .horizontal) {
            line(saying: true)
            line(saying: false)
        }
        // A pill, whose round avatar and round button keep as far from its top and bottom as from
        // its ends, so that their curves follow its own.
        .padding(.leading, (Self.height - AgentAvatarDrawing.side) / 2)
        .padding(.trailing, (Self.height - ControlSize.regular.height) / 2)
        .frame(maxWidth: .infinity)
        .frame(height: Self.height)
        .composerSurface(in: Capsule())
        .composerOutlineShadow()
        .frame(maxWidth: Theme.composerWidth)
    }

    private func line(saying: Bool) -> some View {
        HStack(spacing: 8) {
            AgentAvatar(agent: agent.agent, state: agent.state)
            Text(agent.title)
                .font(.ui(size: 13, weight: .medium))
                .foregroundStyle(Color.themeForeground)
                .lineLimit(1)
            AgentTime(agent: agent)
            Spacer(minLength: 8)
            if saying {
                Text("Runs on its own")
                    .font(.ui(size: 12.5))
                    .foregroundStyle(Color.themeMutedForeground)
                    .lineLimit(1)
                    .fixedSize()
                    .padding(.trailing, Self.backApart)
            }
            ActionButton("Go Back", icon: .cornerLeftUp, help: "Back to the thread", variant: .primary, gap: Self.backGap, round: true) { store.rightPanel.showAgents() }
        }
    }
}
