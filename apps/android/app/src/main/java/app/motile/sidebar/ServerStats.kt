package app.motile.sidebar

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.AppStore
import app.motile.core.LimitsReport
import app.motile.core.RestartWhen
import app.motile.core.Server
import app.motile.core.ServerStats
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.IconSize
import app.motile.theme.TextSize
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.ActionMenu
import app.motile.ui.ButtonVariant
import app.motile.ui.Icon
import app.motile.ui.Spinner
import app.motile.ui.Text
import app.motile.usage.LimitBar
import app.motile.usage.color
import kotlin.math.max
import kotlin.math.roundToInt

/** The server's uptime, the update's button at its end and, while the update goes, how far it is under them. */
@Composable
fun ServerStatsHeader(server: Server, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val step = store.updateStep(server)
    Column(modifier, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = ControlSize.Regular.height),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val uptime = store.stats(server.id)?.uptime?.let { uptime(it, Theme.colors.mutedForeground, Theme.colors.foreground) }
            Text(uptime ?: AnnotatedString(" "), Modifier.weight(1f), style = Theme.uiDigits(rowText), maxLines = 1)
            UpdateButton(server, step)
        }
        UpdateLine(step)
    }
}

@Composable
private fun UpdateButton(server: Server, step: ServerUpdateStep) {
    val store = LocalStore.current
    when (step) {
        ServerUpdateStep.Offered(busy = false) -> ActionButton(
            "Update", help = "Install the latest version on ${server.shortName}. It restarts once no agent works there.",
            variant = ButtonVariant.Secondary, size = ControlSize.Regular, round = true,
        ) { store.update(server) }
        ServerUpdateStep.Offered(busy = true) -> ActionMenu(
            "Update", help = "Agents are working on ${server.shortName}. Update it once they finish, or now: they stop and continue once it is back.",
            variant = ButtonVariant.Secondary, size = ControlSize.Regular, round = true,
        ) {
            item("Update when agents finish") { store.update(server, RestartWhen.Idle) }
            item("Update now and auto-resume agents") { store.update(server, RestartWhen.Now) }
        }
        ServerUpdateStep.Waiting -> ActionButton(
            "Restart Now", help = "Stop the agents on ${server.shortName} and restart it. Their threads continue once it is back.",
            variant = ButtonVariant.Secondary, size = ControlSize.Regular, round = true,
        ) { store.update(server, RestartWhen.Now) }
        else -> Unit
    }
}

@Composable
private fun UpdateLine(step: ServerUpdateStep) {
    when (step) {
        is ServerUpdateStep.Downloading -> Updating(step.progress)
        ServerUpdateStep.Waiting -> Text("Installed, restarts once its agents finish", size = TextSize.Xs, color = Theme.colors.mutedForeground)
        ServerUpdateStep.Restarting -> Updating("Restarting")
        else -> Unit
    }
}

@Composable
private fun Updating(text: String) {
    val muted = Theme.colors.mutedForeground
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
        Spinner(IconSize.Xs, color = muted)
        Text(text, style = Theme.uiDigits(TextSize.Xs), color = muted)
    }
}

/** Where a server's update is, as its sheet shows it. */
sealed interface ServerUpdateStep {
    data object Current : ServerUpdateStep
    /** A newer version is out. While agents work, the update waits for them or stops them. */
    data class Offered(val busy: Boolean) : ServerUpdateStep
    /** "Updating 40%". */
    data class Downloading(val progress: String) : ServerUpdateStep
    /** The new version is installed and starts once the agents have finished. */
    data object Waiting : ServerUpdateStep
    data object Restarting : ServerUpdateStep
}

fun AppStore.updateStep(server: Server): ServerUpdateStep {
    serverUpdate(server)?.let { update ->
        if (update.restarting) return ServerUpdateStep.Restarting
        if (update.waiting) return ServerUpdateStep.Waiting
        return ServerUpdateStep.Downloading(progress(update))
    }
    if (!isOutdated(server)) return ServerUpdateStep.Current
    return ServerUpdateStep.Offered(busy = isBusy(server) && canChooseRestart(server))
}

private val symbolGap = 5.dp
/** As large as the rows of the other sheets. */
private val rowText = TextSize.Lg
/** Between a part's name and its chart, so that they read as one. */
private val chartGap = 8.dp
/** A meter is a thin line, so it stands further from its name than a chart's bars do. */
private val meterGap = chartGap + 4.dp

/** A part of the machine: its symbol, its name and amounts, how much it is now, and under them the last minutes as bars or how full it is. */
@Composable
fun ServerStatsRow(row: ServerStats.Row, modifier: Modifier = Modifier) {
    val colors = Theme.colors
    val style = Theme.uiDigits(rowText)
    Column(modifier, verticalArrangement = Arrangement.spacedBy(chartGap)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Icon(row.symbol, IconSize.Md, Modifier.padding(end = symbolGap), color = colors.mutedForeground)
            Text(label(row, colors.mutedForeground, colors.mutedStrongerForeground), Modifier.weight(1f).padding(end = 8.dp), style = style, maxLines = 1)
            Text(value(row, colors.mutedForeground, colors.mutedStrongerForeground, colors.foreground), style = Theme.uiDigits(rowText, FontWeight.Medium), maxLines = 1)
        }
        val fill = row.fill
        if (fill != null) {
            LimitBar(fill.height * 100, fill.tone, Modifier.padding(top = meterGap - chartGap))
        } else {
            StatsBars(row.bars)
        }
    }
}

/** "Uptime: 2d 10h", named as a part's row is and as much as its value. */
private fun uptime(uptime: String, muted: Color, foreground: Color): AnnotatedString = buildAnnotatedString {
    withStyle(SpanStyle(color = muted)) { append("Uptime: ") }
    withStyle(SpanStyle(color = foreground, fontWeight = FontWeight.Medium)) { append(uptime) }
}

/** "CPU · 12 cores", the separator a muted step behind the words it divides. */
private fun label(row: ServerStats.Row, muted: Color, separator: Color): AnnotatedString = buildAnnotatedString {
    withStyle(SpanStyle(color = muted)) { append(row.label) }
    val detail = row.detail ?: return@buildAnnotatedString
    withStyle(SpanStyle(color = separator)) { append(" · ") }
    withStyle(SpanStyle(color = muted)) { append(detail) }
}

/** "12.2 GB · 76%". */
private fun value(row: ServerStats.Row, muted: Color, separator: Color, foreground: Color): AnnotatedString = buildAnnotatedString {
    row.used?.let { used ->
        withStyle(SpanStyle(color = muted)) { append(used) }
        withStyle(SpanStyle(color = separator)) { append(" · ") }
    }
    withStyle(SpanStyle(color = foreground)) { append(row.value) }
}

private val ServerStats.Row.symbol: Symbol
    get() = when (kind) {
        ServerStats.Row.Kind.Cpu -> Symbol.Cpu
        ServerStats.Row.Kind.Memory -> Symbol.MemoryStick
        ServerStats.Row.Kind.Download -> Symbol.ArrowDown
        ServerStats.Row.Kind.Upload -> Symbol.ArrowUp
        ServerStats.Row.Kind.Disk -> Symbol.HardDrive
    }

private const val BARS_HEIGHT = 12

/** A bar for each sample, standing on a line of dots: one under every place a bar can stand. */
@Composable
private fun StatsBars(bars: List<ServerStats.Bar>) {
    val tones = LimitsReport.Window.Tone.entries.associateWith { it.color }
    val dot = Theme.colors.mutedStrongestForeground
    Canvas(Modifier.fillMaxWidth().height(BARS_HEIGHT.dp)) {
        if (bars.isEmpty()) return@Canvas
        val unit = 1.dp.toPx()
        val pitch = size.width / bars.size
        val width = max(unit, pitch - unit)
        bars.forEachIndexed { index, bar ->
            val x = index * pitch
            if (bar.height <= 0) {
                drawRect(dot, Offset(x, size.height - unit), Size(width, unit))
                return@forEachIndexed
            }
            val height = max(1, (bar.height * BARS_HEIGHT).roundToInt()) * unit
            drawRect(tones.getValue(bar.tone), Offset(x, size.height - height), Size(width, height))
        }
    }
}
