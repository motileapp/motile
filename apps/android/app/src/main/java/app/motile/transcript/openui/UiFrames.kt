package app.motile.transcript.openui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.unit.dp
import app.motile.core.UiNode
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * Where each node of a program was laid out, for the frame dumps the clients are compared by: the
 * views record their frames here while one is given, with the layout they are drawn with.
 */
class UiFrames {
    private var root: LayoutCoordinates? = null
    private val nodes = LinkedHashMap<String, LayoutCoordinates>()

    fun root(coordinates: LayoutCoordinates) {
        root = coordinates
    }

    fun node(id: String, coordinates: LayoutCoordinates) {
        nodes[id] = coordinates
    }

    /** `{"<node id>": [x, y, width, height], …}` in points from the row's top left, while the views are on screen. */
    fun json(): String {
        val root = root ?: return "{}"
        val frames = nodes.filterValues { it.isAttached }.mapValues { (_, coordinates) ->
            val at = root.localPositionOf(coordinates, Offset.Zero)
            JsonArray(
                listOf(at.x, at.y, coordinates.size.width.toFloat(), coordinates.size.height.toFloat()).map { JsonPrimitive(it / scale) },
            )
        }
        return JsonObject(frames).toString()
    }

    /** Pixels in a point. */
    var scale = 1f
}

val LocalUiFrames = staticCompositionLocalOf<UiFrames?> { null }

/** Records the node's frame while frames are being taken. */
@Composable
fun Modifier.uiFrame(id: String): Modifier {
    val frames = LocalUiFrames.current ?: return this
    return onGloballyPositioned { frames.node(id, it) }
}

/**
 * Debug: lays the tree out `width` points wide as the transcript does, without a host, recording
 * every node's frame into `frames`, whose `json()` answers them once it is laid out.
 */
@Composable
fun UiFrameDump(tree: UiNode, width: Float, frames: UiFrames) {
    val density = androidx.compose.ui.platform.LocalDensity.current
    SideEffect { frames.scale = density.density }
    CompositionLocalProvider(LocalUiFrames provides frames) {
        Box(Modifier.width(width.dp).onGloballyPositioned { frames.root(it) }) {
            UiProgram(tree, width)
        }
    }
}
