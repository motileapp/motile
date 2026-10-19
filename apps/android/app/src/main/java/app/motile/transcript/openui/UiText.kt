package app.motile.transcript.openui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredHeight
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.layout.AlignmentLine
import androidx.compose.ui.layout.FirstBaseline
import androidx.compose.ui.layout.IntrinsicMeasurable
import androidx.compose.ui.layout.IntrinsicMeasureScope
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.Measurable
import androidx.compose.ui.layout.MeasurePolicy
import androidx.compose.ui.layout.MeasureResult
import androidx.compose.ui.layout.MeasureScope
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import app.motile.core.UiItemMarker
import app.motile.core.UiKind
import app.motile.core.UiListMarker
import app.motile.core.UiMarkdownBlock
import app.motile.core.UiNode
import app.motile.core.UiRich
import app.motile.core.UiTextStyle
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Palette
import app.motile.theme.Theme
import app.motile.transcript.CodeBox
import app.motile.transcript.MarkedText
import app.motile.transcript.ProseBlockView
import app.motile.transcript.ProseSource
import app.motile.transcript.TextMarks
import app.motile.transcript.Typesetter
import app.motile.ui.Chip
import app.motile.ui.Icon
import app.motile.ui.Text
import app.motile.ui.lined

private object Runs {
    const val BOLD = 1
    const val ITALIC = 2
    const val CODE = 4
    const val STRIKE = 8
}

/** The tag of an inline component in a Text's text. */
private const val INLINE = '￼'

/** Core's text with its runs and links, as LAYOUT.md's inline formatting draws it. */
fun UiRich.annotated(color: Color, palette: Palette, inlines: Int = 0): AnnotatedString {
    val builder = AnnotatedString.Builder()
    builder.pushStyle(SpanStyle(color = color))
    var inline = 0
    for (char in text) {
        if (char == INLINE && inline < inlines) {
            builder.appendInlineContent("$inline", "$INLINE")
            inline += 1
        } else {
            builder.append(char)
        }
    }
    builder.pop()
    val length = text.length
    for (run in runs) {
        if (run.size != 3) continue
        val (start, count, bits) = Triple(run[0], run[1], run[2])
        val end = start + count
        if (start < 0 || count <= 0 || end > length) continue
        if (bits and Runs.CODE != 0) {
            builder.addStyle(SpanStyle(fontFamily = Theme.mono, fontSize = Theme.inlineCodeFont.fontSize, color = palette.foreground), start, end)
            builder.addStringAnnotation(TextMarks.code, "", start, end)
        }
        if (bits and Runs.BOLD != 0) builder.addStyle(SpanStyle(fontWeight = FontWeight.SemiBold, color = palette.foreground), start, end)
        if (bits and Runs.ITALIC != 0) builder.addStyle(SpanStyle(fontStyle = FontStyle.Italic), start, end)
        if (bits and Runs.STRIKE != 0) builder.addStyle(SpanStyle(textDecoration = TextDecoration.LineThrough), start, end)
    }
    for (link in links) {
        val end = link.start + link.len
        if (link.start < 0 || link.len <= 0 || end > length) continue
        val style = SpanStyle(color = palette.primary)
        builder.addStyle(style, link.start, end)
        builder.addLink(LinkAnnotation.Url(link.url, TextLinkStyles(style)) { Platform.open(link.url) }, link.start, end)
    }
    return builder.toAnnotatedString()
}

/**
 * Core's text in a style, with its inline components laid on its lines: each a box that never
 * breaks, 1 from the text on each side, its bottom 0.15 of the text size under the baseline, a
 * Badge's 0.1 higher.
 */
@Composable
fun UiRichText(
    rich: UiRich,
    style: TextStyle,
    color: Color,
    modifier: Modifier = Modifier,
    inlines: List<UiNode> = emptyList(),
    maxLines: Int = Int.MAX_VALUE,
) {
    val palette = LocalPalette.current
    val align = LocalUiTextAlign.current
    val text = remember(rich, color, palette, inlines.size) { rich.annotated(color, palette, inlines.size) }
    val styled = if (align != null) style.copy(textAlign = align) else style
    if (inlines.isEmpty()) {
        if (maxLines == Int.MAX_VALUE) {
            MarkedText(text, styled, modifier)
        } else {
            Text(text, modifier, style = styled, color = color, maxLines = maxLines, overflow = TextOverflow.Ellipsis)
        }
        return
    }
    val density = LocalDensity.current
    val textSize = with(density) { style.fontSize.toPx() }
    var sizes by remember(inlines.size) { mutableStateOf(List(inlines.size) { InlineSize(0, 0, null) }) }
    val content = inlines.mapIndexed { index, node ->
        val size = sizes.getOrElse(index) { InlineSize(0, 0, null) }
        val below = size.baseline?.let { size.height - it.toFloat() } ?: (textSize * if (node.kind is UiKind.Badge) 0.05f else 0.15f)
        val placeholder = with(density) {
            Placeholder((size.width + 2.dp.toPx()).toSp(), (size.height - below).coerceAtLeast(1f).toSp(), PlaceholderVerticalAlign.AboveBaseline)
        }
        "$index" to InlineTextContent(placeholder) {
            Box(Modifier.fillMaxSize().padding(horizontal = 1.dp).wrapContentHeight(Alignment.Top, unbounded = true)) {
                Box(Modifier.requiredHeight(with(density) { size.height.toDp() })) { UiNodeView(node) }
            }
        }
    }.toMap()
    val measure = remember(inlines) { InlineMeasure(inlines) { measured -> if (measured != sizes) sizes = measured } }
    CompositionLocalProvider(LocalUiTextSize provides style.fontSize.value, LocalForeground provides color) {
        Layout(
            content = {
                inlines.forEach { node -> Box { UiNodeView(node) } }
                Text(text, style = styled, color = color, maxLines = maxLines, overflow = TextOverflow.Ellipsis, inlineContent = content)
            },
            modifier = modifier,
            measurePolicy = measure,
        )
    }
}

/** An inline component's size, and the baseline a formula shares with the text around it. */
private data class InlineSize(val width: Int, val height: Int, val baseline: Int?)

/**
 * Measures a text's inline components, which it composes without placing them, for the boxes
 * the text keeps for them, then lays the text out. Its intrinsic sizes are the text's.
 */
private class InlineMeasure(val inlines: List<UiNode>, val measured: (List<InlineSize>) -> Unit) : MeasurePolicy {
    override fun MeasureScope.measure(measurables: List<Measurable>, constraints: Constraints): MeasureResult {
        val sizes = measurables.dropLast(1).mapIndexed { index, measurable ->
            val placeable = measurable.measure(Constraints(maxWidth = constraints.maxWidth))
            val baseline = placeable[FirstBaseline].takeIf { it != AlignmentLine.Unspecified && inlines[index].kind is UiKind.Math }
            InlineSize(placeable.width, placeable.height, baseline)
        }
        measured(sizes)
        val text = measurables.last().measure(constraints)
        return layout(text.width, text.height) { text.place(0, 0) }
    }

    override fun IntrinsicMeasureScope.maxIntrinsicHeight(measurables: List<IntrinsicMeasurable>, width: Int): Int =
        measurables.last().maxIntrinsicHeight(width)

    override fun IntrinsicMeasureScope.minIntrinsicHeight(measurables: List<IntrinsicMeasurable>, width: Int): Int =
        measurables.last().minIntrinsicHeight(width)

    override fun IntrinsicMeasureScope.maxIntrinsicWidth(measurables: List<IntrinsicMeasurable>, height: Int): Int =
        measurables.last().maxIntrinsicWidth(height)

    override fun IntrinsicMeasureScope.minIntrinsicWidth(measurables: List<IntrinsicMeasurable>, height: Int): Int =
        measurables.last().minIntrinsicWidth(height)
}

@Composable
fun UiTitle(node: UiNode, kind: UiKind.Title) {
    val narrow = LocalUiNarrow.current
    UiRichText(kind.text, UiStyle.text(kind.style, narrow), kind.color.color(), Modifier.uiNode(node).semantics { heading() })
}

@Composable
fun UiTextView(node: UiNode, kind: UiKind.Text) {
    val narrow = LocalUiNarrow.current
    val base = UiStyle.text(kind.style, narrow)
    var style = base.copy(
        fontWeight = if (kind.weight == app.motile.core.UiWeight.Regular) base.fontWeight else UiStyle.weight(kind.weight),
        fontStyle = if (kind.italic) FontStyle.Italic else null,
        textDecoration = if (kind.strike) TextDecoration.LineThrough else null,
    )
    if (kind.digits) style = style.copy(fontFeatureSettings = "tnum")
    UiRichText(kind.text, style, kind.color.color(), Modifier.uiNode(node), kind.inlines, kind.maxLines ?: Int.MAX_VALUE)
}

@Composable
fun UiCaption(node: UiNode, kind: UiKind.Caption) {
    UiRichText(kind.text, UiStyle.text(kind.style, LocalUiNarrow.current), kind.color.color(), Modifier.uiNode(node))
}

@Composable
fun UiLabel(node: UiNode, kind: UiKind.Label) {
    Text(kind.text, Modifier.uiNode(node), style = UiStyle.text(UiTextStyle.Label, false), color = kind.color.color(), align = LocalUiTextAlign.current)
}

/** Motile's chip, in the badge's tone. */
@Composable
fun UiBadge(node: UiNode, kind: UiKind.Badge) {
    val palette = LocalPalette.current
    Chip(kind.text, Modifier.uiNode(node), glyph = kind.icon?.glyph, tone = kind.tone?.let { palette.token(it) })
}

/** Text in the link colour with an arrow after it, which opens the address; plain text without one. */
@Composable
fun UiLink(node: UiNode, kind: UiKind.Link) {
    val palette = LocalPalette.current
    val style = UiStyle.text(UiTextStyle.Body, false)
    val url = kind.url
    if (url == null) {
        Text(kind.text, Modifier.uiNode(node), style = style, color = palette.foreground)
        return
    }
    Row(
        Modifier.uiNode(node).clickable(role = Role.Button) { Platform.open(url) }.semantics { role = Role.Button },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(kind.text, Modifier.weight(1f, fill = false), style = style, color = palette.primary)
        Icon(Symbol.ArrowUpRight, 14f * 0.85f, Modifier.padding(start = 2.dp), color = palette.primary)
    }
}

/** A quote as prose draws one: a bar, the text in from it, and who said it under it. */
@Composable
fun UiQuote(node: UiNode, kind: UiKind.Quote) {
    val palette = LocalPalette.current
    Column(
        Modifier.uiNode(node).fillMaxWidth().drawBehind {
            drawRect(palette.border, Offset(1.dp.toPx(), 0f), Size(2.dp.toPx(), size.height))
        }.padding(start = 14.dp),
    ) {
        UiRichText(kind.text, UiStyle.text(UiTextStyle.Body, false), palette.mutedForeground)
        kind.cite?.let { UiRichText(it, UiStyle.text(UiTextStyle.Small, false), palette.mutedStrongerForeground, Modifier.padding(top = 4.dp)) }
    }
}

/** Code in Motile's code box, coloured by core's spans, wrapping. */
@Composable
fun UiCode(node: UiNode, kind: UiKind.Code) {
    val palette = LocalPalette.current
    val text = remember(kind, palette) { Typesetter.code(kind.code, kind.spans?.toIntArray() ?: IntArray(0), palette) }
    SelectionContainer(Modifier.uiNode(node)) { CodeBox(kind.language, kind.code, text, wraps = true) }
}

/** Markdown as the transcript's own prose. */
@Composable
fun UiMarkdown(node: UiNode, kind: UiKind.Markdown) {
    val palette = LocalPalette.current
    val prose = remember(kind, palette) {
        kind.blocks.map { block -> (block as? UiMarkdownBlock.Prose)?.let { Typesetter.prose(ProseSource(it.prose), palette) } }
    }
    SelectionContainer(Modifier.uiNode(node)) {
        Column(Modifier.fillMaxWidth()) {
            kind.blocks.forEachIndexed { index, block ->
                val top = if (index == 0) 0.dp else 12.dp
                when (block) {
                    is UiMarkdownBlock.Code -> {
                        val code = remember(block, palette) { Typesetter.code(block.code, block.spans?.toIntArray() ?: IntArray(0), palette) }
                        CodeBox(block.language, block.code, code, Modifier.padding(top = top), wraps = true)
                    }
                    is UiMarkdownBlock.Prose -> Column(Modifier.padding(top = top)) {
                        for (paragraph in prose[index].orEmpty()) ProseBlockView(paragraph)
                    }
                }
            }
        }
    }
}

/** A list's items, each its marker in a lane beside its body, joined by a line in a timeline. */
@Composable
fun UiList(node: UiNode, kind: UiKind.List) {
    val gap = if (kind.timeline) 0.dp else 10.dp
    Column(Modifier.uiNode(node).fillMaxWidth()) {
        kind.items.forEachIndexed { index, item ->
            val listItem = item.kind as? UiKind.ListItem ?: return@forEachIndexed UiNodeView(item)
            val last = index == kind.items.lastIndex
            UiListItem(item, listItem, kind, last, Modifier.padding(top = if (index == 0) 0.dp else gap))
        }
    }
}

@Composable
private fun UiListItem(node: UiNode, item: UiKind.ListItem, list: UiKind.List, last: Boolean, modifier: Modifier) {
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    val narrow = LocalUiNarrow.current
    val style = UiStyle.text(item.style, narrow).let { if (item.medium) it.copy(fontWeight = FontWeight.Medium) else it }
    val density = LocalDensity.current
    val textSize = UiStyle.size(item.style, narrow) * Platform.scale
    val firstLine = with(density) { style.lineHeight.toDp() }
    val marker = item.marker
    val wide = list.wide || marker is UiItemMarker.Label
    val (lane, laneGap) = when {
        wide && list.timeline -> (if (narrow) 64f else 76f) to (if (narrow) 20f else 24f)
        wide -> (if (narrow) 64f else 76f) to (if (narrow) 20f else 16f)
        marker is UiItemMarker.Dot && list.timeline -> 24f to 12f
        marker is UiItemMarker.Dot -> 14f to 8f
        marker is UiItemMarker.Number || marker is UiItemMarker.Chip -> 24f to 12f
        else -> 0f to 0f
    }
    val ring = list.timeline && wide
    val ringAt = if (narrow) 6f else 4f
    val lineX = when {
        ring -> lane + if (narrow) 9.5f else 7.5f
        else -> 12f
    }
    val joins = list.timeline && !last && !(list.marker == UiListMarker.Plain && !wide)
    val below = if (list.timeline && !last) 18f else 0f
    val extra = if (item.description != null || item.children.isNotEmpty()) 4f else 0f
    val lineColor = palette.borderAccent
    val inset = lane + laneGap
    Box(
        modifier.uiNode(node).fillMaxWidth().drawBehind {
            if (!joins) return@drawBehind
            val top = (1.7f * textSize).dp.toPx()
            val bottom = size.height - (0.15f * textSize).dp.toPx()
            drawRect(lineColor, Offset((lineX - 0.5f).dp.toPx(), top), Size(1.dp.toPx(), bottom - top))
        }.padding(bottom = (below + extra).dp),
    ) {
        if (lane > 0) {
            Box(Modifier.size(lane.dp, firstLine)) {
                when (marker) {
                    UiItemMarker.Dot -> {
                        if (list.timeline) {
                            Box(
                                Modifier.align(Alignment.Center).size(9.dp).background(palette.foreground, CircleShape).padding(2.dp)
                                    .background(surface.color(palette), CircleShape),
                            )
                        } else {
                            Box(Modifier.align(Alignment.Center).size(5.dp).background(palette.mutedStrongerForeground, CircleShape))
                        }
                    }
                    is UiItemMarker.Number -> Box(
                        Modifier.size(24.dp).background(palette.foreground, CircleShape),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text("${marker.number}", style = Theme.uiDigits(13f).lined(18f), color = surface.color(palette), maxLines = 1)
                    }
                    is UiItemMarker.Chip -> Box(
                        Modifier.height(20.dp).widthIn(min = 20.dp).background(palette.foreground, CircleShape).padding(horizontal = 5.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(marker.text, style = Theme.uiDigits(12f, FontWeight.Medium), color = surface.color(palette), maxLines = 1)
                    }
                    is UiItemMarker.Label -> Text(
                        marker.text, Modifier.align(Alignment.CenterStart), style = Theme.uiDigits(13f),
                        color = palette.mutedForeground, maxLines = 1, overflow = TextOverflow.Visible,
                    )
                    UiItemMarker.Blank -> Unit
                }
            }
            if (ring) {
                Box(
                    Modifier.padding(start = (lane + ringAt).dp, top = (0.55f * textSize - 4.5f).coerceAtLeast(0f).dp).size(9.dp)
                        .background(palette.foreground, CircleShape).padding(2.dp).background(surface.color(palette), CircleShape),
                )
            }
        }
        Column(Modifier.padding(start = inset.dp)) {
            UiRichText(item.title, style, palette.foreground)
            item.description?.let {
                UiRichText(it, UiStyle.text(UiTextStyle.Small, narrow), palette.mutedForeground, Modifier.padding(top = 2.dp))
            }
            if (item.children.isNotEmpty()) {
                Column(Modifier.padding(top = 8.dp)) { for (child in item.children) UiNodeView(child) }
            }
        }
    }
}
