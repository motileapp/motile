package app.motile.usage

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.max
import androidx.compose.ui.unit.min
import app.motile.LocalStore
import app.motile.core.ActivityReport
import app.motile.core.Preferences
import app.motile.core.Server
import app.motile.core.Time
import app.motile.core.UsageModel
import app.motile.platform.scaled
import app.motile.theme.ControlSize
import app.motile.theme.IconSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Palette
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.theme.TextSize
import app.motile.theme.Theme
import app.motile.theme.at
import app.motile.theme.wash
import app.motile.ui.Card
import app.motile.ui.Chip
import app.motile.ui.FadingScroll
import app.motile.ui.Icon
import app.motile.ui.Segmented
import app.motile.ui.Text
import app.motile.ui.ThemeDivider
import app.motile.ui.color
import app.motile.ui.dotted
import app.motile.ui.reaching
import java.text.NumberFormat

private enum class ActivityMeasure(val raw: String, val label: String) {
    Messages("messages", "Messages"), Turns("turns", "Turns"), Yours("engaged", "Your time"), Time("time", "Agent time");

    fun points(source: ActivityReport.Source): List<Double> = when (this) {
        Messages -> source.messagePoints
        Turns -> source.turnPoints
        Yours -> source.engagedPoints
        Time -> source.secondPoints
    }

    fun total(source: ActivityReport.Source): Double = when (this) {
        Messages -> source.messages.toDouble()
        Turns -> source.turns.toDouble()
        Yours -> source.engagedSeconds
        Time -> source.seconds
    }

    /** The lines of a chart of durations, where people count time; null leaves a count's to the chart. */
    fun ticks(report: ActivityReport): List<Double>? = when (this) {
        Yours -> report.engagedTicks
        Time -> report.secondTicks
        Messages, Turns -> null
    }

    fun amount(value: Double): String = when (this) {
        Yours, Time -> activityDuration(value)
        Messages, Turns -> Math.round(value).toInt().grouped
    }
}

private enum class TurnBreakdown(val raw: String, val label: String) {
    Agents("agents", "Agents"), Models("models", "Models"), Projects("projects", "Projects")
}

private enum class Who(val raw: String, val label: String) { You("you", "You"), Agents("agents", "Agents") }

/** What the heatmap's cells are coloured by. */
private enum class HourMeasure(val raw: String, val label: String) { Time("time", "Time spent"), Messages("messages", "Messages sent") }

private const val MEASURE_KEY = "activity.measure"
private const val BREAKDOWN_KEY = "activity.breakdown"
private const val WHO_KEY = "activity.who"
private const val HOURS_KEY = "activity.hours"

/** The most a long list shows before it scrolls: half the screen. */
@Composable
private fun listHeight(): Dp = maxOf(240.dp, LocalConfiguration.current.screenHeightDp.dp / 2)

@Composable
private fun Hours(report: ActivityReport, beside: Boolean) {
    var measure by remember { mutableStateOf(HourMeasure.entries.firstOrNull { it.raw == Preferences.string(HOURS_KEY) } ?: HourMeasure.Time) }
    val busiest = if (measure == HourMeasure.Time) report.busiestTime else report.busiestMessages
    Section("When you work", busiest, stacked = true, trailing = {
        Segmented(HourMeasure.entries.map { it.label to it }, measure, {
            measure = it
            Preferences.set(HOURS_KEY, it.raw)
        }, fills = !beside)
    }) {
        Card(Modifier.fillMaxWidth()) { Heatmap(report.heatmap, measure, Modifier.padding(16.dp)) }
    }
}

/** Narrower than this, the tiles go two to a row and the turns' cards under each other. */
private val besideWidth = 640.dp

private val integers = NumberFormat.getIntegerInstance()
private val Int.grouped: String get() = integers.format(this)

/**
 * What the user and the agents did on the servers counted, the Activity tab: the headline numbers,
 * the work over time by where it came from, the clients it came from, when the work was done, how
 * the turns went and the features used.
 */
@Composable
fun ActivityView(report: ActivityReport, period: UsageModel.Period) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    var pointed by remember { mutableStateOf<Int?>(null) }
    LaunchedEffect(period) { pointed = null }
    val outdated = store.servers.filter { it.state == Server.State.Connected && it.protocolVersion < 40 }
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val beside = maxWidth >= besideWidth
        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(36.dp)) {
            if (report.empty) {
                UsageNote("Nothing was done in this time. Servers keep what is done from the version that shows this on.")
            } else {
                StatGrid(report.tiles.map { Stat(it.label, it.value, it.detail) }, if (beside) 4 else 2, TextSize.Xl)
                OverTime(report, period, beside, pointed) { pointed = it }
                Hours(report, beside)
                if (report.clients.isNotEmpty()) Clients(report, beside)
                if (report.outcomes.isNotEmpty()) Turns(report, beside)
                if (report.features.isNotEmpty() || report.agentFeatures.isNotEmpty()) Features(report, beside)
            }
            for (server in outdated) Text("Update ${server.name} to see what was done on it.", size = TextSize.Xxs, color = palette.mutedForeground)
        }
    }
}

/**
 * A section's title, with what it says under it and a control on its right, or `stacked` across
 * the row under it, where a narrow page has no room beside the title. What it shows comes under.
 */
@Composable
private fun Section(
    title: String,
    detail: String? = null,
    stacked: Boolean = false,
    trailing: @Composable () -> Unit = {},
    content: @Composable () -> Unit,
) {
    val palette = LocalPalette.current
    val words = @Composable { modifier: Modifier ->
        Column(modifier, verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, size = TextSize.Md, weight = FontWeight.Medium, color = palette.foreground)
            detail?.let { Text(it, size = TextSize.Xs, color = palette.mutedForeground) }
        }
    }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (stacked) {
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                words(Modifier.padding(horizontal = 4.dp))
                trailing()
            }
        } else {
            val overhang = (ControlSize.Regular.height - ControlSize.Small.height) / 2
            Row(Modifier.fillMaxWidth().padding(horizontal = 4.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                words(Modifier.weight(1f))
                Box(Modifier.reaching(top = overhang, bottom = overhang)) { trailing() }
            }
        }
        content()
    }
}

/** The items in rows of `columns`, each as wide as the others, the last row's gaps left empty. With `even`, every item of a row is as tall as its tallest. */
@Composable
private fun <T> Grid(items: List<T>, columns: Int, spacing: Dp, even: Boolean = false, content: @Composable (T, Modifier) -> Unit) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(spacing)) {
        for (row in items.chunked(columns)) {
            Row(
                Modifier.fillMaxWidth().then(if (even) Modifier.height(IntrinsicSize.Min) else Modifier),
                horizontalArrangement = Arrangement.spacedBy(spacing),
                verticalAlignment = Alignment.Top,
            ) {
                for (item in row) content(item, if (even) Modifier.weight(1f).fillMaxHeight() else Modifier.weight(1f))
                repeat(columns - row.size) { Spacer(Modifier.weight(1f)) }
            }
        }
    }
}

private data class Stat(val label: String, val value: String, val detail: String)

/** Numbers in cards, `columns` to a row, every card of a row as tall as its tallest. */
@Composable
private fun StatGrid(stats: List<Stat>, columns: Int, valueSize: TextSize) {
    val palette = LocalPalette.current
    Grid(stats, columns, 10.dp, even = true) { stat, modifier ->
        Card(modifier) {
            Column(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(stat.label, size = TextSize.Xs, color = palette.mutedForeground, maxLines = 1)
                Text(stat.value, style = Theme.uiDigits(valueSize, FontWeight.SemiBold), color = palette.foreground, maxLines = 1)
                Text(stat.detail, style = Theme.uiDigits(TextSize.Xxs), color = palette.mutedStrongerForeground)
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun OverTime(report: ActivityReport, period: UsageModel.Period, beside: Boolean, pointed: Int?, point: (Int?) -> Unit) {
    val palette = LocalPalette.current
    var measure by remember { mutableStateOf(ActivityMeasure.entries.firstOrNull { it.raw == Preferences.string(MEASURE_KEY) } ?: ActivityMeasure.Messages) }
    val shown = report.sources.filter { measure.total(it) > 0 }
    val index = pointed?.takeIf { it in report.starts.indices }
    Section("Over time", stacked = true, trailing = {
        Segmented(ActivityMeasure.entries.map { it.label to it }, measure, {
            measure = it
            Preferences.set(MEASURE_KEY, it.raw)
        }, fills = !beside)
    }) {
        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.Top) {
                FlowRow(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    for (source in shown) {
                        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                            Dot(palette.chart(source.color))
                            Icon(source.symbol, IconSize.Xs, color = palette.mutedForeground)
                            Text(source.label, size = TextSize.Xs, color = palette.foreground, maxLines = 1)
                            val value = index?.let { measure.points(source).getOrElse(it) { 0.0 } } ?: measure.total(source)
                            Text(measure.amount(value), style = Theme.uiDigits(TextSize.Xs), color = palette.mutedForeground, maxLines = 1)
                        }
                    }
                }
                if (index != null) Text(pointedTime(report.starts[index], period), size = TextSize.Xs, color = palette.mutedForeground, maxLines = 1)
            }
            ActivityChart(report.starts, shown.map { palette.chart(it.color) to measure.points(it) }, measure.ticks(report), period, index, measure::amount, point)
        }
    }
}

@Composable
private fun Clients(report: ActivityReport, beside: Boolean) {
    Section("Clients", "Where you worked from") {
        Card(Modifier.fillMaxWidth()) {
            FadingScroll(listHeight()) {
                Column {
                    report.clients.forEachIndexed { index, client ->
                        if (index > 0) ThemeDivider()
                        ClientRow(client, beside)
                    }
                }
            }
        }
    }
}

private val badgeSide = scaled(32f).dp

/**
 * A client: its device, what it is, how it reaches the servers, and under them its part of your time
 * or of the messages, as wide as every client's so that the bars line up. Its words take one line when
 * `beside`.
 */
@Composable
private fun ClientRow(client: ActivityReport.Client, beside: Boolean) {
    val palette = LocalPalette.current
    val color = palette.chart(client.color)
    val seen = if (client.active) "Active now" else client.lastSeen?.let { Time.ago(it) }?.let { if (it == "now") "Seen just now" else "Seen $it ago" }
    Column(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(badgeSide).background(color.wash(), RoundedCornerShape(Radius.md)), contentAlignment = Alignment.Center) {
                Icon(client.symbol, IconSize.Md, color = color)
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        client.name, Modifier.weight(1f, fill = false), size = TextSize.Sm, weight = FontWeight.Medium, color = palette.foreground,
                        maxLines = 1, overflow = TextOverflow.MiddleEllipsis,
                    )
                    if (client.thisDevice) Chip("This device", small = true)
                    if (client.removed) Chip("Removed", muted = true, small = true)
                }
                if (beside) {
                    Text(
                        dotted(listOf(client.client, client.detail).filter { it.isNotEmpty() }.joinToString(" · "), palette.mutedStrongerForeground),
                        size = TextSize.Xs, color = palette.mutedForeground, maxLines = 1,
                    )
                } else {
                    Text(client.client, size = TextSize.Xs, color = palette.mutedForeground, maxLines = 1)
                    if (client.detail.isNotEmpty()) Text(client.detail, size = TextSize.Xs, color = palette.mutedForeground)
                }
            }
        }
        Column(Modifier.fillMaxWidth().padding(start = badgeSide + 12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            ActivityBar(client.share, color, Modifier.fillMaxWidth())
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(client.summary, Modifier.weight(1f), style = Theme.uiDigits(TextSize.Xxs), color = palette.foreground, maxLines = 1)
                seen?.let { Text(it, style = Theme.uiDigits(TextSize.Xxs), color = palette.mutedStrongerForeground, maxLines = 1) }
            }
        }
    }
}

@Composable
private fun Turns(report: ActivityReport, beside: Boolean) {
    Section("Turns") {
        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if (beside) {
                Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.Top) {
                    Outcomes(report, Modifier.weight(1f).fillMaxHeight())
                    Breakdown(report, Modifier.weight(1f).fillMaxHeight())
                }
            } else {
                Outcomes(report, Modifier.fillMaxWidth())
                Breakdown(report, Modifier.fillMaxWidth())
            }
            if (report.facts.isNotEmpty()) StatGrid(report.facts.map { Stat(it.label, it.value, it.detail) }, if (beside) 4 else 2, TextSize.Lg)
        }
    }
}

@Composable
private fun Outcomes(report: ActivityReport, modifier: Modifier) {
    val palette = LocalPalette.current
    Card(modifier) {
        Box(Modifier.tableHeader(breakdownHeaderHeight()).padding(horizontal = 14.dp), contentAlignment = Alignment.CenterStart) {
            Text("How they ended", size = TextSize.Sm, weight = FontWeight.Medium, color = palette.foreground)
        }
        ThemeDivider()
        report.outcomes.forEachIndexed { index, line ->
            if (index > 0) ThemeDivider()
            LineRow(line, palette.outcome(line.name))
        }
    }
}

private val BREAKDOWN_PADDING = 4.dp

/** The breakdown's picker with its padding, which the outcomes' header beside it matches. */
private fun breakdownHeaderHeight(): Dp = ControlSize.Regular.height + (Segmented.inset + Segmented.border + BREAKDOWN_PADDING) * 2

/** The turns by agent, model or project, picked by the segments over them. Projects are offered once there is more than one. */
@Composable
private fun Breakdown(report: ActivityReport, modifier: Modifier) {
    var breakdown by remember { mutableStateOf(TurnBreakdown.entries.firstOrNull { it.raw == Preferences.string(BREAKDOWN_KEY) } ?: TurnBreakdown.Agents) }
    val offered = TurnBreakdown.entries.filter { it != TurnBreakdown.Projects || report.projects.size > 1 }
    val current = if (breakdown in offered) breakdown else TurnBreakdown.Agents
    val shown = when (current) {
        TurnBreakdown.Agents -> report.agents
        TurnBreakdown.Models -> report.models
        TurnBreakdown.Projects -> report.projects
    }
    Card(modifier) {
        Box(Modifier.fillMaxWidth().padding(BREAKDOWN_PADDING), contentAlignment = Alignment.Center) {
            Segmented(offered.map { it.label to it }, current, {
                breakdown = it
                Preferences.set(BREAKDOWN_KEY, it.raw)
            }, fills = true)
        }
        ThemeDivider()
        FadingScroll(listHeight()) {
            Column {
                shown.forEachIndexed { index, line ->
                    if (index > 0) ThemeDivider()
                    LineRow(line, null)
                }
            }
        }
    }
}

/** An outcome, an agent, a model or a project: its name, how long its turns took, and its part of them. */
@Composable
private fun LineRow(line: ActivityReport.Line, tint: Color?) {
    val palette = LocalPalette.current
    val style = Theme.uiDigits(TextSize.Xs)
    val dot = line.agent?.color(palette) ?: tint
    Column(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 11.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            dot?.let { Dot(it) }
            Row(Modifier.weight(1f).padding(end = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(line.name, Modifier.weight(1f, fill = false), style = style, color = palette.foreground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
                line.server?.let { Text(it, Modifier.weight(1f, fill = false), style = style, color = palette.mutedForeground, maxLines = 1) }
            }
            Text(line.detail, style = style, color = palette.mutedForeground, maxLines = 1)
            Text(line.turns.grouped, Modifier.widthIn(min = scaled(36f).dp), style = style, color = palette.foreground, maxLines = 1, align = TextAlign.End)
        }
        ActivityBar(line.share, tint ?: dot ?: palette.mutedForeground, Modifier.fillMaxWidth())
    }
}

@Composable
private fun Features(report: ActivityReport, beside: Boolean) {
    var who by remember { mutableStateOf(Who.entries.firstOrNull { it.raw == Preferences.string(WHO_KEY) } ?: Who.You) }
    val offered = Who.entries.filter { if (it == Who.You) report.features.isNotEmpty() else report.agentFeatures.isNotEmpty() }
    val current = if (who in offered) who else offered.firstOrNull() ?: Who.You
    Section("Features", stacked = !beside && offered.size > 1, trailing = {
        if (offered.size > 1) {
            Segmented(offered.map { it.label to it }, current, {
                who = it
                Preferences.set(WHO_KEY, it.raw)
            }, fills = !beside)
        }
    }) {
        when (current) {
            Who.You -> if (beside) {
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.Top) {
                    FeatureColumn(report.features.filter { it.column == 0 }, Modifier.weight(1f))
                    FeatureColumn(report.features.filter { it.column == 1 }, Modifier.weight(1f))
                }
            } else {
                FeatureColumn(report.features, Modifier.fillMaxWidth())
            }
            Who.Agents -> FeatureCard("Motile tools", report.agentFeatures.sumOf { it.count }, report.agentFeatures, Modifier.fillMaxWidth())
        }
    }
}

@Composable
private fun FeatureColumn(groups: List<ActivityReport.FeatureGroup>, modifier: Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        groups.forEach { group ->
            key(group.name) { FeatureCard(group.name, group.count, group.features, Modifier.fillMaxWidth()) }
        }
    }
}

/** The header row of a card that is a table, filled as a table's header is. */
@Composable
private fun Modifier.tableHeader(minHeight: Dp = scaled(40f).dp): Modifier = fillMaxWidth().background(LocalPalette.current.cardAccent).heightIn(min = minHeight)

/** The features of a group as a table under a header of its name and total, each with how often it was used, scrolling past half the screen. */
@Composable
private fun FeatureCard(title: String, count: Int, features: List<ActivityReport.Feature>, modifier: Modifier) {
    val palette = LocalPalette.current
    Card(modifier) {
        Row(Modifier.tableHeader().padding(horizontal = 14.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(title, Modifier.weight(1f).padding(end = 8.dp), size = TextSize.Sm, weight = FontWeight.Medium, color = palette.foreground)
            Text(count.grouped, style = Theme.uiDigits(TextSize.Sm, FontWeight.Medium), color = palette.mutedForeground)
        }
        ThemeDivider()
        FadingScroll(listHeight()) {
            Column {
                features.forEachIndexed { index, feature ->
                    if (index > 0) ThemeDivider()
                    FeatureRow(feature)
                }
            }
        }
    }
}

@Composable
private fun FeatureRow(feature: ActivityReport.Feature) {
    val palette = LocalPalette.current
    val style = Theme.uiDigits(TextSize.Xs)
    Row(
        Modifier.fillMaxWidth().heightIn(min = scaled(40f).dp).padding(horizontal = 14.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f).padding(vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(feature.label, style = style, color = palette.foreground, maxLines = 2)
            if (feature.detail.isNotEmpty()) Text(feature.detail, size = TextSize.Xxs, color = palette.mutedForeground, maxLines = 1)
        }
        Spacer(Modifier.width(8.dp))
        ActivityBar(feature.share, palette.chart1, Modifier.width(scaled(56f).dp))
        Text(feature.count.grouped, Modifier.widthIn(min = scaled(32f).dp), style = style, color = palette.foreground, maxLines = 1, align = TextAlign.End)
    }
}

/** A part of the whole as a thin bar across what it is given. */
@Composable
private fun ActivityBar(share: Double, color: Color, modifier: Modifier) {
    val track = LocalSurface.current.color(Surface.Layer.Control)
    Canvas(modifier.height(4.dp)) {
        val radius = CornerRadius(size.height / 2)
        drawRoundRect(track, cornerRadius = radius)
        if (share <= 0) return@Canvas
        val width = maxOf(size.width * share.toFloat().coerceAtMost(1f), 4.dp.toPx())
        drawRoundRect(color, size = Size(width, size.height), cornerRadius = radius)
    }
}

private val cellGap = 3.dp
private val keySide = 10.dp

/**
 * When the work was done, each cell as busy as its colour is strong. The cell a finger slid onto or
 * tapped last says what was done in it, the cell of now to start with. The gaps are only drawn: each cell's tap area reaches halfway into them.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Heatmap(heatmap: ActivityReport.Heatmap, measure: HourMeasure, modifier: Modifier) {
    val palette = LocalPalette.current
    val density = LocalDensity.current
    val surface = LocalSurface.current
    var picked by remember(heatmap.columns, heatmap.cells.size, heatmap.latest) { mutableStateOf<Int?>(null) }
    val chosen = picked ?: heatmap.latest
    val fills = listOf(
        surface.color(Surface.Layer.Control),
        palette.chart1.at(Opacity.ColorTintChart),
        palette.chart1.at(Opacity.ColorTintChartStronger),
        palette.chart1.at(Opacity.ColorTintChartStrongest),
        palette.chart1,
    )
    val measurer = rememberTextMeasurer()
    val labelStyle = Theme.ui(TextSize.Xxs).copy(color = palette.mutedForeground)
    val rowLabels = heatmap.rows.map { measurer.measure(it, labelStyle, maxLines = 1) }
    val axisLabels = heatmap.axis.map { it.column to measurer.measure(it.label, labelStyle, maxLines = 1) }
    val axisHeight = with(density) { measurer.measure(" ", labelStyle).size.height.toDp() }
    val leading = with(density) { rowLabels.maxOfOrNull { it.size.width }?.toDp()?.plus(8.dp) ?: 0.dp }
    // The canvas reaches half a gap past each side, so that the slots, each a cell and half the gaps around it, start at its edge.
    val reach = cellGap / 2
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        BoxWithConstraints(Modifier.fillMaxWidth()) {
            val pitch = (maxWidth - leading + cellGap) / maxOf(heatmap.columns, 1)
            val cellWidth = max(pitch - cellGap, 1.dp)
            val labelLine = with(density) { rowLabels.maxOfOrNull { it.size.height }?.toDp() ?: 0.dp }
            val cellHeight = max(min(max(cellWidth, 4.dp), scaled(18f).dp), labelLine)
            // Small cells round less, so that they don't turn into dots.
            val corner = min(Radius.xs, min(cellWidth, cellHeight) / 4)
            val gridHeight = (cellHeight + cellGap) * heatmap.rowCount - cellGap
            Canvas(
                Modifier
                    .fillMaxWidth()
                    .height(max(gridHeight, 0.dp) + 6.dp + axisHeight)
                    .reaching(reach, reach, reach, reach)
                    .chartPointing(chosen, { picked = it ?: chosen }) { at, _ ->
                        with(density) {
                            val (x, y) = at.x - leading.toPx() to at.y
                            if (x < 0 || y < 0) return@chartPointing null
                            val column = (x / pitch.toPx()).toInt()
                            val row = (y / (cellHeight + cellGap).toPx()).toInt()
                            if (column >= heatmap.columns || row >= heatmap.rowCount) return@chartPointing null
                            (row * heatmap.columns + column).takeIf { heatmap.cells.getOrNull(it) != null }
                        }
                    },
            ) {
                val origin = reach.toPx()
                val (pitchPx, cellPx, gapPx) = Triple(pitch.toPx(), cellHeight.toPx(), cellGap.toPx())
                val start = origin + leading.toPx()
                val cellSize = Size(cellWidth.toPx(), cellPx)
                val radius = CornerRadius(corner.toPx())
                val stroke = 1.dp.toPx()
                rowLabels.forEachIndexed { row, label ->
                    drawText(label, topLeft = Offset(origin, origin + row * (cellPx + gapPx) + (cellPx - label.size.height) / 2))
                }
                heatmap.cells.forEachIndexed { index, cell ->
                    if (cell == null) return@forEachIndexed
                    val left = start + index % heatmap.columns * pitchPx
                    val topLeft = Offset(left, origin + index / heatmap.columns * (cellPx + gapPx))
                    val level = if (measure == HourMeasure.Time) cell.timeLevel else cell.messageLevel
                    drawRoundRect(fills[level.coerceIn(0, 4)], topLeft, cellSize, radius)
                    if (index != chosen) return@forEachIndexed
                    drawRoundRect(
                        palette.foreground, topLeft + Offset(stroke / 2, stroke / 2), Size(cellSize.width - stroke, cellPx - stroke), radius, style = Stroke(stroke),
                    )
                }
                val axisTop = origin + gridHeight.toPx() + 6.dp.toPx()
                axisLabels.forEach { (column, label) -> drawText(label, topLeft = Offset(start + column * pitchPx, axisTop)) }
            }
        }
        FlowRow(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            heatmap.cells.getOrNull(chosen)?.let { shown ->
                val text = listOf(shown.label, shown.time, messages(shown.messages), turns(shown.turns)).filter { it.isNotEmpty() }.joinToString(" · ")
                Text(
                    dotted(text, palette.mutedStrongerForeground),
                    Modifier.padding(end = 8.dp).align(Alignment.CenterVertically),
                    style = Theme.uiDigits(TextSize.Xxs),
                    color = palette.mutedForeground,
                )
            }
            Row(Modifier.align(Alignment.CenterVertically), horizontalArrangement = Arrangement.spacedBy(cellGap), verticalAlignment = Alignment.CenterVertically) {
                Text("Less", Modifier.padding(end = 3.dp), size = TextSize.Xxs, color = palette.mutedForeground, maxLines = 1)
                for (fill in fills) Box(Modifier.size(keySide).background(fill, RoundedCornerShape(min(Radius.xs, keySide / 4))))
                Text("More", Modifier.padding(start = 3.dp), size = TextSize.Xxs, color = palette.mutedForeground, maxLines = 1)
            }
        }
    }
}

/** A source's or a client's colour: one of the six chart colours, the agents' colour or its tint, or muted for clients that didn't say which they are. */
@Composable
private fun Palette.chart(index: Int): Color = when (index) {
    1 -> chart1
    2 -> chart2
    3 -> chart3
    4 -> chart4
    5 -> chart5
    6 -> chart6
    7 -> pending
    8 -> pending.at(Opacity.ColorTintChartStronger)
    else -> mutedForeground
}

private fun Palette.outcome(name: String): Color = when (name) {
    "Finished" -> success
    "Stopped" -> mutedForeground
    "Hit the usage limit" -> warning
    else -> destructive
}

private fun messages(count: Int): String = if (count == 1) "1 message" else "${count.grouped} messages"

private fun turns(count: Int): String = if (count == 1) "1 turn" else "${count.grouped} turns"

/** The time pointed at, in the legend: the weekday and the hour over a day, the weekday and the date otherwise. */
private fun pointedTime(seconds: Double, period: UsageModel.Period): String =
    formatted(seconds, if (period == UsageModel.Period.Day) "EEEEj" else "EEEEMMMMd")

private fun activityDuration(value: Double): String {
    val seconds = Math.round(maxOf(value, 0.0)).toInt()
    if (seconds < 60) return "${seconds}s"
    if (seconds < 3600) return "${seconds / 60}m"
    val hours = seconds / 3600
    val minutes = seconds % 3600 / 60
    return if (minutes == 0 || hours >= 100) "${hours.grouped}h" else "${hours}h ${minutes}m"
}
