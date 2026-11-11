package app.motile.core

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.inspector.WindowInspector
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalWindowInfo
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.launch

/**
 * Tells the core whether the client is focused, with the project on screen, and when the user last
 * used it, so that it counts the user's time. Focused is the activity resumed on top with a window
 * of the client's own focused: a sheet or a dialog takes the focus from the activity, the
 * notification shade from all of them.
 */
class Engagement {
    private val main = Handler(Looper.getMainLooper())
    private val scope = MainScope()
    private lateinit var store: AppStore
    private var resumed = false
    private var topResumed = false
    private var windowFocused = false
    private var place: Place? = null
    private var sent: Triple<Boolean, String?, String?>? = null
    /** When the user last touched or typed, and the last of those the core was told of, in `SystemClock.elapsedRealtime`. */
    private var lastInput = 0L
    private var reportedInput = 0L
    private val tick = object : Runnable {
        override fun run() {
            checkWindows()
            reportInput()
            if (focused) main.postDelayed(this, INTERVAL)
        }
    }
    private val focused: Boolean get() = resumed && topResumed && windowFocused

    private data class Place(val serverID: String, val projectID: String)

    fun start(store: AppStore) {
        this.store = store
        shared = this
        scope.launch {
            snapshotFlow { store.shownPlace() }.collect {
                place = it
                report()
            }
        }
    }

    fun resumed(resumed: Boolean) {
        this.resumed = resumed
        if (!resumed) topResumed = false
        report()
    }

    fun topResumed(top: Boolean) {
        topResumed = top
        report()
    }

    /** The activity's window gained or lost the focus. Losing it to a window of the client's own, that window has it a moment later. */
    fun activityFocused(focused: Boolean) {
        if (focused) return checkWindows()
        main.postDelayed(::checkWindows, HANDOVER)
    }

    /** A window of the client's gained or lost the focus. */
    fun checkWindows() {
        windowFocused = WindowInspector.getGlobalWindowViews().any { it.hasWindowFocus() }
        report()
    }

    fun interacted() {
        lastInput = SystemClock.elapsedRealtime()
        if (!focused && resumed && topResumed) checkWindows()
    }

    private fun report() {
        if (!::store.isInitialized) return
        val now = Triple(focused, place?.serverID, place?.projectID)
        if (now == sent) return
        val gained = focused && sent?.first != true
        sent = now
        store.core.send(
            "focus",
            buildMap {
                put("focused", focused)
                place?.let {
                    put("server_id", it.serverID)
                    put("project_id", it.projectID)
                }
            },
        )
        if (!focused) return main.removeCallbacks(tick)
        if (!gained) return
        reportedInput = lastInput
        main.postDelayed(tick, INTERVAL)
    }

    private fun reportInput() {
        if (!focused || lastInput <= reportedInput) return
        reportedInput = lastInput
        store.core.send("input", mapOf("idle_secs" to (SystemClock.elapsedRealtime() - lastInput) / 1000.0))
    }

    private fun AppStore.shownPlace(): Place? {
        if (settings != null || showsUsage) return null
        val thread = selectedThread?.takeIf { selection == Selection.Thread(it.id) }
        if (thread != null) return Place(thread.serverID, thread.projectID)
        val project = project(selectedDraft?.projectID) ?: return null
        return Place(project.serverID, project.id)
    }

    companion object {
        private const val INTERVAL = 5_000L
        private const val HANDOVER = 300L

        /** The store's, for the sheets and the text fields: the activity doesn't see what the soft keyboard types. */
        var shared: Engagement? = null
            private set
    }
}

/**
 * A window of the client's own other than the activity's, a sheet or a dialog, whose touches and
 * keys count as the user's, as the activity's do, and whose focus counts as the client's.
 */
@Composable
fun EngagedWindow(content: @Composable () -> Unit) {
    val window = LocalWindowInfo.current
    LaunchedEffect(window) { snapshotFlow { window.isWindowFocused }.collect { Engagement.shared?.checkWindows() } }
    Column(
        Modifier
            .pointerInput(Unit) {
                awaitPointerEventScope {
                    while (true) {
                        awaitPointerEvent(PointerEventPass.Initial)
                        Engagement.shared?.interacted()
                    }
                }
            }
            .onPreviewKeyEvent {
                Engagement.shared?.interacted()
                false
            },
    ) { content() }
}
