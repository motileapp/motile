package app.motile.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import kotlinx.coroutines.delay

/**
 * The button that copies. It shows a check mark for a moment once it has, also when `copies` goes up
 * because something else, like a shortcut, copied.
 */
@Composable
fun CopyButton(
    help: String,
    modifier: Modifier = Modifier,
    variant: ButtonVariant = ButtonVariant.Ghost,
    size: ControlSize = ControlSize.Regular,
    symbolSize: Float? = null,
    round: Boolean = false,
    quiet: Boolean = false,
    copies: Int = 0,
    copy: () -> Unit,
) {
    var copied by remember { mutableStateOf(false) }
    var shown by remember { mutableIntStateOf(0) }
    val seen = remember { intArrayOf(copies) }
    LaunchedEffect(copies) {
        if (copies == seen[0]) return@LaunchedEffect
        seen[0] = copies
        copied = true
        shown += 1
    }
    LaunchedEffect(shown) {
        if (!copied) return@LaunchedEffect
        delay(1200)
        copied = false
    }
    ActionButton(
        icon = if (copied) Symbol.Check else Symbol.Copy, help = help, modifier = modifier, variant = variant, size = size,
        symbolSize = symbolSize, round = round, quiet = quiet,
    ) {
        copy()
        copied = true
        shown += 1
    }
}
