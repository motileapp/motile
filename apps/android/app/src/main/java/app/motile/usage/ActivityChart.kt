package app.motile.usage

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import app.motile.core.UsageModel
import app.motile.theme.LocalDark
import app.motile.theme.LocalPalette
import app.motile.theme.Opacity
import app.motile.theme.TextSize
import app.motile.theme.Theme
import app.motile.theme.at
import kotlin.math.max

private val chartHeight = 190.dp
private val labelGap = 6.dp

/**
 * A bar for each bucket, stacked from each source's part, the first at the bottom, over four grid
 * lines with their amounts at their end and five times under them. A finger on it points at the
 * nearest bucket, which the legend then reads, and dims the others.
 */
@Composable
fun ActivityChart(
    starts: List<Double>,
    series: List<Pair<Color, List<Double>>>,
    period: UsageModel.Period,
    pointed: Int?,
    amount: (Double) -> String,
    point: (Int?) -> Unit,
) {
    val palette = LocalPalette.current
    val dark = LocalDark.current
    val density = LocalDensity.current
    val measurer = rememberTextMeasurer()
    val labelStyle = Theme.uiDigits(TextSize.Xs).copy(color = palette.mutedForeground)
    val count = starts.size
    val highest = (0 until count).maxOfOrNull { index -> series.sumOf { (_, points) -> points.getOrElse(index) { 0.0 } } } ?: 0.0
    val ticks = remember(highest) { niceTicks(highest, 4) }
    val yLabels = ticks.map { measurer.measure(amount(it), labelStyle) }
    val xIndices = remember(count) { timeTicks(count, 5) }
    val xLabels = xIndices.map { measurer.measure(axisTime(starts[it], period), labelStyle) }
    val labelWidth = yLabels.maxOfOrNull { it.size.width } ?: 0
    val gap = with(density) { labelGap.toPx() }
    val slotOf = { width: Float -> (width - labelWidth - gap) / max(count, 1) }
    Canvas(
        Modifier
            .fillMaxWidth()
            .height(chartHeight)
            .pointerInput(count, labelWidth) {
                fun nearest(x: Float): Int? {
                    if (count == 0) return null
                    return (x / slotOf(size.width.toFloat())).toInt().coerceIn(0, count - 1)
                }
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    point(nearest(down.position.x))
                    while (true) {
                        val change = awaitPointerEvent().changes.firstOrNull() ?: break
                        if (!change.pressed) break
                        point(nearest(change.position.x))
                    }
                    point(null)
                }
            },
    ) {
        val xLabelHeight = xLabels.maxOfOrNull { it.size.height } ?: 0
        val top = (yLabels.firstOrNull()?.size?.height ?: 0) / 2f
        val bottom = size.height - xLabelHeight - gap
        val right = size.width - labelWidth - gap
        val slot = slotOf(size.width)
        val bar = max(1f, slot * 0.7f)
        val ceiling = ticks.last().takeIf { it > 0 } ?: 1.0
        val heightOf = { value: Double -> ((bottom - top) * (value / ceiling)).toFloat() }
        val corner = CornerRadius(2.dp.toPx())

        ticks.forEachIndexed { index, tick ->
            val y = bottom - heightOf(tick)
            drawLine(palette.border, Offset(0f, y), Offset(right, y), strokeWidth = 1.dp.toPx())
            val label = yLabels[index]
            drawText(label, topLeft = Offset(right + gap, y - label.size.height / 2f))
        }
        xIndices.forEachIndexed { index, at ->
            val label = xLabels[index]
            val x = (slot * (at + 0.5f) - label.size.width / 2f).coerceIn(0f, max(0f, right - label.size.width))
            drawText(label, topLeft = Offset(x, bottom + gap))
        }
        for (index in 0 until count) {
            val dimmed = pointed != null && pointed != index
            val left = slot * (index + 0.5f) - bar / 2
            var base = bottom
            for ((color, points) in series) {
                val height = heightOf(points.getOrElse(index) { 0.0 })
                if (height <= 0f) continue
                drawRoundRect(
                    if (dimmed) color.at(Opacity.Disabled, dark) else color, Offset(left, base - height), Size(bar, height), corner,
                )
                base -= height
            }
        }
    }
}
