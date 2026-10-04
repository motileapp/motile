import SwiftUI

/// The panel that opens over the window to start a thread, open one, add a project or run a
/// command, all from the keyboard. ⌘N and the new-thread button open it on the projects when
/// there is more than one, ⌘P on the threads and ⌘K on the commands.
struct CommandPanel: View {
    @Environment(AppStore.self) private var store
    #if os(macOS)
    @Environment(\.openSettings) private var openSettings
    #endif
    @State private var pages: [PanelPage]
    @State private var query = ""
    @State private var highlighted = 0
    /// Whether the keys have moved the highlight. A finger picks without one.
    @State private var steered = Platform.name == "macos"
    @State private var keys: Any?
    /// The folders under the path typed on the folder page. Missing while they are asked for.
    @State private var listing: FolderListing?
    @State private var browseError: String?
    @State private var browsesAsked = 0
    @State private var browsesAnswered = 0
    @State private var copied = false
    @FocusState private var searching: Bool

    init(start: PanelPage) {
        _pages = State(initialValue: [start])
    }

    private var page: PanelPage { pages.last ?? .commands }

    var body: some View {
        let sections = self.sections
        let items = sections.flatMap(\.items)
        panel(sections, rows: items.count)
            .onChange(of: query) {
                highlighted = 0
                store.panelNotice = nil
                browse()
            }
            .onChange(of: store.github) { followGitHub() }
            .onChange(of: store.projectsAdded) { store.closePanel() }
    }

    #if os(macOS)
    private func panel(_ sections: [PanelSection], rows: Int) -> some View {
        ZStack(alignment: .top) {
            Color.black.opacity(0.32)
                .ignoresSafeArea()
                .onTapGesture { store.closePanel() }
            VStack(spacing: 0) {
                header
                Divider()
                results(sections, rows: rows)
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
    }
    #else
    /// The panel as a sheet: the same pages, picked by a tap. The keyboard only comes by itself
    /// where the page is something to type.
    private func panel(_ sections: [PanelSection], rows: Int) -> some View {
        VStack(spacing: 0) {
            header
                .padding(.top, 10)
            Divider()
            results(sections, rows: rows)
        }
        .background(Color.themeBackground)
        .onAppear(perform: focusIfTyped)
        .onChange(of: pages) { focusIfTyped() }
        .onKeyPress(.downArrow) { steer(1) }
        .onKeyPress(.upArrow) { steer(-1) }
        .onKeyPress(.escape) {
            store.closePanel()
            return .handled
        }
    }

    private func focusIfTyped() {
        switch page {
        case .newProject, .folder: searching = true
        default: break
        }
    }

    private func steer(_ step: Int) -> KeyPress.Result {
        let count = sections.flatMap(\.items).filter(\.selectable).count
        highlighted = steered ? min(max(highlighted + step, 0), max(0, count - 1)) : 0
        steered = true
        return .handled
    }
    #endif

    // MARK: Parts

    private var header: some View {
        HStack(spacing: 10) {
            if pages.count > 1 {
                IconOnlyButton(symbol: "arrow.left", help: "Back", size: 26, symbolSize: 14, faded: true) { back() }
            } else {
                Image(systemName: "magnifyingglass")
                    .font(.ui(size: 15, weight: .medium))
                    .foregroundStyle(Color.themeTertiary)
                    .frame(width: 26, height: 26)
            }
            TextField(prompt, text: $query)
                .textFieldStyle(.plain)
                .font(.ui(size: 16))
                .focused($searching)
                #if os(iOS)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.go)
                .onSubmit(runHighlighted)
                #endif
        }
        .padding(.horizontal, 16)
        .frame(height: 52)
    }

    #if os(iOS)
    private func runHighlighted() {
        guard let item = sections.flatMap(\.items).first(where: { $0.index == highlighted }) else { return }
        run(item)
    }
    #endif

    private var prompt: String {
        switch page {
        case .commands: "Search threads, projects and commands…"
        case .projects: "Start a thread in…"
        case .threads: "Go to thread…"
        case .servers: "Add a project on…"
        case .sources: "Add a project…"
        case .newProject: "Project name"
        case .github: "Search your repositories…"
        case .githubSetup: "Set up GitHub…"
        case .folder(let id): "Path on \(serverName(id))"
        }
    }

    private func results(_ sections: [PanelSection], rows: Int) -> some View {
        ScrollViewReader { scroller in
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    if rows == 0 {
                        Text(emptyText)
                            .font(.ui(size: 13))
                            .foregroundStyle(Color.themeTertiary)
                            .multilineTextAlignment(.center)
                            .frame(maxWidth: .infinity)
                            .padding(.horizontal, 24)
                            .padding(.vertical, 28)
                    }
                    ForEach(sections) { section in
                        Text(section.title)
                            .font(.ui(size: 12, weight: .medium))
                            .foregroundStyle(Color.themeTertiary)
                            .padding(.horizontal, PanelRow.sideMargin + 10)
                            .padding(.top, 10)
                            .padding(.bottom, 4)
                        ForEach(section.items) { item in
                            PanelRow(item: item, highlighted: steered && item.selectable && item.index == highlighted)
                                .button(.highlight(radius: PanelRow.radius, inset: PanelRow.margin)) { run(item) }
                                .id(item.index >= 0 ? AnyHashable(item.index) : AnyHashable(item.id))
                                .onHover { if $0, item.index >= 0 { highlighted = item.index } }
                        }
                    }
                    if let notice = store.panelNotice {
                        Text(notice)
                            .font(.ui(size: 12))
                            .foregroundStyle(Color.themeDanger)
                            .lineLimit(2)
                            .padding(.horizontal, PanelRow.sideMargin + 10)
                            .frame(height: Self.noticeHeight, alignment: .leading)
                    }
                }
                .padding(.bottom, 8)
            }
            #if os(macOS)
            .frame(height: height(sections, rows: rows))
            #else
            .frame(maxHeight: .infinity)
            .scrollDismissesKeyboard(.interactively)
            #endif
            .onChange(of: highlighted) { scroller.scrollTo(highlighted) }
        }
    }

    private static let noticeHeight: CGFloat = 36

    /// The pages that fill as the server answers keep one height, so nothing moves when it does.
    private func height(_ sections: [PanelSection], rows: Int) -> CGFloat {
        switch page {
        case .github, .folder: return 420
        default:
            let notice = store.panelNotice == nil ? 0 : Self.noticeHeight
            return min(420, max(90, CGFloat(rows) * PanelRow.height + CGFloat(sections.count) * 32 + 10 + notice))
        }
    }

    private var emptyText: String {
        switch page {
        case .folder: browseError ?? "No folders found"
        case .github(let id): store.repoErrors[id] ?? "No repository found"
        default: "Nothing found"
        }
    }

    #if os(macOS)
    private var hints: some View {
        HStack(spacing: 14) {
            hint(["↑", "↓"], "Navigate")
            if case .folder = page {
                hint(["↩"], "Open")
                hint(["⌘", "↩"], "Add")
            } else {
                hint(["↩"], "Select")
            }
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
                    .font(.ui(size: 11, weight: .medium))
                    .padding(.horizontal, 6)
                    .frame(minWidth: 22, minHeight: 20)
                    .background(Color.themeHover, in: RoundedRectangle(cornerRadius: 5, style: .continuous))
            }
            Text(text)
                .font(.ui(size: 12))
        }
        .foregroundStyle(Color.themeSecondary)
    }

    #endif

    // MARK: What is offered

    private var sections: [PanelSection] {
        var sections: [PanelSection]
        var narrows = true
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
        case .servers: sections = [PanelSection(title: "Servers", items: serverItems)]
        case .sources(let id):
            let title = store.servers.count > 1 ? "Add a project on \(serverName(id))" : "Add a project"
            sections = [PanelSection(title: title, items: sourceItems(id))]
        case .newProject(let id):
            sections = [PanelSection(title: "New project", items: [newProject(on: id)])]
            narrows = false
        case .github(let id): sections = [PanelSection(title: "Your GitHub", items: repoItems(id))]
        case .githubSetup(let id):
            let missing = store.github[id] == .missing
            let title = missing ? "GitHub's gh isn't installed on \(serverName(id))" : "GitHub isn't signed in on \(serverName(id))"
            sections = [PanelSection(title: title, items: setupItems(id))]
        case .folder(let id):
            sections = [PanelSection(title: "Folders on \(serverName(id))", items: folderItems(id))]
            narrows = false
        }
        var index = 0
        return sections.compactMap { section -> PanelSection? in
            var items = narrows ? section.items.filter(matches) : section.items
            guard !items.isEmpty else { return nil }
            for position in items.indices where items[position].selectable {
                items[position].index = index
                index += 1
            }
            return PanelSection(title: section.title, items: items)
        }
    }

    private func matches(_ item: PanelItem) -> Bool {
        guard !query.isEmpty, item.placeholderLines == 0 else { return true }
        return item.title.localizedCaseInsensitiveContains(query) || item.detail.localizedCaseInsensitiveContains(query)
    }

    private func serverName(_ id: String) -> String {
        store.server(id)?.name ?? "your server"
    }

    /// The projects, the one the open thread works in first, then by when a thread last started.
    private var projects: [Project] {
        let recent = store.recentProjects
        guard let current = store.composerProject else { return recent }
        return [current] + recent.filter { $0.id != current.id }
    }

    private var projectItems: [PanelItem] {
        projects.enumerated().map { position, project in
            let server = store.server(project.serverID)?.name ?? ""
            return PanelItem(
                id: "project-\(project.id)",
                title: project.name,
                detail: server.isEmpty ? project.path : "\(server) · \(project.path)",
                icon: .project(project),
                shortcut: position < 9 && page == .projects && Platform.name == "macos" ? position + 1 : nil
            ) { store.startNewThread(in: project) }
        }
    }

    private var addProject: PanelItem {
        PanelItem(id: "add-project", title: "Add a project…", detail: "A new one, one of your GitHub's or a folder", icon: .symbol("folder.badge.plus"), keepsOpen: true) {
            open(store.addProjectPage)
        }
    }

    /// The servers a project can be added on: the open thread's first, the offline ones last.
    private var serverItems: [PanelItem] {
        let current = store.composerServer?.id
        let rank = { (server: Server) in server.state != .connected ? 2 : server.id == current ? 0 : 1 }
        let servers = store.servers.enumerated().sorted { (rank($0.element), $0.offset) < (rank($1.element), $1.offset) }
        return servers.map(\.element).map { server in
            let projects = store.projects.filter { $0.serverID == server.id }.count
            let detail = server.state != .connected ? "Offline" : projects == 1 ? "1 project" : "\(projects) projects"
            var item = PanelItem(id: "server-\(server.id)", title: server.name, detail: detail, icon: .symbol("server.rack"), keepsOpen: true) {
                open(.sources(server.id))
            }
            item.off = server.state != .connected
            return item
        }
    }

    /// GitHub comes last while it still needs setting up on the server.
    private func sourceItems(_ id: String) -> [PanelItem] {
        let folder = PanelItem(id: "source-folder", title: "Local folder", detail: "Browse the folders on \(serverName(id))", icon: .symbol("folder"), keepsOpen: true) {
            open(.folder(id))
        }
        guard store.startsProjects(store.server(id)) else { return [folder] }
        let new = PanelItem(id: "source-new", title: "New project", detail: "Start a new Git repository from a name", icon: .symbol("plus.square"), keepsOpen: true) {
            open(.newProject(id))
        }
        guard let state = store.github[id] else { return [new, .placeholder(0, lines: 2), folder] }
        var github = PanelItem(id: "source-github", title: "Your GitHub", detail: "Clone one of your repositories", icon: .logo(AgentLogo.github), keepsOpen: true) {
            open(state == .ready ? .github(id) : .githubSetup(id))
        }
        guard state != .ready else { return [new, github, folder] }
        github.note = "Setup required"
        github.warns = true
        return [new, folder, github]
    }

    private func newProject(on id: String) -> PanelItem {
        let name = query.trimmingCharacters(in: .whitespaces)
        var item = PanelItem(id: "create", title: name.isEmpty ? "Name the project" : "Create \(name)", detail: "A new Git repository in ~/projects on \(serverName(id))", icon: .symbol("plus.square"), keepsOpen: true) {
            store.newProject(named: name, on: id)
        }
        item.off = name.isEmpty
        item.busy = store.addingProject == AddingProject(serverID: id, name: name)
        return item
    }

    private func repoItems(_ id: String) -> [PanelItem] {
        guard let repos = store.repos[id] else {
            return store.repoErrors[id] == nil ? (0..<8).map { .placeholder($0, lines: 2) } : []
        }
        let clone = { (name: String, title: String, detail: String, symbol: String) in
            var item = PanelItem(id: "repo-\(name)", title: title, detail: detail, icon: .symbol(symbol), keepsOpen: true) {
                store.clone(name, on: id)
            }
            item.busy = store.addingProject == AddingProject(serverID: id, name: name)
            return item
        }
        var items = repos.map { repo in
            var item = clone(repo.name, repo.name, repo.description ?? "", "book.closed")
            item.note = repo.isPrivate ? "Private" : nil
            return item
        }
        // A repository that isn't listed is cloned by its name.
        let typed = query.trimmingCharacters(in: .whitespaces)
        let listed = repos.contains { $0.name.caseInsensitiveCompare(typed) == .orderedSame }
        if !listed, typed.wholeMatch(of: #/[\w.-]+/[\w.-]+/#) != nil {
            items.append(clone(typed, "Clone \(typed)", "A repository that isn't in your list", "arrow.down.circle"))
        }
        return items
    }

    private func setupItems(_ id: String) -> [PanelItem] {
        let name = serverName(id)
        let check = PanelItem(id: "check-github", title: "Check again", detail: "Once that is done", icon: .symbol("arrow.clockwise"), keepsOpen: true) {
            store.readGitHub(id)
        }
        guard store.github[id] != .missing else {
            let install = PanelItem(id: "install-gh", title: "Open cli.github.com", detail: "Install gh on \(name), then run gh auth login there", icon: .symbol("arrow.up.right.square"), keepsOpen: true) {
                guard let site = URL(string: "https://cli.github.com") else { return }
                Platform.open(site)
            }
            return [install, check]
        }
        let copy = PanelItem(id: "copy-login", title: copied ? "Copied" : "Copy gh auth login", detail: "Run it in a terminal on \(name)", icon: .symbol(copied ? "checkmark" : "doc.on.doc"), keepsOpen: true) {
            Platform.copy("gh auth login")
            copied = true
        }
        return [copy, check]
    }

    /// The folder that is typed, the one above it, and the folders in it.
    private func folderItems(_ id: String) -> [PanelItem] {
        guard let listing else {
            return browseError == nil ? (0..<8).map { .placeholder($0, lines: 1) } : []
        }
        let projects = Set(store.projects.filter { $0.serverID == id }.map(\.path))
        var items: [PanelItem] = []
        if query.hasSuffix("/") || query == "~" {
            items.append(PanelItem(id: "add-here", title: "Add this folder", detail: listing.typed, icon: .symbol("folder.badge.plus")) {
                store.addProject(serverID: id, path: listing.path)
            })
            if let parent = listing.parent {
                items.append(PanelItem(id: "parent", title: "..", detail: "", icon: .symbol("arrow.turn.left.up"), keepsOpen: true) {
                    query = parent
                })
            }
        }
        items += listing.folders.map { folder in
            var item = PanelItem(id: folder.path, title: folder.name, detail: "", icon: .symbol("folder"), keepsOpen: true) {
                query = folder.typed
            }
            item.note = projects.contains(folder.path) ? "Project" : nil
            item.alternate = { store.addProject(serverID: id, path: folder.path) }
            return item
        }
        return items
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

    /// The servers that run an older version than the newest release.
    private var serverUpdates: [PanelItem] {
        store.servers.filter { store.isOutdated($0) && store.serverUpdates[$0.id] == nil }.map { server in
            PanelItem(id: "update-\(server.id)", title: "Update \(server.name)", detail: "From version \(server.version) to \(store.updater.latest ?? "")", icon: .symbol("arrow.down.circle")) {
                store.update(server)
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
            PanelItem(id: "add-server", title: "Add a server…", detail: "A machine that runs your agents", icon: .symbol("server.rack")) {
                store.showsAddServer = true
            },
        ]
        let settings = PanelItem(id: "settings", title: "Settings…", detail: "Appearance, servers and projects", icon: .symbol("gearshape")) {
            #if os(macOS)
            openSettings()
            #else
            store.showsSettings = true
            #endif
        }
        return serverUpdates + always + appUpdate + [settings]
    }

    /// The Mac app updates itself; the others are updated by where they came from.
    private var appUpdate: [PanelItem] {
        #if os(macOS)
        [
            PanelItem(id: "check-updates", title: "Check for updates", detail: "Motile \(store.updater.current)", icon: .symbol("arrow.triangle.2.circlepath")) {
                store.updater.check(asked: true)
            }
        ]
        #else
        []
        #endif
    }

    // MARK: Acting

    private func open(_ page: PanelPage) {
        pages.append(page)
        arrive()
    }

    private func back() {
        guard pages.count > 1 else { return }
        pages.removeLast()
        arrive()
    }

    /// Moves between the repositories and what GitHub still needs as the server's answer changes.
    private func followGitHub() {
        switch page {
        case .github(let id) where store.github[id] != .ready: pages[pages.count - 1] = .githubSetup(id)
        case .githubSetup(let id) where store.github[id] == .ready: pages[pages.count - 1] = .github(id)
        default: return
        }
        arrive()
    }

    private func arrive() {
        highlighted = 0
        store.panelNotice = nil
        copied = false
        listing = nil
        browseError = nil
        query = ""
        switch page {
        case .folder: query = "~/"
        case .github(let id): store.loadRepos(id)
        default: break
        }
    }

    /// Asks for the folders under the typed path. Placeholders take the place of the folders
    /// that are shown when the answer takes a while.
    private func browse() {
        guard case .folder(let id) = page else { return }
        browsesAsked += 1
        let asked = browsesAsked
        store.browse(serverID: id, query: query) { result in
            guard asked == browsesAsked else { return }
            browsesAnswered = asked
            switch result {
            case .success(let found):
                listing = found
                browseError = nil
            case .failure(let error):
                listing = nil
                browseError = error.message
            }
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
            guard asked == browsesAsked, browsesAnswered < asked else { return }
            listing = nil
            browseError = nil
        }
    }

    private func run(_ item: PanelItem) {
        guard item.selectable else { return }
        if !item.keepsOpen { store.closePanel() }
        item.action()
    }

    #if os(macOS)
    /// Takes the keys the panel is steered with. `false` leaves the key to the search field.
    private func handle(_ event: NSEvent) -> Bool {
        let items = sections.flatMap(\.items)
        let command = event.modifierFlags.contains(.command)
        switch event.keyCode {
        case 53:
            store.closePanel()
        case 125:
            highlighted = min(highlighted + 1, max(0, items.filter(\.selectable).count - 1))
        case 126:
            highlighted = max(highlighted - 1, 0)
        case 36, 76:
            guard let item = items.first(where: { $0.index == highlighted }) else { return true }
            guard command, let alternate = item.alternate else {
                run(item)
                return true
            }
            store.closePanel()
            alternate()
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
    #endif
}

private struct PanelSection: Identifiable {
    let title: String
    let items: [PanelItem]
    var id: String { title }
}

private struct PanelItem: Identifiable {
    enum Icon {
        case symbol(String)
        /// A logo, drawn in the colour of the symbols.
        case logo(PlatformImage?)
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
    /// Its place among what can be selected, for moving through with the arrow keys.
    var index = -1
    /// Said at the row's end, as a warning with `warns`.
    var note: String?
    var warns = false
    /// It is shown and can't be selected.
    var off = false
    /// What it started is still going on.
    var busy = false
    /// Stands in for a row that is on its way, with as many lines of text.
    var placeholderLines = 0
    /// What ⌘↩ does instead.
    var alternate: (() -> Void)?
    let action: () -> Void

    var selectable: Bool { !off && placeholderLines == 0 }

    static func placeholder(_ position: Int, lines: Int) -> PanelItem {
        var item = PanelItem(id: "placeholder-\(position)", title: "", detail: "", icon: .symbol("")) {}
        item.placeholderLines = lines
        item.index = position
        return item
    }

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
    static let height: CGFloat = scaled(46)
    static let sideMargin: CGFloat = 8
    static let radius: CGFloat = 9
    static let margin = EdgeInsets(top: 0, leading: sideMargin, bottom: 0, trailing: sideMargin)
    private static let titleHeight: CGFloat = scaled(17)
    private static let detailHeight: CGFloat = scaled(15)

    let item: PanelItem
    let highlighted: Bool
    @State private var faded = false

    var body: some View {
        HStack(spacing: 12) {
            icon
                .frame(width: 22, height: 22)
            VStack(alignment: .leading, spacing: 1) {
                if item.placeholderLines > 0 {
                    placeholderLines
                } else {
                    Text(item.title)
                        .font(.ui(size: 14))
                        .lineLimit(1)
                        .frame(height: Self.titleHeight)
                    if !item.detail.isEmpty {
                        Text(item.detail)
                            .font(.ui(size: 12))
                            .foregroundStyle(Color.themeSecondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                            .frame(height: Self.detailHeight)
                    }
                }
            }
            Spacer(minLength: 8)
            trailing
        }
        .padding(.horizontal, 10)
        .frame(height: Self.height)
        .frame(maxWidth: .infinity, alignment: .leading)
        .opacity(item.off ? 0.45 : 1)
        .background(highlighted ? Color.themeHover : Color.clear, in: RoundedRectangle(cornerRadius: Self.radius, style: .continuous))
        .padding(.horizontal, Self.sideMargin)
        .contentShape(Rectangle())
        .opacity(faded ? 0.45 : 1)
        .animation(item.placeholderLines > 0 ? .easeInOut(duration: 0.8).repeatForever(autoreverses: true) : nil, value: faded)
        .onAppear { faded = item.placeholderLines > 0 }
        .onChange(of: item.placeholderLines) { faded = item.placeholderLines > 0 }
    }

    /// Bars where the text will be, each in the room its line takes.
    @ViewBuilder private var placeholderLines: some View {
        let titles: [CGFloat] = [150, 210, 120, 180, 240, 140, 200, 160]
        let details: [CGFloat] = [280, 190, 320, 230, 150, 300, 210, 260]
        bar(width: titles[item.index % titles.count], height: 10)
            .frame(height: Self.titleHeight)
        if item.placeholderLines > 1 {
            bar(width: details[item.index % details.count], height: 8)
                .frame(height: Self.detailHeight)
        }
    }

    private func bar(width: CGFloat, height: CGFloat) -> some View {
        RoundedRectangle(cornerRadius: height / 2, style: .continuous)
            .fill(Color.themeSelected)
            .frame(width: width, height: height)
    }

    @ViewBuilder private var trailing: some View {
        if item.busy {
            ProgressView()
                .controlSize(.small)
        } else if let note = item.note {
            Text(note)
                .font(.ui(size: 12, weight: .medium))
                .foregroundStyle(item.warns ? Color.themeWarning : Color.themeTertiary)
                .padding(.horizontal, item.warns ? 8 : 0)
                .padding(.vertical, item.warns ? 3 : 0)
                .background(item.warns ? Color.themeWarning.opacity(0.14) : Color.clear, in: Capsule())
        } else if let shortcut = item.shortcut {
            Text("⌘\(shortcut)")
                .font(.ui(size: 12, weight: .medium))
                .foregroundStyle(Color.themeTertiary)
                .monospacedDigit()
        }
    }

    @ViewBuilder private var icon: some View {
        switch item.icon {
        case .symbol where item.placeholderLines > 0:
            RoundedRectangle(cornerRadius: 5, style: .continuous)
                .fill(Color.themeSelected)
                .frame(width: 20, height: 20)
        case .symbol(let name):
            Image(systemName: name)
                .font(.ui(size: 15, weight: .medium))
                .foregroundStyle(Color.themeSecondary)
        case .logo(let logo):
            Image(platform: logo ?? PlatformImage())
                .renderingMode(.template)
                .resizable()
                .interpolation(.high)
                .frame(width: 16, height: 16)
                .foregroundStyle(Color.themeSecondary)
        case .project(let project):
            ProjectIcon(project: project, size: 20)
        }
    }
}
