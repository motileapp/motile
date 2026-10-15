package app.motile.transcript

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.ViewedMedia
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.Theme
import app.motile.ui.ContextMenuArea
import app.motile.ui.Spinner
import app.motile.ui.Text
import kotlin.math.max

private sealed interface MediaState {
    data object Loading : MediaState
    data object Shown : MediaState
    data class Failed(val why: String) : MediaState
}

/** An image or a video a reply shows, in a box of its size; a press opens it in the viewer, which plays a video. */
@Composable
fun MediaRow(media: MediaContent) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val scope = rememberCoroutineScope()
    var picture by remember(media.id) { mutableStateOf(PictureCache.cached(media.picture)) }
    var state by remember(media.id) { mutableStateOf<MediaState>(MediaState.Shown) }
    var attempt by remember(media.id) { mutableIntStateOf(0) }
    val maxPixels = with(LocalDensity.current) {
        val (width, height) = media.box(Theme.contentWidth.value)
        (max(width, height).dp).roundToPx()
    }
    LaunchedEffect(media.id, attempt) {
        val pictureID = media.picture ?: return@LaunchedEffect
        if (picture != null) return@LaunchedEffect
        state = MediaState.Loading
        val file = store.mediaFile(pictureID)
        if (file == null) {
            state = if (media.video) MediaState.Shown else MediaState.Failed("This image couldn't be loaded.")
            return@LaunchedEffect
        }
        val image = PictureCache.decode(file, pictureID, maxPixels)
        picture = image
        state = if (image == null && !media.video) MediaState.Failed("This image couldn't be shown.") else MediaState.Shown
    }
    val open = {
        if (media.video || picture != null) store.view(listOf(mediaViewed(media)), 0)
    }
    Box(Modifier.fillMaxWidth().padding(vertical = 6.dp)) {
        ContextMenuArea(
            items = { mediaActions(media.video, media.name) { done -> fetchThen(store, scope, media.id, done) } },
            modifier = Modifier.boxed(media).semantics { contentDescription = media.alt.ifEmpty { media.name } },
            onClick = open,
        ) {
            PictureBox(picture, Modifier.fillMaxSize()) {
                if (media.video) OverlayButton(Symbol.Play, "Play", ControlSize.Large, Modifier.align(Alignment.Center), onClick = open)
                if (media.video && media.poster == null) {
                    Text(
                        "${media.name} · ${byteCount(media.bytes)}",
                        Modifier.align(Alignment.BottomStart).padding(start = 12.dp, end = 12.dp, bottom = 10.dp),
                        style = Theme.smallFont, color = palette.mutedForeground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis,
                    )
                }
                when (val shown = state) {
                    MediaState.Loading -> Column(
                        Modifier.align(Alignment.Center),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        Spinner(ControlSize.Large.symbol, color = palette.mutedForeground)
                        store.mediaProgress[media.id]?.let { fraction ->
                            Text("${(fraction * 100).toInt()}%", style = Theme.uiDigits(12f), color = palette.mutedForeground)
                        }
                    }
                    is MediaState.Failed -> Column(
                        Modifier.align(Alignment.Center).padding(horizontal = 12.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        Text(shown.why, style = Theme.smallFont, color = palette.mutedForeground, align = TextAlign.Center, maxLines = 1)
                        RowButton("Try again", "Load the image again", bordered = true) { attempt += 1 }
                    }
                    MediaState.Shown -> Unit
                }
            }
        }
    }
}

private fun mediaViewed(media: MediaContent) = ViewedMedia(media.name, media.video, ViewedMedia.Source.Media(media.id))

/** As large as the media's box in the column it is in. */
private fun Modifier.boxed(media: MediaContent): Modifier = layout { measurable, constraints ->
    val (width, height) = media.box(constraints.maxWidth.toDp().value)
    val placeable = measurable.measure(Constraints.fixed(width.dp.roundToPx(), height.dp.roundToPx()))
    layout(placeable.width, placeable.height) { placeable.place(0, 0) }
}
