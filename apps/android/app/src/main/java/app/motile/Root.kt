package app.motile

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusTarget
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import app.motile.commandpanel.CommandPanel
import app.motile.core.AppStore
import app.motile.core.Appearance
import app.motile.core.PanelPage
import app.motile.core.Project
import app.motile.layout.Drawer
import app.motile.layout.MainScreen
import app.motile.layout.handleShortcut
import app.motile.media.MediaViewer
import app.motile.onboarding.ConnectServerView
import app.motile.onboarding.SignInView
import app.motile.platform.Platform
import app.motile.settings.SettingsSheet
import app.motile.theme.LocalDark
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Palette
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.thread.CommitSheet
import app.motile.thread.ThreadSettingsSheet
import app.motile.ui.Text
import app.motile.usage.UsageSheet

/** The store the views show, given to them all. */
val LocalStore = staticCompositionLocalOf<AppStore> { error("No store") }

/** What is shown over the client, one at a time. What is asked for last comes over what was there. */
private sealed interface RootSheet {
    data class Commit(val project: Project) : RootSheet
    data object AddServer : RootSheet
    data class Panel(val page: PanelPage) : RootSheet
    data object Settings : RootSheet
    data object Usage : RootSheet
    data object ThreadSettings : RootSheet
}

private val AppStore.sheet: RootSheet?
    get() {
        committingProject?.let { return RootSheet.Commit(it) }
        if (showsAddServer) return RootSheet.AddServer
        panel?.let { return RootSheet.Panel(it) }
        if (settings != null) return RootSheet.Settings
        if (showsUsage) return RootSheet.Usage
        if (showsThreadSettings) return RootSheet.ThreadSettings
        return null
    }

private fun AppStore.closeSheet() {
    when {
        committingProject != null -> committingProject = null
        showsAddServer -> showsAddServer = false
        panel != null -> closePanel()
        settings != null -> closeSettings()
        showsUsage -> showsUsage = false
        else -> {
            showsThreadSettings = false
            showsBranches = false
        }
    }
}

/** Closes the sheet the content lies in. */
val LocalCloseSheet = staticCompositionLocalOf<() -> Unit> { {} }

@Composable
fun Root(store: AppStore) {
    val dark = when (Appearance.chosen) {
        Appearance.System -> isSystemInDarkTheme()
        Appearance.Light -> false
        Appearance.Dark -> true
    }
    val palette = if (dark) Palette.dark else Palette.light
    Theme.palette = palette
    Theme.dark = dark
    val drawer = remember { Drawer() }
    val focus = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current
    SideEffect {
        Platform.endEditing = {
            focus.clearFocus()
            keyboard?.hide()
        }
    }
    val keys = remember { FocusRequester() }
    CompositionLocalProvider(
        LocalStore provides store, LocalPalette provides palette, LocalDark provides dark, LocalForeground provides palette.foreground,
        LocalSurface provides Surface.Background,
    ) {
        Box(
            Modifier
                .fillMaxSize()
                .background(palette.background)
                .onPreviewKeyEvent { store.handleShortcut(it) }
                .focusRequester(keys)
                .focusTarget(),
        ) {
            when {
                !store.ready -> Unit
                !store.account.signedIn -> SignInView()
                store.servers.isEmpty() -> ConnectServerView(isFirst = true)
                else -> MainScreen(drawer)
            }
        }
        store.escapes?.let { escape -> BackHandler { escape() } }
        Sheets(store)
        store.viewing?.let { viewing ->
            Dialog(
                onDismissRequest = { store.closeViewer() },
                properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
            ) {
                MediaViewer(viewing)
            }
        }
        store.errorMessage?.let {
            val (title, detail) = store.errorAlert
            AlertDialog(
                onDismissRequest = { store.errorMessage = null },
                confirmButton = { TextButton(onClick = { store.errorMessage = null }) { Text("OK", color = palette.primary, size = 14f) } },
                title = { Text(title, size = 16f) },
                text = if (detail.isEmpty()) null else ({ Text(detail, size = 14f, color = palette.mutedForeground) }),
                containerColor = palette.popover,
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Sheets(store: AppStore) {
    val sheet = store.sheet ?: return
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    ModalBottomSheet(
        onDismissRequest = { store.closeSheet() },
        sheetState = state,
        containerColor = Theme.colors.background,
        contentColor = Theme.colors.foreground,
        dragHandle = { BottomSheetDefaults.DragHandle(color = Theme.colors.mutedStrongestForeground) },
    ) {
        CompositionLocalProvider(LocalSurface provides Surface.Background, LocalCloseSheet provides { store.closeSheet() }) {
            when (sheet) {
                is RootSheet.Commit -> CommitSheet(sheet.project)
                RootSheet.AddServer -> ConnectServerView(isFirst = false)
                is RootSheet.Panel -> CommandPanel(sheet.page)
                RootSheet.Settings -> SettingsSheet()
                RootSheet.Usage -> UsageSheet()
                RootSheet.ThreadSettings -> ThreadSettingsSheet()
            }
        }
    }
}
