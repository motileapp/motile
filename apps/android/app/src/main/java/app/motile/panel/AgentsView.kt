package app.motile.panel

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.AgentInfo
import app.motile.core.Time
import app.motile.core.ToolStatus
import app.motile.platform.Symbol
import app.motile.theme.LocalPalette
import app.motile.theme.Theme
import app.motile.transcript.TranscriptView
import app.motile.ui.ActionButton
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.Text
import app.motile.ui.dotted
import kotlinx.coroutines.delay

/** The space between the right panel's list rows, split around each: it looks empty but is the row's. */
val panelRowMargin = PaddingValues(vertical = 1.dp)

/** The room under the end of what the right panel scrolls through, so that it never ends on the screen's edge. */
val panelEndRoom = 48.dp

/** The agents the thread's agent has started, and what one of them did once it is opened. */
@Composable
fun AgentsSurface() {
    val store = LocalStore.current
    val agents = store.agents
    val shown = agents.firstOrNull { it.id == store.rightPanel.shownAgent }
    if (shown != null) return AgentTranscript(shown)
    if (agents.isEmpty()) return PanelMessage("The agents this thread starts show up here.")
    val palette = LocalPalette.current
    Column(Modifier.fillMaxSize()) {
        PanelBar {
            Text(dotted(AgentsSummary.of(agents), palette.mutedForeground), Modifier.weight(1f), size = 12f, weight = FontWeight.Medium, color = palette.mutedForeground, maxLines = 1)
        }
        LazyColumn(
            Modifier.fillMaxSize(),
            contentPadding = PaddingValues(
                start = 8.dp, end = 8.dp,
                top = 8.dp - panelRowMargin.calculateTopPadding(),
                bottom = panelEndRoom - panelRowMargin.calculateBottomPadding(),
            ),
        ) {
            items(agents, key = { it.id }) { AgentRow(it) }
        }
    }
}

object AgentsSummary {
    /** "2 working · 1 done · 1 failed · 48k tokens", leaving out what there is none of. */
    fun of(agents: List<AgentInfo>): String {
        val counts = listOf(
            agents.count { it.status == ToolStatus.Running } to "working",
            agents.count { it.status == ToolStatus.Succeeded } to "done",
            agents.count { it.status == ToolStatus.Failed } to "failed",
        )
        val spent = agents.mapNotNull { it.tokens }
        val total = if (spent.isEmpty()) emptyList() else listOf(AgentInfo.tokens(spent.sum()))
        return (counts.filter { it.first > 0 }.map { "${it.first} ${it.second}" } + total).joinToString(" · ")
    }
}

@Composable
private fun AgentRow(agent: AgentInfo) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    HighlightButton(onClick = { store.rightPanel.showAgent(agent.id) }, modifier = Modifier.fillMaxWidth(), inset = panelRowMargin) {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(panelRowMargin)
                // An agent that another agent started stands in from it.
                .padding(start = if (agent.parent == null) 0.dp else 18.dp)
                .padding(horizontal = 10.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(9.dp),
        ) {
            Box(Modifier.width(16.dp).height(18.dp), contentAlignment = Alignment.Center) { AgentStatusIcon(agent.status) }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    // The kind gives way to the title.
                    Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) {
                        Text(agent.title, size = 13f, weight = FontWeight.Medium, color = palette.foreground, maxLines = 1)
                        agent.kind?.let {
                            Text(it, Modifier.padding(start = 6.dp).weight(1f, fill = false), size = 11f, color = palette.mutedForeground, maxLines = 1)
                        }
                    }
                    AgentTime(agent, Modifier.padding(start = 6.dp))
                }
                if (agent.detail.isNotEmpty()) Text(agent.detail, size = 12f, color = palette.mutedForeground, maxLines = 2)
                agent.usage?.let { Text(it, size = 11f, color = palette.mutedStrongerForeground) }
            }
        }
    }
}

@Composable
private fun AgentStatusIcon(status: ToolStatus) {
    val palette = LocalPalette.current
    when (status) {
        ToolStatus.Running -> Icon(Symbol.CircleDashed, 11f, color = palette.process)
        ToolStatus.Succeeded -> Icon(Symbol.Check, 11f, color = palette.mutedForeground)
        ToolStatus.Failed -> Icon(Symbol.X, 11f, color = palette.destructive)
    }
}

/** How long the agent has worked so far, or worked in all. */
@Composable
private fun AgentTime(agent: AgentInfo, modifier: Modifier = Modifier) {
    val palette = LocalPalette.current
    if (agent.working) {
        var now by remember { mutableDoubleStateOf(Time.now()) }
        LaunchedEffect(agent.startedAt) {
            while (true) {
                now = Time.now()
                delay(1000)
            }
        }
        AgentTimeText(Time.elapsed(agent.startedAt, now), palette.process, modifier)
        return
    }
    val milliseconds = agent.durationMs ?: return
    AgentTimeText(Time.duration(milliseconds), palette.mutedStrongerForeground, modifier)
}

@Composable
private fun AgentTimeText(text: String, color: Color, modifier: Modifier) {
    Text(text, modifier, style = Theme.uiDigits(11f, FontWeight.Medium), color = color, maxLines = 1)
}

@Composable
private fun AgentTranscript(agent: AgentInfo) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    Column(Modifier.fillMaxSize()) {
        PanelBar {
            ActionButton(icon = Symbol.ChevronLeft, help = "All agents", modifier = Modifier.outdent(8.dp)) { store.rightPanel.showAgents() }
            Box(Modifier.width(16.dp), contentAlignment = Alignment.Center) { AgentStatusIcon(agent.status) }
            Text(agent.title, Modifier.weight(1f), size = 12f, weight = FontWeight.Medium, color = palette.foreground, maxLines = 1)
            AgentTime(agent, Modifier.padding(end = 6.dp))
        }
        TranscriptView(store.agentTranscript, Modifier.weight(1f), contentPadding = PaddingValues(bottom = panelEndRoom))
    }
}
