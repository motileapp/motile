package app.motile.ui

import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.sp
import app.motile.platform.scaled
import app.motile.theme.LocalForeground
import app.motile.theme.Theme

/** Text in the system's font at a size given as on the Mac, in the foreground colour unless told otherwise. */
@Composable
fun Text(
    text: String,
    modifier: Modifier = Modifier,
    size: Float = 13f,
    weight: FontWeight = FontWeight.Normal,
    color: Color = LocalForeground.current,
    style: TextStyle? = null,
    maxLines: Int = Int.MAX_VALUE,
    overflow: TextOverflow = TextOverflow.Ellipsis,
    align: TextAlign? = null,
    onTextLayout: ((TextLayoutResult) -> Unit)? = null,
) {
    val base = style ?: Theme.ui(size, weight)
    BasicText(
        text = text,
        modifier = modifier,
        style = base.copy(color = color, textAlign = align ?: base.textAlign),
        maxLines = maxLines,
        overflow = overflow,
        onTextLayout = onTextLayout,
    )
}

@Composable
fun Text(
    text: AnnotatedString,
    modifier: Modifier = Modifier,
    size: Float = 13f,
    weight: FontWeight = FontWeight.Normal,
    color: Color = LocalForeground.current,
    style: TextStyle? = null,
    maxLines: Int = Int.MAX_VALUE,
    overflow: TextOverflow = TextOverflow.Ellipsis,
    align: TextAlign? = null,
    inlineContent: Map<String, InlineTextContent> = emptyMap(),
    onTextLayout: ((TextLayoutResult) -> Unit)? = null,
) {
    val base = style ?: Theme.ui(size, weight)
    BasicText(
        text = text,
        modifier = modifier,
        style = base.copy(color = color, textAlign = align ?: base.textAlign),
        maxLines = maxLines,
        overflow = overflow,
        inlineContent = inlineContent,
        onTextLayout = onTextLayout,
    )
}

/** The style with every line `height` tall, given as on the Mac, its text centred in it, the first and the last too. */
fun TextStyle.lined(height: Float): TextStyle = copy(
    lineHeight = scaled(height).sp,
    lineHeightStyle = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None),
)
