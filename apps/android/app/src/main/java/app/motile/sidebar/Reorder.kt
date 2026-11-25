package app.motile.sidebar

import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.lazy.LazyListItemInfo
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.abs

/**
 * A row of the list lifted by a hold and moved by the finger, down a column or along a row. The rows
 * it passes make way for it, and the list scrolls while it is held past an end. Only rows that move
 * can make way.
 */
@Stable
class Reorder(private val list: LazyListState) {
    /** The id of the row that is lifted. */
    var key by mutableStateOf<String?>(null)
        private set
    /** The ids of the rows as they lie while one is moved. */
    private var order by mutableStateOf<List<String>?>(null)
    private var movable: Set<String> = emptySet()
    private var startOffset = 0
    private var dragged by mutableFloatStateOf(0f)
    /** Where the lifted row was laid out when it last made way, until the list lays it out again. */
    private var movedFrom: Int? = null

    private val item: LazyListItemInfo? get() = list.layoutInfo.visibleItemsInfo.firstOrNull { it.key == key }

    val horizontal: Boolean get() = list.layoutInfo.orientation == Orientation.Horizontal

    /** How far the lifted row is drawn from its place in the list. */
    val offset: Float
        get() {
            val item = item ?: return 0f
            return startOffset + dragged - item.offset
        }

    /** The rows in the order they lie in while one is moved. */
    fun <T> arranged(items: List<T>, id: (T) -> String): List<T> {
        val order = order ?: return items
        val byID = items.associateBy(id)
        if (byID.keys != order.toSet()) return items
        return order.map { byID.getValue(it) }
    }

    fun start(key: String, keys: List<String>, movable: Set<String>) {
        this.key = key
        this.movable = movable
        order = keys
        startOffset = item?.offset ?: 0
        dragged = 0f
        movedFrom = null
    }

    fun drag(by: Float) {
        dragged += by
        makeWay()
    }

    /**
     * Moves the lifted row past a neighbour once it is halfway across it: into the place whose middle
     * is nearest its own, whatever their sizes, and back only once another place is nearer by more
     * than a little.
     */
    fun makeWay() {
        val item = item ?: return
        val order = order ?: return
        if (movedFrom == item.offset) return
        movedFrom = null
        val from = order.indexOf(key)
        if (from < 0) return
        val visible = list.layoutInfo.visibleItemsInfo
        fun neighbour(index: Int) = order.getOrNull(index)?.takeIf { it in movable }?.let { other -> visible.firstOrNull { it.key == other } }
        val spacing = list.layoutInfo.mainAxisItemSpacing
        val held = startOffset + dragged + item.size / 2f
        val here = item.offset + item.size / 2f
        val next = neighbour(from + 1)
        val previous = neighbour(from - 1)
        val to = when {
            next != null && held - here > (next.size + spacing + HYSTERESIS) / 2f -> from + 1
            previous != null && here - held > (previous.size + spacing + HYSTERESIS) / 2f -> from - 1
            else -> return
        }
        movedFrom = item.offset
        // The list keeps its first row where it is, rather than following it.
        if (minOf(from, to) <= list.firstVisibleItemIndex) list.requestScrollToItem(list.firstVisibleItemIndex, list.firstVisibleItemScrollOffset)
        this.order = order.toMutableList().apply { add(to, removeAt(from)) }
    }

    /** How far past an end of the list the lifted row is held. */
    fun overscroll(): Float {
        val item = item ?: return 0f
        val info = list.layoutInfo
        val top = startOffset + dragged
        val bottom = top + item.size
        if (top < info.viewportStartOffset) return top - info.viewportStartOffset
        if (bottom > info.viewportEndOffset) return bottom - info.viewportEndOffset
        return 0f
    }

    /** Puts the row down: where it is among the rows, or null if it can't say. */
    fun finish(): Int? {
        val index = order?.indexOf(key)
        cancel()
        return index?.takeIf { it >= 0 }
    }

    fun cancel() {
        key = null
        order = null
        dragged = 0f
        movedFrom = null
    }

    private companion object {
        /** How much nearer, in pixels, another place must be before the row leaves the one it would land in. */
        const val HYSTERESIS = 8f
    }
}

/** Scrolls the list while the lifted row is held past one of its ends. */
@Composable
fun ReorderScrolling(reorder: Reorder, list: LazyListState) {
    LaunchedEffect(reorder.key) {
        if (reorder.key == null) return@LaunchedEffect
        while (true) {
            withFrameNanos {}
            val over = reorder.overscroll()
            if (over == 0f) continue
            list.scrollBy(over * 0.2f)
            reorder.makeWay()
        }
    }
}

/**
 * A hold lifts the row and a move drags it; a hold that doesn't move brings up its menu, and a move
 * out of the menu still drags it. `lift` is told when the row is lifted, `put` where it was put down.
 */
@Composable
fun Modifier.reorders(
    enabled: Boolean,
    key: String,
    reorder: Reorder,
    keys: () -> List<String>,
    movable: () -> Set<String>,
    openMenu: () -> Unit,
    closeMenu: () -> Unit,
    put: (Int) -> Unit,
    lift: () -> Unit = {},
): Modifier {
    val scope = rememberCoroutineScope()
    val currentKeys by rememberUpdatedState(keys)
    val currentMovable by rememberUpdatedState(movable)
    val currentOpen by rememberUpdatedState(openMenu)
    val currentClose by rememberUpdatedState(closeMenu)
    val currentPut by rememberUpdatedState(put)
    val currentLift by rememberUpdatedState(lift)
    if (!enabled) return this
    return pointerInput(key) {
        var travelled = 0f
        var moving = false
        var waiting: Job? = null
        detectDragGesturesAfterLongPress(
            onDragStart = {
                travelled = 0f
                moving = false
                reorder.start(key, currentKeys(), currentMovable())
                currentLift()
                waiting = scope.launch {
                    delay(MENU_AFTER)
                    currentOpen()
                }
            },
            onDrag = { change, amount ->
                change.consume()
                travelled += abs(amount.x) + abs(amount.y)
                if (!moving && travelled > viewConfiguration.touchSlop) {
                    moving = true
                    waiting?.cancel()
                    currentClose()
                }
                reorder.drag(if (reorder.horizontal) amount.x else amount.y)
            },
            onDragEnd = {
                waiting?.cancel()
                if (!moving) {
                    reorder.cancel()
                    currentOpen()
                    return@detectDragGesturesAfterLongPress
                }
                reorder.finish()?.let(currentPut)
            },
            onDragCancel = {
                waiting?.cancel()
                reorder.cancel()
            },
        )
    }
}

/** How long a lifted row is held still before its menu comes up. */
private const val MENU_AFTER = 500L
