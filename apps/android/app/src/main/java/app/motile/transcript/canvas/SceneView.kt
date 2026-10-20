package app.motile.transcript.canvas

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.takeOrElse
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.layout.layout
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import app.motile.core.Native
import app.motile.core.json
import app.motile.theme.LocalDark
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Radius
import app.motile.theme.Theme
import app.motile.ui.DropdownCard
import app.motile.ui.Text
import app.motile.ui.lined
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.roundToInt
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonObject

/** Lays canvas components out through the core: Chart, Svg, Math, Celebration and Flashcard. */
object Canvases {
    /** Core's scene for the component at a column `width`, ready to paint. Blocks: call it off the main thread. */
    fun layout(kind: String, props: JsonObject, width: Float, textSize: Float, text: CanvasText): CanvasScene? {
        val input = json("kind" to kind, "props" to props, "width" to width, "text_size" to textSize).toString()
        val scene = Scene.parse(Native.uiLayout(input)) ?: return null
        return CanvasScene(scene, text)
    }

    suspend fun laidOut(kind: String, props: JsonObject, width: Float, textSize: Float, text: CanvasText): CanvasScene? =
        withContext(Dispatchers.Default) { layout(kind, props, width, textSize, text) }
}

/** The text styles of the legend and the tooltip, as LAYOUT.md names them. */
private object CanvasType {
    val caption: TextStyle get() = Theme.smallFont.lined(18f)
    val captionTitle: TextStyle get() = Theme.ui(12f, FontWeight.SemiBold).lined(18f)
    val readout: TextStyle get() = Theme.uiDigits(12f, FontWeight.Medium).lined(18f)
    val small: TextStyle get() = Theme.ui(13f).lined(20f)
    val share: TextStyle get() = Theme.uiDigits(13f, FontWeight.Medium).lined(20f)
}

private object Legend {
    val dot = 8.dp
    val chartTop = 10.dp
    val chartDotGap = 7.dp
    val chartItemGap = 16.dp
    val chartLineGap = 6.dp
    val pieGap = 24.dp
    val pieRowGap = 9.dp
    val pieDotGap = 8.dp
    val pieShareGap = 8.dp
}

/**
 * How tall the view of a scene is in a column `width` wide: the scene's own height, and its legend's
 * under it or beside it. Measures off the main thread, as the view lays the legend out.
 */
fun sceneHeight(scene: Scene, width: Float, text: CanvasText): Float = with(text.density) {
    val legend = scene.legend ?: return scene.height
    val widthPx = (width * density).roundToInt()
    when {
        !legend.isPie -> scene.height + (Legend.chartTop.roundToPx() + chartLegendHeight(legend, widthPx, text)) / density
        !legend.beside -> scene.height + (Legend.pieGap.roundToPx() + pieLegendHeight(legend, widthPx, text)) / density
        else -> {
            val column = widthPx - scene.width.dp.roundToPx() - Legend.pieGap.roundToPx()
            max(scene.height, pieLegendHeight(legend, column, text) / density)
        }
    }
}

private fun chartLegendHeight(legend: SceneLegend, width: Int, text: CanvasText): Int = with(text.density) {
    val dot = Legend.dot.roundToPx()
    val items = legend.entries.map { text.measure(it.label, CanvasType.caption, wraps = false).size }
    val gap = Legend.chartItemGap.roundToPx()
    var lines = 0
    var height = 0
    var lineWidth = 0
    var lineHeight = 0
    for (size in items) {
        val itemWidth = dot + Legend.chartDotGap.roundToPx() + size.width
        val itemHeight = max(dot, size.height)
        if (lines > 0 && lineWidth + gap + itemWidth <= width) {
            lineWidth += gap + itemWidth
            lineHeight = max(lineHeight, itemHeight)
            continue
        }
        height += lineHeight
        lines += 1
        lineWidth = itemWidth
        lineHeight = itemHeight
    }
    height + lineHeight + (lines - 1).coerceAtLeast(0) * Legend.chartLineGap.roundToPx()
}

private fun pieLegendHeight(legend: SceneLegend, width: Int, text: CanvasText): Int = with(text.density) {
    val dot = Legend.dot.roundToPx()
    val rows = legend.entries.map { entry ->
        val share = text.measure(entry.share.orEmpty(), CanvasType.share, wraps = false).size
        val room = width - dot - Legend.pieDotGap.roundToPx() - Legend.pieShareGap.roundToPx() - share.width
        val label = text.measure(entry.label, CanvasType.small, room.coerceAtLeast(0)).size
        maxOf(pieDotTop() + dot, label.height, share.height)
    }
    rows.sum() + (rows.size - 1).coerceAtLeast(0) * Legend.pieRowGap.roundToPx()
}

/** A pie legend's dot stands centred on the first line of its label. */
private fun Density.pieDotTop(): Int = ((CanvasType.small.lineHeight.toPx() - Legend.dot.roundToPx()) / 2f).roundToInt()

/**
 * A canvas scene, laid out at the view's density, on the surface it lies in, with its legend:
 * `canvas.scene`'s size without one, the column's width with one, `sceneHeight` tall. A tap lights
 * the hit under it and shows its tooltip, a sideways drag moves it along, a tap elsewhere hides it;
 * an upward or downward drag is the transcript's. `current` is the text's colour around it.
 */
@Composable
fun SceneView(canvas: CanvasScene, modifier: Modifier = Modifier, current: Color = LocalForeground.current) {
    val palette = LocalPalette.current
    val colors = SceneColors(palette, LocalDark.current, LocalSurface.current, current.takeOrElse { palette.foreground })
    val scene = canvas.scene
    val described = scene.label?.let { label -> modifier.clearAndSetSemantics { contentDescription = label; role = Role.Image } } ?: modifier
    val legend = scene.legend
    when {
        legend == null -> ScenePlot(canvas, colors, described)
        legend.isPie && legend.beside -> Row(described.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            ScenePlot(canvas, colors)
            Spacer(Modifier.width(Legend.pieGap))
            PieLegend(legend, colors, Modifier.weight(1f))
        }
        legend.isPie -> Column(described.fillMaxWidth()) {
            ScenePlot(canvas, colors)
            PieLegend(legend, colors, Modifier.fillMaxWidth().padding(top = Legend.pieGap))
        }
        else -> Column(described.fillMaxWidth()) {
            ScenePlot(canvas, colors)
            ChartLegend(legend, colors, Modifier.fillMaxWidth().padding(top = Legend.chartTop))
        }
    }
}

@Composable
private fun ScenePlot(canvas: CanvasScene, colors: SceneColors, modifier: Modifier = Modifier) {
    val scene = canvas.scene
    var lit by remember(canvas) { mutableStateOf<Int?>(null) }
    Box(modifier.size(scene.width.dp, scene.height.dp)) {
        Canvas(
            Modifier
                .size(scene.width.dp, scene.height.dp)
                .then(if (scene.hits.isEmpty()) Modifier else Modifier.pointerInput(canvas) {
                    fun at(position: Offset) = scene.hit(position.x / density, position.y / density)
                    awaitEachGesture {
                        val down = awaitFirstDown(requireUnconsumed = false)
                        val slop = viewConfiguration.touchSlop
                        var moved = Offset.Zero
                        var dragging = false
                        while (true) {
                            val change = awaitPointerEvent().changes.firstOrNull { it.id == down.id } ?: return@awaitEachGesture
                            if (!change.pressed) {
                                if (!dragging) lit = at(change.position)
                                return@awaitEachGesture
                            }
                            if (dragging) {
                                change.consume()
                                lit = at(change.position) ?: lit
                                continue
                            }
                            if (change.isConsumed) return@awaitEachGesture
                            moved += change.positionChange()
                            if (abs(moved.x) > slop && abs(moved.x) >= abs(moved.y)) {
                                dragging = true
                                change.consume()
                                lit = at(change.position) ?: lit
                            } else if (abs(moved.y) > slop) {
                                return@awaitEachGesture
                            }
                        }
                    }
                }),
        ) { paintScene(canvas, colors, lit) }
        lit?.let { scene.hits.getOrNull(it) }?.let { Tooltip(it.tip, colors) }
    }
}

/** A hit's tooltip, in the popover's look, centred on its x with its top at its y, over the scene and beyond it, taking no presses. */
@Composable
private fun Tooltip(tip: SceneHit.Tip, colors: SceneColors) {
    val palette = colors.palette
    DropdownCard(
        Modifier
            .zIndex(1f)
            .layout { measurable, _ ->
                val placeable = measurable.measure(Constraints())
                layout(0, 0) { placeable.place((tip.x.dp.toPx() - placeable.width / 2f).roundToInt(), tip.y.dp.roundToPx()) }
            }
            .widthIn(min = 110.dp)
            .width(IntrinsicSize.Max),
        radius = Radius.lg,
    ) {
        Column(Modifier.fillMaxWidth().padding(vertical = 10.dp, horizontal = 12.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(tip.title, style = CanvasType.captionTitle, color = palette.foreground)
            for (row in tip.rows) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    row.color?.let { color ->
                        Dot(colors.of(color))
                        Spacer(Modifier.width(6.dp))
                    }
                    Text(row.name, style = CanvasType.caption, color = palette.mutedForeground)
                    Spacer(Modifier.weight(1f).widthIn(min = 8.dp))
                    Text(row.value, style = CanvasType.readout, color = palette.foreground)
                }
            }
        }
    }
}

@Composable
private fun Dot(color: Color, modifier: Modifier = Modifier) {
    Box(modifier.size(Legend.dot).background(color, CircleShape))
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ChartLegend(legend: SceneLegend, colors: SceneColors, modifier: Modifier) {
    FlowRow(
        modifier,
        horizontalArrangement = Arrangement.spacedBy(Legend.chartItemGap),
        verticalArrangement = Arrangement.spacedBy(Legend.chartLineGap),
    ) {
        for (entry in legend.entries) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Dot(colors.of(entry.color))
                Spacer(Modifier.width(Legend.chartDotGap))
                Text(entry.label, style = CanvasType.caption, color = colors.palette.mutedForeground, maxLines = 1)
            }
        }
    }
}

@Composable
private fun PieLegend(legend: SceneLegend, colors: SceneColors, modifier: Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(Legend.pieRowGap)) {
        for (entry in legend.entries) {
            Row {
                Box(Modifier.layout { measurable, constraints ->
                    val placeable = measurable.measure(constraints)
                    val top = pieDotTop()
                    layout(placeable.width, top + placeable.height) { placeable.place(0, top) }
                }) { Dot(colors.of(entry.color)) }
                Spacer(Modifier.width(Legend.pieDotGap))
                Text(entry.label, Modifier.weight(1f), style = CanvasType.small, color = colors.palette.foreground)
                Spacer(Modifier.width(Legend.pieShareGap))
                Text(entry.share.orEmpty(), style = CanvasType.share, color = colors.palette.foreground, maxLines = 1)
            }
        }
    }
}
