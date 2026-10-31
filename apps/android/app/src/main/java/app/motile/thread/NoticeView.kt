package app.motile.thread

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.motile.LocalStore
import app.motile.core.Notice
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.iconSize
import app.motile.theme.ControlSize
import app.motile.theme.LocalForeground
import app.motile.theme.LocalRow
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.ShadowSize
import app.motile.theme.Surface
import app.motile.theme.SurfaceBox
import app.motile.theme.Theme
import app.motile.theme.shadow
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.FadingScroll
import app.motile.ui.Icon
import app.motile.ui.Spinner
import app.motile.ui.Text

/**
 * What the last git action or project add did, or what was refused, at the top of the thread.
 * What follows is one tap away: the push after a commit, the pull request after a push, the
 * thread in a project that was cloned.
 */
object NoticeView {
    val maxWidth = 440.dp
    private val radius = Radius.lg
    private val padding = 14.dp
    private val closeSize = ControlSize.Regular.height
    internal val titleHeight = scaled(16f).dp
    /** Tucks the close button into the corner, its own corner running along the notice's. */
    internal val closeMargin = radius - ControlSize.Regular.radius
    internal val trailing = closeMargin + closeSize + 4.dp
    internal val shape = RoundedCornerShape(radius)
    internal val edge = padding

    internal const val outputSize = 11.5f
    internal val outputPadding = scaled(8f).dp
    /** A line of output with the room between lines. */
    internal val outputLine = (outputSize * Platform.scale * 1.2f + 2).dp
    /** Eight lines of output and the room around them. */
    internal val outputHeight = outputLine * 8 + outputPadding * 2
}

@Composable
fun NoticeView(notice: Notice, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val palette = Theme.colors
    CompositionLocalProvider(LocalSurface provides Surface.Popover, LocalRow provides null, LocalForeground provides palette.foreground) {
        Box(
            modifier
                .widthIn(max = NoticeView.maxWidth)
                .shadow(ShadowSize.Lg, NoticeView.shape, Opacity.ShadowStronger)
                .background(palette.popover, NoticeView.shape)
                .border(1.dp, palette.borderPopover, NoticeView.shape),
        ) {
            Row(
                Modifier.padding(start = NoticeView.edge, top = NoticeView.edge, bottom = NoticeView.edge, end = NoticeView.trailing),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalAlignment = Alignment.Top,
            ) {
                Box(Modifier.height(NoticeView.titleHeight), contentAlignment = Alignment.Center) {
                    val symbolSize = iconSize(text = 13f)
                    when {
                        notice.pending -> Spinner(symbolSize)
                        notice.failed -> Icon(Symbol.CircleAlert, symbolSize, color = palette.destructive)
                        else -> Icon(Symbol.CircleCheck, symbolSize, color = palette.success)
                    }
                }
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Box(Modifier.heightIn(min = NoticeView.titleHeight), contentAlignment = Alignment.CenterStart) {
                        Text(notice.title, size = 13f, weight = FontWeight.Medium)
                    }
                    notice.description?.let { description ->
                        if (notice.failed) {
                            Output(description)
                        } else {
                            SelectionContainer {
                                Text(description, size = 12.5f, color = palette.mutedForeground, maxLines = 2)
                            }
                        }
                    }
                    Actions(notice)
                }
            }
            ActionButton(
                icon = Symbol.X, help = "Close", symbolSize = ControlSize.Regular.smallSymbol,
                modifier = Modifier.align(Alignment.TopEnd).padding(NoticeView.closeMargin),
            ) { store.dismissNotice() }
        }
    }
}

/** What follows from the notice: its pull request, the next git action, the project it added. */
@Composable
private fun Actions(notice: Notice) {
    val store = LocalStore.current
    val url = notice.url
    val next = notice.nextLabel
    if (url == null && next == null && notice.projectID == null) return
    Row(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        if (url != null) Action("View PR", prominent = next == null) { store.showPullRequest(url) }
        if (next != null) Action(next, prominent = true) { store.runNextGit() }
        if (notice.projectID != null) Action("Open project", prominent = true) { store.openNoticeProject() }
    }
}

@Composable
private fun Action(title: String, prominent: Boolean, run: () -> Unit) {
    ActionButton(title, variant = if (prominent) ButtonVariant.Primary else ButtonVariant.Secondary, onClick = run)
}

/** What git printed, scrolled to its end, where a hook says what it found. */
@Composable
private fun Output(text: String) {
    val style = Theme.uiMono(NoticeView.outputSize).copy(lineHeight = NoticeView.outputLine.value.sp)
    SurfaceBox(
        RoundedCornerShape(Radius.md),
        Modifier.padding(top = 4.dp).outset(end = NoticeView.trailing - NoticeView.edge).fillMaxWidth(),
        bordered = true,
    ) {
        FadingScroll(NoticeView.outputHeight, anchor = Alignment.Bottom, padding = NoticeView.outputPadding) {
            SelectionContainer {
                Text(text, Modifier.fillMaxWidth(), style = style, color = Theme.colors.mutedForeground)
            }
        }
    }
}
