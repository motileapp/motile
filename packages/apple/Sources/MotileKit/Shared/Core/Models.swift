import Foundation
import UniformTypeIdentifiers

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
    var picture: URL?
    var deviceKey = ""
    var authURL = ""
    var error: String?

    init() {}

    init(json: JSON) {
        signedIn = json.bool("signed_in")
        let user = json.object("user")
        email = user?.string("email") ?? ""
        name = user?.optionalString("name")
        picture = user?.optionalString("picture").flatMap(URL.init(string:))
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
    /// The name without its maker, for the model picker.
    let shortName: String
    let agent: Agent
    /// The account whose agent lists it. Empty from a server that lists models for an agent as a whole.
    let account: String
    let efforts: [String]
    /// Missing when the agent applies its own default, as Claude Code does.
    let defaultEffort: String?

    init(json: JSON, shortNames: JSON?) {
        id = json.string("id")
        name = json.string("name")
        shortName = shortNames?.optionalString(id) ?? name
        agent = Agent(rawValue: json.string("agent")) ?? .claude
        account = json.string("account")
        efforts = json.strings("efforts")
        defaultEffort = json.optionalString("default_effort")
    }

    /// What the effort menu offers: the agent's own default, as "", ahead of the efforts when the
    /// agent has one.
    var effortChoices: [String] {
        guard !efforts.isEmpty, defaultEffort == nil else { return efforts }
        return [""] + efforts
    }

    func runs(under account: AgentAccount) -> Bool {
        agent == account.agent && (self.account.isEmpty || self.account == account.id)
    }
}

/// One of an agent's accounts on a server: the folder its CLI keeps the sign-in in.
struct AgentAccount: Equatable, Identifiable {
    struct Variable: Equatable {
        var name: String
        /// Empty for a sensitive one: its value stays on the server, which keeps it when it is
        /// saved without one.
        var value: String
        var sensitive: Bool
    }

    var id: String
    var agent: Agent
    var name: String
    /// `CLAUDE_CONFIG_DIR` or `CODEX_HOME`. Empty for the default account, which uses the agent's
    /// usual folder.
    var folder: String
    /// A Codex account that keeps only its sign-in in `folder` and shares the rest, its sessions
    /// too, with the default account.
    var sharesSessions: Bool
    var variables: [Variable]
    /// Who is signed in, as the agent's CLI last said.
    let email: String?
    let plan: String?

    /// The account every agent has, which uses the agent's usual folder.
    var isDefault: Bool { id == agent.rawValue }

    init(json: JSON) {
        id = json.string("id")
        agent = Agent(rawValue: json.string("agent")) ?? .claude
        name = json.string("name")
        folder = json.string("folder")
        sharesSessions = json.bool("shares_sessions")
        variables = json.objects("variables").map {
            Variable(name: $0.string("name"), value: $0.string("value"), sensitive: $0.bool("sensitive"))
        }
        email = json.optionalString("email")
        plan = json.optionalString("plan")
    }

    /// A new account of `agent`, until the server keeps it.
    init(agent: Agent) {
        id = ""
        self.agent = agent
        name = ""
        folder = ""
        sharesSessions = false
        variables = []
        email = nil
        plan = nil
    }

    var json: JSON {
        [
            "id": id, "agent": agent.rawValue, "name": name, "folder": folder, "shares_sessions": sharesSessions,
            "variables": variables.map { ["name": $0.name, "value": $0.value, "sensitive": $0.sensitive] },
        ]
    }

    /// The variable the account's folder is given to its agent as.
    var folderVariable: String { agent == .claude ? "CLAUDE_CONFIG_DIR" : "CODEX_HOME" }

    /// The folder the name suggests, as `~/.claude-personal`, with a number when another account
    /// keeps that one.
    func suggestedFolder(besides accounts: [AgentAccount]) -> String {
        guard !name.trimmingCharacters(in: .whitespaces).isEmpty else { return "" }
        let words = name.lowercased().split { !($0.isASCII && ($0.isLetter || $0.isNumber)) }
        let base = "~/.\(agent.rawValue)-\(words.isEmpty ? "account" : words.joined(separator: "-"))"
        let taken = Set(accounts.map(\.folder))
        return (1...).lazy.map { $0 == 1 ? base : "\(base)-\($0)" }.first { !taken.contains($0) } ?? base
    }

    /// What signs the account in, run on its server.
    var signInCommand: String {
        let folder = folder.isEmpty ? "" : "\(folderVariable)=\(folder) "
        return folder + (agent == .claude ? "claude auth login" : "codex login")
    }
}

struct Server: Equatable, Identifiable {
    enum State: String {
        case connecting, connected, disconnected, refused
    }

    let id: String
    let name: String
    /// The name cut short, for the places a long name crowds.
    let shortName: String
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
    /// What the server does with pull requests by itself.
    let doneOnMerge: Bool
    let removeMergedWorktrees: Bool
    /// What the server goes on with by itself: threads whose usage limit reset, and threads whose
    /// agents worked when it restarted.
    let continueAfterLimits: Bool
    let continueAfterRestarts: Bool
    /// How the writer there is told to name branches, and what that is until it is changed.
    let branchInstructions: String
    let defaultBranchInstructions: String
    /// The agents installed on the server, with their versions.
    let agents: [Agent: String]
    /// The agents' accounts, the default ones first.
    let agentAccounts: [AgentAccount]
    /// Whether the server has ever told us about itself.
    let known: Bool
    /// The update the server is putting in place, as it last said.
    let update: ServerUpdate?

    init(json: JSON) {
        id = json.string("id")
        name = json.string("name")
        shortName = json.string("short_name")
        platform = json.string("platform")
        state = State(rawValue: json.string("state")) ?? .connecting
        error = json.optionalString("error")
        path = json.optionalString("path")
        rttMs = (json["rtt_ms"] as? NSNumber)?.intValue
        let info = json.object("info")
        known = info != nil
        home = info?.string("home") ?? ""
        let version = info?.string("version") ?? ""
        self.version = version
        update = info?.object("update").map { ServerUpdate(json: $0, from: version) }
        protocolVersion = (info?["protocol"] as? NSNumber)?.intValue ?? 0
        models = (info?.objects("models") ?? []).map { ModelInfo(json: $0, shortNames: json.object("short_model_names")) }
        textModel = info?.optionalString("text_model")
        let settings = info?.object("pull_request_settings")
        doneOnMerge = settings?.bool("done_on_merge") ?? false
        removeMergedWorktrees = settings?.bool("remove_merged_worktrees") ?? false
        let continues = info?.object("continue_settings")
        continueAfterLimits = continues?.bool("after_limits") ?? false
        continueAfterRestarts = continues?.bool("after_restarts") ?? false
        let naming = info?.object("branch_instructions")
        branchInstructions = naming?.string("text") ?? ""
        defaultBranchInstructions = naming?.string("default") ?? ""
        var installed: [Agent: String] = [:]
        for agent in info?.objects("agents") ?? [] {
            guard let kind = Agent(rawValue: agent.string("agent")), let version = agent.optionalString("version") else { continue }
            installed[kind] = version
        }
        agents = installed
        let accounts = (info?.objects("agent_accounts") ?? []).map(AgentAccount.init(json:))
        agentAccounts = accounts.isEmpty ? Agent.allCases.map(Server.defaultAccount(of:)) : accounts
    }

    /// The account a server from before accounts has for each agent.
    private static func defaultAccount(of agent: Agent) -> AgentAccount {
        var account = AgentAccount(agent: agent)
        account.id = agent.rawValue
        account.name = "Default"
        return account
    }

    /// Whether threads there can move between accounts and agents.
    var switchesAccounts: Bool { protocolVersion >= 19 }

    /// Whether the server keeps where the user moves its threads in the sidebar.
    var movesThreads: Bool { protocolVersion >= 21 }

    func accounts(of agent: Agent) -> [AgentAccount] {
        agentAccounts.filter { $0.agent == agent }
    }

    /// Each model once, however many accounts run it, for picking one by its id.
    var distinctModels: [ModelInfo] {
        var seen = Set<String>()
        return models.filter { seen.insert($0.id).inserted }
    }

    /// The account with that id, or the agent's default one.
    func account(_ id: String?, of agent: Agent) -> AgentAccount? {
        let accounts = accounts(of: agent)
        return accounts.first { $0.id == id } ?? accounts.first { $0.isDefault } ?? accounts.first
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
    /// The branch new work starts from. `nil` from a server that doesn't say yet.
    let defaultBranch: String?
    let remote: Bool
    let added: Int
    let removed: Int
    let pullRequest: PullRequest?

    init(json: JSON) {
        branch = json.optionalString("branch")
        isDefault = json.bool("default")
        defaultBranch = json.optionalString("default_branch")
        remote = json.bool("remote")
        added = json.int("added")
        removed = json.int("removed")
        pullRequest = json.object("pull_request").map { PullRequest(json: $0) }
    }
}

struct PullRequest: Equatable {
    enum State {
        case open
        case draft
        case merged
        /// Closed without being merged.
        case closed

        var symbol: Symbol {
            switch self {
            case .open: .gitPullRequest
            case .draft: .gitPullRequestDraft
            case .merged: .gitMerge
            case .closed: .gitPullRequestClosed
            }
        }
    }

    let number: Int
    let title: String
    let url: String
    let state: State

    init(json: JSON) {
        number = json.int("number")
        title = json.string("title")
        url = json.string("url")
        if json.bool("merged") {
            state = .merged
        } else if json.bool("closed") {
            state = .closed
        } else {
            state = json.bool("draft") ? .draft : .open
        }
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

/// The symbol of a git action, as the server spells it: "commit_push".
enum GitSymbol {
    static func symbol(for action: String?) -> Symbol {
        switch action {
        case "pull": .cloudDownload
        case "push", "commit_push", "commit_push_pr": .cloudUpload
        case "create_pr": .gitPullRequestCreate
        default: .gitCommitHorizontal
        }
    }
}

/// What the button does: an action, or opening the pull request at `url`. With neither it is
/// off, and `hint` says why.
struct GitQuick: Equatable {
    let label: String
    /// Said before the label: "Merged".
    let state: String?
    let action: String?
    let url: String?
    let hint: String?
    let confirm: GitConfirm?
    /// The pull request the button opens, for its symbol and its title.
    let pullRequest: PullRequest?

    init(json: JSON) {
        label = json.string("label")
        state = json.optionalString("state")
        action = json.optionalString("action")
        url = json.optionalString("url")
        hint = json.optionalString("hint")
        confirm = json.object("confirm").map { GitConfirm(json: $0) }
        pullRequest = json.object("pull_request").map { PullRequest(json: $0) }
    }

    var title: String {
        guard let state else { return label }
        return "\(state) \(label)"
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
    case message
    case commit
    case push
    case pullRequestText = "pull_request_text"
    case pullRequest = "pull_request"
    case pull
    case merge
    case autoMerge = "auto_merge"
    case updateBranch = "update_branch"
    case close
    case reopen
    case revert
    /// A stage of a newer server.
    case unknown

    var label: String {
        switch self {
        case .message: "Writing Commit"
        case .commit: "Committing"
        case .push: "Pushing"
        case .pullRequestText: "Writing PR"
        case .pullRequest: "Creating PR"
        case .pull: "Pulling"
        case .merge: "Merging PR"
        case .autoMerge: "Setting Auto-merge"
        case .updateBranch: "Updating Branch"
        case .close: "Closing PR"
        case .reopen: "Reopening PR"
        case .revert: "Reverting PR"
        case .unknown: "Working"
        }
    }
}

/// What a git action did, or why it couldn't, shown under the button until it is dismissed.
struct GitNotice: Equatable {
    let checkoutID: String
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
    /// Its server's "No project": each of its threads works in a folder of its own.
    let noProject: Bool
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
        noProject = json.bool("no_project")
        createdAt = json.double("created_at")
    }

    /// How a new thread's headline reads with the project picked: "Let’s build in motile", or
    /// "Let’s build without a project".
    static func headline(_ project: Project?) -> (lead: String, name: String) {
        guard let project else { return ("Let’s build in", "a project") }
        return project.noProject ? ("Let’s build", "without a project") : ("Let’s build in", project.name)
    }

    /// Names the folder git works in for it: its worktree, or the project's folder.
    var checkoutID: String { "\(id):\(worktree?.path ?? path)" }

    /// The pull request that was opened for the thread, or the one of its worktree's branch.
    /// The project's own folder is shared by its threads, so its branch says nothing of one.
    func pullRequest(of thread: ThreadInfo) -> PullRequest? {
        thread.pullRequest ?? worktrees.first { $0.path == thread.cwd }?.git?.pullRequest
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

    var symbol: Symbol {
        switch self {
        case .supervised: .shield
        case .acceptEdits: .pencilLine
        case .auto: .sparkles
        case .full: .lockOpen
        }
    }
}

struct ThreadInfo: Equatable, Identifiable {
    let id: String
    let serverID: String
    var title: String
    let projectID: String
    let cwd: String
    var agent: Agent
    /// The id of the agent's account the thread works with.
    var agentAccount: String
    var model: String?
    var effort: String?
    var access: Access
    var plan: Bool
    let createdAt: Double
    let updatedAt: Double
    var doneAt: Double?
    /// Where the sidebar lists it among the active threads, the highest first: when it was
    /// created or last came back from done, until the user moves it.
    var position: Double
    let running: Bool
    /// The turn is over, but the agent still watches something it left running.
    let monitoring: Bool
    let needsApproval: Bool
    /// How many agents the thread's agent has started that still work.
    let agents: Int
    let turnEndedAt: Double?
    /// The pull request that was opened for it or linked to it.
    let pullRequest: PullRequest?
    /// Its agent is told what happens on its pull request.
    let watching: Bool
    /// What a commit, a push or the like that was started from it is at.
    let gitStage: GitStage?
    /// Why the agent stopped before it finished, until it works again.
    var interruption: Interruption?
    let unread: Bool

    init(json: JSON) {
        id = json.string("id")
        serverID = json.string("server_id")
        title = json.string("title")
        projectID = json.string("project_id")
        cwd = json.string("cwd")
        agent = Agent(rawValue: json.string("agent")) ?? .claude
        agentAccount = json.optionalString("agent_account").flatMap { $0.isEmpty ? nil : $0 } ?? agent.rawValue
        model = json.optionalString("model")
        effort = json.optionalString("effort")
        access = Access(rawValue: json.string("access")) ?? .full
        plan = json.bool("plan")
        createdAt = json.double("created_at")
        updatedAt = json.double("updated_at")
        doneAt = json.optionalDouble("done_at")
        // A server from before positions lists its threads by when they were created.
        let position = json.double("position")
        self.position = position > 0 ? position : createdAt
        running = json.bool("running")
        monitoring = json.bool("monitoring")
        needsApproval = json.bool("needs_approval")
        agents = json.int("agents")
        turnEndedAt = json.optionalDouble("turn_ended_at")
        pullRequest = json.object("pull_request").map { PullRequest(json: $0) }
        watching = json.bool("watching")
        gitStage = json.optionalString("git_stage").map { GitStage(rawValue: $0) ?? .unknown }
        interruption = json.object("interruption").flatMap { Interruption(json: $0) }
        unread = json.bool("unread")
    }

    var isDone: Bool { doneAt != nil }

    /// The agent's process is still there, working or monitoring.
    var busy: Bool { running || monitoring }

    var monitoringSince: Double { turnEndedAt ?? updatedAt }

    /// What its list is in order of, the highest first: when it was marked done, or else its
    /// position. Active threads keep their place when something happens in them.
    var listedAt: Double { isDone ? doneAt ?? 0 : position }

    static func listed(_ one: ThreadInfo, before other: ThreadInfo) -> Bool {
        (one.listedAt, one.id) > (other.listedAt, other.id)
    }
}

extension [ThreadInfo] {
    /// Where the thread is, or would go, in a list in the order of `ThreadInfo.listed`.
    func place(of thread: ThreadInfo) -> Int {
        var (low, high) = (0, count)
        while low < high {
            let middle = (low + high) / 2
            if ThreadInfo.listed(self[middle], before: thread) { low = middle + 1 } else { high = middle }
        }
        return low
    }

    func index(of thread: ThreadInfo) -> Int? {
        let place = place(of: thread)
        return place < count && self[place].id == thread.id ? place : nil
    }
}

/// Why a thread's agent stopped before it finished.
enum Interruption: Equatable {
    /// The agent reached its usage limit. `resetsAt` is when it resets, when the agent said; with
    /// `continues` the thread goes on by itself then.
    case limit(resetsAt: Double?, continues: Bool)
    /// The server restarted while the agent worked.
    case restart

    init?(json: JSON) {
        switch json.string("kind") {
        case "limit": self = .limit(resetsAt: json.optionalDouble("resets_at"), continues: json.bool("continues"))
        case "restart": self = .restart
        default: return nil
        }
    }

    /// What a thread's status says of it.
    var word: String {
        switch self {
        case .limit: "limited"
        case .restart: "interrupted"
        }
    }

    var title: String {
        switch self {
        case .limit: "Usage limit reached"
        case .restart: "Interrupted"
        }
    }

    var symbol: Symbol {
        switch self {
        case .limit: .clock
        case .restart: .refreshCw
        }
    }

    /// When the thread goes on, or what it waits for to be continued.
    func detail(now: Double = Date().timeIntervalSince1970) -> String {
        switch self {
        case .restart: "Your server restarted before the agent finished"
        case .limit(let resetsAt?, let continues) where resetsAt > now:
            continues ? "Continues at \(Time.stamp(resetsAt))" : "Resets at \(Time.stamp(resetsAt))"
        case .limit(_?, true): "Continues in a moment"
        case .limit(_?, false): "The limit has reset"
        case .limit(nil, _): "Reset time unknown"
        }
    }
}

struct Activity: Equatable {
    var running = false
    var monitoring = false
    var thinking = false
    /// The agent is making its conversation shorter to go on with it.
    var compacting = false
    /// How many agents it has started that still work.
    var agents = 0
    var startedAt: Double?
    /// The tool calls the running turn waits with until they are allowed or refused.
    var approvals: [Approval] = []
    /// The messages that wait for the agent to take them.
    var queued: [QueuedMessage] = []

    var busy: Bool { running || monitoring }

    init() {}

    /// What the thread list says the agent does, until the thread's server says more.
    init(thread: ThreadInfo) {
        running = thread.running
        monitoring = thread.monitoring
    }

    init(json: JSON, waiting: [JSON]) {
        running = json.bool("running")
        monitoring = json.bool("monitoring")
        thinking = json.bool("thinking")
        compacting = json.bool("compacting")
        agents = json.int("agents")
        startedAt = json.optionalDouble("started_at")
        approvals = waiting.map { Approval(json: $0) }
        queued = json.objects("queued").map { QueuedMessage(json: $0) }
    }
}

/// An agent the thread's agent started. What it did is a transcript of its own.
struct AgentInfo: Equatable, Identifiable {
    /// The tool call that started it.
    let id: String
    /// The agent that started it, when the thread's own didn't.
    let parent: String?
    let title: String
    let kind: String?
    let status: ToolContent.Status
    /// What it is doing now, or what it reported once it has ended.
    let detail: String
    let startedAt: Double
    let durationMs: Int?
    let tokens: Int?
    let toolUses: Int?

    init(json: JSON) {
        let number = { (key: String) in (json[key] as? NSNumber)?.intValue }
        id = json.string("id")
        parent = json.optionalString("parent")
        title = json.string("title")
        kind = json.optionalString("kind")
        status = ToolContent.Status(rawValue: json.string("status")) ?? .succeeded
        detail = json.string("detail")
        startedAt = json.double("started_at")
        durationMs = number("duration_ms")
        tokens = number("tokens")
        toolUses = number("tool_uses")
    }

    var working: Bool { status == .running }

    /// "3 tools · 21k tokens", as far as either is known.
    var usage: String? {
        let tools = toolUses.map { "\($0) \($0 == 1 ? "tool" : "tools")" }
        let spent = tokens.map { $0 < 1000 ? "\($0) tokens" : "\($0 / 1000)k tokens" }
        let parts = [tools, spent].compactMap { $0 }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }
}

/// A message sent while the agent was working. The transcript shows it as its last rows until
/// the agent takes it.
struct QueuedMessage: Equatable, Identifiable {
    let id: String
    let text: String
    /// The files attached to it, as paths on the server.
    let attachments: [String]
    /// The images and videos among them, by their path.
    let media: [String: AttachedFile]

    init(json: JSON) {
        id = json.string("id")
        text = json.string("text")
        attachments = json.strings("attachments")
        let shown = json.objects("media").map { media in
            let file = AttachedFile(name: "", media: media.string("id"), video: media.bool("video"), poster: media.optionalString("poster"))
            return (media.string("src"), file)
        }
        media = Dictionary(shown, uniquingKeysWith: { first, _ in first })
    }
}

/// A file attached to a message, as the transcript shows it. An image or a video names the file
/// the core has it under, and a video the image that stands for it until it plays.
struct AttachedFile: Equatable {
    let name: String
    let media: String?
    let video: Bool
    let poster: String?

    /// The image a tile of it shows.
    var picture: String? { video ? poster : media }
}

extension AttachedFile {
    init(json: JSON) {
        self.init(name: json.string("name"), media: json.optionalString("media"), video: json.bool("video"), poster: json.optionalString("poster"))
    }
}

/// A file in the composer: on its way to the server, or there and ready to be sent.
struct Attachment: Equatable, Identifiable {
    enum State: Equatable {
        case uploading(Double)
        case ready
        case failed(String)
    }

    let id: String
    /// Where it is on this Mac. A file that came back from a queued message is only on its server.
    let file: URL?
    let name: String
    let bytes: Int64?
    let video: Bool
    /// Shown as a tile with its picture, not by its name.
    let pictured: Bool
    var serverID: String
    var state: State
    /// Where it is on the server, once it is there.
    var path: String?
    var media: String?
    var poster: String?

    init(file: URL, serverID: String) {
        id = UUID().uuidString
        self.file = file
        name = file.lastPathComponent
        bytes = (try? file.resourceValues(forKeys: [.fileSizeKey]).fileSize).map(Int64.init)
        let type = UTType(filenameExtension: file.pathExtension)
        video = type?.conforms(to: .movie) == true
        pictured = video || type?.conforms(to: .image) == true
        self.serverID = serverID
        state = .uploading(0)
    }

    init(path: String, shown: AttachedFile?, serverID: String) {
        id = UUID().uuidString
        file = nil
        name = (path as NSString).lastPathComponent
        bytes = nil
        video = shown?.video == true
        pictured = shown != nil
        self.serverID = serverID
        state = .ready
        self.path = path
        media = shown?.media
        poster = shown?.poster
    }

    var attached: AttachedFile {
        AttachedFile(name: name, media: media, video: media != nil && video, poster: poster)
    }

    var viewed: ViewedMedia? {
        if let file { return ViewedMedia(name: name, video: video, source: .file(file)) }
        return media.map { ViewedMedia(name: name, video: video, source: .media($0)) }
    }
}

/// An image or a video the viewer shows: a file on this Mac, or one the core has or fetches.
struct ViewedMedia: Equatable {
    enum Source: Equatable {
        case file(URL)
        case media(String)
    }

    let name: String
    let video: Bool
    let source: Source
}

/// What the viewer has open: the images and videos of one message or of the composer, and
/// which of them is shown.
struct Viewing: Equatable {
    let items: [ViewedMedia]
    var index: Int

    var item: ViewedMedia { items[index] }
}

/// A tool call the turn waits with until the user has answered it.
struct Approval: Equatable, Identifiable {
    let id: String
    /// What is asked for: the tool, or what to do with a plan.
    let title: String
    /// What the tool acts on: the command, the file.
    let target: String
    let symbol: Symbol
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
    /// The command's colours, as the core's `[start, length, palette index]` spans.
    let spans: [Int]

    init(json: JSON) {
        command = json.string("command")
        expiresAt = json.double("expires_at")
        spans = (json["spans"] as? [NSNumber] ?? []).map(\.intValue)
    }

    func isExpired(at date: Date) -> Bool {
        expiresAt <= date.timeIntervalSince1970
    }

    /// How long the command still works, as "14:59".
    func timeLeft(at date: Date) -> String {
        let seconds = max(0, Int((expiresAt - date.timeIntervalSince1970).rounded(.up)))
        return String(format: "%d:%02d", seconds / 60, seconds % 60)
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
    private let lowercasedName: String
    private let lowercasedRepoName: Substring
    private let lowercasedDescription: String

    init(json: JSON) {
        name = json.string("name")
        description = json.optionalString("description")
        isPrivate = json.bool("private")
        lowercasedName = name.lowercased()
        lowercasedRepoName = lowercasedName.split(separator: "/", maxSplits: 1).last ?? Substring(lowercasedName)
        lowercasedDescription = description?.lowercased() ?? ""
    }

    /// How well it answers a lowercased search, the best at 0: the name after the owner
    /// starting with it, then the whole name, then the name holding it, then the description.
    func rank(_ search: String) -> Int? {
        if lowercasedRepoName.hasPrefix(search) { return 0 }
        if lowercasedName.hasPrefix(search) { return 1 }
        if lowercasedName.contains(search) { return 2 }
        if lowercasedDescription.contains(search) { return 3 }
        return nil
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
    /// The images that can be a project's icon, when one is being chosen.
    let images: [(name: String, path: String)]

    init(json: JSON) {
        path = json.string("path")
        typed = json.string("typed")
        parent = json.optionalString("parent")
        folders = json.objects("folders").map { Folder(name: $0.string("name"), path: $0.string("path"), typed: $0.string("typed")) }
        images = json.objects("images").map { ($0.string("name"), $0.string("path")) }
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

    /// The time of day, with the date before it when that isn't today.
    static func stamp(_ timestamp: Double) -> String {
        let date = Date(timeIntervalSince1970: timestamp)
        let today = Calendar.current.isDateInToday(date)
        return date.formatted(date: today ? .omitted : .abbreviated, time: .shortened)
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

/// What the agents spent on the account's servers in a stretch of time, as the core added it up.
struct UsageReport {
    struct Series: Identifiable {
        let agent: Agent
        let costUSD: Double
        let tokens: Int
        let costPoints: [Double]
        let tokenPoints: [Double]

        var id: Agent { agent }
    }

    /// A model, a project, a server, an account or a kind of token, and its part of the whole.
    struct Line: Identifiable {
        let name: String
        let agent: Agent?
        /// The project's server, when more than one server spent.
        let server: String?
        let tokens: Int
        let costUSD: Double?
        let share: Double

        var id: String { "\(agent?.rawValue ?? "")/\(server ?? "")/\(name)" }
    }

    let costUSD: Double
    let tokens: Int
    let unpricedTokens: Int
    let cacheSavingsUSD: Double
    /// The part that went into writing titles, branch names, commit messages and pull requests.
    let writingCostUSD: Double
    let writingTokens: Int
    let starts: [Date]
    let agents: [Series]
    let kinds: [Line]
    let models: [Line]
    let projects: [Line]
    let servers: [Line]
    /// The agents' accounts, only when an agent has more than one on a server.
    let accounts: [Line]

    init(json: JSON) {
        costUSD = json.double("cost_usd")
        tokens = json.int("tokens")
        unpricedTokens = json.int("unpriced_tokens")
        cacheSavingsUSD = json.double("cache_savings_usd")
        writingCostUSD = json.double("writing_cost_usd")
        writingTokens = json.int("writing_tokens")
        starts = (json["starts"] as? [NSNumber] ?? []).map { Date(timeIntervalSince1970: $0.doubleValue) }
        agents = json.objects("agents").map { series in
            let points = { (key: String) in (series[key] as? [NSNumber] ?? []).map(\.doubleValue) }
            return Series(
                agent: Agent(rawValue: series.string("agent")) ?? .claude, costUSD: series.double("cost_usd"),
                tokens: series.int("tokens"), costPoints: points("cost_points"), tokenPoints: points("token_points"))
        }
        let line = { (line: JSON, share: Double?) in
            Line(
                name: line.string("name"), agent: line.optionalString("agent").flatMap(Agent.init(rawValue:)),
                server: line.optionalString("server"), tokens: line.int("tokens"), costUSD: line.optionalDouble("cost_usd"),
                share: share ?? line.double("share"))
        }
        let (cost, tokens) = (costUSD, tokens)
        kinds = json.objects("kinds").map { kind in
            let share = cost > 0 ? kind.double("cost_usd") / cost : Double(kind.int("tokens")) / Double(max(tokens, 1))
            return line(kind, share)
        }
        models = json.objects("models").map { line($0, nil) }
        projects = json.objects("projects").map { line($0, nil) }
        servers = json.objects("servers").map { line($0, nil) }
        accounts = json.objects("accounts").map { line($0, nil) }
    }
}

/// How much of their plans the agents' logins have used: a section for each login, a row for
/// each of its windows, and what kept a server from saying.
struct LimitsReport {
    struct Section: Identifiable {
        let agent: Agent
        /// The account's name on the server.
        let name: String?
        /// Who is signed in.
        let account: String?
        let plan: String?
        let servers: [String]
        let windows: [Window]
        /// Why there are no windows.
        let note: String?

        var id: String { "\(agent.rawValue)/\(name ?? "")/\(account ?? servers.joined(separator: ","))/\(note ?? "")" }
    }

    struct Window: Identifiable {
        enum Pace: String {
            case ahead, on, under
        }

        enum Tone: String {
            case success, pending, warning
        }

        let label: String
        /// Between 0 and 100.
        let usedPercent: Double
        let used: String
        let resetsIn: String?
        let pace: Pace?
        let tone: Tone
        let resetCredits: Int

        var id: String { label }
    }

    let sections: [Section]
    let notes: [String]
    /// Some of it is old or missing, from what was last read.
    let stale: Bool

    init(json: JSON) {
        sections = json.objects("sections").map { section in
            Section(
                agent: Agent(rawValue: section.string("agent")) ?? .claude, name: section.optionalString("name"),
                account: section.optionalString("account"),
                plan: section.optionalString("plan"), servers: section.strings("servers"),
                windows: section.objects("windows").map { window in
                    Window(
                        label: window.string("label"), usedPercent: window.double("used_percent"), used: window.string("used"),
                        resetsIn: window.optionalString("resets_in"), pace: window.optionalString("pace").flatMap(Window.Pace.init),
                        tone: Window.Tone(rawValue: window.string("tone")) ?? .success,
                        resetCredits: window.int("reset_credits"))
                },
                note: section.optionalString("note"))
        }
        notes = json.strings("notes")
        stale = json.bool("stale")
    }
}
