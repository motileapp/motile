package app.motile.core

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.motile.panel.TurnChange
import app.motile.transcript.RowModel

/**
 * The rows of the open thread, kept in step with the core, as the transcript draws them. Rows
 * arrive as splices; the list only changes where they say.
 */
class TranscriptModel {
    var threadID: String? = null
        private set

    /** What the transcript draws. While `keepsRows` it still shows the rows of the thread open before. */
    val rows = mutableStateListOf<RowModel>()
    /** The message on its way to the server, drawn after the rows. */
    var pending by mutableStateOf<RowModel?>(null)
        private set
    var activity by mutableStateOf(Activity())
        private set
    /** The thread has turns before the first row. */
    var earlier by mutableStateOf(false)
        private set
    /** The thread has caught up with its server. */
    var live by mutableStateOf(false)
        private set
    /** Counts up when the rows are replaced as a whole, which puts the transcript at its end. */
    var resets by mutableIntStateOf(0)
        private set
    /** Counts up whenever the rows change. */
    var version by mutableIntStateOf(0)
        private set
    /** The row that ended the turn the server still reports as running. */
    private var endOfRunningTurn by mutableStateOf<String?>(null)

    /** The view still shows the rows of the thread that was open before. */
    private var keepsRows = false
    /** The text of the message on its way. */
    private var pendingText: String? = null

    /**
     * Whether the line that says the agent is at work shows under the rows. The row that ends a
     * turn replaces it right away, a moment before the server says that the agent stopped, unless
     * agents it started work on.
     */
    val showsWorking: Boolean
        get() {
            if (!activity.running) return false
            val last = pending ?: rows.lastOrNull()
            return activity.agents > 0 || endOfRunningTurn == null || last?.id != endOfRunningTurn
        }

    val isEmpty: Boolean get() = (keepsRows || rows.isEmpty()) && pending == null

    /** The turns that changed files, the first one first. */
    val turns: List<TurnChange>
        get() = if (keepsRows) emptyList() else rows.mapNotNull { row ->
            val changes = row.kind as? RowModel.Kind.Changes ?: return@mapNotNull null
            TurnChange(row.itemID, changes.content.at, changes.content.files)
        }

    /** Starts showing another thread, or none. Its rows follow from the core. With `keepingRows` the view shows the rows it has until they do, so that it isn't empty in between. */
    fun begin(threadID: String?, live: Boolean = false, keepingRows: Boolean = false) {
        this.threadID = threadID
        pending = null
        pendingText = null
        activity = Activity()
        earlier = false
        this.live = live
        keepsRows = keepingRows
        if (!keepingRows) clear()
    }

    private fun clear() {
        rows.clear()
        endOfRunningTurn = null
        resets += 1
        version += 1
    }

    /** The thread's rows aren't coming: the view lets go of the ones it kept. */
    fun dropKeptRows() {
        if (!keepsRows) return
        keepsRows = false
        clear()
    }

    /** The message on screen now belongs to a thread; its rows are about to arrive. */
    fun adopt(threadID: String) {
        this.threadID = threadID
    }

    fun apply(reset: Boolean, start: Int, remove: Int, new: List<RowModel>, earlier: Boolean) {
        this.earlier = earlier
        // The server has the message now, so the copy shown while it travelled goes.
        val sent = pending != null && new.any { it.isSentMessage }
        if (reset) {
            if (sent) pending = null
            keepsRows = false
            rows.clear()
            rows.addAll(new)
            endOfRunningTurn = null
            resets += 1
            version += 1
            return
        }
        if (keepsRows || start < 0 || remove < 0 || start + remove > rows.size) return
        if (sent) pending = null
        if (remove > 0) rows.removeRange(start, start + remove)
        rows.addAll(start, new)
        val last = rows.lastOrNull()
        if (activity.running && last?.kind is RowModel.Kind.TurnEnd && new.any { it.id == last.id }) endOfRunningTurn = last.id
        version += 1
    }

    fun setPending(text: String?, attachments: List<AttachedFile> = emptyList(), queued: Boolean = false, skills: List<SkillSpan> = emptyList()) {
        pendingText = text
        pending = text?.let { RowModel.pending(it, attachments, queued, skills) }
    }

    /** Shows the skills of the message on its way as chips, while it is still the one shown. */
    fun setPendingSkills(skills: List<SkillSpan>, text: String, attachments: List<AttachedFile>, queued: Boolean) {
        if (skills.isEmpty() || pending == null || pendingText != text) return
        setPending(text, attachments, queued, skills)
    }

    fun changeActivity(activity: Activity) {
        if (!activity.running || activity.startedAt != this.activity.startedAt) endOfRunningTurn = null
        if (this.activity != activity) this.activity = activity
    }

    fun changeLive(live: Boolean) {
        this.live = live
    }

    fun apply(spans: IntArray, rowID: String) {
        if (keepsRows) return
        rows.lastOrNull { it.id == rowID }?.applySpans(spans)
    }

    /** Changes the tree of a program row. */
    fun patchUi(rowID: String, ops: List<JSON>) {
        if (keepsRows) return
        val program = rows.lastOrNull { it.id == rowID }?.kind as? RowModel.Kind.Ui ?: return
        program.content.patch(ops)
    }
}
