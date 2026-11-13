package app.motile.usage

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Dp
import app.motile.theme.LocalSurface
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.ui.Text
import app.motile.ui.color
import app.motile.ui.pulsing

/** A bar where a line of `style` will be, in its line and as tall as its capitals, fading in and out. */
@Composable
fun PlaceholderBar(width: Dp, style: TextStyle, modifier: Modifier = Modifier) {
    val density = LocalDensity.current
    val cap = remember(style.fontSize, density) {
        val bounds = android.graphics.Rect()
        android.graphics.Paint().apply { textSize = with(density) { style.fontSize.toPx() } }.getTextBounds("H", 0, 1, bounds)
        with(density) { bounds.height().toDp() }
    }
    Box(modifier.width(width), contentAlignment = Alignment.CenterStart) {
        Text(" ", style = style, color = Color.Transparent, maxLines = 1)
        Box(Modifier.fillMaxWidth().height(cap).pulsing().background(LocalSurface.current.color(Surface.Layer.Control), RoundedCornerShape(Radius.xs)))
    }
}
