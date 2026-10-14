package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Surface
import app.motile.theme.opacity

/**
 * A short row of choices of which one is on, in a track on the background with the border around
 * it. The choices touch each other and the track's edge, so no press falls between them; their
 * light is drawn `inset` from their edges, as the sidebar's rows are.
 */
object Segmented {
    /** How far a choice's light stands from the track's border, and from the next one's. */
    val inset = 4.dp
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
                .background(palette.background, shape)
                .border(Segmented.border, palette.border, shape),
        ) {
            options.forEachIndexed { index, (title, value) ->
                val margin = PaddingValues(
                    start = if (index == 0) edge else half,
                    top = edge,
                    end = if (index == options.lastIndex) edge else half,
                    bottom = edge,
                )
                HighlightButton(
                    onClick = { onSelect(value) },
                    modifier = if (fills) Modifier.weight(1f) else Modifier,
                    radius = size.radius,
                    selected = value == selection,
                    inset = margin,
                    faded = true,
                    small = true,
                    contentAlignment = Alignment.Center,
                ) {
                    Box(
                        Modifier.padding(margin).height(size.height).padding(horizontal = size.padding),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(title, style = size.font, maxLines = 1)
                    }
                }
            }
        }
    }
}
