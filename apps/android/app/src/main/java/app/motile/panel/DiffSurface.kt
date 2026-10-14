package app.motile.panel

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Loaded
import app.motile.platform.Symbol
import app.motile.theme.LocalPalette
import app.motile.ui.ActionButton
import app.motile.ui.ActionMenu
import app.motile.ui.Modal
import app.motile.ui.Text

/** The changes in the thread's folder: what isn't committed, what the branch adds, what one turn did, or a pull request or one of its commits. */
@Composable
fun DiffSurface(target: PanelTarget) {
    val store = LocalStore.current
    val panel = store.rightPanel
    val content = panel.content
    val scope = panel.scope(target)
    var asked by remember { mutableIntStateOf(0) }
    var commenting by remember { mutableStateOf<CommentableLine?>(null) }
    val page = shownPage(panel.pullRequests.page.value, scope)
    val document = content.diff.value
    Column(Modifier.fillMaxSize()) {
        PanelBar {
            ScopeMenu(target, scope)
            if (document != null && document.files.isNotEmpty()) {
                Text(LineCountText.text(document.added, document.removed), Modifier.padding(start = 4.dp), style = LineCountText.style, maxLines = 1)
            }
            if (scope is DiffScope.PullRequest && page != null && store.pullRequestsExtended && page.viewed.isNotEmpty()) {
                val viewed = page.viewed.values.count { it }
                Text(
                    "$viewed of ${page.viewed.size} viewed",
                    Modifier.padding(start = 6.dp).semantics { if (page.canReviewLines) contentDescription = "Tap a line's number to comment on it" },
                    size = 12f, color = LocalPalette.current.mutedStrongerForeground, maxLines = 1,
                )
            }
            Spacer(Modifier.weight(1f))
            if (document != null && document.files.size > 1) {
                val allClosed = content.collapsed.size >= document.files.size
                ActionButton(
                    icon = if (allClosed) Symbol.UnfoldVertical else Symbol.FoldVertical,
                    help = if (allClosed) "Open every file" else "Close every file",
                ) { content.setAllCollapsed(!allClosed) }
            }
            ActionButton(icon = Symbol.RotateCw, help = "Read the changes again", pending = PanelTab.Diff in content.reading) { asked += 1 }
        }
        when {
            !target.repository -> NoRepositoryMessage(target.projectID)
            target.awaitsWorktree -> PanelMessage("Nothing has changed.")
            else -> Changes(target, scope, page) { commenting = it }
        }
    }
    LaunchedEffect(target, scope, store.workspaceVersion, asked) {
        if (!target.repository || target.awaitsWorktree) return@LaunchedEffect
        // A pull request's diff shows which files were viewed and where its conversations are.
        if (scope is DiffScope.PullRequest && store.pullRequestsExtended && panel.pullRequests.page.value?.number != scope.number) {
            panel.pullRequests.loadPullRequest(target, scope.number)
        }
        content.loadDiff(target, scope)
    }
    Modal(commenting, onDismiss = { commenting = null }) { line ->
        page?.let { LineCommentSheet(target, it, line) }
    }
}

/** The pull request the diff shows, when it shows one or one of its commits, as far as it has been read. */
private fun shownPage(page: PullRequestPage?, scope: DiffScope): PullRequestPage? = when (scope) {
    is DiffScope.PullRequest -> page?.takeIf { it.number == scope.number }
    is DiffScope.Commit -> page?.takeIf { shown -> shown.activity.any { entry -> entry.commits.any { it.sha == scope.sha } } }
    else -> null
}

@Composable
private fun Changes(target: PanelTarget, scope: DiffScope, page: PullRequestPage?, comment: (CommentableLine) -> Unit) {
    val store = LocalStore.current
    val panel = store.rightPanel
    val content = panel.content
    when (val diff = content.diff) {
        Loaded.Loading -> PanelLoading()
        is Loaded.Failed -> PanelMessage(diff.message, failed = true)
        is Loaded.Ready -> {
            val document = diff.value
            if (document.files.isEmpty()) {
                PanelMessage(if (scope == DiffScope.Uncommitted) "Everything is committed." else "Nothing has changed.")
                return
            }
            if (document.truncated) PanelNote("These changes are too long to show in full. This is their start.")
            val pending = (scope as? DiffScope.PullRequest)?.let { panel.pullRequests.pendingComments[it.number] } ?: emptyList()
            val marks = remember(document, scope, page, pending, store.pullRequestsExtended) {
                marks(document, scope, page.takeIf { store.pullRequestsExtended }, pending)
            }
            CodeView(
                document,
                collapsed = content.collapsed,
                reveal = panel.reveal,
                marks = marks,
                onToggle = content::toggleCollapsed,
                onOpenFile = { panel.open(PanelTab.File(it)) },
                onViewed = { path ->
                    if (scope !is DiffScope.PullRequest) return@CodeView
                    content.setViewed(path, path !in marks.viewed, target, scope.number)
                },
                onComment = { place -> document.files.getOrNull(place.file)?.commentable(place.line)?.let(comment) },
                onMedia = { content.loadMedia(it, target) },
                onOpenMedia = content::viewMedia,
            )
        }
    }
}

/** What the diff shows of the pull request besides its lines. */
private fun marks(document: CodeDocument, scope: DiffScope, page: PullRequestPage?, pending: List<PendingLineComment>): CodeMarks {
    if (scope !is DiffScope.PullRequest || page == null) return CodeMarks()
    val places = page.threads.mapNotNull { thread -> thread.line?.let { Triple(thread.path, it, thread.side) } } +
        pending.map { Triple(it.path, it.line, it.side) }
    val marked = mutableMapOf<String, Set<Int>>()
    for ((path, line, side) in places) {
        val file = document.files.firstOrNull { it.path == path } ?: continue
        val index = file.lines.indices.firstOrNull { index ->
            val removed = file.kind(index) == CodeFile.Kind.Removed
            if (side == "left") removed && file.old[index] == line else !removed && file.new[index] == line
        } ?: continue
        marked[path] = (marked[path] ?: emptySet()) + index
    }
    return CodeMarks(viewable = true, viewed = page.viewed.filterValues { it }.keys, commentable = page.canReviewLines, marked = marked)
}

/** Which changes the diff shows. */
@Composable
private fun ScopeMenu(target: PanelTarget, scope: DiffScope) {
    val store = LocalStore.current
    val panel = store.rightPanel
    ActionMenu(title(scope, panel), modifier = Modifier.outdent(8.dp)) {
        toggle("Uncommitted changes", scope == DiffScope.Uncommitted) { panel.choose(DiffScope.Uncommitted) }
        toggle("Branch changes", scope == DiffScope.Branch) { panel.choose(DiffScope.Branch) }
        // The pull request the menu offers: the thread's, or the one the panel shows.
        val number = (scope as? DiffScope.PullRequest)?.number ?: target.pullRequest ?: panel.pullRequests.page.value?.number
        if (number != null && store.pullRequestsUnavailable == null) {
            toggle("Pull request #$number", scope == DiffScope.PullRequest(number)) { panel.choose(DiffScope.PullRequest(number)) }
            val page = panel.pullRequests.page.value
            if (page != null && page.number == number) {
                val commits = page.activity.flatMap { it.commits }.filter { it.sha.isNotEmpty() }
                if (commits.size > 1 || scope == DiffScope.Commit(commits.firstOrNull()?.sha ?: "")) {
                    submenu("Its Commits") {
                        for (commit in commits) {
                            toggle("${commit.oid}  ${commit.headline}", scope == DiffScope.Commit(commit.sha)) { panel.choose(DiffScope.Commit(commit.sha)) }
                        }
                    }
                }
            }
        }
        if (panel.turns.isNotEmpty()) {
            divider()
            for (turn in panel.turns.reversed()) {
                val name = panel.nameOfTurn(turn.id)
                val files = if (turn.files == 1) "1 file" else "${turn.files} files"
                toggle("$name · $files", scope == DiffScope.Turn(turn.id)) { panel.choose(DiffScope.Turn(turn.id)) }
            }
        }
    }
}

private fun title(scope: DiffScope, panel: RightPanel): String = when (scope) {
    DiffScope.Uncommitted -> "Uncommitted"
    DiffScope.Branch -> "Branch"
    is DiffScope.PullRequest -> "PR #${scope.number}"
    is DiffScope.Commit -> "Commit ${scope.sha.take(7)}"
    is DiffScope.Turn -> panel.nameOfTurn(scope.itemID)
}

/** Stands `by` further out at its start than the room it is laid out in, as a ghost control at the start of a bar does. */
fun Modifier.outdent(by: Dp): Modifier = layout { measurable, constraints ->
    val shift = by.roundToPx()
    val placeable = measurable.measure(constraints)
    layout(maxOf(0, placeable.width - shift), placeable.height) { placeable.place(-shift, 0) }
}
