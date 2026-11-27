package app.motile.core

import app.motile.platform.Symbol
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

// The core's notification settings and what the servers push, as crates/core/src/alerts.rs and
// crates/protocol/src/alerts.rs write them.

/** How alerts reach the user on this device. A phone has them on, with sound, or off. */
enum class NotificationMode(val raw: String) {
    Off("off"), Notifications("notifications"), Sound("sound"), NotificationsAndSound("notifications_and_sound");

    companion object {
        fun of(raw: String?): NotificationMode = entries.firstOrNull { it.raw == raw } ?: NotificationsAndSound
    }
}

/** Which alerts the user wants. */
data class AlertChoices(
    val finished: Boolean = true,
    val failed: Boolean = true,
    val approval: Boolean = true,
    /** Questions and plans. */
    val question: Boolean = true,
    val limit: Boolean = true,
) {
    val json: JSON get() = json("finished" to finished, "failed" to failed, "approval" to approval, "question" to question, "limit" to limit)

    companion object {
        fun of(json: JSON?): AlertChoices = AlertChoices(
            finished = json?.optionalBool("finished") ?: true,
            failed = json?.optionalBool("failed") ?: true,
            approval = json?.optionalBool("approval") ?: true,
            question = json?.optionalBool("question") ?: true,
            limit = json?.optionalBool("limit") ?: true,
        )

        /** The switches under "Alert me when an agent", in order. */
        val rows: List<Pair<String, AlertChoice>> = listOf(
            "Finishes" to AlertChoice({ it.finished }) { choices, on -> choices.copy(finished = on) },
            "Fails" to AlertChoice({ it.failed }) { choices, on -> choices.copy(failed = on) },
            "Needs approval" to AlertChoice({ it.approval }) { choices, on -> choices.copy(approval = on) },
            "Asks a question or presents a plan" to AlertChoice({ it.question }) { choices, on -> choices.copy(question = on) },
            "Reaches its usage limit" to AlertChoice({ it.limit }) { choices, on -> choices.copy(limit = on) },
        )
    }
}

/** One of the choices, read and changed. */
class AlertChoice(val isOn: (AlertChoices) -> Boolean, val with: (AlertChoices, Boolean) -> AlertChoices)

/** This device's notification settings, which the core keeps. */
data class NotificationSettings(
    val mode: NotificationMode = NotificationMode.NotificationsAndSound,
    /** While the client is in front on another thread, an alert shows inside it. Desktop clients only. */
    val toasts: Boolean = false,
    val choices: AlertChoices = AlertChoices(),
    /** The ongoing notification of what the agents do while they work. */
    val live: Boolean = false,
) {
    val notifies: Boolean get() = mode != NotificationMode.Off

    val json: JSON get() = json("mode" to mode.raw, "toasts" to toasts, "choices" to choices.json, "live" to live)

    companion object {
        fun of(json: JSON): NotificationSettings = NotificationSettings(
            mode = NotificationMode.of(json.optionalString("mode")),
            toasts = json.optionalBool("toasts") ?: false,
            choices = AlertChoices.of(json.obj("choices")),
            live = json.optionalBool("live") ?: false,
        )
    }
}

/** What the settings say of notifications on Android. */
object NotificationsText {
    const val title = "Notifications"
    const val mode = "Alerts when an agent finishes, fails, or needs your approval or an answer."
    const val refused = "Allow notifications for Motile in Settings."
    const val toastsTitle = "Show alerts inside Motile"
    const val toasts = "When you're looking at another thread."
    const val choices = "Alert me when an agent"
    const val live = "Ongoing activity"
    const val openSettings = "Open Settings"
    const val liveCaption = "Show what your agents are doing in the notification shade while they work."
    const val widget = "Add the Agents widget to your home screen to see them at a glance."
}

enum class AlertKind(val raw: String) {
    Finished("finished"), Failed("failed"), Approval("approval"), Question("question"), Plan("plan"), Limit("limit"), Other("other");

    /** It waits for the user, rather than telling what happened. */
    val waitsForUser: Boolean get() = this == Approval || this == Question || this == Plan

    val symbol: Symbol
        get() = when (this) {
            Finished -> Symbol.CircleCheck
            Failed, Limit -> Symbol.CircleAlert
            Approval -> Symbol.ShieldQuestionMark
            else -> Symbol.MessageCircleQuestionMark
        }

    /** What a notification says of an alert that couldn't be opened, as `AlertKind::placeholder` does. */
    val placeholder: String
        get() = when (this) {
            Finished -> "An agent finished"
            Failed -> "An agent failed"
            Limit -> "An agent reached its usage limit"
            else -> "An agent needs you"
        }

    companion object {
        fun of(raw: String?): AlertKind = entries.firstOrNull { it.raw == raw } ?: Other
    }
}

/** A server's alert: an agent finished, failed, needs the user or reached its usage limit. */
data class PushedAlert(
    val id: String,
    val serverID: String,
    val threadID: String,
    val kind: AlertKind,
    val threadTitle: String,
    val project: String,
    val body: String,
) {
    companion object {
        fun of(json: JSON) = PushedAlert(
            id = json.string("id"),
            serverID = json.string("server_id"),
            threadID = json.string("thread_id"),
            kind = AlertKind.of(json.optionalString("kind")),
            threadTitle = json.string("thread_title"),
            project = json.string("project"),
            body = json.string("body"),
        )
    }
}

enum class WorkState(val raw: String) {
    Working("working"), Waiting("waiting"), Finished("finished"), Failed("failed"), Other("other");

    /** It works or waits for the user, rather than having ended. */
    val isActive: Boolean get() = this == Working || this == Waiting

    /** Waiting first, then working, then what ended. */
    val rank: Int
        get() = when (this) {
            Waiting -> 0
            Working -> 1
            else -> 2
        }

    companion object {
        fun of(raw: String?): WorkState = entries.firstOrNull { it.raw == raw } ?: Other
    }
}

data class WorkItem(val threadID: String, val title: String, val project: String, val state: WorkState, val since: Double) {
    val json: JSON get() = json("thread_id" to threadID, "title" to title, "project" to project, "state" to state.raw, "since" to since)

    companion object {
        fun of(json: JSON) = WorkItem(
            json.string("thread_id"), json.string("title"), json.string("project"), WorkState.of(json.optionalString("state")), json.double("since"),
        )
    }
}

/** What a server's agents are doing: its first items, and how many work and wait in all. */
data class ServerWork(
    val serverID: String,
    val serverName: String,
    val items: List<WorkItem>,
    val working: Int,
    val waiting: Int,
    val at: Double,
) {
    val json: JSON
        get() = json(
            "server_id" to serverID, "server_name" to serverName, "items" to items.map { it.json },
            "working" to working, "waiting" to waiting, "at" to at,
        )

    companion object {
        fun of(json: JSON) = ServerWork(
            json.string("server_id"), json.string("server_name"), json.objects("items").map(WorkItem::of),
            json.int("working"), json.int("waiting"), json.double("at"),
        )

        fun list(json: JsonArray): List<ServerWork> = json.mapNotNull { (it as? JsonObject)?.let(::of) }

        fun listJSON(servers: List<ServerWork>): String = JsonArray(servers.map { it.json }).toString()

        /** How long a server's work is kept without word from it, as long as the auth server keeps its parts. */
        const val KEPT_FOR = 2 * 60 * 60.0

        /** Each server's latest work, of what was kept and what was just heard, by when its server said it. */
        fun merge(kept: List<ServerWork>, heard: List<ServerWork>, now: Double): List<ServerWork> =
            (kept + heard).filter { now - it.at < KEPT_FOR }.groupBy { it.serverID }.values.map { works -> works.maxBy { it.at } }.sortedBy { it.serverName }
    }
}

/** What a push carries, opened. */
sealed interface Pushed {
    data class Alert(val alert: PushedAlert) : Pushed
    data class Work(val work: ServerWork) : Pushed

    companion object {
        fun of(text: String): Pushed? {
            val json = parseJSON(text) ?: return null
            return when (json.optionalString("type")) {
                "alert" -> Alert(PushedAlert.of(json))
                "work" -> Work(ServerWork.of(json))
                else -> null
            }
        }
    }
}

/** What the agents of every server do together: what waits for the user first, then what works, then what ended. */
data class AgentsWork(val items: List<Item>, val working: Int, val waiting: Int) {
    data class Item(val serverID: String, val threadID: String, val title: String, val project: String, val state: WorkState, val since: Double, val heard: Double) {
        /** It works or waits, but its server said so too long ago to be sure it still does. */
        fun isStale(now: Double): Boolean = state.isActive && now - heard > STALE_AFTER
    }

    val active: Int get() = working + waiting

    /** "2 working", "1 needs you"; "No agents working" when none works or waits. */
    val summaryParts: List<String>
        get() {
            if (active == 0) return listOf("No agents working")
            return buildList {
                if (working > 0) add("$working working")
                if (waiting > 0) add(if (waiting == 1) "1 needs you" else "$waiting need you")
            }
        }

    /** "2 working · 1 needs you". */
    val summary: String get() = summaryParts.joinToString(" · ")

    /** What is still worth showing at `now`: what ended over 15 minutes before is gone. */
    fun shown(now: Double): List<Item> = items.filter { it.state.isActive || now - it.since < ENDED_STAYS }

    companion object {
        /** How long what an agent works or waits on is trusted without word from its server. */
        const val STALE_AFTER = 10 * 60.0

        /** How long a thread that ended stays, as `ENDED_WORK_STAYS_SECS`. */
        const val ENDED_STAYS = 15 * 60.0

        fun of(servers: List<ServerWork>): AgentsWork {
            val seen = mutableSetOf<String>()
            val items = servers
                .flatMap { server -> server.items.map { Item(server.serverID, it.threadID, it.title, it.project, it.state, it.since, server.at) } }
                .sortedWith(compareBy<Item> { it.state.rank }.thenByDescending { it.since })
                .filter { seen.add(it.threadID) }
            return AgentsWork(items, servers.sumOf { it.working }, servers.sumOf { it.waiting })
        }
    }
}

/** The link that opens a thread, as the notifications and the widget give it: `motile://thread?server=…&thread=…`. */
data class ThreadLink(val serverID: String, val threadID: String) {
    val url: String get() = "motile://thread?server=${encode(serverID)}&thread=${encode(threadID)}"

    companion object {
        fun of(url: String): ThreadLink? {
            val uri = runCatching { java.net.URI(url) }.getOrNull() ?: return null
            if (uri.scheme != "motile" || uri.host != "thread") return null
            val query = uri.rawQuery?.split("&")?.mapNotNull { pair ->
                pair.split("=", limit = 2).takeIf { it.size == 2 }?.let { it[0] to java.net.URLDecoder.decode(it[1], "UTF-8") }
            }?.toMap() ?: return null
            val server = query["server"] ?: return null
            val thread = query["thread"] ?: return null
            return ThreadLink(server, thread)
        }

        private fun encode(text: String): String = java.net.URLEncoder.encode(text, "UTF-8")
    }
}

/** A JSON array kept as text, read back. */
internal fun parseJSONArray(text: String?): JsonArray? =
    text?.let { runCatching { kotlinx.serialization.json.Json.parseToJsonElement(it) as? JsonArray }.getOrNull() }

/** The sealed parts a `work` push carries, a JSON array of strings in a string. */
fun sealedParts(text: String?): List<String> = parseJSONArray(text)?.mapNotNull { (it as? JsonPrimitive)?.takeIf { p -> p.isString }?.content } ?: emptyList()
