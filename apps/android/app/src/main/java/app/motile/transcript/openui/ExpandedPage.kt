package app.motile.transcript.openui

import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import android.widget.FrameLayout
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.systemGestureExclusion
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import app.motile.LocalStore
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import kotlin.math.abs

/**
 * An HtmlView's page over the whole window. The web view its row showed moves here and back, so
 * that the page keeps where it was. Here it is zoomed by what core's fit script answers, so that a
 * page drawn at a fixed size fills the window one way.
 */
class ExpandedPage internal constructor(
    val title: String,
    private val view: WebView,
    private val page: UiPage,
    private var reported: Float,
    private val back: (WebView) -> Unit,
) {
    /** How tall its content is drawn, zoomed, once it has been fitted to the room it has. */
    var content by mutableStateOf<Float?>(null)
        private set
    private var zoom = 1f
    /** How wide the view was when it was zoomed, which the viewport's width follows. */
    private var zoomedWidth = 0
    private var fitting = false
    private var holder: ViewGroup? = null
    private val resized = View.OnLayoutChangeListener { _, left, top, right, bottom, oldLeft, oldTop, oldRight, oldBottom ->
        if (right - left != oldRight - oldLeft || bottom - top != oldBottom - oldTop) refitSoon()
    }

    /** Puts the web view in what holds it over the window, which it fills. */
    internal fun place(holder: ViewGroup) {
        (view.parent as? ViewGroup)?.removeView(view)
        holder.addView(view, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        this.holder = holder
        view.addOnLayoutChangeListener(resized)
        refitSoon()
    }

    /** A height the page reported while it is here. */
    internal fun reported(height: Float) {
        reported = height
        if (content != null) content = height * zoom
        refit()
    }

    /** Gives the page back to its row, unzoomed. */
    fun close() {
        view.removeOnLayoutChangeListener(resized)
        holder?.removeView(view)
        holder = null
        if (view.tag !== Gone) zoom(1f)
        back(view)
    }

    /** Once the page has been laid out at its new size. */
    private fun refitSoon() {
        view.postDelayed(::refit, 100)
    }

    /**
     * Zooms the page by what the script says it has room for; the height it reports then asks
     * again, until the answer settles. The first answer, zoomed or not, gives the content's height.
     */
    private fun refit() {
        if (fitting || holder == null || view.parent !== holder || view.tag === Gone || view.width == 0) return
        fitting = true
        view.evaluateJavascript(page.fitScript) { answer ->
            fitting = false
            if (holder == null) return@evaluateJavascript
            val ratio = answer?.toFloatOrNull()?.takeIf { it.isFinite() && it > 0f } ?: return@evaluateJavascript
            val fitted = (zoom * ratio).coerceIn(1f, page.maxFit)
            val changes = abs(fitted - zoom) > 0.01f
            if (changes || (zoom != 1f && view.width != zoomedWidth)) zoom(if (changes) fitted else zoom)
            if ((changes || content == null) && reported > 0f) content = reported * zoom
        }
    }

    /**
     * Zooms as a browser does: the viewport becomes `zoom` times smaller in CSS pixels and is drawn
     * that much larger, so that the page lays out again in it, its `vh` and `vw` too.
     */
    private fun zoom(to: Float) {
        zoom = to
        zoomedWidth = view.width
        val width = view.width / view.resources.displayMetrics.density / to
        view.settings.useWideViewPort = to != 1f
        view.evaluateJavascript("($VIEWPORT)($to,$width)", null)
    }
}

/** Sets every viewport the page names to `z` times smaller, `w` wide, or back to its own at 1. */
private const val VIEWPORT = "function(z,w){var m=document.querySelectorAll('meta[name=viewport]');for(var i=0;i<m.length;i++){var e=m[i];" +
    "if(e.motile===undefined)e.motile=e.content;e.content=z===1?e.motile:'width='+w+',initial-scale='+z+',minimum-scale='+z+',maximum-scale='+z}}"

/**
 * An HtmlView's page over the whole window, with the pill that puts it back under it, where the
 * thumb is. A page shorter than the room is only as tall as its content, so that the pill stays by
 * it, and both sit at the bottom. Every touch on the page is the page's, and the system's edge
 * gestures leave it alone where they can; the system's back closes it.
 */
@Composable
fun ExpandedPageCover(page: ExpandedPage) {
    val store = LocalStore.current
    Column(
        Modifier.fillMaxSize().background(LocalPalette.current.background).safeDrawingPadding(),
        verticalArrangement = Arrangement.spacedBy(EXIT_GAP, Alignment.Bottom),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        val content = page.content
        val height = if (content == null) Modifier else Modifier.heightIn(max = content.dp).fillMaxHeight()
        AndroidView(
            factory = { context -> FrameLayout(context).also(page::place) },
            modifier = Modifier.fillMaxWidth().weight(1f, fill = content == null).then(height).systemGestureExclusion(),
        )
        ActionButton(
            "Exit full screen", Symbol.Minimize2, variant = ButtonVariant.Outline, size = EXIT_SIZE, symbolSize = EXIT_SIZE.smallSymbol, round = true,
        ) { store.closeExpandedPage() }
    }
}

private val EXIT_SIZE = ControlSize.Large
private val EXIT_GAP = 12.dp
