package app.motile.usage

import android.text.format.DateFormat
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import app.motile.core.UsageModel
import app.motile.core.UsageReport
import app.motile.theme.LocalPalette
import app.motile.theme.Opacity
import app.motile.theme.at
import app.motile.ui.color
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.log10
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlin.math.sign

private val chartHeight = 190.dp
/** The room the first and the last point keep from the plot's sides. */
private val edgeRoom = 16.dp
private val labelGap = 6.dp

/**
 * A line for each agent over time, with the area under it tinted, unstacked as Swift Charts draws
 * them, over four grid lines with their amounts at their end and five times under them. A finger
 * on it points at the nearest time, which the legend then reads.
 */
@Composable
fun SpendingChart(report: UsageReport, measure: Measure, period: UsageModel.Period, pointed: Int?, point: (Int?) -> Unit) {
    val palette = LocalPalette.current
    val density = LocalDensity.current
    val measurer = rememberTextMeasurer()
    val labelStyle = app.motile.theme.Theme.uiDigits(11f).copy(color = palette.mutedForeground)
    val series = report.agents.map { it.agent.color(palette) to it.points(measure) }
    val fills = report.agents.map { it.agent.color(palette).at(Opacity.ColorTintChart) }
    val highest = series.maxOfOrNull { (_, points) -> points.maxOrNull() ?: 0.0 } ?: 0.0
    val ticks = remember(highest) { niceTicks(highest, 4) }
    val yLabels = ticks.map { measurer.measure(amount(it, measure), labelStyle) }
    val count = report.starts.size
    val xIndices = remember(count) { timeTicks(count, 5) }
    val xLabels = xIndices.map { measurer.measure(axisTime(report.starts[it], period), labelStyle) }
    val labelWidth = yLabels.maxOfOrNull { it.size.width } ?: 0
    val gap = with(density) { labelGap.toPx() }
    val room = with(density) { edgeRoom.toPx() }
    val plotRight = { width: Float -> width - labelWidth - gap }
    val xOf = { index: Int, width: Float ->
        val span = plotRight(width) - room * 2
        if (count <= 1) plotRight(width) / 2 else room + span * index / (count - 1)
    }
    Canvas(
        Modifier
            .fillMaxWidth()
            .height(chartHeight)
            .pointerInput(count, labelWidth) {
                fun nearest(x: Float): Int? {
                    if (count == 0) return null
                    val width = size.width.toFloat()
                    return (0 until count).minByOrNull { abs(xOf(it, width) - x) }
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
        val right = plotRight(size.width)
        val ceiling = ticks.last().takeIf { it > 0 } ?: 1.0
        val yOf = { value: Double -> (bottom - (bottom - top) * (value / ceiling)).toFloat() }

        ticks.forEachIndexed { index, tick ->
            val y = yOf(tick)
            drawLine(palette.border, Offset(0f, y), Offset(right, y), strokeWidth = 1.dp.toPx())
            val label = yLabels[index]
            drawText(label, topLeft = Offset(right + gap, y - label.size.height / 2f))
        }
        xIndices.forEachIndexed { index, at ->
            val label = xLabels[index]
            val x = (xOf(at, size.width) - label.size.width / 2f).coerceIn(0f, max(0f, right - label.size.width))
            drawText(label, topLeft = Offset(x, bottom + gap))
        }
        series.forEachIndexed { index, (color, points) ->
            if (points.isEmpty()) return@forEachIndexed
            val offsets = (0 until count).map { Offset(xOf(it, size.width), yOf(points.getOrElse(it) { 0.0 })) }
            val line = monotone(offsets)
            val area = Path().apply {
                addPath(line)
                lineTo(offsets.last().x, bottom)
                lineTo(offsets.first().x, bottom)
                close()
            }
            drawPath(area, fills[index])
            drawPath(line, color, style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round))
        }
        if (pointed != null && pointed in 0 until count) {
            val x = xOf(pointed, size.width)
            drawLine(palette.border, Offset(x, top), Offset(x, bottom), strokeWidth = 1.dp.toPx())
        }
    }
}

fun UsageReport.Series.points(measure: Measure): List<Double> = if (measure == Measure.Cost) costPoints else tokenPoints

/** About `desired` steps from nothing to past `highest`, each 1, 2 or 5 of a power of ten. */
private fun niceTicks(highest: Double, desired: Int): List<Double> {
    if (highest <= 0) return listOf(0.0)
    val rough = highest / desired
    val magnitude = 10.0.pow(floor(log10(rough)))
    val step = listOf(1.0, 2.0, 5.0, 10.0).map { it * magnitude }.first { it >= rough }
    val steps = ceil(highest / step - 1e-9).toInt()
    return (0..steps).map { it * step }
}

/** The buckets labelled under the chart: about `desired`, evenly apart from the first. */
private fun timeTicks(count: Int, desired: Int): List<Int> {
    if (count == 0) return emptyList()
    val stride = max(1, ceil(count / desired.toDouble()).toInt())
    return (0 until count step stride).toList()
}

/** Through every point, curving without passing above or below them, as Swift Charts' monotone interpolation. */
private fun monotone(points: List<Offset>): Path {
    val path = Path()
    if (points.isEmpty()) return path
    path.moveTo(points[0].x, points[0].y)
    if (points.size == 1) return path
    if (points.size == 2) {
        path.lineTo(points[1].x, points[1].y)
        return path
    }
    val n = points.size
    val widths = FloatArray(n - 1) { points[it + 1].x - points[it].x }
    val slopes = FloatArray(n - 1) { if (widths[it] == 0f) 0f else (points[it + 1].y - points[it].y) / widths[it] }
    val tangents = FloatArray(n)
    for (i in 1 until n - 1) {
        val (h0, h1) = widths[i - 1] to widths[i]
        val (s0, s1) = slopes[i - 1] to slopes[i]
        val p = if (h0 + h1 == 0f) 0f else (s0 * h1 + s1 * h0) / (h0 + h1)
        tangents[i] = (sign(s0) + sign(s1)) * min(min(abs(s0), abs(s1)), 0.5f * abs(p))
    }
    tangents[0] = (3 * slopes[0] - tangents[1]) / 2
    tangents[n - 1] = (3 * slopes[n - 2] - tangents[n - 2]) / 2
    for (i in 0 until n - 1) {
        val third = widths[i] / 3
        val (a, b) = points[i] to points[i + 1]
        path.cubicTo(a.x + third, a.y + third * tangents[i], b.x - third, b.y - third * tangents[i + 1], b.x, b.y)
    }
    return path
}

/** What a measure of `value` reads as: dollars, or tokens counted short. */
fun amount(value: Double, measure: Measure): String = if (measure == Measure.Cost) cost(value) else count(value.roundToInt().toLong())

private fun formatted(seconds: Double, skeleton: String): String {
    val locale = Locale.getDefault()
    val pattern = DateFormat.getBestDateTimePattern(locale, skeleton)
    return SimpleDateFormat(pattern, locale).format(Date((seconds * 1000).toLong()))
}

/** A time under the chart: the hour over a day, the day otherwise. */
private fun axisTime(seconds: Double, period: UsageModel.Period): String = formatted(seconds, if (period == UsageModel.Period.Day) "j" else "MMMd")

/** The time pointed at, in the legend: with its weekday. */
fun legendTime(seconds: Double, period: UsageModel.Period): String = formatted(seconds, if (period == UsageModel.Period.Day) "EEEj" else "EEEMMMd")
