package app.motile.notifications

import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.app.PendingIntent
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import app.motile.MainActivity
import app.motile.R
import app.motile.core.AgentsWork
import app.motile.core.WorkState

/** The ongoing notification of what the agents do while any works or waits: a Live Update from Android 16. */
object OngoingWork {
    private const val TAG = "work"
    private const val ID = 2
    private const val SHOWN_ITEMS = 3

    /** Its lines: the first items still worth showing, what waits for the user first. */
    fun lines(work: AgentsWork, now: Double): List<String> = work.shown(now).take(SHOWN_ITEMS).map { item ->
        val state = when (item.state) {
            WorkState.Waiting -> "needs you"
            WorkState.Working -> "working"
            WorkState.Failed -> "failed"
            else -> "finished"
        }
        "${item.title} · $state"
    }

    @SuppressLint("MissingPermission")
    fun show(context: Context, work: AgentsWork) {
        if (work.active == 0 || !AlertNotifications.allowed(context)) return cancel(context)
        AlertNotifications.createChannels(context)
        val lines = lines(work, System.currentTimeMillis() / 1000.0)
        val open = PendingIntent.getActivity(context, 0, Intent(context, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val notification = NotificationCompat.Builder(context, AlertNotifications.ONGOING)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(work.summary)
            .setContentText(lines.firstOrNull())
            .setStyle(NotificationCompat.BigTextStyle().bigText(lines.joinToString("\n")))
            .setShortCriticalText("${work.active}")
            .setRequestPromotedOngoing(true)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_PROGRESS)
            .setContentIntent(open)
            .build()
        NotificationManagerCompat.from(context).notify(TAG, ID, notification)
    }

    fun cancel(context: Context) {
        NotificationManagerCompat.from(context).cancel(TAG, ID)
    }
}
