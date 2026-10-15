package app.motile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.unit.dp
import app.motile.platform.Symbol
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.symbolSide

/**
 * One of a short list a press on a control offers. The list is built only then and lives only while
 * it is open: a sheet from the bottom, as iOS's action sheet.
 */
data class Choice(val title: String, val symbol: Symbol? = null, val chosen: Boolean = false, val run: () -> Unit)

/** Shows `offered` while it isn't empty; picking one, or closing the sheet, calls `onDismiss`. */
@Composable
fun Choices(offered: List<Choice>, onDismiss: () -> Unit) {
    Modal(shown = offered.isNotEmpty(), onDismiss = onDismiss) {
        val symbols = offered.any { it.symbol != null }
        Column(Modifier.verticalScroll(rememberScrollState()).navigationBarsPadding().padding(horizontal = 8.dp, vertical = 8.dp)) {
            for (choice in offered) {
                HighlightButton(
                    onClick = {
                        onDismiss()
                        choice.run()
                    },
                    modifier = Modifier.fillMaxWidth(),
                    radius = app.motile.theme.Radius.md,
                ) {
                    CompositionLocalProvider(LocalForeground provides LocalPalette.current.foreground) {
                        Row(
                            Modifier.heightIn(min = pressable(32f)).padding(horizontal = 12.dp),
                            horizontalArrangement = Arrangement.spacedBy(12.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            if (symbols) {
                                Box(Modifier.size(symbolSide(15f)), contentAlignment = Alignment.Center) {
                                    if (choice.symbol != null) Icon(choice.symbol, 15f)
                                }
                            }
                            Text(choice.title, Modifier.weight(1f), size = 15f, maxLines = 2)
                            Icon(Symbol.Check, 14f, Modifier.alpha(if (choice.chosen) 1f else 0f), color = LocalPalette.current.primary)
                        }
                    }
                }
            }
        }
    }
}
