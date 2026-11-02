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

/** The account's proxy as its settings page shows it, as the core's `proxy::View` says. */
data class ProxyView(
    /** The account has a proxy. */
    val exists: Boolean = false,
    val name: String? = null,
    val key: String? = null,
    val connection: Connection = Connection.Off,
    /** "Connected", "Can't reach your proxy": shown as it is. */
    val statusText: String = "Not connected",
    val error: String? = null,
    val version: String? = null,
    /** Claude Code's first, each agent's in the order they are used. */
    val accounts: List<ProxyAccount> = emptyList(),
    val capture: ProxyCapture = ProxyCapture(),
    /** The sign-in under way. */
    val signIn: ProxySignIn? = null,
    /** The newest entry of the request log, which changes when a request comes. */
    val logHead: Long? = null,
) {
    enum class Connection(val raw: String) {
        Off("off"), Connecting("connecting"), Connected("connected"), Disconnected("disconnected"), Refused("refused");

        companion object {
            fun of(raw: String) = entries.firstOrNull { it.raw == raw } ?: Off
        }
    }

    fun accounts(of: Agent): List<ProxyAccount> = accounts.filter { it.agent == of }

    companion object {
        fun of(json: JSON) = ProxyView(
            exists = json.bool("exists"),
            name = json.optionalString("name"),
            key = json.optionalString("key"),
            connection = Connection.of(json.string("connection")),
            statusText = json.string("status_text"),
            error = json.optionalString("error"),
            version = json.optionalString("version"),
            accounts = json.objects("accounts").map(ProxyAccount::of),
            capture = json.obj("capture")?.let(ProxyCapture::of) ?: ProxyCapture(),
            signIn = json.obj("sign_in")?.let(ProxySignIn::of),
            logHead = json.optionalLong("log_head"),
        )
    }
}

/** An account of Claude Code or Codex that the proxy serves turns from. */
data class ProxyAccount(
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
        fun of(json: JSON) = ProxyAccount(
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

/** Whether the proxy keeps requests in full, and what it keeps. */
data class ProxyCapture(val on: Boolean = false, val text: String = "Not capturing.") {
    companion object {
        fun of(json: JSON) = ProxyCapture(json.bool("on"), json.string("text"))
    }
}

/** An account's sign-in the proxy started: Claude's page shows a code to paste back, Codex's takes the code shown here. */
data class ProxySignIn(
    val agent: Agent,
    val url: String,
    val userCode: String?,
    val needsCode: Boolean,
    /** The proxy is finishing it. */
    val waiting: Boolean,
    val text: String,
    val error: String?,
) {
    companion object {
        fun of(json: JSON) = ProxySignIn(
            agent = Agent.of(json.string("agent")) ?: Agent.Claude,
            url = json.string("url"),
            userCode = json.optionalString("user_code"),
            needsCode = json.bool("needs_code"),
            waiting = json.bool("waiting"),
            text = json.string("text"),
            error = json.optionalString("error"),
        )
    }
}

/** How to run a new proxy: the code it joins the account with, the variables it is given and the command that does it all. */
data class ProxyEnroll(
    val code: String,
    val expiresAt: Double,
    val variables: List<Pair<String, String>>,
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
        fun of(json: JSON) = ProxyEnroll(
            code = json.string("code"),
            expiresAt = json.double("expires_at"),
            variables = json.objects("variables").map { it.string("name") to it.string("value") },
            command = json.string("command"),
            spans = json.ints("spans").toList(),
            steps = json.strings("steps"),
        )
    }
}

/** A request the proxy passed on, as its log lists it. */
data class ProxyLogRow(
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
    /** The proxy kept it in full. */
    val captured: Boolean,
) {
    companion object {
        fun of(json: JSON) = ProxyLogRow(
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
data class ProxyCaptured(
    val row: ProxyLogRow,
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

        fun of(json: JSON): ProxyCaptured {
            val request = json.string("request_body")
            val response = json.string("response_body")
            return ProxyCaptured(
                row = ProxyLogRow.of(json.obj("row") ?: EmptyJSON),
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

/** The account's proxy: what its page shows and what it does to it, through the core. */
class Proxy {
    var store: AppStore? = null
    var view by mutableStateOf(ProxyView())
        private set
    /** How to run a new proxy, while the sheet that adds one shows it. */
    var enroll by mutableStateOf<ProxyEnroll?>(null)
        private set
    var enrollPending by mutableStateOf(false)
        private set
    var enrollError by mutableStateOf<String?>(null)
        private set
    /** The agent whose sign-in is being started. */
    var startingSignIn by mutableStateOf<Agent?>(null)
        private set
    val log = ProxyLog(this)
    private var watchers = 0
    /** The proxy the add sheet would replace, so that it closes once another joins. */
    var replacing by mutableStateOf<String?>(null)
        private set

    fun apply(view: ProxyView) {
        if (this.view != view) this.view = view
    }

    internal fun send(type: String, fields: Map<String, Any?> = emptyMap(), reply: ((Result<JSON>) -> Unit)? = null) {
        store?.core?.send(type, fields, reply)
    }

    private fun alert(result: Result<*>) {
        store?.alert(result)
    }

    /** A view that shows the proxy appeared or went: the core follows the proxy while one shows it. */
    fun watch(on: Boolean) {
        val watched = watchers > 0
        watchers = max(0, watchers + if (on) 1 else -1)
        if ((watchers > 0) != watched) send("watch_proxy", mapOf("on" to (watchers > 0)))
    }

    // Adding and removing

    /** The add sheet opens: a code to join with, and the account watched so that the sheet sees the proxy join. */
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

    /** A proxy other than the one there was when the sheet opened is on the account. */
    val joined: Boolean get() = view.exists && view.key != replacing

    fun makeCode() {
        if (enrollPending) return
        enrollPending = true
        enrollError = null
        send("add_proxy") { result ->
            enrollPending = false
            result.onSuccess { enroll = ProxyEnroll.of(it) }
            enrollError = result.coreError?.message
        }
    }

    fun remove() = send("remove_proxy", reply = ::alert)

    // Accounts

    /** Starts an account's sign-in and opens its page; the sheet follows it from the view's `signIn`. */
    fun signIn(agent: Agent) {
        if (startingSignIn != null) return
        startingSignIn = agent
        send("proxy_sign_in", mapOf("agent" to agent.raw)) { result ->
            startingSignIn = null
            result.onSuccess { openPage(it.string("url")) }
            alert(result)
        }
    }

    /** Hands Claude's sign-in the code its page showed. A refusal comes back as the sign-in's error. */
    fun finishSignIn(code: String) {
        send("proxy_finish_sign_in", mapOf("code" to code.trim())) { result ->
            if (view.signIn?.error == null) alert(result)
        }
    }

    fun cancelSignIn() {
        view = view.copy(signIn = null)
        send("proxy_cancel_sign_in")
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

    fun rename(account: ProxyAccount, name: String) = update(account, mapOf("type" to "name", "name" to name.trim())) { it.copy(name = name.trim()) }

    /** `priority` counts from 0, which is used first. */
    fun setPriority(account: ProxyAccount, priority: Int) = update(account, mapOf("type" to "priority", "priority" to priority)) { it.copy(priority = priority) }

    fun setEnabled(account: ProxyAccount, enabled: Boolean) = update(account, mapOf("type" to "enabled", "enabled" to enabled)) { it.copy(enabled = enabled) }

    /** Shows the change at once; the proxy's state, which follows, has the last word. */
    private fun update(account: ProxyAccount, change: Map<String, Any?>, shown: (ProxyAccount) -> ProxyAccount) {
        view = view.copy(accounts = view.accounts.map { if (it.id == account.id) shown(it) else it })
        send("proxy_update_account", mapOf("account" to account.id, "change" to change), reply = ::alert)
    }

    fun removeAccount(account: ProxyAccount) = send("proxy_remove_account", mapOf("account" to account.id), reply = ::alert)

    // Capturing

    fun captureNext(count: Int = 20) = capture(mapOf("type" to "next", "left" to count))

    fun captureThread(thread: ThreadInfo) = capture(mapOf("type" to "thread", "server" to thread.serverID, "thread" to thread.id))

    fun stopCapturing() = capture(mapOf("type" to "off"))

    private fun capture(capture: Map<String, Any?>) = send("proxy_capture", mapOf("capture" to capture), reply = ::alert)

    /** A captured request in full, its bodies cut into lines off the main thread. Null when the proxy no longer has it. */
    fun captured(entry: Long, reply: (Result<ProxyCaptured?>) -> Unit) {
        val core = store?.core ?: return
        core.send("proxy_captured", mapOf("entry" to entry), read = { answer -> Found(answer.obj("request")?.let(ProxyCaptured::of)) }) { result ->
            reply(result.map { it.captured })
        }
    }

    /** What `captured` read, which may be nothing. */
    private class Found(val captured: ProxyCaptured?)
}

/** The proxy's request log, a page at a time, newest first. */
class ProxyLog(private val proxy: Proxy) {
    var rows by mutableStateOf(emptyList<ProxyLogRow>())
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
        proxy.send("proxy_log", mapOf("errors_only" to errorsOnly)) { result ->
            val answer = result.getOrNull() ?: return@send
            if (number != asked) return@send
            val page = answer.objects("rows").map(ProxyLogRow::of)
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
        proxy.send("proxy_log", fields) { result ->
            if (number != asked) return@send
            loading = false
            val answer = result.getOrElse {
                error = result.coreError?.message
                return@send
            }
            error = null
            val page = answer.objects("rows").map(ProxyLogRow::of)
            rows = if (before == null) page else rows + page
            more = answer.bool("more")
        }
    }
}
