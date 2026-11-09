package app.motile.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.EaseOut
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
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
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.addOutline
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.offset
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.IconSize
import app.motile.theme.LocalDark
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.LocalRow
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.OverPicture
import app.motile.theme.Palette
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.theme.at
import app.motile.theme.symbolSide
import app.motile.theme.value
import kotlin.math.floor
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlin.math.sqrt
import kotlinx.coroutines.delay

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
    /** What waits for something else, as the composer's buttons while their server connects. */
    Waiting,
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
    /** A ghost button that, until it is lit, is as muted as the time beside it. */
    val quiet: Boolean = false,
) {
    fun foreground(palette: Palette): Color {
        if (tint != null && (variant == ButtonVariant.Ghost || variant == ButtonVariant.Secondary || variant == ButtonVariant.Outline)) return tint
        return when (variant) {
            ButtonVariant.Secondary, ButtonVariant.Outline -> palette.foreground
            ButtonVariant.Primary -> palette.primaryForeground
            ButtonVariant.Danger -> palette.destructiveForeground
            ButtonVariant.Success -> palette.successForeground
            ButtonVariant.Warning -> palette.warningForeground
            ButtonVariant.Waiting -> palette.pendingForeground
            ButtonVariant.Ghost -> when {
                selected || lit -> palette.foreground
                quiet -> palette.mutedStrongerForeground
                else -> palette.mutedForeground
            }
            ButtonVariant.Link, ButtonVariant.Accent -> palette.primary
            ButtonVariant.Overlay -> OverPicture.foreground
        }
    }

    /** A button filled with a colour of its own lights by that colour at opacity lit. */
    fun fill(palette: Palette, dark: Boolean): Color {
        fun filled(color: Color) = if (lit) color.at(Opacity.Lit, dark) else color
        return when (variant) {
            ButtonVariant.Primary -> filled(palette.primary)
            ButtonVariant.Secondary -> surface.color(if (selected || lit) Surface.Layer.ControlLit else Surface.Layer.Control, palette)
            ButtonVariant.Outline -> if (selected || lit) surface.color(Surface.Layer.Control, palette) else palette.background
            ButtonVariant.Danger -> filled(palette.destructive)
            ButtonVariant.Success -> filled(palette.success)
            ButtonVariant.Warning -> filled(palette.warning)
            ButtonVariant.Waiting -> filled(palette.pending)
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

/** The rings a button draws: in its waiting look, and once it no longer signals, from when it `stopped`. */
private class Rings(val look: ControlLook, val stopped: Long?)

/**
 * The rings to draw and the clock they move by: while the button signals, and for as long as a ring takes after, so
 * that the rings on their way still go out and no more start.
 */
@Composable
private fun rememberRings(signals: Boolean, look: ControlLook): Pair<Rings, State<Long>>? {
    val clock = remember { mutableLongStateOf(0L) }
    val waiting = remember { arrayOfNulls<ControlLook>(1) }
    var lingering by remember { mutableStateOf<Rings?>(null) }
    SideEffect { if (signals) waiting[0] = look }
    LaunchedEffect(signals) {
        if (signals) {
            lingering = null
            return@LaunchedEffect
        }
        val last = waiting[0] ?: return@LaunchedEffect
        lingering = Rings(last, clock.longValue)
        delay(SIGNAL_TRAVEL_MILLIS.toLong())
        waiting[0] = null
        lingering = null
    }
    if (Platform.reducesMotion) return null
    val rings = when {
        signals -> Rings(look, null)
        else -> lingering ?: waiting[0]?.let { Rings(it, clock.longValue) }
    } ?: return null
    LaunchedEffect(Unit) { while (true) withFrameNanos { clock.longValue = it } }
    return rings to clock
}

/**
 * Rings in the look's fill that move out from behind it, a new one each `SIGNAL_EVERY_MILLIS` while the last is still
 * on its way, seen only outside it: a ring's line is what has come out past the edge, up to its full width.
 */
@Composable
private fun Modifier.signalRings(rings: Rings, clock: State<Long>): Modifier {
    val color = rings.look.fill(LocalPalette.current, LocalDark.current)
    val look = rings.look
    return drawBehind {
        val line = SIGNAL_LINE.toPx()
        val radius = if (look.round) size.height / 2 else look.size.radius.toPx()
        val now = clock.value / 1e6f
        val newest = floor((rings.stopped?.let { it / 1e6f } ?: now) / SIGNAL_EVERY_MILLIS) * SIGNAL_EVERY_MILLIS
        for (older in 0..SIGNAL_TRAVEL_MILLIS / SIGNAL_EVERY_MILLIS) {
            val progress = (now - newest + older * SIGNAL_EVERY_MILLIS) / SIGNAL_TRAVEL_MILLIS
            if (progress >= 1f) continue
            val out = 1f - (1f - progress).pow(3)
            val grown = size.height * SIGNAL_SPREAD * out
            val width = line.coerceAtMost(grown)
            if (width <= 0f) continue
            val middle = grown - width / 2
            drawRoundRect(
                color, topLeft = Offset(-middle, -middle), size = Size(size.width + middle * 2, size.height + middle * 2),
                cornerRadius = CornerRadius(radius + middle), alpha = SIGNAL_OPACITY * (1f - out), style = Stroke(width),
            )
        }
    }
}

private const val SIGNAL_TRAVEL_MILLIS = 1500
private const val SIGNAL_EVERY_MILLIS = 1000
private const val SIGNAL_SPREAD = 0.22f
private const val SIGNAL_OPACITY = 0.6f
private val SIGNAL_LINE = 3.dp

/**
 * The look's fill and, for an outline or over a picture, its border. With an `under` fill, the look's grows from the
 * centre over it, `revealed` of the way.
 */
@Composable
fun Modifier.controlBackground(look: ControlLook, under: Color? = null, revealed: () -> Float = { 1f }): Modifier {
    val palette = LocalPalette.current
    val shape = look.shape
    val fill = look.fill(palette, LocalDark.current)
    val filled = if (under == null) background(fill, shape) else drawBehind {
        clipPath(Path().apply { addOutline(shape.createOutline(size, layoutDirection, this@drawBehind)) }) {
            drawRect(under)
            val reach = sqrt(size.width * size.width + size.height * size.height) / 2
            drawCircle(fill, radius = reach * RevealEasing.transform(revealed()))
        }
    }
    val border = when (look.variant) {
        ButtonVariant.Outline -> look.surface.border(palette)
        ButtonVariant.Overlay -> OverPicture.controlBorder
        else -> return filled
    }
    return filled.border(1.dp, border, shape)
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
    /** The side of the symbol's or the picture's square where it isn't the one that goes with `size`. */
    symbolSize: IconSize? = null,
    /** The room between its symbol and its words where it isn't the one that goes with `size`. */
    gap: Dp? = null,
    /** The room between its words and its sides where it isn't the one that goes with `size`. */
    padding: Dp? = null,
    /** A symbol after a dot behind the words, as the reasoning effort after the model. */
    detail: Symbol? = null,
    /** Words after another dot, as the server after the effort. */
    place: String? = null,
    /** Words after the title in a colour of their own, as "working" after "2 agents", which leave the title in the foreground. */
    tinted: Pair<String, Color>? = null,
    /** What the words name runs through the gateway, which its symbol after them says. */
    proxied: Boolean = false,
    /** What the words name is pinned to one of the gateway's accounts, which a pin after the gateway's symbol says. */
    pinned: Boolean = false,
    /** Its chevron points up, at what it hides again. */
    chevronUp: Boolean = false,
    /** Its words are the prose's, as the text it stands under. */
    prose: Boolean = false,
) {
    val font = if (prose) Theme.proseFont else size.font
    val (wordsShift, chevronShift) = if (prose) proseShifts(size.height) else 0.dp to 0.dp
    val wordless = title == null && !chevron
    val markSize = symbolSize ?: if (wordless && icon is ControlIcon.Glyph) size.wordlessSymbol else size.symbol
    val waitingTitle = if (pending) pendingTitle else null
    val sidePadding = padding ?: size.padding
    val pictureInset = if (icon is ControlIcon.Picture && symbolSize != null) (size.symbolSide - symbolSide(symbolSize)) / 2 else 0.dp
    val leading = if (wordless) 0.dp else sidePadding - (if (icon == null) 0.dp else size.symbolOutset) + pictureInset
    val trailing = if (wordless) 0.dp else sidePadding - (if (chevron) size.chevronOutset else 0.dp)
    val width = if (fills) Modifier.fillMaxWidth() else Modifier
    val pictureSide = symbolSide(markSize)
    Box(modifier.height(size.height).widthIn(min = size.height).then(width), contentAlignment = Alignment.Center) {
        Row(
            Modifier.then(width).padding(start = leading, end = trailing),
            horizontalArrangement = Arrangement.spacedBy(size.gap, alignment),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (icon != null || title != null) {
                Box(Modifier.weight(1f, fill = false).offset(y = wordsShift), contentAlignment = Alignment.Center) {
                    ControlWords(title, tinted, proxied, pinned, icon, size, font, markSize, pictureSide, gap, detail, place, pending, spins = pending && icon != null, Modifier.alpha(if (waitingTitle == null) 1f else 0f))
                    if (waitingTitle != null) ControlWords(waitingTitle, null, false, false, icon, size, font, markSize, pictureSide, gap, null, null, pending, spins = true)
                }
            }
            if (chevron) Icon(
                if (chevronUp) Symbol.ChevronUp else Symbol.ChevronDown, size.chevron, Modifier.offset(y = chevronShift),
                color = LocalPalette.current.mutedStrongerForeground, trimmed = true,
            )
        }
        if (pending && icon == null && waitingTitle == null) Spinner(markSize)
    }
}

/**
 * How far prose words, centred in a control `height` tall, and the chevron after them move. The
 * words are mostly small letters, so the middle of their capitals and of their small letters is the
 * control's, and the chevron is on the small letters' middle.
 */
@Composable
private fun proseShifts(height: Dp): Pair<Dp, Dp> {
    val density = LocalDensity.current
    val measurer = rememberTextMeasurer()
    return remember(height, density) {
        with(density) {
            val paint = android.graphics.Paint().apply { textSize = Theme.proseSize.toPx() }
            val bounds = android.graphics.Rect()
            fun letterHeight(letter: String): Float = bounds.also { paint.getTextBounds(letter, 0, 1, it) }.height().toFloat()
            val (cap, x) = letterHeight("H") to letterHeight("x")
            val line = measurer.measure("x", Theme.proseFont)
            val box = height.toPx()
            val drawn = (box - line.size.height) / 2 + line.firstBaseline
            val baseline = (box / 2 + (cap + x) / 4).roundToInt().toFloat()
            (baseline - drawn).toDp() to (baseline - x / 2 - box / 2).toDp()
        }
    }
}

@Composable
private fun ControlWords(
    title: String?,
    tinted: Pair<String, Color>?,
    proxied: Boolean,
    pinned: Boolean,
    icon: ControlIcon?,
    size: ControlSize,
    font: TextStyle,
    markSize: IconSize,
    pictureSide: Dp,
    gap: Dp?,
    detail: Symbol?,
    place: String?,
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
                val markColor = if (tinted == null) LocalForeground.current else LocalPalette.current.foreground
                Text(
                    words(title, tinted, proxied, pinned, muted), style = font, maxLines = 1, modifier = Modifier.weight(1f, fill = false),
                    inlineContent = if (proxied) GatewayMark.content(markColor, size.textSize, markSize) else emptyMap(),
                )
                if (detail != null) {
                    Text(" · ", style = font, color = muted, maxLines = 1)
                    Icon(detail, size.smallSymbol, color = muted)
                }
                if (place != null) {
                    Text(" · ", style = font, color = muted, maxLines = 1)
                    Text(place, style = font, color = muted, maxLines = 1)
                }
            }
        }
    }
}

@Composable
private fun words(title: String, tinted: Pair<String, Color>?, proxied: Boolean, pinned: Boolean, muted: Color): AnnotatedString {
    val named = dotted(title, muted).proxied(proxied, pinned)
    if (tinted == null) return named
    val foreground = LocalPalette.current.foreground
    return buildAnnotatedString {
        withStyle(SpanStyle(color = foreground)) { append(named) }
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
 * Room beside a control that is its own: the gap to the control after it, so that the gap is only
 * seen and a press in it lands on one of them. The whole gap goes to one, as halves would each be
 * rounded to pixels apart and could draw it a pixel off.
 */
fun beside(start: Dp = 0.dp, end: Dp = 0.dp): PaddingValues = PaddingValues(start = start, end = end)

/** The room of the control at `index` of `count` side by side, whose lights stand `gap` apart. */
fun sharing(gap: Dp, index: Int, count: Int): PaddingValues = beside(end = if (index < count - 1) gap else 0.dp)

/**
 * Lays out what holds controls with margins that far past its sides, as a flow of them that each
 * take the whole gap after them, so that their lights stand where they would without.
 */
fun Modifier.reaching(start: Dp = 0.dp, top: Dp = 0.dp, end: Dp = 0.dp, bottom: Dp = 0.dp): Modifier = layout { measurable, constraints ->
    val (left, up) = start.roundToPx() to top.roundToPx()
    val (width, height) = left + end.roundToPx() to up + bottom.roundToPx()
    val placeable = measurable.measure(constraints.offset(horizontal = width, vertical = height))
    layout((placeable.width - width).coerceAtLeast(0), (placeable.height - height).coerceAtLeast(0)) { placeable.placeRelative(-left, -up) }
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
    /** Its press reaches across the room it stands in, while its light keeps its own width. */
    stretches: Boolean = false,
    /** Rings move out from it, while what it does waits for something else. */
    signals: Boolean = false,
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
    val reveal = rememberReveal(shown.variant, if (!enabled && !pending) Opacity.Disabled.value(LocalDark.current) else 1f)
    val rings = rememberRings(signals, shown)
    Box(
        modifier
            .then(if (stretches) Modifier.fillMaxWidth() else Modifier)
            .then(if (reveal.alpha < 1f) Modifier.alpha(reveal.alpha) else Modifier)
            .semantics { if (help != null) contentDescription = help }
            .arrowPointer()
            .pressesAtOnce(interaction, enabled && !pending)
            .combinedClickable(
                interactionSource = interaction,
                indication = null,
                enabled = enabled && !pending,
                role = role,
                onLongClick = onLongClick,
                onClick = onClick,
            )
            .padding(margin)
            .then(if (stretches) Modifier.wrapContentWidth(Alignment.Start) else Modifier)
            .then(rings?.let { (rings, clock) -> Modifier.signalRings(rings, clock) } ?: Modifier)
            .controlBackground(shown, reveal.under?.let { shown.copy(variant = it, lit = false).fill(palette, LocalDark.current) }, reveal.revealed),
        contentAlignment = Alignment.Center,
    ) {
        CompositionLocalProvider(LocalForeground provides shown.foreground(palette), content = content)
    }
}

/** What a button was for, which its new fill grows over, how far it has grown, and its opacity while it does. */
private class RevealState(variant: ButtonVariant, alpha: Float) {
    var settled by mutableStateOf(variant)
    var from by mutableStateOf<ButtonVariant?>(null)
    var revealedAt = 0L
    val progress = Animatable(1f)
    val alpha = Animatable(alpha)
}

/** The fill a button's new one grows over, how far it has grown, and the button's opacity. */
private class Reveal(val under: ButtonVariant?, val revealed: () -> Float, val alpha: Float)

/**
 * When what a button is for changes, as from waiting to sending, its new fill grows from the centre over the old one
 * and its opacity eases, over `RECOLOR_MILLIS`. Until the change is taken up the old fill stays whole, so that the new
 * one never shows whole first.
 */
@Composable
private fun rememberReveal(variant: ButtonVariant, alpha: Float): Reveal {
    val state = remember { RevealState(variant, alpha) }
    LaunchedEffect(variant) {
        if (state.settled == variant) return@LaunchedEffect
        state.progress.snapTo(0f)
        state.from = state.settled
        state.settled = variant
        state.revealedAt = System.nanoTime()
        state.progress.animateTo(1f, tween(RECOLOR_MILLIS, easing = LinearEasing))
        state.from = null
    }
    LaunchedEffect(variant, alpha) {
        val easing = System.nanoTime() - state.revealedAt < RECOLOR_MILLIS * 1_000_000L
        if (easing) state.alpha.animateTo(alpha, tween(RECOLOR_MILLIS, easing = EaseOut)) else state.alpha.snapTo(alpha)
    }
    val changing = state.settled != variant
    return Reveal(
        under = if (changing) state.settled else state.from,
        revealed = { if (changing) 0f else state.progress.value },
        alpha = if (changing || state.alpha.isRunning) state.alpha.value else alpha,
    )
}

private const val RECOLOR_MILLIS = 200

/** How the new fill's circle grows: fast out of the centre, slowly to the edge. */
private val RevealEasing = CubicBezierEasing(0.25f, 1f, 0.5f, 1f)

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
    place: String? = null,
    /** Words after its title in its tint, which leave the title in the foreground. */
    tinted: String? = null,
    /** What its title names runs through the gateway, which its symbol after the title says. */
    proxied: Boolean = false,
    /** What its title names is pinned to one of the gateway's accounts, which a pin after the gateway's symbol says. */
    pinned: Boolean = false,
    help: String? = null,
    variant: ButtonVariant = if (title.isNullOrEmpty()) ButtonVariant.Ghost else ButtonVariant.Secondary,
    size: ControlSize = ControlSize.Regular,
    symbolSize: IconSize? = null,
    gap: Dp? = null,
    padding: Dp? = null,
    pending: Boolean = false,
    pendingTitle: String? = null,
    selected: Boolean = false,
    round: Boolean = false,
    fills: Boolean = false,
    alignment: Alignment.Horizontal = Alignment.CenterHorizontally,
    opens: Boolean = false,
    /** Shows a chevron that points up while what it shows more of is open. */
    expanded: Boolean? = null,
    joined: Set<HorizontalEdge> = emptySet(),
    tint: Color? = null,
    quiet: Boolean = false,
    margin: PaddingValues = PaddingValues(0.dp),
    disabled: Boolean = false,
    inline: Boolean = false,
    /** Its words are the prose's, as the text it stands under. */
    prose: Boolean = false,
    stretches: Boolean = false,
    /** Rings move out from it, while what it does waits for something else. */
    signals: Boolean = false,
    onLongClick: (() -> Unit)? = null,
    onClick: () -> Unit,
) {
    val words = title?.takeIf { it.isNotEmpty() }
    val look = ControlLook(variant, size, selected = selected, round = round, joined = joined, tint = tint, quiet = quiet)
    val label = @Composable {
        ControlLabel(
            words, ControlIcon.of(icon, picture, pictures), size, chevron = opens || expanded != null, pending = pending, pendingTitle = pendingTitle, fills = fills,
            alignment = alignment, symbolSize = symbolSize, gap = gap, padding = padding, detail = detail, place = place,
            tinted = tinted?.let { tail -> tint?.let { tail to it } }, proxied = proxied, pinned = pinned, chevronUp = expanded == true, prose = prose,
        )
    }
    if (!inline) {
        return ControlButton(
            look, onClick, modifier, pending, disabled, margin, words?.let { listOfNotNull(it, tinted).joinToString(" ") } ?: help, onLongClick,
            stretches = stretches, signals = signals, content = label,
        )
    }
    val line = with(LocalDensity.current) { rememberTextMeasurer().measure("M", size.font).size.height.toDp() }
    val vertical = ((size.height - line) / 2).coerceAtLeast(0.dp)
    val leading = when {
        words == null && !opens -> (size.height - symbolSide(symbolSize ?: size.wordlessSymbol)) / 2
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
