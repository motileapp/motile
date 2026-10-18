package app.motile.sidebar

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.motile.LocalStore
import app.motile.core.AppStore
import app.motile.core.MessageMatch
import app.motile.core.SearchResult
import app.motile.core.Time
import app.motile.core.searchResults
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.ControlSize
import app.motile.theme.LocalForeground
import app.motile.theme.Opacity
import app.motile.theme.Palette
import app.motile.theme.Radius
import app.motile.theme.Theme
import app.motile.theme.opacity
import app.motile.ui.HighlightButton
import app.motile.ui.Highlight
import app.motile.ui.Icon
import app.motile.ui.InputField
import app.motile.ui.InputVariant
import app.motile.ui.KeyCaps
import app.motile.ui.ProjectIcon
import app.motile.ui.PullRequestLabel
import app.motile.ui.Spinner
import app.motile.ui.Text

/** The field the sidebar searches its threads with. `bare` leaves out its background and height, for a field that lies on glass. */
@Composable
fun SearchField(
    text: String,
    onTextChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    bare: Boolean = false,
    clearable: Boolean = true,
    focusRequester: FocusRequester? = null,
    onFocusChange: ((Boolean) -> Unit)? = null,
    onSubmit: (() -> Unit)? = null,
) {
    InputField(
        "Search", text, onTextChange, modifier,
        icon = Symbol.Search,
        variant = if (bare) InputVariant.Bare else InputVariant.Filled,
        size = if (Platform.scale > 1) ControlSize.Large else ControlSize.Regular,
        clearable = clearable,
        focusRequester = focusRequester,
        onFocusChange = onFocusChange,
        onSubmit = onSubmit,
    )
}

/**
 * The arrow keys of a keyboard move the highlight through what the search found, round from
 * either end. They read what is found as they are pressed.
 */
fun Modifier.steersSearch(store: AppStore, highlighted: Highlight): Modifier = onPreviewKeyEvent { event ->
    if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
    val step = when (event.key) {
        Key.DirectionDown -> 1
        Key.DirectionUp -> -1
        else -> return@onPreviewKeyEvent false
    }
    val count = store.searchResults.size
    if (count == 0) return@onPreviewKeyEvent false
    highlighted.index = (highlighted.index + step + count) % count
    true
}

/** The line the search shows when it found nothing, in the place of its first row. */
@Composable
fun SearchEmptyLine(size: Float) {
    val store = LocalStore.current
    val color = Theme.colors.mutedStrongerForeground
    CompositionLocalProvider(LocalForeground provides color) {
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            if (store.messageSearch.searching) {
                Spinner(size)
                Text("Searching messages", size = size)
            } else {
                Text("No threads found", size = size)
            }
        }
    }
}

/**
 * What the search found, on two lines that are as tall whatever was found: its project's icon, its
 * title and what it is up to, then the message it was found by, or else its project.
 */
@Composable
fun SearchResultRow(
    result: SearchResult,
    /** The arrow keys are on it. */
    highlighted: Boolean,
    modifier: Modifier = Modifier,
    /** The keys that open it, while they are held. */
    jump: String? = null,
    onLongClick: (() -> Unit)? = null,
    open: (SearchResult) -> Unit,
) {
    val store = LocalStore.current
    val palette = Theme.colors
    val project = when (result) {
        is SearchResult.Draft -> store.project(result.listed.draft.projectID)
        is SearchResult.Thread -> store.project(result.thread.projectID)?.seen(result.thread)
    }
    val title = when (result) {
        is SearchResult.Draft -> result.listed.preview
        is SearchResult.Thread -> result.thread.title
    }
    val isDone = result.threadInfo?.isDone == true
    HighlightButton(onClick = { open(result) }, modifier = modifier, radius = Radius.md, lit = highlighted, inset = rowMargin, opaque = true, onLongClick = onLongClick) {
        Column(
            Modifier
                .fillMaxWidth()
                .padding(rowMargin)
                .padding(start = ThreadRow.sidePadding, end = ThreadRow.sidePadding, top = ThreadRow.topPadding, bottom = ThreadRow.bottomPadding),
        ) {
            Row(Modifier.height(scaled(22f).dp), verticalAlignment = Alignment.CenterVertically) {
                ProjectIcon(project, Modifier.opacity(Opacity.Disabled, on = isDone && !highlighted), size = 14f)
                Text(
                    title, Modifier.weight(1f).padding(start = 6.dp, end = 6.dp), size = 13f, weight = FontWeight.Medium, maxLines = 1,
                    color = when {
                        highlighted -> palette.foreground
                        isDone -> palette.mutedStrongerForeground
                        else -> palette.mutedForeground
                    },
                )
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                    ResultStatus(result, jump)
                }
            }
            val found = (result as? SearchResult.Thread)?.match
            val detail = Modifier.fillMaxWidth().height(scaled(18f).dp)
            if (found != null) {
                Text(excerpt(found, palette), detail, size = 12f, maxLines = 1)
            } else {
                val place = project?.name ?: result.threadInfo?.let { lastComponent(it.cwd) } ?: "No project"
                Text(place, detail, size = 12f, maxLines = 1, color = palette.mutedStrongerForeground)
            }
        }
    }
}

@Composable
private fun ResultStatus(result: SearchResult, jump: String?) {
    val store = LocalStore.current
    val palette = Theme.colors
    when (result) {
        is SearchResult.Draft -> CompositionLocalProvider(LocalForeground provides palette.mutedForeground) {
            Row(horizontalArrangement = Arrangement.spacedBy(3.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(Symbol.File, 11f)
                Text("Draft", size = 11f, weight = FontWeight.Medium)
            }
        }
        is SearchResult.Thread -> {
            val thread = result.thread
            val readOnly = store.onRemovedServer(thread)
            if (!readOnly) store.project(thread.projectID)?.seen(thread)?.pullRequest(thread)?.let { PullRequestLabel(it, colored = !thread.isDone) }
            when {
                jump != null -> CompositionLocalProvider(LocalForeground provides palette.foreground) { KeyCaps(listOf(jump)) }
                readOnly -> ThreadStatus(thread, readOnly = true)
                thread.isDone -> Text(Time.ago(thread.doneAt ?: thread.updatedAt, AgoClock.now), size = 11f, color = palette.mutedStrongerForeground)
                else -> ThreadStatus(thread)
            }
        }
    }
}

/** Who wrote the message, then the message with what was searched for set off. */
private fun excerpt(found: MessageMatch, palette: Palette): AnnotatedString = buildAnnotatedString {
    val size = (12 * Platform.scale).sp
    withStyle(SpanStyle(color = palette.mutedForeground, fontWeight = FontWeight.Medium, fontSize = size)) {
        append(if (found.byUser) "You: " else "Agent: ")
    }
    for (part in found.parts) {
        val style = if (part.matched) {
            SpanStyle(color = palette.foreground, fontWeight = FontWeight.SemiBold)
        } else {
            SpanStyle(color = palette.mutedStrongerForeground)
        }
        withStyle(style) { append(part.text) }
    }
}
