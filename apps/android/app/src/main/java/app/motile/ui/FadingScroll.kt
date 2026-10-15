package app.motile.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * Content as tall as it is up to `maxHeight`, past which it scrolls, fading out at an edge with more
 * behind it. With `anchor` at the bottom it starts at the end and stays there while it grows, as an
 * output that streams does. `padding` insets the content inside the scrolling edges.
 */
@Composable
fun FadingScroll(
    maxHeight: Dp,
    modifier: Modifier = Modifier,
    anchor: Alignment.Vertical = Alignment.Top,
    padding: Dp = 0.dp,
    content: @Composable BoxScope.() -> Unit,
) {
    val scroll = rememberScrollState()
    val followed = remember { intArrayOf(0) }
    if (anchor == Alignment.Bottom) {
        LaunchedEffect(scroll.maxValue) {
            val atEnd = scroll.value >= followed[0]
            followed[0] = scroll.maxValue
            if (atEnd) scroll.scrollTo(scroll.maxValue)
        }
    }
    Box(
        modifier
            .heightIn(max = maxHeight)
            .fadesHiddenEdges(scroll)
            .verticalScroll(scroll)
            .padding(padding),
        content = content,
    )
}
