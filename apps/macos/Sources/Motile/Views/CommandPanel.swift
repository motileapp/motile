import AppKit
import SwiftUI

/// The panel that opens over the window to start a thread, open one, or run a command, all from
/// the keyboard. ⌘N opens it on the projects, ⌘P on the threads and ⌘K on the commands.
struct CommandPanel: View {
    @Environment(AppStore.self) private var store
    @Environment(\.openSettings) private var openSettings
    @State private var pages: [PanelPage]
    @State private var query = ""
    @State private var highlighted = 0
    @State private var keys: Any?
    @FocusState private var searching: Bool

    init(start: PanelPage) {
        _pages = State(initialValue: [start])
    }

    private var page: PanelPage { pages.last ?? .commands }

    var body: some View {
        let sections = self.sections
        let items = sections.flatMap(\.items)
        ZStack(alignment: .top) {
            Color.black.opacity(0.32)
                .ignoresSafeArea()
                .onTapGesture { store.closePanel() }
            VStack(spacing: 0) {
                header
                Divider()
                results(sections, count: items.count)
                Divider()
                hints
            }
            .frame(width: 620)
            .background(Color.themeRaised, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(Color.themeStrongBorder, lineWidth: 1))
            .shadow(color: .black.opacity(0.3), radius: 30, y: 14)
            .padding(.top, 70)
        }
        .onAppear {
            DispatchQueue.main.async { searching = true }
            keys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                handle(event) ? nil : event
            }
        }
        .onDisappear {
            if let keys { NSEvent.removeMonitor(keys) }
            keys = nil
        }
        .onChange(of: query) { highlighted = 0 }
    }

    // MARK: Parts

    private var header: some View {
        HStack(spacing: 10) {
            if pages.count > 1 {
                IconOnlyButton(symbol: "arrow.left", help: "Back", size: 26, symbolSize: 14) { back() }
                    .foregroundStyle(Color.themeSecondary)
            } else {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 15, weight: .medium))
                    .foregroundStyle(Color.themeTertiary)
                    .frame(width: 26, height: 26)
            }
            TextField(prompt, text: $query)
                .textFieldStyle(.plain)
                .font(.system(size: 16))
                .focused($searching)
        }
        .padding(.horizontal, 16)
        .frame(height: 52)
    }

    private var prompt: String {
        switch page {
        case .commands: "Search threads, projects and commands…"
        case .projects: "Start a thread in…"
        case .threads: "Go to thread…"
        }
    }

    private func results(_ sections: [PanelSection], count: Int) -> some View {
        ScrollViewReader { scroller in
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    if count == 0 {
                        Text("Nothing found")
                            .font(.system(size: 13))
                            .foregroundStyle(Color.themeTertiary)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 28)
                    }
                    ForEach(sections) { section in
                        Text(section.title)
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(Color.themeTertiary)
                            .padding(.horizontal, PanelRow.sideMargin + 10)
                            .padding(.top, 10)
                            .padding(.bottom, 4)
                        ForEach(section.items) { item in
                            PanelRow(item: item, highlighted: item.index == highlighted)
                                .id(item.index)
                                .onHover { if $0 { highlighted = item.index } }
                                .onTapGesture { run(item) }
                        }
                    }
                }
                .padding(.bottom, 8)
            }
            .frame(height: min(420, max(90, CGFloat(count) * PanelRow.height + CGFloat(sections.count) * 32 + 10)))
            .onChange(of: highlighted) { scroller.scrollTo(highlighted) }
        }
    }

    private var hints: some View {
        HStack(spacing: 14) {
            hint(["↑", "↓"], "Navigate")
            hint(["↩"], "Select")
            if pages.count > 1 { hint(["⌫"], "Back") }
            hint(["esc"], "Close")
            Spacer()
        }
        .padding(.horizontal, 16)
        .frame(height: 40)
    }

    private func hint(_ keys: [String], _ text: String) -> some View {
        HStack(spacing: 5) {
            ForEach(keys, id: \.self) { key in
                Text(key)
                    .font(.system(size: 11, weight: .medium))
                    .padding(.horizontal, 6)
                    .frame(minWidth: 22, minHeight: 20)
                    .background(Color.themeHover, in: RoundedRectangle(cornerRadius: 5, style: .continuous))
            }
            Text(text)
                .font(.system(size: 12))
        }
        .foregroundStyle(Color.themeSecondary)
    }

    // MARK: What is offered

    private var sections: [PanelSection] {
        var sections: [PanelSection]
        switch page {
        case .projects: sections = [PanelSection(title: "Projects", items: projectItems + [addProject])]
        case .threads: sections = [PanelSection(title: "Threads", items: threadItems)]
        case .commands:
            sections = [
                PanelSection(title: "This thread", items: threadCommands),
                PanelSection(title: "Commands", items: commands),
            ]
            // Searching from here looks through everything.
            if !query.isEmpty {
                sections.append(PanelSection(title: "Threads", items: threadItems))
                sections.append(PanelSection(title: "Start a thread in", items: projectItems))
            }
        }
        var index = 0
        return sections.compactMap { section -> PanelSection? in
            var items = section.items.filter(matches)
            guard !items.isEmpty else { return nil }
            for position in items.indices {
                items[position].index = index
                index += 1
            }
            return PanelSection(title: section.title, items: items)
        }
    }

    private func matches(_ item: PanelItem) -> Bool {
        guard !query.isEmpty else { return true }
        return item.title.localizedCaseInsensitiveContains(query) || item.detail.localizedCaseInsensitiveContains(query)
    }

    private var projectItems: [PanelItem] {
        store.recentProjects.enumerated().map { position, project in
            let host = store.host(project.hostID)?.name ?? ""
            return PanelItem(
                id: "project-\(project.id)",
                title: project.name,
                detail: host.isEmpty ? project.path : "\(host) · \(project.path)",
                icon: .project(project),
                shortcut: position < 9 && page == .projects ? position + 1 : nil
            ) { store.startNewThread(in: project) }
        }
    }

    private var addProject: PanelItem {
        PanelItem(id: "add-project", title: "Add a project…", detail: "A folder on a host", icon: .symbol("folder.badge.plus")) {
            store.showsFolderPicker = true
        }
    }

    private var threadItems: [PanelItem] {
        (store.activeThreads + store.doneThreads).map { thread in
            let project = store.project(thread.projectID)
            let name = project?.name ?? URL(fileURLWithPath: thread.cwd).lastPathComponent
            let state = thread.isDone ? "done" : thread.needsApproval ? "needs approval" : thread.running ? "working" : thread.monitoring ? "monitoring" : Time.ago(thread.updatedAt)
            return PanelItem(id: "thread-\(thread.id)", title: thread.title, detail: "\(name) · \(state)", icon: .project(project)) {
                store.select(.thread(thread.id))
            }
        }
    }

    /// What can be done with the open thread.
    private var threadCommands: [PanelItem] {
        guard let thread = store.selectedThread else { return [] }
        var items: [PanelItem] = []
        if thread.busy {
            items.append(PanelItem(id: "stop", title: "Stop the agent", detail: thread.title, icon: .symbol("stop.circle")) { store.stop() })
        } else {
            let done = thread.isDone
            items.append(
                PanelItem(id: "done", title: done ? "Mark undone" : "Mark done", detail: thread.title, icon: .symbol(done ? "arrow.uturn.backward.circle" : "checkmark.circle")) {
                    store.toggleDone()
                }
            )
        }
        return items
    }

    /// The hosts that run an older version than the newest release.
    private var hostUpdates: [PanelItem] {
        store.hosts.filter { store.isOutdated($0) && store.hostUpdates[$0.id] == nil }.map { host in
            PanelItem(id: "update-\(host.id)", title: "Update \(host.name)", detail: "From version \(host.version) to \(store.updater.latest ?? "")", icon: .symbol("arrow.down.circle")) {
                store.update(host)
            }
        }
    }

    private var commands: [PanelItem] {
        let always: [PanelItem] = [
            PanelItem(id: "new-thread", title: "New thread…", detail: "Choose a project to start in", icon: .symbol("square.and.pencil"), keepsOpen: true) {
                open(.projects)
            },
            PanelItem(id: "go-to-thread", title: "Go to thread…", detail: "\(store.threads.count) threads", icon: .symbol("text.bubble"), keepsOpen: true) {
                open(.threads)
            },
            addProject,
            PanelItem(id: "add-host", title: "Add a host…", detail: "A machine that runs your agents", icon: .symbol("server.rack")) {
                store.showsAddHost = true
            },
            PanelItem(id: "check-updates", title: "Check for updates", detail: "Motile \(store.updater.current)", icon: .symbol("arrow.triangle.2.circlepath")) {
                store.updater.check(asked: true)
            },
            PanelItem(id: "settings", title: "Settings…", detail: "Appearance, hosts and projects", icon: .symbol("gearshape")) {
                openSettings()
            },
        ]
        return hostUpdates + always
    }

    // MARK: Acting

    private func open(_ page: PanelPage) {
        pages.append(page)
        query = ""
        highlighted = 0
    }

    private func back() {
        guard pages.count > 1 else { return }
        pages.removeLast()
        query = ""
        highlighted = 0
    }

    private func run(_ item: PanelItem) {
        if !item.keepsOpen { store.closePanel() }
        item.action()
    }

    /// Takes the keys the panel is steered with. `false` leaves the key to the search field.
    private func handle(_ event: NSEvent) -> Bool {
        let items = sections.flatMap(\.items)
        let command = event.modifierFlags.contains(.command)
        switch event.keyCode {
        case 53:
            store.closePanel()
        case 125:
            highlighted = min(highlighted + 1, max(0, items.count - 1))
        case 126:
            highlighted = max(highlighted - 1, 0)
        case 36, 76:
            guard let item = items.first(where: { $0.index == highlighted }) else { return true }
            run(item)
        case 51 where query.isEmpty && pages.count > 1:
            back()
        default:
            guard command, let digit = Int(event.charactersIgnoringModifiers ?? ""),
                let item = items.first(where: { $0.shortcut == digit })
            else { return false }
            run(item)
        }
        return true
    }
}

private struct PanelSection: Identifiable {
    let title: String
    let items: [PanelItem]
    var id: String { title }
}

private struct PanelItem: Identifiable {
    enum Icon {
        case symbol(String)
        case project(Project?)
    }

    let id: String
    let title: String
    let detail: String
    let icon: Icon
    /// The digit that runs it with ⌘.
    var shortcut: Int?
    /// It leads to another page of the panel.
    var keepsOpen = false
    /// Its place among everything shown, for moving through with the arrow keys.
    var index = 0
    let action: () -> Void

    init(id: String, title: String, detail: String, icon: Icon, shortcut: Int? = nil, keepsOpen: Bool = false, action: @escaping () -> Void) {
        self.id = id
        self.title = title
        self.detail = detail
        self.icon = icon
        self.shortcut = shortcut
        self.keepsOpen = keepsOpen
        self.action = action
    }
}

private struct PanelRow: View {
    static let height: CGFloat = 46
    static let sideMargin: CGFloat = 8

    let item: PanelItem
    let highlighted: Bool

    var body: some View {
        HStack(spacing: 12) {
            icon
                .frame(width: 22, height: 22)
            VStack(alignment: .leading, spacing: 1) {
                Text(item.title)
                    .font(.system(size: 14))
                    .lineLimit(1)
                Text(item.detail)
                    .font(.system(size: 12))
                    .foregroundStyle(Color.themeSecondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: 8)
            if let shortcut = item.shortcut {
                Text("⌘\(shortcut)")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(Color.themeTertiary)
                    .monospacedDigit()
            }
        }
        .padding(.horizontal, 10)
        .frame(height: Self.height)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(highlighted ? Color.themeHover : Color.clear, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
        .padding(.horizontal, Self.sideMargin)
        .contentShape(Rectangle())
    }

    @ViewBuilder private var icon: some View {
        switch item.icon {
        case .symbol(let name):
            Image(systemName: name)
                .font(.system(size: 15, weight: .medium))
                .foregroundStyle(Color.themeSecondary)
        case .project(let project):
            ProjectIcon(project: project, size: 20)
        }
    }
}
