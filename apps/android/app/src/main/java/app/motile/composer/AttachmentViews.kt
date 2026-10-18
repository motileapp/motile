package app.motile.composer

import android.text.format.Formatter
import android.util.LruCache
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Attachment
import app.motile.core.ContinuedThread
import app.motile.core.Pictures
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalForeground
import app.motile.theme.LocalRow
import app.motile.theme.LocalSurface
import app.motile.theme.OverPicture
import app.motile.theme.Radius
import app.motile.theme.ShadowSize
import app.motile.theme.Opacity
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.theme.scrim
import app.motile.theme.shadow
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.DimButton
import app.motile.ui.Icon
import app.motile.ui.Text
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import java.io.File
import kotlin.coroutines.resume
import kotlin.math.roundToInt

/** An image or a video in the composer: its picture, how far it is on its way to the server, and the button that removes it. A tap opens it in the viewer. */
object AttachmentTile {
    val side = 64.dp
    internal val pictures = LruCache<String, ImageBitmap>(32)
}

@Composable
fun AttachmentTile(attachment: Attachment, open: () -> Unit) {
    val store = LocalStore.current
    val palette = Theme.colors
    val shape = RoundedCornerShape(Radius.lg)
    val maxPixels = with(LocalDensity.current) { (AttachmentTile.side.toPx() * 2).roundToInt() }
    val picture by produceState(AttachmentTile.pictures.get("tile:${attachment.id}"), attachment.id) {
        if (value != null) return@produceState
        val file = attachment.file ?: attachment.attached.picture?.let { id ->
            suspendCancellableCoroutine<File?> { going -> store.media(id) { going.resume(it) } }
        } ?: return@produceState
        val decoded = withContext(Dispatchers.IO) {
            val frame = if (attachment.video && attachment.file != null) Pictures.firstFrame(file, maxPixels) else Pictures.decode(file, maxPixels)
            frame?.asImageBitmap()
        } ?: return@produceState
        AttachmentTile.pictures.put("tile:${attachment.id}", decoded)
        value = decoded
    }
    Box(Modifier.semantics { contentDescription = attachment.name }) {
        DimButton(open, Modifier.size(AttachmentTile.side).clip(shape).border(1.dp, palette.border, shape)) {
            Box(Modifier.fillMaxSize().background(palette.backgroundSecondary), contentAlignment = Alignment.Center) {
                picture?.let { Image(it, null, Modifier.fillMaxSize(), contentScale = ContentScale.Fit) }
                if (attachment.video) {
                    Icon(Symbol.Play, 16f, Modifier.shadow(ShadowSize.Sm, CircleShape, Opacity.ShadowStrongest), color = OverPicture.foreground)
                }
                Box(Modifier.align(Alignment.BottomCenter).fillMaxWidth()) { AttachmentProgress(attachment, onPicture = true) }
            }
        }
        ActionButton(
            icon = Symbol.X, modifier = Modifier.align(Alignment.TopEnd).padding(2.dp), help = "Remove", variant = ButtonVariant.Overlay,
            size = ControlSize.Small, round = true,
        ) { store.removeAttachment(attachment.id) }
    }
}

/** A file in the composer that isn't shown as a picture: its name, its size, and how far it is on its way to the server. */
@Composable
fun AttachmentChip(attachment: Attachment) {
    val store = LocalStore.current
    val palette = Theme.colors
    ComposerChip {
        Icon(Symbol.File, 12f)
        Text(attachment.name, size = 12f, maxLines = 1)
        attachment.bytes?.let { Text(Formatter.formatShortFileSize(Platform.context, it), size = 12f, color = palette.mutedForeground, maxLines = 1) }
        AttachmentProgress(attachment, onPicture = false)
        CompositionLocalProvider(LocalRow provides Surface.Layer.Chip) {
            ActionButton(icon = Symbol.X, help = "Remove", size = ControlSize.Small, round = true) { store.removeAttachment(attachment.id) }
        }
    }
}

/** The thread of a removed server that a new thread goes on from, and the button that starts it without it. */
@Composable
fun ContinuedThreadChip(from: ContinuedThread, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    ComposerChip(modifier) {
        Icon(Symbol.MessageSquareText, 12f)
        Text("Continues “${from.title}”", Modifier.weight(1f, fill = false), size = 12f, maxLines = 1)
        CompositionLocalProvider(LocalRow provides Surface.Layer.Chip) {
            ActionButton(icon = Symbol.X, help = "Start without it", size = ControlSize.Small, round = true) { store.dropContinuation() }
        }
    }
}

/** The capsule of what a message goes with in the composer: a file, or the thread it continues. */
@Composable
private fun ComposerChip(modifier: Modifier = Modifier, content: @Composable RowScope.() -> Unit) {
    Row(
        modifier
            .background(LocalSurface.current.color(Surface.Layer.Chip), CircleShape)
            .border(1.dp, Theme.colors.borderAccent, CircleShape)
            .padding(start = 9.dp, top = 2.dp, bottom = 2.dp, end = 2.dp),
        horizontalArrangement = Arrangement.spacedBy(5.dp),
        verticalAlignment = Alignment.CenterVertically,
        content = content,
    )
}

/** How far an attachment is on its way to the server, or the button that tries again when it didn't get there. Nothing once it is there. */
@Composable
private fun AttachmentProgress(attachment: Attachment, onPicture: Boolean) {
    val store = LocalStore.current
    val palette = Theme.colors
    when (val state = attachment.state) {
        Attachment.State.Ready -> Unit
        is Attachment.State.Uploading -> {
            val percent = "${(state.fraction * 100).toInt()}%"
            if (onPicture) {
                Text(
                    percent, Modifier.fillMaxWidth().background(palette.scrim).padding(vertical = 2.dp),
                    style = Theme.uiDigits(10f, FontWeight.Medium), color = OverPicture.foreground, align = androidx.compose.ui.text.style.TextAlign.Center,
                )
            } else {
                Text(percent, style = Theme.uiDigits(12f), color = palette.mutedForeground)
            }
        }
        is Attachment.State.Failed -> {
            val help = "${state.message} Tap to try again."
            if (onPicture) {
                DimButton({ store.retryAttachment(attachment.id) }, Modifier.fillMaxWidth().background(palette.destructive).semantics { contentDescription = help }) {
                    CompositionLocalProvider(LocalForeground provides palette.destructiveForeground) {
                        Row(
                            Modifier.align(Alignment.Center).padding(vertical = 2.dp),
                            horizontalArrangement = Arrangement.spacedBy(3.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Icon(Symbol.RotateCw, 10f)
                            Text("Retry", size = 10f, weight = FontWeight.Medium)
                        }
                    }
                }
            } else {
                ActionButton("Retry", Symbol.RotateCw, help = help, variant = ButtonVariant.Danger, size = ControlSize.Small, round = true) {
                    store.retryAttachment(attachment.id)
                }
            }
        }
    }
}
