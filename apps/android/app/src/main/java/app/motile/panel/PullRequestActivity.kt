package app.motile.panel

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import app.motile.core.Time
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.Radius
import app.motile.theme.SurfaceBox
import app.motile.theme.Theme
import app.motile.theme.wash
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.Chip
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.Text
import app.motile.ui.TextArea
import app.motile.ui.pressable

private val lineHeight get() = scaled(17f).dp

/** "Author said · 2h": who did something, what, and when. */
@Composable
internal fun byline(author: String, said: String?, at: Double) = buildAnnotatedString {
    val palette = LocalPalette.current
    withStyle(SpanStyle(fontWeight = FontWeight.SemiBold, color = palette.foreground)) { append(author) }
    if (said != null) withStyle(SpanStyle(color = palette.mutedForeground)) { append(if (author.isEmpty()) said else " $said") }
    withStyle(SpanStyle(color = palette.mutedStrongerForeground)) { append(" · ${Time.ago(at)}") }
}

/**
 * One thing that happened on the pull request: its opening with the description, commits,
 * comments, reviews and conversations on lines, and how it ended.
 */
@Composable
fun ActivityRow(entry: PullRequestPage.Entry, actions: PullRequestActions, modifier: Modifier = Modifier) {
    val palette = LocalPalette.current
    val symbol = when {
        entry.kind == "opened" -> Symbol.GitPullRequest
        entry.kind == "commits" -> Symbol.GitCommitHorizontal
        entry.kind == "merged" -> Symbol.GitMerge
        entry.kind == "closed" -> Symbol.GitPullRequestClosed
        entry.kind == "review" && entry.tone == Tone.Success -> Symbol.CircleCheck
        entry.kind == "review" && entry.tone == Tone.Danger -> Symbol.CircleAlert
        entry.kind == "review" -> Symbol.Eye
        entry.kind == "thread" -> Symbol.Code
        else -> Symbol.MessageSquareText
    }
    Row(modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Box(Modifier.size(width = 18.dp, height = lineHeight), contentAlignment = Alignment.Center) {
            Icon(symbol, 12f, color = if (entry.tone == Tone.Neutral) palette.mutedForeground else entry.tone.color)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Byline(entry, actions)
            if (entry.commits.isNotEmpty()) Commits(entry.commits, actions)
            val thread = entry.thread
            if (thread != null) {
                ConversationCard(thread, actions)
            } else if (entry.body.isNotEmpty()) {
                SurfaceBox(RoundedCornerShape(Radius.lg), Modifier.padding(top = 1.dp, bottom = 2.dp).fillMaxWidth()) {
                    PullRequestTextView(entry.body, Modifier.padding(12.dp))
                }
            }
            val subject = entry.subject
            if (subject != null && thread == null && entry.reactions.isNotEmpty()) {
                ReactionBar(entry.reactions) { kind, on -> actions.react(subject, kind, on) }
            }
        }
    }
}

@Composable
private fun Byline(entry: PullRequestPage.Entry, actions: PullRequestActions) {
    Row(verticalAlignment = Alignment.Top) {
        Text(byline(entry.author, entry.said, entry.at), Modifier.weight(1f), size = 12.5f)
        Spacer(Modifier.width(6.dp))
        Row(Modifier.height(lineHeight), verticalAlignment = Alignment.CenterVertically) {
            val subject = entry.subject
            if (subject != null && entry.thread == null && entry.reactions.isEmpty()) {
                ReactionPicker(emptyList()) { kind, on -> actions.react(subject, kind, on) }
            }
            val edit = actions.editDescription
            if (entry.kind == "opened" && edit != null) {
                ActionButton(icon = Symbol.Pencil, help = "Edit the description", pending = actions.working?.key == "menu:body", onClick = edit)
            }
            entry.url?.let { url -> ActionButton(icon = Symbol.SquareArrowOutUpRight, help = "Open on GitHub") { Platform.open(url) } }
        }
    }
}

/** The commits, each one opening what it changed. */
@Composable
private fun Commits(commits: List<PullRequestPage.Commit>, actions: PullRequestActions) {
    val palette = LocalPalette.current
    Column(Modifier.offset(x = (-6).dp).padding(vertical = 0.dp)) {
        for (commit in commits) {
            HighlightButton(
                onClick = { actions.showCommit(commit.sha) },
                modifier = Modifier.fillMaxWidth().semantics { contentDescription = "Show what this commit changed" },
                enabled = commit.sha.isNotEmpty(),
                radius = Radius.sm,
            ) {
                Row(
                    Modifier.height(pressable(24f)).padding(horizontal = 6.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(commit.oid, style = Theme.uiMono(11.5f), color = palette.primary)
                    Text(commit.headline, Modifier.weight(1f), size = 12.5f, color = palette.foreground, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

/** A conversation on a line: the line it is on, what was said, and a reply. A resolved one is folded until it is opened. */
@Composable
fun ConversationCard(thread: PullRequestPage.Thread, actions: PullRequestActions) {
    var open by rememberSaveable(thread.id) { mutableStateOf<Boolean?>(null) }
    val isOpen = open ?: !thread.resolved
    SurfaceBox(RoundedCornerShape(Radius.lg), Modifier.fillMaxWidth()) {
        Column {
            ConversationHeader(thread, isOpen) { open = !isOpen }
            if (isOpen) {
                if (thread.hunk.isNotEmpty()) HunkView(thread.hunk)
                for (comment in thread.comments) {
                    PanelLine()
                    CommentView(comment, actions.react)
                }
                PanelLine()
                ConversationFooter(thread, actions)
            }
        }
    }
}

@Composable
private fun ConversationHeader(thread: PullRequestPage.Thread, isOpen: Boolean, toggle: () -> Unit) {
    val palette = LocalPalette.current
    HighlightButton(onClick = toggle, modifier = Modifier.fillMaxWidth(), radius = 0.dp) {
        Row(
            Modifier.height(pressable(32f)).padding(horizontal = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.width(10.dp), contentAlignment = Alignment.Center) {
                Icon(if (isOpen) Symbol.ChevronDown else Symbol.ChevronRight, 8f, color = palette.mutedStrongerForeground)
            }
            Icon(FileSymbol.symbol(thread.path), 11f, color = palette.mutedForeground)
            Text(
                thread.path, Modifier.weight(1f, fill = false), size = 12f, weight = FontWeight.Medium, color = palette.foreground,
                maxLines = 1, overflow = TextOverflow.StartEllipsis,
            )
            thread.line?.let { Text("line $it", size = 12f, color = palette.mutedStrongerForeground, maxLines = 1) }
            if (thread.outdated) Chip("Outdated", tone = palette.warning)
            if (thread.resolved) Chip("Resolved", tone = palette.success)
            Spacer(Modifier.weight(1f))
            if (!isOpen) {
                val count = thread.comments.size
                Text(if (count == 1) "1 comment" else "$count comments", size = 11.5f, color = palette.mutedStrongerForeground, maxLines = 1)
            }
        }
    }
}

@Composable
private fun ConversationFooter(thread: PullRequestPage.Thread, actions: PullRequestActions) {
    var replying by rememberSaveable(thread.id) { mutableStateOf(false) }
    var reply by rememberSaveable(thread.id) { mutableStateOf("") }
    val busy = actions.working != null
    if (replying) {
        Column(Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            TextArea("Reply", reply, { reply = it }, Modifier.fillMaxWidth().editsText())
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Spacer(Modifier.weight(1f))
                ActionButton("Cancel") {
                    replying = false
                    reply = ""
                }
                ActionButton(
                    "Reply", variant = ButtonVariant.Primary, pending = actions.working?.key == "reply:${thread.id}",
                    disabled = busy || reply.isBlank(),
                ) {
                    actions.reply(thread.id, reply.trim()) {
                        replying = false
                        reply = ""
                    }
                }
            }
        }
        return
    }
    Row(Modifier.padding(10.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
        ActionButton("Reply") { replying = true }
        thread.fix?.let { fix -> ActionButton("Fix", help = "Have the agent do what it asks") { actions.handOff(fix) } }
        Spacer(Modifier.weight(1f))
        if (thread.canResolve) {
            ActionButton(
                if (thread.resolved) "Unresolve" else "Resolve", pending = actions.working?.key == "resolve:${thread.id}", disabled = busy,
            ) { actions.resolve(thread.id, !thread.resolved) }
        }
    }
}

/** The lines of the diff a conversation was written under, the line itself last. */
@Composable
private fun HunkView(lines: List<String>) {
    val palette = LocalPalette.current
    Column(Modifier.padding(bottom = 6.dp)) {
        lines.forEachIndexed { index, line ->
            val lit = index == lines.size - 1
            // The line's wash, lit on the last line, the one the comment is on.
            val fill = when {
                line.startsWith("+") -> palette.success.wash(lit)
                line.startsWith("-") -> palette.destructive.wash(lit)
                else -> androidx.compose.ui.graphics.Color.Transparent
            }
            Box(Modifier.fillMaxWidth().heightIn(min = 18.dp).background(fill).padding(horizontal = 12.dp), contentAlignment = Alignment.CenterStart) {
                Text(line.ifEmpty { " " }, style = Theme.uiMono(11.5f), color = palette.foreground, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

/** A comment in a conversation on a line. */
@Composable
private fun CommentView(comment: PullRequestPage.Comment, react: (subject: String, kind: String, on: Boolean) -> Unit) {
    Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(byline(comment.author, null, comment.at), Modifier.weight(1f), size = 12.5f)
            Row(Modifier.offset(x = 6.dp).height(lineHeight), verticalAlignment = Alignment.CenterVertically) {
                if (comment.reactions.isEmpty()) ReactionPicker(emptyList(), ControlSize.Small) { kind, on -> react(comment.id, kind, on) }
                comment.url?.let { url ->
                    ActionButton(icon = Symbol.SquareArrowOutUpRight, help = "Open on GitHub", size = ControlSize.Small) { Platform.open(url) }
                }
            }
        }
        PullRequestTextView(comment.body)
        if (comment.reactions.isNotEmpty()) ReactionBar(comment.reactions) { kind, on -> react(comment.id, kind, on) }
    }
}
