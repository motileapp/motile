package app.motile.settings

import androidx.compose.runtime.Composable
import app.motile.LocalStore
import app.motile.core.AgentAccount
import app.motile.core.AppStore
import app.motile.core.Project
import app.motile.core.Server
import app.motile.ui.Answer
import app.motile.ui.Asking
import app.motile.ui.ButtonVariant
import app.motile.ui.Confirmation

/** What the settings take away once the user says so: a server, every copy of a server's threads, a project or an agent's account. */
sealed interface Removal {
    data class OfServer(val server: Server) : Removal
    data class OfServerCompletely(val server: Server) : Removal
    data class OfProject(val project: Project) : Removal
    data class OfAccount(val account: AgentAccount, val server: Server) : Removal

    val title: String
        get() = when (this) {
            is OfServer -> "Unlink “${server.name}”?"
            is OfServerCompletely -> "Delete “${server.name}”?"
            is OfProject -> "Remove “${project.name}”?"
            is OfAccount -> "Remove “${account.agent.title} · ${account.name}”?"
        }

    fun message(store: AppStore): String = when (this) {
        is OfServer ->
            "Your account lets go of ${server.name}. Its threads stay readable, and can go on, from the copies your other servers keep. If it is your only server, nothing is kept. What is on it stays there."
        is OfServerCompletely ->
            "${server.name} leaves your account and every copy of its threads is deleted, on all your servers and clients. What is on ${server.name} itself stays there."
        is OfProject -> "${project.name} leaves your clients. Its folder stays on ${store.server(project.serverID)?.name ?: "its server"} with everything in it."
        is OfAccount -> "Its threads move to the default account on ${server.name}. Its folder and its sign-in stay there."
    }

    /** The button that goes ahead. */
    val answer: String get() = when (this) {
        is OfServer -> "Unlink"
        is OfServerCompletely -> "Delete"
        else -> "Remove"
    }

    fun run(store: AppStore) = when (this) {
        is OfServer -> store.removeServer(server)
        is OfServerCompletely -> store.deleteServerCompletely(server)
        is OfProject -> store.removeProject(project)
        is OfAccount -> store.removeAgentAccount(account, server)
    }
}

/** Asks before the removal, and goes ahead only when the user says so. */
@Composable
fun ConfirmsRemoval(removal: Removal?, onDismiss: () -> Unit) {
    val store = LocalStore.current
    Asking(removal, onDismiss) { asked ->
        Confirmation(asked.title, asked.message(store), listOf(Answer(asked.answer, ButtonVariant.Danger) { asked.run(store) }))
    }
}
