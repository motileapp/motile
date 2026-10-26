package app.motile.media

import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import android.net.Uri
import android.view.ViewGroup
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateCentroid
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChanged
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.DialogWindowProvider
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import app.motile.LocalStore
import app.motile.core.Pictures
import app.motile.core.ViewedMedia
import app.motile.core.Viewing
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.IconSize
import app.motile.theme.ControlSize
import app.motile.theme.LocalDark
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.Opacity
import app.motile.theme.OverPicture
import app.motile.theme.Palette
import app.motile.theme.value
import app.motile.transcript.MediaFiles
import app.motile.transcript.mediaFile
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.GlassButton
import app.motile.ui.Spinner
import app.motile.ui.Text
import app.motile.ui.glassButton
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.roundToInt

/**
 * The images and videos of a message or of the composer, one at a time over the whole screen, with
 * its name and a close button on glass at the top and copy and share at the bottom. A swipe goes
 * through them, a pinch or a double tap zooms an image, a tap hides or shows the buttons, and a
 * pull down closes it. It is always dark.
 */
@Composable
fun MediaViewer(viewing: Viewing) {
    val dark = Palette.dark
    CompositionLocalProvider(LocalPalette provides dark, LocalDark provides true, LocalForeground provides dark.foreground) {
        Viewer(viewing)
    }
}

private object Viewer {
    val barGap = 6.dp
    /** What a video keeps clear of above and below: the buttons. */
    val room = GlassButton.side + barGap
}

@Composable
private fun Viewer(viewing: Viewing) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val density = LocalDensity.current
    val scope = rememberCoroutineScope()
    val pager = rememberPagerState(initialPage = viewing.index) { viewing.items.size }
    val pulled = remember { Animatable(0f) }
    var zoomed by remember { mutableStateOf(false) }
    var chrome by remember { mutableStateOf(true) }
    var copied by remember { mutableStateOf(false) }
    val files = remember { mutableStateMapOf<Int, File>() }
    val item = viewing.item
    /** The system's player draws its own controls, so a video keeps the buttons. */
    val buttonsShown = (chrome || item.video) && pulled.value == 0f

    LaunchedEffect(pager.settledPage) {
        if (pager.settledPage != viewing.index) store.viewNext(pager.settledPage - viewing.index)
    }
    LaunchedEffect(viewing.index) {
        if (pager.currentPage != viewing.index) pager.scrollToPage(viewing.index)
    }
    LaunchedEffect(copied) {
        if (!copied) return@LaunchedEffect
        delay(1200)
        copied = false
    }
    StatusBarShown(buttonsShown)

    val closeAt = with(density) { 110.dp.toPx() }
    val fling = with(density) { 1200.dp.toPx() }
    val pull = rememberDraggableState { delta -> scope.launch { pulled.snapTo(max(0f, pulled.value + delta)) } }
    val overlay = max(Opacity.OverlayPulled.value(true), 1 - abs(pulled.value) / with(density) { 500.dp.toPx() })
    Box(
        Modifier
            .fillMaxSize()
            .background(palette.overlay.copy(alpha = overlay))
            .draggable(
                pull, Orientation.Vertical, enabled = !zoomed,
                onDragStopped = { velocity ->
                    if (pulled.value > closeAt || velocity > fling) store.closeViewer() else pulled.animateTo(0f, spring())
                },
            ),
    ) {
        HorizontalPager(
            pager,
            Modifier.fillMaxSize().offset { IntOffset(0, pulled.value.roundToInt()) },
            userScrollEnabled = !zoomed,
            key = { it },
        ) { page ->
            MediaPage(
                viewing.items[page],
                shown = page == pager.currentPage,
                onZoom = { zoomed = it },
                onFile = { files[page] = it },
                tapped = { chrome = !chrome },
            )
        }
        AnimatedVisibility(
            buttonsShown, Modifier.align(Alignment.TopCenter),
            enter = fadeIn(tween(300)) + slideInVertically(spring()) { -it * 2 },
            exit = fadeOut(tween(300)) + slideOutVertically(spring()) { -it * 2 },
        ) {
            Box(Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing).padding(start = 16.dp, end = 16.dp, top = Viewer.barGap)) {
                Column(
                    Modifier.align(Alignment.Center).padding(horizontal = GlassButton.side + 8.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text(item.name, size = 13f, weight = FontWeight.SemiBold, color = OverPicture.foreground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
                    if (viewing.items.size > 1) {
                        Text("${viewing.index + 1} of ${viewing.items.size}", size = 10.5f, color = OverPicture.mutedForeground)
                    }
                }
                GlassButton(Symbol.X, "Close", onClick = { store.closeViewer() }, symbolSize = IconSize.Xl)
            }
        }
        val file = files[pager.currentPage]
        AnimatedVisibility(
            buttonsShown && file != null, Modifier.align(Alignment.BottomEnd),
            enter = fadeIn(tween(300)) + slideInVertically(spring()) { it },
            exit = fadeOut(tween(300)) + slideOutVertically(spring()) { it },
        ) {
            val shown = file ?: return@AnimatedVisibility
            Row(
                Modifier
                    .windowInsetsPadding(WindowInsets.safeDrawing)
                    .padding(end = 16.dp, bottom = Viewer.barGap)
                    .glassButton(CircleShape)
                    .padding(horizontal = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(0.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                GlassButton(if (copied) Symbol.Check else Symbol.Copy, if (item.video) "Copy Video" else "Copy Image", onClick = {
                    MediaFiles.copy(shown, item.name)
                    copied = true
                }, symbolSize = IconSize.Xl, bare = true)
                GlassButton(Symbol.Share, "Share", onClick = { MediaFiles.share(shown, item.name) }, symbolSize = IconSize.Xl, bare = true)
            }
        }
    }
}

/** Hides the status bar with the buttons, as the viewer's window is its own. */
@Composable
private fun StatusBarShown(shown: Boolean) {
    val view = LocalView.current
    DisposableEffect(shown) {
        val window = (view.parent as? DialogWindowProvider)?.window
        val bars = window?.let { WindowCompat.getInsetsController(it, view) }
        if (shown) bars?.show(WindowInsetsCompat.Type.statusBars()) else bars?.hide(WindowInsetsCompat.Type.statusBars())
        onDispose {}
    }
}

private sealed interface Loaded {
    class Picture(val image: ImageBitmap) : Loaded
    class Video(val file: File) : Loaded
    /** A video the player can't play. */
    class Other(val file: File) : Loaded
}

/** One image or video of the viewer, fetched when it is the one shown or next to it. */
@Composable
private fun MediaPage(item: ViewedMedia, shown: Boolean, onZoom: (Boolean) -> Unit, onFile: (File) -> Unit, tapped: () -> Unit) {
    val store = LocalStore.current
    var loaded by remember(item) { mutableStateOf<Loaded?>(null) }
    var failed by remember(item) { mutableStateOf(false) }
    LaunchedEffect(item) {
        val file = when (val source = item.source) {
            is ViewedMedia.Source.LocalFile -> source.file
            is ViewedMedia.Source.Media -> store.mediaFile(source.id)
        }
        if (file == null) {
            failed = true
            return@LaunchedEffect
        }
        onFile(file)
        if (item.video) {
            loaded = Loaded.Video(file)
            return@LaunchedEffect
        }
        val image = withContext(Dispatchers.IO) { Pictures.decode(file, 6144)?.asImageBitmap() }
        loaded = image?.let(Loaded::Picture)
        failed = image == null
    }
    val tapping = Modifier.fillMaxSize().pointerInput(tapped) { detectTapGestures(onTap = { tapped() }) }
    when (val shownNow = loaded) {
        is Loaded.Picture -> ZoomableImage(shownNow.image, onZoom, tapped)
        is Loaded.Video -> VideoPlayer(shownNow.file, playing = shown, Modifier.fillMaxSize().padding(vertical = Viewer.room)) {
            loaded = Loaded.Other(shownNow.file)
        }
        is Loaded.Other -> Column(
            tapping,
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(14.dp, Alignment.CenterVertically),
        ) {
            Text("This video can't be played here.", size = 13f, color = OverPicture.foreground)
            ActionButton("Open in Another App", variant = ButtonVariant.Primary, size = ControlSize.Large) { MediaFiles.open(shownNow.file, item.name) }
        }
        null -> Box(tapping, contentAlignment = Alignment.Center) {
            val id = (item.source as? ViewedMedia.Source.Media)?.id
            val fraction = id?.let { store.mediaProgress[it] }
            when {
                failed -> Text(
                    if (item.video) "This video couldn't be loaded." else "This image couldn't be loaded.",
                    size = 13f, color = OverPicture.mutedForeground, align = TextAlign.Center,
                )
                fraction != null -> Text("Downloading ${item.name} · ${(fraction * 100).toInt()}%", style = app.motile.theme.Theme.uiDigits(13f), color = OverPicture.mutedForeground)
                else -> Spinner(ControlSize.Large.symbol, color = OverPicture.mutedForeground)
            }
        }
    }
}

/** An image that fits the screen until it is zoomed, by pinching or a double tap. Zoomed, it is moved by dragging. */
@Composable
private fun ZoomableImage(image: ImageBitmap, onZoom: (Boolean) -> Unit, tapped: () -> Unit) {
    var scale by remember(image) { mutableFloatStateOf(1f) }
    var offset by remember(image) { mutableStateOf(Offset.Zero) }
    var size by remember { mutableStateOf(IntSize.Zero) }
    fun clamped(value: Offset, at: Float): Offset {
        val x = size.width * (at - 1) / 2
        val y = size.height * (at - 1) / 2
        return Offset(value.x.coerceIn(-x, x), value.y.coerceIn(-y, y))
    }
    fun zoom(to: Float, around: Offset) {
        val center = Offset(size.width / 2f, size.height / 2f)
        val next = to.coerceIn(1f, 8f)
        offset = clamped((around - center) - (around - center - offset) * (next / scale), next)
        scale = next
        if (scale <= 1.001f) offset = Offset.Zero
        onZoom(scale > 1.001f)
    }
    Box(
        Modifier
            .fillMaxSize()
            .onSizeChanged { size = it }
            .pointerInput(image) {
                detectTapGestures(
                    onDoubleTap = { point -> if (scale > 1.001f) zoom(1f, point) else zoom(2.5f, point) },
                    onTap = { tapped() },
                )
            }
            .pointerInput(image) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    do {
                        val event = awaitPointerEvent()
                        val fingers = event.changes.count { it.pressed }
                        if (fingers >= 2 || scale > 1.001f) {
                            zoom(scale * event.calculateZoom(), event.calculateCentroid(useCurrent = true))
                            offset = clamped(offset + event.calculatePan(), scale)
                            event.changes.forEach { if (it.positionChanged()) it.consume() }
                        }
                    } while (event.changes.any { it.pressed })
                }
            },
    ) {
        Image(
            image, null,
            Modifier.fillMaxSize().graphicsLayer {
                scaleX = scale
                scaleY = scale
                translationX = offset.x
                translationY = offset.y
            },
            contentScale = ContentScale.Fit,
        )
    }
}

/** The system's player, Media3's, with its controls. It plays while `playing`; `failed` is called when it can't play the file. */
@OptIn(UnstableApi::class)
@Composable
private fun VideoPlayer(file: File, playing: Boolean, modifier: Modifier, failed: () -> Unit) {
    val context = LocalContext.current
    val player = remember(file) {
        ExoPlayer.Builder(context).build().apply {
            setMediaItem(MediaItem.fromUri(Uri.fromFile(file)))
            prepare()
        }
    }
    DisposableEffect(player) {
        val listener = object : Player.Listener {
            override fun onPlayerError(error: PlaybackException) = failed()
        }
        player.addListener(listener)
        onDispose {
            player.removeListener(listener)
            player.release()
        }
    }
    LaunchedEffect(player, playing) { player.playWhenReady = playing }
    AndroidView(
        factory = { viewContext ->
            PlayerView(viewContext).apply {
                layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
                setShutterBackgroundColor(android.graphics.Color.TRANSPARENT)
                this.player = player
            }
        },
        update = { view -> if (view.player !== player) view.player = player },
        modifier = modifier,
    )
}
