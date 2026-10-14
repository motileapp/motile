import SwiftUI

/// The space between the right panel's list rows, split around each: it looks empty but is the row's.
let panelRowMargin = EdgeInsets(top: 1, leading: 0, bottom: 1, trailing: 0)

/// The room under the end of what the right panel scrolls through, so that it never ends on the window's edge.
#if os(macOS)
let panelEndRoom: CGFloat = 64
#else
let panelEndRoom: CGFloat = 48
#endif

/// The agents the thread's agent has started, and what one of them did once it is opened.
struct AgentsSurface: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        let agents = store.agents
        if let agent = agents.first(where: { $0.id == store.rightPanel.shownAgent }) {
            AgentTranscript(agent: agent)
        } else if agents.isEmpty {
            PanelMessage(text: "The agents this thread starts show up here.")
        } else {
            VStack(spacing: 0) {
                PanelBar {
                    Text(dotted: Self.summary(of: agents))
                        .font(.ui(size: 12, weight: .medium))
                        .foregroundStyle(Color.themeMutedForeground)
                    Spacer()
                }
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(agents) { AgentRow(agent: $0) }
                    }
                    .padding(.horizontal, 8)
                    .padding(.top, 8 - panelRowMargin.top)
                    .padding(.bottom, panelEndRoom - panelRowMargin.bottom)
                }
            }
        }
    }

    /// "2 working · 1 done · 1 failed · 48k tokens", leaving out what there is none of.
    static func summary(of agents: [AgentInfo]) -> String {
        let counts: [(Int, String)] = [
            (agents.count { $0.status == .running }, "working"),
            (agents.count { $0.status == .succeeded }, "done"),
            (agents.count { $0.status == .failed }, "failed"),
        ]
        let spent = agents.compactMap(\.tokens)
        let total = spent.isEmpty ? [] : [AgentInfo.tokens(spent.reduce(0, +))]
        return (counts.filter { $0.0 > 0 }.map { "\($0.0) \($0.1)" } + total).joined(separator: " · ")
    }
}

private struct AgentRow: View {
    @Environment(AppStore.self) private var store
    let agent: AgentInfo

    var body: some View {
        Button {
            store.rightPanel.showAgent(agent.id)
        } label: {
            HStack(alignment: .top, spacing: 9) {
                AgentStatusIcon(status: agent.status)
                    .frame(width: 16, height: 18)
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 6) {
                        Text(agent.title)
                            .font(.ui(size: 13, weight: .medium))
                            .foregroundStyle(Color.themeForeground)
                            .lineLimit(1)
                        if let kind = agent.kind {
                            Text(kind)
                                .font(.ui(size: 11))
                                .foregroundStyle(Color.themeMutedForeground)
                                .lineLimit(1)
                                .layoutPriority(-1)
                        }
                        Spacer(minLength: 6)
                        AgentTime(agent: agent)
                    }
                    if !agent.detail.isEmpty {
                        Text(agent.detail)
                            .font(.ui(size: 12))
                            .foregroundStyle(Color.themeMutedForeground)
                            .lineLimit(2)
                            .multilineTextAlignment(.leading)
                    }
                    if let usage = agent.usage {
                        Text(usage)
                            .font(.ui(size: 11))
                            .foregroundStyle(Color.themeMutedStrongerForeground)
                    }
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            // An agent that another agent started stands in from it.
            .padding(.leading, agent.parent == nil ? 0 : 18)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(panelRowMargin)
        }
        .buttonStyle(.highlight(inset: panelRowMargin))
    }
}

private struct AgentStatusIcon: View {
    let status: ToolContent.Status

    var body: some View {
        switch status {
        case .running: icon(.circleDashed, Color.themeProcess)
        case .succeeded: icon(.check, Color.themeMutedForeground)
        case .failed: icon(.x, Color.themeDestructive)
        }
    }

    private func icon(_ name: Symbol, _ color: Color) -> some View {
        Image(name, size: 11)
            .foregroundStyle(color)
    }
}

/// How long the agent has worked so far, or worked in all.
private struct AgentTime: View {
    let agent: AgentInfo

    var body: some View {
        if agent.working {
            TimelineView(.periodic(from: .now, by: 1)) { context in
                time(Time.elapsed(since: agent.startedAt, now: context.date.timeIntervalSince1970), Color.themeProcess)
            }
        } else if let milliseconds = agent.durationMs {
            time(Time.duration(milliseconds: milliseconds), Color.themeMutedStrongerForeground)
        }
    }

    private func time(_ text: String, _ color: Color) -> some View {
        Text(text)
            .font(.ui(size: 11, weight: .medium))
            .monospacedDigit()
            .foregroundStyle(color)
            .fixedSize()
    }
}

private struct AgentTranscript: View {
    @Environment(AppStore.self) private var store
    let agent: AgentInfo

    var body: some View {
        VStack(spacing: 0) {
            PanelBar {
                ActionButton(icon: .chevronLeft, help: "All agents") { store.rightPanel.showAgents() }
                    .padding(.leading, -8)
                AgentStatusIcon(status: agent.status)
                    .frame(width: 16)
                Text(agent.title)
                    .font(.ui(size: 12, weight: .medium))
                    .foregroundStyle(Color.themeForeground)
                    .lineLimit(1)
                Spacer(minLength: 6)
                AgentTime(agent: agent)
                    .padding(.trailing, 6)
            }
            TranscriptRepresentable(store: store, ofAgent: true, bottomInset: panelEndRoom)
        }
    }
}
