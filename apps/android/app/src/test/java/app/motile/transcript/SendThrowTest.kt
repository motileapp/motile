package app.motile.transcript

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SendThrowTest {
    private fun assertRect(expected: Rect, actual: Rect, delta: Float) {
        assertEquals(expected.left, actual.left, delta)
        assertEquals(expected.top, actual.top, delta)
        assertEquals(expected.right, actual.right, delta)
        assertEquals(expected.bottom, actual.bottom, delta)
    }

    @Test
    fun springsRunAsCoreAnimationRunsThem() {
        val travel = ThrownMessage.travel
        assertEquals(0f, travel.progress(-0.05), 0f)
        assertEquals(0f, travel.progress(0.0), 0f)
        assertEquals(0.38680f, travel.progress(0.1), 1e-4f)
        assertEquals(0.82330f, travel.progress(0.2), 1e-4f)
        // It overshoots by three and a half percent, and has settled by the end of the throw.
        assertEquals(1.03536f, travel.progress(0.3852), 1e-4f)
        assertEquals(0.99988f, travel.progress(1.0), 1e-4f)
        assertEquals(0.40284f, ThrownMessage.shrink.progress(0.1), 1e-4f)
        assertEquals(0.41432f, ThrownMessage.regrow.progress(0.1), 1e-4f)
    }

    @Test
    fun theCurveIsTheMediaTimingFunctions() {
        val curve = ThrownMessage.reshape
        assertEquals(0f, curve.value(-1.0), 0f)
        assertEquals(0f, curve.value(0.0), 0f)
        assertEquals(0.08640f, curve.value(0.25), 1e-4f)
        assertEquals(0.41374f, curve.value(0.5), 1e-4f)
        assertEquals(0.85312f, curve.value(0.75), 1e-4f)
        assertEquals(1f, curve.value(1.0), 0f)
        assertEquals(1f, curve.value(2.0), 0f)
    }

    @Test
    fun aTallerBubbleShrinksLess() {
        assertEquals(0.7f, ThrownMessage.smallestScale(20f), 0f)
        assertEquals(0.7f, ThrownMessage.smallestScale(40.25f), 1e-6f)
        assertEquals(0.74941f, ThrownMessage.smallestScale(100f), 1e-5f)
        assertEquals(0.9f, ThrownMessage.smallestScale(300f), 0f)
    }

    @Test
    fun theBubbleStartsAroundTheTextWhereItWasWritten() {
        val text = Rect(14f, 10f, 186f, 30f)
        val written = Rect(20f, 500f, 320f, 520f)
        assertRect(Rect(6f, 490f, 334f, 530f), ThrownMessage.throwStart(text, written, Size(200f, 40f), reshapes = true), 0f)
        // A queued message keeps its size, its text still on the written text.
        assertRect(Rect(6f, 490f, 206f, 610f), ThrownMessage.throwStart(text, written, Size(200f, 120f), reshapes = false), 0f)
    }

    @Test
    fun theBubbleLeavesFromItsStartAndLandsOnItsRow() {
        val start = Rect(6f, 490f, 334f, 530f)
        val target = Rect(150f, 300f, 350f, 340f)
        val first = ThrownMessage.throwFrame(start, target, 0.0, smallest = 0.7f)
        assertRect(start, first.bubble, 0f)
        assertEquals(1f, first.scale, 0f)
        assertEquals(0f, first.shaped, 0f)

        // On its way it has shrunk, and its right edge has gone further than its middle, which leaves later.
        val flying = ThrownMessage.throwFrame(start, target, 0.15, smallest = 0.7f)
        assertEquals(0.80528f, flying.scale, 1e-4f)
        val across = (flying.bubble.right - start.right) / (target.right - start.right)
        val up = (flying.bubble.center.y - start.center.y) / (target.center.y - start.center.y)
        assertTrue(across > up)

        val last = ThrownMessage.throwFrame(start, target, 1.0, smallest = 0.7f)
        assertRect(target, last.bubble, 0.1f)
        assertEquals(1f, last.scale, 1e-3f)
        assertEquals(1f, last.shaped, 0f)
    }

    @Test
    fun aFileFliesFromItsTileToItsPlace() {
        val piece = ThrownMessage.Piece(from = Rect(0f, 0f, 96f, 96f), to = Rect(100f, 20f, 292f, 116f), fades = true)
        val first = piece.at(0f)
        assertEquals(Offset(-148f, -20f), first.offset)
        assertEquals(0.5f, first.scale, 0f)
        assertEquals(0f, first.alpha, 0f)
        val last = piece.at(1f)
        assertEquals(0f, last.offset.x, 0f)
        assertEquals(0f, last.offset.y, 0f)
        assertEquals(1f, last.scale, 0f)
        assertEquals(1f, last.alpha, 0f)
        assertEquals(1f, ThrownMessage.Piece(Rect(0f, 0f, 96f, 96f), Rect(0f, 0f, 96f, 96f), fades = false).at(0f).alpha, 0f)
    }
}
