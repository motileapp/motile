package app.motile.core

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** What the usage route shows, kept while it is closed so that it opens with what was last read. */
class UsageModel {
    enum class Tab(val raw: String) {
        Activity("activity"), Cost("cost"), Tokens("tokens"), Limits("limits");

        /** What the tab asks the servers for; the cost and the tokens are read together. */
        val reads: Int
            get() = when (this) {
                Activity -> 0
                Cost, Tokens -> 1
                Limits -> 2
            }

        val label: String
            get() = when (this) {
                Activity -> "Activity"
                Cost -> "Cost"
                Tokens -> "Tokens"
                Limits -> "Limits"
            }
    }

    enum class Period(val raw: String) {
        Day("day"), Week("week"), Month("month"), Quarter("quarter");

        val label: String
            get() = when (this) {
                Day -> "24h"
                Week -> "7d"
                Month -> "30d"
                Quarter -> "90d"
            }

        val bucketSeconds: Int get() = if (this == Day) 3600 else 86400

        val buckets: Int
            get() = when (this) {
                Day -> 24
                Week -> 7
                Month -> 30
                Quarter -> 90
            }
    }

    private var tabState by mutableStateOf(Tab.entries.firstOrNull { it.raw == Preferences.string(TAB_KEY) } ?: Tab.Activity)
    var tab: Tab
        get() = tabState
        set(value) {
            val old = tabState
            tabState = value
            Preferences.set(TAB_KEY, value.raw)
            if (old.reads != value.reads) load()
        }

    private var periodState by mutableStateOf(Period.entries.firstOrNull { it.raw == Preferences.string(PERIOD_KEY) } ?: Period.Month)
    var period: Period
        get() = periodState
        set(value) {
            periodState = value
            Preferences.set(PERIOD_KEY, value.raw)
            load()
        }

    /** The servers picked. None picked counts all of them, as does every one. */
    private var serversState by mutableStateOf(Preferences.stringList(SERVERS_KEY).toSet())
    var servers: Set<String>
        get() = serversState
        set(value) {
            serversState = value
            Preferences.set(SERVERS_KEY, value.toList())
            load()
        }

    var limits by mutableStateOf<Loaded<LimitsReport>>(Loaded.Loading)
        private set
    var spending by mutableStateOf<Loaded<UsageReport>>(Loaded.Loading)
        private set
    var activity by mutableStateOf<Loaded<ActivityReport>>(Loaded.Loading)
        private set
    var refreshing by mutableStateOf(false)
        private set
    private var store: AppStore? = null
    /** Counts what was asked, so that only the last answer is shown. */
    private var asked = 0

    fun start(store: AppStore) {
        this.store = store
        load()
    }

    /** Asks the servers again for what the tab shows. */
    fun refresh() = load()

    /** The servers counted among these, or null for all of them. */
    private fun picked(all: List<Server>): Set<String>? {
        val picked = servers.intersect(all.map { it.id }.toSet())
        if (picked.isEmpty() || picked.size >= all.size) return null
        return picked
    }

    fun toggle(server: Server, all: List<Server>) {
        val picked = servers.intersect(all.map { it.id }.toSet()).toMutableSet()
        if (!picked.remove(server.id)) picked.add(server.id)
        servers = picked
    }

    /** "All servers", the one server's name, or how many. */
    fun serversLabel(all: List<Server>): String {
        val chosen = picked(all) ?: return "All servers"
        val named = all.filter { it.id in chosen }
        return if (named.size == 1) named[0].name else "${named.size} servers"
    }

    /** Shows what the core kept for the tab at once, then asks the servers. */
    private fun load() {
        val store = store ?: return
        asked += 1
        val number = asked
        refreshing = true
        val counted = picked(store.usageServers)
        if (tab == Tab.Limits) {
            store.loadLimits(kept = true, servers = counted) { kept ->
                if (number != asked) return@loadLimits
                kept.getOrNull()?.takeIf { it.sections.isNotEmpty() }?.let { limits = Loaded.Ready(it) }
                store.loadLimits(kept = false, servers = counted) { again ->
                    if (number != asked) return@loadLimits
                    refreshing = false
                    limits = Loaded.of(again, limits)
                }
            }
        } else if (tab == Tab.Activity) {
            val seconds = period.bucketSeconds
            val buckets = period.buckets
            store.loadActivity(seconds, buckets, kept = true, servers = counted) { kept ->
                if (number != asked) return@loadActivity
                kept.getOrNull()?.let { activity = Loaded.Ready(it) }
                store.loadActivity(seconds, buckets, kept = false, servers = counted) { result ->
                    if (number != asked) return@loadActivity
                    refreshing = false
                    activity = Loaded.of(result, activity)
                }
            }
        } else {
            val seconds = period.bucketSeconds
            val buckets = period.buckets
            store.loadUsage(seconds, buckets, kept = true, servers = counted) { kept ->
                if (number != asked) return@loadUsage
                kept.getOrNull()?.let { spending = Loaded.Ready(it) }
                store.loadUsage(seconds, buckets, kept = false, servers = counted) { result ->
                    if (number != asked) return@loadUsage
                    refreshing = false
                    spending = Loaded.of(result, spending)
                }
            }
        }
    }

    companion object {
        private const val TAB_KEY = "usage.tab"
        private const val PERIOD_KEY = "usage.period"
        private const val SERVERS_KEY = "usage.servers"
    }
}
