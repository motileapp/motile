package app.motile.sidebar

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.RestartWhen
import app.motile.core.Server
import app.motile.core.ServerUpdate
import app.motile.theme.ControlSize
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.ActionMenu
import app.motile.ui.ButtonVariant
import app.motile.ui.Spinner
import app.motile.ui.Text

/**
 * What stands at the end of a server's line: the offer to update it, the update as it goes, or
 * `otherwise`. While agents work there, the server restarts once they have finished, or at once
 * with their threads going on after.
 */
@Composable
fun ServerUpdateStatus(
    server: Server,
    variant: ButtonVariant = ButtonVariant.Secondary,
    size: ControlSize = ControlSize.Small,
    otherwise: @Composable () -> Unit,
) {
    val store = LocalStore.current
    val muted = Theme.colors.mutedForeground
    val update = store.serverUpdate(server)
    when {
        update != null && update.waiting && !update.restarting -> Row(
            Modifier.semantics { contentDescription = "${server.name} restarts once its agents have finished" },
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Waiting", size = 11f, color = muted)
            ActionButton(
                "Restart Now", help = "Stop the agents on ${server.name} and restart it. Their threads continue once it is back.",
                variant = variant, size = size,
            ) {
                store.update(server, RestartWhen.Now)
            }
        }
        update != null -> Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(progress(update), style = Theme.uiDigits(11f), color = muted)
            Spinner(size.symbol, color = muted)
        }
        store.isOutdated(server) && store.isBusy(server) && store.canChooseRestart(server) -> ActionMenu("Update", variant = variant, size = size) {
            item("Update when agents finish") { store.update(server, RestartWhen.Idle) }
            item("Update now and auto-resume agents") { store.update(server, RestartWhen.Now) }
        }
        store.isOutdated(server) -> {
            val version = store.updater.latest ?: ""
            val help = if (store.canChooseRestart(server)) {
                "Install version $version on ${server.name}. It restarts once no agent works there."
            } else {
                "Install version $version on ${server.name}. It restarts, and no agent may be working."
            }
            ActionButton("Update", help = help, variant = variant, size = size) { store.update(server) }
        }
        else -> otherwise()
    }
}

private fun progress(update: ServerUpdate): String {
    if (update.restarting) return "Restarting"
    val fraction = update.fraction ?: return "Updating"
    return "Updating ${(fraction * 100).toInt()}%"
}
