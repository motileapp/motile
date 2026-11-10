package app.motile.ui

import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.Palette
import app.motile.theme.Radius
import app.motile.theme.SurfaceBox
import app.motile.theme.TextSize
import app.motile.theme.Theme
import kotlin.math.roundToInt

/** The room between the buttons' lights, which their presses share. */
private val buttonGap = 4.dp

/**
 * A command to run in a terminal, in the theme's code colours, with the buttons that share and copy
 * it. Without a command yet it says what it waits for. A link is shown on one line, cut short, with
 * the buttons that open and copy it.
 */
@Composable
fun CommandBox(
    command: String?,
    modifier: Modifier = Modifier,
    /** What `Theme.syntax` colours each stretch: its start, its length and the colour, in threes. */
    spans: List<Int> = emptyList(),
    placeholder: String = "Preparing the command…",
    link: String? = null,
    /** Opens the link, in the browser unless the caller has a better place for it. */
    open: (String) -> Unit = Platform::open,
) {
    val noun = if (link == null) "command" else "link"
    val palette = LocalPalette.current
    val density = LocalDensity.current
    val style = Theme.uiMono(TextSize.Xs).copy(lineHeight = Theme.codeLineHeight)
    // Puts the first line of the command level with the middle of the buttons.
    val textInset = with(density) { ((ControlSize.Regular.height.toPx() - Theme.codeLineHeight.toPx()) / 2).roundToInt().toDp() }
    val shown = remember(command, spans, palette, link) {
        when {
            command == null -> AnnotatedString(placeholder)
            link != null -> AnnotatedString(command)
            else -> highlighted(command, spans, palette)
        }
    }
    SurfaceBox(RoundedCornerShape(Radius.lg), modifier, bordered = true) {
        Row(Modifier.padding(scaled(4f).dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.Top) {
            Text(
                shown,
                Modifier.weight(1f).padding(start = textInset, top = textInset, bottom = textInset),
                style = style,
                color = if (command == null) palette.mutedStrongerForeground else palette.foreground,
                maxLines = if (link == null) Int.MAX_VALUE else 1,
            )
            Disabled(command == null) {
                Row {
                    link?.let { url ->
                        ActionButton("Open", icon = Symbol.SquareArrowOutUpRight, variant = ButtonVariant.Ghost, margin = PaddingValues(end = buttonGap)) {
                            open(url)
                        }
                    }
                    if (link == null) {
                        ActionButton(icon = Symbol.Share, help = "Share the command", margin = PaddingValues(end = buttonGap)) { command?.let(::shareText) }
                    }
                    CopyButton(help = "Copy the $noun") { command?.let(Platform::copy) }
                }
            }
        }
    }
}

/** The command in its colours. It wraps between any two characters, as CSS's `break-all` does, so it is not selectable: a copy would carry the zero-width spaces into the shell. */
private fun highlighted(command: String, spans: List<Int>, palette: Palette): AnnotatedString {
    val colours = IntArray(command.length)
    var index = 0
    while (index + 2 < spans.size) {
        val start = spans[index]
        val end = start + spans[index + 1]
        if (start >= 0 && end <= colours.size) colours.fill(spans[index + 2], start, end)
        index += 3
    }
    val syntax = Theme.syntax(palette)
    return buildAnnotatedString {
        var offset = 0
        while (offset < command.length) {
            val next = command.offsetByCodePoints(offset, 1)
            val colour = colours[offset]
            withStyle(SpanStyle(color = syntax[if (colour < syntax.size) colour else 0])) {
                append(command, offset, next)
                append('​')
            }
            offset = next
        }
    }
}

private fun shareText(text: String) {
    val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text)
    Platform.context.startActivity(Intent.createChooser(send, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
}
