package app.motile.composer

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.compose.ui.util.lerp
import app.motile.LocalStore
import app.motile.theme.ControlSize
import app.motile.theme.IconSize
import app.motile.theme.LocalPalette
import app.motile.theme.floatingHeight
import app.motile.theme.symbolSide
import app.motile.usage.color
import app.motile.thread.effortSymbol
import app.motile.ui.ActionButton
import app.motile.ui.AgentIcon
import app.motile.ui.Appearing
import app.motile.ui.ButtonVariant
import kotlin.math.max
import kotlin.math.roundToInt

private val sideInset = 14.dp
private val topInset = 4.dp
/** The room the attach button takes at the row's start, and the send button at its end: a square as tall as the folded composer. */
private val buttonRoom = floatingHeight

/**
 * Places the composer's text and the row of its buttons. Collapsed, they are one line: the text
 * between the button that attaches and the one that sends. Otherwise the text is above the row.
 */
@Composable
fun ComposerRows(collapsed: Boolean, text: @Composable () -> Unit, controls: @Composable () -> Unit) {
    val expanded by animateFloatAsState(if (collapsed) 0f else 1f, tween(220, easing = FastOutSlowInEasing), label = "rows")
    Layout(contents = listOf(text, controls)) { (texts, rows), constraints ->
        val width = constraints.maxWidth
        val inset = lerp(buttonRoom.toPx(), sideInset.toPx(), expanded)
        val textWidth = max(0, (width - 2 * inset).roundToInt())
        val textPlaced = texts.first().measure(Constraints(minWidth = textWidth, maxWidth = textWidth))
        val row = rows.first().measure(Constraints(minWidth = width, maxWidth = width))
        val oneLine = max(textPlaced.height, row.height)
        val twoLines = topInset.toPx() + textPlaced.height + row.height
        val height = lerp(oneLine.toFloat(), twoLines, expanded).roundToInt()
        layout(width, height) {
            row.place(0, height - row.height)
            val y = lerp((height - textPlaced.height) / 2f, topInset.toPx(), expanded)
            textPlaced.place(inset.roundToInt(), y.roundToInt())
        }
    }
}

/**
 * The row of the composer's buttons: the one that attaches, the model's name with the effort and the server,
 * which opens the thread's settings, and the ones that stop and send. Collapsed, the text is in the
 * middle of the row instead of the names.
 */
@Composable
fun ComposerTouchControls(collapsed: Boolean) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        AttachMenu()
        Box(Modifier.weight(1f).padding(start = 8.dp), contentAlignment = Alignment.CenterEnd) {
            Appearing(!collapsed) { Settings() }
        }
        ComposerSendButtons()
    }
}

/** A narrow row names the model without its account, then leaves out the server, then the effort, so that the send button always has its place. */
@Composable
private fun Settings() {
    val store = LocalStore.current
    val short = store.composerModel?.shortName ?: store.composerModelLabel
    FirstThatFits(
        Modifier,
        { SettingsButton(store.composerModelLabel, effortShown = true, server = true) },
        { SettingsButton(short, effortShown = true, server = true) },
        { SettingsButton(short, effortShown = true, server = false) },
        { SettingsButton(short, effortShown = false, server = false) },
    )
}

@Composable
private fun SettingsButton(title: String, effortShown: Boolean, server: Boolean) {
    val store = LocalStore.current
    val model = store.composerModel
    val effort = store.composerEffort?.takeIf { effortShown && it.isNotEmpty() }?.let(::effortSymbol)
    val logo: (@Composable () -> Unit)? = model?.let { { AgentIcon(it.agent) } }
    val fill = store.composerContextFill
    val ring: (@Composable () -> Unit)? = fill.usedPercent?.let { { ContextRing(it, fill.tone.color, ControlSize.Regular.smallSymbol) } }
    Box(Modifier.semantics { contentDescription = "Thread settings" }) {
        ActionButton(
            title, picture = logo, detail = effort, detailMark = ring, place = store.composerServer?.shortName?.takeIf { server }, proxied = store.composerProxied,
            pinned = store.composerPinned, variant = ButtonVariant.Ghost, gap = 5.dp,
        ) { store.showsThreadSettings = true }
    }
}

/**
 * How full the context window is: a ring drawn as a Lucide circle, 22 wide with its stroke in a
 * square of 24, a step quieter than the dots beside it, with an arc in `color` over it from the top
 * round clockwise.
 */
@Composable
private fun ContextRing(usedPercent: Double, color: Color, size: IconSize) {
    val track = LocalPalette.current.mutedStrongestPlusForeground
    val used = (usedPercent / 100).toFloat().coerceIn(0f, 1f)
    Canvas(Modifier.size(symbolSide(size))) {
        val line = this.size.width / 12
        val inset = this.size.width / 24 + line / 2
        val topLeft = Offset(inset, inset)
        val circle = Size(this.size.width - inset * 2, this.size.height - inset * 2)
        drawArc(track, 0f, 360f, useCenter = false, topLeft = topLeft, size = circle, style = Stroke(line))
        if (used > 0f) drawArc(color, -90f, 360f * used, useCenter = false, topLeft = topLeft, size = circle, style = Stroke(line, cap = StrokeCap.Round))
    }
}
