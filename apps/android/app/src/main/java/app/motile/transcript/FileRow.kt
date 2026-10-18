package app.motile.transcript

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.AppStore
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.ui.Icon
import app.motile.ui.MenuBuilder
import app.motile.ui.MenuPopup
import app.motile.ui.Text
import app.motile.ui.pressPosition
import app.motile.ui.rememberMenuState
import java.io.File

/**
 * A file a reply sends, as a card with its name, its kind and size. Its icon says where the
 * download is, as a messaging app's does: an arrow to download it, the progress with a cross that
 * stops it, then the kind of file, which the card opens.
 */
@Composable
fun FileRow(file: SentFile) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val state = FileDownloads.state(file.id, store)
    val fetching = state is FileDownloads.State.Fetching
    // Only a removed server had a file it couldn't hand over, so asking again is no use.
    val gone = state is FileDownloads.State.Failed && !store.fetchesAgain
    val menu = rememberMenuState()
    val card = remember { MutableInteractionSource() }
    val cardPressed by card.collectIsPressedAsState()
    val tile = remember { MutableInteractionSource() }
    val tilePressed by tile.collectIsPressedAsState()
    val shape = RoundedCornerShape(Radius.lg)
    val inset = (FileRow.cardHeight - FileRow.tileSide) / 2
    Box(Modifier.fillMaxWidth().padding(vertical = 6.dp)) {
        Row(
            Modifier
                .widthIn(max = FileRow.widest)
                .fillMaxWidth()
                .height(FileRow.cardHeight)
                .clip(shape)
                .background(if (cardPressed && !fetching && !gone) palette.backgroundSecondaryAccent else palette.backgroundSecondary, shape)
                .border(1.dp, palette.border, shape)
                .pressPosition(menu)
                .semantics { contentDescription = "${file.name}, ${file.detail}" }
                .combinedClickable(
                    interactionSource = card, indication = null, role = Role.Button,
                    onLongClick = { if (!gone) menu.openAtPress() },
                    onClick = { if (!gone) press(file, state, store) },
                ),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Spacer(Modifier.width(inset))
            val tileShape = RoundedCornerShape(Radius.md)
            val tileLit = tilePressed && fetching
            Box(
                Modifier
                    .size(FileRow.tileSide)
                    .clip(tileShape)
                    .background(if (tileLit) palette.backgroundSecondaryAccentStronger else palette.backgroundSecondaryAccent, tileShape)
                    .then(
                        if (fetching) {
                            Modifier.clickable(tile, null, role = Role.Button, onClickLabel = "Cancel Download") { FileDownloads.cancel(file.id, store) }
                        } else {
                            Modifier
                        },
                    ),
                contentAlignment = Alignment.Center,
            ) {
                val iconColor = if (tileLit) palette.foreground else palette.mutedForeground
                when {
                    state is FileDownloads.State.Fetching -> {
                        ProgressRing(state.fraction, Modifier.fillMaxSize().padding(FileRow.ringInset))
                        Icon(Symbol.X, ControlSize.Small.symbol, color = iconColor)
                    }
                    state is FileDownloads.State.Downloaded || gone -> Icon(file.symbol, ControlSize.Large.symbol, color = iconColor)
                    else -> Icon(Symbol.Download, ControlSize.Large.symbol, color = iconColor)
                }
            }
            Column(Modifier.weight(1f).padding(start = inset - 4.dp, end = inset)) {
                Text(file.name, style = Theme.ui(13f, FontWeight.Medium), color = palette.foreground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
                Spacer(Modifier.height(2.dp))
                val (detail, detailColor) = when (state) {
                    is FileDownloads.State.Fetching -> "${byteCount((state.fraction * file.bytes).toLong())} / ${file.size}" to palette.mutedForeground
                    is FileDownloads.State.Failed -> firstSentence(state.reason) to palette.destructive
                    else -> file.detail to palette.mutedForeground
                }
                Text(detail, style = Theme.uiDigits(12f), color = detailColor, maxLines = 1)
            }
            if (state is FileDownloads.State.Downloaded) {
                val margin = (FileRow.cardHeight - RowButton.size.height) / 2
                CompositionLocalProvider(LocalSurface provides Surface.Secondary) {
                    RowButton(MediaFiles.revealTitle, MediaFiles.revealTitle, bordered = true, margin = PaddingValues(end = margin)) {
                        MediaFiles.share(state.file, file.name)
                    }
                }
            }
        }
        MenuPopup(menu) { fileActions(file, state, store) }
    }
}

object FileRow {
    val cardHeight = scaled(56f).dp
    val widest = scaled(420f).dp
    val tileSide = scaled(36f).dp
    val ringInset = scaled(5f).dp
}

private fun press(file: SentFile, state: FileDownloads.State, store: AppStore) {
    when (state) {
        is FileDownloads.State.Remote, is FileDownloads.State.Failed -> FileDownloads.download(file, store)
        is FileDownloads.State.Fetching -> Unit
        is FileDownloads.State.Downloaded -> MediaFiles.open(state.file, file.name)
    }
}

/** The words that say what went wrong, without the system's reasons after them. */
private fun firstSentence(reason: String): String {
    val end = reason.indexOf(". ")
    return if (end < 0) reason else reason.substring(0, end) + "."
}

private fun MenuBuilder.fileActions(file: SentFile, state: FileDownloads.State, store: AppStore) {
    /** Hands over the downloaded file, or the client's copy, which the core fetches if need be. */
    fun withFile(done: (File) -> Unit) {
        val downloaded = FileDownloads.state(file.id, store) as? FileDownloads.State.Downloaded
        if (downloaded != null) return done(downloaded.file)
        FileDownloads.fetch(file, store, done)
    }
    when (state) {
        is FileDownloads.State.Fetching -> item("Cancel Download", Symbol.X) { FileDownloads.cancel(file.id, store) }
        is FileDownloads.State.Downloaded -> {
            item("Open", Symbol.SquareArrowOutUpRight) { MediaFiles.open(state.file, file.name) }
            item(MediaFiles.revealTitle, Symbol.Share) { MediaFiles.share(state.file, file.name) }
            item("Copy File", Symbol.Copy) { withFile { MediaFiles.copy(it, file.name) } }
        }
        else -> {
            item("Download", Symbol.Download) { FileDownloads.download(file, store) }
            item(MediaFiles.saveTitle, Symbol.Share) { withFile { MediaFiles.share(it, file.name) } }
            item("Copy File", Symbol.Copy) { withFile { MediaFiles.copy(it, file.name) } }
        }
    }
}
