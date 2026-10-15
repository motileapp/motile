package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import app.motile.theme.LocalForeground
import app.motile.theme.LocalPalette
import app.motile.theme.LocalRow
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.ShadowSize
import app.motile.theme.Surface
import app.motile.theme.scrim
import app.motile.theme.shadow
import kotlinx.coroutines.launch

/** Closes the modal a view lies in. */
class CloseModal(private val action: () -> Unit) {
    operator fun invoke() = action()
}

val LocalCloseModal = staticCompositionLocalOf { CloseModal {} }

/**
 * What `Asking` puts to the user: a title, what it means, and the answers besides the one that lets
 * it go. The last answer is the one Enter picks.
 */
data class Confirmation(
    val title: String,
    val message: String = "",
    val answers: List<Answer> = emptyList(),
    /** What the button that lets it go says. */
    val dismissal: String = "Cancel",
)

data class Answer(val title: String, val variant: ButtonVariant = ButtonVariant.Primary, val action: () -> Unit)

/** Shows the content while `item` is set, as a sheet from the bottom on the background. The content closes it with `LocalCloseModal`. */
@Composable
fun <T : Any> Modal(item: T?, onDismiss: () -> Unit, content: @Composable (T) -> Unit) {
    if (item == null) return
    Sheet(onDismiss) { content(item) }
}

@Composable
fun Modal(shown: Boolean, onDismiss: () -> Unit, content: @Composable () -> Unit) {
    if (!shown) return
    Sheet(onDismiss, content)
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Sheet(onDismiss: () -> Unit, content: @Composable () -> Unit) {
    val palette = LocalPalette.current
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val scope = rememberCoroutineScope()
    val close = CloseModal { scope.launch { state.hide() }.invokeOnCompletion { onDismiss() } }
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = state,
        shape = RoundedCornerShape(topStart = Radius.xxl, topEnd = Radius.xxl),
        containerColor = palette.background,
        contentColor = palette.foreground,
        scrimColor = palette.scrim,
        tonalElevation = 0.dp,
        dragHandle = { Grabber() },
    ) {
        SheetSurface(close, content)
    }
}

/** The bar across a sheet's top that says it can be pulled down. */
@Composable
private fun Grabber() {
    Box(Modifier.padding(vertical = 8.dp).size(width = 36.dp, height = 5.dp).background(LocalPalette.current.mutedStrongestForeground, CircleShape))
}

/** A sheet's content: on the background, with what lies on it filled from there, and closed by `LocalCloseModal`. */
@Composable
fun SheetSurface(close: CloseModal, content: @Composable () -> Unit) {
    CompositionLocalProvider(
        LocalSurface provides Surface.Background,
        LocalRow provides null,
        LocalForeground provides LocalPalette.current.foreground,
        LocalCloseModal provides close,
        content = content,
    )
}

/** Asks before something happens, while `item` is set: a card over the window with the answers under what it asks. */
@Composable
fun <T : Any> Asking(item: T?, onDismiss: () -> Unit, confirmation: (T) -> Confirmation) {
    if (item == null) return
    ConfirmationDialog(confirmation(item), onDismiss)
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ConfirmationDialog(confirmation: Confirmation, onDismiss: () -> Unit) {
    val palette = LocalPalette.current
    val shape = RoundedCornerShape(Radius.xl)
    val last = confirmation.answers.lastOrNull()
    val answer = { chosen: Answer ->
        onDismiss()
        chosen.action()
    }
    Dialog(onDismissRequest = onDismiss) {
        SheetSurface(CloseModal(onDismiss)) {
            Column(
                Modifier
                    .widthIn(max = 420.dp)
                    .shadow(ShadowSize.Xl, shape, Opacity.ShadowStrongest)
                    .background(palette.background, shape)
                    .border(1.dp, palette.border, shape)
                    .onPreviewKeyEvent { event ->
                        if (event.type != KeyEventType.KeyDown || event.key != Key.Enter || last == null) return@onPreviewKeyEvent false
                        answer(last)
                        true
                    }
                    .padding(20.dp),
            ) {
                Text(confirmation.title, size = 15f, weight = FontWeight.SemiBold)
                if (confirmation.message.isNotEmpty()) {
                    Text(confirmation.message, Modifier.padding(top = 6.dp), size = 13f, color = palette.mutedForeground)
                }
                FlowRow(
                    Modifier.fillMaxWidth().padding(top = 22.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    val dismissal = if (confirmation.answers.isEmpty()) ButtonVariant.Primary else ButtonVariant.Secondary
                    ActionButton(confirmation.dismissal, variant = dismissal, size = app.motile.theme.ControlSize.Large, onClick = onDismiss)
                    for (each in confirmation.answers) {
                        ActionButton(each.title, variant = each.variant, size = app.motile.theme.ControlSize.Large) { answer(each) }
                    }
                }
            }
        }
    }
}
