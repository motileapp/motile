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
    @State private var renaming: ThreadInfo?
    @State private var newTitle = ""
    @State private var deleting: ThreadInfo?
    @State private var search = ""
    /// The row a swipe has slid aside, by its id in the list.
    @State private var swiped: String?
    /// Why a swipe couldn't mark a thread done: its agent is still at it.
    @State private var busy = ""
    @State private var showsBusy = false

    var body: some View {
        let active = store.searched(store.activeThreads, for: search)
        let done = store.searched(store.doneThreads, for: search)
        let expanded = !search.isEmpty || doneExpanded
        let shown = Shown(projects: store.projectsByID, selection: store.selection, swiped: swiped)
        let items = items(active: active, done: done, expanded: expanded)
        VStack(spacing: 0) {
            header
            RecycledList(
                items: items, bottomInset: 12, scrollTarget: store.settledThreadID.map(SidebarItem.doneID), scrolled: { self.swiped = nil },
                rowInset: UIEdgeInsets(top: rowMargin.top, left: rowMargin.leading, bottom: rowMargin.bottom, right: rowMargin.trailing),
                rowRadius: 8,
                movable: { item in
                    guard search.isEmpty, active.count > 1, let thread = item.activeThread else { return false }
                    return store.moves(thread)
                },
                moved: { item, index in
                    guard let thread = item.activeThread else { return }
                    store.move(thread, to: index, among: items.map(\.activeThread))
                },
                menu: { item in
                    guard let thread = item.thread else { return [] }
                    return store.rowActions(for: thread, rename: beginRename, delete: { deleting = $0 })
                }
            ) { item in
                row(item, done: done, expanded: expanded, shown: shown)
            }
            .onChange(of: store.settledThreadID) { _, settled in
                guard settled != nil else { return }
                doneExpanded = true
            }
            if let undo = store.undo {
                UndoRow(notice: undo)
                    .appearing()
            }
            footer
        }
        .contentShape(Rectangle())
        .onTapGesture { Platform.endEditing() }
        .animation(.easeOut(duration: 0.15), value: store.undo)
        .onChange(of: store.threadsShown) {
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
        .padding(.leading, sidebarRowInset + 8)
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
        if store.offersNewThread(drafts: store.searched(store.listedDrafts, for: search), active: active, search: search) {
            items.append(.newThread)
        }
        guard !done.isEmpty else { return items }
        items.append(.doneHeader)
        if expanded { items += done.map(SidebarItem.done) }
        return items
    }

    /// What the rows show as the list was last drawn. They are drawn outside this view, so they
    /// only hear of a change when the list draws them again.
    private struct Shown {
        let projects: [String: Project]
        let selection: Selection
        let swiped: String?
    }

    @ViewBuilder
    private func row(_ item: SidebarItem, done: [ThreadInfo], expanded: Bool, shown: Shown) -> some View {
        let (projects, selection) = (shown.projects, shown.selection)
        switch item {
        case .drafts:
            DraftRows(search: search, swiped: { swipe("draft:\($0)", swiped: shown.swiped) }, open: open)
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
        case .empty:
            Text(!search.isEmpty ? "No threads found" : done.isEmpty ? "No threads yet" : "No active threads")
                .font(.ui(size: 13))
                .foregroundStyle(Color.themeMutedMoreForeground)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, sidebarRowInset + 8)
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
                Image(.chevronRight, size: 10)
                    .rotationEffect(.degrees(expanded ? 90 : 0))
                    .frame(width: 14)
                Text("Done")
                    .font(.ui(size: 12, weight: .medium))
                Spacer()
                Text("\(count)")
                    .font(.ui(size: 11))
                    .foregroundStyle(Color.themeMutedMoreForeground)
                    .monospacedDigit()
            }
            .foregroundStyle(Color.themeMutedForeground)
            .padding(.horizontal, sidebarRowInset + 8)
            .frame(height: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.top, 8)
    }

    /// The servers and how the client reaches them, and under them the account, which leads to the
    /// settings and the usage, the search and the way to a new thread.
    private var footer: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(store.servers) { ServerLine(server: $0) }
            HStack(spacing: 8) {
                AccountMenu()
                SearchField(text: $search, bare: true)
                    .padding(.horizontal, 6)
                    .frame(height: 46)
                    .glassButton(in: Capsule())
                circleButton(.squarePen, label: "New thread") { store.newThread() }
                    .disabled(store.projects.isEmpty && store.noProjects.isEmpty)
            }
            .padding(.top, 6)
        }
        .padding(.horizontal, sidebarRowInset + 8)
        .padding(.top, 10)
        .padding(.bottom, 8)
        .background(alignment: .top) {
            Rectangle()
                .fill(Color.themeBorder)
                .frame(height: 1)
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

    private func showThread() {
        guard underThread else { return }
        store.sidePanel.isOpen = false
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

private enum SidebarItem: Identifiable {
    case drafts
    case active(ThreadInfo)
    case empty
    case newThread
    case doneHeader
    case done(ThreadInfo)

    /// A done thread's row has an id of its own, or the list keeps its active row for it.
    static func doneID(_ threadID: String) -> String { "done:\(threadID)" }

    var id: String {
        switch self {
        case .drafts: "drafts"
        case .active(let thread): thread.id
        case .empty: "empty"
        case .newThread: "newThread"
        case .doneHeader: "done"
        case .done(let thread): Self.doneID(thread.id)
        }
    }

    var activeThread: ThreadInfo? {
        guard case .active(let thread) = self else { return nil }
        return thread
    }

    var thread: ThreadInfo? {
        switch self {
        case .active(let thread), .done(let thread): thread
        default: nil
        }
    }
}
#endif
