package app.motile.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.text.font.FontWeight
import app.motile.theme.ControlSize
import app.motile.platform.Platform
import androidx.compose.ui.unit.dp
import kotlin.math.max

/** What a sheet is for, across its bottom: a filled capsule in the primary colour, with a spinner while it waits. */
@Composable
fun SheetButton(title: String, modifier: Modifier = Modifier, pending: Boolean = false, disabled: Boolean = false, onClick: () -> Unit) {
    val look = ControlLook(ButtonVariant.Primary, ControlSize.Large, round = true)
    val height = max(ControlSize.Large.height.value, Platform.minimumPress).dp
    ControlButton(look, onClick, modifier.fillMaxWidth(), pending = pending, disabled = disabled, help = title) {
        Box(Modifier.fillMaxWidth().heightIn(min = height), contentAlignment = Alignment.Center) {
            Text(title, Modifier.alpha(if (pending) 0f else 1f), size = 15f, weight = FontWeight.SemiBold, maxLines = 1)
            if (pending) Spinner(17f)
        }
    }
}
