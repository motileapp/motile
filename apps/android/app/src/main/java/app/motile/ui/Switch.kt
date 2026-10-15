package app.motile.ui

import androidx.compose.material3.SwitchDefaults
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Surface
import app.motile.theme.at
import androidx.compose.material3.Switch as SystemSwitch

/** On or off: the system's switch, in the primary colour while it is on. */
@Composable
fun Switch(isOn: Boolean, onChange: (Boolean) -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    SystemSwitch(
        checked = isOn,
        onCheckedChange = onChange,
        modifier = modifier,
        enabled = enabled && LocalEnabled.current,
        colors = SwitchDefaults.colors(
            checkedThumbColor = palette.primaryForeground,
            checkedTrackColor = palette.primary,
            checkedBorderColor = Color.Transparent,
            uncheckedThumbColor = palette.mutedForeground,
            uncheckedTrackColor = surface.color(Surface.Layer.Control),
            uncheckedBorderColor = surface.border,
            disabledCheckedThumbColor = palette.primaryForeground,
            disabledCheckedTrackColor = palette.primary.at(Opacity.Disabled),
            disabledUncheckedThumbColor = palette.mutedStrongerForeground,
            disabledUncheckedTrackColor = surface.color(Surface.Layer.Control),
            disabledUncheckedBorderColor = surface.border,
        ),
    )
}
