package app.motile.ui

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import app.motile.theme.LocalDark
import app.motile.theme.Opacity
import app.motile.theme.value

/** The opacity of what stands in for what is yet to come, fading in and out. */
@Composable
fun pulse(): State<Float> {
    val faded = Opacity.Disabled.value(LocalDark.current)
    return rememberInfiniteTransition(label = "placeholder").animateFloat(1f, faded, infiniteRepeatable(tween(800), RepeatMode.Reverse), label = "fade")
}

/** Fades in and out, as what stands in for what is yet to come. */
@Composable
fun Modifier.pulsing(): Modifier {
    val alpha by pulse()
    return graphicsLayer { this.alpha = alpha }
}
