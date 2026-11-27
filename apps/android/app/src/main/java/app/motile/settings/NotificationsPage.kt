package app.motile.settings

import android.Manifest
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.LifecycleResumeEffect
import app.motile.LocalStore
import app.motile.core.AlertChoices
import app.motile.core.NotificationMode
import app.motile.core.NotificationsText
import app.motile.core.Preferences
import app.motile.notifications.AlertNotifications
import app.motile.platform.Symbol
import app.motile.ui.Switch

/** Whether the phone shows the alerts its servers push, which ones, and the ongoing activity. */
@Composable
fun NotificationsPage() {
    val store = LocalStore.current
    val context = LocalContext.current
    val settings = store.notificationSettings
    var allowed by remember { mutableStateOf(AlertNotifications.allowed(context)) }
    var asked by remember { mutableStateOf(Preferences.bool(ASKED_KEY)) }
    LifecycleResumeEffect(Unit) {
        allowed = AlertNotifications.allowed(context)
        onPauseOrDispose {}
    }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        allowed = AlertNotifications.allowed(context)
        if (allowed) store.changeNotifications(settings.copy(mode = NotificationMode.Notifications))
        store.syncPush()
    }

    /** Turning notifications on asks the system first; while it refuses, they stay off. */
    fun turn(on: Boolean) {
        if (!on || allowed || Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            store.changeNotifications(settings.copy(mode = if (on) NotificationMode.Notifications else NotificationMode.Off))
            return
        }
        Preferences.set(ASKED_KEY, true)
        asked = true
        permission.launch(Manifest.permission.POST_NOTIFICATIONS)
    }

    val refused = !allowed && (asked || Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU)
    SettingsList(target = store.settingsTarget, reached = { store.settingsTarget = null }) {
        group("notifications", footer = if (refused) NotificationsText.refused else NotificationsText.mode) {
            row { SettingsToggle(NotificationsText.title, settings.notifies && allowed, onChange = ::turn) }
            if (settings.notifies && allowed) row {
                SettingsToggle("Sound", settings.mode == NotificationMode.NotificationsAndSound) { on ->
                    store.changeNotifications(settings.copy(mode = if (on) NotificationMode.NotificationsAndSound else NotificationMode.Notifications))
                }
            }
            if (refused) row {
                SettingsPressRow({ AlertNotifications.openSystemSettings(context) }) { SettingsActionLabel(NotificationsText.openSettings, Symbol.Settings) }
            }
        }
        group("toasts", footer = NotificationsText.toasts) {
            row {
                SettingsToggle(NotificationsText.toastsTitle, settings.toasts) { on ->
                    store.changeNotifications(settings.copy(toasts = on))
                }
            }
        }
        group("alert-choices", header = NotificationsText.choices) {
            for ((title, choice) in AlertChoices.rows) row {
                SettingsToggle(title, choice.isOn(settings.choices), enabled = settings.notifies && !refused) { on ->
                    store.changeNotifications(settings.copy(choices = choice.with(settings.choices, on)))
                }
            }
        }
        group("live-activities", footer = NotificationsText.liveCaption) {
            row { SettingsToggle(NotificationsText.live, settings.live) { store.changeNotifications(settings.copy(live = it)) } }
        }
        group(footer = NotificationsText.widget) {}
    }
}

/** Asks the system to let the client notify, once, the first time it opens signed in: by default it does. */
@Composable
fun AsksForNotificationsOnce() {
    val store = LocalStore.current
    val context = LocalContext.current
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { store.syncPush() }
    LaunchedEffect(Unit) {
        if (!store.asksForNotifications || Preferences.bool(ASKED_KEY)) return@LaunchedEffect
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU || AlertNotifications.allowed(context)) return@LaunchedEffect
        Preferences.set(ASKED_KEY, true)
        permission.launch(Manifest.permission.POST_NOTIFICATIONS)
    }
}

private const val ASKED_KEY = "notifications.asked"
