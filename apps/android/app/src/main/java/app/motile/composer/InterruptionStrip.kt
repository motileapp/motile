package app.motile.composer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.constrainWidth
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Interruption
import app.motile.core.Time
import app.motile.theme.TextSize
import app.motile.theme.iconSize
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.Icon
import app.motile.ui.Text
import app.motile.usage.ResetMenu
import kotlinx.coroutines.delay

/**
 * Why the agent stopped before it finished, above the composer: the usage limit, with when the
 * thread goes on by itself and the account's resets to refill it now, or a restart of the server,
 * whose agent is continued from here.
 */
@Composable
fun InterruptionStrip(interruption: Interruption) {
    val store = LocalStore.current
    val palette = Theme.colors
    var now by remember { mutableDoubleStateOf(Time.now()) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(30_000)
            now = Time.now()
        }
    }
    val detail = interruption.detail(now)
    Row(Modifier.fillMaxWidth().composerStrip(ComposerStrip.Edge.Top), verticalAlignment = Alignment.CenterVertically) {
        Icon(interruption.symbol, iconSize(text = TextSize.Xs), Modifier.padding(start = 14.dp, end = 7.dp), color = palette.warning)
        // A narrow composer keeps what happens next and leaves out the title.
        FirstThatFits(
            Modifier.weight(1f).padding(end = 8.dp).semantics { contentDescription = "${interruption.title}. $detail." },
            {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(interruption.title, size = TextSize.Xs, weight = FontWeight.Medium, color = palette.warning, maxLines = 1)
                    Text(detail, size = TextSize.Xs, color = palette.mutedForeground, maxLines = 1)
                }
            },
            { Text(detail, size = TextSize.Xs, color = palette.warning, maxLines = 1, overflow = TextOverflow.MiddleEllipsis) },
        )
        val thread = store.selectedThread
        val resets = thread?.takeIf { interruption is Interruption.Limit }?.let(store::resets)
            ?.takeIf { held -> held.options.any { it.usable || it.atLimit } }
        // As the strip's other controls, filled where a control isn't lit under the pointer.
        if (thread != null && resets != null) ResetMenu(resets, ButtonVariant.Secondary, ComposerStrip.margin, thread = thread)
        val button = @Composable { title: String, help: String, action: () -> Unit ->
            ActionButton(title, help = help, variant = ButtonVariant.Ghost, margin = ComposerStrip.margin, onClick = action)
        }
        when {
            interruption is Interruption.Limit && interruption.resetsAt != null && interruption.resetsAt > now ->
                if (interruption.continues) {
                    button("Cancel", "Don't continue when the limit resets") { store.setContinues(false) }
                } else {
                    button("Continue at Reset", "Continue the agent once the limit resets") { store.setContinues(true) }
                }
            interruption is Interruption.Restart -> button("Continue", "Have the agent go on where it left off") { store.continueThread() }
        }
    }
}

/** The first of the views that fits the width it is given, as SwiftUI's `ViewThatFits`; the last when none does. */
@Composable
fun FirstThatFits(modifier: Modifier = Modifier, vararg options: @Composable () -> Unit) {
    Layout(contents = options.toList(), modifier = modifier) { measurables, constraints ->
        val fitting = measurables.indexOfFirst { group ->
            val measurable = group.firstOrNull() ?: return@indexOfFirst false
            measurable.maxIntrinsicWidth(Constraints.Infinity) <= constraints.maxWidth
        }
        val chosen = measurables[if (fitting >= 0) fitting else measurables.lastIndex].firstOrNull()
        val placeable = chosen?.measure(constraints.copy(minWidth = 0))
        layout(constraints.constrainWidth(placeable?.width ?: 0), placeable?.height ?: 0) { placeable?.place(0, 0) }
    }
}
