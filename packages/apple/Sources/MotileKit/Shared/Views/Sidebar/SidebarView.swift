import SwiftUI

/// The space between the rows' highlights, and around each one: it looks empty but is the row's.
private let rowGap = 2.0
/// How far the rows' highlights stay from the sidebar's edges.
let sidebarRowInset: CGFloat = 10
let rowMargin = EdgeInsets(top: rowGap / 2, leading: sidebarRowInset, bottom: rowGap / 2, trailing: sidebarRowInset)
let doneRowHeight = Double(scaled(30))
/// One clock for every "5m" in the sidebar, so that they all change at once.
@Observable
final class AgoClock {
    static let shared = AgoClock()
    private(set) var now = Date().timeIntervalSince1970

    private init() {
        Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
            self?.now = Date().timeIntervalSince1970
        }
    }
}

#if os(macOS)
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
        let active = store.searched(store.activeThreads, for: search)
        let done = store.searched(store.doneThreads, for: search)
        let projects = store.projectsByID
        let selection = store.selection
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                SearchField(text: $search)
                ProjectButtons()
            }
            .padding(.horizontal, 10)
                .padding(.top, 2)
                .padding(.bottom, 6)
            GeometryReader { list in
                threads(active: active, done: done, projects: projects, selection: selection, sidebarHeight: list.size.height)
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
        active: [ThreadInfo], done: [ThreadInfo], projects: [String: Project], selection: Selection, sidebarHeight: Double
    ) -> some View {
        let drafts = store.searched(store.listedDrafts, for: search)
        let items = items(drafts: drafts, active: active)
        return VStack(spacing: 0) {
            // A table, which picks its rows up and moves them itself.
            RecycledList(
                items: items, height: { height(of: $0, drafts: drafts.count) }, topInset: 4 - rowGap / 2, bottomInset: 4 - rowGap / 2,
                clicked: { item in
                    guard case .active(let thread) = item else { return }
                    store.select(.thread(thread.id))
                },
                rowInset: NSEdgeInsets(top: rowMargin.top, left: rowMargin.leading, bottom: rowMargin.bottom, right: rowMargin.trailing),
                rowRadius: 8,
                movable: { item in
                    guard search.isEmpty, active.count > 1, case .active(let thread) = item else { return false }
                    return store.moves(thread)
                },
                moved: { item, index in
                    guard case .active(let thread) = item else { return }
                    store.move(thread, to: index, among: items.map(\.activeThread))
                },
                pointed: { item, row, view in ThreadPeek.shared.point(at: item?.activeThread, row: row, in: view, store: store) }
            ) { item in
                switch item {
                case .drafts:
                    VStack(spacing: 0) {
                        DraftRows(search: search) { store.select($0) }
                    }
                case .active(let thread):
                    ThreadRow(
                        thread: thread, project: projects[thread.projectID]?.seen(from: thread),
                        selected: selection == .thread(thread.id), rename: beginRename, delete: { deleting = $0 }
                    ) { store.select($0) }
                    .equatable()
                case .empty:
                    Text(!search.isEmpty ? "No threads found" : done.isEmpty ? "No threads yet" : "No active threads")
                        .font(.ui(size: 12))
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                        .frame(height: Self.emptyLineHeight)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, rowMargin.leading + 8)
                        .padding(.vertical, 6 + rowGap / 2)
                case .newThread:
                    NewThreadRow()
                }
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
                    maxHeight: sidebarHeight * 0.5,
                    expanded: search.isEmpty ? $doneExpanded : .constant(true),
                    rename: beginRename,
                    delete: { deleting = $0 }
                )
            }
            if !store.servers.isEmpty {
                ServersShelf(maxHeight: sidebarHeight * 0.3)
            }
            if !done.isEmpty || !store.servers.isEmpty {
                ThemeDivider()
            }
            SidebarFooter()
        }
        .clipped()
        .animation(.easeOut(duration: 0.15), value: store.undo)
    }

    private static let emptyLineHeight = scaled(15)

    /// The drafts as one row, then the active threads.
    private func items(drafts: [ListedDraft], active: [ThreadInfo]) -> [SidebarItem] {
        var items: [SidebarItem] = drafts.isEmpty ? [] : [.drafts]
        items += active.map(SidebarItem.active)
        if active.isEmpty { items.append(.empty) }
        if store.offersNewThread(drafts: drafts, active: active, search: search) { items.append(.newThread) }
        return items
    }

    private func height(of item: SidebarItem, drafts: Int) -> CGFloat {
        switch item {
        case .drafts: CGFloat(drafts) * (DraftRow.height + rowGap) + DraftRows.dividerHeight
        case .active: ThreadRow.height + rowGap
        case .empty: Self.emptyLineHeight + 2 * (6 + rowGap / 2)
        case .newThread: doneRowHeight + rowGap
        }
    }

    private func beginRename(_ thread: ThreadInfo) {
        newTitle = thread.title
        renaming = thread
    }
}

private enum SidebarItem: Identifiable {
    case drafts
    case active(ThreadInfo)
    case empty
    case newThread

    var id: String {
        switch self {
        case .drafts: "drafts"
        case .active(let thread): thread.id
        case .empty: "empty"
        case .newThread: "newThread"
        }
    }

    var activeThread: ThreadInfo? {
        guard case .active(let thread) = self else { return nil }
        return thread
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

    func searched(_ threads: [ThreadInfo], for search: String) -> [ThreadInfo] {
        search.isEmpty ? threads : threads.filter { matches($0, search: search) }
    }

    /// The drafts whose text or project matches what is being searched for.
    func searched(_ drafts: [ListedDraft], for search: String) -> [ListedDraft] {
        guard !search.isEmpty else { return drafts }
        return drafts.filter { listed in
            let project = project(listed.draft.projectID)?.name ?? ""
            return listed.preview.localizedCaseInsensitiveContains(search) || project.localizedCaseInsensitiveContains(search)
        }
    }
}

#if os(macOS)
/// Adds a project or starts a thread, beside the sidebar's search.
struct ProjectButtons: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        HStack(spacing: 0) {
            ActionButton(icon: .folderPlus, help: "Add a project") { store.addProject() }
            ActionButton(icon: .squarePen, help: "New thread (⌘N). ⇧-click starts one in this project") {
                guard NSApp.currentEvent?.modifierFlags.contains(.shift) == true else { return store.newThread() }
                store.startNewThread(in: store.composerProject)
            }
        }
    }
}
#endif

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

/// Something a row's menu offers.
struct RowAction {
    let title: String
    var destructive = false
    var disabled = false
    let perform: () -> Void
}

extension AppStore {
    /// What a thread's menu offers, in groups a line divides.
    func rowActions(for thread: ThreadInfo, rename: @escaping (ThreadInfo) -> Void, delete: @escaping (ThreadInfo) -> Void) -> [[RowAction]] {
        let done = thread.isDone
            ? RowAction(title: "Mark Undone") { self.setDone([thread.id], done: false) }
            : RowAction(title: "Mark Done", disabled: thread.busy) { self.setDone([thread.id], done: true, fromSidebar: true) }
        return [[done, RowAction(title: "Rename") { rename(thread) }], [RowAction(title: "Delete", destructive: true) { delete(thread) }]]
    }
}

/// A thread's menu, as `rowActions` says.
struct ThreadMenu: View {
    @Environment(AppStore.self) private var store
    let thread: ThreadInfo
    let rename: (ThreadInfo) -> Void
    let delete: (ThreadInfo) -> Void

    var body: some View {
        let groups = store.rowActions(for: thread, rename: rename, delete: delete)
        ForEach(groups.indices, id: \.self) { index in
            if index > 0 { Divider() }
            ForEach(groups[index], id: \.title) { action in
                Button(action.title, role: action.destructive ? .destructive : nil, action: action.perform)
                    .disabled(action.disabled)
            }
        }
    }
}

#if os(macOS)
private struct MarkDoneButton: View {
    let action: () -> Void

    var body: some View {
        ActionButton(
            "Mark Done", icon: .check, variant: .ghost, size: .small, symbolSize: ControlSize.small.smallSymbol, action: action
        )
            .fixedSize()
    }
}

#endif

/// An active thread: its project and what it is doing on the first line, its title on the second,
/// its branch (or its folder outside git), its pull request, its server and its agent on the third.
/// It is redrawn only when what it shows changes.
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

    static let sidePadding: CGFloat = 8
    static let topPadding: CGFloat = 5
    static let bottomPadding: CGFloat = 7
    static let titleHeight = scaled(16)
    /// A button on the first line is as far from the row's side as from its top.
    static let buttonInset = topPadding + (scaled(22) - ControlSize.small.height) / 2
    /// How tall the row is, without the gap around it.
    static let height = topPadding + scaled(22) + 1 + titleHeight + 5 + scaled(16) + bottomPadding

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
            .foregroundStyle(Color.themeMutedForeground)
            .frame(height: scaled(22))

            Text(thread.title)
                .font(.ui(size: 13, weight: .medium))
                .lineLimit(1)
                .foregroundStyle(selected || hovering ? Color.themeForeground : Color.themeMutedForeground)
                .frame(height: Self.titleHeight)
                .padding(.top, 1)
                .padding(.bottom, 5)

            HStack(spacing: 6) {
                if let project, let branch = project.branch {
                    CheckoutLabel(symbol: project.checkoutSymbol, text: branch)
                } else {
                    ThreadFolderLabel(serverID: thread.serverID, folder: thread.cwd)
                }
                Spacer(minLength: 6)
                if let pullRequest = project?.pullRequest(of: thread) {
                    ThreadPullRequestLabel(thread: thread, pullRequest: pullRequest, open: open)
                }
                ThreadServerLabel(serverID: thread.serverID)
                AgentIcon(agent: thread.agent, size: 12)
            }
            .foregroundStyle(Color.themeMutedStrongerForeground)
            .frame(height: scaled(16))
        }
        .padding(.horizontal, Self.sidePadding)
        .padding(.top, Self.topPadding)
        .padding(.bottom, Self.bottomPadding)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(rowMargin)
        #if os(macOS)
        // The table the row is in takes the click, so that it can drag the row as well. The menu
        // still comes up anywhere on the row.
        .contentShape(Rectangle())
        .hoverHighlight(radius: Radius.md, selected: selected, inset: rowMargin, hovered: hovering)
        #else
        // The list the row is in brings up its menu, so that a hold can drag the row as well.
        .button(.highlight(radius: Radius.md, selected: selected, inset: rowMargin, hovered: hovering)) { open(.thread(thread.id)) }
        #endif
        .onHover { hovering = $0 }
        #if os(macOS)
        .contextMenu { ThreadMenu(thread: thread, rename: rename, delete: delete) }
        #endif
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

/// The folder a thread works in, with its server's home as `~`. It is its own view so that news
/// of a server only redraws this.
struct ThreadFolderLabel: View {
    @Environment(AppStore.self) private var store
    let serverID: String
    let folder: String

    var body: some View {
        CheckoutLabel(symbol: .folder, text: Self.shortened(folder, home: store.server(serverID)?.home ?? ""))
    }

    static func shortened(_ path: String, home: String) -> String {
        guard !home.isEmpty, path.hasPrefix(home) else { return path }
        let rest = path.dropFirst(home.count)
        guard rest.isEmpty || rest.hasPrefix("/") else { return path }
        return "~" + rest
    }
}

/// Where a thread works, on the last line of its row.
private struct CheckoutLabel: View {
    let symbol: Symbol
    let text: String

    var body: some View {
        HStack(spacing: 3) {
            Image(symbol, size: 11)
            Text(text)
                .font(.ui(size: 11))
                .lineLimit(1)
                .truncationMode(.middle)
        }
    }
}

extension Project {
    var checkoutSymbol: Symbol {
        if let git, git.branch == nil { return .gitCommitHorizontal }
        return worktree == nil ? .gitBranch : .folderGit2
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
                .padding(.leading, 1)
        }
    }
}

/// The threads that haven't been sent yet, above the others. They are their own view so that
/// typing only redraws them.
struct DraftRows: View {
    @Environment(AppStore.self) private var store
    let search: String
    /// Whether a draft's row is slid aside to show its discard button, by the draft's id.
    var swiped: (String) -> Binding<Bool> = { _ in .constant(false) }
    var open: (Selection) -> Void = { _ in }

    /// What the line under the drafts takes, with the room around it.
    static let dividerHeight = 1 + 2 * (4 + rowGap / 2)

    var body: some View {
        let listed = store.searched(store.listedDrafts, for: search)
        if !listed.isEmpty {
            ForEach(listed) { listed in
                DraftRow(listed: listed, open: open)
                    #if os(iOS)
                    .rowSwipe(.trash2, "Discard Draft", tint: .themeDestructive, size: 36, isOpen: swiped(listed.id)) {
                        store.discard(listed.draft)
                    }
                    #endif
            }
            ThemeDivider()
                .padding(.horizontal, rowMargin.leading + 8)
                .padding(.vertical, 4 + rowGap / 2)
        }
    }
}

/// A draft, laid out like a thread's row: its project and "Draft" on the first line, what was
/// written in it on the second, where it will work, its server and its agent on the third.
private struct DraftRow: View {
    @Environment(AppStore.self) private var store
    let listed: ListedDraft
    var open: (Selection) -> Void = { _ in }
    @State private var hovering = false

    static let height = ThreadRow.height

    var body: some View {
        let project = store.project(listed.draft.projectID)
        let selected = store.selection == .draft(listed.id)
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                ProjectIcon(project: project, size: 14)
                Text(project?.name ?? "No project")
                    .font(.ui(size: 11, weight: .medium))
                    .lineLimit(1)
                    .layoutPriority(1)
                Spacer(minLength: 6)
                if hovering {
                    ActionButton(icon: .x, help: "Discard draft", size: .small) { store.discard(listed.draft) }
                        .padding(.trailing, ThreadRow.buttonInset - ThreadRow.sidePadding)
                } else {
                    HStack(spacing: 3) {
                        Image(.file, size: 11)
                        Text("Draft")
                            .font(.ui(size: 11, weight: .medium))
                    }
                }
            }
            .foregroundStyle(Color.themeMutedForeground)
            .frame(height: scaled(22))

            Text(listed.preview)
                .font(.ui(size: 13, weight: .medium))
                .lineLimit(1)
                .foregroundStyle(selected || hovering ? Color.themeForeground : Color.themeMutedForeground)
                .frame(height: ThreadRow.titleHeight)
                .padding(.top, 1)
                .padding(.bottom, 5)

            HStack(spacing: 6) {
                if let project, let branch = project.branch {
                    CheckoutLabel(symbol: project.checkoutSymbol, text: branch)
                } else if let project {
                    ThreadFolderLabel(serverID: project.serverID, folder: project.path)
                }
                Spacer(minLength: 6)
                if let project {
                    ThreadServerLabel(serverID: project.serverID)
                }
                if let agent = store.agent(of: listed.draft) {
                    AgentIcon(agent: agent, size: 12)
                }
            }
            .foregroundStyle(Color.themeMutedStrongerForeground)
            .frame(height: scaled(16))
        }
        .padding(.horizontal, ThreadRow.sidePadding)
        .padding(.top, ThreadRow.topPadding)
        .padding(.bottom, ThreadRow.bottomPadding)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(rowMargin)
        .button(.highlight(radius: Radius.md, selected: selected, inset: rowMargin, hovered: hovering)) {
            open(.draft(listed.id))
        }
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
/// A shelf at the bottom of the sidebar: a line that opens into a list, which the line above it
/// makes taller or shorter.
private struct Shelf<Item: Identifiable, Header: View, Row: View>: View {
    private static var rowHeight: Double { doneRowHeight }
    private static var minHeight: Double { 4 * (rowHeight + rowGap) + 4 }

    let items: [Item]
    let maxHeight: Double
    @Binding var expanded: Bool
    @Binding var height: Double
    var scrollTarget: Item.ID?
    var pointed: (Item?, CGRect, NSView) -> Void = { _, _, _ in }
    @ViewBuilder let header: () -> Header
    @ViewBuilder let row: (Item) -> Row
    @GestureState private var pulledUp = 0.0

    var body: some View {
        let contentHeight = Double(items.count) * (Self.rowHeight + rowGap) + 4
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
                    header()
                }
                .padding(.leading, 11)
                .padding(.trailing, 18)
                .frame(height: Self.rowHeight)
                .padding(.top, 3)
                .padding(.bottom, expanded ? 3 - rowGap / 2 : 3)
            }
            .buttonStyle(.highlight(radius: 0, faded: true))

            if expanded {
                RecycledList(
                    items: items, height: { _ in Self.rowHeight + rowGap }, bottomInset: 4 - rowGap / 2, scrollTarget: scrollTarget,
                    pointed: pointed, row: row
                )
                .frame(height: listHeight(in: heights, pulledUp: pulledUp) + rowGap / 2)
            }
        }
    }

    /// The line above the shelf. Dragging it makes the list taller or shorter; the height is saved
    /// when the drag ends.
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

/// The threads marked done.
private struct DoneShelf: View {
    @Environment(AppStore.self) private var store
    let threads: [ThreadInfo]
    let projects: [String: Project]
    let selection: Selection
    let maxHeight: Double
    @Binding var expanded: Bool
    let rename: (ThreadInfo) -> Void
    let delete: (ThreadInfo) -> Void
    @AppStorage("sidebar.doneHeight") private var height = 250.0

    var body: some View {
        Shelf(
            items: threads, maxHeight: maxHeight, expanded: $expanded, height: $height, scrollTarget: store.settledThreadID,
            pointed: { thread, row, view in ThreadPeek.shared.point(at: thread, row: row, in: view, store: store) }
        ) {
            Text("Done")
                .font(.ui(size: 12, weight: .medium))
            Spacer()
            Text("\(threads.count)")
                .font(.ui(size: 11))
                .foregroundStyle(Color.themeMutedStrongerForeground)
                .monospacedDigit()
        } row: { thread in
            DoneRow(
                thread: thread, project: projects[thread.projectID], selected: selection == .thread(thread.id),
                rename: rename, delete: delete
            ) { store.select($0) }
            .equatable()
        }
    }
}

/// The servers: one line for them all that opens into a line for each, or the one server's line.
private struct ServersShelf: View {
    @Environment(AppStore.self) private var store
    let maxHeight: Double
    @AppStorage("sidebar.serversExpanded") private var expanded = false
    @AppStorage("sidebar.serversHeight") private var height = 250.0

    var body: some View {
        let servers = store.servers
        if servers.count == 1 {
            VStack(spacing: 0) {
                ThemeDivider()
                ServerLine(server: servers[0])
                    .padding(.horizontal, 18)
                    .frame(height: doneRowHeight)
                    .padding(.vertical, 3)
            }
        } else {
            Shelf(items: servers, maxHeight: maxHeight, expanded: $expanded, height: $height) {
                AllServersLine(servers: servers)
            } row: { server in
                ServerLine(server: server)
                    .padding(.horizontal, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .padding(rowMargin)
            }
        }
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
    /// The undo button is as far from the row's side as from its top.
    private static let buttonInset = (doneRowHeight - ControlSize.small.height) / 2

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
                .foregroundStyle(Color.themeMutedForeground)
            Spacer(minLength: 6)
            if let pullRequest {
                ThreadPullRequestLabel(thread: thread, pullRequest: pullRequest, colored: false, open: open)
            }
            Text(Time.ago(thread.doneAt ?? thread.updatedAt, now: AgoClock.shared.now))
                .font(.ui(size: 11))
                .foregroundStyle(Color.themeMutedStrongerForeground)
                .opacity(hovering ? 0 : 1)
        }
        .padding(.horizontal, Self.sidePadding)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        .overlay(alignment: .trailing) {
            OnRowSurface {
                ActionButton(icon: .undo2, help: "Mark undone", size: .small) { store.setDone([thread.id], done: false) }
            }
            .padding(.trailing, Self.buttonInset)
            .opacity(hovering ? 1 : 0)
            .allowsHitTesting(hovering)
        }
        .padding(rowMargin)
        .button(.highlight(radius: Radius.md, selected: selected, inset: rowMargin, hovered: hovering)) { open(.thread(thread.id)) }
        .onHover { hovering = $0 }
        #if os(macOS)
        .contextMenu { ThreadMenu(thread: thread, rename: rename, delete: delete) }
        #endif
    }
}

/// Starts a thread, where the sidebar has no draft and no active thread. Laid out like a done row.
struct NewThreadRow: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        HStack(spacing: 7) {
            Image(.plus, size: 14)
                .frame(width: 14)
            Text("Create a thread")
                .font(.ui(size: 13))
                .lineLimit(1)
            Spacer(minLength: 6)
        }
        .foregroundStyle(Color.themeMutedForeground)
        .padding(.horizontal, 8)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        .padding(rowMargin)
        .button(.highlight(radius: Radius.md, inset: rowMargin)) { store.newThread() }
    }
}

extension AppStore {
    /// Whether the sidebar offers to start a thread: there is somewhere to start it, and nothing
    /// above the offer.
    func offersNewThread(drafts: [ListedDraft], active: [ThreadInfo], search: String) -> Bool {
        guard search.isEmpty, drafts.isEmpty, active.isEmpty else { return false }
        return !projects.isEmpty || !noProjects.isEmpty
    }
}

/// Covers what lies under it with the colour the row is lit in, hovered or selected.
private struct OnRowSurface<Content: View>: View {
    @ViewBuilder let content: Content
    @Environment(\.surface) private var surface
    @Environment(\.row) private var row

    var body: some View {
        content
            .padding(.leading, 6)
            .background(surface.color(row ?? .row))
    }
}

extension ThreadInfo {
    /// The colour of its status when it waits for the user: an approval, or a reply they haven't seen.
    var attentionColor: Color? {
        if needsApproval { return .themeWarning }
        if unread { return .themeSuccess }
        return nil
    }
}

/// What a thread is up to, or how long ago it last was.
struct ThreadStatus: View {
    let thread: ThreadInfo

    var body: some View {
        if let asking = thread.asking {
            label(asking.word, Color.themeWarning) {
                symbol(asking.symbol)
            }
        } else if thread.running {
            HStack(spacing: 9) {
                if thread.agents > 0 {
                    label("\(thread.agents)", Color.themePending) {
                        symbol(.users)
                    }
                    .help(thread.agents == 1 ? "1 agent is working" : "\(thread.agents) agents are working")
                }
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    label(Time.elapsed(since: thread.updatedAt, now: context.date.timeIntervalSince1970), Color.themeProcess) {
                        Image(.circleDashed, size: 10)
                    }
                }
            }
        } else if let stage = thread.gitStage {
            label(stage.label, Color.themeProcess) {
                Spinner(size: 11)
            }
        } else if thread.monitoring {
            TimelineView(.periodic(from: .now, by: 1)) { context in
                label(Time.elapsed(since: thread.monitoringSince, now: context.date.timeIntervalSince1970), Color.themeForeground) {
                    symbol(.eye)
                }
            }
        } else if let interruption = thread.interruption {
            label(interruption.word.capitalized, Color.themeWarning) {
                symbol(interruption.symbol)
            }
            .help(interruption.detail())
        } else if thread.unread {
            label("Finished", Color.themeSuccess) {
                symbol(.flag)
            }
        } else {
            Text(Time.ago(thread.updatedAt, now: AgoClock.shared.now))
                .font(.ui(size: 11))
                .foregroundStyle(Color.themeMutedStrongerForeground)
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

/// Every server in one line, which opens into a line for each: the worst of their states, how
/// they are reached and the slowest round trip.
struct AllServersLine: View {
    let servers: [Server]

    var body: some View {
        HStack(spacing: 7) {
            StateDot(tint: servers.worstTint)
            Text("Servers")
                .font(.ui(size: 12, weight: .medium))
                .lineLimit(1)
            Spacer(minLength: 4)
            Text(servers.reach)
                .font(.ui(size: 11))
                .foregroundStyle(Color.themeMutedStrongerForeground)
                .monospacedDigit()
                .lineLimit(1)
        }
    }
}

struct StateDot: View {
    let tint: Color

    var body: some View {
        Circle()
            .fill(tint)
            .frame(width: 7, height: 7)
    }
}

extension [Server] {
    /// The worst state among the servers: one out of reach, then one connecting.
    var worstTint: Color {
        if contains(where: { $0.state == .disconnected || $0.state == .refused }) { return .themeDestructive }
        if contains(where: { $0.state == .connecting }) { return .themePending }
        return .themeSuccess
    }

    /// How the connected ones are reached, "Mixed" if not all alike, and their slowest round trip.
    var reach: String {
        let connected = filter { $0.state == .connected }
        let paths = Set(connected.compactMap(\.path))
        let path = paths.count > 1 ? "Mixed" : paths.first?.capitalized
        let slowest = connected.compactMap(\.rttMs).max().map { "\($0) ms" }
        return [path, slowest].compactMap { $0 }.joined(separator: " · ")
    }
}

/// A server and how the client reaches it.
struct ServerLine: View {
    let server: Server

    var body: some View {
        HStack(spacing: 7) {
            StateDot(tint: server.stateTint ?? .themeSuccess)
                .frame(width: 14)
            Text(server.name)
                .font(.ui(size: 12, weight: .medium))
                .lineLimit(1)
            Spacer(minLength: 4)
            ServerUpdateStatus(server: server) {
                Text(detail)
                    .font(.ui(size: 11))
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                    .monospacedDigit()
            }
        }
        .help(server.error ?? detail)
    }

    private var detail: String {
        switch server.state {
        case .connected:
            let path = server.path?.capitalized ?? "Connected"
            return server.rttMs.map { "\(path) · \($0) ms" } ?? path
        case .connecting: return "Connecting…"
        case .disconnected: return "Offline"
        case .refused: return "Refused"
        }
    }
}

#if os(macOS)
/// The ways to the settings and the usage (or back from them) and a new version of the client.
struct SidebarFooter: View {
    private static let reach = EdgeInsets(top: 4, leading: ToolbarButton.margin, bottom: 4, trailing: ToolbarButton.margin)

    @Environment(AppStore.self) private var store

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if ![.idle, .checking, .upToDate].contains(store.updater.state) {
                AppUpdateRow(updater: store.updater)
            }
            HStack(spacing: 0) {
                AccountMenu(margin: Self.reach)
                if store.showsUsage || store.settings != nil {
                    ActionButton(
                        "Back", icon: .arrowLeft, help: "Back to the threads (Esc)", variant: .ghost, fills: true, alignment: .leading,
                        margin: Self.reach
                    ) {
                        store.closeRoute()
                    }
                } else {
                    ActionButton(icon: .settings, help: "Settings (⌘,)", margin: Self.reach) { store.openSettings() }
                    ActionButton(icon: .chartColumn, help: "Usage: what the agents spent and what is left of their plans", margin: Self.reach) {
                        store.openUsage()
                    }
                }
                Spacer(minLength: 0)
                if store.updater.state == .upToDate {
                    UpdateLabel.upToDate(store.updater.current)
                }
                ActionButton(icon: .refreshCw, help: "Check for Updates", pending: store.updater.state == .checking, margin: Self.reach) {
                    store.updater.check(asked: true)
                }
            }
            .padding(.horizontal, -(ControlSize.regular.height - ControlSize.regular.symbol) / 2 - Self.reach.leading)
            .padding(.vertical, -Self.reach.top)
        }
        .padding(.horizontal, 18)
        .padding(.top, 10)
        .padding(.bottom, 11)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
#endif
