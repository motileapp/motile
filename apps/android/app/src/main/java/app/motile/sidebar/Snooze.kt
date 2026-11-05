package app.motile.sidebar

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalCloseSheet
import app.motile.LocalStore
import app.motile.core.AppStore
import app.motile.core.CustomSnooze
import app.motile.core.Native
import app.motile.core.double
import app.motile.core.json
import app.motile.core.objects
import app.motile.core.optionalDouble
import app.motile.core.optionalString
import app.motile.core.parseJSON
import app.motile.core.string
import app.motile.theme.ControlSize
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.DateField
import app.motile.ui.InputField
import app.motile.ui.MenuBuilder
import app.motile.ui.Text
import app.motile.ui.beside
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.time.format.TextStyle
import java.util.Locale

/** A time a snooze menu offers, as the core picks them. */
data class SnoozePreset(val label: String, val until: Double) {
    /** "In 1 Hour (3:15 PM)", "Next Week (Mon 9:00 AM)". */
    val title: String
        get() {
            val date = Instant.ofEpochMilli((until * 1000).toLong()).atZone(ZoneId.systemDefault())
            val time = date.format(DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT))
            val days = date.toLocalDate().toEpochDay() - LocalDate.now().toEpochDay()
            val weekday = date.dayOfWeek.getDisplayName(TextStyle.SHORT, Locale.getDefault())
            return "$label (${if (days <= 1) time else "$weekday $time"})"
        }
}

object Snooze {
    val presets: List<SnoozePreset>
        get() = ask("type" to "presets")?.objects("presets")?.map { SnoozePreset(it.string("label"), it.double("until")) } ?: emptyList()

    /** When a snooze written as a day and a time ends, or why it can't. */
    fun until(day: LocalDate, time: String): Result<Double> {
        val answer = ask("type" to "custom", "day" to day.toString(), "time" to time)
        val until = answer?.optionalDouble("until") ?: return Result.failure(IllegalArgumentException(answer?.optionalString("error") ?: "Pick a time."))
        return Result.success(until)
    }

    private fun ask(vararg question: Pair<String, Any?>) = parseJSON(Native.snooze(json(*question).toString()))
}

/** The times to snooze threads until, read when the menu opens, and the way to pick another. */
fun MenuBuilder.snoozeItems(store: AppStore, threadIDs: List<String>, fromSidebar: Boolean) {
    for (preset in Snooze.presets) item(preset.title) { store.snooze(threadIDs, preset.until, fromSidebar) }
    divider()
    item("Custom…") { store.customSnooze = CustomSnooze(threadIDs, fromSidebar) }
}

/** Snoozes threads until a day and a time the user picks. */
@Composable
fun CustomSnoozeSheet(snooze: CustomSnooze) {
    val store = LocalStore.current
    val close = LocalCloseSheet.current
    var day by remember { mutableStateOf(LocalDate.now().plusDays(1)) }
    var time by remember { mutableStateOf("9:00") }
    var error by remember { mutableStateOf<String?>(null) }
    fun submit() {
        Snooze.until(day, time)
            .onSuccess {
                store.snooze(snooze.threadIDs, it, snooze.fromSidebar)
                close()
            }
            .onFailure { error = it.message }
    }
    Column(
        Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 20.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Text("Snooze Until", size = 15f, weight = FontWeight.SemiBold)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            DateField(
                day.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM)), day,
                onPick = {
                    day = it
                    error = null
                },
                modifier = Modifier.weight(1f), min = LocalDate.now(), size = ControlSize.Large,
            )
            InputField(
                "Time", time,
                {
                    time = it
                    error = null
                },
                Modifier.width(110.dp), size = ControlSize.Large, onSubmit = ::submit,
            )
        }
        error?.let { Text(it, size = 12f, color = Theme.colors.destructive) }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
            ActionButton("Cancel", size = ControlSize.Large, margin = beside(end = 8.dp), onClick = close)
            ActionButton("Snooze", variant = ButtonVariant.Primary, size = ControlSize.Large, onClick = ::submit)
        }
    }
}
