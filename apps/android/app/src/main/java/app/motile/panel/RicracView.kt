package app.motile.panel

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Loaded
import app.motile.core.RicracChoice
import app.motile.core.RicracPage
import app.motile.core.RicracRow
import app.motile.core.RicracState
import app.motile.core.RicracTeam
import app.motile.core.RicracUser
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.ui.ActionButton
import app.motile.ui.ActionMenu
import app.motile.ui.ButtonVariant
import app.motile.ui.Chip
import app.motile.ui.Choice
import app.motile.ui.Choices
import app.motile.ui.Icon
import app.motile.ui.InputField
import app.motile.ui.LocalCloseModal
import app.motile.ui.MenuBuilder
import app.motile.ui.Modal
import app.motile.ui.Text
import app.motile.ui.TextArea
import kotlinx.coroutines.delay

/** The issues of a Ricrac workspace the server is connected to, or how to connect one. */
@Composable
fun RicracSurface(target: PanelTarget) {
    val store = LocalStore.current
    val reason = store.ricracUnavailable
    if (reason != null) return PanelMessage(reason)
    LaunchedEffect(target.serverID) { store.ricrac.read(target.serverID) }
    if (store.ricrac.connected(target.serverID).isEmpty()) RicracConnect(target) else RicracIssues(target)
}

@Composable
private fun RicracConnect(target: PanelTarget) {
    val ricrac = LocalStore.current.ricrac
    val palette = LocalPalette.current
    Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
        Column(Modifier.widthIn(max = 320.dp), verticalArrangement = Arrangement.spacedBy(16.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            Icon(Symbol.Ricrac, 28f, color = palette.foreground)
            Column(verticalArrangement = Arrangement.spacedBy(6.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Text("Connect Ricrac", size = 15f, weight = FontWeight.SemiBold, color = palette.foreground)
                Text(
                    "See your issues and hand them to your agents. The connection is kept on your server.",
                    size = 12.5f, color = palette.mutedForeground, align = TextAlign.Center,
                )
            }
            ActionButton("Connect Ricrac", variant = ButtonVariant.Primary, pending = ricrac.connecting == target.serverID) { ricrac.connect(target.serverID) }
            ricrac.error?.let { error ->
                SelectionContainer { Text(error, size = 12.5f, color = palette.destructive, align = TextAlign.Center) }
            }
        }
    }
}

/** Under this the bar is two rows, and the labels beside the titles go. */
private val wraps = (520 * Platform.scale).dp

/** A workspace's issues under their statuses, with what chooses them over the list. */
@Composable
private fun RicracIssues(target: PanelTarget) {
    val store = LocalStore.current
    val ricrac = store.ricrac
    var asked by remember { mutableIntStateOf(0) }
    var filing by remember { mutableStateOf(false) }
    var offered by remember { mutableStateOf(emptyList<Choice>()) }
    val choice = ricrac.choice(target)
    val search = ricrac.searches[target.projectID] ?: ""
    BoxWithConstraints(Modifier.fillMaxSize()) {
        // A phone, or a panel too narrow for the search and the buttons beside the workspace and the team.
        val narrow = !Platform.isTablet || maxWidth < wraps
        Column(Modifier.fillMaxSize()) {
            if (narrow) {
                PanelBar(second = { Tools(target, choice, narrow, asked = { asked += 1 }) { filing = true } }) {
                    Pickers(target, choice)
                    Spacer(Modifier.weight(1f))
                }
            } else {
                PanelBar {
                    Pickers(target, choice)
                    Spacer(Modifier.weight(1f).widthIn(min = 4.dp))
                    Tools(target, choice, narrow, asked = { asked += 1 }) { filing = true }
                }
            }
            ricrac.error?.let { PanelNote(it) }
            LaunchedEffect(target, choice, search, asked) {
                // Ricrac is asked once the typing has stopped.
                if (search.isNotEmpty()) delay(300)
                ricrac.load(target, search)
            }
            Issues(target, choice, search, narrow) { offered = it }
        }
    }
    val workspace = choice.workspace
    Modal(filing && workspace != null, { filing = false }) {
        if (workspace != null) RicracNewIssue(target, workspace, choice.team)
    }
    Choices(offered) { offered = emptyList() }
}

@Composable
private fun Pickers(target: PanelTarget, choice: RicracChoice) {
    val ricrac = LocalStore.current.ricrac
    val connected = ricrac.connected(target.serverID)
    val workspace = connected.firstOrNull { it.id == choice.workspace }
    val teams = ricrac.teams[choice.workspace ?: ""] ?: emptyList()
    ActionMenu(workspace?.workspace ?: "Ricrac", modifier = Modifier.offset(x = (-8).dp), help = "Which workspace to show") {
        for (connection in connected) toggle(connection.workspace, connection.id == choice.workspace) { ricrac.pick(connection.id, target = target) }
        divider()
        item("Add Workspace") { ricrac.connect(target.serverID) }
        if (workspace != null) item("Disconnect ${workspace.workspace}", destructive = true) { ricrac.disconnect(workspace.id, target.serverID) }
    }
    ActionMenu(teams.firstOrNull { it.id == choice.team }?.name ?: "All Teams", modifier = Modifier.offset(x = (-8).dp), help = "Which team's issues to show") {
        toggle("All Teams", choice.team == null) { ricrac.pick(choice.workspace, target = target) }
        for (team in teams) toggle(team.name, choice.team == team.id) { ricrac.pick(choice.workspace, team.id, target) }
    }
}

@Composable
private fun androidx.compose.foundation.layout.RowScope.Tools(target: PanelTarget, choice: RicracChoice, narrow: Boolean, asked: () -> Unit, file: () -> Unit) {
    val ricrac = LocalStore.current.ricrac
    val search = ricrac.searches[target.projectID] ?: ""
    val setSearch = { text: String -> ricrac.searches = ricrac.searches + (target.projectID to text) }
    val focus = remember { FocusRequester() }
    DemoSearch(focus) { setSearch(search + it) }
    InputField(
        "Search", search, setSearch,
        (if (narrow) Modifier.weight(1f) else Modifier.weight(1f, fill = false).widthIn(max = 180.dp)),
        icon = Symbol.Search, clearable = true, focusRequester = focus,
    )
    ActionMenu(icon = Symbol.ListFilter, help = "Which issues to show") {
        toggle("Assigned to Me", choice.mine) { mine -> ricrac.changeFilter { it.copy(mine = mine) } }
        divider()
        for ((kind, name) in RicracState.kinds) {
            toggle(name, kind in choice.states) { listed ->
                ricrac.changeFilter { filter -> filter.copy(states = filter.states.filter { it != kind } + if (listed) listOf(kind) else emptyList()) }
            }
        }
    }
    ActionButton(icon = Symbol.RotateCw, help = "Read the issues again", pending = "issues" in ricrac.working, onClick = asked)
    ActionButton(icon = Symbol.Plus, help = "New Issue", onClick = file)
}

@Composable
private fun Issues(target: PanelTarget, choice: RicracChoice, search: String, narrow: Boolean, offer: (List<Choice>) -> Unit) {
    val store = LocalStore.current
    val ricrac = store.ricrac
    val workspace = choice.workspace ?: ""
    val teams = ricrac.teams[workspace] ?: emptyList()
    when (val issues = ricrac.issues) {
        Loaded.Loading -> PanelLoading()
        is Loaded.Failed -> PanelMessage(issues.message, failed = true)
        is Loaded.Ready -> {
            val groups = issues.value
            if (groups.isEmpty()) {
                val filtered = choice.mine || choice.states.isNotEmpty()
                return PanelMessage(if (search.isNotEmpty()) "None have “$search”." else if (filtered) "No issues match the filter." else "No issues.")
            }
            val entries = groups.flatMap { group ->
                val folded = group.state.id in ricrac.filter.folded
                val header = RicracListEntry.Header(group.state, group.rows.size, folded)
                if (folded) listOf(header) else listOf(header) + group.rows.map { row ->
                    RicracListEntry.Issue(row, group.state, teams.firstOrNull { it.id == row.team }?.states ?: emptyList())
                }
            }
            RicracList(
                entries, pending = ricrac.working, narrow = narrow, users = ricrac.users[workspace] ?: emptyList(), offer = offer,
                open = { row -> store.rightPanel.open(PanelTab.RicracIssue(workspace, row.id, row.identifier)) },
                fold = { ricrac.fold(it) },
                change = { row, change -> ricrac.change(row.id, change, workspace, target.serverID) },
            )
        }
    }
}

/** What the menus that change an issue offer. */
private object RicracChoices {
    fun states(menu: MenuBuilder, states: List<RicracState>, current: String?, pick: (RicracState) -> Unit) {
        for (state in states) menu.toggle(state.name, state.id == current) { pick(state) }
    }

    fun priorities(menu: MenuBuilder, current: Int, pick: (Int) -> Unit) {
        for ((value, name) in RicracRow.priorities) menu.toggle(name, value == current) { pick(value) }
    }

    /** `pick` is given nothing for nobody. */
    fun assignees(menu: MenuBuilder, users: List<RicracUser>, current: String?, pick: (String?) -> Unit) {
        menu.toggle("Unassigned", current == null) { pick(null) }
        menu.divider()
        for (user in users) menu.toggle(if (user.me) "${user.name} (you)" else user.name, user.id == current) { pick(user.id) }
    }
}

/** Whose the issue is, by initials in a disc that says the name. */
@Composable
internal fun RicracInitials(initials: String, name: String) {
    val palette = LocalPalette.current
    Box(
        Modifier.size(scaled(18f).dp).background(palette.backgroundAccent, CircleShape).semantics { contentDescription = name },
        contentAlignment = Alignment.Center,
    ) {
        Text(initials, size = 9f, weight = FontWeight.SemiBold, color = palette.mutedForeground, maxLines = 1)
    }
}

/** One issue: what it is and who has it, which menus change, what it asks for, and what was said of it, with a field to say more. */
@Composable
fun RicracIssueSurface(target: PanelTarget, workspace: String, id: String) {
    val store = LocalStore.current
    val ricrac = store.ricrac
    val palette = LocalPalette.current
    var asked by remember { mutableIntStateOf(0) }
    val page = ricrac.pages[id] ?: Loaded.Loading
    Column(Modifier.fillMaxSize()) {
        PanelBar {
            Text(page.value?.row?.identifier ?: "", size = 12.5f, weight = FontWeight.Medium, color = palette.mutedForeground)
            Spacer(Modifier.weight(1f).widthIn(min = 4.dp))
            page.value?.row?.url?.let { url -> ActionButton(icon = Symbol.SquareArrowOutUpRight, help = "Open in Ricrac") { Platform.open(url) } }
            ActionButton(icon = Symbol.RotateCw, help = "Read the issue again", pending = "read:$id" in ricrac.working) { asked += 1 }
        }
        ricrac.error?.let { PanelNote(it) }
        LaunchedEffect(target.serverID, asked) {
            ricrac.needTeams(workspace, target.serverID)
            ricrac.loadIssue(id, workspace, target.serverID)
        }
        when (page) {
            Loaded.Loading -> PanelLoading()
            is Loaded.Failed -> PanelMessage(page.message, failed = true)
            is Loaded.Ready -> IssuePage(page.value, target, workspace, id)
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun IssuePage(page: RicracPage, target: PanelTarget, workspace: String, id: String) {
    val store = LocalStore.current
    val ricrac = store.ricrac
    val palette = LocalPalette.current
    var comment by rememberSaveable(id) { mutableStateOf("") }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 16.dp, bottom = panelEndRoom)) {
        item("title") {
            SelectionContainer { Text(page.row.title, size = 17f, weight = FontWeight.SemiBold, color = palette.foreground) }
        }
        item("properties") {
            FlowRow(Modifier.padding(top = 16.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Properties(page, target, workspace, id)
            }
        }
        if (page.row.labels.isNotEmpty()) {
            item("labels") {
                FlowRow(Modifier.padding(top = 16.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    for (label in page.row.labels) Chip(label.name, dot = label.color)
                }
            }
        }
        item("work") {
            ActionButton(
                "Work on Issue", icon = Symbol.Play, modifier = Modifier.padding(top = 16.dp), help = "Start a thread on this issue",
                variant = ButtonVariant.Primary,
            ) { ricrac.work(page, workspace, target) }
        }
        item("description") {
            Box(Modifier.padding(top = 16.dp)) {
                if (page.description.isEmpty()) {
                    Text("No description.", size = 12.5f, color = palette.mutedStrongerForeground)
                } else {
                    PullRequestTextView(page.description)
                }
            }
        }
        item("line") { PanelLine(Modifier.padding(top = 16.dp)) }
        items(page.comments, key = { "comment:${it.id}" }) { comment ->
            Column(Modifier.padding(top = 16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                    RicracInitials(comment.initials, comment.author)
                    Text(byline(comment.author, null, comment.at), size = 12.5f)
                }
                PullRequestTextView(comment.body)
            }
        }
        item("writing") {
            val words = comment.trim()
            Column(Modifier.padding(top = if (page.comments.isEmpty()) 16.dp else 20.dp), verticalArrangement = Arrangement.spacedBy(8.dp), horizontalAlignment = Alignment.End) {
                TextArea("Leave a comment", comment, { comment = it }, Modifier.fillMaxWidth(), lines = 4)
                ActionButton("Comment", variant = ButtonVariant.Secondary, pending = "comment:$id" in ricrac.working, disabled = words.isEmpty()) {
                    ricrac.loadIssue(id, workspace, target.serverID, comment = words) { comment = "" }
                }
            }
        }
    }
}

@Composable
private fun Properties(page: RicracPage, target: PanelTarget, workspace: String, id: String) {
    val ricrac = LocalStore.current.ricrac
    val states = ricrac.teams[workspace]?.firstOrNull { it.id == page.row.team }?.states ?: emptyList()
    val change = { change: Map<String, Any?> -> ricrac.change(id, change, workspace, target.serverID) }
    val stateColor = page.state.color
    ActionMenu(
        page.state.name, picture = { Icon(page.state.symbol, ControlSize.Small.symbol, color = stateColor) }, help = "Status",
        variant = ButtonVariant.Secondary, size = ControlSize.Small, pending = id in ricrac.working,
    ) {
        RicracChoices.states(this, states, page.state.id) { change(mapOf("state" to it.id)) }
    }
    ActionMenu(
        RicracRow.priorities.firstOrNull { it.first == page.row.priority }?.second, icon = page.row.prioritySymbol, help = "Priority",
        variant = ButtonVariant.Secondary, size = ControlSize.Small,
    ) {
        RicracChoices.priorities(this, page.row.priority) { change(mapOf("priority" to it)) }
    }
    ActionMenu(page.row.assignee ?: "Unassigned", icon = Symbol.CircleUser, help = "Assignee", variant = ButtonVariant.Secondary, size = ControlSize.Small) {
        RicracChoices.assignees(this, ricrac.users[workspace] ?: emptyList(), page.row.assigneeID) { change(mapOf("assignee" to (it ?: ""))) }
    }
}

/** The sheet that files an issue. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun RicracNewIssue(target: PanelTarget, workspace: String, team: String?) {
    val store = LocalStore.current
    val ricrac = store.ricrac
    val close = LocalCloseModal.current
    val palette = LocalPalette.current
    var title by rememberSaveable { mutableStateOf("") }
    var description by rememberSaveable { mutableStateOf("") }
    var teamID by rememberSaveable { mutableStateOf<String?>(null) }
    var stateID by rememberSaveable { mutableStateOf<String?>(null) }
    var assigneeID by rememberSaveable { mutableStateOf<String?>(null) }
    var priority by rememberSaveable { mutableIntStateOf(0) }
    LaunchedEffect(Unit) { ricrac.needTeams(workspace, target.serverID) }
    val teams = ricrac.teams[workspace] ?: emptyList()
    val picked: RicracTeam? = teams.firstOrNull { it.id == (teamID ?: team) } ?: teams.firstOrNull()
    val states = picked?.states ?: emptyList()
    val state = states.firstOrNull { it.id == stateID } ?: states.firstOrNull { it.kind == "unstarted" } ?: states.firstOrNull()
    val users = ricrac.users[workspace] ?: emptyList()
    Column(Modifier.fillMaxSize().navigationBarsPadding().padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text("New Issue", size = 15f, weight = FontWeight.SemiBold)
        InputField("Title", title, { title = it }, Modifier.fillMaxWidth())
        TextArea("Add a description", description, { description = it }, Modifier.weight(1f).fillMaxWidth(), lines = 4, fills = true)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            ActionMenu(picked?.name ?: "Team", help = "Team", variant = ButtonVariant.Secondary, size = ControlSize.Small) {
                for (each in teams) {
                    toggle(each.name, each.id == picked?.id) {
                        teamID = each.id
                        stateID = null
                    }
                }
            }
            ActionMenu(state?.name ?: "Status", icon = state?.symbol, help = "Status", variant = ButtonVariant.Secondary, size = ControlSize.Small) {
                RicracChoices.states(this, states, state?.id) { stateID = it.id }
            }
            ActionMenu(
                RicracRow.priorities.firstOrNull { it.first == priority }?.second, icon = RicracRow.symbol(priority), help = "Priority",
                variant = ButtonVariant.Secondary, size = ControlSize.Small,
            ) {
                RicracChoices.priorities(this, priority) { priority = it }
            }
            ActionMenu(
                users.firstOrNull { it.id == assigneeID }?.name ?: "Unassigned", icon = Symbol.CircleUser, help = "Assignee",
                variant = ButtonVariant.Secondary, size = ControlSize.Small,
            ) {
                RicracChoices.assignees(this, users, assigneeID) { assigneeID = it }
            }
        }
        ricrac.error?.let { Text(it, size = 12.5f, color = palette.destructive) }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Spacer(Modifier.weight(1f))
            ActionButton("Cancel") { close() }
            ActionButton(
                "Create Issue", variant = ButtonVariant.Primary, pending = "create" in ricrac.working,
                disabled = picked == null || title.isBlank(),
            ) {
                val chosen = picked ?: return@ActionButton
                val issue = buildMap {
                    put("team", chosen.id)
                    put("title", title)
                    put("description", description)
                    put("priority", priority)
                    state?.let { put("state", it.id) }
                    assigneeID?.let { put("assignee", it) }
                }
                ricrac.create(issue, workspace, target.serverID) { issueID, identifier ->
                    close()
                    store.rightPanel.open(PanelTab.RicracIssue(workspace, issueID, identifier))
                }
            }
        }
    }
}
