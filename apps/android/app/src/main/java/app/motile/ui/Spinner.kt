package app.motile.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.IconSize
import app.motile.theme.LocalForeground
import app.motile.theme.symbolSide
import kotlin.math.ceil

/** What is under way: Lucide's loader, turning, in the colour of the text around it. */
object Spinner {
    /** The loader reaches every side of its square, so it is drawn smaller in it to look as large as the symbols it stands in for. */
    const val fill = 0.85f
    /** Seconds a turn takes. */
    const val turn = 1.2f
}

@Composable
fun Spinner(size: IconSize = ControlSize.Regular.symbol, modifier: Modifier = Modifier, color: Color = LocalForeground.current) {
    val angle by rememberInfiniteTransition(label = "spinner").animateFloat(
        initialValue = 0f,
        targetValue = 360f,
        animationSpec = infiniteRepeatable(tween((Spinner.turn * 1000).toInt(), easing = LinearEasing), RepeatMode.Restart),
        label = "turn",
    )
    val side = symbolSide(size)
    Box(
        modifier.size(side).semantics { contentDescription = "Working" },
        contentAlignment = Alignment.Center,
    ) {
        Icon(Symbol.Loader, side * Spinner.fill, Modifier.graphicsLayer { rotationZ = angle }, color)
    }
}

/** A symbol that turns while `turning`, and finishes the turn it is on once that ends. */
@Composable
fun TurningSymbol(symbol: Symbol, turning: Boolean, size: IconSize = ControlSize.Regular.symbol, modifier: Modifier = Modifier, color: Color = LocalForeground.current) {
    val angle = remember { Animatable(0f) }
    val turnMillis = Spinner.turn * 1000
    LaunchedEffect(turning) {
        if (turning) {
            while (true) {
                angle.snapTo(angle.value % 360f)
                val left = 360f - angle.value
                angle.animateTo(360f, tween((left / 360f * turnMillis).toInt(), easing = LinearEasing))
            }
        }
        val end = ceil(angle.value / 360f) * 360f
        if (end > angle.value) angle.animateTo(end, tween(((end - angle.value) / 360f * turnMillis).toInt(), easing = LinearEasing))
        angle.snapTo(0f)
    }
    Icon(symbol, size, modifier.graphicsLayer { rotationZ = angle.value }, color)
}
