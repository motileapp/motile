package app.motile.panel

import android.net.Uri
import android.text.format.Formatter
import androidx.compose.foundation.Image
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.media3.common.MediaItem
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import app.motile.LocalStore
import app.motile.core.Loaded
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.iconSize
import app.motile.theme.IconSize
import app.motile.theme.LocalPalette
import app.motile.theme.Radius
import app.motile.ui.ActionButton
import app.motile.ui.CopyButton
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.MenuPopup
import app.motile.ui.Text
import app.motile.ui.pressPosition
import app.motile.ui.pressable
import app.motile.ui.rememberMenuState
import java.io.File
import kotlin.math.min

/** The folder the thread works in: its files under their folders, to open one in a tab. */
@Composable
fun FilesSurface(target: PanelTarget) {
    val store = LocalStore.current
    val content = store.rightPanel.content
    val palette = LocalPalette.current
    var asked by remember { mutableIntStateOf(0) }
    Column(Modifier.fillMaxSize()) {
        PanelBar {
            Icon(Symbol.Folder, iconSize(text = 12.5f), color = palette.mutedForeground)
            Text(
                target.name, Modifier.padding(start = 3.dp).weight(1f), size = 12.5f, weight = FontWeight.Medium, color = palette.foreground,
                maxLines = 1,
            )
            ActionButton(icon = Symbol.RotateCw, help = "Read the folder again", pending = PanelTab.Files in content.reading) { asked += 1 }
        }
        val error = content.filesError
        val top = content.listings[""]
        when {
            error != null -> PanelMessage(error, failed = true)
            top == null -> PanelLoading()
            top.isEmpty() -> PanelMessage("This folder is empty.")
            else -> LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(top = 6.dp, bottom = panelEndRoom)) {
                items(content.nodes, key = { it.path }) { FileRow(it) }
            }
        }
    }
    LaunchedEffect(target, store.workspaceVersion, asked) { content.loadFiles(target) }
}

@Composable
private fun FileRow(node: FileNode) {
    val store = LocalStore.current
    val content = store.rightPanel.content
    val palette = LocalPalette.current
    val menu = rememberMenuState()
    HighlightButton(
        onClick = { if (node.folder) content.toggleFolder(node.path) else store.rightPanel.open(PanelTab.File(node.path)) },
        modifier = Modifier.fillMaxWidth().pressPosition(menu),
        radius = Radius.sm,
        inset = PaddingValues(horizontal = 6.dp),
        onLongClick = { menu.openAtPress() },
    ) {
        Row(
            Modifier.fillMaxWidth().height(pressable(26f)).padding(start = 12.dp + 14.dp * node.depth, end = 10.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                if (node.open) Symbol.ChevronDown else Symbol.ChevronRight, IconSize.Xs, Modifier.alpha(if (node.folder) 1f else 0f),
                color = palette.mutedStrongerForeground,
            )
            Icon(
                if (node.folder) Symbol.Folder else FileSymbol.symbol(node.path), iconSize(text = 12.5f),
                color = if (node.ignored) palette.mutedStrongestForeground else palette.mutedForeground,
            )
            Text(
                node.name, size = 12.5f, color = if (node.ignored) palette.mutedStrongestForeground else palette.foreground, maxLines = 1,
                overflow = TextOverflow.MiddleEllipsis,
            )
        }
        MenuPopup(menu) { item("Copy Path") { Platform.copy(node.path) } }
    }
}

/** One file of the folder the thread works in. */
@Composable
fun FileSurface(target: PanelTarget, path: String) {
    val store = LocalStore.current
    val content = store.rightPanel.content
    val palette = LocalPalette.current
    val tab = PanelTab.File(path)
    var asked by remember { mutableIntStateOf(0) }
    Column(Modifier.fillMaxSize()) {
        PanelBar {
            val folder = path.substringBeforeLast('/', "")
            val title = buildAnnotatedString {
                if (folder.isNotEmpty()) withStyle(SpanStyle(color = palette.mutedForeground)) { append("$folder/") }
                withStyle(SpanStyle(color = palette.foreground)) { append(path.substringAfterLast('/')) }
            }
            Text(title, Modifier.weight(1f), size = 12.5f, weight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.StartEllipsis)
            CopyButton("Copy the path") { Platform.copy(path) }
            ActionButton(icon = Symbol.RotateCw, help = "Read the file again", pending = tab in content.reading) { asked += 1 }
        }
        when (val loaded = content.contents[tab]) {
            null, Loaded.Loading -> PanelLoading()
            is Loaded.Failed -> PanelMessage(loaded.message, failed = true)
            is Loaded.Ready -> when (val shown = loaded.value) {
                is FileContent.Text -> {
                    if (shown.truncated) PanelNote("This file is too long to show in full. This is its start.")
                    if (shown.document.files.firstOrNull()?.lines?.isEmpty() == true) PanelMessage("This file is empty.") else CodeView(shown.document)
                }
                is FileContent.Image -> PanelImage(shown.picture)
                is FileContent.Video -> FileVideo(shown.file)
                is FileContent.Binary -> {
                    val size = Formatter.formatFileSize(LocalContext.current, shown.size)
                    PanelMessage("This file can't be shown here. It is $size.")
                }
            }
        }
    }
    LaunchedEffect(target, path, store.workspaceVersion, asked) { content.loadFile(path, target) }
}

/** An image of the folder, fit to the panel until it is zoomed: by pinching or a double tap. Zoomed, it is moved by dragging. */
@Composable
private fun PanelImage(picture: ImageBitmap) {
    var zoom by remember(picture) { mutableFloatStateOf(1f) }
    var pan by remember(picture) { mutableStateOf(Offset.Zero) }
    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .clipToBounds()
            .pointerInput(picture) {
                detectTransformGestures { _, moved, zoomed, _ ->
                    zoom = (zoom * zoomed).coerceIn(1f, 8f)
                    pan = if (zoom == 1f) Offset.Zero else pan + moved
                }
            }
            .pointerInput(picture) {
                detectTapGestures(onDoubleTap = {
                    zoom = if (zoom > 1f) 1f else 2.5f
                    pan = Offset.Zero
                })
            }
            .padding(16.dp),
        contentAlignment = Alignment.Center,
    ) {
        val density = LocalDensity.current
        val width = with(density) { picture.width.toDp() }
        val height = with(density) { picture.height.toDp() }
        val fit = min(1f, min(maxWidth / width, maxHeight / height))
        Image(
            picture, contentDescription = null,
            modifier = Modifier.size(width * fit, height * fit).graphicsLayer {
                scaleX = zoom
                scaleY = zoom
                translationX = pan.x
                translationY = pan.y
            },
        )
    }
}

/** A video of the folder, which plays where it is. */
@Composable
private fun FileVideo(file: File) {
    val context = LocalContext.current
    val player = remember(file) {
        ExoPlayer.Builder(context).build().apply {
            setMediaItem(MediaItem.fromUri(Uri.fromFile(file)))
            prepare()
        }
    }
    DisposableEffect(player) { onDispose { player.release() } }
    AndroidView(
        factory = { PlayerView(it).apply { this.player = player } },
        update = { it.player = player },
        modifier = Modifier.fillMaxSize().padding(16.dp),
    )
}

/** What one turn changed in one file. */
@Composable
fun ChangeSurface(target: PanelTarget, turn: String, path: String) {
    val store = LocalStore.current
    val panel = store.rightPanel
    val content = panel.content
    var closed by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize()) {
        PanelBar {
            Text(panel.nameOfTurn(turn), Modifier.weight(1f), size = 12.5f, weight = FontWeight.Medium, color = LocalPalette.current.foreground, maxLines = 1)
            ActionButton(icon = Symbol.Diff, help = "Show everything this turn changed") { panel.showDiff(DiffScope.Turn(turn), revealing = path) }
        }
        val loaded = content.contents[PanelTab.Change(turn, path)]
        val text = (loaded?.value as? FileContent.Text)?.takeIf { it.document.files.isNotEmpty() }
        when {
            loaded == null || loaded == Loaded.Loading -> PanelLoading()
            loaded is Loaded.Failed -> PanelMessage(loaded.message, failed = true)
            text == null -> PanelMessage("The turn's changes to this file can't be shown.")
            else -> {
                if (text.truncated) PanelNote("The turn's changes are too long to show in full, so this file's may be cut short.")
                CodeView(
                    text.document,
                    collapsed = if (closed) setOf(path) else emptySet(),
                    onToggle = { closed = !closed },
                    onOpenFile = { panel.open(PanelTab.File(it)) },
                    onMedia = { content.loadMedia(it, target) },
                    onOpenMedia = content::viewMedia,
                )
            }
        }
    }
    LaunchedEffect(target) { content.loadChange(turn, path, target) }
}
