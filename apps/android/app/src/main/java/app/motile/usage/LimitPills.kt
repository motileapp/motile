package app.motile.usage

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.motile.core.LimitsReport
import app.motile.platform.scaled
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Surface
import app.motile.theme.at

/** An account's windows as thin pills side by side, each filled as far as it was used. */
@Composable
fun LimitPills(windows: List<LimitsReport.Window>, modifier: Modifier = Modifier) {
    val track = LocalSurface.current.color(Surface.Layer.Control)
    Row(modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        for (window in windows) {
            Box(
                Modifier
                    .weight(1f)
                    .height(scaled(4f).dp)
                    .clip(CircleShape)
                    .background(track)
                    .semantics { contentDescription = description(window) },
            ) {
                val used = (window.usedPercent / 100).toFloat().coerceIn(0f, 1f)
                Box(Modifier.fillMaxWidth(used).fillMaxHeight().background(window.color.at(Opacity.ColorTintChartStrongest), CircleShape))
            }
        }
    }
}

/** "Session · 34% used · resets in 3h 30m". */
private fun description(window: LimitsReport.Window): String =
    listOfNotNull(window.label, "${window.used} used", window.resetsIn?.let { "resets in $it" }).joinToString(" · ")

val LimitsReport.Window.color: Color
    @Composable @ReadOnlyComposable get() {
        val palette = LocalPalette.current
        return when (tone) {
            LimitsReport.Window.Tone.Success -> palette.success
            LimitsReport.Window.Tone.Pending -> palette.pending
            LimitsReport.Window.Tone.Warning -> palette.warning
        }
    }
