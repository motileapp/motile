package app.motile.ui

import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.buildAnnotatedString
import app.motile.platform.Symbol
import app.motile.theme.IconSize
import app.motile.theme.symbolSide

/** The proxy's symbol in a line of text, after what runs through the proxy, centred on the line, where DM Sans centres its capitals. */
object ProxyMark {
    private const val ID = "proxy"

    /** The symbol where the text has it, which a `Text` draws given `content`, and reads as "Proxy". */
    fun AnnotatedString.Builder.appendMark() = appendInlineContent(ID, "Proxy")

    /** The symbol's size beside words of `fontSize`, as the Mac app sizes it. */
    fun size(fontSize: Float): IconSize = when {
        fontSize <= 12.5f -> IconSize.Xs
        fontSize <= 15f -> IconSize.Sm
        else -> IconSize.Md
    }

    /** What draws the symbol of `appendMark` beside words of `fontSize`, in `color`. */
    @Composable
    fun content(color: Color, fontSize: Float): Map<String, InlineTextContent> {
        val size = size(fontSize)
        val side = with(LocalDensity.current) { symbolSide(size).toSp() }
        return mapOf(ID to InlineTextContent(Placeholder(side, side, PlaceholderVerticalAlign.TextCenter)) { Icon(Symbol.Waypoints, size, color = color) })
    }
}

/** The words with the proxy's symbol after them where what they name runs through the proxy. */
fun AnnotatedString.proxied(proxied: Boolean): AnnotatedString {
    if (!proxied) return this
    val words = this
    return buildAnnotatedString {
        append(words)
        append(" ")
        with(ProxyMark) { appendMark() }
    }
}
