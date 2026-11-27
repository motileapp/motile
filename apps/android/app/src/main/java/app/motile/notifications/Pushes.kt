package app.motile.notifications

import android.content.Context
import app.motile.MotileApplication
import app.motile.core.AgentsWork
import app.motile.core.AlertKind
import app.motile.core.Native
import app.motile.core.NotificationSettings
import app.motile.core.Preferences
import app.motile.core.Pushed
import app.motile.core.ServerWork
import app.motile.core.parseJSON
import app.motile.core.parseJSONArray
import app.motile.core.sealedParts
import app.motile.platform.Platform
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage

/** Where Firebase hands the auth server's pushes, which may start the process without the core. */
class PushService : FirebaseMessagingService() {
    // Firebase's default registration, which `register()` and `onRegistered` only replace when the manifest opts in.
    @Suppress("OVERRIDE_DEPRECATION")
    override fun onNewToken(token: String) {
        Pushes.tokenChanged(token)
        val store = (application as MotileApplication).store
        android.os.Handler(mainLooper).post { store.syncPush() }
    }

    override fun onMessageReceived(message: RemoteMessage) {
        Pushes.received(this, message.data)
    }
}

/** What the servers push, through the auth server, sealed to this device's key. */
object Pushes {
    private const val TOKEN_KEY = "push.token"
    private const val DATA_DIR_KEY = "dataDir"

    val token: String? get() = Preferences.string(TOKEN_KEY)

    fun tokenChanged(token: String) = Preferences.set(TOKEN_KEY, token)

    /** Where the core keeps the device's key, which the pushes are opened with. */
    fun dataDir(context: Context): String =
        Preferences.string(DATA_DIR_KEY) ?: java.io.File(context.noBackupFilesDir, "Motile").apply { mkdirs() }.path

    fun keepDataDir(path: String) = Preferences.set(DATA_DIR_KEY, path)

    /** Handles a push's data: an alert to notify of, or what the agents of the account's servers do. */
    fun received(context: Context, data: Map<String, String>) {
        when (data["motile"]) {
            "alert" -> alert(context, data)
            "work" -> work(context, sealedParts(data["parts"]).mapNotNull { (open(context, it) as? Pushed.Work)?.work })
        }
    }

    /** In front, the core shows the alert as a toast inside the client, so the system shows none. */
    private fun alert(context: Context, data: Map<String, String>) {
        if (Platform.isActive) return
        val threadID = data["thread"] ?: return
        val opened = data["sealed"]?.let { open(context, it) } as? Pushed.Alert
        val notice = opened?.let { AlertNotice.of(it.alert) } ?: AlertNotice.placeholder(threadID, AlertKind.of(data["kind"]))
        AlertNotifications.post(context, notice, sound = data["sound"] == "1")
    }

    private fun open(context: Context, sealed: String): Pushed? = Pushed.of(Native.openPush(dataDir(context), sealed))

    private val background = java.util.concurrent.Executors.newSingleThreadExecutor()

    /** Runs off the main thread and the core's events, one at a time. */
    fun later(work: () -> Unit) = background.execute(work)

    /** What the agents do, heard from the core or a push: kept, merged by server, for the widget and the ongoing notification. */
    @Synchronized
    fun work(context: Context, heard: List<ServerWork>) {
        val now = System.currentTimeMillis() / 1000.0
        val merged = ServerWork.merge(KeptWork.servers, heard, now)
        KeptWork.servers = merged
        AgentsWidget.refresh(context)
        if (settings.live) OngoingWork.show(context, AgentsWork.of(merged)) else OngoingWork.cancel(context)
    }

    /** The notification settings as the core last told them, for when it isn't running. */
    var settings: NotificationSettings
        get() = Preferences.string("notifications")?.let(::parseJSON)?.let(NotificationSettings::of) ?: NotificationSettings()
        set(value) = Preferences.set("notifications", value.json.toString())
}

/** Each server's work as last heard, which the widget draws. */
object KeptWork {
    private const val KEY = "work"

    var servers: List<ServerWork>
        get() = parseJSONArray(Preferences.string(KEY))?.let(ServerWork::list) ?: emptyList()
        set(value) = Preferences.set(KEY, ServerWork.listJSON(value))

    fun clear(context: Context) {
        Preferences.remove(KEY)
        AgentsWidget.refresh(context)
        OngoingWork.cancel(context)
    }
}
