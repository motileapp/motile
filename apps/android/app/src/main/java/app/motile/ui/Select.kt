package app.motile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette

/**
 * One of `options`, chosen from a menu: a secondary button that fills its width, with the chosen
 * one's title, or the `placeholder` without one, at its start and a chevron at its end. The menu
 * checks the chosen one.
 */
@Composable
fun Select(
    options: List<String>,
    selected: Int?,
    onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String = "Choose",
    size: ControlSize = ControlSize.Regular,
    disabled: Boolean = false,
) {
    val palette = LocalPalette.current
    val menu = rememberMenuState()
    val chosen = selected?.let { options.getOrNull(it) }
    val title = chosen ?: placeholder
    Box(modifier.fillMaxWidth()) {
        ControlButton(
            ControlLook(ButtonVariant.Secondary, size),
            onClick = { menu.open() },
            modifier = Modifier.fillMaxWidth(),
            disabled = disabled,
            help = title,
            role = Role.DropdownList,
        ) {
            Row(
                Modifier.fillMaxWidth().height(size.height).padding(start = size.padding, end = size.padding - size.chevronOutset),
                horizontalArrangement = Arrangement.spacedBy(size.gap),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val color = if (chosen == null) palette.mutedStrongerForeground else LocalForeground.current
                Text(title, Modifier.weight(1f), style = size.font, color = color, maxLines = 1)
                Icon(Symbol.ChevronDown, size.chevron, color = palette.mutedStrongerForeground, trimmed = true)
            }
        }
        MenuPopup(menu) {
            options.forEachIndexed { index, option -> item(option, checked = index == selected) { onSelect(index) } }
        }
    }
}
