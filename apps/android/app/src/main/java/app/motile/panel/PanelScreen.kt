package app.motile.panel

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.ui.ActionButton
import app.motile.ui.GlassButton
import app.motile.ui.Text

/**
 * What the Mac has in the panel beside the thread: the changes in the folder the thread works in,
 * its files, the files opened from either, and the agents the thread started. On a phone it is
 * under the thread, which a swipe to the left slides aside, with a button back to the thread; on
 * a tablet, `beside` it, with its own buttons to cover the thread and to close.
 */
@Composable
fun PanelScreen(beside: Boolean) {
    val store = LocalStore.current
    val panel = store.rightPanel
    val tabs = panel.tabs
    val palette = LocalPalette.current
    Column(
        Modifier
            .fillMaxSize()
            .background(palette.background)
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))
            .keyboardTap(),
    ) {
        if (!beside) {
            Box(Modifier.fillMaxWidth().height(52.dp), contentAlignment = Alignment.Center) {
                GlassButton(Symbol.ChevronLeft, "Back", onClick = { panel.isOpen = false }, modifier = Modifier.align(Alignment.CenterStart).padding(start = 12.dp))
                Text(
                    store.panelTarget?.name ?: "Files", Modifier.padding(horizontal = 64.dp), size = 15f, weight = FontWeight.SemiBold,
                    color = palette.foreground, maxLines = 1,
                )
            }
        }
        Row(Modifier.fillMaxWidth().height(46.dp), verticalAlignment = Alignment.CenterVertically) {
            PanelTabStrip(tabs, Modifier.weight(1f))
            if (beside) PaneButtons()
        }
        PanelLine()
        Box(Modifier.weight(1f).windowInsetsPadding(WindowInsets.navigationBars.only(WindowInsetsSides.Bottom))) {
            PanelTabContent(tabs.active)
        }
    }
}

@Composable
private fun PaneButtons() {
    val panel = LocalStore.current.rightPanel
    val maximized = panel.isMaximized
    Row(Modifier.padding(end = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        ActionButton(
            icon = if (maximized) Symbol.Minimize2 else Symbol.Maximize2,
            help = if (maximized) "Restore the right panel" else "Maximize the right panel",
            size = ControlSize.Large,
            disabled = !panel.canMaximize,
        ) { panel.toggleMaximized() }
        ActionButton(icon = Symbol.X, help = "Close the right panel", size = ControlSize.Large) { panel.isOpen = false }
    }
}

/** A tap anywhere puts the keyboard away and still reaches the views under it. */
private fun Modifier.keyboardTap(): Modifier = pointerInput(Unit) {
    awaitEachGesture {
        awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
        if (waitForUpOrCancellation(PointerEventPass.Initial) != null) Platform.endEditing()
    }
}
