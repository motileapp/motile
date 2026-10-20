package app.motile.transcript.map

import androidx.compose.runtime.mutableStateListOf

/**
 * Which maps run their engine: those on screen, at most `limit` at once. A map that leaves the
 * screen keeps its engine until another needs it, the one shown least recently giving it up first.
 */
class LivePool(private val limit: Int) {
    private val live = mutableStateListOf<Any>()
    private val onScreen = mutableSetOf<Any>()
    private val shownAt = mutableMapOf<Any, Long>()
    private var clock = 0L

    fun isLive(map: Any): Boolean = map in live

    fun show(map: Any) {
        onScreen += map
        shownAt[map] = ++clock
        grant(map)
    }

    fun hide(map: Any) {
        onScreen -= map
        waiting().forEach(::grant)
    }

    fun leave(map: Any) {
        onScreen -= map
        shownAt -= map
        live -= map
        waiting().forEach(::grant)
    }

    private fun waiting(): List<Any> = onScreen.filter { it !in live }.sortedByDescending { shownAt[it] ?: 0 }

    private fun grant(map: Any) {
        if (map in live) return
        if (live.size >= limit) {
            val off = live.filter { it !in onScreen }.minByOrNull { shownAt[it] ?: 0 } ?: return
            live -= off
        }
        live += map
    }

    companion object {
        /** Every map of the client. */
        val maps = LivePool(3)
    }
}
