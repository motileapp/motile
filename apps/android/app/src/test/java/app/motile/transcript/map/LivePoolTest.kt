package app.motile.transcript.map

import org.junit.Assert.assertEquals
import org.junit.Test

class LivePoolTest {
    private fun LivePool.liveOf(vararg maps: String) = maps.filter { isLive(it) }

    @Test
    fun atMostThreeRunAndTheOneShownLeastRecentlyGivesItsEngineUp() {
        val pool = LivePool(3)
        listOf("a", "b", "c").forEach(pool::show)
        pool.hide("a")
        pool.hide("b")
        assertEquals("off screen, they keep their engines", listOf("a", "b", "c"), pool.liveOf("a", "b", "c", "d"))
        pool.show("d")
        assertEquals(listOf("b", "c", "d"), pool.liveOf("a", "b", "c", "d"))
        pool.show("a")
        assertEquals(listOf("a", "c", "d"), pool.liveOf("a", "b", "c", "d"))
    }

    @Test
    fun aMapOnScreenWaitsForAnEngineWhenEveryOneIsOnScreen() {
        val pool = LivePool(3)
        listOf("a", "b", "c", "d").forEach(pool::show)
        assertEquals(listOf("a", "b", "c"), pool.liveOf("a", "b", "c", "d"))
        pool.hide("b")
        assertEquals(listOf("a", "c", "d"), pool.liveOf("a", "b", "c", "d"))
        pool.leave("a")
        pool.show("e")
        assertEquals(listOf("c", "d", "e"), pool.liveOf("a", "b", "c", "d", "e"))
    }
}
