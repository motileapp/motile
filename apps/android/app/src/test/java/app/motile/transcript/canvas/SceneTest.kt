package app.motile.transcript.canvas

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

class SceneTest {
    private fun scene(hits: String) = Scene.parse("""{"width": 400, "height": 240, "items": [], "hits": [$hits]}""")!!

    private fun tip() = """"at": 0, "tip": {"x": 0, "y": 0, "title": ""}"""

    private fun Scene.hitAt(angle: Double, distance: Double) =
        hit((100 + distance * cos(angle)).toFloat(), (100 + distance * sin(angle)).toFloat())

    @Test
    fun aBarBandHoldsItsEdgesAndTheLastHitWins() {
        val bands = scene(
            """{"shape": "rect", "x": 0, "y": 0, "width": 100, "height": 240, ${tip()}},
               {"shape": "rect", "x": 100, "y": 0, "width": 100, "height": 240, ${tip()}}""",
        )
        assertEquals(0, bands.hit(50f, 120f))
        assertEquals(1, bands.hit(100f, 120f))
        assertEquals(1, bands.hit(200f, 240f))
        assertNull(bands.hit(201f, 120f))
    }

    @Test
    fun aDonutSectorHoldsWhatLiesInItsRing() {
        val donut = scene("""{"shape": "sector", "cx": 100, "cy": 100, "inner": 50, "outer": 100, "start": ${-PI / 2}, "end": 0, ${tip()}}""")
        assertEquals(0, donut.hitAt(-PI / 4, 70.0))
        assertNull(donut.hitAt(-PI / 4, 30.0))
        assertNull(donut.hitAt(-PI / 4, 110.0))
        assertNull(donut.hitAt(PI / 4, 70.0))
    }

    @Test
    fun aSectorPastAWholeTurnHoldsWhatLiesAcrossIt() {
        val sector = scene("""{"shape": "sector", "cx": 100, "cy": 100, "inner": 0, "outer": 100, "start": 5.5, "end": 7.0, ${tip()}}""")
        assertEquals(0, sector.hitAt(0.3, 50.0))
        assertEquals(0, sector.hitAt(-0.5, 50.0))
        assertNull(sector.hitAt(1.0, 50.0))
        assertNull(sector.hitAt(PI, 50.0))
    }

    @Test
    fun aSceneReadsAsTheCoreWritesIt() {
        val scene = Scene.parse(
            """{"width": 10, "height": 20, "label": "bar chart", "items": [
                {"kind": "rect", "x": 1, "y": 2, "width": 3, "height": 4, "rx": 2, "ry": 2, "fill": {"surface": "row"}},
                {"kind": "path", "verbs": "MLZ", "points": [0, 0, 1, 1], "fill": {"wash": "chart-1"}, "fill_opacity": 0.5,
                 "stroke": {"color": [255, 0, 0, 128], "width": 2, "cap": "round", "dash": [3, 4]}},
                {"kind": "text", "text": "Jan", "x": 5, "y": 6, "anchor": "middle", "role": "ui", "size": 13, "weight": "semibold", "color": "current"}
            ]}""",
        )!!
        assertEquals("bar chart", scene.label)
        val rect = scene.items[0] as SceneItem.Rect
        assertEquals(SceneColor.Surface(SceneColor.Layer.Row), rect.paint.fill)
        val path = scene.items[1] as SceneItem.Path
        assertEquals(SceneColor.Wash("chart-1"), path.paint.fill)
        assertEquals(0.5f, path.paint.fillOpacity)
        assertEquals(SceneColor.Rgba(255, 0, 0, 128), path.paint.stroke?.color)
        assertEquals(SceneStroke.Cap.Round, path.paint.stroke?.cap)
        val label = scene.items[2] as SceneItem.Label
        assertEquals(SceneFont.Ui(13f, SceneFont.Weight.Semibold), label.font)
        assertEquals(SceneItem.Anchor.Middle, label.anchor)
        assertEquals(SceneColor.Current, label.color)
    }

    private val flip = Flip(0.45f, floatArrayOf(0.3f, 0.7f, 0.3f, 1f), 0.2f, listOf(floatArrayOf(0f, 1f), floatArrayOf(0.2f, 0f), floatArrayOf(1f, 1f)))

    @Test
    fun aFlipNarrowsToNothingWhereTheFacesSwap() {
        assertEquals(FlipFrame(1f, false), flipScale(flip, 0f))
        val swap = flipScale(flip, 0.09f)
        assertEquals(0f, swap.scaleX, 1e-4f)
        assertTrue(swap.pastSwap)
        assertEquals(0.5f, flipScale(flip, 0.27f).scaleX, 1e-4f)
        assertEquals(FlipFrame(1f, true), flipScale(flip, 0.45f))
        assertEquals(FlipFrame(1f, true), flipScale(flip, 2f))
        assertFalse(flipScale(flip, 0.08f).pastSwap)
    }

    private val confetti = Confetti(
        duration = 2.2f, curve = floatArrayOf(0.2f, 0.6f, 0.4f, 1f), width = 8f, height = 12f, radius = 2f, top = -12f, fall = 1.05f, endOpacity = 0.7f,
        pieces = listOf(Confetti.Piece(x = 0.5f, color = SceneColor.Token("chart-1"), delay = 0.1f, duration = 1f, drift = 50f, turn = 360f)),
    )

    private fun piece(seconds: Float) = confettiFrames(confetti, 400f, 800f, seconds).single()

    @Test
    fun aPieceOfConfettiFallsOnItsCurve() {
        val start = piece(0f)
        assertEquals(200f, start.left, 1e-3f)
        assertEquals(-12f, start.top, 1e-3f)
        assertEquals(0f, start.turn, 1e-3f)
        assertEquals(1f, start.opacity, 1e-4f)

        val middle = piece(0.6f)
        assertEquals(243.084f, middle.left, 1e-2f)
        assertEquals(711.81f, middle.top, 1e-1f)
        assertEquals(310.2f, middle.turn, 1e-1f)
        assertEquals(0.7415f, middle.opacity, 1e-3f)

        val end = piece(2.2f)
        assertEquals(250f, end.left, 1e-3f)
        assertEquals(828f, end.top, 1e-2f)
        assertEquals(360f, end.turn, 1e-2f)
        assertEquals(0.7f, end.opacity, 1e-4f)
    }
}
