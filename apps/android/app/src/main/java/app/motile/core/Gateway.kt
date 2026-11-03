package app.motile.core

import android.content.Intent
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.net.toUri
import app.motile.platform.Platform
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlin.math.ceil
import kotlin.math.max

/** The account's gateway as its settings page shows it, as the core's `gateway::View` says. */
data class GatewayView(
    /** The account has a gateway. */
    val exists: Boolean = false,
    val name: String? = null,
    val key: String? = null,
    val connection: Connection = Connection.Off,
    /** "Connected", "Can't reach your gateway": shown as it is. */
    val statusText: String = "Not connected",
    val error: String? = null,
    val version: String? = null,
    /** Claude Code's first, each agent's in the order they are used. */
    val accounts: List<GatewayAccount> = emptyList(),
    val capture: GatewayCapture = GatewayCapture(),
    /** The sign-in under way. */
    val signIn: GatewaySignIn? = null,
    /** The newest entry of the request log, which changes when a request comes. */
    val logHead: Long? = null,
    /** Null while the gateway isn't known, or is too old to have MCP servers. */
    val mcpServers: List<McpServerRow>? = null,
    /** The newest call of the MCP servers, which changes when an agent calls one. */
    val callsHead: Long? = null,
    /** The MCPs the add sheet offers. */
    val known: List<KnownMcpServer> = emptyList(),
) {
    enum class Connection(val raw: String) {
        Off("off"), Connecting("connecting"), Connected("connected"), Disconnected("disconnected"), Refused("refused");

        companion object {
            fun of(raw: String) = entries.firstOrNull { it.raw == raw } ?: Off
        }
    }

    fun accounts(of: Agent): List<GatewayAccount> = accounts.filter { it.agent == of }

    companion object {
        fun of(json: JSON) = GatewayView(
            exists = json.bool("exists"),
            name = json.optionalString("name"),
            key = json.optionalString("key"),
            connection = Connection.of(json.string("connection")),
            statusText = json.string("status_text"),
            error = json.optionalString("error"),
            version = json.optionalString("version"),
            accounts = json.objects("accounts").map(GatewayAccount::of),
            capture = json.obj("capture")?.let(GatewayCapture::of) ?: GatewayCapture(),
            signIn = json.obj("sign_in")?.let(GatewaySignIn::of),
            logHead = json.optionalLong("log_head"),
            mcpServers = (json["mcp_servers"] as? JsonArray)?.let { json.objects("mcp_servers").map(McpServerRow::of) },
            callsHead = json.optionalLong("calls_head"),
            known = json.objects("known").map(KnownMcpServer::of),
        )
    }
}

/** An account of Claude Code or Codex that the gateway serves turns from. */
data class GatewayAccount(
    val id: String,
    val agent: Agent,
    val name: String,
    /** Who is signed in. */
    val email: String?,
    val plan: String?,
    val status: Status,
    /** "Off", "Sign in again", "Resting until 14:30". None while it is ready. */
    val statusText: String?,
    /** Lower is used first. */
    val priority: Int,
    /** "Priority 1". */
    val priorityText: String,
    val enabled: Boolean,
    /** New threads of its agent start on it. */
    val next: Boolean,
    val windows: List<LimitsReport.Window>,
    /** Why there are no windows. */
    val note: String?,
    /** "3 threads": the threads on it now. None when none is. */
    val threadsText: String?,
) {
    enum class Status(val raw: String) {
        Ready("ready"), Off("off"), SignInAgain("sign_in_again"), Resting("resting");

        companion object {
            fun of(raw: String) = entries.firstOrNull { it.raw == raw } ?: Ready
        }
    }

    companion object {
        fun of(json: JSON) = GatewayAccount(
            id = json.string("id"),
            agent = Agent.of(json.string("agent")) ?: Agent.Claude,
            name = json.string("name"),
            email = json.optionalString("email"),
            plan = json.optionalString("plan"),
            status = Status.of(json.string("status")),
            statusText = json.optionalString("status_text"),
            priority = json.int("priority"),
            priorityText = json.string("priority_text"),
            enabled = json.bool("enabled"),
            next = json.bool("next"),
            windows = json.objects("windows").map(LimitsReport::Window),
            note = json.optionalString("note"),
            threadsText = json.optionalString("threads_text"),
        )
    }
}

/** Whether the gateway keeps requests in full, and what it keeps. */
data class GatewayCapture(val on: Boolean = false, val text: String = "Not capturing.") {
    companion object {
        fun of(json: JSON) = GatewayCapture(json.bool("on"), json.string("text"))
    }
}

/** An account's sign-in the gateway started: Claude's page shows a code to paste back, Codex's takes the code shown here. */
data class GatewaySignIn(
    val agent: Agent,
    /** "Sign In to Claude Code", "Sign In to Claude Code Again". */
    val title: String,
    /** The id of the account it signs in again. */
    val again: String?,
    /** Who signs in again: the account's email, or its name. */
    val account: String?,
    val url: String,
    val userCode: String?,
    val needsCode: Boolean,
    /** The gateway is finishing it. */
    val waiting: Boolean,
    val text: String,
    val error: String?,
) {
    companion object {
        fun of(json: JSON) = GatewaySignIn(
            agent = Agent.of(json.string("agent")) ?: Agent.Claude,
            title = json.string("title"),
            again = json.optionalString("again"),
            account = json.optionalString("account"),
            url = json.string("url"),
            userCode = json.optionalString("user_code"),
            needsCode = json.bool("needs_code"),
            waiting = json.bool("waiting"),
            text = json.string("text"),
            error = json.optionalString("error"),
        )
    }
}

/** How to run a new gateway, and the command with the code that links it to the account. */
data class GatewayEnroll(
    val code: String,
    val expiresAt: Double,
    val command: String,
    /** The command's colours, as the core's `[start, length, palette index]` spans. */
    val spans: List<Int>,
    val steps: List<String>,
) {
    fun isExpired(at: Double = Time.now()): Boolean = expiresAt <= at

    /** How long the code still works, as "14:59". */
    fun timeLeft(at: Double = Time.now()): String {
        val seconds = max(0, ceil(expiresAt - at).toInt())
        return "%d:%02d".format(seconds / 60, seconds % 60)
    }

    companion object {
        fun of(json: JSON) = GatewayEnroll(
            code = json.string("code"),
            expiresAt = json.double("expires_at"),
            command = json.string("command"),
            spans = json.ints("spans").toList(),
            steps = json.strings("steps"),
        )
    }
}

/** A request the gateway passed on, as its log lists it. */
data class GatewayLogRow(
    val id: Long,
    /** "14:32:05" today, "Nov 1, 14:32" before. */
    val time: String,
    val server: String,
    val thread: String,
    /** The account's name. */
    val account: String?,
    val model: String?,
    val path: String,
    val status: Int,
    val ok: Boolean,
    val latency: String,
    val tokens: String,
    val retries: String?,
    val size: String,
    val error: String?,
    /** The gateway kept it in full. */
    val captured: Boolean,
) {
    companion object {
        fun of(json: JSON) = GatewayLogRow(
            id = json.long("id"),
            time = json.string("time"),
            server = json.string("server"),
            thread = json.string("thread"),
            account = json.optionalString("account"),
            model = json.optionalString("model"),
            path = json.string("path"),
            status = json.int("status"),
            ok = json.bool("ok"),
            latency = json.string("latency"),
            tokens = json.string("tokens"),
            retries = json.optionalString("retries"),
            size = json.string("size"),
            error = json.optionalString("error"),
            captured = json.bool("captured"),
        )
    }
}

/** A captured request in full, its bodies cut into lines ready to lay out a few at a time. */
data class GatewayCaptured(
    val row: GatewayLogRow,
    val requestHeaders: List<Pair<String, String>>,
    val requestBody: String,
    val requestLines: List<String>,
    val responseHeaders: List<Pair<String, String>>,
    val responseBody: String,
    val responseLines: List<String>,
) {
    companion object {
        /** The longest stretch of a line one row lays out. */
        private const val LINE_CHUNK = 2000

        fun of(json: JSON): GatewayCaptured {
            val request = json.string("request_body")
            val response = json.string("response_body")
            return GatewayCaptured(
                row = GatewayLogRow.of(json.obj("row") ?: EmptyJSON),
                requestHeaders = headers(json, "request_headers"),
                requestBody = request,
                requestLines = lines(request),
                responseHeaders = headers(json, "response_headers"),
                responseBody = response,
                responseLines = lines(response),
            )
        }

        private fun headers(json: JSON, key: String): List<Pair<String, String>> =
            (json[key] as? JsonArray)?.mapNotNull { pair ->
                val parts = (pair as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.content } ?: return@mapNotNull null
                if (parts.size < 2) return@mapNotNull null
                parts[0] to parts[1]
            } ?: emptyList()

        fun lines(text: String): List<String> {
            if (text.isEmpty()) return emptyList()
            return text.split('\n').flatMap { line -> if (line.length <= LINE_CHUNK) listOf(line) else line.chunked(LINE_CHUNK) }
        }
    }
}

/** The account's gateway: what its page shows and what it does to it, through the core. */
class Gateway {
    var store: AppStore? = null
    var view by mutableStateOf(GatewayView())
        private set
    /** How to run a new gateway, while the sheet that adds one shows it. */
    var enroll by mutableStateOf<GatewayEnroll?>(null)
        private set
    var enrollPending by mutableStateOf(false)
        private set
    var enrollError by mutableStateOf<String?>(null)
        private set
    /** The agent whose sign-in is being started. */
    var startingSignIn by mutableStateOf<Agent?>(null)
        private set
    val log = GatewayLog(this)
    val mcpServers = McpServers(this)
    private var watchers = 0
    /** The gateway the add sheet would replace, so that it closes once another joins. */
    var replacing by mutableStateOf<String?>(null)
        private set

    fun apply(view: GatewayView) {
        if (this.view != view) this.view = view
    }

    internal fun send(type: String, fields: Map<String, Any?> = emptyMap(), reply: ((Result<JSON>) -> Unit)? = null) {
        store?.core?.send(type, fields, reply)
    }

    private fun alert(result: Result<*>) {
        store?.alert(result)
    }

    /** A view that shows the gateway appeared or went: the core follows the gateway while one shows it. */
    fun watch(on: Boolean) {
        val watched = watchers > 0
        watchers = max(0, watchers + if (on) 1 else -1)
        if ((watchers > 0) != watched) send("watch_gateway", mapOf("on" to (watchers > 0)))
    }

    // Adding and removing

    /** The add sheet opens: a code to join with, and the account watched so that the sheet sees the gateway join. */
    fun startAdding() {
        replacing = view.key
        send("watch_servers", mapOf("on" to true))
        val current = enroll
        if (current != null && !current.isExpired()) return
        makeCode()
    }

    fun stopAdding() {
        send("watch_servers", mapOf("on" to false))
        if (joined) enroll = null
    }

    /** A gateway other than the one there was when the sheet opened is on the account. */
    val joined: Boolean get() = view.exists && view.key != replacing

    fun makeCode() {
        if (enrollPending) return
        enrollPending = true
        enrollError = null
        send("add_gateway") { result ->
            enrollPending = false
            result.onSuccess { enroll = GatewayEnroll.of(it) }
            enrollError = result.coreError?.message
        }
    }

    fun remove() = send("remove_gateway", reply = ::alert)

    // Accounts

    /** Starts an account's sign-in; the sheet follows it from the view's `signIn` and shows the page to open. */
    fun signIn(agent: Agent, again: String? = null) {
        if (startingSignIn != null) return
        startingSignIn = agent
        send("gateway_sign_in", mapOf("agent" to agent.raw, "account" to again)) { result ->
            startingSignIn = null
            alert(result)
        }
    }

    /** Hands Claude's sign-in the code its page showed. A refusal comes back as the sign-in's error. */
    fun finishSignIn(code: String) {
        send("gateway_finish_sign_in", mapOf("code" to code.trim())) { result ->
            if (view.signIn?.error == null) alert(result)
        }
    }

    fun cancelSignIn() {
        view = view.copy(signIn = null)
        send("gateway_cancel_sign_in")
    }

    /** Opens the sign-in's page again, in a Custom Tab over the client. */
    fun openPage(url: String) {
        if (url.isEmpty()) return
        val context = Platform.context
        val intent = CustomTabsIntent.Builder().setShowTitle(true).build()
        intent.intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        try {
            intent.launchUrl(context, url.toUri())
        } catch (_: Exception) {
            Platform.open(url)
        }
    }

    fun rename(account: GatewayAccount, name: String) = update(account, mapOf("type" to "name", "name" to name.trim())) { it.copy(name = name.trim()) }

    /** `priority` counts from 0, which is used first. */
    fun setPriority(account: GatewayAccount, priority: Int) = update(account, mapOf("type" to "priority", "priority" to priority)) { it.copy(priority = priority) }

    fun setEnabled(account: GatewayAccount, enabled: Boolean) = update(account, mapOf("type" to "enabled", "enabled" to enabled)) { it.copy(enabled = enabled) }

    /** Shows the change at once; the gateway's state, which follows, has the last word. */
    private fun update(account: GatewayAccount, change: Map<String, Any?>, shown: (GatewayAccount) -> GatewayAccount) {
        view = view.copy(accounts = view.accounts.map { if (it.id == account.id) shown(it) else it })
        send("gateway_update_account", mapOf("account" to account.id, "change" to change), reply = ::alert)
    }

    fun removeAccount(account: GatewayAccount) = send("gateway_remove_account", mapOf("account" to account.id), reply = ::alert)

    // Capturing

    fun captureNext(count: Int = 20) = capture(mapOf("type" to "next", "left" to count))

    fun captureThread(thread: ThreadInfo) = capture(mapOf("type" to "thread", "server" to thread.serverID, "thread" to thread.id))

    fun stopCapturing() = capture(mapOf("type" to "off"))

    private fun capture(capture: Map<String, Any?>) = send("gateway_capture", mapOf("capture" to capture), reply = ::alert)

    /** A captured request in full, its bodies cut into lines off the main thread. Null when the gateway no longer has it. */
    fun captured(entry: Long, reply: (Result<GatewayCaptured?>) -> Unit) {
        val core = store?.core ?: return
        core.send("gateway_captured", mapOf("entry" to entry), read = { answer -> Found(answer.obj("request")?.let(GatewayCaptured::of)) }) { result ->
            reply(result.map { it.captured })
        }
    }

    /** What `captured` read, which may be nothing. */
    private class Found(val captured: GatewayCaptured?)
}

/** The gateway's request log, a page at a time, newest first. */
class GatewayLog(private val gateway: Gateway) {
    var rows by mutableStateOf(emptyList<GatewayLogRow>())
        private set
    var more by mutableStateOf(false)
        private set
    var loading by mutableStateOf(false)
        private set
    var error by mutableStateOf<String?>(null)
        private set
    private var errorsOnlyState by mutableStateOf(false)
    var errorsOnly: Boolean
        get() = errorsOnlyState
        set(value) {
            if (value == errorsOnlyState) return
            errorsOnlyState = value
            rows = emptyList()
            reload()
        }
    /** Counts what was asked, so that only the last answer is shown. */
    private var asked = 0

    fun reload() = load(before = null)

    fun loadMore() {
        if (!more || loading) return
        load(before = rows.lastOrNull()?.id)
    }

    /** Puts the requests that came since the newest row shown on top, or starts over from the newest page when more came than a page holds. */
    fun refresh() {
        if (loading) return
        val number = asked
        gateway.send("gateway_log", mapOf("errors_only" to errorsOnly)) { result ->
            val answer = result.getOrNull() ?: return@send
            if (number != asked) return@send
            val page = answer.objects("rows").map(GatewayLogRow::of)
            val newest = rows.firstOrNull()?.id ?: 0
            val fresh = page.filter { it.id > newest }
            if (fresh.size == page.size && rows.isNotEmpty()) {
                rows = page
                more = answer.bool("more")
                return@send
            }
            rows = fresh + rows
        }
    }

    private fun load(before: Long?) {
        asked += 1
        val number = asked
        loading = true
        val fields = buildMap<String, Any?> {
            put("errors_only", errorsOnly)
            before?.let { put("before", it) }
        }
        gateway.send("gateway_log", fields) { result ->
            if (number != asked) return@send
            loading = false
            val answer = result.getOrElse {
                error = result.coreError?.message
                return@send
            }
            error = null
            val page = answer.objects("rows").map(GatewayLogRow::of)
            rows = if (before == null) page else rows + page
            more = answer.bool("more")
        }
    }
}
