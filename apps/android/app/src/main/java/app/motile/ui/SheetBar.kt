package app.motile.ui

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.platform.Symbol
import app.motile.theme.LocalPalette
import app.motile.theme.IconSize
import app.motile.theme.TextSize
import app.motile.theme.symbolSide

/** How tall a sheet's bar is, as Android's top app bar. */
private val barHeight = 56.dp
/** Where the title starts without the way back: in line with the edge of what the sheet shows. */
private val titleStart = 16.dp
/** How far a bar button's edge stands outside the line its symbol is drawn on: Lucide leaves a twelfth of its square empty. */
private val buttonOutset: Dp
    get() = symbolSide(IconSize.Lg).let { (GlassButton.side - it) / 2 + it / 12 }
/** Where a bar button starts or ends, so that its symbol stands on the title's line. */
private val buttonEdge: Dp
    get() = (titleStart - buttonOutset).coerceAtLeast(0.dp)

/**
 * A sheet's bar, as Android's: the way back when there is one, the title at the start and the
 * actions at the end, on no glass. A sheet closes by its handle, the back gesture or a press
 * outside it, so the bar has no button for that.
 */
@Composable
fun SheetBar(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    back: (() -> Unit)? = null,
    trailing: (@Composable RowScope.() -> Unit)? = null,
) {
    Row(
        modifier.fillMaxWidth().height(barHeight).padding(start = if (back == null) titleStart else buttonEdge, end = buttonEdge),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        back?.let { GlassButton(Symbol.ArrowLeft, "Back", onClick = it, bare = true) }
        Column(Modifier.weight(1f).padding(start = if (back == null) 0.dp else 8.dp)) {
            Text(title, Modifier.semantics { heading() }, size = TextSize.Xl, weight = FontWeight.SemiBold, maxLines = 1)
            subtitle?.let { Text(it, size = TextSize.Xs, color = LocalPalette.current.mutedForeground, maxLines = 1) }
        }
        trailing?.let { Row(verticalAlignment = Alignment.CenterVertically, content = it) }
    }
}
