package app.motile.usage

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.core.LimitsReport
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.TextSize
import app.motile.theme.iconSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.theme.at
import app.motile.ui.Icon
import app.motile.ui.Text

/** Narrower than this, a window puts its bar under its numbers. */
private val besideWidth = 520.dp

/** A window as the Limits tab draws it, its bar beside its numbers where the row is wide enough. */
@Composable
fun LimitWindowRow(window: LimitsReport.Window) {
    BoxWithConstraints(Modifier.fillMaxWidth()) { LimitRow(window, maxWidth >= besideWidth) }
}

/** One window of a plan: what was used of it, how that compares with the time passed, and a bar with when it starts over. */
@Composable
private fun LimitRow(window: LimitsReport.Window, beside: Boolean) {
    val color = window.color
    val padding = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 14.dp)
    if (beside) {
        Row(padding, horizontalArrangement = Arrangement.spacedBy(24.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.width(168.dp)) { Numbers(window) }
            LimitBar(window, color, Modifier.weight(1f))
        }
    } else {
        Column(padding, verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Numbers(window)
            LimitBar(window, color, Modifier.fillMaxWidth())
        }
    }
}

@Composable
private fun Numbers(window: LimitsReport.Window) {
    val palette = LocalPalette.current
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(window.label, size = TextSize.Sm, weight = FontWeight.Medium, color = palette.foreground, maxLines = 1)
        Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
            Text(window.used, Modifier.alignByBaseline(), style = Theme.uiDigits(TextSize.Xxl, FontWeight.SemiBold), color = palette.foreground)
            Text("used", Modifier.alignByBaseline(), size = TextSize.Sm, color = palette.mutedForeground)
        }
        window.pace?.let { pace ->
            Row(
                Modifier.semantics { contentDescription = "${pace.label}. ${pace.help}" },
                horizontalArrangement = Arrangement.spacedBy(5.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(pace.symbol, iconSize(text = TextSize.Xs), color = palette.mutedStrongerForeground)
                Text(pace.label, size = TextSize.Xs, color = palette.mutedStrongerForeground)
            }
        }
    }
}

private val LimitsReport.Window.Pace.symbol: Symbol
    get() = when (this) {
        LimitsReport.Window.Pace.Limited -> Symbol.Clock
        LimitsReport.Window.Pace.Ahead -> Symbol.TrendingUp
        LimitsReport.Window.Pace.On -> Symbol.Gauge
        LimitsReport.Window.Pace.Under -> Symbol.TrendingDown
    }

private val LimitsReport.Window.Pace.label: String
    get() = when (this) {
        LimitsReport.Window.Pace.Limited -> "Limit reached"
        LimitsReport.Window.Pace.Ahead -> "Ahead of pace"
        LimitsReport.Window.Pace.On -> "On pace"
        LimitsReport.Window.Pace.Under -> "Under pace"
    }

private val LimitsReport.Window.Pace.help: String
    get() = when (this) {
        LimitsReport.Window.Pace.Limited -> "Waits until the window starts over"
        LimitsReport.Window.Pace.Ahead -> "Used faster than the window passes"
        LimitsReport.Window.Pace.On -> "Used as fast as the window passes"
        LimitsReport.Window.Pace.Under -> "Room left for the rest of the window"
    }

/** What was used of a window as the part of a bar it fills, and at its end when the window starts over. */
@Composable
private fun LimitBar(window: LimitsReport.Window, color: Color, modifier: Modifier) {
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    val shape = RoundedCornerShape(Radius.md)
    val fill = color.at(Opacity.ColorTintChart)
    val text = Theme.uiDigits(TextSize.Xs, FontWeight.Medium)
    BoxWithConstraints(modifier.height(scaled(28f).dp).clip(shape).background(surface.color(Surface.Layer.Control))) {
        val filled = maxWidth * (window.usedPercent / 100).toFloat().coerceIn(0f, 1f)
        Box(Modifier.offset(x = filled - maxWidth).fillMaxSize().background(fill, shape))
        Row(
            Modifier.fillMaxSize().padding(horizontal = 9.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(window.used, Modifier.weight(1f), style = Theme.uiDigits(TextSize.Xs, FontWeight.SemiBold), color = palette.foreground, maxLines = 1)
            window.resetsIn?.let { Text(it, Modifier.semantics { contentDescription = "Starts over in $it" }, style = text, color = palette.foreground, maxLines = 1) }
        }
    }
}
