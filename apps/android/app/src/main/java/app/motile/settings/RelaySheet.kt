package app.motile.settings

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import app.motile.LocalStore
import app.motile.theme.IconSize
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.HoldsModal
import app.motile.ui.LocalCloseModal
import app.motile.ui.Spinner

/** The iroh relay every device of the account uses, to set up, change or clear. */
@Composable
fun RelaySheet() {
    val store = LocalStore.current
    val close = LocalCloseModal.current
    var address by remember { mutableStateOf(store.account.relay ?: "") }
    var saving by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }

    HoldsModal(saving)

    fun save() {
        saving = true
        error = null
        store.setRelay(address.trim().ifEmpty { null }) { failure ->
            saving = false
            error = failure
            if (failure == null) close()
        }
    }

    val trailing: @Composable () -> Unit = {
        if (saving) Spinner(IconSize.Xl)
        else ActionButton("Save", variant = ButtonVariant.Primary) { save() }
    }
    Column(Modifier.fillMaxSize()) {
        CompositionLocalProvider(LocalBarLeading provides BarLeading.Close { close() }) { SettingsBar("Relay", trailing) }
        SettingsList(Modifier.weight(1f)) {
            group(
                footer = "Every device of your account uses this iroh relay in place of iroh's public ones. Leave it empty to go back to them.",
                failure = error,
            ) {
                row { SettingsField("https://relay.example.com", address, { address = it }, monospaced = true, keyboardOptions = urlKeyboard) }
            }
        }
    }
}
