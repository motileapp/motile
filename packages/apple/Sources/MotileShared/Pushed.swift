import Foundation

/// What a server's alert or its agents' work is, as crates/protocol's `alerts.rs` writes it.
/// Decoded and encoded with `Pushed.decoder` and `Pushed.encoder`, whose keys are snake_case.
public enum Pushed: Equatable, Sendable {
    case alert(PushedAlert)
    case work(ServerWork)

    public static var decoder: JSONDecoder {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }

    public static var encoder: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        return encoder
    }

    public init?(json: Data) {
        let decoder = Self.decoder
        guard let tagged = try? decoder.decode(Tagged.self, from: json) else { return nil }
        switch tagged.type {
        case "alert":
            guard let alert = try? decoder.decode(PushedAlert.self, from: json) else { return nil }
            self = .alert(alert)
        case "work":
            guard let work = try? decoder.decode(ServerWork.self, from: json) else { return nil }
            self = .work(work)
        default:
            return nil
        }
    }

    private struct Tagged: Decodable {
        let type: String
    }
}

public enum AlertKind: String, Codable, Sendable {
    case finished, failed, approval, question, plan, limit, other

    public init(from decoder: Decoder) throws {
        self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .other
    }
}

public struct PushedAlert: Codable, Hashable, Sendable {
    public let id: String
    public let serverId: String
    public let threadId: String
    public let kind: AlertKind
    public let threadTitle: String
    public let project: String
    public let detail: String?
    public let body: String
    public let at: Double
}

public enum WorkState: String, Codable, Sendable {
    case working, waiting, finished, failed, other

    public init(from decoder: Decoder) throws {
        self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .other
    }

    /// The agent still works or waits for the user, rather than having ended.
    public var isActive: Bool {
        self == .working || self == .waiting
    }
}

public struct WorkItem: Codable, Hashable, Sendable {
    public let threadId: String
    public let title: String
    public let project: String
    public let state: WorkState
    /// When it came to this state.
    public let since: Double

    public init(threadId: String, title: String, project: String, state: WorkState, since: Double) {
        self.threadId = threadId
        self.title = title
        self.project = project
        self.state = state
        self.since = since
    }
}

/// What a server's agents are doing: the threads that wait for the user, then those that work,
/// then those that ended in the last 15 minutes, as many as fit; and how many work and wait in all.
public struct ServerWork: Codable, Hashable, Sendable {
    public let serverId: String
    public let serverName: String
    public let items: [WorkItem]
    public let working: Int
    public let waiting: Int
    public let at: Double

    public init(serverId: String, serverName: String, items: [WorkItem], working: Int, waiting: Int, at: Double) {
        self.serverId = serverId
        self.serverName = serverName
        self.items = items
        self.working = working
        self.waiting = waiting
        self.at = at
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        serverId = try container.decode(String.self, forKey: .serverId)
        serverName = try container.decode(String.self, forKey: .serverName)
        items = try container.decode([WorkItem].self, forKey: .items)
        working = try container.decodeIfPresent(Int.self, forKey: .working) ?? 0
        waiting = try container.decodeIfPresent(Int.self, forKey: .waiting) ?? 0
        at = try container.decode(Double.self, forKey: .at)
    }
}
