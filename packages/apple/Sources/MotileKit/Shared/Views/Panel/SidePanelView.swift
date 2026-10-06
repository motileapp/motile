import AVKit
import SwiftUI

#if os(macOS)
/// The panel beside the thread: its tabs in the window's top bar, and under them what the open
/// tab shows of the folder the thread works in.
struct SidePanelView: View {
    @Environment(AppStore.self) private var store
    /// The height of the window's top bar, which the tabs are drawn in.
    let topInset: CGFloat
    /// How far the tabs start from the panel's left edge: past the window's buttons when the
    /// panel reaches them.
    var tabInset: CGFloat = 0

    var body: some View {
        let tabs = store.sidePanel.tabs
        VStack(spacing: 0) {
            PanelLine()
            PanelContent(active: tabs.active)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .overlay(alignment: .top) {
            PanelTabStrip(tabs: tabs)
                // The buttons that maximize and hide the panel are at the window's edge.
                .padding(.trailing, 2 * ToolbarButton.width + 14)
                .padding(.leading, tabInset)
                .frame(height: topInset)
                .offset(y: -topInset)
        }
    }
}
#endif

/// What the open tab shows of the folder the thread works in.
struct PanelContent: View {
    @Environment(AppStore.self) private var store
    let active: PanelTab?

    var body: some View {
        if let reason = store.panelUnavailable {
            PanelMessage(text: reason)
        } else if let target = store.panelTarget {
            switch active {
            case .diff: DiffSurface(target: target)
            case .files: FilesSurface(target: target)
            case .file(let path): FileSurface(target: target, path: path).id(path)
            case .change(let turn, let path): ChangeSurface(target: target, turn: turn, path: path).id(active)
            case .agents: AgentsSurface()
            case .pullRequest: PullRequestSurface(target: target)
            case .pullRequestNumber(let number): PullRequestSurface(target: target, number: number).id(number)
            case .pullRequests: PullRequestListSurface(target: target)
            case .linear: LinearSurface(target: target)
            case .linearIssue(let workspace, let id, _): LinearIssueSurface(target: target, workspace: workspace, id: id).id(id)
            case .blank, nil: PanelLauncher(target: target)
            }
        }
    }
}

struct PanelLine: View {
    var body: some View {
        Rectangle()
            .fill(Color.themeBorder)
            .frame(height: 1)
    }
}

/// What a tab says in the middle of the panel when it has nothing to show.
struct PanelMessage: View {
    let text: String
    var failed = false

    var body: some View {
        Text(text)
            .font(.ui(size: 12.5))
            .foregroundStyle(failed ? Color.themeDanger : Color.themeSecondary)
            .multilineTextAlignment(.center)
            .textSelection(.enabled)
            .padding(24)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// What the changes tab says of a folder that is no git repository, with the button that makes
/// it one.
private struct NoRepositoryMessage: View {
    @Environment(AppStore.self) private var store
    let projectID: String

    var body: some View {
        VStack(spacing: 12) {
            Text("This folder isn't a git repository.")
                .font(.ui(size: 12.5))
                .foregroundStyle(Color.themeSecondary)
                .multilineTextAlignment(.center)
            if let project = store.project(projectID), store.canInitializeGit(of: project) {
                ActionButton("Initialize git", icon: .gitBranch, pending: store.initializingGit.contains(project.id)) {
                    store.initializeGit(in: project)
                }
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct PanelLoading: View {
    var body: some View {
        Spinner(size: ControlSize.large.symbol)
            .foregroundStyle(Color.themeTertiary)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// The strip under the tabs with what the open tab is about and its buttons, on one row or two.
struct PanelBar<Content: View, Second: View>: View {
    private let content: Content
    private let second: Second?

    init(@ViewBuilder content: () -> Content) where Second == EmptyView {
        self.content = content()
        second = nil
    }

    init(@ViewBuilder content: () -> Content, @ViewBuilder second: () -> Second) {
        self.content = content()
        self.second = second()
    }

    var body: some View {
        VStack(spacing: 0) {
            row { content }
            if let second {
                row { second }
            }
            PanelLine()
        }
    }

    private func row<Row: View>(@ViewBuilder _ row: () -> Row) -> some View {
        HStack(spacing: 4) { row() }
            .padding(.leading, 12)
            .padding(.trailing, 4)
            .frame(height: pressable(36))
    }
}

/// Said over what a tab shows when that is only a part of it.
struct PanelNote: View {
    let text: String

    var body: some View {
        VStack(spacing: 0) {
            Text(text)
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeWarning)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 12)
                .padding(.vertical, 7)
                .background(Color(platform: Theme.warningBackground))
            PanelLine()
        }
    }
}

struct PanelTabStrip: View {
    @Environment(AppStore.self) private var store
    let tabs: PanelTabs

    var body: some View {
        ScrollViewReader { strip in
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 2) {
                    if !tabs.isBlank {
                        ForEach(tabs.tabs) { tab in
                            PanelTabChip(tab: tab, active: tab == tabs.active)
                        }
                    }
                    if store.panelUnavailable == nil, !tabs.isBlank {
                        ActionButton(icon: .plus, help: "New tab") { store.sidePanel.openBlank() }
                    }
                }
                .padding(.horizontal, 8)
                .frame(maxHeight: .infinity)
            }
            .onChange(of: tabs.active, initial: true) {
                guard let active = tabs.active else { return }
                strip.scrollTo(active.id)
            }
        }
    }
}

/// The menu items that open and switch the panel's tabs, on the keys Safari and Chrome use.
struct PanelTabCommands: View {
    let store: AppStore

    var body: some View {
        let panel = store.sidePanel
        let switchable = panel.isOpen && panel.tabs.tabs.count > 1
        Button("New Tab") { panel.openBlank() }
            .keyboardShortcut("t")
            .disabled(store.panelUnavailable != nil)
        Button("Show Next Tab") { panel.activate(offset: 1) }
            .keyboardShortcut(.tab, modifiers: .control)
            .disabled(!switchable)
        Button("Show Previous Tab") { panel.activate(offset: -1) }
            .keyboardShortcut(.tab, modifiers: [.control, .shift])
            .disabled(!switchable)
        Button("Show Next Tab") { panel.activate(offset: 1) }
            .keyboardShortcut("]", modifiers: [.command, .shift])
            .disabled(!switchable)
        Button("Show Previous Tab") { panel.activate(offset: -1) }
            .keyboardShortcut("[", modifiers: [.command, .shift])
            .disabled(!switchable)
    }
}

private struct PanelTabChip: View {
    @Environment(AppStore.self) private var store
    let tab: PanelTab
    let active: Bool
    @State private var hovering = false
    @Environment(\.surface) private var surface

    private static let height = ControlSize.regular.height
    private static let closeSize = ControlSize.small.height
    /// The close button is as far from the tab's side as from its top and bottom.
    private static let closeMargin = (height - closeSize) / 2
    /// How far above and under the tab a finger still presses it.
    private static let reach = max(0, (Platform.minimumPress - height) / 2)

    var body: some View {
        let panel = store.sidePanel
        HStack(spacing: 6) {
            Image(tab.symbol, size: 11)
                .frame(width: 14)
            Text(tab.title)
                .font(.ui(size: 12, weight: .medium))
                .lineLimit(1)
                .truncationMode(.middle)
            Color.clear
                .frame(width: Self.closeSize, height: Self.closeSize)
        }
        .padding(.leading, 9)
        .padding(.trailing, Self.closeMargin)
        .frame(height: Self.height)
        .frame(maxWidth: 180)
        .padding(.vertical, Self.reach)
        .button(.highlight(selected: active, lit: hovering, inset: EdgeInsets(top: Self.reach, leading: 0, bottom: Self.reach, trailing: 0), faded: true)) { panel.activate(tab) }
        .overlay(alignment: .trailing) {
            ActionButton(icon: .x, help: "Close (⌘W)", size: .small, symbolSize: 11) { panel.close(tab) }
                .environment(\.surface, active ? surface.further : surface.next)
                .padding(.trailing, Self.closeMargin)
                .opacity(hovering || active ? 1 : 0)
        }
        .padding(.vertical, -Self.reach)
        .onHover { hovering = $0 }
        .help(tab.path ?? tab.title)
        .contextMenu {
            Button("Close") { panel.close(tab) }
            Button("Close Others") { panel.closeOthers(tab) }
            Button("Close All") { panel.closeAll() }
            if let path = tab.path {
                Divider()
                Button("Copy Path") { Platform.copy(path) }
            }
        }
    }
}

/// What a blank tab shows: the tabs there are to open.
private struct PanelLauncher: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget

    var body: some View {
        VStack(spacing: 12) {
            Text("Open")
                .font(.ui(size: 13, weight: .semibold))
                .foregroundStyle(Color.themeText)
            VStack(spacing: 2) {
                row(.folder, "Files", keys: "⇧⌘E", reason: nil) { store.sidePanel.open(.files) }
                row(.diff, "Diff", keys: "⌘D", reason: target.repository ? nil : "Available in git repositories.") {
                    store.sidePanel.showDiff()
                }
                row(.users, "Agents", keys: "⇧⌘A", reason: nil) { store.sidePanel.open(.agents) }
                row(.gitPullRequest, "Pull Request", keys: "⇧⌘R", reason: store.pullRequestsUnavailable) {
                    store.sidePanel.open(.pullRequest)
                }
                if store.pullRequestsExtended {
                    row(.list, "All Pull Requests", keys: "⌥⇧⌘R", reason: store.pullRequestsUnavailable) {
                        store.sidePanel.open(.pullRequests)
                    }
                }
                row(.linear, store.linear.connected(target.serverID).isEmpty ? "Connect Linear" : "Linear", keys: nil, reason: store.linearUnavailable) {
                    store.sidePanel.open(.linear)
                }
            }
            .frame(width: 250)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .task(id: target.serverID) { store.linear.read(target.serverID) }
    }

    private func row(_ symbol: Symbol, _ title: String, keys: String?, reason: String?, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(symbol, size: 13)
                    .frame(width: 18)
                Text(title)
                    .font(.ui(size: 13))
                Spacer()
                #if os(macOS)
                if let keys {
                    Text(keys)
                        .font(.ui(size: 11, weight: .medium))
                        .foregroundStyle(Color.themeSecondary)
                        .padding(.horizontal, 6)
                        .frame(height: 20)
                        .background(Color.themeBackgroundTertiary, in: RoundedRectangle(cornerRadius: 5, style: .continuous))
                }
                #endif
            }
            .foregroundStyle(Color.themeText)
            .padding(.horizontal, 10)
            .frame(height: pressable(34))
        }
        .buttonStyle(.highlight())
        .disabled(reason != nil)
        .opacity(reason == nil ? 1 : 0.45)
        .help(reason ?? "")
    }
}

extension EnvironmentValues {
    /// Whether the panel can be seen. The Mac keeps a hidden one, which doesn't ask its server.
    @Entry var panelInView = true
}

extension View {
    /// Asks the server when `id` changes and when the panel comes back into view, never while
    /// it is out of sight.
    func panelTask<ID: Equatable>(id: ID, _ ask: @escaping @MainActor () async -> Void) -> some View {
        modifier(PanelTask(id: id, ask: ask))
    }
}

private struct PanelTask<ID: Equatable>: ViewModifier {
    struct Trigger: Equatable {
        let id: ID
        let inView: Bool
    }

    @Environment(\.panelInView) private var inView
    let id: ID
    let ask: @MainActor () async -> Void

    func body(content: Content) -> some View {
        content.task(id: Trigger(id: id, inView: inView)) {
            guard inView else { return }
            await ask()
        }
    }
}

/// What makes a tab ask its server again: another folder, a turn that ended there, or the
/// button that asks.
struct PanelTrigger: Equatable {
    let target: PanelTarget
    var scope: DiffScope?
    var path: String?
    let version: Int
    let asked: Int
}

/// The changes in the thread's folder: what isn't committed, what the branch adds, or what one
/// turn did.
struct DiffSurface: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget
    @State private var asked = 0
    @State private var commenting: CommentedLine?

    /// The pull request the diff shows, when it shows one or one of its commits, as far as it
    /// has been read.
    private func page(for scope: DiffScope) -> PullRequestPage? {
        let page = store.sidePanel.pullRequest.value
        switch scope {
        case .pullRequest(let number): return page?.number == number ? page : nil
        case .commit(let sha): return page?.activity.contains { $0.commits.contains { $0.sha == sha } } == true ? page : nil
        default: return nil
        }
    }

    var body: some View {
        let panel = store.sidePanel
        let scope = panel.scope(for: target)
        VStack(spacing: 0) {
            PanelBar {
                scopeMenu(scope)
                if let document = panel.diff.value, !document.files.isEmpty {
                    Text(AttributedString(LineCountText.text(added: document.added, removed: document.removed)))
                        .padding(.leading, 4)
                }
                if case .pullRequest = scope, let page = page(for: scope), store.pullRequestsExtended, !page.viewed.isEmpty {
                    let viewed = page.viewed.values.filter { $0 }.count
                    Text("\(viewed) of \(page.viewed.count) viewed")
                        .font(.ui(size: 12))
                        .foregroundStyle(Color.themeTertiary)
                        .padding(.leading, 6)
                        .help(page.canReviewLines ? "Click a line's number to comment on it" : "")
                }
                Spacer(minLength: 4)
                if let document = panel.diff.value, document.files.count > 1 {
                    let allClosed = panel.collapsed.count >= document.files.count
                    ActionButton(icon: allClosed ? .unfoldVertical : .foldVertical, help: allClosed ? "Open every file" : "Close every file") {
                        panel.setAllCollapsed(!allClosed)
                    }
                }
                ActionButton(icon: .rotateCw, help: "Read the changes again") { asked += 1 }
            }
            if !target.repository {
                NoRepositoryMessage(projectID: target.projectID)
            } else if target.awaitsWorktree {
                PanelMessage(text: "Nothing has changed.")
            } else {
                changes(panel.diff, scope: scope)
            }
        }
        .panelTask(id: PanelTrigger(target: target, scope: scope, version: store.workspaceVersion, asked: asked)) {
            guard target.repository, !target.awaitsWorktree else { return }
            // A pull request's diff shows which files were viewed and where its conversations are.
            if case .pullRequest(let number) = scope, store.pullRequestsExtended, panel.pullRequest.value?.number != number {
                panel.loadPullRequest(of: target, number: number)
            }
            panel.loadDiff(of: target, scope: scope)
        }
        .sheet(item: $commenting) { line in
            if let page = page(for: scope) {
                LineCommentSheet(target: target, page: page, commented: line)
                    .sheetSurface()
            }
        }
    }

    /// What the diff shows of the pull request besides its lines.
    private func marks(_ document: CodeDocument, scope: DiffScope) -> CodeMarks {
        guard case .pullRequest(let number) = scope, store.pullRequestsExtended, let page = page(for: scope) else { return CodeMarks() }
        var marks = CodeMarks(viewable: true, viewed: Set(page.viewed.filter(\.value).keys), commentable: page.canReviewLines)
        let pending = store.sidePanel.pendingComments[number] ?? []
        let places = page.threads.compactMap { thread in thread.line.map { (thread.path, $0, thread.side) } }
            + pending.map { ($0.path, $0.line, $0.side) }
        for (path, line, side) in places {
            guard let file = document.files.first(where: { $0.path == path }) else { continue }
            let index = (0..<file.lines.count).first { index in
                let removed = file.kind(index) == .removed
                return side == "left" ? removed && Int(file.old[index]) == line : !removed && Int(file.new[index]) == line
            }
            if let index { marks.marked[path, default: []].insert(index) }
        }
        return marks
    }

    @ViewBuilder
    private func changes(_ diff: Loaded<CodeDocument>, scope: DiffScope) -> some View {
        let panel = store.sidePanel
        switch diff {
        case .loading:
            PanelLoading()
        case .failed(let message):
            PanelMessage(text: message, failed: true)
        case .ready(let document) where document.files.isEmpty:
            PanelMessage(text: scope == .uncommitted ? "Everything is committed." : "Nothing has changed.")
        case .ready(let document):
            if document.truncated {
                PanelNote(text: "These changes are too long to show in full. This is their start.")
            }
            let marks = marks(document, scope: scope)
            CodeViewRepresentable(
                document: document, collapsed: panel.collapsed, reveal: panel.reveal, marks: marks,
                onToggle: { panel.toggleCollapsed($0) }, onOpenFile: { panel.open(.file($0)) },
                onViewed: { path in
                    guard case .pullRequest(let number) = scope else { return }
                    panel.setViewed(path, !marks.viewed.contains(path), on: target, number: number)
                },
                onComment: { place in comment(on: place, in: document) },
                onMedia: { panel.loadMedia(of: $0, in: target) }, onOpenMedia: { panel.viewMedia(of: $0) })
        }
    }

    private func comment(on place: CodeSheet.Place, in document: CodeDocument) {
        guard place.file < document.files.count else { return }
        let file = document.files[place.file]
        guard place.line < file.lines.count else { return }
        let removed = file.kind(place.line) == .removed
        let line = Int(removed ? file.old[place.line] : file.new[place.line])
        guard line > 0 else { return }
        commenting = CommentedLine(path: file.path, line: line, side: removed ? "left" : "right", code: file.lines[place.line])
    }

    private func scopeMenu(_ scope: DiffScope) -> some View {
        let panel = store.sidePanel
        return ActionMenu(title(of: scope)) {
            Toggle("Uncommitted changes", isOn: chosen(.uncommitted, scope))
            Toggle("Branch changes", isOn: chosen(.branch, scope))
            if let number = pullRequestNumber(scope), store.pullRequestsUnavailable == nil {
                Toggle("Pull request #\(number)", isOn: chosen(.pullRequest(number), scope))
                if let page = store.sidePanel.pullRequest.value, page.number == number {
                    let commits = page.activity.flatMap(\.commits).filter { !$0.sha.isEmpty }
                    if commits.count > 1 || scope == .commit(commits.first?.sha ?? "") {
                        Menu("Its Commits") {
                            ForEach(commits) { commit in
                                Toggle("\(commit.oid)  \(commit.headline)", isOn: chosen(.commit(commit.sha), scope))
                            }
                        }
                    }
                }
            }
            if !panel.turns.isEmpty {
                Divider()
                ForEach(panel.turns.reversed()) { turn in
                    Toggle(label(of: turn), isOn: chosen(.turn(turn.id), scope))
                }
            }
        }
        .padding(.leading, -8)
    }

    private func chosen(_ option: DiffScope, _ scope: DiffScope) -> Binding<Bool> {
        Binding { option == scope } set: { _ in store.sidePanel.choose(option) }
    }

    private func title(of scope: DiffScope) -> String {
        switch scope {
        case .uncommitted: return "Uncommitted"
        case .branch: return "Branch"
        case .pullRequest(let number): return "PR #\(number)"
        case .commit(let sha): return "Commit \(sha.prefix(7))"
        case .turn(let id): return store.sidePanel.name(ofTurn: id)
        }
    }

    /// The pull request the scope menu offers: the thread's, or the one the panel shows.
    private func pullRequestNumber(_ scope: DiffScope) -> Int? {
        if case .pullRequest(let number) = scope { return number }
        return target.pullRequest ?? store.sidePanel.pullRequest.value?.number
    }

    private func label(of turn: TurnChange) -> String {
        let name = store.sidePanel.name(ofTurn: turn.id)
        return "\(name) · \(turn.files == 1 ? "1 file" : "\(turn.files) files")"
    }
}

/// The folder the thread works in: its files under their folders, to open one in a tab.
struct FilesSurface: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget
    @State private var asked = 0

    var body: some View {
        let panel = store.sidePanel
        VStack(spacing: 0) {
            PanelBar {
                Image(.folder, size: 11)
                    .foregroundStyle(Color.themeSecondary)
                Text(target.name)
                    .font(.ui(size: 12.5, weight: .medium))
                    .foregroundStyle(Color.themeText)
                    .lineLimit(1)
                    .padding(.leading, 3)
                Spacer(minLength: 4)
                ActionButton(icon: .rotateCw, help: "Read the folder again") { asked += 1 }
            }
            if let error = panel.filesError {
                PanelMessage(text: error, failed: true)
            } else if let top = panel.listings[""] {
                if top.isEmpty {
                    PanelMessage(text: "This folder is empty.")
                } else {
                    ScrollView {
                        LazyVStack(spacing: 0) {
                            ForEach(panel.nodes) { node in
                                FileRow(node: node)
                            }
                        }
                        .padding(.vertical, 6)
                    }
                }
            } else {
                PanelLoading()
            }
        }
        .panelTask(id: PanelTrigger(target: target, version: store.workspaceVersion, asked: asked)) {
            panel.loadFiles(of: target)
        }
    }
}

private struct FileRow: View {
    @Environment(AppStore.self) private var store
    let node: FileNode

    var body: some View {
        let panel = store.sidePanel
        HStack(spacing: 6) {
            Image(node.open ? .chevronDown : .chevronRight, size: 8)
                .foregroundStyle(Color.themeTertiary)
                .frame(width: 10)
                .opacity(node.folder ? 1 : 0)
            Image(node.folder ? .folder : FileSymbol.symbol(for: node.path), size: 11)
                .foregroundStyle(Color.themeSecondary)
                .frame(width: 16)
            Text(node.name)
                .font(.ui(size: 12.5))
                .foregroundStyle(Color.themeText)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 0)
        }
        .opacity(node.ignored ? 0.5 : 1)
        .padding(.leading, 12 + CGFloat(node.depth) * 14)
        .padding(.trailing, 10)
        .frame(height: pressable(26))
        .button(.highlight(radius: 6, inset: EdgeInsets(top: 0, leading: 6, bottom: 0, trailing: 6))) {
            if node.folder { panel.toggleFolder(node.path) } else { panel.open(.file(node.path)) }
        }
        .contextMenu {
            Button("Copy Path") { Platform.copy(node.path) }
        }
    }
}

/// One file of the folder the thread works in.
struct FileSurface: View {
    @Environment(AppStore.self) private var store
    @Environment(\.panelInView) private var inView
    let target: PanelTarget
    let path: String
    @State private var asked = 0

    var body: some View {
        let panel = store.sidePanel
        VStack(spacing: 0) {
            PanelBar {
                (Text(folder).foregroundStyle(Color.themeSecondary) + Text((path as NSString).lastPathComponent).foregroundStyle(Color.themeText))
                    .font(.ui(size: 12.5, weight: .medium))
                    .lineLimit(1)
                    .truncationMode(.head)
                Spacer(minLength: 4)
                CopyButton(help: "Copy the path") { Platform.copy(path) }
                ActionButton(icon: .rotateCw, help: "Read the file again") { asked += 1 }
            }
            switch panel.contents[.file(path)] {
            case nil, .loading:
                PanelLoading()
            case .failed(let message):
                PanelMessage(text: message, failed: true)
            case .ready(.text(let document, let truncated)):
                if truncated {
                    PanelNote(text: "This file is too long to show in full. This is its start.")
                }
                if document.files.first?.lines.isEmpty == true {
                    PanelMessage(text: "This file is empty.")
                } else {
                    CodeViewRepresentable(document: document)
                }
            case .ready(.image(let image)):
                ZoomableImage(image: image, size: image.size, margin: CGSize(width: 16, height: 16), keys: store.viewing == nil && inView)
            case .ready(.video(let file)):
                FileVideo(file: file)
            case .ready(.binary(let size)):
                PanelMessage(text: "This file can't be shown here. It is \(ByteCountFormatter.string(fromByteCount: Int64(size), countStyle: .file)).")
            }
        }
        .panelTask(id: PanelTrigger(target: target, path: path, version: store.workspaceVersion, asked: asked)) {
            panel.loadFile(path, of: target)
        }
    }

    private var folder: String {
        let folder = (path as NSString).deletingLastPathComponent
        return folder.isEmpty ? "" : folder + "/"
    }
}

/// A video of the folder, which plays where it is.
private struct FileVideo: View {
    let file: URL
    @State private var player: AVPlayer?

    var body: some View {
        VideoPlayer(player: player)
            .padding(16)
            .task(id: file) { player = AVPlayer(url: file) }
            .onDisappear { player?.pause() }
    }
}

/// What one turn changed in one file.
struct ChangeSurface: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget
    let turn: String
    let path: String
    @State private var closed = false

    var body: some View {
        let panel = store.sidePanel
        VStack(spacing: 0) {
            PanelBar {
                Text(panel.name(ofTurn: turn))
                    .font(.ui(size: 12.5, weight: .medium))
                    .foregroundStyle(Color.themeText)
                    .lineLimit(1)
                Spacer(minLength: 4)
                ActionButton(icon: .diff, help: "Show everything this turn changed") {
                    panel.showDiff(.turn(turn), revealing: path)
                }
            }
            switch panel.contents[.change(turn: turn, path: path)] {
            case nil, .loading:
                PanelLoading()
            case .failed(let message):
                PanelMessage(text: message, failed: true)
            case .ready(.text(let document, let truncated)) where !document.files.isEmpty:
                if truncated {
                    PanelNote(text: "The turn's changes are too long to show in full, so this file's may be cut short.")
                }
                CodeViewRepresentable(
                    document: document, collapsed: closed ? [path] : [],
                    onToggle: { _ in closed.toggle() }, onOpenFile: { panel.open(.file($0)) },
                    onMedia: { panel.loadMedia(of: $0, in: target) }, onOpenMedia: { panel.viewMedia(of: $0) })
            case .ready:
                PanelMessage(text: "The turn's changes to this file can't be shown.")
            }
        }
        .panelTask(id: target) {
            panel.loadChange(turn: turn, path: path, of: target)
        }
    }
}
