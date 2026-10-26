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
import app.motile.theme.IconSize
import app.motile.theme.LocalForeground
import app.motile.theme.symbolSide

/** Lucide's font, which the symbols are characters of. */
private val lucide: Typeface by lazy { ResourcesCompat.getFont(Platform.context, R.font.lucide) ?: Typeface.DEFAULT }

/** How wide and tall a symbol of `size` is, trimmed to what it fills or as its square. */
@Composable
fun symbolExtent(symbol: Symbol, size: IconSize, trimmed: Boolean = false): Pair<Dp, Dp> {
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
 * The symbol in one colour, in the square of `size`. `trimmed` leaves out the empty room Lucide
 * draws around it, for one like a chevron that fills little of its square.
 */
@Composable
fun Icon(symbol: Symbol, size: IconSize = IconSize.Md, modifier: Modifier = Modifier, color: Color = LocalForeground.current, trimmed: Boolean = false) {
    val (width, height) = symbolExtent(symbol, size, trimmed)
    val side = symbolSide(size)
    Canvas(modifier.size(width, height)) {
        drawSymbol(symbol, side.toPx(), color, trimmed)
    }
}

/** The same in a square exactly `side` wide, for one sized from what is around it. */
@Composable
fun Icon(symbol: Symbol, side: Dp, modifier: Modifier = Modifier, color: Color = LocalForeground.current) {
    Canvas(modifier.size(side)) { drawSymbol(symbol, size.width, color) }
}

/** A character of Lucide's font that no `Symbol` names, as core gives it, filling a square of `side`. */
@Composable
fun Glyph(character: String, side: Dp, modifier: Modifier = Modifier, color: Color = LocalForeground.current) {
    Canvas(modifier.size(side)) { drawGlyph(character, size.width, color) }
}

/** Draws a character of Lucide's font in a square of `side` pixels at the top left. */
fun DrawScope.drawGlyph(character: String, side: Float, color: Color) {
    val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = lucide
        textSize = side
        this.color = color.takeOrElse(Color.Black).toArgb()
    }
    drawContext.canvas.nativeCanvas.drawText(character, 0f, side, paint)
}

/** Draws the symbol in a square of `side` pixels at the top left, or its trimmed glyph. */
fun DrawScope.drawSymbol(symbol: Symbol, side: Float, color: Color, trimmed: Boolean = false) {
    val drawn = color.takeOrElse(Color.Black)
    when (symbol) {
        Symbol.Claude -> drawPath(logo(claudeLogo, side, share = 22f / 24), drawn)
        Symbol.OpenAI -> drawPath(logo(openAILogo, side, share = 22f / 24), drawn)
        Symbol.GitHub -> drawPath(logo(githubLogo, side, share = 20f / 24), drawn)
        Symbol.Linear -> drawPath(logo(linearLogo, side, share = 0.88f), drawn)
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

private val claudeLogo: Path by lazy { PathParser().parsePathString("m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z").toPath() }

private val openAILogo: Path by lazy { PathParser().parsePathString("M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z").toPath() }

private val githubLogo: Path by lazy { PathParser().parsePathString("M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12").toPath() }

private val linearLogo: Path by lazy { PathParser().parsePathString("M2.3 14.4 9.6 21.7a10 10 0 0 1-7.3-7.3ZM2 11.4 12.6 22a10 10 0 0 0 2.6-.6L2.6 8.8a10 10 0 0 0-.6 2.6ZM3.3 7 17 20.7a10 10 0 0 0 1.9-1.4L4.7 5.1A10 10 0 0 0 3.3 7ZM5.8 3.8a10 10 0 0 1 14.4 14.4Z").toPath() }

private val ricracLogo: Path by lazy { PathParser().parsePathString("M10 0a1 1 0 0 1 1 1v22a1 1 0 1 1-2 0V1a1 1 0 0 1 1-1zm4 0a1 1 0 0 1 1 1v22a1 1 0 1 1-2 0V1a1 1 0 0 1 1-1zM5.454 1.941a1 1 0 0 1 1.092 1.676A9.991 9.991 0 0 0 2 12a9.992 9.992 0 0 0 4.546 8.384 1 1 0 0 1-1.092 1.676A11.993 11.993 0 0 1 0 12C0 7.787 2.173 4.08 5.454 1.941zm11.708.292a1 1 0 0 1 1.384-.292A11.992 11.992 0 0 1 24 12c0 4.213-2.173 7.92-5.454 10.06a1.001 1.001 0 0 1-1.092-1.676A9.992 9.992 0 0 0 22 12a9.99 9.99 0 0 0-4.546-8.383 1 1 0 0 1-.292-1.384zM5.435 7.428a1 1 0 0 1 1.64 1.144A5.968 5.968 0 0 0 6 12c0 1.276.398 2.456 1.075 3.428a1 1 0 0 1-1.64 1.144A7.968 7.968 0 0 1 4 12c0-1.698.53-3.276 1.435-4.572zm11.738-.248a1 1 0 0 1 1.392.248A7.968 7.968 0 0 1 20 12c0 1.698-.53 3.276-1.435 4.572a1 1 0 0 1-1.64-1.144A5.967 5.967 0 0 0 18 12a5.968 5.968 0 0 0-1.075-3.428 1 1 0 0 1 .248-1.392z").toPath() }

/**
 * An SVG's path in a square of 24, drawn `share` of a square of `side` across, centred. A logo is
 * drawn as large as Lucide's icons look: one that fills its square, or is solid, is drawn smaller.
 */
private fun logo(mark: Path, side: Float, share: Float): Path {
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
