package app.motile.core

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/**
 * Signing an account of Claude Code in from its sheet: its server runs the CLI's sign-in, the
 * user signs in on the page it links to, and pastes the code that page shows, which saves the
 * account too.
 */
class AccountSignIn {
    sealed interface Step {
        data object Idle : Step
        data object Starting : Step
        data class Waiting(val id: String, val url: String) : Step
        data class SignedIn(val who: String) : Step
    }

    var step by mutableStateOf<Step>(Step.Idle)
        private set
    var finishing by mutableStateOf(false)
        private set
    var error by mutableStateOf<String?>(null)
        private set
    var code by mutableStateOf("")
    private var serverID = ""
    /** Who signed in before this sign-in, who still is if it fails. */
    private var before: Step = Step.Idle

    val canFinish: Boolean get() = !finishing && code.isNotBlank()

    fun start(account: AgentAccount, server: Server, store: AppStore) {
        if (step == Step.Starting) return
        val signedIn = step
        cancel(store)
        if (signedIn is Step.SignedIn) before = signedIn
        serverID = server.id
        step = Step.Starting
        error = null
        val request = mapOf("type" to "start_sign_in", "account" to account.json)
        store.core.send("request", mapOf("server_id" to server.id, "request" to request)) { result ->
            if (step != Step.Starting) return@send
            val answer = result.getOrElse { return@send fail(result.coreError?.message ?: "The sign-in couldn't be started.") }
            val url = answer.string("url").takeIf { it.isNotEmpty() } ?: return@send fail("Claude Code didn't give a page to sign in on.")
            step = Step.Waiting(answer.string("id"), url)
        }
    }

    /** Hands the code to the sign-in and saves `account` with it; `saved` gets the saved account's id. */
    fun finish(account: AgentAccount, store: AppStore, saved: (String) -> Unit) {
        val waiting = step as? Step.Waiting ?: return
        if (!canFinish) return
        finishing = true
        error = null
        val request = mapOf("type" to "finish_sign_in", "id" to waiting.id, "code" to code.trim(), "account" to account.json)
        store.core.send("request", mapOf("server_id" to serverID, "request" to request)) { result ->
            if ((step as? Step.Waiting)?.id != waiting.id) return@send
            finishing = false
            val answer = result.getOrElse { return@send refused(result.coreError?.message ?: "The sign-in didn't finish. Sign in again.") }
            code = ""
            saved(answer.string("account"))
            val who = answer.optionalString("email")?.let { listOfNotNull(it, answer.optionalString("plan")).joinToString(" · ") }
            step = Step.SignedIn(who ?: "Claude Code")
        }
    }

    /** A sign-in that is over says so; one refused for the account it would save waits for the code again. */
    private fun refused(message: String) {
        if (!message.endsWith("Sign in again.")) {
            error = message
            return
        }
        code = ""
        fail(message)
    }

    /** Stops a sign-in that waits for its code, as closing the sheet or changing the folder does. */
    fun cancel(store: AppStore) {
        (step as? Step.Waiting)?.let {
            store.core.send("request", mapOf("server_id" to serverID, "request" to mapOf("type" to "cancel_sign_in", "id" to it.id)))
        }
        step = Step.Idle
        before = Step.Idle
        finishing = false
        code = ""
        error = null
    }

    private fun fail(message: String) {
        step = before
        error = message
    }
}
