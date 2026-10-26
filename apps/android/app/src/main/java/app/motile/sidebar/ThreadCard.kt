package app.motile.sidebar

import android.os.SystemClock
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import app.motile.LocalStore
import app.motile.platform.Symbol
import app.motile.theme.iconSize
import app.motile.theme.LocalForeground
import app.motile.theme.Theme
import app.motile.ui.AgentIcon
import app.motile.ui.DropdownCard
import app.motile.ui.Icon
import app.motile.ui.ProjectIcon
import app.motile.ui.Text
import app.motile.ui.ThemeDivider
import app.motile.ui.color
import app.motile.ui.dotted
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.max
import kotlin.math.min

/** What a thread is, beside its row in the sidebar: where it works, the model and the account as the composer shows them, and its pull request. */
object ThreadCard {
    val maxWidth = 320.dp
    /** The icon of each line, beside its words. */
    val iconSize = iconSize(text = 12f)
}

@Composable
fun ThreadCard(threadID: String, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val thread = store.anyThread(threadID) ?: return
    val project = store.project(thread.projectID)?.seen(thread)
    val palette = Theme.colors
    DropdownCard(modifier.widthIn(max = ThreadCard.maxWidth)) {
        Column(Modifier.padding(12.dp)) {
            Text(thread.title, size = 13f, weight = FontWeight.Medium, color = palette.foreground, maxLines = 1)
            Column(Modifier.padding(top = 10.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                CardLine(project?.name ?: lastComponent(thread.cwd)) { ProjectIcon(project, size = ThreadCard.iconSize) }
                val branch = project?.branch
                if (project != null && branch != null) {
                    CardLine(branch, middle = true) { Icon(project.checkoutSymbol, ThreadCard.iconSize) }
                } else {
                    val home = store.server(thread.serverID)?.home ?: ""
                    CardLine(shortenedFolder(thread.cwd, home), middle = true) { Icon(Symbol.Folder, ThreadCard.iconSize) }
                }
                CardLine(store.modelLabel(thread)) { AgentIcon(thread.agent, size = ThreadCard.iconSize) }
                store.server(thread.serverID)?.let { server -> CardLine(server.name) { Icon(Symbol.Server, ThreadCard.iconSize) } }
            }
            val pullRequest = project?.pullRequest(thread) ?: return@Column
            ThemeDivider(Modifier.padding(vertical = 10.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(pullRequest.state.symbol, ThreadCard.iconSize, color = pullRequest.state.color)
                Text("#${pullRequest.number}", style = Theme.uiDigits(12f, FontWeight.Medium), color = palette.mutedStrongerForeground)
                Text(pullRequest.title, size = 12f, color = palette.mutedForeground, maxLines = 1)
            }
        }
    }
}

@Composable
private fun CardLine(text: String, middle: Boolean = false, icon: @Composable () -> Unit) {
    val palette = Theme.colors
    CompositionLocalProvider(LocalForeground provides palette.mutedForeground) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            icon()
            Text(
                dotted(text, palette.mutedStrongerForeground), size = 12f, maxLines = 1,
                overflow = if (middle) TextOverflow.MiddleEllipsis else TextOverflow.Ellipsis,
            )
        }
    }
}

/** The thread whose card shows, and its row in the window. */
data class Peek(val threadID: String, val row: Rect)

/**
 * Brings up the card of the thread whose row the pointer rests on. It waits a moment the first
 * time, then goes at once to every row the pointer moves to, and shortly after it went away.
 */
@Stable
class ThreadPeek {
    var shown by mutableStateOf<Peek?>(null)
        private set
    private var waiting: Job? = null
    private var hiddenAt = 0L

    /** The pointer is on the thread's row, `row` in the window, or on no thread's. */
    fun point(threadID: String?, row: Rect, scope: CoroutineScope) {
        waiting?.cancel()
        if (threadID == null) return hide()
        val peek = Peek(threadID, row)
        if (shown != null || SystemClock.uptimeMillis() - hiddenAt < LINGER) {
            shown = peek
            return
        }
        waiting = scope.launch {
            delay(REST)
            shown = peek
        }
    }

    fun hide() {
        waiting?.cancel()
        if (shown == null) return
        shown = null
        hiddenAt = SystemClock.uptimeMillis()
    }

    private companion object {
        const val REST = 300L
        const val LINGER = 400L
    }
}

/** Tells `peek` when a pointer comes onto the row and leaves it. */
@Composable
fun Modifier.peeks(threadID: String?, peek: ThreadPeek): Modifier {
    val scope = rememberCoroutineScope()
    val id by rememberUpdatedState(threadID)
    var bounds by remember { mutableStateOf(Rect.Zero) }
    return onGloballyPositioned { bounds = it.boundsInWindow() }
        .pointerInput(peek) {
            awaitPointerEventScope {
                while (true) {
                    val event = awaitPointerEvent()
                    if (event.changes.none { it.type == PointerType.Mouse }) continue
                    when (event.type) {
                        PointerEventType.Enter -> peek.point(id, bounds, scope)
                        PointerEventType.Exit -> peek.point(null, bounds, scope)
                    }
                }
            }
        }
}

/** Where the card stands from its row's light, with room for its shadow. */
private val cardMargin = 16.dp
private val cardGap = 4.dp

/** The card beside its row: level with the row's top, lifted where it would pass the window's bottom. */
@Composable
fun ThreadPeekCard(peek: ThreadPeek) {
    val shown = peek.shown ?: return
    val density = LocalDensity.current
    val position = remember(shown.row, density) {
        with(density) {
            CardPosition(
                left = (shown.row.right - rowMargin.calculateRightPadding(LayoutDirection.Ltr).toPx() + cardGap.toPx() - cardMargin.toPx()).toInt(),
                top = (shown.row.top + rowMargin.calculateTopPadding().toPx() - cardMargin.toPx()).toInt(),
            )
        }
    }
    val alpha = remember { Animatable(0f) }
    LaunchedEffect(Unit) { alpha.animateTo(1f, tween(100)) }
    Popup(popupPositionProvider = position, properties = PopupProperties(focusable = false, dismissOnBackPress = false, dismissOnClickOutside = false)) {
        Box(Modifier.graphicsLayer { this.alpha = alpha.value }.padding(cardMargin)) {
            ThreadCard(shown.threadID)
        }
    }
}

private class CardPosition(val left: Int, val top: Int) : PopupPositionProvider {
    override fun calculatePosition(anchorBounds: IntRect, windowSize: IntSize, layoutDirection: LayoutDirection, popupContentSize: IntSize): IntOffset {
        val y = max(0, min(top, windowSize.height - popupContentSize.height))
        return IntOffset(left, y)
    }
}
