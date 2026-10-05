import Foundation
import Observation

/// Which changes a diff shows.
enum DiffScope: Hashable, Codable {
    /// What isn't committed.
    case uncommitted
    /// Everything since the branch left the one it started from.
    case branch
    /// What the turn that ended with the item changed.
    case turn(String)
    /// What the pull request with the number changes, as GitHub has it.
    case pullRequest(Int)
    /// What one commit changed, by its whole name.
    case commit(String)

    var request: JSON {
        switch self {
        case .uncommitted: ["kind": "uncommitted"]
        case .branch: ["kind": "branch"]
        case .turn(let itemID): ["kind": "turn", "item_id": itemID]
        case .pullRequest(let number): ["kind": "pull_request", "number": number]
        case .commit(let sha): ["kind": "commit", "sha": sha]
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
    /// What one turn changed in one file, by the item that ended the turn.
    case change(turn: String, path: String)
    /// The agents the thread's agent has started, and what one of them did.
    case agents
    /// The pull request of the branch the thread works on.
    case pullRequest
    /// Another pull request of the repository, by its number.
    case pullRequestNumber(Int)
    /// The repository's pull requests.
    case pullRequests
    /// A tab that offers what there is to open. A thread can have several, told apart by number.
    case blank(Int)

    var id: String {
        switch self {
        case .blank(let number): "blank:\(number)"
        case .diff: "diff"
        case .files: "files"
        case .agents: "agents"
        case .pullRequest: "pull_request"
        case .pullRequestNumber(let number): "pull_request:\(number)"
        case .pullRequests: "pull_requests"
        case .file(let path): "file:\(path)"
        case .change(_, let path): "change:\(path)"
        }
    }

    var blankNumber: Int? {
        guard case .blank(let number) = self else { return nil }
        return number
    }

    /// The file a tab is about.
    var path: String? {
        switch self {
        case .file(let path), .change(_, let path): path
        case .diff, .files, .agents, .pullRequest, .pullRequestNumber, .pullRequests, .blank: nil
        }
    }

    var title: String {
        switch self {
        case .diff: "Diff"
        case .files: "Files"
        case .agents: "Agents"
        case .pullRequest: "Pull Request"
        case .pullRequestNumber(let number): "PR #\(number)"
        case .pullRequests: "Pull Requests"
        case .blank: "New Tab"
        case .file(let path), .change(_, let path): URL(fileURLWithPath: path).lastPathComponent
        }
    }

    var symbol: Symbol {
        switch self {
        case .diff, .change: .diff
        case .files: .folder
        case .agents: .users
        case .pullRequest, .pullRequestNumber, .pullRequests: .gitPullRequest
        case .blank: .plus
        case .file(let path): FileSymbol.symbol(for: path)
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

    /// All there is is a blank tab, as before one was opened.
    var isBlank: Bool { tabs.count == 1 && tabs[0].blankNumber != nil }
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
    /// The number of the pull request of the branch it works on, when there is one.
    let pullRequest: Int?

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
    case image(PlatformImage)
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
    /// The agent whose transcript the agents tab shows. Without one it lists them.
    private(set) var shownAgent: String?

    private(set) var diff: Loaded<CodeDocument> = .loading
    /// The files of the diff that are closed, by path.
    private(set) var collapsed: Set<String> = []
    /// The file of the diff to bring into view, and a count that goes up with every request.
    private(set) var reveal: (path: String, count: Int)?

    /// What each folder that was looked into has in it, by its path.
    private(set) var listings: [String: [FileEntry]] = [:]
    private(set) var openFolders: Set<String> = []
    private(set) var filesError: String?
    /// What the tabs of one file show: the file, or what a turn changed in it.
    private(set) var contents: [PanelTab: Loaded<FileContent>] = [:]

    private(set) var pullRequest: Loaded<PullRequestPage> = .loading
    /// The action on the pull request that runs.
    private(set) var pullRequestWorking: PullRequestWork?
    /// The repository's pull requests, as the list tab shows them.
    private(set) var pullRequestList: Loaded<[PullRequestRow]> = .loading
    /// Comments on lines waiting to be sent with a review, by pull request.
    private(set) var pendingComments: [Int: [PendingLineComment]] = [:]
    /// What the last action on the pull request did, or why it couldn't.
    private(set) var pullRequestNotice: PullRequestNotice?
    /// Goes up each time the pull request has been read.
    private(set) var pullRequestReads = 0

    /// The folder all of the above is of.
    @ObservationIgnored private var shown: PanelTarget?
    @ObservationIgnored private var shownScope: DiffScope?
    @ObservationIgnored private var diffRequest: UInt64 = 0
    @ObservationIgnored private var fileRequests: [UInt64: PanelTab] = [:]
    @ObservationIgnored private var shownPullRequest: Int?
    /// How each project's pull requests were last merged, by project.
    @ObservationIgnored private var mergeMethods: [String: String]

    init() {
        isOpen = defaults.bool(forKey: "panel.open")
        let saved = defaults.data(forKey: "panel.tabs").flatMap { try? JSONDecoder().decode([String: PanelTabs].self, from: $0) }
        tabsByKey = saved ?? [:]
        mergeMethods = defaults.dictionary(forKey: "pullRequest.methods") as? [String: String] ?? [:]
    }

    // MARK: Tabs

    private var key: String? { store?.panelTarget?.key }

    /// The thread's tabs. There is always one: a blank one when none was opened.
    var tabs: PanelTabs {
        var tabs = key.flatMap { tabsByKey[$0] } ?? PanelTabs()
        if tabs.tabs.isEmpty { tabs.tabs = [.blank(0)] }
        if tabs.active == nil { tabs.active = tabs.tabs.first }
        return tabs
    }

    private func change(_ change: (inout PanelTabs) -> Void) {
        guard let key else { return }
        var tabs = self.tabs
        change(&tabs)
        let untouched = tabs.isBlank && tabs.scope == nil && tabs.maximized == nil
        tabsByKey[key] = untouched ? nil : tabs
        defaults.set(try? JSONEncoder().encode(tabsByKey), forKey: "panel.tabs")
    }

    /// Shows the tab, opening it and the panel when they aren't. It takes the place of the blank
    /// tab it is opened from.
    func open(_ tab: PanelTab) {
        change { tabs in
            let blank = tabs.tabs.firstIndex { $0 == tabs.active && $0.blankNumber != nil }
            if tabs.tabs.contains(tab) {
                if let blank { tabs.tabs.remove(at: blank) }
            } else if let blank {
                tabs.tabs[blank] = tab
            } else {
                tabs.tabs.append(tab)
            }
            tabs.active = tab
        }
        isOpen = true
    }

    /// Adds a blank tab. A hidden panel that only has its blank tab just opens.
    func openBlank() {
        guard isOpen || !tabs.isBlank else {
            isOpen = true
            return
        }
        change { tabs in
            let tab = PanelTab.blank((tabs.tabs.compactMap(\.blankNumber).max() ?? -1) + 1)
            tabs.tabs.append(tab)
            tabs.active = tab
        }
        isOpen = true
    }

    func activate(_ tab: PanelTab) {
        change { $0.active = tab }
    }

    /// Shows the tab `offset` places from the active one, wrapping around the ends.
    func activate(offset: Int) {
        let tabs = tabs
        guard isOpen, tabs.tabs.count > 1, let active = tabs.active, let index = tabs.tabs.firstIndex(of: active) else { return }
        let count = tabs.tabs.count
        activate(tabs.tabs[((index + offset) % count + count) % count])
    }

    /// Closes the tab. The one beside it is shown in its place.
    func close(_ tab: PanelTab) {
        change { tabs in
            guard let index = tabs.tabs.firstIndex(of: tab) else { return }
            tabs.tabs.remove(at: index)
            guard tabs.active == tab else { return }
            tabs.active = tabs.tabs.isEmpty ? nil : tabs.tabs[min(index, tabs.tabs.count - 1)]
        }
        contents[tab] = nil
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

    /// What ⌘W does while the panel shows a tab. `false` when all it has is a blank one.
    func closeActive() -> Bool {
        let tabs = tabs
        guard isOpen, !tabs.isBlank, let active = tabs.active else { return false }
        close(active)
        return true
    }

    /// The panel covers the thread, so the window shows the sidebar and the panel.
    var isMaximized: Bool { isOpen && tabs.maximized == true }

    /// A panel that has no folder to show, like a draft's whose worktree isn't made yet, stays beside the thread.
    var canMaximize: Bool { isOpen && key != nil }

    func toggleMaximized() {
        guard canMaximize else { return }
        let maximized = !isMaximized
        change { $0.maximized = maximized ? true : nil }
        // The composer is behind the panel now, and must not take what is typed.
        if maximized { Platform.endEditing() }
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

    // MARK: Agents

    /// Opens the agents tab on what the agent did that the tool call started.
    func showAgent(_ id: String) {
        guard let store, let threadID = store.transcript.threadID else { return }
        shownAgent = id
        store.agentTranscript.begin(threadID: threadID, live: store.transcript.live)
        agentsChanged()
        store.core.send("open_agent", ["thread_id": threadID, "agent_id": id])
        open(.agents)
    }

    /// Goes back to the list of agents.
    func showAgents() {
        guard let store, shownAgent != nil else { return }
        shownAgent = nil
        if let threadID = store.agentTranscript.threadID { store.core.send("close_agent", ["thread_id": threadID]) }
        store.agentTranscript.begin(threadID: nil)
    }

    /// The shown agent's transcript says that it works for as long as it does.
    func agentsChanged() {
        guard let store, let agent = store.agents.first(where: { $0.id == shownAgent }) else { return }
        var activity = Activity()
        activity.running = agent.working
        activity.startedAt = agent.startedAt
        if store.agentTranscript.activity != activity { store.agentTranscript.setActivity(activity) }
    }

    // MARK: Diff

    /// What the diff tab shows: what was chosen, or the turn's work as far as it is known.
    func scope(for target: PanelTarget) -> DiffScope {
        if let chosen = tabs.scope, canShow(chosen, of: target) { return chosen }
        return target.worktree ? .branch : .uncommitted
    }

    /// A turn that is no longer known, or a pull request the folder no longer has, isn't shown.
    private func canShow(_ scope: DiffScope, of target: PanelTarget) -> Bool {
        switch scope {
        case .turn(let itemID): turns.contains { $0.id == itemID }
        case .pullRequest(let number): number == target.pullRequest || number == pullRequest.value?.number
        case .commit: true
        case .uncommitted, .branch: true
        }
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

    /// Opens what the turn changed in the file in a tab of its own. A file has one such tab,
    /// which shows the turn that was asked for last.
    func showChange(turn: String, path: String) {
        let tab = PanelTab.change(turn: turn, path: path)
        change { tabs in
            guard let index = tabs.tabs.firstIndex(where: { $0.id == tab.id }), tabs.tabs[index] != tab else { return }
            contents[tabs.tabs[index]] = nil
            tabs.tabs[index] = tab
        }
        open(tab)
    }

    /// What the diff's menu and a change's tab call the turn.
    func name(ofTurn id: String) -> String {
        guard let turn = turns.first(where: { $0.id == id }) else { return "Earlier turn" }
        return turn.id == turns.last?.id ? "Latest turn" : "Turn at \(Time.stamp(turn.at))"
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
                if fresh {
                    let long = document.files.filter { $0.lines.count > CodeFile.openUpToLines }.map(\.path)
                    self.collapsed = Set(long).union(self.viewedPaths(for: scope))
                }
                self.diff = .ready(document)
            case .failure(let error):
                self.diff = .failed(error.message)
            }
        } ?? 0
    }

    /// The files of the pull request the user has marked as viewed, which its diff shows closed.
    private func viewedPaths(for scope: DiffScope) -> [String] {
        guard case .pullRequest(let number) = scope, let page = pullRequest.value, page.number == number else { return [] }
        return page.viewed.filter(\.value).map(\.key)
    }

    /// Marks the file viewed, or not, and closes or opens it in the diff with that.
    func setViewed(_ path: String, _ viewed: Bool, on target: PanelTarget, number: Int) {
        if viewed { collapsed.insert(path) } else { collapsed.remove(path) }
        edit(["kind": "viewed", "path": path, "viewed": viewed], on: target, number: number)
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
        let id = "\(target.key)/\(path)"
        load(.file(path), "file", ["path": path], of: target) { Self.read(file: $0, path: path, id: id) }
    }

    /// Asks the server for what the turn changed in the file.
    func loadChange(turn: String, path: String, of target: PanelTarget) {
        let tab = PanelTab.change(turn: turn, path: path)
        let id = "\(target.key)/\(turn)/\(path)"
        load(tab, "diff", ["scope": DiffScope.turn(turn).request, "path": path], of: target) {
            .text(CodeDocument(diff: $0, id: id), truncated: $0.bool("truncated"))
        }
    }

    private func load(_ tab: PanelTab, _ type: String, _ fields: JSON, of target: PanelTarget, read: @escaping (JSON) -> FileContent) {
        look(into: target)
        if contents[tab] == nil { contents[tab] = .loading }
        var command = target.request.merging(fields) { $1 }
        command["server_id"] = target.serverID
        let request = store?.core.send(type, command, read: read) { [weak self] result in
            guard let self, self.shown == target else { return }
            switch result {
            case .success(let content): self.contents[tab] = .ready(content)
            case .failure(let error): self.contents[tab] = .failed(error.message)
            }
        }
        fileRequests = fileRequests.filter { $0.value != tab }
        if let request { fileRequests[request] = tab }
    }

    private static func read(file answer: JSON, path: String, id: String) -> FileContent {
        switch answer.string("kind") {
        case "text":
            let file = CodeFile(path: path, lines: answer.strings("lines"))
            return .text(CodeDocument(id: id, files: [file], truncated: false, headed: false), truncated: answer.bool("truncated"))
        case "image":
            let image = (try? Data(contentsOf: URL(fileURLWithPath: answer.string("file")))).flatMap(PlatformImage.decoded)
            return image.map { .image($0) } ?? .binary(size: answer.int("size"))
        default:
            return .binary(size: answer.int("size"))
        }
    }

    // MARK: Pull request

    /// Asks the server for the pull request. What is shown of it stays until the answer is there,
    /// unless it is another one.
    func loadPullRequest(of target: PanelTarget, number: Int) {
        look(into: target)
        if shownPullRequest != number {
            shownPullRequest = number
            pullRequest = .loading
            pullRequestNotice = nil
        }
        var command = target.request
        command["server_id"] = target.serverID
        command["number"] = number
        if let method = mergeMethods[target.projectID] { command["method"] = method }
        store?.core.send("pull_request", command, read: PullRequestPage.init) { [weak self] result in
            guard let self, self.shown == target, self.shownPullRequest == number else { return }
            switch result {
            case .success(let page): self.pullRequest = .ready(page)
            // A pull request that was read stays; only its notice says what went wrong.
            case .failure(let error) where self.pullRequest.value != nil:
                self.pullRequestNotice = PullRequestNotice(text: error.message, failed: true)
            case .failure(let error): self.pullRequest = .failed(error.message)
            }
            self.pullRequestReads += 1
        }
    }

    /// Does something to the pull request: an action of a button or a choice, with what the
    /// comment box says. `key` names the button it came from, which shows `label` meanwhile.
    /// `done` is called when it worked.
    func act(
        _ action: String, method: String? = nil, text: String? = nil, key: String, label: String, on target: PanelTarget,
        number: Int, done: (() -> Void)? = nil
    ) {
        guard pullRequestWorking == nil else { return }
        pullRequestWorking = PullRequestWork(key: key, label: label)
        pullRequestNotice = nil
        if action == "merge" || action == "enable_auto_merge", let method { remember(method, for: target.projectID) }
        var command = target.request
        command["server_id"] = target.serverID
        command["number"] = number
        command["action"] = action
        if let method { command["method"] = method }
        if let text, !text.isEmpty { command["text"] = text }
        store?.core.send("pull_request_action", command, read: { answer in
            (answer.string("title"), answer.optionalString("url"), PullRequestPage(json: answer.object("view") ?? [:]))
        }) { [weak self] result in
            guard let self else { return }
            self.pullRequestWorking = nil
            guard self.shown == target, self.shownPullRequest == number else { return }
            switch result {
            case .success(let (title, url, page)):
                self.pullRequest = .ready(page)
                self.pullRequestReads += 1
                self.say(PullRequestNotice(text: title, failed: false, url: url.flatMap(URL.init(string:))))
                done?()
            case .failure(let error):
                self.say(PullRequestNotice(text: error.message, failed: true))
            }
        }
    }

    /// Changes the pull request as `edit` says, the protocol's `PullRequestEdit` as JSON. Without a
    /// `key` it goes quietly, like a reaction, and the page is only read again.
    func edit(
        _ edit: JSON, key: String? = nil, label: String = "", on target: PanelTarget, number: Int,
        done: (() -> Void)? = nil
    ) {
        if let key {
            guard pullRequestWorking == nil else { return }
            pullRequestWorking = PullRequestWork(key: key, label: label)
            pullRequestNotice = nil
        }
        var command = target.request
        command["server_id"] = target.serverID
        command["number"] = number
        command["edit"] = edit
        if let method = mergeMethods[target.projectID] { command["method"] = method }
        store?.core.send("pull_request_edit", command, read: { answer in
            (answer.string("title"), PullRequestPage(json: answer.object("view") ?? [:]))
        }) { [weak self] result in
            guard let self else { return }
            if key != nil { self.pullRequestWorking = nil }
            guard self.shown == target, self.shownPullRequest == number else { return }
            switch result {
            case .success(let (title, page)):
                self.pullRequest = .ready(page)
                self.pullRequestReads += 1
                if !title.isEmpty { self.say(PullRequestNotice(text: title, failed: false)) }
                done?()
            case .failure(let error):
                self.say(PullRequestNotice(text: error.message, failed: true))
            }
        }
    }

    /// Keeps a comment on a line for the next review of the pull request.
    func addPending(_ comment: PendingLineComment, to number: Int) {
        pendingComments[number, default: []].append(comment)
    }

    func removePending(_ comment: PendingLineComment, from number: Int) {
        pendingComments[number]?.removeAll { $0.id == comment.id }
    }

    /// Sends a review with the comments kept for it, and forgets them once it is there.
    func review(_ verdict: String, body: String, key: String, label: String, on target: PanelTarget, number: Int, done: (() -> Void)? = nil) {
        let pending = pendingComments[number] ?? []
        let comments: [JSON] = pending.map { ["path": $0.path, "line": $0.line, "side": $0.side, "body": $0.body] }
        let review: JSON = ["kind": "review", "verdict": verdict, "body": body, "comments": comments]
        edit(review, key: key, label: label, on: target, number: number) { [weak self] in
            self?.pendingComments[number] = nil
            done?()
        }
    }

    /// Asks the server for the repository's pull requests in that state: "open", "closed",
    /// "merged" or "all".
    func loadPullRequests(of target: PanelTarget, state: String) {
        look(into: target)
        var command = target.request
        command["server_id"] = target.serverID
        command["state"] = state
        store?.core.send("pull_requests", command, read: { $0.objects("rows").map(PullRequestRow.init) }) { [weak self] result in
            guard let self, self.shown == target else { return }
            switch result {
            case .success(let rows): self.pullRequestList = .ready(rows)
            case .failure(let error) where self.pullRequestList.value == nil: self.pullRequestList = .failed(error.message)
            case .failure(let error): self.say(PullRequestNotice(text: error.message, failed: true))
            }
        }
    }

    /// Opens the pull request in a tab: the thread's own tab when it is the thread's.
    func showPullRequest(_ number: Int, of target: PanelTarget) {
        open(number == target.pullRequest ? .pullRequest : .pullRequestNumber(number))
    }

    /// Makes the pull request the thread's own, or with `nil` takes its own away.
    func link(_ number: Int?, thread threadID: String, serverID: String) {
        var request: JSON = ["type": "link_pull_request", "thread_id": threadID]
        if let number { request["number"] = number }
        store?.core.send("request", ["server_id": serverID, "request": request]) { [weak self] result in
            switch result {
            case .success:
                let said = number.map { "Linked PR #\($0) to this thread" } ?? "Unlinked the pull request from this thread"
                self?.say(PullRequestNotice(text: said, failed: false))
            case .failure(let error): self?.say(PullRequestNotice(text: error.message, failed: true))
            }
        }
    }

    /// Has the thread's agent told what happens on its pull request, or stops it.
    func watch(_ on: Bool, thread threadID: String, serverID: String) {
        let request: JSON = ["type": "watch_pull_request", "thread_id": threadID, "watch": on]
        store?.core.send("request", ["server_id": serverID, "request": request]) { [weak self] result in
            switch result {
            case .success:
                let said = on
                    ? "Watching: the agent hears when its checks finish, someone comments or it conflicts"
                    : "No longer watching"
                self?.say(PullRequestNotice(text: said, failed: false))
            case .failure(let error): self?.say(PullRequestNotice(text: error.message, failed: true))
            }
        }
    }

    /// How the project's pull requests merge from now on.
    func chooseMethod(_ method: String, for target: PanelTarget, number: Int) {
        remember(method, for: target.projectID)
        loadPullRequest(of: target, number: number)
    }

    private func remember(_ method: String, for projectID: String) {
        mergeMethods[projectID] = method
        defaults.set(mergeMethods, forKey: "pullRequest.methods")
    }

    /// What worked goes away by itself; what failed stays until it is closed.
    private func say(_ notice: PullRequestNotice) {
        pullRequestNotice = notice
        guard !notice.failed else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 8) { [weak self] in
            if self?.pullRequestNotice == notice { self?.pullRequestNotice = nil }
        }
    }

    func dismissPullRequestNotice() {
        pullRequestNotice = nil
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
        shownPullRequest = nil
        pullRequest = .loading
        pullRequestNotice = nil
        pullRequestList = .loading
    }

    /// The highlighting of what a request answered with has arrived.
    func colour(request: UInt64, file: Int, lines: [[Int32]]) {
        let document: CodeDocument?
        if request == diffRequest {
            document = diff.value
        } else if let tab = fileRequests[request], case .text(let text, _) = contents[tab]?.value {
            document = text
        } else {
            document = nil
        }
        guard let document, file < document.files.count, document.files[file].lines.count == lines.count else { return }
        document.files[file].spans = lines
        NotificationCenter.default.post(name: .codeColoured, object: document, userInfo: ["file": file])
    }
}

/// The action on a pull request that runs: `key` names the button that started it, which says
/// `label` meanwhile.
struct PullRequestWork: Equatable {
    let key: String
    let label: String
}

/// A comment on a line, kept for the next review.
struct PendingLineComment: Identifiable, Equatable {
    let id = UUID()
    let path: String
    let line: Int
    /// "left" or "right".
    let side: String
    let body: String
}

/// What an action on a pull request did, or why it couldn't.
struct PullRequestNotice: Equatable {
    let id = UUID()
    let text: String
    let failed: Bool
    /// The pull request the action opened.
    var url: URL?
}

/// The symbol a file is shown with, by what its name ends in.
enum FileSymbol {
    static func symbol(for path: String) -> Symbol {
        switch (path as NSString).pathExtension.lowercased() {
        case "png", "jpg", "jpeg", "gif", "webp", "heic", "bmp", "tiff", "ico", "svg": .image
        case "md", "markdown", "txt", "rst": .fileText
        case "json", "yaml", "yml", "toml", "xml", "plist", "lock": .braces
        case "sh", "bash", "zsh", "fish": .terminal
        case "": .file
        default: .code
        }
    }
}
