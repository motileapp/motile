package app.motile.composer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.AgentInfo
import app.motile.core.ThreadChildren
import app.motile.panel.AgentTime
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.ShadowSize
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.AgentAvatar
import app.motile.ui.ButtonVariant
import app.motile.ui.Icon
import app.motile.ui.Text

/** Above the composer of a thread whose agent started agents that wait for the user: which, and the way to the first of them, where it is answered. */
@Composable
fun ChildrenStrip(text: String, first: ThreadChildren.Waiting, count: Int) {
    val store = LocalStore.current
    val palette = Theme.colors
    Row(Modifier.fillMaxWidth().composerStrip(ComposerStrip.Edge.Top), verticalAlignment = Alignment.CenterVertically) {
        Icon(first.ask.symbol, 13f, Modifier.padding(start = 14.dp, end = 7.dp), color = palette.warning)
        // A narrow composer says what is asked without naming the agent.
        val said = @Composable { words: String -> Text(words, size = 12.5f, weight = FontWeight.Medium, color = palette.warning, maxLines = 1) }
        FirstThatFits(
            Modifier.weight(1f).padding(end = 8.dp).semantics { contentDescription = text },
            { said(text) },
            { said(if (count == 1) "Agent ${first.ask.phrase}" else text) },
        )
        ActionButton("Open ${first.ask.word}", help = "Open ${first.title}", variant = ButtonVariant.Ghost, margin = ComposerStrip.margin) {
            store.openStartedThread(first.threadID)
        }
    }
}

/** In place of the composer of an agent the thread's agent started, which runs on its own and can't be written to: how it is doing, and the way back to the thread. */
/** What the bar says stands twice its gap from the way back. */
private val BACK_APART = 8.dp

@Composable
fun AgentBar(agent: AgentInfo, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val palette = Theme.colors
    val height = ControlSize.Large.height + 8.dp
    Row(
        modifier
            .widthIn(max = Theme.composerWidth)
            .fillMaxWidth()
            // A pill, whose round avatar and round button keep as far from its top and bottom as from its ends, so that their curves follow its own.
            .composerSurface(RoundedCornerShape(percent = 50), ShadowSize.Md)
            .height(height)
            .padding(start = (height - AgentAvatar.side) / 2, end = (height - ControlSize.Regular.height) / 2),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AgentAvatar(agent.agent, agent.state)
        val title = @Composable { modifier: Modifier ->
            Text(agent.title, modifier, size = 13f, weight = FontWeight.Medium, color = palette.foreground, maxLines = 1)
        }
        // A narrow bar leaves out that the agent runs on its own.
        FirstThatFits(
            Modifier.weight(1f),
            {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    title(Modifier)
                    AgentTime(agent)
                    Spacer(Modifier.weight(1f))
                    Text("Runs on its own", Modifier.padding(end = BACK_APART), size = 12.5f, color = palette.mutedForeground, maxLines = 1)
                }
            },
            {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    title(Modifier.weight(1f, fill = false))
                    AgentTime(agent)
                }
            },
        )
        // The way back's arrow stands close to its words, as the crumb's does.
        ActionButton("Go Back", Symbol.CornerLeftUp, help = "Back to the thread", variant = ButtonVariant.Primary, gap = 4.dp, round = true) { store.rightPanel.showAgents() }
    }
}

/** The thread before a thread's title, as a button that opens it: the thread whose agent started the open one, or the thread of the agent shown. */
@Composable
fun TitleCrumb(title: String, modifier: Modifier = Modifier, compact: Boolean = false, action: () -> Unit) {
    // Compact, where the title has no room, it is only the symbol.
    val symbol = ControlSize.Small.smallSymbol
    if (compact) return ActionButton(icon = Symbol.CornerLeftUp, modifier = modifier, help = "Open $title", size = ControlSize.Small, symbolSize = symbol, inline = true, onClick = action)
    ActionButton(title, Symbol.CornerLeftUp, modifier, help = "Open $title", variant = ButtonVariant.Ghost, size = ControlSize.Small, symbolSize = symbol, inline = true, onClick = action)
}
