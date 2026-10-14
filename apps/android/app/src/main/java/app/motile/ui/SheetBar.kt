package app.motile.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.motile.platform.Symbol

/** A sheet's bar, as iOS's inline navigation bar: the title in the middle, a glass button at each side. */
@Composable
fun SheetBar(
    title: String,
    modifier: Modifier = Modifier,
    leading: (@Composable () -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    Box(modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp).height(GlassButton.side)) {
        leading?.let { Box(Modifier.align(Alignment.CenterStart)) { it() } }
        Text(
            title,
            Modifier.align(Alignment.Center).padding(horizontal = GlassButton.side + 8.dp).semantics { heading() },
            size = 15f,
            weight = FontWeight.SemiBold,
            maxLines = 1,
            align = TextAlign.Center,
        )
        trailing?.let { Box(Modifier.align(Alignment.CenterEnd)) { it() } }
    }
}

/** The way back from a pushed page of a sheet. */
@Composable
fun SheetBackButton(onClick: () -> Unit) {
    GlassButton(Symbol.ChevronLeft, "Back", onClick = onClick, symbolSize = 16f)
}
