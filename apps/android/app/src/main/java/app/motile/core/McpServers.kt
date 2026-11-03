package app.motile.core

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.core.net.toUri
import app.motile.platform.Platform
import app.motile.theme.Palette

/** An MCP the account's gateway holds the login of, which every agent is given, as its card shows it. */
data class McpServerRow(
    val id: String,
    val slug: String,
    val name: String,
    val url: String,
    val host: String,
    val monogram: String,
    val tint: String,
    val enabled: Boolean,
    val auth: Auth,
    val state: State,
    /** "Signed in · 30 of 32 tools", "Off · agents don't get it", what went wrong. */
    val statusText: String,
    /** "30 of 32 tools on". */
    val toolsText: String,
    val tools: List<McpServerTool>,
    /** Some tools write and some only read, so "Read Only" changes something. */
    val hasReadOnly: Boolean,
    /** How agents name one of its tools: `mcp__linear__create_issue`. */
    val example: String,
) {
    enum class State(val raw: String) {
        Ready("ready"), Off("off"), Failing("failing"), SignInAgain("sign_in_again");

        companion object {
            fun of(raw: String) = entries.firstOrNull { it.raw == raw } ?: Ready
        }
    }

    enum class Auth(val raw: String) {
        None("none"), Key("key"), SignIn("sign_in");

        companion object {
            fun of(raw: String) = entries.firstOrNull { it.raw == raw } ?: None
        }
    }

    /** Its login stopped working or it fails, which its status says in warning. */
    val troubled: Boolean get() = state == State.SignInAgain || state == State.Failing

    /** How agents name its tools once it is called `slug`. */
    fun example(slug: String): String {
        val tool = example.split("__").lastOrNull() ?: "tool"
        return "mcp__${slug.ifEmpty { this.slug }}__$tool"
    }

    companion object {
        fun of(json: JSON) = McpServerRow(
            id = json.string("id"),
            slug = json.string("slug"),
            name = json.string("name"),
            url = json.string("url"),
            host = json.string("host"),
            monogram = json.string("monogram"),
            tint = json.string("tint"),
            enabled = json.bool("enabled"),
            auth = Auth.of(json.string("auth")),
            state = State.of(json.string("state")),
            statusText = json.string("status_text"),
            toolsText = json.string("tools_text"),
            tools = json.objects("tools").map(McpServerTool::of),
            hasReadOnly = json.bool("has_read_only"),
            example = json.string("example"),
        )
    }
}

data class McpServerTool(val name: String, val title: String, val summary: String?, val writes: Boolean, val enabled: Boolean) {
    companion object {
        fun of(json: JSON) = McpServerTool(
            name = json.string("name"),
            title = json.string("title"),
            summary = json.optionalString("summary"),
            writes = json.bool("writes"),
            enabled = json.bool("enabled"),
        )
    }
}

/** An MCP the add sheet offers by name. */
data class KnownMcpServer(val name: String, val url: String, val host: String, val monogram: String, val tint: String, val keywords: String) {
    fun matches(search: String): Boolean {
        val text = "$name $host $keywords".lowercase()
        return search.lowercase().split(' ').filter { it.isNotEmpty() }.all { it in text }
    }

    companion object {
        fun of(json: JSON) = KnownMcpServer(
            name = json.string("name"),
            url = json.string("url"),
            host = json.string("host"),
            monogram = json.string("monogram"),
            tint = json.string("tint"),
            keywords = json.string("keywords"),
        )
    }
}

/** A call an agent made to an MCP server's tool, or one tried in Settings. */
data class McpServerCallRow(
    val id: Long,
    val time: String,
    val mcpServer: String,
    val monogram: String,
    val tint: String,
    val tool: String,
    /** "Fix the sync race on studio", "Tried in Settings". */
    val source: String,
    val latency: String,
    val ok: Boolean,
    val error: String?,
) {
    /** "14:32:05 · Linear · create_issue · Fix the build on studio · 412 ms". */
    val summary: String get() = listOf(time, mcpServer, tool, source, latency).joinToString(" · ")

    companion object {
        fun of(json: JSON) = McpServerCallRow(
            id = json.long("id"),
            time = json.string("time"),
            mcpServer = json.string("mcp_server"),
            monogram = json.string("monogram"),
            tint = json.string("tint"),
            tool = json.string("tool"),
            source = json.string("source"),
            latency = json.string("latency"),
            ok = json.bool("ok"),
            error = json.optionalString("error"),
        )
    }
}

class McpServerCallPage(val rows: List<McpServerCallRow>, val more: Boolean) {
    companion object {
        fun of(json: JSON) = McpServerCallPage(json.objects("rows").map(McpServerCallRow::of), json.bool("more"))
    }
}

/** A call in full: what the agent sent and what came back. */
data class McpServerCallView(val row: McpServerCallRow, val arguments: String, val result: String, val error: String?) {
    companion object {
        fun of(json: JSON) = McpServerCallView(
            row = McpServerCallRow.of(json.obj("row") ?: EmptyJSON),
            arguments = json.string("arguments"),
            result = json.string("result"),
            error = json.optionalString("error"),
        )
    }
}

/** A field of the form a tool is tried with. */
data class McpServerField(
    val name: String,
    val description: String?,
    val kind: Kind,
    val required: Boolean,
    val choices: List<String>,
    /** What it starts with. */
    val value: String,
) {
    enum class Kind(val raw: String) {
        Text("text"), Number("number"), Choice("choice"), Json("json"), Toggle("switch");

        companion object {
            fun of(raw: String) = entries.firstOrNull { it.raw == raw } ?: Text
        }
    }

    companion object {
        fun of(json: JSON) = McpServerField(
            name = json.string("name"),
            description = json.optionalString("description"),
            kind = Kind.of(json.string("kind")),
            required = json.bool("required"),
            choices = json.strings("choices"),
            value = json.string("value"),
        )
    }
}

/** What a tried tool answered: its text, the whole answer as JSON, and what went wrong. */
data class McpServerAnswer(val text: String, val json: String, val error: String?) {
    companion object {
        fun of(json: JSON) = McpServerAnswer(json.string("text"), json.string("json"), json.optionalString("error"))
    }
}

/** An MCP server's tile colour, by the chart token its row names. */
fun Palette.mcpServerTint(token: String): Color = named(token) ?: chart1

/** The gateway's MCP servers: what their page does to them, through the core. */
class McpServers(private val gateway: Gateway) {
    /** The MCP server signing in again, whose card shows it pending. */
    var signingIn by mutableStateOf<String?>(null)
        private set
    private var session: SignInSession? = null

    private val core: CoreBridge? get() = gateway.store?.core

    /** The MCP server of the gateway by its id, as the page knows it. */
    fun mcpServer(id: String): McpServerRow? = gateway.view.mcpServers?.firstOrNull { it.id == id }

    /** Signs an MCP server in again in the browser; its card shows it pending meanwhile. */
    fun signInAgain(mcpServer: McpServerRow) {
        if (signingIn != null) return
        signingIn = mcpServer.id
        gateway.send("mcp_server_sign_in", mapOf("mcp_server" to mcpServer.id)) { result ->
            val page = result.getOrNull()?.obj("sign_in")?.string("url")?.takeIf { it.isNotEmpty() }
            if (page == null) {
                signingIn = null
                gateway.store?.alert(result)
                return@send
            }
            val session = SignInSession()
            this.session = session
            session.start(Platform.context, page) { callback ->
                this.session = null
                if (callback == null) {
                    signingIn = null
                    return@start
                }
                gateway.send("mcp_server_finish_sign_in", mapOf("address" to callback)) { finished ->
                    signingIn = null
                    gateway.store?.alert(finished)
                }
            }
        }
    }

    fun setEnabled(mcpServer: McpServerRow, enabled: Boolean) = update(mcpServer.id, mapOf("type" to "enabled", "enabled" to enabled))

    fun setTool(mcpServer: McpServerRow, tool: McpServerTool, enabled: Boolean) =
        update(mcpServer.id, mapOf("type" to "tool", "name" to tool.name, "enabled" to enabled))

    /** Turns off every tool that writes, or every tool on. */
    fun setTools(mcpServer: McpServerRow, readOnly: Boolean) = update(mcpServer.id, mapOf("type" to if (readOnly) "read_only" else "all_tools"))

    fun rename(mcpServer: McpServerRow, name: String, slug: String) {
        val newName = name.trim()
        val newSlug = slug.trim()
        if (newName.isNotEmpty() && newName != mcpServer.name) update(mcpServer.id, mapOf("type" to "name", "name" to newName))
        if (newSlug.isNotEmpty() && newSlug != mcpServer.slug) update(mcpServer.id, mapOf("type" to "slug", "slug" to newSlug))
    }

    fun setKey(mcpServer: McpServerRow, header: String, key: String, reply: (Result<JSON>) -> Unit) =
        gateway.send("mcp_server_key", mapOf("mcp_server" to mcpServer.id, "header" to header, "key" to key), reply)

    fun refresh(mcpServer: McpServerRow) = command("mcp_server_refresh", mapOf("mcp_server" to mcpServer.id))

    fun remove(mcpServer: McpServerRow) = command("mcp_server_remove", mapOf("mcp_server" to mcpServer.id))

    fun loadCalls(before: Long?, errorsOnly: Boolean, reply: (Result<McpServerCallPage>) -> Unit) {
        val fields = buildMap<String, Any?> {
            put("errors_only", errorsOnly)
            before?.let { put("before", it) }
        }
        core?.send("mcp_server_calls", fields, read = McpServerCallPage::of, reply = reply)
    }

    /** A call in full; null when the gateway no longer has it. */
    fun loadCall(id: Long, reply: (Result<McpServerCallView?>) -> Unit) {
        core?.send("mcp_server_call", mapOf("call" to id), read = { Found(it.obj("call")?.let(McpServerCallView::of)) }) { result ->
            reply(result.map { it.call })
        }
    }

    fun loadFields(mcpServer: String, tool: String, reply: (Result<List<McpServerField>>) -> Unit) {
        core?.send("mcp_server_fields", mapOf("mcp_server" to mcpServer, "tool" to tool), read = { it.objects("fields").map(McpServerField::of) }, reply = reply)
    }

    fun tryTool(mcpServer: String, tool: String, values: Map<String, String>, reply: (Result<McpServerAnswer>) -> Unit) {
        core?.send("mcp_server_try", mapOf("mcp_server" to mcpServer, "tool" to tool, "values" to values), read = McpServerAnswer::of, reply = reply)
    }

    internal fun send(type: String, fields: Map<String, Any?>, reply: (Result<JSON>) -> Unit) = gateway.send(type, fields, reply)

    private fun update(id: String, change: Map<String, Any?>) = command("mcp_server_update", mapOf("mcp_server" to id, "change" to change))

    private fun command(type: String, fields: Map<String, Any?>) = gateway.send(type, fields) { gateway.store?.alert(it) }

    /** What `loadCall` read, which may be nothing. */
    private class Found(val call: McpServerCallView?)
}

/**
 * The calls agents made to the MCP servers, paged back from the newest: all of them or only those
 * that failed.
 */
class McpServerCalls(private val mcpServers: McpServers) {
    var rows by mutableStateOf(emptyList<McpServerCallRow>())
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
            more = false
            load(before = null)
        }
    /** Counts what was asked, so that only the last answer is shown. */
    private var asked = 0

    fun start() = load(before = null)

    fun loadMore() {
        val last = rows.lastOrNull() ?: return
        if (!more || loading) return
        load(before = last.id)
    }

    /** Puts the calls that came since the newest row on top, or starts over when more came than a page holds. */
    fun refresh() {
        if (loading) return
        val number = asked
        mcpServers.loadCalls(before = null, errorsOnly = errorsOnly) { result ->
            val page = result.getOrNull() ?: return@loadCalls
            if (number != asked) return@loadCalls
            val newest = rows.firstOrNull()?.id ?: 0
            val fresh = page.rows.filter { it.id > newest }
            if (fresh.size == page.rows.size && rows.isNotEmpty()) {
                rows = page.rows
                more = page.more
                return@loadCalls
            }
            rows = fresh + rows
        }
    }

    /** Said where there are no rows. */
    val emptyText: String?
        get() {
            if (rows.isNotEmpty() || loading) return null
            return error ?: if (errorsOnly) "No errors" else "No calls yet"
        }

    private fun load(before: Long?) {
        asked += 1
        val number = asked
        loading = true
        error = null
        mcpServers.loadCalls(before, errorsOnly) { result ->
            if (number != asked) return@loadCalls
            loading = false
            val page = result.getOrElse {
                error = result.coreError?.message
                return@loadCalls
            }
            rows = if (before == null) page.rows else rows + page.rows
            more = page.more
        }
    }
}

/**
 * Adding an MCP server, step by step: the gateway is asked to add it, and it is added at once, signs in
 * in the browser, or wants a key.
 */
class McpServerAdder(private val mcpServers: McpServers) {
    sealed interface Step {
        data object Pick : Step
        data class SigningIn(val name: String) : Step
        /** `name` is what the sheet calls it; `given` the known MCP server's name, sent again with the key. */
        data class Key(val name: String, val url: String, val given: String?) : Step
        data class Added(val id: String) : Step
    }

    var step by mutableStateOf<Step>(Step.Pick)
        private set
    /** What is being added: a known MCP server's url, or the address typed. */
    var adding by mutableStateOf<String?>(null)
        private set
    var error by mutableStateOf<String?>(null)
        private set
    private var session: SignInSession? = null

    fun add(url: String, name: String?, header: String = "", key: String = "") {
        if (adding != null) return
        adding = url
        error = null
        val fields = buildMap<String, Any?> {
            put("url", url)
            put("header", header)
            put("key", key)
            name?.let { put("name", it) }
        }
        mcpServers.send("mcp_server_add", fields) { result ->
            adding = null
            answered(result, name, url)
        }
    }

    /** Back to picking another. */
    fun back() {
        session = null
        adding = null
        error = null
        step = Step.Pick
    }

    private fun answered(result: Result<JSON>, given: String?, url: String) {
        val name = given ?: hostName(url)
        val answer = result.getOrElse {
            error = result.coreError?.message
            return
        }
        val added = answer.optionalString("added")
        val page = answer.obj("sign_in")?.string("url")?.takeIf { it.isNotEmpty() }
        when {
            added != null -> step = Step.Added(added)
            page != null -> signInInBrowser(page, name)
            answer.bool("needs_key") -> step = Step.Key(name, url, given)
        }
    }

    private fun signInInBrowser(page: String, name: String) {
        step = Step.SigningIn(name)
        val session = SignInSession()
        this.session = session
        session.start(Platform.context, page) { callback ->
            if (this.session !== session) return@start
            this.session = null
            if (callback == null) return@start back()
            mcpServers.send("mcp_server_finish_sign_in", mapOf("address" to callback)) { result ->
                val answer = result.getOrElse {
                    step = Step.Pick
                    error = result.coreError?.message
                    return@send
                }
                step = answer.optionalString("added")?.let { Step.Added(it) } ?: Step.Pick
            }
        }
    }

    companion object {
        private val skipped = setOf("mcp", "api", "www", "server", "app")

        /** "Linear" for `https://mcp.linear.app/mcp`, until the gateway names it; an address by itself. */
        fun hostName(url: String): String {
            val host = url.toUri().host ?: url
            if (host.none { it.isLetter() } || host == "localhost") return host
            val labels = host.split('.')
            val named = labels.dropLast(1).firstOrNull { it !in skipped } ?: labels.firstOrNull() ?: host
            return named.replaceFirstChar { it.uppercase() }
        }
    }
}
