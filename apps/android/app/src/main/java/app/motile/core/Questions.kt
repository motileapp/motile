package app.motile.core

import android.os.Handler
import android.os.Looper

/**
 * How far the questions of a tool call are answered: the one that shows, the options chosen for
 * each, what was written for each in the composer, and the questions taking one answer whose
 * chosen answer is what was written. What was written stays when an option is chosen instead.
 */
data class QuestionAnswers(
    val index: Int = 0,
    val chosen: Map<String, Set<String>> = emptyMap(),
    val typed: Map<String, String> = emptyMap(),
    val writtenChosen: Set<String> = emptySet(),
)

private val main = Handler(Looper.getMainLooper())

/** The tool call with questions the open thread's agent waits on. While it shows, the composer writes the answer to its question in place of a message. */
val AppStore.asking: Approval?
    get() {
        if (selectedThread == null) return null
        val approval = activity.approvals.firstOrNull() ?: return null
        return approval.takeIf { it.questions.isNotEmpty() }
    }

private fun AppStore.answersOf(approval: Approval): QuestionAnswers = questionAnswers[approval.id] ?: QuestionAnswers()

private fun AppStore.update(approval: Approval, change: (QuestionAnswers) -> QuestionAnswers) {
    questionAnswers = questionAnswers + (approval.id to change(answersOf(approval)))
}

fun AppStore.questionIndex(approval: Approval): Int = answersOf(approval).index.coerceAtMost(approval.questions.size - 1)

fun AppStore.question(approval: Approval): Question = approval.questions[questionIndex(approval)]

fun AppStore.isLastQuestion(approval: Approval): Boolean = questionIndex(approval) >= approval.questions.size - 1

/** What was written for the question that shows. */
fun AppStore.typedAnswer(approval: Approval): String = answersOf(approval).typed[question(approval).id] ?: ""

/** Writing chooses what is written as the answer. */
fun AppStore.writeAnswer(text: String, approval: Approval) {
    if (text == typedAnswer(approval)) return
    val id = question(approval).id
    update(approval) {
        it.copy(typed = if (text.isEmpty()) it.typed - id else it.typed + (id to text), writtenChosen = it.writtenChosen + id)
    }
}

/** Whether what is written counts: beside the options of a question that takes several, or as the one answer when it was written or chosen last. */
fun AppStore.isWrittenChosen(approval: Approval): Boolean {
    if (typedAnswer(approval).isBlank()) return false
    val question = question(approval)
    return question.multiple || question.id in answersOf(approval).writtenChosen
}

fun AppStore.chooseWritten(approval: Approval) {
    val id = question(approval).id
    update(approval) { it.copy(writtenChosen = it.writtenChosen + id) }
}

fun AppStore.isChosen(option: Question.Choice, approval: Approval): Boolean {
    if (!question(approval).multiple && isWrittenChosen(approval)) return false
    return answersOf(approval).chosen[question(approval).id]?.contains(option.label) == true
}

/** Chooses an option, in place of what was written when it is the only answer, which goes on to the next question once it has been seen. */
fun AppStore.choose(option: Question.Choice, approval: Approval) {
    val question = question(approval)
    val index = questionIndex(approval)
    update(approval) { answers ->
        val picked = answers.chosen[question.id] ?: emptySet()
        val chosen = when {
            !question.multiple -> setOf(option.label)
            option.label in picked -> picked - option.label
            else -> picked + option.label
        }
        val writtenChosen = if (question.multiple) answers.writtenChosen else answers.writtenChosen - question.id
        answers.copy(writtenChosen = writtenChosen, chosen = answers.chosen + (question.id to chosen))
    }
    if (question.multiple || isLastQuestion(approval)) return
    main.postDelayed({
        val answers = questionAnswers[approval.id] ?: return@postDelayed
        if (answers.index != index) return@postDelayed
        update(approval) { it.copy(index = index + 1) }
    }, 200)
}

/** The options chosen for each question, in their order, and what was written: after them, or in their place where a question takes one. */
fun AppStore.answers(approval: Approval): Map<String, String> {
    val state = answersOf(approval)
    return buildMap {
        for (question in approval.questions) {
            val typed = (state.typed[question.id] ?: "").trim()
            val written = if (question.multiple || question.id in state.writtenChosen) typed else ""
            val picked = question.options.map { it.label }.filter { state.chosen[question.id]?.contains(it) == true }
            val parts = if (question.multiple) picked + written else listOf(written.ifEmpty { picked.firstOrNull() ?: "" })
            val answer = parts.filter { it.isNotEmpty() }.joinToString(", ")
            if (answer.isNotEmpty()) put(question.text, answer)
        }
    }
}

fun AppStore.canGoOn(approval: Approval): Boolean {
    val answers = answers(approval)
    if (!isLastQuestion(approval)) return answers[question(approval).text] != null
    return answers.size == approval.questions.size
}

/** Goes on to the next question, or answers them all after the last. */
fun AppStore.goOn(approval: Approval) {
    if (!canGoOn(approval)) return
    if (!isLastQuestion(approval)) {
        val index = questionIndex(approval)
        return update(approval) { it.copy(index = index + 1) }
    }
    answer(approval, allow = true, answers = answers(approval))
}

fun AppStore.goBack(approval: Approval) {
    val index = questionIndex(approval)
    if (index <= 0) return
    update(approval) { it.copy(index = index - 1) }
}
