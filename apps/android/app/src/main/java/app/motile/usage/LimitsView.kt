package app.motile.usage

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import app.motile.core.LimitsReport
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.iconSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.theme.at
import app.motile.ui.AgentIcon
import app.motile.ui.Card
import app.motile.ui.Chip
import app.motile.ui.Icon
import app.motile.ui.Text
import app.motile.ui.ThemeDivider

/** Narrower than this, a window puts its bar under its numbers. */
private val besideWidth = 520.dp

/** How much of their plans the agents' logins have used: a section for each login, its windows in one card, each with a bar of what was used. */
@Composable
fun LimitsView(report: LimitsReport) {
    val palette = LocalPalette.current
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val beside = maxWidth >= besideWidth
        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(40.dp)) {
            if (report.sections.isEmpty() && report.notes.isEmpty()) UsageNote("No agent is installed on these servers.")
            for (section in report.sections) {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Header(section)
                    section.note?.let { note ->
                        Card(Modifier.fillMaxWidth()) {
                            Text(note, Modifier.fillMaxWidth().padding(16.dp), size = 12f, color = palette.mutedForeground)
                        }
                    }
                    if (section.windows.isNotEmpty()) {
                        Card(Modifier.fillMaxWidth()) {
                            section.windows.forEachIndexed { index, window ->
                                if (index > 0) ThemeDivider()
                                LimitRow(window, beside)
                            }
                        }
                    }
                }
            }
            for (note in report.notes) Text(note, size = 11f, color = palette.mutedForeground)
        }
    }
}

@Composable
private fun Header(section: LimitsReport.Section) {
    val palette = LocalPalette.current
    Row(
        Modifier.fillMaxWidth().padding(start = 4.dp, end = 4.dp, bottom = 2.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AgentIcon(section.agent, size = iconSize(text = 14f))
        Text(section.agent.title, size = 14f, weight = FontWeight.Medium, color = palette.foreground, maxLines = 1)
        section.plan?.let { Chip(it) }
        val login = buildAnnotatedString {
            section.name?.let { append(it) }
            if (section.name != null && section.account != null) withStyle(SpanStyle(color = palette.mutedStrongerForeground)) { append(" · ") }
            section.account?.let { append(it) }
        }
        Text(login, Modifier.weight(1f, fill = false), size = 12f, color = palette.mutedForeground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
    }
}

/** One window of a plan: what was used of it, how that compares with the time passed, and a bar with when it starts over. */
@Composable
private fun LimitRow(window: LimitsReport.Window, beside: Boolean) {
    val palette = LocalPalette.current
    val color = when (window.tone) {
        LimitsReport.Window.Tone.Success -> palette.success
        LimitsReport.Window.Tone.Pending -> palette.pending
        LimitsReport.Window.Tone.Warning -> palette.warning
    }
    val padding = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 14.dp)
    if (beside) {
        Row(padding, horizontalArrangement = Arrangement.spacedBy(24.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.width(168.dp)) { Numbers(window) }
            LimitBar(window, color, Modifier.weight(1f))
        }
    } else {
        Column(padding, verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Numbers(window)
            LimitBar(window, color, Modifier.fillMaxWidth())
        }
    }
}

@Composable
private fun Numbers(window: LimitsReport.Window) {
    val palette = LocalPalette.current
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(window.label, size = 13f, weight = FontWeight.Medium, color = palette.foreground, maxLines = 1)
        Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
            Text(window.used, Modifier.alignByBaseline(), style = Theme.uiDigits(28f, FontWeight.SemiBold), color = palette.foreground)
            Text("used", Modifier.alignByBaseline(), size = 13f, color = palette.mutedForeground)
        }
        window.pace?.let { pace ->
            Row(
                Modifier.semantics { contentDescription = "${pace.label}. ${pace.help}" },
                horizontalArrangement = Arrangement.spacedBy(5.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(pace.symbol, iconSize(text = 12f), color = palette.mutedStrongerForeground)
                Text(pace.label, size = 12f, color = palette.mutedStrongerForeground)
            }
        }
    }
}

private val LimitsReport.Window.Pace.symbol: Symbol
    get() = when (this) {
        LimitsReport.Window.Pace.Ahead -> Symbol.TrendingUp
        LimitsReport.Window.Pace.On -> Symbol.Gauge
        LimitsReport.Window.Pace.Under -> Symbol.TrendingDown
    }

private val LimitsReport.Window.Pace.label: String
    get() = when (this) {
        LimitsReport.Window.Pace.Ahead -> "Ahead of pace"
        LimitsReport.Window.Pace.On -> "On pace"
        LimitsReport.Window.Pace.Under -> "Under pace"
    }

private val LimitsReport.Window.Pace.help: String
    get() = when (this) {
        LimitsReport.Window.Pace.Ahead -> "Used faster than the window passes"
        LimitsReport.Window.Pace.On -> "Used as fast as the window passes"
        LimitsReport.Window.Pace.Under -> "Room left for the rest of the window"
    }

/** What was used of a window as the part of a bar it fills, and at its end when the window starts over and the resets the login may use. */
@Composable
private fun LimitBar(window: LimitsReport.Window, color: Color, modifier: Modifier) {
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    val shape = RoundedCornerShape(Radius.md)
    val fill = color.at(Opacity.ColorTintChart)
    val text = Theme.uiDigits(11f, FontWeight.Medium)
    BoxWithConstraints(modifier.height(scaled(28f).dp).background(surface.color(Surface.Layer.Control), shape)) {
        val filled = maxWidth * (window.usedPercent / 100).toFloat().coerceIn(0f, 1f)
        Box(Modifier.width(filled).fillMaxHeight().background(fill, shape))
        Row(
            Modifier.fillMaxSize().padding(horizontal = 9.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(window.used, Modifier.weight(1f), style = Theme.uiDigits(11f, FontWeight.SemiBold), color = palette.foreground, maxLines = 1)
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                window.resetsIn?.let { Text(it, Modifier.semantics { contentDescription = "Starts over in $it" }, style = text, color = palette.foreground, maxLines = 1) }
                if (window.resetsIn != null && window.resetCredits > 0) Text("·", style = text, color = palette.foreground)
                if (window.resetCredits > 0) {
                    val help = if (window.resetCredits == 1) "1 reset to use" else "${window.resetCredits} resets to use"
                    Row(
                        Modifier.semantics { contentDescription = help },
                        horizontalArrangement = Arrangement.spacedBy(3.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(Symbol.Ticket, iconSize(text = 11f), color = palette.foreground)
                        Text("${window.resetCredits}", style = text, color = palette.foreground)
                    }
                }
            }
        }
    }
}
