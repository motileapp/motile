package app.motile.transcript

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.layout.layout
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.motile.core.Activity
import app.motile.core.AgentState
import app.motile.core.SettingsSection
import app.motile.core.Time
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.panel.FileSymbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Palette
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.theme.SurfaceBox
import app.motile.theme.Theme
import app.motile.theme.opacity
import app.motile.theme.wash
import app.motile.ui.AgentAvatar
import app.motile.ui.AgentAvatars
import app.motile.ui.AgentIcon
import app.motile.ui.CopyButton
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.Text
import app.motile.ui.color
import kotlinx.coroutines.delay

/** The view of one row, by its kind. */
@Composable
fun TranscriptRow(row: RowModel) {
    when (val kind = row.kind) {
        is RowModel.Kind.User -> UserRow(row, kind)
        is RowModel.Kind.Queued -> QueuedRow(row, kind.content)
        is RowModel.Kind.Prose -> ProseRow(row, kind.content)
        is RowModel.Kind.Code -> CodeRow(kind.content)
        is RowModel.Kind.Tool, is RowModel.Kind.Thinking, is RowModel.Kind.Group, is RowModel.Kind.Fold -> ToolRow(row)
        is RowModel.Kind.Agent -> AgentRow(row, kind.content)
        is RowModel.Kind.Agents -> AgentsRow(row, kind.content)
        is RowModel.Kind.Media -> MediaRow(kind.content)
        is RowModel.Kind.File -> FileRow(kind.content)
        is RowModel.Kind.Error -> ErrorRow(kind.message)
        is RowModel.Kind.Changes -> ChangesRow(row, kind.content)
        is RowModel.Kind.TurnEnd -> TurnEndRow(row, kind.content)
        is RowModel.Kind.Handoff -> HandoffRow(kind.content)
        is RowModel.Kind.Removed -> RemovedRow(kind.content)
    }
}

/** The time now, again every second while `running`. */
@Composable
fun ticking(running: Boolean): Double {
    val now by produceState(Time.now(), running) {
        value = Time.now()
        while (running) {
            delay(1000)
            value = Time.now()
        }
    }
    return now
}

private object Bubble {
    const val PADDING = 14f
    val radius = 18.dp
}

/** A bubble is as wide as what it holds, up to the widest it may be: most of the column, and never too narrow for a word. */
private fun Modifier.bubbleWidth(): Modifier = layout { measurable, constraints ->
    val widest = maxOf(120.dp.roundToPx(), (constraints.maxWidth * 0.8f).toInt()).coerceAtMost(constraints.maxWidth)
    val placeable = measurable.measure(constraints.copy(minWidth = 0, maxWidth = widest))
    layout(placeable.width, placeable.height) { placeable.place(0, 0) }
}

private val messageStyle: TextStyle get() = TextStyle(fontSize = Theme.proseSize, lineHeight = Typesetter.messageLineHeight)

@Composable
private fun UserRow(row: RowModel, user: RowModel.Kind.User) {
    val palette = LocalPalette.current
    val text = remember(user.text, palette) { user.text.styled.get(palette) }
    val pending = row.id == RowModel.pendingID
    val hasText = text.isNotEmpty()
    val hasFiles = user.attachments.isNotEmpty()
    Column(Modifier.fillMaxWidth().padding(top = 14.dp), horizontalAlignment = Alignment.End) {
        val shape = RoundedCornerShape(Bubble.radius)
        Column(
            Modifier
                .bubbleWidth()
                .opacity(Opacity.Disabled, on = pending)
                .clip(shape)
                .background(palette.backgroundSecondary, shape)
                .padding(
                    start = Bubble.PADDING.dp, end = Bubble.PADDING.dp,
                    top = if (hasFiles) Bubble.PADDING.dp else 10.dp, bottom = if (hasText) 10.dp else Bubble.PADDING.dp,
                ),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            CompositionLocalProvider(LocalSurface provides Surface.Secondary) {
                if (hasFiles) AttachedFiles(user.attachments)
                if (hasText) SelectionContainer { MarkedText(text, messageStyle) }
            }
        }
        MessageMeta(Time.stamp(user.at), trailing = true, help = "Copy message", Modifier.padding(top = 4.dp, bottom = 25.dp)) {
            Platform.copy(user.text.text)
        }
    }
}

/**
 * A message that waits for the agent: what it says, how it waits, and the buttons that steer the
 * turn with it or take it back into the composer. It stands where the user's messages do, dotted
 * around instead of filled.
 */
@Composable
private fun QueuedRow(row: RowModel, queued: QueuedContent) {
    val palette = LocalPalette.current
    val actions = LocalTranscript.current
    val text = remember(queued.text, palette) { queued.text.styled.get(palette) }
    val hasText = text.isNotEmpty()
    val hasFiles = queued.attachments.isNotEmpty()
    val waiting = row.id == RowModel.pendingID
    Box(Modifier.fillMaxWidth().padding(vertical = 14.dp), contentAlignment = Alignment.TopEnd) {
        Column(Modifier.bubbleWidth().dottedBorder(palette.border, Radius.xl)) {
            if (hasText || hasFiles) {
                Column(
                    Modifier.padding(start = Bubble.PADDING.dp, end = Bubble.PADDING.dp, top = if (hasFiles) Bubble.PADDING.dp else 10.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    if (hasFiles) AttachedFiles(queued.attachments)
                    if (hasText) SelectionContainer { MarkedText(text, messageStyle) }
                }
            }
            Row(Modifier.padding(start = Bubble.PADDING.dp, top = 14.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(Symbol.Clock, 11f, Modifier.padding(end = 4.dp), color = palette.mutedForeground)
                Text(queued.status, Modifier.weight(1f, fill = false).padding(end = 6.dp), style = Theme.smallFont, color = palette.mutedForeground, maxLines = 1)
                RowButton("Steer", "Send as a steer instead", bordered = true, margin = PaddingValues(start = 3.dp, end = 3.dp, top = 4.dp, bottom = 10.dp)) {
                    if (!waiting) actions.store.sendNow(row.itemID)
                }
                RowButton("Edit", "Edit in the composer", bordered = true, margin = PaddingValues(start = 3.dp, end = 10.dp, top = 4.dp, bottom = 10.dp)) {
                    if (!waiting) actions.store.takeBack(row.itemID)
                }
            }
        }
    }
}

@Composable
private fun ProseRow(row: RowModel, prose: ProseContent) {
    val palette = LocalPalette.current
    val blocks = remember(prose, palette) { prose.blocks.get(palette) }
    SelectionContainer {
        Column(Modifier.fillMaxWidth().padding(top = (3 + row.above).dp, bottom = (Typesetter.PROSE_GAP - 3).dp)) {
            for (block in blocks) ProseBlockView(block)
        }
    }
}

@Composable
private fun ProseBlockView(block: ProseBlock) {
    val palette = LocalPalette.current
    val bars = Modifier.quoteBars(block.quote, palette.border)
    val placed = if (block.joined) bars.padding(top = block.top.dp) else Modifier.padding(top = block.top.dp).then(bars)
    when (block) {
        is TextBlock -> {
            val style = TextStyle(lineHeight = block.lineHeight)
            Row(placed.fillMaxWidth().padding(start = (if (block.marker != null) block.markerIndent else block.indent).dp)) {
                block.marker?.let { marker ->
                    Box(Modifier.width((block.indent - block.markerIndent).dp)) { MarkedText(marker, style) }
                }
                MarkedText(block.text, style)
            }
        }
        is CodeBlock -> Box(placed.fillMaxWidth().padding(start = block.indent.dp)) { CodeBox(block.language, block.code, block.text) }
        is RuleBlock -> Box(
            placed.fillMaxWidth().height(Theme.proseLineHeight.value.dp).drawBehind {
                drawRect(palette.border, Offset(0f, size.height / 2), Size(size.width, 1.dp.toPx()))
            },
        )
        is TableBlock -> TableView(block, placed)
    }
}

/** Code in a box with its language and a button that copies it, which scrolls sideways when it is wider than the column. */
@Composable
private fun CodeBox(language: String, code: String, text: AnnotatedString, modifier: Modifier = Modifier) {
    SurfaceBox(RoundedCornerShape(Radius.lg), modifier.fillMaxWidth(), bordered = true) {
        Column {
            CodeHeader(language, code)
            BasicText(
                text,
                Modifier.horizontalScroll(rememberScrollState()).padding(start = 14.dp, end = 14.dp, bottom = 12.dp),
                style = TextStyle(fontFamily = Theme.mono, fontSize = Theme.codeSize, lineHeight = Theme.codeLineHeight),
                softWrap = false,
            )
        }
    }
}

@Composable
private fun CodeRow(content: CodeContent) {
    val palette = LocalPalette.current
    val typeset = content.text
    val text = remember(typeset, palette) { typeset.get(palette) }
    SelectionContainer {
        CodeBox(content.language, content.code, text, Modifier.padding(top = 4.dp, bottom = 10.dp))
    }
}

/** A tool call, the agent's thinking, a group of calls or a finished turn's fold: one line, which opens. */
@Composable
private fun ToolRow(row: RowModel) {
    val palette = LocalPalette.current
    val actions = LocalTranscript.current
    val kind = row.kind
    val tool = (kind as? RowModel.Kind.Tool)?.content
    val group = (kind as? RowModel.Kind.Group)?.content
    val fold = (kind as? RowModel.Kind.Fold)?.content
    val thought = (kind as? RowModel.Kind.Thinking)?.text
    val symbol = when {
        tool != null -> tool.symbol
        group != null -> ToolContent.symbol(group.icon)
        fold != null -> Symbol.Clock
        else -> Symbol.Brain
    }
    val running = tool?.status == ToolContent.Status.Running || group?.running == true
    val startedAt = tool?.startedAt ?: group?.startedAt
    val hasDetail = when {
        tool != null -> tool.hasDetail
        thought != null -> thought.isNotEmpty()
        else -> true
    }
    /** For a group or a fold: whether it is open. Their rows are the core's, not a detail here. */
    val open = group?.open ?: fold?.open
    val expanded = open == null && hasDetail && actions.isExpanded(row.id)
    val failed = tool?.status == ToolContent.Status.Failed || group?.failed == true
    val title = remember(row, palette) {
        val (words, target) = when {
            tool != null -> tool.verb to tool.target
            group != null -> group.title to group.target
            fold != null -> fold.label to ""
            else -> "Thought" to ""
        }
        toolTitle(words, target, failed, running, palette)
    }
    val inset = if (row.nested) 24.dp else 0.dp
    val line = ToolRow.height - 2.dp
    val bleed = (line - 16.dp) / 2
    Column(Modifier.fillMaxWidth().padding(top = row.above.dp)) {
        Box(Modifier.height(ToolRow.height).padding(start = inset).offset(x = -bleed), contentAlignment = Alignment.CenterStart) {
            HighlightButton(
                onClick = {
                    when {
                        open != null -> actions.toggleRow(row.id)
                        else -> actions.toggleExpansion(row.id)
                    }
                },
                modifier = Modifier.height(line),
                enabled = hasDetail,
                radius = Radius.sm,
                contentAlignment = Alignment.CenterStart,
            ) {
                Row(Modifier.padding(horizontal = bleed).height(line), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(16.dp), contentAlignment = Alignment.Center) { Icon(symbol, 12f, color = palette.mutedStrongerForeground) }
                    Spacer(Modifier.width(8.dp))
                    ShimmerText(
                        title, Theme.ui(13f), palette.mutedStrongerForeground, palette.foreground, running,
                        Modifier.weight(1f, fill = false),
                    )
                    if (hasDetail) {
                        Box(Modifier.padding(start = 2.dp).width(14.dp), contentAlignment = Alignment.Center) {
                            Icon(if (open ?: expanded) Symbol.ChevronDown else Symbol.ChevronRight, 9f, color = palette.mutedStrongerForeground)
                        }
                    }
                    if (startedAt != null) {
                        val now = ticking(true)
                        Text(Time.elapsed(startedAt, now), Modifier.padding(start = 6.dp), style = Theme.uiDigits(12f), color = palette.mutedStrongerForeground, maxLines = 1)
                    }
                }
            }
        }
        if (expanded) {
            val detail = remember(row, palette) { tool?.detail?.get(palette) ?: AnnotatedString(thought.orEmpty(), SpanStyle(color = palette.mutedForeground)) }
            val shape = RoundedCornerShape(Radius.md)
            Box(
                Modifier
                    .padding(start = inset + 30.dp, top = 3.dp, bottom = 9.dp)
                    .fillMaxWidth()
                    .clip(shape)
                    .background(palette.backgroundSecondary, shape)
                    .padding(horizontal = 12.dp, vertical = 10.dp),
            ) {
                val style = if (tool != null) Theme.smallMono.copy(lineHeight = Theme.codeLineHeight) else Theme.ui(13f).copy(lineHeight = (13f * Platform.scale * 1.2f + 5).sp)
                SelectionContainer { Text(detail, style = style) }
            }
        }
    }
}

object ToolRow {
    val height = 36.dp
}

/** A running call's title is all muted, so the band shows on every part of it. */
private fun toolTitle(words: String, target: String, failed: Boolean, running: Boolean, palette: Palette): AnnotatedString = buildAnnotatedString {
    append(if (target.isEmpty()) words else "$words ")
    val targetColor = when {
        failed -> palette.destructive
        running -> palette.mutedStrongerForeground
        else -> palette.mutedForeground
    }
    withStyle(SpanStyle(fontFamily = Theme.mono, fontSize = Theme.inlineCodeFont.fontSize, color = targetColor)) { append(target) }
}

/**
 * An agent the thread's agent started, or what it was told of one: its avatar, its title and word,
 * then what it runs on and does or said, and how long it worked. A tap opens it: its thread, or what
 * it did. In the card of an open group it lies on the card, which its rows draw as one.
 */
@Composable
private fun AgentRow(row: RowModel, content: AgentContent) {
    val palette = LocalPalette.current
    val actions = LocalTranscript.current
    val place = content.card
    val inset = if (place == null) 0.dp else AgentRow.cardPadding
    val top = if (place == AgentContent.Card.First) AgentRow.cardAbove else 0.dp
    val bottom = if (place == AgentContent.Card.Last) AgentRow.cardBelow else 0.dp
    val card = if (place == null) Modifier else Modifier.clipToBounds().drawBehind {
        // A card's rows are one card: each draws the part of it that is its own.
        val overhang = (Radius.lg + 1.dp).toPx()
        val ends = (AgentRow.cardLight - AgentRow.lightGap).toPx()
        val from = if (place == AgentContent.Card.First) top.toPx() - ends else -overhang
        val to = if (place == AgentContent.Card.Last) size.height - bottom.toPx() + ends else size.height + overhang
        val radius = CornerRadius(Radius.lg.toPx())
        val line = 1.dp.toPx()
        drawRoundRect(palette.card, Offset(0f, from), Size(size.width, to - from), radius)
        drawRoundRect(palette.borderCard, Offset(line / 2, from + line / 2), Size(size.width - line, to - from - line), radius, Stroke(line))
    }
    val time = agentTime(content)
    // In a card the light keeps as far from the card's border as from the next row's light, in the card's accent.
    val lit = if (place == null) 0.dp else (AgentRow.cardLight - (AgentRow.cardPadding - AgentRow.bleed)).coerceAtLeast(0.dp)
    CompositionLocalProvider(LocalSurface provides if (place == null) LocalSurface.current else Surface.Card) {
        Box(card.fillMaxWidth().padding(top = top, bottom = bottom).padding(horizontal = inset).height(AgentRow.height)) {
            HighlightButton(
                onClick = {
                    val thread = content.thread
                    if (thread != null) actions.openThread(thread) else actions.openAgent(row.itemID)
                },
                modifier = Modifier.fillMaxSize().bleed(AgentRow.bleed),
                inset = PaddingValues(horizontal = lit, vertical = AgentRow.lightGap),
                contentAlignment = Alignment.CenterStart,
            ) {
                Row(Modifier.fillMaxSize().padding(horizontal = AgentRow.bleed), verticalAlignment = Alignment.CenterVertically) {
                    AgentAvatar(content.agent, content.state)
                    Spacer(Modifier.width(AgentRow.avatarGap))
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.Center) {
                        // The title first, then its role and its word in what room is left.
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(content.title, size = 13f, weight = FontWeight.Medium, color = palette.foreground, maxLines = 1)
                            content.role?.let { Text(it, Modifier.padding(start = 6.dp), size = 11f, color = palette.mutedStrongerForeground, maxLines = 1) }
                            if (content.flagged) {
                                val color = if (content.state.flagged) content.state.color else palette.mutedForeground
                                Text(content.label, Modifier.padding(start = 6.dp), size = 11f, weight = FontWeight.Medium, color = color, maxLines = 1)
                            }
                        }
                        val color = if (content.state == AgentState.Failed) palette.destructive else palette.mutedForeground
                        Text(content.line, Modifier.padding(top = 1.dp), size = 12f, color = color, maxLines = 1)
                    }
                    if (time.isNotEmpty()) Text(time, Modifier.padding(start = 12.dp), style = Theme.uiDigits(12f), color = palette.mutedStrongerForeground, maxLines = 1)
                    AgentRowChevron(open = false)
                }
            }
        }
    }
}

/** How long the agent has worked, counted while it works, or for what it was told, when it was. */
@Composable
private fun agentTime(content: AgentContent): String {
    val working = content.at == null && content.state in listOf(AgentState.Working, AgentState.Waiting)
    val now = ticking(working)
    return when {
        content.at != null -> Time.stamp(content.at)
        working -> Time.elapsed(content.startedAt, now)
        else -> content.durationMs?.let { Time.duration(it) } ?: ""
    }
}

@Composable
private fun AgentRowChevron(open: Boolean) {
    Box(Modifier.padding(start = 6.dp).width(14.dp), contentAlignment = Alignment.Center) {
        Icon(if (open) Symbol.ChevronDown else Symbol.ChevronRight, 9f, color = LocalPalette.current.mutedStrongerForeground)
    }
}

/** Agents started one after the other: their avatars side by side, how many and how they are doing, and how long they have worked. A tap opens it into a card of their rows. */
@Composable
private fun AgentsRow(row: RowModel, content: AgentsContent) {
    val palette = LocalPalette.current
    val actions = LocalTranscript.current
    val now = ticking(content.startedAt != null)
    val time = content.startedAt?.let { Time.elapsed(it, now) } ?: content.durationMs?.let { Time.duration(it) } ?: ""
    Box(Modifier.fillMaxWidth().height(AgentRow.height)) {
        HighlightButton(
            onClick = { actions.toggleRow(row.id) },
            modifier = Modifier.fillMaxSize().bleed(AgentRow.bleed),
            inset = PaddingValues(vertical = 2.dp),
            contentAlignment = Alignment.CenterStart,
        ) {
            Row(Modifier.fillMaxSize().padding(horizontal = AgentRow.bleed), verticalAlignment = Alignment.CenterVertically) {
                AgentAvatars(content.avatars, content.more)
                Spacer(Modifier.width(AgentRow.avatarGap))
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.Center) {
                    Text(content.title, size = 13f, weight = FontWeight.SemiBold, color = palette.foreground, maxLines = 1)
                    Text(summaryText(content.summary, palette), Modifier.padding(top = 1.dp), size = 12f, maxLines = 1)
                }
                if (time.isNotEmpty()) Text(time, Modifier.padding(start = 12.dp), style = Theme.uiDigits(12f), color = palette.mutedStrongerForeground, maxLines = 1)
                AgentRowChevron(content.open)
            }
        }
    }
}

/** Each count in its state's colour, between muted dots. */
private fun summaryText(counts: List<Pair<String, AgentState>>, palette: Palette): AnnotatedString = buildAnnotatedString {
    counts.forEachIndexed { index, (text, state) ->
        if (index > 0) withStyle(SpanStyle(color = palette.mutedStrongerForeground)) { append(" · ") }
        withStyle(SpanStyle(color = state.color(palette))) { append(text) }
    }
}

private object AgentRow {
    val height = scaled(50f).dp
    val avatarGap = scaled(10f).dp
    /** How far a row in a group's card stands in from the card's sides. */
    val cardPadding = scaled(8f).dp
    /** How far in from the card's border a row in it lights, on every side. */
    val cardLight = scaled(4f).dp
    /** How far the light stands in from the row's top and bottom, half the room between two lights. */
    val lightGap = 2.dp
    /** How far the light reaches past the row's sides, so that the avatar lines up with the text. */
    val bleed = scaled(6f).dp
    /** The room above a group's card and under it, which its border stands in from by what the light leaves at the card's ends. */
    val cardAbove = scaled(4f).dp + cardLight - lightGap
    val cardBelow = scaled(10f).dp + cardLight - lightGap
}

/** Reaches `by` past both sides of the room it is given. */
private fun Modifier.bleed(by: Dp): Modifier = layout { measurable, constraints ->
    val extra = by.roundToPx()
    val placeable = measurable.measure(constraints.copy(minWidth = constraints.minWidth + extra * 2, maxWidth = constraints.maxWidth + extra * 2))
    layout(placeable.width - extra * 2, placeable.height) { placeable.place(-extra, 0) }
}

@Composable
private fun ErrorRow(message: String) {
    val palette = LocalPalette.current
    val shape = RoundedCornerShape(Radius.lg)
    Row(
        Modifier
            .padding(top = 4.dp, bottom = 10.dp)
            .fillMaxWidth()
            .clip(shape)
            .background(palette.destructive.wash(), shape)
            .padding(start = 12.dp, end = 14.dp, top = 10.dp, bottom = 10.dp),
    ) {
        Box(Modifier.width(24.dp).height(scaled(18f).dp), contentAlignment = Alignment.CenterStart) {
            Icon(Symbol.CircleAlert, 13f, color = palette.destructive)
        }
        SelectionContainer {
            Text(message, style = Theme.ui(13f).copy(lineHeight = (13f * Platform.scale * 1.2f + 5).sp), color = palette.destructive)
        }
    }
}

/** What a turn changed: how many files and lines, and the files under their folders. A file opens what the turn changed in it in a tab of the panel. */
@Composable
private fun ChangesRow(row: RowModel, changes: ChangesContent) {
    val palette = LocalPalette.current
    val actions = LocalTranscript.current
    val buttonMargin = 6.dp
    SurfaceBox(RoundedCornerShape(Radius.lg), Modifier.padding(top = 12.dp, bottom = 14.dp).fillMaxWidth(), bordered = true) {
        Column(Modifier.padding(bottom = 6.dp)) {
            Row(Modifier.height(scaled(40f).dp), verticalAlignment = Alignment.CenterVertically) {
                val counts = remember(changes, palette) { Typesetter.lineCounts(changes.added, changes.removed, palette) }
                val title = remember(changes, palette) {
                    buildAnnotatedString {
                        withStyle(Theme.ui(13f, androidx.compose.ui.text.font.FontWeight.Medium).toSpanStyle().copy(color = palette.foreground)) { append(changes.title) }
                        append("   ")
                        withStyle(Theme.uiDigits(11.5f, androidx.compose.ui.text.font.FontWeight.Medium).toSpanStyle()) { append(counts) }
                    }
                }
                Text(title, Modifier.padding(start = 14.dp).weight(1f), maxLines = 1)
                RowButton(
                    "Open diff", "Show what this turn changed",
                    margin = PaddingValues(start = 2.dp, end = buttonMargin, top = buttonMargin, bottom = buttonMargin),
                ) { actions.openDiff(row.itemID, null) }
            }
            for (entry in changes.entries) ChangeEntryRow(row, entry)
        }
    }
}

@Composable
private fun ChangeEntryRow(row: RowModel, entry: ChangesContent.Entry) {
    val palette = LocalPalette.current
    val actions = LocalTranscript.current
    val x = 12 + entry.depth * 16
    val light = ((if (entry.folder) x else x + 16) - 6).dp
    val vertical = (ChangesRow.entryHeight - ControlSize.Small.height) / 2
    HighlightButton(
        onClick = { if (entry.folder) actions.toggleFolder(entry.id, row.id) else actions.openDiff(row.itemID, entry.path) },
        modifier = Modifier.fillMaxWidth().height(ChangesRow.entryHeight),
        radius = Radius.sm,
        small = true,
        inset = PaddingValues(start = light, end = 6.dp, top = vertical, bottom = vertical),
        contentAlignment = Alignment.CenterStart,
    ) {
        Row(Modifier.padding(start = x.dp, end = 14.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.width(16.dp)) {
                if (entry.folder) Icon(if (entry.open) Symbol.ChevronDown else Symbol.ChevronRight, 8f, color = palette.mutedStrongerForeground)
            }
            Box(Modifier.width(24.dp)) {
                Icon(if (entry.folder) Symbol.Folder else FileSymbol.symbol(entry.path), 11f, color = palette.mutedForeground)
            }
            Text(
                entry.name, Modifier.weight(1f).padding(end = 12.dp), style = Theme.smallMono,
                color = if (entry.folder) palette.mutedForeground else palette.foreground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis,
            )
            val counts = remember(entry, palette) { Typesetter.lineCounts(entry.added, entry.removed, palette) }
            Text(counts, style = Theme.uiDigits(11.5f, androidx.compose.ui.text.font.FontWeight.Medium), maxLines = 1)
        }
    }
}

object ChangesRow {
    val entryHeight = 36.dp
}

/** Closes a turn: a way to copy the reply and when it ended. */
@Composable
private fun TurnEndRow(row: RowModel, end: TurnEndContent) {
    val actions = LocalTranscript.current
    MessageMeta(end.stamp, trailing = false, help = "Copy reply", Modifier.padding(bottom = 10.dp)) { actions.copyReply(row.id) }
}

/** Under a message or a reply: when it was sent and a button that copies it, at the side the message is on. */
@Composable
fun MessageMeta(stamp: String, trailing: Boolean, help: String, modifier: Modifier = Modifier, copy: () -> Unit) {
    val palette = LocalPalette.current
    Row(
        modifier.fillMaxWidth().height(MessageMeta.height),
        horizontalArrangement = if (trailing) Arrangement.End else Arrangement.Start,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (!trailing) CopyButton(help, symbolSize = 12f, copy = copy)
        Text(stamp, Modifier.weight(1f, fill = false), style = Theme.smallFont, color = palette.mutedStrongerForeground, maxLines = 1)
        if (trailing) CopyButton(help, symbolSize = 12f, copy = copy)
    }
}

object MessageMeta {
    val height = ControlSize.Regular.height
}

/** Where the thread went on with another agent or account: a line across, broken by the model it left and the one it went on with. */
@Composable
private fun HandoffRow(handoff: HandoffContent) {
    val palette = LocalPalette.current
    val lineGap = ControlSize.Regular.padding
    val labelGap = ControlSize.Regular.gap
    val arrowGap = ControlSize.Small.padding
    @Composable
    fun End(end: HandoffContent.End) {
        AgentIcon(end.agent, size = ControlSize.Regular.symbol)
        Spacer(Modifier.width(labelGap))
        val label = buildAnnotatedString {
            append(end.label)
            val account = end.account ?: return@buildAnnotatedString
            withStyle(SpanStyle(color = palette.mutedStrongerForeground)) { append(" · ") }
            append(account)
        }
        Text(label, style = Theme.smallFont, color = palette.foreground, maxLines = 1)
    }
    Row(Modifier.fillMaxWidth().height(ControlSize.Large.height), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.weight(1f).height(1.dp).background(palette.border))
        Spacer(Modifier.width(lineGap))
        End(handoff.from)
        Spacer(Modifier.width(arrowGap))
        Icon(Symbol.ArrowRight, ControlSize.Regular.smallSymbol, color = palette.mutedStrongerForeground)
        Spacer(Modifier.width(arrowGap))
        End(handoff.to)
        Spacer(Modifier.width(lineGap))
        Box(Modifier.weight(1f).height(1.dp).background(palette.border))
    }
}

/** The end of a thread whose server was unlinked from the account, in place of the composer: why the thread can only be read, and the ways on from it. */
@Composable
private fun RemovedRow(removed: RemovedContent) {
    val palette = LocalPalette.current
    val actions = LocalTranscript.current
    SurfaceBox(RoundedCornerShape(Radius.lg), Modifier.padding(vertical = RemovedRow.margin).fillMaxWidth(), bordered = true) {
        BoxWithConstraints(Modifier.padding(RemovedRow.padding)) {
            val halves = maxWidth < RemovedRow.halvesBelow
            Column {
                Row(Modifier.height(RemovedRow.titleHeight), verticalAlignment = Alignment.CenterVertically) {
                    Icon(Symbol.Unlink, 13f, color = palette.mutedForeground)
                    Text(removed.title, Modifier.padding(start = RemovedRow.iconGap), style = Theme.ui(13f, FontWeight.SemiBold), color = palette.foreground, maxLines = 1)
                }
                SelectionContainer(Modifier.padding(top = RemovedRow.textGap)) {
                    Text(removed.message, style = Theme.ui(13f).copy(lineHeight = (13f * Platform.scale * 1.2f + 5).sp), color = palette.mutedForeground)
                }
                Row(
                    Modifier.fillMaxWidth().padding(top = RemovedRow.buttonsGap),
                    horizontalArrangement = Arrangement.spacedBy(ControlSize.Regular.gap, Alignment.End),
                ) {
                    val half = if (halves) Modifier.weight(1f) else Modifier
                    RowButton("Open Settings", "Unlinked servers are deleted with their threads in settings", half, bordered = true, hover = palette.border, fills = halves) {
                        actions.store.openSettings(SettingsSection.Servers)
                    }
                    RowButton("Continue Thread", "Start a thread on another server with everything that happened here", half, primary = true, fills = halves) {
                        actions.store.continueOnAnotherServer()
                    }
                }
            }
        }
    }
}

private object RemovedRow {
    val margin = scaled(12f).dp
    val padding = scaled(16f).dp
    val titleHeight = scaled(18f).dp
    val iconGap = scaled(8f).dp
    val textGap = scaled(6f).dp
    val buttonsGap = scaled(14f).dp
    /** Under this width the two buttons share the row in halves. */
    val halvesBelow = scaled(420f).dp
}

/** Shown under the transcript while the agent is at work or waits for an approval. */
@Composable
fun WorkingLine(activity: Activity, modifier: Modifier = Modifier) {
    val palette = LocalPalette.current
    val asking = activity.approvals.firstOrNull()?.ask
    val now = ticking(asking == null)
    val words = when {
        asking != null -> asking.waiting
        else -> {
            val verb = if (activity.compacting) "Compacting" else if (activity.thinking) "Thinking" else "Working"
            activity.startedAt?.let { "$verb for ${Time.elapsed(it, now)}" } ?: "$verb…"
        }
    }
    Box(modifier.fillMaxWidth().height(WorkingLine.height)) {
        ShimmerText(
            AnnotatedString(words), Theme.uiDigits(13f), palette.mutedStrongerForeground, palette.emphasizedForeground, asking == null,
            Modifier.padding(top = 5.dp),
        )
    }
}

object WorkingLine {
    /** As tall as the row that ends a turn, which takes its place. */
    val height = MessageMeta.height + 10.dp
}
