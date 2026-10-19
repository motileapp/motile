package app.motile.transcript.openui

import android.util.LruCache
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import app.motile.core.UiCanvas
import app.motile.transcript.canvas.CanvasScene
import app.motile.transcript.canvas.CanvasText
import app.motile.transcript.canvas.Canvases
import kotlinx.serialization.json.JsonObject

/** Core's scenes of the programs' canvas components, kept for the rows that show them again. */
object UiScenes {
    private val kept = LruCache<String, CanvasScene>(64)

    fun key(canvas: UiCanvas, width: Float, textSize: Float, text: CanvasText): String =
        "${canvas.kind} ${canvas.props} $width $textSize ${text.density.density}"

    fun kept(key: String): CanvasScene? = kept.get(key)

    suspend fun laidOut(key: String, canvas: UiCanvas, width: Float, textSize: Float, text: CanvasText): CanvasScene? {
        val props = canvas.props as? JsonObject ?: JsonObject(emptyMap())
        return Canvases.laidOut(canvas.kind, props, width, textSize, text)?.also { kept.put(key, it) }
    }
}

/**
 * The node's scene at a `width` and with text `textSize` points tall, laid out off the main thread:
 * none until the first one is, and the one before while the next one is.
 */
@Composable
fun rememberScene(canvas: UiCanvas, width: Float, textSize: Float): CanvasScene? {
    val context = LocalContext.current
    val density = LocalDensity.current
    val text = remember(density) { CanvasText.of(context, density) }
    val key = remember(canvas, width, textSize, text) { UiScenes.key(canvas, width, textSize, text) }
    var latest by remember { mutableStateOf<CanvasScene?>(null) }
    val kept = UiScenes.kept(key)
    LaunchedEffect(key) {
        if (kept == null) UiScenes.laidOut(key, canvas, width, textSize, text)?.let { latest = it }
    }
    return kept ?: latest
}
