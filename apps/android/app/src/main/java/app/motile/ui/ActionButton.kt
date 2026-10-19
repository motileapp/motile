package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.offset
import androidx.compose.ui.unit.dp
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalDark
import app.motile.theme.LocalPalette
import app.motile.theme.LocalRow
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.OverPicture
import app.motile.theme.Palette
import app.motile.theme.Surface
import app.motile.theme.at
import app.motile.theme.opacity
import app.motile.theme.LocalForeground

/** What a button is for, which is how it looks. */
enum class ButtonVariant {
    /** The one thing the view calls for. */
    Primary,
    /** The others beside it. */
    Secondary,
    /** The page's colour inside its border: the buttons of settings rows. */
    Outline,
    /** What can't be taken back. */
    Danger,
    /** What went well, or is done. */
    Success,
    /** What the user is asked to let happen. */
    Warning,
    /** Nothing until it is pressed: the buttons of bars and rows. */
    Ghost,
    /** In the colour of a link. */
    Link,
    /** A link's colour on a wash of it: a choice that is on. */
    Accent,
    /** A secondary button as it is in the dark, over a picture or a video. */
    Overlay,
}

/** The sides of a control. */
enum class HorizontalEdge { Leading, Trailing }

/** What a button shows before its words: a symbol, a picture of its own like a logo, or pictures side by side, as wide as they are. */
sealed interface ControlIcon {
    data class Glyph(val symbol: Symbol) : ControlIcon
    class Picture(val content: @Composable () -> Unit) : ControlIcon
    class Pictures(val content: @Composable () -> Unit) : ControlIcon

    companion object {
        fun of(symbol: Symbol?, picture: (@Composable () -> Unit)?, pictures: (@Composable () -> Unit)? = null): ControlIcon? =
            symbol?.let { Glyph(it) } ?: picture?.let { Picture(it) } ?: pictures?.let { Pictures(it) }
    }
}

/** Whether what is inside can be used, as SwiftUI's `disabled` says it for a whole view. */
val LocalEnabled = compositionLocalOf { true }

/** Disables every control inside while `disabled`. */
@Composable
fun Disabled(disabled: Boolean, content: @Composable () -> Unit) {
    val enabled = LocalEnabled.current && !disabled
    CompositionLocalProvider(LocalEnabled provides enabled, content = content)
}

/** The colours and the shape of a button or a menu. Pressed looks as lit does. */
data class ControlLook(
    val variant: ButtonVariant,
    val size: ControlSize,
    val selected: Boolean = false,
    val lit: Boolean = false,
    /** The layer it lies on. */
    val surface: Surface = Surface.Background,
    /** The row it lies on, as it is lit. */
    val row: Surface.Layer? = null,
    /** Round ends, for the composer's buttons. */
    val round: Boolean = false,
    /** The sides another control touches, which stay square. */
    val joined: Set<HorizontalEdge> = emptySet(),
    /** A colour that says something, like a pull request's state, in place of the quiet ones. */
    val tint: Color? = null,
    /** What it fills with when lit, where that isn't its variant's colour. */
    val hover: Color? = null,
) {
    fun foreground(palette: Palette): Color {
        if (tint != null && (variant == ButtonVariant.Ghost || variant == ButtonVariant.Secondary || variant == ButtonVariant.Outline)) return tint
        return when (variant) {
            ButtonVariant.Secondary, ButtonVariant.Outline -> palette.foreground
            ButtonVariant.Primary -> palette.primaryForeground
            ButtonVariant.Danger -> palette.destructiveForeground
            ButtonVariant.Success -> palette.successForeground
            ButtonVariant.Warning -> palette.warningForeground
            ButtonVariant.Ghost -> if (selected || lit) palette.foreground else palette.mutedForeground
            ButtonVariant.Link, ButtonVariant.Accent -> palette.primary
            ButtonVariant.Overlay -> OverPicture.foreground
        }
    }

    /** A button filled with a colour of its own lights by that colour at opacity lit. */
    fun fill(palette: Palette, dark: Boolean): Color {
        fun filled(color: Color) = if (lit) color.at(Opacity.Lit, dark) else color
        if (lit && hover != null) return hover
        return when (variant) {
            ButtonVariant.Primary -> filled(palette.primary)
            ButtonVariant.Secondary -> surface.color(if (selected || lit) Surface.Layer.ControlLit else Surface.Layer.Control, palette)
            ButtonVariant.Outline -> if (selected || lit) surface.color(Surface.Layer.Control, palette) else palette.background
            ButtonVariant.Danger -> filled(palette.destructive)
            ButtonVariant.Success -> filled(palette.success)
            ButtonVariant.Warning -> filled(palette.warning)
            ButtonVariant.Ghost -> when {
                selected -> surface.color(Surface.Layer.ControlLit, palette)
                lit -> ghostLit(palette)
                else -> Color.Transparent
            }
            ButtonVariant.Link -> if (lit) palette.primary.at(Opacity.ColorTint, dark) else Color.Transparent
            ButtonVariant.Accent -> palette.primary.at(if (lit) Opacity.ColorTintStronger else Opacity.ColorTint, dark)
            ButtonVariant.Overlay -> if (lit) OverPicture.controlLit else OverPicture.control
        }
    }

    /** A ghost button lights a step above the row it lies on. */
    private fun ghostLit(palette: Palette): Color = when (row) {
        Surface.Layer.RowSelected, Surface.Layer.ControlLit -> surface.color(Surface.Layer.ControlLitOnSelected, palette)
        Surface.Layer.Row, Surface.Layer.Control -> surface.color(Surface.Layer.ControlLit, palette)
        Surface.Layer.Chip -> palette.borderAccent
        else -> surface.color(Surface.Layer.Control, palette)
    }

    val shape: Shape
        get() {
            val leadingJoined = HorizontalEdge.Leading in joined
            val trailingJoined = HorizontalEdge.Trailing in joined
            if (round) {
                val leading = if (leadingJoined) 0 else 50
                val trailing = if (trailingJoined) 0 else 50
                return RoundedCornerShape(topStartPercent = leading, topEndPercent = trailing, bottomEndPercent = trailing, bottomStartPercent = leading)
            }
            val leading = if (leadingJoined) 0.dp else size.radius
            val trailing = if (trailingJoined) 0.dp else size.radius
            return RoundedCornerShape(topStart = leading, topEnd = trailing, bottomEnd = trailing, bottomStart = leading)
        }
}

/** The look's fill and, for an outline, its border. */
@Composable
fun Modifier.controlBackground(look: ControlLook): Modifier {
    val palette = LocalPalette.current
    val shape = look.shape
    val filled = background(look.fill(palette, LocalDark.current), shape)
    if (look.variant != ButtonVariant.Outline) return filled
    return filled.border(1.dp, look.surface.border(palette), shape)
}

/**
 * What a button or a menu says: a symbol, words, or both. While `pending` the spinner stands where
 * the symbol is, or over the words when there is none, and the width stays. A symbol that `turns`
 * turns itself instead. With a `pendingTitle` it says that beside the spinner instead.
 */
@Composable
fun ControlLabel(
    title: String?,
    icon: ControlIcon?,
    size: ControlSize,
    modifier: Modifier = Modifier,
    chevron: Boolean = false,
    pending: Boolean = false,
    pendingTitle: String? = null,
    fills: Boolean = false,
    /** Where its words sit when it fills. */
    alignment: Alignment.Horizontal = Alignment.CenterHorizontally,
    /** The symbol's size, as text, where it isn't the one that goes with `size`; a picture's side. */
    symbolSize: Float? = null,
    /** The room between its symbol and its words where it isn't the one that goes with `size`. */
    gap: Dp? = null,
    /** The room between its words and its sides where it isn't the one that goes with `size`. */
    padding: Dp? = null,
    /** A symbol after a dot behind the words, as the reasoning effort after the model. */
    detail: Symbol? = null,
    /** Words after the title in a colour of their own, as "working" after "2 agents", which leave the title in the foreground. */
    tinted: Pair<String, Color>? = null,
) {
    val wordless = title == null && !chevron
    val markSize = symbolSize ?: size.symbol
    val waitingTitle = if (pending) pendingTitle else null
    val sidePadding = padding ?: size.padding
    val pictureInset = if (icon is ControlIcon.Picture && symbolSize != null) (size.symbolSide - symbolSize.dp) / 2 else 0.dp
    val leading = if (wordless) 0.dp else sidePadding - (if (icon == null) 0.dp else size.symbolOutset) + pictureInset
    val trailing = if (wordless) 0.dp else sidePadding - (if (chevron) size.chevronOutset else 0.dp)
    val width = if (fills) Modifier.fillMaxWidth() else Modifier
    val pictureSide = symbolSize?.dp ?: size.symbolSide
    Box(modifier.height(size.height).widthIn(min = size.height).then(width), contentAlignment = Alignment.Center) {
        Row(
            Modifier.then(width).padding(start = leading, end = trailing),
            horizontalArrangement = Arrangement.spacedBy(size.gap, alignment),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (icon != null || title != null) {
                Box(Modifier.weight(1f, fill = false), contentAlignment = Alignment.Center) {
                    ControlWords(title, tinted, icon, size, markSize, pictureSide, gap, detail, pending, spins = pending && icon != null, Modifier.alpha(if (waitingTitle == null) 1f else 0f))
                    if (waitingTitle != null) ControlWords(waitingTitle, null, icon, size, markSize, pictureSide, gap, null, pending, spins = true)
                }
            }
            if (chevron) Icon(Symbol.ChevronDown, size.textSize, color = LocalPalette.current.mutedStrongerForeground, trimmed = true)
        }
        if (pending && icon == null && waitingTitle == null) Spinner(markSize)
    }
}

@Composable
private fun ControlWords(
    title: String?,
    tinted: Pair<String, Color>?,
    icon: ControlIcon?,
    size: ControlSize,
    markSize: Float,
    pictureSide: Dp,
    gap: Dp?,
    detail: Symbol?,
    pending: Boolean,
    spins: Boolean,
    modifier: Modifier = Modifier,
) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(gap ?: size.gap), verticalAlignment = Alignment.CenterVertically) {
        when {
            icon is ControlIcon.Glyph && icon.symbol.turns -> TurningSymbol(icon.symbol, turning = pending, size = markSize)
            spins -> Spinner(markSize)
            icon is ControlIcon.Glyph -> Icon(icon.symbol, markSize)
            icon is ControlIcon.Picture -> Box(Modifier.size(pictureSide), contentAlignment = Alignment.Center) {
                icon.content()
            }
            icon is ControlIcon.Pictures -> icon.content()
        }
        if (title != null) {
            val muted = LocalPalette.current.mutedStrongerForeground
            Row(
                Modifier.weight(1f, fill = false).alpha(if (pending && !spins) 0f else 1f),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(words(title, tinted, muted), style = size.font, maxLines = 1, modifier = Modifier.weight(1f, fill = false))
                if (detail != null) {
                    Text(" · ", style = size.font, color = muted, maxLines = 1)
                    Icon(detail, markSize - 2, color = muted)
                }
            }
        }
    }
}

@Composable
private fun words(title: String, tinted: Pair<String, Color>?, muted: Color): AnnotatedString {
    if (tinted == null) return dotted(title, muted)
    val foreground = LocalPalette.current.foreground
    return buildAnnotatedString {
        withStyle(SpanStyle(color = foreground)) { append(dotted(title, muted)) }
        withStyle(SpanStyle(color = tinted.second)) { append(" ${tinted.first}") }
    }
}

/** The text with the dots between its parts muted, as "Opus 5.5 · Work". */
fun dotted(text: String, muted: Color): AnnotatedString = buildAnnotatedString {
    text.split(" · ").forEachIndexed { index, part ->
        if (index > 0) withStyle(SpanStyle(color = muted)) { append(" · ") }
        append(part)
    }
}

/**
 * A control's press and look around any label: the button's look for what draws its own label, as
 * SwiftUI's `.control()` button style. `margin` is room around it that is its own, so that
 * neighbours leave no gap to miss. A control shorter than a finger needs takes presses as far as
 * `Platform.minimumPress` around it, which Compose's touch targets do by themselves.
 */
@Composable
fun ControlButton(
    look: ControlLook,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    pending: Boolean = false,
    disabled: Boolean = false,
    margin: PaddingValues = PaddingValues(0.dp),
    help: String? = null,
    onLongClick: (() -> Unit)? = null,
    role: Role = Role.Button,
    content: @Composable () -> Unit,
) {
    val enabled = LocalEnabled.current && !disabled
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val hovered by interaction.collectIsHoveredAsState()
    val shown = look.copy(
        surface = LocalSurface.current,
        row = LocalRow.current,
        lit = enabled && !pending && (pressed || hovered),
    )
    val palette = LocalPalette.current
    Box(
        modifier
            .opacity(Opacity.Disabled, on = !enabled && !pending)
            .semantics { if (help != null) contentDescription = help }
            .combinedClickable(
                interactionSource = interaction,
                indication = null,
                enabled = enabled && !pending,
                role = role,
                onLongClick = onLongClick,
                onClick = onClick,
            )
            .padding(margin)
            .controlBackground(shown),
        contentAlignment = Alignment.Center,
    ) {
        CompositionLocalProvider(LocalForeground provides shown.foreground(palette), content = content)
    }
}

/**
 * The button. It says what it does with words, a symbol or both, in one of three sizes. While
 * `pending` it shows the spinner, or turns its symbol if it `turns`, and takes no presses. A button
 * that is only a symbol says what it does in `help`. `opens` shows a chevron, for one that opens
 * something to choose from.
 */
@Composable
fun ActionButton(
    title: String? = null,
    icon: Symbol? = null,
    modifier: Modifier = Modifier,
    picture: (@Composable () -> Unit)? = null,
    pictures: (@Composable () -> Unit)? = null,
    detail: Symbol? = null,
    /** Words after its title in its tint, which leave the title in the foreground. */
    tinted: String? = null,
    help: String? = null,
    variant: ButtonVariant = if (title.isNullOrEmpty()) ButtonVariant.Ghost else ButtonVariant.Secondary,
    size: ControlSize = ControlSize.Regular,
    symbolSize: Float? = null,
    gap: Dp? = null,
    padding: Dp? = null,
    pending: Boolean = false,
    pendingTitle: String? = null,
    selected: Boolean = false,
    round: Boolean = false,
    fills: Boolean = false,
    alignment: Alignment.Horizontal = Alignment.CenterHorizontally,
    opens: Boolean = false,
    joined: Set<HorizontalEdge> = emptySet(),
    tint: Color? = null,
    hover: Color? = null,
    margin: PaddingValues = PaddingValues(0.dp),
    disabled: Boolean = false,
    inline: Boolean = false,
    onLongClick: (() -> Unit)? = null,
    onClick: () -> Unit,
) {
    val words = title?.takeIf { it.isNotEmpty() }
    val look = ControlLook(variant, size, selected = selected, round = round, joined = joined, tint = tint, hover = hover)
    val label = @Composable {
        ControlLabel(
            words, ControlIcon.of(icon, picture, pictures), size, chevron = opens, pending = pending, pendingTitle = pendingTitle, fills = fills,
            alignment = alignment, symbolSize = symbolSize, gap = gap, padding = padding, detail = detail,
            tinted = tinted?.let { tail -> tint?.let { tail to it } },
        )
    }
    if (!inline) return ControlButton(look, onClick, modifier, pending, disabled, margin, words?.let { listOfNotNull(it, tinted).joinToString(" ") } ?: help, onLongClick, content = label)
    val line = with(LocalDensity.current) { rememberTextMeasurer().measure("M", size.font).size.height.toDp() }
    val vertical = ((size.height - line) / 2).coerceAtLeast(0.dp)
    val leading = when {
        words == null && !opens -> (size.height - size.symbolSide) / 2
        else -> (padding ?: size.padding) - if (icon != null || picture != null || pictures != null) size.symbolOutset else 0.dp
    }
    // Its light hugs its words, while its press keeps the control's height.
    val lightInset = (vertical - 3.dp).coerceAtLeast(0.dp)
    val direction = LocalLayoutDirection.current
    val around = PaddingValues(
        start = margin.calculateStartPadding(direction), top = margin.calculateTopPadding() + lightInset,
        end = margin.calculateEndPadding(direction), bottom = margin.calculateBottomPadding() + lightInset,
    )
    ControlButton(look, onClick, modifier.bleeding(leading, vertical), pending, disabled, around, words ?: help, onLongClick) {
        Box(Modifier.bleeding(0.dp, lightInset)) { label() }
    }
}

/**
 * An inline button stands in a line of text: its symbol or words start where the line does, and it is
 * no taller than its words, so that nothing moves for it. Its light and its press reach around.
 */
private fun Modifier.bleeding(leading: Dp, vertical: Dp): Modifier = layout { measurable, constraints ->
    val (side, edge) = leading.roundToPx() to vertical.roundToPx()
    val placeable = measurable.measure(constraints.offset(horizontal = side, vertical = edge * 2))
    layout(placeable.width - side, placeable.height - edge * 2) { placeable.place(-side, -edge) }
}

/** The half of a split button that opens what the other half doesn't do: only a chevron. */
@Composable
fun ChevronButton(
    help: String,
    modifier: Modifier = Modifier,
    variant: ButtonVariant = ButtonVariant.Ghost,
    size: ControlSize = ControlSize.Regular,
    joined: Set<HorizontalEdge> = emptySet(),
    disabled: Boolean = false,
    onClick: () -> Unit,
) {
    ActionButton(
        help = help, modifier = modifier, variant = variant, size = size, opens = true, joined = joined, disabled = disabled, onClick = onClick,
    )
}
