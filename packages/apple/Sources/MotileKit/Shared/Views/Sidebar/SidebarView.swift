import SwiftUI

/// The space between the rows' highlights, and around each one: it looks empty but is the row's.
private let rowGap = 2.0
/// How far the rows' highlights stay from the sidebar's edges.
let sidebarRowInset: CGFloat = 10
private let rowMargin = EdgeInsets(top: rowGap / 2, leading: sidebarRowInset, bottom: rowGap / 2, trailing: sidebarRowInset)
let doneRowHeight = Double(scaled(30))
/// One clock for every "5m" in the sidebar, so that they all change at once.
private let agoClock = PeriodicTimelineSchedule(from: .now, by: 30)

#if os(macOS)
/// The drafts, then every active thread on every server in one list, with the ones marked done on
/// a shelf at the bottom.
struct SidebarView: View {
    static let rowInset = sidebarRowInset

    @Environment(AppStore.self) private var store
    @AppStorage("sidebar.doneExpanded") private var doneExpanded = false
    @State private var renaming: ThreadInfo?
    @State private var newTitle = ""
    @State private var deleting: ThreadInfo?
    @State private var search = ""

    var body: some View {
        let active = store.activeThreads.filter(matches)
        let done = store.doneThreads.filter(matches)
        let projects = store.projectsByID
        let selection = store.selection
        VStack(spacing: 0) {
            SearchField(text: $search)
                .padding(.horizontal, 10)
                .padding(.top, 2)
                .padding(.bottom, 6)
            GeometryReader { list in
                threads(active: active, done: done, projects: projects, selection: selection, maxDoneHeight: list.size.height * 0.6)
            }
        }
        .onChange(of: store.settledThreadID) { _, settled in
            guard settled != nil else { return }
            doneExpanded = true
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
            Text(store.deletionNote(deleting))
        }
    }

    private func threads(
        active: [ThreadInfo], done: [ThreadInfo], projects: [String: Project], selection: Selection, maxDoneHeight: Double
    ) -> some View {
        VStack(spacing: 0) {
            ScrollView {
                LazyVStack(spacing: 0) {
                    DraftRows(search: search) { store.select($0) }
                    ForEach(active) { thread in
                        ThreadRow(
                            thread: thread, project: projects[thread.projectID]?.seen(from: thread),
                            selected: selection == .thread(thread.id), rename: beginRename, delete: { deleting = $0 }
                        ) { store.select($0) }
                        .equatable()
                    }
                    if active.isEmpty {
                        Text(!search.isEmpty ? "No threads found" : done.isEmpty ? "No threads yet" : "No active threads")
                            .font(.ui(size: 12))
                            .foregroundStyle(Color.themeTertiary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, rowMargin.leading + 8)
                            .padding(.vertical, 6 + rowGap / 2)
                    }
                }
                .padding(.vertical, 4 - rowGap / 2)
            }
            if let undo = store.undo {
                UndoRow(notice: undo)
                    .appearing()
            }
            if !done.isEmpty {
                DoneShelf(
                    threads: done,
                    projects: projects,
                    selection: selection,
                    maxHeight: maxDoneHeight,
                    expanded: search.isEmpty ? $doneExpanded : .constant(true),
                    rename: beginRename,
                    delete: { deleting = $0 }
                )
            }
            SidebarFooter()
        }
        .clipped()
        .animation(.easeOut(duration: 0.15), value: store.undo)
    }

    private func matches(_ thread: ThreadInfo) -> Bool {
        store.matches(thread, search: search)
    }

    private func beginRename(_ thread: ThreadInfo) {
        newTitle = thread.title
        renaming = thread
    }
}

#endif

/// Whether a thread's title or project matches what is being searched for.
extension AppStore {
    func matches(_ thread: ThreadInfo, search: String) -> Bool {
        guard !search.isEmpty else { return true }
        let project = project(thread.projectID)?.name ?? ""
        return thread.title.localizedCaseInsensitiveContains(search) || project.localizedCaseInsensitiveContains(search)
    }
}

struct SearchField: View {
    @Binding var text: String
    /// Without a background or a height of its own, for a field that lies on glass.
    var bare = false

    var body: some View {
        InputField(
            "Search", text: $text, icon: .search, variant: bare ? .bare : .filled, size: Platform.scale > 1 ? .large : .regular, clearable: true
        )
    }
}

struct ThreadMenu: View {
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

#if os(macOS)
private struct MarkDoneButton: View {
    let action: () -> Void

    var body: some View {
        ActionButton("Mark Done", icon: .check, variant: .ghost, size: .small, action: action)
            .fixedSize()
    }
}

#endif

/// An active thread: its project and what it is doing on the first line, its title on the second,
/// its project's branch, its pull request, its server and its agent on the third. It is redrawn only when what it
/// shows changes.
struct ThreadRow: View, Equatable {
    @Environment(AppStore.self) private var store
    let thread: ThreadInfo
    /// Its project as the thread works in it.
    let project: Project?
    let selected: Bool
    let rename: (ThreadInfo) -> Void
    let delete: (ThreadInfo) -> Void
    /// Opens what the row stands for. The sidebar it is in may have more to do then.
    var open: (Selection) -> Void = { _ in }
    @State private var hovering = false

    private static let sidePadding: CGFloat = 8
    private static let topPadding: CGFloat = 5
    /// A button on the first line is as far from the row's side as from its top.
    private static let buttonInset = topPadding + (scaled(22) - ControlSize.small.height) / 2

    static func == (one: ThreadRow, other: ThreadRow) -> Bool {
        one.thread == other.thread && one.project == other.project && one.selected == other.selected
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                ProjectIcon(project: project, size: 14)
                Text(project?.name ?? URL(fileURLWithPath: thread.cwd).lastPathComponent)
                    .font(.ui(size: 11, weight: .medium))
                    .lineLimit(1)
                    .layoutPriority(1)
                Spacer(minLength: 6)
                #if os(macOS)
                if hovering && !thread.busy {
                    MarkDoneButton { store.setDone([thread.id], done: true, fromSidebar: true) }
                        .padding(.trailing, Self.buttonInset - Self.sidePadding)
                } else {
                    ThreadStatus(thread: thread)
                }
                #else
                ThreadStatus(thread: thread)
                #endif
            }
            .foregroundStyle(Color.themeSecondary)
            .frame(height: scaled(22))

            Text(thread.title)
                .font(.ui(size: 13, weight: .medium))
                .lineLimit(1)
                .padding(.top, 1)
                .padding(.bottom, 5)

            HStack(spacing: 6) {
                if let branch = project?.branch {
                    Text(branch)
                        .font(.ui(size: 11))
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer(minLength: 6)
                if let pullRequest = project?.pullRequest(of: thread) {
                    ThreadPullRequestLabel(thread: thread, pullRequest: pullRequest, open: open)
                }
                ThreadServerLabel(serverID: thread.serverID)
                AgentIcon(agent: thread.agent, size: 12)
            }
            .foregroundStyle(Color.themeTertiary)
            .frame(height: scaled(16))
        }
        .padding(.horizontal, Self.sidePadding)
        .padding(.top, Self.topPadding)
        .padding(.bottom, 7)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(rowMargin)
        .button(.highlight(radius: 8, selected: selected, inset: rowMargin)) { open(.thread(thread.id)) }
        .onHover { hovering = $0 }
        .contextMenu { ThreadMenu(thread: thread, rename: rename, delete: delete) }
    }
}

/// A thread's pull request in its row. A click opens the thread and then the pull request's
/// tab, as View PR does.
private struct ThreadPullRequestLabel: View {
    @Environment(AppStore.self) private var store
    let thread: ThreadInfo
    let pullRequest: PullRequest
    var colored = true
    let open: (Selection) -> Void

    var body: some View {
        PullRequestLabel(pullRequest: pullRequest, colored: colored) {
            open(.thread(thread.id))
            guard let url = URL(string: pullRequest.url) else { return }
            store.showPullRequest(url)
        }
    }
}

/// The server a thread is on, when there is more than one. It is its own view so that news of a
/// server only redraws this.
private struct ThreadServerLabel: View {
    @Environment(AppStore.self) private var store
    let serverID: String

    var body: some View {
        if store.servers.count > 1, let server = store.server(serverID) {
            ServerLabel(server: server)
        }
    }
}

/// The threads that haven't been sent yet, above the others. They are their own view so that
/// typing only redraws them.
struct DraftRows: View {
    @Environment(AppStore.self) private var store
    let search: String
    var open: (Selection) -> Void = { _ in }

    var body: some View {
        let listed = store.listedDrafts.filter(matches)
        if !listed.isEmpty {
            ForEach(listed) { DraftRow(listed: $0, open: open) }
            ThemeDivider()
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
    var open: (Selection) -> Void = { _ in }
    @State private var hovering = false

    private static let sidePadding: CGFloat = 8
    private static let topPadding: CGFloat = 5
    private static let buttonInset = topPadding + (scaled(22) - ControlSize.small.height) / 2

    var body: some View {
        let project = store.project(listed.draft.projectID)
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 6) {
                ProjectIcon(project: project, size: 14)
                Text(project?.name ?? "No project")
                    .font(.ui(size: 11, weight: .medium))
                    .lineLimit(1)
                    .layoutPriority(1)
                if store.servers.count > 1, let server = store.server(project?.serverID) {
                    ServerLabel(server: server)
                }
                Spacer(minLength: 6)
                if hovering {
                    ActionButton(icon: .x, help: "Discard draft", size: .small) { store.discard(listed.draft) }
                        .padding(.trailing, Self.buttonInset - Self.sidePadding)
                } else {
                    HStack(spacing: 3) {
                        Image(.file, size: 11)
                        Text("Draft")
                            .font(.ui(size: 11, weight: .medium))
                    }
                    .foregroundStyle(Color.themeSecondary)
                }
            }
            .foregroundStyle(Color.themeSecondary)
            .frame(height: scaled(22))

            Text(listed.preview)
                .font(.ui(size: 13, weight: .medium))
                .lineLimit(1)
        }
        .padding(.horizontal, Self.sidePadding)
        .padding(.top, Self.topPadding)
        .padding(.bottom, 7)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(rowMargin)
        .button(.highlight(radius: 8, selected: store.selection == .draft(listed.id), inset: rowMargin)) { open(.draft(listed.id)) }
        .onHover { hovering = $0 }
        .contextMenu {
            Button("Discard Draft", role: .destructive) { store.discard(listed.draft) }
        }
    }
}

/// The offer to undo marking threads done: a line above the done threads.
struct UndoRow: View {
    @Environment(AppStore.self) private var store
    let notice: UndoNotice

    var body: some View {
        VStack(spacing: 0) {
            ThemeDivider()
            Button {
                store.performUndo()
            } label: {
                HStack(spacing: 7) {
                    Image(.undo2, size: 10)
                        .frame(width: 14)
                    Text("Undo")
                        .font(.ui(size: 12, weight: .medium))
                    Spacer()
                    HStack(spacing: 3) {
                        Image(.check, size: 9)
                        Text(notice.text)
                            .font(.ui(size: 11))
                            .lineLimit(1)
                    }
                }
                .padding(.horizontal, 18)
                .frame(height: doneRowHeight)
                .padding(.vertical, 4)
            }
            .buttonStyle(.highlight(radius: 0, faded: true))
        }
    }
}

#if os(macOS)
/// The threads marked done, at the bottom of the sidebar: a line that opens into their list.
private struct DoneShelf: View {
    static let rowHeight = doneRowHeight
    private static let defaultHeight = 250.0
    private static let minHeight = 4 * (rowHeight + rowGap) + 4

    @Environment(AppStore.self) private var store
    let threads: [ThreadInfo]
    let projects: [String: Project]
    let selection: Selection
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
                ThemeDivider()
            }
            Button {
                expanded.toggle()
            } label: {
                HStack(spacing: 7) {
                    Image(.chevronRight, size: 10)
                        .rotationEffect(.degrees(expanded ? 90 : 0))
                        .frame(width: 14)
                    Text("Done")
                        .font(.ui(size: 12, weight: .medium))
                    Spacer()
                    Text("\(threads.count)")
                        .font(.ui(size: 11))
                        .foregroundStyle(Color.themeTertiary)
                        .monospacedDigit()
                }
                .padding(.horizontal, 18)
                .frame(height: Self.rowHeight)
                .padding(.top, 4)
                .padding(.bottom, expanded ? 4 - rowGap / 2 : 4)
            }
            .buttonStyle(.highlight(radius: 0, faded: true))

            if expanded {
                ScrollViewReader { list in
                    ScrollView {
                        LazyVStack(spacing: 0) {
                            ForEach(threads) { thread in
                                DoneRow(
                                    thread: thread, project: projects[thread.projectID], selected: selection == .thread(thread.id),
                                    rename: rename, delete: delete
                                ) { store.select($0) }
                                .equatable()
                                .frame(height: Self.rowHeight + rowGap)
                            }
                        }
                        .padding(.bottom, 4 - rowGap / 2)
                    }
                    .onChange(of: store.settledThreadID) { _, settled in
                        guard let settled else { return }
                        withAnimation(.easeOut(duration: 0.15)) { list.scrollTo(settled) }
                    }
                }
                .frame(height: listHeight(in: heights, pulledUp: pulledUp) + rowGap / 2)
            }
            ThemeDivider()
        }
    }

    /// The line above the done threads. Dragging it makes their list taller or shorter; the
    /// height is saved when the drag ends.
    private func resizeHandle(_ heights: ClosedRange<Double>) -> some View {
        ThemeDivider()
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

#endif

/// A thread that is done: one quiet line, with its pull request. It is redrawn only when what it shows changes.
struct DoneRow: View, Equatable {
    @Environment(AppStore.self) private var store
    let thread: ThreadInfo
    let project: Project?
    let selected: Bool
    let rename: (ThreadInfo) -> Void
    let delete: (ThreadInfo) -> Void
    var open: (Selection) -> Void = { _ in }
    @State private var hovering = false

    private static let sidePadding = 8.0
    private static let buttonSize = ControlSize.small.height

    static func == (one: DoneRow, other: DoneRow) -> Bool {
        one.thread == other.thread && one.project?.iconPath == other.project?.iconPath && one.pullRequest == other.pullRequest
            && one.selected == other.selected
    }

    private var pullRequest: PullRequest? { project?.pullRequest(of: thread) }

    var body: some View {
        HStack(spacing: 7) {
            ProjectIcon(project: project, size: 14)
            Text(thread.title)
                .font(.ui(size: 13))
                .lineLimit(1)
                .foregroundStyle(Color.themeSecondary)
            Spacer(minLength: 6)
            if let pullRequest {
                ThreadPullRequestLabel(thread: thread, pullRequest: pullRequest, colored: false, open: open)
            }
            if hovering {
                ActionButton(icon: .undo2, help: "Mark undone", size: .small) { store.setDone([thread.id], done: false) }
                .padding(.trailing, (doneRowHeight - Self.buttonSize) / 2 - Self.sidePadding)
            } else {
                TimelineView(agoClock) { context in
                    Text(Time.ago(thread.doneAt ?? thread.updatedAt, now: context.date.timeIntervalSince1970))
                        .font(.ui(size: 11))
                        .foregroundStyle(Color.themeTertiary)
                }
            }
        }
        .padding(.horizontal, Self.sidePadding)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        .padding(rowMargin)
        .button(.highlight(radius: 8, selected: selected, inset: rowMargin)) { open(.thread(thread.id)) }
        .onHover { hovering = $0 }
        .contextMenu { ThreadMenu(thread: thread, rename: rename, delete: delete) }
    }
}

/// What a thread is up to, or how long ago it last was.
struct ThreadStatus: View {
    let thread: ThreadInfo

    var body: some View {
        if thread.needsApproval {
            label("Approval", Color.themeWarning) {
                symbol(.circleQuestionMark)
            }
        } else if thread.running {
            HStack(spacing: 8) {
                if thread.agents > 0 {
                    label("\(thread.agents)", Color.themeWorking) {
                        symbol(.users)
                    }
                    .help(thread.agents == 1 ? "1 agent is working" : "\(thread.agents) agents are working")
                }
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    label(Time.elapsed(since: thread.updatedAt, now: context.date.timeIntervalSince1970), Color.themeWorking) {
                        symbol(.circleDashed)
                    }
                }
            }
        } else if let stage = thread.gitStage {
            label(stage.label, Color.themeWorking) {
                Spinner(size: 11)
            }
        } else if thread.monitoring {
            TimelineView(.periodic(from: .now, by: 1)) { context in
                label(Time.elapsed(since: thread.monitoringSince, now: context.date.timeIntervalSince1970), Color.themeText) {
                    symbol(.eye)
                }
            }
        } else if thread.unread {
            label("Unread", Color.themeUnread) {
                Circle()
                    .frame(width: 6, height: 6)
            }
        } else {
            TimelineView(agoClock) { context in
                Text(Time.ago(thread.updatedAt, now: context.date.timeIntervalSince1970))
                    .font(.ui(size: 11))
                    .foregroundStyle(Color.themeTertiary)
            }
        }
    }

    private func label(_ text: String, _ color: Color, @ViewBuilder icon: () -> some View) -> some View {
        HStack(spacing: 3) {
            icon()
            Text(text)
                .font(.ui(size: 11, weight: .medium))
                .monospacedDigit()
        }
        .foregroundStyle(color)
    }

    private func symbol(_ name: Symbol) -> some View {
        Image(name, size: 11)
    }
}

/// A server and how the client reaches it.
struct ServerLine: View {
    let server: Server

    var body: some View {
        HStack(spacing: 7) {
            Circle()
                .fill(color)
                .frame(width: 7, height: 7)
            Text(server.name)
                .font(.ui(size: 12, weight: .medium))
                .lineLimit(1)
            Spacer(minLength: 4)
            ServerUpdateStatus(server: server) {
                Text(detail)
                    .font(.ui(size: 11))
                    .foregroundStyle(Color.themeTertiary)
                    .monospacedDigit()
            }
        }
        .frame(height: scaled(20))
        .help(server.error ?? detail)
    }

    private var color: Color {
        switch server.state {
        case .connected: return Color.themeSuccess
        case .connecting: return Color.themeWarning
        case .disconnected, .refused: return Color.themeDanger
        }
    }

    private var detail: String {
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

#if os(macOS)
/// The servers and how the client reaches them, and the account.
private struct SidebarFooter: View {
    @Environment(AppStore.self) private var store

    /// The account's line takes clicks up to the sidebar's edges, and halfway to the line above.
    private static let accountMargin = EdgeInsets(top: 4, leading: 10, bottom: 8, trailing: 10)

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            AppUpdateRow(updater: store.updater)
            ForEach(store.servers) { ServerLine(server: $0) }
            Menu {
                Button("Settings…") { store.openSettings() }
                Button("Add a Project…") { store.addProject() }
                Button("Add a Server…") { store.showsAddServer = true }
                Divider()
                Button("Sign Out") { store.signOut() }
            } label: {
                HStack(spacing: 7) {
                    Image(.circleUser, size: 14)
                    Text(store.account.email)
                        .font(.ui(size: 12))
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 8)
                .frame(height: 30)
                .padding(Self.accountMargin)
                .contentShape(Rectangle())
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .hoverHighlight(radius: 8, inset: Self.accountMargin, faded: true)
            .padding(.horizontal, -8 - Self.accountMargin.leading)
            .padding(.top, -Self.accountMargin.top)
            .padding(.bottom, -Self.accountMargin.bottom)
        }
        .padding(.horizontal, 18)
        .padding(.top, 10)
        .padding(.bottom, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
#endif
