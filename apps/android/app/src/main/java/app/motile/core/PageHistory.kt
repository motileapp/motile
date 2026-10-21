package app.motile.core

import android.os.Handler
import android.os.Looper

/** What fills the client in place of the others: a thread or a draft, an agent shown in its thread's place, a section of the settings or the usage. The panels beside them aren't pages. */
sealed interface Page {
    data class Chosen(val selection: Selection) : Page
    data class Agent(val threadID: String, val agentID: String) : Page
    data class Settings(val section: SettingsSection) : Page
    data object Usage : Page
}

/** The pages the client went through, for Go Back and Go Forward. */
class PageHistory {
    private val back = ArrayList<Page>()
    private val forward = ArrayList<Page>()
    var current: Page? = null
        private set
    private val main = Handler(Looper.getMainLooper())
    private var checkPending = false

    /** Runs `check` once what changed with the page has, so that the steps on the way aren't pages. */
    fun schedule(check: () -> Unit) {
        if (checkPending) return
        checkPending = true
        main.post(check)
    }

    fun checked() {
        checkPending = false
    }

    /** The client is on `page`; the one it left goes behind it, and nothing is ahead any more. */
    fun visit(page: Page) {
        val left = current
        current = page
        if (left == null || left == page) return
        back.add(left)
        if (back.size > LIMIT) back.removeAt(0)
        forward.clear()
    }

    fun canStep(isBack: Boolean, exists: (Page) -> Boolean): Boolean = (if (isBack) back else forward).any(exists)

    /** The nearest page behind or ahead that still exists, which becomes the current one. The ones on the way that don't are dropped. */
    fun step(isBack: Boolean, exists: (Page) -> Boolean): Page? {
        val pages = if (isBack) back else forward
        while (pages.isNotEmpty()) {
            val page = pages.removeAt(pages.lastIndex)
            if (!exists(page)) continue
            current?.let { (if (isBack) forward else back).add(it) }
            current = page
            return page
        }
        return null
    }

    /** Where the client ended up after a step, kept without moving the rest. */
    fun land(page: Page) {
        current = page
    }

    private companion object {
        const val LIMIT = 50
    }
}

val AppStore.page: Page
    get() {
        settings?.let { return Page.Settings(it) }
        if (showsUsage) return Page.Usage
        val agentID = rightPanel.shownAgent
        val selection = selection
        if (agentID != null && selection is Selection.Thread) return Page.Agent(selection.id, agentID)
        return Page.Chosen(selection)
    }

fun AppStore.pageChanged() {
    history.schedule { recordPage() }
}

fun AppStore.canGo(back: Boolean): Boolean = history.canStep(back) { pageExists(it) }

/** Goes back or forward to the nearest page that is still there. Whether there was one. */
fun AppStore.go(back: Boolean): Boolean {
    recordPage()
    val page = history.step(back) { pageExists(it) } ?: return false
    show(page)
    val landed = this.page
    if (landed != page) history.land(landed)
    return true
}

private fun AppStore.recordPage() {
    history.checked()
    val page = page
    if (history.current == page) return
    history.visit(page)
}

private fun AppStore.pageExists(page: Page): Boolean = when (page) {
    is Page.Chosen -> when (val selection = page.selection) {
        is Selection.Thread -> anyThread(selection.id) != null
        is Selection.Draft -> threadDrafts.any { it.id == selection.id }
    }
    is Page.Agent -> anyThread(page.threadID) != null
    is Page.Settings, Page.Usage -> account.signedIn
}

private fun AppStore.show(page: Page) {
    when (page) {
        is Page.Settings -> openSettings(page.section)
        Page.Usage -> openUsage()
        is Page.Chosen -> {
            closeRoute()
            show(page.selection)
            rightPanel.showAgents()
        }
        is Page.Agent -> {
            closeRoute()
            show(Selection.Thread(page.threadID))
            rightPanel.showAgent(page.agentID)
        }
    }
}
