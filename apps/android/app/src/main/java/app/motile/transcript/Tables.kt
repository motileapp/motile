package app.motile.transcript

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.platform.Platform.scale
import app.motile.platform.scaled
import app.motile.theme.LocalPalette
import app.motile.ui.ContextMenuArea

/**
 * A table of a reply: its columns are as wide as their widest cell, up to a limit past which a
 * cell wraps, and it scrolls sideways when it is wider than the column. Held, it offers to copy it.
 */
@Composable
fun TableView(table: TableBlock, modifier: Modifier = Modifier) {
    val palette = LocalPalette.current
    /** Where each row ends, for the lines under them, as the last layout placed them. */
    val rowEnds = remember(table) { mutableListOf<Int>() }
    val style = TextStyle(lineHeight = (13f * scale * 1.2f + 3).sp)
    ContextMenuArea(items = { item("Copy Table", Symbol.Copy) { Platform.copy(table.words) } }, modifier = modifier) {
        Box(Modifier.horizontalScroll(rememberScrollState())) {
            Layout(
                content = {
                    for (row in table.cells) {
                        row.forEachIndexed { column, cell ->
                            val align = when (table.aligns.getOrElse(column) { 0 }) {
                                1 -> TextAlign.Center
                                2 -> TextAlign.End
                                else -> TextAlign.Start
                            }
                            MarkedText(cell, style.copy(textAlign = align))
                        }
                    }
                },
                modifier = Modifier.drawBehind {
                    for (end in rowEnds) drawRect(palette.border, Offset(0f, end - 1.dp.toPx()), Size(size.width, 1.dp.toPx()))
                },
            ) { measurables, _ ->
                val columns = table.cells.maxOfOrNull { it.size } ?: 0
                val side = TableView.sidePadding.roundToPx()
                val rowPadding = TableView.rowPadding.roundToPx()
                val line = 1.dp.roundToPx()
                val widest = scaled(240f).dp.roundToPx()
                val widths = IntArray(columns) { TableView.minColumn.roundToPx() }
                var index = 0
                for (row in table.cells) {
                    row.forEachIndexed { column, _ ->
                        val natural = measurables[index].maxIntrinsicWidth(Constraints.Infinity) + 2 * side + line
                        widths[column] = maxOf(widths[column], minOf(natural, widest + 2 * side))
                        index += 1
                    }
                }
                index = 0
                val placed = table.cells.map { row ->
                    row.mapIndexed { column, _ ->
                        measurables[index++].measure(Constraints.fixedWidth(maxOf(0, widths[column] - 2 * side)))
                    }
                }
                val heights = placed.map { row -> (row.maxOfOrNull { it.height } ?: 0) + 2 * rowPadding + line }
                rowEnds.clear()
                heights.runningReduce(Int::plus).forEach { rowEnds += it }
                layout(widths.sum(), heights.sum()) {
                    var y = 0
                    placed.forEachIndexed { row, cells ->
                        var x = 0
                        cells.forEachIndexed { column, cell ->
                            cell.place(x + side, y + rowPadding)
                            x += widths[column]
                        }
                        y += heights[row]
                    }
                }
            }
        }
    }
}

object TableView {
    val sidePadding = 10.dp
    val rowPadding = 7.dp
    val minColumn = 44.dp
}
