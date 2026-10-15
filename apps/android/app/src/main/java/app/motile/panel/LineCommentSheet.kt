package app.motile.panel

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.string
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.Radius
import app.motile.theme.SurfaceBox
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.Icon
import app.motile.ui.LocalCloseModal
import app.motile.ui.Text
import app.motile.ui.TextArea

/**
 * The sheet that comments on a line of a pull request's diff: what was said there already, to
 * reply to, and a comment to keep for the review or to hand to the agent.
 */
@Composable
fun LineCommentSheet(target: PanelTarget, page: PullRequestPage, commented: CommentableLine) {
    val store = LocalStore.current
    val close = LocalCloseModal.current
    val palette = LocalPalette.current
    val state = store.rightPanel.pullRequests
    var text by rememberSaveable { mutableStateOf("") }
    val written = text.trim()
    val threads = page.threads.filter { it.path == commented.path && it.line == commented.line && it.side == commented.side }

    val addToReview = {
        state.addPending(PendingLineComment(commented.path, commented.line, commented.side, written), page.number)
        close()
    }
    val askAgent = {
        val fields = mapOf(
            "number" to page.number, "url" to page.url, "head" to page.head, "path" to commented.path,
            "line" to commented.line, "code" to commented.code, "note" to written,
        )
        store.core.send("line_prompt", fields) { result ->
            result.getOrNull()?.let { store.handOff(it.string("prompt")) }
        }
        close()
    }

    Column(
        Modifier.fillMaxSize().navigationBarsPadding().padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(FileSymbol.symbol(commented.path), 12f, color = palette.mutedForeground)
                Text(
                    commented.path, Modifier.weight(1f, fill = false), size = 13f, weight = FontWeight.SemiBold,
                    maxLines = 1, overflow = TextOverflow.StartEllipsis,
                )
                Text("line ${commented.line}", size = 12.5f, color = palette.mutedStrongerForeground, maxLines = 1)
            }
            SurfaceBox(RoundedCornerShape(Radius.md), Modifier.fillMaxWidth()) {
                Text(
                    commented.code.ifEmpty { " " }, Modifier.padding(horizontal = 10.dp, vertical = 7.dp),
                    style = Theme.uiMono(12f), color = palette.foreground, maxLines = 3,
                )
            }
        }
        if (threads.isNotEmpty()) {
            SurfaceBox(RoundedCornerShape(Radius.lg), Modifier.fillMaxWidth().heightIn(max = 220.dp)) {
                Column(Modifier.verticalScroll(rememberScrollState()).padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    for (thread in threads) {
                        for (comment in thread.comments) {
                            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                Text(byline(comment.author, null, comment.at), size = 12.5f)
                                PullRequestTextView(comment.body)
                            }
                        }
                    }
                }
            }
        }
        TextArea(
            if (threads.isEmpty()) "Comment on this line" else "Reply, or say something new", text, { text = it },
            Modifier.fillMaxWidth(), lines = 5,
        )
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            ActionButton("Add to Review", variant = ButtonVariant.Primary, size = ControlSize.Large, fills = true, disabled = written.isEmpty(), onClick = addToReview)
            threads.lastOrNull()?.let { thread ->
                ActionButton("Reply", size = ControlSize.Large, fills = true, disabled = written.isEmpty()) {
                    state.edit(mapOf("kind" to "reply", "thread" to thread.id, "body" to written), target = target, number = page.number)
                    close()
                }
            }
            ActionButton("Ask the Agent", size = ControlSize.Large, fills = true, disabled = written.isEmpty(), onClick = askAgent)
            ActionButton("Cancel", variant = ButtonVariant.Ghost, size = ControlSize.Large, fills = true) { close() }
        }
    }
}

