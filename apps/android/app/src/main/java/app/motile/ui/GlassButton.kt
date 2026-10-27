package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.IconSize
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.LocalRow
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.ShadowSize
import app.motile.theme.Surface
import app.motile.theme.shadow

/** The surface of a button that floats over the client, with a shadow under it. */
@Composable
fun Modifier.glassButton(shape: Shape, size: GlassSize = GlassSize.Regular): Modifier =
    shadow(size.shadow, shape, size.shadowStrength).floatingSurface(shape)

/**
 * The surface of what floats over the client, the composer and the buttons alike, where iOS has
 * Liquid Glass: the composer's colour with its hairline around it.
 */
@Composable
fun Modifier.floatingSurface(shape: Shape): Modifier {
    val palette = LocalPalette.current
    return clip(shape)
        .background(palette.composer, shape)
        .border(1.dp, palette.borderComposer, shape)
}

/** Glass shapes side by side. */
@Composable
fun GlassGroup(spacing: Dp, modifier: Modifier = Modifier, content: @Composable RowScope.() -> Unit) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(spacing), verticalAlignment = Alignment.CenterVertically, content = content)
}

/** A symbol's button that floats over the client: on a glass circle of its own, or, `bare`, beside others on one glass capsule that `glassButton` draws around them. */
object GlassButton {
    val side = GlassSize.Regular.side
}

/**
 * How large a glass button is: `Regular` over the client, `Inline` and `Compact` over what the
 * transcript shows, like a map, where they cast a smaller, stronger shadow.
 */
enum class GlassSize(val side: Dp, val shadow: ShadowSize, val shadowStrength: Opacity) {
    Regular(46.dp, ShadowSize.Md, Opacity.Shadow),
    Inline(34.dp, ShadowSize.Sm, Opacity.ShadowStronger),
    Compact(28.dp, ShadowSize.Sm, Opacity.ShadowStronger),
}

@Composable
fun GlassButton(
    help: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    bare: Boolean = false,
    enabled: Boolean = true,
    size: GlassSize = GlassSize.Regular,
    state: String? = null,
    content: @Composable () -> Unit,
) {
    val glass = if (bare) Modifier else Modifier.glassButton(CircleShape, size)
    // One node for accessibility: the label and the press together.
    Box(
        modifier.size(size.side).then(glass).semantics(mergeDescendants = true) {
            contentDescription = help
            role = Role.Button
            state?.let { stateDescription = it }
        },
        contentAlignment = Alignment.Center,
    ) {
        DimButton(onClick, Modifier.size(size.side), enabled = enabled, contentAlignment = Alignment.Center) {
            CompositionLocalProvider(
                LocalSurface provides Surface.Background,
                LocalRow provides null,
                LocalForeground provides LocalPalette.current.foreground,
                content = content,
            )
        }
    }
}

@Composable
fun GlassButton(
    symbol: Symbol,
    help: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    symbolSize: IconSize = IconSize.Lg,
    bare: Boolean = false,
    pending: Boolean = false,
    size: GlassSize = GlassSize.Regular,
    state: String? = null,
) {
    GlassButton(help, onClick, modifier, bare, enabled = !pending, size = size, state = state) {
        when {
            symbol.turns -> TurningSymbol(symbol, turning = pending, size = symbolSize)
            pending -> Spinner(symbolSize)
            else -> Icon(symbol, symbolSize)
        }
    }
}

/**
 * Words on a glass capsule, with a symbol after them: a chevron for a choice it opens, or what it
 * does. `Inline` is 34 tall with room for the chevron at its end, `Compact` 28 and even.
 */
@Composable
fun GlassCapsule(title: String, symbol: Symbol, onClick: () -> Unit, modifier: Modifier = Modifier, size: GlassSize = GlassSize.Inline, help: String = title) {
    val (start, end) = if (size == GlassSize.Compact) 10.dp to 10.dp else 12.dp to 10.dp
    Box(
        modifier.height(size.side).glassButton(CircleShape, size).semantics(mergeDescendants = true) {
            contentDescription = help
            role = Role.Button
        },
    ) {
        DimButton(onClick, Modifier.height(size.side), contentAlignment = Alignment.Center) {
            CompositionLocalProvider(
                LocalSurface provides Surface.Background,
                LocalRow provides null,
                LocalForeground provides LocalPalette.current.foreground,
            ) {
                Row(
                    Modifier.height(size.side).padding(start = start, end = end),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(title, Modifier.widthIn(max = 180.dp), style = ControlSize.Regular.font, maxLines = 1)
                    if (symbol == Symbol.ChevronDown) Icon(symbol, ControlSize.Regular.chevron, trimmed = true) else Icon(symbol, ControlSize.Regular.symbol)
                }
            }
        }
    }
}

/** What closes a sheet: a round glass button with an x. It closes the modal it lies in unless told otherwise. */
@Composable
fun SheetCloseButton(modifier: Modifier = Modifier, onClose: (() -> Unit)? = null) {
    val close = LocalCloseModal.current
    GlassButton(Symbol.X, "Close", onClick = { onClose?.invoke() ?: close() }, modifier = modifier, symbolSize = IconSize.Lg)
}
