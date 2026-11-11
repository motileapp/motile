package app.motile.ui

import android.app.TimePickerDialog
import android.text.format.DateFormat
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.stateDescription
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.Opacity
import app.motile.theme.opacity
import java.time.LocalTime
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/**
 * A field that shows a time of day with a clock at its end. A press opens the system's time picker on
 * `time`, in the 12 or 24 hours the phone is set to; picking one calls `onPick`. It looks as
 * `DateField` does and fills its width.
 */
@Composable
fun TimeField(
    time: LocalTime,
    onPick: (LocalTime) -> Unit,
    modifier: Modifier = Modifier,
    size: ControlSize = ControlSize.Regular,
    disabled: Boolean = false,
) {
    val palette = LocalPalette.current
    val context = LocalContext.current
    val enabled = LocalEnabled.current && !disabled
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    var picker by remember { mutableStateOf<TimePickerDialog?>(null) }
    DisposableEffect(Unit) { onDispose { picker?.dismiss() } }

    fun open() {
        val picked = TimePickerDialog.OnTimeSetListener { _, hour, minute -> onPick(LocalTime.of(hour, minute)) }
        val dialog = TimePickerDialog(context, picked, time.hour, time.minute, DateFormat.is24HourFormat(context))
        dialog.setOnDismissListener { picker = null }
        picker = dialog
        dialog.show()
    }

    val text = time.format(DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT))
    Row(
        modifier
            .fillMaxWidth()
            .height(size.height)
            .opacity(Opacity.Disabled, on = !enabled)
            .focusRing(focused, size.radius)
            .fieldFrame(size)
            .arrowPointer()
            .clickable(interactionSource = interaction, indication = null, enabled = enabled, role = Role.Button) { open() }
            .clearAndSetSemantics {
                role = Role.Button
                contentDescription = "Time"
                stateDescription = text
                if (enabled) onClick { open(); true } else disabled()
            }
            .padding(horizontal = size.padding - fieldInset),
        horizontalArrangement = Arrangement.spacedBy(size.gap),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text, Modifier.weight(1f), style = fieldStyle(size, monospaced = false, color = palette.foreground), color = palette.foreground, maxLines = 1)
        Icon(Symbol.Clock, size.smallSymbol, color = palette.mutedStrongerForeground)
    }
}
