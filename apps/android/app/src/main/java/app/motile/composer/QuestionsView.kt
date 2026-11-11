package app.motile.composer

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Approval
import app.motile.core.Question
import app.motile.core.canGoOn
import app.motile.core.choose
import app.motile.core.goBack
import app.motile.core.goOn
import app.motile.core.isChosen
import app.motile.core.isLastQuestion
import app.motile.core.question
import app.motile.core.questionIndex
import app.motile.core.typedAnswer
import app.motile.platform.Symbol
import app.motile.theme.IconSize
import app.motile.theme.LocalForeground
import app.motile.theme.Radius
import app.motile.theme.TextSize
import app.motile.theme.Theme
import app.motile.theme.at
import app.motile.theme.wash
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.Chip
import app.motile.ui.Disabled
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.PickMark
import app.motile.ui.Text
import app.motile.ui.beside

/** How far past its text an option's light and the room to press it reach. */
private val optionReach = 8.dp
private val optionGap = 2.dp
private val markGap = 10.dp

/**
 * The questions an agent asks with a tool call, one at a time. Each takes one of its options,
 * several of them when it allows that, or an answer written in the composer in place of them.
 */
@Composable
fun QuestionsView(approval: Approval, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val palette = Theme.colors
    val questions = approval.questions
    val question = store.question(approval)
    val index = store.questionIndex(approval)

    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            CompositionLocalProvider(LocalForeground provides palette.warning) {
                WaitingTitle(
                    question.header.ifEmpty { approval.title }, Symbol.MessageCircleQuestionMark,
                    if (questions.size > 1) "${index + 1} of ${questions.size}" else null,
                )
            }
            Text(question.text, size = TextSize.Sm, weight = FontWeight.Medium, style = Theme.ui(TextSize.Sm, FontWeight.Medium).copy(lineHeight = Theme.ui(TextSize.Sm).fontSize * 1.35f))
            if (question.multiple) Text("Choose any that apply", size = TextSize.Xs, color = palette.mutedForeground)
        }
        Column(Modifier.overhang(horizontal = optionReach), verticalArrangement = Arrangement.spacedBy(optionGap)) {
            for (option in question.options) OptionRow(approval, option, question.multiple)
            OwnAnswerRow(approval, question.multiple)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            if (index > 0) ActionButton("Back", Symbol.ChevronLeft, variant = ButtonVariant.Ghost) { store.goBack(approval) }
            Spacer(Modifier.weight(1f))
            Row(verticalAlignment = Alignment.CenterVertically) {
                ActionButton(approval.refuseLabel, variant = ButtonVariant.Ghost, margin = beside(end = 8.dp)) { store.answer(approval, allow = false) }
                Disabled(!store.canGoOn(approval)) {
                    ActionButton(if (store.isLastQuestion(approval)) approval.allowLabel else "Next", variant = ButtonVariant.Primary) { store.goOn(approval) }
                }
            }
        }
    }
}

@Composable
private fun OptionRow(approval: Approval, option: Question.Choice, multiple: Boolean) {
    val store = LocalStore.current
    val palette = Theme.colors
    val picked = store.isChosen(option, approval)
    HighlightButton({ store.choose(option, approval) }, Modifier.fillMaxWidth(), radius = Radius.md, fill = palette.borderComposer) {
        Row(Modifier.fillMaxWidth().picked(picked, pickedLook()).padding(horizontal = optionReach, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(markGap)) {
            PickMark(picked, multiple, Modifier.padding(top = 2.dp))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(option.label, Modifier.weight(1f, fill = false).alignByBaseline(), size = TextSize.Sm, weight = FontWeight.Medium, color = palette.foreground)
                    if (option.recommended) Chip("Recommended", Modifier.alignByBaseline(), tone = palette.success, compact = true)
                }
                if (option.detail.isNotEmpty()) Text(option.detail, size = TextSize.Xs, color = palette.mutedForeground)
            }
        }
    }
}

/** The last row, which shows the answer written in the composer, or says to write one there. */
@Composable
private fun OwnAnswerRow(approval: Approval, multiple: Boolean) {
    val store = LocalStore.current
    val palette = Theme.colors
    val written = store.typedAnswer(approval).trim()
    HighlightButton({ store.focusComposer() }, Modifier.fillMaxWidth(), radius = Radius.md, fill = palette.borderComposer) {
        Row(Modifier.fillMaxWidth().picked(written.isNotEmpty(), pickedLook()).padding(horizontal = optionReach, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(markGap)) {
            if (written.isEmpty()) {
                Box(Modifier.width(PickMark.size).padding(top = 2.dp), contentAlignment = Alignment.Center) {
                    Icon(Symbol.PencilLine, IconSize.Sm, color = palette.mutedForeground)
                }
            } else {
                PickMark(true, multiple, Modifier.padding(top = 2.dp))
            }
            Text(
                written.ifEmpty { if (multiple) "Add your own answer below" else "Or type your own answer below" }, Modifier.weight(1f), size = TextSize.Sm, weight = FontWeight.Medium,
                color = if (written.isEmpty()) palette.mutedForeground else palette.foreground, maxLines = 3,
            )
        }
    }
}

/** The option's look once it is the answer: the primary colour's wash inside a line of its stronger wash, apart from the plain light of a press. */
@Composable
private fun pickedLook(): PickedLook {
    val palette = Theme.colors
    return PickedLook(palette.primary.wash(), palette.primary.wash(lit = true))
}

private data class PickedLook(val wash: Color, val line: Color)

private fun Modifier.picked(picked: Boolean, look: PickedLook): Modifier {
    if (!picked) return this
    val shape = RoundedCornerShape(Radius.md)
    return background(look.wash, shape).border(1.dp, look.line, shape)
}
