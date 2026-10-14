package app.motile.core

import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.motile.BuildConfig
import java.net.HttpURLConnection
import java.net.URL
import kotlin.concurrent.thread

/** Finds out about new releases, which the servers are compared with. The client itself is updated by the store it came from. */
class AppUpdater {
    /** The newest release's version. */
    var latest by mutableStateOf<String?>(null)
        private set
    val current: String = BuildConfig.VERSION_NAME

    /** Called with each release found, and whether the user asked for it. */
    var onFound: ((asked: Boolean) -> Unit)? = null
    private val main = Handler(Looper.getMainLooper())

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

    fun check(asked: Boolean = false) {
        thread(isDaemon = true) {
            // The address redirects to the release's own page, whose last part is its tag.
            val tag = runCatching {
                val connection = URL(LATEST_RELEASE).openConnection() as HttpURLConnection
                connection.requestMethod = "HEAD"
                connection.useCaches = false
                connection.instanceFollowRedirects = true
                connection.connect()
                connection.responseCode
                connection.url.path.substringAfterLast('/').also { connection.disconnect() }
            }.getOrNull() ?: return@thread
            if (!tag.startsWith("v")) return@thread
            main.post {
                latest = tag.drop(1)
                onFound?.invoke(asked)
            }
        }
    }

    companion object {
        private const val LATEST_RELEASE = "https://github.com/motileapp/motile/releases/latest"
        private const val CHECK_EVERY = 6 * 3600 * 1000L
    }
}
