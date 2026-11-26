import Foundation

/// What the agents of every server do together, as the widget and the Live Activity show it: what
/// waits for the user first, then what works, then what ended, the latest first.
public struct AgentsWork: Equatable, Sendable {
    /// How long the widget trusts what it was last told of an agent that works or waits.
    public static let staleAfter: TimeInterval = 10 * 60
    /// How long a thread that ended stays, as the servers keep it (`ENDED_WORK_STAYS_SECS`).
    public static let endedStays: TimeInterval = 15 * 60

    public struct Item: Hashable, Identifiable, Sendable {
        public let serverId: String
        public let threadId: String
        public let title: String
        public let project: String
        public let state: WorkState
        public let since: Double
        /// When its server last said what it does.
        public let heard: Double

        public var id: String { "\(serverId)/\(threadId)" }
        public var link: URL { ThreadLink(serverId: serverId, threadId: threadId).url }

        /// It works or waits, but the server said so too long ago to be sure it still does.
        public func isStale(at now: Date) -> Bool {
            state.isActive && now.timeIntervalSince1970 - heard > AgentsWork.staleAfter
        }
    }

    public let items: [Item]
    public let working: Int
    public let waiting: Int

    public init(_ servers: [ServerWork]) {
        working = servers.reduce(0) { $0 + $1.working }
        waiting = servers.reduce(0) { $0 + $1.waiting }
        var seen = Set<String>()
        items = servers
            .flatMap { server in
                server.items.map {
                    Item(
                        serverId: server.serverId, threadId: $0.threadId, title: $0.title, project: $0.project,
                        state: $0.state, since: $0.since, heard: server.at
                    )
                }
            }
            .sorted { ($0.state.rank, -$0.since) < ($1.state.rank, -$1.since) }
            .filter { seen.insert($0.threadId).inserted }
    }

    public var active: Int { working + waiting }

    /// "2 working", "1 needs you"; "No agents working" when none works or waits.
    public var summaryParts: [String] {
        guard active > 0 else { return ["No agents working"] }
        var parts: [String] = []
        if working > 0 { parts.append("\(working) working") }
        if waiting > 0 { parts.append(waiting == 1 ? "1 needs you" : "\(waiting) need you") }
        return parts
    }

    /// "2 working · 1 needs you".
    public var summary: String {
        summaryParts.joined(separator: " · ")
    }
}

private extension WorkState {
    var rank: Int {
        switch self {
        case .waiting: 0
        case .working: 1
        default: 2
        }
    }
}

/// The link that opens a thread: `motile://thread?server=…&thread=…`.
public struct ThreadLink: Equatable, Sendable {
    public let serverId: String
    public let threadId: String

    public init(serverId: String, threadId: String) {
        self.serverId = serverId
        self.threadId = threadId
    }

    public init?(url: URL) {
        guard url.scheme == "motile", url.host == "thread",
              let query = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems,
              let server = query.first(where: { $0.name == "server" })?.value,
              let thread = query.first(where: { $0.name == "thread" })?.value else { return nil }
        self.init(serverId: server, threadId: thread)
    }

    public var url: URL {
        var components = URLComponents()
        components.scheme = "motile"
        components.host = "thread"
        components.queryItems = [URLQueryItem(name: "server", value: serverId), URLQueryItem(name: "thread", value: threadId)]
        return components.url!
    }
}
