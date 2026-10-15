package app.motile.usage

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Preferences
import app.motile.core.Server
import app.motile.core.UsageModel
import app.motile.core.UsageReport
import app.motile.core.abbreviated
import app.motile.platform.scaled
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.ui.AgentIcon
import app.motile.ui.Card
import app.motile.ui.Segmented
import app.motile.ui.Text
import app.motile.ui.ThemeDivider
import app.motile.ui.color
import java.text.NumberFormat
import java.util.Currency

/** What the Cost and Tokens tabs count. */
enum class Measure { Cost, Tokens }

private enum class Breakdown(val raw: String, val label: String) {
    Models("models", "Models"), Projects("projects", "Projects"), Servers("servers", "Servers"), Accounts("accounts", "Accounts"), Kinds("kinds", "Tokens"),
}

private const val BREAKDOWN_KEY = "usage.breakdown"
/** Narrower than this, the chart goes under the totals. */
private val besideWidth = 640.dp
/** Narrower than this, a row's bar goes under its name and numbers. */
private val stackWidth = 520.dp

private val dollars = NumberFormat.getCurrencyInstance().apply { currency = Currency.getInstance("USD") }
private val percent = NumberFormat.getPercentInstance().apply {
    minimumFractionDigits = 0
    maximumFractionDigits = 1
}

fun cost(value: Double): String {
    if (value > 0 && value < 0.01) return "<" + dollars.format(0.01)
    return dollars.format(value)
}

fun count(value: Long): String = value.abbreviated

/**
 * What the agents spent on the servers counted, the Cost and Tokens tabs: the total and each
 * agent's part beside a line for each agent over time, then what each model, project, server,
 * account and kind of token took.
 */
@Composable
fun SpendingView(report: UsageReport, measure: Measure, period: UsageModel.Period) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    var pointed by remember { mutableStateOf<Int?>(null) }
    LaunchedEffect(period) { pointed = null }
    val outdated = store.servers.filter { it.state == Server.State.Connected && it.protocolVersion < 10 }
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val width = maxWidth
        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(24.dp)) {
            if (report.tokens == 0L) {
                UsageNote("Nothing was spent in this time. Your servers keep what their agents spend from the version that shows this on.")
            } else {
                val chart = @Composable { modifier: Modifier ->
                    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Legend(report, measure, period, pointed)
                        SpendingChart(report, measure, period, pointed) { pointed = it }
                    }
                }
                if (width >= besideWidth) {
                    Row(horizontalArrangement = Arrangement.spacedBy(28.dp), verticalAlignment = Alignment.Top) {
                        Box(Modifier.width(240.dp)) { Summary(report, measure) }
                        chart(Modifier.weight(1f))
                    }
                } else {
                    Summary(report, measure)
                    chart(Modifier.fillMaxWidth())
                }
                Lines(report, measure, stacked = width < stackWidth)
            }
            for (server in outdated) Text("Update ${server.name} to see what its agents spend.", size = 11f, color = palette.mutedForeground)
        }
    }
}

@Composable
private fun Summary(report: UsageReport, measure: Measure) {
    val palette = LocalPalette.current
    val cost = measure == Measure.Cost
    Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(
                if (cost) cost(report.costUSD) else count(report.tokens), style = Theme.uiDigits(28f, FontWeight.SemiBold), color = palette.foreground,
            )
            Text(if (cost) "What the API would have charged" else "Tokens in and out", size = 11f, color = palette.mutedForeground)
            if (report.writingTokens > 0) {
                val written = if (cost) cost(report.writingCostUSD) else count(report.writingTokens)
                Text(
                    "$written of ${if (cost) "it" else "them"} for titles, branch names, commit messages and pull requests",
                    size = 11f, color = palette.mutedStrongerForeground,
                )
            }
            if (cost && report.unpricedTokens > 0) {
                Text("Without ${count(report.unpricedTokens)} tokens of models whose price isn’t known", size = 11f, color = palette.mutedStrongerForeground)
            }
        }
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            for (series in report.agents) AgentLine(report, series, measure)
            if (report.cacheSavingsUSD > 0) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Saved by the cache", Modifier.weight(1f).padding(end = 8.dp), size = 12f, color = palette.mutedForeground)
                    Text(cost(report.cacheSavingsUSD), style = Theme.uiDigits(12f), color = palette.foreground)
                }
            }
        }
    }
}

/** An agent's part: what it spent, and its share of the whole with the other measure. */
@Composable
private fun AgentLine(report: UsageReport, series: UsageReport.Series, measure: Measure) {
    val palette = LocalPalette.current
    val cost = measure == Measure.Cost
    val whole = if (cost) report.costUSD else report.tokens.toDouble()
    val part = if (cost) series.costUSD else series.tokens.toDouble()
    val share = if (whole > 0) part / whole else 0.0
    val other = if (cost) "${count(series.tokens)} tokens" else cost(series.costUSD)
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(7.dp), verticalAlignment = Alignment.CenterVertically) {
            Dot(series.agent.color(palette))
            AgentIcon(series.agent, size = 14f)
            Text(series.agent.title, Modifier.weight(1f).padding(end = 8.dp), size = 12f, color = palette.foreground, maxLines = 1)
            Text(amount(part, measure), style = Theme.uiDigits(12f), color = palette.foreground)
        }
        Text(
            "${percent.format(share)} of ${if (cost) "the cost" else "the tokens"} · $other",
            Modifier.padding(start = 15.dp), style = Theme.uiDigits(11f), color = palette.mutedStrongerForeground,
        )
    }
}

/** The agents with what each spent: in all, or at the time the finger is on. */
@Composable
private fun Legend(report: UsageReport, measure: Measure, period: UsageModel.Period, pointed: Int?) {
    val palette = LocalPalette.current
    val index = pointed?.takeIf { it in report.starts.indices }
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.CenterVertically) {
        for (series in report.agents) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Dot(series.agent.color(palette))
                Text(series.agent.title, size = 12f, color = palette.foreground, maxLines = 1)
                val value = index?.let { series.points(measure).getOrElse(it) { 0.0 } } ?: if (measure == Measure.Cost) series.costUSD else series.tokens.toDouble()
                Text(amount(value, measure), style = Theme.uiDigits(12f), color = palette.mutedForeground, maxLines = 1)
            }
        }
        Spacer(Modifier.weight(1f))
        if (index != null) Text(legendTime(report.starts[index], period), size = 12f, color = palette.mutedForeground, maxLines = 1)
    }
}

@Composable
private fun Dot(color: Color) {
    Box(Modifier.size(8.dp).background(color, CircleShape))
}

/** What each model, project, server, account or kind of token took, picked by the segments over it. Accounts are offered once an agent has more than one. */
@Composable
private fun Lines(report: UsageReport, measure: Measure, stacked: Boolean) {
    var breakdown by remember { mutableStateOf(Breakdown.entries.firstOrNull { it.raw == Preferences.string(BREAKDOWN_KEY) } ?: Breakdown.Models) }
    val offered = Breakdown.entries.filter { it != Breakdown.Accounts || report.accounts.isNotEmpty() }
    val current = if (breakdown in offered) breakdown else Breakdown.Models
    val shown = when (current) {
        Breakdown.Models -> report.models
        Breakdown.Projects -> report.projects
        Breakdown.Servers -> report.servers
        Breakdown.Accounts -> report.accounts
        Breakdown.Kinds -> report.kinds
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Segmented(offered.map { it.label to it }, current, {
            breakdown = it
            Preferences.set(BREAKDOWN_KEY, it.raw)
        }, fills = stacked)
        if (shown.isNotEmpty()) {
            Card(Modifier.fillMaxWidth()) {
                shown.forEachIndexed { index, line ->
                    if (index > 0) ThemeDivider()
                    LineRow(line, measure, stacked)
                }
            }
        }
    }
}

private val barWidth = 72.dp
private val dotWidth = 8.dp
private val spacing = 10.dp

/**
 * A model, a project, a server, an account or a kind of token: its name, its part of the whole as a
 * bar, its tokens and what they cost. Stacked, the bar runs under the rest across the row; a row
 * too narrow for the whole name gives up the bar.
 */
@Composable
private fun LineRow(line: UsageReport.Line, measure: Measure, stacked: Boolean) {
    if (stacked) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 11.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            LineNumbers(line, measure, bar = false)
            Bar(line, Modifier.fillMaxWidth().padding(start = if (line.agent == null) 0.dp else dotWidth + spacing))
        }
        return
    }
    BoxWithConstraints(Modifier.fillMaxWidth().heightIn(min = scaled(36f).dp).padding(horizontal = 12.dp), contentAlignment = Alignment.CenterStart) {
        val measurer = rememberTextMeasurer()
        val density = LocalDensity.current
        val style = Theme.ui(12f)
        val named = with(density) {
            val name = measurer.measure(line.name, style, maxLines = 1).size.width.toDp()
            val server = line.server?.let { measurer.measure(it, style, maxLines = 1).size.width.toDp() + spacing } ?: 0.dp
            name + server
        }
        val fixed: Dp = (if (line.agent == null) 0.dp else dotWidth + spacing) + 8.dp + spacing + barWidth + spacing * 2 + scaled(64f).dp + scaled(72f).dp
        LineNumbers(line, measure, bar = named + fixed <= maxWidth)
    }
}

@Composable
private fun LineNumbers(line: UsageReport.Line, measure: Measure, bar: Boolean) {
    val palette = LocalPalette.current
    val style = Theme.uiDigits(12f)
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(spacing), verticalAlignment = Alignment.CenterVertically) {
        line.agent?.let { Dot(it.color(palette)) }
        Row(Modifier.weight(1f).padding(end = 8.dp), horizontalArrangement = Arrangement.spacedBy(spacing), verticalAlignment = Alignment.CenterVertically) {
            Text(line.name, Modifier.weight(1f, fill = false), style = style, color = palette.foreground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
            line.server?.let {
                Text(it, Modifier.weight(1f, fill = false), style = style, color = palette.mutedForeground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
            }
        }
        if (bar) Bar(line, Modifier.width(barWidth))
        Text(
            count(line.tokens), Modifier.width(scaled(64f).dp), style = style, maxLines = 1, align = TextAlign.End,
            color = if (measure == Measure.Tokens) palette.foreground else palette.mutedForeground,
        )
        Text(
            line.costUSD?.let(::cost) ?: "No price", Modifier.width(scaled(72f).dp), style = style, maxLines = 1, align = TextAlign.End,
            color = if (measure == Measure.Cost && line.costUSD != null) palette.foreground else palette.mutedForeground,
        )
    }
}

/** The line's share of the whole. */
@Composable
private fun Bar(line: UsageReport.Line, modifier: Modifier) {
    val track = LocalSurface.current.color(Surface.Layer.Control)
    Box(modifier.height(4.dp).background(track, CircleShape)) {
        Box(Modifier.fillMaxWidth(line.share.toFloat().coerceIn(0f, 1f)).fillMaxHeight().background(LocalPalette.current.mutedForeground, CircleShape))
    }
}
