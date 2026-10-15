package app.motile.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.layout.layout
import androidx.compose.ui.semantics.hideFromAccessibility
import androidx.compose.ui.semantics.semantics

/** Whether the view is put away. */
val LocalPutAway = compositionLocalOf { false }

/**
 * Out of sight and out of reach, but still there: it comes back as it was left. It keeps its size
 * where it is, but is drawn and pressed nowhere, and focus doesn't go into it.
 */
@OptIn(ExperimentalComposeUiApi::class)
fun Modifier.putAway(away: Boolean): Modifier {
    if (!away) return this
    return semantics { hideFromAccessibility() }
        .focusProperties { onEnter = { cancelFocusChange() } }
        .layout { measurable, constraints ->
            val placeable = measurable.measure(constraints)
            layout(placeable.width, placeable.height) { placeable.place(-farAway, -farAway) }
        }
}

/** Puts the content away while `away`, telling what is inside with `LocalPutAway`. */
@Composable
fun PutAway(away: Boolean, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val inherited = LocalPutAway.current
    Box(modifier.putAway(away)) {
        CompositionLocalProvider(LocalPutAway provides (inherited || away), content = content)
    }
}

/** Further than any window reaches. */
private const val farAway = 1 shl 20
