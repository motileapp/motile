package app.motile.composer

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Approval
import app.motile.core.Question
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalForeground
import app.motile.theme.Radius
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.Disabled
import app.motile.ui.HighlightButton
import app.motile.ui.HoverHighlight
import app.motile.ui.InputField
import app.motile.ui.InputVariant
import app.motile.ui.PickMark
import app.motile.ui.Text
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** How far past its text an option's light and the room to press it reach. */
private val optionReach = 8.dp
private val markGap = 10.dp

/**
 * The questions an agent asks with a tool call, one at a time. Each takes one of its options,
 * several of them when it allows that, or an answer typed in place of them.
 */
@Composable
fun QuestionsView(approval: Approval, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val palette = Theme.colors
    val scope = rememberCoroutineScope()
    var index by remember { mutableIntStateOf(0) }
    val chosen = remember { mutableStateMapOf<String, Set<String>>() }
    val typed = remember { mutableStateMapOf<String, String>() }
    var typing by remember { mutableStateOf(false) }
    val typingFocus = remember { FocusRequester() }

    val questions = approval.questions
    val question = questions[index.coerceAtMost(questions.size - 1)]
    val isLast = index >= questions.size - 1

    /** What was typed for a question, or else the options chosen for it, in their order. */
    fun answers(): Map<String, String> = buildMap {
        for (asked in questions) {
            val written = (typed[asked.id] ?: "").trim()
            val picked = asked.options.map { it.label }.filter { chosen[asked.id]?.contains(it) == true }
            val answer = written.ifEmpty { picked.joinToString(", ") }
            if (answer.isNotEmpty()) put(asked.text, answer)
        }
    }

    fun canGoOn(): Boolean = if (isLast) answers().size == questions.size else answers()[question.text] != null

    /** Answers the questions after the last, or else goes on to the next. */
    fun goOn() {
        if (!canGoOn()) return
        if (!isLast) {
            index += 1
            return
        }
        store.answer(approval, allow = true, answers = answers())
    }

    /** What is typed counts in place of the options, so none shows as chosen beside it. */
    fun isChosen(option: Question.Choice): Boolean {
        if ((typed[question.id] ?: "").isNotEmpty()) return false
        return chosen[question.id]?.contains(option.label) == true
    }

    fun choose(option: Question.Choice) {
        typed.remove(question.id)
        typing = false
        if (!question.multiple) {
            chosen[question.id] = setOf(option.label)
            if (isLast) return
            val asked = index
            scope.launch {
                delay(200)
                if (index == asked) index += 1
            }
            return
        }
        val picked = chosen[question.id] ?: emptySet()
        chosen[question.id] = if (option.label in picked) picked - option.label else picked + option.label
    }

    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            CompositionLocalProvider(LocalForeground provides palette.warning) {
                WaitingTitle(
                    question.header.ifEmpty { approval.title }, Symbol.MessageCircleQuestionMark,
                    if (questions.size > 1) "${index + 1} of ${questions.size}" else null,
                )
            }
            Text(question.text, size = 13.5f, weight = FontWeight.Medium, style = Theme.ui(13.5f, FontWeight.Medium).copy(lineHeight = Theme.ui(13.5f).fontSize * 1.35f))
            if (question.multiple) Text("Choose any that apply", size = 12.5f, color = palette.mutedForeground)
        }
        Column(Modifier.overhang(horizontal = optionReach), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            for (option in question.options) {
                val picked = isChosen(option)
                HighlightButton({ choose(option) }, Modifier.fillMaxWidth(), radius = Radius.md, lit = picked, fill = palette.borderComposer) {
                    Row(Modifier.padding(horizontal = optionReach, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(markGap)) {
                        PickMark(picked, question.multiple, Modifier.padding(top = 2.dp))
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(option.label, size = 13f, weight = FontWeight.Medium, color = palette.foreground)
                            if (option.detail.isNotEmpty()) Text(option.detail, size = 12f, color = palette.mutedForeground)
                        }
                    }
                }
            }
            val written = (typed[question.id] ?: "").isNotEmpty()
            HoverHighlight(
                Modifier.fillMaxWidth().clickable(interactionSource = null, indication = null) { runCatching { typingFocus.requestFocus() } },
                radius = Radius.md, lit = written || typing, fill = palette.borderComposer,
            ) {
                Row(
                    Modifier.padding(horizontal = optionReach, vertical = 8.dp),
                    horizontalArrangement = Arrangement.spacedBy(markGap),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    PickMark(written, question.multiple)
                    InputField(
                        "Something else", typed[question.id] ?: "", { typed[question.id] = it },
                        Modifier.weight(1f).height(ControlSize.Large.height).offset(x = -(ControlSize.Large.padding - 2.dp)),
                        variant = InputVariant.Bare, size = ControlSize.Large, focusRequester = typingFocus,
                        onFocusChange = { typing = it }, onSubmit = ::goOn,
                    )
                }
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            if (index > 0) ActionButton("Back", Symbol.ChevronLeft, variant = ButtonVariant.Ghost, size = ControlSize.Small) { index -= 1 }
            Spacer(Modifier.weight(1f))
            ActionButton(approval.refuseLabel, size = ControlSize.Small) { store.answer(approval, allow = false) }
            Disabled(!canGoOn()) {
                ActionButton(if (isLast) approval.allowLabel else "Next", variant = ButtonVariant.Primary, size = ControlSize.Small, onClick = ::goOn)
            }
        }
    }
}
