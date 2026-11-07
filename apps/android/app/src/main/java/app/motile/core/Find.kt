package app.motile.core

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** The matches finding in the thread found in one row, `[start, end)` in UTF-16 units of the row's text, and which of them is the current one. */
data class FoundMarks(val ranges: List<IntRange>, val current: Int?) {
    companion object {
        fun of(json: JSON) = FoundMarks(
            json.intLists("ranges").filter { it.size == 2 && it[1] > 0 }.map { it[0] until it[0] + it[1] },
            json.optionalInt("current"),
        )
    }
}

/** A match within a piece of a row's text, from the piece's start, and whether it is the current one. */
data class FoundSpan(val start: Int, val end: Int, val current: Boolean)

/** The matches that fall in the piece of the row's text from `offset`, `length` long, from the piece's start. */
fun FoundMarks?.within(offset: Int, length: Int): List<FoundSpan> {
    this ?: return emptyList()
    return ranges.mapIndexedNotNull { index, range ->
        val start = maxOf(range.first, offset)
        val end = minOf(range.last + 1, offset + length)
        if (end <= start) null else FoundSpan(start - offset, end - offset, index == current)
    }
}

/** Finding in the open thread, as a browser finds in a page: the bar's query, and what the core found for it. */
class FindState {
    /** Whether the bar shows. */
    var shown by mutableStateOf(false)
        private set
    /** Whether the transcript shows what was found: with the bar, or alone after the sidebar's search opened a thread at a message. */
    var active by mutableStateOf(false)
        private set
    var query by mutableStateOf("")
    var count by mutableIntStateOf(0)
        private set
    var current by mutableStateOf<Int?>(null)
        private set
    /** Whether the whole thread has been searched. */
    var ready by mutableStateOf(true)
        private set
    /** Asks the bar's field to take the keys. */
    var focusRequests by mutableIntStateOf(0)
        private set

    /** "3/41", or "0/0" when nothing matches. */
    val label: String
        get() {
            if (query.isBlank()) return ""
            val current = current ?: return "0/0"
            return if (count == 0) "0/0" else "${current + 1}/$count"
        }

    internal fun open(query: String?) {
        if (query != null) this.query = query
        shown = true
        active = true
        focusRequests += 1
    }

    internal fun light(query: String) {
        this.query = query
        active = true
    }

    internal fun close() {
        shown = false
        active = false
        count = 0
        current = null
        ready = true
    }

    internal fun found(count: Int, current: Int?, ready: Boolean) {
        this.count = count
        this.current = current
        this.ready = ready
    }
}

/** Opens the bar, with `query` in it when given: what the sidebar searched. It finds from the message `itemID` when that has a match. */
fun AppStore.openFind(query: String? = null, itemID: String? = null) {
    val threadID = transcript.threadID ?: return
    if (selectedThread == null) return
    find.open(query)
    sendFind(threadID, itemID)
}

/** Lights what the sidebar searched for in the open thread and scrolls to it in the message `itemID`, without the bar. Finding then opens the bar on it, back puts it away. */
fun AppStore.lightFound(query: String, itemID: String) {
    val threadID = transcript.threadID ?: return
    if (selectedThread == null) return
    find.light(query)
    sendFind(threadID, itemID)
}

fun AppStore.closeFind() {
    if (!find.active) return
    find.close()
    transcript.setFound(emptyMap())
    val threadID = transcript.threadID ?: return
    core.send("find_end", mapOf("thread_id" to threadID))
}

fun AppStore.setFindQuery(query: String) {
    if (find.query == query) return
    find.query = query
    val threadID = transcript.threadID ?: return
    sendFind(threadID, null)
}

/** Moves to the next match, or with `back` the one before. The bar opens first when it is closed. */
fun AppStore.stepFind(back: Boolean) {
    val threadID = transcript.threadID ?: return
    if (selectedThread == null) return
    if (!find.shown) return openFind()
    core.send("find_step", mapOf("thread_id" to threadID, "back" to back))
}

/** Scrolls the thread to the message. */
fun AppStore.revealItem(itemID: String, threadID: String) {
    core.send("reveal_item", mapOf("thread_id" to threadID, "item_id" to itemID))
}

private fun AppStore.sendFind(threadID: String, itemID: String?) {
    val fields = mutableMapOf<String, Any?>("thread_id" to threadID, "query" to find.query)
    transcript.viewport?.let { fields["from"] = it.json }
    if (itemID != null) fields["item_id"] = itemID
    core.send("find", fields)
}

/** What the core found, and the matches in the rows the transcript has. */
internal fun AppStore.takeFound(event: JSON): () -> Unit {
    val threadID = event.string("thread_id")
    val query = event.string("query")
    val count = event.int("count")
    val current = event.optionalInt("current")
    val ready = event.bool("ready")
    val marks = event.objects("rows").associate { it.string("row_id") to FoundMarks.of(it) }
    return {
        if (find.active && transcript.threadID == threadID && find.query == query) {
            find.found(count, current, ready)
            transcript.setFound(marks)
        }
    }
}

internal fun AppStore.takeReveal(event: JSON): () -> Unit {
    val threadID = event.string("thread_id")
    val rowID = event.string("row_id")
    val range = event.ints("range").takeIf { it.size == 2 }?.let { it[0] until it[0] + it[1] }
    return {
        if (transcript.threadID == threadID) transcript.reveal(rowID, range)
    }
}
