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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.graphics.vector.PathParser
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
        Symbol.Linear -> drawPath(logo(linearLogo, side), drawn)
        Symbol.Ricrac -> drawPath(logo(ricracLogo, side, share = 20f / 24), drawn)
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

private val linearLogo: Path by lazy { PathParser().parsePathString("M2.3 14.4 9.6 21.7a10 10 0 0 1-7.3-7.3ZM2 11.4 12.6 22a10 10 0 0 0 2.6-.6L2.6 8.8a10 10 0 0 0-.6 2.6ZM3.3 7 17 20.7a10 10 0 0 0 1.9-1.4L4.7 5.1A10 10 0 0 0 3.3 7ZM5.8 3.8a10 10 0 0 1 14.4 14.4Z").toPath() }

private val ricracLogo: Path by lazy { PathParser().parsePathString("M10 0a1 1 0 0 1 1 1v22a1 1 0 1 1-2 0V1a1 1 0 0 1 1-1zm4 0a1 1 0 0 1 1 1v22a1 1 0 1 1-2 0V1a1 1 0 0 1 1-1zM5.454 1.941a1 1 0 0 1 1.092 1.676A9.991 9.991 0 0 0 2 12a9.992 9.992 0 0 0 4.546 8.384 1 1 0 0 1-1.092 1.676A11.993 11.993 0 0 1 0 12C0 7.787 2.173 4.08 5.454 1.941zm11.708.292a1 1 0 0 1 1.384-.292A11.992 11.992 0 0 1 24 12c0 4.213-2.173 7.92-5.454 10.06a1.001 1.001 0 0 1-1.092-1.676A9.992 9.992 0 0 0 22 12a9.99 9.99 0 0 0-4.546-8.383 1 1 0 0 1-.292-1.384zM5.435 7.428a1 1 0 0 1 1.64 1.144A5.968 5.968 0 0 0 6 12c0 1.276.398 2.456 1.075 3.428a1 1 0 0 1-1.64 1.144A7.968 7.968 0 0 1 4 12c0-1.698.53-3.276 1.435-4.572zm11.738-.248a1 1 0 0 1 1.392.248A7.968 7.968 0 0 1 20 12c0 1.698-.53 3.276-1.435 4.572a1 1 0 0 1-1.64-1.144A5.967 5.967 0 0 0 18 12a5.968 5.968 0 0 0-1.075-3.428 1 1 0 0 1 .248-1.392z").toPath() }

/** An SVG's path in a square of 24, `share` of it across, in a square of `side`. */
private fun logo(mark: Path, side: Float, share: Float = 1f): Path {
    val placed = Path()
    placed.addPath(mark)
    placed.transform(androidx.compose.ui.graphics.Matrix().apply {
        translate(side / 2, side / 2)
        scale(side / 24 * share, side / 24 * share)
        translate(-12f, -12f)
    })
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
