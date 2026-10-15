package app.motile.ui

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.motile.platform.Symbol
import app.motile.theme.ControlSize

/** A button in a toolbar. The buttons touch: the space seen between them is theirs. */
object ToolbarButton {
    val margin = 2.dp
    val width = ControlSize.Regular.height + margin * 2
}

@Composable
fun ToolbarButton(symbol: Symbol, help: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    ActionButton(icon = symbol, help = help, modifier = modifier, margin = PaddingValues(ToolbarButton.margin), onClick = onClick)
}
