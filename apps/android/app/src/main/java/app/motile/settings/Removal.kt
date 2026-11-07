package app.motile.settings

import androidx.compose.runtime.Composable
import app.motile.LocalStore
import app.motile.core.AgentAccount
import app.motile.core.AppStore
import app.motile.core.CoreError
import app.motile.core.GatewayAccount
import app.motile.core.McpServerRow
import app.motile.core.Project
import app.motile.core.Server
import app.motile.ui.Answer
import app.motile.ui.Asking
import app.motile.ui.ButtonVariant
import app.motile.ui.Confirmation

/**
 * What the settings take away once the user says so: a server, every copy of a server's threads, a
 * project, an agent's account, the account's gateway, one of its accounts or one of its MCP servers,
 * or the backup.
 */
sealed interface Removal {
    data class OfServer(val server: Server) : Removal
    data class OfServerCompletely(val server: Server) : Removal
    data class OfProject(val project: Project) : Removal
    data class OfAccount(val account: AgentAccount, val server: Server) : Removal
    data class OfGateway(val name: String) : Removal
    data class OfGatewayAccount(val account: GatewayAccount) : Removal
    data class OfMcpServer(val mcpServer: McpServerRow) : Removal
    data object OfBackup : Removal

    val title: String
        get() = when (this) {
            is OfServer -> "Unlink “${server.name}”?"
            is OfServerCompletely -> "Delete “${server.name}”?"
            is OfProject -> "Remove “${project.name}”?"
            is OfAccount -> "Remove “${account.agent.title} · ${account.name}”?"
            is OfGateway -> "Remove Your Gateway?"
            is OfGatewayAccount -> "Remove “${account.name}” from Your Gateway?"
            is OfMcpServer -> "Remove the “${mcpServer.name}” MCP Server?"
            OfBackup -> "Turn Off the Backup?"
        }

    fun message(store: AppStore): String = when (this) {
        is OfServer ->
            "Your account lets go of ${server.name}. Its threads stay readable, and can go on, from the copies your other servers keep. If it is your only server, nothing is kept. What is on it stays there."
        is OfServerCompletely ->
            "${server.name} leaves your account and every copy of its threads is deleted, on all your servers and clients${if (store.backup.bucket == null) "" else " and in your backup"}. What is on ${server.name} itself stays there."
        is OfProject -> "${project.name} leaves your clients. Its folder stays on ${store.server(project.serverID)?.name ?: "its server"} with everything in it."
        is OfAccount -> "Its threads move to the default account on ${server.name}. Its folder and its sign-in stay there."
        is OfGateway ->
            "“$name” leaves your account, and your servers stop using it. Its threads move to your default accounts."
        is OfGatewayAccount -> "Your gateway forgets its sign-in, and its threads move to your other accounts."
        is OfMcpServer -> "Agents won't get its tools anymore, and your gateway forgets its login."
        OfBackup -> "Your servers stop backing up to the bucket and sync through your clients again. What is in the bucket stays there."
    }

    /** The button that goes ahead. */
    val answer: String get() = when (this) {
        is OfServer -> "Unlink"
        is OfServerCompletely -> "Delete"
        OfBackup -> "Turn Off"
        else -> "Remove"
    }

    fun run(store: AppStore) = when (this) {
        is OfServer -> store.removeServer(server)
        is OfServerCompletely -> store.deleteServerCompletely(server)
        is OfProject -> store.removeProject(project)
        is OfAccount -> store.removeAgentAccount(account, server)
        is OfGateway -> store.gateway.remove()
        is OfGatewayAccount -> store.gateway.removeAccount(account)
        is OfMcpServer -> store.gateway.mcpServers.remove(mcpServer)
        OfBackup -> store.setBackup(null) { error -> error?.let { store.alert(CoreError(it)) } }
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
