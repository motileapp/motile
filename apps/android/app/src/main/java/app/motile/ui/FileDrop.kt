package app.motile.ui

import android.app.Activity
import android.content.ClipData
import android.content.ClipDescription
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.ui.draganddrop.DragAndDropEvent
import androidx.compose.ui.draganddrop.DragAndDropTarget
import androidx.compose.ui.draganddrop.mimeTypes
import androidx.compose.ui.draganddrop.toAndroidDragEvent
import androidx.compose.foundation.background
import androidx.compose.foundation.draganddrop.dragAndDropTarget
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.activity.compose.LocalActivity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.composer.PickedFile
import app.motile.core.AppStore
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.IconSize
import app.motile.theme.Radius
import app.motile.theme.Theme
import kotlin.concurrent.thread

/** Attaches what another app drops on the thread, as a split screen lets one, and covers the thread while files are held over it. */
@Composable
fun FileDrop(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val store = LocalStore.current
    val activity = LocalActivity.current
    val target: DragAndDropTarget = remember(store, activity) {
        object : DragAndDropTarget {
            override fun onEntered(event: DragAndDropEvent) {
                store.dropTargeted = true
            }

            override fun onExited(event: DragAndDropEvent) {
                store.dropTargeted = false
            }

            override fun onEnded(event: DragAndDropEvent) {
                store.dropTargeted = false
            }

            override fun onDrop(event: DragAndDropEvent): Boolean {
                store.dropTargeted = false
                val dragged = event.toAndroidDragEvent()
                // What another app lends may only be read while it lets the client.
                val permissions = activity?.requestDragAndDropPermissions(dragged)
                attach(dragged.clipData, store) { permissions?.release() }
                return true
            }
        }
    }
    Box(
        modifier.dragAndDropTarget(
            shouldStartDragAndDrop = { event -> event.mimeTypes().any { it != ClipDescription.MIMETYPE_TEXT_PLAIN && it != ClipDescription.MIMETYPE_TEXT_HTML } },
            target = target,
        ),
    ) {
        content()
        val shown = (store.dropTargeted || store.composerDropTargeted) && store.composerServer != null
        Appearing(shown, Modifier.fillMaxSize(), enter = fadeIn(tween(120)), exit = fadeOut(tween(120))) { DropCover() }
    }
}

/** Copies the dropped files off the main thread, then attaches them. */
private fun attach(clip: ClipData?, store: AppStore, done: () -> Unit) {
    val uris = (0 until (clip?.itemCount ?: 0)).mapNotNull { clip?.getItemAt(it)?.uri }
    if (uris.isEmpty()) return done()
    thread(isDaemon = true) {
        val files = uris.mapNotNull(PickedFile::copy)
        done()
        android.os.Handler(android.os.Looper.getMainLooper()).post { store.attach(files) }
    }
}

@Composable
private fun DropCover() {
    val colors = Theme.colors
    val shape = RoundedCornerShape(Radius.xl)
    Column(
        Modifier
            .fillMaxSize()
            .background(colors.background)
            .safeDrawingPadding()
            .padding(8.dp)
            .drawWithContent {
                drawContent()
                drawRoundRect(
                    colors.primary,
                    cornerRadius = CornerRadius(Radius.xl.toPx()),
                    style = Stroke(1.5.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(5.dp.toPx(), 4.dp.toPx()))),
                )
            },
        verticalArrangement = Arrangement.spacedBy((12 * Platform.scale).dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Icon(Symbol.Paperclip, IconSize.Display)
        Text("Drop files here", size = 17f, weight = FontWeight.SemiBold)
    }
}
