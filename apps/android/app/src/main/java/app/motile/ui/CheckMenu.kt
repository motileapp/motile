package app.motile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.Radius

/** One of the things a `CheckMenu` turns on and off. */
data class Check(val id: String, val title: String, val checked: Boolean, val toggle: () -> Unit)

/** A menu of things to turn on and off. It stays open while they are picked, until a press elsewhere or back closes it. */
@Composable
fun CheckMenu(
    title: String,
    help: String,
    checks: List<Check>,
    modifier: Modifier = Modifier,
    icon: Symbol? = null,
    variant: ButtonVariant = ButtonVariant.Ghost,
    size: ControlSize = ControlSize.Regular,
    tint: Color? = null,
) {
    var open by remember { mutableStateOf(false) }
    Box(modifier) {
        ActionButton(title, icon, help = help, variant = variant, size = size, selected = open, opens = true, tint = tint) { open = true }
        Dropdown(open, onDismiss = { open = false }) {
            DropdownCard(Modifier.width(IntrinsicSize.Max).widthIn(min = 160.dp)) {
                Column(Modifier.verticalScroll(rememberScrollState()).padding(4.dp)) {
                    for (check in checks) CheckRow(check)
                }
            }
        }
    }
}

@Composable
private fun CheckRow(check: Check) {
    HighlightButton(onClick = check.toggle, modifier = Modifier.fillMaxWidth(), radius = Radius.sm) {
        CompositionLocalProvider(LocalForeground provides LocalPalette.current.foreground) {
            Row(
                Modifier.heightIn(min = pressable(28f)).padding(horizontal = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Symbol.Check, 13f, Modifier.alpha(if (check.checked) 1f else 0f))
                Text(check.title, Modifier.weight(1f).padding(end = 12.dp), size = 13f, maxLines = 1)
            }
        }
    }
}
