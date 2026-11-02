package app.motile.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.layout.SubcomposeLayout
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import app.motile.LocalStore
import app.motile.core.Agent
import app.motile.core.Proxy
import app.motile.core.ProxyEnroll
import app.motile.core.ProxyAccount
import app.motile.core.ProxySignIn
import app.motile.core.ProxyView
import app.motile.core.SettingsSection
import app.motile.core.ThreadInfo
import app.motile.core.Time
import app.motile.layout.KeyboardFocus
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.IconSize
import app.motile.theme.LocalPalette
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.ShadowSize
import app.motile.theme.Theme
import app.motile.theme.iconSize
import app.motile.theme.opacity
import app.motile.theme.shadow
import app.motile.theme.wash
import app.motile.ui.ActionButton
import app.motile.ui.ActionMenu
import app.motile.ui.beside
import app.motile.ui.AgentIcon
import app.motile.ui.Answer
import app.motile.ui.Asking
import app.motile.ui.ButtonVariant
import app.motile.ui.Chip
import app.motile.ui.CloseModal
import app.motile.ui.CommandBox
import app.motile.ui.Confirmation
import app.motile.ui.CopyButton
import app.motile.ui.Icon
import app.motile.ui.InputField
import app.motile.ui.InputVariant
import app.motile.ui.LocalCloseModal
import app.motile.ui.Modal
import app.motile.ui.SheetSurface
import app.motile.ui.Spinner
import app.motile.ui.Switch
import app.motile.ui.Text
import app.motile.usage.LimitWindowRow
import kotlinx.coroutines.delay

/** What the proxy's page asks before it takes away: the proxy itself, or one of its accounts. */
private sealed interface ProxyRemoval {
    data class OfProxy(val name: String) : ProxyRemoval
    data class OfAccount(val account: ProxyAccount) : ProxyRemoval

    fun confirmation(proxy: Proxy): Confirmation = when (this) {
        is OfProxy -> Confirmation(
            "Remove “$name”?",
            "Its accounts go with it.",
            listOf(Answer("Remove", ButtonVariant.Danger) { proxy.remove() }),
        )
        is OfAccount -> Confirmation(
            "Remove “${account.name}”?",
            "Its threads move to your other accounts.",
            listOf(Answer("Remove", ButtonVariant.Danger) { proxy.removeAccount(account) }),
        )
    }
}

/**
 * The account's proxy: how to add one, or how it is, its accounts by agent with their limits and its
 * requests. The core follows the proxy while the page shows.
 */
@Composable
fun ProxyPage() {
    val store = LocalStore.current
    val proxy = store.proxy
    val view = proxy.view
    val navigator = LocalSettingsNavigator.current
    var adding by remember { mutableStateOf(false) }
    var renaming by remember { mutableStateOf<ProxyAccount?>(null) }
    var removal by remember { mutableStateOf<ProxyRemoval?>(null) }
    var pickingThread by remember { mutableStateOf(false) }
    DisposableEffect(proxy) {
        proxy.watch(true)
        onDispose { proxy.watch(false) }
    }
    val add = {
        proxy.startAdding()
        adding = true
    }
    SettingsList(target = store.settingsTarget, reached = { store.settingsTarget = null }) {
        if (!view.exists) return@SettingsList absent(add)
        group("proxy", failure = view.error) {
            row { Header(view, add) { removal = ProxyRemoval.OfProxy(view.name ?: "Proxy") } }
            view.version?.let { version -> row { SettingsLabeled("Version", version) } }
        }
        for (agent in Agent.entries) accounts(agent, view, proxy, rename = { renaming = it }) { removal = ProxyRemoval.OfAccount(it) }
        group("proxy-requests", header = "Log", footer = view.capture.text) {
            row {
                SettingsLink({
                    proxy.log.reload()
                    navigator.push(SettingsRoute.ProxyLog)
                }) { RowText("Request Log", Modifier.weight(1f), maxLines = 1) }
            }
            row { SettingsPressRow({ proxy.captureNext() }) { SettingsActionLabel("Capture the Next 20 Requests", Symbol.CircleDot) } }
            row { SettingsPressRow({ pickingThread = true }) { SettingsActionLabel("Capture a Thread", Symbol.MessageSquareText) } }
            if (view.capture.on) row { SettingsButton("Stop Capturing", destructive = true) { proxy.stopCapturing() } }
        }
    }
    Modal(adding, onDismiss = { adding = false }) { AddProxySheet() }
    Modal(view.signIn, onDismiss = { proxy.cancelSignIn() }) { SignInSheet(it) }
    Modal(pickingThread, onDismiss = { pickingThread = false }) { CaptureThreadSheet() }
    renaming?.let { account -> RenameAccountDialog(account) { renaming = null } }
    Asking(removal, onDismiss = { removal = null }) { it.confirmation(proxy) }
}

/** What a proxy is, for an account that has none, and the way to add one. */
private fun SettingsGroups.absent(add: () -> Unit) {
    group(bare = true) {
        row {
            Column(Modifier.fillMaxWidth().padding(top = 12.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                val palette = LocalPalette.current
                Box(Modifier.size(56.dp).background(palette.primary.wash(), RoundedCornerShape(Radius.lg)), contentAlignment = Alignment.Center) {
                    Icon(Symbol.Waypoints, IconSize.Display, color = palette.primary)
                }
                Text("No proxy", Modifier.padding(top = 6.dp), size = 17f, weight = FontWeight.SemiBold, align = TextAlign.Center)
                Text(
                    "Runs on a machine that is always on",
                    Modifier.widthIn(max = 480.dp).padding(horizontal = 16.dp),
                    size = 13f, color = palette.mutedForeground, align = TextAlign.Center,
                )
            }
        }
    }
    group { row { SettingsPressRow(add) { SettingsActionLabel("Add a Proxy", Symbol.Plus) } } }
}

/** The proxy's name and how the client stands with it, with its menu: another proxy in its place, or its removal. */
@Composable
private fun Header(view: ProxyView, add: () -> Unit, remove: () -> Unit) {
    SettingsRow {
        SettingsItem(view.name ?: "Proxy", view.statusText, picture = { SettingsTile(Symbol.Waypoints) })
        ActionMenu(icon = Symbol.Ellipsis, help = "More") {
            item("Replace with a New Proxy", Symbol.RefreshCw, run = add)
            divider()
            item("Remove Proxy", Symbol.Trash2, destructive = true, run = remove)
        }
    }
}

/** An agent's accounts, a card each with its limits, and the way to add another. */
private fun SettingsGroups.accounts(agent: Agent, view: ProxyView, proxy: Proxy, rename: (ProxyAccount) -> Unit, remove: (ProxyAccount) -> Unit) {
    val accounts = view.accounts(agent)
    group("proxy-add/${agent.raw}", headerContent = { AgentHeader(agent) }) {
        row { SignInRow("Add ${agent.title} Account", agent, proxy, Symbol.Plus) }
    }
    accounts.forEach { account ->
        group("proxy-account/${account.id}") {
            row { AccountRow(account, accounts.size, proxy, rename = { rename(account) }) { remove(account) } }
            if (account.status == ProxyAccount.Status.SignInAgain) row { SignInRow("Sign In Again", agent, proxy, Symbol.CircleUser) }
            for (window in account.windows) row {
                Box(Modifier.opacity(Opacity.Disabled, on = !account.enabled)) { LimitWindowRow(window) }
            }
            account.note?.let { note -> row { SettingsRow { RowText(note, color = LocalPalette.current.mutedForeground) } } }
        }
    }
}

@Composable
private fun AgentHeader(agent: Agent) {
    val palette = LocalPalette.current
    Row(horizontalArrangement = Arrangement.spacedBy(5.dp), verticalAlignment = Alignment.CenterVertically) {
        AgentIcon(agent, size = iconSize(text = NOTE_TEXT))
        Text(agent.title, size = NOTE_TEXT, weight = FontWeight.Medium, color = palette.mutedForeground, maxLines = 1)
    }
}

/** Starts a sign-in of the agent on the proxy, waiting while it starts. */
@Composable
private fun SignInRow(title: String, agent: Agent, proxy: Proxy, symbol: Symbol) {
    SettingsPressRow({ proxy.signIn(agent) }, enabled = proxy.startingSignIn == null) {
        SettingsActionLabel(title, symbol)
        Spacer(Modifier.weight(1f))
        if (proxy.startingSignIn == agent) Spinner(IconSize.Xl)
    }
}

/** An account: its agent and name, how it stands, who is signed in, the threads on it, its switch and its menu. */
@Composable
private fun AccountRow(account: ProxyAccount, count: Int, proxy: Proxy, rename: () -> Unit, remove: () -> Unit) {
    val palette = LocalPalette.current
    SettingsRow {
        // As far from the card's top as its limits' numbers are from theirs.
        AccountLines(account, Modifier.weight(1f).padding(vertical = 8.dp))
        Switch(account.enabled, { proxy.setEnabled(account, it) })
        ActionMenu(icon = Symbol.Ellipsis, help = "More") {
            item("Rename Account", Symbol.Pencil, run = rename)
            submenu(account.priorityText, Symbol.ListFilter) {
                for (number in 1..maxOf(count, account.priority + 1)) {
                    item("Priority $number", checked = account.priority == number - 1) { proxy.setPriority(account, number - 1) }
                }
            }
            item("Sign In Again", Symbol.CircleUser) { proxy.signIn(account.agent) }
            divider()
            item("Remove Account", Symbol.Trash2, destructive = true, run = remove)
        }
    }
}

/** The account's name with its badges beside it, or after who is signed in under it where that would cut the name short. */
@Composable
private fun AccountLines(account: ProxyAccount, modifier: Modifier) {
    val palette = LocalPalette.current
    val tone = account.status.tone()
    val name: @Composable () -> Unit = {
        AgentIcon(account.agent, size = iconSize(text = ROW_TEXT))
        RowText(account.name, color = palette.foreground, maxLines = 1)
    }
    val badges: @Composable () -> Unit = {
        account.statusText?.let { Chip(it, tone = tone) }
        if (account.next) Chip("Next", tone = palette.primary)
    }
    val line = Arrangement.spacedBy(6.dp)
    SubcomposeLayout(modifier) { constraints ->
        val together = subcompose("together") {
            Row(horizontalArrangement = line, verticalAlignment = Alignment.CenterVertically) { name(); badges() }
        }.first().measure(Constraints())
        val beside = together.width <= constraints.maxWidth
        val lines = subcompose("lines") {
            Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Row(horizontalArrangement = line, verticalAlignment = Alignment.CenterVertically) {
                    name()
                    if (beside) badges()
                }
                val who = listOfNotNull(account.email, account.plan, account.threadsText).joinToString(" · ")
                Row(horizontalArrangement = line, verticalAlignment = Alignment.CenterVertically) {
                    if (who.isNotEmpty()) Text(who, Modifier.weight(1f, fill = false), size = NOTE_TEXT, color = palette.mutedForeground, maxLines = 1)
                    if (!beside) badges()
                }
            }
        }.first().measure(constraints.copy(minWidth = 0))
        layout(constraints.maxWidth, lines.height) { lines.place(0, 0) }
    }
}

@Composable
private fun ProxyAccount.Status.tone(): Color? {
    val palette = LocalPalette.current
    return when (this) {
        ProxyAccount.Status.SignInAgain -> palette.warning
        ProxyAccount.Status.Resting -> palette.pending
        ProxyAccount.Status.Ready, ProxyAccount.Status.Off -> null
    }
}

/** Renames an account, as iOS's alert with a field does. */
@Composable
private fun RenameAccountDialog(account: ProxyAccount, onDismiss: () -> Unit) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    var name by remember(account.id) { mutableStateOf(account.name) }
    val field = remember { FocusRequester() }
    val shape = RoundedCornerShape(Radius.xl)
    fun rename() {
        if (name.isBlank()) return
        store.proxy.rename(account, name)
        onDismiss()
    }
    Dialog(onDismissRequest = onDismiss) {
        SheetSurface(CloseModal(onDismiss)) {
            Column(
                Modifier
                    .widthIn(max = 420.dp)
                    .shadow(ShadowSize.Xl, shape, Opacity.ShadowStrongest)
                    .background(palette.background, shape)
                    .border(1.dp, palette.border, shape)
                    .padding(20.dp),
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Text("Rename Account", size = 15f, weight = FontWeight.SemiBold)
                InputField("Name", name, { name = it }, Modifier.fillMaxWidth(), size = ControlSize.Large, focusRequester = field, onSubmit = ::rename)
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    ActionButton("Cancel", size = ControlSize.Large, margin = beside(end = 8.dp), onClick = onDismiss)
                    ActionButton("Save", variant = ButtonVariant.Primary, size = ControlSize.Large, disabled = name.isBlank(), onClick = ::rename)
                }
            }
        }
    }
    LaunchedEffect(Unit) { runCatching { field.requestFocus() } }
    DisposableEffect(Unit) { onDispose { KeyboardFocus.editable = false } }
}

/** A sheet's bar with the way to close it. */
@Composable
private fun SheetTitle(title: String) {
    val close = LocalCloseModal.current
    CompositionLocalProvider(LocalBarLeading provides BarLeading.Close { close() }) { SettingsBar(title) }
}

/**
 * How to run a new proxy and the command that links it to the account. It closes once the proxy
 * joins.
 */
@Composable
private fun AddProxySheet() {
    val proxy = LocalStore.current.proxy
    val close = LocalCloseModal.current
    DisposableEffect(proxy) { onDispose { proxy.stopAdding() } }
    LaunchedEffect(proxy.joined) { if (proxy.joined) close() }
    val enroll = proxy.enroll
    val replaced = proxy.view.name.takeIf { proxy.replacing != null }
    Column(Modifier.fillMaxSize()) {
        SheetTitle(if (replaced == null) "Add a Proxy" else "Replace Your Proxy")
        SettingsList(Modifier.weight(1f)) {
            group(bare = true, failure = proxy.enrollError.takeIf { enroll == null }) { row { EnrollStatus(enroll, proxy, replaced != null) } }
            if (enroll == null) return@SettingsList
            group(header = "Steps") {
                enroll.steps.forEachIndexed { index, step ->
                    row {
                        SettingsRow {
                            Text("${index + 1}", style = Theme.uiDigits(ROW_TEXT, FontWeight.Medium), color = LocalPalette.current.mutedForeground)
                            RowText(step, Modifier.weight(1f))
                        }
                    }
                }
            }
            group(header = "Command", bare = true) {
                row { CommandBox(enroll.command, Modifier.fillMaxWidth(), spans = enroll.spans) }
            }
        }
    }
}

/** How long the code still works while the proxy is awaited, or the way to a new one. */
@Composable
private fun EnrollStatus(enroll: ProxyEnroll?, proxy: Proxy, replacing: Boolean) {
    val palette = LocalPalette.current
    val now by produceState(Time.now()) {
        while (true) {
            delay(1000)
            value = Time.now()
        }
    }
    Column(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        if (enroll == null) {
            if (proxy.enrollPending) Spinner(IconSize.Xl, color = palette.mutedForeground)
            else ActionButton("Try Again") { proxy.makeCode() }
            return@Column
        }
        if (enroll.isExpired(now)) {
            Text("The code has expired", size = 13f, color = palette.mutedForeground)
            ActionButton("New Code", pending = proxy.enrollPending) { proxy.makeCode() }
            return@Column
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            Spinner(iconSize(text = 13f), color = palette.mutedForeground)
            Text(if (replacing) "Waiting for your new proxy" else "Waiting for your proxy", size = 13f, color = palette.mutedForeground)
        }
        Text("Works for ${enroll.timeLeft(now)}", style = Theme.uiDigits(12f), color = palette.mutedStrongerForeground)
    }
}

/**
 * An account's sign-in: Claude's page shows a code to paste here; Codex's takes the code shown here and
 * adds the account once the user approves it. Closing the sheet drops the sign-in.
 */
@Composable
private fun SignInSheet(signIn: ProxySignIn) {
    val proxy = LocalStore.current.proxy
    val palette = LocalPalette.current
    var code by remember { mutableStateOf("") }
    fun finish() {
        if (code.isBlank() || signIn.waiting) return
        proxy.finishSignIn(code)
    }
    Column(Modifier.fillMaxSize()) {
        SheetTitle("Sign In to ${signIn.agent.title}")
        SettingsList(Modifier.weight(1f)) {
            if (signIn.needsCode) {
                group(footer = signIn.text, failure = signIn.error) {
                    row { CommandBox(signIn.url, Modifier.fillMaxWidth(), link = signIn.url, open = proxy::openPage) }
                    row {
                        SettingsRow {
                            Box(Modifier.weight(1f).height(ControlSize.Large.height)) {
                                InputField(
                                    "Paste the code", code, { code = it }, variant = InputVariant.Bare, size = ControlSize.Large, monospaced = true,
                                    keyboardOptions = plainKeyboard.copy(imeAction = ImeAction.Done), onSubmit = ::finish,
                                )
                            }
                            ActionButton("Sign In", variant = ButtonVariant.Primary, pending = signIn.waiting, disabled = code.isBlank(), onClick = ::finish)
                        }
                    }
                }
                return@SettingsList
            }
            signIn.userCode?.let { userCode ->
                group(bare = true) {
                    row {
                        Row(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally), verticalAlignment = Alignment.CenterVertically) {
                            Text(userCode, style = Theme.uiMono(28f, FontWeight.SemiBold), color = palette.foreground)
                            CopyButton("Copy the code") { Platform.copy(userCode) }
                        }
                    }
                }
            }
            group(footer = signIn.text, failure = signIn.error) {
                if (signIn.error == null) row {
                    SettingsRow {
                        Spinner(IconSize.Xl, color = palette.mutedForeground)
                        RowText("Waiting for you to approve it", color = palette.mutedForeground)
                    }
                }
                row { CommandBox(signIn.url, Modifier.fillMaxWidth(), link = signIn.url, open = proxy::openPage) }
                if (signIn.error != null) row { SignInRow("Start Again", signIn.agent, proxy, Symbol.RotateCw) }
            }
        }
    }
}

/** The threads of the account's servers to capture every request of, the latest first, found by their titles. */
@Composable
private fun CaptureThreadSheet() {
    val store = LocalStore.current
    val close = LocalCloseModal.current
    var query by remember { mutableStateOf("") }
    val servers = store.servers.associateBy { it.id }
    val threads = (store.activeThreads + store.doneThreads)
        .filter { it.serverID in servers && (query.isBlank() || it.title.contains(query.trim(), ignoreCase = true)) }
        .sortedByDescending { it.updatedAt }
        .take(MOST_THREADS_LISTED)
    fun capture(thread: ThreadInfo) {
        store.proxy.captureThread(thread)
        close()
    }
    Column(Modifier.fillMaxSize()) {
        SheetTitle("Capture a Thread")
        InputField(
            "Search", query, { query = it }, Modifier.fillMaxWidth().padding(horizontal = 16.dp).padding(top = 4.dp),
            icon = Symbol.Search, size = ControlSize.Large, clearable = true,
        )
        SettingsList(Modifier.weight(1f)) {
            if (threads.isEmpty()) return@SettingsList note(if (query.isBlank()) "No threads yet" else "No matches")
            group {
                for (thread in threads) row {
                    SettingsPressRow({ capture(thread) }) { SettingsItem(thread.title, servers[thread.serverID]?.name ?: "") }
                }
            }
        }
    }
}

private const val MOST_THREADS_LISTED = 60

/** Opens the proxy's page from elsewhere in the settings. */
fun openProxySettings(open: (SettingsSection) -> Unit) = open(SettingsSection.Proxy)
