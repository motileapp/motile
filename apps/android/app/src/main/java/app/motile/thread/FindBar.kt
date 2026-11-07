package app.motile.thread

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.closeFind
import app.motile.core.setFindQuery
import app.motile.core.stepFind
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalForeground
import app.motile.theme.LocalRow
import app.motile.theme.LocalSurface
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.InputField
import app.motile.ui.InputVariant
import app.motile.ui.Text
import app.motile.ui.onEscape

object FindBar {
    private val radius = Radius.lg
    /** Its last button's corner runs along the bar's. */
    internal val margin = radius - ControlSize.Regular.radius
    internal val shape = RoundedCornerShape(radius)
    /** What the bar covers of the transcript's top, which the transcript keeps free above its first row while it shows. */
    val room = 6.dp + ControlSize.Regular.height + margin * 2 + 8.dp
    /** The room above the bar, and between it and the toast under it. */
    val gap = 6.dp
}

/**
 * Finding in the open thread, as a browser finds in a page: a card over the top of the thread, over
 * the toast, with the field, how many matches there are and which one is current, and the buttons
 * that go to the one before and the next and close it.
 */
@Composable
fun FindBar(modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val find = store.find
    val palette = Theme.colors
    val size = ControlSize.Regular
    val focus = remember { FocusRequester() }
    // A bar that has just come in takes the keys once it is placed.
    LaunchedEffect(find.focusRequests) {
        withFrameNanos {}
        runCatching { focus.requestFocus() }
    }
    CompositionLocalProvider(LocalSurface provides Surface.Popover, LocalRow provides null, LocalForeground provides palette.foreground) {
        Row(
            modifier
                .fillMaxWidth()
                .background(palette.popover, FindBar.shape)
                .border(1.dp, palette.borderPopover, FindBar.shape)
                .padding(FindBar.margin)
                .height(size.height),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.weight(1f)) {
                InputField(
                    "Find in thread", find.query, { store.setFindQuery(it) },
                    Modifier
                        .onEscape { store.closeFind() }
                        .onPreviewKeyEvent { event ->
                            if (event.key != Key.Enter && event.key != Key.NumPadEnter) return@onPreviewKeyEvent false
                            if (event.type == KeyEventType.KeyDown) store.stepFind(back = event.isShiftPressed)
                            true
                        },
                    icon = Symbol.Search,
                    variant = InputVariant.Bare,
                    focusRequester = focus,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                    onSubmit = { store.stepFind(back = false) },
                    selectAllRequests = find.focusRequests,
                )
            }
            val label = find.label
            if (label.isNotEmpty()) {
                Text(
                    label, Modifier.padding(horizontal = size.gap), size = size.textSize,
                    color = if (find.count == 0) palette.mutedStrongerForeground else palette.mutedForeground, maxLines = 1,
                )
            }
            Box(Modifier.padding(horizontal = size.gap).width(1.dp).height(size.height - FindBar.margin * 2).background(palette.borderPopover))
            val help = { words: String, command: String -> store.shortcuts.label(command)?.let { "$words ($it)" } ?: words }
            ActionButton(icon = Symbol.ChevronUp, help = help("Previous match", "find.previous"), disabled = find.count == 0) { store.stepFind(back = true) }
            ActionButton(icon = Symbol.ChevronDown, help = help("Next match", "find.next"), disabled = find.count == 0) { store.stepFind(back = false) }
            ActionButton(icon = Symbol.X, help = "Close") { store.closeFind() }
        }
    }
}
