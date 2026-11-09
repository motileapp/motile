package app.motile.composer

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Server
import app.motile.core.SettingsSection
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.IconSize
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.ShadowSize
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.theme.at
import app.motile.theme.wash
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.Icon
import app.motile.ui.Text

/** How far the words stand from the box's side, as the composer's text does. */
private val textInset = 14.dp

/** In place of the composer of a thread whose server was unlinked: the composer's box, saying why the thread can only be read, with the ways on from it under that. */
@Composable
fun UnlinkedThreadBar(server: Server, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val palette = Theme.colors
    CompositionLocalProvider(LocalSurface provides Surface.Background) {
        Column(
            modifier
                .widthIn(max = Theme.composerWidth)
                .fillMaxWidth()
                .composerSurface(RoundedCornerShape(ComposerView.radius), ShadowSize.Md),
        ) {
            Column(Modifier.padding(start = textInset, end = textInset, top = 14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                    Icon(Symbol.Unlink, IconSize.Sm, color = palette.warning)
                    NameChip(server.name)
                    Text("is unlinked", Modifier.weight(1f, fill = false), weight = FontWeight.Medium, color = palette.warning, maxLines = 1)
                }
                Text("This thread is read-only. You can continue it on another server, or delete the server in settings.", color = palette.mutedForeground)
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                ActionButton(
                    "Settings", help = "Delete “${server.name}” and its threads in Settings", round = true, margin = ComposerView.margin(trailing = 4.dp),
                ) { store.openSettings(SettingsSection.Servers) }
                ActionButton(
                    "Continue", help = "Start a thread on another server with everything that happened here", variant = ButtonVariant.Primary, round = true,
                    margin = ComposerView.margin(leading = 4.dp, trailing = ComposerView.edge), disabled = store.servers.isEmpty(),
                ) { store.continueOnAnotherServer() }
            }
        }
    }
}

/** The server's name, set apart as a skill is in a message: on a wash of the title's colour, inside an edge. */
@Composable
private fun NameChip(name: String) {
    val palette = Theme.colors
    val shape = RoundedCornerShape(SkillChip.radius)
    Text(
        name,
        Modifier
            .background(palette.warning.wash(), shape)
            .border(1.dp, palette.warning.at(Opacity.ColorTintChart), shape)
            .padding(horizontal = (5 * Platform.scale).dp, vertical = 1.dp),
        weight = FontWeight.Medium, color = palette.warning, maxLines = 1,
    )
}
