package app.motile.usage

import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.unit.IntSize

/**
 * Points at what is under a finger sliding sideways across a chart, or the one tapped, which a tap
 * on it again lets go. A drag up or down is left to the page's scroll. [at] is what lies at a place,
 * if anything does.
 */
fun Modifier.chartPointing(
    pointed: Int?,
    point: (Int?) -> Unit,
    at: (Offset, IntSize) -> Int?,
): Modifier = composed {
    val haptics = LocalHapticFeedback.current
    val current by rememberUpdatedState(pointed)
    val set by rememberUpdatedState(point)
    val find by rememberUpdatedState(at)
    pointerInput(Unit) {
        detectTapGestures { offset ->
            val tapped = find(offset, size) ?: return@detectTapGestures
            if (tapped != current) haptics.performHapticFeedback(HapticFeedbackType.SegmentFrequentTick)
            set(if (tapped == current) null else tapped)
        }
    }.pointerInput(Unit) {
        var last: Int? = null
        fun slid(offset: Offset) {
            val under = find(offset, size) ?: return
            if (under == last) return
            last = under
            haptics.performHapticFeedback(HapticFeedbackType.SegmentFrequentTick)
            set(under)
        }
        detectHorizontalDragGestures(onDragStart = { last = current; slid(it) }) { change, _ ->
            change.consume()
            slid(change.position)
        }
    }
}
