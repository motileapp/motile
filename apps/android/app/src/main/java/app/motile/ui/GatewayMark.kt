package app.motile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.platform.Symbol
import app.motile.theme.IconSize
import app.motile.theme.TextSize
import app.motile.theme.symbolSide

/** The gateway's symbol in a line of text, after what runs through the gateway, centred on the line, where DM Sans centres its capitals, and a pin after it where that is pinned to one of the gateway's accounts. */
object GatewayMark {
    private const val ID = "gateway"
    private const val PINNED_ID = "gateway-pinned"

    /** What the symbol stands from the words before it beyond a space. */
    val gap: Dp = 1.dp

    /** What the pin stands from the gateway's symbol. */
    private val pinGap: Dp = 2.dp

    /** The symbol where the text has it, with the pin after it when `pinned`, which a `Text` draws given `content`, and reads as "Gateway", or "Gateway, pinned". */
    fun AnnotatedString.Builder.appendMark(pinned: Boolean = false) {
        if (pinned) appendInlineContent(PINNED_ID, "Gateway, pinned") else appendInlineContent(ID, "Gateway")
    }

    /** The symbol's size beside words of `fontSize`, or a step smaller than `icon`, an icon beside it, as the Mac app sizes it. */
    fun size(fontSize: TextSize, icon: IconSize? = null): IconSize {
        if (icon != null) return IconSize.entries.lastOrNull { it.side < icon.side } ?: icon
        return when (fontSize) {
            TextSize.Xxs, TextSize.Xs -> IconSize.Xxs
            TextSize.Sm, TextSize.Md -> IconSize.Xs
            TextSize.Lg, TextSize.Xl, TextSize.Xxl -> IconSize.Sm
        }
    }

    /** What draws the symbols of `appendMark` beside words of `fontSize`, in `color`, a step smaller than `symbol` where they go with an icon beside them. */
    @Composable
    fun content(color: Color, fontSize: TextSize, symbol: IconSize? = null): Map<String, InlineTextContent> {
        val size = size(fontSize, symbol)
        val side = symbolSide(size)
        val density = LocalDensity.current
        val height = with(density) { side.toSp() }
        val mark = { pinned: Boolean ->
            val width = with(density) { (if (pinned) gap + side + pinGap + side else gap + side).toSp() }
            InlineTextContent(Placeholder(width, height, PlaceholderVerticalAlign.TextCenter)) {
                Row(Modifier.fillMaxSize(), Arrangement.spacedBy(pinGap, Alignment.End), Alignment.CenterVertically) {
                    Icon(Symbol.Waypoints, size, color = color)
                    if (pinned) Icon(Symbol.Pin, size, color = color)
                }
            }
        }
        return mapOf(ID to mark(false), PINNED_ID to mark(true))
    }
}

/** The words with the gateway's symbol after the name they start with, ahead of the account after its dot, where what they name runs through the gateway: "Opus 5.5 ⟿ · Work", and a pin after it where it is pinned to that account. */
fun AnnotatedString.proxied(proxied: Boolean, pinned: Boolean = false): AnnotatedString {
    if (!proxied) return this
    val words = this
    val name = text.indexOf(" · ").takeIf { it >= 0 } ?: length
    return buildAnnotatedString {
        append(words.subSequence(0, name))
        if (name > 0) append(" ")
        with(GatewayMark) { appendMark(pinned) }
        append(words.subSequence(name, words.length))
    }
}
