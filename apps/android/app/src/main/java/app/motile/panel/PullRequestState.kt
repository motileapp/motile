package app.motile.panel

import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.motile.core.EmptyJSON
import app.motile.core.Loaded
import app.motile.core.Preferences
import app.motile.core.coreError
import app.motile.core.obj
import app.motile.core.objects
import app.motile.core.optionalString
import app.motile.core.shownPullRequestLink
import app.motile.core.string
import java.util.UUID

/** The action on a pull request that runs: `key` names the button that started it, which is pending meanwhile. */
data class PullRequestWork(val key: String)

/** A comment on a line, kept for the next review. */
data class PendingLineComment(
    val path: String,
    val line: Int,
    /** "left" or "right". */
    val side: String,
    val body: String,
    val id: String = UUID.randomUUID().toString(),
)

/** What an action on a pull request did, or why it couldn't. Each is a notice of its own, however alike. */
class PullRequestNotice(
    val text: String,
    val failed: Boolean,
    /** The pull request the action opened. */
    val url: String? = null,
)

/** What the pull request tabs show, read from the open thread's server, and the actions that run on it. All of it is used on the main thread. */
class PullRequestState(val panel: RightPanel) {
    /** The pull request the tab shows, as last read. */
    var page by mutableStateOf<Loaded<PullRequestPage>>(Loaded.Loading)
        private set
    /** The actions that run, by the pull request they run on. */
    private var work by mutableStateOf(mapOf<String, PullRequestWork>())
    /** The repository's pull requests, as the list tab shows them. */
    var list by mutableStateOf<Loaded<List<PullRequestRow>>>(Loaded.Loading)
        private set
    /** Comments on lines waiting to be sent with a review, by pull request. */
    var pendingComments by mutableStateOf(mapOf<Int, List<PendingLineComment>>())
        private set
    /** What the last action on the pull request did, or why it couldn't. */
    var notice by mutableStateOf<PullRequestNotice?>(null)
        private set
    /** Goes up each time the pull request has been read. */
    var reads by mutableIntStateOf(0)
        private set
    /** The tabs whose content is being read, whose button to read it again turns meanwhile. */
    var reading by mutableStateOf(setOf<PanelTab>())
        private set

    /** The folder all of the above is of. */
    private var shown: PanelTarget? = null
    private var shownNumber: Int? = null
    /** How each project's pull requests were last merged, by project. */
    private var mergeMethods = Preferences.strings("pullRequest.methods")
    private val main = Handler(Looper.getMainLooper())

    private val store get() = panel.store

    /** Shows what was read, and tells the diff and the shortcuts which pull request that is. */
    private fun show(page: Loaded<PullRequestPage>) {
        this.page = page
        val value = page.value
        panel.shownPullRequestNumber = value?.number
        shownPullRequestLink = value?.let { it.url to it.number }
    }

    /** The action that runs on the pull request shown. */
    val working: PullRequestWork?
        get() {
            val shown = shown ?: return null
            val number = shownNumber ?: return null
            return work[workKey(shown, number)]
        }

    private fun workKey(target: PanelTarget, number: Int) = "${target.serverID}/${target.projectID}/$number"

    private fun command(target: PanelTarget, number: Int): MutableMap<String, Any?> = target.request.toMutableMap().apply {
        put("server_id", target.serverID)
        put("number", number)
    }

    /** Asks the server for the pull request. What is shown of it stays until the answer is there, unless it is another one. */
    fun loadPullRequest(target: PanelTarget, number: Int) {
        val store = store ?: return
        look(target)
        if (shownNumber != number) {
            shownNumber = number
            show(Loaded.Loading)
            notice = null
        }
        val command = command(target, number)
        mergeMethods[target.projectID]?.let { command["method"] = it }
        reading = reading + PanelTab.PullRequest
        store.core.send("pull_request", command, read = ::PullRequestPage) { result ->
            if (shown != target || shownNumber != number) return@send
            reading = reading - PanelTab.PullRequest
            val read = result.getOrNull()
            val error = result.coreError
            when {
                read != null -> show(Loaded.Ready(read))
                // A pull request that was read stays; only its notice says what went wrong.
                page.value != null -> notice = PullRequestNotice(error?.message ?: "", failed = true)
                else -> show(Loaded.Failed(error?.message ?: "Something went wrong."))
            }
            reads += 1
        }
    }

    /**
     * Does something to the pull request: an action of a button or a choice, with what the comment
     * box says. `key` names the button it came from, which is pending meanwhile. `done` is called
     * when it worked.
     */
    fun act(action: String, method: String? = null, text: String? = null, key: String, target: PanelTarget, number: Int, done: (() -> Unit)? = null) {
        val store = store ?: return
        val workKey = workKey(target, number)
        if (work[workKey] != null) return
        work = work + (workKey to PullRequestWork(key))
        notice = null
        if ((action == "merge" || action == "enable_auto_merge") && method != null) remember(method, target.projectID)
        val command = command(target, number)
        command["action"] = action
        if (method != null) command["method"] = method
        if (!text.isNullOrEmpty()) command["text"] = text
        val read = { answer: app.motile.core.JSON ->
            Triple(answer.string("title"), answer.optionalString("url"), PullRequestPage(answer.obj("view") ?: EmptyJSON))
        }
        store.core.send("pull_request_action", command, read = read) { result ->
            work = work - workKey
            if (shown != target || shownNumber != number) return@send
            val answer = result.getOrNull()
            if (answer == null) {
                say(PullRequestNotice(result.coreError?.message ?: "", failed = true))
                return@send
            }
            val (title, url, page) = answer
            show(Loaded.Ready(page))
            reads += 1
            if (url != null) say(PullRequestNotice(title, failed = false, url = url))
            done?.invoke()
        }
    }

    /** Changes the pull request as `edit` says, the protocol's `PullRequestEdit` as JSON. A `key` names the control that waits for it; without one it goes like a reaction. */
    fun edit(edit: Map<String, Any?>, key: String? = null, target: PanelTarget, number: Int, done: (() -> Unit)? = null) {
        val store = store ?: return
        val workKey = workKey(target, number)
        if (key != null) {
            if (work[workKey] != null) return
            work = work + (workKey to PullRequestWork(key))
            notice = null
        }
        val command = command(target, number)
        command["edit"] = edit
        mergeMethods[target.projectID]?.let { command["method"] = it }
        store.core.send("pull_request_edit", command, read = { PullRequestPage(it.obj("view") ?: EmptyJSON) }) { result ->
            if (key != null) work = work - workKey
            if (shown != target || shownNumber != number) return@send
            val page = result.getOrNull()
            if (page == null) {
                say(PullRequestNotice(result.coreError?.message ?: "", failed = true))
                return@send
            }
            show(Loaded.Ready(page))
            reads += 1
            done?.invoke()
        }
    }

    /** Keeps a comment on a line for the next review of the pull request. */
    fun addPending(comment: PendingLineComment, number: Int) {
        pendingComments = pendingComments + (number to (pendingComments[number] ?: emptyList()) + comment)
    }

    fun removePending(comment: PendingLineComment, number: Int) {
        val left = (pendingComments[number] ?: return).filter { it.id != comment.id }
        pendingComments = pendingComments + (number to left)
    }

    /** Sends a review with the comments kept for it, and forgets them once it is there. */
    fun review(verdict: String, body: String, key: String, target: PanelTarget, number: Int, done: (() -> Unit)? = null) {
        val comments = (pendingComments[number] ?: emptyList()).map { mapOf("path" to it.path, "line" to it.line, "side" to it.side, "body" to it.body) }
        val review = mapOf("kind" to "review", "verdict" to verdict, "body" to body, "comments" to comments)
        edit(review, key, target, number) {
            pendingComments = pendingComments - number
            done?.invoke()
        }
    }

    /** Asks the server for the repository's pull requests in that state: "open", "closed", "merged" or "all". */
    fun loadPullRequests(target: PanelTarget, state: String) {
        val store = store ?: return
        look(target)
        val command = target.request.toMutableMap().apply {
            put("server_id", target.serverID)
            put("state", state)
        }
        reading = reading + PanelTab.PullRequests
        store.core.send("pull_requests", command, read = { answer -> answer.objects("rows").map(::PullRequestRow) }) { result ->
            if (shown != target) return@send
            reading = reading - PanelTab.PullRequests
            val rows = result.getOrNull()
            val error = result.coreError?.message ?: ""
            when {
                rows != null -> list = Loaded.Ready(rows)
                list.value == null -> list = Loaded.Failed(error)
                else -> say(PullRequestNotice(error, failed = true))
            }
        }
    }

    /** Opens the pull request in a tab: the thread's own tab when it is the thread's. */
    fun showPullRequest(number: Int, target: PanelTarget) {
        panel.open(if (number == target.pullRequest) PanelTab.PullRequest else PanelTab.PullRequestNumber(number))
    }

    /** Makes the pull request the thread's own, or with `null` takes its own away. */
    fun link(number: Int?, threadID: String, serverID: String) {
        val store = store ?: return
        val request = buildMap {
            put("type", "link_pull_request")
            put("thread_id", threadID)
            if (number != null) put("number", number)
        }
        store.core.send("request", mapOf("server_id" to serverID, "request" to request)) { result ->
            val error = result.coreError
            if (error != null) return@send say(PullRequestNotice(error.message, failed = true))
            val said = number?.let { "Linked PR #$it to this thread" } ?: "Unlinked the pull request from this thread"
            say(PullRequestNotice(said, failed = false))
        }
    }

    /** Has the thread's agent told what happens on its pull request, or stops it. */
    fun watch(on: Boolean, threadID: String, serverID: String) {
        val store = store ?: return
        val request = mapOf("type" to "watch_pull_request", "thread_id" to threadID, "watch" to on)
        store.core.send("request", mapOf("server_id" to serverID, "request" to request)) { result ->
            val error = result.coreError
            if (error != null) return@send say(PullRequestNotice(error.message, failed = true))
            val said = if (on) "Watching: the agent hears when its checks finish, someone comments or it conflicts" else "No longer watching"
            say(PullRequestNotice(said, failed = false))
        }
    }

    /** How the project's pull requests merge from now on. */
    fun chooseMethod(method: String, target: PanelTarget, number: Int) {
        remember(method, target.projectID)
        loadPullRequest(target, number)
    }

    private fun remember(method: String, projectID: String) {
        mergeMethods = mergeMethods + (projectID to method)
        Preferences.set("pullRequest.methods", mergeMethods)
    }

    /** What worked goes away by itself; what failed stays until it is closed. */
    private fun say(notice: PullRequestNotice) {
        this.notice = notice
        if (notice.failed) return
        main.postDelayed({ if (this.notice === notice) this.notice = null }, 8_000)
    }

    fun dismissNotice() {
        notice = null
    }

    /** Forgets what was shown when the folder is another one than before. */
    private fun look(target: PanelTarget) {
        if (shown == target) return
        val sameFolder = shown?.key == target.key
        shown = target
        // What is read for another target never answers here.
        reading = emptySet()
        if (sameFolder) return
        shownNumber = null
        show(Loaded.Loading)
        notice = null
        list = Loaded.Loading
    }
}
