package app.motile.sidebar

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Server
import app.motile.core.ServerStats
import app.motile.settings.LocalBarBack
import app.motile.settings.NOTE_TEXT
import app.motile.settings.SettingsBar
import app.motile.settings.SettingsList
import app.motile.settings.SettingsRow
import app.motile.theme.IconSize
import app.motile.theme.Theme
import app.motile.ui.Spinner
import app.motile.ui.Text

private val rowPadding = 6.dp
/** The update's button stands as far from the group's top, bottom and side. */
private val buttonMargin = 8.dp
private val headerStart = 16.dp
private val sectionGap = 24.dp

/**
 * What a server's machine does, as the Mac's dropdown shows it, as the settings' grouped list in
 * a sheet named after the server and as tall as the list: its uptime and update, then a row for
 * each of its parts.
 */
@Composable
fun ServerStatsSheet(server: Server) {
    val store = LocalStore.current
    DisposableEffect(server.id) {
        store.watchStats(server.id, true)
        onDispose { store.watchStats(server.id, false) }
    }
    val shown = store.serverStats[server.id] ?: ServerStats.Shown.Loading
    Column(Modifier.fillMaxWidth()) {
        CompositionLocalProvider(LocalBarBack provides null) { SettingsBar(server.name) }
        SettingsList(spacing = sectionGap, fits = true) {
            group {
                row {
                    ServerStatsHeader(server, Modifier.fillMaxWidth().padding(start = headerStart, end = buttonMargin, top = buttonMargin, bottom = buttonMargin))
                }
            }
            group {
                when (shown) {
                    ServerStats.Shown.Loading -> row {
                        SettingsRow { Box(Modifier.weight(1f), contentAlignment = Alignment.Center) { Spinner(IconSize.Xl) } }
                    }
                    is ServerStats.Shown.Failed -> row {
                        SettingsRow { Text(shown.error, Modifier.weight(1f), size = NOTE_TEXT, color = Theme.colors.mutedForeground) }
                    }
                    is ServerStats.Shown.Showing -> for (part in shown.stats.rows) row {
                        SettingsRow { ServerStatsRow(part, Modifier.weight(1f).padding(vertical = rowPadding)) }
                    }
                }
            }
        }
    }
}
