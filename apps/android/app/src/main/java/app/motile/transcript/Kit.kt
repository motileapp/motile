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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameMillis
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.CornerRadius
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
 * around it, and lights to `hover` where given; a `primary` one is filled; the others light under a
 * finger. `margin` is room around it that still takes the press, so buttons that touch each other
 * and the row's edge look apart.
 */
@Composable
fun RowButton(
    title: String,
    help: String,
    modifier: Modifier = Modifier,
    bordered: Boolean = false,
    primary: Boolean = false,
    hover: Color? = null,
    fills: Boolean = false,
    margin: PaddingValues = PaddingValues(0.dp),
    onClick: () -> Unit,
) {
    val variant = when {
        primary -> ButtonVariant.Primary
        bordered -> ButtonVariant.Outline
        else -> ButtonVariant.Ghost
    }
    ActionButton(
        title = title, help = help, modifier = modifier, variant = variant, size = RowButton.size, fills = fills,
        hover = hover.takeIf { !primary }, margin = margin, onClick = onClick,
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
fun PictureBox(picture: ImageBitmap?, modifier: Modifier = Modifier, content: @Composable BoxScope.() -> Unit = {}) {
    val palette = LocalPalette.current
    val shape = RoundedCornerShape(PictureBox.radius)
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

/**
 * Text with what its text system can't draw drawn behind it: the rounded boxes of inline code and
 * the chips of skills, from the marks `Typesetter` left in it.
 */
@Composable
fun MarkedText(text: AnnotatedString, style: TextStyle, modifier: Modifier = Modifier) {
    val palette = LocalPalette.current
    val dark = LocalDark.current
    val marks = remember(text) {
        text.getStringAnnotations(TextMarks.code, 0, text.length) + text.getStringAnnotations(TextMarks.skill, 0, text.length)
    }
    var layout by remember(text) { mutableStateOf<TextLayoutResult?>(null) }
    val drawn = if (marks.isEmpty()) modifier else modifier.drawBehind {
        val shown = layout ?: return@drawBehind
        for (mark in marks) drawMark(shown, mark.start, mark.end, mark.tag == TextMarks.skill, palette, dark)
    }
    Text(text, drawn, style = style, onTextLayout = { layout = it })
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
