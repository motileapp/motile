package app.motile.usage

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.AppStore
import app.motile.core.LimitResets
import app.motile.core.ThreadInfo
import app.motile.platform.Symbol
import app.motile.settings.NOTE_TEXT
import app.motile.settings.RowText
import app.motile.settings.SettingsPressRow
import app.motile.settings.SettingsTile
import app.motile.theme.IconSize
import app.motile.theme.LocalPalette
import app.motile.ui.ActionMenu
import app.motile.theme.ControlSize
import app.motile.ui.ButtonVariant
import app.motile.ui.Icon
import app.motile.ui.MenuBuilder
import app.motile.ui.MenuPopup
import app.motile.ui.Spinner
import app.motile.ui.Text
import app.motile.ui.rememberMenuState

/**
 * The limit resets an account holds, under its windows: a row of the group that opens the menu that uses one, with how many
 * there are and when the first expires under its words.
 */
@Composable
fun ResetsRow(resets: LimitResets, used: (() -> Unit)? = null) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val pending = store.usingReset(resets)
    val menu = rememberMenuState()
    Box {
        SettingsPressRow({ menu.open() }, enabled = !pending) {
            Box(Modifier.size(SettingsTile.size.dp), contentAlignment = Alignment.Center) {
                Icon(Symbol.Ticket, IconSize.Xl, color = palette.primary)
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                RowText("Use Reset", color = palette.primary, maxLines = 1)
                Text(listOfNotNull(resets.title, resets.detail).joinToString(" · "), size = NOTE_TEXT, color = palette.mutedForeground, maxLines = 1)
            }
            if (pending) Spinner(IconSize.Xl)
        }
        MenuPopup(menu) { resetOptions(store, resets, thread = null, used = used) }
    }
}

/**
 * The menu that uses one of an account's limit resets: each kind with what it refills, how many are left and when they
 * expire. One that waits for a limit is offered at one, in a thread its limit stopped.
 */
@Composable
fun ResetMenu(
    resets: LimitResets,
    variant: ButtonVariant,
    margin: PaddingValues = PaddingValues(0.dp),
    thread: ThreadInfo? = null,
    size: ControlSize = ControlSize.Regular,
    used: (() -> Unit)? = null,
) {
    val store = LocalStore.current
    val pending = store.usingReset(resets)
    ActionMenu(
        "Use Reset",
        help = "Refill this account's limits now with one of its resets",
        variant = variant,
        size = size,
        pending = pending,
        disabled = pending,
        margin = margin,
    ) { resetOptions(store, resets, thread, used) }
}

/** An item for each kind of reset, with its detail or why it can't be used now, which disables it. */
private fun MenuBuilder.resetOptions(store: AppStore, resets: LimitResets, thread: ThreadInfo?, used: (() -> Unit)?) {
    val usable = { option: LimitResets.Option -> option.usable || (thread != null && option.atLimit) }
    for (option in resets.options) {
        val detail = if (usable(option)) option.detail else option.note ?: option.detail
        item(option.title, detail = detail, disabled = !usable(option)) { store.useLimitReset(option, resets, used) }
    }
}
