package app.motile.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.unit.dp
import app.motile.theme.LocalSurface

/** A line across the surface it lies on, in that surface's border. */
@Composable
fun ThemeDivider(modifier: Modifier = Modifier, color: Color? = null, dashed: Boolean = false) {
    val lineColor = color ?: LocalSurface.current.border
    if (!dashed) {
        Box(modifier.fillMaxWidth().height(1.dp).background(lineColor))
        return
    }
    Canvas(modifier.fillMaxWidth().height(1.dp)) {
        val y = size.height / 2
        drawLine(
            lineColor, Offset(0f, y), Offset(size.width, y), strokeWidth = size.height,
            pathEffect = PathEffect.dashPathEffect(floatArrayOf(4.dp.toPx(), 3.dp.toPx())),
        )
    }
}
