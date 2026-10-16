package app.motile.panel

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.motile.core.Activity
import app.motile.core.AppStore
import app.motile.core.JSON
import app.motile.core.Preferences
import app.motile.core.Time
import app.motile.core.bool
import app.motile.core.integration
import app.motile.core.int
import app.motile.core.json
import app.motile.core.jsonOf
import app.motile.core.obj
import app.motile.core.objects
import app.motile.core.optionalBool
import app.motile.core.optionalInt
import app.motile.core.parseJSON
import app.motile.core.string
import app.motile.platform.Platform
import app.motile.platform.Symbol
import kotlinx.serialization.json.JsonObject

/** Which changes a diff shows. */
sealed interface DiffScope {
    /** What isn't committed. */
    data object Uncommitted : DiffScope
    /** Everything since the branch left the one it started from. */
    data object Branch : DiffScope
    /** What the turn that ended with the item changed. */
    data class Turn(val itemID: String) : DiffScope
    /** What the pull request with the number changes, as GitHub has it. */
    data class PullRequest(val number: Int) : DiffScope
    /** What one commit changed, by its whole name. */
    data class Commit(val sha: String) : DiffScope

    val request: Map<String, Any?>
        get() = when (this) {
            Uncommitted -> mapOf("kind" to "uncommitted")
            Branch -> mapOf("kind" to "branch")
            is Turn -> mapOf("kind" to "turn", "item_id" to itemID)
            is PullRequest -> mapOf("kind" to "pull_request", "number" to number)
            is Commit -> mapOf("kind" to "commit", "sha" to sha)
        }

    companion object {
        fun of(json: JSON?): DiffScope? = when (json?.string("kind")) {
            "uncommitted" -> Uncommitted
            "branch" -> Branch
            "turn" -> Turn(json.string("item_id"))
            "pull_request" -> PullRequest(json.int("number"))
            "commit" -> Commit(json.string("sha"))
            else -> null
        }
    }
}

/** A tab of the panel beside the thread. */
sealed interface PanelTab {
    data object Diff : PanelTab
    /** The folder's files, to open one. */
    data object Files : PanelTab
    /** One file, by its path in the folder. */
    data class File(override val path: String) : PanelTab
    /** What one turn changed in one file, by the item that ended the turn. */
    data class Change(val turn: String, override val path: String) : PanelTab
    /** The agents the thread's agent has started, and what one of them did. */
    data object Agents : PanelTab
    /** The pull request of the branch the thread works on. */
    data object PullRequest : PanelTab
    /** Another pull request of the repository, by its number. */
    data class PullRequestNumber(val number: Int) : PanelTab
    /** The repository's pull requests. */
    data object PullRequests : PanelTab
    /** The issues of the Linear workspace the server is connected to, or how to connect it. */
    data object Linear : PanelTab
    /** One issue of a workspace, by its id there, titled with its identifier. */
    data class LinearIssue(val workspace: String, val issueID: String, val identifier: String) : PanelTab
    /** The issues of the Ricrac workspace the server is connected to, or how to connect it. */
    data object Ricrac : PanelTab
    /** One issue of a Ricrac workspace, by its id there, titled with its identifier. */
    data class RicracIssue(val workspace: String, val issueID: String, val identifier: String) : PanelTab
    /** A tab that offers what there is to open. A thread can have several, told apart by number. */
    data class Blank(val number: Int) : PanelTab

    val id: String
        get() = when (this) {
            is Blank -> "blank:$number"
            Diff -> "diff"
            Files -> "files"
            Agents -> "agents"
            PullRequest -> "pull_request"
            is PullRequestNumber -> "pull_request:$number"
            PullRequests -> "pull_requests"
            Linear -> "linear"
            is LinearIssue -> "linear:$issueID"
            Ricrac -> "ricrac"
            is RicracIssue -> "ricrac:$issueID"
            is File -> "file:$path"
            is Change -> "change:$path"
        }

    val blankNumber: Int? get() = (this as? Blank)?.number

    /** The file a tab is about. */
    val path: String? get() = null

    val title: String
        get() = when (this) {
            Diff -> "Diff"
            Files -> "Files"
            Agents -> "Agents"
            PullRequest -> "Pull Request"
            is PullRequestNumber -> "PR #$number"
            PullRequests -> "Pull Requests"
            Linear -> "Linear"
            is LinearIssue -> identifier
            Ricrac -> "Ricrac"
            is RicracIssue -> identifier
            is Blank -> "New Tab"
            is File -> path.substringAfterLast('/')
            is Change -> path.substringAfterLast('/')
        }

    val symbol: Symbol
        get() = when (this) {
            Diff, is Change -> Symbol.Diff
            Files -> Symbol.Folder
            Agents -> Symbol.Users
            PullRequest, is PullRequestNumber, PullRequests -> Symbol.GitPullRequest
            Linear, is LinearIssue -> Symbol.Linear
            Ricrac, is RicracIssue -> Symbol.Ricrac
            is Blank -> Symbol.Plus
            is File -> FileSymbol.symbol(path)
        }

    val json: Map<String, Any?>
        get() = when (this) {
            Diff -> mapOf("tab" to "diff")
            Files -> mapOf("tab" to "files")
            Agents -> mapOf("tab" to "agents")
            PullRequest -> mapOf("tab" to "pull_request")
            PullRequests -> mapOf("tab" to "pull_requests")
            Linear -> mapOf("tab" to "linear")
            Ricrac -> mapOf("tab" to "ricrac")
            is File -> mapOf("tab" to "file", "path" to path)
            is Change -> mapOf("tab" to "change", "turn" to turn, "path" to path)
            is PullRequestNumber -> mapOf("tab" to "pull_request_number", "number" to number)
            is LinearIssue -> mapOf("tab" to "linear_issue", "workspace" to workspace, "id" to issueID, "identifier" to identifier)
            is RicracIssue -> mapOf("tab" to "ricrac_issue", "workspace" to workspace, "id" to issueID, "identifier" to identifier)
            is Blank -> mapOf("tab" to "blank", "number" to number)
        }

    companion object {
        fun of(json: JSON?): PanelTab? = when (json?.string("tab")) {
            "diff" -> Diff
            "files" -> Files
            "agents" -> Agents
            "pull_request" -> PullRequest
            "pull_requests" -> PullRequests
            "linear" -> Linear
            "ricrac" -> Ricrac
            "file" -> File(json.string("path"))
            "change" -> Change(json.string("turn"), json.string("path"))
            "pull_request_number" -> PullRequestNumber(json.int("number"))
            "linear_issue" -> LinearIssue(json.string("workspace"), json.string("id"), json.string("identifier"))
            "ricrac_issue" -> RicracIssue(json.string("workspace"), json.string("id"), json.string("identifier"))
            "blank" -> Blank(json.int("number"))
            else -> null
        }
    }
}

/** The tabs a thread has open in the panel. */
data class PanelTabs(
    val tabs: List<PanelTab> = emptyList(),
    val active: PanelTab? = null,
    /** What the diff tab shows, once that was chosen. */
    val scope: DiffScope? = null,
    /** The panel covers the thread. */
    val maximized: Boolean? = null,
) {
    /** All there is is a blank tab, as before one was opened. */
    val isBlank: Boolean get() = tabs.size == 1 && tabs[0].blankNumber != null

    val json: Map<String, Any?>
        get() = mapOf("tabs" to tabs.map { it.json }, "active" to active?.json, "scope" to scope?.request, "maximized" to maximized)

    companion object {
        fun of(json: JSON) = PanelTabs(
            tabs = json.objects("tabs").mapNotNull(PanelTab::of),
            active = PanelTab.of(json.obj("active")),
            scope = DiffScope.of(json.obj("scope")),
            maximized = json.optionalBool("maximized"),
        )
    }
}

/** The folder the panel looks into: the one the open thread works in, or the project's when a thread is about to start there. */
data class PanelTarget(
    /** What its tabs are kept under: the thread, or the draft. */
    val key: String,
    val serverID: String,
    val projectID: String,
    val threadID: String?,
    val name: String,
    val repository: Boolean,
    /** The thread works in a worktree of its own. */
    val worktree: Boolean,
    /** The thread will start in a worktree that isn't made yet, so the folder is still the project's. */
    val awaitsWorktree: Boolean,
    /** The number of the pull request of the branch it works on, when there is one. */
    val pullRequest: Int?,
) {
    /** Names the folder in a request to its server. */
    val request: Map<String, Any?>
        get() = buildMap {
            put("project_id", projectID)
            if (threadID != null) put("thread_id", threadID)
        }
}

/** A turn of the open thread that changed files. */
data class TurnChange(
    /** The item that ended the turn. */
    val id: String,
    val at: Double,
    val files: Int,
)

/** The symbol a file is shown with, by what its name ends in. */
object FileSymbol {
    fun symbol(path: String): Symbol = when (path.substringAfterLast('/').let { if ('.' in it) it.substringAfterLast('.') else "" }.lowercase()) {
        "png", "jpg", "jpeg", "gif", "webp", "heic", "bmp", "tiff", "ico", "svg" -> Symbol.Image
        "mp4", "mov", "m4v", "webm" -> Symbol.CirclePlay
        "md", "markdown", "txt", "rst" -> Symbol.FileText
        "json", "yaml", "yml", "toml", "xml", "plist", "lock" -> Symbol.Braces
        "sh", "bash", "zsh", "fish" -> Symbol.Terminal
        "" -> Symbol.File
        else -> Symbol.Code
    }
}

/**
 * The panel beside the thread: whether it is open, the tabs each thread has in it, and what the
 * tabs of the open thread show. All of it is used on the main thread.
 */
class RightPanel {
    var store: AppStore? = null

    /** What the changes, files and file tabs show. */
    val content = PanelContent(this)

    /** What the pull request tabs show. */
    val pullRequests = PullRequestState(this)

    private var openState by mutableStateOf(Preferences.bool("panel.open"))
    var isOpen: Boolean
        get() = openState
        set(value) {
            val old = openState
            openState = value
            Preferences.set("panel.open", value)
            if (!value) change { it.copy(maximized = null) }
            if (value && !old) showPullRequestIfBlank()
        }

    var tabsByKey by mutableStateOf(loadTabs())
        private set
    /** The turns of the open thread that changed files, the first one first. */
    var turns by mutableStateOf(emptyList<TurnChange>())
    /** The agent whose transcript the agents tab shows. Without one it lists them. */
    var shownAgent by mutableStateOf<String?>(null)
        private set

    private fun loadTabs(): Map<String, PanelTabs> {
        val saved = Preferences.string("panel.tabs")?.let(::parseJSON) ?: return emptyMap()
        return saved.mapNotNull { (key, value) -> (value as? JsonObject)?.let { key to PanelTabs.of(it) } }.toMap()
    }

    // Tabs

    private val key: String? get() = store?.panelTarget?.key

    /** The thread's tabs. There is always one: a blank one when none was opened. */
    val tabs: PanelTabs
        get() {
            var tabs = key?.let { tabsByKey[it] } ?: PanelTabs()
            tabs = tabs.copy(tabs = tabs.tabs.filter { tab -> tab.integration?.let { store?.isOn(it) != false } ?: true })
            if (tabs.active != null && tabs.active !in tabs.tabs) tabs = tabs.copy(active = null)
            if (tabs.tabs.isEmpty()) tabs = tabs.copy(tabs = listOf(PanelTab.Blank(0)))
            if (tabs.active == null) tabs = tabs.copy(active = tabs.tabs.first())
            return tabs
        }

    private fun change(change: (PanelTabs) -> PanelTabs) {
        val key = key ?: return
        val tabs = change(this.tabs)
        val untouched = tabs.isBlank && tabs.scope == null && tabs.maximized == null
        tabsByKey = if (untouched) tabsByKey - key else tabsByKey + (key to tabs)
        Preferences.set("panel.tabs", jsonOf(tabsByKey.mapValues { it.value.json }).toString())
    }

    /** Shows the tab, opening it and the panel when they aren't. It takes the place of the blank tab it is opened from. */
    fun open(tab: PanelTab) {
        change { tabs ->
            val list = tabs.tabs.toMutableList()
            val blank = list.indexOfFirst { it == tabs.active && it.blankNumber != null }.takeIf { it >= 0 }
            when {
                tab in list -> if (blank != null) list.removeAt(blank)
                blank != null -> list[blank] = tab
                else -> list.add(tab)
            }
            tabs.copy(tabs = list, active = tab)
        }
        isOpen = true
    }

    /** A thread with a pull request is most likely looked into for it, so a panel that opens on its blank tab shows it. */
    fun showPullRequestIfBlank() {
        val store = store ?: return
        if (!tabs.isBlank || store.panelTarget?.pullRequest == null || store.pullRequestsUnavailable != null) return
        open(PanelTab.PullRequest)
    }

    /** Adds a blank tab. A hidden panel that only has its blank tab just opens. */
    fun openBlank() {
        if (!isOpen && tabs.isBlank) {
            isOpen = true
            return
        }
        change { tabs ->
            val tab = PanelTab.Blank((tabs.tabs.mapNotNull { it.blankNumber }.maxOrNull() ?: -1) + 1)
            tabs.copy(tabs = tabs.tabs + tab, active = tab)
        }
        isOpen = true
    }

    fun activate(tab: PanelTab) = change { it.copy(active = tab) }

    /** Shows the tab `offset` places from the active one, wrapping around the ends. */
    fun activate(offset: Int) {
        val tabs = tabs
        val active = tabs.active ?: return
        val index = tabs.tabs.indexOf(active)
        if (!isOpen || tabs.tabs.size <= 1 || index < 0) return
        val count = tabs.tabs.size
        activate(tabs.tabs[((index + offset) % count + count) % count])
    }

    /** Closes the tab. The one beside it is shown in its place. */
    fun close(tab: PanelTab) {
        change { tabs ->
            val index = tabs.tabs.indexOf(tab)
            if (index < 0) return@change tabs
            val list = tabs.tabs.toMutableList().apply { removeAt(index) }
            val active = if (tabs.active != tab) tabs.active else list.getOrNull(minOf(index, list.size - 1))
            tabs.copy(tabs = list, active = active)
        }
        content.forget(tab)
        onClosed?.invoke(tab)
    }

    /** What else a closed tab lets go of, as the tabs' contents. */
    var onClosed: ((PanelTab) -> Unit)? = null

    fun closeOthers(tab: PanelTab) = change { it.copy(tabs = listOf(tab), active = tab) }

    fun closeAll() = change { it.copy(tabs = emptyList(), active = null) }

    /** What closing the active tab does while the panel is open: closes its tab, or hides it when all it has is a blank one. */
    fun closeActive(): Boolean {
        if (!isOpen) return false
        val tabs = tabs
        val active = tabs.active
        if (tabs.isBlank || active == null) {
            isOpen = false
            return true
        }
        close(active)
        return true
    }

    /** The panel covers the thread, so the window shows the sidebar and the panel. */
    val isMaximized: Boolean get() = isOpen && tabs.maximized == true

    /** A panel that has no folder to show stays beside the thread. */
    val canMaximize: Boolean get() = isOpen && key != null

    fun toggleMaximized() {
        if (!canMaximize) return
        val maximized = !isMaximized
        change { it.copy(maximized = if (maximized) true else null) }
        // The composer is behind the panel now, and must not take what is typed.
        if (maximized) Platform.endEditing()
    }

    /** The tabs a draft had go to the thread it became. */
    fun move(from: String, to: String) {
        val tabs = tabsByKey[from] ?: return
        tabsByKey = tabsByKey - from + (to to tabs)
        content.moved(from)
        onMoved?.invoke(from)
    }

    /** What else follows a draft's tabs to its thread. */
    var onMoved: ((String) -> Unit)? = null

    fun forget(key: String) {
        tabsByKey = tabsByKey - key
    }

    // Agents

    /** Opens the agents tab on what the agent did that the tool call started. */
    fun showAgent(id: String) {
        val store = store ?: return
        val threadID = store.transcript.threadID ?: return
        shownAgent = id
        store.agentTranscript.begin(threadID, live = store.transcript.live)
        agentsChanged()
        store.core.send("open_agent", json("thread_id" to threadID, "agent_id" to id))
        open(PanelTab.Agents)
    }

    /** Goes back to the list of agents. */
    fun showAgents() {
        val store = store ?: return
        if (shownAgent == null) return
        shownAgent = null
        store.agentTranscript.threadID?.let { store.core.send("close_agent", json("thread_id" to it)) }
        store.agentTranscript.begin(null)
    }

    /** The shown agent's transcript says that it works for as long as it does. */
    fun agentsChanged() {
        val store = store ?: return
        val agent = store.agents.firstOrNull { it.id == shownAgent } ?: return
        store.agentTranscript.changeActivity(Activity(running = agent.working, startedAt = agent.startedAt))
    }

    // Diff

    /** What the diff tab shows: what was chosen, or the turn's work as far as it is known. */
    fun scope(target: PanelTarget): DiffScope {
        tabs.scope?.let { if (canShow(it, target)) return it }
        return if (target.worktree) DiffScope.Branch else DiffScope.Uncommitted
    }

    /** A turn that is no longer known, or a pull request the folder no longer has, isn't shown. */
    private fun canShow(scope: DiffScope, target: PanelTarget): Boolean = when (scope) {
        is DiffScope.Turn -> turns.any { it.id == scope.itemID }
        is DiffScope.PullRequest -> scope.number == target.pullRequest || scope.number == shownPullRequestNumber
        else -> true
    }

    /** The number of the pull request the pull request tab last read. */
    var shownPullRequestNumber: Int? = null

    fun choose(scope: DiffScope) = change { it.copy(scope = scope) }

    /** The file of the diff to bring into view, and a count that goes up with every request. */
    var reveal by mutableStateOf<Pair<String, Int>?>(null)
        private set

    /** Opens the diff tab on `scope`, with the file at `path` in view. */
    fun showDiff(scope: DiffScope? = null, revealing: String? = null) {
        scope?.let(::choose)
        revealing?.let { reveal = it to ((reveal?.second ?: 0) + 1) }
        open(PanelTab.Diff)
    }

    /** Opens what the turn changed in the file in a tab of its own. A file has one such tab, which shows the turn that was asked for last. */
    fun showChange(turn: String, path: String) {
        val tab = PanelTab.Change(turn, path)
        change { tabs ->
            val index = tabs.tabs.indexOfFirst { it.id == tab.id }
            if (index < 0 || tabs.tabs[index] == tab) return@change tabs
            content.forget(tabs.tabs[index])
            onClosed?.invoke(tabs.tabs[index])
            tabs.copy(tabs = tabs.tabs.toMutableList().apply { this[index] = tab })
        }
        open(tab)
    }

    /** What the diff's menu and a change's tab call the turn. */
    fun nameOfTurn(id: String): String {
        val turn = turns.firstOrNull { it.id == id } ?: return "Earlier turn"
        return if (turn.id == turns.lastOrNull()?.id) "Latest turn" else "Turn at ${Time.stamp(turn.at)}"
    }

    /** The highlighting of what a request answered with has arrived. */
    var onColour: ((request: Long, file: Int, lines: List<IntArray>) -> Unit)? = null

    fun colour(request: Long, file: Int, lines: List<IntArray>) {
        content.colour(request, file, lines)
        onColour?.invoke(request, file, lines)
    }
}
