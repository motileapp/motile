package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Surface
import app.motile.theme.Theme
import androidx.compose.ui.unit.dp

/** How a field is set off from what it lies on. */
enum class InputVariant {
    /** A fill alone. */
    Filled,
    /** No background or height of its own, for a field that lies on glass, with its symbol in the foreground. */
    Bare,
}

/**
 * A line to type in, as tall as a button of its size, with a symbol before it when it has one and a
 * button that empties it when it is `clearable`, as Esc does while it has text. A `secure` one shows
 * dots for what is typed. `onSubmit` runs on the keyboard's action key.
 */
@Composable
fun InputField(
    placeholder: String,
    text: String,
    onTextChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    icon: Symbol? = null,
    variant: InputVariant = InputVariant.Filled,
    size: ControlSize = ControlSize.Regular,
    clearable: Boolean = false,
    monospaced: Boolean = false,
    secure: Boolean = false,
    focusRequester: FocusRequester? = null,
    onFocusChange: ((Boolean) -> Unit)? = null,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    onSubmit: (() -> Unit)? = null,
) {
    val palette = LocalPalette.current
    val bare = variant == InputVariant.Bare
    val focus = focusRequester ?: remember { FocusRequester() }
    val clears = clearable && text.isNotEmpty()
    val clearInset = (size.height - ControlSize.Small.height) / 2
    val enabled = LocalEnabled.current
    val height = if (bare) Modifier.fillMaxHeight() else Modifier.height(size.height)
    Row(
        modifier
            .then(height)
            .fieldFrame(size, filled = !bare)
            .clickable(interactionSource = null, indication = null, enabled = enabled) { focus.requestFocus() }
            .padding(start = size.padding - fieldInset, end = if (clears) clearInset else size.padding - fieldInset),
        horizontalArrangement = Arrangement.spacedBy(size.gap),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) Icon(icon, size.smallSymbol, color = if (bare) palette.foreground else palette.mutedStrongerForeground)
        val style = fieldStyle(size, monospaced, palette.foreground)
        BasicTextField(
            value = text,
            onValueChange = onTextChange,
            modifier = Modifier
                .weight(1f)
                .focusRequester(focus)
                .onFocusChanged { onFocusChange?.invoke(it.isFocused) }
                .onEscape(if (clears) ({ onTextChange("") }) else null),
            enabled = enabled,
            singleLine = true,
            textStyle = style,
            cursorBrush = SolidColor(palette.foreground),
            visualTransformation = if (secure) PasswordVisualTransformation() else VisualTransformation.None,
            keyboardOptions = if (secure) keyboardOptions.copy(keyboardType = KeyboardType.Password) else keyboardOptions,
            keyboardActions = KeyboardActions { onSubmit?.invoke() },
            decorationBox = { field ->
                Box(contentAlignment = Alignment.CenterStart) {
                    if (text.isEmpty()) Text(placeholder, style = style, color = palette.mutedStrongerForeground, maxLines = 1)
                    field()
                }
            },
        )
        if (clears) ActionButton(icon = Symbol.X, help = "Clear", size = ControlSize.Small) { onTextChange("") }
    }
}

/** The text of a field of the size. */
internal fun fieldStyle(size: ControlSize, monospaced: Boolean, color: Color): TextStyle {
    val style = if (monospaced) Theme.uiMono(size.textSize + 0.5f) else Theme.ui(size.textSize + 0.5f)
    return style.copy(color = color)
}

/** The fill of a field of the size, with no border, focused or not. On a card, which is as dark as input, it takes the card's accent. */
@Composable
fun Modifier.fieldFrame(size: ControlSize, filled: Boolean = true): Modifier {
    if (!filled) return this
    val surface = LocalSurface.current
    val fill = if (surface == Surface.Card) surface.color(Surface.Layer.Box) else LocalPalette.current.input
    return background(fill, RoundedCornerShape(size.radius))
}

/** How much nearer its sides a field's text stands than a button's words do. */
internal val fieldInset = 2.dp
