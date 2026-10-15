package app.motile.panel

import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Time
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.Radius
import app.motile.theme.SurfaceBox
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.ActionMenu
import app.motile.ui.Answer
import app.motile.ui.Asking
import app.motile.ui.ButtonVariant
import app.motile.ui.Chip
import app.motile.ui.Confirmation
import app.motile.ui.HighlightButton
import app.motile.ui.HorizontalEdge
import app.motile.ui.Icon
import app.motile.ui.InputField
import app.motile.ui.Modal
import app.motile.ui.Text
import app.motile.ui.color
import app.motile.ui.TextArea
import app.motile.ui.onEscape
import app.motile.ui.pressable
import kotlinx.coroutines.delay

/**
 * A pull request of the repository: the one of the branch the thread works on, or another one by
 * its number. Where it stands, what holds it up, the button its state calls for, what was said on
 * it, and a box to say more.
 */
@Composable
fun PullRequestSurface(target: PanelTarget, number: Int? = null) {
    val store = LocalStore.current
    val state = store.rightPanel.pullRequests
    var asked by remember { mutableIntStateOf(0) }
    val shown = number ?: target.pullRequest
    Column(Modifier.fillMaxSize()) {
        PanelBar { PullRequestBar(state.page.value?.takeIf { it.number == shown }, target, shown) { asked += 1 } }
        state.notice?.let { PullRequestNoticeBar(it) }
        val reason = store.pullRequestsUnavailable
        when {
            reason != null -> PanelMessage(reason)
            shown != null -> {
                LaunchedEffect(target, shown, store.workspaceVersion, asked) { state.loadPullRequest(target, shown) }
                LaunchedEffect(state.reads) {
                    // While GitHub is still working something out, it is asked again.
                    if (state.page.value?.settling != true) return@LaunchedEffect
                    delay(10_000)
                    state.loadPullRequest(target, shown)
                }
                val page = state.page
                when {
                    page is app.motile.core.Loaded.Ready && page.value.number == shown -> PullRequestPageView(page.value, target, shown)
                    page is app.motile.core.Loaded.Failed -> PanelMessage(page.message, failed = true)
                    else -> PanelLoading()
                }
            }
            else -> NoPullRequest(target)
        }
    }
}

@Composable
private fun androidx.compose.foundation.layout.RowScope.PullRequestBar(page: PullRequestPage?, target: PanelTarget, shown: Int?, ask: () -> Unit) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    if (page != null) {
        Icon(page.state.symbol, 12f, color = page.state.color(palette))
        Text("#${page.number}", Modifier.padding(start = 2.dp), style = Theme.uiDigits(12.5f, FontWeight.Medium), color = palette.foreground)
        if (page.number == target.pullRequest && store.selectedThread?.watching == true) {
            Icon(
                Symbol.Eye, 12f,
                Modifier.padding(start = 4.dp).semantics { contentDescription = "The agent hears when its checks finish, someone comments or it conflicts" },
                color = palette.primary,
            )
        }
    } else {
        Text("Pull Request", size = 12.5f, weight = FontWeight.Medium, color = palette.foreground)
    }
    Spacer(Modifier.weight(1f).widthIn(min = 4.dp))
    if (page != null && page.url.isNotEmpty()) {
        ActionButton(icon = Symbol.SquareArrowOutUpRight, help = "Open on GitHub") { Platform.open(page.url) }
    }
    if (shown != null && store.pullRequestsUnavailable == null) {
        ActionButton(
            icon = Symbol.RotateCw, help = "Read the pull request again",
            pending = PanelTab.PullRequest in store.rightPanel.pullRequests.reading, onClick = ask,
        )
    }
}

/** The thread's tab while its branch has no pull request: the way to open one, or to link one. */
@Composable
private fun NoPullRequest(target: PanelTarget) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    var linking by rememberSaveable(target.key) { mutableStateOf("") }
    Column(
        Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Icon(Symbol.GitPullRequest, 22f, color = palette.mutedStrongerForeground)
        Text("This branch has no pull request yet.", size = 13f, color = palette.mutedForeground)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            val project = store.gitProject
            val create = project?.gitControl?.menu?.firstOrNull { it.action == "create_pr" }
            if (project != null && create != null && create.reason == null) {
                ActionButton("Create PR", variant = ButtonVariant.Primary) { store.chooseGit(create, project) }
            }
            if (store.pullRequestsExtended) {
                ActionButton("Show All Pull Requests") { store.rightPanel.open(PanelTab.PullRequests) }
            }
        }
        val thread = store.selectedThread
        if (store.pullRequestsExtended && thread != null) {
            LinkPullRequestField(linking, { linking = it }, Modifier.widthIn(max = 300.dp)) { number ->
                store.rightPanel.pullRequests.link(number, thread.id, thread.serverID)
            }
        }
    }
}

/** A field that takes a pull request's number or address and links it to the thread. */
@Composable
fun LinkPullRequestField(text: String, onTextChange: (String) -> Unit, modifier: Modifier = Modifier, link: (Int) -> Unit) {
    // The number in "#12", "12" or "https://github.com/acme/app/pull/12".
    val number = text.trim().split('/').last().removePrefix("#").toIntOrNull()
    val submit = {
        if (number != null) {
            link(number)
            onTextChange("")
        }
    }
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
        InputField(
            "Link a PR by number or address", text, onTextChange, Modifier.weight(1f), onSubmit = submit,
        )
        ActionButton("Link", disabled = number == null, onClick = submit)
    }
}

/** What the tab can do to the pull request, for the parts of the page that do it. */
class PullRequestActions(
    val react: (subject: String, kind: String, on: Boolean) -> Unit,
    val reply: (thread: String, body: String, done: () -> Unit) -> Unit,
    val resolve: (thread: String, resolved: Boolean) -> Unit,
    val handOff: (String) -> Unit,
    val showCommit: (String) -> Unit,
    val editDescription: (() -> Unit)?,
    /** The action that runs. */
    val working: PullRequestWork?,
)

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun PullRequestPageView(page: PullRequestPage, target: PanelTarget, number: Int) {
    val store = LocalStore.current
    val state = store.rightPanel.pullRequests
    val extended = store.pullRequestsExtended
    var confirming by remember { mutableStateOf<PullRequestButton?>(null) }
    var comment by rememberSaveable(number) { mutableStateOf("") }
    var title by remember { mutableStateOf<String?>(null) }
    var describing by remember { mutableStateOf(false) }

    fun perform(button: PullRequestButton) {
        val action = button.action ?: return
        state.act(action, button.method, key = button.key, target = target, number = number)
    }

    // A button's prompt goes to the composer; its action runs, after asking when it says to.
    fun run(button: PullRequestButton, fromMenu: Boolean = false) {
        button.prompt?.let { return store.handOff(it) }
        val chosen = if (fromMenu) button.inMenu else button
        if (button.confirm != null) {
            confirming = chosen
            return
        }
        perform(chosen)
    }

    val actions = PullRequestActions(
        react = { subject, kind, on -> state.edit(mapOf("kind" to "react", "subject" to subject, "reaction" to kind, "on" to on), target = target, number = number) },
        reply = { thread, body, done ->
            state.edit(mapOf("kind" to "reply", "thread" to thread, "body" to body), "reply:$thread", target, number, done)
        },
        resolve = { thread, resolved ->
            state.edit(mapOf("kind" to "resolve", "thread" to thread, "resolved" to resolved), "resolve:$thread", target, number)
        },
        handOff = { store.handOff(it) },
        showCommit = { store.rightPanel.showDiff(DiffScope.Commit(it)) },
        editDescription = if (extended && page.canEdit) ({ describing = true }) else null,
        working = state.working,
    )

    LazyColumn(
        Modifier.fillMaxSize(),
        state = rememberLazyListState(),
        contentPadding = PaddingValues(start = 14.dp, end = 14.dp, top = 14.dp, bottom = panelEndRoom),
    ) {
        item("header") {
            Header(page, target, number, extended, title, { title = it }, { describing = true }) { button -> run(button, fromMenu = true) }
        }
        val stack = page.stack
        if (extended && stack != null && stack.layers.size > 1) {
            item("stack") { StackCard(stack, Modifier.padding(top = 16.dp)) { state.showPullRequest(it, target) } }
        }
        item("merge") { MergeBox(page, target, state.working, Modifier.padding(top = 16.dp)) { run(it) } }
        item("activity") { ActivityTitle(Modifier.padding(top = 16.dp)) }
        items(page.activity, key = { "entry:${it.id}" }) { entry ->
            ActivityRow(entry, actions, Modifier.padding(top = if (entry.id == 0) 16.dp else 20.dp))
        }
        if (page.state != app.motile.core.PullRequest.State.Merged) {
            item("comment") {
                CommentBox(page, target, number, comment, { comment = it }, Modifier.padding(top = if (page.activity.isEmpty()) 16.dp else 20.dp))
            }
        }
    }
    Asking(confirming, { confirming = null }) { button ->
        val confirm = button.confirm
        Confirmation(
            title = confirm?.title ?: "", message = confirm?.message ?: "",
            answers = listOf(Answer(confirm?.button ?: button.label, if (button.style == "danger") ButtonVariant.Danger else ButtonVariant.Primary) { perform(button) }),
        )
    }
    Modal(describing, { describing = false }) {
        DescriptionEditor(page.body) { body ->
            state.edit(mapOf("kind" to "body", "body" to body), "menu:body", target, number)
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Header(
    page: PullRequestPage,
    target: PanelTarget,
    number: Int,
    extended: Boolean,
    title: String?,
    setTitle: (String?) -> Unit,
    describe: () -> Unit,
    run: (PullRequestButton) -> Unit,
) {
    val store = LocalStore.current
    val panel = store.rightPanel
    val palette = LocalPalette.current
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (title != null) {
                TitleEditor(page, target, number, title, setTitle)
            } else {
                SelectionContainer(Modifier.weight(1f)) {
                    Text(page.title, size = 15f, weight = FontWeight.SemiBold, color = palette.foreground)
                }
                MoreMenu(page, target, number, extended, Modifier.offset(y = (-4).dp), { setTitle(page.title) }, describe, run)
            }
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
            Chip(page.state.title, icon = page.state.symbol, tone = page.state.color(palette))
            Text(page.byline, size = 12f, color = palette.mutedForeground)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            BranchName(page.base, Modifier.weight(1f, fill = false))
            Icon(Symbol.ArrowLeft, 10f, color = palette.mutedStrongerForeground)
            BranchName(page.head, Modifier.weight(1f, fill = false))
            Spacer(Modifier.width(2.dp))
            HighlightButton(
                onClick = { panel.showDiff(DiffScope.PullRequest(page.number)) },
                modifier = Modifier.semantics { contentDescription = "Show what it changes" },
                radius = Radius.sm,
            ) {
                Row(
                    Modifier.height(pressable(24f)).padding(horizontal = 6.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Symbol.Diff, 11f, color = palette.mutedForeground)
                    Text(if (page.files == 1) "1 file" else "${page.files} files", size = 12f, color = palette.foreground, maxLines = 1)
                    if (page.additions + page.deletions > 0) Text(LineCountText.text(page.additions, page.deletions), style = LineCountText.style, maxLines = 1)
                }
            }
        }
        val below = page.stackedOn
        if (extended && below != null) {
            HighlightButton(
                onClick = { panel.pullRequests.showPullRequest(below.number, target) },
                modifier = Modifier.offset(x = (-6).dp).semantics { contentDescription = "Open the pull request it merges into" },
                radius = Radius.sm,
            ) {
                Row(
                    Modifier.height(pressable(24f)).padding(horizontal = 6.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Symbol.Layers, 11f, color = palette.mutedForeground)
                    val words = buildAnnotatedString {
                        withStyle(SpanStyle(color = palette.mutedForeground)) { append("Stacked on ") }
                        withStyle(SpanStyle(color = below.state.color(palette))) { append("#${below.number} ") }
                        withStyle(SpanStyle(color = palette.foreground)) { append(below.title) }
                    }
                    Text(words, size = 12f, maxLines = 1)
                }
            }
        }
        if (extended) People(page, target, number)
    }
}

@Composable
private fun androidx.compose.foundation.layout.RowScope.TitleEditor(page: PullRequestPage, target: PanelTarget, number: Int, editing: String, setTitle: (String?) -> Unit) {
    val state = LocalStore.current.rightPanel.pullRequests
    val working = state.working?.key == "title"
    val focus = remember { FocusRequester() }
    val save = {
        val edited = editing.trim()
        if (edited.isNotEmpty() && edited != page.title) {
            state.edit(mapOf("kind" to "title", "title" to edited), "title", target, number) { setTitle(null) }
        }
    }
    LaunchedEffect(Unit) { focus.requestFocus() }
    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        InputField(
            "Title", editing, { setTitle(it) }, Modifier.fillMaxWidth().onEscape { setTitle(null) },
            size = ControlSize.Large, focusRequester = focus, onSubmit = save,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Spacer(Modifier.weight(1f))
            ActionButton("Cancel", disabled = working) { setTitle(null) }
            ActionButton(
                "Save", variant = ButtonVariant.Primary, pending = working,
                disabled = working || editing.isBlank() || editing == page.title, onClick = save,
            )
        }
    }
}

/** Who reviews it and its labels, each with a menu to change them when the user may. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun People(page: PullRequestPage, target: PanelTarget, number: Int) {
    val state = LocalStore.current.rightPanel.pullRequests
    val working = state.working
    val reviews = page.reviewers.isNotEmpty() || page.reviewerChoices.isNotEmpty()
    val labelled = page.labels.isNotEmpty() || page.labelChoices.isNotEmpty()
    if (!reviews && !labelled) return
    Column(Modifier.padding(top = 2.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if (reviews) {
            PeopleRow(
                "Reviewers", if (page.reviewers.isEmpty()) "None yet" else null, page.reviewerChoices, "Ask for a review",
                working?.key == "menu:reviewers", chips = { page.reviewers.forEach { ReviewerChip(it) } },
            ) { name, on ->
                val edit = mapOf("kind" to "reviewers", "add" to if (on) listOf(name) else emptyList(), "remove" to if (on) emptyList() else listOf(name))
                state.edit(edit, "menu:reviewers", target, number)
            }
        }
        if (labelled) {
            PeopleRow(
                "Labels", if (page.labels.isEmpty()) "None yet" else null, page.labelChoices, "Change the labels",
                working?.key == "menu:labels", chips = { page.labels.forEach { Chip(it.name, dot = it.color, tone = it.color) } },
            ) { name, on ->
                val edit = mapOf("kind" to "labels", "add" to if (on) listOf(name) else emptyList(), "remove" to if (on) emptyList() else listOf(name))
                state.edit(edit, "menu:labels", target, number)
            }
        }
    }
}

/** A row of who reviews or what labels, with a menu of the choices when there are any. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun PeopleRow(
    title: String,
    /** Said in place of the chips when there are none. */
    empty: String?,
    choices: List<PullRequestPage.Toggle>,
    help: String,
    pending: Boolean,
    chips: @Composable () -> Unit,
    toggle: (String, Boolean) -> Unit,
) {
    val palette = LocalPalette.current
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(title, Modifier.width(70.dp), size = 12f, color = palette.mutedStrongerForeground)
        if (empty != null) {
            Text(empty, size = 12f, color = palette.mutedStrongerForeground)
        } else {
            FlowRow(Modifier.weight(1f, fill = false), horizontalArrangement = Arrangement.spacedBy(5.dp), verticalArrangement = Arrangement.spacedBy(5.dp)) { chips() }
        }
        if (choices.isNotEmpty()) {
            ActionMenu(icon = Symbol.Plus, help = help, size = ControlSize.Small, pending = pending) {
                for (choice in choices) toggle(choice.name, choice.on) { toggle(choice.name, it) }
            }
        }
    }
}

/** Everything else there is to do, after what the merge box offers. */
@Composable
private fun MoreMenu(
    page: PullRequestPage,
    target: PanelTarget,
    number: Int,
    extended: Boolean,
    modifier: Modifier,
    editTitle: () -> Unit,
    describe: () -> Unit,
    run: (PullRequestButton) -> Unit,
) {
    val store = LocalStore.current
    val panel = store.rightPanel
    val state = panel.pullRequests
    val work = state.working
    val working = work != null
    val thread = store.selectedThread
    val ownPullRequest = thread?.pullRequest?.number == number
    // What a menu started has no button of its own to be pending.
    val pending = work?.let { it.key.startsWith("menu:") && it.key !in listOf("menu:reviewers", "menu:labels") } ?: false
    ActionMenu(icon = Symbol.Ellipsis, modifier = modifier, help = "More", pending = pending) {
        for (button in page.menu) {
            item(button.label, destructive = button.style == "danger", disabled = working && button.prompt == null) { run(button) }
        }
        if (extended) {
            divider()
            if (page.canEdit) {
                item("Edit Title", run = editTitle)
                item("Edit Description", run = describe)
            }
            if (thread != null) {
                if (ownPullRequest) {
                    if (page.watchable) {
                        item(if (thread.watching) "Stop Watching" else "Watch for Changes") { state.watch(!thread.watching, thread.id, thread.serverID) }
                    }
                    item("Unlink from This Thread") { state.link(null, thread.id, thread.serverID) }
                } else {
                    item("Link to This Thread") { state.link(number, thread.id, thread.serverID) }
                }
            }
            item("Show All Pull Requests") { panel.open(PanelTab.PullRequests) }
        }
        divider()
        if (page.url.isNotEmpty()) item("Copy Link") { Platform.copy(page.url) }
        item("Copy Branch Name") { Platform.copy(page.head) }
    }
}

/** Parts the merge box, which is where the pull request stands now, from what happened on it. */
@Composable
private fun ActivityTitle(modifier: Modifier) {
    val palette = LocalPalette.current
    Row(modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.Bottom) {
        Text("Activity", size = 13f, weight = FontWeight.SemiBold, color = palette.foreground)
        Text("Oldest first", size = 12f, color = palette.mutedStrongerForeground)
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun CommentBox(page: PullRequestPage, target: PanelTarget, number: Int, comment: String, setComment: (String) -> Unit, modifier: Modifier) {
    val state = LocalStore.current.rightPanel.pullRequests
    val work = state.working
    val busy = work != null
    val written = comment.trim()
    val pending = state.pendingComments[number] ?: emptyList()
    val choose = { choice: PullRequestChoice, key: String ->
        state.act(choice.action, choice.method, written, key, target, number) { setComment("") }
    }
    Column(modifier.padding(top = 4.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (pending.isNotEmpty()) PendingComments(pending) { state.removePending(it, number) }
        TextArea(
            if (pending.isEmpty()) "Leave a comment" else "Say something with your review (optional)", comment, setComment,
            Modifier.fillMaxWidth(), lines = 4,
        )
        FlowRow(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            val close = page.withComment
            if (close != null && pending.isEmpty()) {
                ActionButton(close.label, pending = work?.key == "close", disabled = busy || written.isEmpty()) { choose(close, "close") }
            }
            for (verdict in page.verdicts) {
                val key = verdict.action
                ActionButton(
                    verdict.label, pending = work?.key == key,
                    disabled = busy || (verdict.action == "request_changes" && written.isEmpty() && pending.isEmpty()),
                ) {
                    if (pending.isEmpty()) choose(verdict, key) else state.review(key, written, key, target, number) { setComment("") }
                }
            }
            ActionButton(
                if (pending.isEmpty()) "Comment" else "Send Review", variant = ButtonVariant.Primary, pending = work?.key == "comment",
                disabled = busy || (written.isEmpty() && pending.isEmpty()),
            ) {
                if (pending.isEmpty()) {
                    state.act("comment", text = written, key = "comment", target = target, number = number) { setComment("") }
                } else {
                    state.review("comment", written, "comment", target, number) { setComment("") }
                }
            }
        }
    }
}

/** The comments on lines that wait for the review, each one to take back. */
@Composable
private fun PendingComments(comments: List<PendingLineComment>, remove: (PendingLineComment) -> Unit) {
    val palette = LocalPalette.current
    SurfaceBox(RoundedCornerShape(Radius.lg), Modifier.fillMaxWidth()) {
        Column(Modifier.padding(bottom = 6.dp)) {
            Text(
                if (comments.size == 1) "1 comment on a line goes with your review" else "${comments.size} comments on lines go with your review",
                Modifier.padding(start = 12.dp, end = 12.dp, top = 9.dp, bottom = 4.dp), size = 12f, weight = FontWeight.Medium, color = palette.mutedForeground,
            )
            for (comment in comments) {
                Row(Modifier.padding(horizontal = 12.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.Top) {
                    Text("${comment.path.substringAfterLast('/')}:${comment.line}", style = Theme.uiMono(11.5f), color = palette.primary)
                    Text(comment.body, Modifier.weight(1f), size = 12.5f, color = palette.foreground, maxLines = 2)
                    ActionButton(icon = Symbol.X, help = "Take it back", size = ControlSize.Small, modifier = Modifier.offset(y = (-4).dp)) { remove(comment) }
                }
            }
        }
    }
}

/** The stack the pull request is in, the top first, each one to open. */
@Composable
private fun StackCard(stack: PullRequestPage.Stack, modifier: Modifier, open: (Int) -> Unit) {
    val palette = LocalPalette.current
    SurfaceBox(RoundedCornerShape(Radius.lg), modifier.fillMaxWidth()) {
        Column(Modifier.padding(bottom = 6.dp)) {
            Row(
                Modifier.padding(start = 12.dp, end = 4.dp, top = 4.dp).heightIn(min = ControlSize.Small.height),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Symbol.Layers, 12f, color = palette.mutedForeground)
                Text("Stack", size = 13f, weight = FontWeight.Medium, color = palette.foreground)
                Text("${stack.layers.size} pull requests onto ${stack.base}", Modifier.weight(1f), size = 12f, color = palette.mutedStrongerForeground, maxLines = 1)
                stack.url?.let { url ->
                    ActionButton("Open on GitHub", help = "A stack merges on GitHub, bottom first", variant = ButtonVariant.Link, size = ControlSize.Small) {
                        Platform.open(url)
                    }
                }
            }
            for (layer in stack.layers.reversed()) {
                HighlightButton(
                    onClick = { open(layer.number) }, modifier = Modifier.fillMaxWidth(), enabled = !layer.current, radius = Radius.sm,
                    inset = PaddingValues(horizontal = 4.dp),
                ) {
                    Row(
                        Modifier.height(pressable(28f)).padding(horizontal = 12.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(Modifier.width(16.dp), contentAlignment = Alignment.Center) { Icon(layer.state.symbol, 11f, color = layer.state.color(palette)) }
                        Text("#${layer.number}", style = Theme.uiDigits(12f, FontWeight.Medium), color = palette.mutedForeground)
                        Text(layer.title, Modifier.weight(1f), size = 12.5f, color = palette.foreground, maxLines = 1)
                        if (layer.current) Text("This one", size = 11.5f, color = palette.mutedStrongerForeground)
                    }
                }
            }
        }
    }
}

/** What stands between the pull request and merging, and the button its state calls for. */
@Composable
private fun MergeBox(page: PullRequestPage, target: PanelTarget, working: PullRequestWork?, modifier: Modifier, run: (PullRequestButton) -> Unit) {
    var showsAllChecks by remember { mutableStateOf(false) }
    SurfaceBox(RoundedCornerShape(Radius.lg), modifier.fillMaxWidth()) {
        Column {
            page.statuses.forEachIndexed { index, status ->
                if (index > 0) PanelLine()
                StatusRow(status, working, run)
                if (status.kind == "checks") Checks(page, showsAllChecks) { showsAllChecks = !showsAllChecks }
            }
            val primary = page.primary
            if (primary != null) {
                PanelLine()
                Row(Modifier.padding(10.dp), verticalAlignment = Alignment.CenterVertically) {
                    val chooses = page.methods.isNotEmpty() && (primary.action == "merge" || primary.action == "enable_auto_merge")
                    val variant = buttonVariant(primary.style)
                    Row(horizontalArrangement = Arrangement.spacedBy(1.dp)) {
                        ActionButton(
                            primary.label, variant = variant, pending = working?.key == primary.key, pendingTitle = primary.pendingLabel,
                            joined = if (chooses) setOf(HorizontalEdge.Trailing) else emptySet(), disabled = working != null,
                        ) { run(primary) }
                        if (chooses) MethodMenu(page, target, variant, working != null)
                    }
                    Spacer(Modifier.weight(1f))
                }
            }
        }
    }
}

/** The merge button lets the way it merges be chosen, when there are several. */
@Composable
private fun MethodMenu(page: PullRequestPage, target: PanelTarget, variant: ButtonVariant, disabled: Boolean) {
    val state = LocalStore.current.rightPanel.pullRequests
    ActionMenu(help = "Choose how it merges", variant = variant, joined = setOf(HorizontalEdge.Leading), disabled = disabled, chevron = true) {
        for (choice in page.methods) {
            toggle(choice.label, choice.method == page.method) {
                val method = choice.method ?: return@toggle
                state.chooseMethod(method, target, page.number)
            }
        }
    }
}

/** The checks that need looking at and the running ones, and the rest when asked for. */
@Composable
private fun Checks(page: PullRequestPage, showsAll: Boolean, toggle: () -> Unit) {
    val settled = page.checks.filter { it.tone == Tone.Success || it.tone == Tone.Neutral }
    val shown = if (showsAll) page.checks else page.checks.filter { it.tone != Tone.Success && it.tone != Tone.Neutral }
    Column(Modifier.padding(bottom = 6.dp)) {
        for (check in shown) CheckRow(check)
        if (settled.isNotEmpty()) {
            val title = if (showsAll) "Hide Finished Checks" else "Show ${settled.size} Finished ${if (settled.size == 1) "Check" else "Checks"}"
            ActionButton(title, variant = ButtonVariant.Link, size = ControlSize.Small, modifier = Modifier.padding(start = 30.dp), onClick = toggle)
        }
    }
}

@Composable
private fun StatusRow(status: PullRequestPage.Status, working: PullRequestWork?, run: (PullRequestButton) -> Unit) {
    val palette = LocalPalette.current
    val symbol = when {
        status.kind == "merged" || status.kind == "auto_merge" -> Symbol.GitMerge
        status.kind == "closed" -> Symbol.GitPullRequestClosed
        status.kind == "draft" -> Symbol.GitPullRequestDraft
        status.kind == "behind" -> Symbol.CircleArrowDown
        status.kind == "review" && status.tone == Tone.Warning -> Symbol.Eye
        status.tone == Tone.Success -> Symbol.CircleCheck
        status.kind == "conflicts" && status.tone == Tone.Danger -> Symbol.TriangleAlert
        status.tone == Tone.Danger -> Symbol.CircleX
        status.tone == Tone.Pending -> Symbol.CircleDashed
        else -> Symbol.CircleAlert
    }
    Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Box(Modifier.size(width = 18.dp, height = scaled(17f).dp), contentAlignment = Alignment.Center) { Icon(symbol, 13f, color = status.tone.color) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(status.title, size = 13f, weight = FontWeight.Medium, color = palette.foreground)
                status.at?.let { Text(Time.ago(it), size = 12f, color = palette.mutedStrongerForeground) }
            }
            status.detail?.let { Text(it, size = 12f, color = palette.mutedForeground) }
            if (status.buttons.isNotEmpty()) {
                Row(Modifier.padding(top = 5.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    for (button in status.buttons) {
                        ActionButton(button.label, variant = buttonVariant(button.style), pending = working?.key == button.key, disabled = working != null) {
                            run(button)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun CheckRow(check: PullRequestPage.Check) {
    val store = LocalStore.current
    val palette = LocalPalette.current
    Row(
        Modifier.fillMaxWidth().heightIn(min = pressable(26f)).padding(start = 12.dp, end = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(18.dp), contentAlignment = Alignment.Center) { Box(Modifier.size(7.dp).background(check.tone.color, CircleShape)) }
        Text(
            check.name, Modifier.weight(1f, fill = false).semantics { contentDescription = check.description ?: check.name },
            size = 12.5f, color = palette.foreground, maxLines = 1, overflow = TextOverflow.Ellipsis,
        )
        check.workflow?.let { Text(it, Modifier.weight(1f, fill = false), size = 12f, color = palette.mutedStrongerForeground, maxLines = 1) }
        Spacer(Modifier.weight(0.001f).widthIn(min = 6.dp))
        Text(check.label, size = 12f, color = if (check.tone == Tone.Neutral) palette.mutedStrongerForeground else check.tone.color, maxLines = 1)
        check.fix?.let { fix ->
            ActionButton("Fix", help = "Have the agent fix it", variant = ButtonVariant.Link, size = ControlSize.Small) { store.handOff(fix) }
        }
        check.url?.let { url ->
            ActionButton(icon = Symbol.SquareArrowOutUpRight, help = "Show its details", size = ControlSize.Small) { Platform.open(url) }
        }
    }
}
