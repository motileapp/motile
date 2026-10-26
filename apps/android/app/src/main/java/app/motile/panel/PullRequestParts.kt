package app.motile.panel

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.motile.DemoHooks
import app.motile.LocalStore
import app.motile.core.PullRequest
import app.motile.core.objects
import app.motile.layout.KeyboardFocus
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.iconSize
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Radius
import app.motile.theme.SurfaceBox
import app.motile.theme.Theme
import app.motile.theme.wash
import app.motile.ui.ActionButton
import app.motile.ui.ActionMenu
import app.motile.ui.ButtonVariant
import app.motile.ui.Chip
import app.motile.ui.Icon
import app.motile.ui.LocalCloseModal
import app.motile.ui.Segmented
import app.motile.ui.Text
import app.motile.ui.TextArea

/** The look of a button as the core names it. */
fun buttonVariant(style: String): ButtonVariant = when (style) {
    "primary" -> ButtonVariant.Primary
    "danger" -> ButtonVariant.Danger
    else -> ButtonVariant.Secondary
}

val PullRequest.State.title: String
    get() = when (this) {
        PullRequest.State.Open -> "Open"
        PullRequest.State.Draft -> "Draft"
        PullRequest.State.Merged -> "Merged"
        PullRequest.State.Closed -> "Closed"
    }

/** Lets the demo give the tab's search field the keyboard and type in it, while the tab is shown. */
@Composable
internal fun DemoSearch(focus: FocusRequester, type: (String) -> Unit) {
    val typing by rememberUpdatedState(type)
    DisposableEffect(focus) {
        val hook: (String) -> Unit = { text ->
            focus.requestFocus()
            if (text.isNotEmpty()) typing(text)
        }
        DemoHooks.search = hook
        onDispose { if (DemoHooks.search === hook) DemoHooks.search = null }
    }
}

@Composable
fun BranchName(name: String, modifier: Modifier = Modifier) {
    Chip(name, modifier, monospaced = true)
}

/** A reviewer, with a dot in the colour of their latest verdict. */
@Composable
fun ReviewerChip(reviewer: PullRequestPage.Reviewer) {
    val dot = if (reviewer.tone == Tone.Neutral) LocalPalette.current.mutedStrongerForeground else reviewer.tone.color
    Chip(reviewer.name, Modifier.semantics { contentDescription = "${reviewer.name}: ${reviewer.label}" }, dot = dot)
}

/** The reactions to a comment or a review, each one a toggle, and a menu to add another. */
@Composable
fun ReactionBar(reactions: List<PullRequestPage.Reaction>, react: (String, Boolean) -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
        for (reaction in reactions) {
            ActionButton(
                "${reaction.emoji} ${reaction.count}", help = if (reaction.mine) "Take your reaction back" else "React so too",
                variant = if (reaction.mine) ButtonVariant.Accent else ButtonVariant.Secondary, size = ControlSize.Small, round = true,
            ) { react(reaction.kind, !reaction.mine) }
        }
        ReactionPicker(reactions, ControlSize.Small, react)
    }
}

/** The menu of GitHub's reactions, to add one or take it back. */
@Composable
fun ReactionPicker(reactions: List<PullRequestPage.Reaction>, size: ControlSize = ControlSize.Regular, react: (String, Boolean) -> Unit) {
    ActionMenu(icon = Symbol.SmilePlus, help = "Add a reaction", size = size) {
        for ((kind, emoji) in PullRequestPage.Reaction.all) {
            val mine = reactions.firstOrNull { it.kind == kind }?.mine ?: false
            item("$emoji  ${if (mine) "Take Back" else "React"}") { react(kind, !mine) }
        }
    }
}

/** What the last action did or why it couldn't, under the tab's bar. */
@Composable
fun PullRequestNoticeBar(notice: PullRequestNotice) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val tint = if (notice.failed) palette.destructive else palette.success
    val line = scaled(17f).dp
    Column {
        Row(
            Modifier.fillMaxWidth().background(tint.wash()).padding(start = 12.dp, end = 6.dp, top = 8.dp, bottom = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Box(Modifier.height(line), contentAlignment = Alignment.Center) {
                Icon(if (notice.failed) Symbol.CircleAlert else Symbol.CircleCheck, iconSize(text = 12.5f), color = tint)
            }
            SelectionContainer(Modifier.weight(1f).heightIn(min = line)) {
                Text(notice.text, size = 12.5f, color = palette.foreground)
            }
            Row(Modifier.height(line), horizontalArrangement = Arrangement.spacedBy(2.dp), verticalAlignment = Alignment.CenterVertically) {
                notice.url?.let { url -> ActionButton("Open", variant = ButtonVariant.Link, size = ControlSize.Small) { Platform.open(url) } }
                ActionButton(icon = Symbol.X, help = "Close", size = ControlSize.Small) { store.rightPanel.pullRequests.dismissNotice() }
            }
        }
        PanelLine()
    }
}

/** Markdown from the pull request, drawn as the transcript draws a reply. */
@Composable
fun PullRequestTextView(blocks: List<PullRequestText>, modifier: Modifier = Modifier) {
    SelectionContainer(modifier) {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            for (block in blocks) {
                when (block) {
                    is PullRequestText.Prose -> ProseView(block.parts)
                    is PullRequestText.Code -> CodeBox(block.text)
                }
            }
        }
    }
}

@Composable
private fun CodeBox(text: androidx.compose.ui.text.AnnotatedString, modifier: Modifier = Modifier) {
    SurfaceBox(RoundedCornerShape(Radius.lg), modifier.fillMaxWidth()) {
        Text(text, Modifier.padding(10.dp), style = TextStyle(lineHeight = Theme.codeLineHeight * (PullRequestText.SIZE / 14f)))
    }
}

/** A stretch of prose: its paragraphs, lists, quotes, rules, tables and the code inside them. */
@Composable
private fun ProseView(parts: List<ProsePart>) {
    val palette = LocalPalette.current
    Column(Modifier.fillMaxWidth()) {
        for (part in parts) {
            val top = Modifier.padding(top = part.above.dp)
            when (part) {
                is ProsePart.Paragraph -> Row(top.quoteBars(part.quote, palette.border)) {
                    Spacer(Modifier.width(part.indent.dp))
                    part.marker?.let { marker ->
                        Text(marker, Modifier.width(22.dp).padding(start = 4.dp), style = TextStyle(lineHeight = part.lineHeight), maxLines = 1, overflow = TextOverflow.Visible)
                    }
                    Text(part.text, Modifier.weight(1f), style = TextStyle(lineHeight = part.lineHeight))
                }
                is ProsePart.Rule -> Box(top.padding(vertical = 4.dp).fillMaxWidth().height(1.dp).background(palette.border))
                is ProsePart.Code -> CodeBox(part.text, top.padding(start = part.indent.dp))
                is ProsePart.Table -> TableView(part, top)
            }
        }
    }
}

/** Draws a bar beside the paragraph for each quote it is in. */
private fun Modifier.quoteBars(depth: Int, color: androidx.compose.ui.graphics.Color): Modifier {
    if (depth <= 0) return this
    return padding(start = (depth * 14).dp).drawBehind {
        val step = 14.dp.toPx()
        val width = 3.dp.toPx()
        for (level in 0 until depth) {
            drawRect(color, Offset(-step * (depth - level), 0f), Size(width, size.height))
        }
    }
}

@Composable
private fun TableView(table: ProsePart.Table, modifier: Modifier) {
    val surface = LocalSurface.current
    val shape = RoundedCornerShape(Radius.md)
    Box(modifier.fillMaxWidth().horizontalScroll(rememberScrollState())) {
        Column(Modifier.width(IntrinsicSize.Max).border(1.dp, surface.border, shape)) {
            for ((index, row) in table.rows.withIndex()) {
                if (index > 0) Box(Modifier.fillMaxWidth().height(1.dp).background(surface.border))
                Row {
                    for (cell in row) {
                        Text(cell.text, Modifier.width(160.dp).padding(horizontal = 8.dp, vertical = 5.dp), align = cell.align)
                    }
                }
            }
        }
    }
}

/** The sheet that edits the description, with how it will look. */
@Composable
fun DescriptionEditor(original: String, save: (String) -> Unit) {
    val store = LocalStore.current
    val close = LocalCloseModal.current
    var text by remember { mutableStateOf(original) }
    var previewing by remember { mutableStateOf(false) }
    var preview by remember { mutableStateOf<List<PullRequestText>>(emptyList()) }
    LaunchedEffect(previewing) {
        if (!previewing) return@LaunchedEffect
        store.core.send("markdown", mapOf("text" to text), read = { PullRequestText.blocks(it.objects("blocks")) }) { result ->
            result.getOrNull()?.let { preview = it }
        }
    }
    Column(Modifier.fillMaxSize().navigationBarsPadding().padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Edit Description", Modifier.weight(1f), size = 15f, weight = FontWeight.SemiBold)
            Segmented(listOf("Write" to false, "Preview" to true), previewing, { previewing = it })
        }
        if (previewing) {
            SurfaceBox(RoundedCornerShape(Radius.md), Modifier.weight(1f).fillMaxWidth()) {
                Column(Modifier.verticalScroll(rememberScrollState()).padding(12.dp)) {
                    if (text.isBlank()) {
                        Text("Nothing to preview", size = 12.5f, color = LocalPalette.current.mutedStrongerForeground)
                    } else {
                        PullRequestTextView(preview)
                    }
                }
            }
        } else {
            TextArea("Say what this changes and why", text, { text = it }, Modifier.weight(1f).fillMaxWidth(), lines = 4, fills = true)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Spacer(Modifier.weight(1f))
            ActionButton("Cancel") { close() }
            ActionButton("Save", variant = ButtonVariant.Primary, disabled = text == original) {
                save(text)
                close()
            }
        }
    }
}
