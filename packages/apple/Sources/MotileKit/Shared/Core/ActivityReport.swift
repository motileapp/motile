import Foundation

/// What the user and the agents did on the servers counted, as the core lays it out for the
/// Activity tab of the usage route.
struct ActivityReport {
    struct Tile: Identifiable {
        let label: String
        let value: String
        let detail: String

        var id: String { label }
    }

    /// Where work came from: a kind of client of the user's, the agents or the servers.
    struct Source: Identifiable {
        let id: String
        let label: String
        let symbol: Symbol
        let color: Int
        let messages: Int
        let turns: Int
        let seconds: Double
        let messagePoints: [Double]
        let turnPoints: [Double]
        let secondPoints: [Double]
    }

    struct Client: Identifiable {
        let device: String
        let name: String
        let client: String
        let detail: String
        let symbol: Symbol
        let color: Int
        let messages: Int
        let turns: Int
        let share: Double
        let lastSeen: Date?
        let thisDevice: Bool
        let removed: Bool

        var id: String { device }
    }

    struct Hour: Identifiable {
        let weekday: Int
        let hour: Int
        let messages: Int
        let turns: Int
        let level: Int

        var id: Int { weekday * 24 + hour }
    }

    struct Line: Identifiable {
        let name: String
        let agent: Agent?
        let server: String?
        let turns: Int
        let detail: String
        let share: Double

        var id: String { "\(agent?.rawValue ?? "")/\(server ?? "")/\(name)" }
    }

    struct Fact: Identifiable {
        let label: String
        let value: String
        let detail: String

        var id: String { label }
    }

    struct Feature: Identifiable {
        let id: String
        let label: String
        let count: Int
        let share: Double
        let detail: String
    }

    struct FeatureGroup: Identifiable {
        let name: String
        let count: Int
        let features: [Feature]

        var id: String { name }
    }

    let empty: Bool
    let starts: [Date]
    let tiles: [Tile]
    let sources: [Source]
    let clients: [Client]
    let hours: [Hour]
    let busiest: String?
    let outcomes: [Line]
    let agents: [Line]
    let models: [Line]
    let projects: [Line]
    let starters: [Line]
    let facts: [Fact]
    let features: [FeatureGroup]
    let agentFeatures: [Feature]

    init(json: JSON) {
        empty = json.bool("empty")
        starts = (json["starts"] as? [NSNumber] ?? []).map { Date(timeIntervalSince1970: $0.doubleValue) }
        tiles = json.objects("tiles").map { Tile(label: $0.string("label"), value: $0.string("value"), detail: $0.string("detail")) }
        sources = json.objects("sources").map { source in
            let points = { (key: String) in (source[key] as? [NSNumber] ?? []).map(\.doubleValue) }
            return Source(
                id: source.string("id"), label: source.string("label"), symbol: Self.symbol(source.string("symbol")),
                color: source.int("color"), messages: source.int("messages"), turns: source.int("turns"),
                seconds: source.double("seconds"), messagePoints: points("message_points"), turnPoints: points("turn_points"),
                secondPoints: points("second_points"))
        }
        clients = json.objects("clients").map { client in
            Client(
                device: client.string("device"), name: client.string("name"), client: client.string("client"),
                detail: client.string("detail"), symbol: Self.symbol(client.string("symbol")), color: client.int("color"),
                messages: client.int("messages"), turns: client.int("turns"), share: client.double("share"),
                lastSeen: client.optionalDouble("last_seen").map(Date.init(timeIntervalSince1970:)),
                thisDevice: client.bool("this_device"), removed: client.bool("removed"))
        }
        hours = json.objects("hours").map { hour in
            Hour(
                weekday: hour.int("weekday"), hour: hour.int("hour"), messages: hour.int("messages"), turns: hour.int("turns"),
                level: hour.int("level"))
        }
        busiest = json.optionalString("busiest")
        let lines = { (key: String) in
            json.objects(key).map { line in
                Line(
                    name: line.string("name"), agent: line.optionalString("agent").flatMap(Agent.init(rawValue:)),
                    server: line.optionalString("server"), turns: line.int("turns"), detail: line.string("detail"),
                    share: line.double("share"))
            }
        }
        outcomes = lines("outcomes")
        agents = lines("agents")
        models = lines("models")
        projects = lines("projects")
        starters = lines("starters")
        facts = json.objects("facts").map { Fact(label: $0.string("label"), value: $0.string("value"), detail: $0.string("detail")) }
        let feature = { (line: JSON) in
            Feature(
                id: line.string("id"), label: line.string("label"), count: line.int("count"), share: line.double("share"),
                detail: line.string("detail"))
        }
        features = json.objects("features").map { group in
            FeatureGroup(name: group.string("name"), count: group.int("count"), features: group.objects("features").map(feature))
        }
        agentFeatures = json.objects("agent_features").map(feature)
    }

    /// The symbol the core names, by its case's name.
    private static func symbol(_ name: String) -> Symbol {
        switch name {
        case "laptop": .laptop
        case "smartphone": .smartphone
        case "tablet": .tablet
        case "monitor": .monitor
        case "bot": .bot
        case "server": .server
        default: .monitorSmartphone
        }
    }
}
