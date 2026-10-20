package app.motile.transcript.openui

import androidx.compose.foundation.layout.requiredHeight
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.IntrinsicMeasurable
import androidx.compose.ui.layout.IntrinsicMeasureScope
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.Measurable
import androidx.compose.ui.layout.MeasurePolicy
import androidx.compose.ui.layout.MeasureResult
import androidx.compose.ui.layout.MeasureScope
import androidx.compose.ui.layout.Placeable
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import app.motile.core.UiAlign
import app.motile.core.UiBasis
import app.motile.core.UiJustify
import app.motile.core.UiLength
import app.motile.core.UiLengthUnit
import app.motile.core.UiSizing
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/*
 * The containers of LAYOUT.md's Layout section, as Compose layouts: a column whose children are
 * spaced as core says, a row whose children share its width by their sizing, and a grid. They
 * answer intrinsic sizes too, since a row that stretches its children, a grid and a carousel make
 * every child as tall as the tallest from them, and Compose measures a child only once.
 */

/** What lengths are measured against, besides the parent: the transcript's column and the window. */
class UiExtent(val column: Float, val window: Float)

/** A length in pixels, of `parent` for a fraction; none for one that can't be resolved here. */
fun Density.length(length: UiLength, parent: Float?, extent: UiExtent): Float? = when (length.unit) {
    UiLengthUnit.Points -> length.value * density
    UiLengthUnit.Fraction -> parent?.let { it * length.value }
    UiLengthUnit.Column -> extent.column * density * length.value
    UiLengthUnit.Window -> extent.window * density * length.value
}

private fun Constraints.childHeight(): Constraints =
    if (hasFixedHeight) Constraints(maxHeight = maxHeight) else Constraints()

/**
 * Children stacked, `spacing` above each, as wide as the column (`stretch`) or at their own width
 * at its start, centre or end. A child that `keeps` its width stays at its own width at the start
 * even in a stretching column. With a fixed height, the children that `grow` share what is left.
 */
@Composable
fun UiColumnLayout(
    spacing: List<Float>,
    align: UiAlign,
    keeps: List<Boolean>,
    modifier: Modifier = Modifier,
    grow: List<Float> = emptyList(),
    justify: UiJustify = UiJustify.Start,
    content: @Composable () -> Unit,
) {
    Layout(content, modifier, ColumnPolicy(spacing, align, keeps, grow, justify))
}

private class ColumnPolicy(
    val spacing: List<Float>,
    val align: UiAlign,
    val keeps: List<Boolean>,
    val grow: List<Float>,
    val justify: UiJustify,
) : MeasurePolicy {
    private fun Density.space(index: Int): Int = ((spacing.getOrNull(index) ?: 0f) * density).roundToInt()

    private fun stretches(index: Int) = align == UiAlign.Stretch && keeps.getOrNull(index) != true

    override fun MeasureScope.measure(measurables: List<Measurable>, constraints: Constraints): MeasureResult {
        val width = when {
            constraints.hasFixedWidth -> constraints.maxWidth
            else -> {
                val widest = measurables.maxOfOrNull { it.maxIntrinsicWidth(Constraints.Infinity) } ?: 0
                if (constraints.hasBoundedWidth) widest.coerceIn(constraints.minWidth, constraints.maxWidth) else max(widest, constraints.minWidth)
            }
        }
        val childHeight = constraints.childHeight()
        val weights = measurables.indices.map { grow.getOrNull(it) ?: 0f }
        val sharing = constraints.hasFixedHeight && weights.any { it > 0 }
        val placeables = arrayOfNulls<Placeable>(measurables.size)
        fun measure(index: Int, height: Int?) {
            val measurable = measurables[index]
            val loose = if (height != null) Constraints(maxWidth = width, minHeight = height, maxHeight = height) else childHeight.copy(maxWidth = width)
            placeables[index] = measurable.measure(if (stretches(index)) loose.copy(minWidth = width) else loose)
        }
        for (index in measurables.indices) if (!sharing || weights[index] <= 0) measure(index, null)
        val spaces = measurables.indices.sumOf { space(it) }
        if (sharing) {
            val left = max(0, constraints.maxHeight - spaces - placeables.sumOf { it?.height ?: 0 })
            val total = weights.sum()
            for (index in measurables.indices) if (weights[index] > 0) measure(index, (left * weights[index] / total).roundToInt())
        }
        val content = spaces + placeables.sumOf { it!!.height }
        val height = if (constraints.hasFixedHeight) constraints.maxHeight else max(content, constraints.minHeight)
        val free = max(0, height - content)
        var y = when (justify) {
            UiJustify.Center -> free / 2
            UiJustify.End -> free
            else -> 0
        }
        val between = if (justify == UiJustify.Between && measurables.size > 1) free / (measurables.size - 1) else 0
        return layout(width, height) {
            placeables.forEachIndexed { index, placeable ->
                placeable ?: return@forEachIndexed
                y += space(index) + if (index > 0) between else 0
                val x = when {
                    stretches(index) -> 0
                    align == UiAlign.Center -> (width - placeable.width) / 2
                    align == UiAlign.End -> width - placeable.width
                    else -> 0
                }
                placeable.place(x, y)
                y += placeable.height
            }
        }
    }

    override fun IntrinsicMeasureScope.maxIntrinsicWidth(measurables: List<IntrinsicMeasurable>, height: Int): Int =
        measurables.maxOfOrNull { it.maxIntrinsicWidth(Constraints.Infinity) } ?: 0

    override fun IntrinsicMeasureScope.minIntrinsicWidth(measurables: List<IntrinsicMeasurable>, height: Int): Int =
        measurables.maxOfOrNull { it.minIntrinsicWidth(Constraints.Infinity) } ?: 0

    override fun IntrinsicMeasureScope.maxIntrinsicHeight(measurables: List<IntrinsicMeasurable>, width: Int): Int =
        measurables.indices.sumOf { space(it) + measurables[it].maxIntrinsicHeight(width) }

    override fun IntrinsicMeasureScope.minIntrinsicHeight(measurables: List<IntrinsicMeasurable>, width: Int): Int =
        measurables.indices.sumOf { space(it) + measurables[it].minIntrinsicHeight(width) }
}

/** How a row shares its width: LAYOUT.md's base widths, growing and shrinking. */
object UiFlex {
    /** Each child's base width in a row `width` wide, from its widest single line. */
    fun bases(sizing: List<UiSizing>, count: Int, width: Float, widest: (Int) -> Float, length: (UiLength, Float) -> Float?): FloatArray =
        FloatArray(count) { index ->
            val size = sizing.getOrNull(index)
            when (size?.basis) {
                null, UiBasis.Content -> widest(index)
                UiBasis.Zero -> 0f
                UiBasis.Length -> size.length?.let { length(it, width) } ?: widest(index)
                UiBasis.Row -> width
                UiBasis.Media -> width / 3
            }
        }

    /** The lines of a wrapping row, or the one line of a row that doesn't wrap. */
    fun lines(bases: FloatArray, gap: Float, width: Float, wrap: Boolean): List<IntRange> {
        if (!wrap || bases.isEmpty()) return listOf(bases.indices)
        val lines = mutableListOf<IntRange>()
        var start = 0
        var used = bases[0]
        for (index in 1 until bases.size) {
            if (used + gap + bases[index] > width + 0.5f) {
                lines += start until index
                start = index
                used = bases[index]
            } else {
                used += gap + bases[index]
            }
        }
        lines += start until bases.size
        return lines
    }

    /** The widths of a line's children, and the free room `justify` places them in when nothing grows. */
    fun widths(bases: FloatArray, line: IntRange, sizing: List<UiSizing>, gap: Float, width: Float): Pair<FloatArray, Float> {
        val widths = FloatArray(line.count()) { bases[line.first + it] }
        val free = width - widths.sum() - gap * max(0, widths.size - 1)
        val grows = line.map { sizing.getOrNull(it)?.grow ?: 0f }
        if (free >= 0) {
            val weight = grows.sum()
            if (weight <= 0) return widths to free
            grows.forEachIndexed { index, grow -> widths[index] += free * grow / weight }
            return widths to 0f
        }
        val shrinking = line.mapIndexed { index, child -> if (sizing.getOrNull(child)?.shrink != false) widths[index] else 0f }
        val total = shrinking.sum()
        if (total <= 0) return widths to 0f
        shrinking.forEachIndexed { index, base -> widths[index] = max(0f, widths[index] + free * base / total) }
        return widths to 0f
    }
}

/**
 * Children side by side, `gap` apart, sharing the row's width as LAYOUT.md's Row says, aligned
 * across by `align` and placed along by `justify` when nothing grows. A `wrap` row goes on to
 * the next line when the next child's base width doesn't fit.
 */
@Composable
fun UiRowLayout(
    gap: Float,
    align: UiAlign,
    justify: UiJustify,
    wrap: Boolean,
    sizing: List<UiSizing>,
    extent: UiExtent,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    Layout(content, modifier, RowPolicy(gap, align, justify, wrap, sizing, extent))
}

private class RowPolicy(
    val gap: Float,
    val align: UiAlign,
    val justify: UiJustify,
    val wrap: Boolean,
    val sizing: List<UiSizing>,
    val extent: UiExtent,
) : MeasurePolicy {
    private class Plan(val width: Float, val lines: List<IntRange>, val widths: List<FloatArray>, val free: List<Float>)

    /** How the row's children share `available`, all of it when it `fills`, else at most what they need. */
    private fun Density.plan(measurables: List<IntrinsicMeasurable>, available: Float?, fills: Boolean = true): Plan {
        val gapPx = gap * density
        val widest = { index: Int -> measurables[index].maxIntrinsicWidth(Constraints.Infinity).toFloat() }
        val length = { size: UiLength, parent: Float -> length(size, parent, extent) }
        val media = { width: Float, bases: FloatArray ->
            sizing.forEachIndexed { index, size ->
                if (size.basis == UiBasis.Media && index < bases.size) bases[index] = (width / 3).coerceIn(120 * density, 200 * density)
            }
            bases
        }
        val natural = run {
            val bases = UiFlex.bases(sizing, measurables.size, Float.MAX_VALUE, widest, length)
            sizing.forEachIndexed { index, size ->
                if (index >= bases.size) return@forEachIndexed
                if (size.basis == UiBasis.Row || size.basis == UiBasis.Zero || (size.basis == UiBasis.Length && size.length?.unit == UiLengthUnit.Fraction)) bases[index] = widest(index)
                if (size.basis == UiBasis.Media) bases[index] = 200 * density
            }
            if (wrap) bases.maxOrNull() ?: 0f else bases.sum() + gapPx * max(0, bases.size - 1)
        }
        val width = if (available == null) natural else if (fills || wrap) available else min(available, max(natural, 0f))
        val bases = media(width, UiFlex.bases(sizing, measurables.size, width, widest, length))
        val lines = UiFlex.lines(bases, gapPx, width, wrap)
        val shared = lines.map { UiFlex.widths(bases, it, sizing, gapPx, width) }
        return Plan(width, lines, shared.map { it.first }, shared.map { it.second })
    }

    private fun Density.lineHeights(measurables: List<IntrinsicMeasurable>, plan: Plan, min: Boolean): List<Int> =
        plan.lines.mapIndexed { line, range ->
            range.maxOfOrNull { index ->
                val width = plan.widths[line][index - range.first].roundToInt()
                if (min) measurables[index].minIntrinsicHeight(width) else measurables[index].maxIntrinsicHeight(width)
            } ?: 0
        }

    override fun MeasureScope.measure(measurables: List<Measurable>, constraints: Constraints): MeasureResult {
        val available = constraints.maxWidth.toFloat().takeIf { constraints.hasBoundedWidth }
        val plan = plan(measurables, available, fills = constraints.hasFixedWidth)
        val width = if (constraints.hasFixedWidth) constraints.maxWidth else plan.width.roundToInt().coerceIn(constraints.minWidth, constraints.maxWidth)
        val fixedHeight = constraints.hasFixedHeight && plan.lines.size == 1
        val stretchHeights = if (align == UiAlign.Stretch) {
            if (fixedHeight) listOf(constraints.maxHeight) else lineHeights(measurables, plan, min = false)
        } else {
            null
        }
        val childHeight = constraints.childHeight()
        val placeables = arrayOfNulls<Placeable>(measurables.size)
        plan.lines.forEachIndexed { line, range ->
            for (index in range) {
                val childWidth = plan.widths[line][index - range.first].roundToInt()
                val height = stretchHeights?.get(line)
                placeables[index] = measurables[index].measure(
                    if (height != null) Constraints.fixed(childWidth, height) else childHeight.copy(minWidth = childWidth, maxWidth = childWidth),
                )
            }
        }
        val gapPx = (gap * density).roundToInt()
        val heights = plan.lines.map { range -> range.maxOfOrNull { placeables[it]!!.height } ?: 0 }
        val content = heights.sum() + gapPx * max(0, plan.lines.size - 1)
        val height = if (constraints.hasFixedHeight) constraints.maxHeight else max(content, constraints.minHeight)
        return layout(width, height) {
            var y = 0
            plan.lines.forEachIndexed { line, range ->
                val lineHeight = if (fixedHeight) height else heights[line]
                val free = max(0f, plan.free[line] + (width - plan.width)).roundToInt()
                var x = when (justify) {
                    UiJustify.Center -> free / 2
                    UiJustify.End -> free
                    else -> 0
                }
                val between = if (justify == UiJustify.Between && range.count() > 1) free / (range.count() - 1) else 0
                for (index in range) {
                    val placeable = placeables[index]!!
                    val top = when (align) {
                        UiAlign.Center -> (lineHeight - placeable.height) / 2
                        UiAlign.End -> lineHeight - placeable.height
                        else -> 0
                    }
                    placeable.place(x, y + top)
                    x += placeable.width + gapPx + between
                }
                y += lineHeight + gapPx
            }
        }
    }

    override fun IntrinsicMeasureScope.maxIntrinsicWidth(measurables: List<IntrinsicMeasurable>, height: Int): Int =
        plan(measurables, null).width.roundToInt()

    override fun IntrinsicMeasureScope.minIntrinsicWidth(measurables: List<IntrinsicMeasurable>, height: Int): Int {
        val mins = measurables.mapIndexed { index, measurable ->
            val size = sizing.getOrNull(index)
            when {
                size?.basis == UiBasis.Length && size.length?.unit == UiLengthUnit.Points -> size.length.value * density
                size?.shrink == false && size.grow <= 0 -> measurable.maxIntrinsicWidth(Constraints.Infinity).toFloat()
                else -> measurable.minIntrinsicWidth(Constraints.Infinity).toFloat()
            }
        }
        if (wrap) return (mins.maxOrNull() ?: 0f).roundToInt()
        return (mins.sum() + gap * density * max(0, mins.size - 1)).roundToInt()
    }

    override fun IntrinsicMeasureScope.maxIntrinsicHeight(measurables: List<IntrinsicMeasurable>, width: Int): Int {
        val plan = plan(measurables, width.toFloat().takeIf { width != Constraints.Infinity })
        return lineHeights(measurables, plan, min = false).sum() + (gap * density).roundToInt() * max(0, plan.lines.size - 1)
    }

    override fun IntrinsicMeasureScope.minIntrinsicHeight(measurables: List<IntrinsicMeasurable>, width: Int): Int {
        val plan = plan(measurables, width.toFloat().takeIf { width != Constraints.Infinity })
        return lineHeights(measurables, plan, min = true).sum() + (gap * density).roundToInt() * max(0, plan.lines.size - 1)
    }
}

/** How many columns of a grid fit `width`: LAYOUT.md's Grid formula. */
fun gridColumns(columns: Int, minChildWidth: Float?, gap: Float, width: Float): Int {
    val most = columns.coerceIn(1, 12)
    if (minChildWidth == null) return most
    if ((width - (most - 1) * gap) / most >= minChildWidth) return most
    val least = min(width, minChildWidth)
    return max(1, min(most, floor((width + gap) / (least + gap)).toInt()))
}

/** Cells in rows of `columns`, each row as tall as its tallest cell, every cell stretched to it. */
@Composable
fun UiGridLayout(columns: Int, minChildWidth: Float?, gap: Float, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Layout(content, modifier, GridPolicy(columns, minChildWidth, gap))
}

private class GridPolicy(val columns: Int, val minChildWidth: Float?, val gap: Float) : MeasurePolicy {
    private fun Density.cells(width: Int): Pair<Int, Float> {
        val count = gridColumns(columns, minChildWidth, gap, width / density)
        val gapPx = gap * density
        return count to (width - (count - 1) * gapPx) / count
    }

    private fun Density.heights(measurables: List<IntrinsicMeasurable>, count: Int, cell: Int): List<Int> =
        measurables.indices.chunked(count).map { row -> row.maxOf { measurables[it].maxIntrinsicHeight(cell) } }

    override fun MeasureScope.measure(measurables: List<Measurable>, constraints: Constraints): MeasureResult {
        val width = if (constraints.hasBoundedWidth) constraints.maxWidth else maxIntrinsicWidth(measurables, Constraints.Infinity)
        val (count, cell) = cells(width)
        val cellWidth = cell.roundToInt()
        val heights = heights(measurables, count, cellWidth)
        val gapPx = gap * density
        val placeables = measurables.mapIndexed { index, measurable -> measurable.measure(Constraints.fixed(cellWidth, heights[index / count])) }
        val height = heights.sum() + (gapPx * max(0, heights.size - 1)).roundToInt()
        return layout(width, max(height, constraints.minHeight)) {
            var y = 0f
            heights.forEachIndexed { row, rowHeight ->
                for (column in 0 until count) {
                    val placeable = placeables.getOrNull(row * count + column) ?: break
                    placeable.place((column * (cell + gapPx)).roundToInt(), y.roundToInt())
                }
                y += rowHeight + gapPx
            }
        }
    }

    override fun IntrinsicMeasureScope.maxIntrinsicWidth(measurables: List<IntrinsicMeasurable>, height: Int): Int {
        val widest = measurables.maxOfOrNull { it.maxIntrinsicWidth(Constraints.Infinity) } ?: 0
        val count = columns.coerceIn(1, 12)
        return (widest * count + gap * density * (count - 1)).roundToInt()
    }

    override fun IntrinsicMeasureScope.minIntrinsicWidth(measurables: List<IntrinsicMeasurable>, height: Int): Int =
        measurables.maxOfOrNull { it.minIntrinsicWidth(Constraints.Infinity) } ?: 0

    override fun IntrinsicMeasureScope.maxIntrinsicHeight(measurables: List<IntrinsicMeasurable>, width: Int): Int {
        val (count, cell) = cells(width)
        val heights = heights(measurables, count, cell.roundToInt())
        return heights.sum() + (gap * density * max(0, heights.size - 1)).roundToInt()
    }

    override fun IntrinsicMeasureScope.minIntrinsicHeight(measurables: List<IntrinsicMeasurable>, width: Int): Int =
        maxIntrinsicHeight(measurables, width)
}

/**
 * A box's own `width` and `height`: points, a share of what its parent offers, of the transcript's
 * column or of the window. A share of a height the parent doesn't fix is left out.
 */
fun Modifier.uiSize(width: UiLength?, height: UiLength?, extent: UiExtent, around: Float? = null): Modifier {
    if (width == null && height == null) return this
    // Lengths known here are Compose's own sizes, which a grid's intrinsic measuring reads too.
    val points = { length: UiLength, parent: Float? ->
        when (length.unit) {
            UiLengthUnit.Points -> length.value
            UiLengthUnit.Fraction -> parent?.let { it * length.value }
            UiLengthUnit.Column -> extent.column * length.value
            UiLengthUnit.Window -> extent.window * length.value
        }
    }
    val sized = height?.let { points(it, around) }?.let { requiredHeight(it.dp) } ?: this
    if (width == null) return sized
    points(width, null)?.let { return sized.width(it.dp) }
    return sized.layout { measurable, constraints ->
        val share = constraints.maxWidth.takeIf { constraints.hasBoundedWidth }?.toFloat()
        val w = share?.let { length(width, it, extent) }?.roundToInt()?.let { min(it, constraints.maxWidth) }
        val placeable = measurable.measure(if (w == null) constraints else constraints.copy(minWidth = w, maxWidth = w))
        layout(placeable.width, placeable.height) { placeable.place(0, 0) }
    }
}
