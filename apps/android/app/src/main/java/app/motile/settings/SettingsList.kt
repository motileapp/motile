package app.motile.settings

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.motile.layout.KeyboardFocus
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.ControlSize
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.Surface
import app.motile.theme.Opacity
import app.motile.theme.opacity
import app.motile.ui.ActionMenu
import app.motile.ui.ButtonVariant
import app.motile.ui.Card
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.InputField
import app.motile.ui.InputVariant
import app.motile.ui.LocalEnabled
import app.motile.ui.Switch
import app.motile.ui.Text
import app.motile.ui.TextArea
import app.motile.ui.ThemeDivider
import app.motile.ui.pressable
import kotlinx.coroutines.delay

/** The text of a row, as iOS's body. */
internal const val ROW_TEXT = 14.5f
/** The text under a row's title, and of the headers and footers, as iOS's footnote. */
internal const val NOTE_TEXT = 12f
private val rowPadding = 16.dp

/** One of the system's inset groups: a header over a card of rows and a footer under it. Its `id` is what a search scrolls to. */
class SettingsGroup(
    val id: String?,
    /** The ids of the groups SwiftUI gives its rows, which a search finds this one by too. */
    val also: List<String>,
    val header: (@Composable () -> Unit)?,
    val footer: String?,
    val rows: List<@Composable () -> Unit>,
    /** Its rows lie on the page, not on a card, like a project's name under its icon. */
    val bare: Boolean,
)

/** Builds a page's groups, as SwiftUI's sections in a list. */
class SettingsGroups {
    internal val groups = mutableListOf<SettingsGroup>()

    fun group(
        id: String? = null,
        also: List<String> = emptyList(),
        header: String? = null,
        footer: String? = null,
        headerContent: (@Composable () -> Unit)? = null,
        bare: Boolean = false,
        rows: SettingsRows.() -> Unit,
    ) {
        val built = SettingsRows().apply(rows).rows
        if (built.isEmpty() && footer == null) return
        val shownHeader = headerContent ?: header?.let { title -> @Composable { GroupHeader(title) } }
        groups += SettingsGroup(id, also, shownHeader, footer, built, bare)
    }

    /** Said on a page with nothing to set yet. */
    fun note(text: String) = group { row { RowText(text, color = LocalPalette.current.mutedForeground) } }
}

class SettingsRows {
    internal val rows = mutableListOf<@Composable () -> Unit>()

    fun row(content: @Composable () -> Unit) {
        rows += content
    }
}

/**
 * The settings' list: the system's inset groups, drawn on the theme's colours. It scrolls to the
 * group `target` names and lights it for a moment, then calls `reached`.
 */
@Composable
fun SettingsList(
    modifier: Modifier = Modifier,
    target: String? = null,
    reached: () -> Unit = {},
    state: LazyListState = rememberLazyListState(),
    build: SettingsGroups.() -> Unit,
) {
    val groups = SettingsGroups().apply(build).groups
    var lit by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(target) {
        val index = groups.indexOfFirst { it.names(target) }.takeIf { target != null && it >= 0 } ?: return@LaunchedEffect
        state.animateScrollToItem(index)
        val item = state.layoutInfo.visibleItemsInfo.firstOrNull { it.index == index }
        if (item != null) {
            val viewport = state.layoutInfo.viewportEndOffset - state.layoutInfo.viewportStartOffset
            state.animateScrollBy(-(viewport - item.size) / 2f)
        }
        lit = target
        reached()
    }
    LaunchedEffect(lit) {
        if (lit == null) return@LaunchedEffect
        delay(1200)
        lit = null
    }
    LazyColumn(
        modifier.fillMaxSize(),
        state = state,
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 40.dp),
    ) {
        items(groups.size, key = { groups[it].id ?: "#$it" }) { index ->
            Group(groups[index], lit = lit != null && groups[index].names(lit))
        }
    }
}

private fun SettingsGroup.names(target: String?): Boolean = target != null && (id == target || target in also)

@Composable
private fun Group(group: SettingsGroup, lit: Boolean) {
    val palette = LocalPalette.current
    Column(Modifier.fillMaxWidth().padding(top = 20.dp)) {
        group.header?.let {
            Box(Modifier.padding(start = rowPadding, end = rowPadding, bottom = 7.dp)) {
                CompositionLocalProvider(LocalForeground provides palette.mutedForeground) { it() }
            }
        }
        if (group.bare) {
            Column(Modifier.fillMaxWidth()) { group.rows.forEach { it() } }
        } else if (group.rows.isNotEmpty()) {
            Card(Modifier.fillMaxWidth()) {
                val light by animateColorAsState(
                    if (lit) Surface.Card.color(Surface.Layer.RowSelected) else Surface.Card.color,
                    tween(if (lit) 200 else 900),
                    label = "found",
                )
                Column(Modifier.background(light)) {
                    CompositionLocalProvider(LocalForeground provides palette.foreground) {
                        group.rows.forEachIndexed { index, row ->
                            if (index > 0) ThemeDivider(Modifier.padding(start = rowPadding))
                            row()
                        }
                    }
                }
            }
        }
        group.footer?.let {
            Text(it, Modifier.padding(start = rowPadding, end = rowPadding, top = 7.dp), size = NOTE_TEXT, color = palette.mutedForeground)
        }
    }
}

@Composable
private fun GroupHeader(title: String) {
    Text(title, size = NOTE_TEXT, weight = FontWeight.Medium, color = LocalPalette.current.mutedForeground, maxLines = 1)
}

/** A row's frame: as tall as a finger needs, with the room the system's rows have at their sides. */
@Composable
fun SettingsRow(modifier: Modifier = Modifier, content: @Composable RowScope.() -> Unit) {
    Row(
        modifier.fillMaxWidth().heightIn(min = pressable(38f)).padding(horizontal = rowPadding, vertical = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
        content = content,
    )
}

/** A row that does something when pressed, lit while it is. */
@Composable
fun SettingsPressRow(onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, onLongClick: (() -> Unit)? = null, content: @Composable RowScope.() -> Unit) {
    val usable = enabled && LocalEnabled.current
    HighlightButton(onClick, modifier.fillMaxWidth().opacity(Opacity.Disabled, on = !usable), enabled = enabled, radius = 0.dp, onLongClick = onLongClick) {
        SettingsRow(content = content)
    }
}

/** A row that opens a page: its content, then a chevron. */
@Composable
fun SettingsLink(onClick: () -> Unit, enabled: Boolean = true, content: @Composable RowScope.() -> Unit) {
    SettingsPressRow(onClick, enabled = enabled) {
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically, content = content)
        Icon(Symbol.ChevronRight, 13f, color = LocalPalette.current.mutedStrongerForeground)
    }
}

/** A button's row: its words alone, in the tint or the destructive colour. */
@Composable
fun SettingsButton(title: String, destructive: Boolean = false, enabled: Boolean = true, onClick: () -> Unit) {
    val palette = LocalPalette.current
    SettingsPressRow(onClick, enabled = enabled) {
        RowText(title, color = if (destructive) palette.destructive else palette.primary)
    }
}

/** A button's row in a list: its symbol and its words, in the tint. */
@Composable
fun SettingsActionLabel(title: String, symbol: Symbol) {
    val color = LocalPalette.current.primary
    Box(Modifier.size(SettingsTile.size.dp), contentAlignment = Alignment.Center) { Icon(symbol, 17f, color = color) }
    RowText(title, color = color)
}

/** A row's words. */
@Composable
fun RowText(text: String, modifier: Modifier = Modifier, color: Color = LocalForeground.current, maxLines: Int = Int.MAX_VALUE) {
    Text(text, modifier, size = ROW_TEXT, color = color, maxLines = maxLines)
}

/** A row's picture: its symbol, as wide as the agents' and projects' icons beside it. */
object SettingsTile {
    const val size = 30f
}

@Composable
fun SettingsTile(symbol: Symbol) {
    Box(Modifier.size(SettingsTile.size.dp), contentAlignment = Alignment.Center) { Icon(symbol, 16f, color = LocalPalette.current.foreground) }
}

/** What names something, with what it is under it and its picture before it. */
@Composable
fun RowScope.SettingsItem(title: String, description: String, truncates: TextOverflow = TextOverflow.Ellipsis, picture: @Composable () -> Unit) {
    picture()
    Column(Modifier.weight(1f).padding(vertical = 2.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        RowText(title, color = LocalPalette.current.foreground, maxLines = 1)
        Text(description, size = NOTE_TEXT, color = LocalPalette.current.mutedForeground, maxLines = 1, overflow = truncates)
    }
}

/** A name with its value after it, as SwiftUI's `LabeledContent`. */
@Composable
fun SettingsLabeled(title: String, value: String, truncates: TextOverflow = TextOverflow.Ellipsis) {
    SettingsRow {
        RowText(title, maxLines = 1)
        Text(
            value, Modifier.weight(1f), size = ROW_TEXT, color = LocalPalette.current.mutedForeground, maxLines = 1, overflow = truncates,
            align = androidx.compose.ui.text.style.TextAlign.End,
        )
    }
}

/** A name with something of its own after it, like an update's button. */
@Composable
fun SettingsLabeled(title: String, value: @Composable () -> Unit) {
    SettingsRow {
        RowText(title, Modifier.weight(1f), maxLines = 1)
        value()
    }
}

@Composable
fun SettingsToggle(title: String, isOn: Boolean, onChange: (Boolean) -> Unit) {
    SettingsRow {
        RowText(title, Modifier.weight(1f))
        Switch(isOn, onChange)
    }
}

/** A choice of a few, its name before the menu that shows the one chosen, as SwiftUI's picker in a list. */
@Composable
fun <T> SettingsPicker(title: String, options: List<Pair<String, T>>, selection: T, onSelect: (T) -> Unit) {
    SettingsRow {
        RowText(title, Modifier.weight(1f), maxLines = 2)
        ActionMenu(options.firstOrNull { it.second == selection }?.first ?: "", variant = ButtonVariant.Ghost) {
            for ((label, value) in options) item(label, checked = value == selection) { onSelect(value) }
        }
    }
}

/** A line to type in, in a row of its own. */
@Composable
fun SettingsField(placeholder: String, text: String, onTextChange: (String) -> Unit, monospaced: Boolean = false, secure: Boolean = false) {
    SettingsRow {
        Box(Modifier.weight(1f).height(ControlSize.Large.height)) {
            InputField(
                placeholder, text, onTextChange, variant = InputVariant.Bare, size = ControlSize.Large, monospaced = monospaced, secure = secure,
                keyboardOptions = plainKeyboard,
            )
        }
    }
}

/** Text to write a few lines of, in a row of its own. */
@Composable
fun SettingsTextEditor(placeholder: String, text: String, onTextChange: (String) -> Unit, monospaced: Boolean = false) {
    Box(Modifier.fillMaxWidth().padding(scaled(6f).dp)) {
        TextArea(
            placeholder, text, onTextChange, Modifier.fillMaxWidth(),
            size = ControlSize.Large, monospaced = monospaced, lines = 6, maxLines = 16,
        )
    }
}

/** What is typed as it is: no capitals or corrections, as names, folders and scripts need. */
internal val plainKeyboard = androidx.compose.foundation.text.KeyboardOptions(
    capitalization = androidx.compose.ui.text.input.KeyboardCapitalization.None,
    autoCorrectEnabled = false,
)
