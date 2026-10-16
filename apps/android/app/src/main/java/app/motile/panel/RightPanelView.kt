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
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Integration
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalRow
import app.motile.theme.Opacity
import app.motile.theme.Surface
import app.motile.theme.opacity
import app.motile.theme.wash
import app.motile.ui.ActionButton
import app.motile.ui.Disabled
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.MenuPopup
import app.motile.ui.Spinner
import app.motile.ui.Text
import app.motile.ui.pressPosition
import app.motile.ui.pressable
import app.motile.ui.rememberMenuState
import kotlin.math.max

/** What the open tab shows of the folder the thread works in. */
@Composable
fun PanelTabContent(active: PanelTab?) {
    val store = LocalStore.current
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
            Text(text, size = 12.5f, color = if (failed) palette.destructive else palette.mutedForeground, align = TextAlign.Center)
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
        Text("This folder isn't a git repository.", size = 12.5f, color = LocalPalette.current.mutedForeground, align = TextAlign.Center)
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

/** The strip under the tabs with what the open tab is about and its buttons, on one row or two. */
@Composable
fun PanelBar(second: (@Composable RowScope.() -> Unit)? = null, content: @Composable RowScope.() -> Unit) {
    Column(Modifier.fillMaxWidth()) {
        PanelBarRow(content)
        if (second != null) PanelBarRow(second)
        PanelLine()
    }
}

@Composable
private fun PanelBarRow(content: @Composable RowScope.() -> Unit) {
    Row(
        Modifier.fillMaxWidth().height(pressable(36f)).padding(start = 12.dp, end = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
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
            size = 12f, color = palette.warning,
        )
        PanelLine()
    }
}

object PanelTabStrip {
    val edge = 8.dp
}

/** The open tabs, side by side and scrolled sideways when they don't fit, and the button that adds one. */
@Composable
fun PanelTabStrip(tabs: PanelTabs, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val strip = rememberLazyListState()
    val index = tabs.tabs.indexOf(tabs.active)
    LaunchedEffect(tabs.active) { if (index >= 0 && !tabs.isBlank) strip.animateScrollToItem(index) }
    LazyRow(
        modifier.fillMaxHeight(),
        state = strip,
        contentPadding = PaddingValues(start = PanelTabStrip.edge - PanelTabChip.margin, end = PanelTabStrip.edge - PanelTabChip.margin),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (!tabs.isBlank) {
            items(tabs.tabs, key = { it.id }) { tab -> PanelTabChip(tab, active = tab == tabs.active) }
        }
        if (store.panelUnavailable == null && !tabs.isBlank) {
            item(key = "new") { ActionButton(icon = Symbol.Plus, help = "New tab") { store.rightPanel.openBlank() } }
        }
    }
}

object PanelTabChip {
    /** The room on each side that looks empty but is the tab's, so no tap falls between tabs. */
    val margin = 1.dp
}

@Composable
fun PanelTabChip(tab: PanelTab, active: Boolean) {
    val store = LocalStore.current
    val panel = store.rightPanel
    val height = ControlSize.Regular.height
    val closeSize = ControlSize.Small.height
    // The close button is as far from the tab's side as from its top and bottom.
    val closeMargin = (height - closeSize) / 2
    // How far above and under the tab a finger still presses it.
    val reach = max(0f, (Platform.minimumPress - height.value) / 2).dp
    val menu = rememberMenuState()
    Box(contentAlignment = Alignment.CenterEnd) {
        HighlightButton(
            onClick = { panel.activate(tab) },
            modifier = Modifier.pressPosition(menu),
            selected = active,
            inset = PaddingValues(vertical = reach, horizontal = PanelTabChip.margin),
            faded = true,
            small = true,
            onLongClick = { menu.openAtPress() },
        ) {
            Row(
                Modifier
                    .padding(vertical = reach, horizontal = PanelTabChip.margin)
                    .height(height)
                    .widthIn(max = 180.dp)
                    .padding(start = 9.dp, end = closeMargin),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(Modifier.width(14.dp), contentAlignment = Alignment.Center) { Icon(tab.symbol, 11f) }
                Text(tab.title, Modifier.weight(1f, fill = false), size = 12f, weight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
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
                    icon = Symbol.X, help = store.shortcuts.help("Close", "rightPanel.close"), size = ControlSize.Small, symbolSize = 11f,
                    modifier = Modifier.padding(end = closeMargin + PanelTabChip.margin),
                ) { panel.close(tab) }
            }
        }
    }
}

/** What a blank tab shows: the tabs there are to open. */
@Composable
private fun PanelLauncher(target: PanelTarget) {
    val store = LocalStore.current
    val panel = store.rightPanel
    LaunchedEffect(target.serverID) {
        store.linear.read(target.serverID)
        store.ricrac.read(target.serverID)
    }
    Column(
        Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("Open", size = 13f, weight = FontWeight.SemiBold, color = LocalPalette.current.foreground)
        Column(Modifier.width(250.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            LauncherRow(Symbol.Folder, "Files", null) { panel.open(PanelTab.Files) }
            LauncherRow(Symbol.Diff, "Diff", if (target.repository) null else "Available in git repositories.") { panel.showDiff() }
            LauncherRow(Symbol.Users, "Agents", null) { panel.open(PanelTab.Agents) }
            LauncherRow(Symbol.GitPullRequest, "Pull Request", store.pullRequestsUnavailable) { panel.open(PanelTab.PullRequest) }
            if (store.pullRequestsExtended) {
                LauncherRow(Symbol.List, "All Pull Requests", store.pullRequestsUnavailable) { panel.open(PanelTab.PullRequests) }
            }
            if (store.isOn(Integration.Linear)) {
                LauncherRow(Symbol.Linear, if (store.linearConnected) "Linear" else "Connect Linear", store.linearUnavailable) { panel.open(PanelTab.Linear) }
            }
            if (store.isOn(Integration.Ricrac)) {
                LauncherRow(Symbol.Ricrac, if (store.ricracConnected) "Ricrac" else "Connect Ricrac", store.ricracUnavailable) { panel.open(PanelTab.Ricrac) }
            }
        }
    }
}

@Composable
private fun LauncherRow(symbol: Symbol, title: String, reason: String?, action: () -> Unit) {
    val foreground = LocalPalette.current.foreground
    Disabled(reason != null) {
        HighlightButton(onClick = action, modifier = Modifier.fillMaxWidth().opacity(Opacity.Disabled, on = reason != null)) {
            Row(
                Modifier.fillMaxWidth().height(pressable(34f)).padding(horizontal = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(Modifier.width(18.dp), contentAlignment = Alignment.Center) { Icon(symbol, 13f, color = foreground) }
                Text(title, size = 13f, color = foreground, maxLines = 1)
            }
        }
    }
}
