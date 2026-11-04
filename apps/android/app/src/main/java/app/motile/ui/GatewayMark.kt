package app.motile.ui

import androidx.compose.foundation.layout.Box
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
import app.motile.theme.symbolSide

/** The gateway's symbol in a line of text, after what runs through the gateway, centred on the line, where DM Sans centres its capitals. */
object GatewayMark {
    private const val ID = "gateway"

    /** What the symbol stands from the words before it beyond a space. */
    val gap: Dp = 1.dp

    /** The symbol where the text has it, which a `Text` draws given `content`, and reads as "Gateway". */
    fun AnnotatedString.Builder.appendMark() = appendInlineContent(ID, "Gateway")

    /** The symbol's size beside words of `fontSize`, as the Mac app sizes it. */
    fun size(fontSize: Float): IconSize = when {
        fontSize <= 12.5f -> IconSize.Xs
        fontSize <= 15f -> IconSize.Sm
        else -> IconSize.Md
    }

    /** What draws the symbol of `appendMark` beside words of `fontSize`, in `color`, as large as `symbol` where it matches an icon beside it. */
    @Composable
    fun content(color: Color, fontSize: Float, symbol: IconSize? = null): Map<String, InlineTextContent> {
        val size = symbol ?: size(fontSize)
        val (width, height) = with(LocalDensity.current) { (symbolSide(size) + gap).toSp() to symbolSide(size).toSp() }
        return mapOf(
            ID to InlineTextContent(Placeholder(width, height, PlaceholderVerticalAlign.TextCenter)) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.CenterEnd) { Icon(Symbol.Waypoints, size, color = color) }
            },
        )
    }
}

/** The words with the gateway's symbol after the name they start with, ahead of the account after its dot, where what they name runs through the gateway: "Opus 5.5 ⟿ · Work". */
fun AnnotatedString.proxied(proxied: Boolean): AnnotatedString {
    if (!proxied) return this
    val words = this
    val name = text.indexOf(" · ").takeIf { it >= 0 } ?: length
    return buildAnnotatedString {
        append(words.subSequence(0, name))
        if (name > 0) append(" ")
        with(GatewayMark) { appendMark() }
        append(words.subSequence(name, words.length))
    }
}
