package app.motile.panel

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import app.motile.LocalStore
import app.motile.core.AppStore
import app.motile.core.Integration
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.sidebar.Reorder
import app.motile.sidebar.ReorderScrolling
import app.motile.sidebar.reorders
import app.motile.theme.Radius
import app.motile.theme.ShadowSize
import app.motile.theme.TextSize
import app.motile.theme.iconSize
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalRow
import app.motile.theme.Opacity
import app.motile.theme.Surface
import app.motile.theme.opacity
import app.motile.theme.shadow
import app.motile.theme.wash
import app.motile.ui.ActionButton
import app.motile.ui.Disabled
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.MenuPopup
import app.motile.ui.Spinner
import app.motile.ui.Text
import app.motile.ui.beside
import app.motile.ui.pressPosition
import app.motile.ui.pressable
import app.motile.ui.rememberMenuState
import kotlin.math.max

/** What the open tab shows of the folder the thread works in. */
@Composable
fun PanelTabContent(active: PanelTab?) {
    val store = LocalStore.current
    // The agents are the thread's own, kept with it, so they show while the server is away too.
    if (active == PanelTab.Agents && store.panelTarget != null) return AgentsSurface()
    store.panelUnavailable?.let { return PanelMessage(it) }
    val target = store.panelTarget ?: return
    when (active) {
        PanelTab.Diff -> DiffSurface(target)
        PanelTab.Files -> FilesSurface(target)
        is PanelTab.File -> key(active.path) { FileSurface(target, active.path) }
        is PanelTab.Change -> key(active) { ChangeSurface(target, active.turn, active.path) }
        PanelTab.Agents -> AgentsSurface()
        PanelTab.PullRequest -> PullRequestSurface(target)
        is PanelTab.PullRequestNumber -> key(active.number) { PullRequestSurface(target, active.number) }
        PanelTab.PullRequests -> PullRequestListSurface(target)
        PanelTab.Linear -> LinearSurface(target)
        is PanelTab.LinearIssue -> key(active.issueID) { LinearIssueSurface(target, active.workspace, active.issueID) }
        PanelTab.Ricrac -> RicracSurface(target)
        is PanelTab.RicracIssue -> key(active.issueID) { RicracIssueSurface(target, active.workspace, active.issueID) }
        is PanelTab.Blank, null -> PanelLauncher(target)
    }
}

@Composable
fun PanelLine(modifier: Modifier = Modifier) {
    Box(modifier.fillMaxWidth().height(1.dp).background(LocalPalette.current.border))
}

/** What a tab says in the middle of the panel when it has nothing to show. */
@Composable
fun PanelMessage(text: String, failed: Boolean = false) {
    val palette = LocalPalette.current
    Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
        SelectionContainer {
            Text(text, size = TextSize.Xs, color = if (failed) palette.destructive else palette.mutedForeground, align = TextAlign.Center)
        }
    }
}

/** What the changes tab says of a folder that is no git repository, with the button that makes it one. */
@Composable
internal fun NoRepositoryMessage(projectID: String) {
    val store = LocalStore.current
    Column(
        Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("This folder isn't a git repository.", size = TextSize.Xs, color = LocalPalette.current.mutedForeground, align = TextAlign.Center)
        val project = store.project(projectID)
        if (project != null && store.canInitializeGit(project)) {
            ActionButton("Initialize git", Symbol.GitBranch, pending = project.id in store.initializingGit) { store.initializeGit(project) }
        }
    }
}

@Composable
fun PanelLoading() {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Spinner(ControlSize.Large.symbol, color = LocalPalette.current.mutedStrongerForeground)
    }
}

/** The room between what a panel bar holds. Buttons side by side are put in a row of their own that gives it to them, so that it only looks empty. */
val panelBarGap = 2.dp

/**
 * The strip under the tabs with what the open tab is about and its buttons. A bar of controls over a
 * list keeps them `edge` from the sides, as far as the list keeps its rows.
 */
@Composable
fun PanelBar(edge: Dp? = null, content: @Composable RowScope.() -> Unit) {
    Column(Modifier.fillMaxWidth()) {
        PanelBarRow(edge, content)
        PanelLine()
    }
}

/** A bar of controls over a list on two rows, the second `edge` from the line under it. */
@Composable
fun PanelBar(edge: Dp, second: @Composable RowScope.() -> Unit, content: @Composable RowScope.() -> Unit) {
    Column(Modifier.fillMaxWidth()) {
        PanelBarRow(edge, content)
        Row(
            Modifier.fillMaxWidth().padding(start = edge, end = edge, top = panelBarGap / 2, bottom = edge),
            horizontalArrangement = Arrangement.spacedBy(panelBarGap),
            verticalAlignment = Alignment.CenterVertically,
            content = second,
        )
        PanelLine()
    }
}

@Composable
private fun PanelBarRow(edge: Dp?, content: @Composable RowScope.() -> Unit) {
    Row(
        Modifier.fillMaxWidth().height(pressable(36f)).padding(start = edge ?: 12.dp, end = edge ?: 4.dp),
        horizontalArrangement = Arrangement.spacedBy(panelBarGap),
        verticalAlignment = Alignment.CenterVertically,
        content = content,
    )
}

/** Said over what a tab shows when that is only a part of it. */
@Composable
fun PanelNote(text: String) {
    val palette = LocalPalette.current
    Column(Modifier.fillMaxWidth()) {
        Text(
            text, Modifier.fillMaxWidth().background(palette.warning.wash()).padding(horizontal = 12.dp, vertical = 7.dp),
            size = TextSize.Xs, color = palette.warning,
        )
        PanelLine()
    }
}

object PanelTabStrip {
    /** How far the tabs keep from the strip's ends, as far as from its top and bottom. */
    val edge = 8.dp
    val height get() = ControlSize.Regular.height + edge * 2
}

/** The open tabs, side by side and scrolled sideways when they don't fit, and the button that adds one. A hold lifts a tab to move it. */
@Composable
fun PanelTabStrip(tabs: PanelTabs, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val strip = rememberLazyListState()
    val reorder = remember(strip) { Reorder(strip) }
    val index = tabs.tabs.indexOf(tabs.active)
    LaunchedEffect(tabs.active) { if (index >= 0 && !tabs.isBlank) strip.animateScrollToItem(index) }
    ReorderScrolling(reorder, strip)
    val shown = reorder.arranged(tabs.tabs) { it.id }
    LazyRow(
        modifier.fillMaxHeight(),
        state = strip,
        contentPadding = PaddingValues(start = PanelTabStrip.edge - PanelTabChip.margin, end = PanelTabStrip.edge - PanelTabChip.margin),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (!tabs.isBlank) {
            items(shown, key = { it.id }) { tab ->
                val lifted = reorder.key == tab.id
                val placed = if (lifted) {
                    Modifier.zIndex(1f).graphicsLayer { translationX = reorder.offset }
                } else {
                    Modifier.animateItem(fadeInSpec = null, fadeOutSpec = null)
                }
                Box(placed) {
                    PanelTabChip(
                        tab, active = tab == tabs.active, lifted = lifted, movable = tabs.tabs.size > 1, reorder = reorder,
                        keys = { shown.map { it.id } }, put = { store.rightPanel.moveTab(tab, it) },
                    )
                }
            }
        }
        if (store.panelUnavailable == null && !tabs.isBlank) {
            item(key = "new") { ActionButton(icon = Symbol.Plus, help = "New tab", margin = beside(start = PanelTabChip.margin)) { store.rightPanel.openBlank() } }
        }
    }
}

object PanelTabChip {
    /** The room on each side that looks empty but is the tab's, so no tap falls between tabs. */
    val margin = 1.dp
}

/** A tab of the strip, with its menu where it is held, and lifted by a hold to be moved where it moves. */
@Composable
fun PanelTabChip(tab: PanelTab, active: Boolean, lifted: Boolean, movable: Boolean, reorder: Reorder, keys: () -> List<String>, put: (Int) -> Unit) {
    val store = LocalStore.current
    val panel = store.rightPanel
    val height = ControlSize.Regular.height
    val closeSize = ControlSize.Inset.height
    val closeMargin = ControlSize.insetMargin
    // How far above and under the tab a finger still presses it.
    val reach = max(0f, (Platform.minimumPress - height.value) / 2).dp
    val menu = rememberMenuState()
    Box(contentAlignment = Alignment.CenterEnd) {
        if (lifted) {
            Box(Modifier.matchParentSize().padding(vertical = reach, horizontal = PanelTabChip.margin).shadow(ShadowSize.Lg, RoundedCornerShape(Radius.md), Opacity.ShadowStronger))
        }
        HighlightButton(
            onClick = { panel.activate(tab) },
            modifier = Modifier.pressPosition(menu),
            selected = active,
            lit = lifted,
            inset = PaddingValues(vertical = reach, horizontal = PanelTabChip.margin),
            faded = true,
            small = true,
            opaque = lifted,
            // A hold on a tab that moves lifts it, and its menu comes up from there.
            onLongClick = if (movable) ({}) else ({ menu.openAtPress() }),
        ) {
            Row(
                Modifier
                    // Inside the press, which takes every move after a hold for itself.
                    .reorders(
                        movable, tab.id, reorder, keys, { keys().toSet() }, openMenu = { menu.openAtPress() }, closeMenu = { menu.close() }, put = put,
                    )
                    .padding(vertical = reach, horizontal = PanelTabChip.margin)
                    .height(height)
                    .widthIn(max = 180.dp)
                    .padding(start = 9.dp, end = closeMargin),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(tab.symbol, iconSize(text = TextSize.Xs))
                Text(tab.title, Modifier.weight(1f, fill = false), size = TextSize.Xs, weight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
                Spacer(Modifier.size(closeSize))
            }
            MenuPopup(menu) {
                item("Close") { panel.close(tab) }
                item("Close Others") { panel.closeOthers(tab) }
                item("Close All") { panel.closeAll() }
                tab.path?.let { path ->
                    divider()
                    item("Copy Path") { Platform.copy(path) }
                }
            }
        }
        if (active) {
            CompositionLocalProvider(LocalRow provides Surface.Layer.ControlLit) {
                ActionButton(
                    icon = Symbol.X, help = store.shortcuts.help("Close", "rightPanel.close"), size = ControlSize.Inset, symbolSize = ControlSize.Inset.smallSymbol,
                    modifier = Modifier.padding(end = closeMargin + PanelTabChip.margin),
                ) { panel.close(tab) }
            }
        }
    }
}

/** What a blank tab shows: the tabs there are to open, in the order kept for them. A hold lifts a row to move it. */
@Composable
private fun PanelLauncher(target: PanelTarget) {
    val store = LocalStore.current
    val panel = store.rightPanel
    val haptics = LocalHapticFeedback.current
    val list = rememberLazyListState()
    val reorder = remember(list) { Reorder(list) }
    LaunchedEffect(target.serverID) {
        store.linear.read(target.serverID)
        store.ricrac.read(target.serverID)
    }
    ReorderScrolling(reorder, list)
    val entries = launcherEntries(store, target)
    val shown = reorder.arranged(entries) { it.tab.id }
    Column(
        Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("Open", size = TextSize.Sm, weight = FontWeight.SemiBold, color = LocalPalette.current.foreground)
        LazyColumn(Modifier.width(250.dp), state = list) {
            items(shown, key = { it.tab.id }) { entry ->
                val lifted = reorder.key == entry.tab.id
                val placed = if (lifted) {
                    Modifier.zIndex(1f).graphicsLayer { translationY = reorder.offset }
                } else {
                    Modifier.animateItem(fadeInSpec = null, fadeOutSpec = null)
                }
                Box(placed) {
                    LauncherRow(
                        entry, lifted,
                        Modifier.reorders(
                            entries.size > 1, entry.tab.id, reorder,
                            keys = { shown.map { it.tab.id } },
                            movable = { entries.map { it.tab.id }.toSet() },
                            openMenu = {}, closeMenu = {},
                            put = { index -> panel.moveLauncherRow(entry.tab, index, entries.map { it.tab }) },
                            lift = { haptics.performHapticFeedback(HapticFeedbackType.LongPress) },
                        ),
                    )
                }
            }
        }
    }
}

/** A row of the launcher: the tab it opens, and why it can't when it can't. */
private class LauncherEntry(val tab: PanelTab, val symbol: Symbol, val title: String, val reason: String?, val count: Int = 0, val action: () -> Unit)

/** The launcher's rows that show, in the order kept for them. */
private fun launcherEntries(store: AppStore, target: PanelTarget): List<LauncherEntry> {
    val panel = store.rightPanel
    return panel.launcherOrder.mapNotNull { tab ->
        when (tab) {
            PanelTab.Agents -> LauncherEntry(tab, Symbol.Bot, "Agents", null, count = store.agents.count { it.working }) { panel.open(tab) }
            PanelTab.Files -> LauncherEntry(tab, Symbol.Folder, "Files", null) { panel.open(tab) }
            PanelTab.Diff -> LauncherEntry(tab, Symbol.Diff, "Diff", if (target.repository) null else "Available in git repositories.") { panel.showDiff() }
            PanelTab.PullRequest -> LauncherEntry(tab, Symbol.GitPullRequest, "Pull Request", store.pullRequestsUnavailable) { panel.open(tab) }
            PanelTab.PullRequests -> if (!store.pullRequestsExtended) null else {
                LauncherEntry(tab, Symbol.List, "All Pull Requests", store.pullRequestsUnavailable) { panel.open(tab) }
            }
            PanelTab.Linear -> if (!store.isOn(Integration.Linear)) null else {
                LauncherEntry(tab, Symbol.Linear, if (store.linearConnected) "Linear" else "Connect Linear", store.linearUnavailable) { panel.open(tab) }
            }
            PanelTab.Ricrac -> if (!store.isOn(Integration.Ricrac)) null else {
                LauncherEntry(tab, Symbol.Ricrac, if (store.ricracConnected) "Ricrac" else "Connect Ricrac", store.ricracUnavailable) { panel.open(tab) }
            }
            else -> null
        }
    }
}

/** The space between two launcher rows' lights, which is the rows' own, so that it takes a tap. */
private val LAUNCHER_GAP = 2.dp

/** `modifier` goes inside the press, which takes every move after a hold for itself. */
@Composable
private fun LauncherRow(entry: LauncherEntry, lifted: Boolean, modifier: Modifier = Modifier) {
    val palette = LocalPalette.current
    val foreground = palette.foreground
    val disabled = entry.reason != null
    Box {
        if (lifted) {
            Box(Modifier.matchParentSize().padding(vertical = LAUNCHER_GAP / 2).shadow(ShadowSize.Lg, RoundedCornerShape(Radius.md), Opacity.ShadowStronger))
        }
        Disabled(disabled) {
            HighlightButton(
                onClick = entry.action,
                modifier = Modifier.fillMaxWidth(),
                lit = lifted,
                inset = PaddingValues(vertical = LAUNCHER_GAP / 2),
                opaque = lifted,
            ) {
                Row(
                    modifier
                        .fillMaxWidth()
                        .opacity(Opacity.Disabled, on = disabled)
                        .height(maxOf(ControlSize.Regular.height, Platform.minimumPress.dp) + LAUNCHER_GAP)
                        .padding(horizontal = 10.dp),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(entry.symbol, iconSize(text = TextSize.Sm), color = foreground)
                    // How many of its things work, in the working colour.
                    val words = buildAnnotatedString {
                        append(entry.title)
                        if (entry.count > 0) {
                            append(" ")
                            withStyle(SpanStyle(color = palette.mutedForeground)) { append("·") }
                            withStyle(SpanStyle(color = palette.process)) { append(" ${entry.count}") }
                        }
                    }
                    Text(words, size = TextSize.Sm, color = foreground, maxLines = 1)
                }
            }
        }
    }
}
