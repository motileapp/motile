package app.motile.ui

import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import app.motile.layout.KeyboardFocus
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.foundation.layout.Box
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette

/**
 * A box to write more than a line in, in a field's frame. It is `lines` tall while empty, grows with
 * what is written up to `maxLines`, and scrolls past that. One that `fills` is as tall as there is
 * room instead. Return is a line break.
 */
@Composable
fun TextArea(
    placeholder: String,
    text: String,
    onTextChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    size: ControlSize = ControlSize.Regular,
    monospaced: Boolean = false,
    lines: Int = 3,
    maxLines: Int = 12,
    fills: Boolean = false,
    focusRequester: FocusRequester? = null,
) {
    val palette = LocalPalette.current
    val style = fieldStyle(size, monospaced, palette.foreground)
    val focus = focusRequester ?: remember { FocusRequester() }
    BasicTextField(
        value = text,
        onValueChange = onTextChange,
        modifier = modifier
            .then(if (fills) Modifier.fillMaxHeight() else Modifier)
            .fieldFrame(size)
            .focusRequester(focus)
            .onFocusChanged { KeyboardFocus.editable = it.isFocused },
        enabled = LocalEnabled.current,
        textStyle = style,
        minLines = lines,
        maxLines = if (fills) Int.MAX_VALUE else maxOf(lines, maxLines),
        cursorBrush = SolidColor(palette.foreground),
        decorationBox = { field ->
            Box(Modifier.padding(horizontal = size.padding - fieldInset, vertical = size.gap)) {
                if (text.isEmpty()) Text(placeholder, style = style, color = palette.mutedStrongerForeground)
                field()
            }
        },
    )
}
