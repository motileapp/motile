package app.motile.core

import androidx.core.net.toUri
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.motile.BuildConfig
import app.motile.platform.Platform
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import kotlin.concurrent.thread

/**
 * Finds out about new releases and replaces this client with one, as the Mac app's updater does:
 * it downloads the release's APK, checks that it is this client signed with this client's key, and
 * hands it to Android's installer, which asks the user and starts the new version.
 */
class AppUpdater {
    sealed interface State {
        data object Idle : State
        /** The user asked and the answer isn't here yet. */
        data object Checking : State
        data object UpToDate : State
        data class Available(val version: String) : State
        data class Downloading(val version: String, val fraction: Double) : State
        /** With Android's installer, which asks the user before it replaces this client. */
        data class Installing(val version: String) : State
        data class Failed(val message: String) : State
    }

    var state by mutableStateOf<State>(State.Idle)
        private set
    /** The newest release's version. The servers are compared with it as well. */
    var latest by mutableStateOf<String?>(null)
        private set
    val current: String = BuildConfig.VERSION_NAME

    /** Called with each release found, and whether the user asked for it. */
    var onFound: ((asked: Boolean) -> Unit)? = null
    /** Where releases are looked for: GitHub's, or the dev app's stand-in. */
    var releases = RELEASES
    private val main = Handler(Looper.getMainLooper())
    @Volatile
    private var downloading: String? = null

    /** Looks for a new release now and every few hours. */
    fun start() {
        check()
        main.postDelayed(object : Runnable {
            override fun run() {
                check()
                main.postDelayed(this, CHECK_EVERY)
            }
        }, CHECK_EVERY)
    }

    /** `asked` is for when the user wants to know: finding nothing new is then said too. */
    fun check(asked: Boolean = false) {
        if (state is State.Checking || state is State.Downloading || state is State.Installing) return
        if (asked) state = State.Checking
        val releases = releases
        thread(isDaemon = true) {
            // The address redirects to the release's own page, whose last part is its tag.
            val tag = runCatching {
                val connection = URL("$releases/latest").openConnection() as HttpURLConnection
                connection.requestMethod = "HEAD"
                connection.useCaches = false
                connection.instanceFollowRedirects = true
                connection.responseCode
                connection.url.path.substringAfterLast('/').also { connection.disconnect() }
            }.getOrNull()
            val version = tag?.takeIf { it.startsWith("v") }?.drop(1)
            main.post { found(version, asked) }
        }
    }

    private fun found(version: String?, asked: Boolean) {
        if (version == null) {
            if (asked) state = State.Failed("GitHub couldn’t be reached to look for a new version.")
            return
        }
        latest = version
        onFound?.invoke(asked)
        when {
            Version.isOlder(current, version) -> state = State.Available(version)
            asked -> {
                state = State.UpToDate
                main.postDelayed({ if (state == State.UpToDate) state = State.Idle }, 4000)
            }
            state == State.Checking -> state = State.Idle
        }
    }

    /** Why this copy can't replace itself, if it can't. */
    val obstacle: String?
        get() = when {
            BuildConfig.DEBUG && releases == RELEASES ->
                "This copy of Motile is a development build, so it can’t update itself. Download the new version from motile.app."
            fromPlay -> "This copy of Motile came from Google Play, which updates it."
            else -> null
        }

    private val fromPlay: Boolean
        get() {
            val context = Platform.context
            val installer = runCatching {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) context.packageManager.getInstallSourceInfo(context.packageName).installingPackageName
                else @Suppress("DEPRECATION") context.packageManager.getInstallerPackageName(context.packageName)
            }.getOrNull()
            return installer == "com.android.vending"
        }

    /** Whether Android lets this client install what it downloads, which the user allows once in its settings. */
    var canInstall by mutableStateOf(false)
        private set

    /** Reads again whether Android lets this client install, as the user may have changed it in its settings. */
    fun readPermission() {
        canInstall = Platform.context.packageManager.canRequestPackageInstalls()
    }

    /** Opens the system's page that lets this client install apps. */
    fun allowInstalling() {
        val intent = Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, "package:${Platform.context.packageName}".toUri())
        Platform.context.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    fun install() {
        val version = (state as? State.Available)?.version ?: return
        obstacle?.let {
            state = State.Failed(it)
            return
        }
        readPermission()
        if (!canInstall) return allowInstalling()
        downloading = version
        state = State.Downloading(version, 0.0)
        val url = "$releases/download/v$version/$ASSET"
        thread(isDaemon = true) {
            val result = runCatching { download(url, version) }
            main.post {
                if (downloading != version) return@post
                downloading = null
                result.fold(
                    onSuccess = { apk -> hand(apk, version) },
                    onFailure = { state = State.Failed(it.message ?: "The new version couldn’t be downloaded.") },
                )
            }
        }
    }

    /** Stops the download and offers the update again. */
    fun cancelDownload() {
        val version = (state as? State.Downloading)?.version ?: return
        downloading = null
        state = State.Available(version)
    }

    /** Back to offering the update, after it failed. */
    fun retry() {
        state = State.Idle
        check(asked = true)
    }

    private fun download(url: String, version: String): File {
        val connection = URL(url).openConnection() as HttpURLConnection
        connection.instanceFollowRedirects = true
        val status = connection.responseCode
        if (status != 200) throw CoreError("The new version couldn’t be downloaded ($status).")
        val folder = File(Platform.context.cacheDir, "updates").apply {
            deleteRecursively()
            mkdirs()
        }
        val apk = File(folder, ASSET)
        val size = connection.contentLengthLong
        var received = 0L
        var shown = 0.0
        connection.inputStream.use { input ->
            apk.outputStream().use { output ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    if (downloading != version) throw CoreError("The download was stopped.")
                    val read = input.read(buffer)
                    if (read < 0) break
                    output.write(buffer, 0, read)
                    received += read
                    val fraction = if (size > 0) received.toDouble() / size else 0.0
                    if (fraction - shown >= 0.01) {
                        shown = fraction
                        main.post { if (downloading == version) state = State.Downloading(version, fraction) }
                    }
                }
            }
        }
        verify(apk)
        return apk
    }

    /** The download has to be this client, newer, and signed with the key this one was. */
    private fun verify(apk: File) {
        val manager = Platform.context.packageManager
        val flags = PackageManager.GET_SIGNING_CERTIFICATES
        val downloaded = manager.getPackageArchiveInfo(apk.path, flags)
        val running = manager.getPackageInfo(Platform.context.packageName, flags)
        if (downloaded == null || downloaded.packageName != running.packageName) throw CoreError("The download isn’t a copy of Motile.")
        if (signers(downloaded) != signers(running)) {
            throw CoreError("The download isn’t signed by Motile’s developer, so it wasn’t installed.")
        }
        if (downloaded.longVersionCode <= running.longVersionCode) throw CoreError("The download is no newer than this copy of Motile.")
    }

    private fun signers(info: PackageInfo): Set<String> =
        info.signingInfo?.apkContentsSigners?.map { it.toCharsString() }?.toSet() ?: emptySet()

    /** Gives the APK to Android's installer, which asks the user and then replaces this client and starts it. */
    private fun hand(apk: File, version: String) {
        state = State.Installing(version)
        thread(isDaemon = true) {
            val result = runCatching {
                val installer = Platform.context.packageManager.packageInstaller
                val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
                params.setAppPackageName(Platform.context.packageName)
                val id = installer.createSession(params)
                installer.openSession(id).use { session ->
                    apk.inputStream().use { input ->
                        session.openWrite(ASSET, 0, apk.length()).use { output ->
                            input.copyTo(output)
                            session.fsync(output)
                        }
                    }
                    val told = Intent(Platform.context, UpdateReceiver::class.java).setPackage(Platform.context.packageName)
                    val flags = PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE
                    session.commit(PendingIntent.getBroadcast(Platform.context, id, told, flags).intentSender)
                }
            }
            result.exceptionOrNull()?.let { error ->
                main.post { state = State.Failed("The new version couldn’t be installed: ${error.message}") }
            }
        }
    }

    /** What Android's installer said. */
    internal fun installerSaid(status: Int, message: String?, confirm: Intent?) {
        val version = (state as? State.Installing)?.version ?: return
        when (status) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> confirm?.let {
                Platform.context.startActivity(it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            }
            PackageInstaller.STATUS_SUCCESS -> Unit
            // The user said no; the update is still on offer.
            PackageInstaller.STATUS_FAILURE_ABORTED -> state = State.Available(version)
            else -> state = State.Failed(message?.let { "The new version couldn’t be installed: $it" } ?: "The new version couldn’t be installed.")
        }
    }

    companion object {
        private const val RELEASES = "https://github.com/motileapp/motile/releases"
        /** The client's file in every release. */
        const val ASSET = "Motile-android.apk"
        private const val CHECK_EVERY = 6 * 3600 * 1000L

        /** The updater of the running client, for the installer's answer. */
        var shared: AppUpdater? = null
    }
}

/** Takes the installer's answers to `AppUpdater`. */
class UpdateReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
        val message = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE)
        @Suppress("DEPRECATION")
        val confirm = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
        else intent.getParcelableExtra(Intent.EXTRA_INTENT)
        AppUpdater.shared?.installerSaid(status, message, confirm)
    }
}
