package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.platform.Platform
import app.motile.platform.scaled
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.LocalRow
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.theme.value
import app.motile.theme.LocalDark
import kotlin.math.max

/**
 * Lights up what is pressed (or under a pointer), what is selected and what is `lit`. What is
 * selected is lit no further. The light is drawn `inset` from the view's edges: that margin looks
 * empty but is the view's, so neighbours leave no gap to miss. What is `faded` is in the muted
 * colour until it lights up. A view that follows the press itself passes `hovered`. What is
 * `small`, like a tab, lights as a control does, not as a row. A `fill` lights it in that colour
 * instead of the surface's accent.
 */
@Composable
fun HoverHighlight(
    modifier: Modifier = Modifier,
    radius: Dp = Radius.md,
    selected: Boolean = false,
    lit: Boolean = false,
    inset: PaddingValues = PaddingValues(0.dp),
    faded: Boolean = false,
    hovered: Boolean? = null,
    small: Boolean = false,
    fill: Color? = null,
    contentAlignment: Alignment = Alignment.TopStart,
    content: @Composable BoxScope.() -> Unit,
) {
    val interaction = remember { MutableInteractionSource() }
    val tracked by interaction.collectIsHoveredAsState()
    val tracking = if (hovered == null) Modifier.hoverable(interaction) else Modifier
    Lighted(modifier.then(tracking), radius, selected, lit, inset, faded, hovered ?: tracked, small, fill, contentAlignment, content)
}

/**
 * A button that looks as `HoverHighlight` lights: the same light under a finger and a pointer. One
 * that is selected, or `lit` as the one the arrow keys are on, has it without either. All of it
 * takes the press, so wherever it lights it can be pressed.
 */
@Composable
fun HighlightButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    radius: Dp = Radius.md,
    selected: Boolean = false,
    lit: Boolean = false,
    inset: PaddingValues = PaddingValues(0.dp),
    faded: Boolean = false,
    hovered: Boolean? = null,
    small: Boolean = false,
    fill: Color? = null,
    onLongClick: (() -> Unit)? = null,
    contentAlignment: Alignment = Alignment.TopStart,
    content: @Composable BoxScope.() -> Unit,
) {
    val usable = enabled && LocalEnabled.current
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val tracked by interaction.collectIsHoveredAsState()
    val pressing = modifier.combinedClickable(
        interactionSource = interaction,
        indication = null,
        enabled = usable,
        role = Role.Button,
        onLongClick = onLongClick,
        onClick = onClick,
    )
    CompositionLocalProvider(LocalEnabled provides usable) {
        Lighted(pressing, radius, selected, lit || pressed, inset, faded, hovered ?: tracked, small, fill, contentAlignment, content)
    }
}

@Composable
private fun Lighted(
    modifier: Modifier,
    radius: Dp,
    selected: Boolean,
    lit: Boolean,
    inset: PaddingValues,
    faded: Boolean,
    hovering: Boolean,
    small: Boolean,
    fill: Color?,
    contentAlignment: Alignment,
    content: @Composable BoxScope.() -> Unit,
) {
    val enabled = LocalEnabled.current
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    val light: Surface.Layer? = when {
        selected -> if (small) Surface.Layer.ControlLit else Surface.Layer.RowSelected
        enabled && (hovering || lit) -> if (small) Surface.Layer.Control else Surface.Layer.Row
        else -> null
    }
    val text = when {
        !enabled -> palette.mutedStrongerForeground
        selected || hovering || lit -> palette.foreground
        else -> palette.mutedForeground
    }
    Box(modifier, contentAlignment = contentAlignment) {
        if (light != null) {
            Box(Modifier.matchParentSize().padding(inset).background(fill ?: surface.color(light, palette), RoundedCornerShape(radius)))
        }
        val foreground = if (faded) text else LocalForeground.current
        CompositionLocalProvider(LocalRow provides light, LocalForeground provides foreground) { content() }
    }
}

/** A button whose own look says what it does, like a picture: pressed, it is at opacity lit. */
@Composable
fun DimButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    onLongClick: (() -> Unit)? = null,
    contentAlignment: Alignment = Alignment.TopStart,
    content: @Composable BoxScope.() -> Unit,
) {
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val lit = Opacity.Lit.value(LocalDark.current)
    Box(
        modifier
            .combinedClickable(interactionSource = interaction, indication = null, enabled = enabled && LocalEnabled.current, onLongClick = onLongClick, onClick = onClick)
            .alpha(if (pressed) lit else 1f),
        contentAlignment = contentAlignment,
        content = content,
    )
}

/** As tall as it is on the Mac, and where fingers press it at least as tall as a finger needs. */
fun pressable(height: Float): Dp = max(scaled(height), Platform.minimumPress).dp
