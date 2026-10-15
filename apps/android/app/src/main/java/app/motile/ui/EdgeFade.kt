package app.motile.ui

import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.platform.scaled
import kotlin.math.min

/** A scroll view's mask that fades its content out at an edge with more behind it, the further the more is hidden, up to `length`. */
object EdgeFade {
    val length = scaled(24f).dp
}

/** Fades the content out at its top and bottom by how much is hidden past each, in pixels, read as it draws. */
fun Modifier.edgeFade(above: () -> Float, below: () -> Float): Modifier =
    graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
        .drawWithContent {
            drawContent()
            val length = EdgeFade.length.toPx()
            if (size.height <= 0f) return@drawWithContent
            val edge = min(0.5f, length / size.height)
            val top = 1f - min(1f, above() / length)
            val bottom = 1f - min(1f, below() / length)
            val mask = Brush.verticalGradient(
                0f to Color.Black.copy(alpha = top),
                edge to Color.Black,
                1f - edge to Color.Black,
                1f to Color.Black.copy(alpha = bottom),
            )
            drawRect(mask, blendMode = BlendMode.DstIn)
        }

/** Fades a scrolling column out at an edge with more behind it. Put it before the `verticalScroll` of the same state. */
fun Modifier.fadesHiddenEdges(state: ScrollState): Modifier =
    edgeFade(above = { state.value.toFloat() }, below = { (state.maxValue - state.value).toFloat() })

/** Fades a lazy list out at an edge with more behind it. */
fun Modifier.fadesHiddenEdges(state: LazyListState): Modifier = edgeFade(
    above = {
        if (state.firstVisibleItemIndex > 0) Float.MAX_VALUE else state.firstVisibleItemScrollOffset.toFloat()
    },
    below = {
        val info = state.layoutInfo
        val last = info.visibleItemsInfo.lastOrNull()
        when {
            last == null -> 0f
            last.index < info.totalItemsCount - 1 -> Float.MAX_VALUE
            else -> (last.offset + last.size - info.viewportEndOffset + info.afterContentPadding).coerceAtLeast(0).toFloat()
        }
    },
)

/** Fades content out under a top bar `top` tall, as the transcript does, but for a strip `keep` wide at the end, where a scroll indicator stands. */
fun Modifier.fadesUnderTopBar(top: Dp, keep: Dp = 0.dp): Modifier =
    graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
        .drawWithContent {
            drawContent()
            val start = top.toPx()
            val end = start + 20.dp.toPx()
            val kept = keep.toPx()
            val width = size.width - kept
            drawRect(Color.Transparent, size = Size(width, start), blendMode = BlendMode.Clear)
            drawRect(
                Brush.verticalGradient(0f to Color.Transparent, 1f to Color.Black, startY = start, endY = end),
                topLeft = Offset(0f, start),
                size = Size(width, end - start),
                blendMode = BlendMode.DstIn,
            )
        }
