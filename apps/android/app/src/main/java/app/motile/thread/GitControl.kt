package app.motile.thread

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import app.motile.LocalStore
import app.motile.core.GitControl
import app.motile.core.GitQuick
import app.motile.core.GitSymbol
import app.motile.core.AppStore
import app.motile.core.Project
import app.motile.core.PullRequest
import app.motile.platform.Symbol
import app.motile.theme.Palette
import app.motile.theme.Theme
import app.motile.ui.Answer
import app.motile.ui.Asking
import app.motile.ui.ButtonVariant
import app.motile.ui.Confirmation
import app.motile.ui.GlassButton
import app.motile.ui.Icon
import app.motile.ui.MenuBuilder
import app.motile.ui.MenuPopup
import app.motile.ui.Spinner
import app.motile.ui.color
import app.motile.ui.rememberMenuState

private const val SYMBOL_SIZE = 15f

/**
 * The git button in a thread's top bar: one symbol that opens the menu of every action, under the
 * one the repository calls for. Its symbol is that action's, or its pull request's in the colour of
 * its state. An action that can't run now is greyed, with why under its name. It asks before an
 * action pushes from the default branch.
 */
@Composable
fun GitButton(project: Project, control: GitControl, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val palette = Theme.colors
    val stage = store.gitStage(project)
    val running = stage != null
    val quick = control.quick
    val acts = quick.action != null || quick.url != null
    val showsQuick = running || (acts && control.menu.none { it.label == quick.label })
    val color = pullRequestColor(quick, palette)
    BarMenu(
        modifier = modifier,
        items = {
            if (showsQuick) {
                section {
                    when {
                        stage != null -> item(stage.label, Symbol.Hourglass) { store.runQuick(project) }
                        color != null -> item(quick.title, picture = { Icon(symbol(quick), SYMBOL_SIZE, color = color) }) { store.runQuick(project) }
                        else -> item(quick.title, symbol(quick)) { store.runQuick(project) }
                    }
                }
            }
            for (item in control.menu) {
                item(item.label, GitSymbol.symbol(item.action), detail = item.reason, disabled = item.reason != null || running) {
                    store.chooseGit(item, project)
                }
            }
            control.warning?.let { warning -> section { note(warning) } }
        },
    ) {
        when {
            running -> Spinner(SYMBOL_SIZE)
            color != null -> Icon(symbol(quick), SYMBOL_SIZE, color = color)
            acts -> Icon(symbol(quick), SYMBOL_SIZE)
            else -> Icon(Symbol.GitBranch, SYMBOL_SIZE)
        }
    }
    Asking(store.pendingGit, onDismiss = { store.pendingGit = null }) { pending ->
        Confirmation(
            title = pending.confirm.title, message = pending.confirm.description,
            answers = listOf(
                Answer(pending.confirm.branchOff, ButtonVariant.Secondary) { store.confirmGit(pending, onNewBranch = true) },
                Answer(pending.confirm.proceed) { store.confirmGit(pending, onNewBranch = false) },
            ),
        )
    }
}

private fun symbol(quick: GitQuick): Symbol {
    if (quick.url == null) return GitSymbol.symbol(quick.action)
    return quick.pullRequest?.state?.symbol ?: Symbol.GitPullRequest
}

/** A pull request the button opens is in the colour of its state; a draft's is the foreground. */
private fun pullRequestColor(quick: GitQuick, palette: Palette): Color? {
    if (quick.url == null) return null
    val state = quick.pullRequest?.state ?: return null
    return if (state == PullRequest.State.Draft) palette.foreground else state.color(palette)
}

private fun AppStore.runQuick(project: Project) {
    if (gitStage(project) != null) return
    runQuickGit(project)
}

/** What stands in for the git button in a folder that is no git repository: it makes it one. */
@Composable
fun InitializeGitButton(project: Project, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val pending = project.id in store.initializingGit
    BarMenu(modifier, items = { item("Initialize git", Symbol.GitBranch, disabled = pending) { store.initializeGit(project) } }) {
        if (pending) Spinner(SYMBOL_SIZE) else Icon(Symbol.GitBranch, SYMBOL_SIZE)
    }
}

/**
 * What stands in for the git button in a draft that starts a new worktree, while the remote's base
 * has commits the local one lacks: it brings the local one up to date.
 */
@Composable
fun PullBaseButton(base: String, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val pending = store.pullingDraftBase
    val symbol = GitSymbol.symbol("pull")
    BarMenu(modifier, items = { item("Pull $base", symbol, disabled = pending) { store.pullDraftBase() } }) {
        if (pending) Spinner(SYMBOL_SIZE) else Icon(symbol, SYMBOL_SIZE)
    }
}

/** A button of the top bar's glass group, labelled "Git", that opens a menu under it. */
@Composable
private fun BarMenu(modifier: Modifier = Modifier, items: MenuBuilder.() -> Unit, label: @Composable () -> Unit) {
    val menu = rememberMenuState()
    Box(modifier) {
        GlassButton(help = "Git", onClick = { menu.open() }, bare = true, content = label)
        MenuPopup(menu, items)
    }
}
