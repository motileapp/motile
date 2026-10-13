#if os(iOS)
import SwiftUI

/// The sidebar: the drafts, every active thread on every server, the ones marked done, the
/// servers and the account. On a phone it lies under the thread; in a wide window beside it.
struct SidebarScreen: View {
    @Environment(AppStore.self) private var store
    @Environment(Drawer.self) private var drawer
    /// The sidebar lies under the thread, which comes back over it when something is opened.
    var underThread = true
    @AppStorage("sidebar.doneExpanded") private var doneExpanded = false
    @AppStorage("sidebar.serversExpanded") private var serversExpanded = false
    @State private var renaming: ThreadInfo?
    @State private var newTitle = ""
    @State private var deleting: ThreadInfo?
    @FocusState private var searchFocused: Bool
    /// The row a swipe has slid aside, by its id in the list.
    @State private var swiped: String?
    /// Why a swipe couldn't mark a thread done: its agent is still at it.
    @State private var busy = ""
    @State private var showsBusy = false
    /// The search's result the arrow keys are on, which return opens.
    @State private var highlighted = Highlight()

    var body: some View {
        let searching = store.isSearchingThreads
        let active = searching ? [] : store.activeThreads
        let done = searching ? [] : store.doneThreads
        let results = store.searchResults
        let highlightedID = results.indices.contains(highlighted.index) ? results[highlighted.index].id : nil
        let shown = Shown(projects: store.projectsByID, selection: store.selection, swiped: swiped, highlighted: highlightedID)
        let items = searching ? searchItems(results) : items(active: active, done: done, expanded: doneExpanded)
        VStack(spacing: 0) {
            header
            RecycledList(
                items: items, bottomInset: 12,
                scrollTarget: searching
                    ? highlightedID.map { "result:\($0)" } : items.first { $0.thread?.id == store.revealedThreadID }?.id,
                scrollsTargetToTop: done.contains { $0.id == store.revealedThreadID },
                scrolled: { self.swiped = nil },
                rowInset: UIEdgeInsets(top: rowMargin.top, left: rowMargin.leading, bottom: rowMargin.bottom, right: rowMargin.trailing),
                rowRadius: 8,
                movable: { item in
                    guard !searching, active.count > 1, let thread = item.activeThread else { return false }
                    return store.moves(thread)
                },
                moved: { item, index in
                    guard let thread = item.activeThread else { return }
                    store.move(thread, to: index, among: items.map(\.activeThread))
                },
                menu: { item in
                    guard let thread = item.thread else { return [] }
                    return store.rowActions(for: thread, rename: beginRename, delete: { deleting = $0 })
                },
                pointed: { item, row, view in
                    if case .result(let result) = item, let index = results.firstIndex(where: { $0.id == result.id }) { highlighted.index = index }
                    ThreadPeek.shared.point(at: item?.thread, row: row, in: view, store: store)
                }
            ) { item in
                row(item, done: done, expanded: doneExpanded, shown: shown)
            }
            .onChange(of: store.revealedThreadID) { _, revealed in
                guard let revealed, store.doneThreads.contains(where: { $0.id == revealed }) else { return }
                doneExpanded = true
            }
            if let undo = store.undo {
                UndoRow(notice: undo)
                    .appearing()
            }
            footer(results)
        }
        .contentShape(Rectangle())
        .onTapGesture { Platform.endEditing() }
        .animation(.easeOut(duration: 0.15), value: store.undo)
        .onChange(of: store.threadsShown) {
            ThreadPeek.shared.hide()
            guard underThread else { return }
            drawer.isOpen = false
        }
        .alert(busy, isPresented: $showsBusy) {
            Button("OK", role: .cancel) {}
        } message: {
            Text("Stop it first, then mark it done.")
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
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            titleVisibility: .visible
        ) {
            Button("Delete", role: .destructive) {
                if let thread = deleting { store.delete(thread) }
                deleting = nil
            }
        } message: {
            Text(store.deletionNote(deleting))
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Text("Motile")
                .font(.system(size: 28, weight: .semibold))
                .foregroundStyle(Color.themeForeground)
            Spacer()
            Button {
                store.openPanel(.commands)
            } label: {
                Image(.command, size: 15)
                    .foregroundStyle(Color.themeForeground)
                    .frame(width: 42, height: 42)
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
            .glassButton(in: Circle())
            .accessibilityLabel("Commands")
        }
        .padding(.leading, sidebarEdge)
        .padding(.trailing, sidebarRowInset + 2)
        .padding(.top, 6)
        .padding(.bottom, 12)
    }

    /// The drafts, the active threads, and under them the threads marked done: a line that opens
    /// into their list.
    private func items(active: [ThreadInfo], done: [ThreadInfo], expanded: Bool) -> [SidebarItem] {
        var items: [SidebarItem] = [.drafts]
        items += active.map(SidebarItem.active)
        if active.isEmpty { items.append(.empty) }
        if store.offersNewThread(drafts: store.listedDrafts, active: active) {
            items.append(.newThread)
        }
        guard !done.isEmpty else { return items }
        items.append(.doneHeader)
        if expanded { items += done.map(SidebarItem.done) }
        return items
    }

    /// What the search found, or the line that says it found nothing.
    private func searchItems(_ results: [SearchResult]) -> [SidebarItem] {
        results.isEmpty ? [.empty] : results.map(SidebarItem.result)
    }

    /// What the rows show as the list was last drawn. They are drawn outside this view, so they
    /// only hear of a change when the list draws them again.
    private struct Shown {
        let projects: [String: Project]
        let selection: Selection
        let swiped: String?
        let highlighted: String?
    }

    @ViewBuilder
    private func row(_ item: SidebarItem, done: [ThreadInfo], expanded: Bool, shown: Shown) -> some View {
        let (projects, selection) = (shown.projects, shown.selection)
        switch item {
        case .drafts:
            DraftRows(swiped: { swipe("draft:\($0)", swiped: shown.swiped) }, open: open)
        case .active(let thread):
            ThreadRow(
                thread: thread, project: projects[thread.projectID]?.seen(from: thread),
                selected: selection == .thread(thread.id), rename: beginRename, delete: { deleting = $0 }, open: open
            )
            .equatable()
            .rowSwipe(
                .check, "Mark Done", tint: thread.busy ? .themeMutedForeground : .themeSuccess, size: 36,
                leaves: !thread.busy, isOpen: swipe(item.id, swiped: shown.swiped)
            ) {
                markDone(thread)
            }
        case .result(let result):
            SearchResultRow(result: result, highlighted: result.id == shown.highlighted, open: openSearchResult)
        case .empty:
            Group {
                if store.isSearchingThreads {
                    SearchEmptyLine(size: 13)
                } else {
                    Text(done.isEmpty ? "No threads yet" : "No active threads")
                        .font(.ui(size: 13))
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, sidebarEdge)
            .padding(.vertical, 10)
        case .newThread:
            NewThreadRow()
                .frame(height: 44)
        case .doneHeader:
            doneHeader(count: done.count, expanded: expanded)
        case .done(let thread):
            DoneRow(
                thread: thread, project: projects[thread.projectID], selected: selection == .thread(thread.id),
                rename: beginRename, delete: { deleting = $0 }, open: open
            )
            .equatable()
            .frame(height: 44)
            .rowSwipe(.undo2, "Mark Undone", tint: .themeMutedForeground, size: 28, isOpen: swipe(item.id, swiped: shown.swiped)) {
                store.setDone([thread.id], done: false)
            }
        }
    }

    private func doneHeader(count: Int, expanded: Bool) -> some View {
        Button {
            doneExpanded.toggle()
        } label: {
            HStack(spacing: 7) {
                SidebarChevron(expanded: expanded)
                ShelfTitle("Done", expanded: expanded)
                Spacer()
                Text("\(count)")
                    .font(.ui(size: 11))
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                    .monospacedDigit()
            }
            .padding(.horizontal, sidebarEdge)
            .frame(height: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.top, 8)
    }

    /// The servers and how the client reaches them, and under them the account, which leads to the
    /// settings and the usage, the search and the way to a new thread. While it searches, the
    /// search takes the line with a button that closes it, as the system's search does.
    private func footer(_ results: [SearchResult]) -> some View {
        @Bindable var store = store
        let searching = searchFocused || !store.threadSearch.isEmpty
        return VStack(alignment: .leading, spacing: 0) {
            if !store.servers.isEmpty {
                servers
                ThemeDivider()
            }
            HStack(spacing: 8) {
                if !searching {
                    AccountMenu()
                        .appearing(.scale.combined(with: .opacity))
                }
                SearchField(text: $store.threadSearch, bare: true, clearable: false, focus: $searchFocused)
                    .steersSearch(highlighted: highlighted, open: openSearchResult)
                    .padding(.horizontal, 6)
                    .frame(height: 46)
                    .glassButton(in: Capsule())
                if searching {
                    circleButton(.x, label: "Close search") {
                        store.threadSearch = ""
                        searchFocused = false
                    }
                    .appearing(.scale.combined(with: .opacity))
                } else {
                    circleButton(.squarePen, label: "New thread") { store.newThread() }
                        .disabled(store.projects.isEmpty && store.noProjects.isEmpty)
                        .appearing(.scale.combined(with: .opacity))
                }
            }
            .glassGroup(spacing: 8)
            .animation(.smooth(duration: 0.3), value: searching)
            .padding(.horizontal, sidebarEdge)
            .padding(.top, 16)
        }
        .padding(.bottom, 8)
        .background(alignment: .top) {
            Rectangle()
                .fill(Color.themeBorder)
                .frame(height: 1)
        }
    }

    /// One line for every server that opens into a line for each, or the one server's line.
    @ViewBuilder
    private var servers: some View {
        let servers = store.servers
        if servers.count == 1 {
            ServerLine(server: servers[0])
                .padding(.horizontal, sidebarEdge)
                .frame(height: ServerLines.lineHeight)
        } else if servers.count > 1 {
            Button {
                // Out of the tap's update, which the button animates.
                DispatchQueue.main.async { serversExpanded.toggle() }
            } label: {
                HStack(spacing: 7) {
                    SidebarChevron(expanded: serversExpanded)
                    AllServersLine(servers: servers, expanded: serversExpanded)
                }
                .padding(.horizontal, sidebarEdge)
                .frame(height: ServerLines.lineHeight)
            }
            .buttonStyle(.highlight(radius: 0))
            if serversExpanded {
                ServerLines(servers: servers)
            }
        }
    }

    private func circleButton(_ symbol: Symbol, label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(symbol, size: 15)
                .foregroundStyle(Color.themeForeground)
                .frame(width: 46, height: 46)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .glassButton(in: Circle())
        .accessibilityLabel(label)
    }

    /// Opens a thread or a draft, and puts the sidebar away where it lies under the thread.
    private func open(_ selection: Selection) {
        store.select(selection)
        showThread()
    }

    /// Opens what the search found, ends the search and puts the sidebar away.
    private func openSearchResult(_ result: SearchResult) {
        searchFocused = false
        store.openSearchResult(result)
        showThread()
    }

    private func showThread() {
        guard underThread else { return }
        store.rightPanel.isOpen = false
        drawer.isOpen = false
    }

    private func markDone(_ thread: ThreadInfo) {
        guard !thread.busy else {
            busy = thread.running ? "Still working" : "Still monitoring"
            showsBusy = true
            return
        }
        store.setDone([thread.id], done: true, fromSidebar: true)
    }

    private func swipe(_ row: String, swiped: String?) -> Binding<Bool> {
        Binding(get: { swiped == row }, set: { open in
            if open { self.swiped = row } else if self.swiped == row { self.swiped = nil }
        })
    }

    private func beginRename(_ thread: ThreadInfo) {
        newTitle = thread.title
        renaming = thread
    }
}

/// A line for each server, under the line of them all. A long list scrolls, and rests between two lines.
private struct ServerLines: View {
    static let lineHeight: CGFloat = 44
    private static let mostShown = 5

    let servers: [Server]

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                ForEach(servers) {
                    ServerLine(server: $0)
                        .padding(.horizontal, sidebarEdge)
                        .frame(height: Self.lineHeight)
                }
            }
            .scrollTargetLayout()
        }
        .scrollTargetBehavior(.viewAligned)
        .scrollBounceBehavior(.basedOnSize)
        .fadesHiddenEdges()
        .frame(height: CGFloat(min(servers.count, Self.mostShown)) * Self.lineHeight)
    }
}

private enum SidebarItem: Identifiable {
    case drafts
    case active(ThreadInfo)
    case result(SearchResult)
    case empty
    case newThread
    case doneHeader
    case done(ThreadInfo)

    var id: String {
        switch self {
        case .drafts: "drafts"
        case .active(let thread): thread.id
        case .result(let result): "result:\(result.id)"
        case .empty: "empty"
        case .newThread: "newThread"
        case .doneHeader: "done"
        // A done thread's row has an id of its own, or the list keeps its active row for it.
        case .done(let thread): "done:\(thread.id)"
        }
    }

    var activeThread: ThreadInfo? {
        guard case .active(let thread) = self else { return nil }
        return thread
    }

    var thread: ThreadInfo? {
        switch self {
        case .active(let thread), .done(let thread): thread
        case .result(let result): result.thread
        default: nil
        }
    }
}
#endif
