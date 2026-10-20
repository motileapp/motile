package app.motile.transcript.map

import app.motile.core.UiKind
import app.motile.core.UiMapPin
import app.motile.core.UiRadius
import app.motile.core.UiRadiusKind
import app.motile.core.UiRouteKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Frames on the first point at a zoom that says which framing it was; reveals a share of the points. */
private object FakeMaths : MapMaths {
    override fun frame(points: List<LatLngPoint>, width: Float, height: Float, framing: Framing): MapCamera? {
        val first = points.firstOrNull() ?: return null
        val zoom = when (framing) {
            Framing.First -> 10.5
            Framing.Fit -> 12.0 + points.size
            Framing.Recenter -> 14.0
        }
        return MapCamera(first[0], first[1], zoom)
    }

    override fun reveal(line: List<LatLngPoint>, t: Double): List<LatLngPoint> = line.take(maxOf(1, (line.size * t).toInt()))
}

private class FakeClock : MapClock {
    val waits = mutableListOf<Pair<Long, () -> Unit>>()
    var frame: ((Long) -> Boolean)? = null

    override fun after(ms: Long, run: () -> Unit): () -> Unit {
        val wait = ms to run
        waits += wait
        return { waits -= wait }
    }

    override fun frames(run: (elapsedMs: Long) -> Boolean): () -> Unit {
        frame = run
        return { frame = null }
    }

    fun elapse() = waits.toList().also { waits.clear() }.forEach { it.second() }

    fun draw(elapsed: Long) {
        val run = frame ?: return
        if (!run(elapsed)) frame = null
    }
}

private class FakeEngine : MapEngine {
    val commands = mutableListOf<String>()
    override fun jump(camera: MapCamera) { commands += "jump ${camera.zoom}" }
    override fun fly(camera: MapCamera, ms: Int) { commands += "fly ${camera.zoom} $ms" }
    override fun setPins(pins: List<UiMapPin>) { commands += "pins ${pins.count { it.visible }}" }
    override fun setLine(line: List<LatLngPoint>) { commands += "line ${line.size}" }

    fun moves() = commands.filter { it.startsWith("jump") || it.startsWith("fly") }
    fun lines() = commands.filter { it.startsWith("line") }
    fun clear() = commands.clear()
}

class MapDirectorTest {
    private fun pin(n: Int, category: String = "Food", visible: Boolean = true) =
        UiMapPin("P$n", "P$n", 37.0 + n, -122.0 - n, category, null, n, false, visible)

    private fun node(pins: List<UiMapPin>, route: UiRouteKind = UiRouteKind.Off, filter: String? = null) = UiKind.Map(
        pins = pins,
        route = route,
        line = if (route == UiRouteKind.Off) emptyList() else pins.filter { it.visible }.map { listOf(it.lat, it.lng) },
        selected = null,
        height = 400f,
        categories = pins.mapNotNull { it.category }.distinct(),
        filter = filter,
        note = "",
        radius = UiRadius(UiRadiusKind.Xl, 0f),
    )

    private fun director(engine: FakeEngine?, clock: FakeClock = FakeClock()) = MapDirector(FakeMaths, clock).apply {
        resize(400f, 300f)
        engine?.let(::attach)
        engine?.clear()
    }

    @Test
    fun theFirstPinAnchorsTheCameraAndLaterOnesNeverMoveItWhileStreaming() {
        val engine = FakeEngine()
        val director = director(engine)
        director.update(node(emptyList()), streaming = true, reducedMotion = false)
        assertEquals(emptyList<String>(), engine.moves())
        director.update(node(listOf(pin(1))), streaming = true, reducedMotion = false)
        director.update(node(listOf(pin(1), pin(2))), streaming = true, reducedMotion = false)
        assertEquals(listOf("jump 10.5"), engine.moves())
        assertEquals(MapCamera(38.0, -123.0, 10.5), director.camera)
    }

    @Test
    fun onceStreamingEndsItFliesToEveryPinThenRevealsTheRoute() {
        val clock = FakeClock()
        val engine = FakeEngine()
        val director = director(engine, clock)
        val pins = listOf(pin(1), pin(2), pin(3), pin(4))
        director.update(node(pins, UiRouteKind.Loading), streaming = true, reducedMotion = false)
        director.update(node(pins, UiRouteKind.Loading), streaming = false, reducedMotion = false)
        assertEquals(listOf("jump 10.5", "fly 16.0 920"), engine.moves())
        assertEquals(listOf(960L), clock.waits.map { it.first })
        engine.clear()
        clock.elapse()
        clock.draw(0)
        clock.draw(380)
        clock.draw(760)
        assertEquals(listOf("line 1", "line 2", "line 4"), engine.lines())
        assertEquals(null, clock.frame)

        engine.clear()
        director.update(node(pins, UiRouteKind.Street).copy(line = List(9) { listOf(1.0, 2.0) }), streaming = false, reducedMotion = false)
        assertEquals("the street route comes at once", listOf("line 9"), engine.lines())
    }

    @Test
    fun aFilterClearsTheRouteFliesAgainAndDrawsItAtOnce() {
        val clock = FakeClock()
        val engine = FakeEngine()
        val director = director(engine, clock)
        val pins = listOf(pin(1, "Food"), pin(2, "Food"), pin(3, "Views"))
        director.update(node(pins, UiRouteKind.Loading), streaming = false, reducedMotion = false)
        clock.elapse()
        clock.draw(760)
        engine.clear()

        val food = pins.map { it.copy(visible = it.category == "Food") }
        director.update(node(food, UiRouteKind.Loading, filter = "Food"), streaming = false, reducedMotion = false)
        assertEquals(listOf("fly 14.0 920"), engine.moves())
        assertEquals(listOf("line 0"), engine.lines())
        clock.elapse()
        assertEquals(listOf("line 0", "line 2"), engine.lines())
        assertEquals(null, clock.frame)
    }

    @Test
    fun theUsersOwnMoveKeepsTheCameraWhereItIs() {
        val clock = FakeClock()
        val engine = FakeEngine()
        val director = director(engine, clock)
        val pins = listOf(pin(1), pin(2))
        director.update(node(pins, UiRouteKind.Loading), streaming = true, reducedMotion = false)
        director.movedByUser(engine)
        director.movedByUser(FakeEngine())
        director.update(node(pins, UiRouteKind.Loading), streaming = false, reducedMotion = false)
        assertEquals(listOf("jump 10.5"), engine.moves())
        assertTrue("framed at once", clock.waits.isEmpty())
        clock.draw(760)
        assertEquals("line 2", engine.lines().last())
    }

    @Test
    fun reducedMotionJumpsAndDrawsTheRouteAtOnce() {
        val clock = FakeClock()
        val engine = FakeEngine()
        val director = director(engine, clock)
        val pins = listOf(pin(1), pin(2), pin(3))
        director.update(node(pins, UiRouteKind.Loading), streaming = false, reducedMotion = true)
        assertEquals(listOf("jump 10.5", "jump 15.0"), engine.moves())
        assertEquals(listOf("line 0", "line 3"), engine.lines())
        assertTrue(clock.waits.isEmpty() && clock.frame == null)
        director.recenter()
        assertEquals("jump 14.0", engine.moves().last())
    }

    @Test
    fun recenterFliesToEveryPin() {
        val engine = FakeEngine()
        val director = director(engine)
        director.update(node(listOf(pin(1), pin(2))), streaming = false, reducedMotion = false)
        director.recenter()
        assertEquals("fly 14.0 920", engine.moves().last())
    }

    @Test
    fun withoutAnEngineTheCameraStillMovesForTheSnapshotAndAnEngineTakesItUp() {
        val clock = FakeClock()
        val director = director(null, clock)
        val pins = listOf(pin(1), pin(2))
        director.update(node(pins, UiRouteKind.Loading), streaming = false, reducedMotion = false)
        assertEquals(MapCamera(38.0, -123.0, 14.0), director.camera)
        assertEquals(2, director.line.size)
        assertTrue(clock.waits.isEmpty() && clock.frame == null)

        val engine = FakeEngine()
        director.attach(engine)
        assertEquals(listOf("jump 14.0", "pins 2", "line 2"), engine.commands)
        director.settled(engine, MapCamera(1.0, 2.0, 3.0))
        director.detach(engine)
        director.settled(engine, MapCamera(9.0, 9.0, 9.0))
        assertEquals(MapCamera(1.0, 2.0, 3.0), director.camera)
    }
}
