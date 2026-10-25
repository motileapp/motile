package app.motile.settings

import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.AccountSignIn
import app.motile.core.Agent
import app.motile.core.AgentAccount
import app.motile.core.Server
import app.motile.layout.KeyboardFocus
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.InputField
import app.motile.ui.HoldsModal
import app.motile.ui.InputVariant
import app.motile.ui.LocalCloseModal
import app.motile.ui.Spinner
import app.motile.ui.Text
import kotlinx.coroutines.delay
import java.util.UUID

/** An account the settings add or change, on its server. */
data class EditedAccount(val server: Server, val account: AgentAccount) {
    val id: String get() = "${server.id}/${account.id}"
}

/** A variable as the account's sheet edits it. */
private class EditedVariable(variable: AgentAccount.Variable) {
    val id: String = UUID.randomUUID().toString()
    var variable by mutableStateOf(variable)
}

/**
 * An account of an agent to add or change: its name, the folder its sign-in is kept in, and the
 * variables its agent is given, with the way to sign it in.
 */
@Composable
fun AgentAccountSheet(server: Server, initial: AgentAccount) {
    val store = LocalStore.current
    val close = LocalCloseModal.current
    var account by remember { mutableStateOf(initial) }
    val variables = remember { mutableStateListOf<EditedVariable>().apply { addAll(initial.variables.map(::EditedVariable)) } }
    var saving by remember { mutableStateOf(false) }
    var suggestedFolder by remember { mutableStateOf("") }
    var copied by remember { mutableStateOf(false) }
    var copies by remember { mutableIntStateOf(0) }
    val signIn = remember { AccountSignIn() }
    // The account as signing in saved it, until something else changes.
    var savedBySignIn by remember { mutableStateOf<AgentAccount?>(null) }
    val installed = Agent.entries.filter { server.agents[it] != null }
    val isNew = account.id.isEmpty()
    val title = if (isNew) "New Account" else "${account.agent.title} Account"
    val canSave = account.name.isNotBlank()
    val edited = account.copy(
        variables = variables.map { it.variable }.filter { it.name.isNotBlank() },
        sharesSessions = account.agent == Agent.Codex && account.sharesSessions,
    )
    // Signing in saved everything the form has, so there is nothing left to save or to throw away.
    val savedAsIs = savedBySignIn == edited
    // Claude Code signs in here; Codex, and a server too old for it, with a command.
    val signsInHere = account.agent == Agent.Claude && server.signsInAccounts
    val step = signIn.step
    val signedInAs = (step as? AccountSignIn.Step.SignedIn)?.who
        ?: initial.email?.takeIf { account.folder == initial.folder }?.let { listOfNotNull(it, initial.plan).joinToString(" · ") }
    val signInCaption = when {
        step is AccountSignIn.Step.Waiting -> "Sign in on the page, then paste the code it shows."
        savedAsIs -> "Signing in saved the account."
        else -> "Opens Claude's sign-in page. Signing in saves the account on ${server.name}."
    }

    LaunchedEffect(account.folder, account.agent) { signIn.cancel(store) }
    DisposableEffect(Unit) { onDispose { signIn.cancel(store) } }
    HoldsModal(saving || signIn.finishing)
    // Fills a new account's folder from its name until the folder is typed by hand.
    LaunchedEffect(account.name, account.agent) {
        if (!isNew || signIn.step != AccountSignIn.Step.Idle || (account.folder.isNotEmpty() && account.folder != suggestedFolder)) return@LaunchedEffect
        suggestedFolder = account.suggestedFolder(server.agentAccounts)
        account = account.copy(folder = suggestedFolder)
    }
    LaunchedEffect(copies) {
        if (!copied) return@LaunchedEffect
        delay(1200)
        copied = false
    }

    fun copyLink(url: String) {
        Platform.copy(url)
        copied = true
        copies += 1
    }

    fun startSignIn() = signIn.start(edited, server, store)

    fun finishSignIn() = signIn.finish(edited, store) { id ->
        account = account.copy(id = id)
        savedBySignIn = edited.copy(id = id)
    }

    fun save() {
        saving = true
        store.saveAgentAccount(edited, server) { kept ->
            saving = false
            if (kept) close()
        }
    }

    val trailing: @Composable () -> Unit = {
        when {
            saving -> Spinner(17f)
            savedAsIs -> ActionButton("Done", variant = ButtonVariant.Primary) { close() }
            else -> ActionButton("Save", variant = ButtonVariant.Primary, disabled = !canSave) { save() }
        }
    }
    Column(Modifier.fillMaxSize()) {
        CompositionLocalProvider(LocalBarLeading provides BarLeading.Close { close() }) { SettingsBar(title, trailing) }
        SettingsList(Modifier.weight(1f)) {
            if (isNew && installed.size > 1) {
                group {
                    row { SettingsPicker("Agent", installed.map { it.title to it }, account.agent) { account = account.copy(agent = it) } }
                }
            }
            group(header = "Name") {
                row { SettingsField("Personal", account.name, { account = account.copy(name = it) }) }
            }
            group(header = "Folder", footer = "Where its sign-in is kept, as ${account.folderVariable}. Empty for ${account.usualFolder}.") {
                row { SettingsField(account.usualFolder, account.folder, { account = account.copy(folder = it) }, monospaced = true) }
            }
            if (account.agent == Agent.Codex) {
                group(footer = "The folder keeps only the sign-in. Threads move between the two and go on where they were.") {
                    row {
                        SettingsToggle("Share sessions with ${account.usualFolder}", account.sharesSessions) { account = account.copy(sharesSessions = it) }
                    }
                }
            }
            group(
                header = "Variables",
                footer = "Given to its agent: an API key or a router. A sensitive value stays on ${server.name} and is never shown again.",
            ) {
                for (edited in variables) row {
                    VariableRow(edited, kept = edited.variable.sensitive && account.variables.any { it.name == edited.variable.name && it.sensitive }, server) {
                        variables.remove(edited)
                    }
                }
                row {
                    SettingsPressRow({ variables.add(EditedVariable(AgentAccount.Variable("", "", sensitive = true))) }) {
                        SettingsActionLabel("Add a Variable", Symbol.Plus)
                    }
                }
            }
            if (signsInHere) {
                group(header = "Sign In", footer = signInCaption, failure = signIn.error) {
                    if (step is AccountSignIn.Step.Waiting) {
                        row {
                            SettingsRow {
                                Text(step.url, Modifier.weight(1f), style = Theme.uiMono(13f), color = LocalPalette.current.mutedForeground, maxLines = 1)
                            }
                        }
                        row { SettingsPressRow({ Platform.open(step.url) }) { SettingsActionLabel("Open the Sign-In Page", Symbol.SquareArrowOutUpRight) } }
                        row {
                            SettingsPressRow({ copyLink(step.url) }) {
                                SettingsActionLabel(if (copied) "Copied" else "Copy the Link", if (copied) Symbol.Check else Symbol.Copy)
                            }
                        }
                        row {
                            SettingsRow {
                                Box(Modifier.weight(1f).height(ControlSize.Large.height)) {
                                    InputField(
                                        "Paste the code", signIn.code, { signIn.code = it }, variant = InputVariant.Bare, size = ControlSize.Large,
                                        monospaced = true, keyboardOptions = plainKeyboard.copy(imeAction = ImeAction.Done), onSubmit = ::finishSignIn,
                                    )
                                }
                                if (signIn.finishing) {
                                    Spinner(17f)
                                } else {
                                    ActionButton("Sign In", variant = ButtonVariant.Link, disabled = !signIn.canFinish || !canSave, onClick = ::finishSignIn)
                                }
                            }
                        }
                    } else {
                        signedInAs?.let { who -> row { SettingsRow { RowText("Signed in as $who", color = LocalPalette.current.foreground) } } }
                        row {
                            SettingsPressRow(::startSignIn, enabled = step != AccountSignIn.Step.Starting) {
                                SettingsActionLabel(if (signedInAs == null) "Sign In" else "Sign In Again", Symbol.CircleUser)
                                Spacer(Modifier.weight(1f))
                                if (step == AccountSignIn.Step.Starting) Spinner(17f)
                            }
                        }
                    }
                }
            } else group(header = "Sign In", footer = "Run it in a terminal on ${server.name}. Who is signed in shows in the list once the agent says.") {
                row {
                    SettingsRow {
                        SelectionContainer(Modifier.weight(1f)) {
                            Text(account.signInCommand, style = Theme.uiMono(13f), color = LocalPalette.current.foreground)
                        }
                    }
                }
                row {
                    SettingsPressRow({
                        Platform.copy(account.signInCommand)
                        copied = true
                        copies += 1
                    }) {
                        SettingsActionLabel(if (copied) "Copied" else "Copy Command", if (copied) Symbol.Check else Symbol.Copy)
                    }
                }
                row { SettingsPressRow({ shareText(account.signInCommand) }) { SettingsActionLabel("Share Command", Symbol.Share) } }
            }
        }
    }
}

/** A variable's name and value, with whether the value is sensitive and the way to take it out. */
@Composable
private fun VariableRow(edited: EditedVariable, kept: Boolean, server: Server, remove: () -> Unit) {
    val variable = edited.variable
    SettingsRow {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Box(Modifier.fillMaxWidth().height(ControlSize.Small.height)) {
                InputField(
                    "NAME", variable.name, { edited.variable = variable.copy(name = it) }, variant = InputVariant.Bare, size = ControlSize.Small,
                    monospaced = true, keyboardOptions = plainKeyboard,
                )
            }
            Box(Modifier.fillMaxWidth().height(ControlSize.Regular.height)) {
                InputField(
                    if (kept) "••••••••" else "value", variable.value, { edited.variable = variable.copy(value = it) }, variant = InputVariant.Bare,
                    monospaced = true, secure = variable.sensitive, keyboardOptions = plainKeyboard,
                    )
            }
        }
        ActionButton(
            icon = if (variable.sensitive) Symbol.EyeOff else Symbol.Eye,
            help = if (variable.sensitive) "Sensitive: hidden and kept on ${server.name}" else "Shown: mark it sensitive",
            selected = variable.sensitive,
        ) { edited.variable = variable.copy(sensitive = !variable.sensitive) }
        ActionButton(icon = Symbol.X, help = "Remove the variable", onClick = remove)
    }
}

private fun shareText(text: String) {
    val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text)
    Platform.context.startActivity(Intent.createChooser(send, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
}
