package app.motile.transcript.openui

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.AlignmentLine
import androidx.compose.ui.layout.FirstBaseline
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import app.motile.core.UiCanvas
import app.motile.core.UiNode
import app.motile.transcript.canvas.SceneView
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlin.math.roundToInt

/** A Chart, an Svg or a formula, painted from core's scene. */
@Composable
fun UiCanvasView(node: UiNode, canvas: UiCanvas) {
    when {
        canvas.kind != "Math" -> UiScene(node, canvas)
        ((canvas.props as? JsonObject)?.get("inline") as? JsonPrimitive)?.booleanOrNull == true -> UiInlineMath(node, canvas)
        else -> UiBlockMath(node, canvas)
    }
}

/** A chart or a drawing, laid out at the width the view is given: a chart fills it, a drawing is centred in it. */
@Composable
private fun UiScene(node: UiNode, canvas: UiCanvas) {
    val density = LocalDensity.current
    var width by remember { mutableFloatStateOf(0f) }
    val scene = if (width > 0) rememberScene(canvas, width, LocalUiTextSize.current) else null
    Box(Modifier.fillMaxWidth().onSizeChanged { width = it.width / density.density }, contentAlignment = Alignment.TopCenter) {
        scene?.let { SceneView(it, Modifier.uiNode(node)) }
    }
}

/** A formula on its own line: centred, 4 from what is around it, scrolling sideways when wider than the column. */
@Composable
private fun UiBlockMath(node: UiNode, canvas: UiCanvas) {
    val scene = rememberScene(canvas, LocalUiExtent.current.column, LocalUiTextSize.current)
    Box(Modifier.uiNode(node).fillMaxWidth().padding(vertical = 4.dp), contentAlignment = Alignment.Center) {
        Box(Modifier.horizontalScroll(rememberScrollState())) { scene?.let { SceneView(it) } }
    }
}

/** A formula in a text, in its colour, its baseline on the text's. */
@Composable
private fun UiInlineMath(node: UiNode, canvas: UiCanvas) {
    val scene = rememberScene(canvas, LocalUiExtent.current.column, LocalUiTextSize.current)
    val baseline = scene?.scene?.baseline
    Box(
        Modifier.uiNode(node).layout { measurable, constraints ->
            val placeable = measurable.measure(constraints)
            val lines = baseline?.let { mapOf<AlignmentLine, Int>(FirstBaseline to (it * density).roundToInt()) } ?: emptyMap()
            layout(placeable.width, placeable.height, lines) { placeable.place(0, 0) }
        },
    ) {
        scene?.let { SceneView(it) }
    }
}
