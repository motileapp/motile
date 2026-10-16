package app.motile.core

import android.net.Uri
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import app.motile.panel.PanelTarget
import app.motile.panel.PullRequestPage
import app.motile.panel.PullRequestText
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.LocalPalette
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject

/** A Ricrac workspace a server is connected to, and the user who connected it. */
data class RicracConnection(val id: String, val workspace: String, val user: String) {
    constructor(json: JSON) : this(json.string("id"), json.string("workspace"), json.string("user"))
}

/** A status a team's issues can have. */
data class RicracState(val id: String, val name: String, val kind: String, val hex: Color?) {
    constructor(json: JSON) : this(json.string("id"), json.string("name"), json.string("kind"), PullRequestPage.color(json.string("color")))

    /** Its colour at Ricrac, or a muted one when Ricrac gave none. */
    val color: Color @Composable @ReadOnlyComposable get() = hex ?: LocalPalette.current.mutedStrongerForeground

    val symbol: Symbol
        get() = when (kind) {
            "triage" -> Symbol.CircleDotDashed
            "backlog" -> Symbol.CircleDashed
            "started" -> Symbol.CircleDot
            "completed" -> Symbol.CircleCheck
            "canceled" -> Symbol.CircleX
            else -> Symbol.Circle
        }

    companion object {
        /** The kinds of status in the order an issue goes through them, with what Ricrac calls them. */
        val kinds = listOf(
            "triage" to "Triage", "backlog" to "Backlog", "unstarted" to "Todo", "started" to "In Progress",
            "completed" to "Done", "canceled" to "Canceled",
        )
    }
}

data class RicracUser(
    val id: String,
    val name: String,
    /** The one who connected the workspace. */
    val me: Boolean,
) {
    constructor(json: JSON) : this(json.string("id"), json.string("name"), json.bool("me"))
}

data class RicracTeam(
    val id: String,
    val name: String,
    /** In the order an issue goes through them. */
    val states: List<RicracState>,
) {
    constructor(json: JSON) : this(json.string("id"), json.string("name"), json.objects("states").map(::RicracState))
}

/** The issues of one status, as the core lists them. */
data class RicracGroup(val state: RicracState, val rows: List<RicracRow>) {
    constructor(json: JSON) : this(RicracState(json.obj("state") ?: EmptyJSON), json.objects("rows").map(::RicracRow))
}

data class RicracRow(
    val id: String,
    val identifier: String,
    val title: String,
    val url: String?,
    val priority: Int,
    val priorityLabel: String,
    val team: String,
    val assignee: String?,
    val assigneeID: String?,
    val initials: String?,
    val labels: List<Label>,
    val updatedAt: Double,
) {
    data class Label(val name: String, val hex: Color?) {
        val color: Color @Composable @ReadOnlyComposable get() = hex ?: LocalPalette.current.mutedStrongerForeground
    }

    constructor(json: JSON) : this(
        id = json.string("id"),
        identifier = json.string("identifier"),
        title = json.string("title"),
        url = json.string("url").takeIf { it.isNotEmpty() },
        priority = json.int("priority"),
        priorityLabel = json.string("priority_label"),
        team = json.string("team"),
        assignee = json.optionalString("assignee"),
        assigneeID = json.optionalString("assignee_id"),
        initials = json.optionalString("initials"),
        labels = json.objects("labels").map { Label(it.string("name"), PullRequestPage.color(it.string("color"))) },
        updatedAt = json.double("updated_at"),
    )

    val prioritySymbol: Symbol get() = symbol(priority)

    companion object {
        /** Ricrac's priorities in the order it offers them, with what it calls them. */
        val priorities = listOf(0 to "No priority", 1 to "Urgent", 2 to "High", 3 to "Medium", 4 to "Low")

        fun symbol(priority: Int): Symbol = when (priority) {
            1 -> Symbol.TriangleAlert
            2 -> Symbol.SignalHigh
            3 -> Symbol.SignalMedium
            4 -> Symbol.SignalLow
            else -> Symbol.Ellipsis
        }
    }
}

/** One issue as its tab shows it, set by the core. Read off the main thread. */
class RicracPage(json: JSON) {
    class Comment(val id: String, val author: String, val initials: String, val at: Double, val body: List<PullRequestText>)

    val row = RicracRow(json.obj("row") ?: EmptyJSON)
    val state = RicracState(json.obj("state") ?: EmptyJSON)
    val description = PullRequestText.blocks(json.objects("description"))
    val comments = json.objects("comments").map { comment ->
        Comment(
            comment.string("id"), comment.string("author"), comment.string("initials"), comment.double("at"),
            PullRequestText.blocks(comment.objects("body")),
        )
    }
    /** What hands the issue to an agent. */
    val prompt = json.string("prompt")
}

/** The workspace and team the Ricrac tab lists for a project. */
data class RicracPick(val workspace: String? = null, val team: String? = null) {
    val json: Map<String, Any?> get() = mapOf("workspace" to workspace, "team" to team)

    companion object {
        fun of(json: JSON) = RicracPick(json.optionalString("workspace"), json.optionalString("team"))
    }
}

/** How the Ricrac tab lists issues, the same in every project. */
data class RicracFilter(
    val mine: Boolean = true,
    /** The kinds of status listed, or none for all of them. */
    val states: List<String> = listOf("unstarted", "started"),
    /** The statuses folded up, by id. */
    val folded: Set<String> = emptySet(),
) {
    val json: Map<String, Any?> get() = mapOf("mine" to mine, "states" to states, "folded" to folded.toList())

    companion object {
        fun of(json: JSON) = RicracFilter(json.bool("mine"), json.strings("states"), json.strings("folded").toSet())
    }
}

/** What the Ricrac tab lists for a project. */
data class RicracChoice(val workspace: String?, val team: String?, val mine: Boolean, val states: List<String>)

/** The Ricrac workspaces the servers are connected to, and the issues the tab lists. */
class Ricrac {
    var store: AppStore? = null

    /** By server, as last heard. */
    private var connections by mutableStateOf(mapOf<String, List<RicracConnection>>())
    /** The server that waits for the user to approve it at Ricrac. */
    var connecting by mutableStateOf<String?>(null)
        private set
    var error by mutableStateOf<String?>(null)
    /** By workspace. */
    var teams by mutableStateOf(mapOf<String, List<RicracTeam>>())
        private set
    /** Who can be assigned, by workspace. */
    var users by mutableStateOf(mapOf<String, List<RicracUser>>())
        private set
    /** The lists read so far, by what asked for them, so that a tab opens as it was left. */
    private var lists by mutableStateOf(mapOf<Query, Loaded<List<RicracGroup>>>())
    /** The open issues, by id. */
    var pages by mutableStateOf(mapOf<String, Loaded<RicracPage>>())
        private set
    /**
     * The issues that are being changed, those commented on as `comment:` and their id, those read
     * as `read:` and their id, `issues` while the list is read and `create` while one is filed.
     */
    var working by mutableStateOf(setOf<String>())
        private set
    /** By project. */
    private var picks = keptPicks()
    /** What was picked last, which a project that has none of its own starts from. */
    private var lastPick by mutableStateOf(Preferences.string("ricrac.last")?.let(::parseJSON)?.let(RicracPick::of))
    var filter by mutableStateOf(Preferences.string("ricrac.filter")?.let(::parseJSON)?.let(RicracFilter::of) ?: RicracFilter())
        private set
    /** What the tab searches for, by project, while the client runs. */
    var searches by mutableStateOf(mapOf<String, String>())

    private var session: SignInSession? = null
    /** What the tab lists now. */
    private var listed by mutableStateOf<Query?>(null)

    /** What a list is asked with. Two threads of one project ask the same. */
    private data class Query(
        val serverID: String,
        val workspace: String,
        val team: String?,
        val mine: Boolean,
        val states: List<String>,
        val search: String,
    )

    /** The issues the tab lists. */
    val issues: Loaded<List<RicracGroup>>
        get() {
            val listed = listed ?: return Loaded.Loading
            return lists[listed] ?: Loaded.Loading
        }

    private fun keptPicks(): Map<String, RicracPick> {
        val kept = Preferences.string("ricrac.choices")?.let(::parseJSON) ?: return emptyMap()
        return kept.mapNotNull { (project, pick) -> (pick as? JsonObject)?.let { project to RicracPick.of(it) } }.toMap()
    }

    /** The workspaces the server is connected to. */
    fun connected(serverID: String): List<RicracConnection> = connections[serverID] ?: emptyList()

    fun choice(target: PanelTarget): RicracChoice {
        val pick = pick(target)
        return RicracChoice(pick.workspace, pick.team, filter.mine, filter.states)
    }

    /** What was picked for the project last, or else for any project, in a workspace the server is still connected to. */
    private fun pick(target: PanelTarget): RicracPick {
        val connected = connected(target.serverID)
        val kept = listOfNotNull(picks[target.projectID], lastPick)
        return kept.firstOrNull { pick -> connected.any { it.id == pick.workspace } } ?: RicracPick(connected.firstOrNull()?.id)
    }

    fun pick(workspace: String?, team: String? = null, target: PanelTarget) {
        val pick = RicracPick(workspace, team)
        picks = picks + (target.projectID to pick)
        lastPick = pick
        Preferences.set("ricrac.choices", jsonOf(picks.mapValues { it.value.json }).toString())
        Preferences.set("ricrac.last", jsonOf(pick.json).toString())
    }

    fun changeFilter(change: (RicracFilter) -> RicracFilter) {
        filter = change(filter)
        Preferences.set("ricrac.filter", jsonOf(filter.json).toString())
    }

    fun fold(state: String) = changeFilter { filter ->
        filter.copy(folded = if (state in filter.folded) filter.folded - state else filter.folded + state)
    }

    /** Asks the server which workspaces it is connected to. */
    fun read(serverID: String) {
        val server = store?.server(serverID) ?: return
        if (server.protocolVersion < 29) return
        if (connections[serverID] == null) {
            val kept = Preferences.string("ricrac-$serverID")?.let { text ->
                runCatching { kotlinx.serialization.json.Json.parseToJsonElement(text) as? JsonArray }.getOrNull()
            }
            if (kept != null) connections = connections + (serverID to kept.mapNotNull { (it as? JsonObject)?.let(::RicracConnection) })
        }
        ask(json("type" to "ricrac_status"), serverID)
    }

    /** Has the user pick a workspace and approve Motile at Ricrac in the browser, then hands what Ricrac sent back to the server, which alone can finish with it. */
    fun connect(serverID: String) {
        val store = store ?: return
        if (connecting != null) return
        connecting = serverID
        error = null
        store.core.send("request", json("server_id" to serverID, "request" to mapOf("type" to "ricrac_connect"))) { result ->
            val url = result.getOrNull()?.string("url")?.takeIf { it.isNotEmpty() }
            if (url == null) {
                connecting = null
                error = result.coreError?.message ?: "The connection couldn't be started."
                return@send
            }
            val session = SignInSession()
            this.session = session
            session.start(Platform.context, url) { callback ->
                this.session = null
                val sent = callback?.let(Uri::parse)
                val code = sent?.getQueryParameter("code")
                val state = sent?.getQueryParameter("state")
                if (code == null || state == null) {
                    connecting = null
                    val refusal = sent?.getQueryParameter("error")
                    if (refusal != null && refusal != "access_denied") error = "Ricrac didn't connect: $refusal"
                    return@start
                }
                ask(json("type" to "ricrac_finish", "code" to code, "state" to state), serverID)
            }
        }
    }

    fun disconnect(workspace: String, serverID: String) {
        ask(json("type" to "ricrac_disconnect", "workspace" to workspace), serverID)
    }

    /**
     * Asks the server for the issues the project's tab lists, or for the ones that have the words of
     * `search`, and for the workspace's teams and users when they aren't known yet. A list read
     * before shows at once while it is read again.
     */
    fun load(target: PanelTarget, search: String = "") {
        val choice = choice(target)
        val workspace = choice.workspace ?: return
        val query = Query(target.serverID, workspace, choice.team, choice.mine, choice.states, search.trim())
        val listed = listed
        if (listed != null && listed != query && listed.search.isNotEmpty()) lists = lists - listed
        this.listed = query
        if (teams[workspace] == null) loadTeams(workspace, target.serverID)
        fetch(query)
    }

    private fun fetch(query: Query) {
        val store = store ?: return
        if (lists[query] == null) lists = lists + (query to Loaded.Loading)
        working = working + "issues"
        val closed = query.states.isEmpty() || query.states.any { it == "completed" || it == "canceled" }
        val command = buildMap {
            put("server_id", query.serverID)
            put("workspace", query.workspace)
            put("mine", query.mine)
            put("closed", closed)
            put("states", query.states)
            query.team?.let { put("team", it) }
            if (query.search.isNotEmpty()) put("search", query.search)
        }
        store.core.send("ricrac_issues", command, read = { answer -> answer.objects("groups").map(::RicracGroup) }) { result ->
            val shown = listed == query
            if (shown) working = working - "issues"
            result.onSuccess { groups ->
                lists = lists + (query to Loaded.Ready(groups))
                if (shown) error = null
            }
            val failure = result.coreError ?: return@send
            if (!shown) return@send
            if (lists[query]?.value == null) lists = lists + (query to Loaded.Failed(failure.message)) else error = failure.message
            // Ricrac may have ended the connection.
            ask(json("type" to "ricrac_status"), query.serverID)
        }
    }

    /** Reads the issue for its tab, and with `comment` says that on it first. */
    fun loadIssue(id: String, workspace: String, serverID: String, comment: String? = null, done: (() -> Unit)? = null) {
        val store = store ?: return
        val command = buildMap {
            put("server_id", serverID)
            put("workspace", workspace)
            put("issue", id)
            if (comment != null) put("comment", comment)
        }
        if (comment != null) working = working + "comment:$id"
        if (pages[id]?.value == null) pages = pages + (id to Loaded.Loading)
        working = working + "read:$id"
        store.core.send("ricrac_issue", command, read = { answer -> RicracPage(answer.obj("page") ?: EmptyJSON) }) { result ->
            working = working - "read:$id"
            if (comment != null) working = working - "comment:$id"
            result.onSuccess { page ->
                pages = pages + (id to Loaded.Ready(page))
                error = null
                done?.invoke()
            }
            val failure = result.coreError ?: return@send
            if (pages[id]?.value == null) pages = pages + (id to Loaded.Failed(failure.message)) else error = failure.message
        }
    }

    /** Changes the issue's status, assignee or priority at Ricrac, then reads what shows it again. */
    fun change(id: String, change: Map<String, Any?>, workspace: String, serverID: String) {
        val store = store ?: return
        if (id in working) return
        working = working + id
        val request = mapOf("type" to "ricrac_update", "workspace" to workspace, "issue" to id, "change" to change)
        store.core.send("request", json("server_id" to serverID, "request" to request)) { result ->
            working = working - id
            result.coreError?.let { error = it.message }
            refresh(id, workspace, serverID)
        }
    }

    /** Files an issue and answers with its id and identifier. */
    fun create(issue: Map<String, Any?>, workspace: String, serverID: String, done: (String, String) -> Unit) {
        val store = store ?: return
        if ("create" in working) return
        working = working + "create"
        val request = mapOf("type" to "ricrac_create", "workspace" to workspace, "issue" to issue)
        store.core.send("request", json("server_id" to serverID, "request" to request)) { result ->
            working = working - "create"
            val answer = result.getOrNull()
            if (answer == null) {
                error = result.coreError?.message
                return@send
            }
            val filed = answer.obj("issue") ?: EmptyJSON
            error = null
            refresh(null, workspace, serverID)
            done(filed.string("id"), filed.string("identifier"))
        }
    }

    /**
     * Puts what the issue asks for in the open draft's composer, or from a thread in that of a new
     * draft. An issue nobody works on yet is taken: assigned to the user and moved to the first
     * status of work.
     */
    fun work(page: RicracPage, workspace: String, target: PanelTarget) {
        val taking = mutableMapOf<String, Any?>()
        val states = teams[workspace]?.firstOrNull { it.id == page.row.team }?.states ?: emptyList()
        val started = states.firstOrNull { it.kind == "started" }
        if (page.state.kind in listOf("triage", "backlog", "unstarted") && started != null) taking["state"] = started.id
        val me = users[workspace]?.firstOrNull { it.me }
        if (page.row.assigneeID == null && me != null) taking["assignee"] = me.id
        if (taking.isNotEmpty()) change(page.row.id, taking, workspace, target.serverID)
        store?.startWork(page.prompt, target.projectID)
    }

    private fun refresh(id: String?, workspace: String, serverID: String) {
        if (id != null && pages[id] != null) loadIssue(id, workspace, serverID)
        val listed = listed ?: return
        if (listed.serverID == serverID) fetch(listed)
    }

    private fun loadTeams(workspace: String, serverID: String) {
        val store = store ?: return
        val request = mapOf("type" to "ricrac_teams", "workspace" to workspace)
        store.core.send("request", json("server_id" to serverID, "request" to request)) { result ->
            val answer = result.getOrNull() ?: return@send
            teams = teams + (workspace to answer.objects("teams").map(::RicracTeam))
            users = users + (workspace to answer.objects("users").map(::RicracUser))
        }
    }

    /** The teams and users of the workspace, asked for when they aren't known yet. */
    fun needTeams(workspace: String, serverID: String) {
        if (teams[workspace] != null) return
        loadTeams(workspace, serverID)
    }

    /** Sends a request that `Ricrac` answers, with the server's workspaces. */
    private fun ask(request: JSON, serverID: String) {
        val store = store ?: return
        val finishing = request.string("type") == "ricrac_finish"
        store.core.send("request", json("server_id" to serverID, "request" to request)) { result ->
            if (finishing) connecting = null
            val answer = result.getOrNull()
            if (answer == null) {
                if (request.string("type") != "ricrac_status") error = result.coreError?.message
                return@send
            }
            val before = connected(serverID).map { it.id }.toSet()
            val heard = answer.objects("connections")
            connections = connections + (serverID to heard.map(::RicracConnection))
            Preferences.set("ricrac-$serverID", JsonArray(heard).toString())
            if (!finishing) return@send
            val target = store.panelTarget ?: return@send
            val added = connected(serverID).firstOrNull { it.id !in before } ?: return@send
            pick(added.id, target = target)
        }
    }
}
