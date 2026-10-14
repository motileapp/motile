package app.motile

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import app.motile.core.SignInSession

/**
 * The one activity. The dev scripts start it with the variables the Apple clients read from their
 * environment, as extras: `MOTILE_AUTH_URL`, `MOTILE_LOCAL`, `MOTILE_SERVER_ADDR`,
 * `MOTILE_DEMO_SIGN_IN` and `MOTILE_DEMO_SCRIPT`.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val application = application as MotileApplication
        if (!started) {
            started = true
            val environment = ENVIRONMENT.mapNotNull { name -> intent.getStringExtra(name)?.let { name to it } }.toMap()
            application.store.start(environment)
            DemoDriver.startIfRequested(application.store, environment)
        }
        intent?.let(::opened)
        setContent { Root(application.store) }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        opened(intent)
    }

    override fun onResume() {
        super.onResume()
        SignInSession.resumed()
    }

    /** A `motile://` address the browser was sent back to, after a sign-in or connecting Linear. */
    private fun opened(intent: Intent) {
        val data = intent.data ?: return
        if (data.scheme != "motile") return
        SignInSession.opened(data.toString())
    }

    companion object {
        private var started = false
        private val ENVIRONMENT = listOf(
            "MOTILE_AUTH_URL", "MOTILE_LOCAL", "MOTILE_SERVER_ADDR", "MOTILE_DATA_DIR", "MOTILE_DEMO",
            "MOTILE_DEMO_SIGN_IN", "MOTILE_DEMO_SCRIPT",
        )
    }
}
