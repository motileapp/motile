package app.motile.transcript

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.AppStore
import app.motile.core.AttachedFile
import app.motile.core.ViewedMedia
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.Theme
import app.motile.ui.ContextMenuArea
import app.motile.ui.MenuBuilder
import app.motile.ui.Text
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import java.io.File

/**
 * The files attached to a message, in its bubble: the images and videos as tiles that open in the
 * viewer, and the others by name under them. The tiles are all one size, so the row knows its
 * height before any picture is on this device.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun AttachedFiles(files: List<AttachedFile>, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val pictured = files.filter { it.media != null }
    val names = files.filter { it.media == null }.joinToString("   ") { "📎 ${it.name}" }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(AttachedFiles.gap)) {
        if (pictured.isNotEmpty()) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(AttachedFiles.gap), verticalArrangement = Arrangement.spacedBy(AttachedFiles.gap)) {
                pictured.forEachIndexed { index, file ->
                    Tile(file) {
                        val viewed = files.mapNotNull { shown -> shown.media?.let { ViewedMedia(shown.name, shown.video, ViewedMedia.Source.Media(it)) } }
                        store.view(viewed, index)
                    }
                }
            }
        }
        if (names.isNotEmpty()) Text(names, style = Theme.smallFont, color = palette.mutedForeground, maxLines = 1)
    }
}

object AttachedFiles {
    val tile = 96.dp
    val gap = 6.dp
}

/** One image or video: all of its picture, fitted in the tile, with a play sign on a video. */
@Composable
private fun Tile(file: AttachedFile, open: () -> Unit) {
    val store = LocalStore.current
    val scope = rememberCoroutineScope()
    val key = file.picture?.let { "tile:$it" }
    val maxPixels = with(LocalDensity.current) { (AttachedFiles.tile * 2).roundToPx() }
    val picture by produceState(PictureCache.cached(key), key) {
        val id = file.picture ?: return@produceState
        if (value != null || key == null) return@produceState
        val fetched = store.mediaFile(id) ?: return@produceState
        value = PictureCache.decode(fetched, key, maxPixels)
    }
    ContextMenuArea(
        items = { mediaActions(file.video, file.name) { done -> file.media?.let { fetchThen(store, scope, it, done) } } },
        modifier = Modifier.size(AttachedFiles.tile).semantics { contentDescription = file.name },
        onClick = open,
    ) {
        PictureBox(picture, Modifier.fillMaxSize()) {
            if (file.video) OverlayButton(Symbol.Play, "Play", ControlSize.Regular, Modifier.align(Alignment.Center), onClick = open)
        }
    }
}

/** Fetches the file of the media and hands it over, when it can be had. */
fun fetchThen(store: AppStore, scope: CoroutineScope, id: String, done: (File) -> Unit) {
    scope.launch { store.mediaFile(id)?.let(done) }
}

/** What a long press on an image or a video offers. `fetch` hands over its file. */
fun MenuBuilder.mediaActions(video: Boolean, name: String, fetch: ((File) -> Unit) -> Unit) {
    item(if (video) "Copy Video" else "Copy Image", Symbol.Copy) { fetch { MediaFiles.copy(it, name) } }
    item(MediaFiles.saveTitle, Symbol.Share) { fetch { MediaFiles.share(it, name) } }
}
