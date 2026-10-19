package app.motile.transcript.canvas

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.rotate
import app.motile.platform.Platform
import app.motile.theme.LocalDark
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.Surface

/** A flip's moment: the face's width as a share of its own, and whether the other face shows by now. */
data class FlipFrame(val scaleX: Float, val pastSwap: Boolean)

/** The flip `seconds` after it started, its frames interpolated linearly. */
fun flipScale(flip: Flip, seconds: Float): FlipFrame {
    val t = if (flip.duration > 0) (seconds / flip.duration).coerceIn(0f, 1f) else 1f
    val frames = flip.frames.filter { it.size >= 2 }
    val after = frames.indexOfFirst { it[0] >= t }
    val scale = when {
        frames.isEmpty() -> 1f
        after == -1 -> frames.last()[1]
        after == 0 -> frames.first()[1]
        else -> {
            val (from, to) = frames[after - 1] to frames[after]
            val span = to[0] - from[0]
            if (span <= 0) to[1] else from[1] + (to[1] - from[1]) * (t - from[0]) / span
        }
    }
    return FlipFrame(scale, t >= flip.swapAt)
}

/** What a flashcard draws now: which face, squashed horizontally about its centre by `scaleX`. */
data class FlipLook(val scaleX: Float, val back: Boolean)

/**
 * Plays the flip on the frame clock whenever `back` changes, from the face that showed to the other;
 * the faces swap at once when the system reduces motion.
 */
@Composable
fun rememberFlip(flip: Flip, back: Boolean): State<FlipLook> {
    val look = remember { mutableStateOf(FlipLook(1f, back)) }
    LaunchedEffect(back) {
        if (look.value.back == back && look.value.scaleX == 1f) return@LaunchedEffect
        if (Platform.reducesMotion) {
            look.value = FlipLook(1f, back)
            return@LaunchedEffect
        }
        val start = withFrameNanos { it }
        while (true) {
            val seconds = withFrameNanos { (it - start) / 1e9f }
            val frame = flipScale(flip, seconds)
            look.value = FlipLook(frame.scaleX, if (frame.pastSwap) back else !back)
            if (seconds >= flip.duration) break
        }
    }
    return look
}

/** Where a piece of confetti is at a moment, in points of the window, and how it shows: rotated `turn` degrees about its centre, at `opacity`. */
data class PieceFrame(val left: Float, val top: Float, val turn: Float, val opacity: Float, val color: SceneColor)

/** Every piece `seconds` into the burst over a window `width` × `height` points. */
fun confettiFrames(confetti: Confetti, width: Float, height: Float, seconds: Float): List<PieceFrame> = confetti.pieces.map { piece ->
    val progress = if (piece.duration > 0) ((seconds - piece.delay) / piece.duration).coerceIn(0f, 1f) else 1f
    val eased = ease(confetti.curve, progress)
    PieceFrame(
        left = piece.x * width + piece.drift * eased,
        top = confetti.top + confetti.fall * height * eased,
        turn = piece.turn * eased,
        opacity = 1f + (confetti.endOpacity - 1f) * eased,
        color = piece.color,
    )
}

/** The CSS timing function `cubic-bezier(x1, y1, x2, y2)` at the moment `x`. */
fun ease(curve: FloatArray, x: Float): Float {
    if (curve.size < 4) return x
    fun bezier(a: Float, b: Float, t: Float): Float {
        val u = 1 - t
        return 3 * u * u * t * a + 3 * u * t * t * b + t * t * t
    }
    var (lo, hi) = 0f to 1f
    repeat(40) {
        val middle = (lo + hi) / 2
        if (bezier(curve[0], curve[2], middle) < x) lo = middle else hi = middle
    }
    return bezier(curve[1], curve[3], (lo + hi) / 2)
}

/** The bursts of confetti playing over the window. A Celebration fires one; nothing shows when the system reduces motion. */
object Celebrations {
    internal class Burst(val id: Long, val confetti: Confetti)

    internal val bursts = mutableStateListOf<Burst>()
    private var next = 0L

    fun fire(confetti: Confetti) {
        if (Platform.reducesMotion) return
        bursts += Burst(next++, confetti)
    }
}

/** The confetti over everything in the window, clipped to it, taking no presses. The root view lays it last. */
@Composable
fun ConfettiLayer(modifier: Modifier = Modifier) {
    for (burst in Celebrations.bursts) {
        key(burst.id) { ConfettiBurst(burst.confetti, modifier) { Celebrations.bursts.remove(burst) } }
    }
}

@Composable
private fun ConfettiBurst(confetti: Confetti, modifier: Modifier, ended: () -> Unit) {
    val palette = LocalPalette.current
    val colors = SceneColors(palette, LocalDark.current, Surface.Background, LocalForeground.current)
    var seconds by remember { mutableFloatStateOf(0f) }
    LaunchedEffect(Unit) {
        val start = withFrameNanos { it }
        while (seconds < confetti.duration) {
            seconds = withFrameNanos { (it - start) / 1e9f }
        }
        ended()
    }
    Canvas(modifier.fillMaxSize().clipToBounds()) {
        val scale = density
        val pieceSize = Size(confetti.width * scale, confetti.height * scale)
        val corner = CornerRadius(confetti.radius * scale)
        for (piece in confettiFrames(confetti, size.width / scale, size.height / scale, seconds)) {
            val topLeft = Offset(piece.left * scale, piece.top * scale)
            rotate(piece.turn, topLeft + Offset(pieceSize.width / 2, pieceSize.height / 2)) {
                drawRoundRect(colors.of(piece.color), topLeft, pieceSize, corner, alpha = piece.opacity.coerceIn(0f, 1f))
            }
        }
    }
}
