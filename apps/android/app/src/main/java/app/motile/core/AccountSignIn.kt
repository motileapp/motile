package app.motile.core

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/**
 * Signing an account in from its sheet: its server runs the CLI's sign-in and the user signs in
 * on the page it links to. Claude Code's page shows a code to paste here; Codex's takes the code
 * shown here, and the sign-in finishes by itself once it was entered. Signing in saves the account
 * too.
 */
class AccountSignIn {
    sealed interface Step {
        data object Idle : Step
        data object Starting : Step
        /** `deviceCode` is the code to enter on the page, for Codex. */
        data class Waiting(val id: String, val url: String, val deviceCode: String?) : Step
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

    /** What to do on the page while the sign-in waits. */
    val waitingCaption: String?
        get() {
            val waiting = step as? Step.Waiting ?: return null
            if (waiting.deviceCode == null) return "Sign in on the page, then paste the code it shows."
            return "Sign in on the page and enter the code shown here."
        }

    val canFinish: Boolean
        get() {
            if (finishing) return false
            if ((step as? Step.Waiting)?.deviceCode != null) return true
            return code.isNotBlank()
        }

    /** Starts the sign-in; one that finishes by itself saves `account` and hands `saved` its id. */
    fun start(account: AgentAccount, server: Server, store: AppStore, saved: (String) -> Unit) {
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
            val url = answer.string("url").takeIf { it.isNotEmpty() } ?: return@send fail("${account.agent.title} didn't give a page to sign in on.")
            val deviceCode = answer.optionalString("code")
            step = Step.Waiting(answer.string("id"), url, deviceCode)
            if (deviceCode != null) finish(account, store, saved)
        }
    }

    /**
     * Hands the code to the sign-in, or waits for Codex's to be entered, and saves `account` with
     * it; `saved` gets the saved account's id.
     */
    fun finish(account: AgentAccount, store: AppStore, saved: (String) -> Unit) {
        val waiting = step as? Step.Waiting ?: return
        if (!canFinish) return
        finishing = true
        error = null
        val request = mapOf("type" to "finish_sign_in", "id" to waiting.id, "code" to code.trim(), "account" to account.json)
        store.core.send("request", mapOf("server_id" to serverID, "request" to request)) { result ->
            if ((step as? Step.Waiting)?.id != waiting.id) return@send
            finishing = false
            val refusal = result.coreError?.message
            if (waiting.deviceCode != null && refusal?.startsWith(STILL_WAITING) == true) return@send finish(account, store, saved)
            val answer = result.getOrElse { return@send refused(refusal ?: "The sign-in didn't finish. Sign in again.") }
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

    companion object {
        /** What the server says while Codex waits for its code to be entered, which is asked again. */
        private const val STILL_WAITING = "Codex is still waiting"

        /** Whose page a sign-in of the agent opens. */
        fun page(agent: Agent): String = if (agent == Agent.Claude) "Claude's sign-in page" else "OpenAI's sign-in page"
    }
}
