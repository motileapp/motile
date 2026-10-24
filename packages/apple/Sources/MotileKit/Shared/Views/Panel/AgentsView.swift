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
            let working = agents.filter { !$0.previous }
            let ended = agents.filter(\.previous)
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
                        if let parent { ParentRow(thread: parent).agentsRow("parent") }
                        ForEach(working) { AgentRow(agent: $0).agentsRow($0.id) }
                        if !ended.isEmpty {
                            EndedHeader(agents: ended, shown: shown) { showsEnded = !shown }.agentsRow("ended")
                            if shown { ForEach(ended) { AgentRow(agent: $0).agentsRow($0.id) } }
                        }
                    }
                    .padding(.horizontal, 8)
                    .padding(.top, 8 - panelRowMargin.top)
                    .padding(.bottom, panelEndRoom - panelRowMargin.bottom)
                }
                .keepsAgentsScroll(key: store.panelTarget?.key, ready: !agents.isEmpty)
                .id(store.panelTarget?.key)
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

extension View {
    /// Keeps the row at the top of a thread's agents and how far the list is scrolled past it, to
    /// scroll back to when the thread shows them again.
    @ViewBuilder fileprivate func keepsAgentsScroll(key: String?, ready: Bool) -> some View {
        if #available(macOS 15, *) {
            modifier(KeepsAgentsScroll(key: key, ready: ready))
        } else {
            self
        }
    }

    /// A row of the agents list, which says when it is the one at the top.
    fileprivate func agentsRow(_ id: String) -> some View {
        modifier(AgentsListRow(id: id))
    }
}

/// The row at the top of an agents list, how far the list is scrolled past its top, and how tall it is.
struct AgentsTop: Equatable {
    let row: String
    let past: CGFloat
    let height: CGFloat
}

extension EnvironmentValues {
    /// Takes the row at the top of the agents list.
    @Entry fileprivate var keepAgentsTop: ((AgentsTop) -> Void)?
}

/// Scrolls to the row that was at the top, by its id, so that it lands there however tall the rows
/// above are guessed to be.
@available(macOS 15, *)
private struct KeepsAgentsScroll: ViewModifier {
    @Environment(AppStore.self) private var store
    let key: String?
    /// The thread's agents have arrived.
    let ready: Bool
    @State private var position = ScrollPosition()
    @State private var restored = false
    /// How tall the part of the list it shows is.
    @State private var height: CGFloat = 0

    func body(content: Content) -> some View {
        content
            .environment(\.keepAgentsTop) { top in
                guard restored, let key else { return }
                store.rightPanel.agentsTop[key] = top
            }
            .scrollPosition($position)
            .onScrollGeometryChange(for: Spot.self) { Spot(offset: $0.contentOffset.y, height: $0.containerSize.height) } action: { _, spot in
                height = spot.height
                if !restored {
                    restore()
                } else if spot.offset <= 0, let key {
                    store.rightPanel.agentsTop[key] = nil
                }
            }
            .onChange(of: ready, initial: true) { restore() }
    }

    private struct Spot: Equatable {
        let offset: CGFloat
        let height: CGFloat
    }

    private func restore() {
        guard ready, !restored, height > 0 else { return }
        restored = true
        guard let key, let top = store.rightPanel.agentsTop[key] else { return }
        // The anchor puts the row's top `past` above the list's: an anchor at `y` of the row meets the same of the list.
        position.scrollTo(id: top.row, anchor: UnitPoint(x: 0, y: -top.past / max(1, height - top.height)))
    }
}

private struct AgentsListRow: ViewModifier {
    @Environment(\.keepAgentsTop) private var keep
    let id: String

    func body(content: Content) -> some View {
        content
            .id(id)
            .onGeometryChange(for: AgentsTop?.self) { geometry in
                let frame = geometry.frame(in: .scrollView)
                return frame.minY <= 0 && frame.maxY > 0 ? AgentsTop(row: id, past: -frame.minY, height: frame.height) : nil
            } action: { top in
                guard let top else { return }
                keep?(top)
            }
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
            HStack(spacing: 6) {
                Image(.cornerLeftUp, size: 12)
                    .foregroundStyle(Color.themeForeground)
                    .frame(width: 16)
                Text("Go back to parent thread")
                    .font(.ui(size: 13, weight: .medium))
                    .foregroundStyle(Color.themeForeground)
                    .lineLimit(1)
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
    @Environment(\.surface) private var surface

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
        .overlay {
            if store.rightPanel.visitedAgent == agent.id {
                RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
                    .strokeBorder(surface.color(.controlLit), style: StrokeStyle(lineWidth: 1, dash: [4, 3]))
                    .padding(panelRowMargin)
                    .allowsHitTesting(false)
            }
        }
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
        store.rightPanel.visit(agent: agent.id)
        if let thread = agent.threadID { return store.openStartedThread(thread) }
        store.rightPanel.showAgent(agent.id)
    }

    private func stop() {
        guard let thread = agent.threadID else { return }
        store.stop(threadID: thread)
    }
}

/// How long the agent has worked so far, or worked in all, or until its usage limit stopped it.
struct AgentTime: View {
    let agent: AgentInfo

    var body: some View {
        if agent.working && agent.state != .limited {
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
