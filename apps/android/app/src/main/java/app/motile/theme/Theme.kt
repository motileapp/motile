package app.motile.theme

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.draw.dropShadow
import app.motile.platform.Platform
import app.motile.platform.scaled
import kotlin.math.roundToInt

/** The palette in effect, and whether it is the dark one. */
val LocalPalette = staticCompositionLocalOf { Palette.dark }
val LocalDark = staticCompositionLocalOf { true }

/** The colour text and symbols are drawn in where nothing else is said, as SwiftUI's foreground style. */
val LocalForeground = compositionLocalOf { Color.Unspecified }

/** The layer the view lies on. */
val LocalSurface = compositionLocalOf { Surface.Background }

/** The row the view lies on, as it is lit, so that a control on it lights a step above it. */
val LocalRow = compositionLocalOf<Surface.Layer?> { null }

/**
 * The design tokens of `Tokens.kt`, generated from packages/theme/tokens.json, with what is made of
 * them: every colour the app shows, `Opacity`, `ShadowSize` and `Radius`. A view never makes a
 * colour of its own.
 */
object Theme {
    /** The palette of the appearance in effect, for text built off the main thread. Set by the root view. */
    @Volatile
    var palette: Palette = Palette.dark

    @Volatile
    var dark: Boolean = true

    val colors: Palette
        @Composable @ReadOnlyComposable get() = LocalPalette.current

    /** The colours code is highlighted with, by the core's palette index. */
    fun syntax(palette: Palette = this.palette): List<Color> = with(palette) {
        listOf(
            foreground,
            syntaxComment,
            syntaxKeyword,
            syntaxString,
            syntaxConstant,
            syntaxFunction,
            syntaxType,
            syntaxType, // variable
            syntaxTag,
            syntaxConstant, // attribute
            success, // inserted
            destructive, // deleted
            syntaxConstant, // heading
            syntaxConstant, // escape
            syntaxConstant, // property
            syntaxComment, // meta
        )
    }

    // Type, in sizes as on the Mac, enlarged.
    val proseSize: TextUnit = (14 * Platform.scale).sp
    val codeSize: TextUnit = (12.5f * Platform.scale).sp
    val proseLineHeight: TextUnit = scaled(24f).sp
    val codeLineHeight: TextUnit = scaled(18f).sp
    val mono: FontFamily = FontFamily.Monospace

    /** The system's font at a size given as it is on the Mac. */
    fun ui(size: Float, weight: FontWeight = FontWeight.Normal, italic: Boolean = false): TextStyle = TextStyle(
        fontSize = (size * Platform.scale).sp,
        fontWeight = weight,
        fontStyle = if (italic) FontStyle.Italic else FontStyle.Normal,
    )

    fun uiMono(size: Float, weight: FontWeight = FontWeight.Normal): TextStyle =
        ui(size, weight).copy(fontFamily = mono)

    /** Digits that keep their width, for numbers that count. */
    fun uiDigits(size: Float, weight: FontWeight = FontWeight.Normal): TextStyle =
        ui(size, weight).copy(fontFeatureSettings = "tnum")

    val proseFont: TextStyle get() = TextStyle(fontSize = proseSize)
    val proseBold: TextStyle get() = TextStyle(fontSize = proseSize, fontWeight = FontWeight.SemiBold)
    val codeFont: TextStyle get() = TextStyle(fontSize = codeSize, fontFamily = mono)
    val inlineCodeFont: TextStyle get() = uiMono(12.5f)
    val smallFont: TextStyle get() = ui(12f)
    val smallMono: TextStyle get() = uiMono(11.5f)

    fun heading(level: Int): TextStyle = when (level) {
        1 -> ui(20f, FontWeight.SemiBold)
        2 -> ui(17f, FontWeight.SemiBold)
        3 -> ui(15f, FontWeight.SemiBold)
        else -> ui(14f, FontWeight.SemiBold)
    }

    // Layout
    /** The widest the transcript gets. */
    val contentWidth: Dp = 768.dp
    val contentPadding: Dp = 16.dp
    /** How far the composer reaches past the transcript on each side. */
    val composerReach: Dp = 6.dp
    val composerWidth: Dp = contentWidth + composerReach * 2
    val composerPadding: Dp = contentPadding - composerReach
}

fun Opacity.value(dark: Boolean): Float = if (dark) this.dark else light

/** The colour at one of the opacities, in the appearance in effect. */
@Composable
@ReadOnlyComposable
fun Color.at(opacity: Opacity): Color = copy(alpha = alpha * opacity.value(LocalDark.current))

fun Color.at(opacity: Opacity, dark: Boolean): Color = copy(alpha = alpha * opacity.value(dark))

/** The colour's wash: at color-tint, or color-tint-stronger when lit. */
@Composable
@ReadOnlyComposable
fun Color.wash(lit: Boolean = false): Color = at(if (lit) Opacity.ColorTintStronger else Opacity.ColorTint)

/** The scrim at its opacity. */
val Palette.scrim: Color @Composable @ReadOnlyComposable get() = overlay.at(Opacity.Overlay)

/** Thins the whole view to one of the opacities, while `on`. */
@Composable
fun Modifier.opacity(opacity: Opacity, on: Boolean = true): Modifier {
    val value = opacity.value(LocalDark.current)
    return if (on) alpha(value) else this
}

/** A shadow of the size, in the colour shadow at one of its three opacities. */
@Composable
fun Modifier.shadow(size: ShadowSize, shape: Shape, strength: Opacity = Opacity.Shadow): Modifier {
    val color = Theme.colors.shadow.at(strength)
    return dropShadow(shape, Shadow(radius = (size.blur / 2).dp, color = color, offset = DpOffset(0.dp, size.down.dp)))
}

/** The text colours as they are in the dark, for what lies over a picture. */
object OverPicture {
    val foreground get() = Palette.dark.foreground
    val mutedForeground get() = Palette.dark.mutedForeground
    /** A control as it is in the dark, over a picture: the background's accent, stronger when lit. */
    val control get() = Palette.dark.backgroundAccent
    val controlLit get() = Palette.dark.backgroundAccentStronger
}

/**
 * How large a control is. Its height, symbol, text, padding and corners go together, so that no
 * view picks them one by one. Sizes are as on the Mac, enlarged.
 */
enum class ControlSize {
    /** Inside a row, a tab, a chip or a card. */
    Small,
    /** Everywhere else. */
    Regular,
    /** What a screen is about, and the bars fingers press. */
    Large;

    val height: Dp
        get() = when (this) {
            Small -> scaled(24f).dp
            Regular -> scaled(28f).dp
            Large -> scaled(36f).dp
        }

    /** The size of text its symbol is drawn for. */
    val symbol: Float
        get() = when (this) {
            Small -> 13f
            Regular -> 14f
            Large -> 16f
        }

    /** A symbol that should weigh less than the others, like the x that closes a tab. */
    val smallSymbol: Float get() = symbol - 2

    val textSize: Float
        get() = when (this) {
            Small -> 11.5f
            Regular -> 12f
            Large -> 13f
        }

    val font: TextStyle get() = Theme.ui(textSize, FontWeight.Medium)

    /** The room between what it says and its sides. */
    val padding: Dp
        get() = when (this) {
            Small -> 8.dp
            Regular -> 11.dp
            Large -> 14.dp
        }

    /** How much nearer its side a chevron stands than words do, so that the control's sides look alike. */
    val chevronOutset: Dp get() = if (this == Regular) 2.dp else 0.dp

    /** The room between its symbol and its words. */
    val gap: Dp
        get() = when (this) {
            Small -> 5.dp
            Regular -> 6.dp
            Large -> 8.dp
        }

    val radius: Dp
        get() = when (this) {
            Small -> Radius.sm
            Regular, Large -> Radius.md
        }

    /** The side of the square its symbol is drawn in. */
    val symbolSide: Dp get() = symbolSide(symbol)

    /** How much nearer its side a symbol stands than words do, so that both look as far from it. */
    val symbolOutset: Dp get() = ((symbolSide.value / 5 * 2).roundToInt() / 2f).dp
}

/** The side of the square a symbol for text of `size` is drawn in. */
fun symbolSide(size: Float): Dp = (size * Platform.scale * 1.2f).roundToInt().dp

/** What a view lies on: the page, a box on it, a card or a popover. What is drawn on a surface takes the surface's accent. */
enum class Surface {
    Background, Secondary, Card, Popover;

    /** What is drawn on a surface, each in the surface's own accent. */
    enum class Layer {
        /** A filled control at rest: a secondary button, a chip's fill, a track. */
        Control,
        /** The same lit, or selected. */
        ControlLit,
        /** A control lit on a selected row, which is already as light as a lit control. */
        ControlLitOnSelected,
        /** A row or a tab lit. */
        Row,
        /** A row or a tab selected. */
        RowSelected,
        /** A box on it: code, a message, a notice, a quoted line. */
        Box,
    }

    fun color(palette: Palette): Color = when (this) {
        Background -> palette.background
        Secondary -> palette.backgroundSecondary
        Card -> palette.card
        Popover -> palette.popover
    }

    /** The surface's edge, which the lines across it share. */
    fun border(palette: Palette): Color = when (this) {
        Background, Secondary -> palette.border
        Card -> palette.borderCard
        Popover -> palette.borderPopover
    }

    fun color(layer: Layer, palette: Palette): Color = when (this) {
        Background -> when (layer) {
            Layer.Control -> palette.backgroundAccent
            Layer.ControlLit -> palette.backgroundAccentStronger
            Layer.ControlLitOnSelected -> palette.backgroundAccentStrongest
            Layer.Row -> palette.backgroundAccentLarger
            Layer.RowSelected -> palette.backgroundAccentLargerStronger
            Layer.Box -> palette.backgroundSecondary
        }
        Secondary -> when (layer) {
            Layer.Control, Layer.Row, Layer.Box -> palette.backgroundSecondaryAccent
            Layer.ControlLit, Layer.ControlLitOnSelected, Layer.RowSelected -> palette.backgroundSecondaryAccentStronger
        }
        Card -> palette.cardAccent
        Popover -> palette.popoverAccent
    }

    /** What lies in a box on it. */
    val inBox: Surface get() = if (this == Background) Secondary else this

    val color: Color @Composable @ReadOnlyComposable get() = color(LocalPalette.current)

    val border: Color @Composable @ReadOnlyComposable get() = border(LocalPalette.current)

    @Composable
    @ReadOnlyComposable
    fun color(layer: Layer): Color = color(layer, LocalPalette.current)
}

/** Makes the content a box on the surface it is on, `bordered` in that surface's border; what lies in it takes the box's surface. */
@Composable
fun SurfaceBox(shape: Shape, modifier: Modifier = Modifier, bordered: Boolean = false, content: @Composable BoxScope.() -> Unit) {
    val surface = LocalSurface.current
    var box = modifier.clip(shape).background(surface.color(Surface.Layer.Box), shape)
    if (bordered) box = box.border(1.dp, surface.border, shape)
    Box(box) {
        CompositionLocalProvider(LocalSurface provides surface.inBox) { content() }
    }
}

/** Lays content on a surface of its own: a card, a popover, a sheet. */
@Composable
fun OnSurface(surface: Surface, content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalSurface provides surface, LocalRow provides null, content = content)
}
