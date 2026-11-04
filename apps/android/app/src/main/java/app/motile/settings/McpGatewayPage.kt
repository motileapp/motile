package app.motile.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.McpServerCallRow
import app.motile.core.McpServerCalls
import app.motile.core.McpServerRow
import app.motile.core.McpServerTool
import app.motile.core.McpToolGroup
import app.motile.core.ToolPermission
import app.motile.core.McpServers
import app.motile.core.GatewayView
import app.motile.core.mcpServerTint
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.IconSize
import app.motile.theme.LocalPalette
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.Theme
import app.motile.theme.opacity
import app.motile.theme.wash
import app.motile.ui.ButtonVariant
import app.motile.ui.ActionMenu
import app.motile.ui.Card
import app.motile.ui.Chip
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.MenuBuilder
import app.motile.ui.Modal
import app.motile.ui.Segmented
import app.motile.ui.Spinner
import app.motile.ui.Switch
import app.motile.ui.Text
import app.motile.ui.dotted

/** What the MCP Gateway page says. */
internal object McpServerText {
    const val updateGateway = "Update your gateway to use MCP servers"
    const val search = "Search, or paste an MCP address"
    const val runsForReal = "It changes things for real, as an agent's call would."
    const val none = "No MCP servers yet"
    const val noneDetail = "Add Linear, GitHub, Notion or any MCP by its address."
}

/** What a sheet asks of the page: an MCP server to try, rename or give a key. */
private data class McpServerSheet(val kind: Kind, val mcpServer: McpServerRow) {
    enum class Kind { TryTool, Rename, Key }
}

/** Rows from the end of the activity at which its next page is read. */
private const val PAGE_AHEAD = 5

/**
 * Settings → MCP Gateway: the account's gateway, the MCP servers it signs in to, a group each with its
 * tools, and the way to the calls agents made to them. The core follows the gateway while the page shows.
 */
@Composable
fun McpGatewayPage() {
    val store = LocalStore.current
    val gateway = store.gateway
    val view = gateway.view
    val navigator = LocalSettingsNavigator.current
    var adding by remember { mutableStateOf<String?>(null) }
    var addingGateway by remember { mutableStateOf(false) }
    var sheet by remember { mutableStateOf<McpServerSheet?>(null) }
    var removal by remember { mutableStateOf<Removal?>(null) }
    var unfolded by remember { mutableStateOf(emptySet<String>()) }
    DisposableEffect(gateway) {
        gateway.watch(true)
        onDispose { gateway.watch(false) }
    }
    LaunchedEffect(store.mcpStep) {
        val step = store.mcpStep?.takeIf { it != "call" } ?: return@LaunchedEffect
        store.mcpStep = null
        val action = step.substringBefore(' ')
        val argument = step.substringAfter(' ', "")
        val named = view.mcpServers?.firstOrNull { it.name == argument }
        when (action) {
            "add" -> adding = argument
            "activity" -> navigator.push(SettingsRoute.McpServerActivity)
            "unfold" -> named?.let { mcpServer -> unfolded = unfolded + mcpServer.groups.map { groupKey(mcpServer, it) } }
            "try" -> sheet = named?.let { McpServerSheet(McpServerSheet.Kind.TryTool, it) }
            "rename" -> sheet = named?.let { McpServerSheet(McpServerSheet.Kind.Rename, it) }
            "key" -> sheet = named?.let { McpServerSheet(McpServerSheet.Kind.Key, it) }
            "remove" -> removal = named?.let { Removal.OfMcpServer(it) }
        }
    }
    SettingsList(target = store.settingsTarget, reached = { store.settingsTarget = null }) {
        gatewayHeader(view, GatewayText.mcpCaption, add = {
            gateway.startAdding()
            addingGateway = true
        }) { removal = Removal.OfGateway(it) }
        if (!view.exists) return@SettingsList
        mcpServersHeader(view) { adding = "" }
        val mcpServers = view.mcpServers
        if (mcpServers.isNullOrEmpty()) return@SettingsList
        for (mcpServer in mcpServers) {
            mcpServerGroup(
                mcpServer, unfolded, gateway.mcpServers,
                fold = { key -> unfolded = if (key in unfolded) unfolded - key else unfolded + key },
                ask = { sheet = McpServerSheet(it, mcpServer) },
            ) { removal = Removal.OfMcpServer(mcpServer) }
        }
        group("mcp-activity") {
            row { SettingsLink({ navigator.push(SettingsRoute.McpServerActivity) }) { RowText("Activity", Modifier.weight(1f), maxLines = 1) } }
        }
    }
    Modal(adding, onDismiss = { adding = null }) { McpServerAddSheet(it) }
    Modal(addingGateway, onDismiss = { addingGateway = false }) { AddGatewaySheet() }
    Modal(sheet, onDismiss = { sheet = null }) { asked ->
        when (asked.kind) {
            McpServerSheet.Kind.TryTool -> McpServerTrySheet(asked.mcpServer)
            McpServerSheet.Kind.Rename -> McpServerRenameSheet(asked.mcpServer)
            McpServerSheet.Kind.Key -> McpServerKeySheet(asked.mcpServer)
        }
    }
    ConfirmsRemoval(removal) { removal = null }
}

/** The way to add an MCP server, or why there can't be any yet. */
private fun SettingsGroups.mcpServersHeader(view: GatewayView, add: () -> Unit) {
    group("mcp-servers", header = "MCP Servers") {
        when {
            view.mcpServers != null -> {
                if (view.mcpServers.isEmpty()) row { Note(McpServerText.none, McpServerText.noneDetail) }
                row { SettingsPressRow(add) { SettingsActionLabel("Add MCP Server", Symbol.Plus) } }
            }
            view.connection == GatewayView.Connection.Connected -> row { Note(McpServerText.updateGateway, view.version?.let { "Your gateway runs version $it" }) }
            else -> row { Note(view.statusText, view.error) }
        }
    }
}

@Composable
private fun Note(title: String, detail: String?) {
    SettingsRow {
        if (detail == null) {
            SettingsTile(Symbol.Plug)
            RowText(title, Modifier.weight(1f))
            return@SettingsRow
        }
        SettingsItem(title, detail, lines = Int.MAX_VALUE, picture = { SettingsTile(Symbol.Plug) })
    }
}

/** What folds a group of an MCP server's tools out. */
private fun groupKey(mcpServer: McpServerRow, group: McpToolGroup) = "${mcpServer.id}/${group.kind}"

/**
 * An MCP server's group: its name and how it stands, its switch, and its tools by what they do, each
 * kind with its permission and folded out when `unfolded` holds it.
 */
private fun SettingsGroups.mcpServerGroup(
    mcpServer: McpServerRow,
    unfolded: Set<String>,
    mcpServers: McpServers,
    fold: (String) -> Unit,
    ask: (McpServerSheet.Kind) -> Unit,
    remove: () -> Unit,
) {
    group("mcp-server/${mcpServer.id}") {
        row {
            SettingsRow {
                McpServerTile(mcpServer.monogram, mcpServer.tint, SettingsTile.size)
                Column(Modifier.weight(1f).padding(vertical = 6.dp).opacity(Opacity.Disabled, on = !mcpServer.enabled), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    val palette = LocalPalette.current
                    RowText(mcpServer.name, color = palette.foreground, maxLines = 1)
                    Text(mcpServer.statusText, size = NOTE_TEXT, color = if (mcpServer.troubled) palette.warning else palette.mutedForeground)
                }
                ActionMenu(icon = Symbol.Ellipsis, help = "What to do with ${mcpServer.name}") {
                    mcpServerActions(mcpServer, mcpServers, ask, remove)
                }
            }
        }
        row {
            SettingsRow {
                SettingsItem("Active", "Agents can use its tools", lines = Int.MAX_VALUE)
                Switch(mcpServer.enabled, { mcpServers.setEnabled(mcpServer, it) })
            }
        }
        if (mcpServer.state == McpServerRow.State.SignInAgain) row {
            SettingsPressRow({ mcpServers.signInAgain(mcpServer) }, enabled = mcpServers.signingIn == null) {
                SettingsActionLabel("Sign In Again", Symbol.CircleUser)
                Spacer(Modifier.weight(1f))
                if (mcpServers.signingIn == mcpServer.id) Spinner(IconSize.Xl)
            }
        }
        for (group in mcpServer.groups) {
            val key = groupKey(mcpServer, group)
            val open = key in unfolded
            row {
                SettingsPressRow({ fold(key) }, Modifier.opacity(Opacity.Disabled, on = !mcpServer.enabled)) {
                    McpToolGroupHeader(mcpServer, group, mcpServers, open)
                }
            }
            if (open) for (tool in group.tools) row {
                SettingsRow(Modifier.opacity(Opacity.Disabled, on = !mcpServer.enabled)) { McpServerToolRow(mcpServer, tool, mcpServers) }
            }
        }
    }
}

/** What can be done with an MCP server, in its menu. */
private fun MenuBuilder.mcpServerActions(mcpServer: McpServerRow, mcpServers: McpServers, ask: (McpServerSheet.Kind) -> Unit, remove: () -> Unit) {
    if (mcpServer.tools.isNotEmpty()) item("Try a Tool", Symbol.Play) { ask(McpServerSheet.Kind.TryTool) }
    item("Rename", Symbol.Pencil) { ask(McpServerSheet.Kind.Rename) }
    when (mcpServer.auth) {
        McpServerRow.Auth.SignIn -> item("Sign In Again", Symbol.CircleUser) { mcpServers.signInAgain(mcpServer) }
        McpServerRow.Auth.Key -> item("Change Key", Symbol.KeyRound) { ask(McpServerSheet.Kind.Key) }
        McpServerRow.Auth.None -> Unit
    }
    item("Ask for Its Tools Again", Symbol.RefreshCw) { mcpServers.refresh(mcpServer) }
    item("Copy Address", Symbol.Copy) { Platform.copy(mcpServer.url) }
    divider()
    item("Remove MCP Server", Symbol.Trash2, destructive = true, run = remove)
}

/** An MCP server's picture: its first letter on a wash of its colour, `side` points wide. */
@Composable
fun McpServerTile(monogram: String, tint: String, side: Float = scaled(28f)) {
    val color = LocalPalette.current.mcpServerTint(tint)
    val shape = RoundedCornerShape(if (side <= scaled(18f)) Radius.xs else Radius.sm)
    Box(Modifier.size(side.dp).background(color.wash(), shape), contentAlignment = Alignment.Center) {
        Text(monogram, size = side * 0.5f / Platform.scale, weight = FontWeight.SemiBold, color = color, maxLines = 1)
    }
}

/** A permission's menu: its symbol, its name unless `compact`, and a chevron; "Custom" while the tools under it differ. */
@Composable
fun ToolPermissionMenu(
    permission: ToolPermission?,
    help: String,
    compact: Boolean = false,
    choose: (ToolPermission) -> Unit,
) {
    ActionMenu(
        title = if (compact) null else permission?.title ?: "Custom",
        icon = permission?.symbol ?: Symbol.Ellipsis,
        help = help,
        variant = ButtonVariant.Outline,
        chevron = true,
    ) {
        for (each in ToolPermission.entries) item(each.title, each.symbol, checked = each == permission) { choose(each) }
    }
}

/** A kind of an MCP server's tools: its fold, its name and count, and the permission they share. */
@Composable
fun RowScope.McpToolGroupHeader(mcpServer: McpServerRow, group: McpToolGroup, mcpServers: McpServers, open: Boolean? = null) {
    val palette = LocalPalette.current
    open?.let { Icon(if (it) Symbol.ChevronDown else Symbol.ChevronRight, IconSize.Md, color = palette.mutedForeground) }
    Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        RowText(group.title, Modifier.weight(1f, fill = false), maxLines = 1)
        Chip("${group.tools.size}", monospaced = true)
    }
    ToolPermissionMenu(group.permission, "What agents may do with ${group.title.lowercase()}") { mcpServers.setGroup(mcpServer, group, it) }
}

/** An MCP server's tool: what it is, what it does, and what agents may do with it. */
@Composable
fun RowScope.McpServerToolRow(mcpServer: McpServerRow, tool: McpServerTool, mcpServers: McpServers) {
    val palette = LocalPalette.current
    Column(Modifier.weight(1f).opacity(Opacity.Disabled, on = tool.permission == ToolPermission.Block), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(tool.title, size = 13f, color = palette.foreground, maxLines = 1)
        tool.summary?.let { Text(it, size = 11.5f, color = palette.mutedForeground, maxLines = 1) }
    }
    ToolPermissionMenu(tool.permission, "What agents may do with ${tool.title}", compact = true) { mcpServers.setTool(mcpServer, tool, it) }
}

/** The calls agents made to the MCP servers, newest first, all of them or only those that failed. A call opens in full. */
@Composable
fun McpServerActivityPage() {
    val store = LocalStore.current
    val gateway = store.gateway
    val palette = LocalPalette.current
    val calls = remember { McpServerCalls(gateway.mcpServers) }
    var opened by remember { mutableStateOf<Long?>(null) }
    val state = rememberLazyListState()
    DisposableEffect(gateway) {
        gateway.watch(true)
        calls.start()
        onDispose { gateway.watch(false) }
    }
    val nearEnd by remember {
        derivedStateOf {
            val last = state.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: return@derivedStateOf false
            last >= state.layoutInfo.totalItemsCount - PAGE_AHEAD
        }
    }
    LaunchedEffect(nearEnd, calls.rows.size) { if (nearEnd) calls.loadMore() }
    LaunchedEffect(gateway.view.callsHead) { calls.refresh() }
    LaunchedEffect(store.mcpStep, calls.rows.firstOrNull()) {
        val first = calls.rows.firstOrNull()
        if (store.mcpStep != "call" || first == null) return@LaunchedEffect
        store.mcpStep = null
        opened = first.id
    }
    SettingsScreen("Activity") {
        LazyColumn(
            Modifier.fillMaxSize(),
            state = state,
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 40.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            item(key = "filter") {
                Segmented(listOf("All" to false, "Errors" to true), calls.errorsOnly, { calls.errorsOnly = it }, Modifier.fillMaxWidth(), fills = true)
            }
            calls.emptyText?.let { text ->
                item(key = "empty") { Text(text, Modifier.padding(horizontal = 16.dp, vertical = 8.dp), size = NOTE_TEXT, color = palette.mutedForeground) }
            }
            items(calls.rows, key = { it.id }) { row ->
                Card(Modifier.fillMaxWidth()) {
                    HighlightButton({ opened = row.id }, Modifier.fillMaxWidth(), radius = 0.dp) {
                        Box(Modifier.padding(horizontal = 16.dp, vertical = 10.dp)) { McpServerCallRowView(row) }
                    }
                }
            }
            if (calls.loading) item(key = "loading") {
                Box(Modifier.fillMaxWidth().padding(12.dp), contentAlignment = Alignment.Center) { Spinner(color = palette.mutedForeground) }
            }
        }
    }
    Modal(opened, onDismiss = { opened = null }) { McpServerCallSheet(it) }
}

/** A call an agent made: the MCP server and its tool, where it came from, how long it took, and what went wrong. */
@Composable
fun McpServerCallRowView(row: McpServerCallRow) {
    val palette = LocalPalette.current
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        McpServerTile(row.monogram, row.tint, scaled(18f))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                if (!row.ok) Icon(Symbol.CircleAlert, IconSize.Sm, color = palette.warning)
                Text(row.mcpServer, size = 13f, weight = FontWeight.Medium, maxLines = 1)
                Text(row.tool, Modifier.weight(1f, fill = false), style = Theme.uiMono(12f), maxLines = 1)
                Spacer(Modifier.weight(1f))
                Text(dotted("${row.latency} · ${row.time}", palette.mutedStrongerForeground), style = Theme.uiDigits(12f), color = palette.mutedForeground, maxLines = 1)
            }
            Text(row.source, size = 11.5f, color = palette.mutedForeground)
            row.error?.let { Text(it, size = 11.5f, color = palette.warning) }
        }
    }
}
