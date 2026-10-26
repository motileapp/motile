package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.IconSize
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.theme.symbolSide
import app.motile.theme.wash

/**
 * A word or two that says what something is: a state, a label, a name. With a `tone` it is in that
 * colour on a wash of it; without, it is bordered. With a `dot` the colour is the dot's and the
 * words stay plain. Its symbol is a `Symbol`, or a Lucide `glyph` by its character.
 */
object Chip {
    val height = scaled(20f).dp
}

@Composable
fun Chip(
    title: String,
    modifier: Modifier = Modifier,
    icon: Symbol? = null,
    glyph: String? = null,
    dot: Color? = null,
    tone: Color? = null,
    monospaced: Boolean = false,
) {
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    val fill = tone?.wash() ?: surface.color(Surface.Layer.Control)
    val bordered = if (tone == null) Modifier.border(1.dp, surface.border, CircleShape) else Modifier
    val foreground = if (dot == null) tone ?: palette.foreground else palette.foreground
    CompositionLocalProvider(LocalForeground provides foreground) {
        Row(
            modifier.height(Chip.height).background(fill, CircleShape).then(bordered).padding(horizontal = 7.dp),
            horizontalArrangement = Arrangement.spacedBy(5.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (dot != null) Box(Modifier.size(7.dp).background(dot, CircleShape))
            if (icon != null) Icon(icon, IconSize.Sm)
            if (glyph != null) Glyph(glyph, symbolSide(IconSize.Sm))
            Text(title, style = if (monospaced) Theme.uiMono(11.5f, FontWeight.Medium) else Theme.ui(11.5f, FontWeight.Medium), maxLines = 1)
        }
    }
}
