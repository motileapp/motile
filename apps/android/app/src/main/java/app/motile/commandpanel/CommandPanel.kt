package app.motile.commandpanel

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.motile.DemoHooks
import app.motile.LocalCloseSheet
import app.motile.LocalStore
import app.motile.core.PanelPage
import app.motile.layout.KeyboardFocus
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.ui.GlassButton
import app.motile.ui.InputField
import app.motile.ui.InputVariant
import app.motile.ui.SheetBackButton
import app.motile.ui.SheetBar
import app.motile.ui.SheetCloseButton
import app.motile.ui.Text
import app.motile.ui.glassButton
import kotlinx.coroutines.flow.drop

/**
 * The panel to start a thread, open one, add a project, choose a project's icon or run a command.
 * Its pages push on a stack of their own, each with a search field at the bottom, picked by a tap;
 * the keyboard only comes by itself where the page is something to type.
 *
 * `inStack` is the panel pushed on another sheet's pages, like the settings': its first page goes
 * back with `onLeave`, which is also what closes it.
 */
@Composable
fun CommandPanel(start: PanelPage, inStack: Boolean = false, onLeave: (() -> Unit)? = null) {
    val store = LocalStore.current
    val closeSheet = LocalCloseSheet.current
    val scope = rememberCoroutineScope()
    val state = remember { PanelState(start, store, scope, inStack) }
    state.leave = onLeave ?: closeSheet
    val page = state.page
    val field = remember { FocusRequester() }
    val focus = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current

    LaunchedEffect(start) {
        if (!inStack && start != state.page) state.restart(start)
    }
    LaunchedEffect(Unit) {
        if (inStack) store.readGitHubOfServers()
        if (state.lists(state.page)) state.arrive()
    }
    LaunchedEffect(Unit) { snapshotFlow { store.github }.drop(1).collect { state.followGitHub() } }
    LaunchedEffect(Unit) { snapshotFlow { store.projectsAdded }.drop(1).collect { state.close() } }
    LaunchedEffect(page) {
        withFrameNanos {}
        if (!page.isTyped) return@LaunchedEffect focus.clearFocus()
        field.requestFocus()
        keyboard?.show()
    }
    DisposableEffect(Unit) {
        val previous = DemoHooks.search
        val typing: (String) -> Unit = { text ->
            field.requestFocus()
            if (text.isNotEmpty()) state.type(state.query + text)
        }
        DemoHooks.search = typing
        onDispose {
            if (DemoHooks.search === typing) DemoHooks.search = previous
            KeyboardFocus.editable = false
        }
    }
    BackHandler(enabled = state.pages.size > 1 || inStack) {
        if (state.pages.size > 1) state.back() else state.leave()
    }

    Column(
        Modifier
            .fillMaxSize()
            .onPreviewKeyEvent { event ->
                if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                when (event.key) {
                    Key.DirectionDown -> state.steer(1)
                    Key.DirectionUp -> state.steer(-1)
                    else -> return@onPreviewKeyEvent false
                }
                true
            }
            .onKeyEvent { event ->
                if (event.key != Key.Escape || event.type != KeyEventType.KeyDown) return@onKeyEvent false
                state.close()
                true
            },
    ) {
        SheetBar(
            state.title(page),
            leading = {
                when {
                    inStack -> SheetBackButton { if (state.pages.size > 1) state.back() else state.leave() }
                    state.pages.size > 1 -> SheetBackButton { state.back() }
                    else -> SheetCloseButton(onClose = { state.close() })
                }
            },
            trailing = page.githubID?.let { id ->
                {
                    GlassButton(Symbol.RotateCw, "Refresh", onClick = { store.loadRepos(id, fresh = true) }, pending = id in store.listingRepos)
                }
            },
        )
        AnimatedContent(
            targetState = page,
            modifier = Modifier.fillMaxWidth().weight(1f),
            transitionSpec = {
                val side = if (state.forward) 1 else -1
                (slideInHorizontally { it * side } + fadeIn()) togetherWith (slideOutHorizontally { -it * side / 3 } + fadeOut())
            },
            label = "page",
        ) { shown ->
            Results(state, shown, onDrag = {
                focus.clearFocus()
                keyboard?.hide()
            })
        }
        SearchField(state, field)
    }
}

/** The search field, at the bottom of the sheet as on iOS, over the keyboard while it is up. */
@Composable
private fun SearchField(state: PanelState, field: FocusRequester) {
    val page = state.page
    Box(
        Modifier
            .fillMaxWidth()
            .windowInsetsPadding(WindowInsets.ime.union(WindowInsets.navigationBars))
            .padding(horizontal = 16.dp, vertical = 8.dp)
            .height(GlassButton.side)
            .glassButton(CircleShape)
            .padding(horizontal = 8.dp),
    ) {
        InputField(
            state.prompt(page),
            state.query,
            state::type,
            Modifier.fillMaxSize(),
            icon = Symbol.Search,
            variant = InputVariant.Bare,
            size = ControlSize.Large,
            clearable = true,
            focusRequester = field,
            keyboardOptions = KeyboardOptions(
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
                imeAction = if (page.isTyped) ImeAction.Go else ImeAction.Search,
            ),
            onSubmit = state::runHighlighted,
        )
    }
}

private val noticeHeight = 36.dp

@Composable
private fun Results(state: PanelState, page: PanelPage, onDrag: () -> Unit) {
    val palette = LocalPalette.current
    val sections = state.sections(page)
    val items = sections.flatMap { it.items }
    val refused = state.notice(page)
    val list = rememberLazyListState()
    LaunchedEffect(list) {
        list.interactionSource.interactions.collect { if (it is DragInteraction.Start) onDrag() }
    }
    LaunchedEffect(state.highlighted) {
        val position = listPosition(sections, state.highlighted) ?: return@LaunchedEffect
        val visible = list.layoutInfo.visibleItemsInfo
        val shown = visible.any { it.index == position && it.offset >= 0 && it.offset + it.size <= list.layoutInfo.viewportEndOffset }
        if (!shown) list.animateScrollToItem(position)
    }
    LazyColumn(Modifier.fillMaxSize(), state = list, contentPadding = PaddingValues(bottom = 32.dp)) {
        if (items.isEmpty()) {
            item("empty") {
                Text(
                    state.emptyText(page),
                    Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 28.dp),
                    size = 13f,
                    color = palette.mutedStrongerForeground,
                    align = TextAlign.Center,
                )
            }
        }
        for (section in sections) {
            item("section-${section.title}") {
                Text(
                    section.title,
                    Modifier.padding(start = PanelRow.sideMargin + 10.dp, end = PanelRow.sideMargin + 10.dp, top = 10.dp, bottom = 4.dp),
                    size = 12f,
                    weight = FontWeight.Medium,
                    color = palette.mutedStrongerForeground,
                )
            }
            items(section.items, key = { "${section.title}/${it.id}" }) { item ->
                PanelRow(item, lit = state.steered && item.selectable && item.index == state.highlighted) { state.run(item) }
            }
        }
        if (refused != null) {
            item("notice") {
                Box(Modifier.fillMaxWidth().height(noticeHeight).padding(horizontal = PanelRow.sideMargin + 10.dp), contentAlignment = Alignment.CenterStart) {
                    Text(refused, size = 12f, color = palette.destructive, maxLines = 2)
                }
            }
        }
    }
}

/** Where the selectable item at `index` is in the list, past the section titles. */
private fun listPosition(sections: List<PanelSection>, index: Int): Int? {
    var position = 0
    for (section in sections) {
        position += 1
        for (item in section.items) {
            if (item.index == index && item.selectable) return position
            position += 1
        }
    }
    return null
}
