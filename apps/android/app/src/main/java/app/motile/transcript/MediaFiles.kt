package app.motile.transcript

import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.util.LruCache
import android.webkit.MimeTypeMap
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.core.content.FileProvider
import app.motile.core.AppStore
import app.motile.core.Pictures
import app.motile.core.Preferences
import app.motile.platform.Platform
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import java.io.File
import kotlin.coroutines.resume

/** The file of an image or a video the open thread shows, which the core fetches from its server if this device doesn't have it. */
suspend fun AppStore.mediaFile(id: String): File? = suspendCancellableCoroutine { waiting ->
    media(id) { file -> if (waiting.isActive) waiting.resume(file) }
}

/** Decodes pictures off the main thread, at the size they are shown, and keeps the latest ones. */
object PictureCache {
    private val cache = object : LruCache<String, ImageBitmap>(256 * 1024 * 1024) {
        override fun sizeOf(key: String, value: ImageBitmap): Int = value.width * value.height * 4
    }

    fun cached(key: String?): ImageBitmap? = key?.let { cache.get(it) }

    /** The image, no larger than `maxPixels` on a side, kept under `key`. */
    suspend fun decode(file: File, key: String, maxPixels: Int): ImageBitmap? = withContext(Dispatchers.IO) {
        Pictures.decode(file, maxPixels)?.asImageBitmap()?.also { cache.put(key, it) }
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

    /** Puts the file on the clipboard, which apps that take images and videos paste. */
    fun copy(file: File, name: String) = named(file, name) { copy ->
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        clipboard.setPrimaryClip(ClipData.newUri(context.contentResolver, name, uri(copy)))
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
    private var downloads by mutableStateOf<Map<String, String>?>(null)

    private val downloaded: Map<String, String>
        get() = downloads ?: Preferences.strings(KEY).also { downloads = it }

    fun state(id: String, store: AppStore): State {
        if (id in fetching) return State.Fetching(store.mediaProgress[id] ?: 0.0)
        downloaded[id]?.let(::File)?.takeIf { it.exists() }?.let { return State.Downloaded(it) }
        failures[id]?.let { return State.Failed(it) }
        return State.Remote
    }

    /** Fetches the file and keeps it as a download. */
    fun download(file: SentFile, store: AppStore, then: ((File) -> Unit)? = null) {
        fetch(file, store) { fetched ->
            val kept = downloaded + (file.id to fetched.path)
            downloads = kept
            Preferences.set(KEY, kept)
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
