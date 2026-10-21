package app.motile.settings

import android.text.format.Formatter
import app.motile.core.AgentAccount
import app.motile.core.AppStore
import app.motile.core.Server
import app.motile.core.ServerSync
import app.motile.core.Time
import app.motile.platform.Platform

// What the settings say of a server, an account and the images kept here.

val Server.settingsDescription: String
    get() {
        if (state != Server.State.Connected) return stateLabel
        return "Connected · version $version · $installedAgents"
    }

val Server.stateLabel: String
    get() = when (state) {
        Server.State.Connected -> "Connected"
        Server.State.Connecting -> "Connecting"
        Server.State.Disconnected -> "Offline"
        Server.State.Refused -> "This server no longer accepts this ${Platform.device}"
    }

/** The line under a server, with when it last synced unless it is syncing now. */
fun ServerSync.line(now: Double): String {
    val syncedAt = syncedAt?.takeIf { state != ServerSync.State.Copying } ?: return text
    val ago = Time.ago(syncedAt, now)
    val whenSynced = if (ago == "now") "just now" else "$ago ago"
    return if (state == ServerSync.State.Synced) "$text $whenSynced" else "$text · synced $whenSynced"
}

val Server.installedAgents: String
    get() {
        val named = agents.entries.sortedBy { it.key.raw }.map { "${it.key.title} ${it.value}" }
        return if (named.isEmpty()) "no agent installed" else named.joinToString(", ")
    }

/** Why its accounts can't be shown or changed, if they can't. */
val Server.accountsUnavailable: String?
    get() {
        if (state != Server.State.Connected) return settingsDescription
        if (!switchesAccounts) return "Update it to give its agents more accounts"
        return null
    }

val Server.textModelName: String
    get() = models.firstOrNull { it.id == textModel }?.name ?: "Automatic"

val AgentAccount.settingsDescription: String
    get() {
        val signedIn = email?.let { listOfNotNull(it, plan).joinToString(" · ") }
        val who = signedIn ?: if (variables.isEmpty()) "Not signed in" else "Signs in with its variables"
        return if (folder.isEmpty()) who else "$who · $folder"
    }

/** The servers keep every image and video; the ones kept here only make threads open with them. */
val AppStore.mediaStorageDescription: String
    get() {
        if (mediaStorage == null) return "Kept on this ${Platform.device} so threads open with them"
        return "$mediaStorageUsed on this ${Platform.device}. Your servers keep them all."
    }

val AppStore.mediaStorageUsed: String
    get() {
        val storage = mediaStorage ?: return ""
        return "${bytes(storage.used)} of ${bytes(storage.limit)}"
    }

private fun bytes(count: Long): String = Formatter.formatFileSize(Platform.context, count)

fun steersDescription(steers: Boolean): String =
    if (steers) "The agent reads it at once, in the turn that runs" else "Waits for the turn to end and starts the next one"
