package app.motile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.motile.platform.Platform
import app.motile.theme.ControlSize
import app.motile.theme.IconSize
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.Radius

/**
 * How every list a dropdown draws is set: rows as tall as a regular control, with the same room
 * around them.
 */
object DropdownList {
    /** The room between the card's edge and its rows. */
    val padding = 4.dp
    val rowHeight = maxOf(ControlSize.Regular.height, Platform.minimumPress.dp)
    /** The room between a row's side and what it shows. */
    val rowPadding = 8.dp
    /** The room between a row's symbol and its words. */
    val gap = 8.dp
    const val textSize = 13f
    val symbol = IconSize.Md
    val check = IconSize.Sm
    /** The room between two rows' lights: only drawn, so that the rows' presses meet. */
    val light = PaddingValues(top = 1.dp, bottom = 1.dp)
}

/** A row of a dropdown's list, lit while pressed or `lit`, as by the arrow keys. */
@Composable
fun DropdownRow(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    lit: Boolean = false,
    content: @Composable RowScope.() -> Unit,
) {
    HighlightButton(onClick, modifier.fillMaxWidth(), radius = Radius.sm, lit = lit, inset = DropdownList.light) {
        CompositionLocalProvider(LocalForeground provides LocalPalette.current.foreground) {
            Row(
                Modifier.fillMaxWidth().heightIn(min = DropdownList.rowHeight).padding(horizontal = DropdownList.rowPadding),
                horizontalArrangement = Arrangement.spacedBy(DropdownList.gap),
                verticalAlignment = Alignment.CenterVertically,
                content = content,
            )
        }
    }
}
