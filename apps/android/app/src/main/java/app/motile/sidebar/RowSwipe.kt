package app.motile.sidebar

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.horizontalDrag
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import app.motile.platform.Symbol
import app.motile.theme.Theme
import app.motile.ui.Icon
import kotlinx.coroutines.launch
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

private val settleSpring = spring<Float>(dampingRatio = 0.86f, stiffness = 470f)

/**
 * Lets a swipe to the right slide a sidebar row aside and show a round button behind it. A short
 * swipe leaves the button there to tap, a long one does what the button does. The row leaves with a
 * long swipe unless the button won't take it away (`leaves`). From rest only a swipe to the right
 * moves it; a swipe to the left is left to what lies under the sidebar.
 */
@Composable
fun RowSwipe(
    symbol: Symbol,
    label: String,
    tint: Color,
    size: Dp,
    isOpen: Boolean,
    onOpenChange: (Boolean) -> Unit,
    action: () -> Unit,
    modifier: Modifier = Modifier,
    leaves: Boolean = true,
    content: @Composable () -> Unit,
) {
    val density = LocalDensity.current
    val haptics = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    val offset = remember { Animatable(0f) }
    var width by remember { mutableFloatStateOf(0f) }
    var dragging by remember { mutableStateOf(false) }
    val currentAction by rememberUpdatedState(action)
    val currentOpenChange by rememberUpdatedState(onOpenChange)
    val currentLeaves by rememberUpdatedState(leaves)

    /** How far the row rests aside while its button shows. */
    val openWidth = with(density) { (size + 20.dp).toPx() }
    /** From here on, letting go does what the button does. */
    val shortest = openWidth + with(density) { 44.dp.toPx() }
    fun fullSwipe() = max(shortest, width * 0.3f)
    val armed = offset.value >= fullSwipe()

    fun settle(open: Boolean) {
        currentOpenChange(open)
        scope.launch { offset.animateTo(if (open) openWidth else 0f, settleSpring) }
    }

    /** The row leaves to the right, and then what the button does happens. */
    fun commit() {
        if (!currentLeaves) {
            settle(false)
            currentAction()
            return
        }
        scope.launch {
            offset.animateTo(max(width, offset.value), tween(180, easing = FastOutSlowInEasing))
            currentAction()
            currentOpenChange(false)
            offset.snapTo(0f)
        }
    }

    LaunchedEffect(armed) { if (armed) haptics.performHapticFeedback(HapticFeedbackType.GestureThresholdActivate) }
    LaunchedEffect(isOpen) {
        if (isOpen || dragging || offset.value <= 0f) return@LaunchedEffect
        offset.animateTo(0f, settleSpring)
    }

    Box(
        modifier
            .clipToBounds()
            .onSizeChanged { width = it.width.toFloat() }
            .semantics { customActions = listOf(CustomAccessibilityAction(label) { currentAction(); true }) }
            .pointerInput(openWidth) {
                val speedLimit = 300.dp.toPx()
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    val opened = offset.value > 0f
                    var moved = Offset.Zero
                    var taken: PointerInputChange? = null
                    withTimeoutOrNull(viewConfiguration.longPressTimeoutMillis) {
                        while (true) {
                            val change = awaitPointerEvent().changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed || change.isConsumed) break
                            moved += change.position - change.previousPosition
                            if (moved.getDistance() <= viewConfiguration.touchSlop) continue
                            val sideways = abs(moved.x) > abs(moved.y) * 1.3f
                            if (sideways && (opened || moved.x > 0)) taken = change
                            break
                        }
                    }
                    val start = taken ?: return@awaitEachGesture
                    start.consume()
                    val atStart = offset.value
                    dragging = true
                    currentOpenChange(true)
                    val tracker = VelocityTracker()
                    tracker.addPosition(start.uptimeMillis, start.position)
                    var travelled = moved.x
                    scope.launch { offset.snapTo(max(0f, atStart + travelled)) }
                    horizontalDrag(down.id) { change ->
                        travelled += change.position.x - change.previousPosition.x
                        tracker.addPosition(change.uptimeMillis, change.position)
                        change.consume()
                        scope.launch { offset.snapTo(max(0f, atStart + travelled)) }
                    }
                    dragging = false
                    val end = max(0f, atStart + travelled)
                    val speed = tracker.calculateVelocity().x
                    if (end >= fullSwipe()) return@awaitEachGesture commit()
                    settle(if (abs(speed) > speedLimit) speed > 0 else end > openWidth / 2)
                }
            },
    ) {
        if (offset.value > 0f) SwipeButton(symbol, label, tint, size, offset.value, openWidth, armed, Modifier.matchParentSize()) { commit() }
        Box(Modifier.offset { IntOffset(offset.value.roundToInt(), 0) }) {
            content()
            if (offset.value > 0f) {
                Box(Modifier.matchParentSize().clickable(interactionSource = null, indication = null) { settle(false) })
            }
        }
    }
}

/** The button comes in as the row leaves, and past its place stretches after the row. */
@Composable
private fun SwipeButton(
    symbol: Symbol,
    label: String,
    tint: Color,
    size: Dp,
    offset: Float,
    openWidth: Float,
    armed: Boolean,
    modifier: Modifier,
    onClick: () -> Unit,
) {
    val density = LocalDensity.current
    val stretch = with(density) { max(0f, offset - openWidth).toDp() }
    val entry = min(1f, max(0f, (offset - 8f) / (openWidth * 0.72f - 8f)))
    val shift by animateDpAsState(if (armed) stretch / 2 else 0.dp, tween(150), label = "shift")
    Box(modifier, contentAlignment = Alignment.CenterStart) {
        Box(
            Modifier
                .fillMaxHeight()
                .width(sidebarEdge + size + stretch)
                .clickable(interactionSource = null, indication = null, onClick = onClick)
                .semantics { contentDescription = label }
                .graphicsLayer {
                    val scale = 0.78f + 0.22f * entry
                    scaleX = scale
                    scaleY = scale
                    alpha = entry
                },
            contentAlignment = Alignment.CenterStart,
        ) {
            Box(
                Modifier.padding(start = sidebarEdge).size(width = size + stretch, height = size).background(tint, CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                Icon(symbol, size.value * 0.37f, Modifier.offset(x = shift), color = Theme.colors.background)
            }
        }
    }
}
