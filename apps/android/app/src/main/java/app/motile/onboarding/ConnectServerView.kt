package app.motile.onboarding

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.motile.LocalCloseSheet
import app.motile.LocalStore
import app.motile.core.Time
import app.motile.platform.Symbol
import app.motile.theme.iconSize
import app.motile.theme.IconSize
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.Radius
import app.motile.theme.Theme
import app.motile.theme.wash
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.CommandBox
import app.motile.ui.Icon
import app.motile.ui.SheetBar
import app.motile.ui.SheetCloseButton
import app.motile.ui.Spinner
import app.motile.ui.Text
import kotlinx.coroutines.delay

/**
 * The one command that turns a machine into a server, centred. Shown when the account has no
 * server yet, and when adding another, `titled` as a sheet of its own with the way to close it.
 */
@Composable
fun ConnectServerView(isFirst: Boolean, titled: Boolean = !isFirst) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val close = LocalCloseSheet.current
    DisposableEffect(Unit) {
        store.prepareToAddServer()
        onDispose { store.stopAddingServer() }
    }
    val root = if (isFirst) Modifier.background(palette.background).safeDrawingPadding() else Modifier
    Column(Modifier.fillMaxSize().then(root)) {
        if (titled) SheetBar("Add a Server", leading = { SheetCloseButton(onClose = close) })
        // Raises the centred content a little above the middle.
        BoxWithConstraints(Modifier.fillMaxWidth().weight(1f)) {
            val lift = maxHeight * 0.08f
            Column(Modifier.fillMaxSize().padding(horizontal = 16.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Spacer(Modifier.weight(1f))
                Box(
                    Modifier.size(56.dp).background(palette.primary.wash(), RoundedCornerShape(Radius.lg)),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(Symbol.Server, IconSize.Display, color = palette.primary)
                }
                Text(
                    if (isFirst) "Connect your first server" else "Run this on your server",
                    Modifier.padding(start = 24.dp, end = 24.dp, top = 18.dp),
                    size = 24f,
                    weight = FontWeight.SemiBold,
                    align = TextAlign.Center,
                )
                val explained = Theme.ui(14f)
                Text(
                    "Run the command below on the server that will run your agents",
                    Modifier.widthIn(max = 480.dp).padding(start = 24.dp, end = 24.dp, top = 8.dp),
                    style = explained.copy(lineHeight = (explained.fontSize.value * 1.2f + 3).sp),
                    color = palette.mutedForeground,
                    align = TextAlign.Center,
                )

                val token = store.enrollToken
                CommandBox(
                    token?.command,
                    Modifier.padding(top = 26.dp).widthIn(max = 560.dp).fillMaxWidth(),
                    spans = token?.spans?.toList() ?: emptyList(),
                )

                Status(Modifier.padding(top = 22.dp))

                Spacer(Modifier.weight(1f))
                Spacer(Modifier.height(lift))
                if (isFirst) {
                    Row(Modifier.padding(bottom = 20.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text("Signed in as ${store.account.email}", size = 12f, color = palette.mutedStrongerForeground)
                        ActionButton("Sign out", variant = ButtonVariant.Link, size = ControlSize.Small) { store.signOut() }
                    }
                }
            }
        }
    }
}

/** The spinner and the time the command has left, or that it has run out and a way to a new one. */
@Composable
private fun Status(modifier: Modifier) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val now by produceState(Time.now()) {
        while (true) {
            delay(1000)
            value = Time.now()
        }
    }
    val token = store.enrollToken
    Column(modifier, verticalArrangement = Arrangement.spacedBy(14.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        if (token != null && token.isExpired(now)) {
            Text("The command has expired", size = 13f, color = palette.mutedForeground)
            ActionButton("Regenerate", pending = store.enrollTokenPending) { store.regenerateEnrollToken() }
            return@Column
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            Spinner(iconSize(text = 13f), color = palette.mutedForeground)
            Text("Waiting for your server", size = 13f, color = palette.mutedForeground)
        }
        Text(token?.timeLeft(now) ?: "15:00", style = Theme.uiDigits(12f), color = palette.mutedStrongerForeground)
    }
}
