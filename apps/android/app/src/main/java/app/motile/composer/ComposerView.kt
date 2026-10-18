package app.motile.composer

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.AppStore
import app.motile.core.Server
import app.motile.core.ThreadChildren
import app.motile.core.ThreadInfo
import app.motile.core.Time
import app.motile.core.closeSkillMenu
import app.motile.layout.KeyboardFocus
import app.motile.panel.PanelTab
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.ControlSize
import app.motile.theme.LocalForeground
import app.motile.theme.LocalSurface
import app.motile.theme.Radius
import app.motile.theme.ShadowSize
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.ui.ActionButton
import app.motile.ui.AgentAvatars
import app.motile.ui.ButtonVariant
import app.motile.ui.Disabled
import app.motile.ui.Icon
import app.motile.ui.Text
import kotlinx.coroutines.delay

/**
 * Where messages are written. A strip above says what the agent waits for, what the agents it
 * started wait for, that it is monitoring, or why it stopped before it finished. The model, the effort and the access are in the thread's
 * settings, which the model's name opens, and the composer is one line until it is written in.
 */
object ComposerView {
    val radius = Radius.xxl

    /** The room around a control in the composer's rows, which is the control's to press. */
    fun margin(leading: Dp = 1.dp, trailing: Dp = 1.dp) = PaddingValues(start = leading, top = 8.dp, end = trailing, bottom = 8.dp)
}

@Composable
fun ComposerView(modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val model = remember(store) { ComposerTextModel(store) }
    val focus = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    val press = {
        runCatching { focus.requestFocus() }
        keyboard?.show()
        Unit
    }

    LaunchedEffect(store.draftWorktreeKey) { store.readWorktreeStart(fetch = true) }
    OnChange(store.draftKey) { store.closeSkillMenu() }
    OnChange(store.composerProject?.git) { store.readWorktreeStart(fetch = false) }
    ComposerDemoHooks(model, focus)
    DisposableEffect(Unit) { onDispose { KeyboardFocus.composer = false } }
    LaunchedEffect(model.focused) { KeyboardFocus.composer = model.focused }

    CompositionLocalProvider(LocalSurface provides Surface.Background) {
        Box(modifier, contentAlignment = Alignment.BottomCenter) {
            Box(Modifier.widthIn(max = Theme.composerWidth).fillMaxWidth()) {
                Column(Modifier.fillMaxWidth()) {
                    val thread = store.selectedThread
                    val approval = store.activity.approvals.firstOrNull()
                    val interruption = thread?.interruption
                    val children = thread?.children
                    val waitingChild = children?.waiting?.firstOrNull()
                    val waitingText = children?.waitingText
                    when {
                        thread != null && approval != null -> WaitingStrip(approval, store.activity.approvals.size)
                        waitingChild != null && waitingText != null -> ChildrenStrip(waitingText, waitingChild, children.waiting.size)
                        thread != null && store.activity.monitoring -> MonitoringStrip(thread)
                        thread != null && children?.runningText != null ->
                            AgentsStrip(children.runningText, children, stops = thread.waitsOnAgents && !store.activity.running)
                        thread != null && !thread.busy && !store.activity.busy && interruption != null -> InterruptionStrip(interruption)
                    }
                    ComposerBox(model, focus, press)
                }
                val menu = store.skillMenu
                if (menu != null) SkillMenuStrip(menu, Modifier.fillMaxWidth().hangingAbove())
            }
        }
    }
}

/** Runs `action` when `value` changes, not when it is first seen. */
@Composable
private fun <T> OnChange(value: T, action: () -> Unit) {
    val seen = remember { arrayOf<Any?>(value) }
    LaunchedEffect(value) {
        if (seen[0] == value) return@LaunchedEffect
        seen[0] = value
        action()
    }
}

/** Lies above the top of where it is placed, over what is there, and takes no room. */
private fun Modifier.hangingAbove(): Modifier = layout { measurable, constraints ->
    val placeable = measurable.measure(constraints.copy(minHeight = 0, maxHeight = Constraints.Infinity))
    layout(placeable.width, 0) { placeable.place(0, -placeable.height) }
}

@Composable
private fun ComposerBox(model: ComposerTextModel, focus: FocusRequester, press: () -> Unit) {
    val store = LocalStore.current
    val shape = RoundedCornerShape(ComposerView.radius)
    val collapsed = !model.focused && store.draft.isEmpty() && store.attachments.isEmpty()
    Column(
        Modifier
            .fillMaxWidth()
            .composerSurface(shape, ShadowSize.Md)
            .clickable(interactionSource = null, indication = null, onClick = press),
    ) {
        val thread = store.selectedThread
        if (thread != null && thread.isDone) DoneBanner(thread)
        store.selectedDraft?.from?.let { ContinuedThreadChip(it, Modifier.padding(start = 14.dp, end = 14.dp, top = 12.dp)) }
        if (store.attachments.isNotEmpty()) Attachments()
        ComposerRows(
            collapsed,
            text = { ComposerTextField(model, placeholder(store), focus) },
            controls = { ComposerTouchControls(collapsed) },
        )
    }
}

private fun placeholder(store: AppStore): String {
    val server = store.composerServer ?: return "Ask anything"
    if (store.selectedThread?.isDone == true) return "Message to bring it back"
    if (server.state != Server.State.Connected) return "Waiting for ${server.name}"
    if (server.models.isEmpty() && server.known) return "Install Claude Code or Codex"
    if (store.activity.running) return "Send a follow-up"
    return "Ask anything"
}

@Composable
private fun MonitoringStrip(thread: ThreadInfo) {
    val store = LocalStore.current
    var now by remember { mutableDoubleStateOf(Time.now()) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(1000)
            now = Time.now()
        }
    }
    val children = thread.children
    val agentsText = children?.runningText
    Row(Modifier.fillMaxWidth().composerStrip(ComposerStrip.Edge.Top), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.padding(start = 14.dp, end = 8.dp).size(6.dp).background(Theme.colors.foreground, CircleShape))
        Text("Monitoring for ${Time.elapsed(thread.monitoringSince, now)}", style = ControlSize.Regular.font, color = Theme.colors.foreground, maxLines = 1)
        if (children != null && agentsText != null) {
            // The button's own room before its avatars is the dot's on its other side.
            Box(Modifier.padding(start = AgentsButton.lead).size(3.dp).background(Theme.colors.mutedStrongerForeground, CircleShape))
            AgentsButton(agentsText, children, Modifier.weight(1f).padding(end = 8.dp))
        } else {
            Spacer(Modifier.weight(1f).widthIn(min = 8.dp))
        }
        val help = if (agentsText == null) "Stop monitoring" else "Stop monitoring and its agents"
        ActionButton("Stop", help = store.shortcuts.help(help, "thread.stop"), variant = ButtonVariant.Ghost, margin = ComposerStrip.margin) { store.stop() }
    }
}

/**
 * The agents the thread's agent started work: which, and once its turn is over, the way to stop them.
 * During the turn the composer's own Stop does.
 */
@Composable
private fun AgentsStrip(text: String, children: ThreadChildren, stops: Boolean) {
    val store = LocalStore.current
    Row(Modifier.fillMaxWidth().composerStrip(ComposerStrip.Edge.Top), verticalAlignment = Alignment.CenterVertically) {
        AgentsButton(text, children, Modifier.weight(1f).padding(start = ComposerStrip.inset, end = 8.dp))
        if (stops) {
            ActionButton("Stop", help = store.shortcuts.help("Stop its agents", "thread.stop"), variant = ButtonVariant.Ghost, margin = ComposerStrip.margin) { store.stop() }
        }
    }
}

private object AgentsButton {
    val avatarSide = 20.dp
    /** The room between the agents button's side and its avatars. */
    val lead = ControlSize.Regular.padding - ControlSize.Regular.symbolOutset
}

/** The agents that work, side by side, as the way to the Agents tab. */
@Composable
private fun AgentsButton(text: String, children: ThreadChildren, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val button = @Composable { words: String, tinted: String? ->
        ActionButton(
            words, tinted = tinted, help = text, variant = ButtonVariant.Ghost, tint = Theme.colors.process,
            margin = PaddingValues(vertical = ComposerStrip.inset),
            pictures = {
                // Taller than the words, they reach into the button's room above and below them.
                AgentAvatars(
                    children.working.map { it to null }, more = (children.running - children.working.size).coerceAtLeast(0),
                    Modifier.overhang(vertical = (AgentsButton.avatarSide - ControlSize.Regular.symbolSide) / 2),
                    side = AgentsButton.avatarSide,
                )
            },
        ) { store.rightPanel.open(PanelTab.Agents) }
    }
    // A narrow composer only counts them.
    val count = children.runningCount
    FirstThatFits(modifier, { if (count == null) button(text, null) else button(count, "working") }, { button("${children.running}", null) })
}

@Composable
private fun DoneBanner(thread: ThreadInfo) {
    val store = LocalStore.current
    Row(
        Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp - ControlSize.Small.padding, top = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Symbol.CircleCheck, 13f, color = Theme.colors.success)
        Text("Done", size = 12.5f, weight = FontWeight.Medium)
        Spacer(Modifier.weight(1f))
        ActionButton("Mark Undone", modifier = Modifier.overhang(vertical = 4.dp), variant = ButtonVariant.Link, size = ControlSize.Small) {
            store.setDone(listOf(thread.id), done = false)
        }
    }
}

@Composable
private fun Attachments() {
    val store = LocalStore.current
    val pictured = store.attachments.filter { it.pictured }
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(start = 14.dp, end = 14.dp, top = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.Bottom,
    ) {
        for (attachment in pictured) {
            AttachmentTile(attachment) {
                store.view(pictured.mapNotNull { it.viewed }, pictured.indexOf(attachment).coerceAtLeast(0))
            }
        }
        for (attachment in store.attachments.filter { !it.pictured }) AttachmentChip(attachment)
    }
}

/** Stops the turn that runs, and sends what is written: into that turn, or queued behind it, as the settings say. */
@Composable
fun ComposerSendButtons() {
    val store = LocalStore.current
    val running = store.activity.running && store.selectedThread != null
    val sends = !running || store.canSend
    Row(verticalAlignment = Alignment.CenterVertically) {
        if (running) {
            ActionButton(
                "", picture = { StopMark() }, help = "Stop", variant = ButtonVariant.Danger, round = true,
                margin = ComposerView.margin(leading = 4.dp, trailing = if (sends) 4.dp else 8.dp),
            ) { store.stop() }
        }
        if (sends) {
            Disabled(!store.canSend) {
                ActionButton(
                    icon = Symbol.ArrowUp, help = "Send", variant = ButtonVariant.Primary, round = true,
                    margin = ComposerView.margin(leading = 4.dp, trailing = 8.dp),
                ) { store.send() }
            }
        }
    }
}

/** The square the button that stops shows. */
@Composable
private fun StopMark() {
    Box(Modifier.size(scaled(10f).dp).background(LocalForeground.current, RoundedCornerShape(Radius.xs)))
}
