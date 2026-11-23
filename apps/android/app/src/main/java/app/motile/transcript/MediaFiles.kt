package app.motile.transcript

import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Intent
import android.util.LruCache
import android.webkit.MimeTypeMap
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.core.content.FileProvider
import app.motile.core.AppStore
import app.motile.core.Pictures
import app.motile.core.Preferences
import app.motile.platform.Platform
import app.motile.theme.Palette
import app.motile.theme.Theme
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import java.io.File
import kotlin.coroutines.resume

/** The file of an image or a video the open thread shows, which the core fetches from its server if this device doesn't have it. */
suspend fun AppStore.mediaFile(id: String): File? = fetchedMedia(id).getOrNull()

/** The file, or why it can't be had. */
suspend fun AppStore.fetchedMedia(id: String): Result<File> = suspendCancellableCoroutine { waiting ->
    fetchMedia(id) { result -> if (waiting.isActive) waiting.resume(result) }
}

/** Decodes pictures off the main thread, at the size they are shown, and keeps the latest ones. */
object PictureCache {
    private val cache = object : LruCache<String, ImageBitmap>(256 * 1024 * 1024) {
        override fun sizeOf(key: String, value: ImageBitmap): Int = value.width * value.height * 4
    }
    /** How light what each picture with see-through parts draws is, by the picture's key. */
    private val inks = LruCache<String, Double>(1024)
    /** Each SVG's own size, by the picture's key. */
    private val svgSizes = LruCache<String, Pair<Float, Float>>(256)

    /** An SVG is kept for the appearance it was drawn in, since its `currentColor` is the text's. */
    private fun svgKey(key: String, dark: Boolean) = "$key#${if (dark) "dark" else "light"}"

    fun cached(key: String?): ImageBitmap? = key?.let { cache.get(it) ?: cache.get(svgKey(it, Theme.dark)) }

    fun put(key: String, picture: ImageBitmap) {
        cache.put(key, picture)
    }

    /** The image, no larger than `maxPixels` on a side, kept under `key`. */
    suspend fun decode(file: File, key: String, maxPixels: Int): ImageBitmap? = withContext(Dispatchers.IO) {
        val svg = file.extension.equals("svg", ignoreCase = true)
        val kept = if (svg) svgKey(key, Theme.dark) else key
        if (svg) runCatching { Pictures.svgSize(file.readText()) }.getOrNull()?.let { svgSizes.put(key, it) }
        val bitmap = Pictures.decode(file, maxPixels) ?: return@withContext null
        Pictures.inkLuminance(bitmap)?.let { inks.put(kept, it) }
        bitmap.asImageBitmap().also { cache.put(kept, it) }
    }

    /** How light what the picture kept under `key` draws is, when it has see-through parts. */
    fun ink(key: String?): Double? = key?.let { inks.get(it) ?: inks.get(svgKey(it, Theme.dark)) }

    /** The own size of the SVG kept under `key`. */
    fun svgSize(key: String): Pair<Float, Float>? = svgSizes.get(key)

    /** The checkerboard behind the picture kept under `key`: `checker(ink)` of what it draws. */
    fun checker(key: String?, palette: Palette): Checker = checker(ink(key), palette)

    /**
     * The checkerboard behind a picture: the dark one under what draws in light colours, `ink`, the
     * light one under anything else.
     */
    fun checker(ink: Double?, palette: Palette): Checker {
        ink ?: return Checker.Light
        val onDark = Pictures.onDark(ink, Checker.Dark.luminance(palette), Checker.Light.luminance(palette))
        return if (onDark) Checker.Dark else Checker.Light
    }
}

/** What is done with the file of an image, a video or a file an agent sent, outside the client. */
object MediaFiles {
    const val saveTitle = "Share"
    const val revealTitle = "Share"

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private val context get() = Platform.context

    fun mime(name: String): String? =
        MimeTypeMap.getSingleton().getMimeTypeFromExtension(name.substringAfterLast('.', "").lowercase())

    /** A copy of the file under its name, in a folder the share sheet can hand to other apps, for them to know it by it. */
    fun namedCopy(file: File, name: String): File {
        val folder = File(context.cacheDir, "shared").apply { mkdirs() }
        val copy = File(folder, name.ifEmpty { file.name })
        return runCatching {
            copy.delete()
            file.copyTo(copy, overwrite = true)
        }.getOrDefault(file)
    }

    private fun uri(file: File) = FileProvider.getUriForFile(context, "${context.packageName}.files", file)

    private fun named(file: File, name: String, then: (File) -> Unit) {
        scope.launch {
            val copy = withContext(Dispatchers.IO) { namedCopy(file, name) }
            runCatching { then(copy) }
        }
    }

    /**
     * Puts the file on the clipboard, which apps that take images and videos paste. An SVG goes with
     * its text, for what reads it.
     */
    fun copy(file: File, name: String) = named(file, name) { copy ->
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        if (!copy.extension.equals("svg", ignoreCase = true)) {
            return@named clipboard.setPrimaryClip(ClipData.newUri(context.contentResolver, name, uri(copy)))
        }
        scope.launch {
            val text = withContext(Dispatchers.IO) { runCatching { copy.readText() }.getOrNull() }
            val description = ClipDescription(name, arrayOf("image/svg+xml", ClipDescription.MIMETYPE_TEXT_PLAIN))
            clipboard.setPrimaryClip(ClipData(description, ClipData.Item(text, null, uri(copy))))
        }
    }

    /** Opens the share sheet on a copy of the file that has its name, which is where it can be saved too. */
    fun share(file: File, name: String) = named(file, name) { copy -> Platform.share(copy, mime(name), name) }

    /** Opens the file in the app that shows its kind, or the share sheet when none does. */
    fun open(file: File, name: String) = named(file, name) { copy ->
        val view = Intent(Intent.ACTION_VIEW)
            .setDataAndType(uri(copy), mime(name) ?: "*/*")
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        try {
            context.startActivity(view)
        } catch (_: ActivityNotFoundException) {
            Platform.share(copy, mime(name), name)
        }
    }
}

/**
 * The files agents sent that are downloading or were downloaded, by their ids. A download goes on
 * while its row is off screen, and where it went is remembered across launches. As on iOS, a
 * download is kept in the client, where the core put it.
 */
object FileDownloads {
    sealed interface State {
        data object Remote : State
        data class Fetching(val fraction: Double) : State
        data class Failed(val reason: String) : State
        data class Downloaded(val file: File) : State
    }

    private const val KEY = "downloadedFiles"
    private val fetching = mutableStateMapOf<String, Unit>()
    private val failures = mutableStateMapOf<String, String>()
    /** Goes up when a fetch is stopped, so that its answer is not taken for a later one's. */
    private val generations = HashMap<String, Int>()
    /** Where the files downloaded in earlier launches are, and those downloaded since. */
    private val saved by lazy { Preferences.strings(KEY) }
    private val added = mutableStateMapOf<String, String>()

    private fun path(id: String): String? = added[id] ?: saved[id]

    fun state(id: String, store: AppStore): State {
        if (id in fetching) return State.Fetching(store.mediaProgress[id] ?: 0.0)
        path(id)?.let(::File)?.takeIf { it.exists() }?.let { return State.Downloaded(it) }
        failures[id]?.let { return State.Failed(it) }
        return State.Remote
    }

    /** Fetches the file and keeps it as a download. */
    fun download(file: SentFile, store: AppStore, then: ((File) -> Unit)? = null) {
        fetch(file, store) { fetched ->
            added[file.id] = fetched.path
            Preferences.set(KEY, saved + added)
            then?.invoke(fetched)
        }
    }

    /** Hands over the client's copy of the file, which the core fetches if this device doesn't have it. */
    fun fetch(file: SentFile, store: AppStore, done: (File) -> Unit) {
        val generation = generations[file.id] ?: 0
        fetching[file.id] = Unit
        failures.remove(file.id)
        store.fetchMedia(file.id) { result ->
            if ((generations[file.id] ?: 0) != generation) return@fetchMedia
            fetching.remove(file.id)
            result.onSuccess(done).onFailure { failures[file.id] = it.message ?: "Something went wrong." }
        }
    }

    fun cancel(id: String, store: AppStore) {
        if (fetching.remove(id) == null) return
        generations[id] = (generations[id] ?: 0) + 1
        store.cancelMedia(id)
    }
}
