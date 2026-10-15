package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.theme.LocalPalette
import app.motile.theme.LocalRow
import app.motile.theme.LocalSurface
import app.motile.theme.Radius
import app.motile.theme.Surface

/**
 * A card: a box of its own with its edge around it, like a group of settings, a table or a note.
 * What is inside lies on the card.
 */
@Composable
fun Card(modifier: Modifier = Modifier, radius: Dp = Radius.lg, content: @Composable ColumnScope.() -> Unit) {
    val palette = LocalPalette.current
    val shape = RoundedCornerShape(radius)
    Column(modifier.clip(shape).background(palette.card, shape).border(1.dp, palette.borderCard, shape)) {
        CompositionLocalProvider(LocalSurface provides Surface.Card, LocalRow provides null) { content() }
    }
}
