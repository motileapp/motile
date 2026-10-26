package app.motile.composer

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Branch
import app.motile.core.Project
import app.motile.core.coreError
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.iconSize
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.theme.opacity
import app.motile.theme.value
import app.motile.theme.LocalDark
import app.motile.ui.HighlightButton
import app.motile.ui.Highlight
import app.motile.ui.Icon
import app.motile.ui.InputField
import app.motile.ui.Text
import app.motile.ui.ThemeDivider

/**
 * The branches of the project's repository, to switch to one or make a new one. What is typed
 * narrows the list, and becomes the name of a branch to make when it matches none. With `base` it
 * picks the branch a new worktree starts from instead, and switches nothing. A page of the thread's
 * settings, which draw its bar: the search field is at its bottom.
 */
object BranchPicker {
    val rowHeight = if (Platform.scale > 1) 44.dp else 30.dp
    val listPadding = 8.dp
    const val placeholderCount = 5
}

private sealed interface BranchChoice {
    val id: String

    data class Existing(val branch: Branch) : BranchChoice {
        override val id get() = "branch:${branch.name}"
    }

    data class Create(val name: String) : BranchChoice {
        override val id get() = "create:$name"
    }
}

@Composable
fun BranchPicker(project: Project, base: String? = null) {
    val store = LocalStore.current
    val palette = Theme.colors
    var query by remember { mutableStateOf("") }
    var problem by remember { mutableStateOf<String?>(null) }
    var switching by remember { mutableStateOf(false) }
    val highlighted = remember { Highlight() }
    val working = base == null && store.isWorking(project)
    val prompt = if (base == null) "Switch or create a branch" else "Start from a branch"
    val listed = store.listedBranches
    val branches = listed?.getOrNull()
    val choices = remember(branches, query, base) {
        val all = branches ?: return@remember emptyList()
        val needle = query.trim()
        val matching = all.filter { needle.isEmpty() || it.name.contains(needle, ignoreCase = true) }
        val made = matching.map<Branch, BranchChoice> { BranchChoice.Existing(it) }
        if (base == null && needle.isNotEmpty() && all.none { it.name == needle }) made + BranchChoice.Create(needle) else made
    }
    LaunchedEffect(query) { highlighted.index = 0 }

    fun choose(index: Int) {
        if (switching || working || index !in choices.indices) return
        val (name, create) = when (val choice = choices[index]) {
            is BranchChoice.Existing -> choice.branch.name to false
            is BranchChoice.Create -> choice.name to true
        }
        if (base != null) {
            store.setDraftBase(name)
            store.showsBranches = false
            return
        }
        if (!create && branches?.firstOrNull { it.name == name }?.current == true) {
            store.showsBranches = false
            return
        }
        switching = true
        problem = null
        store.switchBranch(project, name, create) { failure ->
            switching = false
            if (failure == null) {
                store.showsBranches = false
                return@switchBranch
            }
            problem = failure
        }
    }

    CompositionLocalProvider(LocalSurface provides Surface.Background) {
        Column(Modifier.fillMaxSize().background(palette.background)) {
            Box(Modifier.weight(1f).fillMaxWidth()) {
                when {
                    listed == null -> Column(Modifier.padding(BranchPicker.listPadding)) {
                        repeat(BranchPicker.placeholderCount) { BranchPlaceholder(it) }
                    }
                    listed.isFailure -> Text(
                        listed.coreError?.message ?: "Couldn't list the branches.", Modifier.fillMaxWidth().padding(16.dp),
                        size = 12.5f, color = palette.destructive,
                    )
                    choices.isEmpty() -> Text("No branch matches.", Modifier.fillMaxWidth().padding(16.dp), size = 12.5f, color = palette.mutedForeground)
                    else -> BranchList(choices, base, working, highlighted, ::choose)
                }
            }
            val note = if (working) "An agent is working in this project. Switch when it has finished." else problem
            if (note != null) {
                ThemeDivider()
                Text(
                    note, Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), size = 11.5f,
                    color = if (working) palette.mutedForeground else palette.destructive,
                )
            }
            InputField(
                prompt, query, { query = it }, Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                icon = Symbol.Search, clearable = true,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, imeAction = ImeAction.Search),
                onSubmit = { choose(highlighted.index) },
            )
        }
    }
}

@Composable
private fun BranchList(choices: List<BranchChoice>, base: String?, working: Boolean, highlighted: Highlight, choose: (Int) -> Unit) {
    val list = rememberLazyListState()
    LaunchedEffect(highlighted.index) {
        val index = highlighted.index
        if (index !in choices.indices) return@LaunchedEffect
        val shown = list.layoutInfo.visibleItemsInfo.map { it.index }
        if (index !in shown) list.scrollToItem(index)
    }
    LazyColumn(
        Modifier.fillMaxSize(),
        state = list,
        contentPadding = PaddingValues(start = BranchPicker.listPadding, end = BranchPicker.listPadding, top = BranchPicker.listPadding, bottom = BranchPicker.listPadding + 24.dp),
    ) {
        itemsIndexed(choices, key = { _, choice -> choice.id }) { index, choice ->
            HighlightButton({ choose(index) }, Modifier.fillMaxWidth(), radius = Radius.md, lit = index == highlighted.index) {
                BranchRow(choice, base, working)
            }
        }
    }
}

@Composable
private fun BranchRow(choice: BranchChoice, base: String?, working: Boolean) {
    val palette = Theme.colors
    Row(
        Modifier.fillMaxWidth().height(BranchPicker.rowHeight).padding(horizontal = 8.dp).opacity(Opacity.Disabled, on = working),
        horizontalArrangement = Arrangement.spacedBy(7.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        when (choice) {
            is BranchChoice.Existing -> {
                val branch = choice.branch
                val chosen = if (base != null) base == branch.name else branch.current
                Icon(if (chosen) Symbol.Check else Symbol.GitBranch, iconSize(text = 12.5f), color = if (chosen) palette.foreground else palette.mutedStrongerForeground)
                Text(branch.name, Modifier.weight(1f), size = 12.5f, color = palette.foreground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
                val tag = when {
                    base != null && branch.current -> "current"
                    branch.isDefault -> "default"
                    branch.remote -> "remote"
                    else -> null
                }
                if (tag != null) Text(tag, size = 11f, color = palette.mutedStrongerForeground)
            }
            is BranchChoice.Create -> {
                Icon(Symbol.Plus, iconSize(text = 12.5f), color = palette.mutedStrongerForeground)
                Text("Create branch “${choice.name}”", Modifier.weight(1f), size = 12.5f, color = palette.foreground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
            }
        }
    }
}

/** A branch's row before the branches are listed: bars where its icon and name will be. */
@Composable
private fun BranchPlaceholder(index: Int) {
    val fill = LocalSurface.current.color(Surface.Layer.Control)
    val faded = Opacity.Disabled.value(LocalDark.current)
    val widths = listOf(90, 150, 120, 170, 105)
    val pulse = rememberInfiniteTransition(label = "placeholder")
    val alpha by pulse.animateFloat(1f, faded, infiniteRepeatable(tween(800), RepeatMode.Reverse), label = "fade")
    Row(
        Modifier.fillMaxWidth().height(BranchPicker.rowHeight).padding(horizontal = 8.dp).alpha(alpha),
        horizontalArrangement = Arrangement.spacedBy(7.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(14.dp), contentAlignment = Alignment.Center) {
            Box(Modifier.size(11.dp).background(fill, RoundedCornerShape(Radius.xs)))
        }
        Box(Modifier.width(widths[index % widths.size].dp).height(8.dp).background(fill, CircleShape))
    }
}
