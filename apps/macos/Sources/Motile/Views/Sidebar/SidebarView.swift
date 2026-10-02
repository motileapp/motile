import SwiftUI

/// The space between the rows' highlights, and around each one: it looks empty but is the row's.
private let rowGap = 2.0
private let rowMargin = EdgeInsets(top: rowGap / 2, leading: 10, bottom: rowGap / 2, trailing: 10)

/// The drafts, then every active thread on every server in one list, with the ones marked done on
/// a shelf at the bottom.
struct SidebarView: View {
    @Environment(AppStore.self) private var store
    @AppStorage("sidebar.doneExpanded") private var doneExpanded = false
    @State private var renaming: ThreadInfo?
    @State private var newTitle = ""
    @State private var deleting: ThreadInfo?
    @State private var search = ""

    var body: some View {
        let active = store.activeThreads.filter(matches)
        let done = store.doneThreads.filter(matches)
        VStack(spacing: 0) {
            SearchField(text: $search)
                .padding(.horizontal, 10)
                .padding(.top, 2)
                .padding(.bottom, 6)
            GeometryReader { list in
                threads(active: active, done: done, maxDoneHeight: list.size.height * 0.6)
            }
        }
        .alert("Rename thread", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
            TextField("Title", text: $newTitle)
            Button("Rename") {
                if let thread = renaming { store.rename(thread, to: newTitle) }
                renaming = nil
            }
            Button("Cancel", role: .cancel) { renaming = nil }
        }
        .confirmationDialog(
            "Delete “\(deleting?.title ?? "")”?",
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })
        ) {
            Button("Delete", role: .destructive) {
                if let thread = deleting { store.delete(thread) }
                deleting = nil
            }
        } message: {
            Text("The thread and its transcript are removed from its server. Files the agent changed stay as they are.")
        }
    }

    private func threads(active: [ThreadInfo], done: [ThreadInfo], maxDoneHeight: Double) -> some View {
        VStack(spacing: 0) {
            ScrollView {
                LazyVStack(spacing: 0) {
                    DraftRows(search: search)
                    ForEach(active) { thread in
                        ThreadRow(thread: thread, rename: beginRename, delete: { deleting = $0 })
                    }
                    if active.isEmpty {
                        Text(!search.isEmpty ? "No threads found" : done.isEmpty ? "No threads yet" : "No active threads")
                            .font(.system(size: 12))
                            .foregroundStyle(Color.themeTertiary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, rowMargin.leading + 8)
                            .padding(.vertical, 6 + rowGap / 2)
                    }
                }
                .padding(.vertical, 4 - rowGap / 2)
            }
            if !done.isEmpty {
                DoneShelf(
                    threads: done,
                    maxHeight: maxDoneHeight,
                    expanded: search.isEmpty ? $doneExpanded : .constant(true),
                    rename: beginRename,
                    delete: { deleting = $0 }
                )
            }
            SidebarFooter()
        }
        .clipped()
    }

    /// Whether the thread's title or project matches what is being searched for.
    private func matches(_ thread: ThreadInfo) -> Bool {
        guard !search.isEmpty else { return true }
        let project = store.project(thread.projectID)?.name ?? ""
        return thread.title.localizedCaseInsensitiveContains(search) || project.localizedCaseInsensitiveContains(search)
    }

    private func beginRename(_ thread: ThreadInfo) {
        newTitle = thread.title
        renaming = thread
    }
}

private struct SearchField: View {
    @Binding var text: String

    private static let height: CGFloat = 28
    private static let sidePadding: CGFloat = 8
    private static let clearSize: CGFloat = 18

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(Color.themeTertiary)
            TextField("Search", text: $text)
                .textFieldStyle(.plain)
                .font(.system(size: 13))
            if !text.isEmpty {
                IconOnlyButton(symbol: "xmark.circle.fill", help: "Clear", size: Self.clearSize, symbolSize: 12) { text = "" }
                    .foregroundStyle(Color.themeTertiary)
                    .padding(.trailing, (Self.height - Self.clearSize) / 2 - Self.sidePadding)
            }
        }
        .padding(.horizontal, Self.sidePadding)
        .frame(height: Self.height)
        .background(Color.themeHover, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
    }
}

private struct ThreadMenu: View {
    @Environment(AppStore.self) private var store
    let thread: ThreadInfo
    let rename: (ThreadInfo) -> Void
    let delete: (ThreadInfo) -> Void

    var body: some View {
        if thread.isDone {
            Button("Mark Undone") { store.setDone([thread.id], done: false) }
        } else {
            Button("Mark Done") { store.setDone([thread.id], done: true, fromSidebar: true) }
                .disabled(thread.busy)
        }
        Button("Rename…") { rename(thread) }
        Divider()
        Button("Delete…", role: .destructive) { delete(thread) }
    }
}

/// An active thread: its project and what it is doing on the first line, its title on the second,
/// its project's branch and its agent on the third.
private struct ThreadRow: View {
    @Environment(AppStore.self) private var store
    let thread: ThreadInfo
    let rename: (ThreadInfo) -> Void
    let delete: (ThreadInfo) -> Void
    @State private var hovering = false

    private static let sidePadding: CGFloat = 8
    private static let topPadding: CGFloat = 3

    var body: some View {
        let project = store.project(thread.projectID)
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 6) {
                ProjectIcon(project: project, size: 14)
                Text(projectName)
                    .font(.system(size: 11, weight: .medium))
                    .lineLimit(1)
                    .layoutPriority(1)
                if store.servers.count > 1, let server = store.server(thread.serverID) {
                    ServerLabel(server: server)
                }
                Spacer(minLength: 6)
                if hovering && !thread.busy {
                    IconOnlyButton(symbol: "checkmark", help: "Mark done", size: 22, symbolSize: 12) {
                        store.setDone([thread.id], done: true, fromSidebar: true)
                    }
                    .padding(.trailing, Self.topPadding - Self.sidePadding)
                } else {
                    ThreadStatus(thread: thread)
                }
            }
            .foregroundStyle(.secondary)
            .frame(height: 22)

            Text(thread.title)
                .font(.system(size: 13, weight: .medium))
                .lineLimit(1)

            HStack(spacing: 6) {
                if let branch = project?.branch {
                    Text(branch)
                        .font(.system(size: 11))
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer(minLength: 6)
                AgentIcon(agent: thread.agent, size: 12)
            }
            .foregroundStyle(.tertiary)
            .frame(height: 16)
        }
        .padding(.horizontal, Self.sidePadding)
        .padding(.top, Self.topPadding)
        .padding(.bottom, 7)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(rowMargin)
        .contentShape(Rectangle())
        .hoverHighlight(radius: 8, selected: store.selection == .thread(thread.id), inset: rowMargin)
        .onHover { hovering = $0 }
        .onTapGesture { store.select(.thread(thread.id)) }
        .contextMenu { ThreadMenu(thread: thread, rename: rename, delete: delete) }
    }

    private var projectName: String {
        store.project(thread.projectID)?.name ?? URL(fileURLWithPath: thread.cwd).lastPathComponent
    }
}

/// The threads that haven't been sent yet, above the others. They are their own view so that
/// typing only redraws them.
private struct DraftRows: View {
    @Environment(AppStore.self) private var store
    let search: String

    var body: some View {
        let listed = store.listedDrafts.filter(matches)
        if !listed.isEmpty {
            ForEach(listed) { DraftRow(listed: $0) }
            Divider()
                .padding(.horizontal, rowMargin.leading + 8)
                .padding(.vertical, 4 + rowGap / 2)
        }
    }

    private func matches(_ listed: ListedDraft) -> Bool {
        guard !search.isEmpty else { return true }
        let project = store.project(listed.draft.projectID)?.name ?? ""
        return listed.preview.localizedCaseInsensitiveContains(search) || project.localizedCaseInsensitiveContains(search)
    }
}

/// A draft: its project on the first line, what was written in it on the second, if anything.
private struct DraftRow: View {
    @Environment(AppStore.self) private var store
    let listed: ListedDraft
    @State private var hovering = false

    private static let sidePadding: CGFloat = 8
    private static let topPadding: CGFloat = 3

    var body: some View {
        let project = store.project(listed.draft.projectID)
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 6) {
                ProjectIcon(project: project, size: 14)
                Text(project?.name ?? "No project")
                    .font(.system(size: 11, weight: .medium))
                    .lineLimit(1)
                    .layoutPriority(1)
                if store.servers.count > 1, let server = store.server(project?.serverID) {
                    ServerLabel(server: server)
                }
                Spacer(minLength: 6)
                if hovering {
                    IconOnlyButton(symbol: "xmark", help: "Discard draft", size: 22, symbolSize: 12) {
                        store.discard(listed.draft)
                    }
                    .padding(.trailing, Self.topPadding - Self.sidePadding)
                } else {
                    HStack(spacing: 3) {
                        Image(systemName: "square.and.pencil")
                            .font(.system(size: 11, weight: .semibold))
                        Text("Draft")
                            .font(.system(size: 11, weight: .medium))
                    }
                    .foregroundStyle(Color.themeWarning)
                }
            }
            .foregroundStyle(.secondary)
            .frame(height: 22)

            Text(listed.preview)
                .font(.system(size: 13, weight: .medium))
                .lineLimit(1)
        }
        .padding(.horizontal, Self.sidePadding)
        .padding(.top, Self.topPadding)
        .padding(.bottom, 7)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(rowMargin)
        .contentShape(Rectangle())
        .hoverHighlight(radius: 8, selected: store.selection == .draft(listed.id), inset: rowMargin)
        .onHover { hovering = $0 }
        .onTapGesture { store.select(.draft(listed.id)) }
        .contextMenu {
            Button("Discard Draft", role: .destructive) { store.discard(listed.draft) }
        }
    }
}

/// The threads marked done, at the bottom of the sidebar: a line that opens into their list.
private struct DoneShelf: View {
    static let rowHeight = 30.0
    private static let defaultHeight = 250.0
    private static let minHeight = 4 * (rowHeight + rowGap) + 4

    let threads: [ThreadInfo]
    let maxHeight: Double
    @Binding var expanded: Bool
    let rename: (ThreadInfo) -> Void
    let delete: (ThreadInfo) -> Void
    @AppStorage("sidebar.doneHeight") private var height = DoneShelf.defaultHeight
    @GestureState private var pulledUp = 0.0

    var body: some View {
        let contentHeight = Double(threads.count) * (Self.rowHeight + rowGap) + 4
        let tallest = min(maxHeight, contentHeight)
        let heights = min(Self.minHeight, tallest)...tallest
        VStack(spacing: 0) {
            if expanded && heights.lowerBound < heights.upperBound {
                resizeHandle(heights)
            } else {
                Divider()
            }
            Button {
                expanded.toggle()
            } label: {
                HStack(spacing: 7) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 10, weight: .semibold))
                        .rotationEffect(.degrees(expanded ? 90 : 0))
                        .frame(width: 14)
                    Text("Done")
                        .font(.system(size: 12, weight: .medium))
                    Spacer()
                    Text("\(threads.count)")
                        .font(.system(size: 11))
                        .foregroundStyle(.tertiary)
                        .monospacedDigit()
                }
                .foregroundStyle(.secondary)
                .padding(.horizontal, 18)
                .frame(height: Self.rowHeight)
                .padding(.top, 4)
                .padding(.bottom, expanded ? 4 - rowGap / 2 : 4)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .hoverHighlight(radius: 0)

            if expanded {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(threads) { thread in
                            DoneRow(thread: thread, rename: rename, delete: delete)
                                .frame(height: Self.rowHeight + rowGap)
                        }
                    }
                    .padding(.bottom, 4 - rowGap / 2)
                }
                .frame(height: listHeight(in: heights, pulledUp: pulledUp) + rowGap / 2)
            }
        }
    }

    /// The line above the done threads. Dragging it makes their list taller or shorter; the
    /// height is saved when the drag ends.
    private func resizeHandle(_ heights: ClosedRange<Double>) -> some View {
        Divider()
            .overlay {
                Color.clear
                    .frame(height: Theme.resizeGrab)
                    .contentShape(Rectangle())
                    .onHover { inside in
                        if inside { NSCursor.resizeUpDown.push() } else { NSCursor.pop() }
                    }
                    .gesture(
                        DragGesture(minimumDistance: 1, coordinateSpace: .global)
                            .updating($pulledUp) { drag, pull, _ in pull = -drag.translation.height }
                            .onEnded { drag in
                                height = listHeight(in: heights, pulledUp: -drag.translation.height)
                            }
                    )
            }
            .zIndex(1)
    }

    private func listHeight(in heights: ClosedRange<Double>, pulledUp: Double) -> Double {
        let resting = min(heights.upperBound, max(heights.lowerBound, height))
        return min(heights.upperBound, max(heights.lowerBound, resting + pulledUp))
    }
}

/// A thread that is done: one quiet line.
private struct DoneRow: View {
    @Environment(AppStore.self) private var store
    let thread: ThreadInfo
    let rename: (ThreadInfo) -> Void
    let delete: (ThreadInfo) -> Void
    @State private var hovering = false

    private static let sidePadding = 8.0
    private static let buttonSize = 22.0

    var body: some View {
        HStack(spacing: 7) {
            ProjectIcon(project: store.project(thread.projectID), size: 14)
            Text(thread.title)
                .font(.system(size: 13))
                .lineLimit(1)
                .foregroundStyle(.secondary)
            Spacer(minLength: 6)
            if hovering {
                IconOnlyButton(symbol: "arrow.uturn.backward", help: "Mark undone", size: Self.buttonSize, symbolSize: 12) {
                    store.setDone([thread.id], done: false)
                }
                .foregroundStyle(.secondary)
                .padding(.trailing, (DoneShelf.rowHeight - Self.buttonSize) / 2 - Self.sidePadding)
            } else {
                TimelineView(.periodic(from: .now, by: 30)) { context in
                    Text(Time.ago(thread.doneAt ?? thread.updatedAt, now: context.date.timeIntervalSince1970))
                        .font(.system(size: 11))
                        .foregroundStyle(.tertiary)
                }
            }
        }
        .padding(.horizontal, Self.sidePadding)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        .padding(rowMargin)
        .contentShape(Rectangle())
        .hoverHighlight(radius: 8, selected: store.selection == .thread(thread.id), inset: rowMargin)
        .onHover { hovering = $0 }
        .onTapGesture { store.select(.thread(thread.id)) }
        .contextMenu { ThreadMenu(thread: thread, rename: rename, delete: delete) }
    }
}

/// What a thread is up to, or how long ago it last was.
private struct ThreadStatus: View {
    let thread: ThreadInfo

    var body: some View {
        if thread.needsApproval {
            label("Approval", Color.themeWarning) {
                symbol("questionmark.circle")
            }
        } else if thread.running {
            TimelineView(.periodic(from: .now, by: 1)) { context in
                label("Working \(Time.elapsed(since: thread.updatedAt, now: context.date.timeIntervalSince1970))", Color.themeWorking) {
                    symbol("circle.dashed")
                }
            }
        } else if thread.monitoring {
            label("Monitoring", Color.themeSecondary) {
                symbol("eye")
            }
        } else if thread.unread {
            label("Unread", Color.themeUnread) {
                Circle()
                    .frame(width: 6, height: 6)
            }
        } else {
            TimelineView(.periodic(from: .now, by: 30)) { context in
                Text(Time.ago(thread.updatedAt, now: context.date.timeIntervalSince1970))
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
            }
        }
    }

    private func label(_ text: String, _ color: Color, @ViewBuilder icon: () -> some View) -> some View {
        HStack(spacing: 3) {
            icon()
            Text(text)
                .font(.system(size: 11, weight: .medium))
                .monospacedDigit()
        }
        .foregroundStyle(color)
    }

    private func symbol(_ name: String) -> some View {
        Image(systemName: name)
            .font(.system(size: 11, weight: .semibold))
    }
}

/// The servers and how the app reaches them, the account, and the offer to undo.
private struct SidebarFooter: View {
    @Environment(AppStore.self) private var store
    @Environment(\.openSettings) private var openSettings

    /// The account's line takes clicks up to the sidebar's edges, and halfway to the line above.
    private static let accountMargin = EdgeInsets(top: 4, leading: 10, bottom: 8, trailing: 10)

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            AppUpdateRow(updater: store.updater)
            if let undo = store.undo {
                HStack(spacing: 6) {
                    Text(undo.text)
                        .foregroundStyle(.secondary)
                    Button("Undo") { store.performUndo() }
                        .buttonStyle(.link)
                    Spacer()
                }
                .font(.system(size: 12))
                .transition(.opacity)
            }
            ForEach(store.servers) { server in
                HStack(spacing: 7) {
                    Circle()
                        .fill(color(of: server))
                        .frame(width: 7, height: 7)
                    Text(server.name)
                        .font(.system(size: 12, weight: .medium))
                        .lineLimit(1)
                    Spacer(minLength: 4)
                    ServerUpdateStatus(server: server) {
                        Text(detail(of: server))
                            .font(.system(size: 11))
                            .foregroundStyle(.tertiary)
                            .monospacedDigit()
                    }
                }
                .frame(height: 20)
                .help(server.error ?? detail(of: server))
            }
            Menu {
                Button("Settings…") { openSettings() }
                Button("Add a Project…") { store.showsFolderPicker = true }
                Button("Add a Server…") { store.showsAddServer = true }
                Divider()
                Button("Sign Out") { store.signOut() }
            } label: {
                HStack(spacing: 7) {
                    Image(systemName: "person.crop.circle")
                        .font(.system(size: 14))
                    Text(store.account.email)
                        .font(.system(size: 12))
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Spacer(minLength: 0)
                }
                .foregroundStyle(.secondary)
                .padding(.horizontal, 8)
                .frame(height: 30)
                .padding(Self.accountMargin)
                .contentShape(Rectangle())
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .hoverHighlight(radius: 8, inset: Self.accountMargin)
            .padding(.horizontal, -8 - Self.accountMargin.leading)
            .padding(.top, -Self.accountMargin.top)
            .padding(.bottom, -Self.accountMargin.bottom)
        }
        .padding(.horizontal, 18)
        .padding(.top, 10)
        .padding(.bottom, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .animation(.easeOut(duration: 0.15), value: store.undo)
    }

    private func color(of server: Server) -> Color {
        switch server.state {
        case .connected: return Color.themeSuccess
        case .connecting: return Color.themeWarning
        case .disconnected, .refused: return Color.themeDanger
        }
    }

    private func detail(of server: Server) -> String {
        switch server.state {
        case .connected:
            let path = server.path ?? "connected"
            return server.rttMs.map { "\(path) · \($0) ms" } ?? path
        case .connecting: return "connecting…"
        case .disconnected: return "offline"
        case .refused: return "refused"
        }
    }
}
