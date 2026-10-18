package app.motile.sidebar

import android.os.Handler
import android.os.Looper
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.AppStore
import app.motile.core.ListedDraft
import app.motile.core.Project
import app.motile.core.PullRequest
import app.motile.core.Selection
import app.motile.core.Server
import app.motile.core.ThreadInfo
import app.motile.core.Time
import app.motile.core.UndoNotice
import app.motile.core.isSearchingThreads
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalRow
import app.motile.theme.LocalSurface
import app.motile.theme.LocalForeground
import app.motile.theme.Palette
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.AgentIcon
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.KeyCaps
import app.motile.ui.MenuBuilder
import app.motile.ui.MenuPopup
import app.motile.ui.pressPosition
import app.motile.ui.rememberMenuState
import app.motile.ui.ProjectIcon
import app.motile.ui.PullRequestLabel
import app.motile.ui.ServerLabel
import app.motile.ui.Spinner
import app.motile.ui.Text
import app.motile.ui.ThemeDivider
import app.motile.ui.stateTint
import kotlinx.coroutines.delay

/** How far the rows' highlights stay from the sidebar's edges. */
val sidebarRowInset = 10.dp

/** The space between the rows' highlights, and around each one: it looks empty but is the row's. */
private val rowGap = 2.dp
val rowMargin = PaddingValues(start = sidebarRowInset, top = rowGap / 2, end = sidebarRowInset, bottom = rowGap / 2)

/** How far everything the sidebar shows stays from its sides, a row's content as well as a line's. */
val sidebarEdge = sidebarRowInset + 8.dp
val doneRowHeight = scaled(30f).dp

/** The height of a sidebar line that opens, or of one server's line. */
internal val lineHeight = 44.dp

/** One clock for every "5m" in the sidebar, so that they all change at once. */
object AgoClock {
    var now by mutableDoubleStateOf(Time.now())
        private set
    private val main = Handler(Looper.getMainLooper())
    private val tick = object : Runnable {
        override fun run() {
            now = Time.now()
            main.postDelayed(this, 30_000)
        }
    }

    init {
        main.postDelayed(tick, 30_000)
    }
}

/** The time now, once a second, for what counts up. */
@Composable
private fun everySecond(): Double {
    val now by produceState(Time.now()) {
        while (true) {
            delay(1000)
            value = Time.now()
        }
    }
    return now
}

/** The last part of a path, as a folder's name. */
internal fun lastComponent(path: String): String = path.trimEnd('/').substringAfterLast('/')

/** The folder with its server's home as `~`. */
fun shortenedFolder(path: String, home: String): String {
    if (home.isEmpty() || !path.startsWith(home)) return path
    val rest = path.drop(home.length)
    if (rest.isNotEmpty() && !rest.startsWith("/")) return path
    return "~$rest"
}

val Project.checkoutSymbol: Symbol
    get() {
        val git = git
        if (git != null && git.branch == null) return Symbol.GitCommitHorizontal
        return if (worktree == null) Symbol.GitBranch else Symbol.FolderGit2
    }

/** The colour of its status when it waits for the user: an approval, or a reply they haven't seen. */
fun ThreadInfo.attentionColor(palette: Palette): Color? = when {
    waitsFor != null -> palette.warning
    unread -> palette.success
    else -> null
}

val ThreadInfo.attentionColor: Color?
    @Composable @ReadOnlyComposable get() = attentionColor(LocalPalette.current)

/** Whether the sidebar offers to start a thread: there is somewhere to start it, and nothing above the offer. */
fun AppStore.offersNewThread(drafts: List<ListedDraft>, active: List<ThreadInfo>): Boolean {
    if (isSearchingThreads || drafts.isNotEmpty() || active.isNotEmpty()) return false
    return projects.isNotEmpty() || noProjects.isNotEmpty()
}

/** What a thread's menu offers, in groups a line divides. */
fun MenuBuilder.threadMenu(store: AppStore, thread: ThreadInfo, rename: (ThreadInfo) -> Unit, delete: (ThreadInfo) -> Unit) {
    if (store.onRemovedServer(thread)) return
    if (thread.isDone) {
        item("Mark Undone") { store.setDone(listOf(thread.id), done = false) }
    } else {
        item("Mark Done", disabled = thread.unfinished) { store.setDone(listOf(thread.id), done = true, fromSidebar = true) }
    }
    item("Rename") { rename(thread) }
    divider()
    item("Delete", destructive = true) { delete(thread) }
}

/** Motile's mark, as the app icon draws it, in a square of 24. */
internal val motileMark: Path by lazy {
    PathParser().parsePathString(
        "M18.477 1.929c1.107.2 1.988 1.241 1.701 2.468-.281 1.205.603 2.372 1.883 2.488 1.863.167 2.616 2.39 1.212 3.577a1.994 1.994 0 0 0 0 3.075c1.404 1.186.651 3.41-1.212 3.577-1.28.116-2.164 1.284-1.883 2.488.41 1.752-1.56 3.126-3.17 2.212-1.107-.63-2.538-.184-3.048.95-.508 1.128-1.81 1.484-2.818 1.068-.934-.385-.922-1.601-.598-2.559L16.52 3.63c.315-.929.993-1.874 1.958-1.7zm-8.437-.693C10.547.108 11.849-.248 12.857.168c.934.385.922 1.6.598 2.558L7.48 20.37c-.314.929-.992 1.874-1.957 1.7-1.107-.199-1.988-1.241-1.701-2.469.281-1.204-.603-2.372-1.884-2.488-1.863-.167-2.615-2.39-1.21-3.577a1.995 1.995 0 0 0 0-3.075c-1.405-1.187-.653-3.41 1.21-3.577 1.28-.116 2.165-1.283 1.884-2.488-.41-1.751 1.56-3.125 3.17-2.21 1.106.629 2.538.182 3.048-.95z",
    ).toPath()
}

/** An active thread: its project and what it is doing on the first line, its title on the second, where it works, its pull request, its server and its agent on the third. */
object ThreadRow {
    val sidePadding = 8.dp
    val topPadding = 5.dp
    val bottomPadding = 7.dp
    val titleHeight = scaled(16f).dp
}

@Composable
fun ThreadRow(
    thread: ThreadInfo,
    project: Project?,
    selected: Boolean,
    modifier: Modifier = Modifier,
    /** The keys that open it, while they are held. */
    jump: String? = null,
    lit: Boolean = false,
    onLongClick: (() -> Unit)? = null,
    /** On what the row shows, inside its press: it sees a touch before the press does. */
    contentModifier: Modifier = Modifier,
    open: (Selection) -> Unit,
) {
    val store = LocalStore.current
    val palette = Theme.colors
    val readOnly = store.onRemovedServer(thread)
    val hover = remember { MutableInteractionSource() }
    val hovering by hover.collectIsHoveredAsState()
    HighlightButton(
        onClick = { open(Selection.Thread(thread.id)) },
        modifier = modifier.hoverable(hover),
        radius = Radius.md,
        selected = selected,
        lit = lit,
        inset = rowMargin,
        hovered = hovering,
        opaque = true,
        onLongClick = onLongClick,
    ) {
        Column(
            contentModifier
                .fillMaxWidth()
                .padding(rowMargin)
                .padding(start = ThreadRow.sidePadding, end = ThreadRow.sidePadding, top = ThreadRow.topPadding, bottom = ThreadRow.bottomPadding),
        ) {
            Row(Modifier.height(scaled(22f).dp), verticalAlignment = Alignment.CenterVertically) {
                ProjectIcon(project, size = 14f)
                Text(
                    project?.name ?: lastComponent(thread.cwd), Modifier.weight(1f).padding(start = 6.dp, end = 6.dp),
                    size = 11f, weight = FontWeight.Medium, color = palette.mutedForeground, maxLines = 1,
                )
                CompositionLocalProvider(LocalForeground provides palette.mutedForeground) {
                    if (jump != null) KeyCaps(listOf(jump)) else ThreadStatus(thread, readOnly)
                }
            }
            Text(
                thread.title,
                Modifier.height(ThreadRow.titleHeight).padding(top = 1.dp),
                size = 13f, weight = FontWeight.Medium, maxLines = 1,
                color = if (selected || hovering) palette.foreground else palette.mutedForeground,
            )
            Spacer(Modifier.height(5.dp))
            CompositionLocalProvider(LocalForeground provides palette.mutedStrongerForeground) {
                Row(Modifier.height(scaled(16f).dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.weight(1f)) {
                        val branch = project?.branch
                        if (project != null && branch != null) {
                            CheckoutLabel(project.checkoutSymbol, branch)
                        } else {
                            ThreadFolderLabel(thread.serverID, thread.cwd)
                        }
                    }
                    Row(Modifier.padding(start = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                        if (!readOnly) project?.pullRequest(thread)?.let { ThreadPullRequestLabel(thread, it, open = open) }
                        ThreadServerLabel(thread.serverID)
                        AgentIcon(thread.agent, size = 12f)
                    }
                }
            }
        }
    }
}

/** A thread's pull request in its row. A tap opens the thread and then the pull request's tab, as View PR does. */
@Composable
private fun ThreadPullRequestLabel(thread: ThreadInfo, pullRequest: PullRequest, colored: Boolean = true, open: (Selection) -> Unit) {
    val store = LocalStore.current
    PullRequestLabel(pullRequest, colored = colored) {
        open(Selection.Thread(thread.id))
        store.showPullRequest(pullRequest.url)
    }
}

/** The folder a thread works in, with its server's home as `~`. */
@Composable
fun ThreadFolderLabel(serverID: String, folder: String) {
    val store = LocalStore.current
    CheckoutLabel(Symbol.Folder, shortenedFolder(folder, store.server(serverID)?.home ?: ""))
}

/** Where a thread works, on the last line of its row. */
@Composable
private fun CheckoutLabel(symbol: Symbol, text: String) {
    Row(horizontalArrangement = Arrangement.spacedBy(3.dp), verticalAlignment = Alignment.CenterVertically) {
        Icon(symbol, 11f)
        Text(text, size = 11f, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
    }
}

/** The server a thread is on, when there is more than one. */
@Composable
private fun ThreadServerLabel(serverID: String) {
    val store = LocalStore.current
    if (store.servers.size + store.removedServers.size <= 1) return
    val server = store.server(serverID) ?: return
    ServerLabel(server, Modifier.padding(start = 1.dp))
}

/** The threads that haven't been sent yet, above the others, and a line under them. */
@Composable
fun DraftRows(
    swipe: (String) -> SwipeState = { SwipeState.Closed },
    open: (Selection) -> Unit,
) {
    val store = LocalStore.current
    val listed = store.listedDrafts
    if (listed.isEmpty()) return
    Column {
        for (draft in listed) {
            if (draft.sending) {
                DraftRow(draft, open = open)
                continue
            }
            val swiped = swipe(draft.id)
            RowSwipe(
                Symbol.Trash2, "Discard Draft", tint = Theme.colors.destructive, size = 36.dp, isOpen = swiped.isOpen, onOpenChange = swiped.onOpenChange,
                action = { store.discard(draft.draft) },
            ) {
                WithMenu({ item("Discard Draft", destructive = true) { store.discard(draft.draft) } }) { modifier, openMenu ->
                    DraftRow(draft, modifier, openMenu, open)
                }
            }
        }
        ThemeDivider(Modifier.padding(horizontal = sidebarEdge, vertical = 4.dp + rowGap / 2))
    }
}

/** Whether a row is slid aside to show its button, and what changes that. */
class SwipeState(val isOpen: Boolean, val onOpenChange: (Boolean) -> Unit) {
    companion object {
        val Closed = SwipeState(false) {}
    }
}

/** A draft, laid out like a thread's row: its project and "Draft", or "Sending" while its first message is on its way, what was written in it, where it will work, its server and its agent. */
@Composable
private fun DraftRow(listed: ListedDraft, modifier: Modifier = Modifier, onLongClick: (() -> Unit)? = null, open: (Selection) -> Unit) {
    val store = LocalStore.current
    val palette = Theme.colors
    val project = store.project(listed.draft.projectID)
    val selected = store.selection == Selection.Draft(listed.id)
    val hover = remember { MutableInteractionSource() }
    val hovering by hover.collectIsHoveredAsState()
    HighlightButton(
        onClick = { open(Selection.Draft(listed.id)) },
        modifier = modifier.hoverable(hover),
        radius = Radius.md,
        selected = selected,
        inset = rowMargin,
        hovered = hovering,
        opaque = true,
        onLongClick = onLongClick,
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .padding(rowMargin)
                .padding(start = ThreadRow.sidePadding, end = ThreadRow.sidePadding, top = ThreadRow.topPadding, bottom = ThreadRow.bottomPadding),
        ) {
            CompositionLocalProvider(LocalForeground provides palette.mutedForeground) {
                Row(Modifier.height(scaled(22f).dp), verticalAlignment = Alignment.CenterVertically) {
                    ProjectIcon(project, size = 14f)
                    Text(project?.name ?: "No project", Modifier.weight(1f).padding(horizontal = 6.dp), size = 11f, weight = FontWeight.Medium, maxLines = 1)
                    when {
                        listed.sending -> Row(horizontalArrangement = Arrangement.spacedBy(3.dp), verticalAlignment = Alignment.CenterVertically) {
                            Spinner(11f)
                            Text("Sending", size = 11f, weight = FontWeight.Medium)
                        }
                        hovering -> ActionButton(icon = Symbol.X, help = "Discard draft", size = ControlSize.Small) { store.discard(listed.draft) }
                        else -> Row(horizontalArrangement = Arrangement.spacedBy(3.dp), verticalAlignment = Alignment.CenterVertically) {
                            Icon(Symbol.File, 11f)
                            Text("Draft", size = 11f, weight = FontWeight.Medium)
                        }
                    }
                }
            }
            Text(
                listed.preview,
                Modifier.height(ThreadRow.titleHeight).padding(top = 1.dp),
                size = 13f, weight = FontWeight.Medium, maxLines = 1,
                color = if (selected || hovering) palette.foreground else palette.mutedForeground,
            )
            Spacer(Modifier.height(5.dp))
            CompositionLocalProvider(LocalForeground provides palette.mutedStrongerForeground) {
                Row(Modifier.height(scaled(16f).dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.weight(1f)) {
                        val branch = project?.branch
                        when {
                            project != null && branch != null -> CheckoutLabel(project.checkoutSymbol, branch)
                            project != null -> ThreadFolderLabel(project.serverID, project.path)
                        }
                    }
                    Row(Modifier.padding(start = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                        project?.let { ThreadServerLabel(it.serverID) }
                        store.agentOf(listed.draft)?.let { AgentIcon(it, size = 12f) }
                    }
                }
            }
        }
    }
}

/** A thread that is done: one quiet line, with its pull request. */
@Composable
fun DoneRow(
    thread: ThreadInfo,
    project: Project?,
    selected: Boolean,
    modifier: Modifier = Modifier,
    onLongClick: (() -> Unit)? = null,
    open: (Selection) -> Unit,
) {
    val store = LocalStore.current
    val palette = Theme.colors
    val hover = remember { MutableInteractionSource() }
    val hovering by hover.collectIsHoveredAsState()
    val buttonInset = (doneRowHeight - ControlSize.Small.height) / 2
    HighlightButton(
        onClick = { open(Selection.Thread(thread.id)) },
        modifier = modifier.hoverable(hover),
        radius = Radius.md,
        selected = selected,
        inset = rowMargin,
        hovered = hovering,
        opaque = true,
        onLongClick = onLongClick,
        contentAlignment = Alignment.CenterStart,
    ) {
        Row(
            Modifier.fillMaxWidth().padding(rowMargin).padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            ProjectIcon(project, size = 14f)
            Text(thread.title, Modifier.weight(1f).padding(start = 7.dp, end = 6.dp), size = 13f, maxLines = 1, color = palette.mutedForeground)
            Row(horizontalArrangement = Arrangement.spacedBy(7.dp), verticalAlignment = Alignment.CenterVertically) {
                project?.pullRequest(thread)?.let { ThreadPullRequestLabel(thread, it, colored = false, open = open) }
                Box(contentAlignment = Alignment.CenterEnd) {
                    val ago = Time.ago(thread.doneAt ?: thread.updatedAt, AgoClock.now)
                    Text(ago, Modifier.alpha(if (hovering) 0f else 1f), size = 11f, color = palette.mutedStrongerForeground)
                }
            }
        }
        if (hovering && !store.onRemovedServer(thread)) {
            OnRowSurface(Modifier.align(Alignment.CenterEnd).padding(end = sidebarRowInset + buttonInset)) {
                ActionButton(icon = Symbol.Undo2, help = "Mark undone", size = ControlSize.Small) { store.setDone(listOf(thread.id), done = false) }
            }
        }
    }
}

/** Covers what lies under it with the colour the row is lit in. */
@Composable
private fun OnRowSurface(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val surface = LocalSurface.current
    val row = LocalRow.current ?: Surface.Layer.Row
    Box(modifier.background(surface.color(row)).padding(start = 6.dp)) { content() }
}

/** Starts a thread, where the sidebar has no draft and no active thread. Laid out like a done row. */
@Composable
fun NewThreadRow(modifier: Modifier = Modifier) {
    val store = LocalStore.current
    HighlightButton(onClick = { store.newThread() }, modifier = modifier, radius = Radius.md, inset = rowMargin, faded = true, opaque = true, contentAlignment = Alignment.CenterStart) {
        Row(Modifier.fillMaxWidth().padding(rowMargin).padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.width(14.dp), contentAlignment = Alignment.Center) { Icon(Symbol.Plus, 14f) }
            Text("Create a thread", Modifier.padding(start = 7.dp), size = 13f, maxLines = 1)
        }
    }
}

/** What a thread is up to, or how long ago it last was. A thread of an unlinked server is only ever read-only. */
@Composable
fun ThreadStatus(thread: ThreadInfo, readOnly: Boolean = false) {
    val palette = Theme.colors
    val asking = thread.waitsFor
    val stage = thread.gitStage
    val interruption = thread.interruption
    when {
        readOnly -> StatusLabel("Read-only", palette.warning, Modifier.semantics { contentDescription = readOnlyHelp }) { Icon(Symbol.Unlink, 11f) }
        asking != null -> {
            val help = thread.children?.waitingText?.takeIf { thread.asking == null }
            val described = if (help == null) Modifier else Modifier.semantics { contentDescription = help }
            StatusLabel(asking.word, palette.warning, described) { Icon(asking.symbol, 11f) }
        }
        thread.waitsOnAgents -> Row(
            Modifier.semantics { contentDescription = thread.children?.runningText.orEmpty() },
            horizontalArrangement = Arrangement.spacedBy(9.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            StatusLabel("${thread.children?.running ?: 0}", palette.pending) { Icon(Symbol.Bot, 11f) }
            StatusLabel(Time.elapsed(thread.monitoringSince, everySecond()), palette.mutedForeground) { Icon(Symbol.CircleDotDashed, 10f) }
        }
        thread.working -> Row(horizontalArrangement = Arrangement.spacedBy(9.dp), verticalAlignment = Alignment.CenterVertically) {
            val agents = maxOf(thread.agents, thread.children?.running ?: 0)
            if (agents > 0) {
                val working = if (agents == 1) "1 agent is working" else "$agents agents are working"
                StatusLabel("$agents", palette.pending, Modifier.semantics { contentDescription = working }) { Icon(Symbol.Bot, 11f) }
            }
            StatusLabel(Time.elapsed(thread.updatedAt, everySecond()), palette.process) { Icon(Symbol.CircleDashed, 10f) }
        }
        stage != null -> StatusLabel(stage.label, palette.process) { Spinner(11f) }
        thread.monitoring -> StatusLabel(Time.elapsed(thread.monitoringSince, everySecond()), palette.foreground) { Icon(Symbol.Eye, 11f) }
        interruption != null -> StatusLabel(capitalized(interruption.word), palette.warning, Modifier.semantics { contentDescription = interruption.detail() }) {
            Icon(interruption.symbol, 11f)
        }
        thread.unread -> StatusLabel("Finished", palette.success) { Icon(Symbol.Flag, 11f) }
        else -> Text(Time.ago(thread.updatedAt, AgoClock.now), size = 11f, color = palette.mutedStrongerForeground)
    }
}

private const val readOnlyHelp = "Its server is unlinked, so this thread can only be read or continued on another server."

private fun capitalized(text: String): String = text.split(" ").joinToString(" ") { word -> word.replaceFirstChar { it.uppercase() } }

@Composable
private fun StatusLabel(text: String, color: Color, modifier: Modifier = Modifier, icon: @Composable () -> Unit) {
    CompositionLocalProvider(LocalForeground provides color) {
        Row(modifier, horizontalArrangement = Arrangement.spacedBy(3.dp), verticalAlignment = Alignment.CenterVertically) {
            icon()
            Text(text, style = Theme.uiDigits(11f, FontWeight.Medium), color = color, maxLines = 1)
        }
    }
}

/** What a sidebar line starts with, against its edge, in a column that lines up the words after it. */
@Composable
fun SidebarMark(content: @Composable () -> Unit) {
    Box(Modifier.width(9.dp), contentAlignment = Alignment.CenterStart) { content() }
}

/** The mark of a sidebar line like Done's, a step quieter than its `ShelfTitle`. */
@Composable
fun ShelfMark(lit: Boolean, content: @Composable () -> Unit) {
    val palette = Theme.colors
    val color = if (lit || LocalRow.current != null) palette.mutedForeground else palette.mutedStrongerForeground
    CompositionLocalProvider(LocalForeground provides color) { SidebarMark(content) }
}

/** Opens or closes a sidebar line's list. */
@Composable
fun SidebarChevron(expanded: Boolean) {
    val angle by animateFloatAsState(if (expanded) 90f else 0f, label = "chevron")
    ShelfMark(lit = expanded) { Icon(Symbol.ChevronRight, 10f, Modifier.rotate(angle), trimmed = true) }
}

/** The name of a sidebar line like Done's: muted while it is closed and not lit. */
@Composable
fun ShelfTitle(title: String, expanded: Boolean) {
    val palette = Theme.colors
    val color = if (expanded || LocalRow.current != null) palette.foreground else palette.mutedForeground
    Text(title, size = 12f, weight = FontWeight.Medium, color = color, maxLines = 1)
}

/** The offer to undo marking threads done, held while a finger or the pointer is on it. */
@Composable
fun UndoRow(notice: UndoNotice, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val palette = Theme.colors
    DisposableEffect(Unit) { onDispose { store.holdUndo(false) } }
    Column(modifier.holdsUndo(store)) {
        ThemeDivider()
        HighlightButton(onClick = { store.performUndo() }, radius = 0.dp, contentAlignment = Alignment.CenterStart) {
            Row(
                Modifier.fillMaxWidth().height(doneRowHeight + 8.dp).padding(horizontal = sidebarEdge),
                horizontalArrangement = Arrangement.spacedBy(7.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                ShelfMark(lit = false) { Icon(Symbol.Undo2, 10f, Modifier.offset(x = (-2).dp), trimmed = true) }
                ShelfTitle("Undo", expanded = false)
                Spacer(Modifier.weight(1f))
                CompositionLocalProvider(LocalForeground provides palette.mutedStrongerForeground) {
                    Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
                        Icon(Symbol.Check, 10f)
                        Text(notice.text, size = 11f, maxLines = 1)
                    }
                }
            }
        }
    }
}

/** Every server in one line, which opens into a line for each: the worst of their states, how they are reached and the slowest round trip. */
@Composable
fun AllServersLine(servers: List<Server>, expanded: Boolean, modifier: Modifier = Modifier) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(7.dp), verticalAlignment = Alignment.CenterVertically) {
        StateDot(servers.worstTint(Theme.colors))
        ShelfTitle("Servers", expanded)
        Spacer(Modifier.weight(1f))
        Text(servers.reach, style = Theme.uiDigits(11f), color = Theme.colors.mutedStrongerForeground, maxLines = 1)
    }
}

@Composable
fun StateDot(tint: Color) {
    Box(Modifier.size(7.dp).background(tint, CircleShape))
}

/** The worst state among the servers: one out of reach, then one connecting. */
fun List<Server>.worstTint(palette: Palette): Color = when {
    any { it.state == Server.State.Disconnected || it.state == Server.State.Refused } -> palette.destructive
    any { it.state == Server.State.Connecting } -> palette.pending
    else -> palette.success
}

/** How the connected ones are reached, "Mixed" if not all alike, and their slowest round trip. */
val List<Server>.reach: String
    get() {
        val connected = filter { it.state == Server.State.Connected }
        val paths = connected.mapNotNull { it.path }.toSet()
        val path = if (paths.size > 1) "Mixed" else paths.firstOrNull()?.let(::capitalized)
        val slowest = connected.mapNotNull { it.rttMs }.maxOrNull()?.let { "$it ms" }
        return listOfNotNull(path, slowest).joinToString(" · ")
    }

/** A server and how the client reaches it, or its update. */
@Composable
fun ServerLine(server: Server, modifier: Modifier = Modifier) {
    val palette = Theme.colors
    val detail = when (server.state) {
        Server.State.Connected -> {
            val path = server.path?.let(::capitalized) ?: "Connected"
            server.rttMs?.let { "$path · $it ms" } ?: path
        }
        Server.State.Connecting -> "Connecting"
        Server.State.Disconnected -> "Offline"
        Server.State.Refused -> "Refused"
    }
    Row(
        modifier.semantics(mergeDescendants = true) { contentDescription = "${server.name}, ${server.error ?: detail}" },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        SidebarMark { StateDot(server.stateTint ?: palette.success) }
        Text(server.name, Modifier.weight(1f).padding(start = 7.dp, end = 4.dp), size = 12f, weight = FontWeight.Medium, maxLines = 1)
        ServerUpdateStatus(server) {
            Text(detail, style = Theme.uiDigits(11f), color = palette.mutedStrongerForeground, maxLines = 1)
        }
    }
}

/** Holds the offer to undo while a finger presses it or the pointer is over it. */
private fun Modifier.holdsUndo(store: AppStore): Modifier = pointerInput(store) {
    awaitPointerEventScope {
        while (true) {
            val event = awaitPointerEvent(PointerEventPass.Initial)
            val mouse = event.changes.any { it.type == PointerType.Mouse }
            when (event.type) {
                PointerEventType.Enter, PointerEventType.Press -> store.holdUndo(true)
                PointerEventType.Exit -> store.holdUndo(false)
                PointerEventType.Release -> if (!mouse && event.changes.none { it.pressed }) store.holdUndo(false)
            }
        }
    }
}

/** A row that opens a menu where it is held: `content` is given the modifier that finds where, and what opens it. */
@Composable
fun WithMenu(items: (MenuBuilder.() -> Unit)?, content: @Composable (Modifier, (() -> Unit)?) -> Unit) {
    if (items == null) return content(Modifier, null)
    val menu = rememberMenuState()
    Box {
        content(Modifier.pressPosition(menu)) { menu.openAtPress() }
        MenuPopup(menu, items)
    }
}
