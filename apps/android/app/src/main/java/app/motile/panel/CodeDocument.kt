package app.motile.panel

import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import app.motile.core.JSON
import app.motile.core.bool
import app.motile.core.int
import app.motile.core.ints
import app.motile.core.objects
import app.motile.core.optionalString
import app.motile.core.string
import app.motile.core.strings
import app.motile.theme.LocalPalette
import app.motile.theme.Theme

/** An image or a video a diff shows in place of a file's lines, by the blob git keeps it as. */
class CodeMedia(val blob: String, val video: Boolean) {
    /** It couldn't be fetched or drawn. */
    var failed by mutableStateOf(false)
    var asked = false
    /** Its picture once it is decoded: the image, or the video's first frame. */
    var picture by mutableStateOf<ImageBitmap?>(null)

    val id: String get() = "blob:$blob"
}

/** The lines of one file: of its diff, or all of them. */
class CodeFile(
    val path: String,
    /** Where a renamed file was. */
    val from: String?,
    val change: String,
    val added: Int,
    val removed: Int,
    val media: CodeMedia?,
    val lines: List<String>,
    private val kinds: IntArray,
    /** Each line's number in the file as it was and as it is, 0 where it isn't in that one. */
    val old: IntArray,
    val new: IntArray,
    /** The length of the longest line, in columns. */
    val columns: Int,
) {
    enum class Kind {
        Unchanged, Added, Removed,
        /** The heading of a hunk, or a note in place of the lines. */
        Note,
    }

    /** The highlighting of each line once it has arrived, as the core's span triples. */
    var spans by mutableStateOf<List<IntArray>>(emptyList())

    fun kind(line: Int): Kind = Kind.entries.getOrNull(kinds.getOrElse(line) { 0 }) ?: Kind.Unchanged

    /** The line as GitHub counts it, and on which side, for a comment on it. */
    fun commentable(line: Int): CommentableLine? {
        if (line !in lines.indices) return null
        return when (kind(line)) {
            Kind.Note -> null
            Kind.Removed -> old[line].takeIf { it > 0 }?.let { CommentableLine(path, it, "left", lines[line]) }
            Kind.Added, Kind.Unchanged -> new[line].takeIf { it > 0 }?.let { CommentableLine(path, it, "right", lines[line]) }
        }
    }

    companion object {
        /** A file with more lines than this starts closed in a diff. */
        const val OPEN_UP_TO_LINES = 1500

        fun diff(json: JSON): CodeFile {
            val path = json.string("path")
            val change = json.string("change")
            val blob = json.string("blob")
            val kind = json.string("media")
            val media = if (blob.isEmpty() || kind.isEmpty()) null else CodeMedia(blob, kind == "video")
            val from = json.optionalString("from")
            val added = json.int("added")
            val removed = json.int("removed")
            if (media != null) return CodeFile(path, from, change, added, removed, media, emptyList(), IntArray(0), IntArray(0), IntArray(0), 0)
            val lines = json.strings("lines")
            if (lines.isEmpty()) {
                val note = if (json.bool("binary")) "Binary file" else if (change == "renamed") "Renamed without changes" else "Empty file"
                return CodeFile(path, from, change, added, removed, null, listOf(note), intArrayOf(Kind.Note.ordinal), IntArray(1), IntArray(1), note.length)
            }
            return CodeFile(path, from, change, added, removed, null, lines, json.ints("kinds"), json.ints("old"), json.ints("new"), widest(lines))
        }

        /** A whole file, every line as it is. */
        fun whole(path: String, lines: List<String>) = CodeFile(
            path, null, "", 0, 0, null, lines, IntArray(lines.size), IntArray(lines.size), IntArray(lines.size) { it + 1 }, widest(lines),
        )

        /** Counts what is wider than a letter as two columns, and a tab as four. */
        private fun widest(lines: List<String>): Int {
            var widest = 0
            for (line in lines) {
                var columns = 0
                for (unit in line) {
                    columns += when {
                        unit == '\t' -> 4
                        unit.code < 0x2e80 -> 1
                        else -> 2
                    }
                }
                widest = maxOf(widest, columns)
            }
            return widest
        }
    }
}

/** A line of a file that a comment can be written on: its number as GitHub counts it, "left" or "right", and its text. */
data class CommentableLine(val path: String, val line: Int, val side: String, val code: String)

class CodeDocument(
    /** Names what it shows. A document with the same name takes the place of the one before it without the view moving. */
    val id: String,
    val files: List<CodeFile>,
    val truncated: Boolean,
    /** Every file is under a heading with its name, as in a diff. */
    val headed: Boolean,
) {
    val added: Int get() = files.sumOf { it.added }
    val removed: Int get() = files.sumOf { it.removed }

    companion object {
        fun diff(json: JSON, id: String) = CodeDocument(id, json.objects("files").map(CodeFile::diff), json.bool("truncated"), headed = true)
    }
}

/**
 * What a pull request's diff shows besides its lines: which files the user has viewed, and the
 * lines that can have comments and those that have some.
 */
data class CodeMarks(
    /** Each file's heading has a box to mark it viewed. */
    val viewable: Boolean = false,
    val viewed: Set<String> = emptySet(),
    /** A line's number can be tapped to comment on it. */
    val commentable: Boolean = false,
    /** The lines with conversations or comments waiting, by file, by their index in it. */
    val marked: Map<String, Set<Int>> = emptyMap(),
)

/** A line of a document: the file, by its index, and the line's index in it. */
data class CodePlace(val file: Int, val line: Int)

/** The lines added and removed, as they are written everywhere: `+12 −3`. */
object LineCountText {
    val style: TextStyle get() = Theme.uiDigits(11.5f, FontWeight.Medium)

    @Composable
    @ReadOnlyComposable
    fun text(added: Int, removed: Int): AnnotatedString {
        val palette = LocalPalette.current
        return buildAnnotatedString {
            if (added > 0 || removed == 0) withStyle(SpanStyle(color = palette.success)) { append("+$added") }
            if (removed > 0 || added == 0) {
                val space = if (length > 0) " " else ""
                withStyle(SpanStyle(color = palette.destructive)) { append("$space−$removed") }
            }
        }
    }
}
