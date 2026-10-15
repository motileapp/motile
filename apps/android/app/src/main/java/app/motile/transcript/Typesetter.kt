package app.motile.transcript

import app.motile.composer.SkillChip

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp
import app.motile.core.JSON
import app.motile.core.string
import app.motile.core.objects
import app.motile.platform.Platform
import app.motile.theme.Palette
import app.motile.theme.Theme
import app.motile.theme.symbolSide

/** A block of a prose row, drawn as a text of its own. */
sealed interface ProseBlock {
    /** The room above it. */
    val top: Float
    /** How many quote bars stand beside it. */
    val quote: Int
    /** The bars go on through the room above it, from the quoted block above. */
    val joined: Boolean
}

/** A paragraph, a heading, an item of a list or a quoted paragraph. A list item's marker stands in the room before its text. */
class TextBlock(
    val text: AnnotatedString,
    val lineHeight: TextUnit,
    val indent: Float,
    val marker: AnnotatedString?,
    val markerIndent: Float,
    override val top: Float,
    override val quote: Int,
    override val joined: Boolean,
) : ProseBlock

/** Code inside a list or a quote, in a box of its own. */
class CodeBlock(
    val language: String,
    val code: String,
    val text: AnnotatedString,
    val indent: Float,
    override val top: Float,
    override val quote: Int,
    override val joined: Boolean,
) : ProseBlock

class RuleBlock(override val top: Float) : ProseBlock {
    override val quote: Int get() = 0
    override val joined: Boolean get() = false
}

/** A table: its cells by row, each column's alignment, and how many rows head it. */
class TableBlock(
    val cells: List<List<AnnotatedString>>,
    val aligns: List<Int>,
    val headers: Int,
    /** The table as it is copied: its rows as lines, its cells apart by tabs. */
    val words: String,
    override val top: Float,
) : ProseBlock {
    override val quote: Int get() = 0
    override val joined: Boolean get() = false
}

/** Turns the core's rows into typeset text. Safe to call from any thread. */
object Typesetter {
    /** What a paragraph, a list, a quote and a table keep from what follows them. */
    private const val BLOCK_GAP = 12f
    private const val HEADING_GAP = 14f
    private const val BODY_LINE_SPACING = 7f
    /** The room a prose row has around its text, which is what keeps two rows apart. */
    const val PROSE_GAP = 9f
    /** What a tool row keeps from a paragraph above it, so that the two are a paragraph apart. */
    const val AFTER_PROSE = 8f
    private const val LIST_INDENT = 22f
    private const val QUOTE_INDENT = 14f

    /**
     * A reply is cut into rows, and its rows are as far apart as its paragraphs: a paragraph after
     * another, and a tool row after a paragraph or before one.
     */
    fun spaceAbove(json: JSON): Float {
        val after = json.string("after")
        if (after.isEmpty()) return 0f
        if (json.string("kind") != "prose") return if (after == "prose" || after == "media") AFTER_PROSE else 0f
        val heading = if (json.objects("paras").firstOrNull()?.string("kind") == "heading") HEADING_GAP else 0f
        return when (after) {
            "prose", "media", "work" -> BODY_LINE_SPACING + BLOCK_GAP - PROSE_GAP + heading
            "table" -> BLOCK_GAP - PROSE_GAP + heading
            else -> heading
        }
    }

    private fun sized(size: Float) = (size * Platform.scale).sp

    /** A message's line height: its text with five points between its lines. */
    val messageLineHeight: TextUnit get() = (14f * Platform.scale * 1.2f + 5f).sp

    /** A message's text with its links, and its skills' `$name`s as chips that the transcript draws behind their names. */
    fun message(message: MessageText, color: androidx.compose.ui.graphics.Color, palette: Palette): AnnotatedString {
        val text = message.text
        val chips = message.skills.filter { it.start >= 0 && it.length > 0 && it.end <= text.length }.sortedBy { it.start }
        val builder = AnnotatedString.Builder()
        builder.pushStyle(SpanStyle(color = color))
        /** Where a place in the message's text went: past every chip before it, which is longer than its `$name`. */
        val moves = mutableListOf<Pair<Int, Int>>()
        var at = 0
        // A chip shows as the composer shows it: its symbol and its name, in the skill colour.
        val (chipStyle, symbolStyle) = SkillChip.styles(palette)
        for (chip in chips) {
            if (chip.start < at) continue
            builder.append(text.substring(at, chip.start))
            val start = builder.length
            val shown = SkillChip.shown(chip.label)
            builder.pushStyle(chipStyle)
            builder.append(shown)
            builder.pop()
            builder.addStyle(symbolStyle, start + 1, start + 2)
            builder.addStringAnnotation(TextMarks.skill, chip.name, start, builder.length)
            moves += chip.end to (shown.length - chip.length)
            at = chip.end
        }
        builder.append(text.substring(at))
        builder.pop()
        fun moved(index: Int) = index + moves.filter { it.first <= index }.sumOf { it.second }
        for (link in message.links) {
            val end = link.start + link.length
            if (link.start < 0 || link.length <= 0 || end > text.length) continue
            if (chips.any { it.start < end && link.start < it.end }) continue
            addLink(builder, link.url, moved(link.start), moved(end), palette)
        }
        return builder.toAnnotatedString()
    }

    private fun addLink(builder: AnnotatedString.Builder, url: String, start: Int, end: Int, palette: Palette) {
        val style = SpanStyle(color = palette.primary)
        builder.addStyle(style, start, end)
        builder.addLink(LinkAnnotation.Url(url, TextLinkStyles(style)) { Platform.open(url) }, start, end)
    }

    /** Code in the colours of its highlighting. */
    fun code(code: String, spans: IntArray, palette: Palette): AnnotatedString {
        val builder = AnnotatedString.Builder(code)
        builder.addStyle(SpanStyle(color = palette.foreground), 0, code.length)
        colour(builder, spans, 0, code.length, palette)
        return builder.toAnnotatedString()
    }

    /** Colours the code between `start` and `end` with its spans, which count from `start`. */
    private fun colour(builder: AnnotatedString.Builder, spans: IntArray, start: Int, end: Int, palette: Palette) {
        val syntax = Theme.syntax(palette)
        var index = 0
        while (index + 2 < spans.size) {
            val from = start + spans[index]
            val to = from + spans[index + 1]
            val color = spans[index + 2]
            index += 3
            if (from < start || to > end || from >= to || color <= 0 || color >= syntax.size) continue
            builder.addStyle(SpanStyle(color = syntax[color]), from, to)
        }
    }

    /** What opening a tool row shows: its input, removed lines red and added lines green in a diff, then what came back. */
    fun toolDetail(tool: ToolContent, palette: Palette): AnnotatedString {
        val builder = AnnotatedString.Builder()
        if (tool.input.isNotEmpty()) {
            if (tool.inputLanguage == "diff") {
                val syntax = Theme.syntax(palette)
                tool.input.split("\n").forEachIndexed { index, line ->
                    val color = when {
                        line.startsWith("+") -> syntax[10]
                        line.startsWith("-") -> syntax[11]
                        else -> palette.foreground
                    }
                    builder.withColor(color) { (if (index == 0) "" else "\n") + line }
                }
            } else {
                builder.withColor(palette.foreground) { tool.input }
            }
        }
        val output = tool.output
        if (!output.isNullOrEmpty()) {
            if (builder.length > 0) builder.append("\n\n")
            val color = if (tool.status == ToolContent.Status.Failed) palette.destructive else palette.mutedForeground
            builder.withColor(color) { ToolContent.clipped(output) }
        }
        return builder.toAnnotatedString()
    }

    private fun AnnotatedString.Builder.withColor(color: androidx.compose.ui.graphics.Color, text: () -> String) {
        pushStyle(SpanStyle(color = color))
        append(text())
        pop()
    }

    /** The lines added and removed, as they are written everywhere: `+12 −3`. */
    fun lineCounts(added: Int, removed: Int, palette: Palette): AnnotatedString {
        val builder = AnnotatedString.Builder()
        if (added > 0 || removed == 0) builder.withColor(palette.success) { "+$added" }
        if (removed > 0 || added == 0) {
            val space = if (builder.length > 0) " " else ""
            builder.withColor(palette.destructive) { "$space−$removed" }
        }
        return builder.toAnnotatedString()
    }

    /** The list, quote or table a paragraph is part of. */
    private fun block(para: Para): String = when (para.kind) {
        "list_item" -> if (para.quote > 0) "quote" else "list"
        "quote" -> "quote"
        "cell" -> "table ${para.table}"
        else -> ""
    }

    private fun after(para: Para): Float = when (para.kind) {
        "heading" -> 8f
        "list_item" -> 6f
        "quote" -> 8f
        "pre" -> 12f
        "rule" -> 10f
        "cell" -> 0f
        else -> BLOCK_GAP
    }

    private fun before(para: Para, first: Boolean): Float = when (para.kind) {
        "heading" -> if (first) 0f else HEADING_GAP
        "rule" -> 4f
        else -> 0f
    }

    private fun quoteOf(para: Para): Int = when (para.kind) {
        "quote" -> para.depth
        "list_item" -> para.quote
        else -> 0
    }

    /** A reply's paragraphs as blocks, with their runs, links and code coloured. */
    fun prose(source: ProseSource, palette: Palette): List<ProseBlock> {
        val text = source.text
        val length = text.length
        fun valid(start: Int, len: Int) = start >= 0 && len > 0 && start + len <= length
        val builder = AnnotatedString.Builder(text)
        builder.addStyle(SpanStyle(color = palette.mutedForeground, fontSize = Theme.proseSize), 0, length)

        val headings = mutableListOf<IntRange>()
        for (para in source.paras) {
            if (!valid(para.start, para.length)) continue
            when (para.kind) {
                "heading" -> {
                    val font = Theme.heading(para.level)
                    builder.addStyle(SpanStyle(fontSize = font.fontSize, fontWeight = font.fontWeight, color = palette.foreground), para.start, para.end)
                    headings += para.start until para.end
                }
                "pre" -> {
                    builder.addStyle(SpanStyle(fontFamily = Theme.mono, fontSize = Theme.codeSize, color = palette.foreground), para.start, para.end)
                    colour(builder, para.spans ?: IntArray(0), para.start, para.end, palette)
                }
                "cell" -> {
                    val header = para.header
                    val style = if (header) {
                        SpanStyle(fontSize = sized(13f), fontWeight = FontWeight.SemiBold, color = palette.foreground)
                    } else {
                        SpanStyle(fontSize = sized(13f))
                    }
                    builder.addStyle(style, para.start, para.end)
                    if (header) headings += para.start until para.end
                }
            }
        }

        for (run in source.runs) {
            if (run.size != 3 || !valid(run[0], run[1])) continue
            val (start, end, style) = Triple(run[0], run[0] + run[1], run[2])
            val bold = style and 1 != 0
            val italic = style and 2 != 0
            val code = style and 4 != 0
            val heading = headings.any { start in it }
            when {
                code -> {
                    val weight = if (bold || heading) FontWeight.SemiBold else FontWeight.Normal
                    builder.addStyle(SpanStyle(fontFamily = Theme.mono, fontSize = sized(12.5f), fontWeight = weight, color = palette.foreground), start, end)
                    builder.addStringAnnotation(TextMarks.code, "", start, end)
                }
                heading -> if (italic) builder.addStyle(SpanStyle(fontStyle = FontStyle.Italic), start, end)
                bold || italic -> builder.addStyle(
                    SpanStyle(
                        fontWeight = if (bold) FontWeight.SemiBold else null,
                        fontStyle = if (italic) FontStyle.Italic else null,
                        color = if (bold) palette.foreground else androidx.compose.ui.graphics.Color.Unspecified,
                    ),
                    start, end,
                )
            }
            if (style and 8 != 0) builder.addStyle(SpanStyle(textDecoration = TextDecoration.LineThrough), start, end)
        }
        for (link in source.links) {
            if (!valid(link.start, link.length)) continue
            addLink(builder, link.url, link.start, link.start + link.length, palette)
        }
        return blocks(source, builder.toAnnotatedString())
    }

    private fun blocks(source: ProseSource, whole: AnnotatedString): List<ProseBlock> {
        val text = source.text
        fun piece(start: Int, end: Int): AnnotatedString {
            val from = start.coerceIn(0, text.length)
            var to = end.coerceIn(from, text.length)
            if (to > from && text[to - 1] == '\n') to -= 1
            return whole.subSequence(from, to)
        }
        val blocks = mutableListOf<ProseBlock>()
        val tables = mutableSetOf<Int>()
        var previous: Para? = null
        var previousQuote = 0
        val paras = source.paras.filter { it.start >= 0 && it.length > 0 && it.end <= text.length }
        for (para in paras) {
            if (para.kind == "cell" && para.table in tables) {
                previous = para
                continue
            }
            val ended = previous?.let(::block).orEmpty()
            val top = when {
                previous == null -> before(para, true)
                ended.isNotEmpty() && ended != block(para) && para.kind != "pre" -> BLOCK_GAP + before(para, false)
                else -> after(previous) + before(para, false)
            }
            val quote = quoteOf(para)
            val joined = quote > 0 && previousQuote > 0
            val block: ProseBlock = when (para.kind) {
                "list_item" -> {
                    val quoteIndent = para.quote * QUOTE_INDENT
                    val depth = maxOf(1, para.depth)
                    val indent = quoteIndent + depth * LIST_INDENT
                    val markerIndent = quoteIndent + (depth - 1) * LIST_INDENT + 4
                    val all = piece(para.start, para.end)
                    val tab = all.text.indexOf('\t')
                    if (para.marker && tab >= 0) {
                        TextBlock(all.subSequence(tab + 1, all.length), Theme.proseLineHeight, indent, all.subSequence(0, tab), markerIndent, top, quote, joined)
                    } else {
                        TextBlock(all, Theme.proseLineHeight, indent, null, 0f, top, quote, joined)
                    }
                }
                "quote" -> TextBlock(piece(para.start, para.end), Theme.proseLineHeight, para.depth * QUOTE_INDENT, null, 0f, top, quote, joined)
                "heading" -> {
                    val size = Theme.heading(para.level).fontSize.value
                    TextBlock(piece(para.start, para.end), (size * 1.2f + 3).sp, 0f, null, 0f, top, 0, false)
                }
                "pre" -> {
                    val code = source.slice(para.start, para.end).trimEnd('\n')
                    CodeBlock(para.language, code, piece(para.start, para.end), para.depth * LIST_INDENT, top, quote, joined)
                }
                "rule" -> RuleBlock(top)
                "cell" -> {
                    tables += para.table
                    table(paras.filter { it.kind == "cell" && it.table == para.table }, ::piece, source, top)
                }
                else -> TextBlock(piece(para.start, para.end), Theme.proseLineHeight, 0f, null, 0f, top, 0, false)
            }
            blocks += block
            previousQuote = quote
            previous = para
        }
        return blocks
    }

    private fun table(cells: List<Para>, piece: (Int, Int) -> AnnotatedString, source: ProseSource, top: Float): TableBlock {
        val rows = (cells.maxOfOrNull { it.row } ?: 0) + 1
        val columns = (cells.maxOfOrNull { it.column } ?: 0) + 1
        val grid = MutableList(rows) { MutableList(columns) { AnnotatedString("") } }
        val words = MutableList(rows) { MutableList(columns) { "" } }
        val aligns = MutableList(columns) { 0 }
        for (cell in cells) {
            grid[cell.row][cell.column] = piece(cell.start, cell.end)
            words[cell.row][cell.column] = source.slice(cell.start, cell.end).trimEnd('\n')
            aligns[cell.column] = cell.align.coerceIn(0, 2)
        }
        val headers = cells.filter { it.header }.map { it.row }.toSet().size
        return TableBlock(grid, aligns, headers, words.joinToString("\n") { it.joinToString("\t") }, top)
    }
}
