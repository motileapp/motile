package app.motile.transcript.canvas

import android.content.Context
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.TileMode
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.text.drawText
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.sp
import app.motile.theme.Opacity
import app.motile.theme.Palette
import app.motile.theme.Surface
import app.motile.theme.at
import java.util.IdentityHashMap

/** Measures text off the main thread at one density, for scenes and the legends around them. */
class CanvasText(context: Context, val density: Density) {
    private val measurer = TextMeasurer(createFontFamilyResolver(context), density, LayoutDirection.Ltr, cacheSize = 0)

    @Synchronized
    fun measure(text: String, style: TextStyle, width: Int = Constraints.Infinity, wraps: Boolean = true): TextLayoutResult =
        measurer.measure(text, style, softWrap = wraps, maxLines = if (wraps) Int.MAX_VALUE else 1, constraints = Constraints(maxWidth = width))

    companion object {
        private val made = HashMap<Density, CanvasText>()

        /** The one for the density, made the first time. */
        @Synchronized
        fun of(context: Context, density: Density): CanvasText = made.getOrPut(density) { CanvasText(context.applicationContext, density) }
    }
}

/** How the scene's fonts are set: at the scene's own sizes, which aren't enlarged, since core placed them in points. */
internal fun SceneFont.style(density: Density): TextStyle {
    val (size, weight, digits) = when (this) {
        SceneFont.Tick -> Triple(11f, SceneFont.Weight.Regular, true)
        SceneFont.Caption -> Triple(12f, SceneFont.Weight.Regular, false)
        is SceneFont.Ui -> Triple(this.size, this.weight, false)
    }
    val fontWeight = when (weight) {
        SceneFont.Weight.Regular -> FontWeight.Normal
        SceneFont.Weight.Medium -> FontWeight.Medium
        SceneFont.Weight.Semibold -> FontWeight.SemiBold
    }
    return TextStyle(fontSize = (size / density.fontScale).sp, fontWeight = fontWeight, fontFeatureSettings = if (digits) "tnum" else null)
}

/**
 * A scene ready to paint: its paths built and its labels measured, at the text's density. Made off
 * the main thread, so that painting is all a frame does.
 */
class CanvasScene(val scene: Scene, val text: CanvasText) {
    private val paths = IdentityHashMap<SceneItem, Path>()
    private val labels = IdentityHashMap<SceneItem.Label, TextLayoutResult>()

    init {
        (scene.items + scene.hits.flatMap { it.lit }).forEach(::prepare)
    }

    private fun prepare(item: SceneItem) {
        when (item) {
            is SceneItem.Label -> labels[item] = text.measure(item.text, item.font.style(text.density), wraps = false)
            else -> outline(item)?.let { paths[item] = it }
        }
    }

    internal fun path(item: SceneItem): Path? = paths[item]

    internal fun label(item: SceneItem.Label): TextLayoutResult? = labels[item]
}

private fun outline(item: SceneItem): Path? = when (item) {
    is SceneItem.Path -> path(item.verbs, item.points)
    is SceneItem.Rect -> Path().apply {
        addRoundRect(RoundRect(item.x, item.y, item.x + item.width, item.y + item.height, CornerRadius(item.rx, item.ry)))
    }
    is SceneItem.Circle -> Path().apply {
        addOval(androidx.compose.ui.geometry.Rect(Offset(item.cx, item.cy), item.r))
    }
    is SceneItem.Label -> null
}

private fun path(verbs: String, points: FloatArray): Path? {
    val path = Path().apply { fillType = PathFillType.NonZero }
    var at = 0
    fun has(count: Int) = at + count * 2 <= points.size
    for (verb in verbs) {
        when (verb) {
            'M' -> {
                if (!has(1)) return null
                path.moveTo(points[at], points[at + 1])
                at += 2
            }
            'L' -> {
                if (!has(1)) return null
                path.lineTo(points[at], points[at + 1])
                at += 2
            }
            'Q' -> {
                if (!has(2)) return null
                path.quadraticTo(points[at], points[at + 1], points[at + 2], points[at + 3])
                at += 4
            }
            'C' -> {
                if (!has(3)) return null
                path.cubicTo(points[at], points[at + 1], points[at + 2], points[at + 3], points[at + 4], points[at + 5])
                at += 6
            }
            'Z' -> path.close()
            else -> return null
        }
    }
    return path
}

/** The colours of a scene on the surface it lies on, in one appearance, with `current` the text's colour around it. */
class SceneColors(val palette: Palette, val dark: Boolean, val surface: Surface, val current: Color) {
    fun of(color: SceneColor, opacity: Float = 1f): Color {
        val resolved = when (color) {
            SceneColor.Current -> current
            is SceneColor.Token -> token(color.name)
            is SceneColor.Wash -> token(color.name).at(Opacity.ColorTint, dark)
            is SceneColor.Surface -> when (color.layer) {
                SceneColor.Layer.Base -> surface.color(palette)
                SceneColor.Layer.Border -> surface.border(palette)
                SceneColor.Layer.Row -> surface.color(Surface.Layer.Row, palette)
                SceneColor.Layer.Control -> surface.color(Surface.Layer.Control, palette)
                SceneColor.Layer.Box -> surface.color(Surface.Layer.Box, palette)
            }
            is SceneColor.Rgba -> Color(color.r, color.g, color.b, color.a)
        }
        return resolved.copy(alpha = (resolved.alpha * opacity).coerceIn(0f, 1f))
    }

    private fun token(name: String): Color = palette.named(name) ?: palette.foreground
}

/** Paints the scene in points from the scope's top left, with the hit `lit` lit. The scope itself stays transparent; its density is the one the scene's text was measured at. */
fun DrawScope.paintScene(canvas: CanvasScene, colors: SceneColors, lit: Int? = null) {
    val items = canvas.scene.items
    val hit = lit?.let { canvas.scene.hits.getOrNull(it) }
    items.forEachIndexed { index, item ->
        if (hit != null && hit.at == index) hit.lit.forEach { draw(canvas, it, colors) }
        draw(canvas, item, colors)
    }
    if (hit != null && hit.at >= items.size) hit.lit.forEach { draw(canvas, it, colors) }
}

private fun DrawScope.draw(canvas: CanvasScene, item: SceneItem, colors: SceneColors) {
    if (item is SceneItem.Label) return label(canvas, item, colors)
    val path = canvas.path(item) ?: return
    val paint = when (item) {
        is SceneItem.Path -> item.paint
        is SceneItem.Rect -> item.paint
        is SceneItem.Circle -> item.paint
        is SceneItem.Label -> return
    }
    val scale = density
    withTransform({ scale(scale, scale, Offset.Zero) }) {
        fill(paint, colors)?.let { drawPath(path, it) }
        val stroke = paint.stroke ?: return@withTransform
        drawPath(path, colors.of(stroke.color, paint.opacity), style = stroke.style())
    }
}

private fun fill(paint: ScenePaint, colors: SceneColors): Brush? {
    val gradient = paint.gradient?.let { gradient ->
        val stops = gradient.stops.map { it.at to colors.of(it.color, it.opacity * paint.opacity) }
        when (stops.size) {
            0 -> null
            1 -> SolidColor(stops[0].second)
            else -> Brush.linearGradient(*stops.toTypedArray(), start = Offset(gradient.x1, gradient.y1), end = Offset(gradient.x2, gradient.y2), tileMode = TileMode.Clamp)
        }
    }
    return gradient ?: paint.fill?.let { SolidColor(colors.of(it, paint.fillOpacity * paint.opacity)) }
}

private fun SceneStroke.style(): Stroke = Stroke(
    width = width,
    miter = 4f,
    cap = when (cap) {
        SceneStroke.Cap.Butt -> StrokeCap.Butt
        SceneStroke.Cap.Round -> StrokeCap.Round
        SceneStroke.Cap.Square -> StrokeCap.Square
    },
    join = when (join) {
        SceneStroke.Join.Miter -> StrokeJoin.Miter
        SceneStroke.Join.Round -> StrokeJoin.Round
        SceneStroke.Join.Bevel -> StrokeJoin.Bevel
    },
    pathEffect = dash.takeIf(::dashes)?.let { PathEffect.dashPathEffect(it, 0f) },
)

/** Dashes as SVG and the reference take them: pairs of lengths, none negative, not all nothing. */
private fun dashes(lengths: FloatArray): Boolean =
    lengths.size >= 2 && lengths.size % 2 == 0 && lengths.all { it >= 0f && it.isFinite() } && lengths.sum() > 0f

private fun DrawScope.label(canvas: CanvasScene, label: SceneItem.Label, colors: SceneColors) {
    val layout = canvas.label(label) ?: return
    val advance = layout.getLineRight(0) - layout.getLineLeft(0)
    val start = label.x * density - when (label.anchor) {
        SceneItem.Anchor.Start -> 0f
        SceneItem.Anchor.Middle -> advance / 2
        SceneItem.Anchor.End -> advance
    }
    drawText(layout, colors.of(label.color, label.opacity), Offset(start, label.y * density - layout.firstBaseline))
}
