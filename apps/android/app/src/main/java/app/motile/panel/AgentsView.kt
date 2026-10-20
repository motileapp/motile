package app.motile.panel

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.AgentInfo
import app.motile.core.AgentState
import app.motile.core.ThreadInfo
import app.motile.core.Time
import app.motile.core.ToolStatus
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.Palette
import app.motile.theme.Theme
import app.motile.ui.AgentAvatar
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.MenuPopup
import app.motile.ui.Text
import app.motile.ui.color
import app.motile.ui.pressPosition
import app.motile.ui.rememberMenuState
import kotlinx.coroutines.delay

/** The space between the right panel's list rows, split around each: it looks empty but is the row's. */
val panelRowMargin = PaddingValues(vertical = 1.dp)

/** The room under the end of what the right panel scrolls through, so that it never ends on the screen's edge. */
val panelEndRoom = 48.dp

/**
 * The agents the thread's agent has started, the ones at work first and the ones that ended folded
 * under them, and the thread whose agent started this one. An agent opens in the thread's place:
 * one started through your server in its own thread, any other read only.
 */
@Composable
fun AgentsSurface() {
    val store = LocalStore.current
    val agents = store.agents
    val parent = store.parentThread
    if (agents.isEmpty() && parent == null) return PanelMessage("The agents this thread starts show up here.")
    val palette = LocalPalette.current
    // Whether the agents that ended are shown, once the user has said. Until then they are while none works.
    var showsEnded by remember { mutableStateOf<Boolean?>(null) }
    val working = agents.filter { it.working }
    val ended = agents.filter { !it.working }
    val shown = showsEnded ?: working.isEmpty()
    Column(Modifier.fillMaxSize()) {
        PanelBar {
            Text(AgentsSummary.styled(agents, palette), Modifier.weight(1f), size = 12f, weight = FontWeight.Medium, color = palette.mutedForeground, maxLines = 1)
        }
        LazyColumn(
            Modifier.fillMaxSize(),
            contentPadding = PaddingValues(
                start = 8.dp, end = 8.dp,
                top = 8.dp - panelRowMargin.calculateTopPadding(),
                bottom = panelEndRoom - panelRowMargin.calculateBottomPadding(),
            ),
        ) {
            if (parent != null) item(key = "parent") { ParentRow(parent) }
            items(working, key = { it.id }) { AgentRow(it) }
            if (ended.isNotEmpty()) {
                item(key = "ended") { EndedHeader(ended, shown) { showsEnded = !shown } }
                if (shown) items(ended, key = { it.id }) { AgentRow(it) }
            }
        }
    }
}

object AgentsSummary {
    /** "1 waiting · 2 working · 1 done · 48k tokens", leaving out what there is none of, each count with its state. */
    fun parts(agents: List<AgentInfo>): List<Pair<String, AgentState?>> {
        if (agents.isEmpty()) return listOf("No agents" to null)
        val counts = listOf(AgentState.Waiting, AgentState.Working, AgentState.Limited, AgentState.Done, AgentState.Failed, AgentState.Stopped)
            .map { state -> agents.count { it.state == state } to state }
            .filter { it.first > 0 }
            .map { (count, state) -> "$count ${state.raw}" to state }
        val spent = agents.mapNotNull { it.tokens }
        val total = if (spent.isEmpty()) emptyList() else listOf(AgentInfo.tokens(spent.sum()) to null)
        return counts + total
    }

    /** The same, each count in its state's colour, between muted dots. */
    fun styled(agents: List<AgentInfo>, palette: Palette): AnnotatedString = buildAnnotatedString {
        parts(agents).forEachIndexed { index, (part, state) ->
            if (index > 0) withStyle(SpanStyle(color = palette.mutedStrongerForeground)) { append(" · ") }
            if (state == null) append(part) else withStyle(SpanStyle(color = state.color(palette))) { append(part) }
        }
    }
}

/** The thread whose agent started this one, which a tap opens. */
@Composable
private fun ParentRow(thread: ThreadInfo) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    HighlightButton(onClick = { store.openStartedThread(thread.id) }, modifier = Modifier.fillMaxWidth().padding(bottom = 4.dp), inset = panelRowMargin) {
        Row(
            Modifier.fillMaxWidth().padding(panelRowMargin).padding(horizontal = 10.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.width(16.dp), contentAlignment = Alignment.Center) { Icon(Symbol.CornerLeftUp, 12f, color = palette.foreground) }
            Text("Go back to parent thread", Modifier.weight(1f), size = 13f, weight = FontWeight.Medium, color = palette.foreground, maxLines = 1)
        }
    }
}

/** Over the agents that ended: how many, and how many of them failed. A tap folds them. */
@Composable
private fun EndedHeader(agents: List<AgentInfo>, shown: Boolean, toggle: () -> Unit) {
    val palette = LocalPalette.current
    val failed = agents.count { it.status == ToolStatus.Failed }
    HighlightButton(onClick = toggle, modifier = Modifier.fillMaxWidth().padding(top = 6.dp), inset = panelRowMargin) {
        Row(
            Modifier.fillMaxWidth().padding(panelRowMargin).height(ControlSize.Regular.height).padding(horizontal = 10.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Previous agents", size = 12f, weight = FontWeight.SemiBold, color = palette.foreground, maxLines = 1)
            Text("${agents.size}", size = 12f, color = palette.mutedStrongerForeground, maxLines = 1)
            if (failed > 0) Text("$failed failed", size = 12f, color = palette.destructive, maxLines = 1)
            Spacer(Modifier.weight(1f))
            Icon(if (shown) Symbol.ChevronDown else Symbol.ChevronRight, 10f, color = palette.mutedStrongerForeground)
        }
    }
}

/**
 * An agent: how it is doing, what it is doing or said last, and what it runs on and has spent. One
 * that works in a thread of its own can be stopped from the menu it holds.
 */
@Composable
private fun AgentRow(agent: AgentInfo) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val menu = rememberMenuState()
    val threadID = agent.threadID
    val open = {
        if (threadID != null) store.openStartedThread(threadID) else store.rightPanel.showAgent(agent.id)
    }
    // "6.1-Sol · work · 3 tools · 21k tokens", as far as each is known.
    val facts = listOfNotNull(agent.model, agent.account, agent.usage).joinToString(" · ").ifEmpty { null }
    Box(Modifier.fillMaxWidth().pressPosition(menu)) {
        HighlightButton(
            onClick = open, modifier = Modifier.fillMaxWidth(), inset = panelRowMargin,
            selected = store.rightPanel.shownAgent == agent.id, onLongClick = { menu.openAtPress() },
        ) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(panelRowMargin)
                    // An agent that another agent started stands in from it.
                    .padding(start = if (agent.parent == null) 0.dp else 18.dp)
                    .padding(horizontal = 10.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                AgentAvatar(agent.agent, agent.state)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        // The kind gives way to the title and its word.
                        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) {
                            Text(agent.title, size = 13f, weight = FontWeight.Medium, color = palette.foreground, maxLines = 1)
                            agent.kind?.let {
                                Text(it, Modifier.padding(start = 6.dp).weight(1f, fill = false), size = 11f, color = palette.mutedForeground, maxLines = 1)
                            }
                            if (agent.state.flagged) {
                                Text(agent.label, Modifier.padding(start = 6.dp), size = 11f, weight = FontWeight.Medium, color = agent.state.color, maxLines = 1)
                            }
                        }
                        AgentTime(agent, Modifier.padding(start = 6.dp))
                    }
                    // Always one line, so that a row keeps its height as what its agent does changes.
                    val color = if (agent.status == ToolStatus.Failed) palette.destructive else palette.mutedForeground
                    Text(agent.detail, size = 12f, color = color, maxLines = 1)
                    facts?.let { Text(it, size = 11f, color = palette.mutedStrongerForeground, maxLines = 1) }
                }
            }
        }
        MenuPopup(menu) {
            item(if (threadID == null) "Show What It Did" else "Open Thread", Symbol.ArrowRight, run = open)
            if (agent.working && threadID != null) item("Stop Agent", Symbol.CircleStop, destructive = true) { store.stop(threadID) }
        }
    }
}

/** How long the agent has worked so far, or worked in all, or until its usage limit stopped it. */
@Composable
fun AgentTime(agent: AgentInfo, modifier: Modifier = Modifier) {
    val palette = LocalPalette.current
    if (agent.working && agent.state != AgentState.Limited) {
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
