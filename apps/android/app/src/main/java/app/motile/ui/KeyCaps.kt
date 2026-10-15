package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.theme.LocalSurface
import app.motile.theme.Radius
import app.motile.theme.Surface

/** Keys to press, each on a cap of its own, or all on one when `joined`. */
object KeyCaps {
    val height = 20.dp
}

@Composable
fun KeyCaps(caps: List<String>, modifier: Modifier = Modifier, joined: Boolean = false) {
    val shown = if (joined) listOf(caps.joinToString("+")) else caps
    val fill = LocalSurface.current.color(Surface.Layer.Control)
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(5.dp), verticalAlignment = Alignment.CenterVertically) {
        for (cap in shown) {
            Box(
                Modifier
                    .defaultMinSize(minWidth = 22.dp, minHeight = KeyCaps.height)
                    .background(fill, RoundedCornerShape(Radius.xs))
                    .padding(horizontal = 6.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(cap, size = 11f, weight = FontWeight.Medium, maxLines = 1)
            }
        }
    }
}
