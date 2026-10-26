package app.motile.composer

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Approval
import app.motile.platform.Symbol
import app.motile.theme.iconSize
import app.motile.theme.ControlSize
import app.motile.theme.LocalForeground
import app.motile.theme.Palette
import app.motile.theme.Radius
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.FadingScroll
import app.motile.ui.Icon
import app.motile.ui.Text

/**
 * The tool call the agent waits with, above the composer: it is allowed or refused, and one that
 * asks questions is answered. The calls behind it wait their turn.
 */
object WaitingStrip {
    val padding = 12.dp
    internal val targetInset = 10.dp
}

@Composable
fun WaitingStrip(approval: Approval, count: Int) {
    Box(Modifier.fillMaxWidth().composerStrip(ComposerStrip.Edge.Top, height = null)) {
        if (approval.questions.isEmpty()) {
            Call(approval, count)
        } else {
            key(approval.id) { QuestionsView(approval, Modifier.padding(WaitingStrip.padding)) }
        }
    }
}

@Composable
private fun Call(approval: Approval, count: Int) {
    val store = LocalStore.current
    val palette = Theme.colors
    val padding = WaitingStrip.padding
    // Eight lines of a command, after which it scrolls inside its box.
    val targetHeight = with(LocalDensity.current) { Theme.codeLineHeight.toDp() } * 8 + WaitingStrip.targetInset * 2
    Column(Modifier.fillMaxWidth()) {
        Row(
            Modifier.fillMaxWidth().padding(start = padding, end = padding, top = padding, bottom = if (approval.target.isEmpty()) padding else 8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            CompositionLocalProvider(LocalForeground provides palette.warning) {
                WaitingTitle(approval.title, approval.symbol, if (count > 1) "1 of $count" else null, Modifier.weight(1f))
            }
            ActionButton(approval.refuseLabel, size = ControlSize.Small) { store.answer(approval, allow = false) }
            ActionButton(approval.allowLabel, variant = ButtonVariant.Warning, size = ControlSize.Small) { store.answer(approval, allow = true) }
        }
        if (approval.target.isNotEmpty()) {
            val typeset = remember(approval, palette) { typesetTarget(approval, palette) }
            FadingScroll(
                targetHeight,
                Modifier
                    .padding(start = padding, end = padding, bottom = padding)
                    .fillMaxWidth()
                    .background(palette.borderComposer, RoundedCornerShape(Radius.md)),
            ) {
                Text(
                    typeset, Modifier.padding(WaitingStrip.targetInset),
                    style = Theme.inlineCodeFont.copy(lineHeight = Theme.codeLineHeight), color = palette.foreground,
                )
            }
        }
    }
}

/** What the tool acts on, in the code colours of its spans. */
private fun typesetTarget(approval: Approval, palette: Palette): AnnotatedString {
    val syntax = Theme.syntax(palette)
    val target = approval.target
    val spans = approval.targetSpans
    return buildAnnotatedString {
        append(target)
        var index = 0
        while (index + 2 < spans.size) {
            val start = spans[index]
            val end = start + spans[index + 1]
            val colour = spans[index + 2]
            index += 3
            if (start < 0 || end > target.length || colour <= 0 || colour >= syntax.size) continue
            addStyle(SpanStyle(color = syntax[colour]), start, end)
        }
    }
}

/** What a waiting strip is about, and which of several it is. */
@Composable
fun WaitingTitle(title: String, symbol: Symbol, place: String?, modifier: Modifier = Modifier) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
        Icon(symbol, iconSize(text = 12f))
        Text(title, Modifier.weight(1f, fill = false), size = 12f, weight = FontWeight.SemiBold, maxLines = 1)
        if (place != null) Text(place, style = Theme.uiDigits(12f), color = Theme.colors.mutedForeground, maxLines = 1)
    }
}
