package app.motile.transcript.map

import android.os.Handler
import android.os.Looper
import android.view.Choreographer
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.setValue
import app.motile.core.UiKind
import app.motile.core.UiMapPin
import app.motile.core.UiRouteKind
import kotlin.math.min

/** A live map's engine, which the director moves. A snapshot has none. */
interface MapEngine {
    fun jump(camera: MapCamera)
    fun fly(camera: MapCamera, ms: Int)
    fun setPins(pins: List<UiMapPin>)
    fun setLine(line: List<LatLngPoint>)
}

/** Waits and frames. Each call answers what cancels it. */
interface MapClock {
    fun after(ms: Long, run: () -> Unit): () -> Unit

    /** Calls `run` with the time since the first frame, every frame while it answers true. */
    fun frames(run: (elapsedMs: Long) -> Boolean): () -> Unit
}

/** The main thread's handler and Choreographer. */
object MainClock : MapClock {
    private val handler by lazy { Handler(Looper.getMainLooper()) }

    override fun after(ms: Long, run: () -> Unit): () -> Unit {
        val runnable = Runnable(run)
        handler.postDelayed(runnable, ms)
        return { handler.removeCallbacks(runnable) }
    }

    override fun frames(run: (elapsedMs: Long) -> Boolean): () -> Unit {
        val choreographer = Choreographer.getInstance()
        var start = -1L
        val callback = object : Choreographer.FrameCallback {
            override fun doFrame(now: Long) {
                if (start < 0) start = now
                if (run((now - start) / 1_000_000)) choreographer.postFrameCallback(this)
            }
        }
        choreographer.postFrameCallback(callback)
        return { choreographer.removeFrameCallback(callback) }
    }
}

/**
 * The camera and the route of one map, as open-intelligent-ui's map.tsx moves them: the first pin
 * while the program streams, every pin once it ends, the route drawn after. It outlives the engines
 * (inline, expanded, none while a snapshot shows) and hands each the camera and the sources.
 */
class MapDirector(private val maths: MapMaths, private val clock: MapClock) {
    /** Where the map looks, or none before its first pin. A snapshot is taken here. */
    var camera by mutableStateOf<MapCamera?>(null)
        private set

    /** The route as drawn now. */
    var line by mutableStateOf<List<LatLngPoint>>(emptyList())
        private set

    var userMoved = false
        private set

    private var engine: MapEngine? = null
    private var width = 0f
    private var height = 0f
    private var node: UiKind.Map? = null
    private var streaming = false
    private var reducedMotion = false

    private var anchored = false
    private var fitSignature = ""
    private var framedKey = ""
    private var lineDrawn = false
    private var linePoints: List<LatLngPoint> = emptyList()

    private var fitInputs: Any? = null
    private var framing: (() -> Unit)? = null
    private var routeInputs: Any? = null
    private var drawing: (() -> Unit)? = null

    private val animates get() = engine != null && !reducedMotion

    /** The size of the map shown, in points: the inline one's, or the expanded one's. */
    fun resize(width: Float, height: Float) {
        this.width = width
        this.height = height
    }

    fun update(node: UiKind.Map, streaming: Boolean, reducedMotion: Boolean) {
        this.node = node
        this.streaming = streaming
        this.reducedMotion = reducedMotion
        engine?.setPins(node.pins)
        anchor()
        fit()
        route()
    }

    /** Hands the map to a live engine, which takes up the camera and the sources where they are. */
    fun attach(engine: MapEngine) {
        this.engine = engine
        camera?.let(engine::jump)
        node?.let { engine.setPins(it.pins) }
        engine.setLine(line)
    }

    fun detach(engine: MapEngine) {
        if (this.engine === engine) this.engine = null
    }

    /** Where the engine's camera came to rest. */
    fun settled(engine: MapEngine, camera: MapCamera) {
        if (this.engine !== engine) return
        this.camera = camera
    }

    /** The user's own drag or pinch, never the camera's own moves: the map is never fitted again. */
    fun movedByUser(engine: MapEngine) {
        if (this.engine !== engine) return
        userMoved = true
    }

    fun recenter() {
        val target = maths.frame(visible(), width, height, Framing.Recenter) ?: return
        if (animates) fly(target) else jump(target)
    }

    private fun visible(): List<LatLngPoint> = node?.pins.orEmpty().filter { it.visible }.map { listOf(it.lat, it.lng) }

    private fun routingKey(): String = node?.pins.orEmpty().filter { it.visible }.joinToString(";") { "${it.lng},${it.lat}" }

    private fun anchor() {
        val visible = visible()
        if (visible.isEmpty() || anchored) return
        maths.frame(visible, width, height, Framing.First)?.let(::jump)
        anchored = true
    }

    private fun fit() {
        val node = node ?: return
        val key = routingKey()
        val inputs = listOf(streaming, key, node.filter)
        if (inputs != fitInputs) {
            fitInputs = inputs
            framing?.invoke()
            framing = null
        }
        val visible = visible()
        if (streaming || visible.isEmpty()) return
        val signature = "${node.filter.orEmpty()}|$key"
        if (signature == fitSignature) return
        fitSignature = signature
        linePoints = emptyList()
        show(emptyList())
        if (userMoved || !animates) {
            if (!userMoved) maths.frame(visible, width, height, Framing.Fit)?.let(::jump)
            framedKey = key
            return
        }
        maths.frame(visible, width, height, Framing.Fit)?.let(::fly)
        framing = clock.after(FLY_MS + 40L) {
            framing = null
            framedKey = key
            route()
        }
    }

    private fun route() {
        val node = node ?: return
        val positions = if (node.route == UiRouteKind.Off) emptyList() else node.line
        val inputs = listOf(framedKey, routingKey(), positions, streaming)
        if (inputs == routeInputs) return
        routeInputs = inputs
        drawing?.let { stop ->
            stop()
            drawing = null
            show(linePoints)
        }
        if (streaming || framedKey != routingKey()) return
        linePoints = if (positions.size > 1) positions else emptyList()
        if (positions.size < 2 || lineDrawn || !animates) {
            lineDrawn = positions.size > 1
            show(linePoints)
            return
        }
        drawing = clock.frames { elapsed ->
            val t = min(1.0, elapsed.toDouble() / DRAW_MS)
            show(maths.reveal(positions, t))
            if (t >= 1) {
                lineDrawn = true
                drawing = null
            }
            t < 1
        }
    }

    private fun jump(target: MapCamera) {
        camera = target
        engine?.jump(target)
    }

    private fun fly(target: MapCamera) {
        camera = target
        engine?.fly(target, FLY_MS)
    }

    private fun show(points: List<LatLngPoint>) {
        line = points
        engine?.setLine(points)
    }

    companion object {
        const val FLY_MS = 920
        const val DRAW_MS = 760

        /** Keeps where the map was framed while its row scrolls away, so that it isn't framed again. */
        fun saver(maths: MapMaths = CoreMapMaths, clock: MapClock = MainClock): Saver<MapDirector, Any> = listSaver(
            save = {
                val camera = it.camera
                listOf(camera?.lat, camera?.lng, camera?.zoom, it.anchored, it.userMoved, it.fitSignature, it.framedKey, it.lineDrawn)
            },
            restore = { saved ->
                MapDirector(maths, clock).apply {
                    val lat = saved[0] as Double?
                    val lng = saved[1] as Double?
                    val zoom = saved[2] as Double?
                    if (lat != null && lng != null && zoom != null) camera = MapCamera(lat, lng, zoom)
                    anchored = saved[3] as Boolean
                    userMoved = saved[4] as Boolean
                    fitSignature = saved[5] as String
                    framedKey = saved[6] as String
                    lineDrawn = saved[7] as Boolean
                }
            },
        )
    }
}

/** How tall a map is in a window `windowWidth` wide: shorter on a phone, so that it never fills the screen. */
fun mapHeight(node: UiKind.Map, windowWidth: Float): Float = min(node.height, maxOf(280f, 0.82f * windowWidth))
