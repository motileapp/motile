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
import androidx.compose.foundation.Canvas
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.input.pointer.PointerEvent
import app.motile.theme.Surface
import app.motile.transcript.canvas.CanvasScene
import app.motile.transcript.canvas.CanvasText
import app.motile.transcript.canvas.SceneColors
import app.motile.transcript.canvas.paintScene
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
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChanged
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
import androidx.activity.compose.BackHandler
import app.motile.DemoHooks
import app.motile.LocalStore
import app.motile.core.MarkupSession
import app.motile.core.MarkupTool
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
import app.motile.theme.TextSize
import app.motile.theme.value
import app.motile.transcript.MediaFiles
import app.motile.transcript.mediaFile
import app.motile.ui.ActionButton
import app.motile.ui.Answer
import app.motile.ui.Asking
import app.motile.ui.Confirmation
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
import androidx.compose.animation.core.animate
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.displayCutout
import androidx.compose.foundation.layout.imeAnimationTarget
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.union
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.geometry.lerp
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.util.lerp
import app.motile.theme.floatingHeight
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlin.math.min

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
    /** The most the markup bar takes, which a pin's note fills. */
    val markupWidth = 560.dp
    /** How far from a pin a finger may land and still take it. */
    val markupSlop = 12.dp
    /** What the buttons at the top and the bar at the bottom take while an image is marked up, and a gap. */
    val markupControls = PaddingValues(top = barGap + GlassButton.side + 8.dp, bottom = barGap + floatingHeight + 8.dp)
    /** How much of the picture around the pin whose note is written stays in sight. */
    val pinMargin = 16.dp
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
    val pictures = remember { mutableStateMapOf<Int, ImageBitmap>() }
    var markup by remember { mutableStateOf<MarkupSession?>(null) }
    /** The image before the marks of a composer attachment marked up again, shown in its place. */
    var original by remember { mutableStateOf<ImageBitmap?>(null) }
    var discarding by remember { mutableStateOf(false) }
    var markupFailed by remember { mutableStateOf(false) }
    val item = viewing.item
    val page = pager.currentPage
    val markable = !item.video && files[page] != null && pictures[page] != null
    /** The system's player draws its own controls, so a video keeps the buttons. */
    val buttonsShown = (chrome || item.video || markup != null) && pulled.value == 0f

    fun endMarkup() {
        markup?.close()
        markup = null
        original = null
    }

    fun startMarkup() {
        val file = files[page] ?: return
        val picture = pictures[page] ?: return
        markupFailed = false
        val saved = item.attachment?.let { id -> store.attachments.firstOrNull { it.id == id }?.markup }
        if (saved == null) {
            markup = MarkupSession.open(file, item.name, item.attachment, picture.width, picture.height, null)
            return
        }
        scope.launch {
            val before = withContext(Dispatchers.IO) { Pictures.decode(saved.original, 6144)?.asImageBitmap() } ?: return@launch
            if (viewing.item != item) return@launch
            original = before
            markup = MarkupSession.open(saved.original, saved.name, item.attachment, before.width, before.height, saved)
        }
    }

    fun cancelMarkup() {
        val session = markup ?: return
        if (session.changed) discarding = true else endMarkup()
    }

    fun finishMarkup() {
        val session = markup ?: return
        if (session.finishing || (!session.changed && session.attachment != null)) return
        markupFailed = false
        scope.launch {
            val result = session.finish(MarkupSession.folder)
            if (result == null) {
                markupFailed = true
                return@launch
            }
            store.attachMarkup(result, session.attachment)
            endMarkup()
            store.closeViewer()
        }
    }

    val open by rememberUpdatedState(markup)
    DisposableEffect(Unit) {
        onDispose { open?.close() }
    }
    SideEffect {
        DemoHooks.markup = { step -> drive(step, markup, ::startMarkup, ::cancelMarkup, ::finishMarkup) }
    }
    DisposableEffect(Unit) {
        onDispose { DemoHooks.markup = null }
    }
    BackHandler(markup != null) {
        val session = markup ?: return@BackHandler
        if (session.selected != null) session.deselect() else cancelMarkup()
    }
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
                pull, Orientation.Vertical, enabled = !zoomed && markup == null,
                onDragStopped = { velocity ->
                    if (pulled.value > closeAt || velocity > fling) store.closeViewer() else pulled.animateTo(0f, spring())
                },
            ),
    ) {
        HorizontalPager(
            pager,
            Modifier.fillMaxSize().offset { IntOffset(0, pulled.value.roundToInt()) },
            userScrollEnabled = !zoomed && markup == null,
            key = { it },
        ) { shown ->
            val current = shown == pager.currentPage
            MediaPage(
                viewing.items[shown],
                shown = current,
                markup = markup.takeIf { current },
                original = original.takeIf { current },
                controls = Viewer.markupControls.takeIf { current && markup != null },
                onZoom = { zoomed = it },
                onFile = { files[shown] = it },
                onPicture = { pictures[shown] = it },
                tapped = { chrome = !chrome },
            )
        }
        AnimatedVisibility(
            buttonsShown, Modifier.align(Alignment.TopCenter),
            enter = fadeIn(tween(300)) + slideInVertically(spring()) { -it * 2 },
            exit = fadeOut(tween(300)) + slideOutVertically(spring()) { -it * 2 },
        ) {
            Box(Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing).padding(start = 16.dp, end = 16.dp, top = Viewer.barGap)) {
                val session = markup
                if (session == null) {
                    Column(
                        Modifier.align(Alignment.Center).padding(horizontal = GlassButton.side + 8.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        Text(item.name, size = TextSize.Sm, weight = FontWeight.SemiBold, color = OverPicture.foreground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
                        if (viewing.items.size > 1) {
                            Text("${viewing.index + 1} of ${viewing.items.size}", size = TextSize.Xs, color = OverPicture.mutedForeground)
                        }
                    }
                    GlassButton(Symbol.X, "Close", onClick = { store.closeViewer() }, symbolSize = IconSize.Xl)
                } else {
                    GlassButton(Symbol.X, "Cancel", onClick = { cancelMarkup() }, symbolSize = IconSize.Xl)
                    ActionButton(
                        if (session.attachment == null) "Add to Message" else "Done",
                        modifier = Modifier.align(Alignment.CenterEnd),
                        variant = ButtonVariant.Primary,
                        size = ControlSize.Large,
                        pending = session.finishing,
                        round = true,
                        disabled = !session.changed && session.attachment != null,
                    ) { finishMarkup() }
                }
            }
        }
        val file = files[page]
        AnimatedVisibility(
            buttonsShown && file != null && markup == null, Modifier.align(Alignment.BottomEnd),
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
                if (markable) GlassButton(Symbol.PencilLine, "Mark Up", onClick = { startMarkup() }, symbolSize = IconSize.Xl, bare = true)
                GlassButton(if (copied) Symbol.Check else Symbol.Copy, if (item.video) "Copy Video" else "Copy Image", onClick = {
                    MediaFiles.copy(shown, item.name)
                    copied = true
                }, symbolSize = IconSize.Xl, bare = true)
                GlassButton(Symbol.Share, "Share", onClick = { MediaFiles.share(shown, item.name) }, symbolSize = IconSize.Xl, bare = true)
            }
        }
        AnimatedVisibility(
            markup != null, Modifier.align(Alignment.BottomCenter),
            enter = fadeIn(tween(250)) + slideInVertically(spring()) { it },
            exit = fadeOut(tween(250)) + slideOutVertically(spring()) { it },
        ) {
            val session = markup ?: return@AnimatedVisibility
            Column(
                Modifier
                    .windowInsetsPadding(WindowInsets.safeDrawing)
                    .padding(start = 16.dp, end = 16.dp, bottom = Viewer.barGap)
                    .widthIn(max = Viewer.markupWidth),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                if (markupFailed) {
                    Text(
                        "This image couldn't be marked up.",
                        Modifier.background(palette.popover, CircleShape).padding(horizontal = 10.dp, vertical = 4.dp),
                        size = TextSize.Sm, color = OverPicture.foreground,
                    )
                }
                MarkupBar(session)
            }
        }
        Asking(if (discarding) Unit else null, { discarding = false }) {
            Confirmation(
                "Discard your marks?", "The image stays as it was.",
                listOf(Answer("Discard", ButtonVariant.Danger) { endMarkup() }),
            )
        }
    }
}

/** A step of the demo driver, as DemoDriverIOS's: `start`, `tool <tool>`, `ink <ink>`, `draw x y x y…` in shares of the image, `note <text>`, `keep`, `done` or `cancel`. */
private fun drive(step: String, session: MarkupSession?, start: () -> Unit, cancel: () -> Unit, finish: () -> Unit) {
    val words = step.split(' ')
    val rest = words.drop(1).joinToString(" ")
    when (words.first()) {
        "start" -> start()
        "tool" -> session?.pick(MarkupTool.entries.firstOrNull { it.key == rest } ?: MarkupTool.Pen)
        "ink" -> session?.pick(rest)
        "draw" -> {
            val markup = session ?: return
            val values = words.drop(1).mapNotNull { it.toFloatOrNull() }
            val points = values.chunked(2).filter { it.size == 2 }.map { (x, y) -> x * markup.width to y * markup.height }
            val first = points.firstOrNull() ?: return
            val last = points.last()
            markup.down(first.first, first.second, 8f)
            points.drop(1).forEach { (x, y) -> markup.drag(x, y) }
            markup.up(last.first, last.second)
        }
        "note" -> session?.note = rest
        "keep" -> session?.deselect()
        "done" -> finish()
        "cancel" -> cancel()
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

/** One image or video of the viewer, fetched when it is the one shown or next to it. An image marked up shows `original` in its place when it has one. */
@Composable
private fun MediaPage(
    item: ViewedMedia,
    shown: Boolean,
    markup: MarkupSession?,
    original: ImageBitmap?,
    /** The room the viewer's buttons take over it, which the picture keeps clear of. */
    controls: PaddingValues?,
    onZoom: (Boolean) -> Unit,
    onFile: (File) -> Unit,
    onPicture: (ImageBitmap) -> Unit,
    tapped: () -> Unit,
) {
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
        image?.let(onPicture)
    }
    val tapping = Modifier.fillMaxSize().pointerInput(tapped) { detectTapGestures(onTap = { tapped() }) }
    when (val shownNow = loaded) {
        is Loaded.Picture -> ZoomableImage(original ?: shownNow.image, markup, controls, onZoom, tapped)
        is Loaded.Video -> VideoPlayer(shownNow.file, playing = shown, Modifier.fillMaxSize().padding(vertical = Viewer.room)) {
            loaded = Loaded.Other(shownNow.file)
        }
        is Loaded.Other -> Column(
            tapping,
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(14.dp, Alignment.CenterVertically),
        ) {
            Text("This video can't be played here.", size = TextSize.Sm, color = OverPicture.foreground)
            ActionButton("Open in Another App", variant = ButtonVariant.Primary, size = ControlSize.Large) { MediaFiles.open(shownNow.file, item.name) }
        }
        null -> Box(tapping, contentAlignment = Alignment.Center) {
            val id = (item.source as? ViewedMedia.Source.Media)?.id
            val fraction = id?.let { store.mediaProgress[it] }
            when {
                failed -> Text(
                    if (item.video) "This video couldn't be loaded." else "This image couldn't be loaded.",
                    size = TextSize.Sm, color = OverPicture.mutedForeground, align = TextAlign.Center,
                )
                fraction != null -> Text("Downloading ${item.name} · ${(fraction * 100).toInt()}%", style = app.motile.theme.Theme.uiDigits(TextSize.Sm), color = OverPicture.mutedForeground)
                else -> Spinner(ControlSize.Large.symbol, color = OverPicture.mutedForeground)
            }
        }
    }
}

/** What covers the edges of the screen, in pixels. */
private data class Edges(val left: Float = 0f, val top: Float = 0f, val right: Float = 0f, val bottom: Float = 0f)

/** What the viewer's `controls` and the system's bars or the keyboard cover; nothing without controls. */
@kotlin.OptIn(ExperimentalLayoutApi::class)
@Composable
private fun covered(controls: PaddingValues?): Edges {
    controls ?: return Edges()
    val density = LocalDensity.current
    val direction = LocalLayoutDirection.current
    val bars = WindowInsets.systemBars.union(WindowInsets.displayCutout)
    val keyboard = WindowInsets.imeAnimationTarget.getBottom(density)
    return with(density) {
        Edges(
            bars.getLeft(density, direction) + controls.calculateLeftPadding(direction).toPx(),
            bars.getTop(density) + controls.calculateTopPadding().toPx(),
            bars.getRight(density, direction) + controls.calculateRightPadding(direction).toPx(),
            max(bars.getBottom(density), keyboard) + controls.calculateBottomPadding().toPx(),
        )
    }
}

/**
 * Where a picture sits on the screen: `zoom` screen pixels to one of its own, and `origin` its top
 * left corner. It fits in what `clear` leaves, in the middle while it is smaller, and a larger one
 * moves out from under what covers the edges.
 */
private class Framing {
    private var width by mutableIntStateOf(1)
    private var height by mutableIntStateOf(1)
    private var size by mutableStateOf(IntSize.Zero)
    private var clear by mutableStateOf(Edges())
    var zoom by mutableFloatStateOf(0f)
        private set
    var origin by mutableStateOf(Offset.Zero)
        private set
    private var gliding: Job? = null

    /** Enlarges a small picture to twice its size at most. */
    val fit: Float
        get() {
            if (size.width == 0 || size.height == 0) return 1f
            val across = (size.width - clear.left - clear.right) / width
            val down = (size.height - clear.top - clear.bottom) / height
            return minOf(2f, across, down).coerceAtLeast(0.01f)
        }
    val fitted: Boolean get() = abs(zoom - fit) < 0.001f
    val zoomed: Boolean get() = zoom > fit + 0.001f
    /** An image enlarged to fit can still be shown at its real size. */
    private val smallest: Float get() = min(fit, 1f)

    fun show(image: ImageBitmap) {
        if (image.width == width && image.height == height && zoom > 0f) return
        width = image.width
        height = image.height
        settle(reset = true)
    }

    fun resize(new: IntSize) {
        val reset = fitted
        size = new
        settle(reset)
    }

    /** Zooms around a point of the screen, and moves by `pan`. */
    fun zoom(to: Float, around: Offset, pan: Offset = Offset.Zero) {
        stop()
        val next = to.coerceIn(smallest, fit * 8)
        origin = placed(around - (around - origin) * (next / zoom) + pan, next)
        zoom = next
    }

    /** A point of the screen, in the picture's pixels. */
    fun pixel(at: Offset): Offset = (at - origin) / zoom

    fun stop() {
        gliding?.cancel()
    }

    /** Takes what covers the edges now, fits it again if it was fitted, and keeps `rows` of the picture in sight. */
    fun keepClear(scope: CoroutineScope, edges: Edges, rows: ClosedFloatingPointRange<Float>?, margin: Float) {
        if (edges == clear) return
        val wasFitted = fitted
        clear = edges
        if (size == IntSize.Zero || zoom == 0f) return
        val next = if (wasFitted || zoom < smallest) fit else zoom
        glide(scope, next, showing(rows, margin, next))
    }

    /** Moves the picture just enough for its `rows`, `margin` screen pixels around them, to be clear of what covers the edges. */
    fun bringIntoSight(scope: CoroutineScope, rows: ClosedFloatingPointRange<Float>?, margin: Float) {
        if (rows == null || zoom == 0f) return
        glide(scope, zoom, showing(rows, margin, zoom))
    }

    private fun settle(reset: Boolean) {
        if (size == IntSize.Zero) return
        zoom = if (reset || zoom < smallest) fit else zoom
        origin = placed(origin, zoom)
    }

    private fun showing(rows: ClosedFloatingPointRange<Float>?, margin: Float, zoom: Float): Offset {
        val at = placed(origin, zoom)
        rows ?: return at
        val top = at.y + rows.start * zoom - margin
        val bottom = at.y + rows.endInclusive * zoom + margin
        val lowest = size.height - clear.bottom
        val y = when {
            bottom > lowest -> at.y - (bottom - lowest)
            top < clear.top -> at.y + clear.top - top
            else -> at.y
        }
        return placed(Offset(at.x, y), zoom)
    }

    private fun placed(at: Offset, zoom: Float): Offset {
        fun axis(at: Float, start: Float, end: Float, length: Float): Float {
            val room = end - start
            return if (length <= room) start + (room - length) / 2 else at.coerceIn(end - length, start)
        }
        return Offset(
            axis(at.x, clear.left, size.width - clear.right, width * zoom),
            axis(at.y, clear.top, size.height - clear.bottom, height * zoom),
        )
    }

    private fun glide(scope: CoroutineScope, toZoom: Float, toOrigin: Offset) {
        stop()
        val fromZoom = zoom
        val fromOrigin = origin
        if (fromZoom == toZoom && fromOrigin == toOrigin) return
        gliding = scope.launch {
            animate(0f, 1f, animationSpec = tween(300)) { t, _ ->
                zoom = lerp(fromZoom, toZoom, t)
                origin = lerp(fromOrigin, toOrigin, t)
            }
        }
    }
}

/**
 * An image that fits the screen until it is zoomed, by pinching or a double tap. Zoomed, it is moved
 * by dragging. While it is marked up one finger draws the marks, painted over it, and two move and
 * zoom it. It fits between the `controls` over it and keeps the pin whose note is written in sight.
 */
@Composable
private fun ZoomableImage(image: ImageBitmap, markup: MarkupSession?, controls: PaddingValues?, onZoom: (Boolean) -> Unit, tapped: () -> Unit) {
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current
    val framing = remember { Framing() }
    val clear = covered(controls)
    SideEffect { framing.show(image) }
    /** The selected pin's rows of the image, with its radius. */
    fun pinRows(): ClosedFloatingPointRange<Float>? {
        val session = markup ?: return null
        val pin = session.selectedPin ?: return null
        val share = image.height / session.height
        return (pin.y - pin.radius) * share..(pin.y + pin.radius) * share
    }
    val pinMargin = with(density) { Viewer.pinMargin.toPx() }
    LaunchedEffect(clear) { framing.keepClear(scope, clear, pinRows(), pinMargin) }
    LaunchedEffect(markup?.selected) { framing.bringIntoSight(scope, pinRows(), pinMargin) }
    LaunchedEffect(framing) { snapshotFlow { framing.zoomed }.collect(onZoom) }
    fun pinched(event: PointerEvent) {
        framing.zoom(framing.zoom * event.calculateZoom(), event.calculateCentroid(useCurrent = true), event.calculatePan())
        event.changes.forEach { if (it.positionChanged()) it.consume() }
    }
    val gestures = if (markup == null) {
        Modifier
            .pointerInput(image) {
                detectTapGestures(
                    onDoubleTap = { point -> framing.zoom(if (framing.fitted) framing.fit * 2.5f else framing.fit, point) },
                    onTap = { tapped() },
                )
            }
            .pointerInput(image) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    framing.stop()
                    do {
                        val event = awaitPointerEvent()
                        if (event.changes.count { it.pressed } >= 2 || framing.zoomed) pinched(event)
                    } while (event.changes.any { it.pressed })
                }
            }
    } else {
        Modifier.pointerInput(image, markup) {
            awaitEachGesture {
                val first = awaitFirstDown(requireUnconsumed = false)
                framing.stop()
                val slop = Viewer.markupSlop.toPx() / framing.zoom
                var last = first.position
                var drawing = false
                var zooming = false
                do {
                    val event = awaitPointerEvent()
                    if (!zooming && event.changes.count { it.pressed } >= 2) {
                        if (drawing) markup.cancel()
                        zooming = true
                    }
                    if (zooming) {
                        pinched(event)
                        continue
                    }
                    val change = event.changes.firstOrNull { it.id == first.id } ?: continue
                    last = change.position
                    change.consume()
                    if (!drawing && (last - first.position).getDistance() <= viewConfiguration.touchSlop) continue
                    if (!drawing) framing.pixel(first.position).let { markup.down(it.x, it.y, slop) }
                    drawing = true
                    framing.pixel(last).let { markup.drag(it.x, it.y) }
                } while (event.changes.any { it.pressed })
                if (zooming) return@awaitEachGesture
                if (!drawing) framing.pixel(first.position).let { markup.down(it.x, it.y, slop) }
                framing.pixel(last).let { markup.up(it.x, it.y) }
            }
        }
    }
    Box(Modifier.fillMaxSize().onSizeChanged { framing.resize(it) }.then(gestures)) {
        Canvas(Modifier.fillMaxSize()) {
            val zoom = framing.zoom
            if (zoom == 0f) return@Canvas
            withTransform({
                translate(framing.origin.x, framing.origin.y)
                scale(zoom, zoom, Offset.Zero)
            }) { drawImage(image) }
        }
        if (markup != null) MarkupCanvas(markup, image, framing, Modifier.fillMaxSize())
    }
}

/** The marks of an image being marked up, painted from the session's scene over the picture where `framing` puts it. */
@Composable
private fun MarkupCanvas(session: MarkupSession, image: ImageBitmap, framing: Framing, modifier: Modifier) {
    val context = LocalContext.current
    val text = CanvasText.of(context, LocalDensity.current)
    val palette = LocalPalette.current
    val scene = session.scene ?: return
    val painted = remember(scene) { CanvasScene(scene, text) }
    val colors = remember(palette) { SceneColors(palette, dark = true, surface = Surface.Background, current = palette.foreground) }
    Canvas(modifier) {
        val zoom = framing.zoom
        if (zoom == 0f) return@Canvas
        // The painter scales points by the density; the scene is in the image's pixels.
        val across = image.width * zoom / scene.width / density
        val down = image.height * zoom / scene.height / density
        withTransform({
            translate(framing.origin.x, framing.origin.y)
            scale(across, down, Offset.Zero)
        }) { paintScene(painted, colors) }
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
