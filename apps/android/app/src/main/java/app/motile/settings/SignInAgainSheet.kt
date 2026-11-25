package app.motile.settings

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import app.motile.LocalCloseSheet
import app.motile.LocalStore
import app.motile.core.AccountSignIn
import app.motile.core.ThreadSignIn
import app.motile.platform.Symbol
import app.motile.theme.LocalPalette
import app.motile.theme.TextSize
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.Text

/**
 * Signs an agent's account in again once its sign-in stopped working, and has the thread's agent
 * go on. The agent signs in here, as its account's sheet does; on a server too old for it, with a
 * command run on the server.
 */
@Composable
fun SignInAgainSheet(request: ThreadSignIn) {
    val store = LocalStore.current
    val close = LocalCloseSheet.current
    val signIn = remember { AccountSignIn() }
    val server = store.server(store.threads[request.threadID]?.serverID)
    val account = server?.agentAccounts?.firstOrNull { it.id == request.accountID }
    val agentName = account?.agent?.title ?: "the agent"
    val serverName = server?.shortName ?: "its server"
    val signsInHere = account != null && server.signsIn(account.agent)
    val signedInAs = (signIn.step as? AccountSignIn.Step.SignedIn)?.who
    val signedIn = signedInAs != null
    // The agent, and the account when the agent has several of its own on the server.
    val accountTitle = when {
        account == null -> agentName
        server.agentAccounts.count { it.agent == account.agent && !it.gateway } > 1 -> "$agentName (${account.name})"
        else -> agentName
    }
    val message = "$accountTitle on $serverName was signed out, so the agent stopped. Sign in again and it goes on where it left off."
    val caption = when {
        !signsInHere -> "Run it in a terminal on $serverName, then continue."
        signedIn -> "Continue has the agent go on where it left off."
        else -> signIn.waitingCaption ?: "Opens ${AccountSignIn.page(account.agent)}."
    }

    fun start() {
        if (!signsInHere || signIn.step != AccountSignIn.Step.Idle) return
        signIn.start(account, server, store) {}
    }

    fun finish() {
        if (account == null) return
        signIn.finish(account, store) {}
    }

    fun goOn() {
        close()
        store.continueThread()
    }

    LaunchedEffect(Unit) { start() }
    DisposableEffect(Unit) { onDispose { signIn.cancel(store) } }
    val trailing: (@Composable () -> Unit)? = if (signedIn || !signsInHere) {
        { ActionButton("Continue", variant = ButtonVariant.Primary, onClick = ::goOn) }
    } else null
    Column(Modifier.fillMaxSize()) {
        CompositionLocalProvider(LocalBarLeading provides BarLeading.Close(close)) { SettingsBar("Sign In to $agentName", trailing) }
        SettingsList(Modifier.weight(1f)) {
            group(
                headerContent = { Text(message, size = NOTE_TEXT, weight = FontWeight.Medium, color = LocalPalette.current.mutedForeground) },
                footer = caption,
                failure = signIn.error,
            ) {
                if (signsInHere) {
                    signInRows(signIn, signedInAs, signsInAgain = false, start = ::start, finish = ::finish)
                } else if (account != null) {
                    row {
                        SettingsRow {
                            SelectionContainer(Modifier.weight(1f)) {
                                Text(account.signInCommand, style = Theme.uiMono(TextSize.Sm), color = LocalPalette.current.foreground)
                            }
                        }
                    }
                    row { SettingsPressRow({ shareText(account.signInCommand) }) { SettingsActionLabel("Share Command", Symbol.Share) } }
                }
            }
        }
    }
}
