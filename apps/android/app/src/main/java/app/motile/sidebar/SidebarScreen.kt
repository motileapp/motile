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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListLayoutInfo
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.withFrameNanos
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
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
import app.motile.core.Server
import app.motile.core.ThreadInfo
import app.motile.core.ThreadSection
import app.motile.core.isSearchingThreads
import app.motile.core.jumpHint
import app.motile.core.jumpThreads
import app.motile.core.openSearchResult
import app.motile.core.searchResults
import app.motile.layout.KeyboardFocus
import app.motile.layout.LocalDrawer
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.IconSize
import app.motile.theme.ControlSize
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.ShadowSize
import app.motile.theme.Theme
import app.motile.theme.opacity
import app.motile.theme.shadow
import app.motile.ui.AccountMenu
import app.motile.ui.beside
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

    data class Header(val section: ThreadSection, val count: Int) : SidebarItem {
        override val id: String get() = "header:${section.raw}"
    }

    /** A slim row has an id of its own, or the list keeps the thread's active row for it. */
    data class Slim(val thread: ThreadInfo) : SidebarItem {
        override val id: String get() = "slim:${thread.id}"
    }

    /** The room that holds the sections at the bottom of the list while everything fits. */
    data object Gap : SidebarItem {
        override val id = "gap"
    }

    /** The line under an open section that has another below it. */
    data class SectionEnd(val section: ThreadSection) : SidebarItem {
        override val id: String get() = "end:${section.raw}"
    }

    val activeThread: ThreadInfo? get() = (this as? Active)?.thread

    val rowThread: ThreadInfo?
        get() = when (this) {
            is Active -> thread
            is Slim -> thread
            is Result -> result.threadInfo
            else -> null
        }
}

/**
 * The sidebar: the drafts, every active thread on every server, the working, the snoozed and the
 * archived ones, the servers and the account. On a phone it lies under the thread (`underThread`), which comes back over it
 * when something is opened; in a wide window it is a column beside it.
 */
@Composable
fun SidebarScreen(underThread: Boolean) {
    val store = LocalStore.current
    val drawer = LocalDrawer.current
    val focus = LocalFocusManager.current
    var archivedExpanded by remember { mutableStateOf(Preferences.bool("sidebar.doneExpanded")) }
    var workingExpanded by remember { mutableStateOf(Preferences.optionalBool("sidebar.workingExpanded") ?: true) }
    var snoozedExpanded by remember { mutableStateOf(Preferences.bool("sidebar.snoozedExpanded")) }
    var serversExpanded by remember { mutableStateOf(Preferences.bool("sidebar.serversExpanded")) }
    var renaming by remember { mutableStateOf<ThreadInfo?>(null) }
    var deleting by remember { mutableStateOf<ThreadInfo?>(null) }
    /** Why a swipe couldn't archive a thread: its agent is still at it. */
    var busy by remember { mutableStateOf<String?>(null) }
    /** The row a swipe has slid aside, by its id in the list. */
    var swiped by remember { mutableStateOf<String?>(null) }
    /** The search's result the arrow keys are on, which the keyboard's action key opens. */
    val highlighted = remember { Highlight() }
    val searchFocus = remember { FocusRequester() }
    var searchFocused by remember { mutableStateOf(false) }
    val list = rememberLazyListState()
    val gap by remember(list) { derivedStateOf { gapHeight(list.layoutInfo) } }
    val reorder = remember(list) { Reorder(list) }
    val peek = remember { ThreadPeek() }

    val searching = store.isSearchingThreads
    val active = if (searching) emptyList() else store.activeThreads
    val sections = if (searching) emptyList() else listOf(
        ThreadSection.Working to store.workingThreads, ThreadSection.Snoozed to store.snoozedThreads, ThreadSection.Archived to store.archivedThreads,
    )
    fun expanded(section: ThreadSection) = when (section) {
        ThreadSection.Working -> workingExpanded
        ThreadSection.Snoozed -> snoozedExpanded
        else -> archivedExpanded
    }
    val results = store.searchResults
    val highlightedID = results.getOrNull(highlighted.index)?.id
    val items = if (searching) searchItems(results, store.messageSearch.searching) else items(store, active, sections, ::expanded)
    val shown = reorder.arranged(items) { it.id }
    val places = if (store.showsJumpHints) jumpPlaces(store.jumpThreads) else emptyMap()

    fun setExpanded(section: ThreadSection, expanded: Boolean) {
        when (section) {
            ThreadSection.Working -> workingExpanded = expanded
            ThreadSection.Snoozed -> snoozedExpanded = expanded
            else -> archivedExpanded = expanded
        }
        Preferences.set(if (section == ThreadSection.Archived) "sidebar.doneExpanded" else "sidebar.${section.raw}Expanded", expanded)
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

    fun archive(thread: ThreadInfo) {
        if (thread.unfinished) {
            busy = when {
                thread.running -> "Still working"
                thread.monitoring -> "Still monitoring"
                else -> "Still waiting on its agents"
            }
            return
        }
        store.setArchived(listOf(thread.id), true, fromSidebar = true)
    }

    val palette = Theme.colors

    /** What a swipe offers on a thread's row: an active one is archived, its tick first, or snoozed; a working one only snoozed; a snoozed one woken and an archived one brought back. */
    fun swipeActions(thread: ThreadInfo): List<SwipeAction> {
        val snooze = if (store.snoozes(thread)) {
            listOf(SwipeAction(Symbol.AlarmClock, "Snooze", palette.warning, menu = { snoozeItems(store, listOf(thread.id), fromSidebar = true) }))
        } else {
            emptyList()
        }
        return when (thread.section) {
            ThreadSection.Active -> {
                val tint = if (thread.unfinished) palette.mutedForeground else palette.success
                listOf(SwipeAction(Symbol.Check, "Archive", tint, leaves = !thread.unfinished, perform = { archive(thread) })) + snooze
            }
            ThreadSection.Working -> snooze
            ThreadSection.Snoozed -> listOf(SwipeAction(Symbol.AlarmClockOff, "Wake", palette.warning, perform = { store.wake(listOf(thread.id)) }))
            ThreadSection.Archived -> listOf(
                SwipeAction(Symbol.Undo2, "Unarchive", palette.mutedForeground, perform = { store.setArchived(listOf(thread.id), false) }),
            )
        }
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
        val revealed = store.revealedThreadID?.let(store::anyThread) ?: return@LaunchedEffect
        if (revealed.section != ThreadSection.Active) setExpanded(revealed.section, true)
    }
    LaunchedEffect(store.expandStep) {
        val step = store.expandStep ?: return@LaunchedEffect
        val section = ThreadSection.entries.firstOrNull { it.raw == step.removePrefix("-") } ?: return@LaunchedEffect
        setExpanded(section, !step.startsWith("-"))
    }
    LaunchedEffect(store.swipeStep) {
        val thread = store.swipeStep?.let(store::anyThread) ?: return@LaunchedEffect
        swiped = if (thread.section == ThreadSection.Active) thread.id else "slim:${thread.id}"
    }
    val target = if (searching) highlightedID?.let { "result:$it" } else items.firstOrNull { it.rowThread?.id == store.revealedThreadID }?.id
    val targetIndex = target?.let { id -> shown.indexOfFirst { it.id == id } } ?: -1
    val revealedSection = sections.firstOrNull { (_, threads) -> threads.any { it.id == store.revealedThreadID } }?.first
    val anchorIndex = revealedSection?.let { section -> shown.indexOfFirst { it.id == "header:${section.raw}" } } ?: -1
    LaunchedEffect(target) {
        if (targetIndex < 0) return@LaunchedEffect
        reveal(list, targetIndex, !searching && store.revealedThreadID != null, anchorIndex.takeIf { it in 0..targetIndex })
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
    LaunchedEffect(store.searchFocusRequests) {
        if (store.searchFocusRequests == 0) return@LaunchedEffect
        if (underThread) drawer.isOpen = true
        withFrameNanos {}
        runCatching { searchFocus.requestFocus() }
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
                                RowSwipe(swipeActions(thread), isOpen = swiped == item.id, onOpenChange = swipe(item.id).onOpenChange, content = row)
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
                                Text(if (store.allThreads.isEmpty()) "No threads yet" else "No active threads", size = 13f, color = Theme.colors.mutedStrongerForeground)
                            }
                        }
                        SidebarItem.NewThread -> NewThreadRow(Modifier.fillMaxWidth().height(lineHeight))
                        SidebarItem.Gap -> Spacer(Modifier.height(with(LocalDensity.current) { gap.toDp() }.coerceAtLeast(1.dp)))
                        is SidebarItem.SectionEnd -> ListDivider()
                        is SidebarItem.Header -> SectionHeader(item.section, item.count, expanded(item.section)) {
                            setExpanded(item.section, !expanded(item.section))
                        }
                        is SidebarItem.Slim -> {
                            val thread = item.thread
                            val readOnly = store.onRemovedServer(thread)
                            val row = @Composable {
                                WithMenu(if (readOnly) null else ({ threadMenu(store, thread, ::beginRename) { deleting = it } })) { modifier, openMenu ->
                                    SlimThreadRow(
                                        thread, store.projectsByID[thread.projectID], selected = store.selection == Selection.Thread(thread.id),
                                        modifier = modifier.fillMaxWidth().height(lineHeight).peeks(thread.id, peek), onLongClick = openMenu, open = ::open,
                                    )
                                }
                            }
                            val actions = swipeActions(thread)
                            if (readOnly || actions.isEmpty()) {
                                row()
                            } else {
                                RowSwipe(actions, isOpen = swiped == item.id, onOpenChange = swipe(item.id).onOpenChange, content = row)
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
        ConfirmationDialog(Confirmation(title, "Stop it first, then archive it.", dismissal = "OK")) { busy = null }
    }
    renaming?.let { thread -> RenameDialog(thread) { renaming = null } }
    Asking(deleting, onDismiss = { deleting = null }) { thread ->
        Confirmation(
            "Delete “${thread.title}”?", store.deletionNote(thread),
            answers = listOf(Answer("Delete", ButtonVariant.Danger) { store.delete(thread) }),
        )
    }
}

/** The drafts, the active threads, and under them the working, the snoozed and the archived ones: each a line that opens into their list. */
private fun items(
    store: AppStore,
    active: List<ThreadInfo>,
    sections: List<Pair<ThreadSection, List<ThreadInfo>>>,
    expanded: (ThreadSection) -> Boolean,
): List<SidebarItem> = buildList {
    add(SidebarItem.Drafts)
    active.forEach { add(SidebarItem.Active(it)) }
    if (active.isEmpty()) add(SidebarItem.Empty)
    if (store.offersNewThread(store.listedDrafts, active)) add(SidebarItem.NewThread)
    val shown = sections.filter { it.second.isNotEmpty() }
    if (shown.isNotEmpty()) add(SidebarItem.Gap)
    shown.forEachIndexed { index, (section, threads) ->
        add(SidebarItem.Header(section, threads.size))
        if (!expanded(section)) return@forEachIndexed
        threads.forEach { add(SidebarItem.Slim(it)) }
        if (index < shown.size - 1) add(SidebarItem.SectionEnd(section))
    }
}

/**
 * How tall the gap is: the room the other rows leave while they all fit, read from how the list
 * last laid them out. The gap's own size takes no part, so this settles after one layout.
 */
private fun gapHeight(info: LazyListLayoutInfo): Int {
    val shown = info.visibleItemsInfo
    if (shown.isEmpty() || shown.size < info.totalItemsCount) return 0
    val others = shown.filter { it.key != SidebarItem.Gap.id }.sumOf { it.size }
    return maxOf(0, info.viewportSize.height - info.beforeContentPadding - info.afterContentPadding - others)
}

/** What the search found, then the line that says the servers still search, or that line alone when it found nothing. */
private fun searchItems(results: List<SearchResult>, searching: Boolean): List<SidebarItem> = when {
    results.isEmpty() -> listOf(SidebarItem.Empty)
    searching -> results.map { SidebarItem.Result(it) } + SidebarItem.Empty
    else -> results.map { SidebarItem.Result(it) }
}

/** Where each thread is among those the jump keys go through, the first place it has. */
private fun jumpPlaces(threads: List<ThreadInfo>): Map<String, Int> = buildMap {
    threads.forEachIndexed { index, thread -> putIfAbsent(thread.id, index) }
}

/** Scrolls the row into view, or to the top of the list, with `anchor` there in its place when both fit: its section's line. */
private suspend fun reveal(list: LazyListState, index: Int, toTop: Boolean, anchor: Int? = null) {
    if (toTop && anchor != null) {
        list.animateScrollToItem(anchor)
        val info = list.layoutInfo
        val row = info.visibleItemsInfo.firstOrNull { it.index == index }
        if (row == null || row.offset + row.size > info.viewportEndOffset) list.animateScrollToItem(index)
        return
    }
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
        GlassButton(Symbol.Command, "Commands", onClick = { store.openPanel(PanelPage.Commands) })
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
private fun SectionHeader(section: ThreadSection, count: Int, expanded: Boolean, toggle: () -> Unit) {
    HighlightButton(onClick = toggle, modifier = Modifier.fillMaxWidth().height(lineHeight), radius = Radius.md, inset = rowMargin, opaque = true) {
        SectionLine(section, count, expanded, Modifier.fillMaxSize().padding(rowMargin).padding(horizontal = sidebarEdge - sidebarRowInset))
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
            AppUpdateRow(store.updater, Modifier.padding(start = sidebarEdge, end = sidebarEdge - serverButtonOutset, top = 8.dp, bottom = 8.dp))
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
                    .height(GlassButton.side)
                    .glassButton(CircleShape)
                    .padding(start = 6.dp)
                    .steersSearch(store, highlighted),
                capsule = true, clearable = false, focusRequester = searchFocus, onFocusChange = onFocusChange, onSubmit = openHighlighted,
                selectAllRequests = store.searchFocusRequests,
            )
            Appearing(searching, enter = appear, exit = leave) { GlassButton(Symbol.X, "Close search", onClick = closeSearch) }
            Appearing(!searching, enter = appear, exit = leave) {
                GlassButton("New thread", onClick = { store.newThread() }, modifier = Modifier.opacity(Opacity.Disabled, on = !canStart), enabled = canStart) {
                    Icon(Symbol.SquarePen, IconSize.Lg)
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
        ServerLine(servers[0], Modifier.fillMaxWidth().height(lineHeight).padding(start = sidebarEdge, end = sidebarEdge - serverButtonOutset))
        ServerErrors(servers)
        return
    }
    HighlightButton(onClick = toggle, radius = 0.dp, contentAlignment = Alignment.CenterStart) {
        Row(
            Modifier.fillMaxWidth().height(lineHeight).padding(horizontal = sidebarEdge),
            horizontalArrangement = Arrangement.spacedBy(7.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            AllServersLine(servers, expanded, Modifier.weight(1f))
            SidebarChevron(expanded)
        }
    }
    if (expanded) ServerLines(store)
    ServerErrors(servers)
}

/** Why each server that isn't connected couldn't be reached, as its link last said, under the servers' lines. */
@Composable
private fun ServerErrors(servers: List<Server>) {
    val failing = servers.filter { it.state != Server.State.Connected && it.error != null }
    for (server in failing) {
        val error = server.error ?: continue
        Text(
            if (servers.size == 1) error else "${server.name}: $error",
            Modifier.fillMaxWidth().padding(start = sidebarEdge + 16.dp, end = sidebarEdge, bottom = 10.dp),
            size = 11f,
            color = Theme.colors.mutedStrongerForeground,
            maxLines = 3,
        )
    }
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
            ServerLine(server, Modifier.fillMaxWidth().height(lineHeight).padding(start = sidebarEdge, end = sidebarEdge - serverButtonOutset))
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
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    ActionButton("Cancel", size = ControlSize.Large, margin = beside(end = 8.dp), onClick = onDismiss)
                    ActionButton("Rename", variant = ButtonVariant.Primary, size = ControlSize.Large, disabled = title.isBlank(), onClick = ::rename)
                }
            }
        }
    }
    LaunchedEffect(Unit) { runCatching { field.requestFocus() } }
    DisposableEffect(Unit) { onDispose { KeyboardFocus.editable = false } }
}
