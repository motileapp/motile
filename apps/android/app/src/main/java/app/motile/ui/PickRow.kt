package app.motile.ui

import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import app.motile.platform.scaled
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.Theme
import app.motile.theme.opacity

/**
 * A checkbox (`multiple`) or a radio as a row: its `PickMark`, then its title with the `detail`
 * under it. A `struck` title is crossed out and muted. The whole row is one press, which calls
 * `onPress`.
 */
@Composable
fun PickRow(
    title: String,
    picked: Boolean,
    multiple: Boolean,
    onPress: () -> Unit,
    modifier: Modifier = Modifier,
    detail: String? = null,
    struck: Boolean = false,
    size: ControlSize = ControlSize.Regular,
    disabled: Boolean = false,
) {
    val palette = LocalPalette.current
    val enabled = LocalEnabled.current && !disabled
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val press = if (multiple) {
        Modifier.toggleable(picked, interaction, indication = null, enabled = enabled, role = Role.Checkbox) { onPress() }
    } else {
        Modifier.selectable(picked, interaction, indication = null, enabled = enabled, role = Role.RadioButton, onClick = onPress)
    }
    Row(
        modifier.fillMaxWidth().heightIn(min = size.height).opacity(Opacity.Disabled, on = !enabled).then(press),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val markRadius = if (multiple) Radius.xs else (scaled(16f) / 2).dp
        PickMark(picked, multiple, Modifier.focusRing(focused, markRadius))
        Column(Modifier.weight(1f)) {
            val titleStyle = TextStyle(fontSize = Theme.proseSize, textDecoration = if (struck) TextDecoration.LineThrough else null).lined(24f)
            Text(title, style = titleStyle, color = if (struck) palette.mutedForeground else palette.foreground)
            if (detail != null) Text(detail, Modifier.padding(top = 2.dp), style = Theme.ui(13f).lined(20f), color = palette.mutedForeground)
        }
    }
}
