package app.motile.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.motile.core.ServerSync
import app.motile.core.Time
import app.motile.platform.Symbol
import app.motile.theme.LocalPalette
import app.motile.ui.Icon
import app.motile.ui.Spinner
import app.motile.ui.Text
import kotlinx.coroutines.delay

/** The line under a server that says how its copies of the other servers' threads stand. It is
 *  always one line, whatever it says, so the rows around it never move. */
@Composable
fun ServerSyncLine(sync: ServerSync, size: Float = NOTE_TEXT) {
    val palette = LocalPalette.current
    val now by produceState(Time.now()) {
        while (true) {
            delay(30_000)
            value = Time.now()
        }
    }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
        Box(Modifier.size(size.dp), contentAlignment = Alignment.Center) {
            when (sync.state) {
                ServerSync.State.Synced -> Icon(Symbol.Check, size, color = palette.success)
                ServerSync.State.Copying -> Spinner(size, color = palette.mutedForeground)
                ServerSync.State.Failed -> Icon(Symbol.TriangleAlert, size - 1, color = palette.warning)
                ServerSync.State.Waiting, ServerSync.State.Off -> Icon(Symbol.CircleDashed, size - 1, color = palette.mutedForeground)
            }
        }
        Text(sync.line(now), size = size, color = palette.mutedForeground, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}
