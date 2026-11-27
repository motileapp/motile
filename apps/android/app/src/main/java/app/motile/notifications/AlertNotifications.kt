package app.motile.notifications

import android.Manifest
import android.annotation.SuppressLint
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.net.toUri
import app.motile.MainActivity
import app.motile.R
import app.motile.core.AlertKind
import app.motile.core.PushedAlert
import app.motile.core.ThreadLink

/** What a notification of an alert says, and the channel it goes on. */
data class AlertNotice(val link: ThreadLink, val title: String, val subtitle: String?, val body: String, val channel: String) {
    companion object {
        fun of(alert: PushedAlert) = AlertNotice(
            ThreadLink(alert.serverID, alert.threadID), alert.threadTitle, alert.project.ifEmpty { null }, alert.body, channelOf(alert.kind),
        )

        /** What shows of an alert that couldn't be opened. */
        fun placeholder(threadID: String, kind: AlertKind) =
            AlertNotice(ThreadLink("", threadID), "Motile", null, kind.placeholder, channelOf(kind))

        fun channelOf(kind: AlertKind): String = if (kind.waitsForUser) AlertNotifications.NEEDS_YOU else AlertNotifications.UPDATES
    }
}

/** The system's notifications of the servers' alerts: one per thread, a newer one in the older's place. */
object AlertNotifications {
    const val NEEDS_YOU = "needs_you"
    const val UPDATES = "agent_updates"
    const val ONGOING = "ongoing"
    private const val ALERT_ID = 1

    /** Whether the system lets the client notify. */
    fun allowed(context: Context): Boolean {
        val asked = Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
        return asked && NotificationManagerCompat.from(context).areNotificationsEnabled()
    }

    fun createChannels(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannels(
            listOf(
                NotificationChannel(NEEDS_YOU, "Needs you", NotificationManager.IMPORTANCE_HIGH),
                NotificationChannel(UPDATES, "Agent updates", NotificationManager.IMPORTANCE_DEFAULT),
                NotificationChannel(ONGOING, "Ongoing activity", NotificationManager.IMPORTANCE_LOW),
            ),
        )
    }

    @SuppressLint("MissingPermission")
    fun post(context: Context, notice: AlertNotice, sound: Boolean) {
        if (!allowed(context)) return
        createChannels(context)
        val notification = NotificationCompat.Builder(context, notice.channel)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(notice.title)
            .setSubText(notice.subtitle)
            .setContentText(notice.body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(notice.body))
            .setCategory(if (notice.channel == NEEDS_YOU) NotificationCompat.CATEGORY_REMINDER else NotificationCompat.CATEGORY_STATUS)
            .setContentIntent(opening(context, notice.link))
            .setAutoCancel(true)
            .setSilent(!sound)
            .build()
        NotificationManagerCompat.from(context).notify(notice.link.threadID, ALERT_ID, notification)
    }

    /** Takes away the notification of the thread, which has been opened. */
    fun cancel(context: Context, threadID: String) {
        NotificationManagerCompat.from(context).cancel(threadID, ALERT_ID)
    }

    /** Takes away every alert's notification: in front, what came while the client was away has been seen. */
    fun cancelAll(context: Context) {
        val manager = NotificationManagerCompat.from(context)
        for (shown in manager.activeNotifications) {
            if (shown.id == ALERT_ID) manager.cancel(shown.tag, ALERT_ID)
        }
    }

    /** Opens the thread in the client. */
    fun opening(context: Context, link: ThreadLink): PendingIntent {
        val intent = Intent(Intent.ACTION_VIEW, link.url.toUri(), context, MainActivity::class.java)
        return PendingIntent.getActivity(context, link.threadID.hashCode(), intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    /** The system's settings of the client's notifications, where the user allows them. */
    fun openSystemSettings(context: Context) {
        val intent = Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
            .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        context.startActivity(intent)
    }
}
