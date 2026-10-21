package app.motile.thread

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.composer.AgentBar
import app.motile.composer.ComposerView
import app.motile.core.AppStore
import app.motile.core.Notice
import app.motile.core.PanelPage
import app.motile.core.Project
import app.motile.core.Selection
import app.motile.core.Server
import app.motile.layout.LocalDrawer
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.sidebar.attentionColor
import app.motile.theme.ControlSize
import app.motile.theme.Palette
import app.motile.theme.Radius
import app.motile.theme.Theme
import app.motile.theme.symbolSide
import app.motile.transcript.TranscriptView
import app.motile.ui.FileDrop
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.DimButton
import app.motile.ui.GlassButton
import app.motile.ui.Icon
import app.motile.ui.ProjectIcon
import app.motile.ui.Text
import app.motile.ui.glassButton

private val composerBottomGap = 8.dp
private val barPadding = 12.dp
private val barHeight = GlassButton.side + 10.dp

/**
 * The open thread with the composer over its end, or the start of a new one, under a top bar with
 * the thread's name, its git button and the way to its files and changes. `overSidebar`: the
 * sidebar is under the thread, so the top bar has the button that shows it.
 */
@Composable
fun ThreadScreen(overSidebar: Boolean = true) {
    val store = LocalStore.current
    val density = LocalDensity.current
    var composerHeight by remember { mutableStateOf(120.dp) }
    val isStart = store.isStart
    // An agent the thread's agent started shows in the thread's place, with a bar in the composer's.
    val agent = store.shownAgent?.takeIf { !isStart }
    // A thread whose server was removed ends with the row that says so, in the composer's place.
    val removed = agent == null && store.openThreadRemovedServer != null
    val below = WindowInsets.ime.union(WindowInsets.navigationBars).only(WindowInsetsSides.Bottom)
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding() + barHeight
    FileDrop(Modifier.fillMaxSize()) {
        Box(Modifier.fillMaxSize().background(Theme.colors.background)) {
            if (!isStart) {
                val bottom = below.asPaddingValues().calculateBottomPadding() + if (removed) composerBottomGap else composerHeight
                val model = if (agent != null) store.agentTranscript else store.transcript
                key(model) {
                    TranscriptView(model, Modifier.fillMaxSize(), contentPadding = PaddingValues(top = top, bottom = bottom), underComposer = !removed)
                }
            }
            Column(Modifier.fillMaxSize()) {
                TopBar(overSidebar)
                Box(Modifier.weight(1f).fillMaxWidth().windowInsetsPadding(below)) {
                    if (isStart) Start(composerHeight)
                    val composerPlace = Modifier
                        .align(Alignment.BottomCenter)
                        .fillMaxWidth()
                        .onSizeChanged { composerHeight = with(density) { it.height.toDp() } }
                        .padding(start = Theme.composerPadding, end = Theme.composerPadding, top = TranscriptView.composerGap, bottom = composerBottomGap)
                    if (agent != null) {
                        Box(composerPlace, contentAlignment = Alignment.BottomCenter) { AgentBar(agent) }
                    } else if (!removed && (!isStart || store.projects.isNotEmpty() || store.noProjects.isNotEmpty())) {
                        ComposerView(composerPlace)
                    }
                }
            }
            OpenNotice(Modifier.align(Alignment.TopEnd).padding(top = top + 6.dp, start = 14.dp, end = 14.dp))
        }
    }
}

/** A new thread with nothing sent yet, which shows where it will start instead of a transcript. */
private val AppStore.isStart: Boolean
    get() {
        val draft = selectedDraft ?: return false
        return transcriptIsEmpty && !isSending(draft)
    }

/** The notice over the top of the thread, which comes down a little as it appears. */
@Composable
private fun OpenNotice(modifier: Modifier) {
    val notice = LocalStore.current.openNotice
    val kept = remember { arrayOfNulls<Notice>(1) }
    if (notice != null) kept[0] = notice
    val rise = with(LocalDensity.current) { 6.dp.roundToPx() }
    AnimatedVisibility(
        notice != null, modifier,
        enter = fadeIn(tween(150)) + slideInVertically(tween(150)) { -rise },
        exit = fadeOut(tween(150)) + slideOutVertically(tween(150)) { -rise },
    ) {
        kept[0]?.let { NoticeView(it) }
    }
}

@Composable
private fun TopBar(overSidebar: Boolean) {
    val store = LocalStore.current
    val drawer = LocalDrawer.current
    Row(
        Modifier
            .fillMaxWidth()
            .windowInsetsPadding(WindowInsets.statusBars)
            .height(barHeight)
            .padding(horizontal = barPadding),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val parent = if (store.shownAgent == null) store.parentThread else null
        if (overSidebar || parent != null) {
            Row(Modifier.glassButton(CircleShape), verticalAlignment = Alignment.CenterVertically) {
                if (overSidebar) {
                    GlassButton(help = "Threads", onClick = { drawer.isOpen = !drawer.isOpen }, bare = true) { MenuIcon(store.attention(Theme.colors)) }
                }
                parent?.let {
                    GlassButton(Symbol.CornerLeftUp, "Open ${it.title}", onClick = { store.openStartedThread(it.id) }, symbolSize = 16f, bare = true)
                }
            }
        }
        Title(Modifier.weight(1f))
        Row(Modifier.glassButton(CircleShape), verticalAlignment = Alignment.CenterVertically) {
            val project = store.gitProject
            val control = project?.gitControl
            val base = store.draftBaseToPull
            when {
                project != null && control != null -> GitButton(project, control)
                project != null && store.canInitializeGit(project) -> InitializeGitButton(project)
                base != null -> PullBaseButton(base)
            }
            GlassButton(Symbol.PanelRight, "Files and changes", onClick = { store.rightPanel.isOpen = !store.rightPanel.isOpen }, symbolSize = 16f, bare = true)
        }
    }
}

/**
 * The colour of the first thread, other than the open one, that waits for the user or has a reply
 * they haven't seen, or else the working colour while one works, or else the monitoring colour
 * while one monitors.
 */
private fun AppStore.attention(palette: Palette): Color? {
    val others = activeThreads.filter { selection != Selection.Thread(it.id) }
    others.firstNotNullOfOrNull { it.attentionColor(palette) }?.let { return it }
    if (others.any { it.working || it.gitStage != null }) return palette.process
    if (others.none { it.monitoring }) return null
    return palette.foreground
}

/** The menu icon, with a dot on its top right corner, cut out of the icon, when another thread needs attention, works or monitors, in the colour of its status. */
@Composable
private fun MenuIcon(attention: Color?) {
    if (attention == null) {
        Icon(Symbol.Menu, 16f)
        return
    }
    val side = symbolSide(16f)
    Icon(
        Symbol.Menu, 16f,
        Modifier
            .size(side)
            .graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
            .drawWithContent {
                drawContent()
                val dot = Offset(size.width * 20 / 24, size.height * 5 / 24)
                drawCircle(Color.Black, radius = 6.dp.toPx(), center = dot, blendMode = BlendMode.Clear)
                drawCircle(attention, radius = 4.dp.toPx(), center = dot)
            },
    )
}

/** The thread's name, after the thread whose agent started it, or for an agent shown in its place, the agent's after the thread's. Then the project. */
@Composable
private fun Title(modifier: Modifier) {
    val store = LocalStore.current
    Column(modifier, verticalArrangement = Arrangement.spacedBy(1.dp)) {
        store.composerProjectLine?.let { parts -> ProjectLine(store.composerProject, parts, size = 12f, iconSize = 12f) }
        Text(store.shownAgent?.title ?: store.selectedThread?.title ?: "New thread", style = system(15f, FontWeight.SemiBold), maxLines = 1)
    }
}

/** The empty state of a new thread: a question, over the composer. */
@Composable
private fun Start(composerHeight: androidx.compose.ui.unit.Dp) {
    val store = LocalStore.current
    val palette = Theme.colors
    Column(
        Modifier
            .fillMaxSize()
            .clickable(remember { MutableInteractionSource() }, indication = null) { Platform.endEditing() }
            .padding(start = 24.dp, end = 24.dp, bottom = composerHeight),
        verticalArrangement = Arrangement.spacedBy(10.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        if (store.projects.isEmpty() && store.noProjects.isEmpty()) {
            Text("Add a project to start", style = system(26f))
            Text(
                "A project is a folder on your server that threads work in.", style = system(16f), color = palette.mutedForeground,
                align = TextAlign.Center,
            )
            ActionButton(
                "Add Project", Symbol.FolderPlus, Modifier.padding(top = 10.dp), variant = ButtonVariant.Primary, size = ControlSize.Large,
                disabled = store.servers.none { it.state == Server.State.Connected },
            ) { store.addProject() }
        } else {
            Headline()
        }
    }
}

@Composable
private fun Headline() {
    val store = LocalStore.current
    val palette = Theme.colors
    val selected = store.project(store.selectedDraft?.projectID)
    val (lead, name) = Project.headline(selected)
    val shape = RoundedCornerShape(Radius.lg)
    Column(verticalArrangement = Arrangement.spacedBy(12.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Text(lead, style = system(26f))
        DimButton(onClick = { store.openPanel(PanelPage.DraftProject) }, contentAlignment = Alignment.Center) {
            Row(
                Modifier
                    .border(1.dp, palette.border, shape)
                    .height(48.dp)
                    .padding(start = 12.dp, end = 14.dp),
                horizontalArrangement = Arrangement.spacedBy(9.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                ProjectIcon(selected, size = 24f)
                Text(name, Modifier.weight(1f, fill = false), style = system(26f), maxLines = 1)
                Icon(Symbol.ChevronDown, 11f, color = palette.mutedStrongerForeground)
            }
        }
    }
}
