package app.motile.transcript.openui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.core.AppStore
import app.motile.core.UiColor
import app.motile.core.UiRadius
import app.motile.core.UiRadiusKind
import app.motile.core.UiTextStyle
import app.motile.core.UiWeight
import app.motile.theme.LocalDark
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Palette
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.theme.at
import app.motile.ui.lined

/** What a program's views send their events through: core's host of the program in the row. */
class UiHost(private val store: AppStore, private val threadID: String, val rowID: String, val limited: Boolean) {
    fun send(node: String, type: String, vararg fields: Pair<String, Any?>) {
        store.sendUi(threadID, rowID, node, mapOf("type" to type) + fields)
    }
}

/** The host of the program drawn, none where it is only laid out. */
val LocalUiHost = staticCompositionLocalOf<UiHost?> { null }

/** The row's program still streams in. */
val LocalUiStreaming = staticCompositionLocalOf { false }

/** The height of the Box around a node, when it has one, in points: what a share of its height is of. */
val LocalUiParentHeight = staticCompositionLocalOf<Float?> { null }

/** The transcript column is under 640 wide: LAYOUT.md's narrow variants apply. */
val LocalUiNarrow = staticCompositionLocalOf { false }

/** The transcript column's width, which `vw` lengths and carousel items count in. */
val LocalUiColumn = staticCompositionLocalOf { 0.dp }

/** The size of the text around an inline formula, in points, which it is set 1.12 times as large as. */
val LocalUiTextSize = staticCompositionLocalOf { Theme.proseSize.value }

/** How a column that centres or ends its children aligns the lines of their text. */
val LocalUiTextAlign = compositionLocalOf<TextAlign?> { null }

object UiStyle {
    const val NARROW_BELOW = 640f

    fun cardPadding(narrow: Boolean): Float = if (narrow) 16f else 20f

    /** A text style of LAYOUT.md's type table, its line height fixed. */
    fun text(style: UiTextStyle, narrow: Boolean): TextStyle = when (style) {
        UiTextStyle.Body -> TextStyle(fontSize = Theme.proseSize).lined(24f)
        UiTextStyle.BodyCard -> TextStyle(fontSize = Theme.proseSize).lined(22f)
        UiTextStyle.Large -> Theme.ui(16f).lined(26f)
        UiTextStyle.Small -> Theme.ui(13f).lined(20f)
        UiTextStyle.Caption -> Theme.smallFont.lined(18f)
        UiTextStyle.Label -> Theme.ui(13f, FontWeight.Medium).lined(18f)
        UiTextStyle.TitleSm -> Theme.heading(4).digits().lined(20f)
        UiTextStyle.TitleMd -> Theme.heading(3).digits().lined(22f)
        UiTextStyle.TitleLg, UiTextStyle.Word -> Theme.heading(2).digits().lined(24f)
        UiTextStyle.TitleXl -> Theme.heading(1).digits().lined(26f)
        UiTextStyle.Title2xl -> if (narrow) Theme.uiDigits(24f, FontWeight.SemiBold).lined(28f) else Theme.uiDigits(26f, FontWeight.SemiBold).lined(30f)
        UiTextStyle.Title3xl -> if (narrow) Theme.uiDigits(28f, FontWeight.SemiBold).lined(32f) else Theme.uiDigits(32f, FontWeight.SemiBold).lined(36f)
        UiTextStyle.CardTitle -> Theme.uiDigits(14f, FontWeight.Medium).lined(20f)
        UiTextStyle.CardSubtitle -> Theme.ui(13f).lined(18f)
        UiTextStyle.CardHeading -> Theme.uiDigits(16f, FontWeight.SemiBold).lined(22f)
        UiTextStyle.Readout -> Theme.uiDigits(12f, FontWeight.Medium).lined(18f)
        UiTextStyle.Tick -> Theme.uiDigits(11f).lined(14f)
        UiTextStyle.Timer -> Theme.uiDigits(28f, FontWeight.SemiBold).lined(32f)
        UiTextStyle.Code -> Theme.codeFont.copy(lineHeight = Theme.codeLineHeight)
        UiTextStyle.WordBack -> TextStyle(fontSize = Theme.proseSize, fontWeight = FontWeight.Medium).lined(24f)
    }

    /** The size the style's text is, as on the Mac, for what is measured in it. */
    fun size(style: UiTextStyle, narrow: Boolean): Float = text(style, narrow).fontSize.value / app.motile.platform.Platform.scale

    fun weight(weight: UiWeight): FontWeight = when (weight) {
        UiWeight.Regular -> FontWeight.Normal
        UiWeight.Medium -> FontWeight.Medium
        UiWeight.Semibold -> FontWeight.SemiBold
    }

    fun radius(radius: UiRadius?, side: Dp = 0.dp): Dp = when (radius?.kind) {
        null -> 0.dp
        UiRadiusKind.Xs -> Radius.xs
        UiRadiusKind.Sm -> Radius.sm
        UiRadiusKind.Md -> Radius.md
        UiRadiusKind.Lg -> Radius.lg
        UiRadiusKind.Xl -> Radius.xl
        UiRadiusKind.Full -> side / 2
        UiRadiusKind.Points -> radius.points.dp
    }

    private fun TextStyle.digits(): TextStyle = copy(fontFeatureSettings = "tnum")
}

/** A colour core named, on the surface the view lies on. */
@Composable
@ReadOnlyComposable
fun UiColor.color(): Color {
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    return resolve(palette, surface, LocalDark.current, LocalForeground.current)
}

fun UiColor.resolve(palette: Palette, surface: Surface, dark: Boolean, current: Color): Color = when (this) {
    UiColor.Current -> if (current == Color.Unspecified) palette.foreground else current
    is UiColor.Token -> palette.token(name)
    is UiColor.Wash -> palette.token(name).at(Opacity.ColorTint, dark)
    is UiColor.Surface -> when (layer) {
        "base" -> surface.color(palette)
        "border" -> surface.border(palette)
        "row" -> surface.color(Surface.Layer.Row, palette)
        "control" -> surface.color(Surface.Layer.Control, palette)
        else -> surface.color(Surface.Layer.Box, palette)
    }
    is UiColor.Rgba -> Color(
        channels.getOrElse(0) { 0 }, channels.getOrElse(1) { 0 }, channels.getOrElse(2) { 0 }, channels.getOrElse(3) { 255 },
    )
}

/** A token of `tokens.json` by its name. */
fun Palette.token(name: String): Color = when (name) {
    "background" -> background
    "background-secondary" -> backgroundSecondary
    "card" -> card
    "card-accent" -> cardAccent
    "popover" -> popover
    "foreground" -> foreground
    "emphasized-foreground" -> emphasizedForeground
    "muted-foreground" -> mutedForeground
    "muted-stronger-foreground" -> mutedStrongerForeground
    "muted-strongest-foreground" -> mutedStrongestForeground
    "primary" -> primary
    "primary-foreground" -> primaryForeground
    "destructive" -> destructive
    "destructive-foreground" -> destructiveForeground
    "success" -> success
    "success-foreground" -> successForeground
    "warning" -> warning
    "warning-foreground" -> warningForeground
    "merged" -> merged
    "merged-foreground" -> mergedForeground
    "pink" -> pink
    "border" -> border
    "border-card" -> borderCard
    "border-popover" -> borderPopover
    "border-accent" -> borderAccent
    "chart-1" -> chart1
    "chart-2" -> chart2
    "chart-3" -> chart3
    "chart-4" -> chart4
    "chart-5" -> chart5
    "chart-6" -> chart6
    else -> foreground
}
