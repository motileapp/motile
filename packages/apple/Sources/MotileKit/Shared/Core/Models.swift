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
    /// The effort the agent runs it at when the thread doesn't pick one. Missing when the server
    /// couldn't learn it.
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

    /// What the effort menu offers: the efforts, with the agent's own default as "" ahead of them
    /// only when the server couldn't say which one that is.
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
    var usualFolder: String { agent == .claude ? "~/.claude" : "~/.codex" }

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

/// What a server keeps of the account's other servers' threads, as the core says it.
struct ServerSync: Equatable {
    enum State: String {
        case synced, copying, waiting, failed, off
    }

    let state: State
    /// "Synced", "Syncing · 48%", "Couldn't sync from vibe".
    let text: String
    /// Why the last round couldn't copy to it.
    let error: String?
    /// When a round last left its copies whole.
    let syncedAt: Double?

    init(json: JSON) {
        state = State(rawValue: json.string("state")) ?? .off
        text = json.string("text")
        error = json.optionalString("error")
        syncedAt = (json["synced_at"] as? NSNumber)?.doubleValue
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
    /// The server was removed from the account. Its threads are the copies the account's other
    /// servers keep, to read and to go on with on another server.
    let removed: Bool
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
    let pullDefaultBranch: Bool
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
    /// The account a new thread is better off starting on, by account id and model id.
    let roomierAccounts: [String: [String: String]]
    /// Whether the server has ever told us about itself.
    let known: Bool
    /// The update the server is putting in place, as it last said.
    let update: ServerUpdate?
    /// How its copies of the account's other servers' threads stand.
    let sync: ServerSync?

    init(json: JSON) {
        id = json.string("id")
        name = json.string("name")
        shortName = json.string("short_name")
        platform = json.string("platform")
        removed = json.bool("removed")
        state = State(rawValue: json.string("state")) ?? .connecting
        error = json.optionalString("error")
        path = json.optionalString("path")
        rttMs = (json["rtt_ms"] as? NSNumber)?.intValue
        roomierAccounts = json["roomier_accounts"] as? [String: [String: String]] ?? [:]
        sync = json.object("sync").map(ServerSync.init(json:))
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
        pullDefaultBranch = settings?.bool("pull_default_branch") ?? false
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

    /// Whether it signs its agents' accounts in for the client.
    var signsInAccounts: Bool { protocolVersion >= 31 }

    /// Whether the server keeps where the user moves its threads in the sidebar.
    var movesThreads: Bool { protocolVersion >= 21 }

    /// Whether the server asks its agents for their models when asked to.
    var refreshesModels: Bool { protocolVersion >= 23 }

    /// Whether the server can keep its projects' folders pulled on their default branch.
    var pullsDefaultBranch: Bool { protocolVersion >= 25 }

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
        return accounts.first { $0.id == id } ?? accounts.first
    }

    /// An agent's first account is its default.
    func isDefault(_ account: AgentAccount) -> Bool {
        accounts(of: account.agent).first?.id == account.id
    }

    /// The model with that id as the account lists it, or else the first one offered.
    static func model(_ id: String?, account: String?, among offered: [ModelInfo]) -> ModelInfo? {
        let named = offered.filter { $0.id == id }
        let listedByAccount = { (model: ModelInfo) in model.account.isEmpty || model.account == account }
        return named.first(where: listedByAccount) ?? named.first ?? offered.first(where: listedByAccount) ?? offered.first
    }

    /// The account and model a new thread starts on, when another account of its agent runs the
    /// model and has used clearly less of its limits.
    func roomier(than accountID: String?, model id: String?) -> (account: String, model: String)? {
        guard let model = Server.model(id, account: accountID, among: models), let account = account(accountID, of: model.agent),
              let roomier = roomierAccounts[account.id]?[model.id] else { return nil }
        return (roomier, model.id)
    }

    /// The model's name, with the account's when its agent has more than one.
    func label(of model: ModelInfo, account id: String?) -> String {
        guard accounts(of: model.agent).count > 1, let account = account(id, of: model.agent) else { return model.shortName }
        return "\(model.shortName) · \(account.name)"
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
    /// The checked-out branch has a commit a worktree can start from.
    let hasCommit: Bool
    let remote: Bool
    let added: Int
    let removed: Int
    let pullRequest: PullRequest?

    init(json: JSON) {
        branch = json.optionalString("branch")
        isDefault = json.bool("default")
        defaultBranch = json.optionalString("default_branch")
        hasCommit = !json.bool("no_commit")
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

/// What a git action or a project add did, or why it couldn't, shown at the top of the thread
/// until it is dismissed. A git notice belongs to its checkout; a project's shows anywhere.
struct Notice: Equatable {
    var checkoutID: String?
    let title: String
    var description: String?
    var failed = false
    /// Still under way, like a clone.
    var pending = false
    /// The pull request to open.
    var url: String?
    /// The action that follows, like a push after a commit.
    var next: String?
    /// The project to open a thread in.
    var projectID: String?

    var nextLabel: String? {
        switch next {
        case "push": "Push"
        case "create_pr": "Create PR"
        default: nil
        }
    }
}

/// An action that waits for the user to say where it should happen.
/// A program's `run` that waits for the user to allow the command.
struct ProgramConfirm: Identifiable {
    let threadID: String
    let rowID: String
    let callID: UInt64
    let command: String

    var id: String { "\(rowID)/\(callID)" }

    /// What the user is asked, with the command as it will run.
    func confirmation(_ store: AppStore) -> Confirmation {
        Confirmation(
            title: "Run this command?", message: command,
            answers: [Answer(title: "Run", variant: .primary) { store.answer(self, allow: true) }],
            dismissal: "Don’t Run")
    }
}

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
    /// The same on every server that has the project, which a picker lists once.
    let key: String
    let createdAt: Double

    static let noProjectKey = "no-project"

    init(json: JSON, serverID: String) {
        id = json.string("id")
        key = json.string("key")
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
    /// What the tool call that waits for the user asks of them.
    let asking: Ask?
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
    /// The thread whose agent started this one. The sidebar lists only threads without one.
    let parentID: String?
    /// How the agents its agent started are doing, theirs too.
    let children: ThreadChildren?

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
        createdAt = json.double("created_at")
        updatedAt = json.double("updated_at")
        doneAt = json.optionalDouble("done_at")
        // A server from before positions lists its threads by when they were created.
        let position = json.double("position")
        self.position = position > 0 ? position : createdAt
        running = json.bool("running")
        monitoring = json.bool("monitoring")
        needsApproval = json.bool("needs_approval")
        // A server from before `asking` only says that something waits.
        asking = needsApproval ? Ask(rawValue: json.string("asking")) ?? .approval : nil
        agents = json.int("agents")
        turnEndedAt = json.optionalDouble("turn_ended_at")
        pullRequest = json.object("pull_request").map { PullRequest(json: $0) }
        watching = json.bool("watching")
        gitStage = json.optionalString("git_stage").map { GitStage(rawValue: $0) ?? .unknown }
        interruption = json.object("interruption").flatMap { Interruption(json: $0) }
        unread = json.bool("unread")
        parentID = json.object("parent")?.optionalString("thread_id")
        children = json.object("children").map(ThreadChildren.init(json:))
    }

    var isDone: Bool { doneAt != nil }

    /// The agent's process is still there, working or monitoring.
    var busy: Bool { running || monitoring }

    /// It works, or an agent it started does.
    var working: Bool { running || (children?.running ?? 0) > 0 }

    /// Its agent ended its turn, and the agents it started work on.
    var waitsOnAgents: Bool { !running && (children?.running ?? 0) > 0 }

    /// Its agent's process is still there or the agents it started work, so it can't be done yet.
    var unfinished: Bool { busy || waitsOnAgents }

    /// What it, or else an agent it started, waits for the user to answer.
    var waitsFor: Ask? { asking ?? children?.waiting.first?.ask }

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
/// What a tool call that waits for the user asks of them.
enum Ask: String {
    case approval
    case question
    case plan

    var word: String {
        switch self {
        case .approval: "Approval"
        case .question: "Question"
        case .plan: "Plan"
        }
    }

    /// What a thread is said to do while it waits.
    var phrase: String {
        switch self {
        case .approval: "needs approval"
        case .question: "has a question"
        case .plan: "has a plan"
        }
    }

    var symbol: Symbol {
        switch self {
        case .approval: .shield
        case .question: .messageCircleQuestionMark
        case .plan: .clipboardList
        }
    }

    /// What the transcript says while the agent waits for it.
    var waiting: String {
        switch self {
        case .approval: "Waiting for your approval"
        case .question: "Waiting for your answer"
        case .plan: "Waiting for you to review the plan"
        }
    }
}

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
        // What the agent is told of the agents it started isn't the user's to send or take back.
        queued = json.objects("queued").filter { $0["notice"] == nil }.map { QueuedMessage(json: $0) }
    }
}

/// How an agent is doing, which the dot on its avatar shows.
enum AgentState: String {
    /// `limited`: it reached its account's usage limit, and waits for the limit to reset.
    case working, waiting, limited, done, failed, stopped

    /// Its word is in its colour: one that is done or works says so quietly.
    var flagged: Bool { self == .waiting || self == .limited || self == .failed || self == .stopped }
}

/// The agents a thread's agent started, and theirs, as the thread shows them.
struct ThreadChildren: Equatable {
    struct Waiting: Equatable {
        let threadID: String
        let title: String
        let ask: Ask
    }

    let running: Int
    /// The agents of the first few that work, whose avatars stand side by side.
    let working: [Agent]
    /// Those that wait for the user, the one that has waited longest first.
    let waiting: [Waiting]
    /// "Run greet.py needs your approval", "2 agents need you".
    let waitingText: String?
    /// "1 agent", "3 agents", which the agents strip says "working" after.
    let runningCount: String?
    /// "1 agent working", "3 agents working", while they work.
    let runningText: String?
    /// "Stop 1 agent?", "Stop 3 agents?", while they work.
    let stopQuestion: String?

    init(json: JSON) {
        running = json.int("running")
        working = json.strings("working").compactMap(Agent.init(rawValue:))
        waiting = json.objects("waiting").map {
            Waiting(threadID: $0.string("thread_id"), title: $0.string("title"), ask: Ask(rawValue: $0.string("ask")) ?? .approval)
        }
        waitingText = json.optionalString("waiting_text")
        runningCount = json.optionalString("running_count")
        runningText = json.optionalString("running_text")
        stopQuestion = json.optionalString("stop_question")
    }
}

/// A stop that asks first, since it ends the agents the thread started.
struct StopQuestion: Identifiable {
    let threadID: String
    let question: String

    var id: String { threadID }

    func confirmation(_ store: AppStore) -> Confirmation {
        Confirmation(
            title: question, message: "What they did so far stays in their threads.",
            answers: [Answer(title: "Stop", variant: .danger) { store.stopNow(threadID: threadID) }]
        )
    }
}

/// An agent the thread's agent started. What it did is a transcript of its own, or a thread of
/// its own when it was started through your server.
struct AgentInfo: Equatable, Identifiable {
    /// The tool call that started it.
    let id: String
    /// The agent that started it, when the thread's own didn't.
    let parent: String?
    let title: String
    let kind: String?
    let status: ToolContent.Status
    let state: AgentState
    /// Whether it is listed under the previous agents: it and every agent of its family ended.
    let previous: Bool
    /// "Working", "Needs approval", "Failed".
    let label: String
    /// What it is doing now, or what it reported once it has ended.
    let detail: String
    let startedAt: Double
    let durationMs: Int?
    let tokens: Int?
    let toolUses: Int?
    /// The thread it works in, when it was started through your server.
    let threadID: String?
    /// The agent it runs as, whose logo it shows, when that is known.
    let agent: Agent?
    /// The model's short name, "Opus 5.5".
    let model: String?
    /// The account's name, when its agent has more than one.
    let account: String?

    init(json: JSON) {
        let number = { (key: String) in (json[key] as? NSNumber)?.intValue }
        id = json.string("id")
        parent = json.optionalString("parent")
        title = json.string("title")
        kind = json.optionalString("kind")
        status = ToolContent.Status(rawValue: json.string("status")) ?? .succeeded
        state = AgentState(rawValue: json.string("state")) ?? .done
        previous = json.bool("previous")
        label = json.string("label")
        detail = json.string("detail")
        startedAt = json.double("started_at")
        durationMs = number("duration_ms")
        tokens = number("tokens")
        toolUses = number("tool_uses")
        threadID = json.optionalString("thread_id")
        agent = json.optionalString("agent").flatMap(Agent.init(rawValue:))
        model = json.optionalString("model")
        account = json.optionalString("account")
    }

    var working: Bool { status == .running }

    /// "3 tools · 21k tokens", as far as either is known.
    var usage: String? {
        let tools = toolUses.map { "\($0) \($0 == 1 ? "tool" : "tools")" }
        let parts = [tools, tokens.map(Self.tokens)].compactMap { $0 }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    /// "820 tokens", "21.4K tokens", "4.57M tokens".
    static func tokens(_ count: Int) -> String {
        "\(count.abbreviated) tokens"
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
    /// What the tool acts on: the whole command, the file.
    let target: String
    /// The target set in its colours.
    let typesetTarget: NSAttributedString
    let symbol: Symbol
    let ask: Ask
    /// What the buttons that allow and refuse it say.
    let allowLabel: String
    let refuseLabel: String
    /// The questions the agent asks with it; allowing it takes an answer to each.
    let questions: [Question]

    init(json: JSON) {
        id = json.string("id")
        title = json.string("title")
        target = json.string("target")
        typesetTarget = Typesetter.command(target, spans: json["spans"] as? [NSNumber] ?? [])
        symbol = ToolContent.symbol(for: json.string("icon"))
        ask = Ask(rawValue: json.string("ask")) ?? .approval
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

    /// A word or two that says what it is about.
    let header: String
    let text: String
    let options: [Choice]
    /// More than one option can be chosen.
    let multiple: Bool

    var id: String { text }

    init(json: JSON) {
        header = json.string("header")
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

extension Int {
    /// "820", "1.5K", "45.7M", "123B", "1.2T": at most three significant digits.
    var abbreviated: String {
        let units = ["", "K", "M", "B", "T"]
        var value = Double(magnitude)
        var unit = 0
        while value >= 999.5 && unit < units.count - 1 {
            value /= 1000
            unit += 1
        }
        let decimals = unit == 0 || value >= 100 ? 0 : value >= 10 ? 1 : 2
        var digits = String(format: "%.\(decimals)f", value)
        if decimals > 0 {
            while digits.hasSuffix("0") { digits.removeLast() }
            if digits.hasSuffix(".") { digits.removeLast() }
        }
        return (self < 0 ? "-" : "") + digits + units[unit]
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

    /// "850ms", "12s", "3m 05s", "1h 02m".
    static func duration(milliseconds: Int) -> String {
        if milliseconds < 1000 { return "\(milliseconds)ms" }
        let seconds = milliseconds / 1000
        if seconds < 60 { return "\(seconds)s" }
        return Self.minutes(seconds)
    }

    /// "3m 05s", "1h 02m": the second unit always takes two digits, so a timer doesn't jump.
    private static func minutes(_ seconds: Int) -> String {
        if seconds < 3600 { return "\(seconds / 60)m \(String(format: "%02d", seconds % 60))s" }
        return "\(seconds / 3600)h \(String(format: "%02d", seconds % 3600 / 60))m"
    }

    /// A running timer: "5s", "12m 03s", "1h 03m".
    static func elapsed(since timestamp: Double, now: Double = Date().timeIntervalSince1970) -> String {
        let seconds = max(0, Int(now - timestamp))
        if seconds < 60 { return "\(seconds)s" }
        return Self.minutes(seconds)
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

/// What the servers found in the threads' messages for the sidebar's search, by thread.
struct MessageSearch: Equatable {
    var query = ""
    /// Some server may still answer.
    var searching = false
    var matches: [String: MessageMatch] = [:]
}

extension MessageSearch {
    init(json: JSON) {
        query = json.string("query")
        searching = json.bool("searching")
        let found = json.objects("matches").map { ($0.string("thread_id"), MessageMatch(json: $0)) }
        matches = Dictionary(found, uniquingKeysWith: { first, _ in first })
    }
}

/// The message a search found a thread by, in parts that set off what was searched for.
struct MessageMatch: Equatable {
    struct Part: Equatable {
        let text: String
        let matched: Bool
    }

    /// The user wrote it, or else the agent.
    let byUser: Bool
    let parts: [Part]

    init(json: JSON) {
        byUser = json.string("speaker") == "user"
        parts = json.objects("parts").map { Part(text: $0.string("text"), matched: $0.bool("matched")) }
    }
}
