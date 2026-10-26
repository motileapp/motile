package app.motile.composer

import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.BaselineShift
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.motile.R
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.IconSize
import app.motile.theme.Opacity
import app.motile.theme.Palette
import app.motile.theme.Radius
import app.motile.theme.at
import app.motile.theme.symbolSide

/**
 * A skill a message starts, drawn as a chip in the composer: the skill's symbol and its name in the
 * skill colour, on its wash. In the text it stays its `$name`; only what is shown is the chip.
 */
object SkillChip {
    val symbol = Symbol.Box
    private const val textSize = 13f
    /** Smaller than the icon for its text, to keep clear of the box's edges, which hug the text. */
    private val symbolSize = IconSize.Sm
    val radius = Radius.sm
    /** The room between the chip's box and the line's top and bottom. */
    private val lineInset = 2.dp

    private val lucide = FontFamily(Font(R.font.lucide))
    private const val narrowSpace = ' '
    private const val noBreakSpace = ' '

    /**
     * What the chip shows in place of its `$name`: the room before its symbol, the symbol, the name
     * and the room after it, on one line.
     */
    fun shown(label: String): String =
        "$narrowSpace${symbol.character ?: ""}$noBreakSpace${label.replace(' ', noBreakSpace)}$narrowSpace"

    /** The style of what the chip shows, and of its symbol, the second character of it. */
    fun styles(palette: Palette): Pair<SpanStyle, SpanStyle> {
        val text = SpanStyle(color = palette.skill, fontSize = (textSize * Platform.scale).sp, fontWeight = FontWeight.Normal)
        val mark = SpanStyle(fontFamily = lucide, fontSize = symbolSide(symbolSize).value.sp, baselineShift = BaselineShift(-0.12f))
        return text to mark
    }

    /** Draws the chip's box behind what it shows, from `start` to `end` of the shown text, on each line it lies on. */
    fun DrawScope.drawBox(layout: TextLayoutResult, start: Int, end: Int, scrolled: Float, palette: Palette, dark: Boolean) {
        if (end <= start || end > layout.layoutInput.text.length) return
        val wash = palette.skill.at(Opacity.ColorTint, dark)
        val edge = palette.skill.at(Opacity.ColorTintChart, dark)
        val inset = lineInset.toPx()
        val corner = CornerRadius(radius.toPx())
        val firstLine = layout.getLineForOffset(start)
        val lastLine = layout.getLineForOffset(end - 1)
        for (line in firstLine..lastLine) {
            val left = if (line == firstLine) layout.getBoundingBox(start).left else layout.getLineLeft(line)
            val right = if (line == lastLine) layout.getBoundingBox(end - 1).right else layout.getLineRight(line)
            val top = layout.getLineTop(line) + inset - scrolled
            val bottom = layout.getLineBottom(line) - inset - scrolled
            if (right <= left || bottom <= top) continue
            val half = 0.5.dp.toPx()
            val origin = Offset(left + half, top + half)
            val size = Size(right - left - half * 2, bottom - top - half * 2)
            drawRoundRect(wash, origin, size, corner)
            drawRoundRect(edge, origin, size, corner, style = Stroke(1.dp.toPx()))
        }
    }
}

/** A skill's `$name` in the composer's text that shows as a chip, where it is in the text as written. */
data class ChipRange(val start: Int, val written: String, val name: String, val label: String) {
    val end: Int get() = start + written.length

    fun holds(text: CharSequence): Boolean = end <= text.length && text.regionMatches(start, written, 0, written.length, ignoreCase = false)
}

/**
 * The word that ends at `cursor` when it starts with `$` or `/`, which a skill picked from the menu
 * replaces: what is typed now, which the menu may not have caught up with.
 */
fun skillWord(text: String, cursor: Int): IntRange? {
    if (cursor <= 0 || cursor > text.length) return null
    val space = text.substring(0, cursor).indexOfLast { it.isWhitespace() }
    val start = space + 1
    if (start >= cursor || text[start] !in "$/") return null
    return start until cursor
}
