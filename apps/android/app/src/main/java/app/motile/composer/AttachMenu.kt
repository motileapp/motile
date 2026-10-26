package app.motile.composer

import android.content.pm.PackageManager
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.core.content.FileProvider
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.Theme
import app.motile.ui.ActionMenu
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.UUID

/**
 * The button that attaches files to what is being written: from the photo library, from the
 * camera, or from the files of the device and its apps. What is picked is copied into the client's
 * own folder first, which is where the core reads it from.
 */
@Composable
fun AttachMenu() {
    val store = LocalStore.current
    val scope = rememberCoroutineScope()
    val attach = { uris: List<Uri> ->
        if (uris.isNotEmpty()) {
            scope.launch {
                val copies = withContext(Dispatchers.IO) { uris.mapNotNull(PickedFile::copy) }
                store.attach(copies)
            }
        }
    }
    val photos = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia(maxItems = 10)) { attach(it) }
    val files = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { attach(it) }
    var photoPath by rememberSaveable { mutableStateOf<String?>(null) }
    val camera = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { taken ->
        val photo = photoPath?.let(::File)
        photoPath = null
        if (taken && photo != null && photo.length() > 0) store.attach(listOf(photo))
    }
    val hasCamera = remember { Platform.context.packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY) }

    ActionMenu(
        icon = Symbol.Plus, help = "Attach files", round = true, tint = Theme.colors.foreground,
        margin = ComposerView.margin(leading = 8.dp),
    ) {
        item("Photo Library", Symbol.Images) {
            photos.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageAndVideo))
        }
        if (hasCamera) {
            item("Take Photo", Symbol.Camera) {
                val photo = PickedFile.newPhoto()
                photoPath = photo.path
                camera.launch(FileProvider.getUriForFile(Platform.context, "${Platform.context.packageName}.files", photo))
            }
        }
        item("Choose Files", Symbol.Folder) { files.launch(arrayOf("*/*")) }
    }
}

/** A file the user picked, copied to where the client can keep reading it. */
object PickedFile {
    private val folder: File get() = File(Platform.context.cacheDir, "attachments")

    /** Every file gets a folder of its own, so that two with one name are two files. */
    private fun newFolder(): File = File(folder, UUID.randomUUID().toString()).apply { mkdirs() }

    /** Copies what the picker gave into a file of the client's, under the name it goes by. */
    fun copy(uri: Uri): File? = runCatching {
        val resolver = Platform.context.contentResolver
        val copy = File(newFolder(), name(uri))
        val input = resolver.openInputStream(uri) ?: return null
        input.use { source -> copy.outputStream().use { source.copyTo(it) } }
        copy
    }.getOrNull()

    /** A file for the camera to take a photo into. */
    fun newPhoto(): File {
        val stamp = SimpleDateFormat("yyyy-MM-dd 'at' HH.mm.ss", Locale.US).format(Date())
        return File(newFolder(), "Photo $stamp.jpg")
    }

    private fun name(uri: Uri): String {
        val resolver = Platform.context.contentResolver
        val shown = runCatching {
            resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) cursor.getString(0) else null
            }
        }.getOrNull()
        val name = shown?.substringAfterLast('/')?.takeIf { it.isNotBlank() } ?: uri.lastPathSegment?.substringAfterLast('/') ?: "File"
        if (name.contains('.')) return name
        val extension = resolver.getType(uri)?.let { android.webkit.MimeTypeMap.getSingleton().getExtensionFromMimeType(it) } ?: return name
        return "$name.$extension"
    }
}
