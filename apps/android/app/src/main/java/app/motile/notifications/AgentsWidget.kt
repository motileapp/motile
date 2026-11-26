package app.motile.notifications

import android.content.Context
import android.content.Intent
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.net.toUri
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.LocalContext
import androidx.glance.LocalSize
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.provideContent
import androidx.glance.appwidget.updateAll
import androidx.glance.background
import androidx.glance.color.ColorProvider
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider
import app.motile.MainActivity
import app.motile.core.AgentsWork
import app.motile.core.ThreadLink
import app.motile.core.WorkState
import app.motile.platform.Platform
import app.motile.theme.Opacity
import app.motile.theme.Palette
import app.motile.theme.Radius
import app.motile.theme.TextSize
import kotlinx.coroutines.runBlocking

/** The "Agents" home screen widget: what the agents of every server do, as last heard. */
class AgentsWidget : GlanceAppWidget() {
    override val sizeMode = SizeMode.Exact

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val work = AgentsWork.of(KeptWork.servers)
        provideContent { Agents(work) }
    }

    companion object {
        /** Draws every placed widget again. Blocks: off the main thread. */
        fun refresh(context: Context) {
            runCatching { runBlocking { AgentsWidget().updateAll(context) } }
        }
    }
}

class AgentsWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = AgentsWidget()
}

private fun color(pick: (Palette) -> Color, faded: Boolean = false): ColorProvider {
    val alpha = Opacity.Disabled
    return ColorProvider(
        day = pick(Palette.light).let { if (faded) it.copy(alpha = alpha.light) else it },
        night = pick(Palette.dark).let { if (faded) it.copy(alpha = alpha.dark) else it },
    )
}

private fun size(size: TextSize) = (size.points * Platform.scale).sp

/** Wider than this, each thread shows its project and opens itself. */
private val MEDIUM_WIDTH = 250.dp
private val DOT = 7.dp
private val PADDING = 14.dp
private val GAP = 6.dp
private val SUMMARY_HEIGHT = 22.dp
private val ROW_HEIGHT = 20.dp

/** How many one-line threads fit under the summary in a widget this tall. */
internal fun rowsThatFit(height: androidx.compose.ui.unit.Dp): Int =
    ((height - PADDING * 2 - SUMMARY_HEIGHT) / (ROW_HEIGHT + GAP)).toInt().coerceAtLeast(0)

private fun stateColor(state: WorkState): (Palette) -> Color = when (state) {
    WorkState.Waiting -> { palette -> palette.warning }
    WorkState.Working -> { palette -> palette.process }
    WorkState.Finished -> { palette -> palette.success }
    WorkState.Failed -> { palette -> palette.destructive }
    WorkState.Other -> { palette -> palette.mutedForeground }
}

private fun opening(context: Context, item: AgentsWork.Item?): Intent {
    val link = item?.let { ThreadLink(it.serverID, it.threadID) } ?: return Intent(context, MainActivity::class.java)
    return Intent(Intent.ACTION_VIEW, link.url.toUri(), context, MainActivity::class.java)
}

@Composable
private fun Agents(work: AgentsWork) {
    val context = LocalContext.current
    val now = System.currentTimeMillis() / 1000.0
    val items = work.shown(now)
    val size = LocalSize.current
    val medium = size.width >= MEDIUM_WIDTH
    Column(
        GlanceModifier.fillMaxSize().cornerRadius(Radius.xl).background(color({ it.background })).padding(PADDING)
            .clickable(actionStartActivity(opening(context, items.firstOrNull()))),
    ) {
        Summary(work)
        for (item in items.take(rowsThatFit(size.height))) {
            Spacer(GlanceModifier.height(GAP))
            val row = GlanceModifier.fillMaxWidth().height(ROW_HEIGHT)
            Item(item, item.isStale(now), medium, if (medium) row.clickable(actionStartActivity(opening(context, item))) else row)
        }
    }
}

/** "2 working · 1 needs you", its separators one muted step weaker than its words. */
@Composable
private fun Summary(work: AgentsWork) {
    Row {
        work.summaryParts.forEachIndexed { index, part ->
            val words = if (index == 0 && work.active > 0) color({ it.foreground }) else color({ it.mutedForeground })
            if (index > 0) Text(" · ", style = TextStyle(color = color({ it.mutedStrongerForeground }), fontSize = size(TextSize.Md), fontWeight = FontWeight.Bold))
            Text(part, style = TextStyle(color = words, fontSize = size(TextSize.Md), fontWeight = FontWeight.Bold), maxLines = 1)
        }
    }
}

/** A thread on one line: a dot of its state, its title, then its project, faded once it may no longer be so. */
@Composable
private fun Item(item: AgentsWork.Item, stale: Boolean, showsProject: Boolean, modifier: GlanceModifier) {
    val style = TextStyle(color = color({ it.foreground }, stale), fontSize = size(TextSize.Sm), fontWeight = FontWeight.Medium)
    Row(modifier, verticalAlignment = Alignment.CenterVertically) {
        Box(GlanceModifier.size(DOT).cornerRadius(DOT / 2).background(color(stateColor(item.state), stale))) {}
        Spacer(GlanceModifier.width(6.dp))
        Text(item.title, style = style, maxLines = 1, modifier = GlanceModifier.defaultWeight())
        if (showsProject && item.project.isNotEmpty()) {
            Text(" · ", style = style.copy(color = color({ it.mutedStrongerForeground }, stale)), maxLines = 1)
            Text(item.project, style = style.copy(color = color({ it.mutedForeground }, stale)), maxLines = 1)
        }
    }
}
