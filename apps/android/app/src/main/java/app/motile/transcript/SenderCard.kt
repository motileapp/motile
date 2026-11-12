package app.motile.transcript

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Agent
import app.motile.platform.Symbol
import app.motile.sidebar.CardLine
import app.motile.sidebar.PullRequestLine
import app.motile.sidebar.ThreadCard
import app.motile.sidebar.lastComponent
import app.motile.theme.TextSize
import app.motile.theme.Theme
import app.motile.ui.AgentIcon
import app.motile.ui.DropdownCard
import app.motile.ui.Icon
import app.motile.ui.ProjectIcon
import app.motile.ui.Text
import app.motile.ui.ThemeDivider

/** What the avatar beside a message the user didn't write opens: who sent it and what it did, in the look of the sidebar's thread card. `threadID` is the thread the message is in. */
@Composable
fun SenderCard(avatar: MessageAvatar, sender: MessageSender, threadID: String) {
    DropdownCard(Modifier.widthIn(max = ThreadCard.maxWidth)) {
        Column(Modifier.padding(12.dp)) {
            val agent = avatar.agent
            if (agent != null) AgentLines(agent, sender, threadID) else ServerLines(sender, threadID)
        }
    }
}

@Composable
private fun AgentLines(agent: Agent, sender: MessageSender, threadID: String) {
    val store = LocalStore.current
    val sending = store.anyThread(sender.threadID)
    Title(sending?.title ?: agent.title)
    Column(Modifier.padding(top = 10.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        val serverID = store.anyThread(threadID)?.serverID ?: ""
        CardLine(
            store.modelLabel(sender.model, sender.account, agent, serverID),
            proxied = store.modelProxied(sender.account, agent, serverID),
            pinned = store.modelPinned(sender.account, agent, serverID),
            wraps = true,
        ) { AgentIcon(agent, size = ThreadCard.iconSize) }
        if (sender.note.isNotEmpty()) {
            CardLine(sender.note, wraps = true) { Icon(Symbol.MessageSquareText, ThreadCard.iconSize) }
        }
        if (sending == null) {
            CardLine("Its thread was deleted", wraps = true) { Icon(Symbol.Trash2, ThreadCard.iconSize) }
            return@Column
        }
        val project = store.project(sending.projectID)?.seen(sending)
        CardLine(project?.name ?: lastComponent(sending.cwd), wraps = true) { ProjectIcon(project, size = ThreadCard.iconSize) }
    }
}

@Composable
private fun ColumnScope.ServerLines(sender: MessageSender, threadID: String) {
    val store = LocalStore.current
    val palette = Theme.colors
    val thread = store.anyThread(threadID)
    val server = thread?.let { store.server(it.serverID) }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        Icon(Symbol.Server, ThreadCard.iconSize, color = palette.foreground)
        Title(server?.name ?: "The server")
    }
    Text(sender.note, size = TextSize.Xs, color = palette.mutedForeground, modifier = Modifier.padding(top = 4.dp))
    if (!sender.pullRequest || thread == null) return
    val pullRequest = store.project(thread.projectID)?.seen(thread)?.pullRequest(thread) ?: return
    ThemeDivider(Modifier.padding(vertical = 10.dp))
    PullRequestLine(pullRequest, wraps = true)
}

@Composable
private fun Title(text: String) {
    Text(text, size = TextSize.Sm, weight = FontWeight.Medium, color = Theme.colors.foreground)
}
