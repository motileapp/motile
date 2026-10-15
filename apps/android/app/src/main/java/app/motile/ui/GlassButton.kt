package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.size
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
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.platform.Symbol
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.LocalRow
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.ShadowSize
import app.motile.theme.Surface
import app.motile.theme.at
import app.motile.theme.shadow

/**
 * The surface of a button that floats over the client, where iOS has Liquid Glass: the popover's
 * colour a little see-through, with its border around it and a shadow under it.
 */
@Composable
fun Modifier.glassButton(shape: Shape): Modifier {
    val palette = LocalPalette.current
    return shadow(ShadowSize.Md, shape)
        .clip(shape)
        .background(palette.popover.at(Opacity.InputGlassTint), shape)
        .border(1.dp, palette.borderPopover, shape)
}

/** Glass shapes side by side. */
@Composable
fun GlassGroup(spacing: Dp, modifier: Modifier = Modifier, content: @Composable RowScope.() -> Unit) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(spacing), verticalAlignment = Alignment.CenterVertically, content = content)
}

/** A symbol's button that floats over the client: on a glass circle of its own, or, `bare`, beside others on one glass capsule that `glassButton` draws around them. */
object GlassButton {
    val side = 46.dp
}

@Composable
fun GlassButton(
    help: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    bare: Boolean = false,
    enabled: Boolean = true,
    content: @Composable () -> Unit,
) {
    val glass = if (bare) Modifier else Modifier.glassButton(CircleShape)
    // One node for accessibility: the label and the press together.
    Box(
        modifier.size(GlassButton.side).then(glass).semantics(mergeDescendants = true) {
            contentDescription = help
            role = Role.Button
        },
        contentAlignment = Alignment.Center,
    ) {
        DimButton(onClick, Modifier.size(GlassButton.side), enabled = enabled, contentAlignment = Alignment.Center) {
            CompositionLocalProvider(
                LocalSurface provides Surface.Popover,
                LocalRow provides null,
                LocalForeground provides LocalPalette.current.foreground,
                content = content,
            )
        }
    }
}

@Composable
fun GlassButton(symbol: Symbol, help: String, onClick: () -> Unit, modifier: Modifier = Modifier, symbolSize: Float = 15f, bare: Boolean = false, pending: Boolean = false) {
    GlassButton(help, onClick, modifier, bare, enabled = !pending) {
        when {
            symbol.turns -> TurningSymbol(symbol, turning = pending, size = symbolSize)
            pending -> Spinner(symbolSize)
            else -> Icon(symbol, symbolSize)
        }
    }
}

/** What closes a sheet: a round glass button with an x. It closes the modal it lies in unless told otherwise. */
@Composable
fun SheetCloseButton(modifier: Modifier = Modifier, onClose: (() -> Unit)? = null) {
    val close = LocalCloseModal.current
    GlassButton(Symbol.X, "Close", onClick = { onClose?.invoke() ?: close() }, modifier = modifier, symbolSize = 16f)
}
