import AuthenticationServices
import Foundation
import Observation
import UniformTypeIdentifiers

#if os(macOS)
import AppKit
#else
import UIKit
#endif

enum Selection: Hashable {
    case draft(String)
    case thread(String)
}

/// A thread that hasn't been sent yet: where it will start and with what. Its text is in `drafts`.
struct ThreadDraft: Identifiable, Equatable, Codable {
    var id = UUID().uuidString
    var projectID: String?
    var model: String?
    /// The id of the account of the model's agent the thread starts with.
    var agentAccount: String?
    var effort: String?
    var access: Access = .full
    var plan = false
    /// The thread starts in a new worktree, on a branch that starts from `base`.
    var worktree: Bool?
    var base: String?
}

/// A draft as the sidebar lists it.
struct ListedDraft: Identifiable {
    let draft: ThreadDraft
    let preview: String

    var id: String { draft.id }
}

/// Where the command panel opens.
enum PanelPage: Hashable {
    /// Everything that can be done from here.
    case commands
    /// The projects, to start a thread in one.
    case projects
    /// The projects, to choose the one the open draft works in.
    case draftProject
    /// The threads, to open one.
    case threads
    /// The servers, to add a project on one.
    case servers
    /// The ways to add a project on the server.
    case sources(String)
    case newProject(String)
    /// The repositories of the server's GitHub login, to clone one.
    case github(String)
    /// What GitHub needs on the server before its repositories can be listed.
    case githubSetup(String)
    /// The server's folders, to add one.
    case folder(String)
    /// The folders and images of the project's server, to make one its icon.
    case icon(String)
}

/// A project that a server is making or cloning.
struct AddingProject: Equatable {
    let serverID: String
    let name: String
}

/// A server that is installing a new version of itself.
struct ServerUpdate: Equatable {
    /// The version it had when the update began.
    let from: String
    /// How much of the download has arrived, when the server knows how much there is.
    var fraction: Double?
    /// The new version is installed, and the server starts it once its agents have finished.
    var waiting = false
    /// The new version is installed and the server is starting it.
    var restarting = false
}

extension ServerUpdate {
    init(json: JSON, from: String) {
        self.from = from
        fraction = json.optionalDouble("percent").map { $0 / 100 }
        waiting = json.string("state") == "waiting"
        restarting = json.string("state") == "restarting"
    }
}

/// When an updated server restarts while its agents work.
enum RestartWhen: String {
    /// Once no agent works.
    case idle
    /// At once: the agents are stopped and their threads continue after the restart.
    case now
}

/// What the images and videos fetched from the servers take on this Mac, and what they may take.
struct MediaStorage: Equatable {
    let used: Int64
    let limit: Int64
}

struct UndoNotice: Equatable {
    let threadIDs: [String]
    let text: String
}

/// Everything the views show. It mirrors the core: commands go to it, and its events change the
/// state here. All of it is used on the main thread.
@Observable
final class AppStore {
    // Account
    private(set) var account = Account()
    /// Whether the core has sent what it remembers from last time. Until then the window is hidden.
    private(set) var ready = false
    private(set) var signingIn = false
    var signInError: String?
    private(set) var enrollToken: EnrollToken?
    private(set) var enrollTokenPending = false
    var showsAddServer = false
    /// The section of the settings open over the client, while they are.
    var settings: SettingsSection?
    /// What the agents spent and what is left of their plans is open over the client.
    var showsUsage = false
    /// What the usage shows, kept while it is closed so that it opens with what was last read.
    private(set) var usage = UsageModel()
    /// The group of settings a search picked, until its page has scrolled to it.
    var settingsTarget: String?
    /// What is typed in the settings' search.
    var settingsQuery = ""
    /// The thread's settings are open over the client, where they aren't around the composer.
    var showsThreadSettings = false
    /// Files are being dragged over the window.
    var dropTargeted = false
    /// Files are being dragged over the composer's text, which takes drops itself.
    var composerDropTargeted = false
    /// The branch picker is open. Its branches are `nil` until your server has listed them.
    var showsBranches = false
    private(set) var listedBranches: Result<[Branch], CoreBridge.CoreError>?
    private var branchesProjectID: String?
    /// Each project's branches as last listed, which the picker opens on while it lists them again.
    private var knownBranches: [String: [Branch]] = [:]
    /// The project whose changes the commit sheet is open on, with the files it was opened with.
    var committingProject: Project?
    private(set) var gitFiles: [ChangedFile] = []
    /// An action that pushes from the default branch, until the user says where it should happen.
    var pendingGit: PendingGit?
    /// The stage a git action is at, by the checkout it runs in.
    private(set) var gitStages: [String: GitStage] = [:]
    /// The projects whose folders their servers are making git repositories.
    private(set) var initializingGit: Set<String> = []
    /// The `draftWorktreeKey`s whose base is being pulled.
    private(set) var pullingBases: Set<String> = []
    private(set) var gitNotice: GitNotice?
    /// What a new worktree starts at, like `origin/main`, by `draftWorktreeKey`.
    private(set) var worktreeStarts: [String: String] = [:]
    private(set) var panel: PanelPage?
    /// What a server refused while the panel was adding a project.
    var panelNotice: String?
    /// Whether GitHub can be used on each server, as last heard.
    private(set) var github: [String: GitHubState] = [:]
    /// The GitHub repositories each server last listed, and why one couldn't.
    private(set) var repos: [String: [Repo]] = [:]
    private(set) var repoErrors: [String: String] = [:]
    /// The servers that are listing their repositories now.
    private(set) var listingRepos: Set<String> = []
    @ObservationIgnored private var reposListed: [String: Date] = [:]
    private(set) var addingProject: AddingProject?
    /// Counts up when a project has been made or cloned.
    private(set) var projectsAdded = 0
    @ObservationIgnored private var awaitedProjectID: String?
    /// The folder just added as a project, which the next thread starts in once the server has told about it.
    @ObservationIgnored private var awaitedFolder: (serverID: String, path: String)?
    /// Counts up when the composer should take the keyboard back.
    private(set) var composerFocus = 0
    /// Counts up when an empty draft is opened for a new thread.
    private(set) var newThreadsStarted = 0

    // What the servers hold
    private(set) var servers: [Server] = []
    private var serverUpdates: [String: ServerUpdate] = [:]
    /// The folders the user added. "No project" of each server is in `noProjects`.
    private(set) var projects: [Project] = [] {
        didSet { indexProjects() }
    }
    /// The project of each server that threads start in without one.
    private(set) var noProjects: [Project] = [] {
        didSet { indexProjects() }
    }
    private(set) var projectsByID: [String: Project] = [:]
    private(set) var threads: [String: ThreadInfo] = [:]
    /// The threads as the sidebar lists them, kept in order as they change so that it never sorts them.
    private(set) var activeThreads: [ThreadInfo] = []
    private(set) var doneThreads: [ThreadInfo] = []

    // The open thread
    /// Always a draft or a thread; `loadPreferences` opens the first draft.
    private(set) var selection: Selection = .draft("")
    private(set) var activity = Activity()
    private(set) var transcriptIsEmpty = true
    /// The agents the open thread's agent has started, in the order it started them.
    private(set) var agents: [AgentInfo] = []
    /// The drafts whose first message is on its way to the server.
    private(set) var sendingDraftIDs: Set<String> = []
    var errorMessage: String?
    /// What deleting a thread takes with it.
    func deletionNote(_ thread: ThreadInfo?) -> String {
        let inWorktree = thread.map { project($0.projectID)?.seen(from: $0).worktree != nil } ?? false
        return inWorktree
            ? "Its worktree and the uncommitted changes there are deleted too. Its branch stays."
            : "The files the agent changed stay as they are."
    }
    /// The error's first sentence as the alert's title, and the rest below it.
    var errorAlert: (title: String, detail: String) {
        let message = errorMessage ?? ""
        let sentences = message.split(separator: ". ", maxSplits: 1)
        guard let first = sentences.first, first.count <= 60, !first.contains("\n") else {
            return ("Something went wrong", message)
        }
        let title = first.hasSuffix(".") ? first.dropLast() : first
        return (String(title), sentences.count > 1 ? String(sentences[1]) : "")
    }
    private(set) var threadDrafts: [ThreadDraft] = []
    /// What the open draft said when it was opened, if it said anything. Its row in the sidebar
    /// shows this, so the sidebar doesn't change while the draft is being written.
    private(set) var openedDraftPreview: String?
    private(set) var undo: UndoNotice?
    /// The thread its pull request's end last marked done, which the sidebar then shows.
    private(set) var settledThreadID: String?
    /// Unknown until Settings asks for it.
    private(set) var mediaStorage: MediaStorage?
    private var drafts: [String: String] = [:]
    private var attachmentsByKey: [String: [Attachment]] = [:]
    /// The images and videos the viewer has open over the window.
    private(set) var viewing: Viewing?

    /// Counts up when the folder the open thread works in may have changed: a turn ended there,
    /// or git did something.
    private(set) var workspaceVersion = 0

    let updater = AppUpdater()
    let sidePanel = SidePanel()
    let linear = Linear()
    @ObservationIgnored let core = CoreBridge()
    @ObservationIgnored let transcript = TranscriptModel()
    /// What the agent did that the side panel shows.
    @ObservationIgnored let agentTranscript = TranscriptModel()
    @ObservationIgnored private var signInSession: SignInSession?
    @ObservationIgnored private let lifecycle = Lifecycle()
    @ObservationIgnored private var undoTimer: Timer?
    @ObservationIgnored private var openThreadID: String?
    @ObservationIgnored private let defaults = UserDefaults.standard
    /// The thread that was open when the client was last closed, until it has been opened again.
    @ObservationIgnored private var lastSelection: String?

    init() {
        loadPreferences()
        sidePanel.store = self
        linear.store = self
    }

    // MARK: Starting

    func start() {
        core.decode = { [weak self] event in self?.decode(event) }
        let environment = ProcessInfo.processInfo.environment
        let dataDir = environment["MOTILE_DATA_DIR"] ?? Self.dataFolder()
        let bundled = Bundle.main.object(forInfoDictionaryKey: "MotileAuthURL") as? String
        let authURL = environment["MOTILE_AUTH_URL"] ?? bundled.flatMap { $0.isEmpty ? nil : $0 } ?? "https://auth.motile.app"
        var config: JSON = [
            "data_dir": dataDir,
            "auth_url": authURL,
            "device_name": Platform.deviceName,
            "platform": Platform.name,
            "local_only": environment["MOTILE_LOCAL"] == "1",
        ]
        if let address = environment["MOTILE_SERVER_ADDR"] { config["direct_addr"] = address }
        #if os(iOS)
        // A phone has less room for the images and videos it keeps than a Mac.
        config["media_limit"] = 500_000_000
        #endif
        if !core.start(config: config) {
            errorMessage = "Motile couldn't start. Its data folder may not be writable."
        }
        if environment["MOTILE_DEMO"] != "1" { updater.start() }
        lifecycle.start(self)
        NotificationCenter.default.addObserver(forName: Platform.becameActive, object: nil, queue: .main) { [weak self] _ in
            self?.markOpenThreadSeen()
            self?.readGit(fetch: true)
            self?.readWorktreeStart(fetch: true)
        }
    }

    /// Where the core keeps the device's key, the copy of the threads and the fetched files.
    private static func dataFolder() -> String {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
        guard let folder = support?.appendingPathComponent("Motile") else { return NSTemporaryDirectory() }
        #if os(iOS)
        // The key is this device's alone: a backup restored on another one must not bring it.
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        var excluded = folder
        try? excluded.setResourceValues(values)
        #endif
        return folder.path
    }

    // MARK: Events

    /// Runs off the main thread: reads the event and returns what to do with it on the main thread.
    private func decode(_ event: JSON) -> (() -> Void)? {
        switch event.string("type") {
        case "account":
            let account = Account(json: event.object("account") ?? [:])
            return { [weak self] in self?.apply(account) }
        case "restored":
            return { [weak self] in
                self?.ready = true
                self?.ensureDraftProject()
            }
        case "servers":
            let servers = event.objects("servers").map { Server(json: $0) }
            return { [weak self] in self?.apply(servers: servers) }
        case "threads":
            let serverID = event.string("server_id")
            let threads = event.objects("threads").map { ThreadInfo(json: $0) }
            return { [weak self] in self?.apply(threads: threads, serverID: serverID) }
        case "thread_upsert":
            let thread = ThreadInfo(json: event.object("thread") ?? [:])
            return { [weak self] in self?.upsert(thread) }
        case "thread_deleted":
            let threadID = event.string("thread_id")
            return { [weak self] in self?.removeThread(threadID) }
        case "projects":
            let serverID = event.string("server_id")
            let projects = event.objects("projects").map { Project(json: $0, serverID: serverID) }
            return { [weak self] in self?.apply(projects: projects, serverID: serverID) }
        case "server_update":
            let serverID = event.string("server_id")
            let (received, total) = (event.double("received"), event.optionalDouble("total"))
            let waiting = event.bool("waiting")
            return { [weak self] in
                guard !waiting else {
                    self?.serverUpdates[serverID]?.waiting = true
                    return
                }
                self?.serverUpdates[serverID]?.fraction = total.flatMap { $0 > 0 ? received / $0 : nil }
            }
        case "git_progress":
            let (projectID, threadID) = (event.string("project_id"), event.optionalString("thread_id"))
            let stage = GitStage(rawValue: event.string("stage")) ?? .unknown
            return { [weak self] in
                guard let self, let project = self.project(projectID) else { return }
                let checkoutID = (threadID.flatMap { self.threads[$0] }.map { project.seen(from: $0) } ?? project).checkoutID
                guard self.gitStages[checkoutID] != nil else { return }
                self.gitStages[checkoutID] = stage
            }
        case "upload_progress":
            let (key, sent, size) = (event.string("key"), event.double("sent"), event.double("size"))
            guard size > 0 else { return nil }
            return { [weak self] in
                self?.changeAttachment(key) { attachment in
                    guard case .uploading = attachment.state else { return }
                    attachment.state = .uploading(sent / size)
                }
            }
        case "media_progress":
            let (id, received, size) = (event.string("id"), event.double("received"), event.double("size"))
            guard size > 0 else { return nil }
            return {
                let progress: [String: Any] = ["id": id, "fraction": received / size]
                NotificationCenter.default.post(name: .mediaProgress, object: nil, userInfo: progress)
            }
        case "rows":
            let threadID = event.string("thread_id")
            let (reset, start, remove) = (event.bool("reset"), event.int("start"), event.int("remove"))
            let rows = event.objects("rows").compactMap { RowModel(json: $0) }
            let earlier = event.bool("earlier")
            return { [weak self] in
                guard let self, self.transcript.threadID == threadID else { return }
                self.transcript.apply(reset: reset, start: start, remove: remove, rows: rows, earlier: earlier)
                // Assigned only when it changes: every assignment makes the views that read it
                // update, and rows arrive many times a second.
                let isEmpty = self.transcript.isEmpty
                if self.transcriptIsEmpty != isEmpty { self.transcriptIsEmpty = isEmpty }
                let turns = self.transcript.turns
                if self.sidePanel.turns != turns { self.sidePanel.turns = turns }
            }
        case "agents":
            let threadID = event.string("thread_id")
            let agents = event.objects("agents").map { AgentInfo(json: $0) }
            return { [weak self] in
                guard let self, self.transcript.threadID == threadID else { return }
                if self.agents != agents { self.agents = agents }
                self.sidePanel.agentsChanged()
            }
        case "agent_rows":
            let (threadID, agentID) = (event.string("thread_id"), event.string("agent_id"))
            let (reset, start, remove) = (event.bool("reset"), event.int("start"), event.int("remove"))
            let rows = event.objects("rows").compactMap { RowModel(json: $0) }
            return { [weak self] in
                guard let self, self.transcript.threadID == threadID, self.sidePanel.shownAgent == agentID else { return }
                self.agentTranscript.apply(reset: reset, start: start, remove: remove, rows: rows, earlier: false)
            }
        case "code_spans":
            let request = (event["id"] as? NSNumber)?.uint64Value ?? 0
            let lines = (event["lines"] as? [[NSNumber]] ?? []).map { $0.map(\.int32Value) }
            let file = event.int("file")
            return { [weak self] in self?.sidePanel.colour(request: request, file: file, lines: lines) }
        case "spans":
            let (threadID, rowID) = (event.string("thread_id"), event.string("row_id"))
            let spans = event["spans"] as? [NSNumber] ?? []
            return { [weak self] in
                guard let self, self.transcript.threadID == threadID else { return }
                self.transcript.apply(spans: spans, rowID: rowID)
                self.agentTranscript.apply(spans: spans, rowID: rowID)
            }
        case "live":
            let threadID = event.string("thread_id")
            let live = event.bool("live")
            return { [weak self] in
                guard let self, self.transcript.threadID == threadID else { return }
                self.transcript.setLive(live)
                if self.agentTranscript.threadID == threadID { self.agentTranscript.setLive(live) }
            }
        case "activity":
            let threadID = event.string("thread_id")
            let activity = Activity(json: event.object("activity") ?? [:], waiting: event.objects("waiting"))
            return { [weak self] in
                guard let self, self.transcript.threadID == threadID else { return }
                if self.activity != activity { self.activity = activity }
                self.transcript.setActivity(activity)
            }
        case "thread_error":
            let message = event.string("message")
            return { [weak self] in self?.errorMessage = message }
        default:
            return nil
        }
    }

    private func apply(_ account: Account) {
        let wasSignedIn = self.account.signedIn
        self.account = account
        if wasSignedIn && !account.signedIn {
            openEmptyDraft()
            enrollToken = nil
            usage = UsageModel()
        }
    }

    private func apply(servers: [Server]) {
        self.servers = servers
        let known = Set(servers.map(\.id))
        if projects.contains(where: { !known.contains($0.serverID) }) {
            projects.removeAll { !known.contains($0.serverID) }
        }
        if noProjects.contains(where: { !known.contains($0.serverID) }) {
            noProjects.removeAll { !known.contains($0.serverID) }
        }
        if threads.values.contains(where: { !known.contains($0.serverID) }) {
            setThreads(threads.filter { known.contains($0.value.serverID) })
        }
        // A server that is back with another version has finished updating.
        for server in servers where server.state == .connected {
            guard let update = serverUpdates[server.id], update.restarting, server.version != update.from else { continue }
            serverUpdates[server.id] = nil
        }
        ensureDraftProject()
        // The server has arrived; the install command has done its job.
        if showsAddServer, servers.count > addServerCount {
            showsAddServer = false
        }
    }

    private func apply(threads new: [ThreadInfo], serverID: String) {
        var kept = threads.filter { $0.value.serverID != serverID }
        for thread in new { kept[thread.id] = thread }
        setThreads(kept)
        if case .thread(let id) = selection, threads[id] == nil {
            openEmptyDraft()
        }
        markOpenThreadSeen()
        restoreSelection()
    }

    private func upsert(_ thread: ThreadInfo) {
        let before = threads[thread.id]
        guard before != thread else { return }
        setThread(thread, id: thread.id)
        markOpenThreadSeen()
        guard let before else { return }
        if !before.isDone, thread.isDone, before.pullRequest != thread.pullRequest { settledThreadID = thread.id }
        guard selection == .thread(thread.id) else { return }
        if before.turnEndedAt != thread.turnEndedAt || (before.running && !thread.running) { workspaceVersion += 1 }
    }

    /// A reply in the open thread has been seen once Motile is in front.
    private func markOpenThreadSeen() {
        guard Platform.isActive, let thread = selectedThread, thread.unread else { return }
        core.send("mark_seen", ["thread_id": thread.id])
    }

    private func removeThread(_ id: String) {
        setThread(nil, id: id)
        sidePanel.forget(id)
        if selection == .thread(id) { openEmptyDraft() }
    }

    private func apply(projects new: [Project], serverID: String) {
        let git = gitProject?.git
        defer { if gitProject?.git != git { workspaceVersion += 1 } }
        var updated = projects.filter { $0.serverID != serverID } + new.filter { !$0.noProject }
        updated.sort { $0.createdAt < $1.createdAt }
        // Every row of the sidebar looks at the projects, so they only change when they did.
        if updated != projects { projects = updated }
        let order = servers.map(\.id)
        var none = noProjects.filter { $0.serverID != serverID } + new.filter(\.noProject)
        none.sort { (order.firstIndex(of: $0.serverID) ?? order.count) < (order.firstIndex(of: $1.serverID) ?? order.count) }
        if none != noProjects { noProjects = none }
        ImageFiles.shared.warm(new.compactMap(\.iconPath))
        ensureDraftProject()
        openAwaitedProject()
        pickAwaitedFolder()
        // A server that has just started hasn't read the repository yet.
        if let project = gitProject, project.serverID == serverID, project.git == nil { readGit() }
    }

    private func indexProjects() {
        projectsByID = Dictionary((projects + noProjects).map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
    }

    // MARK: Lookups

    private func setThreads(_ new: [String: ThreadInfo]) {
        threads = new
        activeThreads = new.values.filter { !$0.isDone }.sorted(by: ThreadInfo.listed)
        doneThreads = new.values.filter(\.isDone).sorted(by: ThreadInfo.listed)
    }

    /// Changes one thread, and moves it in its list only when its place there changed.
    private func setThread(_ thread: ThreadInfo?, id: String) {
        let before = threads[id]
        threads[id] = thread
        if let before, let thread, before.isDone == thread.isDone, before.listedAt == thread.listedAt {
            changeList(done: thread.isDone) { list in
                guard let index = list.index(of: before) else { return }
                list[index] = thread
            }
            return
        }
        if let before {
            changeList(done: before.isDone) { list in
                guard let index = list.index(of: before) else { return }
                list.remove(at: index)
            }
        }
        if let thread {
            changeList(done: thread.isDone) { $0.insert(thread, at: $0.place(of: thread)) }
        }
    }

    private func changeList(done: Bool, _ change: (inout [ThreadInfo]) -> Void) {
        if done { change(&doneThreads) } else { change(&activeThreads) }
    }

    var selectedThread: ThreadInfo? {
        guard case .thread(let id) = selection else { return nil }
        return threads[id]
    }

    var selectedDraft: ThreadDraft? {
        guard case .draft(let id) = selection else { return nil }
        return threadDrafts.first { $0.id == id }
    }

    /// Every draft with something written in it that isn't being sent, newest first. The open
    /// one is listed as it was when it was opened.
    var listedDrafts: [ListedDraft] {
        threadDrafts.reversed().compactMap { draft -> ListedDraft? in
            guard !sendingDraftIDs.contains(draft.id) else { return nil }
            let written = selection == .draft(draft.id) ? openedDraftPreview : preview(of: draft)
            return written.map { ListedDraft(draft: draft, preview: $0) }
        }
    }

    /// The first line of what was written in a draft, or what is attached to it. Nothing for a
    /// draft that is still empty.
    private func preview(of draft: ThreadDraft) -> String? {
        let text = (drafts[draft.id] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        if let line = text.split(separator: "\n").first { return String(line) }
        guard let files = attachmentsByKey[draft.id] else { return nil }
        return files.count == 1 ? "1 attachment" : "\(files.count) attachments"
    }

    func project(_ id: String?) -> Project? {
        id.flatMap { projectsByID[$0] }
    }

    /// The projects, the one a thread was last started in first. A project without threads
    /// counts from when it was added.
    var recentProjects: [Project] {
        var lastUsed: [String: Double] = [:]
        for thread in threads.values {
            lastUsed[thread.projectID] = max(lastUsed[thread.projectID] ?? 0, thread.createdAt)
        }
        return projects.sorted { (lastUsed[$0.id] ?? $0.createdAt, $0.id) > (lastUsed[$1.id] ?? $1.createdAt, $1.id) }
    }

    func server(_ id: String?) -> Server? {
        servers.first { $0.id == id }
    }

    /// The project the composer's thread works in, or the one the open draft would start in.
    var composerProject: Project? {
        threadProject ?? project(selectedDraft?.projectID)
    }

    /// The line over the thread's title: its project, its branch, and whether it starts in a new
    /// worktree.
    var composerProjectLine: [String]? {
        let project = composerProject
        let folder = selectedThread.map { URL(fileURLWithPath: $0.cwd).lastPathComponent }
        guard let name = project?.name ?? folder else { return nil }
        if draftUsesWorktree, let start = draftStart { return [name, start, "New worktree"] }
        guard let branch = project?.branch else { return [name] }
        return [name, branch]
    }

    /// The open thread's project, as the thread works in it.
    var threadProject: Project? {
        guard let thread = selectedThread else { return nil }
        return project(thread.projectID)?.seen(from: thread)
    }

    /// The project git works in from here: the open thread's, or the open draft's, unless its
    /// thread starts in a new worktree, where only its base can be pulled.
    var gitProject: Project? {
        let project = threadProject ?? (draftUsesWorktree ? nil : project(selectedDraft?.projectID))
        guard let project, canUseGit(of: project) else { return nil }
        return project
    }

    /// The git notice of the checkout git works in from here, or of the project the open draft
    /// starts a worktree in.
    var openGitNotice: GitNotice? {
        guard let notice = gitNotice else { return nil }
        let draftProject = draftUsesWorktree ? project(selectedDraft?.projectID) : nil
        guard let checkoutID = (gitProject ?? draftProject)?.checkoutID, notice.checkoutID == checkoutID else { return nil }
        return notice
    }

    /// The folder the side panel looks into: the one the open thread works in, or the project's
    /// for the open draft.
    var panelTarget: PanelTarget? {
        guard let project = threadProject ?? project(selectedDraft?.projectID) else { return nil }
        // A thread without a project has no folder until it starts.
        guard !project.noProject || selectedThread != nil else { return nil }
        let folder = project.noProject ? selectedThread?.cwd : project.worktree?.path
        let awaitsWorktree = selectedThread == nil && draftUsesWorktree
        return PanelTarget(
            key: draftKey, serverID: project.serverID, projectID: project.id, threadID: selectedThread?.id,
            name: URL(fileURLWithPath: folder ?? project.path).lastPathComponent,
            repository: project.branch != nil, worktree: project.worktree != nil, awaitsWorktree: awaitsWorktree,
            pullRequest: awaitsWorktree ? nil : (selectedThread?.pullRequest ?? project.git?.pullRequest)?.number)
    }

    /// Why no pull request can be shown here, when none can.
    var pullRequestsUnavailable: String? {
        guard let target = panelTarget, let server = server(target.serverID) else { return nil }
        guard server.protocolVersion >= 8 else { return "Update \(server.name) to see pull requests here." }
        return target.repository ? nil : "This folder isn't a git repository."
    }

    /// Why the thread's pull request tab has nothing to show here, when it hasn't.
    var pullRequestUnavailable: String? {
        guard let target = panelTarget else { return nil }
        return pullRequestsUnavailable ?? (target.pullRequest == nil ? "This branch has no pull request yet." : nil)
    }

    /// The server lists, links, edits and watches pull requests, and reviews their lines.
    var pullRequestsExtended: Bool {
        guard let target = panelTarget, let server = server(target.serverID) else { return false }
        return server.protocolVersion >= 9
    }

    /// What the server does with pull requests by itself.
    func setPullRequestSettings(doneOnMerge: Bool, removeMergedWorktrees: Bool, on server: Server) {
        let settings: JSON = ["done_on_merge": doneOnMerge, "remove_merged_worktrees": removeMergedWorktrees]
        core.send("set_pull_request_settings", ["server_id": server.id, "settings": settings]) { [weak self] result in
            if case .failure(let error) = result { self?.errorMessage = error.message }
        }
    }

    /// Opens the pull request's tab, or the pull request on GitHub where the tab can't show it.
    func showPullRequest(_ url: URL) {
        gitNotice = nil
        guard panelUnavailable == nil, pullRequestUnavailable == nil else { return Platform.open(url) }
        sidePanel.open(.pullRequest)
    }

    /// Puts a prompt that a pull request's tab wrote in the composer, under what is there, for
    /// the user to read and send.
    func handOff(_ prompt: String) {
        let written = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        draft = written.isEmpty ? prompt : "\(written)\n\n\(prompt)"
        if sidePanel.isMaximized { sidePanel.toggleMaximized() }
        if Platform.panelCoversThread { sidePanel.isOpen = false }
        composerFocus += 1
    }

    /// Puts the prompt in the composer of the open draft. From a thread it opens a draft in the
    /// project first, which takes the panel's open tab along.
    func startWork(_ prompt: String, in projectID: String) {
        guard selectedDraft == nil else { return handOff(prompt) }
        guard let project = project(projectID) else { return }
        let tab = sidePanel.tabs.active
        startNewThread(in: project)
        if let tab, tab.blankNumber == nil { sidePanel.open(tab) }
        handOff(prompt)
    }

    /// Why the side panel has nothing to show here, when it hasn't.
    var panelUnavailable: String? {
        guard let target = panelTarget else { return "Add a project to see its files." }
        guard let server = server(target.serverID), server.state == .connected else { return "Your server isn't connected." }
        return server.protocolVersion >= 7 ? nil : "Update \(server.name) to see files and changes."
    }

    /// The server the composer is talking to: the open thread's, or the open draft's project's.
    var composerServer: Server? {
        if let thread = selectedThread { return server(thread.serverID) }
        return server(project(selectedDraft?.projectID)?.serverID) ?? servers.first
    }

    /// The models the composer offers. An open thread on a server from before accounts stays
    /// with its agent.
    var composerModels: [ModelInfo] {
        let models = composerServer?.models ?? []
        guard let thread = selectedThread, composerServer?.switchesAccounts != true else { return models }
        return models.filter { $0.agent == thread.agent }
    }

    var composerModel: ModelInfo? {
        let id = selectedThread.map { $0.model } ?? selectedDraft?.model
        let agent = selectedThread?.agent
        return composerModels.first { $0.id == id && (agent == nil || $0.agent == agent) }
            ?? composerModels.first { agent == nil || $0.agent == agent }
    }

    /// The agent a draft starts with: its model's, or its server's first model's.
    func agent(of draft: ThreadDraft) -> Agent? {
        guard let server = server(project(draft.projectID)?.serverID) ?? servers.first else { return nil }
        return (server.models.first { $0.id == draft.model } ?? server.models.first)?.agent
    }

    /// The accounts the composer offers the models of, an agent's default one first.
    var composerAccounts: [AgentAccount] {
        let agents = Set(composerModels.map(\.agent))
        return Agent.allCases.filter(agents.contains).flatMap { composerServer?.accounts(of: $0) ?? [] }
    }

    /// The account of the composer's model's agent the thread works with.
    var composerAccount: AgentAccount? {
        guard let model = composerModel else { return nil }
        let id = selectedThread.map { $0.agentAccount } ?? selectedDraft?.agentAccount
        return composerServer?.account(id, of: model.agent)
    }

    /// The model the composer shows, with the account when its agent has more than one.
    var composerModelLabel: String {
        guard let model = composerModel else { return "No agent" }
        guard let account = composerAccount, (composerServer?.accounts(of: account.agent).count ?? 0) > 1 else { return model.shortName }
        return "\(model.shortName) · \(account.name)"
    }

    /// What the composer's model menu lists: each account with its agent's models, titled with
    /// the account's name when its agent has more than one.
    var composerChoices: [(title: String, account: AgentAccount, models: [ModelInfo])] {
        composerAccounts.map { account in
            let several = (composerServer?.accounts(of: account.agent).count ?? 0) > 1
            let title = several ? "\(account.agent.name) · \(account.name)" : account.agent.name
            return (title, account, composerModels.filter { $0.agent == account.agent })
        }
    }

    var composerEffort: String? {
        let effort = selectedThread.map { $0.effort } ?? selectedDraft?.effort
        guard let model = composerModel, !model.efforts.isEmpty else { return nil }
        if let effort, model.efforts.contains(effort) { return effort }
        return model.defaultEffort ?? model.efforts.first
    }

    var composerAccess: Access { selectedThread?.access ?? selectedDraft?.access ?? .full }
    var composerPlan: Bool { selectedThread?.plan ?? selectedDraft?.plan ?? false }

    /// What the composer's text and attachments are kept under: the open draft or thread.
    var draftKey: String {
        switch selection {
        case .draft(let id), .thread(let id): return id
        }
    }

    var draft: String {
        get { drafts[draftKey] ?? "" }
        set { setText(newValue, for: draftKey) }
    }

    var attachments: [Attachment] { attachmentsByKey[draftKey] ?? [] }

    private func setText(_ text: String, for key: String) {
        drafts[key] = text.isEmpty ? nil : text
        defaults.set(drafts, forKey: "drafts")
    }

    // MARK: Preferences

    private func loadPreferences() {
        drafts = defaults.dictionary(forKey: "drafts") as? [String: String] ?? [:]
        lastSelection = defaults.string(forKey: "selection")
        let saved = defaults.data(forKey: "threadDrafts").flatMap { try? JSONDecoder().decode([ThreadDraft].self, from: $0) }
        threadDrafts = (saved ?? []).filter { preview(of: $0) != nil }
        let opened = threadDrafts.first { $0.id == lastSelection } ?? emptyDraft()
        selection = .draft(opened.id)
        openedDraftPreview = preview(of: opened)
    }

    private func saveThreadDrafts() {
        defaults.set(try? JSONEncoder().encode(threadDrafts), forKey: "threadDrafts")
    }

    /// A draft that starts with what the last one was set to.
    private func addDraft() -> ThreadDraft {
        var draft = ThreadDraft()
        draft.projectID = defaults.string(forKey: "new.project")
        draft.worktree = defaults.bool(forKey: "new.worktree")
        applyLastSettings(to: &draft)
        threadDrafts.append(draft)
        saveThreadDrafts()
        return draft
    }

    private func applyLastSettings(to draft: inout ThreadDraft) {
        draft.model = defaults.string(forKey: "new.model")
        draft.agentAccount = defaults.string(forKey: "new.agentAccount")
        draft.effort = defaults.string(forKey: "new.effort")
        draft.access = Access(rawValue: defaults.string(forKey: "new.access") ?? "") ?? .full
        draft.base = nil
    }

    /// Has the next draft start with what was just chosen, and only that.
    private func rememberChoices(of draft: ThreadDraft, changedFrom before: ThreadDraft) {
        if draft.projectID != before.projectID { defaults.set(draft.projectID, forKey: "new.project") }
        if draft.worktree != before.worktree { defaults.set(draft.worktree == true, forKey: "new.worktree") }
        if draft.model != before.model { defaults.set(draft.model, forKey: "new.model") }
        if draft.agentAccount != before.agentAccount { defaults.set(draft.agentAccount, forKey: "new.agentAccount") }
        if draft.effort != before.effort { defaults.set(draft.effort, forKey: "new.effort") }
        if draft.access != before.access { defaults.set(draft.access.rawValue, forKey: "new.access") }
    }

    /// A draft with nothing in it: the one that is already there, or a new one.
    private func emptyDraft() -> ThreadDraft {
        if let index = threadDrafts.lastIndex(where: { preview(of: $0) == nil && !sendingDraftIDs.contains($0.id) }) {
            var empty = threadDrafts[index]
            applyLastSettings(to: &empty)
            threadDrafts[index] = empty
            saveThreadDrafts()
            return empty
        }
        return addDraft()
    }

    /// Changes the open draft, and has the next draft start with what changed.
    private func updateDraft(_ change: (inout ThreadDraft) -> Void) {
        guard let index = threadDrafts.firstIndex(where: { selection == .draft($0.id) }) else { return }
        let before = threadDrafts[index]
        change(&threadDrafts[index])
        rememberChoices(of: threadDrafts[index], changedFrom: before)
        saveThreadDrafts()
    }

    private func removeDraft(_ id: String) {
        threadDrafts.removeAll { $0.id == id }
        attachmentsByKey[id] = nil
        setText("", for: id)
        saveThreadDrafts()
    }

    /// Keeps the open draft pointing at a project that exists, once the projects are known.
    private func ensureDraftProject() {
        guard ready, let draft = selectedDraft, project(draft.projectID) == nil, let first = projects.first ?? noProjects.first else {
            return
        }
        updateDraft { $0.projectID = first.id }
    }

    /// Opens the thread that was open when the client was last closed, once it is known.
    private func restoreSelection() {
        guard let wanted = lastSelection, threads[wanted] != nil else { return }
        lastSelection = nil
        guard let draft = selectedDraft, preview(of: draft) == nil else { return }
        select(.thread(wanted))
    }

    // MARK: Account

    func signIn() {
        guard !signingIn else { return }
        signingIn = true
        signInError = nil
        core.send("begin_sign_in") { [weak self] result in
            guard let self else { return }
            guard case .success(let value) = result, let url = URL(string: value.string("url")) else {
                self.signInFailed("The sign-in couldn't be started.")
                return
            }
            let session = SignInSession()
            self.signInSession = session
            session.start(url: url) { [weak self] callback in
                guard let self else { return }
                self.signInSession = nil
                guard let callback else {
                    self.signingIn = false
                    return
                }
                self.core.send("complete_sign_in", ["url": callback.absoluteString]) { [weak self] result in
                    if case .failure(let error) = result { self?.signInFailed(error.message) } else { self?.signingIn = false }
                }
            }
        }
    }

    /// Signs in on an auth server that allows it without Google. Used by the demo and local work.
    func devSignIn(email: String, done: (() -> Void)? = nil) {
        signingIn = true
        core.send("dev_sign_in", ["email": email]) { [weak self] result in
            if case .failure(let error) = result { self?.signInFailed(error.message) } else { self?.signingIn = false }
            done?()
        }
    }

    private func signInFailed(_ message: String) {
        signingIn = false
        signInError = message
    }

    func signOut() {
        closeSettings()
        drafts = [:]
        attachmentsByKey = [:]
        threadDrafts = []
        defaults.removeObject(forKey: "drafts")
        openEmptyDraft()
        core.send("sign_out")
    }

    // MARK: Servers

    @ObservationIgnored private var addServerCount = 0

    /// Asks for an install command and keeps looking for the server it will link.
    func prepareToAddServer() {
        addServerCount = servers.count
        core.send("watch_servers", ["on": true])
        if let token = enrollToken, !token.isExpired(at: Date()) { return }
        regenerateEnrollToken()
    }

    func regenerateEnrollToken() {
        guard !enrollTokenPending else { return }
        enrollTokenPending = true
        core.send("create_enroll_token") { [weak self] result in
            self?.enrollTokenPending = false
            switch result {
            case .success(let value): self?.enrollToken = EnrollToken(json: value)
            case .failure(let error): self?.errorMessage = error.message
            }
        }
    }

    func stopAddingServer() {
        core.send("watch_servers", ["on": false])
        // A token links one server; the next server gets a new one.
        if servers.count != addServerCount { enrollToken = nil }
    }

    /// Whether the server runs an older version than the newest release.
    func isOutdated(_ server: Server) -> Bool {
        server.state == .connected && Version.isOlder(server.version, than: updater.latest)
    }

    /// Whether an agent works on the server: in a turn, or watching something it left running.
    func isBusy(_ server: Server) -> Bool {
        threads.values.contains { $0.serverID == server.id && $0.busy }
    }

    /// Whether the server can wait for its agents before it restarts, or stop them and go on with
    /// them after.
    func canChooseRestart(_ server: Server) -> Bool {
        server.protocolVersion >= 14
    }

    /// Has the server install the newest release and start it: `when` its agents work, once they
    /// have finished or at once. A server that can't choose refuses while they work.
    func update(_ server: Server, when: RestartWhen = .idle) {
        let current = serverUpdate(of: server)
        if current?.waiting == true, when == .now {
            return restartNow(server)
        }
        guard current == nil else { return }
        serverUpdates[server.id] = ServerUpdate(from: server.version)
        var command: JSON = ["server_id": server.id]
        if canChooseRestart(server) { command["when"] = when.rawValue }
        core.send("update_server", command) { [weak self] result in
            guard let self else { return }
            switch result {
            case .success:
                self.serverUpdates[server.id]?.waiting = false
                self.serverUpdates[server.id]?.restarting = true
                // If the server never says it is back, the row stops waiting for it.
                DispatchQueue.main.asyncAfter(deadline: .now() + 60) { [weak self] in
                    if self?.serverUpdates[server.id]?.restarting == true { self?.serverUpdates[server.id] = nil }
                }
            case .failure(let error):
                self.serverUpdates[server.id] = nil
                self.errorMessage = error.message
            }
        }
    }

    /// The update a server is putting in place: as it says, or, from a server too old to say, as
    /// the request that began it goes.
    func serverUpdate(of server: Server) -> ServerUpdate? {
        server.update ?? serverUpdates[server.id]
    }

    /// Has a server that waits for its agents to update stop them and restart now. The request
    /// that started the update answers once it restarts.
    private func restartNow(_ server: Server) {
        core.send("update_server", ["server_id": server.id, "when": RestartWhen.now.rawValue]) { [weak self] result in
            if case .failure(let error) = result { self?.errorMessage = error.message }
        }
    }

    func setContinueSettings(afterLimits: Bool, afterRestarts: Bool, on server: Server) {
        let settings: JSON = ["after_limits": afterLimits, "after_restarts": afterRestarts]
        core.send("set_continue_settings", ["server_id": server.id, "settings": settings]) { [weak self] result in
            if case .failure(let error) = result { self?.errorMessage = error.message }
        }
    }

    func removeServer(_ server: Server) {
        core.send("remove_server", ["server_id": server.id]) { [weak self] result in
            if case .failure(let error) = result { self?.errorMessage = error.message }
        }
    }

    // MARK: Projects

    func addProject(serverID: String, path: String) {
        request(serverID, ["type": "add_project", "path": path], done: { [weak self] in
            guard let self else { return }
            let trimmed = path.count > 1 && path.hasSuffix("/") ? String(path.dropLast()) : path
            awaitedFolder = (serverID, trimmed)
            pickAwaitedFolder()
        })
    }

    private func pickAwaitedFolder() {
        guard let folder = awaitedFolder,
              let project = projects.first(where: { $0.serverID == folder.serverID && $0.path == folder.path })
        else { return }
        awaitedFolder = nil
        setNewThreadProject(project.id)
    }

    /// Opens the panel on the ways to add a project, after the servers when there is a choice.
    func addProject() {
        openPanel(addProjectPage)
    }

    var addProjectPage: PanelPage {
        let connected = servers.filter { $0.state == .connected }
        guard connected.count == 1, let server = connected.first else { return .servers }
        return .sources(server.id)
    }

    /// Whether the server can start a project from a name or from GitHub.
    func startsProjects(_ server: Server?) -> Bool {
        (server?.protocolVersion ?? 0) >= 5
    }

    /// With `icons`, the images that can be a project's icon are listed too.
    func browse(serverID: String, query: String, icons: Bool, done: @escaping (Result<FolderListing, CoreBridge.CoreError>) -> Void) {
        core.send("browse", ["server_id": serverID, "query": query, "icons": icons]) { result in
            done(result.map { FolderListing(json: $0) })
        }
    }

    func readGitHub(_ serverID: String) {
        core.send("request", ["server_id": serverID, "request": ["type": "github_status"]]) { [weak self] result in
            guard case .success(let answer) = result else { return }
            self?.heard(github: answer, serverID: serverID)
        }
    }

    private func heard(github answer: JSON, serverID: String) {
        guard let state = GitHubState(rawValue: answer.string("state")) else { return }
        github[serverID] = state
        defaults.set(state.rawValue, forKey: "github-\(serverID)")
    }

    /// Why Linear can't be shown here, when it can't.
    var linearUnavailable: String? {
        guard let target = panelTarget, let server = server(target.serverID) else { return nil }
        return server.protocolVersion >= 12 ? nil : "Update \(server.name) to connect it to Linear."
    }

    /// Asks the server for its GitHub repositories, unless it listed them in the last minute or
    /// `fresh` asks again anyway. The ones it listed before stay until it answers.
    func loadRepos(_ serverID: String, fresh: Bool = false) {
        guard !listingRepos.contains(serverID) else { return }
        if !fresh, let listed = reposListed[serverID], Date().timeIntervalSince(listed) < 60 { return }
        listingRepos.insert(serverID)
        repoErrors[serverID] = nil
        core.send("request", ["server_id": serverID, "request": ["type": "github_repos"]]) { [weak self] result in
            guard let self else { return }
            listingRepos.remove(serverID)
            switch result {
            case .success(let answer) where answer.string("type") == "repos":
                repos[serverID] = answer.objects("repos").map { Repo(json: $0) }
                reposListed[serverID] = Date()
            case .success(let answer):
                repos[serverID] = nil
                heard(github: answer, serverID: serverID)
            case .failure(let error):
                repoErrors[serverID] = error.message
            }
        }
    }

    func newProject(named name: String, on serverID: String) {
        add(["type": "new_project", "name": name], named: name, on: serverID)
    }

    func clone(_ repo: String, on serverID: String) {
        add(["type": "clone_repo", "repo": repo], named: repo, on: serverID)
    }

    private func add(_ request: JSON, named name: String, on serverID: String) {
        guard addingProject == nil else { return }
        addingProject = AddingProject(serverID: serverID, name: name)
        panelNotice = nil
        core.send("request", ["server_id": serverID, "request": request]) { [weak self] result in
            guard let self else { return }
            addingProject = nil
            switch result {
            case .success(let answer):
                projectsAdded += 1
                awaitedProjectID = answer.string("project_id")
                openAwaitedProject()
            case .failure(let error) where panel != nil:
                panelNotice = error.message
            case .failure(let error):
                errorMessage = error.message
            }
        }
    }

    /// Starts a thread in the project that was just made, once the server has told about it.
    private func openAwaitedProject() {
        guard let project = project(awaitedProjectID) else { return }
        awaitedProjectID = nil
        guard selectedDraft == nil else { return setNewThreadProject(project.id) }
        startNewThread(in: project)
    }

    // MARK: Branches

    /// Whether a turn is running in the project's folder, which is when its branch can't be
    /// switched.
    func isWorking(in project: Project) -> Bool {
        threads.values.contains { $0.projectID == project.id && $0.running && $0.cwd == project.path }
    }

    /// Whether the project's server is new enough to start threads in worktrees of their own.
    func canUseWorktrees(of project: Project) -> Bool {
        project.branch != nil && (server(project.serverID)?.protocolVersion ?? 0) >= 6
    }

    /// Whether the open draft starts its thread in a new worktree.
    var draftUsesWorktree: Bool {
        guard let draft = selectedDraft, let project = project(draft.projectID) else { return false }
        return draft.worktree == true && canUseWorktrees(of: project)
    }

    /// The branch the open draft's worktree starts from: the one picked, the one picked last in
    /// its project, or the default one.
    var draftBase: String? {
        guard let draft = selectedDraft, let project = project(draft.projectID) else { return nil }
        return draft.base ?? defaults.string(forKey: "new.base-\(project.id)") ?? project.git?.defaultBranch ?? project.branch
    }

    /// The project and the base of the open draft's worktree, when its thread starts in one.
    var draftWorktreeKey: String? {
        guard draftUsesWorktree, let projectID = selectedDraft?.projectID, let base = draftBase else { return nil }
        return "\(projectID) \(base)"
    }

    /// What the open draft's worktree starts at: its base, or the remote's when that has commits
    /// the local one lacks.
    var draftStart: String? {
        guard let base = draftBase else { return nil }
        return draftWorktreeKey.flatMap { worktreeStarts[$0] } ?? base
    }

    /// Asks the project's server what the open draft's worktree would start at. With `fetch` the
    /// remote is asked first, and a fetch that fails is said.
    func readWorktreeStart(fetch: Bool) {
        guard let key = draftWorktreeKey, let base = draftBase, let project = project(selectedDraft?.projectID),
            (server(project.serverID)?.protocolVersion ?? 0) >= 13
        else { return }
        let request: JSON = ["type": "worktree_start", "project_id": project.id, "base": base, "fetch": fetch]
        core.send("request", ["server_id": project.serverID, "request": request]) { [weak self] result in
            guard let self, case .success(let answer) = result else { return }
            worktreeStarts[key] = answer.string("start")
            guard let problem = answer.optionalString("problem") else { return }
            show(GitNotice(checkoutID: project.checkoutID, title: "Couldn't fetch \(base)", description: problem, failed: true))
        }
    }

    /// The open draft's base, when its worktree would start from the remote's instead, which has
    /// commits the local one lacks.
    var draftBaseToPull: String? {
        guard let base = draftBase, let start = draftStart, start != base, let project = project(selectedDraft?.projectID),
            (server(project.serverID)?.protocolVersion ?? 0) >= 20
        else { return nil }
        return base
    }

    var pullingDraftBase: Bool {
        draftWorktreeKey.map(pullingBases.contains) ?? false
    }

    /// Brings the open draft's local base up to the remote's, for its worktree to start from it.
    func pullDraftBase() {
        guard let key = draftWorktreeKey, let base = draftBaseToPull, let project = project(selectedDraft?.projectID),
            pullingBases.insert(key).inserted
        else { return }
        gitNotice = nil
        let request: JSON = ["type": "update_base", "project_id": project.id, "base": base]
        core.send("request", ["server_id": project.serverID, "request": request]) { [weak self] result in
            guard let self else { return }
            pullingBases.remove(key)
            switch result {
            case .success(let answer):
                worktreeStarts[key] = answer.string("start")
                show(GitNotice(checkoutID: project.checkoutID, title: "Pulled \(base)"))
            case .failure(let error):
                show(GitNotice(checkoutID: project.checkoutID, title: "Couldn't pull \(base)", description: error.message, failed: true))
            }
        }
    }

    func setDraftWorktree(_ worktree: Bool) {
        updateDraft { $0.worktree = worktree }
        readGit(fetch: true)
    }

    func setDraftBase(_ branch: String) {
        updateDraft { $0.base = branch }
        guard let projectID = selectedDraft?.projectID else { return }
        defaults.set(branch, forKey: "new.base-\(projectID)")
    }

    /// Whether the project's server is new enough to list and switch branches.
    func canSwitchBranches(of project: Project) -> Bool {
        project.branch != nil && (server(project.serverID)?.protocolVersion ?? 0) >= 3
    }

    /// Opens the branch picker at once, on the branches last listed or on placeholders, and
    /// lists them again.
    func showBranches(of project: Project) {
        listedBranches = knownBranches[project.id].map { .success($0) }
        branchesProjectID = project.id
        showsBranches = true
        listBranches(of: project)
    }

    /// Lists the branches of the project the composer is on before the picker is opened.
    private func listComposerBranches() {
        guard let project = composerProject, canSwitchBranches(of: project) else { return }
        listBranches(of: project)
    }

    private func listBranches(of project: Project) {
        let request: JSON = ["type": "branches", "project_id": project.id]
        core.send("request", ["server_id": project.serverID, "request": request]) { [weak self] result in
            guard let self else { return }
            let listed = result.map { $0.objects("branches").map { Branch(json: $0) } }
            if case .success(let branches) = listed { knownBranches[project.id] = branches }
            guard branchesProjectID == project.id, showsBranches else { return }
            listedBranches = listed
        }
    }

    /// Checks the branch out in the project's folder, making it first if asked to. `done` gets
    /// what went wrong, if anything.
    func switchBranch(of project: Project, to name: String, create: Bool, done: @escaping (String?) -> Void) {
        let request: JSON = ["type": "switch_branch", "project_id": project.id, "branch": name, "create": create]
        core.send("request", ["server_id": project.serverID, "request": request]) { result in
            switch result {
            case .success: done(nil)
            case .failure(let error): done(error.message)
            }
        }
    }

    // MARK: Git

    /// Whether the project's server is new enough to commit, push and open pull requests.
    func canUseGit(of project: Project) -> Bool {
        (server(project.serverID)?.protocolVersion ?? 0) >= 4
    }

    /// Whether the project's folder is no git repository and its server can make it one.
    func canInitializeGit(of project: Project) -> Bool {
        !project.noProject && project.branch == nil && project.git == nil && (server(project.serverID)?.protocolVersion ?? 0) >= 16
    }

    func initializeGit(in project: Project) {
        guard initializingGit.insert(project.id).inserted else { return }
        let request: JSON = ["type": "init_repository", "project_id": project.id]
        core.send("request", ["server_id": project.serverID, "request": request]) { [weak self] result in
            self?.initializingGit.remove(project.id)
            guard case .failure(let error) = result else { return }
            self?.errorMessage = error.message
        }
    }

    /// Has the server read the repository git works in from here again, which the project then
    /// arrives with. With `fetch` the remote is asked first, and a fetch that fails is said.
    private func readGit(fetch: Bool = false, done: (([ChangedFile]) -> Void)? = nil) {
        if fetch { listComposerBranches() }
        guard let project = gitProject else { return }
        var request: JSON = ["type": "git_status", "project_id": project.id, "fetch": fetch]
        if let thread = selectedThread { request["thread_id"] = thread.id }
        core.send("request", ["server_id": project.serverID, "request": request]) { [weak self] result in
            switch result {
            case .success(let answer):
                if let problem = answer.optionalString("problem") {
                    let title = "Couldn't fetch from the remote"
                    self?.show(GitNotice(checkoutID: project.checkoutID, title: title, description: problem, failed: true))
                }
                done?(answer.objects("files").map { ChangedFile(json: $0) })
            // Only said when the user is waiting for the answer.
            case .failure(let error): if done != nil { self?.errorMessage = error.message }
            }
        }
    }

    /// What the run in the project's checkout is at, started here or from another client.
    func gitStage(in project: Project) -> GitStage? {
        if let stage = gitStages[project.checkoutID] { return stage }
        guard let thread = selectedThread, thread.projectID == project.id else { return nil }
        return thread.gitStage
    }

    /// What a run starts with, until your server says what it is at.
    private func firstStage(of action: String, in project: Project, written: Bool) -> GitStage {
        switch action {
        case "pull": return .pull
        case "push": return .push
        case "create_pr":
            let pushes = project.gitControl?.menu.contains { $0.action == "push" && $0.reason == nil } ?? false
            return pushes ? .push : .pullRequestText
        default: return written ? .commit : .message
        }
    }

    /// What a click on the git button does: the one action the repository calls for, at once.
    func runQuickGit(in project: Project) {
        guard let quick = project.gitControl?.quick, gitStage(in: project) == nil else { return }
        if let url = quick.url.flatMap({ URL(string: $0) }) {
            showPullRequest(url)
            return
        }
        guard let action = quick.action else {
            show(GitNotice(checkoutID: project.checkoutID, title: quick.hint ?? quick.label))
            return
        }
        startGit(action, in: project, confirm: quick.confirm)
    }

    /// What a pick from the menu does: a commit opens its sheet, the others happen at once.
    func chooseGit(_ item: GitMenuItem, in project: Project) {
        guard item.reason == nil, gitStage(in: project) == nil else { return }
        guard item.action == "commit" else { return startGit(item.action, in: project, confirm: item.confirm) }
        // A sheet takes its size from what it opens with, so the files come first.
        readGit { [weak self] files in
            self?.gitFiles = files
            self?.committingProject = project
        }
    }

    /// Runs the action, after asking where when it would push from the default branch.
    private func startGit(_ action: String, in project: Project, confirm: GitConfirm?) {
        guard let confirm else { return runGit(action, in: project) }
        pendingGit = PendingGit(project: project, action: action, confirm: confirm)
    }

    /// Carries on with the action that waited, on the default branch or on a branch made for it.
    func confirmGit(_ pending: PendingGit, onNewBranch: Bool) {
        runGit(pending.action, in: pending.project, message: pending.message, paths: pending.paths, newBranch: onNewBranch)
    }

    /// Has the project's server carry the action out. It writes the commit message when there
    /// is none, and the pull request. What it did, or what git refused, shows under the button.
    func runGit(_ action: String, in project: Project, message: String? = nil, paths: [String] = [], newBranch: Bool = false) {
        let checkoutID = project.checkoutID
        guard gitStage(in: project) == nil else { return }
        gitStages[checkoutID] = firstStage(of: action, in: project, written: message?.isEmpty == false && !newBranch)
        gitNotice = nil
        var command: JSON = [
            "server_id": project.serverID, "project_id": project.id, "action": action, "paths": paths, "new_branch": newBranch,
        ]
        if let message, !message.isEmpty { command["message"] = message }
        if let thread = selectedThread, thread.projectID == project.id { command["thread_id"] = thread.id }
        core.send("git_run", command) { [weak self] result in
            guard let self else { return }
            self.gitStages[checkoutID] = nil
            switch result {
            case .success(let done):
                let notice = GitNotice(
                    checkoutID: checkoutID, title: done.string("title"), description: done.optionalString("description"),
                    url: done.optionalString("url"), next: done.optionalString("next")
                )
                self.show(notice)
            case .failure(let error):
                self.show(GitNotice(checkoutID: checkoutID, title: "Git stopped", description: error.message, failed: true))
            }
        }
    }

    /// What worked goes away by itself; what failed stays until it is closed.
    private func show(_ notice: GitNotice) {
        gitNotice = notice
        guard !notice.failed else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 10) { [weak self] in
            if self?.gitNotice == notice { self?.gitNotice = nil }
        }
    }

    func dismissGitNotice() {
        gitNotice = nil
    }

    /// The action a notice says comes next, like the push after a commit.
    func runNextGit() {
        guard let notice = gitNotice, let next = notice.next else { return }
        guard let project = gitProject, project.checkoutID == notice.checkoutID else { return }
        let confirm = project.gitControl?.menu.first { $0.action == next }?.confirm
        gitNotice = nil
        startGit(next, in: project, confirm: confirm)
    }

    /// What the agents spent on the connected `servers`, or all of them, in the last `buckets`
    /// spans of `bucketSeconds`, by this device's clock. `kept` answers at once with what was last
    /// read.
    func loadUsage(
        bucketSeconds: Int, buckets: Int, kept: Bool, servers: Set<String>?,
        reply: @escaping (Result<UsageReport, CoreBridge.CoreError>) -> Void
    ) {
        var command: JSON = [
            "bucket_secs": bucketSeconds, "buckets": buckets, "utc_offset_secs": TimeZone.current.secondsFromGMT(), "kept": kept,
        ]
        if let servers { command["servers"] = Array(servers) }
        core.send("usage", command, read: UsageReport.init, reply: reply)
    }

    /// How much of their plans the agents' logins on the connected `servers`, or all of them,
    /// have used. `refresh` has the servers read it anew, `kept` answers at once with what was
    /// last read.
    func loadLimits(
        refresh: Bool, kept: Bool, servers: Set<String>?, reply: @escaping (Result<LimitsReport, CoreBridge.CoreError>) -> Void
    ) {
        var command: JSON = ["refresh": refresh, "kept": kept]
        if let servers { command["servers"] = Array(servers) }
        core.send("limits", command, read: LimitsReport.init, reply: reply)
    }

    /// Picks the model that writes titles, commit messages and pull requests on the server.
    /// Without one, the lightest model of the thread's agent writes.
    func setTextModel(_ model: String?, on server: Server) {
        var command: JSON = ["server_id": server.id]
        if let model { command["model"] = model }
        core.send("set_text_model", command) { [weak self] result in
            if case .failure(let error) = result { self?.errorMessage = error.message }
        }
    }

    /// Says how the server's writer names branches. Without instructions the server goes back
    /// to its own.
    func setBranchInstructions(_ instructions: String?, on server: Server) {
        var command: JSON = ["server_id": server.id]
        if let instructions { command["instructions"] = instructions }
        core.send("set_branch_instructions", command) { [weak self] result in
            if case .failure(let error) = result { self?.errorMessage = error.message }
        }
    }

    /// Sets the shell script that runs in every new worktree of the project, or takes it away.
    func setSetup(of project: Project, to script: String) {
        var change: JSON = ["type": "set_project_setup", "project_id": project.id]
        if !script.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { change["script"] = script }
        request(project.serverID, change)
    }

    func removeProject(_ project: Project) {
        request(project.serverID, ["type": "remove_project", "project_id": project.id])
    }

    /// Makes an image on the project's server its icon. Without one, the project goes back to the
    /// icon found in its folder.
    func setIcon(of project: Project, to path: String?) {
        var command: JSON = ["server_id": project.serverID, "project_id": project.id]
        if let path { command["path"] = path }
        core.send("set_project_icon", command) { [weak self] result in
            if case .failure(let error) = result { self?.errorMessage = error.message }
        }
    }

    // MARK: Settings

    func openSettings(_ section: SettingsSection = .general, target: String? = nil) {
        guard account.signedIn else { return }
        showsUsage = false
        settings = section
        settingsTarget = target
    }

    func closeSettings() {
        settings = nil
        settingsTarget = nil
        settingsQuery = ""
    }

    func openUsage() {
        guard account.signedIn else { return }
        closeSettings()
        showsUsage = true
    }

    /// What Esc does when the focused view claims nothing: leaves a route, or restores the
    /// maximized side panel. Nothing while the command panel, the viewer or the branches have it.
    var escapes: (() -> Void)? {
        guard panel == nil, viewing == nil, !showsBranches else { return nil }
        if settings != nil || showsUsage { return { [self] in closeRoute() } }
        if sidePanel.isMaximized { return { [self] in sidePanel.toggleMaximized() } }
        return nil
    }

    /// Leaves the settings or the usage for the threads.
    func closeRoute() {
        closeSettings()
        showsUsage = false
    }

    // MARK: Command panel

    func openPanel(_ page: PanelPage) {
        guard account.signedIn, !servers.isEmpty else { return }
        panel = page
        for server in servers where server.state == .connected && startsProjects(server) {
            if github[server.id] == nil {
                github[server.id] = defaults.string(forKey: "github-\(server.id)").flatMap(GitHubState.init)
            }
            readGitHub(server.id)
        }
    }

    func closePanel() {
        guard panel != nil else { return }
        panel = nil
        panelNotice = nil
        composerFocus += 1
    }

    // MARK: Threads

    func select(_ new: Selection) {
        lastSelection = nil
        showsUsage = false
        let reopens = selectedThread != nil && openThreadID == nil
        guard new != selection || reopens else { return }
        if let open = openThreadID {
            core.send("close_thread", ["thread_id": open])
            openThreadID = nil
        }
        let left = selectedDraft
        selection = new
        if let left, preview(of: left) == nil, !sendingDraftIDs.contains(left.id) { removeDraft(left.id) }
        openedDraftPreview = selectedDraft.flatMap { preview(of: $0) }
        activity = Activity()
        transcriptIsEmpty = true
        sidePanel.turns = []
        sidePanel.showAgents()
        agents = []
        guard case .thread(let id) = new, let thread = threads[id] else {
            transcript.begin(threadID: nil)
            defaults.set(draftKey, forKey: "selection")
            ensureDraftProject()
            readGit(fetch: true)
            return
        }
        open(thread)
    }

    private func open(_ thread: ThreadInfo) {
        openThreadID = thread.id
        activity = Activity(thread: thread)
        transcript.begin(threadID: thread.id, keepingRows: true)
        defaults.set(thread.id, forKey: "selection")
        core.send("open_thread", ["server_id": thread.serverID, "thread_id": thread.id]) { [weak self] result in
            guard case .failure = result, let self, self.transcript.threadID == thread.id else { return }
            self.transcript.dropKeptRows()
        }
        core.send("mark_seen", ["thread_id": thread.id])
        readGit(fetch: true)
    }

    /// What the toolbar button and ⌘N do: with one project there is nothing to pick and the draft
    /// opens at once, without one it starts without a project, and with more the panel asks which.
    func newThread() {
        guard projects.count > 1 else { return startNewThread(in: projects.first ?? noProjects.first) }
        openPanel(.projects)
    }

    /// Opens an empty draft. The one that was open stays in the sidebar if something was written
    /// in it.
    func startNewThread(in project: Project? = nil) {
        openEmptyDraft()
        if let project { setNewThreadProject(project.id) }
        newThreadsStarted += 1
    }

    /// Where the client goes when what was open is gone.
    private func openEmptyDraft() {
        select(.draft(emptyDraft().id))
    }

    func setNewThreadProject(_ id: String?) {
        updateDraft {
            $0.projectID = id
            $0.base = nil
        }
        uploadToComposerServer()
        readGit(fetch: true)
    }

    func discard(_ draft: ThreadDraft) {
        let wasOpen = selection == .draft(draft.id)
        removeDraft(draft.id)
        guard wasOpen else { return }
        if let next = threadDrafts.last(where: { !sendingDraftIDs.contains($0.id) }) { return select(.draft(next.id)) }
        if let next = activeThreads.first { return select(.thread(next.id)) }
        openEmptyDraft()
    }

    var canSend: Bool {
        guard !sendingDraftIDs.contains(draftKey), let server = composerServer, server.state == .connected else { return false }
        if selectedThread == nil && project(selectedDraft?.projectID) == nil { return false }
        guard attachments.allSatisfy({ $0.state == .ready && $0.serverID == server.id }) else { return false }
        return !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !attachments.isEmpty
    }

    /// Why a message with these attachments can't be sent yet.
    var attachmentsHold: String? {
        if attachments.contains(where: { if case .failed = $0.state { true } else { false } }) {
            return "Try the attachment that failed again, or remove it"
        }
        return attachments.contains { $0.state != .ready } ? "Waiting for the attachments to upload" : nil
    }

    /// Whether a message sent while the agent works steers the turn that runs instead of waiting for it.
    static let steersKey = "send.steers"
    /// It steers until the setting says otherwise.
    static let steersByDefault = true

    func send() {
        guard canSend else { return }
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        let attached = attachments
        let key = draftKey
        var command: JSON = ["text": text, "attachments": attached.compactMap(\.path)]
        let existing = selectedThread
        let steers = existing != nil && (defaults.object(forKey: AppStore.steersKey) as? Bool ?? AppStore.steersByDefault)
        if let thread = existing {
            command["server_id"] = thread.serverID
            command["thread_id"] = thread.id
            command["now"] = steers
        } else {
            guard let draft = selectedDraft, let project = project(draft.projectID), let model = composerModel else {
                errorMessage = "No agent is installed. Install Claude Code or Codex on your server and try again."
                return
            }
            var settings: JSON = [
                "project_id": project.id,
                "agent": model.agent.rawValue,
                "model": model.id,
                "agent_account": composerAccount?.id ?? model.agent.rawValue,
                "access": draft.access.rawValue,
                "plan": draft.plan,
            ]
            if let effort = composerEffort { settings["effort"] = effort }
            if draftUsesWorktree, let base = draftBase {
                var worktree: JSON = ["base": base]
                settings["worktree"] = worktree
            }
            command["server_id"] = project.serverID
            command["new_thread"] = settings
            sendingDraftIDs.insert(draft.id)
            activity = Activity.starting
            transcript.setActivity(activity)
        }
        let serverID = command.string("server_id")
        draft = ""
        attachmentsByKey[key] = nil
        // The server queues what is sent while the agent works, unless the message steers it.
        transcript.setPending(text, attachments: attached.map(\.attached), queued: existing != nil && activity.running && !steers)
        transcriptIsEmpty = false

        core.send("send", command) { [weak self] result in
            guard let self else { return }
            if existing == nil { self.sendingDraftIDs.remove(key) }
            switch result {
            case .failure(let error):
                // The message goes back to where it was written, wherever the client is now.
                self.setText(text, for: key)
                self.attachmentsByKey[key] = attached.isEmpty ? nil : attached
                self.errorMessage = error.message
                guard self.draftKey == key else { return }
                self.transcript.setPending(nil)
                self.transcriptIsEmpty = self.transcript.isEmpty
                self.activity = existing == nil ? Activity() : self.activity
                self.transcript.setActivity(self.activity)
            case .success(let value):
                guard existing == nil else { return }
                self.openNewThread(id: value.string("thread_id"), serverID: serverID, draftID: key)
            }
        }
    }

    /// Replaces a draft with the thread its first message created. If the draft is still open,
    /// the thread opens in its place with the message kept on screen.
    private func openNewThread(id: String, serverID: String, draftID: String) {
        let wasOpen = selection == .draft(draftID)
        removeDraft(draftID)
        sidePanel.move(from: draftID, to: id)
        guard wasOpen else { return }
        selection = .thread(id)
        openThreadID = id
        defaults.set(id, forKey: "selection")
        transcript.adopt(threadID: id)
        core.send("open_thread", ["server_id": serverID, "thread_id": id])
        core.send("mark_seen", ["thread_id": id])
    }

    // MARK: Images and videos

    /// The file of an image or a video the open thread shows. The core fetches it from the
    /// thread's server if this Mac doesn't have it.
    func media(_ id: String, done: @escaping (URL?) -> Void) {
        guard let serverID = selectedThread?.serverID ?? composerServer?.id else { return done(nil) }
        core.send("media", ["server_id": serverID, "media_id": id]) { result in
            guard case .success(let value) = result, let path = value["path"] as? String else { return done(nil) }
            done(URL(fileURLWithPath: path))
        }
    }

    func refreshMediaStorage() {
        core.send("storage") { [weak self] result in
            guard case .success(let value) = result else { return }
            let bytes = { (key: String) in (value[key] as? NSNumber)?.int64Value ?? 0 }
            self?.mediaStorage = MediaStorage(used: bytes("media_bytes"), limit: bytes("media_limit"))
        }
    }

    /// Removes the images and videos kept on this Mac. The servers still have them.
    func clearMedia() {
        core.send("clear_media") { [weak self] _ in self?.refreshMediaStorage() }
    }

    func stop() {
        guard let thread = selectedThread else { return }
        request(thread.serverID, ["type": "stop", "thread_id": thread.id])
    }

    /// Allows or refuses a tool call the agent waits with. `answers` is what was chosen, by
    /// question, when the call asks questions.
    func answer(_ approval: Approval, allow: Bool, answers: [String: String] = [:]) {
        guard let thread = selectedThread else { return }
        let answer: JSON = ["type": "answer", "thread_id": thread.id, "approval_id": approval.id, "allow": allow, "answers": answers]
        request(thread.serverID, answer)
    }

    /// Gives the agent a queued message now, in the turn that runs.
    func sendNow(queued messageID: String) {
        guard let thread = selectedThread else { return }
        request(thread.serverID, ["type": "send_queued", "thread_id": thread.id, "message_id": messageID])
    }

    /// Takes a queued message back into the composer of its thread, after what is written there.
    func takeBack(queued messageID: String) {
        guard let thread = selectedThread, let message = activity.queued.first(where: { $0.id == messageID }) else { return }
        let key = thread.id
        request(thread.serverID, ["type": "cancel_queued", "thread_id": thread.id, "message_id": messageID], done: { [weak self] in
            guard let self else { return }
            let written = [self.drafts[key] ?? "", message.text].filter { !$0.isEmpty }
            self.setText(written.joined(separator: "\n\n"), for: key)
            let back = message.attachments.map { Attachment(path: $0, shown: message.media[$0], serverID: thread.serverID) }
            let attached = (self.attachmentsByKey[key] ?? []) + back
            self.attachmentsByKey[key] = attached.isEmpty ? nil : attached
            self.composerFocus += 1
        })
    }

    func rename(_ thread: ThreadInfo, to title: String) {
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty, title != thread.title else { return }
        update(thread, ["title": title]) { $0.title = title }
    }

    func delete(_ thread: ThreadInfo) {
        request(thread.serverID, ["type": "delete", "thread_id": thread.id])
    }

    /// Has the thread or the draft work with `model` under `account`. A thread that moves to an
    /// account without its session goes on in a new one, which is told what was said.
    func setModel(_ model: ModelInfo, account: AgentAccount) {
        guard let thread = selectedThread else {
            updateDraft {
                $0.model = model.id
                $0.agentAccount = account.id
            }
            return
        }
        let effort = thread.effort.flatMap { model.efforts.contains($0) ? $0 : nil }
        var change: JSON = ["model": model.id, "effort": effort ?? ""]
        if account.id != thread.agentAccount { change["agent_account"] = account.id }
        update(thread, change) {
            $0.model = model.id
            $0.effort = effort
            $0.agent = model.agent
            $0.agentAccount = account.id
        }
    }

    /// Adds an account of an agent on the server, or changes one. `done` says whether it was kept.
    func saveAgentAccount(_ account: AgentAccount, on server: Server, done: @escaping (Bool) -> Void) {
        request(server.id, ["type": "save_agent_account", "account": account.json], done: { done(true) }, failed: { done(false) })
    }

    /// Removes an account. Its threads go on with the agent's default account.
    func removeAgentAccount(_ account: AgentAccount, on server: Server) {
        request(server.id, ["type": "remove_agent_account", "id": account.id])
    }

    func setEffort(_ effort: String) {
        guard let thread = selectedThread else {
            updateDraft { $0.effort = effort }
            return
        }
        update(thread, ["effort": effort]) { $0.effort = effort }
    }

    func setAccess(_ access: Access) {
        guard let thread = selectedThread else {
            updateDraft { $0.access = access }
            return
        }
        update(thread, ["access": access.rawValue]) { $0.access = access }
    }

    func setPlan(_ plan: Bool) {
        guard let thread = selectedThread else {
            updateDraft { $0.plan = plan }
            return
        }
        update(thread, ["plan": plan]) { $0.plan = plan }
    }

    /// Marks threads done or brings them back. A thread that is working or monitoring can't be
    /// marked done.
    func setDone(_ ids: [String], done: Bool, fromSidebar: Bool = false) {
        let changed = ids.compactMap { threads[$0] }.filter { $0.isDone != done && !(done && $0.busy) }
        guard !changed.isEmpty else { return }
        // Leaving the thread that was just put away, for the next one that is still active.
        if done, fromSidebar, case .thread(let open) = selection, changed.contains(where: { $0.id == open }) {
            let active = activeThreads
            let position = active.firstIndex { $0.id == open } ?? 0
            let remaining = active.filter { thread in !changed.contains { $0.id == thread.id } }
            let next = remaining.isEmpty ? nil : remaining[min(position, remaining.count - 1)]
            if let next { select(.thread(next.id)) } else { openEmptyDraft() }
        }
        let now = Date().timeIntervalSince1970
        for thread in changed {
            update(thread, ["done": done]) { $0.doneAt = done ? now : nil }
        }
        guard done else {
            undo = nil
            return
        }
        showUndo(UndoNotice(threadIDs: changed.map(\.id), text: changed.count == 1 ? "Marked done" : "Marked \(changed.count) threads done"))
    }

    /// Has the agent of the open thread go on with what it was doing when it was interrupted.
    func continueThread() {
        guard let thread = selectedThread else { return }
        request(thread.serverID, ["type": "continue", "thread_id": thread.id])
    }

    /// Whether the open thread goes on by itself once its usage limit resets.
    func setContinues(_ on: Bool) {
        guard let thread = selectedThread, case .limit(let resetsAt, _) = thread.interruption else { return }
        update(thread, ["continues": on]) { $0.interruption = .limit(resetsAt: resetsAt, continues: on) }
    }

    func toggleDone() {
        guard let thread = selectedThread else { return }
        setDone([thread.id], done: !thread.isDone)
    }

    /// Whether the user can move the thread among the active ones.
    func moves(_ thread: ThreadInfo) -> Bool {
        !thread.isDone && server(thread.serverID)?.movesThreads == true
    }

    /// Puts the thread where `index` is among the sidebar's rows without it: `rows` is the
    /// active thread of each, or `nil` for a row that isn't one.
    func move(_ thread: ThreadInfo, to index: Int, among rows: [ThreadInfo?]) {
        let others = rows.filter { $0?.id != thread.id }
        let neighbour = { (at: Int) -> ThreadInfo? in others.indices.contains(at) ? others[at] : nil }
        move(thread, under: neighbour(index - 1), over: neighbour(index))
    }

    /// Puts the thread between two active ones in the sidebar, or at the top or the bottom when
    /// one of them is missing. Its position is kept on its server.
    private func move(_ thread: ThreadInfo, under above: ThreadInfo?, over below: ThreadInfo?) {
        guard moves(thread) else { return }
        let position: Double
        switch (above, below) {
        case (nil, nil): return
        case (nil, let below?): position = max(Date().timeIntervalSince1970, below.position + 1)
        case (let above?, nil): position = above.position - 1
        case (let above?, let below?): position = (above.position + below.position) / 2
        }
        guard position != thread.position else { return }
        update(thread, ["position": position]) { $0.position = position }
    }

    private func showUndo(_ notice: UndoNotice) {
        undo = notice
        undoTimer?.invalidate()
        undoTimer = Timer.scheduledTimer(withTimeInterval: 5, repeats: false) { [weak self] _ in self?.undo = nil }
    }

    func performUndo() {
        guard let notice = undo else { return }
        undo = nil
        setDone(notice.threadIDs, done: false)
    }

    /// Changes a thread on its server, and here at once so the client doesn't wait for the answer.
    private func update(_ thread: ThreadInfo, _ change: JSON, locally: (inout ThreadInfo) -> Void) {
        var changed = thread
        locally(&changed)
        setThread(changed, id: thread.id)
        request(thread.serverID, ["type": "update", "thread_id": thread.id, "change": change], failed: { [weak self] in
            self?.setThread(thread, id: thread.id)
        })
    }

    private func request(_ serverID: String, _ request: JSON, done: (() -> Void)? = nil, failed: (() -> Void)? = nil) {
        core.send("request", ["server_id": serverID, "request": request]) { [weak self] result in
            switch result {
            case .success: done?()
            case .failure(let error):
                failed?()
                self?.errorMessage = error.message
            }
        }
    }

    // MARK: Attachments

    /// Adds the files to what is being written and starts sending them to its server, so they
    /// are there when the message is sent.
    func attach(_ urls: [URL]) {
        guard let server = composerServer else { return }
        let key = draftKey
        for url in urls where url.isFileURL && !attachments.contains(where: { $0.file == url }) {
            let attachment = Attachment(file: url, serverID: server.id)
            attachmentsByKey[key, default: []].append(attachment)
            upload(attachment.id)
        }
        composerFocus += 1
    }

    func removeAttachment(_ id: String) {
        if case .uploading = attachments.first(where: { $0.id == id })?.state {
            core.send("cancel_upload", ["key": id])
        }
        attachmentsByKey[draftKey]?.removeAll { $0.id == id }
        if attachmentsByKey[draftKey]?.isEmpty == true { attachmentsByKey[draftKey] = nil }
    }

    func retryAttachment(_ id: String) {
        upload(id)
    }

    private func changeAttachment(_ id: String, _ change: (inout Attachment) -> Void) {
        for (key, attached) in attachmentsByKey {
            guard let index = attached.firstIndex(where: { $0.id == id }) else { continue }
            change(&attachmentsByKey[key]![index])
            return
        }
    }

    private func upload(_ id: String) {
        guard let attachment = attachmentsByKey.values.joined().first(where: { $0.id == id }), let file = attachment.file else { return }
        let serverID = attachment.serverID
        changeAttachment(id) { $0.state = .uploading(0) }
        core.send("upload", ["server_id": serverID, "key": id, "file": file.path]) { [weak self] result in
            guard let self else { return }
            switch result {
            case .failure(let error):
                self.changeAttachment(id) { attachment in
                    // An upload that was stopped to go to another server is on its way there.
                    guard attachment.serverID == serverID else { return }
                    attachment.state = .failed(error.message)
                }
            case .success(let value):
                let (path, media) = (value.string("path"), value.optionalString("media"))
                self.changeAttachment(id) {
                    $0.path = path
                    $0.media = media
                }
                guard attachment.video, media != nil else { return self.changeAttachment(id) { $0.state = .ready } }
                self.uploadPoster(of: id, file: file, path: path, serverID: serverID)
            }
        }
    }

    /// Sends the first frame of a video after it, which stands for it wherever it isn't played.
    /// A video without one is sent all the same.
    private func uploadPoster(of id: String, file: URL, path: String, serverID: String) {
        let ready = { [weak self] (poster: String?) in
            self?.changeAttachment(id) { attachment in
                guard attachment.path == path else { return }
                attachment.poster = poster
                attachment.state = .ready
            }
            return
        }
        Pictures.firstFrame(file, id: "poster:\(id)", maxPixels: 1280) { [weak self] image in
            guard let self, let image else { return ready(nil) }
            DispatchQueue.global(qos: .userInitiated).async {
                let poster = Pictures.writeJPEG(image)
                DispatchQueue.main.async {
                    guard let poster else { return ready(nil) }
                    let upload: JSON = ["server_id": serverID, "key": "\(id).poster", "file": poster.path, "poster_of": path]
                    self.core.send("upload", upload) { result in
                        try? FileManager.default.removeItem(at: poster)
                        guard case .success(let value) = result else { return ready(nil) }
                        ready(value.optionalString("media"))
                    }
                }
            }
        }
    }

    /// Sends the open draft's files again when its project is on another server than they are.
    private func uploadToComposerServer() {
        guard let server = composerServer else { return }
        for attachment in attachments where attachment.serverID != server.id && attachment.file != nil {
            core.send("cancel_upload", ["key": attachment.id])
            changeAttachment(attachment.id) {
                $0.serverID = server.id
                $0.path = nil
                $0.media = nil
                $0.poster = nil
            }
            upload(attachment.id)
        }
    }

    // MARK: Viewer

    func view(_ media: [ViewedMedia], at index: Int) {
        guard media.indices.contains(index) else { return }
        viewing = Viewing(items: media, index: index)
    }

    /// Shows the image or video before or after the one that is shown, around the ends.
    func viewNext(_ step: Int) {
        guard let count = viewing?.items.count, count > 1, let index = viewing?.index else { return }
        viewing?.index = (index + step + count) % count
    }

    func closeViewer() {
        viewing = nil
        composerFocus += 1
    }

    /// Attaches what was dropped on the window: files, and images that aren't files yet.
    func attach(dropped providers: [NSItemProvider]) {
        for provider in providers {
            if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
                _ = provider.loadObject(ofClass: URL.self) { url, _ in
                    guard let url else { return }
                    DispatchQueue.main.async { self.attach([url]) }
                }
                continue
            }
            guard let type = ImageFiles.attachable.first(where: { provider.hasItemConformingToTypeIdentifier($0.identifier) }) else { continue }
            provider.loadDataRepresentation(forTypeIdentifier: type.identifier) { data, _ in
                guard let data, let file = ImageFiles.saveForAttaching(data, type: type) else { return }
                DispatchQueue.main.async { self.attach([file]) }
            }
        }
    }
}

extension Activity {
    /// What is shown between sending a first message and the server saying the turn runs.
    static var starting: Activity {
        var activity = Activity()
        activity.running = true
        activity.startedAt = Date().timeIntervalSince1970
        return activity
    }
}

/// Shows the browser sheet for signing in and reports the address it was sent back to.
final class SignInSession: NSObject, ASWebAuthenticationPresentationContextProviding {
    private var session: ASWebAuthenticationSession?

    func start(url: URL, done: @escaping (URL?) -> Void) {
        let session = ASWebAuthenticationSession(url: url, callbackURLScheme: "motile") { callback, _ in
            DispatchQueue.main.async { done(callback) }
        }
        session.presentationContextProvider = self
        self.session = session
        if !session.start() {
            done(nil)
        }
    }

    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        #if os(macOS)
        NSApp.keyWindow ?? NSApp.windows.first ?? ASPresentationAnchor()
        #else
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        return scenes.flatMap(\.windows).first(where: \.isKeyWindow) ?? scenes.first.map { ASPresentationAnchor(windowScene: $0) } ?? ASPresentationAnchor()
        #endif
    }
}
