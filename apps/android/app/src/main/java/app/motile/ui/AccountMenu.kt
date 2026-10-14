package app.motile.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Account
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import java.net.URL

/**
 * The account as its picture, which opens a menu with who is signed in, the settings, the usage and
 * the way to sign out. It floats on glass beside the search, as the sidebar's buttons do.
 */
object AccountMenu {
    /** The picture's side in a button of the regular size, which it fills but for a margin. */
    val side = ControlSize.Regular.height - 8.dp
}

@Composable
fun AccountMenu(modifier: Modifier = Modifier) {
    val store = LocalStore.current
    Menu(
        items = {
            section(store.account.email) {
                item("Settings") { store.openSettings() }
                item("Usage") { store.openUsage() }
            }
            divider()
            item("Sign Out") { store.signOut() }
        },
        modifier = modifier.size(GlassButton.side).glassButton(CircleShape).semantics { contentDescription = "Account" },
    ) {
        AccountPicture(store.account, Modifier.size(34.dp))
    }
}

/** The account's picture, or the first letter of its name on a disc until there is one. */
@Composable
fun AccountPicture(account: Account, modifier: Modifier = Modifier) {
    val picture by produceState<ImageBitmap?>(null, account.picture) {
        value = account.picture?.let { address -> runCatching { URL(address) }.getOrNull()?.let { ImageFiles.load(it) } }
    }
    val palette = LocalPalette.current
    Box(modifier.clip(CircleShape), contentAlignment = Alignment.Center) {
        val shown = picture
        if (shown != null) {
            Image(shown, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
            return@Box
        }
        BoxWithConstraints(Modifier.fillMaxSize().background(palette.border), contentAlignment = Alignment.Center) {
            val size = with(LocalDensity.current) { (maxHeight * 0.5f).toSp() }
            val initial = (account.name ?: account.email).take(1).uppercase()
            Text(initial, style = TextStyle(fontSize = size, fontWeight = FontWeight.SemiBold), color = palette.foreground, maxLines = 1)
        }
    }
}
