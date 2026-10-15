package app.motile.thread

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.motile.core.Project
import app.motile.theme.Theme
import app.motile.ui.ProjectIcon
import app.motile.ui.Text

/** The line over a thread's title: its project's icon, then its parts with muted dots between. */
@Composable
fun ProjectLine(project: Project?, parts: List<String>, size: Float, iconSize: Float, modifier: Modifier = Modifier) {
    val palette = Theme.colors
    val line = buildAnnotatedString {
        parts.forEachIndexed { index, part ->
            if (index > 0) withStyle(SpanStyle(color = palette.mutedStrongerForeground)) { append(" · ") }
            append(part)
        }
    }
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
        ProjectIcon(project, size = iconSize)
        Text(line, style = system(size), color = palette.mutedForeground, maxLines = 1)
    }
}

/** The system's font at a size as iOS gives it, where the Apple client doesn't use the client's scaled sizes. */
internal fun system(size: Float, weight: FontWeight = FontWeight.Normal): TextStyle = TextStyle(fontSize = size.sp, fontWeight = weight)

/** Lets the content reach past its place by the given amounts, as a negative padding does in SwiftUI. */
internal fun Modifier.outset(start: Dp = 0.dp, end: Dp = 0.dp, vertical: Dp = 0.dp): Modifier = layout { measurable, constraints ->
    val left = start.roundToPx()
    val right = end.roundToPx()
    val upDown = vertical.roundToPx()
    val wider = constraints.copy(
        minWidth = constraints.minWidth + left + right,
        maxWidth = if (constraints.hasBoundedWidth) constraints.maxWidth + left + right else constraints.maxWidth,
        minHeight = constraints.minHeight + upDown * 2,
        maxHeight = if (constraints.hasBoundedHeight) constraints.maxHeight + upDown * 2 else constraints.maxHeight,
    )
    val placeable = measurable.measure(wider)
    val width = (placeable.width - left - right).coerceAtLeast(0)
    val height = (placeable.height - upDown * 2).coerceAtLeast(0)
    layout(width, height) { placeable.place(-left, -upDown) }
}
