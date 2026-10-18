package app.motile.settings

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.motile.DemoHooks
import app.motile.LocalCloseSheet
import app.motile.LocalStore
import app.motile.commandpanel.CommandPanel
import app.motile.core.AppStore
import app.motile.core.PanelPage
import app.motile.core.SettingsEntry
import app.motile.core.SettingsSection
import app.motile.layout.KeyboardFocus
import app.motile.layout.SIDEBAR_WIDTH
import app.motile.platform.Symbol
import app.motile.theme.LocalPalette
import app.motile.theme.Surface
import app.motile.ui.SheetBackButton
import app.motile.ui.SheetBar
import app.motile.ui.HighlightButton
import app.motile.ui.InputField
import app.motile.ui.SheetCloseButton
import app.motile.ui.Text

/** A page the settings push over their list. */
sealed interface SettingsRoute {
    data class Section(val section: SettingsSection) : SettingsRoute
    /** A server's page; a `removed` one's only deletes its copies. */
    data class ServerPage(val serverID: String, val removed: Boolean = false) : SettingsRoute
    data object AddServer : SettingsRoute
    data class ProjectPage(val projectID: String) : SettingsRoute
    data class WorktreeSetup(val projectID: String) : SettingsRoute
    data class BranchNames(val serverID: String) : SettingsRoute
    data object AddProject : SettingsRoute
    data class ProjectIcon(val projectID: String) : SettingsRoute
}

/** The pages pushed over the settings' list, kept while the command panel covers the settings so that they come back as they were. */
private object SettingsPath {
    var routes by mutableStateOf(listOf<SettingsRoute>())
}

/** Pushes a page of the settings, and goes back from one. */
class SettingsNavigator(val push: (SettingsRoute) -> Unit, val pop: () -> Unit)

val LocalSettingsNavigator = compositionLocalOf { SettingsNavigator({}, {}) }

/** What stands at the start of a page's bar: the sheet's close button, the way back, or nothing beside the sections' list. */
sealed interface BarLeading {
    class Close(val action: () -> Unit) : BarLeading
    class Back(val action: () -> Unit) : BarLeading
}

val LocalBarLeading = compositionLocalOf<BarLeading?> { null }

/** A sheet at least this wide has the sections beside the open one. */
private val wideSheet = 700.dp

/**
 * The settings in a sheet, with a close button. A narrow sheet lists the sections and pushes the
 * open one over the list; a wide one has the list beside it. Back goes back a page.
 */
@Composable
fun SettingsSheet() {
    val store = LocalStore.current
    val close = LocalCloseSheet.current
    DisposableEffect(Unit) {
        onDispose { if (store.settings == null) SettingsPath.routes = emptyList() }
    }
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val wide = maxWidth >= wideSheet
        LaunchedEffect(wide, store.settings, store.settingsTarget) { follow(store, wide) }
        val navigator = remember {
            SettingsNavigator(
                push = { SettingsPath.routes = SettingsPath.routes + it },
                pop = { SettingsPath.routes = SettingsPath.routes.dropLast(1) },
            )
        }
        CompositionLocalProvider(LocalSettingsNavigator provides navigator) {
            if (wide) Split(close) else Stack(store, close)
        }
        BackHandler(enabled = SettingsPath.routes.isNotEmpty()) { navigator.pop() }
    }
}

/** A narrow sheet opens the section the store was pointed at, unless that is only the list's first one. A wide one shows it beside the list. */
private fun follow(store: AppStore, wide: Boolean) {
    if (wide) {
        SettingsPath.routes = emptyList()
        return
    }
    val section = store.settings ?: return
    if (section == SettingsSection.General && store.settingsTarget == null) return
    if (SettingsPath.routes.firstOrNull() == SettingsRoute.Section(section)) return
    SettingsPath.routes = listOf(SettingsRoute.Section(section))
}

@Composable
private fun Stack(store: AppStore, close: () -> Unit) {
    val navigator = LocalSettingsNavigator.current
    Pages(SettingsPath.routes) { route ->
        if (route == null) {
            CompositionLocalProvider(LocalBarLeading provides BarLeading.Close(close)) {
                SettingsSections(selected = null) { section ->
                    SettingsPath.routes = listOf(SettingsRoute.Section(section))
                    if (section != store.settings) store.openSettings(section, store.settingsTarget)
                }
            }
        } else {
            CompositionLocalProvider(LocalBarLeading provides BarLeading.Back(navigator.pop)) { RoutePage(route) }
        }
    }
}

@Composable
private fun Split(close: () -> Unit) {
    val store = LocalStore.current
    val navigator = LocalSettingsNavigator.current
    val section = store.settings ?: SettingsSection.General
    Row(Modifier.fillMaxSize()) {
        Box(Modifier.width(SIDEBAR_WIDTH.dp).fillMaxHeight()) {
            CompositionLocalProvider(LocalBarLeading provides BarLeading.Close(close)) {
                SettingsSections(selected = section) { store.openSettings(it, store.settingsTarget) }
            }
        }
        Box(Modifier.width(1.dp).fillMaxHeight().background(Surface.Background.border))
        Box(Modifier.weight(1f).fillMaxHeight()) {
            Pages(listOf(SettingsRoute.Section(section)) + SettingsPath.routes, root = true) { route ->
                val leading = if (route is SettingsRoute.Section) null else BarLeading.Back(navigator.pop)
                CompositionLocalProvider(LocalBarLeading provides leading) { route?.let { RoutePage(it) } }
            }
        }
    }
}

/** The top page of `routes`, slid in from the side it comes from. Without `root` an empty path shows the list, given as null. */
@Composable
private fun Pages(routes: List<SettingsRoute>, root: Boolean = false, page: @Composable (SettingsRoute?) -> Unit) {
    AnimatedContent(
        targetState = routes,
        transitionSpec = {
            val forward = targetState.size >= initialState.size
            val sign = if (forward) 1 else -1
            slideInHorizontally(tween(260)) { sign * it } togetherWith slideOutHorizontally(tween(260)) { -sign * it / 3 }
        },
        contentKey = { it.size to it.lastOrNull() },
        label = if (root) "settings detail" else "settings",
    ) { shown ->
        Box(Modifier.fillMaxSize().background(LocalPalette.current.background)) { page(shown.lastOrNull()) }
    }
}

@Composable
private fun RoutePage(route: SettingsRoute) {
    when (route) {
        is SettingsRoute.Section -> SettingsScreen(route.section.title) { SettingsPage(route.section) }
        is SettingsRoute.ServerPage -> ServerSettingsPage(route.serverID, route.removed)
        SettingsRoute.AddServer -> AddServerPage()
        is SettingsRoute.ProjectPage -> ProjectSettingsPage(route.projectID)
        is SettingsRoute.WorktreeSetup -> WorktreeSetupPage(route.projectID)
        is SettingsRoute.BranchNames -> BranchNamesPage(route.serverID)
        SettingsRoute.AddProject -> CommandPanel(LocalStore.current.addProjectPage, inStack = true, onLeave = LocalSettingsNavigator.current.pop)
        is SettingsRoute.ProjectIcon -> CommandPanel(PanelPage.Icon(route.projectID), inStack = true, onLeave = LocalSettingsNavigator.current.pop)
    }
}

/** A page with its bar: what leads it, its title in the middle, and what it does at its end. */
@Composable
fun SettingsScreen(title: String, trailing: (@Composable () -> Unit)? = null, content: @Composable () -> Unit) {
    Column(Modifier.fillMaxSize()) {
        SettingsBar(title, trailing)
        Box(Modifier.fillMaxWidth().weight(1f)) { content() }
    }
}

@Composable
fun SettingsBar(title: String, trailing: (@Composable () -> Unit)? = null) {
    val leading: (@Composable () -> Unit)? = when (val shown = LocalBarLeading.current) {
        is BarLeading.Close -> ({ SheetCloseButton(onClose = shown.action) })
        is BarLeading.Back -> ({ SheetBackButton(shown.action) })
        null -> null
    }
    SheetBar(title, leading = leading, trailing = trailing)
}

private val sectionGroups = listOf(
    listOf(SettingsSection.General),
    listOf(SettingsSection.Accounts, SettingsSection.Servers, SettingsSection.Projects),
    listOf(SettingsSection.PullRequests, SettingsSection.TextGeneration),
    listOf(SettingsSection.Integrations),
)

/**
 * The settings' sections in groups, each pushing its page, or selected beside it on a wide sheet.
 * While a search is typed, the groups it finds take their place.
 */
@Composable
private fun SettingsSections(selected: SettingsSection?, choose: (SettingsSection) -> Unit) {
    val store = LocalStore.current
    val focus = remember { FocusRequester() }
    DisposableEffect(Unit) {
        val before = DemoHooks.search
        DemoHooks.search = { text ->
            focus.requestFocus()
            store.settingsQuery = text
        }
        onDispose {
            DemoHooks.search = before
            KeyboardFocus.editable = false
        }
    }
    SettingsScreen("Settings") {
        Column(Modifier.fillMaxSize()) {
            InputField(
                "Search", store.settingsQuery, { store.settingsQuery = it },
                Modifier.fillMaxWidth().padding(horizontal = 16.dp).padding(top = 4.dp),
                icon = Symbol.Search, size = app.motile.theme.ControlSize.Large, clearable = true, focusRequester = focus,
            )
            val query = store.settingsQuery
            if (query.isEmpty()) {
                SettingsList(Modifier.weight(1f)) {
                    for (sections in sectionGroups) group {
                        for (section in sections) row { SectionRow(section, selected, choose) }
                    }
                }
            } else {
                Results(query, Modifier.weight(1f), choose)
            }
        }
    }
}

@Composable
private fun SectionRow(section: SettingsSection, selected: SettingsSection?, choose: (SettingsSection) -> Unit) {
    if (selected == null) {
        SettingsLink({ choose(section) }) {
            SettingsTile(section.symbol)
            RowText(section.title)
        }
        return
    }
    HighlightButton({ choose(section) }, Modifier.fillMaxWidth(), radius = 0.dp, selected = selected == section) {
        SettingsRow {
            SettingsTile(section.symbol)
            RowText(section.title)
        }
    }
}

@Composable
private fun Results(query: String, modifier: Modifier, choose: (SettingsSection) -> Unit) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val found = remember(query) { SettingsEntry.matching(query) }
    if (found.isEmpty()) {
        Column(modifier.fillMaxWidth().padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text("No Results for “$query”", size = 17f, weight = FontWeight.SemiBold, align = TextAlign.Center)
            Text("Check the spelling or try a new search.", size = NOTE_TEXT, color = palette.mutedForeground, align = TextAlign.Center)
        }
        return
    }
    SettingsList(modifier) {
        group {
            for (entry in found) row {
                SettingsPressRow({
                    store.openSettings(entry.section, entry.id)
                    choose(entry.section)
                }) {
                    SettingsTile(entry.section.symbol)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        RowText(entry.title, color = palette.foreground)
                        Text(entry.section.title, size = NOTE_TEXT, color = palette.mutedForeground)
                    }
                }
            }
        }
    }
}
