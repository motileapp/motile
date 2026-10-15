package app.motile.layout

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.awaitHorizontalTouchSlopOrCancellation
import androidx.compose.foundation.gestures.horizontalDrag
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.focus.FocusManager
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import app.motile.panel.RightPanel
import app.motile.theme.Opacity
import app.motile.theme.ShadowSize
import app.motile.theme.Theme
import app.motile.theme.shadow
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlin.math.abs
import kotlin.math.roundToInt

/** Whether the sidebar is shown beside the thread, which a swipe to the right does on a phone. */
class Drawer {
    var isOpen by mutableStateOf(false)
}

val LocalDrawer = compositionLocalOf { Drawer() }

/** The drawer of the screen shown, for what drives the client from outside its views. */
object LocalDrawerHolder {
    var drawer: Drawer? = null
}

private enum class Side { Sidebar, Panel }

/** A screen narrower than this gives all of its width to the sidebar and the panel. */
private val fullBelow = 500.dp
private val widest = 360.dp
/** What the right panel leaves of the card on a screen too wide to give it all. */
private val panelPeek = 64.dp

/**
 * Where the card is and how a finger moves it. A swipe reaches it two ways: on what doesn't
 * scroll sideways, as the touch itself, and on what does, like code or a table, as the scroll it
 * leaves over at its end, so that what scrolls sideways has the swipe first.
 */
private class DrawerCard(private val scope: CoroutineScope, private val drawer: Drawer, private val rightPanel: RightPanel, private val focus: FocusManager) {
    /** How far the card is aside: to the right towards 1, over the sidebar, and to the left towards -1, over the panel. */
    val progress = Animatable(0f)
    var open by mutableStateOf<Side?>(null)
    var sidebarWidth = 1f
    var panelWidth = 1f

    /** The side a finger moves the card over, while one does. */
    var dragged by mutableStateOf<Side?>(null)
        private set
    private var start = 0f
    private var moved = 0f

    fun setOpen(side: Side?, velocity: Float = 0f) {
        open = side
        if (side != null) focus.clearFocus()
        if (drawer.isOpen != (side == Side.Sidebar)) drawer.isOpen = side == Side.Sidebar
        if (rightPanel.isOpen != (side == Side.Panel)) rightPanel.isOpen = side == Side.Panel
        val target = when (side) {
            Side.Sidebar -> 1f
            Side.Panel -> -1f
            null -> 0f
        }
        scope.launch { progress.animateTo(target, spring(dampingRatio = 1f, stiffness = 484f), velocity) }
    }

    /** Only the swipe back is the drawer's while a side is open. */
    fun takes(rightwards: Boolean): Boolean = when (open) {
        Side.Sidebar -> !rightwards
        Side.Panel -> rightwards
        null -> true
    }

    /** A finger starts moving the card, the way it set out. One swipe moves it over one side only. */
    fun begin(direction: Float) {
        start = progress.value
        moved = 0f
        dragged = if (start > 0 || (start == 0f && direction > 0)) Side.Sidebar else Side.Panel
        focus.clearFocus()
        scope.launch { progress.stop() }
    }

    fun move(by: Float) {
        val side = dragged ?: return
        moved += by
        val (low, high) = if (side == Side.Sidebar) 0f to 1f else -1f to 0f
        val wanted = start + moved / width(side)
        // Past its ends the card gives a little and no more.
        val over = if (wanted > high) wanted - high else if (wanted < low) wanted - low else 0f
        scope.launch { progress.snapTo(wanted.coerceIn(low, high) + over * 0.12f) }
    }

    /** The finger lets go at `speed` pixels a second: a fast swipe opens or closes the way it went, a slow one by how far it went. */
    fun end(speed: Float, fast: Float) {
        val side = dragged ?: return
        dragged = null
        val outwards = if (side == Side.Sidebar) speed else -speed
        val opens = if (abs(speed) > fast) outwards > 0 else abs(progress.value) > 0.5f
        setOpen(if (opens) side else null, speed / width(side))
    }

    private fun width(side: Side) = if (side == Side.Sidebar) sidebarWidth else panelWidth
}

/**
 * The sidebar and the right panel under the thread: swiping right anywhere on the thread slides
 * it aside as a card and shows the sidebar, swiping left shows the panel, and a swipe back closes
 * either. On a phone they take the whole screen; a wider one leaves some of the card in view, and
 * a tap on it closes too. What scrolls sideways under the finger, like code, has the swipe first.
 */
@Composable
fun DrawerView(
    drawer: Drawer,
    rightPanel: RightPanel,
    sidebar: @Composable () -> Unit,
    content: @Composable () -> Unit,
    panel: @Composable () -> Unit,
) {
    BoxWithConstraints(Modifier.fillMaxSize().background(Theme.colors.background)) {
        val density = LocalDensity.current
        val full = maxWidth < fullBelow
        val sidebarWidth = if (full) maxWidth else widest
        val panelWidth = if (full) maxWidth else maxWidth - panelPeek
        val fast = with(density) { 280.dp.toPx() }
        val scope = rememberCoroutineScope()
        val focus = LocalFocusManager.current
        val card = remember { DrawerCard(scope, drawer, rightPanel, focus) }
        card.sidebarWidth = with(density) { sidebarWidth.toPx() }
        card.panelWidth = with(density) { panelWidth.toPx() }

        // Opens or closes as the client's state says, unless a finger is moving the card. Asked for both sides, the one that isn't open yet was asked for last.
        LaunchedEffect(Unit) {
            snapshotFlow { drawer.isOpen to rightPanel.isOpen }.collect { (sidebarOpen, panelOpen) ->
                if (card.dragged != null) return@collect
                val wanted = when (card.open) {
                    Side.Sidebar -> if (panelOpen) Side.Panel else if (sidebarOpen) Side.Sidebar else null
                    else -> if (sidebarOpen) Side.Sidebar else if (panelOpen) Side.Panel else null
                }
                if (wanted != card.open) card.setOpen(wanted)
            }
        }
        BackHandler(enabled = card.open != null) { card.setOpen(null) }

        val progress = card.progress.value
        val panelShown = progress < 0
        LaunchedEffect(panelShown) { if (panelShown) rightPanel.showPullRequestIfBlank() }

        // What scrolls sideways leaves the drawer what it doesn't take, and once the drawer has the swipe it keeps all of it.
        val leftOver = remember(card) {
            object : NestedScrollConnection {
                override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                    if (card.dragged == null || source != NestedScrollSource.UserInput) return Offset.Zero
                    card.move(available.x)
                    return Offset(available.x, 0f)
                }

                override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                    if (source != NestedScrollSource.UserInput || available.x == 0f || abs(available.x) < abs(available.y)) return Offset.Zero
                    if (card.dragged == null) {
                        if (!card.takes(available.x > 0)) return Offset.Zero
                        card.begin(available.x)
                    }
                    card.move(available.x)
                    return Offset(available.x, 0f)
                }

                override suspend fun onPreFling(available: Velocity): Velocity {
                    if (card.dragged == null) return Velocity.Zero
                    card.end(available.x, fast)
                    return Velocity(available.x, 0f)
                }
            }
        }

        val aside = abs(progress)
        // The swipe is the whole screen's: over the thread, the sidebar or the panel, whichever shows.
        Box(
            Modifier
                .fillMaxSize()
                .nestedScroll(leftOver)
                .pointerInput(card) {
                    awaitEachGesture {
                        val down = awaitFirstDown(requireUnconsumed = false)
                        var direction = 0f
                        val drag = awaitHorizontalTouchSlopOrCancellation(down.id) { change, over ->
                            if (!card.takes(over > 0)) return@awaitHorizontalTouchSlopOrCancellation
                            direction = over
                            change.consume()
                        } ?: return@awaitEachGesture
                        card.begin(direction)
                        card.move(direction)
                        val tracker = VelocityTracker()
                        tracker.addPosition(drag.uptimeMillis, drag.position)
                        horizontalDrag(drag.id) { change ->
                            card.move(change.position.x - change.previousPosition.x)
                            tracker.addPosition(change.uptimeMillis, change.position)
                            change.consume()
                        }
                        card.end(tracker.calculateVelocity().x, fast)
                    }
                },
        ) {
            if (progress > 0) {
                Box(Modifier.width(sidebarWidth).fillMaxHeight()) { sidebar() }
            }
            if (panelShown) {
                Box(Modifier.width(panelWidth).fillMaxHeight().align(Alignment.TopEnd)) { panel() }
            }
            val cardOffset = progress * (if (progress < 0) card.panelWidth else card.sidebarWidth)
            Box(
                Modifier
                    .fillMaxSize()
                    .offset { IntOffset(cardOffset.roundToInt(), 0) }
                    .then(if (aside > 0) Modifier.shadow(ShadowSize.Lg, RectangleShape, Opacity.ShadowStronger) else Modifier),
            ) {
                Box(Modifier.fillMaxSize().clipToBounds().background(Theme.colors.background)) {
                    content()
                    if (progress != 0f) {
                        // Over the card while a side shows: it dims the thread and takes the tap that closes.
                        Box(
                            Modifier
                                .fillMaxSize()
                                .alpha(0.55f * aside)
                                .background(Theme.colors.background)
                                .clickable(remember { MutableInteractionSource() }, indication = null) { card.setOpen(null) },
                        )
                    }
                    // The card's left and right edges, lined while it is aside.
                    val lifted = minOf(1f, aside * 6)
                    if (lifted > 0) {
                        Box(Modifier.width(1.dp).fillMaxHeight().alpha(lifted).background(Theme.colors.border))
                        Box(Modifier.width(1.dp).fillMaxHeight().align(Alignment.TopEnd).alpha(lifted).background(Theme.colors.border))
                    }
                }
            }
        }
    }
}
