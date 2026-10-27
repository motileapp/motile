package app.motile.transcript

import android.text.format.Formatter
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.motile.core.Agent
import app.motile.core.AgentState
import app.motile.core.AttachedFile
import app.motile.core.JSON
import app.motile.core.SkillSpan
import app.motile.core.Time
import app.motile.core.ViewedMedia
import app.motile.core.bool
import app.motile.core.double
import app.motile.core.int
import app.motile.core.intLists
import app.motile.core.long
import app.motile.core.obj
import app.motile.core.objects
import app.motile.core.optionalDouble
import app.motile.core.optionalInt
import app.motile.core.optionalInts
import app.motile.core.optionalString
import app.motile.core.string
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.Palette
import app.motile.theme.Theme
import app.motile.transcript.openui.UiContent
import java.util.concurrent.Executors
import android.os.Handler
import android.os.Looper

/**
 * One row of the transcript, decoded from the core's JSON on the event thread. Its text is typeset
 * there too, in the palette in effect, so that showing a row costs the main thread nothing but
 * laying it out; a row typesets itself again only when the appearance changes.
 */
class RowModel(
    val id: String,
    val itemID: String,
    /** The row belongs to the group above it, which is open. */
    val nested: Boolean,
    val kind: Kind,
    /** The space it keeps from the row above it. */
    val above: Float = 0f,
) {
    sealed interface Kind {
        /** `collapsible` when it is long enough to be shown cut, with a button that shows the rest. */
        class User(val text: MessageText, val attachments: List<AttachedFile>, val values: SentValues?, val at: Double, val collapsible: Boolean) : Kind
        class Prose(val content: ProseContent) : Kind
        class Code(val content: CodeContent) : Kind
        /** A program the core runs, drawn from the tree it resolves. */
        class Ui(val content: UiContent) : Kind
        class Tool(val content: ToolContent) : Kind
        class Thinking(val text: String) : Kind
        class Media(val content: MediaContent) : Kind
        class File(val content: SentFile) : Kind
        class Group(val content: GroupContent) : Kind
        class Fold(val content: FoldContent) : Kind
        class Error(val message: String) : Kind
        class Changes(val content: ChangesContent) : Kind
        class TurnEnd(val content: TurnEndContent) : Kind
        class Handoff(val content: HandoffContent) : Kind
        /** An agent the thread's agent started, or what it was told of one. */
        class Agent(val content: AgentContent) : Kind
        /** Agents started one after the other, as one row that opens into a card of them. */
        class Agents(val content: AgentsContent) : Kind
        class Queued(val content: QueuedContent) : Kind
        /** The thread's server was unlinked from the account: the last row, in place of the composer, which says why the thread can only be read. */
        class Removed(val content: RemovedContent) : Kind
        /** A row this client doesn't know how to show, which takes no room. */
        object Unknown : Kind
    }

    val isUser: Boolean get() = kind is Kind.User
    val isQueued: Boolean get() = kind is Kind.Queued

    /** The row is a message the server has: one in the transcript, or one that waits for the agent. */
    val isSentMessage: Boolean get() = isUser || isQueued

    /** What kind of view draws it, for the list to reuse views between rows of one kind. */
    val contentType: String get() = kind::class.simpleName ?: ""

    /** The row has code that came without highlighting. */
    val needsHighlight: Boolean
        get() = when (kind) {
            is Kind.Code -> kind.content.spans == null
            is Kind.Prose -> kind.content.uncoloured
            else -> false
        }

    /** The plain text of the row, for copying a whole reply. */
    val plainText: String?
        get() = when (kind) {
            is Kind.Prose -> kind.content.plain
            is Kind.Code -> "```${kind.content.language}\n${kind.content.code}\n```"
            is Kind.Ui -> "```${kind.content.code.language}\n${kind.content.code.code}\n```"
            else -> null
        }

    /** The image or the video the row shows, for the viewer. */
    val mediaShown: ViewedMedia?
        get() {
            val media = (kind as? Kind.Media)?.content ?: return null
            return ViewedMedia(media.name, media.video, ViewedMedia.Source.Media(media.id))
        }

    fun applySpans(spans: IntArray) {
        (kind as? Kind.Code)?.content?.applySpans(spans)
    }

    companion object {
        const val pendingID = "pending"

        fun parse(json: JSON): RowModel? {
            val kind = when (json.string("kind")) {
                "user" -> Kind.User(
                    MessageText(json, muted = false), json.objects("attachments").map(::AttachedFile), json.obj("values")?.let(::SentValues), json.double("at"),
                    json.bool("collapsible"),
                )
                "prose" -> Kind.Prose(ProseContent(json))
                "code" -> Kind.Code(CodeContent(json.string("language"), json.string("code"), json.optionalInts("spans")))
                "ui" -> Kind.Ui(UiContent(json))
                "tool" -> Kind.Tool(ToolContent(json))
                "thinking" -> Kind.Thinking(json.string("text"))
                "media" -> Kind.Media(MediaContent(json))
                "file" -> Kind.File(SentFile(json))
                "group" -> Kind.Group(GroupContent(json))
                "fold" -> Kind.Fold(FoldContent(json))
                "error" -> Kind.Error(json.string("message"))
                "changes" -> Kind.Changes(ChangesContent(json))
                "turn_end" -> Kind.TurnEnd(TurnEndContent(json))
                "handoff" -> Kind.Handoff(HandoffContent(json))
                "agent" -> Kind.Agent(AgentContent(json))
                "agents" -> Kind.Agents(AgentsContent(json))
                "queued" -> Kind.Queued(QueuedContent(MessageText(json, muted = true), json.objects("attachments").map(::AttachedFile), json.string("status")))
                "removed" -> Kind.Removed(RemovedContent(json.string("server"), json.string("title"), json.string("message")))
                else -> Kind.Unknown
            }
            return RowModel(json.string("id"), json.string("item"), json.bool("nested"), kind, Typesetter.spaceAbove(json))
        }

        /** A message on its way to the server. One sent while the agent works will wait for it, so it shows as it will once the server has it. */
        fun pending(text: String, attachments: List<AttachedFile>, queued: Boolean, skills: List<SkillSpan> = emptyList(), collapsible: Boolean = false): RowModel {
            val message = MessageText(text, emptyList(), skills, muted = queued)
            val kind = if (queued) Kind.Queued(QueuedContent(message, attachments, "Queued")) else Kind.User(message, attachments, null, Time.now(), collapsible)
            return RowModel(pendingID, pendingID, false, kind)
        }
    }
}

/** What the end of an unlinked server's thread says, as the core wrote it. */
data class RemovedContent(val server: String, val title: String, val message: String)

/** Text typeset for one palette, and again for another when the appearance changes. */
class Typeset<T>(private val build: (Palette) -> T) {
    @Volatile
    private var made: Pair<Palette, T>? = null

    fun get(palette: Palette): T {
        made?.let { if (it.first === palette) return it.second }
        return build(palette).also { made = palette to it }
    }

    /** Typesets it now, in the palette in effect, off the main thread. */
    fun warm(): Typeset<T> = apply { get(Theme.palette) }
}

/** A link in a row's text, by its place in UTF-16 units. */
class TextLink(val start: Int, val length: Int, val url: String) {
    constructor(json: JSON) : this(json.int("start"), json.int("len"), json.string("url"))
}

/** A message's text, its links and the skills it starts, which show as chips. */
class MessageText(val text: String, val links: List<TextLink>, val skills: List<SkillSpan>, muted: Boolean) {
    constructor(json: JSON, muted: Boolean) : this(json.string("text"), json.objects("links").map(::TextLink), json.objects("skills").map(::SkillSpan), muted)

    val styled = Typeset { palette ->
        Typesetter.message(this, if (muted) palette.mutedForeground else palette.foreground, palette)
    }.warm()
}

class ProseContent(json: JSON) {
    val source = ProseSource(json)
    /** Code inside it still waits for highlighting. */
    val uncoloured: Boolean = source.paras.any { it.kind == "pre" && it.spans == null }
    /** The text as it is copied: what a table stands for instead of its cells one under another. */
    val plain: String = source.plain()
    val blocks = Typeset { palette -> Typesetter.prose(source, palette) }.warm()
}

/** A paragraph of a prose row, as the core sets it. */
class Para(json: JSON) {
    val start = json.int("start")
    val length = json.int("len")
    val kind = json.string("kind")
    val level = json.int("level")
    val depth = json.int("depth")
    val marker = json.bool("marker")
    val quote = json.int("quote")
    val language = json.string("language")
    val spans = json.optionalInts("spans")
    val table = json.int("table")
    val row = json.int("row")
    val column = json.int("column")
    val header = json.bool("header")
    val align = json.int("align")
    val end: Int get() = start + length
}

class ProseSource(json: JSON) {
    /** Lines broken inside a paragraph are joined by line separators, which become line breaks here. */
    val text: String = json.string("text").replace(' ', '\n')
    val runs: List<IntArray> = json.intLists("runs")
    val links: List<TextLink> = json.objects("links").map(::TextLink)
    val paras: List<Para> = json.objects("paras").map(::Para)

    fun plain(): String {
        val out = StringBuilder()
        var index = 0
        while (index < paras.size) {
            val para = paras[index]
            if (para.kind != "cell") {
                out.append(slice(para.start, para.end))
                index += 1
                continue
            }
            val cells = paras.drop(index).takeWhile { it.kind == "cell" && it.table == para.table }
            val rows = cells.groupBy { it.row }.toSortedMap().values
            out.append(rows.joinToString("\n") { row -> row.sortedBy { it.column }.joinToString("\t") { slice(it.start, it.end).trimEnd('\n') } })
            if (slice(cells.last().start, cells.last().end).endsWith("\n")) out.append('\n')
            index += cells.size
        }
        return out.toString()
    }

    fun slice(start: Int, end: Int): String = text.substring(start.coerceIn(0, text.length), end.coerceIn(0, text.length))
}

/** A code row: its highlighting arrives after it, and is typeset off the main thread. */
class CodeContent(val language: String, val code: String, spans: IntArray?) {
    val lineCount: Int = if (code.isEmpty()) 1 else code.count { it == '\n' } + 1
    /** The highlighting, as `[start, length, palette index]`; null until it has arrived. */
    var spans by mutableStateOf(spans)
        private set
    var text by mutableStateOf(typeset(spans).warm())
        private set

    private fun typeset(spans: IntArray?) = Typeset { palette -> Typesetter.code(code, spans ?: IntArray(0), palette) }

    /** The length of the longest line in columns, counting what is wider than a letter as two. */
    val columns: Int = run {
        var widest = 0
        var current = 0
        for (unit in code) {
            when {
                unit == '\n' -> {
                    widest = maxOf(widest, current)
                    current = 0
                }
                unit == '\t' -> current += 4
                unit.code < 0x2e80 -> current += 1
                else -> current += 2
            }
        }
        maxOf(widest, current)
    }

    fun applySpans(new: IntArray) {
        Typesetting.run({ typeset(new).warm() }) { coloured ->
            spans = new
            text = coloured
        }
    }
}

/** Typesets off the main thread and hands what it made to the main thread. */
object Typesetting {
    private val worker = Executors.newSingleThreadExecutor { Thread(it, "motile-typeset").apply { isDaemon = true } }
    private val main = Handler(Looper.getMainLooper())

    fun <T> run(work: () -> T, done: (T) -> Unit) {
        worker.execute {
            val made = work()
            main.post { done(made) }
        }
    }
}

class ToolContent(json: JSON) {
    enum class Status { Running, Succeeded, Failed }

    val name = json.string("name")
    val icon = json.string("icon")
    val verb = json.string("verb")
    val target = json.string("target")
    val status = when (json.string("status")) {
        "running" -> Status.Running
        "failed" -> Status.Failed
        else -> Status.Succeeded
    }
    val input = json.string("input")
    val inputLanguage = json.string("input_language")
    val output = json.optionalString("output")
    /** When it started, while it runs. */
    val startedAt = json.optionalDouble("started_at")

    val symbol: Symbol get() = symbol(icon)
    val hasDetail: Boolean get() = input.isNotEmpty() || output != null

    /** What opening the row shows: the input, then what came back. */
    val detail = Typeset { palette -> Typesetter.toolDetail(this, palette) }

    companion object {
        fun symbol(icon: String): Symbol = when (icon) {
            "terminal" -> Symbol.Terminal
            "file" -> Symbol.FileText
            "edit" -> Symbol.Pencil
            "search" -> Symbol.Search
            "web" -> Symbol.Globe
            "agent" -> Symbol.Bot
            "watch" -> Symbol.Eye
            "question" -> Symbol.MessageCircleQuestionMark
            "todo" -> Symbol.ListChecks
            else -> Symbol.Wrench
        }

        /** The first lines of long output; the rest is in the transcript on the server. */
        fun clipped(output: String): String {
            val lines = output.split("\n")
            if (lines.size <= 60) return output
            return lines.take(60).joinToString("\n") + "\n… ${lines.size - 60} more lines"
        }
    }
}

/** Tool calls that followed one another, as one row that opens into them. */
class GroupContent(json: JSON) {
    val title = json.string("title")
    val target = json.string("target")
    val icon = json.string("icon")
    val running = json.bool("running")
    val failed = json.bool("failed")
    val open = json.bool("open")
    /** When the latest of them that still runs started. */
    val startedAt = json.optionalDouble("started_at")
}

/** Stands for what a finished turn did before its last message, and opens into it. */
class FoldContent(json: JSON) {
    val label = TurnEndContent.label(json.bool("stopped"), json.optionalInt("duration_ms"))
    val open = json.bool("open")
}

/** What a finished turn changed in the thread's folder: its files under their folders. The row's item is the one that ends the turn. */
class ChangesContent(json: JSON) {
    class Entry(json: JSON) {
        /** What opens and closes a folder. */
        val id = json.string("id")
        val name = json.string("name")
        val path = json.string("path")
        val depth = json.int("depth")
        val folder = json.bool("folder")
        val open = json.bool("open")
        val added = json.int("added")
        val removed = json.int("removed")
    }

    val files = json.int("files")
    val added = json.int("added")
    val removed = json.int("removed")
    val at = json.double("at")
    val entries = json.objects("entries").map(::Entry)

    val title: String get() = if (files == 1) "1 changed file" else "$files changed files"
}

/** A message that waits to be given to the agent. The row's item is the message. */
class QueuedContent(
    val text: MessageText,
    val attachments: List<AttachedFile>,
    /** How it waits: queued or held. */
    val status: String,
)

class TurnEndContent(json: JSON) {
    val durationMs = json.optionalInt("duration_ms")
    val costUSD = json.optionalDouble("cost_usd")
    val isError = json.bool("is_error")
    val stopped = json.bool("stopped")
    /** The turn's fold says how long it took, so the end of the turn doesn't. */
    val folded = json.bool("folded")
    val at = json.double("at")

    /** When the turn ended and, unless its fold says so, how long it took. */
    val stamp: String
        get() {
            if (folded || (!stopped && durationMs == null)) return Time.stamp(at)
            return "${Time.stamp(at)} · ${label(stopped, durationMs)}"
        }

    companion object {
        fun label(stopped: Boolean, durationMs: Int?): String {
            val duration = durationMs?.let { Time.duration(it) }
            return when {
                stopped && duration != null -> "You stopped after $duration"
                stopped -> "You stopped this response"
                duration != null -> "Worked for $duration"
                else -> "Done"
            }
        }
    }
}

/** An agent as its row shows it: its avatar, its title and word, what it does or said and what it runs on, and how long it worked. */
data class AgentContent(
    val title: String,
    val role: String?,
    val state: AgentState,
    val label: String,
    /** The label is shown beside the title, as what the user is to look at. */
    val flagged: Boolean,
    val detail: String,
    val agent: Agent?,
    /** "Opus 5.5 · work". */
    val runsOn: String,
    val startedAt: Double,
    val durationMs: Int?,
    /** When the thread's agent was told, for what it was told. */
    val at: Double?,
    /** The thread it works in, which the row opens. Without one the row shows what it did. */
    val thread: String?,
    /** Where the row is in the card of an open group. */
    val card: Card?,
) {
    enum class Card { First, Middle, Last }

    constructor(json: JSON) : this(
        title = json.string("title"),
        role = json.optionalString("role"),
        state = AgentState.of(json.string("state")),
        label = json.string("label"),
        flagged = json.bool("flagged"),
        detail = json.string("detail"),
        agent = Agent.of(json.optionalString("agent")),
        runsOn = json.string("runs_on"),
        startedAt = json.double("started_at"),
        durationMs = json.optionalInt("duration_ms"),
        at = json.optionalDouble("at"),
        thread = json.optionalString("thread"),
        card = Card.entries.firstOrNull { it.name.lowercase() == json.optionalString("card") },
    )

    /** What the second line says: what it runs on, then what it does or said. */
    val line: String
        get() {
            val said = if (detail.isEmpty() && runsOn.isEmpty()) label else detail
            return listOf(runsOn, said).filter { it.isNotEmpty() }.joinToString(" · ")
        }
}

/** Agents started one after the other: their avatars, how many, and how they are doing. */
data class AgentsContent(
    val title: String,
    val summary: List<Pair<String, AgentState>>,
    val avatars: List<Pair<Agent?, AgentState>>,
    val more: Int,
    val startedAt: Double?,
    val durationMs: Int?,
    val open: Boolean,
) {
    constructor(json: JSON) : this(
        title = json.string("title"),
        summary = json.objects("summary").map { it.string("text") to AgentState.of(it.string("state")) },
        avatars = json.objects("avatars").map { Agent.of(it.optionalString("agent")) to AgentState.of(it.string("state")) },
        more = json.int("more"),
        startedAt = json.optionalDouble("started_at"),
        durationMs = json.optionalInt("duration_ms"),
        open = json.bool("open"),
    )
}

/** The thread went on with another agent or account: the model before and the one after. */
class HandoffContent(json: JSON) {
    class End(json: JSON?) {
        val agent = Agent.of(json?.string("agent")) ?: Agent.Claude
        val label = json?.string("label") ?: ""
        val account = json?.optionalString("account")
    }

    val from = End(json.obj("from"))
    val to = End(json.obj("to"))
}

/** An image or a video a reply shows. The core says how large it is, so the row has its place before the file is on this device. */
class MediaContent(json: JSON) {
    val id = json.string("media")
    val video = json.bool("video")
    /** Unknown for a video. */
    val pixels: Pair<Float, Float>? = run {
        val width = json.double("width").toFloat()
        val height = json.double("height").toFloat()
        if (width > 0 && height > 0) width to height else null
    }
    val bytes = json.long("size")
    val alt = json.string("alt")
    val name = json.string("name")
    /** The image that stands for a video until it plays. */
    val poster = json.optionalString("poster")

    /** The image the box shows: the image itself, or a video's poster. */
    val picture: String? get() = if (video) poster else id

    /** The box it is shown in, in a column `width` wide, in dp: its own size, or smaller to fit. */
    fun box(width: Float): Pair<Float, Float> {
        val pixels = pixels ?: run {
            val boxWidth = minOf(width, 640f)
            return boxWidth to Math.round(boxWidth * 9 / 16).toFloat()
        }
        val scale = minOf(1f, width / pixels.first, MAX_HEIGHT / pixels.second)
        return maxOf(1f, Math.round(pixels.first * scale).toFloat()) to maxOf(1f, Math.round(pixels.second * scale).toFloat())
    }

    companion object {
        const val MAX_HEIGHT = 480f
    }
}

/** A file a reply sends, other than an image or a video, for the user to download. */
class SentFile(json: JSON) {
    val id = json.string("media")
    val bytes = json.long("size")
    val alt = json.string("alt")
    val name = json.string("name")

    val size: String get() = byteCount(bytes)

    private val extension: String get() = if ('.' in name) name.substringAfterLast('.') else ""

    /** What kind of file it is and how large, as "PDF · 2.4 MB". */
    val detail: String
        get() {
            val kind = extension.uppercase()
            return if (kind.isEmpty()) size else "$kind · $size"
        }

    val symbol: Symbol
        get() = when (extension.lowercase()) {
            "pdf", "md", "markdown", "txt", "rst", "csv", "tsv", "doc", "docx", "rtf", "pages", "odt", "log" -> Symbol.FileText
            else -> Symbol.File
        }
}

/** A size in bytes as files are given, "2.4 MB". */
fun byteCount(bytes: Long): String = Formatter.formatShortFileSize(Platform.context, bytes)

/** The parts of a typeset text that the text system can't draw, by tag. */
object TextMarks {
    const val code = "motile.code"
    const val skill = "motile.skill"
}
