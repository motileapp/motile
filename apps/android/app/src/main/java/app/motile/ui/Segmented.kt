package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.ShadowSize
import app.motile.theme.Surface
import app.motile.theme.opacity
import app.motile.theme.shadow

/**
 * A short row of choices of which one is on, in a track with the border around it. The choices
 * touch each other and the track's edge, so no press falls between them; their light is drawn
 * `inset` from their edges, as the sidebar's rows are.
 */
object Segmented {
    /** How far a choice's light stands from the track's border, and from the next one's. */
    val inset = 2.dp
    val border = 1.dp
}

/** With `fills`, the choices share the whole width they are given. */
@Composable
fun <T> Segmented(
    options: List<Pair<String, T>>,
    selection: T,
    onSelect: (T) -> Unit,
    modifier: Modifier = Modifier,
    size: ControlSize = ControlSize.Regular,
    fills: Boolean = false,
) {
    val palette = LocalPalette.current
    val enabled = LocalEnabled.current
    val shape = RoundedCornerShape(size.radius + Segmented.inset + Segmented.border)
    val half = Segmented.inset / 2
    val edge = Segmented.inset + Segmented.border
    CompositionLocalProvider(LocalSurface provides Surface.Background) {
        Row(
            modifier
                .then(if (fills) Modifier.fillMaxWidth() else Modifier)
                .opacity(Opacity.Disabled, on = !enabled)
                .background(palette.segment, shape)
                .border(Segmented.border, palette.border, shape),
        ) {
            options.forEachIndexed { index, (title, value) ->
                val margin = PaddingValues(
                    start = if (index == 0) edge else half,
                    top = edge,
                    end = if (index == options.lastIndex) edge else half,
                    bottom = edge,
                )
                Segment(title, value == selection, size, margin, if (fills) Modifier.weight(1f) else Modifier) { onSelect(value) }
            }
        }
    }
}

/**
 * The chosen segment stands on the track in its own colour with a small shadow; another lies on the
 * track's accent while a pointer is over it or a press holds it. Its words are in the foreground
 * then, and muted else.
 */
@Composable
private fun Segment(title: String, selected: Boolean, size: ControlSize, margin: PaddingValues, modifier: Modifier, onClick: () -> Unit) {
    val palette = LocalPalette.current
    val enabled = LocalEnabled.current
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val hovering by interaction.collectIsHoveredAsState()
    val up = enabled && (pressed || hovering)
    val shape = RoundedCornerShape(size.radius)
    val color = when {
        !enabled -> palette.mutedStrongerForeground
        selected || up -> palette.foreground
        else -> palette.mutedForeground
    }
    Box(
        modifier.clickable(interactionSource = interaction, indication = null, enabled = enabled, role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        when {
            selected -> Box(Modifier.matchParentSize().padding(margin).shadow(ShadowSize.Sm, shape).background(palette.segmentSelected, shape))
            up -> Box(Modifier.matchParentSize().padding(margin).background(palette.segmentAccent, shape))
        }
        Box(Modifier.padding(margin).height(size.height).padding(horizontal = size.padding), contentAlignment = Alignment.Center) {
            Text(title, style = size.font, color = color, maxLines = 1)
        }
    }
}
