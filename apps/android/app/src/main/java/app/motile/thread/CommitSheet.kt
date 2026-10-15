package app.motile.thread

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.motile.LocalCloseSheet
import app.motile.LocalStore
import app.motile.core.ChangedFile
import app.motile.core.Project
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.Radius
import app.motile.theme.SurfaceBox
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.Text
import app.motile.ui.TextArea

private val rowHeight = 34.dp
private val listMaxHeight = 192.dp

/**
 * The sheet behind the git menu's Commit: the files to commit, which can be left out one by one,
 * and a message that is written for the user when they leave it empty.
 */
@Composable
fun CommitSheet(project: Project) {
    val store = LocalStore.current
    val close = LocalCloseSheet.current
    val palette = Theme.colors
    var message by remember { mutableStateOf("") }
    var excluded by remember { mutableStateOf(setOf<String>()) }
    val files = store.gitFiles
    val included = files.map { it.path }.filter { it !in excluded }

    fun commit(onNewBranch: Boolean) {
        val paths = if (excluded.isEmpty()) emptyList() else included
        val written = message.trim()
        close()
        store.runGit("commit", project, message = written, paths = paths, newBranch = onNewBranch)
    }

    Column(
        Modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .navigationBarsPadding()
            .imePadding()
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("Commit Changes", size = 15f, weight = FontWeight.SemiBold)
            Text("Review and confirm your commit. Leave the message empty to have one written.", size = 12f, color = palette.mutedForeground)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Branch", size = 12.5f, color = palette.mutedForeground)
            Text(project.git?.branch ?: "No branch", Modifier.weight(1f), size = 12.5f, weight = FontWeight.Medium, maxLines = 1)
            if (project.git?.isDefault == true) Text("Default branch", size = 12.5f, color = palette.warning)
        }
        FileList(files, excluded) { excluded = it }
        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text("Commit message (optional)", size = 12.5f, weight = FontWeight.Medium)
            TextArea("Leave empty to have one written", message, { message = it }, Modifier.fillMaxWidth(), lines = 4)
        }
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            ActionButton("Commit", variant = ButtonVariant.Primary, size = ControlSize.Large, fills = true, disabled = included.isEmpty()) {
                commit(onNewBranch = false)
            }
            ActionButton("Commit on New Branch", size = ControlSize.Large, fills = true, disabled = included.isEmpty()) {
                commit(onNewBranch = true)
            }
            ActionButton("Cancel", variant = ButtonVariant.Ghost, size = ControlSize.Large, fills = true) { close() }
        }
    }
}

/** The files with changes; while editing, each can be left out, or all at once. */
@Composable
private fun FileList(files: List<ChangedFile>, excluded: Set<String>, setExcluded: (Set<String>) -> Unit) {
    val palette = Theme.colors
    var editing by remember { mutableStateOf(false) }
    val included = files.count { it.path !in excluded }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Files", size = 12.5f, weight = FontWeight.Medium)
            if (excluded.isNotEmpty() && !editing) Text("$included of ${files.size}", size = 12f, color = palette.mutedForeground)
            Spacer(Modifier.weight(1f))
            Row(Modifier.outset(end = ControlSize.Small.padding, vertical = 4.dp)) {
                if (editing) {
                    ActionButton(if (excluded.isEmpty()) "Select None" else "Select All", variant = ButtonVariant.Link, size = ControlSize.Small) {
                        setExcluded(if (excluded.isEmpty()) files.map { it.path }.toSet() else emptySet())
                    }
                }
                ActionButton(if (editing) "Done" else "Edit", variant = ButtonVariant.Link, size = ControlSize.Small) { editing = !editing }
            }
        }
        val height = minOf(rowHeight * files.size, listMaxHeight) + 8.dp
        SurfaceBox(RoundedCornerShape(Radius.md), Modifier.fillMaxWidth().height(height), bordered = true) {
            LazyColumn(Modifier.fillMaxWidth().padding(horizontal = 10.dp, vertical = 4.dp)) {
                items(files, key = { it.path }) { file ->
                    FileRow(file, file.path in excluded, editing) {
                        setExcluded(if (file.path in excluded) excluded - file.path else excluded + file.path)
                    }
                }
            }
        }
    }
}

@Composable
private fun FileRow(file: ChangedFile, isExcluded: Boolean, editing: Boolean, toggle: () -> Unit) {
    val palette = Theme.colors
    Row(Modifier.fillMaxWidth().height(rowHeight), verticalAlignment = Alignment.CenterVertically) {
        if (editing) {
            ActionButton(
                icon = if (isExcluded) Symbol.Circle else Symbol.CircleCheck, help = if (isExcluded) "Include" else "Leave out", size = ControlSize.Small,
                tint = if (isExcluded) palette.mutedStrongerForeground else palette.primary, modifier = Modifier.outset(start = 6.dp), onClick = toggle,
            )
            Spacer(Modifier.width(8.dp))
        }
        Text(
            file.path, Modifier.weight(1f), size = 12.5f, maxLines = 1, overflow = TextOverflow.StartEllipsis,
            color = if (isExcluded) palette.mutedStrongerForeground else palette.foreground,
        )
        Spacer(Modifier.width(8.dp))
        when {
            isExcluded -> Text("Excluded", size = 12.5f, color = palette.mutedStrongerForeground)
            file.added + file.removed > 0 -> LineCounts(file.added, file.removed)
            else -> Text(if (file.change == "added") "new" else file.change, size = 12.5f, color = palette.mutedStrongerForeground)
        }
    }
}

/** The lines added and removed, as git counts them. */
@Composable
private fun LineCounts(added: Int, removed: Int) {
    val palette = Theme.colors
    val style = Theme.uiDigits(11f, FontWeight.Medium)
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        if (added > 0) Text("+$added", style = style, color = palette.success)
        if (removed > 0) Text("−$removed", style = style, color = palette.destructive)
    }
}
