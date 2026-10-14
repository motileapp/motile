package app.motile.panel

import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp
import app.motile.core.JSON
import app.motile.core.PullRequest
import app.motile.core.bool
import app.motile.core.double
import app.motile.core.int
import app.motile.core.ints
import app.motile.core.obj
import app.motile.core.objects
import app.motile.core.optionalDouble
import app.motile.core.optionalInt
import app.motile.core.optionalString
import app.motile.core.string
import app.motile.core.strings
import app.motile.theme.LocalPalette
import app.motile.theme.Palette
import app.motile.theme.Theme
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.longOrNull

/** How a status, a check or a review went, as the core says it. */
enum class Tone {
    Success, Danger, Warning, Pending, Neutral, Merged;

    fun color(palette: Palette): Color = when (this) {
        Success -> palette.success
        Danger -> palette.destructive
        Warning -> palette.warning
        Pending -> palette.process
        Neutral -> palette.mutedForeground
        Merged -> palette.merged
    }

    val color: Color @Composable @ReadOnlyComposable get() = color(LocalPalette.current)

    companion object {
        fun of(name: String?): Tone? = entries.firstOrNull { it.name.lowercase() == name }

        /** The `tone` of the JSON, neutral when it has none. */
        fun of(json: JSON): Tone = of(json.string("tone")) ?: Neutral
    }
}

/** The state of a pull request as the core spells it. */
internal fun pullRequestState(name: String): PullRequest.State = when (name) {
    "merged" -> PullRequest.State.Merged
    "closed" -> PullRequest.State.Closed
    "draft" -> PullRequest.State.Draft
    else -> PullRequest.State.Open
}

/** The pull request tab as the core worked it out, its text set for drawing. It is read off the main thread. */
class PullRequestPage(json: JSON) {
    class Status(val kind: String, val tone: Tone, val title: String, val detail: String?, val at: Double?, val buttons: List<PullRequestButton>)

    class Check(
        val id: Int,
        val name: String,
        val workflow: String?,
        val label: String,
        val tone: Tone,
        val description: String?,
        val url: String?,
        /** The prompt that has the agent fix it. */
        val fix: String?,
    )

    class Entry(
        val id: Int,
        val kind: String,
        val author: String,
        val said: String,
        val tone: Tone,
        val at: Double,
        val body: List<PullRequestText>,
        val commits: List<Commit>,
        val url: String?,
        /** Names the comment or the review, to react to it. */
        val subject: String?,
        val reactions: List<Reaction>,
        val thread: Thread?,
    )

    class Commit(val oid: String, val headline: String, val sha: String) {
        val id: String get() = sha.ifEmpty { oid }
    }

    /** A conversation on a line. */
    class Thread(json: JSON) {
        val id = json.string("id")
        val path = json.string("path")
        val line = json.optionalInt("line")
        /** "left" for a line as it was, "right" as the pull request makes it. */
        val side = json.string("side")
        val resolved = json.bool("resolved")
        val outdated = json.bool("outdated")
        /** The last lines of the diff it was written under. */
        val hunk = json.strings("hunk")
        val comments = json.objects("comments").map(::Comment)
        val fix = json.optionalString("fix")
        val canResolve = json.bool("can_resolve")
    }

    class Comment(json: JSON) {
        val id = json.string("id")
        val author = json.string("author")
        val at = json.double("at")
        val body = PullRequestText.blocks(json.objects("body"))
        val url = json.optionalString("url")
        val reactions = json.objects("reactions").map(::Reaction)
    }

    data class Reaction(
        /** As the protocol spells it: "thumbs_up". */
        val kind: String,
        val count: Int,
        val mine: Boolean,
    ) {
        constructor(json: JSON) : this(json.string("kind"), json.int("count"), json.bool("mine"))

        val emoji: String get() = all.firstOrNull { it.first == kind }?.second ?: ""

        companion object {
            /** GitHub's reactions, in the order it offers them. */
            val all = listOf(
                "thumbs_up" to "👍", "thumbs_down" to "👎", "laugh" to "😄", "hooray" to "🎉",
                "confused" to "😕", "heart" to "❤️", "rocket" to "🚀", "eyes" to "👀",
            )
        }
    }

    class Toggle(val name: String, val color: Color?, val on: Boolean)

    class Reviewer(val name: String, val label: String, val tone: Tone)

    class Label(val name: String, val hex: Color?) {
        val color: Color @Composable @ReadOnlyComposable get() = hex ?: LocalPalette.current.mutedStrongerForeground
    }

    class StackLayer(val number: Int, val title: String, val state: PullRequest.State, val current: Boolean)

    class Stack(val url: String?, val base: String, val layers: List<StackLayer>)

    class StackedOn(val number: Int, val title: String, val state: PullRequest.State)

    val number = json.int("number")
    val title = json.string("title")
    val url = json.string("url")
    val state = pullRequestState(json.string("state"))
    val byline = json.string("byline")
    val base = json.string("base")
    val head = json.string("head")
    val files = json.int("files")
    val additions = json.int("additions")
    val deletions = json.int("deletions")
    val statuses = json.objects("statuses").map { status ->
        Status(
            status.string("kind"), Tone.of(status), status.string("title"), status.optionalString("detail"), status.optionalDouble("at"),
            status.objects("buttons").map(::PullRequestButton),
        )
    }
    val checks = json.objects("checks").mapIndexed { index, check ->
        Check(
            index, check.string("name"), check.optionalString("workflow"), check.string("label"), Tone.of(check),
            check.optionalString("description"), check.optionalString("url"), check.optionalString("fix"),
        )
    }
    val primary = json.obj("primary")?.let(::PullRequestButton)
    /** How it merges, as the core spells it: "squash". */
    val method = json.optionalString("method")
    val methods = json.objects("methods").map(::PullRequestChoice)
    val menu = json.objects("menu").map(::PullRequestButton)
    val activity = json.objects("activity").mapIndexed { index, entry ->
        Entry(
            index, entry.string("kind"), entry.string("author"), entry.string("said"), Tone.of(entry), entry.double("at"),
            PullRequestText.blocks(entry.objects("body")),
            entry.objects("commits").map { Commit(it.string("oid"), it.string("headline"), it.string("sha")) },
            entry.optionalString("url"), entry.optionalString("id"), entry.objects("reactions").map(::Reaction),
            entry.obj("thread")?.let(::Thread),
        )
    }
    val verdicts = json.objects("verdicts").map(::PullRequestChoice)
    val withComment = json.obj("with_comment")?.let(::PullRequestChoice)
    /** Something is still being worked out, so it is read again in a while. */
    val settling = json.bool("settling")
    /** The description as it was written, for editing it. */
    val body = json.string("body")
    val canEdit = json.bool("can_edit")
    val labels = json.objects("labels").map { Label(it.string("name"), color(it.string("color"))) }
    val labelChoices = json.objects("label_choices").map { Toggle(it.string("name"), it.optionalString("color")?.let(::color), it.bool("on")) }
    val reviewers = json.objects("reviewers").map { Reviewer(it.string("name"), it.string("label"), Tone.of(it)) }
    val reviewerChoices = json.objects("reviewer_choices").map { Toggle(it.string("name"), null, it.bool("on")) }
    /** Which of its files the user has marked as viewed, by path. */
    val viewed: Map<String, Boolean> = buildMap {
        for (file in json.objects("viewed")) if (file.string("path") !in this) put(file.string("path"), file.bool("viewed"))
    }
    val canReviewLines = json.bool("can_review_lines")
    val threads = json.objects("threads").map(::Thread)
    val stack = json.obj("stack")?.let { stack ->
        Stack(
            stack.string("url").takeIf { it.isNotEmpty() }, stack.string("base"),
            stack.objects("layers").map { StackLayer(it.int("number"), it.string("title"), pullRequestState(it.string("state")), it.bool("current")) },
        )
    }
    val stackedOn = json.obj("stacked_on")?.let { StackedOn(it.int("number"), it.string("title"), pullRequestState(it.string("state"))) }
    val watchable = json.bool("watchable")

    /** The conversations on lines of the file. */
    fun threads(path: String): List<Thread> = threads.filter { it.path == path }

    companion object {
        /** A label's colour, from GitHub's or Linear's hex; nothing when it isn't one. */
        fun color(hex: String): Color? {
            val value = hex.removePrefix("#").toLongOrNull(16) ?: return null
            return Color(0xFF000000 or value)
        }
    }
}

/** A pull request in the repository's list. */
class PullRequestRow(json: JSON) {
    val number = json.int("number")
    val title = json.string("title")
    val url = json.string("url").takeIf { it.isNotEmpty() }
    val state = pullRequestState(json.string("state"))
    val author = json.string("author")
    val head = json.string("head")
    val base = json.string("base")
    val updatedAt = json.double("updated_at")
    val checks = Tone.of(json.optionalString("checks"))
    val checksLabel = json.optionalString("checks_label")
    val review: Pair<Tone, String>? = (json["review"] as? JsonArray)?.let { pair ->
        val words = pair.mapNotNull { (it as? JsonPrimitive)?.content }
        if (words.size != 2) return@let null
        Tone.of(words[0])?.let { it to words[1] }
    }
    val additions = json.int("additions")
    val deletions = json.int("deletions")
}

/** Something the tab offers: an action on the pull request, or a prompt for the thread's agent. */
data class PullRequestButton(
    val label: String,
    val pendingLabel: String?,
    val action: String?,
    val method: String?,
    val prompt: String?,
    val style: String,
    val confirm: Confirm?,
    /** It was chosen from the menu, so what it does is said in the bar rather than on it. */
    val fromMenu: Boolean = false,
) {
    data class Confirm(val title: String, val message: String, val button: String)

    constructor(json: JSON) : this(
        label = json.string("label"),
        pendingLabel = json.optionalString("pending_label"),
        action = json.optionalString("action"),
        method = json.optionalString("method"),
        prompt = json.optionalString("prompt"),
        style = json.string("style"),
        confirm = json.obj("confirm")?.let { Confirm(it.string("title"), it.string("message"), it.string("button")) },
    )

    /** Names it while its action runs. */
    val key: String get() = if (fromMenu) "menu:$label" else label

    /** The same button as the menu offers it. */
    val inMenu: PullRequestButton get() = copy(fromMenu = true)
}

data class PullRequestChoice(val label: String, val action: String, val method: String?) {
    constructor(json: JSON) : this(json.string("label"), json.string("action"), json.optionalString("method"))
}

/**
 * A stretch of Markdown from the pull request, set like a reply in the transcript, a little smaller
 * to sit in the panel. Set off the main thread, in the colours of the appearance in effect.
 */
sealed interface PullRequestText {
    /** Paragraphs, lists, quotes, tables and the code inside them. */
    class Prose(val parts: List<ProsePart>) : PullRequestText

    /** A code block, highlighted. */
    class Code(val text: AnnotatedString) : PullRequestText

    companion object {
        /** The panel's text size, as it is on the Mac. */
        const val SIZE = 13f

        fun blocks(json: List<JSON>): List<PullRequestText> = json.map { block ->
            if (block.string("kind") == "code") Code(ProseSetter.code(block.string("code"), block.ints("spans"))) else Prose(ProseSetter.prose(block))
        }
    }
}

/** A piece of a stretch of prose, drawn one under another. */
sealed interface ProsePart {
    /** How far it is from what is above it, in points. */
    val above: Float

    /** A paragraph. A list item's marker stands apart, `indent` points in, with the text after it. */
    class Paragraph(
        val text: AnnotatedString,
        val lineHeight: TextUnit,
        override val above: Float,
        val indent: Float,
        val marker: AnnotatedString?,
        val quote: Int,
    ) : ProsePart

    class Rule(override val above: Float, val quote: Int) : ProsePart

    /** A code block inside a list or a quote. */
    class Code(val text: AnnotatedString, override val above: Float, val indent: Float, val quote: Int) : ProsePart

    class Table(val rows: List<List<Cell>>, override val above: Float) : ProsePart

    class Cell(val text: AnnotatedString, val align: TextAlign, val header: Boolean)
}

/** Sets the core's prose as the transcript's typesetter does, smaller. Safe to call from any thread. */
private object ProseSetter {
    private val factor = PullRequestText.SIZE / 14f
    private val bodySize get() = Theme.ui(PullRequestText.SIZE).fontSize
    private const val BLOCK_GAP = 12f
    private const val HEADING_GAP = 14f
    private const val LIST_INDENT = 22f

    fun code(code: String, spans: IntArray): AnnotatedString {
        val palette = Theme.palette
        return buildAnnotatedString {
            append(code)
            addStyle(SpanStyle(color = palette.foreground, fontFamily = Theme.mono, fontSize = Theme.codeSize * factor), 0, code.length)
            colour(this, spans, 0, code.length, palette)
        }
    }

    private fun colour(builder: AnnotatedString.Builder, spans: IntArray, start: Int, end: Int, palette: Palette) {
        val syntax = Theme.syntax(palette)
        var index = 0
        while (index + 2 < spans.size) {
            val from = start + spans[index]
            val to = from + spans[index + 1]
            val color = spans[index + 2]
            index += 3
            if (to > end || from < start || color <= 0 || color >= syntax.size) continue
            builder.addStyle(SpanStyle(color = syntax[color]), from, to)
        }
    }

    /** The list, quote or table a paragraph is part of. */
    private fun block(para: JSON): String = when (para.string("kind")) {
        "list_item" -> if (para.int("quote") > 0) "quote" else "list"
        "quote" -> "quote"
        "cell" -> "table ${para.int("table")}"
        else -> ""
    }

    /** What a paragraph keeps from the next, as the transcript's paragraph styles say it. */
    private fun spacingBelow(kind: String): Float = when (kind) {
        "heading" -> 8f
        "list_item" -> 6f
        "quote" -> 8f
        "rule" -> 10f
        "cell" -> 0f
        "pre" -> 0f
        else -> BLOCK_GAP
    }

    fun prose(json: JSON): List<ProsePart> {
        val palette = Theme.palette
        val text = json.string("text")
        val whole = styled(json, text, palette)
        val paras = json.objects("paras").filter { para ->
            val start = para.int("start")
            val len = para.int("len")
            start >= 0 && len > 0 && start + len <= text.length
        }
        if (paras.isEmpty()) {
            val trimmed = whole.subSequence(0, text.trimEnd('\n').length)
            return if (trimmed.isEmpty()) emptyList() else listOf(ProsePart.Paragraph(trimmed, lineHeight(7f), 0f, 0f, null, 0))
        }
        val parts = mutableListOf<ProsePart>()
        var table: MutableMap<Int, MutableList<ProsePart.Cell>>? = null
        var tableIndex = -1
        var tableAbove = 0f
        fun endTable() {
            val rows = table ?: return
            parts += ProsePart.Table(rows.toSortedMap().values.map { it.toList() }, tableAbove)
            table = null
        }
        for ((index, para) in paras.withIndex()) {
            val kind = para.string("kind")
            val start = para.int("start")
            var end = start + para.int("len")
            while (end > start && text[end - 1] == '\n') end -= 1
            val piece = whole.subSequence(start, end)
            val previous = paras.getOrNull(index - 1)
            var above = if (previous == null) 0f else spacingBelow(previous.string("kind"))
            if (kind == "heading" && previous != null) above += HEADING_GAP
            if (kind == "rule") above += 4f
            if (previous != null && kind != "pre") {
                val ended = block(previous)
                if (ended.isNotEmpty() && ended != block(para)) above += BLOCK_GAP - spacingBelow(previous.string("kind"))
            }
            if (kind != "cell" || para.int("table") != tableIndex) endTable()
            when (kind) {
                "heading" -> parts += ProsePart.Paragraph(piece, lineHeight(3f, Theme.heading(para.int("level")).fontSize * factor), above, 0f, null, 0)
                "list_item" -> {
                    val depth = maxOf(1, para.int("depth"))
                    val quote = para.int("quote")
                    val tab = piece.indexOf('\t')
                    val marked = para.bool("marker") && tab >= 0
                    val indent = (depth - 1) * LIST_INDENT
                    parts += ProsePart.Paragraph(
                        if (marked) piece.subSequence(tab + 1, piece.length) else piece, lineHeight(7f), above,
                        if (marked) indent else indent + LIST_INDENT, if (marked) piece.subSequence(0, tab) else null, quote,
                    )
                }
                "quote" -> parts += ProsePart.Paragraph(piece, lineHeight(7f), above, 0f, null, para.int("depth"))
                "pre" -> parts += ProsePart.Code(piece, above, para.int("depth") * LIST_INDENT, 0)
                "rule" -> parts += ProsePart.Rule(above, 0)
                "cell" -> {
                    val rows = table ?: sortedMapOf<Int, MutableList<ProsePart.Cell>>().also {
                        table = it
                        tableIndex = para.int("table")
                        tableAbove = above
                    }
                    val align = when (para.int("align")) {
                        1 -> TextAlign.Center
                        2 -> TextAlign.End
                        else -> TextAlign.Start
                    }
                    rows.getOrPut(para.int("row")) { mutableListOf() } += ProsePart.Cell(piece, align, para.bool("header"))
                }
                else -> parts += ProsePart.Paragraph(piece, lineHeight(7f), above, 0f, null, 0)
            }
        }
        endTable()
        return parts
    }

    private fun lineHeight(spacing: Float, size: TextUnit = bodySize): TextUnit = (size.value * 1.2f + spacing * factor).sp

    /** The whole text with its fonts, colours, code and links, which its paragraphs are cut from. */
    private fun styled(json: JSON, text: String, palette: Palette): AnnotatedString = buildAnnotatedString {
        append(text)
        val length = text.length
        fun inRange(start: Int, len: Int) = start >= 0 && len > 0 && start + len <= length
        addStyle(SpanStyle(fontSize = bodySize, color = palette.mutedForeground), 0, length)
        val headings = mutableListOf<IntRange>()
        for (para in json.objects("paras")) {
            val start = para.int("start")
            val len = para.int("len")
            if (!inRange(start, len)) continue
            when (para.string("kind")) {
                "heading" -> {
                    val font = Theme.heading(para.int("level"))
                    addStyle(SpanStyle(fontSize = font.fontSize * factor, fontWeight = font.fontWeight, color = palette.foreground), start, start + len)
                    headings += start until start + len
                }
                "pre" -> {
                    addStyle(SpanStyle(fontFamily = Theme.mono, fontSize = Theme.codeSize * factor, color = palette.foreground), start, start + len)
                    colour(this, para.ints("spans"), start, start + len, palette)
                }
                "cell" -> {
                    addStyle(SpanStyle(fontSize = Theme.ui(13f).fontSize * factor), start, start + len)
                    if (para.bool("header")) addStyle(SpanStyle(fontWeight = FontWeight.SemiBold, color = palette.foreground), start, start + len)
                }
            }
        }
        for (run in runs(json)) {
            val (start, len, style) = run
            if (!inRange(start, len)) continue
            val bold = style and 1 != 0
            val italic = style and 2 != 0
            val code = style and 4 != 0
            val heading = headings.any { start in it }
            when {
                code -> addStyle(
                    SpanStyle(
                        fontFamily = Theme.mono, fontSize = Theme.inlineCodeFont.fontSize * factor, color = palette.foreground,
                        background = palette.backgroundAccent, fontWeight = if (bold || heading) FontWeight.SemiBold else null,
                    ),
                    start, start + len,
                )
                heading -> if (italic) addStyle(SpanStyle(fontStyle = FontStyle.Italic), start, start + len)
                bold || italic -> addStyle(
                    SpanStyle(
                        fontWeight = if (bold) FontWeight.SemiBold else null, fontStyle = if (italic) FontStyle.Italic else null,
                        color = if (bold) palette.foreground else Color.Unspecified,
                    ),
                    start, start + len,
                )
            }
            if (style and 8 != 0) addStyle(SpanStyle(textDecoration = TextDecoration.LineThrough), start, start + len)
        }
        for (link in json.objects("links")) {
            val start = link.int("start")
            val len = link.int("len")
            val url = link.string("url")
            if (!inRange(start, len) || url.isEmpty()) continue
            addLink(LinkAnnotation.Url(url, TextLinkStyles(SpanStyle(color = palette.primary))), start, start + len)
        }
    }

    /** The core's `[start, length, style]` runs. */
    private fun runs(json: JSON): List<Triple<Int, Int, Int>> = (json["runs"] as? JsonArray)?.mapNotNull { run ->
        val numbers = (run as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.longOrNull?.toInt() } ?: return@mapNotNull null
        if (numbers.size != 3) null else Triple(numbers[0], numbers[1], numbers[2])
    } ?: emptyList()
}
