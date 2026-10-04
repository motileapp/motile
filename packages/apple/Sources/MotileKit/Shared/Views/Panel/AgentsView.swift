import SwiftUI

/// The agents the thread's agent has started, and what one of them did once it is opened.
struct AgentsSurface: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        let agents = store.agents
        if let agent = agents.first(where: { $0.id == store.sidePanel.shownAgent }) {
            AgentTranscript(agent: agent)
        } else if agents.isEmpty {
            PanelMessage(text: "The agents this thread starts show up here.")
        } else {
            VStack(spacing: 0) {
                PanelBar {
                    Text(Self.summary(of: agents))
                        .font(.ui(size: 12, weight: .medium))
                        .foregroundStyle(Color.themeSecondary)
                    Spacer()
                }
                ScrollView {
                    LazyVStack(spacing: 2) {
                        ForEach(agents) { AgentRow(agent: $0) }
                    }
                    .padding(8)
                }
            }
        }
    }

    /// "2 working · 1 done · 1 failed", leaving out what there is none of.
    static func summary(of agents: [AgentInfo]) -> String {
        let counts: [(Int, String)] = [
            (agents.count { $0.status == .running }, "working"),
            (agents.count { $0.status == .succeeded }, "done"),
            (agents.count { $0.status == .failed }, "failed"),
        ]
        return counts.filter { $0.0 > 0 }.map { "\($0.0) \($0.1)" }.joined(separator: " · ")
    }
}

private struct AgentRow: View {
    @Environment(AppStore.self) private var store
    let agent: AgentInfo

    var body: some View {
        Button {
            store.sidePanel.showAgent(agent.id)
        } label: {
            HStack(alignment: .top, spacing: 9) {
                AgentStatusIcon(status: agent.status)
                    .frame(width: 16, height: 18)
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 6) {
                        Text(agent.title)
                            .font(.ui(size: 13, weight: .medium))
                            .foregroundStyle(Color.themeText)
                            .lineLimit(1)
                        if let kind = agent.kind {
                            Text(kind)
                                .font(.ui(size: 11))
                                .foregroundStyle(Color.themeSecondary)
                                .lineLimit(1)
                                .layoutPriority(-1)
                        }
                        Spacer(minLength: 6)
                        AgentTime(agent: agent)
                    }
                    if !agent.detail.isEmpty {
                        Text(agent.detail)
                            .font(.ui(size: 12))
                            .foregroundStyle(Color.themeSecondary)
                            .lineLimit(2)
                            .multilineTextAlignment(.leading)
                    }
                    if let usage = agent.usage {
                        Text(usage)
                            .font(.ui(size: 11))
                            .foregroundStyle(Color.themeTertiary)
                    }
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            // An agent that another agent started stands in from it.
            .padding(.leading, agent.parent == nil ? 0 : 18)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.highlight())
    }
}

private struct AgentStatusIcon: View {
    let status: ToolContent.Status

    var body: some View {
        switch status {
        case .running: icon("circle.dashed", Color.themeWorking)
        case .succeeded: icon("checkmark", Color.themeSecondary)
        case .failed: icon("xmark", Color.themeDanger)
        }
    }

    private func icon(_ name: String, _ color: Color) -> some View {
        Image(systemName: name)
            .font(.ui(size: 11, weight: .semibold))
            .foregroundStyle(color)
    }
}

/// How long the agent has worked so far, or worked in all.
private struct AgentTime: View {
    let agent: AgentInfo

    var body: some View {
        if agent.working {
            TimelineView(.periodic(from: .now, by: 1)) { context in
                time(Time.elapsed(since: agent.startedAt, now: context.date.timeIntervalSince1970), Color.themeWorking)
            }
        } else if let milliseconds = agent.durationMs {
            time(Time.duration(milliseconds: milliseconds), Color.themeTertiary)
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
                IconOnlyButton(symbol: "chevron.left", help: "All agents") { store.sidePanel.showAgents() }
                    .padding(.leading, -6)
                AgentStatusIcon(status: agent.status)
                    .frame(width: 16)
                Text(agent.title)
                    .font(.ui(size: 12, weight: .medium))
                    .foregroundStyle(Color.themeText)
                    .lineLimit(1)
                Spacer(minLength: 6)
                AgentTime(agent: agent)
                    .padding(.trailing, 6)
            }
            TranscriptRepresentable(store: store, ofAgent: true, bottomInset: 0)
        }
    }
}
