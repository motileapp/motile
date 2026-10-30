package app.motile.composer

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.theme.ControlSize
import app.motile.theme.Radius
import app.motile.theme.ShadowSize
import app.motile.theme.Theme
import app.motile.theme.shadow
import app.motile.ui.floatingSurface

/** The surface of the composer and its strips, the one the buttons that float over the client lie on. `shadow` is cast by the composer's box alone. */
@Composable
fun Modifier.composerSurface(shape: Shape, shadow: ShadowSize? = null): Modifier {
    val shaded = if (shadow == null) this else shadow(shadow, shape)
    return shaded.floatingSurface(shape)
}

/**
 * A strip against the composer's top or bottom, on the composer's surface: rounded on its outer
 * corners and open where it meets the composer. It stands in from the composer's sides by the
 * composer's corner radius, so it meets the composer's straight edge. Under the skill menu, whose
 * corners are the rounded ones, it is square.
 */
object ComposerStrip {
    enum class Edge { Top, Bottom }

    val radius = Radius.lg
    /** The room between a control and the strip's visible edges. */
    val inset = 3.dp
    val height = ControlSize.Regular.height + inset * 2
    /**
     * The room around a control in a strip, which is the control's to press. One that ends the strip
     * stands as far from its side as from its top and bottom.
     */
    val margin = PaddingValues(start = 4.dp, top = inset, end = inset, bottom = inset)
    /** How far the strip's open edge goes under the composer, which covers its outline there. */
    val overlap = 1.dp
}

val LocalUnderSkillMenu = compositionLocalOf { false }

/** Makes the view a strip of the composer on `edge`; a `height` of null makes it as tall as what is in it. */
@Composable
fun Modifier.composerStrip(edge: ComposerStrip.Edge, height: Dp? = ComposerStrip.height): Modifier {
    val top = edge == ComposerStrip.Edge.Top
    val radius = if (LocalUnderSkillMenu.current) 0.dp else ComposerStrip.radius
    val shape = if (top) RoundedCornerShape(topStart = radius, topEnd = radius) else RoundedCornerShape(bottomStart = radius, bottomEnd = radius)
    val sized = if (height == null) Modifier else Modifier.height(height + ComposerStrip.overlap)
    val hidden = if (top) Modifier.padding(bottom = ComposerStrip.overlap) else Modifier.padding(top = ComposerStrip.overlap)
    return this
        .padding(horizontal = ComposerView.radius)
        .reaching(ComposerStrip.overlap, below = top)
        .composerSurface(shape)
        .then(sized)
        .then(hidden)
}

/** Takes `length` less room than it draws in, the rest lying under the view below it or above it. */
private fun Modifier.reaching(length: Dp, below: Boolean): Modifier = layout { measurable, constraints ->
    val reach = length.roundToPx()
    val placeable = measurable.measure(constraints.copy(minHeight = 0, maxHeight = if (constraints.hasBoundedHeight) constraints.maxHeight + reach else Constraints.Infinity))
    val height = (placeable.height - reach).coerceAtLeast(0)
    layout(placeable.width, height) { placeable.place(0, if (below) 0 else -reach) }
}

/** Draws past its room by `horizontal` and `vertical` on each side, as SwiftUI's negative padding does. */
fun Modifier.overhang(horizontal: Dp = 0.dp, vertical: Dp = 0.dp): Modifier = layout { measurable, constraints ->
    val x = horizontal.roundToPx()
    val y = vertical.roundToPx()
    fun widen(max: Int, by: Int) = if (max == Constraints.Infinity) max else (max + by * 2).coerceAtLeast(0)
    val placeable = measurable.measure(
        Constraints(
            minWidth = (constraints.minWidth + x * 2).coerceAtLeast(0), maxWidth = widen(constraints.maxWidth, x),
            minHeight = (constraints.minHeight + y * 2).coerceAtLeast(0), maxHeight = widen(constraints.maxHeight, y),
        ),
    )
    layout((placeable.width - x * 2).coerceAtLeast(0), (placeable.height - y * 2).coerceAtLeast(0)) { placeable.place(-x, -y) }
}

/** The line between the parts of the composer's rows. It takes no room, so the controls on either side reach its middle. */
@Composable
fun ComposerDivider(modifier: Modifier = Modifier) {
    Box(modifier.width(0.dp).height(14.dp).overhang(horizontal = 0.5.dp)) {
        Box(Modifier.width(1.dp).fillMaxHeight().background(Theme.colors.borderComposer))
    }
}
