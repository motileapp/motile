package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Surface

object UnderlineTabs {
    val underline = 2.dp
    val gap = 4.dp
    /** Between a tab's words and its sides. */
    val padding = 16.dp
    /** Between a tab's light and the line under it. */
    val lightGap = 4.dp
}

/**
 * Tabs as words on a line across the whole width, the one on underlined. The tabs touch, so no
 * press falls between them; their lights and lines are drawn `gap` apart, and one lights as a
 * ghost button does above the line.
 */
@Composable
fun <T> UnderlineTabs(
    options: List<Pair<String, T>>,
    selection: T,
    onSelect: (T) -> Unit,
    modifier: Modifier = Modifier,
    size: ControlSize = ControlSize.Regular,
) {
    val palette = LocalPalette.current
    Box(modifier.fillMaxWidth()) {
        Box(Modifier.align(Alignment.BottomStart).fillMaxWidth().height(1.dp).background(palette.border))
        Row {
            options.forEachIndexed { index, (title, value) ->
                val margin = PaddingValues(
                    start = if (index == 0) 0.dp else UnderlineTabs.gap / 2,
                    end = if (index == options.lastIndex) 0.dp else UnderlineTabs.gap / 2,
                )
                UnderlineTab(title, value == selection, size, margin) { onSelect(value) }
            }
        }
    }
}

/** The tab that is on has its words in the foreground and the line under it; another lights while a pointer is over it or a press holds it. */
@Composable
private fun UnderlineTab(title: String, selected: Boolean, size: ControlSize, margin: PaddingValues, onClick: () -> Unit) {
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    val enabled = LocalEnabled.current
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val hovering by interaction.collectIsHoveredAsState()
    val lit = enabled && (pressed || hovering)
    val light = if (lit) surface.color(Surface.Layer.Control, palette) else Color.Transparent
    Column(
        Modifier.width(IntrinsicSize.Max).arrowPointer().pressesAtOnce(interaction, enabled)
            .clickable(interactionSource = interaction, indication = null, enabled = enabled, role = Role.Tab, onClick = onClick)
            .padding(margin),
        verticalArrangement = Arrangement.spacedBy(UnderlineTabs.lightGap),
    ) {
        Box(
            Modifier.height(size.height).background(light, RoundedCornerShape(size.radius)).padding(horizontal = UnderlineTabs.padding),
            contentAlignment = Alignment.Center,
        ) {
            Text(title, style = size.font, color = if (selected || lit) palette.foreground else palette.mutedForeground, maxLines = 1)
        }
        Box(Modifier.fillMaxWidth().height(UnderlineTabs.underline).background(if (selected) palette.foreground else Color.Transparent, CircleShape))
    }
}
