package app.motile.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import app.motile.platform.Symbol
import app.motile.theme.ControlSize

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
    margin: PaddingValues = PaddingValues(0.dp),
) {
    var open by remember { mutableStateOf(false) }
    Box(modifier) {
        ActionButton(title, icon, help = help, variant = variant, size = size, selected = open, opens = true, tint = tint, margin = margin) { open = true }
        Dropdown(open, onDismiss = { open = false }) {
            DropdownCard(Modifier.width(IntrinsicSize.Max).widthIn(min = 160.dp)) {
                Column(Modifier.verticalScroll(rememberScrollState()).padding(DropdownList.padding)) {
                    for (check in checks) CheckRow(check)
                }
            }
        }
    }
}

@Composable
private fun CheckRow(check: Check) {
    DropdownRow(onClick = check.toggle) {
        Text(check.title, Modifier.weight(1f), size = DropdownList.textSize, maxLines = 1)
        Spacer(Modifier.width(12.dp))
        Icon(Symbol.Check, DropdownList.check, Modifier.alpha(if (check.checked) 1f else 0f))
    }
}
