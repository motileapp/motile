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

struct Host: Equatable, Identifiable {
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
    let models: [ModelInfo]
    /// The agents installed on the host, with their versions.
    let agents: [Agent: String]
    /// Whether the host has ever told us about itself.
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
        models = (info?.objects("models") ?? []).map { ModelInfo(json: $0) }
        var installed: [Agent: String] = [:]
        for agent in info?.objects("agents") ?? [] {
            guard let kind = Agent(rawValue: agent.string("agent")), let version = agent.optionalString("version") else { continue }
            installed[kind] = version
        }
        agents = installed
    }
}

struct Project: Equatable, Identifiable {
    let id: String
    let hostID: String
    let path: String
    let name: String
    let branch: String?
    let createdAt: Double

    init(json: JSON, hostID: String) {
        id = json.string("id")
        self.hostID = hostID
        path = json.string("path")
        name = json.string("name")
        branch = json.optionalString("branch")
        createdAt = json.double("created_at")
    }
}

enum Access: String, CaseIterable, Identifiable {
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
    let hostID: String
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
    let needsApproval: Bool
    let turnEndedAt: Double?
    let unread: Bool

    init(json: JSON) {
        id = json.string("id")
        hostID = json.string("host_id")
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
        needsApproval = json.bool("needs_approval")
        turnEndedAt = json.optionalDouble("turn_ended_at")
        unread = json.bool("unread")
    }

    var isDone: Bool { doneAt != nil }

    /// Active threads keep their place when something happens in them; only coming back from
    /// done moves one to the top.
    var activeOrder: Double { max(createdAt, undoneAt ?? 0) }
}

struct Activity: Equatable {
    var running = false
    var thinking = false
    var startedAt: Double?

    init() {}

    init(json: JSON) {
        running = json.bool("running")
        thinking = json.bool("thinking")
        startedAt = json.optionalDouble("started_at")
    }
}

struct Denial {
    let toolName: String
    let toolUseID: String
    let input: String

    init(json: JSON) {
        toolName = json.string("tool_name")
        toolUseID = json.string("tool_use_id")
        input = json.string("input")
    }

    var json: JSON { ["tool_name": toolName, "tool_use_id": toolUseID, "input": input] }

    /// What the agent wanted to do, in a line.
    var summary: String {
        let object = (try? JSONSerialization.jsonObject(with: Data(input.utf8))) as? JSON ?? [:]
        if let command = object["command"] as? String { return command }
        if let path = object["file_path"] as? String { return path }
        return toolName
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

struct RemoteFolder {
    let path: String
    let parent: String?
    let folders: [String]

    init(json: JSON) {
        path = json.string("path")
        parent = json.optionalString("parent")
        folders = json.strings("folders")
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
