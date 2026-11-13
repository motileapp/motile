import SwiftUI

/// The space between the rows' highlights, and around each one: it looks empty but is the row's.
private let rowGap = 2.0
/// How far the rows' highlights stay from the sidebar's edges.
#if os(macOS)
let sidebarRowInset: CGFloat = 8
#else
let sidebarRowInset: CGFloat = 10
#endif
let rowMargin = EdgeInsets(top: rowGap / 2, leading: sidebarRowInset, bottom: rowGap / 2, trailing: sidebarRowInset)
/// How far a row's content stays from its highlight's sides.
let sidebarRowPadding: CGFloat = 8
/// How far everything the sidebar shows stays from its sides, a row's content as well as a line's.
let sidebarEdge = sidebarRowInset + sidebarRowPadding
let doneRowHeight = Double(ControlSize.regular.height)
/// How far an Update button on a server's line reaches past the rows' edge, to look in line with it.
let serverButtonOutset: CGFloat = 4
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
/// The drafts, then every active thread on every server in one list, with the working, the
/// snoozed and the archived ones on shelves at the bottom.
struct SidebarView: View {
    @Environment(AppStore.self) private var store
    @AppStorage("sidebar.doneExpanded") private var archivedExpanded = false
    @AppStorage("sidebar.workingExpanded") private var workingExpanded = true
    @AppStorage("sidebar.snoozedExpanded") private var snoozedExpanded = false
    /// The list's height as last laid out, which the gap over the sections is worked out from.
    @State private var listHeight: CGFloat = 0
    @State private var renaming: ThreadInfo?
    @State private var deleting: ThreadInfo?
    /// The search's result the arrow keys are on.
    @State private var highlighted = Highlight()
    @FocusState private var searchFocused: Bool

    var body: some View {
        @Bindable var store = store
        let results = store.searchResults
        let searching = store.isSearchingThreads
        let lists = SidebarLists(
            active: searching ? [] : store.activeThreads, working: searching ? [] : store.workingThreads,
            starting: searching ? [] : store.startingDrafts,
            snoozed: searching ? [] : store.snoozedThreads, archived: searching ? [] : store.archivedThreads
        )
        let projects = store.projectsByID
        let selection = store.selection
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                SearchField(text: $store.threadSearch, focus: $searchFocused)
                    .steersSearch(highlighted: highlighted, open: store.openSearchResult)
                ProjectButtons()
            }
            .padding(.horizontal, sidebarRowInset)
                .padding(.top, 2)
                .padding(.bottom, 6)
            GeometryReader { list in
                threads(lists: lists, results: results, projects: projects, selection: selection, sidebarHeight: list.size.height)
            }
        }
        .onChange(of: store.searchFocusRequests) {
            UserDefaults.standard.set(false, forKey: MainView.sidebarHiddenKey)
            FieldFocus.take { searchFocused = true }
        }
        .onChange(of: store.revealedThreadID) { _, revealed in
            guard let revealed, let section = store.shelf(of: revealed) else { return }
            expanded(section).wrappedValue = true
        }
        .modal(item: $renaming) { thread in
            RenameThread(thread: thread)
        }
        .asking($deleting) { thread in
            Confirmation(
                title: "Delete “\(thread.title)”?", message: store.deletionNote(thread),
                answers: [Answer(title: "Delete", variant: .danger) { store.delete(thread) }]
            )
        }
    }

    private func threads(
        lists: SidebarLists, results: [SearchResult], projects: [String: Project], selection: Selection, sidebarHeight: Double
    ) -> some View {
        let searching = store.isSearchingThreads
        let drafts = searching ? [] : store.listedDrafts
        let active = lists.active
        let jumps = store.jumpThreads
        let places = store.showsJumpHints ? Dictionary(jumps.enumerated().map { ($1.id, $0) }, uniquingKeysWith: { first, _ in first }) : [:]
        return VStack(spacing: 0) {
            let items = searching ? searchItems(results) : items(drafts: drafts, lists: lists)
            // A table, which picks its rows up and moves them itself.
            RecycledList(
                items: items, height: { height(of: $0, drafts: drafts.count) }, topInset: 4 - rowGap / 2,
                // The sections at the bottom stand a little apart from the line under them.
                bottomInset: 4 - rowGap / 2 + (items.contains { $0.isHeader } ? 2 : 0),
                scrollTarget: searching
                    ? highlighted.shown.flatMap { results.indices.contains($0) ? SidebarItem.result(results[$0]).id : nil } : store.revealedThreadID,
                scrollsTargetToTop: !searching && store.revealedThreadID != nil,
                scrollAnchor: revealedSection(lists).map { SidebarItem.header($0, 0).id },
                clicked: { item in
                    switch item {
                    case .active(let thread), .shelved(let thread): store.select(.thread(thread.id))
                    case .starting(let listed): store.select(.draft(listed.id))
                    case .result(let result): store.openSearchResult(result)
                    case .header(let section, _): expanded(section).wrappedValue.toggle()
                    default: break
                    }
                },
                rowInset: NSEdgeInsets(top: rowMargin.top, left: rowMargin.leading, bottom: rowMargin.bottom, right: rowMargin.trailing),
                rowRadius: 8,
                movable: { item in
                    guard !searching, active.count > 1, case .active(let thread) = item else { return false }
                    return store.moves(thread)
                },
                moved: { item, index in
                    guard case .active(let thread) = item else { return }
                    store.move(thread, to: index, among: items.map(\.activeThread))
                },
                pointed: { item, row, view in
                    if case .result(let result) = item, let index = results.firstIndex(where: { $0.id == result.id }) { highlighted.point(index) }
                    ThreadPeek.shared.point(at: item?.thread, row: row, in: view, store: store)
                }
            ) { item in
                switch item {
                case .drafts:
                    VStack(spacing: 0) {
                        DraftRows { store.select($0) }
                    }
                case .active(let thread):
                    ThreadRow(
                        thread: thread, project: projects[thread.projectID]?.seen(from: thread),
                        selected: selection == .thread(thread.id), rename: beginRename, delete: { deleting = $0 },
                        jump: places[thread.id].flatMap(store.jumpHint), open: { store.select($0) }
                    )
                    .equatable()
                case .result(let result):
                    let index = results.firstIndex { $0.id == result.id } ?? 0
                    SearchResultRow(result: result, highlighted: index == highlighted.index, jump: places[result.id].flatMap(store.jumpHint))
                    .contextMenu {
                        if let thread = result.thread { ThreadMenu(thread: thread, rename: beginRename, delete: { deleting = $0 }) }
                    }
                case .empty:
                    Group {
                        if searching {
                            SearchEmptyLine(size: .xs)
                        } else {
                            Text(store.allThreads.isEmpty ? "No threads yet" : "No active threads")
                                .font(.ui(.xs))
                                .foregroundStyle(Color.themeMutedStrongerForeground)
                        }
                    }
                    .frame(height: Self.emptyLineHeight)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, sidebarEdge)
                    .padding(.vertical, 6 + rowGap / 2)
                case .newThread:
                    NewThreadRow()
                case .gap(let height):
                    if height <= ListDivider.closest {
                        ListDivider()
                            .frame(maxHeight: .infinity, alignment: .bottom)
                    } else {
                        Color.clear
                    }
                case .sectionEnd:
                    ListDivider()
                case .header(let section, let count):
                    SectionHeaderRow(section: section, count: count, expanded: expanded(section).wrappedValue)
                case .shelved(let thread):
                    SlimThreadRow(
                        thread: thread, project: projects[thread.projectID], selected: selection == .thread(thread.id),
                        rename: beginRename, delete: { deleting = $0 }
                    ) { store.select($0) }
                    .equatable()
                case .starting(let listed):
                    StartingRow(listed: listed) { store.select($0) }
                }
            }
            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { listHeight = $0 }
            if let undo = store.undo {
                UndoRow(notice: undo)
                    .appearing()
            }
            if !store.servers.isEmpty {
                ServersShelf(maxHeight: sidebarHeight * 0.25)
            }
            if !store.servers.isEmpty {
                ThemeDivider()
            }
            SidebarFooter()
        }
        .clipped()
        .animation(.easeOut(duration: 0.15), value: store.undo)
    }

    private static let emptyLineHeight = scaled(15)

    /// The drafts as one row and the active threads, then the working, the snoozed and the
    /// archived ones, each under its line. A gap holds those at the bottom while everything fits.
    private func items(drafts: [ListedDraft], lists: SidebarLists) -> [SidebarItem] {
        var items: [SidebarItem] = drafts.isEmpty ? [] : [.drafts]
        items += lists.active.map(SidebarItem.active)
        if lists.active.isEmpty { items.append(.empty) }
        if store.offersNewThread(drafts: drafts, active: lists.active) { items.append(.newThread) }
        var shelved: [SidebarItem] = []
        let shelves = lists.shelves
        for (index, (section, threads)) in shelves.enumerated() {
            let starting = section == .working ? lists.starting : []
            shelved.append(.header(section, starting.count + threads.count))
            guard expanded(section).wrappedValue else { continue }
            shelved += starting.map(SidebarItem.starting) + threads.map(SidebarItem.shelved)
            if index < shelves.count - 1 { shelved.append(.sectionEnd(section)) }
        }
        guard !shelved.isEmpty else { return items }
        let insets = 4 - rowGap / 2 + 4 - rowGap / 2 + 2
        let used = (items + shelved).reduce(insets) { $0 + height(of: $1, drafts: drafts.count) }
        return items + [.gap(max(ListDivider.height, (listHeight - used).rounded()))] + shelved
    }

    /// The shelf the thread the sidebar reveals is on.
    private func revealedSection(_ lists: SidebarLists) -> ThreadSection? {
        guard let revealed = store.revealedThreadID else { return nil }
        if lists.starting.contains(where: { $0.id == revealed }) { return .working }
        return lists.shelves.first { $0.threads.contains { $0.id == revealed } }?.section
    }

    private func expanded(_ section: ThreadSection) -> Binding<Bool> {
        switch section {
        case .working: $workingExpanded
        case .snoozed: $snoozedExpanded
        default: $archivedExpanded
        }
    }

    /// What the search found, then the line that says the servers still search, or that line
    /// alone when it found nothing.
    private func searchItems(_ results: [SearchResult]) -> [SidebarItem] {
        guard !results.isEmpty else { return [.empty] }
        return results.map(SidebarItem.result) + (store.messageSearch.searching ? [.empty] : [])
    }

    private func height(of item: SidebarItem, drafts: Int) -> CGFloat {
        switch item {
        case .drafts: CGFloat(drafts) * (DraftRow.height + rowGap) + ListDivider.height
        case .active: ThreadRow.height + rowGap
        case .gap(let height): height
        case .header: SectionHeaderRow.height
        case .sectionEnd: ListDivider.height
        case .shelved, .starting: doneRowHeight + rowGap
        case .result: SearchResultRow.height + rowGap
        case .empty: Self.emptyLineHeight + 2 * (6 + rowGap / 2)
        case .newThread: doneRowHeight + rowGap
        }
    }

    private func beginRename(_ thread: ThreadInfo) {
        renaming = thread
    }
}

private struct RenameThread: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    let thread: ThreadInfo
    @State private var title: String

    init(thread: ThreadInfo) {
        self.thread = thread
        _title = State(initialValue: thread.title)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Rename Thread")
                .font(.ui(.lg, weight: .semibold))
            InputField("Title", text: $title)
            HStack(spacing: 0) {
                Spacer()
                ActionButton("Cancel", margin: .beside(trailing: 4)) { close() }
                ActionButton("Rename", variant: .primary, margin: .beside(leading: 4)) {
                    store.rename(thread, to: title)
                    close()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(title.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(20)
        .frame(width: 400)
    }
}

/// A section's line in the list, which opens and closes it. It is laid out as a row is: the gap
/// around it is its own.
private struct SectionHeaderRow: View {
    let section: ThreadSection
    let count: Int
    let expanded: Bool
    @State private var hovering = false

    static let height = doneRowHeight + rowGap

    var body: some View {
        SectionLine(section: section, count: count, expanded: expanded, lit: hovering)
            .padding(.horizontal, sidebarEdge - rowMargin.leading)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .padding(rowMargin)
            .contentShape(Rectangle())
            .hoverHighlight(radius: Radius.md, inset: rowMargin, hovered: hovering, opaque: true)
            .onHover { hovering = $0 }
    }
}

private struct SidebarLists {
    let active: [ThreadInfo]
    let working: [ThreadInfo]
    /// The drafts being sent, which the working shelf lists above its threads.
    let starting: [ListedDraft]
    let snoozed: [ThreadInfo]
    let archived: [ThreadInfo]

    /// The parts on shelves at the bottom, the ones that hold threads or starting drafts.
    var shelves: [(section: ThreadSection, threads: [ThreadInfo])] {
        [(ThreadSection.working, working), (.snoozed, snoozed), (.archived, archived)]
            .filter { !$0.1.isEmpty || ($0.0 == .working && !starting.isEmpty) }
    }
}

private enum SidebarItem: Identifiable {
    case drafts
    case active(ThreadInfo)
    case result(SearchResult)
    case empty
    case newThread
    /// The room that holds the sections at the bottom of the list.
    case gap(CGFloat)
    case header(ThreadSection, Int)
    case shelved(ThreadInfo)
    /// A draft being sent, whose thread takes its row once it is there.
    case starting(ListedDraft)
    /// The line under an open section that has another below it.
    case sectionEnd(ThreadSection)

    var id: String {
        switch self {
        case .drafts: "drafts"
        case .active(let thread), .shelved(let thread): thread.id
        case .starting(let listed): listed.id
        case .result(let result): "result:\(result.id)"
        case .empty: "empty"
        case .newThread: "newThread"
        case .gap: "gap"
        case .header(let section, _): "header-\(section.rawValue)"
        case .sectionEnd(let section): "end-\(section.rawValue)"
        }
    }

    var thread: ThreadInfo? {
        switch self {
        case .active(let thread), .shelved(let thread): thread
        default: nil
        }
    }

    var isHeader: Bool {
        guard case .header = self else { return false }
        return true
    }

    var activeThread: ThreadInfo? {
        guard case .active(let thread) = self else { return nil }
        return thread
    }
}
#endif

/// Whether a thread's title, project or pull request matches what is being searched for.
extension AppStore {
    func matches(_ thread: ThreadInfo, search: String) -> Bool {
        guard !search.isEmpty else { return true }
        let project = project(thread.projectID)
        let pullRequest = project?.pullRequest(of: thread)
        let names = [thread.title, project?.name, pullRequest.map { "#\($0.number)" }, pullRequest?.title, pullRequest?.url]
        return names.contains { $0?.localizedCaseInsensitiveContains(search) == true }
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
            ActionButton(
                icon: .squarePen, help: "\(store.shortcuts.help("New thread", "chat.new")). ⇧-click starts one in this project"
            ) {
                guard NSApp.currentEvent?.modifierFlags.contains(.shift) == true else { return store.newThread() }
                store.startNewThread(in: store.composerProject)
            }
        }
    }
}
#endif

struct SearchField: View {
    @Binding var text: String
    var prompt = "Search"
    /// Without a background or a height of its own, for a field that lies on a glass capsule and
    /// reaches its trailing end.
    var capsule = false
    var clearable = true
    var focus: FocusState<Bool>.Binding?

    var body: some View {
        InputField(
            prompt, text: $text, icon: .search, variant: capsule ? .capsule : .filled, size: Platform.scale > 1 ? .large : .regular, clearable: clearable,
            focus: focus
        )
    }
}

/// Something a row's menu offers.
struct RowAction {
    let title: String
    var destructive = false
    var disabled = false
    /// A submenu's groups, in place of `perform`.
    var children: [[RowAction]] = []
    var perform: () -> Void = {}
}

extension AppStore {
    /// What a thread's menu offers, in groups a line divides.
    func rowActions(for thread: ThreadInfo, rename: @escaping (ThreadInfo) -> Void, delete: @escaping (ThreadInfo) -> Void) -> [[RowAction]] {
        guard !onRemovedServer(thread) else { return [] }
        let archive = thread.isArchived
            ? RowAction(title: "Unarchive") { self.setArchived([thread.id], archived: false) }
            : RowAction(title: "Archive", disabled: thread.unfinished) { self.setArchived([thread.id], archived: true, fromSidebar: true) }
        let snooze = snoozeAction(for: thread).map { [$0] } ?? []
        return [[archive] + snooze + [RowAction(title: "Rename") { rename(thread) }], [RowAction(title: "Delete", destructive: true) { delete(thread) }]]
    }

    /// Wakes a snoozed thread, or snoozes one until a time the presets offer or the user picks.
    func snoozeAction(for thread: ThreadInfo) -> RowAction? {
        if thread.section == .snoozed { return RowAction(title: "Wake") { self.wake([thread.id]) } }
        guard snoozes(thread) else { return nil }
        let presets = Snooze.presets.map { preset in
            RowAction(title: preset.title) { self.snooze([thread.id], until: preset.until, fromSidebar: true) }
        }
        let custom = RowAction(title: "Custom…") { self.customSnooze = CustomSnooze(threadIDs: [thread.id], fromSidebar: true) }
        return RowAction(title: "Snooze", children: [presets, [custom]])
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
                if action.children.isEmpty {
                    Button(action.title, role: action.destructive ? .destructive : nil, action: action.perform)
                        .disabled(action.disabled)
                } else {
                    Menu(action.title) {
                        ForEach(action.children.indices, id: \.self) { index in
                            if index > 0 { Divider() }
                            ForEach(action.children[index], id: \.title) { Button($0.title, action: $0.perform) }
                        }
                    }
                }
            }
        }
    }
}

#if os(macOS)
/// What a thread's row offers under the pointer: a snooze, and archiving it once it has finished.
private struct RowButtons: View {
    @Environment(AppStore.self) private var store
    let thread: ThreadInfo

    private static let gap: CGFloat = 2

    var body: some View {
        let snoozes = store.snoozes(thread)
        let count = (snoozes ? 1 : 0) + (thread.unfinished ? 0 : 1)
        HStack(spacing: 0) {
            if snoozes {
                ActionMenu(
                    icon: .alarmClock, help: "Snooze", size: .small, symbolSize: ControlSize.small.smallSymbol,
                    margin: .sharing(Self.gap, at: 0, of: count)
                ) {
                    SnoozeItems(store: store, threadIDs: [thread.id], fromSidebar: true)
                }
            }
            if !thread.unfinished {
                ActionButton(
                    "Archive", icon: .check, variant: .ghost, size: .small, symbolSize: ControlSize.small.smallSymbol,
                    margin: .sharing(Self.gap, at: count - 1, of: count)
                ) { store.setArchived([thread.id], archived: true, fromSidebar: true) }
                    .fixedSize()
            }
        }
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
    /// The keys that open it, while they are held.
    var jump: String?
    /// Opens what the row stands for. The sidebar it is in may have more to do then.
    var open: (Selection) -> Void = { _ in }
    @State private var hovering = false

    static let sidePadding = sidebarRowPadding
    static let topPadding: CGFloat = 5
    static let bottomPadding: CGFloat = 7
    static let titleHeight = scaled(16)
    /// A button on the first line is as far from the row's side as from its top.
    static let buttonInset = topPadding + (scaled(22) - ControlSize.small.height) / 2
    /// How tall the row is, without the gap around it.
    static let height = topPadding + scaled(22) + 1 + titleHeight + 5 + scaled(16) + bottomPadding

    static func == (one: ThreadRow, other: ThreadRow) -> Bool {
        one.thread == other.thread && one.project == other.project && one.selected == other.selected && one.jump == other.jump
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                ProjectIcon(project: project, size: .sm)
                Text(project?.name ?? URL(fileURLWithPath: thread.cwd).lastPathComponent)
                    .font(.ui(.xs, weight: .medium))
                    .lineLimit(1)
                    .layoutPriority(1)
                Spacer(minLength: 6)
                #if os(macOS)
                if let jump {
                    KeyCaps([jump])
                        .foregroundStyle(Color.themeForeground)
                        .padding(.trailing, Self.buttonInset - Self.sidePadding)
                } else if hovering && (!thread.unfinished || store.snoozes(thread)) && !store.onRemovedServer(thread) {
                    RowButtons(thread: thread)
                        .padding(.trailing, Self.buttonInset - Self.sidePadding)
                } else {
                    ThreadStatus(thread: thread, readOnly: store.onRemovedServer(thread))
                }
                #else
                ThreadStatus(thread: thread, readOnly: store.onRemovedServer(thread))
                #endif
            }
            .foregroundStyle(Color.themeMutedForeground)
            .frame(height: scaled(22))

            Text(thread.title)
                .font(.ui(.sm, weight: .medium))
                .lineLimit(1)
                .rowText()
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
                HStack(spacing: ThreadStatus.gap) {
                    if !store.onRemovedServer(thread), let pullRequest = project?.pullRequest(of: thread) {
                        ThreadPullRequestLabel(thread: thread, pullRequest: pullRequest, open: open)
                    }
                    ThreadServerLabel(serverID: thread.serverID)
                    AgentIcon(agent: thread.agent, size: .sm)
                }
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
        .hoverHighlight(radius: Radius.md, selected: selected, inset: rowMargin, hovered: hovering, opaque: true)
        #else
        // The list the row is in brings up its menu, so that a hold can drag the row as well.
        .button(.highlight(radius: Radius.md, selected: selected, inset: rowMargin, hovered: hovering, opaque: true)) { open(.thread(thread.id)) }
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
            Image(symbol, size: .sm)
            Text(text)
                .font(.ui(.xs))
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
        if store.servers.count + store.removedServers.count > 1, let server = store.server(serverID) {
            ServerLabel(server: server)
        }
    }
}

/// The threads that haven't been sent yet, above the others. They are their own view so that
/// typing only redraws them.
struct DraftRows: View {
    @Environment(AppStore.self) private var store
    /// Whether a draft's row is slid aside to show its discard button, by the draft's id.
    var swiped: (String) -> Binding<Bool> = { _ in .constant(false) }
    var open: (Selection) -> Void = { _ in }

    var body: some View {
        let listed = store.listedDrafts
        if !listed.isEmpty {
            ForEach(listed) { listed in
                #if os(iOS)
                if listed.sending {
                    DraftRow(listed: listed, open: open)
                } else {
                    DraftRow(listed: listed, open: open)
                        .rowSwipe(.trash2, "Discard Draft", tint: .themeDestructive, isOpen: swiped(listed.id)) {
                            store.discard(listed.draft)
                        }
                }
                #else
                DraftRow(listed: listed, open: open)
                #endif
            }
            ListDivider()
        }
    }
}

/// The line that sets a group of the list's rows off from the rows under it: the drafts, or an
/// open section.
/// The line under the drafts or an open section, and over the sections while they come within
/// `closest` of the rows above them.
struct ListDivider: View {
    static let height = 1 + 2 * (4 + rowGap / 2)
    static let closest: CGFloat = 80

    var body: some View {
        ThemeDivider(dashed: true)
            .padding(.horizontal, sidebarEdge)
            .padding(.vertical, 4 + rowGap / 2)
    }
}

/// A draft, laid out like a thread's row: its project and "Draft", or "Sending" while its first
/// message is on its way, on the first line, what was written in it on the second, where it will
/// work, its server and its agent on the third.
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
                ProjectIcon(project: project, size: .sm)
                Text(project?.name ?? "No project")
                    .font(.ui(.xs, weight: .medium))
                    .lineLimit(1)
                    .layoutPriority(1)
                Spacer(minLength: 6)
                if listed.sending {
                    HStack(spacing: 3) {
                        Spinner(size: .sm)
                        Text("Sending")
                            .font(.ui(.xs, weight: .medium))
                    }
                } else if hovering {
                    ActionButton(icon: .x, help: "Discard draft", size: .small) { store.discard(listed.draft) }
                        .padding(.trailing, ThreadRow.buttonInset - ThreadRow.sidePadding)
                } else {
                    HStack(spacing: 3) {
                        Image(.file, size: .sm)
                        Text("Draft")
                            .font(.ui(.xs, weight: .medium))
                    }
                }
            }
            .foregroundStyle(Color.themeMutedForeground)
            .frame(height: scaled(22))

            Text(listed.preview)
                .font(.ui(.sm, weight: .medium))
                .lineLimit(1)
                .rowText()
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
                HStack(spacing: ThreadStatus.gap) {
                    if let project {
                        ThreadServerLabel(serverID: project.serverID)
                    }
                    if let agent = store.agent(of: listed.draft) {
                        AgentIcon(agent: agent, size: .sm)
                    }
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
        .button(.highlight(radius: Radius.md, selected: selected, inset: rowMargin, hovered: hovering, opaque: true)) {
            open(.draft(listed.id))
        }
        .onHover { hovering = $0 }
        .contextMenu {
            if !listed.sending {
                Button("Discard Draft", role: .destructive) { store.discard(listed.draft) }
            }
        }
    }
}

/// The offer to undo archiving or snoozing threads: a line above the archived threads.
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
                    ShelfLineMark { Image(.undo2, size: .sm) }
                        .modifier(ShelfTint(expanded: false))
                    ShelfTitle("Undo", expanded: false)
                    Spacer()
                    Text(notice.text)
                        .font(.ui(.xs))
                        .lineLimit(1)
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                }
                .padding(.horizontal, sidebarEdge)
                .frame(height: doneRowHeight + 8)
            }
            .buttonStyle(.highlight(radius: 0))
        }
        .onHover { store.holdUndo($0) }
        .onDisappear { store.holdUndo(false) }
    }
}

#if os(macOS)
/// A shelf at the bottom of the sidebar: a line that opens into a list, which the line above it
/// makes taller or shorter.
private struct Shelf<Item: Identifiable, Header: View, Row: View>: View {
    private static var rowHeight: Double { doneRowHeight }
    /// The room over the list's first row, under the shelf's line.
    private static var listTop: Double { 2 }
    private static var minHeight: Double { 4 * (rowHeight + rowGap) + 4 + listTop }

    let items: [Item]
    let maxHeight: Double
    @Binding var expanded: Bool
    @Binding var height: Double
    var scrollTarget: Item.ID?
    var pointed: (Item?, CGRect, NSView) -> Void = { _, _, _ in }
    @ViewBuilder let header: () -> Header
    @ViewBuilder let row: (Item) -> Row
    @State private var pulledUp = 0.0

    var body: some View {
        let contentHeight = Double(items.count) * (Self.rowHeight + rowGap) + 4 + Self.listTop
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
                header()
                .padding(.horizontal, sidebarEdge)
                .frame(height: Self.rowHeight)
                .padding(.top, 3)
                .padding(.bottom, expanded ? 3 - rowGap / 2 : 3)
            }
            .buttonStyle(.highlight(radius: 0))

            if expanded {
                RecycledList(
                    items: items, height: { _ in Self.rowHeight + rowGap }, topInset: Self.listTop, bottomInset: 4 - rowGap / 2,
                    scrollTarget: scrollTarget,
                    scrollsTargetToTop: true, pointed: pointed, row: row
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
                ResizeGrab(axis: .vertical) { moved in
                    guard let moved else {
                        height = listHeight(in: heights, pulledUp: pulledUp)
                        return pulledUp = 0
                    }
                    pulledUp = -moved
                }
                .frame(height: Theme.resizeGrab)
            }
            .zIndex(1)
    }

    private func listHeight(in heights: ClosedRange<Double>, pulledUp: Double) -> Double {
        let resting = min(heights.upperBound, max(heights.lowerBound, height))
        return min(heights.upperBound, max(heights.lowerBound, resting + pulledUp))
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
                    .padding(.leading, sidebarEdge)
                    .padding(.trailing, sidebarEdge - serverButtonOutset)
                    .frame(height: doneRowHeight)
                    .padding(.vertical, 3)
                ServerErrors(servers: servers)
            }
        } else {
            VStack(spacing: 0) {
                Shelf(items: servers, maxHeight: maxHeight, expanded: $expanded, height: $height) {
                    HStack(spacing: 7) {
                        AllServersLine(servers: servers, expanded: expanded)
                        SidebarChevron(expanded: expanded)
                    }
                } row: { server in
                    ServerLine(server: server)
                        .padding(.leading, sidebarRowPadding)
                        .padding(.trailing, sidebarRowPadding - serverButtonOutset)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .padding(rowMargin)
                }
                ServerErrors(servers: servers)
            }
        }
    }
}

#endif

/// A thread that is archived or snoozed: one quiet line, with its pull request and when it was
/// archived or how soon it wakes. It is redrawn only when what it shows changes.
struct SlimThreadRow: View, Equatable {
    @Environment(AppStore.self) private var store
    let thread: ThreadInfo
    let project: Project?
    let selected: Bool
    let rename: (ThreadInfo) -> Void
    let delete: (ThreadInfo) -> Void
    var open: (Selection) -> Void = { _ in }
    @State private var hovering = false

    private static let sidePadding = sidebarRowPadding

    static func == (one: SlimThreadRow, other: SlimThreadRow) -> Bool {
        one.thread == other.thread && one.project?.iconPath == other.project?.iconPath && one.pullRequest == other.pullRequest
            && one.selected == other.selected
    }

    private var pullRequest: PullRequest? { project?.pullRequest(of: thread) }

    var body: some View {
        HStack(spacing: 7) {
            ProjectIcon(project: project, size: .md)
            Text(thread.title)
                .font(.ui(.sm))
                .lineLimit(1)
                .rowText()
            Spacer(minLength: 6)
            if let pullRequest {
                ThreadPullRequestLabel(thread: thread, pullRequest: pullRequest, colored: false, open: open)
            }
            if thread.section == .working {
                ThreadStatus(thread: thread, readOnly: store.onRemovedServer(thread))
            } else {
                trailing
            }
        }
        .padding(.horizontal, Self.sidePadding)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        .overlay(alignment: .trailing) {
            if !store.onRemovedServer(thread) && thread.section != .working {
                OnRowSurface {
                    if thread.section == .snoozed {
                        ActionButton(icon: .alarmClockOff, help: "Wake", size: .inset) { store.wake([thread.id]) }
                    } else {
                        ActionButton(icon: .undo2, help: "Unarchive", size: .inset) { store.setArchived([thread.id], archived: false) }
                    }
                }
                .padding(.trailing, ControlSize.insetMargin)
                .opacity(hovering ? 1 : 0)
                .allowsHitTesting(hovering)
            }
        }
        .padding(rowMargin)
        .button(.highlight(radius: Radius.md, selected: selected, inset: rowMargin, hovered: hovering, opaque: true)) {
            open(.thread(thread.id))
        }
        .onHover { hovering = $0 }
        #if os(macOS)
        .contextMenu { ThreadMenu(thread: thread, rename: rename, delete: delete) }
        #endif
    }

    /// When it was archived or how soon it wakes, which its button covers under the pointer.
    private var trailing: some View {
        Group {
            if let until = thread.snoozedUntil, thread.section == .snoozed {
                HStack(spacing: 3) {
                    Image(.alarmClock, size: .sm)
                    Text(Time.left(until, now: AgoClock.shared.now))
                        .monospacedDigit()
                }
                .tooltip("Wakes \(Time.stamp(until))")
            } else {
                Text(Time.ago(thread.archivedAt ?? thread.updatedAt, now: AgoClock.shared.now))
            }
        }
        .font(.ui(.xs))
        .foregroundStyle(Color.themeMutedStrongerForeground)
        .opacity(hovering ? 0 : 1)
    }
}

/// Starts a thread, where the sidebar has no draft and no active thread. Laid out like a slim row.
struct NewThreadRow: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        HStack(spacing: 7) {
            Image(.plus, size: .md)
            Text("Create a thread")
                .font(.ui(.sm))
                .lineLimit(1)
            Spacer(minLength: 6)
        }
        .padding(.horizontal, sidebarRowPadding)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        .padding(rowMargin)
        .button(.highlight(radius: Radius.md, inset: rowMargin, faded: true, opaque: true)) { store.newThread() }
    }
}

extension AppStore {
    /// Whether the sidebar offers to start a thread: there is somewhere to start it, and nothing
    /// above the offer.
    func offersNewThread(drafts: [ListedDraft], active: [ThreadInfo]) -> Bool {
        guard !isSearchingThreads, drafts.isEmpty, active.isEmpty else { return false }
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
    /// The colour of its status when it waits for the user, as its sidebar row shows it: an
    /// approval, its usage limit, or a reply they haven't seen. None while it or its agents work.
    var attentionColor: Color? {
        if waitsFor != nil { return .themeWarning }
        if working || gitStage != nil || monitoring { return nil }
        if interruption != nil || woke { return .themeWarning }
        if unread { return .themeSuccess }
        return nil
    }
}

/// What a thread is up to, or how long ago it last was. A thread of an unlinked server is only
/// ever read only.
struct ThreadStatus: View {
    /// The room between the parts of a row's status, and of its last line.
    static let gap: CGFloat = 9

    let thread: ThreadInfo
    var readOnly = false

    var body: some View {
        if readOnly {
            label("Read-only", Color.themeWarning) {
                symbol(.unlink)
            }
            .tooltip("Its server is unlinked, so this thread can only be read or continued on another server.")
        } else if let asking = thread.waitsFor {
            label(asking.word, Color.themeWarning) {
                symbol(asking.symbol)
            }
            .tooltip(thread.asking == nil ? thread.children?.waitingText ?? "" : "")
        } else if thread.waitsOnAgents {
            let agents = thread.children?.running ?? 0
            HStack(spacing: Self.gap) {
                label("\(agents)", Color.themePending) {
                    symbol(.bot)
                }
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    label(Time.elapsed(since: thread.monitoringSince, now: context.date.timeIntervalSince1970), Color.themeProcess) {
                        symbol(.circleDotDashed)
                    }
                }
            }
            .tooltip(thread.children?.runningText ?? "")
        } else if thread.working {
            let agents = max(thread.agents, thread.children?.running ?? 0)
            HStack(spacing: Self.gap) {
                if agents > 0 {
                    label("\(agents)", Color.themePending) {
                        symbol(.bot)
                    }
                    .tooltip(agents == 1 ? "1 agent is working" : "\(agents) agents are working")
                }
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    label(Time.elapsed(since: thread.updatedAt, now: context.date.timeIntervalSince1970), Color.themeProcess) {
                        symbol(.circleDashed)
                    }
                }
            }
        } else if let stage = thread.gitStage {
            label(stage.label, Color.themeProcess) {
                Spinner(size: .sm)
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
            .tooltip(interruption.detail())
        } else if thread.woke {
            label("Woke", Color.themeWarning) {
                symbol(.alarmClock)
            }
        } else if thread.unread {
            label("Finished", Color.themeSuccess) {
                symbol(.flag)
            }
        } else {
            Text(Time.ago(thread.updatedAt, now: AgoClock.shared.now))
                .font(.ui(.xs))
                .foregroundStyle(Color.themeMutedStrongerForeground)
        }
    }

    private func label(_ text: String, _ color: Color, @ViewBuilder icon: () -> some View) -> some View {
        HStack(spacing: 3) {
            icon()
            Text(text)
                .font(.ui(.xs, weight: .medium))
                .monospacedDigit()
        }
        .foregroundStyle(color)
    }

    private func symbol(_ name: Symbol) -> some View {
        Image(name, size: .sm)
    }
}

/// A draft whose first message is on its way, on the working shelf until its thread takes its row.
struct StartingRow: View {
    @Environment(AppStore.self) private var store
    let listed: ListedDraft
    var open: (Selection) -> Void = { _ in }
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 7) {
            ProjectIcon(project: store.project(listed.draft.projectID), size: .md)
            Text(listed.preview)
                .font(.ui(.sm))
                .lineLimit(1)
                .rowText()
            Spacer(minLength: 6)
            HStack(spacing: 3) {
                Spinner(size: .sm)
                Text("Sending")
                    .font(.ui(.xs, weight: .medium))
            }
            .foregroundStyle(Color.themeMutedForeground)
        }
        .padding(.horizontal, sidebarRowPadding)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        .padding(rowMargin)
        .button(.highlight(radius: Radius.md, selected: store.selection == .draft(listed.id), inset: rowMargin, hovered: hovering, opaque: true)) {
            open(.draft(listed.id))
        }
        .onHover { hovering = $0 }
    }
}

/// The line of a part of the list: its name, lit only under the pointer, how many threads it
/// holds, always the quietest, and the chevron that opens and closes it.
struct SectionLine: View {
    let section: ThreadSection
    let count: Int
    let expanded: Bool
    var lit = false
    @Environment(\.row) private var row

    var body: some View {
        HStack(spacing: 7) {
            Text(section.title)
                .font(.ui(.xs, weight: .medium))
                .lineLimit(1)
                .foregroundStyle(lit || row != nil ? Color.themeForeground : Color.themeMutedForeground)
            Text("\(count)")
                .font(.ui(.xs))
                .foregroundStyle(Color.themeMutedStrongestForeground)
                .monospacedDigit()
            Spacer(minLength: 4)
            SidebarChevron(expanded: expanded, lit: lit)
        }
    }
}

/// What a line at the bottom of the sidebar starts with: a symbol, or a server's state, in a
/// column as wide as a slim row's project icon, so that every line's words start where its title does.
struct ShelfLineMark<Mark: View>: View {
    @ViewBuilder let mark: () -> Mark

    var body: some View {
        mark().frame(width: PlatformImage.symbolSide(.md))
    }
}

/// A state as a dot in a ring of its colour's tint, the size of a symbol. Both are even, so that
/// the dot sits in the ring's middle on whole points.
struct StateRing: View {
    let tint: Color

    var body: some View {
        ZStack {
            Circle()
                .strokeBorder(tint.at(.colorTintChartStronger), lineWidth: 1)
                .frame(width: 12, height: 12)
            Circle()
                .fill(tint)
                .frame(width: 6, height: 6)
        }
    }
}

/// Opens or closes a sidebar line's list, at the line's end. Open or closed, it ends where the
/// rows end, in a box as wide as when it is open, so that what comes before it stays put.
struct SidebarChevron: View {
    let expanded: Bool
    var lit = false
    @Environment(\.row) private var row

    var body: some View {
        ZStack(alignment: .trailing) {
            Image(.chevronDown, size: .xs, trimmed: true).hidden()
            Image(expanded ? .chevronDown : .chevronRight, size: .xs, trimmed: true)
        }
        .foregroundStyle(lit || row != nil ? Color.themeMutedForeground : Color.themeMutedStrongerForeground)
    }
}

/// The name of a sidebar line like Done's: muted while it is closed and not lit.
struct ShelfTitle: View {
    let title: String
    let expanded: Bool

    init(_ title: String, expanded: Bool) {
        self.title = title
        self.expanded = expanded
    }

    var body: some View {
        Text(title)
            .font(.ui(.xs, weight: .medium))
            .lineLimit(1)
            .modifier(ShelfTint(expanded: expanded))
    }
}

/// The colour of a sidebar line's name, and of a mark that goes with it.
struct ShelfTint: ViewModifier {
    let expanded: Bool
    @Environment(\.row) private var row

    func body(content: Content) -> some View {
        content.foregroundStyle(expanded || row != nil ? Color.themeForeground : Color.themeMutedForeground)
    }
}

/// Every server in one line, which opens into a line for each: the worst of their states, how
/// they are reached and the slowest round trip.
struct AllServersLine: View {
    let servers: [Server]
    let expanded: Bool

    var body: some View {
        HStack(spacing: 7) {
            ShelfLineMark { StateRing(tint: servers.worstTint) }
            ShelfTitle("Servers", expanded: expanded)
            Spacer(minLength: 4)
            Text(servers.reach)
                .font(.ui(.xs))
                .foregroundStyle(Color.themeMutedStrongerForeground)
                .monospacedDigit()
                .lineLimit(1)
        }
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

/// Why each server that isn't connected isn't, as its link last said, under the servers' lines,
/// so that it is read without pointing at anything. With more than one server each is named.
struct ServerErrors: View {
    let servers: [Server]

    var body: some View {
        let failing = servers.compactMap { server in
            server.state == .connected ? nil : server.error.map { (server, $0) }
        }
        if !failing.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(failing, id: \.0.id) { server, error in
                    Text(servers.count > 1 ? "\(server.name): \(error)" : error)
                        .font(.ui(.xs))
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                        .lineLimit(3)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .textSelection(.enabled)
            .padding(.horizontal, sidebarEdge)
            .padding(.bottom, 12)
        }
    }
}

/// A server and how the client reaches it.
struct ServerLine: View {
    let server: Server

    var body: some View {
        HStack(spacing: 7) {
            ShelfLineMark { StateRing(tint: server.stateTint ?? .themeSuccess) }
            Text(server.name)
                .font(.ui(.xs, weight: .medium))
                .lineLimit(1)
            Spacer(minLength: 4)
            ServerUpdateStatus(server: server) {
                Text(detail)
                    .font(.ui(.xs))
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                    .monospacedDigit()
            }
        }
        .tooltip(server.error ?? detail)
    }

    private var detail: String {
        switch server.state {
        case .connected:
            let path = server.path?.capitalized ?? "Connected"
            return server.rttMs.map { "\(path) · \($0) ms" } ?? path
        case .connecting: return "Connecting"
        case .disconnected: return "Offline"
        case .refused: return "Refused"
        }
    }
}

#if os(macOS)
/// The ways to the settings and the usage (or back from them) and a new version of the client.
struct SidebarFooter: View {
    private static let reach = EdgeInsets(top: 4, leading: ToolbarButton.margin, bottom: 4, trailing: ToolbarButton.margin)
    /// How far a button's symbol draws from the button's sides: Lucide leaves a twelfth of its
    /// square empty around it.
    private static let symbolInset: CGFloat = {
        let side = PlatformImage.symbolSide(ControlSize.regular.wordlessSymbol)
        return ((ControlSize.regular.height - side) / 2 + side / 12).rounded()
    }()

    /// Its buttons are as far from its sides as from the line over it. It keeps its own distance
    /// from the sides, apart from the list's.
    private static let edge: CGFloat = 16

    @Environment(AppStore.self) private var store

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if ![.idle, .checking, .upToDate].contains(store.updater.state) {
                AppUpdateRow(updater: store.updater)
                    .padding(.leading, sidebarEdge - Self.edge)
                    .padding(.trailing, sidebarEdge - Self.edge - serverButtonOutset)
            }
            HStack(spacing: 0) {
                // The avatar sits 2 further in than the buttons beside it, which stay put.
                AccountMenu(margin: Self.reach)
                    .padding(.leading, 2)
                    .padding(.trailing, -2)
                if store.showsUsage || store.settings != nil {
                    ActionButton(
                        "Back", icon: .arrowLeft, help: "Back to the threads (Esc)", variant: .ghost, fills: true, alignment: .leading,
                        margin: Self.reach
                    ) {
                        store.closeRoute()
                    }
                } else {
                    ActionButton(icon: .settings, help: store.shortcuts.help("Settings", "settings.open"), margin: Self.reach) { store.openSettings() }
                    ActionButton(icon: .chartColumn, help: "Usage: what the agents spent and what is left of their plans",
                        margin: Self.reach
                    ) {
                        store.openUsage()
                    }
                }
                Spacer(minLength: 0)
                if store.updater.state == .upToDate {
                    UpdateLabel.upToDate
                        .padding(.trailing, 2)
                }
                ActionButton(icon: .refreshCw, help: "Check for Updates", pending: store.updater.state == .checking,
                    margin: Self.reach
                ) {
                    store.updater.check(asked: true)
                }
            }
            .padding(.horizontal, -Self.symbolInset - Self.reach.leading)
            .padding(.vertical, -Self.reach.top)
        }
        .padding(.horizontal, Self.edge)
        .padding(.vertical, Self.edge - Self.symbolInset)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
#endif
