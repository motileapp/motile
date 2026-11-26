package app.motile.transcript

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.layer.GraphicsLayer
import androidx.compose.ui.graphics.layer.drawLayer
import androidx.compose.ui.graphics.rememberGraphicsLayer
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onPlaced
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.toSize
import app.motile.core.AttachedFile
import app.motile.core.TranscriptModel
import app.motile.theme.LocalPalette
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin
import kotlin.math.sqrt

/** The composer's text, which a message is thrown from into the transcript when it is sent. */
interface ThrowSource {
    /** What is written, as it was last laid out. */
    val written: String
    /** Where the written text is, in the root's coordinates. */
    val writtenRect: Rect?

    /** A message was thrown from it: its placeholder, which would show under the message's text, comes in a moment later. */
    fun threw()
}

/** Where a message was in the composer when it was sent, in the root's coordinates: its text, and the tile of each file it attaches, when the composer shows one. */
class ThrowOrigin(val source: ThrowSource, val text: Rect?, val tiles: List<Rect?>)

/** Marks where the composer shows a file, for a message that attaches it to be thrown from there. */
class ThrowAnchor(var file: AttachedFile) {
    var coordinates: LayoutCoordinates? = null

    val rect: Rect? get() = coordinates?.takeIf { it.isAttached }?.let { Rect(it.positionInRoot(), it.size.toSize()) }
}

object ThrowSources {
    var composer: ThrowSource? = null
    val anchors = mutableListOf<ThrowAnchor>()

    /** Where the composer shows the message of `row`, while it still shows it. */
    fun origin(row: RowModel): ThrowOrigin? = Snapshot.withoutReadObservation {
        val composer = composer ?: return@withoutReadObservation null
        val text = row.sentText ?: return@withoutReadObservation null
        val written = composer.written.trim()
        if (written != text.trim()) return@withoutReadObservation null
        val free = anchors.toMutableList()
        val tiles = row.sentFiles.map { file ->
            val anchor = free.firstOrNull { it.file == file } ?: return@map null
            free.remove(anchor)
            anchor.rect
        }
        if (written.isEmpty() && tiles.all { it == null }) return@withoutReadObservation null
        val rect = if (written.isEmpty()) null else composer.writtenRect ?: return@withoutReadObservation null
        ThrowOrigin(composer, rect, tiles)
    }
}

/** An anchor for a tile of the composer that shows `file`, while the tile is there. */
@Composable
fun rememberThrowAnchor(file: AttachedFile): ThrowAnchor {
    val anchor = remember { ThrowAnchor(file) }
    anchor.file = file
    DisposableEffect(anchor) {
        ThrowSources.anchors += anchor
        onDispose { ThrowSources.anchors -= anchor }
    }
    return anchor
}

fun Modifier.throwAnchor(anchor: ThrowAnchor): Modifier = onPlaced { anchor.coordinates = it }

/** Where a row lays out its message's bubble, its text and its files in their order, for a thrown message to land on. */
class ThrowTarget {
    var bubble: LayoutCoordinates? = null
    var text: LayoutCoordinates? = null
    val pieces = HashMap<Int, LayoutCoordinates>()

    /** Tells it where the file of the index is. */
    val piece = { index: Int -> Modifier.onPlaced { pieces[index] = it } }
}

/** A row's target, which the transcript finds by the row's id while the row is on screen. */
@Composable
fun rememberThrowTarget(rowID: String): ThrowTarget {
    val actions = LocalTranscript.current
    val target = remember(rowID) { ThrowTarget() }
    DisposableEffect(target) {
        actions.throwTargets[rowID] = target
        onDispose { actions.throwTargets.remove(rowID, target) }
    }
    return target
}

/** The message just sent, on its way from the composer to its row, of one transcript. */
class Throws(private val model: TranscriptModel) {
    var thrown by mutableStateOf<ThrownMessage?>(null)
    /** The pending message last seen, to tell when one is sent. */
    var seenPending: RowModel? = null

    /** The row it lands on: the message on its way, then the one the server has. None once the message is taken back. */
    val rowID: String? by derivedStateOf {
        val thrown = thrown ?: return@derivedStateOf null
        if (model.pending != null) return@derivedStateOf RowModel.pendingID
        model.rows.lastOrNull { it.isSentMessage }?.id?.takeIf { it != thrown.sentBefore }
    }

    fun end(thrown: ThrownMessage) {
        if (this.thrown === thrown) this.thrown = null
    }
}

/**
 * A message on its way from the composer to its row, as Messages throws it: a bubble that starts
 * around the text where it was written and takes its own size, travels to the row on a spring that
 * leaves a moment before the one that lifts it, so that it curves, and shrinks as it leaves and
 * grows back as it lands. Its row stays hidden until it is there.
 */
class ThrownMessage(val row: RowModel, private val origin: ThrowOrigin, val sentBefore: String?) {
    /** A file the message attaches, which flies from its tile in the composer to its place in the bubble while the bubble takes its shape. A file's card looks unlike its tile and comes in. */
    class Piece(val from: Rect, val to: Rect, val fades: Boolean) {
        fun at(shaped: Float): PieceFrame {
            val offset = (from.center - to.center) * (1 - shaped)
            val first = min(from.width / max(1f, to.width), from.height / max(1f, to.height))
            return PieceFrame(offset, first + (1 - first) * shaped, if (fades) shaped else 1f)
        }
    }

    class PieceFrame(val offset: Offset, val scale: Float, val alpha: Float)

    class Frame(val bubble: Rect, val scale: Float, val shaped: Float)

    val reshapes = !row.isQueued
    /** How far it is on its way, which only starts on the first frame after the send and only moves a frame at a time, so that a moment the main thread is held doesn't skip any of it. */
    var elapsed by mutableDoubleStateOf(0.0)
        private set
    private var lastFrame = 0L
    /** The bubble where it starts, in the transcript, once its row is laid out. */
    private var start: Rect? = null
    private var smallest = 1f
    private var pieces = emptyMap<Int, Piece>()
    /** Where its copy is laid out in the transcript, which it is drawn from. */
    var placedAt = Offset.Zero

    val done: Boolean get() = elapsed >= DURATION

    fun tick(nanos: Long) {
        if (lastFrame != 0L) elapsed += min((nanos - lastFrame) / 1e9, 1.0 / 30)
        lastFrame = nanos
    }

    /** How far along the transcript is on its way to its end, which it travels with the message. */
    val scrolled: Float get() = travel.progress(elapsed - LIFT)

    /** Where the bubble is by now on its way to its row's, in `host`, or null while the row isn't laid out. */
    fun frame(host: LayoutCoordinates, target: ThrowTarget?, density: Float): Frame? {
        val bubble = target?.bubble?.takeIf { it.isAttached && host.isAttached } ?: return null
        val landing = host.localBoundingBoxOf(bubble, clipBounds = false)
        val start = start ?: begin(host, bubble, target, density) ?: return null
        return throwFrame(start, landing, elapsed, smallest)
    }

    fun piece(index: Int): PieceFrame? = pieces[index]?.at(reshape.value(elapsed / RESHAPE_TIME))

    /** Puts the bubble's start where the message was written: its text on the composer's, or its files on their tiles. */
    private fun begin(host: LayoutCoordinates, bubble: LayoutCoordinates, target: ThrowTarget, density: Float): Rect? {
        val root = host.positionInRoot()
        val final = bubble.size.toSize()
        val attached = row.sentFiles.indices.map { index -> target.pieces[index]?.takeIf { it.isAttached }?.let { bubble.localBoundingBoxOf(it, clipBounds = false) } }
        val tiles = origin.tiles.map { it?.translate(-root) }
        val known = attached.zip(tiles).mapNotNull { (piece, tile) -> if (piece != null && tile != null) piece to tile else null }
        val text = target.text?.takeIf { it.isAttached }?.let { bubble.localBoundingBoxOf(it, clipBounds = false) }
        val written = origin.text?.translate(-root)
        val (inBubble, inComposer) = when {
            written != null && text != null -> Rect(text.left, text.top, final.width - text.left, text.bottom) to written
            written == null && known.isNotEmpty() -> known.map { it.first }.reduce(::union) to known.map { it.second }.reduce(::union)
            else -> return null
        }
        val start = throwStart(inBubble, inComposer, final, reshapes)
        this.start = start
        smallest = smallestScale(start.height / density)
        pieces = attached.indices.mapNotNull { index ->
            val to = attached[index] ?: return@mapNotNull null
            val tile = tiles.getOrNull(index) ?: return@mapNotNull null
            index to Piece(tile.translate(-start.topLeft), to, fades = !row.sentFiles[index].pictured)
        }.toMap()
        return start
    }

    companion object {
        /** The springs and curves Messages throws with. */
        val travel = Spring(mass = 1.0, stiffness = 141.759, damping = 17.3503)
        const val LIFT = 0.055
        val shrink = Spring(mass = 2.0, stiffness = 310.0, damping = 38.0)
        val regrow = Spring(mass = 2.0, stiffness = 320.0, damping = 38.0)
        const val REGROW_AFTER = 0.185
        val reshape = CubicCurve(0.542, 0.0, 0.58, 1.0)
        const val RESHAPE_TIME = 0.2
        const val DURATION = 1.0

        /** A taller composer throws a message that shrinks less, as in Messages. `height` is in points. */
        fun smallestScale(height: Float): Float = (0.7f + (height - 40.25f) * 0.000827f).coerceIn(0.7f, 0.9f)

        /** The bubble where it starts: what lies `inBubble` in the bubble lies `inComposer`, and the bubble is around it as it is around that, unless it keeps its size. */
        fun throwStart(inBubble: Rect, inComposer: Rect, bubble: Size, reshapes: Boolean): Rect {
            val size = if (!reshapes) bubble else Size(
                inComposer.width + inBubble.left + bubble.width - inBubble.right,
                inComposer.height + inBubble.top + bubble.height - inBubble.bottom,
            )
            return Rect(inComposer.topLeft - inBubble.topLeft, size)
        }

        /** Where the bubble is `time` seconds after it left `start` for `target`, how much it is scaled around its right edge's middle, and how far it has taken its shape. */
        fun throwFrame(start: Rect, target: Rect, time: Double, smallest: Float): Frame {
            val shaped = reshape.value(time / RESHAPE_TIME)
            val width = start.width + (target.width - start.width) * shaped
            val height = start.height + (target.height - start.height) * shaped
            val right = start.right + (target.right - start.right) * travel.progress(time)
            val middle = start.center.y + (target.center.y - start.center.y) * travel.progress(time - LIFT)
            val scale = (1 + (smallest - 1) * shrink.progress(time)) * (1 + (1 / smallest - 1) * regrow.progress(time - REGROW_AFTER))
            return Frame(Rect(right - width, middle - height / 2, right, middle + height / 2), scale, shaped)
        }
    }
}

/** Where the thread draws the message on its way: over the composer it leaves, under what floats over the thread. */
class ThrowLayer {
    /** The message on its way and its row's target, as the transcript has them. */
    class Flying(val thrown: ThrownMessage, val target: () -> ThrowTarget?)

    var flying by mutableStateOf<Flying?>(null)
    /** The copy of the message as the transcript lays it out, which the layer draws where it is by now. */
    var copy: GraphicsLayer? = null
}

/** Draws the message on its way where it is by now, with its fill as far as it has taken its shape. */
@Composable
fun ThrowLayerView(layer: ThrowLayer) {
    val fill = LocalPalette.current.backgroundSecondary
    val copy = rememberGraphicsLayer()
    layer.copy = copy
    val host = remember { arrayOfNulls<LayoutCoordinates>(1) }
    Box(
        Modifier
            .fillMaxSize()
            .onPlaced { host[0] = it }
            .drawBehind {
                val flying = layer.flying ?: return@drawBehind
                val thrown = flying.thrown
                val frame = host[0]?.let { thrown.frame(it, flying.target(), density) } ?: return@drawBehind
                val bubble = frame.bubble
                scale(frame.scale, Offset(bubble.right, bubble.center.y)) {
                    if (thrown.reshapes) drawRoundRect(fill.copy(alpha = fill.alpha * frame.shaped), bubble.topLeft, bubble.size, CornerRadius(Bubble.radius.toPx()))
                    translate(bubble.left - thrown.placedAt.x, bubble.top - thrown.placedAt.y) { drawLayer(copy) }
                }
            },
    )
}

/**
 * The copy of the message on its way, laid out in the transcript out of a finger's reach, and
 * recorded for `layer` to draw over the composer where it is by now. It is recorded after the rows
 * are laid out, so that it keeps up with them as they scroll.
 */
@Composable
fun ThrownCopy(thrown: ThrownMessage, row: RowModel, targets: Map<String, ThrowTarget>, rowID: () -> String?, column: Dp, layer: ThrowLayer) {
    val host = remember { arrayOfNulls<LayoutCoordinates>(1) }
    val piece = remember(thrown) {
        { index: Int ->
            Modifier.graphicsLayer {
                val frame = thrown.piece(index) ?: return@graphicsLayer
                translationX = frame.offset.x
                translationY = frame.offset.y
                scaleX = frame.scale
                scaleY = frame.scale
                alpha = frame.alpha
            }
        }
    }
    Layout(
        content = { Box(Modifier.clearAndSetSemantics {}) { ThrownBubble(row, piece) } },
        modifier = Modifier
            .fillMaxSize()
            .onPlaced { host[0] = it }
            .drawWithContent { layer.copy?.record { this@drawWithContent.drawContent() } },
    ) { measurables, constraints ->
        val placeable = measurables.firstOrNull()?.measure(Constraints(maxWidth = column.roundToPx()))
        layout(constraints.maxWidth, constraints.maxHeight) {
            // The first frame fixes where it starts, while the composer still shows the message.
            host[0]?.let { thrown.frame(it, rowID()?.let(targets::get), density) }
            thrown.placedAt = Offset(OUT_OF_REACH.x.toFloat(), OUT_OF_REACH.y.toFloat())
            placeable?.place(OUT_OF_REACH)
        }
    }
}

/** Where the copy is laid out: off the screen, where no finger reaches its buttons. */
private val OUT_OF_REACH = IntOffset(-20_000, -20_000)

private fun union(a: Rect, b: Rect) = Rect(min(a.left, b.left), min(a.top, b.top), max(a.right, b.right), max(a.bottom, b.bottom))

/** A spring as Core Animation runs one from rest: how far along it is `time` seconds in. */
class Spring(val mass: Double, val stiffness: Double, val damping: Double) {
    fun progress(time: Double): Float {
        if (time <= 0) return 0f
        val natural = sqrt(stiffness / mass)
        val ratio = damping / (2 * sqrt(stiffness * mass))
        if (ratio >= 1) return (1 - exp(-natural * time) * (1 + natural * time)).toFloat()
        val damped = natural * sqrt(1 - ratio * ratio)
        val decay = exp(-ratio * natural * time)
        return (1 - decay * (cos(damped * time) + ratio * natural / damped * sin(damped * time))).toFloat()
    }
}

/** A timing curve as `CAMediaTimingFunction(controlPoints:)` draws it. */
class CubicCurve(private val x1: Double, private val y1: Double, private val x2: Double, private val y2: Double) {
    fun value(x: Double): Float {
        if (x <= 0) return 0f
        if (x >= 1) return 1f
        var low = 0.0
        var high = 1.0
        repeat(24) {
            val middle = (low + high) / 2
            if (bezier(middle, x1, x2) < x) low = middle else high = middle
        }
        return bezier((low + high) / 2, y1, y2).toFloat()
    }

    private fun bezier(t: Double, a: Double, b: Double): Double {
        val u = 1 - t
        return 3 * u * u * t * a + 3 * u * t * t * b + t * t * t
    }
}
