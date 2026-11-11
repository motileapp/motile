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
import androidx.compose.foundation.layout.widthIn
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
import app.motile.theme.TextSize
import app.motile.theme.iconSize
import app.motile.theme.IconSize
import app.motile.theme.ControlSize
import app.motile.theme.LocalForeground
import app.motile.theme.LocalRow
import app.motile.theme.LocalSurface
import app.motile.theme.OnSurface
import app.motile.theme.OverPicture
import app.motile.theme.Radius
import app.motile.theme.ShadowSize
import app.motile.theme.Opacity
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.theme.scrimStronger
import app.motile.theme.shadow
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.Chip
import app.motile.ui.DimButton
import app.motile.ui.Icon
import app.motile.ui.Spinner
import app.motile.ui.Text
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import java.io.File
import kotlin.coroutines.resume
import kotlin.math.roundToInt

/** An image or a video in the composer: its picture, how far it is on its way to the server, and the button that removes it. A tap opens it in the viewer. */
object AttachmentTile {
    val side = 96.dp
    internal val pictures = LruCache<String, ImageBitmap>(32)
}

@Composable
fun AttachmentTile(attachment: Attachment, open: () -> Unit) {
    val store = LocalStore.current
    val palette = Theme.colors
    val maxPixels = with(LocalDensity.current) { (AttachmentTile.side.toPx() * 2).roundToInt() }
    val picture by produceState(AttachmentTile.pictures.get("tile:${attachment.id}"), attachment.id, attachment.file) {
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
    TileLook(attachment, open) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            picture?.let { Image(it, null, Modifier.fillMaxSize(), contentScale = ContentScale.Fit) }
            if (attachment.state == Attachment.State.Preparing) {
                Spinner(IconSize.Lg, color = palette.mutedForeground)
            } else if (attachment.video) {
                Icon(Symbol.Play, IconSize.Lg, Modifier.shadow(ShadowSize.Sm, CircleShape, Opacity.ShadowStrongest), color = OverPicture.foreground)
            }
        }
    }
}

private val kindInset = 6.dp

/** The remove button sits level with a file's kind. */
private val removeInset get() = kindInset - (ControlSize.Small.height - Chip.compactHeight) / 2

/** A file's kind is cut short before the remove button. */
private val kindWidth get() = AttachmentTile.side - kindInset - 4.dp - ControlSize.Small.height - removeInset

/** A file in the composer that isn't shown as a picture, as a tile beside the pictures: its kind, its name, and how far it is on its way to the server. */
@Composable
fun AttachmentFileTile(attachment: Attachment) {
    TileLook(attachment, open = null) {
        Box(Modifier.fillMaxSize()) {
            Text(
                attachment.name, Modifier.align(Alignment.BottomStart).padding(horizontal = 10.dp, vertical = 8.dp),
                size = TextSize.Xxs, weight = FontWeight.Medium, color = Theme.colors.foreground, maxLines = 3,
            )
            attachment.kind?.let { Chip(it, Modifier.align(Alignment.TopStart).padding(kindInset).widthIn(max = kindWidth), small = true) }
        }
    }
}

/** The square of an attachment, a box on the composer: its edge, how far its upload is along its bottom, and the button that removes it. */
@Composable
private fun TileLook(attachment: Attachment, open: (() -> Unit)?, content: @Composable () -> Unit) = OnSurface(Surface.ComposerBox) {
    val store = LocalStore.current
    val shape = RoundedCornerShape(Radius.xl)
    val summary = attachment.bytes?.let { "${attachment.name}, ${Formatter.formatShortFileSize(Platform.context, it)}" } ?: attachment.name
    Box(Modifier.semantics { contentDescription = summary }) {
        val framed = Modifier.size(AttachmentTile.side).clip(shape).background(Surface.ComposerBox.color).border(1.dp, Surface.ComposerBox.border, shape)
        val tile: @Composable () -> Unit = {
            content()
            Box(Modifier.align(Alignment.BottomCenter).fillMaxWidth()) { AttachmentProgress(attachment) }
        }
        if (open != null) DimButton(open, framed) { tile() } else Box(framed) { tile() }
        ActionButton(
            icon = Symbol.X, modifier = Modifier.align(Alignment.TopEnd).padding(removeInset), help = "Remove", variant = ButtonVariant.Secondary,
            size = ControlSize.Small, round = true,
        ) { store.removeAttachment(attachment.id) }
    }
}

/** The thread of a removed server that a new thread goes on from, and the button that starts it without it. */
@Composable
fun ContinuedThreadChip(from: ContinuedThread, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    ComposerChip(modifier) {
        Icon(Symbol.MessageSquareText, iconSize(text = TextSize.Xs))
        Text("Continues “${from.title}”", Modifier.weight(1f, fill = false), size = TextSize.Xs, maxLines = 1)
        CompositionLocalProvider(LocalRow provides Surface.Layer.Chip) {
            ActionButton(icon = Symbol.X, help = "Start without it", size = ControlSize.Small, round = true) { store.dropContinuation() }
        }
    }
}

/** The capsule of the thread a message continues in the composer. */
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

/** How far an attachment is on its way to the server, along its tile's bottom edge, or the button that tries again when it didn't get there. Nothing once it is there. */
@Composable
private fun AttachmentProgress(attachment: Attachment) {
    val store = LocalStore.current
    val palette = Theme.colors
    when (val state = attachment.state) {
        Attachment.State.Ready, Attachment.State.Preparing -> Unit
        is Attachment.State.Uploading -> Text(
            "${(state.fraction * 100).toInt()}%", Modifier.fillMaxWidth().background(palette.scrimStronger).padding(vertical = 2.dp),
            style = Theme.uiDigits(TextSize.Xxs, FontWeight.Medium), color = OverPicture.foreground, align = androidx.compose.ui.text.style.TextAlign.Center,
        )
        is Attachment.State.Failed -> {
            val help = "${state.message} Tap to try again."
            DimButton({ store.retryAttachment(attachment.id) }, Modifier.fillMaxWidth().background(palette.destructive).semantics { contentDescription = help }) {
                CompositionLocalProvider(LocalForeground provides palette.destructiveForeground) {
                    Row(
                        Modifier.align(Alignment.Center).padding(vertical = 2.dp),
                        horizontalArrangement = Arrangement.spacedBy(3.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(Symbol.RotateCw, iconSize(text = TextSize.Xxs))
                        Text("Retry", size = TextSize.Xxs, weight = FontWeight.Medium)
                    }
                }
            }
        }
    }
}
