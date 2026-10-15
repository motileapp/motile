package app.motile.transcript

import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import app.motile.DemoHooks
import app.motile.LocalStore
import app.motile.core.AppStore
import app.motile.core.TranscriptModel
import app.motile.panel.DiffScope
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.LocalPalette
import app.motile.theme.ShadowSize
import app.motile.theme.Theme
import app.motile.theme.shadow
import app.motile.ui.Icon
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow

/** What a row needs from the transcript around it. */
val LocalTranscript = staticCompositionLocalOf<TranscriptActions> { error("No transcript") }

/** What the rows of one transcript do: open and close, copy, and open what they stand for. */
class TranscriptActions(val store: AppStore, val model: TranscriptModel, val ofAgent: Boolean) {
    private val expanded = mutableStateMapOf<String, Unit>()

    /** Called as a row opens or closes, which mustn't drag the view to the end; `byCore` when its rows come from the core. */
    var holding: (byCore: Boolean) -> Unit = {}

    fun isExpanded(id: String): Boolean = id in expanded

    fun toggleExpansion(id: String) {
        holding(false)
        if (expanded.remove(id) == null) expanded[id] = Unit
    }

    /** Opens or closes a group or a fold, whose rows the core adds and removes. */
    fun toggleRow(id: String) {
        val threadID = model.threadID ?: return
        holding(true)
        store.core.send("toggle_row", mapOf("thread_id" to threadID, "row_id" to id))
    }

    /** Opens or closes a folder among a turn's changed files. */
    fun toggleFolder(id: String, rowID: String) = toggleRow(id)

    /** Shows what the turn that ended with the item changed, with the file at `path` in view. */
    fun openDiff(itemID: String, path: String?) {
        if (ofAgent) return
        if (path == null) store.rightPanel.showDiff(DiffScope.Turn(itemID)) else store.rightPanel.showChange(itemID, path)
    }

    /** Shows what the agent did that the item's tool call started. */
    fun openAgent(itemID: String) = store.rightPanel.showAgent(itemID)

    /** Copies the reply that ends at the row: every stretch of prose and code back to the user's message. */
    fun copyReply(rowID: String) {
        val rows = model.rows
        val end = rows.indexOfFirst { it.id == rowID }
        if (end < 0) return
        val parts = mutableListOf<String>()
        var index = end - 1
        while (index >= 0 && !rows[index].isUser) {
            rows[index].plainText?.let(parts::add)
            index -= 1
        }
        Platform.copy(parts.reversed().joinToString("\n\n"))
    }
}

/** Where the transcript is, as the user and the rows move it. */
private class Follower {
    /** Whether the view follows the end of the transcript as it grows. It then rests on the end whenever the user isn't scrolling. */
    var pinned by mutableStateOf(true)
    /** Whether the user's finger, or the fling it gave, moves the viewport. */
    var userScrolling = false
    /** The rows' version a row that opened waits for before the view follows again, and whether it followed before. */
    var hold: Int? = null
    var wasPinned = false
    /** The view has come to rest on the end once since the rows were replaced, and is shown. */
    var settled by mutableStateOf(false)
    var loadingEarlier = false
    var trimming = false
    val highlighted = mutableSetOf<String>()
}

/** The line that says the agent works, among the rows. */
private object Working

object TranscriptView {
    /** The room between the last row and the composer, which the bottom padding the transcript is given starts with. */
    val composerGap = 48.dp
    /** The room above the first row, which the rows fade out in. */
    val topFade = 20.dp
    private val jumpButtonGap = 10.dp
    val jumpButtonSide = 38.dp
    /** How close to the end the viewport has to be to go to the end when the keyboard opens. */
    val nearEnd = 80.dp

    /** Where the jump button stands above the bottom, over the composer's gap. */
    fun jumpButtonBottom(bottom: Dp): Dp = max(jumpButtonGap.value, (bottom - composerGap + jumpButtonGap).value).dp
}

/**
 * The rows of a thread, or of an agent it started, of which only those on screen are composed. It
 * stays at the end while rows come in and the user hasn't scrolled up, asks for earlier turns near
 * its top and lets go of them far above the end, and never moves what is on screen while rows
 * splice in. `contentPadding` leaves room for what floats over its ends: the top bar and the
 * composer, under which the rows fade out.
 */
@Composable
fun TranscriptView(model: TranscriptModel, modifier: Modifier = Modifier, contentPadding: PaddingValues = PaddingValues()) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val density = LocalDensity.current
    val direction = LocalLayoutDirection.current
    val ofAgent = model === store.agentTranscript
    val resets = model.resets
    val rows = model.rows
    val pending = model.pending
    val shown = remember(model.version, pending) { if (pending == null) rows.toList() else rows + pending }
    val entries: List<Any> = remember(shown, model.showsWorking) {
        if (!model.showsWorking) shown else shown.toMutableList<Any>().apply { add(workingIndex(shown, pending), Working) }
    }
    val follower = remember(model, resets) { Follower() }
    val state = remember(model, resets) { LazyListState(firstVisibleItemIndex = max(0, entries.size - 1)) }
    val actions = remember(model, resets) { TranscriptActions(store, model, ofAgent) }
    val scope = rememberCoroutineScope()
    val animates = model.activity.running && model.live && !Platform.reducesMotion
    val top = contentPadding.calculateTopPadding()
    val bottom = contentPadding.calculateBottomPadding()

    actions.holding = { byCore ->
        follower.wasPinned = follower.pinned
        follower.pinned = false
        follower.hold = if (byCore) model.version + 1 else model.version
    }

    fun loadEarlierIfNear() {
        val threadID = model.threadID ?: return
        if (ofAgent || !model.earlier || follower.loadingEarlier || model.rows.isEmpty()) return
        val (above, viewport) = state.distanceAbove() ?: return
        if (above >= viewport * 3) return
        follower.loadingEarlier = true
        store.core.send("load_earlier", mapOf("thread_id" to threadID)) {
            follower.loadingEarlier = false
            loadEarlierIfNear()
        }
    }

    fun trimEarlierIfFar() {
        val threadID = model.threadID ?: return
        if (ofAgent || !follower.pinned || follower.userScrolling || follower.trimming) return
        val (above, viewport) = state.distanceAbove() ?: return
        if (above <= viewport * 12) return
        val info = state.layoutInfo
        val average = info.visibleItemsInfo.sumOf { it.size }.toFloat() / max(1, info.visibleItemsInfo.size)
        val keptFrom = state.firstVisibleItemIndex - floor(viewport * 6 / max(1f, average)).toInt()
        val keep = model.rows.size - keptFrom.coerceIn(0, model.rows.size)
        if (keep >= model.rows.size) return
        follower.trimming = true
        store.core.send("trim_earlier", mapOf("thread_id" to threadID, "keep_rows" to keep)) { follower.trimming = false }
    }

    /** Goes a little over a fifth of the way in every sixtieth of a second, and never slower than two points in that time. */
    suspend fun glide() {
        var last = 0L
        val least = with(density) { 2.dp.toPx() }
        while (true) {
            val remaining = state.remaining() ?: return state.scrollToEnd()
            if (remaining <= 0.5f || !follower.pinned || follower.userScrolling) return
            val sixtieths = withFrameNanos { now ->
                val passed = if (last == 0L) 1f else max(0.25f, (now - last) / 1e9f * 60f)
                last = now
                passed
            }
            val step = max(least * sixtieths, remaining * (1 - 0.78f.pow(sixtieths)))
            state.scrollBy(min(remaining.toFloat(), step))
        }
    }

    /** Brings a view that follows the end to rest on it, unless the user has it or a row that opened holds it. */
    suspend fun follow() {
        try {
            follower.hold?.let { hold ->
                if (model.version < hold) return
                withFrameNanos {}
                val remaining = state.remaining()
                follower.pinned = follower.wasPinned && remaining != null && remaining <= 1
                follower.hold = null
                return
            }
            if (!follower.pinned || follower.userScrolling) return
            val remaining = state.remaining() ?: return state.scrollToEnd()
            if (remaining <= 0) return
            val viewport = state.layoutInfo.viewportSize.height
            if (follower.settled && animates && remaining < viewport) glide() else state.scrollBy(remaining.toFloat())
            trimEarlierIfFar()
        } finally {
            follower.settled = true
        }
    }

    LaunchedEffect(state) {
        snapshotFlow { Triple(state.remaining(), state.layoutInfo.totalItemsCount, model.version) }.collectLatest { follow() }
    }
    LaunchedEffect(state) {
        snapshotFlow { Triple(state.firstVisibleItemIndex, model.earlier, model.version) }.collect { loadEarlierIfNear() }
    }
    LaunchedEffect(state) {
        snapshotFlow { state.layoutInfo.visibleItemsInfo.map { it.key } to model.version }.collect { (keys, _) ->
            val threadID = model.threadID ?: return@collect
            val visible = keys.toSet()
            val wanted = (model.rows + listOfNotNull(model.pending))
                .filter { it.id in visible && it.needsHighlight && it.id !in follower.highlighted }
                .map { it.id }
            if (wanted.isEmpty()) return@collect
            follower.highlighted += wanted
            store.core.send("highlight", mapOf("thread_id" to threadID, "row_ids" to wanted))
        }
    }
    // More room under the rows, as for the keyboard, is followed from near the end too.
    val bottomPx = with(density) { bottom.toPx() }
    val lastBottom = remember(state) { floatArrayOf(bottomPx) }
    LaunchedEffect(bottomPx) {
        val grown = bottomPx - lastBottom[0]
        lastBottom[0] = bottomPx
        if (grown <= 0 || follower.userScrolling) return@LaunchedEffect
        val remaining = state.remaining() ?: return@LaunchedEffect
        if (remaining - grown < with(density) { TranscriptView.nearEnd.toPx() }) follower.pinned = true
    }
    DisposableEffect(state, ofAgent) {
        val hook = {
            scope.launch {
                follower.pinned = false
                state.scrollToItem(0)
            }
            Unit
        }
        if (!ofAgent) DemoHooks.scrollTranscriptToTop = hook
        onDispose { if (DemoHooks.scrollTranscriptToTop === hook) DemoHooks.scrollTranscriptToTop = null }
    }

    val userScroll = remember(follower, state) {
        object : NestedScrollConnection {
            override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                if (source != NestedScrollSource.UserInput) return Offset.Zero
                follower.userScrolling = true
                follower.hold = null
                // Scrolling up, by however little, stops following the end.
                if (available.y > 0.5f) follower.pinned = false
                return Offset.Zero
            }

            override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                if (!state.canScrollForward && consumed.y <= 0f) follower.pinned = true
                return Offset.Zero
            }

            override suspend fun onPreFling(available: Velocity): Velocity {
                if (available.y > 0f) follower.pinned = false
                return Velocity.Zero
            }

            override suspend fun onPostFling(consumed: Velocity, available: Velocity): Velocity {
                follower.userScrolling = false
                if (!state.canScrollForward) follower.pinned = true
                follow()
                return Velocity.Zero
            }
        }
    }

    BoxWithConstraints(
        modifier.pointerInput(Unit) {
            // A tap on the transcript puts the keyboard away, whatever else the tap does.
            awaitEachGesture {
                awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                if (waitForUpOrCancellation(PointerEventPass.Initial) != null) Platform.endEditing()
            }
        },
    ) {
        val side = max(Theme.contentPadding.value, ((maxWidth - Theme.contentWidth) / 2).value).dp
        val bottomFade = max(0f, ((bottom - TranscriptView.composerGap) / 2).value).dp
        CompositionLocalProvider(LocalTranscript provides actions) {
            LazyColumn(
                Modifier
                    .fillMaxSize()
                    .alpha(if (follower.settled || entries.isEmpty()) 1f else 0f)
                    .fades(top, bottomFade)
                    .nestedScroll(userScroll),
                state = state,
                contentPadding = PaddingValues(
                    start = side + contentPadding.calculateStartPadding(direction),
                    end = side + contentPadding.calculateEndPadding(direction),
                    top = top + TranscriptView.topFade,
                    bottom = bottom,
                ),
            ) {
                items(
                    count = entries.size,
                    key = { index -> (entries[index] as? RowModel)?.id ?: "working" },
                    contentType = { index -> (entries[index] as? RowModel)?.contentType ?: "working" },
                ) { index ->
                    val entry = entries[index]
                    val row = entry as? RowModel
                    val fades = animates && row?.isSentMessage != true
                    val placed = Modifier.fillMaxWidth().animateItem(fadeInSpec = if (fades) tween(350) else null, placementSpec = null, fadeOutSpec = null)
                    Box(placed) {
                        if (row == null) WorkingLine(model.activity, Modifier.padding(top = 2.dp)) else TranscriptRow(row)
                    }
                }
            }
        }
        val jumps by remember(follower, state) {
            androidx.compose.runtime.derivedStateOf {
                if (follower.pinned || state.layoutInfo.totalItemsCount == 0) return@derivedStateOf false
                val remaining = state.remaining() ?: return@derivedStateOf true
                remaining > max(with(density) { 200.dp.toPx() }, state.layoutInfo.viewportSize.height / 2f)
            }
        }
        if (jumps) {
            JumpButton(Modifier.align(Alignment.BottomCenter).padding(bottom = TranscriptView.jumpButtonBottom(bottom))) {
                follower.pinned = true
                follower.userScrolling = false
                scope.launch {
                    state.scrollToEnd()
                    trimEarlierIfFar()
                }
            }
        }
    }
}

/** The row the working line goes above: the first of the messages that wait for the agent, which are the last rows, the one on its way among them. Without them it goes under every row. */
private fun workingIndex(rows: List<RowModel>, pending: RowModel?): Int {
    val end = rows.size - (if (pending != null && !pending.isQueued) 1 else 0)
    var index = end
    while (index > 0 && rows[index - 1].isQueued) index -= 1
    return if (index == end) rows.size else index
}

/** How far the end of the rows is below the end of the viewport, or null when the last row isn't on screen. */
private fun LazyListState.remaining(): Int? {
    val info = layoutInfo
    val last = info.visibleItemsInfo.lastOrNull() ?: return 0
    if (last.index < info.totalItemsCount - 1) return null
    return last.offset + last.size + info.afterContentPadding - info.viewportEndOffset
}

/** About how far the viewport's top is below the first row, from the rows on screen, and how tall the viewport is. */
private fun LazyListState.distanceAbove(): Pair<Float, Float>? {
    val info = layoutInfo
    val visible = info.visibleItemsInfo
    if (visible.isEmpty()) return null
    val average = visible.sumOf { it.size }.toFloat() / visible.size
    return (firstVisibleItemIndex * average + firstVisibleItemScrollOffset) to info.viewportSize.height.toFloat()
}

private suspend fun LazyListState.scrollToEnd() {
    val last = layoutInfo.totalItemsCount - 1
    if (last < 0) return
    scrollToItem(last)
    remaining()?.takeIf { it > 0 }?.let { scrollBy(it.toFloat()) }
}

/** Fades the rows out under the top bar, `top` tall, and from `bottomFade` above the bottom down to it, where the composer is. */
private fun Modifier.fades(top: Dp, bottomFade: Dp): Modifier =
    graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
        .drawWithContent {
            drawContent()
            val start = top.toPx()
            val end = start + TranscriptView.topFade.toPx()
            drawRect(Color.Transparent, size = Size(size.width, start), blendMode = BlendMode.Clear)
            drawRect(
                Brush.verticalGradient(0f to Color.Transparent, 1f to Color.Black, startY = start, endY = end),
                topLeft = Offset(0f, start),
                size = Size(size.width, end - start),
                blendMode = BlendMode.DstIn,
            )
            val fade = bottomFade.toPx()
            if (fade <= 0f) return@drawWithContent
            val from = size.height - fade
            drawRect(
                Brush.verticalGradient(0f to Color.Black, 1f to Color.Transparent, startY = from, endY = size.height),
                topLeft = Offset(0f, from),
                size = Size(size.width, fade),
                blendMode = BlendMode.DstIn,
            )
        }

/** The round button that brings the view back to the end, over the composer. */
@Composable
private fun JumpButton(modifier: Modifier, onClick: () -> Unit) {
    val palette = LocalPalette.current
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    Box(
        modifier
            .size(TranscriptView.jumpButtonSide)
            .shadow(ShadowSize.Md, CircleShape)
            .background(if (pressed) palette.borderPopover else palette.popover, CircleShape)
            .border(1.dp, palette.borderPopover, CircleShape)
            .semantics { contentDescription = "Scroll to end" }
            .clickable(interaction, null, role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(Symbol.ArrowDown, 12f, color = palette.foreground)
    }
}
