package app.motile.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.theme.opacity
import app.motile.theme.wash

/**
 * A choice as a card: its title with the `detail` under it, in the surface's border. The `selected`
 * one is edged in the primary colour on its wash. It fills its width; a press calls `onPress`.
 */
@Composable
fun OptionCard(
    title: String,
    selected: Boolean,
    onPress: () -> Unit,
    modifier: Modifier = Modifier,
    detail: String? = null,
    disabled: Boolean = false,
) {
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    val enabled = LocalEnabled.current && !disabled
    val interaction = remember { MutableInteractionSource() }
    val hovered by interaction.collectIsHoveredAsState()
    val pressed by interaction.collectIsPressedAsState()
    val focused by interaction.collectIsFocusedAsState()
    val shape = RoundedCornerShape(Radius.lg)
    val target = when {
        selected -> palette.primary.wash()
        enabled && pressed -> surface.color(Surface.Layer.RowSelected)
        enabled && hovered -> surface.color(Surface.Layer.Row)
        else -> Color.Transparent
    }
    val fill by animateColorAsState(target, tween(120), label = "fill")
    val edge by animateColorAsState(if (selected) palette.primary else surface.border, tween(120), label = "edge")
    Column(
        modifier
            .fillMaxWidth()
            .opacity(Opacity.Disabled, on = !enabled)
            .focusRing(focused, Radius.lg)
            .background(fill, shape)
            .border(1.dp, edge, shape)
            .hoverable(interaction, enabled)
            .selectable(selected, interaction, indication = null, enabled = enabled, role = Role.RadioButton, onClick = onPress)
            .padding(horizontal = 16.dp, vertical = 14.dp),
    ) {
        Text(title, style = Theme.ui(14f, FontWeight.Medium).lined(20f), color = palette.foreground)
        if (detail != null) Text(detail, Modifier.padding(top = 2.dp), style = Theme.ui(13f).lined(20f), color = palette.mutedForeground)
    }
}
