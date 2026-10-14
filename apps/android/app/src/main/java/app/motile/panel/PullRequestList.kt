package app.motile.panel

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Loaded
import app.motile.core.Preferences
import app.motile.core.Time
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.LocalPalette
import app.motile.ui.ActionButton
import app.motile.ui.ActionMenu
import app.motile.ui.Chip
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.InputField
import app.motile.ui.MenuPopup
import app.motile.ui.Text
import app.motile.ui.color
import app.motile.ui.pressPosition
import app.motile.ui.rememberMenuState

/** Which pull requests the list shows, with what its menu calls them. */
private val listStates = listOf("open" to "Open", "merged" to "Merged", "closed" to "Closed", "all" to "All")

/** The repository's pull requests, the last updated first, to open one in a tab of its own or link it to the thread. */
@Composable
fun PullRequestListSurface(target: PanelTarget) {
    val store = LocalStore.current
    val state = store.rightPanel.pullRequests
    var listed by remember { mutableStateOf(Preferences.string("pullRequests.state") ?: "open") }
    var search by rememberSaveable { mutableStateOf("") }
    var asked by remember { mutableIntStateOf(0) }
    val focus = remember { FocusRequester() }
    Column(Modifier.fillMaxSize()) {
        PanelBar {
            ActionMenu(listStates.firstOrNull { it.first == listed }?.second ?: "Open", modifier = Modifier.offset(x = (-8).dp), help = "Which pull requests to show") {
                for ((value, name) in listStates) {
                    toggle(name, listed == value) {
                        listed = value
                        Preferences.set("pullRequests.state", value)
                    }
                }
            }
            DemoSearch(focus) { search += it }
            InputField(
                "Search", search, { search = it }, Modifier.weight(1f, fill = false).widthIn(max = 220.dp).editsText(),
                icon = Symbol.Search, clearable = true, focusRequester = focus,
            )
            Spacer(Modifier.weight(0.001f).widthIn(min = 4.dp))
            ActionButton(icon = Symbol.RotateCw, help = "Read the pull requests again", pending = PanelTab.PullRequests in state.reading) { asked += 1 }
        }
        state.notice?.let { PullRequestNoticeBar(it) }
        val reason = store.pullRequestsUnavailable
        when {
            reason != null -> PanelMessage(reason)
            !store.pullRequestsExtended -> PanelMessage("Update your server to see its pull requests here.")
            else -> {
                LaunchedEffect(target, listed, store.workspaceVersion, asked) { state.loadPullRequests(target, listed) }
                when (val list = state.list) {
                    Loaded.Loading -> PanelLoading()
                    is Loaded.Failed -> PanelMessage(list.message, failed = true)
                    is Loaded.Ready -> Rows(target, filtered(list.value, search), listed, search)
                }
            }
        }
    }
}

private fun filtered(rows: List<PullRequestRow>, search: String): List<PullRequestRow> {
    val words = search.trim().lowercase()
    if (words.isEmpty()) return rows
    return rows.filter { "${it.number} ${it.title} ${it.author} ${it.head}".lowercase().contains(words) }
}

@Composable
private fun Rows(target: PanelTarget, shown: List<PullRequestRow>, listed: String, search: String) {
    val store = LocalStore.current
    val thread = store.selectedThread
    var linking by rememberSaveable { mutableStateOf("") }
    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 8.dp, end = 8.dp, top = 8.dp - panelRowMargin.calculateTopPadding(), bottom = panelEndRoom - panelRowMargin.calculateBottomPadding()),
    ) {
        if (thread != null) {
            item("link") {
                LinkPullRequestField(linking, { linking = it }, Modifier.widthIn(max = 380.dp).padding(start = 6.dp, end = 6.dp, bottom = 9.dp)) { number ->
                    store.rightPanel.pullRequests.link(number, thread.id, thread.serverID)
                }
            }
        }
        if (shown.isEmpty()) {
            item("none") {
                val words = if (search.isEmpty()) "No ${if (listed == "all") "" else "$listed "}pull requests." else "None match “$search”."
                Text(words, Modifier.fillMaxWidth().padding(top = 40.dp), size = 12.5f, color = LocalPalette.current.mutedForeground, align = TextAlign.Center)
            }
        }
        items(shown, key = { it.number }) { row ->
            PullRequestListRow(row, target, linked = thread?.pullRequest?.number == row.number)
        }
    }
}

@Composable
private fun PullRequestListRow(row: PullRequestRow, target: PanelTarget, linked: Boolean) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val menu = rememberMenuState()
    val thread = store.selectedThread
    Box {
        HighlightButton(
            onClick = { store.rightPanel.pullRequests.showPullRequest(row.number, target) },
            modifier = Modifier.fillMaxWidth().pressPosition(menu),
            inset = panelRowMargin,
            onLongClick = { menu.openAtPress() },
        ) {
            Row(
                Modifier.padding(panelRowMargin).padding(horizontal = 10.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Box(Modifier.size(width = 16.dp, height = scaled(18f).dp), contentAlignment = Alignment.Center) {
                    Icon(row.state.symbol, 13f, color = row.state.color(palette))
                }
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.Top) {
                        Text(row.title, Modifier.weight(1f, fill = false), size = 13f, weight = FontWeight.Medium, color = palette.foreground, maxLines = 2)
                        if (linked) Chip("This thread", tone = palette.primary)
                        Spacer(Modifier.weight(0.001f).widthIn(min = 4.dp))
                        Signals(row)
                    }
                    Text(
                        "#${row.number} · ${row.author} · ${row.head} → ${row.base} · ${Time.ago(row.updatedAt)}",
                        size = 11.5f, color = palette.mutedStrongerForeground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis,
                    )
                }
            }
        }
        MenuPopup(menu) {
            if (thread != null && !linked) item("Link to This Thread") { store.rightPanel.pullRequests.link(row.number, thread.id, thread.serverID) }
            row.url?.let { url ->
                item("Open on GitHub") { Platform.open(url) }
                item("Copy Link") { Platform.copy(url) }
            }
        }
    }
}

/** How its checks and its review went, and what it changes. */
@Composable
private fun Signals(row: PullRequestRow) {
    Row(horizontalArrangement = Arrangement.spacedBy(7.dp), verticalAlignment = Alignment.CenterVertically) {
        row.review?.let { (tone, label) ->
            val symbol = when (tone) {
                Tone.Success -> Symbol.CircleCheck
                Tone.Danger -> Symbol.CircleAlert
                else -> Symbol.Eye
            }
            Icon(symbol, 11f, Modifier.semantics { contentDescription = label }, color = tone.color)
        }
        row.checks?.let { checks ->
            Box(Modifier.size(7.dp).background(checks.color, CircleShape).semantics { contentDescription = row.checksLabel ?: "" })
        }
        if (row.additions + row.deletions > 0) Text(LineCountText.text(row.additions, row.deletions), style = LineCountText.style, maxLines = 1)
    }
}
