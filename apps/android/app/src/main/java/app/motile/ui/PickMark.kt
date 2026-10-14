package app.motile.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.LocalPalette
import app.motile.theme.Radius

/**
 * Whether a row of a list is picked: a radio's ring where one of the list is picked, a box where any
 * are, filled in the primary colour once it is.
 */
@Composable
fun PickMark(picked: Boolean, multiple: Boolean, modifier: Modifier = Modifier) {
    val palette = LocalPalette.current
    val side = scaled(16f).dp
    val shape = if (multiple) RoundedCornerShape(Radius.xs) else CircleShape
    val fill by animateColorAsState(if (picked) palette.primary else Color.Transparent, tween(120), label = "fill")
    val edge by animateColorAsState(if (picked) palette.primary else palette.mutedStrongestForeground, tween(120), label = "edge")
    Box(modifier.size(side).background(fill, shape).border(1.5.dp, edge, shape), contentAlignment = Alignment.Center) {
        if (picked && multiple) {
            Icon(Symbol.Check, 10f, color = palette.primaryForeground)
        } else if (picked) {
            Box(Modifier.size(side * 0.375f).background(palette.primaryForeground, CircleShape))
        }
    }
}
