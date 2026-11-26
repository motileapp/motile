package app.motile.transcript

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
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
import app.motile.ui.ContextMenuArea
import app.motile.ui.MenuBuilder
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import java.io.File

/**
 * The files attached to a message, in its bubble: the images and videos as tiles that open in the
 * viewer, and the others under them as the cards an agent's files are, side by side where they fit.
 * The tiles and the cards are all one size, so the row knows its height before any picture is on
 * this device. `piece` is given each file's index among `files`, for its tile or card.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun AttachedFiles(files: List<AttachedFile>, modifier: Modifier = Modifier, piece: (Int) -> Modifier = { Modifier }) {
    val store = LocalStore.current
    val pictured = files.withIndex().filter { it.value.pictured }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(AttachedFiles.gap)) {
        if (pictured.isNotEmpty()) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(AttachedFiles.gap), verticalArrangement = Arrangement.spacedBy(AttachedFiles.gap)) {
                pictured.forEachIndexed { index, (at, file) ->
                    Tile(file, piece(at)) {
                        val viewed = pictured.mapNotNull { (_, shown) -> shown.media?.let { ViewedMedia(shown.name, shown.video, ViewedMedia.Source.Media(it)) } }
                        store.view(viewed, index)
                    }
                }
            }
        }
        val named = files.withIndex().filter { !it.value.pictured }
        if (named.isNotEmpty()) AttachedCards(named, piece)
    }
}

/** As many cards to a line as fit at their narrowest, each as wide as its share up to the widest. */
@Composable
private fun AttachedCards(files: List<IndexedValue<AttachedFile>>, piece: (Int) -> Modifier) {
    BoxWithConstraints {
        val gap = AttachedFiles.gap
        val fit = ((maxWidth + gap) / (FileRow.narrowest + gap)).toInt()
        val columns = fit.coerceIn(1, files.size)
        val share = ((maxWidth - gap * (columns - 1)) / columns).value.toInt().dp
        val width = minOf(share, FileRow.widest)
        Column(verticalArrangement = Arrangement.spacedBy(gap)) {
            for (line in files.chunked(columns)) {
                Row(horizontalArrangement = Arrangement.spacedBy(gap)) {
                    for ((at, file) in line) {
                        val download = file.download
                        FileCard(download ?: SentFile("", file.size, "", file.name), piece(at).width(width), kept = download != null)
                    }
                }
            }
        }
    }
}

/** The file to download, when its server kept it. */
private val AttachedFile.download: SentFile?
    get() {
        if (!file) return null
        val id = media ?: return null
        return SentFile(id, size, "", name)
    }

object AttachedFiles {
    val tile = 96.dp
    val gap = 6.dp
}

/** One image or video: all of its picture, fitted in the tile, with a play sign on a video. */
@Composable
private fun Tile(file: AttachedFile, modifier: Modifier, open: () -> Unit) {
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
        modifier = modifier.size(AttachedFiles.tile).semantics { contentDescription = file.name },
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
