package app.motile.core

import android.graphics.Bitmap
import android.graphics.ImageDecoder
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.transcript.canvas.Scene
import java.io.File
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonObject

/** What a mark is drawn with. */
enum class MarkupTool(val symbol: Symbol, val title: String) {
    Pen(Symbol.Pencil, "Pen"),
    Arrow(Symbol.MoveUpRight, "Arrow"),
    Box(Symbol.Square, "Box"),
    Pin(Symbol.MapPin, "Pin a Note");

    val key: String get() = name.lowercase()
}

/** An image marked up and written out for the composer, with what to go on from. */
class MarkupResult(val file: File, val notes: String?, val saved: SavedMarkup)

/** What a marked-up attachment keeps to be marked up again: the image before the marks, and them. */
data class SavedMarkup(val original: File, val name: String, val marks: String, val notes: String?)

/**
 * An image being marked up in the viewer, through the core (`markup::markup_json`): the pointer
 * goes in, in the image's pixels, and the scene of its marks comes back to paint over it.
 */
class MarkupSession private constructor(
    val source: File,
    val name: String,
    /** The composer's attachment it replaces, when it is one. */
    val attachment: String?,
) {
    data class Pin(val mark: Int, val number: Int, val x: Float, val y: Float, val radius: Float, val note: String, val ink: String)

    private var id = 0L
    var width by mutableFloatStateOf(1f)
        private set
    var height by mutableFloatStateOf(1f)
        private set
    var scene by mutableStateOf<Scene?>(null)
        private set
    var pins by mutableStateOf(emptyList<Pin>())
        private set
    var selected by mutableStateOf<Int?>(null)
        private set
    var canUndo by mutableStateOf(false)
        private set
    var canRedo by mutableStateOf(false)
        private set
    var inks by mutableStateOf(emptyList<String>())
        private set
    /** Whether anything was drawn since it opened, which cancelling throws away. */
    var changed by mutableStateOf(false)
        private set
    var finishing by mutableStateOf(false)
        private set
    var tool by mutableStateOf(lastTool)
    var ink by mutableStateOf(lastInk)
    /** The selected pin's note as it is being written. */
    var note by mutableStateOf("")

    val selectedPin: Pin? get() = pins.firstOrNull { it.mark == selected }

    fun pick(tool: MarkupTool) {
        this.tool = tool
        lastTool = tool
    }

    fun pick(ink: String) {
        this.ink = ink
        lastInk = ink
    }

    fun down(x: Float, y: Float, slop: Float) {
        commitNote()
        send("op" to "down", "tool" to tool.key, "ink" to ink, "x" to x, "y" to y, "slop" to slop)
        changed = true
    }

    fun drag(x: Float, y: Float) = send("op" to "move", "x" to x, "y" to y)

    fun up(x: Float, y: Float) = send("op" to "up", "x" to x, "y" to y)

    /** Drops the mark the pointer started, as a pinch does. */
    fun cancel() = send("op" to "cancel")

    /** Keeps the selected pin's note and puts the pin down. */
    fun deselect() {
        commitNote()
        send("op" to "select")
    }

    fun removeSelected() {
        val selected = selected ?: return
        send("op" to "remove", "mark" to selected)
    }

    fun undo() {
        commitNote()
        send("op" to "undo")
    }

    fun redo() {
        commitNote()
        send("op" to "redo")
    }

    /** Writes the image out with its marks under `folder`, off the main thread; nothing when it couldn't. */
    suspend fun finish(folder: File): MarkupResult? {
        commitNote()
        finishing = true
        val result = withContext(Dispatchers.IO) { flatten(folder) }
        finishing = false
        return result
    }

    fun close() {
        ask(json("op" to "close", "id" to id))
    }

    private fun flatten(folder: File): MarkupResult? {
        val readable = readable(source, folder) ?: return null
        val answer = ask(json("op" to "flatten", "id" to id, "source" to readable.path, "folder" to folder.path, "name" to name)) ?: return null
        val path = answer.optionalString("file") ?: return null
        val marks = answer.optionalString("marks") ?: return null
        val file = File(path)
        val original = File(File(file.parentFile, "original"), source.name)
        runCatching { source.copyTo(original, overwrite = true) }.getOrElse { return null }
        val notes = answer.optionalString("notes")
        return MarkupResult(file, notes, SavedMarkup(original, name, marks, notes))
    }

    private fun commitNote() {
        val pin = selectedPin ?: return
        if (pin.note == note.trim()) return
        send("op" to "note", "mark" to pin.mark, "text" to note)
    }

    private fun send(vararg input: Pair<String, Any?>) {
        apply(ask(json(*input, "id" to id)))
    }

    private fun apply(state: JSON?): Boolean {
        val scene = state?.obj("scene")?.let(::Scene) ?: return false
        val wasSelected = selected
        id = state.long("id")
        width = state.double("width").toFloat()
        height = state.double("height").toFloat()
        this.scene = scene
        pins = state.objects("pins").map {
            Pin(it.int("mark"), it.int("number"), it.double("x").toFloat(), it.double("y").toFloat(), it.double("radius").toFloat(), it.string("note"), it.string("ink"))
        }
        selected = state.optionalInt("selected")
        canUndo = state.bool("can_undo")
        canRedo = state.bool("can_redo")
        inks = state.strings("inks")
        if (selected != wasSelected) note = selectedPin?.note ?: ""
        return true
    }

    companion object {
        private var lastTool = MarkupTool.Pen
        private var lastInk = "red"
        /** What the core reads; anything else, like HEIC, is written out as a PNG for it first. */
        private val decodable = setOf("png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff")

        /** `width` and `height` are the pixels of the picture shown; `saved` goes on from an earlier markup. */
        fun open(source: File, name: String, attachment: String?, width: Int, height: Int, saved: SavedMarkup?): MarkupSession? {
            val session = MarkupSession(source, name, attachment)
            val open = json("op" to "open", "width" to width, "height" to height, "marks" to saved?.marks)
            return session.takeIf { it.apply(ask(open)) }
        }

        /** The composer's text with an image's notes in the place of the ones it had (`markup::place_notes`). */
        fun placingNotes(notes: String?, earlier: String?, text: String): String {
            val answer = ask(json("op" to "place_notes", "text" to text, "notes" to notes, "earlier" to earlier))
            return answer?.optionalString("text") ?: text
        }

        private fun ask(input: JsonObject): JSON? = parseJSON(Native.markup(input.toString()))?.takeIf { it["error"] == null }

        private fun readable(source: File, folder: File): File? {
            if (source.extension.lowercase() in decodable) return source
            return runCatching {
                val bitmap = ImageDecoder.decodeBitmap(ImageDecoder.createSource(source)) { decoder, _, _ ->
                    decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
                }
                val png = File(File(folder, UUID.randomUUID().toString()).apply { mkdirs() }, "${source.nameWithoutExtension}.png")
                png.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
                png
            }.getOrNull()
        }

        /** Where marked-up images are written. */
        val folder: File get() = File(Platform.context.cacheDir, "attachments/markup")
    }
}
