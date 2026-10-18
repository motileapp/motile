import SwiftUI

/// The space between the right panel's list rows, split around each: it looks empty but is the row's.
let panelRowMargin = EdgeInsets(top: 1, leading: 0, bottom: 1, trailing: 0)

/// The room under the end of what the right panel scrolls through, so that it never ends on the window's edge.
#if os(macOS)
let panelEndRoom: CGFloat = 64
#else
let panelEndRoom: CGFloat = 48
#endif

/// The agents the thread's agent has started, the ones at work first and the ones that ended
/// folded under them, and the thread whose agent started this one. An agent opens in the thread's
/// place: one started through your server in its own thread, any other read only.
struct AgentsSurface: View {
    @Environment(AppStore.self) private var store
    /// Whether the agents that ended are shown, once the user has said. Until then they are
    /// while none works.
    @State private var showsEnded: Bool?

    var body: some View {
        let agents = store.agents
        let parent = store.parentThread
        if agents.isEmpty && parent == nil {
            PanelMessage(text: "The agents this thread starts show up here.")
        } else {
            let working = agents.filter(\.working)
            let ended = agents.filter { !$0.working }
            let shown = showsEnded ?? working.isEmpty
            VStack(spacing: 0) {
                PanelBar {
                    Self.summary(of: agents)
                        .font(.ui(size: 12, weight: .medium))
                        .foregroundStyle(Color.themeMutedForeground)
                    Spacer()
                }
                ScrollView {
                    LazyVStack(spacing: 0) {
                        if let parent { ParentRow(thread: parent) }
                        ForEach(working) { AgentRow(agent: $0) }
                        if !ended.isEmpty {
                            EndedHeader(agents: ended, shown: shown) { showsEnded = !shown }
                            if shown { ForEach(ended) { AgentRow(agent: $0) } }
                        }
                    }
                    .padding(.horizontal, 8)
                    .padding(.top, 8 - panelRowMargin.top)
                    .padding(.bottom, panelEndRoom - panelRowMargin.bottom)
                }
            }
        }
    }

    /// "1 waiting · 2 working · 1 done · 48k tokens", leaving out what there is none of, each count
    /// in its state's colour.
    static func summary(of agents: [AgentInfo]) -> Text {
        guard !agents.isEmpty else { return Text(verbatim: "No agents") }
        let words: [(AgentState, String)] = [(.waiting, "waiting"), (.working, "working"), (.limited, "limited"), (.done, "done"), (.failed, "failed"), (.stopped, "stopped")]
        let counts = words.compactMap { state, word -> Text? in
            let count = agents.count { $0.state == state }
            return count > 0 ? Text(verbatim: "\(count) \(word)").foregroundStyle(state.color) : nil
        }
        let spent = agents.compactMap(\.tokens)
        let total = spent.isEmpty ? [] : [Text(verbatim: AgentInfo.tokens(spent.reduce(0, +)))]
        return Text(dotted: counts + total)
    }
}

/// The thread whose agent started this one, which a click opens.
private struct ParentRow: View {
    @Environment(AppStore.self) private var store
    let thread: ThreadInfo

    var body: some View {
        Button {
            store.openStartedThread(thread.id)
        } label: {
            HStack(spacing: 9) {
                Image(.cornerLeftUp, size: 12)
                    .foregroundStyle(Color.themeMutedForeground)
                    .frame(width: 16)
                VStack(alignment: .leading, spacing: 2) {
                    Text("Started by")
                        .font(.ui(size: 11))
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                    Text(thread.title)
                        .font(.ui(size: 13, weight: .medium))
                        .foregroundStyle(Color.themeForeground)
                        .lineLimit(1)
                }
                Spacer(minLength: 6)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(panelRowMargin)
        }
        .buttonStyle(.highlight(inset: panelRowMargin))
        .padding(.bottom, 4)
    }
}

/// Over the agents that ended: how many, and how many of them failed. A click folds them.
private struct EndedHeader: View {
    let agents: [AgentInfo]
    let shown: Bool
    let toggle: () -> Void

    var body: some View {
        let failed = agents.count { $0.status == .failed }
        Button(action: toggle) {
            HStack(spacing: 8) {
                Text("Previous agents")
                    .font(.ui(size: 12, weight: .semibold))
                    .foregroundStyle(Color.themeForeground)
                Text("\(agents.count)")
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                if failed > 0 {
                    Text("\(failed) failed")
                        .font(.ui(size: 12))
                        .foregroundStyle(Color.themeDestructive)
                }
                Spacer(minLength: 6)
                Image(shown ? .chevronDown : .chevronRight, size: 10)
                    .foregroundStyle(Color.themeMutedStrongerForeground)
            }
            .padding(.horizontal, 10)
            .frame(height: ControlSize.regular.height)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(panelRowMargin)
        }
        .buttonStyle(.highlight(inset: panelRowMargin))
        .padding(.top, 6)
    }
}

/// An agent: how it is doing, what it is doing or said last, and what it runs on and has spent.
/// One that works in a thread of its own can be stopped from here.
private struct AgentRow: View {
    @Environment(AppStore.self) private var store
    let agent: AgentInfo
    @State private var hovered = false

    private var stoppable: Bool { agent.working && agent.threadID != nil }

    var body: some View {
        Button {
            open()
        } label: {
            HStack(alignment: .top, spacing: 10) {
                AgentAvatar(agent: agent.agent, state: agent.state)
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
                        if agent.state.flagged {
                            Text(agent.label)
                                .font(.ui(size: 11, weight: .medium))
                                .foregroundStyle(agent.state.color)
                                .lineLimit(1)
                        }
                        Spacer(minLength: 6)
                        AgentTime(agent: agent)
                            .opacity(hovered && stoppable ? 0 : 1)
                    }
                    // Always one line, so that a row keeps its height as what its agent does changes.
                    Text(agent.detail)
                        .font(.ui(size: 12))
                        .foregroundStyle(agent.status == .failed ? Color.themeDestructive : Color.themeMutedForeground)
                        .lineLimit(1, reservesSpace: true)
                    if let facts = facts {
                        Text(facts)
                            .font(.ui(size: 11))
                            .foregroundStyle(Color.themeMutedStrongerForeground)
                            .lineLimit(1)
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
        .buttonStyle(.highlight(selected: store.rightPanel.shownAgent == agent.id, inset: panelRowMargin))
        .overlay(alignment: .topTrailing) {
            if hovered && stoppable {
                ActionButton(icon: .circleStop, help: "Stop \(agent.title)", size: .small) { stop() }
                    .padding(.top, 8 - (ControlSize.small.height - 18) / 2 + panelRowMargin.top)
                    .padding(.trailing, 6)
            }
        }
        .onHover { hovered = $0 }
        .contextMenu {
            Button(agent.threadID == nil ? "Show What It Did" : "Open Thread") { open() }
            if stoppable { Button("Stop Agent") { stop() } }
        }
    }

    /// "6.1-Sol · work · 3 tools · 21k tokens", as far as each is known.
    private var facts: String? {
        let parts = [agent.model, agent.account, agent.usage].compactMap { $0 }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    private func open() {
        if let thread = agent.threadID { return store.openStartedThread(thread) }
        store.rightPanel.showAgent(agent.id)
    }

    private func stop() {
        guard let thread = agent.threadID else { return }
        store.stop(threadID: thread)
    }
}

/// How long the agent has worked so far, or worked in all.
struct AgentTime: View {
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
