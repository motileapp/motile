package app.motile.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.theme.LocalPalette

/** A ring 2 wide in the ring colour, 2 outside the view's edges of `radius`, while a keyboard has `focused` it. */
@Composable
fun Modifier.focusRing(focused: Boolean, radius: Dp): Modifier {
    if (!focused) return this
    val color = LocalPalette.current.ring
    return drawBehind {
        val width = 2.dp.toPx()
        val outset = 2.dp.toPx() + width / 2
        drawRoundRect(
            color,
            topLeft = Offset(-outset, -outset),
            size = Size(size.width + outset * 2, size.height + outset * 2),
            cornerRadius = CornerRadius(radius.toPx() + outset),
            style = Stroke(width),
        )
    }
}
