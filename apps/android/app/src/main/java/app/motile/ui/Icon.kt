package app.motile.ui

import android.graphics.Paint
import android.graphics.Rect
import android.graphics.Typeface
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect as ComposeRect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathOperation
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.core.content.res.ResourcesCompat
import app.motile.R
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.LocalForeground
import app.motile.theme.symbolSide

/** Lucide's font, which the symbols are characters of. */
private val lucide: Typeface by lazy { ResourcesCompat.getFont(Platform.context, R.font.lucide) ?: Typeface.DEFAULT }

/** How wide and tall a symbol drawn for text of `size` is, trimmed to what it fills or as its square. */
@Composable
fun symbolExtent(symbol: Symbol, size: Float, trimmed: Boolean = false): Pair<Dp, Dp> {
    val side = symbolSide(size)
    if (!trimmed || symbol.character == null) return side to side
    val density = LocalDensity.current
    val bounds = remember(symbol, side) { glyphBounds(symbol, with(density) { side.toPx() }) }
    return with(density) { bounds.width().toDp() to bounds.height().toDp() }
}

private fun glyphBounds(symbol: Symbol, sidePx: Float): Rect {
    val paint = Paint().apply { typeface = lucide; textSize = sidePx }
    val bounds = Rect()
    paint.getTextBounds(symbol.character ?: "", 0, (symbol.character ?: "").length, bounds)
    return bounds
}

/**
 * The symbol in one colour, sized for text of `size` as on the Mac, enlarged. `trimmed` leaves
 * out the empty room Lucide draws around it, for one like a chevron that fills little of its square.
 */
@Composable
fun Icon(symbol: Symbol, size: Float = 13f, modifier: Modifier = Modifier, color: Color = LocalForeground.current, trimmed: Boolean = false) {
    val (width, height) = symbolExtent(symbol, size, trimmed)
    val side = symbolSide(size)
    Canvas(modifier.size(width, height)) {
        drawSymbol(symbol, side.toPx(), color, trimmed)
    }
}

/** Draws the symbol in a square of `side` pixels at the top left, or its trimmed glyph. */
fun DrawScope.drawSymbol(symbol: Symbol, side: Float, color: Color, trimmed: Boolean = false) {
    val drawn = color.takeOrElse(Color.Black)
    when (symbol) {
        Symbol.Linear -> drawPath(linearLogo(side), drawn)
        Symbol.EffortLow -> gauge(0.25f, side, drawn)
        Symbol.EffortMedium -> gauge(0.5f, side, drawn)
        Symbol.EffortHigh -> gauge(0.75f, side, drawn)
        else -> {
            val character = symbol.character ?: return
            val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                typeface = lucide
                textSize = side
                this.color = drawn.toArgb()
            }
            // Lucide's glyphs fill their square from the baseline up.
            if (trimmed) {
                val bounds = glyphBounds(symbol, side)
                drawContext.canvas.nativeCanvas.drawText(character, -bounds.left.toFloat(), -bounds.top.toFloat(), paint)
            } else {
                drawContext.canvas.nativeCanvas.drawText(character, 0f, side, paint)
            }
        }
    }
}

private fun Color.takeOrElse(other: Color) = if (this == Color.Unspecified) other else this

/** A disc cut into stripes that fall to the right, in a square of 24 with the room Lucide leaves around its icons. A stripe is where `x + y` lies between two numbers. */
private fun linearLogo(side: Float): Path {
    val stripes = listOf(22.706f to 72f, 17.674f to 20.191f, 12.642f to 15.159f, -24f to 10.127f)
    val disc = Path().apply { addOval(ComposeRect(0f, 0f, 24f, 24f)) }
    val far = 48f
    val logo = Path()
    for ((from, to) in stripes) {
        val stripe = Path().apply {
            moveTo(from / 2 + far, from / 2 - far)
            lineTo(to / 2 + far, to / 2 - far)
            lineTo(to / 2 - far, to / 2 + far)
            lineTo(from / 2 - far, from / 2 + far)
            close()
        }
        val piece = Path().apply { op(disc, stripe, PathOperation.Intersect) }
        logo.addPath(piece)
    }
    // The disc is 20 of the square's 24, as Lucide leaves room around its icons; y runs down here, so the stripes are mirrored to fall right.
    val scale = side / 24 * 20 / 24
    val placed = Path()
    placed.addPath(logo)
    val matrix = androidx.compose.ui.graphics.Matrix().apply {
        translate(side / 2, side / 2)
        scale(scale, -scale)
        translate(-12f, -12f)
    }
    placed.transform(matrix)
    return placed
}

/** A ring as Lucide draws a circle, round a pie that fills `share` of it clockwise from the top, in a square of 24. */
private fun DrawScope.gauge(share: Float, side: Float, color: Color) {
    scale(side / 24, side / 24, pivot = Offset.Zero) {
        drawCircle(color, radius = 10f, center = Offset(12f, 12f), style = Stroke(width = 2f))
        drawArc(color, startAngle = -90f, sweepAngle = 360f * share, useCenter = true, topLeft = Offset(6f, 6f), size = androidx.compose.ui.geometry.Size(12f, 12f))
    }
}

/** A square symbol button's side for a finger: at least `Platform.minimumPress`. */
val pressSide: Dp get() = Platform.minimumPress.dp
