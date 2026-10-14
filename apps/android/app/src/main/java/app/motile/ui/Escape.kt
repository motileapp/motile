package app.motile.ui

import androidx.activity.compose.BackHandler
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import app.motile.LocalStore

/** Claims Esc of a hardware keyboard for the view while it has focus. With null it lets Esc go, to what is around it. */
fun Modifier.onEscape(action: (() -> Unit)?): Modifier {
    if (action == null) return this
    return onPreviewKeyEvent { event ->
        if (event.key != Key.Escape) return@onPreviewKeyEvent false
        if (event.type == KeyEventType.KeyDown) action()
        true
    }
}

/** Does what back means now, where the store says: leaves a route, restores the right panel. Without it back reaches whatever else listens for it. */
@Composable
fun EscapeKey() {
    val escapes = LocalStore.current.escapes
    BackHandler(enabled = escapes != null) { escapes?.invoke() }
}
