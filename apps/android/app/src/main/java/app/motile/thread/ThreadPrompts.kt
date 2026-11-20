package app.motile.thread

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import app.motile.LocalStore
import app.motile.core.AppStore
import app.motile.core.ThreadInfo
import app.motile.core.openFind
import app.motile.layout.KeyboardFocus
import app.motile.sidebar.threadMenu
import app.motile.theme.ControlSize
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.ShadowSize
import app.motile.theme.TextSize
import app.motile.theme.Theme
import app.motile.theme.shadow
import app.motile.ui.ActionButton
import app.motile.ui.Answer
import app.motile.ui.Asking
import app.motile.ui.ButtonVariant
import app.motile.ui.CloseModal
import app.motile.ui.Confirmation
import app.motile.ui.InputField
import app.motile.ui.MenuBuilder
import app.motile.ui.SheetSurface
import app.motile.ui.Text
import app.motile.ui.beside

/** Renaming a thread and asking before deleting one, which its menu starts. */
@Composable
fun ThreadPrompts(renaming: ThreadInfo?, deleting: ThreadInfo?, endRenaming: () -> Unit, endDeleting: () -> Unit) {
    val store = LocalStore.current
    renaming?.let { thread -> RenameDialog(thread, endRenaming) }
    Asking(deleting, onDismiss = endDeleting) { thread ->
        Confirmation(
            "Delete “${thread.title}”?", store.deletionNote(thread),
            answers = listOf(Answer("Delete", ButtonVariant.Danger) { store.delete(thread) }),
        )
    }
}

/** What the open thread's title offers: finding in it, then what its row offers. */
fun MenuBuilder.titleMenu(store: AppStore, thread: ThreadInfo, rename: (ThreadInfo) -> Unit, delete: (ThreadInfo) -> Unit) {
    item("Search in thread") { store.openFind() }
    divider()
    threadMenu(store, thread, rename, delete)
}

/** Renames a thread, as iOS's alert with a field does. */
@Composable
private fun RenameDialog(thread: ThreadInfo, onDismiss: () -> Unit) {
    val store = LocalStore.current
    val palette = Theme.colors
    var title by remember(thread.id) { mutableStateOf(thread.title) }
    val field = remember { FocusRequester() }
    val shape = RoundedCornerShape(Radius.xl)
    fun rename() {
        if (title.isBlank()) return
        store.rename(thread, title)
        onDismiss()
    }
    Dialog(onDismissRequest = onDismiss) {
        SheetSurface(CloseModal(onDismiss)) {
            Column(
                Modifier
                    .widthIn(max = 420.dp)
                    .shadow(ShadowSize.Xl, shape, Opacity.ShadowStrongest)
                    .background(palette.background, shape)
                    .border(1.dp, palette.border, shape)
                    .padding(20.dp),
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Text("Rename thread", size = TextSize.Lg, weight = FontWeight.SemiBold)
                InputField(
                    "Title", title, { title = it }, Modifier.fillMaxWidth(), size = ControlSize.Large, focusRequester = field,
                    onSubmit = ::rename,
                )
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    ActionButton("Cancel", variant = ButtonVariant.Ghost, size = ControlSize.Large, margin = beside(end = 8.dp), onClick = onDismiss)
                    ActionButton("Rename", variant = ButtonVariant.Primary, size = ControlSize.Large, disabled = title.isBlank(), onClick = ::rename)
                }
            }
        }
    }
    LaunchedEffect(Unit) { runCatching { field.requestFocus() } }
    DisposableEffect(Unit) { onDispose { KeyboardFocus.editable = false } }
}
