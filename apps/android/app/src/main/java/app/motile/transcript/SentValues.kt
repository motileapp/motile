package app.motile.transcript

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.motile.core.JSON
import app.motile.core.objects
import app.motile.core.string
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.Theme
import app.motile.transcript.openui.PopoverPanel
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.Text

/** What programs sent with a message, as core names it: the chip's label and the values it opens. */
class SentValues(json: JSON) {
    class Entry(val name: String, val value: String)

    val label = json.string("label")
    val entries = json.objects("entries").map { Entry(it.string("name"), it.string("value")) }
}

/** The chip at the end of a message's bubble that opens its values in Motile's popover. */
@Composable
fun SentValuesChip(values: SentValues) {
    val palette = LocalPalette.current
    var open by remember { mutableStateOf(false) }
    var placed by remember { mutableStateOf<Float?>(null) }
    Box(
        Modifier.onGloballyPositioned { coordinates ->
            val top = coordinates.localToRoot(Offset.Zero).y
            if (open && placed != null && placed != top) open = false
            placed = top
        },
    ) {
        ActionButton(values.label, Symbol.Braces, variant = ButtonVariant.Outline, size = ControlSize.Small) { open = !open }
        if (open) {
            PopoverPanel(320f, onDismiss = { open = false }) {
                SelectionContainer {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        values.entries.forEach { entry ->
                            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                                Text(entry.name, style = Theme.smallFont.copy(lineHeight = scaled(18f).sp), color = palette.mutedForeground)
                                Text(entry.value, style = Theme.ui(13f).copy(lineHeight = scaled(20f).sp), color = palette.foreground)
                            }
                        }
                    }
                }
            }
        }
    }
}
