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
        /// The user's time on its devices.
        let engagedSeconds: Double
        let engagedPoints: [Double]
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
        let engagedSeconds: Double
        /// The user's time on it and its messages: "2h 10m · 31 messages".
        let summary: String
        let share: Double
        let lastSeen: Date?
        let active: Bool
        let thisDevice: Bool
        let removed: Bool

        var id: String { device }
    }

    /// When the work was done: the hours of a day in a row, the hours of a week a row a day, or the
    /// days of weeks a row a weekday, split into four hours for up to thirty days.
    struct Heatmap {
        struct Cell {
            /// When it is: "Wed, Nov 11, 5–6 PM", "Wed, Nov 11, 4–8 PM" or "Wed, Nov 11".
            let label: String
            let messages: Int
            let turns: Int
            /// The user's time in it, or empty: "42m".
            let time: String
            /// From 0, nothing, to 4, by the user's time in it next to the cell with the most.
            let timeLevel: Int
            /// From 0 to 4, by its messages next to the cell with the most.
            let messageLevel: Int
        }

        struct AxisLabel: Identifiable {
            let column: Int
            let label: String

            var id: Int { column }
        }

        /// The rows' names, or none for a single row.
        let rows: [String]
        let columns: Int
        let axis: [AxisLabel]
        /// Row by row; one yet to come is nil.
        let cells: [Cell?]
        /// What a cell is: "an hour", "four hours" or "a day".
        let unit: String

        var rowCount: Int { columns == 0 ? 0 : cells.count / columns }

        init(json: JSON) {
            rows = json.strings("rows")
            columns = json.int("columns")
            axis = json.objects("axis").map { AxisLabel(column: $0.int("column"), label: $0.string("label")) }
            cells = (json["cells"] as? [Any] ?? []).map { cell in
                guard let cell = cell as? JSON else { return nil }
                return Cell(
                    label: cell.string("label"), messages: cell.int("messages"), turns: cell.int("turns"),
                    time: cell.string("time"), timeLevel: cell.int("time_level"), messageLevel: cell.int("message_level"))
            }
            unit = json.string("unit")
        }
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
        /// Which of two columns side by side it goes in, 0 or 1.
        let column: Int

        var id: String { name }
    }

    let empty: Bool
    let starts: [Date]
    let tiles: [Tile]
    let sources: [Source]
    /// Where the chart of the user's time, and of the agents', draws its lines.
    let engagedTicks: [Double]
    let secondTicks: [Double]
    let clients: [Client]
    let heatmap: Heatmap
    let busiestTime: String?
    let busiestMessages: String?
    let outcomes: [Line]
    let agents: [Line]
    let models: [Line]
    let projects: [Line]
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
                secondPoints: points("second_points"), engagedSeconds: source.double("engaged_seconds"),
                engagedPoints: points("engaged_points"))
        }
        engagedTicks = (json["engaged_ticks"] as? [NSNumber] ?? []).map(\.doubleValue)
        secondTicks = (json["second_ticks"] as? [NSNumber] ?? []).map(\.doubleValue)
        clients = json.objects("clients").map { client in
            Client(
                device: client.string("device"), name: client.string("name"), client: client.string("client"),
                detail: client.string("detail"), symbol: Self.symbol(client.string("symbol")), color: client.int("color"),
                messages: client.int("messages"), turns: client.int("turns"),
                engagedSeconds: client.double("engaged_seconds"), summary: client.string("summary"), share: client.double("share"),
                lastSeen: client.optionalDouble("last_seen").map(Date.init(timeIntervalSince1970:)),
                active: client.bool("active"), thisDevice: client.bool("this_device"), removed: client.bool("removed"))
        }
        heatmap = Heatmap(json: json.object("heatmap") ?? [:])
        busiestTime = json.optionalString("busiest_time")
        busiestMessages = json.optionalString("busiest_messages")
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
        facts = json.objects("facts").map { Fact(label: $0.string("label"), value: $0.string("value"), detail: $0.string("detail")) }
        let feature = { (line: JSON) in
            Feature(
                id: line.string("id"), label: line.string("label"), count: line.int("count"), share: line.double("share"),
                detail: line.string("detail"))
        }
        features = json.objects("features").map { group in
            FeatureGroup(
                name: group.string("name"), count: group.int("count"), features: group.objects("features").map(feature),
                column: group.int("column"))
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
        default: .monitorSmartphone
        }
    }
}
