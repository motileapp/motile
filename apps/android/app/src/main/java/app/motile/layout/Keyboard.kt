package app.motile.layout

import android.view.KeyEvent as AndroidKeyEvent
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isMetaPressed
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.type
import app.motile.core.AppStore
import app.motile.core.perform
import app.motile.core.shortcutContext

/** What has the keys, for the shortcuts' conditions: the composer sets the first, any other field the second. */
object KeyboardFocus {
    var composer by mutableStateOf(false)
    var editable by mutableStateOf(false)
}

/**
 * Runs the core's keybindings from a keyboard with keys, ahead of the focused view, as an iPad's
 * menus do. A plain key goes to what is focused unless no field has the keys.
 */
fun AppStore.handleShortcut(event: KeyEvent): Boolean {
    if (event.type != KeyEventType.KeyDown) return false
    val modified = event.isCtrlPressed || event.isMetaPressed || event.isAltPressed
    val typing = KeyboardFocus.composer || KeyboardFocus.editable
    if (!modified && typing && event.key != Key.Escape) return false
    val names = keyNames(event)
    if (names.isEmpty()) return false
    val context = shortcutContext(KeyboardFocus.composer, KeyboardFocus.editable)
    val command = shortcuts.command(
        names, command = event.isMetaPressed, control = event.isCtrlPressed, option = event.isAltPressed, shift = event.isShiftPressed, context = context,
    ) ?: return false
    return perform(command)
}

/** The names the pressed key goes by, as keybindings.json writes them. */
private fun keyNames(event: KeyEvent): Set<String> {
    val special = when (event.key) {
        Key.Enter, Key.NumPadEnter -> "enter"
        Key.Tab -> "tab"
        Key.Spacebar -> "space"
        Key.Escape -> "escape"
        Key.Backspace -> "backspace"
        Key.Delete -> "delete"
        Key.DirectionUp -> "arrowup"
        Key.DirectionDown -> "arrowdown"
        Key.DirectionLeft -> "arrowleft"
        Key.DirectionRight -> "arrowright"
        Key.MoveHome -> "home"
        Key.MoveEnd -> "end"
        Key.PageUp -> "pageup"
        Key.PageDown -> "pagedown"
        else -> null
    }
    if (special != null) return setOf(special)
    val native = event.nativeKeyEvent
    val plain = native.getUnicodeChar(0)
    val shifted = native.getUnicodeChar(AndroidKeyEvent.META_SHIFT_ON)
    return listOf(plain, shifted).filter { it > 0 }.map { it.toChar().lowercase() }.toSet()
}
