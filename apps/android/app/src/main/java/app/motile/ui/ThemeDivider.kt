package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import app.motile.theme.LocalSurface

/** A line across the surface it lies on, in that surface's border. */
@Composable
fun ThemeDivider(modifier: Modifier = Modifier, color: Color? = null) {
    Box(modifier.fillMaxWidth().height(1.dp).background(color ?: LocalSurface.current.border))
}
