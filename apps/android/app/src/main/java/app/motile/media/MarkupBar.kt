package app.motile.media

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import app.motile.composer.ComposerView
import app.motile.core.MarkupSession
import app.motile.core.MarkupTool
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.LocalRow
import app.motile.theme.LocalSurface
import app.motile.theme.floatingHeight
import app.motile.ui.glassButton
import app.motile.theme.Palette
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.theme.TextSize
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.InputField
import app.motile.ui.InputVariant
import app.motile.ui.Swatch
import app.motile.ui.Text

/** The composer's buttons, in a bar as tall as the composer folded. */
private object MarkupBar {
    val size = ControlSize.Regular
}

/**
 * The bar under an image being marked up: the tools, the colour, undo and redo. The colours open
 * from the one picked, to leave room for the tools. While a pin is selected it is the pin's note
 * instead, which the keyboard's action keeps.
 */
@Composable
fun MarkupBar(session: MarkupSession, modifier: Modifier = Modifier) {
    val palette = LocalPalette.current
    CompositionLocalProvider(LocalSurface provides Surface.Background, LocalRow provides null, LocalForeground provides palette.foreground) {
        AnimatedContent(
            session.selectedPin,
            modifier,
            transitionSpec = { fadeIn(tween(200)) togetherWith fadeOut(tween(200)) },
            contentKey = { it?.mark },
            contentAlignment = Alignment.Center,
            label = "markup bar",
        ) { pin ->
            val bar = Modifier
                .glassButton(CircleShape)
                .height(floatingHeight)
                .padding(horizontal = ComposerView.edge)
            if (pin == null) ToolBar(session, bar) else NoteBar(session, pin, bar.fillMaxWidth())
        }
    }
}

@Composable
private fun ToolBar(session: MarkupSession, modifier: Modifier) {
    var inksOpen by remember { mutableStateOf(false) }
    val size = MarkupBar.size
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(2.dp), verticalAlignment = Alignment.CenterVertically) {
        if (inksOpen) {
            for (ink in session.inks) {
                Swatch(markupColor(ink), ink.replaceFirstChar { it.uppercase() }, selected = session.ink == ink, size = size) {
                    session.pick(ink)
                    inksOpen = false
                }
            }
            return@Row
        }
        for (tool in MarkupTool.entries) {
            val variant = if (session.tool == tool) ButtonVariant.Accent else ButtonVariant.Ghost
            ActionButton(icon = tool.symbol, help = tool.title, variant = variant, size = size, round = true) { session.pick(tool) }
        }
        Divider()
        Swatch(markupColor(session.ink), "Colour", selected = true, size = size) { inksOpen = true }
        Divider()
        ActionButton(icon = Symbol.Undo2, help = "Undo", size = size, round = true, disabled = !session.canUndo) { session.undo() }
        ActionButton(icon = Symbol.Redo2, help = "Redo", size = size, round = true, disabled = !session.canRedo) { session.redo() }
    }
}

@Composable
private fun NoteBar(session: MarkupSession, pin: MarkupSession.Pin, modifier: Modifier) {
    val palette = LocalPalette.current
    val size = MarkupBar.size
    val focus = remember { FocusRequester() }
    LaunchedEffect(pin.mark) {
        withFrameNanos {}
        runCatching { focus.requestFocus() }
    }
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size(size.height).background(markupColor(pin.ink), CircleShape), contentAlignment = Alignment.Center) {
            val contrast = if (pin.ink == "yellow" || pin.ink == "white") palette.markupInk else palette.markupWhite
            Text("${pin.number}", style = Theme.uiDigits(TextSize.Sm, FontWeight.SemiBold), color = contrast, maxLines = 1)
        }
        Box(Modifier.weight(1f).height(size.height)) {
            InputField(
                "Note for the agent", session.note, { session.note = it },
                variant = InputVariant.Bare,
                size = size,
                focusRequester = focus,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                onSubmit = { session.deselect() },
            )
        }
        ActionButton(icon = Symbol.Trash2, help = "Remove Pin", size = size, round = true) { session.removeSelected() }
        ActionButton(icon = Symbol.Check, help = "Keep Note", variant = ButtonVariant.Primary, size = size, round = true) { session.deselect() }
    }
}

@Composable
private fun Divider() {
    Box(
        Modifier
            .padding(horizontal = 4.dp)
            .width(1.dp)
            .height(MarkupBar.size.height * 0.6f)
            .background(LocalPalette.current.borderComposer),
    )
}

/** The colour of an ink the core names, which is the same in light and dark. */
@Composable
fun markupColor(ink: String): Color {
    val palette: Palette = LocalPalette.current
    return palette.named("markup-$ink") ?: palette.markupRed
}
