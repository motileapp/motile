package app.motile.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.McpServerAdder
import app.motile.core.McpServerAnswer
import app.motile.core.McpServerCallView
import app.motile.core.McpServerField
import app.motile.core.McpServerRow
import app.motile.core.KnownMcpServer
import app.motile.core.Loaded
import app.motile.core.coreError
import app.motile.layout.KeyboardFocus
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.ControlSize
import app.motile.theme.IconSize
import app.motile.theme.LocalPalette
import app.motile.theme.Radius
import app.motile.theme.Theme
import app.motile.theme.iconSize
import app.motile.ui.ActionButton
import app.motile.ui.ActionMenu
import app.motile.ui.ButtonVariant
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.InputField
import app.motile.ui.LocalCloseModal
import app.motile.ui.Segmented
import app.motile.ui.Spinner
import app.motile.ui.Switch
import app.motile.ui.Text
import app.motile.ui.TextArea
import app.motile.ui.beside
import app.motile.ui.dotted

/** A sheet's bar over what it holds, which scrolls above the keyboard. */
@Composable
private fun McpServerSheetFrame(title: String, content: @Composable ColumnScope.() -> Unit) {
    DisposableEffect(Unit) { onDispose { KeyboardFocus.editable = false } }
    Column(Modifier.fillMaxSize()) {
        SheetTitle(title)
        Column(
            Modifier.fillMaxWidth().weight(1f).imePadding().verticalScroll(rememberScrollState()).navigationBarsPadding().padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
            content = content,
        )
    }
}

/** A small heading over a part of a sheet. */
@Composable
private fun SheetLabel(text: String) {
    Text(text, size = 12f, weight = FontWeight.Medium, color = LocalPalette.current.mutedForeground)
}

@Composable
private fun Failure(text: String) {
    Text(text, size = 12f, color = LocalPalette.current.warning)
}

/** How agents name an MCP server's tools once it is called `slug`. */
@Composable
private fun SlugExample(mcpServer: McpServerRow, slug: String) {
    SelectionContainer {
        Text(mcpServer.example(slug.trim()), style = Theme.uiMono(11.5f), color = LocalPalette.current.mutedStrongerForeground)
    }
}

/** Text that can be selected and scrolled, in the code font: a call's arguments or a tool's answer. */
@Composable
private fun McpServerTextBox(text: String, height: Dp) {
    val palette = LocalPalette.current
    val shape = RoundedCornerShape(Radius.md)
    Box(
        Modifier
            .fillMaxWidth()
            .heightIn(min = scaled(60f).dp, max = height)
            .background(palette.background, shape)
            .border(1.dp, palette.border, shape)
            .verticalScroll(rememberScrollState())
            .horizontalScroll(rememberScrollState()),
    ) {
        SelectionContainer {
            Text(text, Modifier.padding(10.dp), style = Theme.uiMono(12f), color = palette.foreground)
        }
    }
}

/**
 * Adds an MCP server: picks a known one or takes an address, then signs it in, takes its key, and
 * shows what agents will call it.
 */
@Composable
fun McpServerAddSheet(query: String = "") {
    val gateway = LocalStore.current.gateway
    val mcpServers = gateway.mcpServers
    val close = LocalCloseModal.current
    val adder = remember { McpServerAdder(mcpServers) }
    var search by remember { mutableStateOf(query) }
    var header by remember { mutableStateOf("") }
    var key by remember { mutableStateOf("") }
    var slug by remember { mutableStateOf("") }
    val step = adder.step
    val typed = search.trim()
    val address = typed.takeIf { (it.startsWith("https://") || it.startsWith("http://")) && it.length > "https://".length }
    val title = when (step) {
        McpServerAdder.Step.Pick -> "Add an MCP Server"
        is McpServerAdder.Step.SigningIn -> "Sign In to ${step.name}"
        is McpServerAdder.Step.Key -> "${step.name} Takes a Key"
        is McpServerAdder.Step.Added -> "Connected to ${mcpServers.mcpServer(step.id)?.name ?: "It"}"
    }
    fun addWithKey(step: McpServerAdder.Step.Key) {
        if (key.isBlank()) return
        adder.add(step.url, step.given, header = header, key = key)
    }
    fun done() {
        val added = step as? McpServerAdder.Step.Added
        added?.let { mcpServers.mcpServer(it.id) }?.let { mcpServers.rename(it, it.name, slug) }
        close()
    }
    McpServerSheetFrame(title) {
        when (step) {
            McpServerAdder.Step.Pick -> {
                val known = if (typed.isEmpty()) gateway.view.known else gateway.view.known.filter { it.matches(typed) }
                Pick(search, { search = it }, address, known, adder)
            }
            is McpServerAdder.Step.SigningIn -> SigningIn()
            is McpServerAdder.Step.Key -> {
                KeyFields(
                    "Paste a key from ${step.name}'s settings.",
                    key, { key = it }, header, { header = it },
                ) { addWithKey(step) }
            }
            is McpServerAdder.Step.Added -> Added(step.id, slug) { slug = it }
        }
        adder.error?.let { Failure(it) }
        when (step) {
            McpServerAdder.Step.Pick -> if (address != null) {
                ActionButton("Continue", variant = ButtonVariant.Primary, size = ControlSize.Large, fills = true, pending = adder.adding == address) {
                    adder.add(address, null)
                }
            }
            is McpServerAdder.Step.Key -> Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                ActionButton("Back", size = ControlSize.Large, margin = beside(end = 8.dp)) { adder.back() }
                ActionButton(
                    "Add", variant = ButtonVariant.Primary, size = ControlSize.Large, pending = adder.adding != null, disabled = key.isBlank(),
                ) { addWithKey(step) }
            }
            is McpServerAdder.Step.Added -> ActionButton("Done", variant = ButtonVariant.Primary, size = ControlSize.Large, fills = true, onClick = ::done)
            else -> Unit
        }
    }
}

@Composable
private fun Pick(search: String, onSearch: (String) -> Unit, address: String?, known: List<KnownMcpServer>, adder: McpServerAdder) {
    val palette = LocalPalette.current
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    InputField(
        McpServerText.search, search, onSearch, Modifier.fillMaxWidth(), icon = if (address == null) Symbol.Search else Symbol.Link,
        size = ControlSize.Large, clearable = true, focusRequester = focus,
        keyboardOptions = plainKeyboard.copy(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go),
        onSubmit = { address?.let { adder.add(it, null) } },
    )
    if (address != null) {
        Text("Your gateway reaches it there, and signs in or asks you for a key if it needs one.", size = 12.5f, color = palette.mutedForeground)
        return
    }
    if (known.isEmpty()) {
        Text("Nothing known by that name. Paste its address instead.", size = 12.5f, color = palette.mutedForeground)
        return
    }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        for (pair in known.chunked(2)) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                for (mcpServer in pair) KnownTile(mcpServer, adder, Modifier.weight(1f))
                if (pair.size == 1) Spacer(Modifier.weight(1f))
            }
        }
    }
}

@Composable
private fun KnownTile(known: KnownMcpServer, adder: McpServerAdder, modifier: Modifier) {
    val palette = LocalPalette.current
    val shape = RoundedCornerShape(Radius.md)
    HighlightButton({ adder.add(known.url, known.name) }, modifier.border(1.dp, palette.border, shape), enabled = adder.adding == null, radius = Radius.md) {
        Row(Modifier.fillMaxWidth().padding(8.dp), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            McpServerTile(known.monogram, known.tint)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                Text(known.name, size = 13f, weight = FontWeight.Medium, color = palette.foreground, maxLines = 1)
                Text(known.host, size = 11.5f, color = palette.mutedForeground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
            }
            if (adder.adding == known.url) Spinner(IconSize.Md, color = palette.mutedForeground)
        }
    }
}

@Composable
private fun SigningIn() {
    val palette = LocalPalette.current
    Text("Approve Motile in the browser.", size = 13f, color = palette.mutedForeground)
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        Spinner(iconSize(text = 13f), color = palette.mutedForeground)
        Text("Waiting for you to approve it", size = 13f, color = palette.mutedForeground)
    }
}

/** A key and the header it goes in, with what they are for over them. */
@Composable
private fun KeyFields(caption: String?, key: String, onKey: (String) -> Unit, header: String, onHeader: (String) -> Unit, submit: () -> Unit) {
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    caption?.let { Text(it, size = 13f, color = LocalPalette.current.mutedForeground) }
    InputField(
        "Key", key, onKey, Modifier.fillMaxWidth(), size = ControlSize.Large, monospaced = true, secure = true, focusRequester = focus,
        keyboardOptions = plainKeyboard.copy(imeAction = ImeAction.Done), onSubmit = submit,
    )
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        SheetLabel("Header")
        InputField("Authorization", header, onHeader, Modifier.fillMaxWidth(), size = ControlSize.Large, monospaced = true, keyboardOptions = plainKeyboard)
    }
}

/** What an MCP server that was just added is called by agents, and its tools. */
@Composable
private fun Added(id: String, slug: String, onSlug: (String) -> Unit) {
    val mcpServers = LocalStore.current.gateway.mcpServers
    val palette = LocalPalette.current
    val mcpServer = mcpServers.mcpServer(id)
    if (mcpServer == null) {
        Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { Spinner(color = palette.mutedForeground) }
        return
    }
    LaunchedEffect(mcpServer.id) { onSlug(mcpServer.slug) }
    Text(mcpServer.toolsText, size = 13f, color = palette.mutedForeground)
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        SheetLabel("Name agents see")
        InputField(mcpServer.slug, slug, onSlug, Modifier.fillMaxWidth(), size = ControlSize.Large, monospaced = true, keyboardOptions = plainKeyboard)
        SlugExample(mcpServer, slug)
    }
    if (mcpServer.groups.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        SheetLabel("Tool permissions")
        for (group in mcpServer.groups) {
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                McpToolGroupHeader(mcpServer, group, mcpServers)
            }
        }
    }
}

/** Names an MCP server, for the user and for the agents. */
@Composable
fun McpServerRenameSheet(mcpServer: McpServerRow) {
    val mcpServers = LocalStore.current.gateway.mcpServers
    val close = LocalCloseModal.current
    var name by remember(mcpServer.id) { mutableStateOf(mcpServer.name) }
    var slug by remember(mcpServer.id) { mutableStateOf(mcpServer.slug) }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    fun save() {
        mcpServers.rename(mcpServer, name, slug)
        close()
    }
    McpServerSheetFrame("Rename ${mcpServer.name}") {
        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            SheetLabel("Name")
            InputField(mcpServer.name, name, { name = it }, Modifier.fillMaxWidth(), size = ControlSize.Large, focusRequester = focus, onSubmit = ::save)
        }
        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            SheetLabel("Name agents see")
            InputField(
                mcpServer.slug, slug, { slug = it }, Modifier.fillMaxWidth(), size = ControlSize.Large, monospaced = true,
                keyboardOptions = plainKeyboard.copy(imeAction = ImeAction.Done), onSubmit = ::save,
            )
            SlugExample(mcpServer, slug)
        }
        ActionButton("Save", variant = ButtonVariant.Primary, size = ControlSize.Large, fills = true, onClick = ::save)
    }
}

/** Gives an MCP server a new key. */
@Composable
fun McpServerKeySheet(mcpServer: McpServerRow) {
    val mcpServers = LocalStore.current.gateway.mcpServers
    val close = LocalCloseModal.current
    var key by remember { mutableStateOf("") }
    var header by remember { mutableStateOf("") }
    var pending by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    fun save() {
        if (key.isBlank() || pending) return
        pending = true
        error = null
        mcpServers.setKey(mcpServer, header, key) { result ->
            pending = false
            error = result.coreError?.message
            if (result.isSuccess) close()
        }
    }
    McpServerSheetFrame("New Key for ${mcpServer.name}") {
        KeyFields("Paste a new key from ${mcpServer.name}'s settings.", key, { key = it }, header, { header = it }, ::save)
        error?.let { Failure(it) }
        ActionButton("Save", variant = ButtonVariant.Primary, size = ControlSize.Large, fills = true, pending = pending, disabled = key.isBlank(), onClick = ::save)
    }
}

/** Calls one of an MCP server's tools with what its fields hold, and shows what it answered. */
@Composable
fun McpServerTrySheet(mcpServer: McpServerRow) {
    val mcpServers = LocalStore.current.gateway.mcpServers
    val palette = LocalPalette.current
    var tool by remember { mutableStateOf("") }
    var fields by remember { mutableStateOf<Loaded<List<McpServerField>>>(Loaded.Loading) }
    var values by remember { mutableStateOf(emptyMap<String, String>()) }
    var running by remember { mutableStateOf(false) }
    var answer by remember { mutableStateOf<McpServerAnswer?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var showsJSON by remember { mutableStateOf(false) }
    val picked = mcpServer.tools.firstOrNull { it.name == tool }
    fun pick(name: String) {
        tool = name
        answer = null
        error = null
        fields = Loaded.Loading
        mcpServers.loadFields(mcpServer.id, name) { result ->
            if (tool != name) return@loadFields
            val made = result.getOrElse {
                fields = Loaded.Failed(result.coreError?.message ?: "Its fields couldn't be read.")
                return@loadFields
            }
            values = made.associate { it.name to it.value }
            fields = Loaded.Ready(made)
        }
    }
    fun run() {
        if (running || tool.isEmpty()) return
        running = true
        error = null
        mcpServers.tryTool(mcpServer.id, tool, values) { result ->
            running = false
            val answered = result.getOrElse {
                error = result.coreError?.message
                return@tryTool
            }
            answer = answered
            error = answered.error
        }
    }
    LaunchedEffect(Unit) { mcpServer.tools.firstOrNull()?.let { pick(it.name) } }
    McpServerSheetFrame("Try a Tool") {
        ActionMenu(picked?.title ?: "Pick a Tool", variant = ButtonVariant.Outline, size = ControlSize.Large) {
            for (each in mcpServer.tools) item(each.title, checked = each.name == tool) { pick(each.name) }
        }
        picked?.summary?.let { Text(it, size = 12.5f, color = palette.mutedForeground) }
        when (val shown = fields) {
            Loaded.Loading -> if (tool.isNotEmpty()) Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { Spinner(color = palette.mutedForeground) }
            is Loaded.Failed -> Failure(shown.message)
            is Loaded.Ready -> for (field in shown.value) {
                FieldInput(field, values[field.name] ?: "") { values = values + (field.name to it) }
            }
        }
        if (picked != null && picked.kind != "read") Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Symbol.TriangleAlert, IconSize.Sm, color = palette.warning)
            Text(McpServerText.runsForReal, size = 12f, color = palette.warning)
        }
        error?.let { Failure(it) }
        answer?.let { answered ->
            Row(verticalAlignment = Alignment.CenterVertically) {
                SheetLabel("Answer")
                Spacer(Modifier.weight(1f))
                Segmented(listOf("Text" to false, "JSON" to true), showsJSON, { showsJSON = it }, size = ControlSize.Small)
            }
            McpServerTextBox(if (showsJSON) answered.json else answered.text, scaled(240f).dp)
        }
        ActionButton(
            "Run", Symbol.Play, variant = ButtonVariant.Primary, size = ControlSize.Large, fills = true, pending = running, disabled = tool.isEmpty(), onClick = ::run,
        )
    }
}

/** A field of the tool's form, named as its schema names it, with what it is for under its name. */
@Composable
private fun FieldInput(field: McpServerField, value: String, onChange: (String) -> Unit) {
    val palette = LocalPalette.current
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(field.name, style = Theme.uiMono(12f, FontWeight.Medium), color = palette.foreground)
            if (field.required) Text("required", size = 11.5f, color = palette.mutedStrongerForeground)
            Spacer(Modifier.weight(1f))
            if (field.kind == McpServerField.Kind.Toggle) Switch(value == "true", { onChange(if (it) "true" else "false") })
        }
        field.description?.let { Text(it, size = 11.5f, color = palette.mutedForeground) }
        when (field.kind) {
            McpServerField.Kind.Text -> InputField(field.name, value, onChange, Modifier.fillMaxWidth(), size = ControlSize.Large, keyboardOptions = plainKeyboard)
            McpServerField.Kind.Number -> InputField(
                "0", value, onChange, Modifier.fillMaxWidth(), size = ControlSize.Large,
                keyboardOptions = plainKeyboard.copy(keyboardType = KeyboardType.Decimal),
            )
            McpServerField.Kind.Json -> TextArea("{}", value, onChange, Modifier.fillMaxWidth(), size = ControlSize.Large, monospaced = true, lines = 2, maxLines = 8)
            McpServerField.Kind.Choice -> ActionMenu(value.ifEmpty { "Pick One" }, variant = ButtonVariant.Outline, size = ControlSize.Large) {
                item("None", checked = value.isEmpty()) { onChange("") }
                for (choice in field.choices) item(choice, checked = value == choice) { onChange(choice) }
            }
            McpServerField.Kind.Toggle -> Unit
        }
    }
}

/** A call in full: what the agent sent and what came back. */
@Composable
fun McpServerCallSheet(entry: Long) {
    val mcpServers = LocalStore.current.gateway.mcpServers
    val palette = LocalPalette.current
    var call by remember(entry) { mutableStateOf<Loaded<McpServerCallView?>>(Loaded.Loading) }
    LaunchedEffect(entry) {
        mcpServers.loadCall(entry) { result ->
            call = result.fold({ Loaded.Ready(it) }, { Loaded.Failed(result.coreError?.message ?: "The call couldn't be read.") })
        }
    }
    McpServerSheetFrame("Tool Call") {
        when (val shown = call) {
            Loaded.Loading -> Box(Modifier.fillMaxWidth().heightIn(min = 120.dp), contentAlignment = Alignment.Center) {
                Spinner(color = palette.mutedForeground)
            }
            is Loaded.Failed -> Text(shown.message, size = 13f, color = palette.destructive)
            is Loaded.Ready -> {
                val view = shown.value
                if (view == null) {
                    Text("Your gateway no longer has this call", size = 13f, color = palette.mutedForeground)
                    return@McpServerSheetFrame
                }
                Text(dotted(view.row.summary, palette.mutedStrongerForeground), size = 12f, color = palette.mutedForeground, maxLines = 2)
                SheetLabel("Arguments")
                McpServerTextBox(view.arguments, scaled(180f).dp)
                view.error?.let { failure ->
                    SheetLabel("Error")
                    SelectionContainer { Text(failure, size = 12.5f, color = palette.warning) }
                }
                SheetLabel("Result")
                McpServerTextBox(view.result, scaled(280f).dp)
            }
        }
    }
}
