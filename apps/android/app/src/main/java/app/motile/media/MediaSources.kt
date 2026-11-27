package app.motile.media

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionOnScreen
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.toSize
import app.motile.core.ViewedMedia

/**
 * The pictures on screen that the viewer's images and videos open from, for the viewer to zoom out of
 * one and back into the one it shows as it closes.
 */
object MediaSources {
    /** A picture of an image or a video: where it is drawn on the screen, in pixels, and with what corners. */
    class Source(val key: String, val frame: Rect, val picture: ImageBitmap, val radius: Float)

    private class Mark {
        var key: String? = null
        var picture: ImageBitmap? = null
        var fills = false
        var radius = 0f
        var drawn: Size? = null
        var bounds = Rect.Zero
    }

    private val shown = mutableMapOf<Any, Mark>()
    /** The picture last pressed, which is the one a viewer opened from when several show its file. */
    private var pressed: Any? = null
    /** The pictures a viewer's picture lies over, which aren't drawn meanwhile. */
    val hidden = mutableStateMapOf<String, Unit>()

    /** The picture of it that is on the screen `screen`, in pixels. */
    fun source(media: ViewedMedia, screen: Rect): Source? {
        val key = media.sourceKey
        val marks = listOfNotNull(pressed?.let { token -> shown[token]?.takeIf { it.key == key } }) + shown.values.filter { it.key == key }
        val mark = marks.firstOrNull { it.picture != null && it.bounds.overlaps(screen) } ?: return null
        val picture = mark.picture ?: return null
        val pictureSize = Size(picture.width.toFloat(), picture.height.toFloat())
        val box = mark.drawn?.let { fitted(Size(minOf(it.width, mark.bounds.width), minOf(it.height, mark.bounds.height)), mark.bounds) } ?: mark.bounds
        val frame = if (mark.fills) box else fitted(pictureSize, box)
        return Source(key, frame, picture, if (frame == mark.bounds) mark.radius else 0f)
    }

    /** The size, scaled to fit in the box, in its middle. */
    fun fitted(size: Size, box: Rect): Rect {
        if (size.width <= 0f || size.height <= 0f) return box
        val scale = minOf(box.width / size.width, box.height / size.height)
        val shown = Size(size.width * scale, size.height * scale)
        return Rect(box.center.x - shown.width / 2, box.center.y - shown.height / 2, box.center.x + shown.width / 2, box.center.y + shown.height / 2)
    }

    internal fun set(token: Any, key: String?, picture: ImageBitmap?, fills: Boolean, radius: Float, drawn: Size?) {
        val mark = shown.getOrPut(token) { Mark() }
        mark.key = key
        mark.picture = picture
        mark.fills = fills
        mark.radius = radius
        mark.drawn = drawn
    }

    internal fun place(token: Any, bounds: Rect) {
        shown[token]?.bounds = bounds
    }

    internal fun press(token: Any) {
        pressed = token
    }

    internal fun forget(token: Any) {
        shown.remove(token)
        if (pressed === token) pressed = null
    }
}

/** What its pictures are found by: the attachment it is, or its file, in its row when it has one. */
val ViewedMedia.sourceKey: String
    get() {
        attachment?.let { return "attachment:$it" }
        val file = when (val source = source) {
            is ViewedMedia.Source.LocalFile -> "file:${source.file.path}"
            is ViewedMedia.Source.Media -> "media:${source.id}"
        }
        return shownIn?.let { "$file@${it.row}" } ?: file
    }

/**
 * Marks this as the picture of the image or video, for the viewer to zoom out of and back into. It
 * shows `picture` cropped to fill it, or fitted in it, no larger than `drawn`; it isn't drawn while
 * the viewer's picture lies over it.
 */
fun Modifier.mediaSource(media: ViewedMedia?, picture: ImageBitmap?, fills: Boolean, radius: Dp = 0.dp, drawn: DpSize? = null): Modifier = composed {
    val token = remember { Any() }
    val density = LocalDensity.current
    val key = media?.sourceKey
    SideEffect {
        val drawnSize = drawn?.let { with(density) { Size(it.width.toPx(), it.height.toPx()) } }
        MediaSources.set(token, key, picture, fills, with(density) { radius.toPx() }, drawnSize)
    }
    DisposableEffect(token) { onDispose { MediaSources.forget(token) } }
    this
        .onGloballyPositioned { coordinates -> MediaSources.place(token, Rect(coordinates.positionOnScreen(), coordinates.size.toSize())) }
        .pointerInput(token) {
            awaitEachGesture {
                awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                MediaSources.press(token)
            }
        }
        // Read as it draws, so that hiding one picture doesn't compose the others again.
        .graphicsLayer { alpha = if (key != null && MediaSources.hidden.containsKey(key)) 0f else 1f }
}
