package app.motile.panel

import app.motile.media.mediaSource
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Gallery
import app.motile.core.GalleryMedia
import app.motile.core.addToMessage
import app.motile.core.showInChat
import app.motile.core.watchGallery
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.IconSize
import app.motile.theme.LocalDark
import app.motile.theme.LocalPalette
import app.motile.theme.Opacity
import app.motile.theme.TextSize
import app.motile.theme.value
import app.motile.transcript.PictureCache
import app.motile.transcript.mediaFile
import app.motile.ui.ContextMenuArea
import app.motile.ui.Icon
import app.motile.ui.Text
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

private const val EMPTY = "The images and videos of the thread show up here."

/**
 * The images and videos of the thread, the newest first, in squares that fill the panel's width.
 * They are watched for while the panel is in view.
 */
@Composable
fun MediaSurface(threadID: String?) {
    val store = LocalStore.current
    val watched = threadID?.takeIf { store.rightPanel.isOpen }
    DisposableEffect(watched) {
        if (watched == null) return@DisposableEffect onDispose {}
        store.watchGallery(watched, true)
        onDispose { store.watchGallery(watched, false) }
    }
    val gallery = store.gallery?.takeIf { it.threadID == threadID }
    when {
        threadID == null -> PanelMessage(EMPTY)
        gallery == null -> PanelLoading()
        gallery.media.isEmpty() -> PanelMessage(EMPTY)
        else -> MediaGrid(gallery)
    }
}

object MediaGrid {
    /** How narrow a square gets before a column goes. */
    private const val SMALLEST_SIDE = 128f
    private const val FEWEST_COLUMNS = 3
    /** The gap between squares, which is only drawn: each square takes presses up to its neighbours'. */
    val gap = 2.dp

    fun columns(width: Float): Int = max(FEWEST_COLUMNS, (width / SMALLEST_SIDE).toInt())

    /**
     * How large a picture is decoded on its longer side, for its shorter one to fill a square `side`
     * dp wide: in steps, so that resizing the panel doesn't decode it again at every size.
     */
    fun decodedPixels(side: Float, density: Float, pixels: Pair<Float, Float>?): Int {
        val step = 64f
        val square = ceil(side / step) * step * density
        val (width, height) = pixels ?: return (square * 2).roundToInt()
        if (width <= 0 || height <= 0) return (square * 2).roundToInt()
        return (square * min(max(width, height) / min(width, height), 4f)).roundToInt()
    }
}

@Composable
private fun MediaGrid(gallery: Gallery) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    Column(Modifier.fillMaxSize()) {
        PanelBar {
            Text(gallery.summary, Modifier.weight(1f), size = TextSize.Xs, weight = FontWeight.Medium, color = palette.mutedForeground, maxLines = 1)
        }
        BoxWithConstraints(Modifier.fillMaxSize()) {
            // Half a gap at the edges, which the squares' own halves make as wide as the gap between them.
            val edge = MediaGrid.gap / 2
            val width = maxWidth - MediaGrid.gap
            val columns = MediaGrid.columns(width.value)
            val side = width / columns
            val padding = PaddingValues(start = edge, end = edge, top = edge, bottom = panelEndRoom)
            LazyVerticalGrid(GridCells.Fixed(columns), Modifier.fillMaxSize(), contentPadding = padding) {
                itemsIndexed(gallery.media, key = { _, media -> media.id }) { index, media ->
                    MediaSquare(media, side) { store.view(gallery.media.map { it.viewed }, index) }
                }
            }
        }
    }
}

/**
 * One image or video, cropped to a square, its picture decoded off the main thread at the square's size
 * and at opacity lit-picture while pressed or under the pointer.
 */
@Composable
private fun MediaSquare(media: GalleryMedia, side: Dp, open: () -> Unit) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val pixels = MediaGrid.decodedPixels(side.value, LocalDensity.current.density, media.pixels)
    val key = media.picture?.let { "$it#square$pixels" }
    // What was decoded for another size stays until the picture for this one is there.
    var picture by remember(media.picture) { mutableStateOf(PictureCache.cached(key)) }
    var failed by remember(media.picture) { mutableStateOf(false) }
    LaunchedEffect(key) {
        val id = media.picture ?: return@LaunchedEffect
        val kept = key ?: return@LaunchedEffect
        val decoded = PictureCache.cached(kept) ?: store.mediaFile(id)?.let { PictureCache.decode(it, kept, pixels) }
        if (decoded != null) picture = decoded
        failed = picture == null
    }
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val hovering by interaction.collectIsHoveredAsState()
    val lit = Opacity.LitPicture.value(LocalDark.current)
    ContextMenuArea(
        items = {
            item("Show in Chat", Symbol.MessageSquareText) { store.showInChat(media.shownIn) }
            item("Add to Message", Symbol.Paperclip) { store.addToMessage(media.media, media.name) }
        },
        modifier = Modifier.aspectRatio(1f).semantics { contentDescription = media.name },
        onClick = open,
        interaction = interaction,
    ) {
        val square = Modifier.fillMaxSize().padding(MediaGrid.gap / 2).mediaSource(media.viewed, picture, fills = true)
        Box(square.background(palette.backgroundSecondary), contentAlignment = Alignment.Center) {
            val shown = picture
            if (shown != null) {
                Image(shown, null, Modifier.fillMaxSize().alpha(if (pressed || hovering) lit else 1f), contentScale = ContentScale.Crop)
                if (media.video) PlayMark()
            } else if (media.picture == null || failed) {
                Icon(if (media.video) Symbol.Play else Symbol.Image, IconSize.Lg, color = palette.mutedForeground)
            }
        }
    }
}

/** What a video's square shows over its poster, as a reply's video does. */
@Composable
private fun PlayMark() {
    val palette = LocalPalette.current
    val size = ControlSize.Large
    Box(
        Modifier
            .size(size.height)
            .background(palette.overlayButton, CircleShape)
            .border(1.dp, palette.borderOverlayButton, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Icon(Symbol.Play, size.wordlessSymbol, color = palette.foreground)
    }
}
