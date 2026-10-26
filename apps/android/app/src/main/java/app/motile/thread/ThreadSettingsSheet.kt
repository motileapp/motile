package app.motile.thread

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalCloseSheet
import app.motile.LocalStore
import app.motile.composer.BranchPicker
import app.motile.core.Access
import app.motile.core.AppStore
import app.motile.core.Preferences
import app.motile.core.Project
import app.motile.platform.Symbol
import app.motile.theme.LocalForeground
import app.motile.theme.Radius
import app.motile.theme.SurfaceBox
import app.motile.theme.Theme
import app.motile.ui.AgentIcon
import app.motile.ui.DimButton
import app.motile.ui.GlassButton
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.Menu
import app.motile.ui.SheetCloseButton
import app.motile.ui.Text
import app.motile.ui.ThemeDivider
import app.motile.ui.pressable

private const val rowText = 15f
private const val noteText = 12f
private val rowPadding = 16.dp

/** A page of the sheet other than its first, which goes back to it. */
private enum class SettingsPage { Reasoning, Access }

/**
 * What the Mac has around its composer, as a sheet: the model, the reasoning effort, how much the
 * agent may do without asking, and where the thread works.
 */
@Composable
fun ThreadSettingsSheet() {
    val store = LocalStore.current
    val close = LocalCloseSheet.current
    var page by remember { mutableStateOf<SettingsPage?>(null) }
    LaunchedEffect(Unit) { if (Preferences.bool("tmpReasoning")) page = SettingsPage.Reasoning }
    val branchesOf = store.composerProject?.takeIf { store.showsBranches }
    BackHandler(enabled = page != null || branchesOf != null) {
        if (branchesOf != null) store.showsBranches = false else page = null
    }
    Column(Modifier.fillMaxSize().navigationBarsPadding()) {
        when {
            branchesOf != null -> {
                val base = if (store.draftUsesWorktree) store.draftBase else null
                PageBar(if (base == null) "Branch" else "Start From", back = { store.showsBranches = false })
                Box(Modifier.weight(1f)) { BranchPicker(branchesOf, base) }
            }
            page == SettingsPage.Reasoning -> {
                val model = store.composerModel
                PageBar("Reasoning", back = { page = null })
                if (model != null) {
                    ChoiceList(model.effortChoices, store.composerEffort ?: "", ::effortLabel, ::effortSymbol) {
                        store.setEffort(it)
                        page = null
                    }
                }
            }
            page == SettingsPage.Access -> {
                PageBar("Access", back = { page = null })
                ChoiceList(Access.entries, store.composerAccess, { it.label }, { it.symbol }) {
                    store.setAccess(it)
                    page = null
                }
            }
            else -> {
                PageBar("Thread Settings", close = close)
                Column(
                    Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = rowPadding).padding(bottom = 20.dp),
                    verticalArrangement = Arrangement.spacedBy(20.dp),
                ) {
                    Models()
                    Options { page = it }
                    store.composerProject?.let { Workspace(it) }
                }
            }
        }
    }
}

/** The bar over a page: its title, with the way back or the button that closes the sheet. */
@Composable
private fun PageBar(title: String, back: (() -> Unit)? = null, close: (() -> Unit)? = null) {
    Box(Modifier.fillMaxWidth().height(GlassButton.side + 20.dp).padding(horizontal = 12.dp), contentAlignment = Alignment.Center) {
        if (back != null) {
            GlassButton(Symbol.ChevronLeft, "Back", onClick = back, modifier = Modifier.align(Alignment.CenterStart), symbolSize = 16f)
        } else if (close != null) {
            SheetCloseButton(Modifier.align(Alignment.CenterStart), onClose = close)
        }
        Text(title, style = system(17f, FontWeight.SemiBold), maxLines = 1)
    }
}

/** Each account's models, under its name, which shows or hides them. Only the composer's account's are shown at first. */
@Composable
private fun Models() {
    val store = LocalStore.current
    val palette = Theme.colors
    val current = store.composerModel
    val account = store.composerAccount
    var listed by remember { mutableStateOf(setOfNotNull(account?.id)) }
    for (choices in store.composerChoices) {
        if (choices.models.isEmpty()) continue
        val id = choices.account.id
        val open = id in listed
        val turn by animateFloatAsState(if (open) 180f else 0f, label = "chevron")
        Column {
            DimButton(onClick = { listed = if (open) listed - id else listed + id }) {
                Row(
                    Modifier.fillMaxWidth().heightIn(min = pressable(32f)).padding(horizontal = 4.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    AgentIcon(choices.account.agent, size = 14f)
                    Text(choices.title, Modifier.weight(1f), size = noteText, color = palette.mutedForeground, maxLines = 1)
                    Icon(Symbol.ChevronDown, 10.5f, Modifier.rotate(turn), color = palette.mutedForeground)
                }
            }
            AnimatedVisibility(open) {
                Group(choices.models.map { model ->
                    {
                        ListRow({ store.setModel(model, choices.account) }) {
                            Text(model.name, Modifier.weight(1f), size = rowText)
                            if (model.id == current?.id && id == account?.id) Icon(Symbol.Check, 13f)
                        }
                    }
                })
            }
        }
    }
}

@Composable
private fun Options(open: (SettingsPage) -> Unit) {
    val store = LocalStore.current
    val model = store.composerModel
    val access = store.composerAccess
    Section("Options") {
        Group(buildList {
            if (model != null && model.efforts.isNotEmpty()) {
                val effort = store.composerEffort ?: ""
                add { Labeled("Reasoning", { open(SettingsPage.Reasoning) }) { SymbolValue(effortLabel(effort), effortSymbol(effort)) } }
            }
            add { Labeled("Access", { open(SettingsPage.Access) }) { SymbolValue(access.label, access.symbol) } }
        })
    }
}

/**
 * Where the thread works: the server, the folder or a worktree of its own, and the branch checked
 * out there. A thread that starts in a new worktree picks the branch it starts from.
 */
@Composable
private fun Workspace(project: Project) {
    val store = LocalStore.current
    val draft = store.selectedThread == null
    val branch = project.branch
    val footer = if (draft && branch != null && project.git?.hasCommit == false) "A worktree can start once $branch has a commit." else null
    Section("Workspace", footer) {
        Group(buildList {
            when {
                store.draftProjectCopies.isNotEmpty() -> add { ServerPicker(project) }
                else -> store.server(project.serverID)?.let { server -> add { Labeled("Server", server.name) } }
            }
            add { Labeled("Project", project.name) }
            when {
                project.worktree != null -> add { Labeled("Works in", "Worktree") }
                !draft && branch != null -> add { Labeled("Works in", "Local checkout") }
                draft && store.canUseWorktrees(project) -> add { WorksIn() }
                draft && branch != null && project.git?.hasCommit == false -> add { Labeled("Works in", "Current checkout") }
            }
            branchRow(store, project)?.let(::add)
        })
    }
}

/** The branch the thread works on, or the one its new worktree starts from, which can be picked where branches can be switched. */
private fun branchRow(store: AppStore, project: Project): (@Composable () -> Unit)? {
    val startsInWorktree = store.draftUsesWorktree
    val branch = (if (startsInWorktree) store.draftBase else project.branch) ?: return null
    val title = if (startsInWorktree) "Starts from" else "Branch"
    val value = if (startsInWorktree) store.draftStart ?: branch else branch
    if (project.worktree != null || !store.canSwitchBranches(project)) return { Labeled(title, value) }
    return { Labeled(title, { store.showBranches(project) }) { Text(value, size = rowText, color = Theme.colors.mutedForeground, maxLines = 1) } }
}

/** The server a new thread whose project is on more than one starts on. */
@Composable
private fun ServerPicker(project: Project) {
    val store = LocalStore.current
    val palette = Theme.colors
    ListRow {
        Text("Server", Modifier.weight(1f), size = rowText)
        Menu(
            items = {
                store.draftProjectCopies.forEach { copy ->
                    toggle(store.server(copy.serverID)?.name ?: "", isOn = copy.id == project.id) { store.setNewThreadProject(copy.id) }
                }
            },
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(store.server(project.serverID)?.name ?: "", size = rowText, color = palette.primary)
                Icon(Symbol.ChevronDown, 11f, color = palette.primary)
            }
        }
    }
}

/** Whether a new thread works in the project's checkout or in a new worktree of its own. */
@Composable
private fun WorksIn() {
    val store = LocalStore.current
    val palette = Theme.colors
    val worktree = store.draftUsesWorktree
    ListRow {
        Text("Works in", Modifier.weight(1f), size = rowText)
        Menu(
            items = {
                toggle("Current checkout", isOn = !worktree) { store.setDraftWorktree(false) }
                toggle("New worktree", isOn = worktree) { store.setDraftWorktree(true) }
            },
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(if (worktree) "New worktree" else "Current checkout", size = rowText, color = palette.primary)
                Icon(Symbol.ChevronDown, 11f, color = palette.primary)
            }
        }
    }
}

/** A setting's options on a page of their own, drawn like the sheet's list. Picking one goes back. */
@Composable
private fun <T> ChoiceList(options: List<T>, chosen: T, label: (T) -> String, symbol: (T) -> Symbol, choose: (T) -> Unit) {
    Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = rowPadding)) {
        Group(options.map { option ->
            {
                ListRow({ choose(option) }) {
                    Box(Modifier.weight(1f)) { SymbolValue(label(option), symbol(option)) }
                    if (option == chosen) Icon(Symbol.Check, 13f)
                }
            }
        })
    }
}

/** A group of rows under a header, with a note under it. */
@Composable
private fun Section(header: String, footer: String? = null, content: @Composable ColumnScope.() -> Unit) {
    val palette = Theme.colors
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(header, Modifier.padding(horizontal = 4.dp), size = noteText, color = palette.mutedForeground)
        content()
        footer?.let { Text(it, Modifier.padding(horizontal = 4.dp), size = noteText, color = palette.mutedForeground) }
    }
}

/** Rows in one box, with lines between them. */
@Composable
private fun Group(rows: List<@Composable () -> Unit>) {
    if (rows.isEmpty()) return
    SurfaceBox(RoundedCornerShape(Radius.lg), Modifier.fillMaxWidth()) {
        Column {
            rows.forEachIndexed { index, row ->
                if (index > 0) ThemeDivider(Modifier.padding(start = rowPadding))
                row()
            }
        }
    }
}

/** A row of a group, which does something when tapped if it has `onClick`. */
@Composable
private fun ListRow(onClick: (() -> Unit)? = null, content: @Composable RowScope.() -> Unit) {
    val row = @Composable {
        Row(
            Modifier.fillMaxWidth().heightIn(min = pressable(38f)).padding(horizontal = rowPadding, vertical = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
            content = content,
        )
    }
    if (onClick == null) {
        row()
        return
    }
    HighlightButton(onClick, Modifier.fillMaxWidth(), radius = 0.dp) { row() }
}

/** A setting's name and its value. */
@Composable
private fun Labeled(title: String, value: String) {
    ListRow {
        Text(title, size = rowText)
        Spacer(Modifier.weight(1f))
        Text(value, size = rowText, color = Theme.colors.mutedForeground, maxLines = 1)
    }
}

/** A setting's name and its value, which opens what changes it. */
@Composable
private fun Labeled(title: String, onClick: () -> Unit, value: @Composable () -> Unit) {
    val palette = Theme.colors
    ListRow(onClick) {
        Text(title, size = rowText)
        Spacer(Modifier.weight(1f))
        CompositionLocalProvider(LocalForeground provides palette.mutedForeground) { value() }
        Icon(Symbol.ChevronRight, 11f, color = palette.mutedStrongerForeground)
    }
}

/** Words with their symbol before them, as a row's value or a choice. */
@Composable
private fun SymbolValue(title: String, symbol: Symbol) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        Icon(symbol, 13f)
        Text(title, size = rowText, maxLines = 1)
    }
}

/** How an effort reads: "Extra" for xhigh, "Default" for the agent's own, else the effort capitalised. */
fun effortLabel(effort: String): String = when (effort) {
    "xhigh" -> "Extra"
    "" -> "Default"
    else -> effort.replaceFirstChar { it.uppercase() }
}

fun effortSymbol(effort: String): Symbol = when (effort) {
    "none", "minimal", "low" -> Symbol.EffortLow
    "medium" -> Symbol.EffortMedium
    "high" -> Symbol.EffortHigh
    "xhigh" -> Symbol.Zap
    "max" -> Symbol.Rocket
    "ultracode" -> Symbol.BotMessageSquare
    else -> Symbol.Gauge
}
