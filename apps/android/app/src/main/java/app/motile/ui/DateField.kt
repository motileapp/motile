package app.motile.ui

import android.app.DatePickerDialog
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
import java.time.LocalDate
import java.time.ZoneId

/**
 * A field that shows a day, `text` as the caller writes it or the `placeholder` without one, with a
 * calendar at its end. A press opens the system's date picker on `day`, or today, held to `min`…`max`;
 * picking a day calls `onPick`. It looks as a filled `InputField` of its size and fills its width.
 */
@Composable
fun DateField(
    text: String?,
    day: LocalDate?,
    onPick: (LocalDate) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String = "Date",
    label: String? = null,
    min: LocalDate? = null,
    max: LocalDate? = null,
    size: ControlSize = ControlSize.Regular,
    disabled: Boolean = false,
) {
    val palette = LocalPalette.current
    val context = LocalContext.current
    val enabled = LocalEnabled.current && !disabled
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    var picker by remember { mutableStateOf<DatePickerDialog?>(null) }
    DisposableEffect(Unit) { onDispose { picker?.dismiss() } }

    fun open() {
        val shown = day ?: LocalDate.now()
        val picked = DatePickerDialog.OnDateSetListener { _, year, month, dayOfMonth -> onPick(LocalDate.of(year, month + 1, dayOfMonth)) }
        val dialog = DatePickerDialog(context, picked, shown.year, shown.monthValue - 1, shown.dayOfMonth)
        if (min != null) dialog.datePicker.minDate = min.epochMillis()
        if (max != null) dialog.datePicker.maxDate = max.epochMillis()
        dialog.setOnDismissListener { picker = null }
        picker = dialog
        dialog.show()
    }

    val name = label ?: placeholder
    Row(
        modifier
            .fillMaxWidth()
            .height(size.height)
            .opacity(Opacity.Disabled, on = !enabled)
            .focusRing(focused, size.radius)
            .fieldFrame(size)
            .clickable(interactionSource = interaction, indication = null, enabled = enabled, role = Role.Button) { open() }
            .clearAndSetSemantics {
                role = Role.Button
                contentDescription = name
                if (text != null) stateDescription = text
                if (enabled) onClick { open(); true } else disabled()
            }
            .padding(horizontal = size.padding - fieldInset),
        horizontalArrangement = Arrangement.spacedBy(size.gap),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val color = if (text == null) palette.mutedStrongerForeground else palette.foreground
        Text(text ?: placeholder, Modifier.weight(1f), style = fieldStyle(size, monospaced = false, color = color), color = color, maxLines = 1)
        Icon(Symbol.Calendar, size.smallSymbol, color = palette.mutedStrongerForeground)
    }
}

private fun LocalDate.epochMillis(): Long = atStartOfDay(ZoneId.systemDefault()).toInstant().toEpochMilli()
