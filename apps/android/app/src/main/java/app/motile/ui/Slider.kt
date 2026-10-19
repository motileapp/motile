package app.motile.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.awaitHorizontalTouchSlopOrCancellation
import androidx.compose.foundation.gestures.horizontalDrag
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.theme.opacity
import kotlin.math.roundToInt

/** How far the thumb's centre stands from the track's ends. */
private val thumbInset = 12.dp
private val thumbSide = 18.dp
private val marksHeight = 18.dp
/** How far the marks' row reaches up into the control. */
private val marksOverlap = 2.dp

/**
 * A number between `range`'s ends, set by pressing or dragging along a round track, or with the arrow
 * keys, snapped to `step` (a hundredth of the range without one). The `label` and the `readout` stand
 * over it, the `marks`' titles under it. It keeps its own value while it is dragged, and says when a
 * drag starts and ends with `onEditing`.
 */
@Composable
fun Slider(
    value: Double,
    range: ClosedFloatingPointRange<Double>,
    onChange: (Double) -> Unit,
    modifier: Modifier = Modifier,
    step: Double? = null,
    label: String? = null,
    readout: String? = null,
    marks: List<Pair<Double, String>> = emptyList(),
    disabled: Boolean = false,
    onEditing: (Boolean) -> Unit = {},
) {
    val enabled = LocalEnabled.current && !disabled
    Column(modifier.fillMaxWidth().opacity(Opacity.Disabled, on = !enabled)) {
        if (label != null || readout != null) SliderHead(label, readout)
        val control = ControlSize.Large.height
        val tall = if (marks.isEmpty()) control else control - marksOverlap + marksHeight
        Box(Modifier.fillMaxWidth().height(tall)) {
            SliderControl(value, range, step, label, readout, enabled, onChange, onEditing)
            if (marks.isNotEmpty()) SliderMarks(marks, range, Modifier.offset(y = control - marksOverlap))
        }
    }
}

@Composable
private fun SliderHead(label: String?, readout: String?) {
    val palette = LocalPalette.current
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        if (label != null) {
            Text(label, Modifier.weight(1f).alignByBaseline(), style = Theme.ui(13f).lined(20f), color = palette.foreground)
        } else {
            Spacer(Modifier.weight(1f))
        }
        if (readout != null) {
            Text(
                readout, Modifier.alignByBaseline(), style = Theme.uiDigits(12f, FontWeight.Medium).lined(18f), color = palette.foreground,
                maxLines = 1, overflow = TextOverflow.Visible,
            )
        }
    }
}

@Composable
private fun SliderControl(
    value: Double,
    range: ClosedFloatingPointRange<Double>,
    step: Double?,
    label: String?,
    readout: String?,
    enabled: Boolean,
    onChange: (Double) -> Unit,
    onEditing: (Boolean) -> Unit,
) {
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    val interaction = remember { MutableInteractionSource() }
    val hovered by interaction.collectIsHoveredAsState()
    val focused by interaction.collectIsFocusedAsState()
    var dragging by remember { mutableStateOf(false) }
    var held by remember { mutableStateOf<Double?>(null) }
    LaunchedEffect(value) { if (!dragging) held = null }
    val current by rememberUpdatedState(value)
    val bounds by rememberUpdatedState(range)
    val stride by rememberUpdatedState(step)
    val changed by rememberUpdatedState(onChange)
    val editing by rememberUpdatedState(onEditing)
    val shown = held ?: value
    val track by animateColorAsState(
        surface.color(if (enabled && (hovered || dragging)) Surface.Layer.ControlLit else Surface.Layer.Control), tween(120), label = "track",
    )
    val thumb = palette.foreground

    fun set(next: Double) {
        val snapped = sliderSnap(next, bounds, stride)
        val before = held ?: current
        held = snapped
        if (snapped != before) changed(snapped)
    }

    val trackHeight = ControlSize.Small.height
    val input = if (!enabled) Modifier else Modifier
        .pointerInput(Unit) {
            fun at(x: Float): Double {
                val inset = thumbInset.toPx()
                val t = ((x - inset) / (size.width - inset * 2)).coerceIn(0f, 1f)
                return bounds.start + t * (bounds.endInclusive - bounds.start)
            }
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                dragging = true
                editing(true)
                try {
                    set(at(down.position.x))
                    val moved = awaitHorizontalTouchSlopOrCancellation(down.id) { change, _ ->
                        change.consume()
                        set(at(change.position.x))
                    }
                    if (moved != null) {
                        horizontalDrag(moved.id) { change ->
                            change.consume()
                            set(at(change.position.x))
                        }
                    }
                } finally {
                    dragging = false
                    editing(false)
                }
            }
        }
        .onKeyEvent { event ->
            if (event.type != KeyEventType.KeyDown) return@onKeyEvent false
            val by = sliderStride(bounds, stride)
            val now = held ?: current
            when (event.key) {
                Key.DirectionLeft, Key.DirectionDown -> set(now - by)
                Key.DirectionRight, Key.DirectionUp -> set(now + by)
                Key.MoveHome -> set(bounds.start)
                Key.MoveEnd -> set(bounds.endInclusive)
                else -> return@onKeyEvent false
            }
            true
        }
    Box(
        Modifier
            .fillMaxWidth()
            .height(ControlSize.Large.height)
            .semantics {
                if (label != null) contentDescription = label
                stateDescription = readout ?: sliderNumber(shown)
                progressBarRangeInfo = ProgressBarRangeInfo(
                    shown.toFloat(), range.start.toFloat()..maxOf(range.start, range.endInclusive).toFloat(), sliderSteps(range, step),
                )
                if (enabled) setProgress { set(it.toDouble()); true } else disabled()
            }
            .then(input)
            .hoverable(interaction, enabled)
            .focusable(enabled, interaction)
            .drawWithContent {
                drawContent()
                val inset = thumbInset.toPx()
                val x = inset + (size.width - inset * 2) * sliderFraction(shown, range)
                drawCircle(thumb, radius = thumbSide.toPx() / 2, center = Offset(x, size.height / 2))
            },
        contentAlignment = Alignment.CenterStart,
    ) {
        Box(Modifier.fillMaxWidth().height(trackHeight).focusRing(focused, trackHeight / 2).background(track, CircleShape))
    }
}

@Composable
private fun SliderMarks(marks: List<Pair<Double, String>>, range: ClosedFloatingPointRange<Double>, modifier: Modifier) {
    val muted = LocalPalette.current.mutedForeground
    val style = Theme.uiDigits(11f).lined(14f)
    Layout(
        content = { marks.forEach { Text(it.second, style = style, color = muted, maxLines = 1, overflow = TextOverflow.Visible) } },
        modifier = modifier.fillMaxWidth().height(marksHeight),
    ) { measurables, constraints ->
        val width = constraints.maxWidth
        val height = marksHeight.roundToPx()
        val inset = thumbInset.toPx()
        val titles = measurables.map { it.measure(Constraints()) }
        layout(width, height) {
            titles.forEachIndexed { index, title ->
                val t = sliderFraction(marks[index].first, range)
                val x = when {
                    index == 0 && t <= 0.02f -> 0
                    index == titles.lastIndex && t >= 0.98f -> width - title.width
                    else -> (inset + (width - inset * 2) * t - title.width / 2f).roundToInt()
                }
                title.place(x, (height - title.height) / 2)
            }
        }
    }
}

/** Where the value lies between the range's ends, from 0 to 1. */
internal fun sliderFraction(value: Double, range: ClosedFloatingPointRange<Double>): Float {
    val span = range.endInclusive - range.start
    if (span <= 0) return 0f
    return ((value - range.start) / span).toFloat().coerceIn(0f, 1f)
}

/** The step a slider moves by: `step`, or a hundredth of the range without one. */
internal fun sliderStride(range: ClosedFloatingPointRange<Double>, step: Double?): Double =
    step?.takeIf { it > 0 } ?: ((range.endInclusive - range.start) / 100)

/** The value at the nearest step from the range's start, inside the range. */
internal fun sliderSnap(value: Double, range: ClosedFloatingPointRange<Double>, step: Double?): Double {
    val (min, max) = range.start to range.endInclusive
    if (max <= min) return min
    val stride = sliderStride(range, step)
    return (min + Math.round((value - min) / stride) * stride).coerceIn(min, max)
}

/** How many values lie between the range's ends, for accessibility to step through. */
private fun sliderSteps(range: ClosedFloatingPointRange<Double>, step: Double?): Int {
    val span = range.endInclusive - range.start
    if (span <= 0) return 0
    return ((span / sliderStride(range, step)).roundToInt() - 1).coerceAtLeast(0)
}

private fun sliderNumber(value: Double): String =
    if (value == Math.rint(value) && kotlin.math.abs(value) < 1e15) value.toLong().toString() else value.toString()
