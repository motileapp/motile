package app.motile.sidebar

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import app.motile.core.AppUpdater
import app.motile.platform.Symbol
import app.motile.theme.iconSize
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.ui.TurningSymbol
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

/** A new version of the client: the offer, the download as it goes, and Android's installer that finishes it. */
@Composable
fun AppUpdateRow(updater: AppUpdater, modifier: Modifier = Modifier, variant: ButtonVariant = ButtonVariant.Secondary, size: ControlSize = ControlSize.Small) {
    val colors = Theme.colors
    // The user may have let the client install in the system's settings meanwhile.
    LifecycleResumeEffect(updater) {
        updater.readPermission()
        onPauseOrDispose {}
    }
    when (val state = updater.state) {
        AppUpdater.State.Idle -> Unit
        AppUpdater.State.Checking -> UpdateLine("Checking for updates", Symbol.RefreshCw, modifier, turning = true)
        AppUpdater.State.UpToDate -> UpdateLine("Up to date", Symbol.CircleCheck, modifier, tint = colors.success)
        is AppUpdater.State.Available -> UpdateLine("v${state.version} is available", Symbol.CircleArrowDown, modifier, tint = colors.success) {
            // Android lets a client install what it downloads once the user allows it in its settings.
            val title = if (updater.canInstall || updater.obstacle != null) "Update" else "Allow and Update"
            ActionButton(title, variant = variant, size = size) { updater.install() }
        }
        is AppUpdater.State.Downloading -> Column(modifier, verticalArrangement = Arrangement.spacedBy(5.dp)) {
            UpdateLine("Downloading v${state.version}", Symbol.CircleArrowDown, tint = colors.foreground) {
                Text("${(state.fraction * 100).toInt()}%", style = Theme.uiDigits(11f), color = colors.mutedStrongerForeground)
                ActionButton(icon = Symbol.X, help = "Cancel the download", size = size) { updater.cancelDownload() }
            }
            Box(Modifier.fillMaxWidth().height(3.dp).clip(RoundedCornerShape(Radius.xs)).background(Surface.Background.color(Surface.Layer.Control))) {
                Box(Modifier.fillMaxWidth(state.fraction.toFloat().coerceIn(0f, 1f)).fillMaxHeight().background(colors.mutedForeground))
            }
        }
        is AppUpdater.State.Installing -> UpdateLine("Installing v${state.version}", Symbol.CircleArrowDown, modifier, tint = colors.foreground) {
            Spinner(ControlSize.Small.symbol, color = colors.mutedForeground)
        }
        is AppUpdater.State.Failed -> Column(modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
            UpdateLine("The update didn’t work", Symbol.TriangleAlert) {
                ActionButton("Try Again", size = ControlSize.Small) { updater.retry() }
            }
            Text(state.message, size = 11f, color = colors.mutedForeground)
        }
    }
}

@Composable
private fun UpdateLine(
    text: String,
    symbol: Symbol,
    modifier: Modifier = Modifier,
    tint: Color = Theme.colors.mutedForeground,
    turning: Boolean = false,
    trailing: @Composable RowScope.() -> Unit = {},
) {
    Row(modifier.fillMaxWidth().heightIn(min = ControlSize.Small.height), horizontalArrangement = Arrangement.spacedBy(7.dp), verticalAlignment = Alignment.CenterVertically) {
        UpdateLabel(text, symbol, tint, turning, Modifier.weight(1f))
        trailing()
    }
}

/** What an update is at, after its symbol. */
@Composable
fun UpdateLabel(text: String, symbol: Symbol, tint: Color = Theme.colors.mutedForeground, turning: Boolean = false, modifier: Modifier = Modifier) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(5.dp), verticalAlignment = Alignment.CenterVertically) {
        TurningSymbol(symbol, turning, iconSize(text = 12f), color = tint)
        Text(text, size = 12f, weight = FontWeight.Medium, maxLines = 1)
    }
}
