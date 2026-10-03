import Foundation

// What the core sends, read from its JSON. The shapes are defined in crates/core/src/api.rs and
// crates/protocol/src/wire.rs.

typealias JSON = [String: Any]

extension Dictionary where Key == String, Value == Any {
    func string(_ key: String) -> String { self[key] as? String ?? "" }
    func optionalString(_ key: String) -> String? { self[key] as? String }
    func bool(_ key: String) -> Bool { self[key] as? Bool ?? false }
    func int(_ key: String) -> Int { (self[key] as? NSNumber)?.intValue ?? 0 }
    func double(_ key: String) -> Double { (self[key] as? NSNumber)?.doubleValue ?? 0 }
    func optionalDouble(_ key: String) -> Double? { (self[key] as? NSNumber)?.doubleValue }
    func object(_ key: String) -> JSON? { self[key] as? JSON }
    func objects(_ key: String) -> [JSON] { self[key] as? [JSON] ?? [] }
    func strings(_ key: String) -> [String] { self[key] as? [String] ?? [] }
}

struct Account: Equatable {
    var signedIn = false
    var email = ""
    var name: String?
    var deviceKey = ""
    var authURL = ""
    var error: String?

    init() {}

    init(json: JSON) {
        signedIn = json.bool("signed_in")
        let user = json.object("user")
        email = user?.string("email") ?? ""
        name = user?.optionalString("name")
        deviceKey = json.string("device_key")
        authURL = json.string("auth_url")
        error = json.optionalString("error")
    }
}

enum Agent: String, CaseIterable {
    case claude, codex

    var name: String {
        switch self {
        case .claude: "Claude Code"
        case .codex: "Codex"
        }
    }
}

struct ModelInfo: Equatable, Identifiable {
    let id: String
    let name: String
    let agent: Agent
    let efforts: [String]
    let defaultEffort: String?

    init(json: JSON) {
        id = json.string("id")
        name = json.string("name")
        agent = Agent(rawValue: json.string("agent")) ?? .claude
        efforts = json.strings("efforts")
        defaultEffort = json.optionalString("default_effort")
    }
}

struct Server: Equatable, Identifiable {
    enum State: String {
        case connecting, connected, disconnected, refused
    }

    let id: String
    let name: String
    let platform: String
    let state: State
    let error: String?
    /// "relay" or "direct".
    let path: String?
    let rttMs: Int?
    let home: String
    /// The version of the server's program.
    let version: String
    let protocolVersion: Int
    let models: [ModelInfo]
    /// The model picked to write titles, commit messages and pull requests there.
    let textModel: String?
    /// How the writer there is told to name branches, and what that is until it is changed.
    let branchInstructions: String
    let defaultBranchInstructions: String
    /// The agents installed on the server, with their versions.
    let agents: [Agent: String]
    /// Whether the server has ever told us about itself.
    let known: Bool

    init(json: JSON) {
        id = json.string("id")
        name = json.string("name")
        platform = json.string("platform")
        state = State(rawValue: json.string("state")) ?? .connecting
        error = json.optionalString("error")
        path = json.optionalString("path")
        rttMs = (json["rtt_ms"] as? NSNumber)?.intValue
        let info = json.object("info")
        known = info != nil
        home = info?.string("home") ?? ""
        version = info?.string("version") ?? ""
        protocolVersion = (info?["protocol"] as? NSNumber)?.intValue ?? 0
        models = (info?.objects("models") ?? []).map { ModelInfo(json: $0) }
        textModel = info?.optionalString("text_model")
        let naming = info?.object("branch_instructions")
        branchInstructions = naming?.string("text") ?? ""
        defaultBranchInstructions = naming?.string("default") ?? ""
        var installed: [Agent: String] = [:]
        for agent in info?.objects("agents") ?? [] {
            guard let kind = Agent(rawValue: agent.string("agent")), let version = agent.optionalString("version") else { continue }
            installed[kind] = version
        }
        agents = installed
    }
}

/// A branch of a project's repository.
struct Branch: Equatable, Identifiable {
    let name: String
    let current: Bool
    /// The one the remote starts new work from.
    let isDefault: Bool
    /// Only on the remote so far.
    let remote: Bool

    var id: String { name }

    init(json: JSON) {
        name = json.string("name")
        current = json.bool("current")
        isDefault = json.bool("default")
        remote = json.bool("remote")
    }
}

struct GitStatus: Equatable {
    /// `nil` when no branch is checked out.
    let branch: String?
    /// The checked-out branch is the one the remote starts new work from.
    let isDefault: Bool
    let remote: Bool
    let added: Int
    let removed: Int
    let pullRequest: PullRequest?

    init(json: JSON) {
        branch = json.optionalString("branch")
        isDefault = json.bool("default")
        remote = json.bool("remote")
        added = json.int("added")
        removed = json.int("removed")
        pullRequest = json.object("pull_request").map { PullRequest(json: $0) }
    }
}

struct PullRequest: Equatable {
    let number: Int
    let title: String
    let url: String

    init(json: JSON) {
        number = json.int("number")
        title = json.string("title")
        url = json.string("url")
    }
}

/// The git button of a project, as the core worked it out: what it does when clicked, the menu
/// behind it and what is said under the menu.
struct GitControl: Equatable {
    let quick: GitQuick
    let menu: [GitMenuItem]
    let warning: String?

    init(json: JSON) {
        quick = GitQuick(json: json.object("quick") ?? [:])
        menu = json.objects("menu").map { GitMenuItem(json: $0) }
        warning = json.optionalString("warning")
    }
}

/// Names what a git action or its icon stands for, as the server spells it: "commit_push".
enum GitSymbol {
    static func name(for action: String?) -> String {
        switch action {
        case "pull": "icloud.and.arrow.down"
        case "push", "commit_push", "commit_push_pr": "icloud.and.arrow.up"
        case "create_pr", nil: "arrow.triangle.merge"
        default: "smallcircle.filled.circle"
        }
    }
}

/// What the button does: an action, or opening the pull request at `url`. With neither it is
/// off, and `hint` says why.
struct GitQuick: Equatable {
    let label: String
    let action: String?
    let url: String?
    let hint: String?
    let confirm: GitConfirm?

    init(json: JSON) {
        label = json.string("label")
        action = json.optionalString("action")
        url = json.optionalString("url")
        hint = json.optionalString("hint")
        confirm = json.object("confirm").map { GitConfirm(json: $0) }
    }
}

struct GitMenuItem: Equatable, Identifiable {
    let label: String
    let action: String
    /// Why it can't run now.
    let reason: String?
    let confirm: GitConfirm?

    var id: String { action }

    init(json: JSON) {
        label = json.string("label")
        action = json.string("action")
        reason = json.optionalString("reason")
        confirm = json.object("confirm").map { GitConfirm(json: $0) }
    }
}

/// Asked before an action that pushes from the default branch.
struct GitConfirm: Equatable {
    let title: String
    let description: String
    let proceed: String
    let branchOff: String

    init(json: JSON) {
        title = json.string("title")
        description = json.string("description")
        proceed = json.string("proceed")
        branchOff = json.string("branch_off")
    }
}

enum GitStage: String {
    case branch
    case message
    case commit
    case push
    case pullRequestText = "pull_request_text"
    case pullRequest = "pull_request"
    case pull

    var label: String {
        switch self {
        case .branch: "Branching"
        case .message: "Writing"
        case .commit: "Committing"
        case .push: "Pushing"
        case .pullRequestText: "Writing PR"
        case .pullRequest: "Creating PR"
        case .pull: "Pulling"
        }
    }
}

/// What a git action did, or why it couldn't, shown under the button until it is dismissed.
struct GitNotice: Equatable {
    let projectID: String
    let title: String
    var description: String?
    var failed = false
    /// The pull request to open.
    var url: String?
    /// The action that follows, like a push after a commit.
    var next: String?

    var nextLabel: String? {
        switch next {
        case "push": "Push"
        case "create_pr": "Create PR"
        default: nil
        }
    }
}

/// An action that waits for the user to say where it should happen.
struct PendingGit: Equatable, Identifiable {
    let project: Project
    let action: String
    let confirm: GitConfirm
    var message: String?
    var paths: [String] = []

    var id: String { "\(project.id):\(action)" }
}

/// A file with changes that aren't committed.
struct ChangedFile: Equatable, Identifiable {
    let path: String
    let change: String
    let added: Int
    let removed: Int

    var id: String { path }

    init(json: JSON) {
        path = json.string("path")
        change = json.string("change")
        added = json.int("added")
        removed = json.int("removed")
    }
}

/// A git worktree of a project, made for the thread whose `cwd` it is.
struct Worktree: Equatable {
    let path: String
    let branch: String?
    let git: GitStatus?
    let gitControl: GitControl?
}

struct Project: Equatable, Identifiable {
    let id: String
    let serverID: String
    let path: String
    let name: String
    private(set) var branch: String?
    /// What its server last read from git there.
    private(set) var git: GitStatus?
    /// The git button for its repository.
    private(set) var gitControl: GitControl?
    /// The worktree of the thread the project is seen from, when it works in one.
    private(set) var worktree: Worktree?
    let worktrees: [Worktree]
    /// The shell script that runs in every new worktree.
    let setup: String?
    /// The icon as a file on this Mac, once the core has fetched it.
    let iconPath: String?
    let createdAt: Double

    init(json: JSON, serverID: String) {
        id = json.string("id")
        self.serverID = serverID
        path = json.string("path")
        name = json.string("name")
        branch = json.optionalString("branch")
        git = json.object("git").map { GitStatus(json: $0) }
        gitControl = json.object("git_control").map { GitControl(json: $0) }
        let controls = json.object("worktree_controls")
        worktrees = json.objects("worktrees").map { worktree in
            let path = worktree.string("path")
            return Worktree(
                path: path, branch: worktree.optionalString("branch"), git: worktree.object("git").map { GitStatus(json: $0) },
                gitControl: controls?.object(path).map { GitControl(json: $0) })
        }
        setup = json.optionalString("setup")
        iconPath = json.optionalString("icon_path")
        createdAt = json.double("created_at")
    }

    /// The project as the thread works in it: with the branch and the git state of its worktree,
    /// when it has one.
    func seen(from thread: ThreadInfo) -> Project {
        guard let worktree = worktrees.first(where: { $0.path == thread.cwd }) else { return self }
        var seen = self
        seen.worktree = worktree
        seen.branch = worktree.branch
        seen.git = worktree.git
        seen.gitControl = worktree.gitControl
        return seen
    }
}

enum Access: String, CaseIterable, Identifiable, Codable {
    case supervised
    case acceptEdits = "accept_edits"
    case auto
    case full

    var id: String { rawValue }

    var label: String {
        switch self {
        case .supervised: "Supervised"
        case .acceptEdits: "Auto-accept edits"
        case .auto: "Auto"
        case .full: "Full access"
        }
    }

    var detail: String {
        switch self {
        case .supervised: "Ask before commands and file changes."
        case .acceptEdits: "Auto-approve edits, ask before other actions."
        case .auto: "The agent approves routine actions itself."
        case .full: "Allow commands and edits without prompts."
        }
    }

    var symbol: String {
        switch self {
        case .supervised: "lock"
        case .acceptEdits: "pencil.line"
        case .auto: "sparkles"
        case .full: "lock.open"
        }
    }
}

struct ThreadInfo: Equatable, Identifiable {
    let id: String
    let serverID: String
    var title: String
    let projectID: String
    let cwd: String
    let agent: Agent
    var model: String?
    var effort: String?
    var access: Access
    var plan: Bool
    let createdAt: Double
    let updatedAt: Double
    var doneAt: Double?
    let undoneAt: Double?
    let running: Bool
    /// The turn is over, but the agent still watches something it left running.
    let monitoring: Bool
    let needsApproval: Bool
    let turnEndedAt: Double?
    let unread: Bool

    init(json: JSON) {
        id = json.string("id")
        serverID = json.string("server_id")
        title = json.string("title")
        projectID = json.string("project_id")
        cwd = json.string("cwd")
        agent = Agent(rawValue: json.string("agent")) ?? .claude
        model = json.optionalString("model")
        effort = json.optionalString("effort")
        access = Access(rawValue: json.string("access")) ?? .full
        plan = json.bool("plan")
        createdAt = json.double("created_at")
        updatedAt = json.double("updated_at")
        doneAt = json.optionalDouble("done_at")
        undoneAt = json.optionalDouble("undone_at")
        running = json.bool("running")
        monitoring = json.bool("monitoring")
        needsApproval = json.bool("needs_approval")
        turnEndedAt = json.optionalDouble("turn_ended_at")
        unread = json.bool("unread")
    }

    var isDone: Bool { doneAt != nil }

    /// The agent's process is still there, working or monitoring.
    var busy: Bool { running || monitoring }

    /// Active threads keep their place when something happens in them; only coming back from
    /// done moves one to the top.
    var activeOrder: Double { max(createdAt, undoneAt ?? 0) }
}

struct Activity: Equatable {
    var running = false
    var monitoring = false
    var thinking = false
    var startedAt: Double?
    /// The tool calls the running turn waits with until they are allowed or refused.
    var approvals: [Approval] = []
    /// The messages that wait for the agent to take them.
    var queued: [QueuedMessage] = []

    var busy: Bool { running || monitoring }

    init() {}

    init(json: JSON, waiting: [JSON]) {
        running = json.bool("running")
        monitoring = json.bool("monitoring")
        thinking = json.bool("thinking")
        startedAt = json.optionalDouble("started_at")
        approvals = waiting.map { Approval(json: $0) }
        queued = json.objects("queued").map { QueuedMessage(json: $0) }
    }
}

/// A message sent while the agent was working. The transcript shows it as its last rows until
/// the agent takes it.
struct QueuedMessage: Equatable, Identifiable {
    let id: String
    let text: String
    /// The files attached to it, as paths on the server.
    let attachments: [String]

    init(json: JSON) {
        id = json.string("id")
        text = json.string("text")
        attachments = json.strings("attachments")
    }
}

/// A tool call the turn waits with until the user has answered it.
struct Approval: Equatable, Identifiable {
    let id: String
    /// What is asked for: the tool, or what to do with a plan.
    let title: String
    /// What the tool acts on: the command, the file.
    let target: String
    let symbol: String
    /// What the buttons that allow and refuse it say.
    let allowLabel: String
    let refuseLabel: String
    /// The questions the agent asks with it; allowing it takes an answer to each.
    let questions: [Question]

    init(json: JSON) {
        id = json.string("id")
        title = json.string("title")
        target = json.string("target")
        symbol = ToolContent.symbol(for: json.string("icon"))
        allowLabel = json.string("allow")
        refuseLabel = json.string("refuse")
        questions = json.objects("questions").map { Question(json: $0) }
    }
}

struct Question: Equatable, Identifiable {
    struct Choice: Equatable, Identifiable {
        let label: String
        let detail: String

        var id: String { label }
    }

    let text: String
    let options: [Choice]
    /// More than one option can be chosen.
    let multiple: Bool

    var id: String { text }

    init(json: JSON) {
        text = json.string("text")
        options = json.objects("options").map { Choice(label: $0.string("label"), detail: $0.string("detail")) }
        multiple = json.bool("multiple")
    }
}

struct EnrollToken {
    let command: String
    let expiresAt: Double

    init(json: JSON) {
        command = json.string("command")
        expiresAt = json.double("expires_at")
    }
}

enum GitHubState: String {
    case ready
    case signedOut = "signed_out"
    case missing
}

struct Repo: Equatable {
    /// `owner/name`.
    let name: String
    let description: String?
    let isPrivate: Bool

    init(json: JSON) {
        name = json.string("name")
        description = json.optionalString("description")
        isPrivate = json.bool("private")
    }
}

/// The folders of a server under the path typed in the panel.
struct FolderListing {
    struct Folder {
        let name: String
        let path: String
        /// What to type to look inside it.
        let typed: String
    }

    let path: String
    let typed: String
    let parent: String?
    let folders: [Folder]

    init(json: JSON) {
        path = json.string("path")
        typed = json.string("typed")
        parent = json.optionalString("parent")
        folders = json.objects("folders").map { Folder(name: $0.string("name"), path: $0.string("path"), typed: $0.string("typed")) }
    }
}

struct RemoteFolder {
    let path: String
    let parent: String?
    let folders: [String]
    /// The images in the folder, when an icon is being chosen.
    let files: [String]

    init(json: JSON) {
        path = json.string("path")
        parent = json.optionalString("parent")
        folders = json.strings("folders")
        files = json.strings("files")
    }
}

enum Time {
    /// "now", "5m", "3h", "2d": how long ago, as short as a sidebar needs.
    static func ago(_ timestamp: Double, now: Double = Date().timeIntervalSince1970) -> String {
        let seconds = max(0, now - timestamp)
        if seconds < 60 { return "now" }
        if seconds < 3600 { return "\(Int(seconds / 60))m" }
        if seconds < 86400 { return "\(Int(seconds / 3600))h" }
        return "\(Int(seconds / 86400))d"
    }

    /// "850ms", "12s", "3m 5s", "1h 2m".
    static func duration(milliseconds: Int) -> String {
        if milliseconds < 1000 { return "\(milliseconds)ms" }
        let seconds = milliseconds / 1000
        if seconds < 60 { return "\(seconds)s" }
        if seconds < 3600 { return "\(seconds / 60)m \(seconds % 60)s" }
        return "\(seconds / 3600)h \(seconds % 3600 / 60)m"
    }

    /// A running timer: "5s", "12m", "1h 3m".
    static func elapsed(since timestamp: Double, now: Double = Date().timeIntervalSince1970) -> String {
        let seconds = max(0, Int(now - timestamp))
        if seconds < 60 { return "\(seconds)s" }
        if seconds < 3600 { return "\(seconds / 60)m \(seconds % 60)s" }
        return "\(seconds / 3600)h \(seconds % 3600 / 60)m"
    }
}
