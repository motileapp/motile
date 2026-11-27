package app.motile.transcript

import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListLayoutInfo
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
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
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import app.motile.DemoHooks
import app.motile.LocalStore
import app.motile.composer.composerSurface
import app.motile.core.AppStore
import app.motile.core.FindFrom
import app.motile.core.TranscriptModel
import app.motile.core.TranscriptPlace
import app.motile.panel.DiffScope
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.IconSize
import app.motile.theme.LocalPalette
import app.motile.theme.ShadowSize
import app.motile.theme.Theme
import app.motile.theme.symbolSide
import app.motile.ui.Icon
import app.motile.ui.Text
import app.motile.ui.pressesAtOnce
import kotlin.math.abs
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch

/** What a row needs from the transcript around it. */
val LocalTranscript = staticCompositionLocalOf<TranscriptActions> { error("No transcript") }

/** Whether the user's finger, or the fling it gave, still moves the transcript around a row. */
val LocalTranscriptMoves = staticCompositionLocalOf<() -> Boolean> { { false } }

/** What the rows of one transcript do: open and close, copy, and open what they stand for. */
class TranscriptActions(val store: AppStore, val model: TranscriptModel, val ofAgent: Boolean) {
    private val expanded = mutableStateMapOf<String, Unit>()

    /** The row the transcript scrolls to for finding, whose long message opens when it hides the current match. */
    var revealing by mutableStateOf<String?>(null)

    /** Where the current match of finding was last drawn, in the window, and its row. */
    var matchAt: Pair<String, Rect>? = null
        private set

    fun currentMatch(rowID: String, window: Rect) {
        matchAt = rowID to window
    }

    /** The pieces of the text of each row on screen, for where finding starts. */
    val texts = HashMap<String, RowTexts>()

    /** Where each message's row on screen has its bubble, for a message thrown in from the composer. */
    val throwTargets = HashMap<String, ThrowTarget>()

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

    /** Signs the agent's account `accountID` in again. */
    fun signIn(accountID: String) {
        val threadID = model.threadID ?: return
        store.signInAgain(accountID, threadID)
    }

    /** Shows what the agent did that the item's tool call started. */
    fun openAgent(itemID: String) = store.rightPanel.showAgent(itemID)

    /** Opens the thread an agent the thread's agent started works in. */
    fun openThread(id: String) = store.openStartedThread(id)

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

/** The way back to the thread whose agent started this one, above the first row or, with `end`, under the last. */
private data class StartedBy(val end: Boolean)

private class Kept(var parent: String?, var bottom: Dp)

object TranscriptView {
    /** The room between the last row and the composer, which the bottom padding the transcript is given starts with. */
    val composerGap = 80.dp
    /** How close under the top bar the composer and what lies on it may come, which is less than [composerGap] so that a tall strip has room before it scrolls. */
    val overlayGap = 16.dp
    /** The room above the first row, which the rows fade out in. */
    val topFade = 20.dp
    /** How far the jump buttons are from the composer and from the top bar. */
    val jumpButtonGap = 10.dp
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
fun TranscriptView(
    model: TranscriptModel,
    modifier: Modifier = Modifier,
    contentPadding: PaddingValues = PaddingValues(),
    /** What the find bar covers of the top, which revealing a match scrolls it clear of. */
    findCover: Dp = 0.dp,
    /** Whether something lies over the top of the rows, like the find bar or a toast, which the way to the top gives way to. */
    topTaken: Boolean = false,
    /** Where a message sent is drawn on its way in from the composer; without it, none is thrown. */
    throwLayer: ThrowLayer? = null,
) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val density = LocalDensity.current
    val direction = LocalLayoutDirection.current
    val ofAgent = model === store.agentTranscript
    val resets = model.resets
    val rows = model.rows
    val pending = model.pending
    val shown = remember(model.version, pending) { if (pending == null) rows.toList() else rows + pending }
    // The way back to a parent and the room under the rows stay the kept rows' until the next thread's replace them.
    val parentNow = if (ofAgent) null else store.parentThread?.id
    val kept = remember(model) { Kept(parentNow, contentPadding.calculateBottomPadding()) }
    if (!model.keepsRows) {
        kept.parent = parentNow
        kept.bottom = contentPadding.calculateBottomPadding()
    }
    val parent = kept.parent
    val entries: List<Any> = remember(shown, model.showsWorking, parent != null, model.earlier) {
        buildList {
            if (parent != null && !model.earlier) add(StartedBy(end = false))
            addAll(if (!model.showsWorking) shown else shown.toMutableList<Any>().apply { add(workingIndex(shown, pending), Working) })
            if (parent != null) add(StartedBy(end = true))
        }
    }
    val placed = remember(model, resets) { model.place?.let { entries.shownAt(it, density) } }
    val follower = remember(model, resets) { Follower().apply { pinned = placed == null } }
    val throws = remember(model, resets) { Throws(model) }
    // A message just sent is thrown in from the composer, where it was written, if it is still there.
    val sentNow = pending != null && throws.seenPending == null
    throws.seenPending = pending
    val origin = if (pending != null && sentNow && throwLayer != null && !Platform.reducesMotion) ThrowSources.origin(pending) else null
    val starting = if (pending != null && origin != null) ThrownMessage(pending, origin, rows.lastOrNull { it.isSentMessage }?.id) else null
    SideEffect {
        if (!sentNow) return@SideEffect
        throws.thrown = starting
        if (starting != null) origin?.source?.threw()
    }
    // Drawn from the frame of the send on, when its row is first laid out.
    val thrown = if (sentNow) starting else throws.thrown
    val actions = remember(model, resets) { TranscriptActions(store, model, ofAgent) }
    val flying = remember(thrown) { thrown?.let { ThrowLayer.Flying(it) { throws.rowID?.let(actions.throwTargets::get) } } }
    if (throwLayer != null) {
        throwLayer.flying = flying
        DisposableEffect(throwLayer, flying) { onDispose { if (throwLayer.flying === flying) throwLayer.flying = null } }
    }
    val state = remember(model, resets) {
        placed?.let { (index, offset) -> LazyListState(index, offset) } ?: LazyListState(firstVisibleItemIndex = max(0, entries.size - 1))
    }
    val place by rememberUpdatedState {
        val pinned = if (follower.hold != null) follower.wasPinned else follower.pinned
        if (pinned) null else entries.placeAt(state, density)
    }
    // The way back at the end only shows while the one at the top is out of sight; its room stays, so that nothing moves when it does.
    val topInSight by remember(state, entries) { derivedStateOf { entries.firstOrNull() is StartedBy && state.firstVisibleItemIndex == 0 } }
    val scope = rememberCoroutineScope()
    val animates = model.activity.running && model.live && !Platform.reducesMotion
    val top = contentPadding.calculateTopPadding()
    val bottom = kept.bottom
    val listTop = remember(state) { floatArrayOf(0f) }
    // The find bar's room above the first row, so that it scrolls clear of the bar. The rows in view stay where they are, unless they start at the top, where they make room for it.
    val coverPx = with(density) { findCover.roundToPx() }
    val lastCover = remember(state) { intArrayOf(coverPx) }
    SideEffect {
        val moved = coverPx - lastCover[0]
        if (moved == 0) return@SideEffect
        lastCover[0] = coverPx
        if (state.firstVisibleItemIndex == 0 && state.firstVisibleItemScrollOffset == 0) return@SideEffect
        state.requestScrollToItem(state.firstVisibleItemIndex, state.firstVisibleItemScrollOffset + moved)
    }
    val findFrom by rememberUpdatedState { cover: Dp ->
        entries.findFrom(state, actions, listTop[0], with(density) { (top + cover).toPx() })
    }
    DisposableEffect(model) {
        val view = { place() }
        val finds = { cover: Dp -> findFrom(cover) }
        model.placeOfView = view
        model.findFromOfView = finds
        onDispose {
            if (!model.keepsRows) model.place = view()
            if (model.placeOfView === view) model.placeOfView = null
            if (model.findFromOfView === finds) model.findFromOfView = null
        }
    }

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
        if (ofAgent || !follower.pinned || follower.userScrolling || follower.trimming || model.revealing != null) return
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

    /** Goes to the thread's first row, with the turns before what is here loaded first. */
    fun scrollToStart() {
        follower.pinned = false
        follower.userScrolling = false
        follower.hold = null
        val threadID = model.threadID
        if (ofAgent || !model.earlier || threadID == null) {
            scope.launch { state.scrollToItem(0) }
            return
        }
        follower.loadingEarlier = true
        store.core.send("load_earlier", mapOf("thread_id" to threadID, "all" to true)) {
            follower.loadingEarlier = false
            scope.launch {
                withFrameNanos {}
                state.scrollToItem(0)
            }
        }
    }

    /** Goes a little over a fifth of the way in every sixtieth of a second, and never slower than two points in that time. */
    suspend fun glide() {
        var last = 0L
        val least = with(density) { 2.dp.toPx() }
        while (true) {
            val remaining = state.remaining() ?: return state.scrollToEnd()
            if (remaining <= 0.5f || !follower.pinned || follower.userScrolling || throws.thrown != null) return
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
            // The message just sent takes the view to the end with it.
            if (throws.thrown != null && remaining < viewport) return
            if (follower.settled && animates && remaining < viewport) glide() else state.scrollBy(remaining.toFloat())
            trimEarlierIfFar()
        } finally {
            follower.settled = true
        }
    }

    LaunchedEffect(state) {
        snapshotFlow { Triple(state.remaining(), state.layoutInfo.totalItemsCount, model.version) }.collectLatest { follow() }
    }
    LaunchedEffect(thrown) {
        thrown ?: return@LaunchedEffect
        var travelled = 0f
        while (!thrown.done && throws.rowID != null) {
            withFrameNanos(thrown::tick)
            if (!follower.pinned || follower.userScrolling) continue
            val remaining = state.remaining() ?: continue
            val step = min(remaining.toFloat(), (remaining + travelled) * thrown.scrolled - travelled)
            if (abs(step) <= 0.1f) continue
            // On its own, so that the user's scrolling stops it and not the throw.
            launch(start = CoroutineStart.UNDISPATCHED) { travelled += state.scrollBy(step) }
        }
        throws.end(thrown)
        scope.launch { follow() }
    }
    val shownEntries by rememberUpdatedState(entries)
    val revealing = model.revealing
    val revealIndex = revealing?.let { request -> entries.indexOfFirst { (it as? RowModel)?.id == request.rowID } } ?: -1
    LaunchedEffect(revealing?.serial, revealIndex >= 0) {
        val request = revealing ?: return@LaunchedEffect
        // The turns loaded for the row come after; they aren't trimmed away as the end is followed.
        follower.pinned = false
        follower.hold = null
        if (revealIndex < 0) return@LaunchedEffect
        actions.revealing = request.rowID
        reveal(state, shownEntries, request, actions, listTop[0], with(density) { (top + findCover).toPx() })
        actions.revealing = null
        model.revealed(request.serial)
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

    val moves = remember(follower, state) { { follower.userScrolling && state.isScrollInProgress } }

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
            // A tap on the transcript puts the keyboard away, whatever else the tap does. A press
            // that lasts is not a tap: it selects text, which losing the focus would let go of.
            awaitEachGesture {
                awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                val up = withTimeoutOrNull(viewConfiguration.longPressTimeoutMillis) {
                    waitForUpOrCancellation(PointerEventPass.Initial)
                }
                if (up != null) Platform.endEditing()
            }
        },
    ) {
        val side = max(Theme.contentPadding.value, ((maxWidth - Theme.contentWidth) / 2).value).dp
        val bottomFade = max(0f, ((bottom - TranscriptView.composerGap) / 2).value).dp
        CompositionLocalProvider(LocalTranscript provides actions, LocalTranscriptMoves provides moves) {
            LazyColumn(
                Modifier
                    .fillMaxSize()
                    .alpha(if (follower.settled || entries.isEmpty()) 1f else 0f)
                    .fades(top, bottomFade)
                    .onGloballyPositioned { listTop[0] = it.positionInWindow().y }
                    .nestedScroll(userScroll),
                state = state,
                contentPadding = PaddingValues(
                    start = side + contentPadding.calculateStartPadding(direction),
                    end = side + contentPadding.calculateEndPadding(direction),
                    top = top + TranscriptView.topFade + findCover,
                    bottom = bottom,
                ),
            ) {
                items(
                    count = entries.size,
                    key = { index -> entryKey(entries[index]) },
                    contentType = { index ->
                        when (val entry = entries[index]) {
                            is RowModel -> entry.contentType
                            is StartedBy -> "started-by"
                            else -> "working"
                        }
                    },
                ) { index ->
                    val entry = entries[index]
                    val row = entry as? RowModel
                    val fades = animates && row?.isSentMessage != true
                    val animated = Modifier.fillMaxWidth().animateItem(fadeInSpec = if (fades) tween(350) else null, placementSpec = null, fadeOutSpec = null)
                    // The row of the message on its way shows once it is there.
                    val placed = if (row?.isSentMessage == true) animated.graphicsLayer { alpha = if (throws.rowID == row.id) 0f else 1f } else animated
                    Box(placed) {
                        when {
                            row != null -> {
                                val texts = remember(row.id) { RowTexts() }
                                DisposableEffect(texts) {
                                    actions.texts[row.id] = texts
                                    onDispose { actions.texts.remove(row.id, texts) }
                                }
                                CompositionLocalProvider(LocalRowTexts provides texts) { TranscriptRow(row) }
                            }
                            entry is StartedBy && entry.end && topInSight -> Box(Modifier.height(ControlSize.Large.height))
                            entry is StartedBy && parent != null -> StartedByLine { store.openStartedThread(parent) }
                            else -> WorkingLine(model.activity, Modifier.padding(top = 2.dp))
                        }
                    }
                }
            }
            if (thrown != null && throwLayer != null) {
                val column = maxWidth - side * 2 - contentPadding.calculateStartPadding(direction) - contentPadding.calculateEndPadding(direction)
                ThrownCopy(thrown, pending ?: thrown.row, actions.throwTargets, { throws.rowID }, column, throwLayer)
            }
        }
        val jumps by remember(follower, state) {
            androidx.compose.runtime.derivedStateOf {
                if (follower.pinned || state.layoutInfo.totalItemsCount == 0) return@derivedStateOf false
                val remaining = state.remaining() ?: return@derivedStateOf true
                remaining > max(with(density) { 200.dp.toPx() }, state.layoutInfo.viewportSize.height / 2f)
            }
        }
        // Five screens up from the end, unless the view is at the thread's top already.
        val heights = remember(model) { RowHeights() }
        val topJumps by remember(follower, state, entries) {
            derivedStateOf {
                if (model.rows.isEmpty()) return@derivedStateOf false
                val info = state.layoutInfo
                val first = info.visibleItemsInfo.firstOrNull() ?: return@derivedStateOf false
                val last = info.visibleItemsInfo.last()
                // The list may already have laid out rows that these entries don't have yet.
                if (last.index >= entries.size) return@derivedStateOf false
                heights.note(info)
                val keys = { index: Int -> entryKey(entries[index]) }
                val viewport = info.viewportSize.height.toFloat()
                val above = heights.between(keys, 0, first.index) - first.offset
                val below = last.offset + last.size - info.viewportEndOffset + heights.between(keys, last.index + 1, entries.size)
                val atTop = !model.earlier && above < viewport
                !atTop && below >= viewport * 5
            }
        }
        if (topJumps && !topTaken) {
            JumpButton(Symbol.ArrowUp, "Scroll to top", Modifier.align(Alignment.TopCenter).padding(top = top + TranscriptView.jumpButtonGap)) {
                scrollToStart()
            }
        }
        if (jumps) {
            JumpButton(Symbol.ArrowDown, "Scroll to end", Modifier.align(Alignment.BottomCenter).padding(bottom = TranscriptView.jumpButtonBottom(bottom))) {
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

/** Where finding starts: the first row that shows below `covered`, from the list's top, and where its first whole line there is in its text. */
private fun List<Any>.findFrom(state: LazyListState, actions: TranscriptActions, listTop: Float, covered: Float): FindFrom? {
    val info = state.layoutInfo
    val below = covered + info.viewportStartOffset
    val item = info.visibleItemsInfo.firstOrNull { it.offset + it.size > below && getOrNull(it.index) is RowModel } ?: return null
    val row = this[item.index] as RowModel
    return FindFrom(row.id, actions.texts[row.id]?.offset(below = listTop + covered) ?: 0)
}

/** The first row at or under the viewport's top, and how far the top is below the row's: less than none when the row is under it. */
private fun List<Any>.placeAt(state: LazyListState, density: Density): TranscriptPlace? {
    val top = state.firstVisibleItemIndex
    val index = (top until size).firstOrNull { this[it] is RowModel } ?: return null
    val above = state.layoutInfo.visibleItemsInfo.filter { it.index in top until index }.sumOf { it.size }
    val delta = with(density) { (state.firstVisibleItemScrollOffset - above).toDp().value }
    return TranscriptPlace((this[index] as RowModel).id, delta)
}

/** The entry and the offset in it a list opens at to show the place, or null when its row is gone. A place above its row shows the entry before it. */
private fun List<Any>.shownAt(place: TranscriptPlace, density: Density): Pair<Int, Int>? {
    val index = indexOfFirst { (it as? RowModel)?.id == place.rowID }.takeIf { it >= 0 } ?: return null
    if (place.delta < 0) return max(0, index - 1) to 0
    return index to with(density) { place.delta.dp.roundToPx() }
}

/** How far the end of the rows is below the end of the viewport, or null when the last row isn't on screen. */
private fun LazyListState.remaining(): Int? {
    val info = layoutInfo
    val last = info.visibleItemsInfo.lastOrNull() ?: return 0
    if (last.index < info.totalItemsCount - 1) return null
    return last.offset + last.size + info.afterContentPadding - info.viewportEndOffset
}

/** The rows' heights as they were last laid out, by key. The rows between the viewport and the end have been on screen, so
 *  how far the end is comes out right however much the rows' heights differ; one never laid out counts as the average. */
private class RowHeights {
    private val sizes = HashMap<Any, Int>()

    fun note(info: LazyListLayoutInfo) {
        for (item in info.visibleItemsInfo) sizes[item.key] = item.size
    }

    fun between(keys: (Int) -> Any, from: Int, to: Int): Float {
        if (from >= to) return 0f
        val average = if (sizes.isEmpty()) 0f else sizes.values.average().toFloat()
        return (from until to).sumOf { (sizes[keys(it)]?.toFloat() ?: average).toDouble() }.toFloat()
    }
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

/** A round button with an arrow over the rows, which brings the view to the end over the composer, or to the top under the top bar. */
@Composable
private fun JumpButton(symbol: Symbol, label: String, modifier: Modifier, onClick: () -> Unit) {
    val palette = LocalPalette.current
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    Box(
        modifier
            .size(TranscriptView.jumpButtonSide)
            .composerSurface(CircleShape, ShadowSize.Md)
            .background(if (pressed) palette.borderComposer else Color.Transparent, CircleShape)
            .semantics { contentDescription = label }
            .pressesAtOnce(interaction)
            .clickable(interaction, null, role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(symbol, IconSize.Lg, color = palette.foreground)
    }
}

private fun entryKey(entry: Any): String = when (entry) {
    is RowModel -> entry.id
    is StartedBy -> "started-by-${entry.end}"
    else -> "working"
}

/** A line across, broken by a round button that opens the thread whose agent started this one. */
@Composable
private fun StartedByLine(open: () -> Unit) {
    val palette = LocalPalette.current
    val size = ControlSize.Regular
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    Row(Modifier.fillMaxWidth().height(ControlSize.Large.height), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.weight(1f).height(1.dp).background(palette.border))
        Row(
            Modifier
                .padding(horizontal = ControlSize.Regular.padding)
                .height(size.height)
                .background(if (pressed) palette.borderAccent else palette.backgroundAccent, CircleShape)
                .border(1.dp, palette.borderAccent, CircleShape)
                .pressesAtOnce(interaction)
                .clickable(interaction, indication = null, role = Role.Button, onClick = open)
                // Lucide leaves an eighth of the arrow's square empty on each side; its ink starts at the padding.
                .padding(start = size.padding - symbolSide(size.symbol) / 8, end = size.padding),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // The arrow's square leaves room around its ink, so it needs little more from the words.
            Icon(Symbol.CornerLeftUp, size.symbol, Modifier.padding(end = 4.dp), color = palette.foreground)
            Text("Go back to parent thread", style = size.font, color = palette.foreground, maxLines = 1)
        }
        Box(Modifier.weight(1f).height(1.dp).background(palette.border))
    }
}

/**
 * Scrolls to the row, and to the current match in it in the middle of what shows, unless it is in
 * sight already between the bars, the find bar too, which cover `covered` from the list's top.
 * Without a range, the row's top comes into sight.
 */
private suspend fun reveal(
    state: LazyListState,
    entries: List<Any>,
    request: TranscriptModel.Revealing,
    actions: TranscriptActions,
    listTop: Float,
    covered: Float,
) {
    val index = entries.indexOfFirst { (it as? RowModel)?.id == request.rowID }.takeIf { it >= 0 } ?: return
    if (state.layoutInfo.visibleItemsInfo.none { it.index == index }) {
        state.scrollToItem(index)
        state.scrollBy(-state.layoutInfo.viewportSize.height * 0.35f)
    }
    repeat(3) { withFrameNanos {} }
    val info = state.layoutInfo
    val shownTop = covered + info.viewportStartOffset
    val shownBottom = (info.viewportEndOffset - info.afterContentPadding).toFloat()
    val match = actions.matchAt?.takeIf { request.range != null && it.first == request.rowID }?.second
    val row = info.visibleItemsInfo.firstOrNull { it.index == index }
    val (top, bottom) = when {
        match != null -> match.top - listTop + info.viewportStartOffset to match.bottom - listTop + info.viewportStartOffset
        row != null -> row.offset.toFloat() to row.offset + min(row.size.toFloat(), shownBottom - shownTop)
        else -> return
    }
    if (top >= shownTop && bottom <= shownBottom) return
    state.animateScrollBy((top + bottom) / 2 - (shownTop + shownBottom) / 2)
}
