package app.motile.sidebar

import app.motile.core.AppUpdater
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.snapping.SnapPosition
import androidx.compose.foundation.gestures.snapping.rememberSnapFlingBehavior
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.zIndex
import app.motile.DemoHooks
import app.motile.LocalStore
import app.motile.core.AppStore
import app.motile.core.PanelPage
import app.motile.core.Preferences
import app.motile.core.SearchResult
import app.motile.core.Selection
import app.motile.core.ThreadInfo
import app.motile.core.isSearchingThreads
import app.motile.core.jumpHint
import app.motile.core.jumpThreads
import app.motile.core.openSearchResult
import app.motile.core.searchResults
import app.motile.layout.KeyboardFocus
import app.motile.layout.LocalDrawer
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.ShadowSize
import app.motile.theme.Theme
import app.motile.theme.opacity
import app.motile.theme.shadow
import app.motile.ui.AccountMenu
import app.motile.ui.ActionButton
import app.motile.ui.CloseModal
import app.motile.ui.InputField
import app.motile.ui.MenuBuilder
import app.motile.ui.SheetSurface
import app.motile.ui.Answer
import app.motile.ui.Appearing
import app.motile.ui.Asking
import app.motile.ui.ButtonVariant
import app.motile.ui.Confirmation
import app.motile.ui.ConfirmationDialog
import app.motile.ui.DimButton
import app.motile.ui.GlassButton
import app.motile.ui.GlassGroup
import app.motile.ui.HighlightButton
import app.motile.ui.Highlight
import app.motile.ui.Icon
import app.motile.ui.LogoMark
import app.motile.ui.MenuPopup
import app.motile.ui.Text
import app.motile.ui.ThemeDivider
import app.motile.ui.fadesHiddenEdges
import app.motile.ui.glassButton
import app.motile.ui.pressPosition
import app.motile.ui.rememberMenuState
import kotlinx.coroutines.flow.drop

/** What the sidebar lists, each with an id of its own in the list. */
private sealed interface SidebarItem {
    val id: String

    data object Drafts : SidebarItem {
        override val id = "drafts"
    }

    data class Active(val thread: ThreadInfo) : SidebarItem {
        override val id: String get() = thread.id
    }

    data class Result(val result: SearchResult) : SidebarItem {
        override val id: String get() = "result:${result.id}"
    }

    data object Empty : SidebarItem {
        override val id = "empty"
    }

    data object NewThread : SidebarItem {
        override val id = "newThread"
    }

    data object DoneHeader : SidebarItem {
        override val id = "done"
    }

    /** A done thread's row has an id of its own, or the list keeps its active row for it. */
    data class Done(val thread: ThreadInfo) : SidebarItem {
        override val id: String get() = "done:${thread.id}"
    }

    val activeThread: ThreadInfo? get() = (this as? Active)?.thread

    val rowThread: ThreadInfo?
        get() = when (this) {
            is Active -> thread
            is Done -> thread
            is Result -> result.threadInfo
            else -> null
        }
}

/**
 * The sidebar: the drafts, every active thread on every server, the ones marked done, the servers
 * and the account. On a phone it lies under the thread (`underThread`), which comes back over it
 * when something is opened; in a wide window it is a column beside it.
 */
@Composable
fun SidebarScreen(underThread: Boolean) {
    val store = LocalStore.current
    val drawer = LocalDrawer.current
    val focus = LocalFocusManager.current
    var doneExpanded by remember { mutableStateOf(Preferences.bool("sidebar.doneExpanded")) }
    var serversExpanded by remember { mutableStateOf(Preferences.bool("sidebar.serversExpanded")) }
    var renaming by remember { mutableStateOf<ThreadInfo?>(null) }
    var deleting by remember { mutableStateOf<ThreadInfo?>(null) }
    /** Why a swipe couldn't mark a thread done: its agent is still at it. */
    var busy by remember { mutableStateOf<String?>(null) }
    /** The row a swipe has slid aside, by its id in the list. */
    var swiped by remember { mutableStateOf<String?>(null) }
    /** The search's result the arrow keys are on, which the keyboard's action key opens. */
    val highlighted = remember { Highlight() }
    val searchFocus = remember { FocusRequester() }
    var searchFocused by remember { mutableStateOf(false) }
    val list = rememberLazyListState()
    val reorder = remember(list) { Reorder(list) }
    val peek = remember { ThreadPeek() }

    val searching = store.isSearchingThreads
    val active = if (searching) emptyList() else store.activeThreads
    val done = if (searching) emptyList() else store.doneThreads
    val results = store.searchResults
    val highlightedID = results.getOrNull(highlighted.index)?.id
    val items = if (searching) searchItems(results) else items(store, active, done, doneExpanded)
    val shown = reorder.arranged(items) { it.id }
    val places = if (store.showsJumpHints) jumpPlaces(store.jumpThreads) else emptyMap()

    fun setDoneExpanded(expanded: Boolean) {
        doneExpanded = expanded
        Preferences.set("sidebar.doneExpanded", expanded)
    }

    fun setServersExpanded(expanded: Boolean) {
        serversExpanded = expanded
        Preferences.set("sidebar.serversExpanded", expanded)
    }

    /** Puts the sidebar away where it lies under the thread. */
    fun showThread() {
        if (!underThread) return
        store.rightPanel.isOpen = false
        drawer.isOpen = false
    }

    fun open(selection: Selection) {
        store.select(selection)
        showThread()
    }

    /** Opens what the search found, ends the search and puts the sidebar away. */
    fun openSearchResult(result: SearchResult) {
        focus.clearFocus()
        store.openSearchResult(result)
        showThread()
    }

    fun markDone(thread: ThreadInfo) {
        if (thread.unfinished) {
            busy = when {
                thread.running -> "Still working"
                thread.monitoring -> "Still monitoring"
                else -> "Still waiting on its agents"
            }
            return
        }
        store.setDone(listOf(thread.id), done = true, fromSidebar = true)
    }

    fun swipe(row: String) = SwipeState(swiped == row) { open ->
        if (open) swiped = row else if (swiped == row) swiped = null
    }

    fun beginRename(thread: ThreadInfo) {
        renaming = thread
    }

    LaunchedEffect(store) {
        snapshotFlow { store.threadsShown }.drop(1).collect {
            peek.hide()
            if (underThread) drawer.isOpen = false
        }
    }
    LaunchedEffect(store) {
        snapshotFlow { store.serversExpandedRequests }.drop(1).collect { serversExpanded = true }
    }
    LaunchedEffect(store.revealedThreadID) {
        val revealed = store.revealedThreadID ?: return@LaunchedEffect
        if (store.doneThreads.any { it.id == revealed }) setDoneExpanded(true)
    }
    val target = if (searching) highlightedID?.let { "result:$it" } else items.firstOrNull { it.rowThread?.id == store.revealedThreadID }?.id
    val targetIndex = target?.let { id -> shown.indexOfFirst { it.id == id } } ?: -1
    val targetToTop = done.any { it.id == store.revealedThreadID }
    LaunchedEffect(target) {
        if (targetIndex < 0) return@LaunchedEffect
        reveal(list, targetIndex, targetToTop)
    }
    LaunchedEffect(list.isScrollInProgress) {
        if (list.isScrollInProgress) swiped = null
    }
    val resultIDs = results.map { it.id }
    val lastResults = remember { arrayOf(resultIDs) }
    LaunchedEffect(resultIDs) {
        val kept = lastResults[0].take(highlighted.index + 1)
        if (resultIDs.take(kept.size) != kept) highlighted.index = 0
        lastResults[0] = resultIDs
    }
    DisposableEffect(searchFocus) {
        val hook: (String) -> Unit = { text ->
            runCatching { searchFocus.requestFocus() }
            store.threadSearch = text
        }
        DemoHooks.search = hook
        onDispose {
            if (DemoHooks.search === hook) DemoHooks.search = null
            if (searchFocused) KeyboardFocus.editable = false
        }
    }
    ReorderScrolling(reorder, list)

    Column(
        Modifier
            .fillMaxSize()
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Bottom + WindowInsetsSides.Start))
            .pointerInput(Unit) { detectTapGestures { Platform.endEditing() } },
    ) {
        Header(store)
        LazyColumn(Modifier.weight(1f).fillMaxWidth(), state = list, contentPadding = PaddingValues(bottom = 12.dp)) {
            items(shown, key = { it.id }, contentType = { it::class }) { item ->
                val lifted = reorder.key == item.id
                val placed = if (lifted) {
                    Modifier.zIndex(1f).graphicsLayer { translationY = reorder.offset }
                } else {
                    Modifier.animateItem()
                }
                Box(placed) {
                    when (item) {
                        SidebarItem.Drafts -> DraftRows(swipe = { swipe("draft:$it") }, open = ::open)
                        is SidebarItem.Active -> {
                            val thread = item.thread
                            val movable = !searching && active.size > 1 && store.moves(thread)
                            val row = @Composable {
                                ThreadRowWithMenu(
                                    store, thread, places[thread.id], lifted, movable, peek, reorder,
                                    keys = { shown.map { it.id } },
                                    movableKeys = { items.mapNotNull { row -> row.activeThread?.takeIf { store.moves(it) }?.id }.toSet() },
                                    put = { index -> store.move(thread, index, items.map { it.activeThread }) },
                                    rename = ::beginRename, delete = { deleting = it }, open = ::open,
                                )
                            }
                            if (store.onRemovedServer(thread)) {
                                row()
                            } else {
                                RowSwipe(
                                    Symbol.Check, "Mark Done", tint = if (thread.unfinished) Theme.colors.mutedForeground else Theme.colors.success, size = 36.dp,
                                    isOpen = swiped == item.id, onOpenChange = swipe(item.id).onOpenChange, action = { markDone(thread) }, leaves = !thread.unfinished,
                                    content = row,
                                )
                            }
                        }
                        is SidebarItem.Result -> {
                            val result = item.result
                            val thread = result.threadInfo
                            val menu: (MenuBuilder.() -> Unit)? = thread?.takeIf { !store.onRemovedServer(it) }?.let { found -> { threadMenu(store, found, ::beginRename) { deleting = it } } }
                            WithMenu(menu) { modifier, openMenu ->
                                SearchResultRow(
                                    result, highlighted = result.id == highlightedID, modifier = modifier.peeks(thread?.id, peek),
                                    jump = places[result.id]?.let(store::jumpHint), onLongClick = openMenu, open = ::openSearchResult,
                                )
                            }
                        }
                        SidebarItem.Empty -> Box(Modifier.fillMaxWidth().padding(horizontal = sidebarEdge, vertical = 10.dp)) {
                            if (searching) {
                                SearchEmptyLine(13f)
                            } else {
                                Text(if (done.isEmpty()) "No threads yet" else "No active threads", size = 13f, color = Theme.colors.mutedStrongerForeground)
                            }
                        }
                        SidebarItem.NewThread -> NewThreadRow(Modifier.fillMaxWidth().height(lineHeight))
                        SidebarItem.DoneHeader -> DoneHeader(done.size, doneExpanded) { setDoneExpanded(!doneExpanded) }
                        is SidebarItem.Done -> {
                            val thread = item.thread
                            val readOnly = store.onRemovedServer(thread)
                            val row = @Composable {
                                WithMenu(if (readOnly) null else ({ threadMenu(store, thread, ::beginRename) { deleting = it } })) { modifier, openMenu ->
                                    DoneRow(
                                        thread, store.projectsByID[thread.projectID], selected = store.selection == Selection.Thread(thread.id),
                                        modifier = modifier.fillMaxWidth().height(lineHeight).peeks(thread.id, peek), onLongClick = openMenu, open = ::open,
                                    )
                                }
                            }
                            if (readOnly) {
                                row()
                            } else {
                                RowSwipe(
                                    Symbol.Undo2, "Mark Undone", tint = Theme.colors.mutedForeground, size = 28.dp,
                                    isOpen = swiped == item.id, onOpenChange = swipe(item.id).onOpenChange,
                                    action = { store.setDone(listOf(thread.id), done = false) },
                                    content = row,
                                )
                            }
                        }
                    }
                }
            }
        }
        val undo = store.undo
        val lastUndo = remember { arrayOf(undo) }
        if (undo != null) lastUndo[0] = undo
        Appearing(undo != null) { lastUndo[0]?.let { UndoRow(it) } }
        Footer(
            store, searching = searchFocused || store.threadSearch.isNotEmpty(), searchFocus, highlighted,
            serversExpanded = serversExpanded, toggleServers = { setServersExpanded(!serversExpanded) },
            onFocusChange = { focused ->
                searchFocused = focused
                KeyboardFocus.editable = focused
            },
            openHighlighted = { store.searchResults.getOrNull(highlighted.index)?.let(::openSearchResult) },
            closeSearch = {
                store.threadSearch = ""
                focus.clearFocus()
                Platform.endEditing()
            },
        )
    }
    ThreadPeekCard(peek)
    busy?.let { title ->
        ConfirmationDialog(Confirmation(title, "Stop it first, then mark it done.", dismissal = "OK")) { busy = null }
    }
    renaming?.let { thread -> RenameDialog(thread) { renaming = null } }
    Asking(deleting, onDismiss = { deleting = null }) { thread ->
        Confirmation(
            "Delete “${thread.title}”?", store.deletionNote(thread),
            answers = listOf(Answer("Delete", ButtonVariant.Danger) { store.delete(thread) }),
        )
    }
}

/** The drafts, the active threads, and under them the threads marked done: a line that opens into their list. */
private fun items(store: AppStore, active: List<ThreadInfo>, done: List<ThreadInfo>, expanded: Boolean): List<SidebarItem> = buildList {
    add(SidebarItem.Drafts)
    active.forEach { add(SidebarItem.Active(it)) }
    if (active.isEmpty()) add(SidebarItem.Empty)
    if (store.offersNewThread(store.listedDrafts, active)) add(SidebarItem.NewThread)
    if (done.isEmpty()) return@buildList
    add(SidebarItem.DoneHeader)
    if (expanded) done.forEach { add(SidebarItem.Done(it)) }
}

/** What the search found, or the line that says it found nothing. */
private fun searchItems(results: List<SearchResult>): List<SidebarItem> =
    if (results.isEmpty()) listOf(SidebarItem.Empty) else results.map { SidebarItem.Result(it) }

/** Where each thread is among those the jump keys go through, the first place it has. */
private fun jumpPlaces(threads: List<ThreadInfo>): Map<String, Int> = buildMap {
    threads.forEachIndexed { index, thread -> putIfAbsent(thread.id, index) }
}

/** Scrolls the row into view, or to the top of the list. */
private suspend fun reveal(list: LazyListState, index: Int, toTop: Boolean) {
    if (toTop) return list.animateScrollToItem(index)
    val info = list.layoutInfo
    val visible = info.visibleItemsInfo.firstOrNull { it.index == index } ?: return list.animateScrollToItem(index)
    val below = visible.offset + visible.size - info.viewportEndOffset
    val above = info.viewportStartOffset - visible.offset
    when {
        below > 0 -> list.animateScrollBy(below.toFloat())
        above > 0 -> list.animateScrollBy(-above.toFloat())
    }
}

@Composable
private fun Header(store: AppStore) {
    Row(
        Modifier.fillMaxWidth().padding(start = sidebarEdge, end = sidebarRowInset + 2.dp, top = 6.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        LogoMark(motileMark, 24f, Theme.colors.foreground, Modifier.semantics { contentDescription = "Motile" })
        Spacer(Modifier.weight(1f))
        GlassButton(Symbol.Command, "Commands", onClick = { store.openPanel(PanelPage.Commands) }, modifier = Modifier.size(42.dp))
    }
}

/** An active thread's row, with its menu where it is held, and lifted by a hold to be moved where it moves. */
@Composable
private fun ThreadRowWithMenu(
    store: AppStore,
    thread: ThreadInfo,
    place: Int?,
    lifted: Boolean,
    movable: Boolean,
    peek: ThreadPeek,
    reorder: Reorder,
    keys: () -> List<String>,
    movableKeys: () -> Set<String>,
    put: (Int) -> Unit,
    rename: (ThreadInfo) -> Unit,
    delete: (ThreadInfo) -> Unit,
    open: (Selection) -> Unit,
) {
    val menu = rememberMenuState()
    val readOnly = store.onRemovedServer(thread)
    Box {
        if (lifted) {
            val shape = RoundedCornerShape(Radius.md)
            Box(Modifier.matchParentSize().padding(rowMargin).shadow(ShadowSize.Lg, shape, Opacity.ShadowStronger))
        }
        ThreadRow(
            thread,
            store.projectsByID[thread.projectID]?.seen(thread),
            selected = store.selection == Selection.Thread(thread.id),
            modifier = Modifier.pressPosition(menu).peeks(thread.id, peek),
            // Inside the press, which takes every move after a hold for itself.
            contentModifier = Modifier.reorders(
                movable, thread.id, reorder, keys, movableKeys, openMenu = { menu.openAtPress() }, closeMenu = { menu.close() }, put = put,
            ),
            jump = place?.let(store::jumpHint),
            lit = lifted,
            // A hold on a row that moves lifts it, and its menu comes up from there.
            onLongClick = when {
                movable -> ({})
                readOnly -> null
                else -> ({ menu.openAtPress() })
            },
            open = open,
        )
        if (!readOnly) MenuPopup(menu) { threadMenu(store, thread, rename, delete) }
    }
}

@Composable
private fun DoneHeader(count: Int, expanded: Boolean, toggle: () -> Unit) {
    DimButton(onClick = toggle, modifier = Modifier.fillMaxWidth().padding(top = 8.dp)) {
        Row(
            Modifier.fillMaxWidth().height(lineHeight).padding(horizontal = sidebarEdge),
            horizontalArrangement = Arrangement.spacedBy(7.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SidebarChevron(expanded)
            ShelfTitle("Done", expanded)
            Spacer(Modifier.weight(1f))
            Text("$count", style = Theme.uiDigits(11f), color = Theme.colors.mutedStrongerForeground)
        }
    }
}

/**
 * The servers and how the client reaches them, and under them the account, which leads to the
 * settings and the usage, the search and the way to a new thread. While it searches, the search
 * takes the line with a button that closes it.
 */
@Composable
private fun Footer(
    store: AppStore,
    searching: Boolean,
    searchFocus: FocusRequester,
    highlighted: Highlight,
    serversExpanded: Boolean,
    toggleServers: () -> Unit,
    onFocusChange: (Boolean) -> Unit,
    openHighlighted: () -> Unit,
    closeSearch: () -> Unit,
) {
    val border = Theme.colors.border
    val appear = scaleIn() + fadeIn()
    val leave = scaleOut() + fadeOut()
    val canStart = store.projects.isNotEmpty() || store.noProjects.isNotEmpty()
    Column(
        Modifier
            .fillMaxWidth()
            .drawBehind { drawLine(border, Offset(0f, 0f), Offset(size.width, 0f), 1.dp.toPx()) }
            .padding(bottom = 8.dp),
    ) {
        // A new version of the client, where the sidebar ends, as on the Mac.
        val update = store.updater.state
        if (update != AppUpdater.State.Idle && update != AppUpdater.State.Checking && update != AppUpdater.State.UpToDate) {
            AppUpdateRow(store.updater, Modifier.padding(horizontal = sidebarEdge, vertical = 8.dp))
            ThemeDivider()
        }
        if (store.servers.isNotEmpty()) {
            Servers(store, serversExpanded, toggleServers)
            ThemeDivider()
        }
        GlassGroup(8.dp, Modifier.fillMaxWidth().padding(start = sidebarEdge, end = sidebarEdge, top = 16.dp)) {
            Appearing(!searching, enter = appear, exit = leave) { AccountMenu() }
            SearchField(
                store.threadSearch, { store.threadSearch = it },
                Modifier
                    .weight(1f)
                    .height(46.dp)
                    .glassButton(CircleShape)
                    .padding(horizontal = 6.dp)
                    .steersSearch(store, highlighted),
                bare = true, clearable = false, focusRequester = searchFocus, onFocusChange = onFocusChange, onSubmit = openHighlighted,
            )
            Appearing(searching, enter = appear, exit = leave) { GlassButton(Symbol.X, "Close search", onClick = closeSearch) }
            Appearing(!searching, enter = appear, exit = leave) {
                GlassButton("New thread", onClick = { store.newThread() }, modifier = Modifier.opacity(Opacity.Disabled, on = !canStart), enabled = canStart) {
                    Icon(Symbol.SquarePen, 15f)
                }
            }
        }
    }
}

/** One line for every server that opens into a line for each, or the one server's line. */
@Composable
private fun Servers(store: AppStore, expanded: Boolean, toggle: () -> Unit) {
    val servers = store.servers
    if (servers.size == 1) {
        ServerLine(servers[0], Modifier.fillMaxWidth().height(lineHeight).padding(horizontal = sidebarEdge))
        return
    }
    HighlightButton(onClick = toggle, radius = 0.dp, contentAlignment = Alignment.CenterStart) {
        Row(
            Modifier.fillMaxWidth().height(lineHeight).padding(horizontal = sidebarEdge),
            horizontalArrangement = Arrangement.spacedBy(7.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SidebarChevron(expanded)
            AllServersLine(servers, expanded, Modifier.weight(1f))
        }
    }
    if (expanded) ServerLines(store)
}

/** A line for each server, under the line of them all. A long list scrolls, and rests between two lines. */
@Composable
private fun ServerLines(store: AppStore) {
    val servers = store.servers
    val state = rememberLazyListState()
    LazyColumn(
        Modifier.fillMaxWidth().height(lineHeight * minOf(servers.size, MOST_SERVERS_SHOWN)).fadesHiddenEdges(state),
        state = state,
        flingBehavior = rememberSnapFlingBehavior(state, SnapPosition.Start),
    ) {
        items(servers, key = { it.id }) { server ->
            ServerLine(server, Modifier.fillMaxWidth().height(lineHeight).padding(horizontal = sidebarEdge))
        }
    }
}

private const val MOST_SERVERS_SHOWN = 5

/** Renames a thread, as iOS's alert with a field does. */
@Composable
private fun RenameDialog(thread: ThreadInfo, onDismiss: () -> Unit) {
    val store = LocalStore.current
    val palette = Theme.colors
    var title by remember(thread.id) { mutableStateOf(thread.title) }
    val field = remember { FocusRequester() }
    val shape = RoundedCornerShape(Radius.xl)
    fun rename() {
        if (title.isBlank()) return
        store.rename(thread, title)
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
                Text("Rename thread", size = 15f, weight = FontWeight.SemiBold)
                InputField(
                    "Title", title, { title = it }, Modifier.fillMaxWidth(), size = ControlSize.Large, focusRequester = field,
                    onSubmit = ::rename,
                )
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                    ActionButton("Cancel", size = ControlSize.Large, onClick = onDismiss)
                    ActionButton("Rename", variant = ButtonVariant.Primary, size = ControlSize.Large, disabled = title.isBlank(), onClick = ::rename)
                }
            }
        }
    }
    LaunchedEffect(Unit) { runCatching { field.requestFocus() } }
    DisposableEffect(Unit) { onDispose { KeyboardFocus.editable = false } }
}
