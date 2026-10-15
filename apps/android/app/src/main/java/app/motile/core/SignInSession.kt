package app.motile.core

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Handler
import android.os.Looper
import androidx.browser.customtabs.CustomTabsIntent

/**
 * Opens a page in a Custom Tab, as ASWebAuthenticationSession does on the Apple clients, and
 * reports the `motile://` address the page sent the browser back to: nothing when the user came
 * back without it. MainActivity hands the session the addresses it is opened with.
 */
class SignInSession {
    private var done: ((String?) -> Unit)? = null

    fun start(context: Context, url: String, done: (String?) -> Unit) {
        current?.finish(null)
        this.done = done
        current = this
        val intent = CustomTabsIntent.Builder().setShowTitle(true).build()
        intent.intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        try {
            intent.launchUrl(context, Uri.parse(url))
        } catch (_: Exception) {
            context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }
    }

    private fun finish(callback: String?) {
        val done = done ?: return
        this.done = null
        if (current === this) current = null
        done(callback)
    }

    companion object {
        private var current: SignInSession? = null
        private var answered = false

        /** The browser sent the user back to a `motile://` address. */
        fun opened(url: String) {
            answered = true
            current?.finish(url)
        }

        /** The client is in front again. Without an address the user closed the page, which ends the session. */
        fun resumed() {
            answered = false
            Handler(Looper.getMainLooper()).postDelayed({
                if (!answered) current?.finish(null)
            }, 600)
        }
    }
}
