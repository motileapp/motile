package app.motile.layout

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.sidebarToggle
import app.motile.panel.PanelScreen
import app.motile.sidebar.SidebarScreen
import app.motile.theme.Theme
import app.motile.thread.ThreadScreen

/** The sidebar, the thread and its panel. A narrow screen has the sidebar and the panel under the thread; a wide one has all three side by side. */
@Composable
fun MainScreen(drawer: Drawer) {
    val store = LocalStore.current
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val wide = maxWidth >= WIDE
        DisposableEffect(wide) {
            sidebarToggle = if (wide) null else ({ drawer.isOpen = !drawer.isOpen })
            LocalDrawerHolder.drawer = drawer
            onDispose {
                sidebarToggle = null
                LocalDrawerHolder.drawer = null
            }
        }
        CompositionLocalProvider(LocalDrawer provides drawer) {
            if (wide) {
                LaunchedEffect(Unit) { drawer.isOpen = false }
                Wide(maxWidth.value)
            } else {
                DrawerView(
                    drawer = drawer,
                    rightPanel = store.rightPanel,
                    sidebar = { SidebarScreen(underThread = true) },
                    content = { ThreadScreen(overSidebar = true) },
                    panel = { PanelScreen(beside = false) },
                )
            }
        }
    }
}

@Composable
private fun Wide(width: Float) {
    val store = LocalStore.current
    val open = store.rightPanel.isOpen
    val rest = width - SIDEBAR_WIDTH - 1
    val covers = open && (store.rightPanel.isMaximized || rest - 1 - PANEL_WIDTH < THREAD_BESIDE_PANEL)
    Row(Modifier.fillMaxSize()) {
        Box(Modifier.width(SIDEBAR_WIDTH.dp).fillMaxHeight().background(Theme.colors.background)) { SidebarScreen(underThread = false) }
        Line()
        if (covers) {
            Box(Modifier.weight(1f).fillMaxHeight()) { PanelScreen(beside = true) }
        } else {
            Box(Modifier.weight(1f).fillMaxHeight()) { ThreadScreen(overSidebar = false) }
            if (open) {
                Line()
                Box(Modifier.width(PANEL_WIDTH.dp).fillMaxHeight()) { PanelScreen(beside = true) }
            }
        }
    }
}

@Composable
private fun Line() {
    Box(Modifier.width(1.dp).fillMaxHeight().background(Theme.colors.border))
}

const val SIDEBAR_WIDTH = 320f
private const val PANEL_WIDTH = 420f
/** A screen at least this wide has the sidebar beside the thread. */
private val WIDE = 1000.dp
/** What the panel leaves to the thread. Without that much room, it covers the thread. */
private const val THREAD_BESIDE_PANEL = 400f
