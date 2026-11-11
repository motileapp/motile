package app.motile.transcript

import app.motile.composer.SkillChip

import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameMillis
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onPlaced
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.isSpecified
import app.motile.core.FoundSpan
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalDark
import app.motile.theme.LocalPalette
import app.motile.theme.Opacity
import app.motile.theme.Palette
import app.motile.theme.Radius
import app.motile.theme.Theme
import app.motile.theme.at
import app.motile.theme.value
import app.motile.theme.symbolSide
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.CopyButton
import app.motile.ui.Text
import app.motile.ui.drawSymbol
import kotlin.math.max
import kotlin.math.min

/**
 * A button of words inside a row. A bordered one is drawn as a regular button is, with a line
 * around it; the others light under a finger. `margin` is room around it that still takes the
 * press, so buttons that touch each other and the row's edge look apart.
 */
@Composable
fun RowButton(title: String, help: String, modifier: Modifier = Modifier, bordered: Boolean = false, margin: PaddingValues = PaddingValues(0.dp), onClick: () -> Unit) {
    ActionButton(
        title = title, help = help, modifier = modifier, variant = if (bordered) ButtonVariant.Outline else ButtonVariant.Ghost,
        size = RowButton.size, margin = margin, onClick = onClick,
    )
}

object RowButton {
    val size = ControlSize.Regular
}

/** A round button of one symbol over a picture. */
@Composable
fun OverlayButton(symbol: Symbol, help: String, size: ControlSize, modifier: Modifier = Modifier, onClick: () -> Unit) {
    ActionButton(icon = symbol, help = help, modifier = modifier, variant = ButtonVariant.Overlay, size = size, round = true, onClick = onClick)
}

/** The box an image or a video is shown in: empty until the picture is there. */
@Composable
fun PictureBox(picture: ImageBitmap?, modifier: Modifier = Modifier, radius: Dp = PictureBox.radius, content: @Composable BoxScope.() -> Unit = {}) {
    val palette = LocalPalette.current
    val shape = RoundedCornerShape(radius)
    Box(modifier.clip(shape).background(palette.backgroundSecondary, shape).border(1.dp, palette.border, shape)) {
        if (picture != null) Image(picture, null, Modifier.fillMaxSize(), contentScale = ContentScale.Fit)
        content()
    }
}

object PictureBox {
    val radius = Radius.lg
}

/** How much of something has arrived, as a ring that fills clockwise from the top. It grows smoothly but goes back at once. */
@Composable
fun ProgressRing(fraction: Double, modifier: Modifier = Modifier) {
    val palette = LocalPalette.current
    val shown = remember { Animatable(fraction.toFloat().coerceIn(0f, 1f)) }
    LaunchedEffect(fraction) {
        val target = fraction.toFloat().coerceIn(0f, 1f)
        if (target < shown.value) shown.snapTo(target) else shown.animateTo(target)
    }
    Canvas(modifier) {
        val line = 2.dp.toPx()
        val side = min(size.width, size.height) - line
        val topLeft = Offset((size.width - side) / 2, (size.height - side) / 2)
        val stroke = Stroke(line, cap = StrokeCap.Round)
        drawArc(palette.mutedStrongestForeground, 0f, 360f, false, topLeft, Size(side, side), style = stroke)
        drawArc(palette.foreground, -90f, 360f * shown.value, false, topLeft, Size(side, side), style = stroke)
    }
}

/** The top of a code box: the language, and a button that copies the code. */
@Composable
fun CodeHeader(language: String, code: String, modifier: Modifier = Modifier) {
    val palette = LocalPalette.current
    Row(modifier.height(CodeHeader.height), verticalAlignment = Alignment.CenterVertically) {
        Text(
            language.ifEmpty { "text" }, Modifier.padding(start = 14.dp).weight(1f), style = Theme.smallMono,
            color = palette.mutedForeground, maxLines = 1,
        )
        CopyButton("Copy code", Modifier.padding(CodeHeader.buttonInset)) { Platform.copy(code) }
    }
}

object CodeHeader {
    val buttonInset = 4.dp
    val height: Dp = ControlSize.Regular.height + buttonInset * 2
}

/** A rounded dashed line around the view, `dash` long with `gap` between, as a message that waits is drawn. */
fun Modifier.dashedBorder(color: Color, radius: Dp, dash: Dp = 2.dp, gap: Dp = 3.dp): Modifier = drawBehind {
    val inset = 0.5.dp.toPx()
    val corner = max(0f, radius.toPx() - inset)
    drawRoundRect(
        color,
        topLeft = Offset(inset, inset),
        size = Size(size.width - inset * 2, size.height - inset * 2),
        cornerRadius = CornerRadius(corner),
        style = Stroke(1.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(dash.toPx(), gap.toPx()))),
    )
}

/** The bars beside quoted text, one for each level. */
fun Modifier.quoteBars(depth: Int, color: Color): Modifier {
    if (depth <= 0) return this
    return drawBehind {
        for (level in 0 until depth) {
            drawRect(color, Offset((level * 14 + 1).dp.toPx(), 0f), Size(2.dp.toPx(), size.height))
        }
    }
}

/**
 * The time on the frame clock, which every label that shines reads, so that their bands move
 * together. Read it while drawing, so that only drawing follows it.
 */
@Composable
fun frameTime(running: Boolean): State<Long> = produceState(0L, running) {
    if (!running) return@produceState
    while (true) withFrameMillis { value = it }
}

object Shimmer {
    const val BAND_WIDTH = 72f
    const val PERIOD = 2200L
}

/**
 * Words with a bright copy over them, seen only through a soft band that sweeps across while
 * `sweeps`, as a label that says something is under way.
 */
@Composable
fun ShimmerText(text: AnnotatedString, style: TextStyle, color: Color, bright: Color, sweeps: Boolean, modifier: Modifier = Modifier) {
    val dark = LocalDark.current
    val shines = sweeps && !Platform.reducesMotion
    val time = frameTime(shines)
    /** The same words in the same fonts, all in the bright colour. */
    val brightText = remember(text) {
        AnnotatedString(text.text, text.spanStyles.map { AnnotatedString.Range(it.item.copy(color = Color.Unspecified), it.start, it.end) })
    }
    Box(modifier) {
        Text(text, style = style, color = color, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (!shines) return@Box
        val edge = Opacity.ShimmerBandEdge.value(dark)
        val middle = Opacity.ShimmerBandMiddle.value(dark)
        Text(
            brightText, style = style, color = bright, maxLines = 1, overflow = TextOverflow.Ellipsis,
            modifier = Modifier
                .graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
                .drawWithContent {
                    drawContent()
                    val band = Shimmer.BAND_WIDTH.dp.toPx()
                    val phase = (time.value % Shimmer.PERIOD) / Shimmer.PERIOD.toFloat()
                    val center = -band / 2 + phase * (size.width + band)
                    val start = center - band / 2
                    val mask = Brush.horizontalGradient(
                        0f to Color.Black.copy(alpha = 0f),
                        0.15f to Color.Black.copy(alpha = edge),
                        0.35f to Color.Black.copy(alpha = middle),
                        0.5f to Color.Black,
                        0.65f to Color.Black.copy(alpha = middle),
                        0.85f to Color.Black.copy(alpha = edge),
                        1f to Color.Black.copy(alpha = 0f),
                        startX = start, endX = start + band,
                    )
                    drawRect(mask, blendMode = BlendMode.DstIn)
                },
        )
    }
}

/** A laid out piece of a row's text, and `sent`, where a place in it is in the text the core sent. */
class TextPiece {
    var sent: (Int) -> Int = { it }
    var layout: TextLayoutResult? = null
    var coordinates: LayoutCoordinates? = null
}

/** The pieces of a row's text as they are laid out, for where finding starts in the row. */
class RowTexts {
    private val pieces = mutableSetOf<TextPiece>()

    fun add(piece: TextPiece) = pieces.add(piece)

    fun remove(piece: TextPiece) = pieces.remove(piece)

    /** Where the first whole line at or below `below` in the window starts in the text the core sent, or the text's end when none does. */
    fun offset(below: Float): Int {
        var end = 0
        var first: Pair<Float, Int>? = null
        for (piece in pieces) {
            val layout = piece.layout ?: continue
            val coordinates = piece.coordinates?.takeIf { it.isAttached } ?: continue
            val top = coordinates.positionInWindow().y
            end = max(end, piece.sent(layout.layoutInput.text.length))
            val line = (0 until layout.lineCount).firstOrNull { top + layout.getLineTop(it) >= below } ?: continue
            val at = top + layout.getLineTop(line) to piece.sent(layout.getLineStart(line))
            if (first == null || at.first < first.first || (at.first == first.first && at.second < first.second)) first = at
        }
        return first?.second ?: end
    }
}

/** The pieces of the row being composed, which its texts join. */
val LocalRowTexts = staticCompositionLocalOf<RowTexts?> { null }

/** A piece of the row's text that joins its row's while composed, or null outside a row or without `sent`. */
@Composable
fun rememberTextPiece(sent: ((Int) -> Int)?): TextPiece? {
    val texts = LocalRowTexts.current ?: return null
    sent ?: return null
    val piece = remember(texts) { TextPiece() }
    piece.sent = sent
    DisposableEffect(texts, piece) {
        texts.add(piece)
        onDispose { texts.remove(piece) }
    }
    return piece
}

/** Keeps where the text is placed in its piece. */
fun Modifier.placesPiece(piece: TextPiece?): Modifier = if (piece == null) this else onPlaced { piece.coordinates = it }

/**
 * Text with what its text system can't draw drawn behind it: the rounded boxes of inline code and
 * the chips of skills, from the marks `Typesetter` left in it, and finding's matches in the thread,
 * `found`. `onCurrent` is told where the current match is, in the text and in the window. `sent`
 * maps a place in it to the row's text, where finding starts.
 */
@Composable
fun MarkedText(
    text: AnnotatedString,
    style: TextStyle,
    modifier: Modifier = Modifier,
    found: List<FoundSpan> = emptyList(),
    onCurrent: ((Rect, Rect) -> Unit)? = null,
    sent: ((Int) -> Int)? = null,
) {
    val piece = rememberTextPiece(sent)
    val palette = LocalPalette.current
    val dark = LocalDark.current
    val marks = remember(text) {
        text.getStringAnnotations(TextMarks.code, 0, text.length) + text.getStringAnnotations(TextMarks.skill, 0, text.length)
    }
    val shown = remember(text, found, palette) { text.withCurrent(found, palette) }
    var layout by remember(text) { mutableStateOf<TextLayoutResult?>(null) }
    val density = LocalDensity.current
    val drawn = if (marks.isEmpty() && found.isEmpty()) modifier else modifier.drawBehind {
        val laid = layout ?: return@drawBehind
        drawFound(laid, found, style, palette)
        for (mark in marks) drawMark(laid, mark.start, mark.end, mark.tag == TextMarks.skill, palette, dark)
    }
    Text(
        shown, drawn.reportsCurrent(found, style, density, { layout }, onCurrent).placesPiece(piece), style = style,
        onTextLayout = {
            layout = it
            piece?.layout = it
        },
    )
}

/** Tells `onCurrent` where the current match is once the text is laid out and placed. */
fun Modifier.reportsCurrent(
    found: List<FoundSpan>,
    style: TextStyle,
    density: Density,
    layout: () -> TextLayoutResult?,
    onCurrent: ((Rect, Rect) -> Unit)?,
): Modifier {
    val current = found.firstOrNull { it.current } ?: return this
    onCurrent ?: return this
    return onGloballyPositioned { coordinates ->
        val laid = layout() ?: return@onGloballyPositioned
        val local = with(density) { foundBox(laid, current.start, current.end, style) } ?: return@onGloballyPositioned
        onCurrent(local, local.translate(coordinates.positionInWindow()))
    }
}

/** The text with the current match in its own colour, which changes nothing of its layout. */
fun AnnotatedString.withCurrent(found: List<FoundSpan>, palette: Palette): AnnotatedString {
    val current = found.firstOrNull { it.current && it.end <= length } ?: return this
    val builder = AnnotatedString.Builder(this)
    builder.addStyle(SpanStyle(color = palette.findCurrentForeground), current.start, current.end)
    return builder.toAnnotatedString()
}

/** A box behind each match, as tall as its font around the baseline, the current one in its own colour. */
fun DrawScope.drawFound(layout: TextLayoutResult, found: List<FoundSpan>, style: TextStyle, palette: Palette) {
    for (span in found) {
        val text = layout.layoutInput.text
        if (span.end <= span.start || span.end > text.length) continue
        val em = emOf(text, style, span.start).toPx()
        val pad = 1.dp.toPx()
        for (line in layout.getLineForOffset(span.start)..layout.getLineForOffset(span.end - 1)) {
            val from = max(span.start, layout.getLineStart(line))
            val to = min(span.end, layout.getLineEnd(line, visibleEnd = true))
            if (to <= from) continue
            val left = layout.getHorizontalPosition(from, usePrimaryDirection = true)
            val right = layout.getBoundingBox(to - 1).right
            val baseline = layout.getLineBaseline(line)
            drawRoundRect(
                if (span.current) palette.findCurrent else palette.findMatch,
                topLeft = Offset(left - pad, baseline - em * 0.95f - pad),
                size = Size(right - left + pad * 2, em * 1.25f + pad * 2),
                cornerRadius = CornerRadius(Radius.xs.toPx()),
            )
        }
    }
}

/** Where the match is in the text: the box of its first line. */
private fun Density.foundBox(layout: TextLayoutResult, start: Int, end: Int, style: TextStyle): Rect? {
    val text = layout.layoutInput.text
    if (end <= start || end > text.length) return null
    val em = emOf(text, style, start).toPx()
    val line = layout.getLineForOffset(start)
    val baseline = layout.getLineBaseline(line)
    val left = layout.getHorizontalPosition(start, usePrimaryDirection = true)
    val right = layout.getBoundingBox(min(end, layout.getLineEnd(line, visibleEnd = true).coerceAtLeast(start + 1)) - 1).right
    return Rect(left, baseline - em * 0.95f, max(right, left + 1f), baseline + em * 0.3f)
}

/** The size of the font at `offset`: its span's, or else the style's, or else the prose's. */
private fun emOf(text: AnnotatedString, style: TextStyle, offset: Int): TextUnit {
    val span = text.spanStyles.lastOrNull { it.start <= offset && offset < it.end && it.item.fontSize.isSpecified }
    return span?.item?.fontSize ?: style.fontSize.takeIf { it.isSpecified } ?: Theme.proseSize
}

private fun DrawScope.drawMark(layout: TextLayoutResult, start: Int, end: Int, skill: Boolean, palette: Palette, dark: Boolean) {
    if (end <= start || end > layout.layoutInput.text.length) return
    if (skill) return with(SkillChip) { drawBox(layout, start, end, 0f, palette, dark) }
    val fontSize = Theme.inlineCodeFont.fontSize.toPx()
    val pad = 2.dp.toPx()
    for (line in layout.getLineForOffset(start)..layout.getLineForOffset(end - 1)) {
        val from = max(start, layout.getLineStart(line))
        val to = min(end, layout.getLineEnd(line, visibleEnd = true))
        if (to <= from) continue
        val left = layout.getHorizontalPosition(from, usePrimaryDirection = true)
        val right = layout.getBoundingBox(to - 1).right
        val baseline = layout.getLineBaseline(line)
        drawRoundRect(
            palette.backgroundAccent,
            topLeft = Offset(left - pad, baseline - fontSize * 0.95f),
            size = Size(right - left + pad * 2, fontSize * 1.2f),
            cornerRadius = CornerRadius(Radius.xs.toPx()),
        )
    }
}
