package app.motile.settings

import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
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
 * variables its agent is given, with the command that signs it in.
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
    val installed = Agent.entries.filter { server.agents[it] != null }
    val isNew = account.id.isEmpty()
    val title = if (isNew) "New Account" else "${account.agent.title} Account"
    val canSave = account.name.isNotBlank()

    // Fills a new account's folder from its name until the folder is typed by hand.
    LaunchedEffect(account.name, account.agent) {
        if (!isNew || (account.folder.isNotEmpty() && account.folder != suggestedFolder)) return@LaunchedEffect
        suggestedFolder = account.suggestedFolder(server.agentAccounts)
        account = account.copy(folder = suggestedFolder)
    }
    LaunchedEffect(copies) {
        if (!copied) return@LaunchedEffect
        delay(1200)
        copied = false
    }

    fun save() {
        val saved = account.copy(
            variables = variables.map { it.variable }.filter { it.name.isNotBlank() },
            sharesSessions = account.agent == Agent.Codex && account.sharesSessions,
        )
        saving = true
        store.saveAgentAccount(saved, server) { kept ->
            saving = false
            if (kept) close()
        }
    }

    val trailing: @Composable () -> Unit = {
        if (saving) Spinner(17f) else ActionButton("Save", variant = ButtonVariant.Primary, disabled = !canSave) { save() }
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
            if (!account.isDefault) {
                group(header = "Folder", footer = "Where its sign-in is kept, as ${account.folderVariable}") {
                    row {
                        val placeholder = if (account.agent == Agent.Claude) "~/.claude-personal" else "~/.codex-personal"
                        SettingsField(placeholder, account.folder, { account = account.copy(folder = it) }, monospaced = true)
                    }
                }
                if (account.agent == Agent.Codex) {
                    group(footer = "The folder keeps only the sign-in. Threads move between the two and go on where they were.") {
                        row {
                            SettingsToggle("Share sessions with the default account", account.sharesSessions) { account = account.copy(sharesSessions = it) }
                        }
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
            group(header = "Sign In", footer = "Run it in a terminal on ${server.name}. Who is signed in shows in the list once the agent says.") {
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
                    monospaced = true, keyboardOptions = plainKeyboard, onFocusChange = { KeyboardFocus.editable = it },
                )
            }
            Box(Modifier.fillMaxWidth().height(ControlSize.Regular.height)) {
                InputField(
                    if (kept) "••••••••" else "value", variable.value, { edited.variable = variable.copy(value = it) }, variant = InputVariant.Bare,
                    monospaced = true, secure = variable.sensitive, keyboardOptions = plainKeyboard,
                    onFocusChange = { KeyboardFocus.editable = it },
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
