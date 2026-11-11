package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette

/** A colour to pick: a dot as tall as a button of its size, ringed in the foreground when it is the one picked. */
@Composable
fun Swatch(color: Color, help: String, selected: Boolean = false, size: ControlSize = ControlSize.Regular, onClick: () -> Unit) {
    val palette = LocalPalette.current
    val ring = if (selected) palette.foreground else Color.Transparent
    DimButton(
        onClick,
        Modifier
            .size(size.height)
            .border(2.dp, ring, CircleShape)
            .semantics(mergeDescendants = true) {
                contentDescription = help
                role = Role.Button
                this.selected = selected
            },
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .size(size.height * 0.6f)
                .background(color, CircleShape)
                .border(1.dp, palette.borderAccent, CircleShape),
        )
    }
}
