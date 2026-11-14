package app.motile.settings

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import app.motile.LocalStore
import app.motile.theme.IconSize
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.Spinner

/** The iroh relay every device of the account uses, to set up, change or clear. */
@Composable
fun RelayPage() {
    val store = LocalStore.current
    val navigator = LocalSettingsNavigator.current
    var address by remember { mutableStateOf(store.account.relay ?: "") }
    var saving by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }

    fun save() {
        saving = true
        error = null
        store.setRelay(address.trim().ifEmpty { null }) { failure ->
            saving = false
            error = failure
            if (failure == null) navigator.pop()
        }
    }

    val trailing: @Composable () -> Unit = {
        if (saving) Spinner(IconSize.Xl)
        else ActionButton("Save", variant = ButtonVariant.Primary) { save() }
    }
    SettingsScreen("Relay", trailing) {
        SettingsList {
            group(
                footer = "Every device of your account uses this iroh relay in place of iroh's public ones. Leave it empty to go back to them.",
                failure = error,
            ) {
                row { SettingsField("https://relay.example.com", address, { address = it }, monospaced = true, keyboardOptions = urlKeyboard) }
            }
        }
    }
}
