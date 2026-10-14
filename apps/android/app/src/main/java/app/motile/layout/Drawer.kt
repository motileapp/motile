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
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import app.motile.panel.RightPanel
import app.motile.theme.Opacity
import app.motile.theme.ShadowSize
import app.motile.theme.Theme
import app.motile.theme.shadow
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
        val sidebarPx = with(density) { sidebarWidth.toPx() }
        val panelPx = with(density) { panelWidth.toPx() }
        /** How far the card is aside: to the right towards 1, over the sidebar, and to the left towards -1, over the panel. */
        val progress = remember { Animatable(0f) }
        var open by remember { mutableStateOf<Side?>(null) }
        var dragging by remember { mutableStateOf(false) }
        val scope = rememberCoroutineScope()
        val focus = LocalFocusManager.current

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
        val setOpenNow by rememberUpdatedState(::setOpen)

        // Opens or closes as the client's state says, unless a finger is moving the card. Asked for both sides, the one that isn't open yet was asked for last.
        LaunchedEffect(Unit) {
            snapshotFlow { drawer.isOpen to rightPanel.isOpen }.collect { (sidebarOpen, panelOpen) ->
                if (dragging) return@collect
                val wanted = when (open) {
                    Side.Sidebar -> if (panelOpen) Side.Panel else if (sidebarOpen) Side.Sidebar else null
                    else -> if (sidebarOpen) Side.Sidebar else if (panelOpen) Side.Panel else null
                }
                if (wanted != open) setOpenNow(wanted, 0f)
            }
        }
        BackHandler(enabled = open != null) { setOpen(null) }

        val panelShown = progress.value < 0
        LaunchedEffect(panelShown) { if (panelShown) rightPanel.showPullRequestIfBlank() }

        val aside = abs(progress.value)
        // The swipe is the whole screen's: over the thread, the sidebar or the panel, whichever shows.
        Box(
            Modifier
                .fillMaxSize()
                .pointerInput(sidebarPx, panelPx) {
                    awaitEachGesture {
                        val down = awaitFirstDown(requireUnconsumed = false)
                        var direction = 0f
                        val drag = awaitHorizontalTouchSlopOrCancellation(down.id) { change, over ->
                            val rightwards = over > 0
                            // Only the swipe back is the drawer's while a side is open.
                            val allowed = when (open) {
                                Side.Sidebar -> !rightwards
                                Side.Panel -> rightwards
                                null -> true
                            }
                            if (!allowed) return@awaitHorizontalTouchSlopOrCancellation
                            direction = over
                            change.consume()
                        } ?: return@awaitEachGesture
                        dragging = true
                        val start = progress.value
                        val dragged = if (start > 0 || (start == 0f && direction > 0)) Side.Sidebar else Side.Panel
                        val width = if (dragged == Side.Sidebar) sidebarPx else panelPx
                        val (low, high) = if (dragged == Side.Sidebar) 0f to 1f else -1f to 0f
                        val tracker = VelocityTracker()
                        var moved = direction
                        focus.clearFocus()
                        tracker.addPosition(drag.uptimeMillis, drag.position)
                        horizontalDrag(drag.id) { change ->
                            moved += change.positionChange().x
                            tracker.addPosition(change.uptimeMillis, change.position)
                            val wanted = start + moved / width
                            // Past its ends the card gives a little and no more.
                            val over = if (wanted > high) wanted - high else if (wanted < low) wanted - low else 0f
                            scope.launch { progress.snapTo(wanted.coerceIn(low, high) + over * 0.12f) }
                            change.consume()
                        }
                        dragging = false
                        val speed = tracker.calculateVelocity().x
                        val outwards = if (dragged == Side.Sidebar) speed else -speed
                        val fast = abs(speed) > with(density) { 280.dp.toPx() }
                        val opens = if (fast) outwards > 0 else abs(progress.value) > 0.5f
                        setOpen(if (opens) dragged else null, speed / width)
                    }
                },
        ) {
            if (progress.value > 0) {
                Box(Modifier.width(sidebarWidth).fillMaxHeight()) { sidebar() }
            }
            if (panelShown) {
                Box(Modifier.width(panelWidth).fillMaxHeight().align(Alignment.TopEnd)) { panel() }
            }
            val cardOffset = progress.value * (if (progress.value < 0) panelPx else sidebarPx)
            Box(
                Modifier
                    .fillMaxSize()
                    .offset { IntOffset(cardOffset.roundToInt(), 0) }
                    .then(if (aside > 0) Modifier.shadow(ShadowSize.Lg, RectangleShape, Opacity.ShadowStronger) else Modifier)
            ) {
                Box(Modifier.fillMaxSize().clipToBounds().background(Theme.colors.background)) {
                    content()
                    if (progress.value != 0f) {
                        // Over the card while a side shows: it dims the thread and takes the tap that closes.
                        Box(
                            Modifier
                                .fillMaxSize()
                                .alpha(0.55f * aside)
                                .background(Theme.colors.background)
                                .clickable(remember { MutableInteractionSource() }, indication = null) { setOpen(null) },
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

private fun androidx.compose.ui.input.pointer.PointerInputChange.positionChange() = position - previousPosition
