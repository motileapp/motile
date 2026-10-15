package app.motile.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.scaleIn
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import app.motile.theme.LocalPalette
import app.motile.theme.LocalRow
import app.motile.theme.LocalSurface
import app.motile.theme.Radius
import app.motile.theme.ShadowSize
import app.motile.theme.Surface
import app.motile.theme.shadow
import kotlin.math.max

/** The room a dropdown leaves around its card for the shadow, and between its button and itself. */
private val shadowRoom = ShadowSize.Md.blur.dp
private val gap = 4.dp

/**
 * The look of what drops from a button or lies over the window: the popover's colour, a line
 * around it and a faint shadow under it. What is inside lies on the popover.
 */
@Composable
fun DropdownCard(modifier: Modifier = Modifier, radius: Dp = Radius.md, content: @Composable ColumnScope.() -> Unit) {
    val palette = LocalPalette.current
    val shape = RoundedCornerShape(radius)
    Column(
        modifier
            .shadow(ShadowSize.Md, shape)
            .clip(shape)
            .background(palette.popover, shape)
            .border(1.dp, palette.borderPopover, shape),
    ) {
        CompositionLocalProvider(LocalSurface provides Surface.Popover, LocalRow provides null) { content() }
    }
}

/**
 * Drops `content` from the view it lies in, under it, or above it where there is no room below,
 * over everything else. A press anywhere else closes it, and so does back. `at` puts it at a point
 * of the view instead, as a menu held open on a row is.
 */
@Composable
fun Dropdown(expanded: Boolean, onDismiss: () -> Unit, at: IntOffset? = null, content: @Composable () -> Unit) {
    if (!expanded) return
    val density = LocalDensity.current
    val room = with(density) { shadowRoom.roundToPx() }
    val space = with(density) { gap.roundToPx() }
    val position = remember(at, room, space) { DropdownPosition(at, room, space) }
    val shown = remember { MutableTransitionState(false).apply { targetState = true } }
    Popup(popupPositionProvider = position, onDismissRequest = onDismiss, properties = PopupProperties(focusable = true)) {
        AnimatedVisibility(
            shown,
            enter = fadeIn(tween(120)) + scaleIn(tween(120), initialScale = 0.96f, transformOrigin = TransformOrigin(0f, 0f)),
        ) {
            Column(Modifier.padding(shadowRoom)) { content() }
        }
    }
}

private class DropdownPosition(val at: IntOffset?, val room: Int, val gap: Int) : PopupPositionProvider {
    override fun calculatePosition(anchorBounds: IntRect, windowSize: IntSize, layoutDirection: LayoutDirection, popupContentSize: IntSize): IntOffset {
        val point = at?.let { IntOffset(anchorBounds.left + it.x, anchorBounds.top + it.y) }
        val left = (point?.x ?: anchorBounds.left) - room
        val x = left.coerceIn(0, max(0, windowSize.width - popupContentSize.width))
        val below = (point?.y ?: anchorBounds.bottom) + gap - room
        val above = (point?.y ?: anchorBounds.top) - gap + room - popupContentSize.height
        val y = when {
            below + popupContentSize.height <= windowSize.height -> below
            above >= 0 -> above
            else -> max(0, windowSize.height - popupContentSize.height)
        }
        return IntOffset(x, y)
    }
}
