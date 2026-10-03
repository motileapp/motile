import AppKit
import Observation

/// Which changes a diff shows.
enum DiffScope: Hashable, Codable {
    /// What isn't committed.
    case uncommitted
    /// Everything since the branch left the one it started from.
    case branch
    /// What the turn that ended with the item changed.
    case turn(String)

    var request: JSON {
        switch self {
        case .uncommitted: ["kind": "uncommitted"]
        case .branch: ["kind": "branch"]
        case .turn(let itemID): ["kind": "turn", "item_id": itemID]
        }
    }
}

/// A tab of the panel beside the thread.
enum PanelTab: Hashable, Codable, Identifiable {
    case diff
    /// The folder's files, to open one.
    case files
    /// One file, by its path in the folder.
    case file(String)

    var id: String {
        switch self {
        case .diff: "diff"
        case .files: "files"
        case .file(let path): "file:\(path)"
        }
    }

    var title: String {
        switch self {
        case .diff: "Diff"
        case .files: "Files"
        case .file(let path): URL(fileURLWithPath: path).lastPathComponent
        }
    }

    var symbol: String {
        switch self {
        case .diff: "plusminus"
        case .files: "folder"
        case .file(let path): FileSymbol.name(for: path)
        }
    }
}

/// The tabs a thread has open in the panel.
struct PanelTabs: Equatable, Codable {
    var tabs: [PanelTab] = []
    var active: PanelTab?
    /// What the diff tab shows, once that was chosen.
    var scope: DiffScope?
    /// The panel covers the thread.
    var maximized: Bool?
}

/// The folder the panel looks into: the one the open thread works in, or the project's when a
/// thread is about to start there.
struct PanelTarget: Equatable {
    /// What its tabs are kept under: the thread, or the draft.
    let key: String
    let serverID: String
    let projectID: String
    let threadID: String?
    let name: String
    let repository: Bool
    /// The thread works in a worktree of its own.
    let worktree: Bool

    /// Names the folder in a request to its server.
    var request: JSON {
        var request: JSON = ["project_id": projectID]
        if let threadID { request["thread_id"] = threadID }
        return request
    }
}

/// A turn of the open thread that changed files.
struct TurnChange: Equatable, Identifiable {
    /// The item that ended the turn.
    let id: String
    let at: Double
    let files: Int
}

enum Loaded<Value> {
    case loading
    case ready(Value)
    case failed(String)

    var value: Value? {
        if case .ready(let value) = self { return value }
        return nil
    }
}

struct FileEntry: Equatable {
    let name: String
    let folder: Bool
    let ignored: Bool
}

/// A row of the files tab: a file or a folder, as deep as the folders above it.
struct FileNode: Identifiable, Equatable {
    let path: String
    let name: String
    let folder: Bool
    let ignored: Bool
    let depth: Int
    let open: Bool

    var id: String { path }
}

enum FileContent {
    case text(CodeDocument, truncated: Bool)
    case image(NSImage)
    case binary(size: Int)
}

extension Notification.Name {
    /// A document's highlighting has arrived; the object is the `CodeDocument`.
    static let codeColoured = Notification.Name("motile.codeColoured")
}

/// The panel beside the thread: whether it is open, the tabs each thread has in it, and what the
/// tabs of the open thread show. All of it is used on the main thread.
@Observable
final class SidePanel {
    static let widths: ClosedRange<Double> = 340...900

    @ObservationIgnored weak var store: AppStore?
    @ObservationIgnored private let defaults = UserDefaults.standard

    var isOpen: Bool {
        didSet {
            defaults.set(isOpen, forKey: "panel.open")
            if !isOpen { change { $0.maximized = nil } }
        }
    }
    private(set) var tabsByKey: [String: PanelTabs]
    /// The turns of the open thread that changed files, the first one first.
    var turns: [TurnChange] = []

    private(set) var diff: Loaded<CodeDocument> = .loading
    /// The files of the diff that are closed, by path.
    private(set) var collapsed: Set<String> = []
    /// The file of the diff to bring into view, and a count that goes up with every request.
    private(set) var reveal: (path: String, count: Int)?

    /// What each folder that was looked into has in it, by its path.
    private(set) var listings: [String: [FileEntry]] = [:]
    private(set) var openFolders: Set<String> = []
    private(set) var filesError: String?
    /// The files that are open in tabs, by path.
    private(set) var contents: [String: Loaded<FileContent>] = [:]

    /// The folder all of the above is of.
    @ObservationIgnored private var shown: PanelTarget?
    @ObservationIgnored private var shownScope: DiffScope?
    @ObservationIgnored private var diffRequest: UInt64 = 0
    @ObservationIgnored private var fileRequests: [UInt64: String] = [:]

    init() {
        isOpen = defaults.bool(forKey: "panel.open")
        let saved = defaults.data(forKey: "panel.tabs").flatMap { try? JSONDecoder().decode([String: PanelTabs].self, from: $0) }
        tabsByKey = saved ?? [:]
    }

    // MARK: Tabs

    private var key: String? { store?.panelTarget?.key }

    var tabs: PanelTabs { key.flatMap { tabsByKey[$0] } ?? PanelTabs() }

    private func change(_ change: (inout PanelTabs) -> Void) {
        guard let key else { return }
        var tabs = tabsByKey[key] ?? PanelTabs()
        change(&tabs)
        tabsByKey[key] = tabs.tabs.isEmpty && tabs.scope == nil && tabs.maximized == nil ? nil : tabs
        defaults.set(try? JSONEncoder().encode(tabsByKey), forKey: "panel.tabs")
    }

    /// Shows the tab, opening it and the panel when they aren't.
    func open(_ tab: PanelTab) {
        change { tabs in
            if !tabs.tabs.contains(tab) { tabs.tabs.append(tab) }
            tabs.active = tab
        }
        isOpen = true
    }

    func activate(_ tab: PanelTab) {
        change { $0.active = tab }
    }

    /// Closes the tab. The one beside it is shown in its place.
    func close(_ tab: PanelTab) {
        change { tabs in
            guard let index = tabs.tabs.firstIndex(of: tab) else { return }
            tabs.tabs.remove(at: index)
            guard tabs.active == tab else { return }
            tabs.active = tabs.tabs.isEmpty ? nil : tabs.tabs[min(index, tabs.tabs.count - 1)]
        }
        if case .file(let path) = tab { contents[path] = nil }
    }

    func closeOthers(_ tab: PanelTab) {
        change { tabs in
            tabs.tabs = [tab]
            tabs.active = tab
        }
    }

    func closeAll() {
        change { tabs in
            tabs.tabs = []
            tabs.active = nil
        }
    }

    /// What ⌘W does while the panel shows a tab. `false` when there is none to close.
    func closeActive() -> Bool {
        guard isOpen, let active = tabs.active else { return false }
        close(active)
        return true
    }

    /// The panel covers the thread, so the window shows the sidebar and the panel.
    var isMaximized: Bool { isOpen && tabs.maximized == true }

    func toggleMaximized() {
        guard isOpen else { return }
        let maximized = !isMaximized
        change { $0.maximized = maximized ? true : nil }
        // The composer is behind the panel now, and must not take what is typed.
        if maximized { NSApp.keyWindow?.makeFirstResponder(nil) }
    }

    /// The tabs a draft had go to the thread it became.
    func move(from draftKey: String, to threadKey: String) {
        guard let tabs = tabsByKey.removeValue(forKey: draftKey) else { return }
        tabsByKey[threadKey] = tabs
        if shown?.key == draftKey { shown = nil }
    }

    func forget(_ key: String) {
        tabsByKey[key] = nil
    }

    // MARK: Diff

    /// What the diff tab shows: what was chosen, or the turn's work as far as it is known.
    func scope(for target: PanelTarget) -> DiffScope {
        if let chosen = tabs.scope {
            guard case .turn(let itemID) = chosen else { return chosen }
            if turns.contains(where: { $0.id == itemID }) { return chosen }
        }
        return target.worktree ? .branch : .uncommitted
    }

    func choose(_ scope: DiffScope) {
        change { $0.scope = scope }
    }

    /// Opens the diff tab on `scope`, with the file at `path` in view.
    func showDiff(_ scope: DiffScope? = nil, revealing path: String? = nil) {
        if let scope { choose(scope) }
        if let path { reveal = (path, (reveal?.count ?? 0) + 1) }
        open(.diff)
    }

    /// Asks the server for the diff. What is shown stays until the answer is there, unless it
    /// is of another folder or scope.
    func loadDiff(of target: PanelTarget, scope: DiffScope) {
        look(into: target)
        if shownScope != scope {
            shownScope = scope
            diff = .loading
            collapsed = []
        }
        var command = target.request
        command["server_id"] = target.serverID
        command["scope"] = scope.request
        let id = "\(target.key)/\(scope)"
        let fresh = diff.value?.id != id
        diffRequest = store?.core.send("diff", command, read: { CodeDocument(diff: $0, id: id) }) { [weak self] result in
            guard let self, self.shown == target, self.shownScope == scope else { return }
            switch result {
            case .success(let document):
                if fresh { self.collapsed = Set(document.files.filter { $0.lines.count > CodeFile.openUpToLines }.map(\.path)) }
                self.diff = .ready(document)
            case .failure(let error):
                self.diff = .failed(error.message)
            }
        } ?? 0
    }

    func toggleCollapsed(_ path: String) {
        if collapsed.remove(path) == nil { collapsed.insert(path) }
    }

    func setAllCollapsed(_ closed: Bool) {
        collapsed = closed ? Set(diff.value?.files.map(\.path) ?? []) : []
    }

    // MARK: Files

    /// The rows of the files tab: every folder that is open with what is in it.
    var nodes: [FileNode] {
        var nodes: [FileNode] = []
        func list(_ folder: String, depth: Int) {
            for entry in listings[folder] ?? [] {
                let path = folder.isEmpty ? entry.name : "\(folder)/\(entry.name)"
                let open = entry.folder && openFolders.contains(path)
                nodes.append(FileNode(path: path, name: entry.name, folder: entry.folder, ignored: entry.ignored, depth: depth, open: open))
                if open { list(path, depth: depth + 1) }
            }
        }
        list("", depth: 0)
        return nodes
    }

    /// Reads the folders that have been looked into again, the folder itself first.
    func loadFiles(of target: PanelTarget) {
        look(into: target)
        for folder in Set(listings.keys).union([""]) { list(folder, of: target) }
    }

    func toggleFolder(_ path: String) {
        guard openFolders.remove(path) == nil else { return }
        openFolders.insert(path)
        guard listings[path] == nil, let shown else { return }
        list(path, of: shown)
    }

    private func list(_ folder: String, of target: PanelTarget) {
        var request = target.request
        request["type"] = "list_files"
        request["path"] = folder
        store?.core.send("request", ["server_id": target.serverID, "request": request]) { [weak self] result in
            guard let self, self.shown == target else { return }
            switch result {
            case .success(let answer):
                let entries = answer.objects("entries").map {
                    FileEntry(name: $0.string("name"), folder: $0.bool("folder"), ignored: $0.bool("ignored"))
                }
                if self.listings[folder] != entries { self.listings[folder] = entries }
                if folder.isEmpty { self.filesError = nil }
            case .failure(let error):
                // A folder that has gone closes; only the folder itself says what went wrong.
                self.listings[folder] = nil
                self.openFolders.remove(folder)
                if folder.isEmpty { self.filesError = error.message }
            }
        }
    }

    /// Asks the server for the file. What is shown of it stays until the answer is there.
    func loadFile(_ path: String, of target: PanelTarget) {
        look(into: target)
        if contents[path] == nil { contents[path] = .loading }
        var command = target.request
        command["server_id"] = target.serverID
        command["path"] = path
        let id = "\(target.key)/\(path)"
        let request = store?.core.send("file", command, read: { Self.read(file: $0, path: path, id: id) }) { [weak self] result in
            guard let self, self.shown == target else { return }
            switch result {
            case .success(let content): self.contents[path] = .ready(content)
            case .failure(let error): self.contents[path] = .failed(error.message)
            }
        }
        fileRequests = fileRequests.filter { $0.value != path }
        if let request { fileRequests[request] = path }
    }

    private static func read(file answer: JSON, path: String, id: String) -> FileContent {
        switch answer.string("kind") {
        case "text":
            let file = CodeFile(path: path, lines: answer.strings("lines"))
            return .text(CodeDocument(id: id, files: [file], truncated: false, headed: false), truncated: answer.bool("truncated"))
        case "image":
            let image = (try? Data(contentsOf: URL(fileURLWithPath: answer.string("file")))).flatMap(NSImage.init(data:))
            return image.map { .image($0) } ?? .binary(size: answer.int("size"))
        default:
            return .binary(size: answer.int("size"))
        }
    }

    // MARK: Both

    /// Forgets what was shown when the folder is another one than before.
    private func look(into target: PanelTarget) {
        guard shown != target else { return }
        let sameFolder = shown?.key == target.key
        shown = target
        guard !sameFolder else { return }
        shownScope = nil
        diff = .loading
        collapsed = []
        listings = [:]
        openFolders = []
        filesError = nil
        contents = [:]
        fileRequests = [:]
    }

    /// The highlighting of what a request answered with has arrived.
    func colour(request: UInt64, file: Int, lines: [[Int32]]) {
        let document: CodeDocument?
        if request == diffRequest {
            document = diff.value
        } else if let path = fileRequests[request], case .text(let text, _) = contents[path]?.value {
            document = text
        } else {
            document = nil
        }
        guard let document, file < document.files.count, document.files[file].lines.count == lines.count else { return }
        document.files[file].spans = lines
        NotificationCenter.default.post(name: .codeColoured, object: document, userInfo: ["file": file])
    }
}

/// The symbol a file is shown with, by what its name ends in.
enum FileSymbol {
    static func name(for path: String) -> String {
        switch (path as NSString).pathExtension.lowercased() {
        case "png", "jpg", "jpeg", "gif", "webp", "heic", "bmp", "tiff", "ico", "svg": "photo"
        case "md", "markdown", "txt", "rst": "doc.text"
        case "json", "yaml", "yml", "toml", "xml", "plist", "lock": "curlybraces"
        case "sh", "bash", "zsh", "fish": "terminal"
        case "": "doc"
        default: "chevron.left.forwardslash.chevron.right"
        }
    }
}
