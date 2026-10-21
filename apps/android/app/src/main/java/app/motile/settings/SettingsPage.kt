package app.motile.settings

import app.motile.sidebar.AppUpdateRow
import app.motile.core.AppUpdater
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.motile.LocalCloseSheet
import app.motile.LocalStore
import app.motile.core.Agent
import app.motile.core.AgentAccount
import app.motile.core.AppStore
import app.motile.core.Appearance
import app.motile.core.Integration
import app.motile.core.Preferences
import app.motile.core.Server
import app.motile.core.SettingsSection
import app.motile.core.managesNewThreadsDescription
import app.motile.core.pullDefaultBranchCaption
import app.motile.onboarding.ConnectServerView
import app.motile.platform.Platform
import app.motile.platform.scaled
import app.motile.sidebar.ServerUpdateStatus
import app.motile.theme.ControlSize
import app.motile.platform.Symbol
import app.motile.theme.LocalPalette
import app.motile.theme.Radius
import app.motile.theme.SurfaceBox
import app.motile.ui.ActionButton
import app.motile.ui.AgentIcon
import app.motile.ui.ButtonVariant
import app.motile.ui.Icon
import app.motile.ui.Modal
import app.motile.ui.MenuPopup
import app.motile.ui.ProjectIcon
import app.motile.ui.Switch
import app.motile.ui.Text
import app.motile.ui.pressPosition
import app.motile.ui.rememberMenuState

/** One section of the settings as a list of the system's groups, scrolled to the group a search picked. */
@Composable
fun SettingsPage(section: SettingsSection) {
    val store = LocalStore.current
    val navigator = LocalSettingsNavigator.current
    var edited by remember { mutableStateOf<EditedAccount?>(null) }
    var removal by remember { mutableStateOf<Removal?>(null) }
    var steers by remember { mutableStateOf(Preferences.optionalBool(AppStore.STEERS_KEY) ?: AppStore.STEERS_BY_DEFAULT) }
    var managesNewThreads by remember { mutableStateOf(Preferences.managesNewThreads) }
    LaunchedEffect(Unit) { store.refreshMediaStorage() }
    SettingsList(target = store.settingsTarget, reached = { store.settingsTarget = null }) {
        when (section) {
            SettingsSection.General -> general(store, steers, managesNewThreads, setSteers = {
                steers = it
                Preferences.set(AppStore.STEERS_KEY, it)
            }) {
                managesNewThreads = it
                Preferences.managesNewThreads = it
            }
            SettingsSection.Servers -> servers(store, navigator)
            SettingsSection.Accounts -> agentAccounts(store, edit = { server, account -> edited = EditedAccount(server, account) }) { removal = it }
            SettingsSection.Projects -> projects(store, navigator)
            SettingsSection.TextGeneration -> textGeneration(store, navigator)
            SettingsSection.PullRequests -> pullRequests(store)
            SettingsSection.Integrations -> integrations(store)
            SettingsSection.Keyboard -> Unit
        }
    }
    ConfirmsRemoval(removal) { removal = null }
    Modal(edited, onDismiss = { edited = null }) { AgentAccountSheet(it.server, it.account) }
}

private fun SettingsGroups.general(
    store: AppStore,
    steers: Boolean,
    managesNewThreads: Boolean,
    setSteers: (Boolean) -> Unit,
    setManagesNewThreads: (Boolean) -> Unit,
) {
    group("account", header = "Account") {
        row {
            SettingsLabeled("Signed in as", if (store.account.signedIn) store.account.email else "Not signed in", TextOverflow.MiddleEllipsis)
        }
        if (store.account.signedIn) row { SettingsButton("Sign Out", destructive = true) { store.signOut() } }
    }
    group("appearance", header = "Appearance") {
        row { SettingsPicker("Theme", Appearance.entries.map { it.label to it }, Appearance.chosen) { Appearance.choose(it) } }
    }
    group("messages", header = "Messages", footer = steersDescription(steers)) {
        row { SettingsPicker("Sent while the agent works", listOf("Queue" to false, "Steer" to true), steers, setSteers) }
    }
    group("manage-new-threads", header = "Provider Limits") {
        row {
            SettingsRow {
                SettingsItem("Manage new threads automatically", managesNewThreadsDescription, lines = Int.MAX_VALUE)
                Switch(managesNewThreads, setManagesNewThreads)
            }
        }
    }
    group("storage", header = "Storage", footer = "Kept on this ${Platform.device} so threads open with them. Your servers keep them all.") {
        row { SettingsLabeled("Images and videos", store.mediaStorageUsed) }
        row {
            SettingsButton("Clear Images and Videos", destructive = true, enabled = (store.mediaStorage?.used ?: 0) > 0) { store.clearMedia() }
        }
    }
    group("updates", header = "Updates") {
        row { SettingsLabeled("Version", store.updater.current) }
        row { SettingsButton("Check for Updates", enabled = store.updater.state != AppUpdater.State.Checking) { store.updater.check(asked = true) } }
        if (store.updater.state != AppUpdater.State.Idle) row {
            SettingsRow { AppUpdateRow(store.updater, Modifier.weight(1f), variant = ButtonVariant.Outline) }
        }
    }
}

private fun SettingsGroups.servers(store: AppStore, navigator: SettingsNavigator) {
    group("servers") {
        for (server in store.servers) row {
            SettingsLink({ navigator.push(SettingsRoute.ServerPage(server.id)) }) {
                val description = if (server.state == Server.State.Connected) "Connected · version ${server.version}" else server.stateLabel
                val sync = server.sync
                SettingsItem(server.name, description, footer = sync?.let { { ServerSyncLine(it) } }) { SettingsTile(Symbol.Server) }
            }
        }
        row { SettingsLink({ navigator.push(SettingsRoute.AddServer) }) { SettingsActionLabel("Add a Server", Symbol.Plus) } }
    }
    group("removed-servers", header = "Unlinked") {
        for (server in store.removedServers) row {
            SettingsLink({ navigator.push(SettingsRoute.ServerPage(server.id, removed = true)) }) {
                SettingsItem(server.name, "Its threads stay readable from the copies") { SettingsTile(Symbol.Server) }
            }
        }
    }
    val continuing = store.servers.filter { it.state == Server.State.Connected && store.canChooseRestart(it) }
    if (continuing.isEmpty()) return
    toggles(
        "continue-limits", "Continue after usage limits", continuing,
        caption = "A thread whose agent reached its usage limit goes on once the limit resets.",
        isOn = { it.continueAfterLimits },
    ) { server, on -> store.setContinueSettings(afterLimits = on, afterRestarts = server.continueAfterRestarts, server = server) }
    toggles(
        "continue-restarts", "Continue after restarts", continuing,
        caption = "When your server comes back from a restart, agents that were working carry on.",
        isOn = { it.continueAfterRestarts },
    ) { server, on -> store.setContinueSettings(afterLimits = server.continueAfterLimits, afterRestarts = on, server = server) }
}

private fun SettingsGroups.agentAccounts(store: AppStore, edit: (Server, AgentAccount) -> Unit, remove: (Removal) -> Unit) {
    if (store.servers.isEmpty()) return note("The accounts your agents work with are set on each of your servers, once you have one.")
    store.servers.forEachIndexed { index, server ->
        group(
            id = if (index == 0) "agent-accounts" else "agent-accounts/${server.id}",
            headerContent = {
                Row(horizontalArrangement = Arrangement.spacedBy(5.dp), verticalAlignment = Alignment.CenterVertically) {
                    Icon(Symbol.Server, 12f, color = LocalPalette.current.mutedForeground)
                    Text(server.name, size = NOTE_TEXT, weight = FontWeight.Medium, color = LocalPalette.current.mutedForeground, maxLines = 1)
                }
            },
            footer = "Each account keeps its sign-in in a folder of its own. A thread works with one and can move to another.",
        ) {
            val reason = server.accountsUnavailable
            if (reason != null) {
                row { SettingsRow { RowText(reason, color = LocalPalette.current.mutedForeground) } }
                return@group
            }
            val installed = Agent.entries.filter { server.agents[it] != null }
            for (account in server.agentAccounts.filter { it.agent in installed }) row {
                AccountRow(account, edit = { edit(server, account) }, remove = if (account.isDefault) null else ({ remove(Removal.OfAccount(account, server)) }))
            }
            row {
                SettingsPressRow({ edit(server, AgentAccount.new(installed.firstOrNull() ?: Agent.Claude)) }, enabled = installed.isNotEmpty()) {
                    SettingsActionLabel("Add Account", Symbol.Plus)
                }
            }
        }
    }
}

/** An account: pressed it opens to change, held it offers the same and, but for the default one, its removal. */
@Composable
private fun AccountRow(account: AgentAccount, edit: () -> Unit, remove: (() -> Unit)?) {
    val menu = rememberMenuState()
    Box(Modifier.pressPosition(menu)) {
        SettingsPressRow(edit, onLongClick = { menu.openAtPress() }) {
            SettingsItem("${account.agent.title} · ${account.name}", account.settingsDescription) {
                Box(Modifier.size(SettingsTile.size.dp), contentAlignment = Alignment.Center) { AgentIcon(account.agent, size = 22f) }
            }
        }
        MenuPopup(menu) {
            item("Edit", Symbol.Pencil, run = edit)
            if (remove != null) item("Remove Account", Symbol.Trash2, destructive = true, run = remove)
        }
    }
}

private fun SettingsGroups.projects(store: AppStore, navigator: SettingsNavigator) {
    group("projects") {
        for (project in store.projects) row {
            SettingsLink({ navigator.push(SettingsRoute.ProjectPage(project.id)) }) {
                SettingsItem(project.name, project.path, TextOverflow.StartEllipsis) { ProjectIcon(project, size = SettingsTile.size) }
            }
        }
        row {
            SettingsLink({ navigator.push(SettingsRoute.AddProject) }, enabled = store.servers.isNotEmpty()) { SettingsActionLabel("Add a Project", Symbol.Plus) }
        }
    }
}

private fun SettingsGroups.textGeneration(store: AppStore, navigator: SettingsNavigator) {
    val servers = store.servers.filter { it.state == Server.State.Connected && it.protocolVersion >= 4 }
    if (servers.isEmpty()) {
        return note("The model that writes thread titles, branch names, commit messages and pull requests is set on each of your servers, once one is connected.")
    }
    group(
        "text-model", header = if (servers.size > 1) "Model" else null,
        footer = "The model that writes thread titles, branch names, commit messages and pull requests.",
    ) {
        for (server in servers) row {
            val options = listOf<Pair<String, String?>>("Automatic" to null) + server.distinctModels.map { it.name to it.id }
            SettingsPicker(if (servers.size > 1) server.name else "Model", options, server.textModel) { store.setTextModel(it, server) }
        }
    }
    val naming = servers.filter { it.protocolVersion >= 6 }
    if (naming.isEmpty()) return
    group(
        "branch-names", header = if (naming.size > 1) "Branch names" else null,
        footer = "How the writer is told to name the branches it makes.",
    ) {
        for (server in naming) row {
            SettingsLink({ navigator.push(SettingsRoute.BranchNames(server.id)) }) {
                RowText(if (naming.size > 1) server.name else "Branch Names", Modifier.weight(1f), maxLines = 1)
                val custom = server.branchInstructions != server.defaultBranchInstructions
                RowText(if (custom) "Custom" else "Default", color = LocalPalette.current.mutedForeground)
            }
        }
    }
}

private fun SettingsGroups.pullRequests(store: AppStore) {
    val servers = store.servers.filter { it.state == Server.State.Connected && it.protocolVersion >= 9 }
    if (servers.isEmpty()) return note("What your servers do once a thread's pull request merges is set on each of them, once one is connected.")
    val merged = "Mark the thread done after merge or close"
    val worktrees = "Remove the thread's worktree after merge"
    val pull = "Automatically pull"
    val only = servers.singleOrNull()
    if (only != null) {
        group("merged", also = listOf("worktrees")) {
            row { SettingsToggle(merged, only.doneOnMerge) { store.setPullRequestSettings(only, doneOnMerge = it) } }
            row { SettingsToggle(worktrees, only.removeMergedWorktrees) { store.setPullRequestSettings(only, removeMergedWorktrees = it) } }
        }
        if (only.pullsDefaultBranch) {
            group("pull", footer = pullDefaultBranchCaption) {
                row { SettingsToggle(pull, only.pullDefaultBranch) { store.setPullRequestSettings(only, pullDefaultBranch = it) } }
            }
        }
        return
    }
    toggles("merged", merged, servers, isOn = { it.doneOnMerge }) { server, on -> store.setPullRequestSettings(server, doneOnMerge = on) }
    toggles("worktrees", worktrees, servers, isOn = { it.removeMergedWorktrees }) { server, on -> store.setPullRequestSettings(server, removeMergedWorktrees = on) }
    val pulling = servers.filter { it.pullsDefaultBranch }
    if (pulling.isEmpty()) return
    toggles("pull", pull, pulling, caption = pullDefaultBranchCaption, isOn = { it.pullDefaultBranch }) { server, on ->
        store.setPullRequestSettings(server, pullDefaultBranch = on)
    }
}

private fun SettingsGroups.integrations(store: AppStore) {
    group("integration-linear", also = listOf("integration-ricrac"), header = "Issue trackers", footer = "Each one that is on has a tab in the right panel.") {
        for (integration in Integration.entries) row {
            SettingsRow {
                SettingsItem(integration.title, integration.summary, lines = Int.MAX_VALUE) { IntegrationLogo(integration) }
                Switch(store.isOn(integration), { store.setIntegration(integration, on = it) })
            }
        }
    }
}

/** An integration's logo on a tile of its own, as an app's icon sits. */
@Composable
private fun IntegrationLogo(integration: Integration) {
    SurfaceBox(RoundedCornerShape(Radius.md), Modifier.size(scaled(32f).dp), bordered = true) {
        Icon(integration.symbol, 18f, Modifier.align(Alignment.Center), color = LocalPalette.current.foreground)
    }
}

/** A setting each server has: a switch alone when there is one server, a switch for each under the setting's name when there are more. */
private fun SettingsGroups.toggles(
    id: String,
    title: String,
    servers: List<Server>,
    caption: String? = null,
    isOn: (Server) -> Boolean,
    set: (Server, Boolean) -> Unit,
) {
    group(id, header = if (servers.size > 1) title else null, footer = caption) {
        val only = servers.singleOrNull()
        if (only != null) {
            row { SettingsToggle(title, isOn(only)) { set(only, it) } }
            return@group
        }
        for (server in servers) row { SettingsToggle(server.name, isOn(server)) { set(server, it) } }
    }
}

/** A server's page: how it is, its agents, its update, and the ways to remove it. A `removed` one's only deletes its copies. */
@Composable
internal fun ServerSettingsPage(serverID: String, removed: Boolean) {
    val store = LocalStore.current
    val navigator = LocalSettingsNavigator.current
    var removal by remember { mutableStateOf<Removal?>(null) }
    val server = store.server(serverID)
    val left = server == null || server.removed != removed
    LaunchedEffect(left) { if (left) navigator.pop() }
    SettingsScreen(server?.name ?: "Server") {
        SettingsList {
            if (server == null) return@SettingsList
            if (removed) {
                group { row { SettingsButton("Delete", destructive = true) { removal = Removal.OfServerCompletely(server) } } }
                return@SettingsList
            }
            group {
                row { SettingsLabeled("Status", server.stateLabel) }
                if (server.state == Server.State.Connected) {
                    row { SettingsLabeled("Version", server.version) }
                    for (agent in Agent.entries.filter { server.agents[it] != null }) row { SettingsLabeled(agent.title, server.agents[agent] ?: "") }
                }
            }
            if (store.isOutdated(server) || store.serverUpdate(server) != null) {
                group {
                    row {
                        SettingsLabeled(if (store.serverUpdate(server) == null) "Update available" else "Updating") {
                            ServerUpdateStatus(server, ButtonVariant.Secondary, ControlSize.Regular) {}
                        }
                    }
                }
            }
            group {
                row { SettingsButton("Unlink Server", destructive = true) { removal = Removal.OfServer(server) } }
                row { SettingsButton("Delete", destructive = true) { removal = Removal.OfServerCompletely(server) } }
            }
        }
    }
    ConfirmsRemoval(removal) { removal = null }
}

/** The command that links a new server, until that server shows up. */
@Composable
internal fun AddServerPage() {
    val store = LocalStore.current
    val navigator = LocalSettingsNavigator.current
    val count = store.servers.size
    val before = remember { count }
    LaunchedEffect(count) { if (count > before) navigator.pop() }
    SettingsScreen("Add a Server") {
        CompositionLocalProvider(LocalCloseSheet provides navigator.pop) { ConnectServerView(isFirst = false, titled = false) }
    }
}

/** A project's page: where it is, its icon, the script its new worktrees run, and the way to remove it. */
@Composable
internal fun ProjectSettingsPage(projectID: String) {
    val store = LocalStore.current
    val navigator = LocalSettingsNavigator.current
    var removal by remember { mutableStateOf<Removal?>(null) }
    val project = store.projects.firstOrNull { it.id == projectID }
    LaunchedEffect(project == null) { if (project == null) navigator.pop() }
    SettingsScreen(project?.name ?: "Project") {
        SettingsList {
            if (project == null) return@SettingsList
            group(bare = true) {
                row {
                    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        ProjectIcon(project, Modifier.padding(bottom = 8.dp), size = 56f)
                        Text(project.name, size = 17f, weight = FontWeight.SemiBold, color = LocalPalette.current.foreground, align = TextAlign.Center)
                        Text(
                            project.path, size = NOTE_TEXT, color = LocalPalette.current.mutedForeground, maxLines = 2,
                            overflow = TextOverflow.StartEllipsis, align = TextAlign.Center,
                        )
                    }
                }
            }
            val server = store.server(project.serverID)
            group { if (server != null) row { SettingsLabeled("Server", server.name) } }
            group {
                row { SettingsLink({ navigator.push(SettingsRoute.ProjectIcon(project.id)) }) { SettingsActionLabel("Choose an Icon", Symbol.Image) } }
                if ((server?.protocolVersion ?: 0) >= 6) row {
                    SettingsLink({ navigator.push(SettingsRoute.WorktreeSetup(project.id)) }) {
                        RowText("Worktree Setup", Modifier.weight(1f), maxLines = 1)
                        RowText(if (project.setup == null) "None" else "Script", color = LocalPalette.current.mutedForeground)
                    }
                }
            }
            group { row { SettingsButton("Remove Project", destructive = true) { removal = Removal.OfProject(project) } } }
        }
    }
    ConfirmsRemoval(removal) { removal = null }
}

/** The shell script that runs in each new worktree of a project before the agent starts there. */
@Composable
internal fun WorktreeSetupPage(projectID: String) {
    val store = LocalStore.current
    val navigator = LocalSettingsNavigator.current
    val project = store.projects.firstOrNull { it.id == projectID }
    var script by remember { mutableStateOf(project?.setup ?: "") }
    val save: @Composable () -> Unit = {
        ActionButton("Save", variant = ButtonVariant.Primary, disabled = project == null || script == (project.setup ?: "")) {
            project?.let { store.setSetup(it, script) }
            navigator.pop()
        }
    }
    SettingsScreen("Worktree Setup", trailing = save) {
        SettingsList {
            group(
                footer = "Runs in each new worktree before the agent starts there, to install what the work needs. \$MOTILE_PROJECT is the project's folder.",
            ) {
                row { SettingsTextEditor("cp \"\$MOTILE_PROJECT/.env\" . && pnpm install", script, { script = it }, monospaced = true) }
            }
        }
    }
}

/** How a server's writer is told to name the branches it makes, to change and to put back. */
@Composable
internal fun BranchNamesPage(serverID: String) {
    val store = LocalStore.current
    val navigator = LocalSettingsNavigator.current
    val server = store.server(serverID)
    var text by remember { mutableStateOf(server?.branchInstructions ?: "") }
    val changed = server != null && text.trim() != server.branchInstructions
    val save: @Composable () -> Unit = {
        ActionButton("Save", variant = ButtonVariant.Primary, disabled = !changed) {
            server?.let { store.setBranchInstructions(text, it) }
            navigator.pop()
        }
    }
    SettingsScreen("Branch Names", trailing = save) {
        SettingsList {
            if (server == null) return@SettingsList
            group(footer = "How the writer on ${server.name} is told to name the branches it makes.") {
                row { SettingsTextEditor("How to name a branch", text, { text = it }) }
            }
            group {
                row {
                    val unchanged = server.branchInstructions == server.defaultBranchInstructions && text == server.branchInstructions
                    SettingsButton("Reset to Default", enabled = !unchanged) {
                        text = server.defaultBranchInstructions
                        store.setBranchInstructions(null, server)
                    }
                }
            }
        }
    }
}
