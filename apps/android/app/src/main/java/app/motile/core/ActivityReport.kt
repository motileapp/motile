package app.motile.core

import app.motile.platform.Symbol

/** What the user and the agents did on the servers counted, as the core lays it out for the Activity tab of the usage view. */
data class ActivityReport(
    val empty: Boolean,
    /** When each bucket starts, in seconds since 1970. */
    val starts: List<Double>,
    val tiles: List<Tile>,
    val sources: List<Source>,
    /** Where the chart of the user's time, and of the agents', draws its lines. */
    val engagedTicks: List<Double>,
    val secondTicks: List<Double>,
    val clients: List<Client>,
    val hours: List<Hour>,
    val busiestTime: String?,
    val busiestMessages: String?,
    val outcomes: List<Line>,
    val agents: List<Line>,
    val models: List<Line>,
    val projects: List<Line>,
    val facts: List<Fact>,
    val features: List<FeatureGroup>,
    val agentFeatures: List<Feature>,
) {
    data class Tile(val label: String, val value: String, val detail: String)

    /** Where work came from: a kind of client of the user's, the agents or the servers. */
    data class Source(
        val id: String,
        val label: String,
        val symbol: Symbol,
        val color: Int,
        val messages: Int,
        val turns: Int,
        val seconds: Double,
        val messagePoints: List<Double>,
        val turnPoints: List<Double>,
        val secondPoints: List<Double>,
        /** The user's time on its devices. */
        val engagedSeconds: Double,
        val engagedPoints: List<Double>,
    )

    data class Client(
        val device: String,
        val name: String,
        val client: String,
        val detail: String,
        val symbol: Symbol,
        val color: Int,
        val messages: Int,
        val turns: Int,
        val engagedSeconds: Double,
        /** The user's time on it and its messages: "2h 10m · 31 messages". */
        val summary: String,
        val share: Double,
        /** In seconds since 1970. */
        val lastSeen: Double?,
        val active: Boolean,
        val thisDevice: Boolean,
        val removed: Boolean,
    )

    /** `time` is the user's time in it, or empty: "42m". */
    data class Hour(val weekday: Int, val hour: Int, val messages: Int, val turns: Int, val engagedSeconds: Double, val time: String, val timeLevel: Int, val messageLevel: Int) {
        val id: Int get() = weekday * 24 + hour
    }

    data class Line(val name: String, val agent: Agent?, val server: String?, val turns: Int, val detail: String, val share: Double) {
        val id: String get() = "${agent?.raw ?: ""}/${server ?: ""}/$name"
    }

    data class Fact(val label: String, val value: String, val detail: String)

    data class Feature(val id: String, val label: String, val count: Int, val share: Double, val detail: String)

    data class FeatureGroup(val name: String, val count: Int, val features: List<Feature>)

    companion object {
        operator fun invoke(json: JSON): ActivityReport {
            fun lines(key: String) = json.objects(key).map { line ->
                Line(
                    line.string("name"), Agent.of(line.optionalString("agent")), line.optionalString("server"), line.int("turns"),
                    line.string("detail"), line.double("share"),
                )
            }
            fun feature(line: JSON) = Feature(line.string("id"), line.string("label"), line.int("count"), line.double("share"), line.string("detail"))
            return ActivityReport(
                empty = json.bool("empty"),
                starts = json.numbers("starts"),
                tiles = json.objects("tiles").map { Tile(it.string("label"), it.string("value"), it.string("detail")) },
                sources = json.objects("sources").map { source ->
                    Source(
                        source.string("id"), source.string("label"), symbol(source.string("symbol")), source.int("color"),
                        source.int("messages"), source.int("turns"), source.double("seconds"), source.numbers("message_points"),
                        source.numbers("turn_points"), source.numbers("second_points"), source.double("engaged_seconds"),
                        source.numbers("engaged_points"),
                    )
                },
                engagedTicks = json.numbers("engaged_ticks"),
                secondTicks = json.numbers("second_ticks"),
                clients = json.objects("clients").map { client ->
                    Client(
                        client.string("device"), client.string("name"), client.string("client"), client.string("detail"),
                        symbol(client.string("symbol")), client.int("color"), client.int("messages"), client.int("turns"),
                        client.double("engaged_seconds"), client.string("summary"), client.double("share"),
                        client.optionalDouble("last_seen"), client.bool("active"), client.bool("this_device"), client.bool("removed"),
                    )
                },
                hours = json.objects("hours").map {
                    Hour(it.int("weekday"), it.int("hour"), it.int("messages"), it.int("turns"), it.double("engaged_seconds"), it.string("time"), it.int("time_level"), it.int("message_level"))
                },
                busiestTime = json.optionalString("busiest_time"),
                busiestMessages = json.optionalString("busiest_messages"),
                outcomes = lines("outcomes"),
                agents = lines("agents"),
                models = lines("models"),
                projects = lines("projects"),
                facts = json.objects("facts").map { Fact(it.string("label"), it.string("value"), it.string("detail")) },
                features = json.objects("features").map { group ->
                    FeatureGroup(group.string("name"), group.int("count"), group.objects("features").map(::feature))
                },
                agentFeatures = json.objects("agent_features").map(::feature),
            )
        }

        /** The symbol the core names, by its name in Symbol.swift. */
        private fun symbol(name: String): Symbol = when (name) {
            "laptop" -> Symbol.Laptop
            "smartphone" -> Symbol.Smartphone
            "tablet" -> Symbol.Tablet
            "monitor" -> Symbol.Monitor
            "bot" -> Symbol.Bot
            else -> Symbol.MonitorSmartphone
        }
    }
}
