package app.motile.ui

import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.round
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.opacity
import app.motile.theme.IconSize
import app.motile.theme.iconSize
import app.motile.theme.symbolSide

/** What a menu lists: built when it opens, from the lambda a menu is given. */
sealed interface MenuEntry {
    class Item(
        val title: String,
        val symbol: Symbol?,
        val picture: (@Composable () -> Unit)?,
        /** A line under the title, like why it can't run now. */
        val detail: String?,
        /** Whether it is on, for one that turns something on; null for one that only runs. */
        val checked: Boolean?,
        val destructive: Boolean,
        val disabled: Boolean,
        val run: () -> Unit,
    ) : MenuEntry

    data object Divider : MenuEntry

    class Section(val title: String?, val entries: List<MenuEntry>) : MenuEntry

    class Submenu(val title: String, val symbol: Symbol?, val entries: List<MenuEntry>) : MenuEntry

    /** Words that only say something, like a warning under the actions. */
    class Note(val text: String) : MenuEntry
}

/** Builds a menu's entries, as SwiftUI's menu content: `item`, `toggle`, `divider`, `section`, `submenu`, `note`. */
class MenuBuilder {
    internal val entries = mutableListOf<MenuEntry>()

    fun item(
        title: String,
        symbol: Symbol? = null,
        picture: (@Composable () -> Unit)? = null,
        detail: String? = null,
        checked: Boolean? = null,
        destructive: Boolean = false,
        disabled: Boolean = false,
        run: () -> Unit,
    ) {
        entries += MenuEntry.Item(title, symbol, picture, detail, checked, destructive, disabled, run)
    }

    /** A choice that is on or off, with its check mark; `onToggle` is given what it turns into. */
    fun toggle(
        title: String,
        isOn: Boolean,
        symbol: Symbol? = null,
        picture: (@Composable () -> Unit)? = null,
        disabled: Boolean = false,
        onToggle: (Boolean) -> Unit,
    ) = item(title, symbol, picture, checked = isOn, disabled = disabled) { onToggle(!isOn) }

    fun divider() {
        entries += MenuEntry.Divider
    }

    fun section(title: String? = null, build: MenuBuilder.() -> Unit) {
        entries += MenuEntry.Section(title, MenuBuilder().apply(build).entries)
    }

    fun submenu(title: String, symbol: Symbol? = null, build: MenuBuilder.() -> Unit) {
        entries += MenuEntry.Submenu(title, symbol, MenuBuilder().apply(build).entries)
    }

    fun note(text: String) {
        entries += MenuEntry.Note(text)
    }
}

/** Whether a menu is open, and where. */
@Stable
class MenuState {
    var expanded by mutableStateOf(false)
        private set
    var at by mutableStateOf<IntOffset?>(null)
        private set
    /** Where the last press on the view went down, which `pressPosition` keeps. */
    internal var pressedAt: IntOffset? = null

    /** Opens it under its view, or at a point of the view. */
    fun open(at: IntOffset? = null) {
        this.at = at
        expanded = true
    }

    /** Opens it where the finger that holds the view is. */
    fun openAtPress() = open(pressedAt)

    fun close() {
        expanded = false
    }
}

@Composable
fun rememberMenuState(): MenuState = remember { MenuState() }

/** Keeps where presses on the view go down, so that a menu held open on it opens there. */
fun Modifier.pressPosition(state: MenuState): Modifier = pointerInput(state) {
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
        state.pressedAt = down.position.round()
    }
}

/** The menu while `state` is open, dropped from the view it lies in. */
@Composable
fun MenuPopup(state: MenuState, items: MenuBuilder.() -> Unit) {
    var pages by remember { mutableStateOf(listOf<MenuEntry.Submenu>()) }
    LaunchedEffect(state.expanded) { if (!state.expanded) pages = emptyList() }
    Dropdown(state.expanded, onDismiss = state::close, at = state.at) {
        val root = MenuBuilder().apply(items).entries
        val page = pages.lastOrNull()
        MenuPage(
            title = page?.title,
            entries = page?.entries ?: root,
            back = { pages = pages.dropLast(1) },
            open = { pages = pages + it },
            close = state::close,
        )
    }
}

private val menuPadding = 4.dp
private const val menuText = 14f
/** The size of a menu's symbols and of the pictures its items show. */
val menuIconSize = iconSize(text = menuText)

@Composable
private fun MenuPage(title: String?, entries: List<MenuEntry>, back: () -> Unit, open: (MenuEntry.Submenu) -> Unit, close: () -> Unit) {
    val rows = flatten(entries)
    val checks = rows.any { it is MenuEntry.Item && it.checked != null }
    val palette = LocalPalette.current
    DropdownCard(Modifier.width(IntrinsicSize.Max).widthIn(min = 200.dp, max = 320.dp), radius = Radius.lg) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(menuPadding)) {
            if (title != null) {
                MenuRow(onClick = back) {
                    Icon(Symbol.ChevronLeft, menuIconSize, color = palette.mutedForeground)
                    Text(title, Modifier.weight(1f), size = menuText, weight = FontWeight.SemiBold, maxLines = 1)
                }
                ThemeDivider(Modifier.padding(vertical = menuPadding))
            }
            for (row in rows) {
                when (row) {
                    is MenuEntry.Item -> ItemRow(row, checks, close)
                    is MenuEntry.Submenu -> MenuRow(onClick = { open(row) }) {
                        if (checks) Box(Modifier.size(symbolSide(menuIconSize)))
                        Mark(row.symbol, null)
                        Text(row.title, Modifier.weight(1f), size = menuText, maxLines = 2)
                        Icon(Symbol.ChevronRight, menuIconSize, color = palette.mutedForeground)
                    }
                    is MenuEntry.Divider -> ThemeDivider(Modifier.padding(vertical = menuPadding))
                    is MenuEntry.Section -> Text(
                        row.title ?: "", Modifier.padding(horizontal = 8.dp, vertical = 6.dp), size = 12f, weight = FontWeight.Medium,
                        color = palette.mutedForeground, maxLines = 1,
                    )
                    is MenuEntry.Note -> Text(row.text, Modifier.padding(horizontal = 8.dp, vertical = 6.dp), size = 12f, color = palette.mutedForeground)
                }
            }
        }
    }
}

/** The page as rows: a section becomes its title (as a `Section` without entries) between dividers, and no two dividers touch. */
private fun flatten(entries: List<MenuEntry>): List<MenuEntry> {
    val rows = mutableListOf<MenuEntry>()
    fun divide() {
        if (rows.isNotEmpty() && rows.last() !is MenuEntry.Divider) rows += MenuEntry.Divider
    }
    for (entry in entries) {
        when (entry) {
            is MenuEntry.Section -> {
                divide()
                if (entry.title != null) rows += MenuEntry.Section(entry.title, emptyList())
                rows += flatten(entry.entries)
                divide()
            }
            is MenuEntry.Divider -> divide()
            else -> rows += entry
        }
    }
    while (rows.lastOrNull() is MenuEntry.Divider) rows.removeAt(rows.lastIndex)
    return rows
}

@Composable
private fun ItemRow(item: MenuEntry.Item, checks: Boolean, close: () -> Unit) {
    val palette = LocalPalette.current
    val color = if (item.destructive) palette.destructive else palette.foreground
    MenuRow(
        onClick = {
            close()
            item.run()
        },
        disabled = item.disabled,
        color = color,
    ) {
        if (checks) Icon(Symbol.Check, menuIconSize, Modifier.alpha(if (item.checked == true) 1f else 0f))
        Mark(item.symbol, item.picture)
        Column(Modifier.weight(1f)) {
            Text(item.title, size = menuText, maxLines = 2)
            if (item.detail != null) Text(item.detail, size = 12f, color = palette.mutedForeground, maxLines = 3)
        }
    }
}

@Composable
private fun Mark(symbol: Symbol?, picture: (@Composable () -> Unit)?) {
    if (symbol != null) {
        Icon(symbol, menuIconSize)
        return
    }
    if (picture == null) return
    Box(Modifier.size(symbolSide(menuIconSize)), contentAlignment = Alignment.Center) { picture() }
}

@Composable
private fun MenuRow(
    onClick: () -> Unit,
    disabled: Boolean = false,
    color: Color = LocalPalette.current.foreground,
    content: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit,
) {
    HighlightButton(onClick, Modifier.fillMaxWidth().opacity(Opacity.Disabled, on = disabled), enabled = !disabled, radius = Radius.sm) {
        CompositionLocalProvider(LocalForeground provides color) {
            Row(
                Modifier.heightIn(min = pressable(28f)).padding(horizontal = 8.dp, vertical = menuPadding),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalAlignment = Alignment.CenterVertically,
                content = content,
            )
        }
    }
}

/**
 * A menu that looks and is sized as the button is. One with words shows what is chosen and a
 * chevron after it; one without shows a symbol or a picture, and no chevron, unless `chevron` says
 * otherwise, as the half of a split button does.
 */
@Composable
fun ActionMenu(
    title: String? = null,
    icon: Symbol? = null,
    modifier: Modifier = Modifier,
    picture: (@Composable () -> Unit)? = null,
    help: String? = null,
    variant: ButtonVariant = ButtonVariant.Ghost,
    size: ControlSize = ControlSize.Regular,
    symbolSize: IconSize? = null,
    gap: Dp? = null,
    padding: Dp? = null,
    pending: Boolean = false,
    round: Boolean = false,
    joined: Set<HorizontalEdge> = emptySet(),
    tint: Color? = null,
    margin: PaddingValues = PaddingValues(0.dp),
    disabled: Boolean = false,
    chevron: Boolean = title != null,
    items: MenuBuilder.() -> Unit,
) {
    val menu = rememberMenuState()
    val look = ControlLook(variant, size, round = round, joined = joined, tint = tint)
    Box(modifier) {
        ControlButton(look, onClick = { menu.open() }, pending = pending, disabled = disabled, margin = margin, help = help ?: title) {
            ControlLabel(
                title, ControlIcon.of(icon, picture), size, chevron = chevron, pending = pending, symbolSize = symbolSize, gap = gap, padding = padding,
            )
        }
        MenuPopup(menu, items)
    }
}

/** A menu behind a label of the caller's own, which dims while it is pressed. */
@Composable
fun Menu(
    items: MenuBuilder.() -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    contentAlignment: Alignment = Alignment.Center,
    label: @Composable BoxScope.() -> Unit,
) {
    val menu = rememberMenuState()
    Box(modifier, propagateMinConstraints = true) {
        DimButton(onClick = { menu.open() }, enabled = enabled, contentAlignment = contentAlignment, content = label)
        MenuPopup(menu, items)
    }
}

/** A view that opens a menu where it is held, as iOS's context menus do, and runs `onClick` when it is tapped. */
@Composable
fun ContextMenuArea(
    items: MenuBuilder.() -> Unit,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    content: @Composable BoxScope.() -> Unit,
) {
    val menu = rememberMenuState()
    Box(
        modifier
            .pressPosition(menu)
            .combinedClickable(interactionSource = null, indication = null, onLongClick = { menu.openAtPress() }, onClick = { onClick?.invoke() }),
    ) {
        content()
        MenuPopup(menu, items)
    }
}
