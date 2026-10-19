package app.motile.transcript.openui

import android.annotation.SuppressLint
import android.content.Context
import android.content.ContextWrapper
import android.view.View
import android.view.ViewGroup
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import app.motile.core.Native
import app.motile.core.UiKind
import app.motile.core.UiNode
import app.motile.core.UiTextStyle
import app.motile.platform.Platform
import app.motile.platform.scaled
import app.motile.theme.LocalDark
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Radius
import app.motile.theme.Theme
import app.motile.ui.Spinner
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.floatOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** A page core built for a web view, and how to load it (`web::Page`). */
class UiPage(val html: String, val base: String?, val handler: String, val start: Float, val min: Float, val max: Float) {
    /** The height in a message the page sent, within the page's bounds; none for any other message. */
    fun height(message: String): Float? {
        val json = runCatching { Json.parseToJsonElement(message).jsonObject }.getOrNull() ?: return null
        if (json["type"]?.jsonPrimitive?.content != HEIGHT_MESSAGE) return null
        val height = json["height"]?.jsonPrimitive?.doubleOrNull?.takeIf { it.isFinite() } ?: return null
        return height.toFloat().coerceIn(min, max)
    }

    companion object {
        const val HEIGHT_MESSAGE = "iui-html-view-height"

        /** Asks core for the page of a `web::PageRequest`. */
        fun build(request: JsonObject): UiPage? {
            val json = runCatching { Json.parseToJsonElement(Native.uiPage(request.toString())).jsonObject }.getOrNull() ?: return null
            val html = json["html"]?.jsonPrimitive?.content ?: return null
            val height = json["height"]?.jsonObject ?: return null
            fun bound(key: String) = height[key]?.jsonPrimitive?.floatOrNull ?: 0f
            return UiPage(
                html = html,
                base = (json["base"] as? JsonPrimitive)?.takeIf { it.isString }?.content,
                handler = json["handler"]?.jsonPrimitive?.content ?: return null,
                start = bound("start"),
                min = bound("min"),
                max = bound("max"),
            )
        }
    }
}

/** The heights pages reported, by row and node, so that a page back on screen keeps its own. */
private object UiWebHeights {
    private val heights = object : LinkedHashMap<String, Float>(64, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, Float>) = size > 256
    }

    @Synchronized fun get(key: String): Float? = heights[key]

    @Synchronized fun set(key: String, height: Float) {
        heights[key] = height
    }
}

/**
 * The program's page in a sandboxed web view: its height `height`, else what the page reports, 360
 * until it does. While the program streams, the spinner in its place.
 */
@Composable
fun UiHtmlView(node: UiNode, kind: UiKind.HtmlView) {
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    val dark = LocalDark.current
    val shape = RoundedCornerShape(Radius.lg)
    val key = "${LocalUiHost.current?.rowID}/${node.id}"
    val foreground = palette.foreground
    val background = surface.color
    val page by produceState<UiPage?>(null, kind.html, dark, foreground, background) {
        value = withContext(Dispatchers.Default) {
            UiPage.build(
                buildJsonObject {
                    put("kind", "html")
                    put("html", kind.html)
                    put("bridge", "android")
                    put("dark", dark)
                    put("text_size", Theme.proseSize.value)
                    put("line_height", scaled(24f))
                    put("color", css(foreground))
                    put("background", css(background))
                },
            )
        }
    }
    var reported by remember(key) { mutableFloatStateOf(UiWebHeights.get(key) ?: 0f) }
    val height = kind.height ?: reported.takeIf { it > 0f } ?: page?.start ?: START_HEIGHT
    Box(
        Modifier.uiNode(node).fillMaxWidth().height(height.dp).clip(shape).background(background, shape).border(1.dp, surface.border, shape),
        contentAlignment = Alignment.Center,
    ) {
        val loaded = page
        when {
            kind.streaming -> Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Spinner(13f, color = palette.mutedForeground)
                Text("Preparing the interactive view", style = UiStyle.text(UiTextStyle.Small, false), color = palette.mutedForeground)
            }
            loaded != null -> PageView(loaded, Modifier.fillMaxSize(), opensYouTube = false) { reportedHeight ->
                UiWebHeights.set(key, reportedHeight)
                reported = reportedHeight
            }
        }
    }
}

/** YouTube's player, 16:9 and the column's width, in the same kind of web view. */
@Composable
fun UiYouTube(node: UiNode, kind: UiKind.YouTube) {
    val url = kind.url ?: return
    val shape = RoundedCornerShape(Radius.lg)
    val page by produceState<UiPage?>(null, url, kind.title) {
        value = withContext(Dispatchers.Default) {
            UiPage.build(
                buildJsonObject {
                    put("kind", "youtube")
                    put("url", url)
                    put("title", kind.title)
                },
            )
        }
    }
    Box(Modifier.uiNode(node).fillMaxWidth().aspectRatio(16f / 9f).clip(shape).background(Color.Black, shape)) {
        page?.let { PageView(it, Modifier.fillMaxSize(), opensYouTube = true) }
    }
}

/**
 * A web view that loads the page once and goes nowhere else: no files, no storage, no other
 * windows, and one channel, for its height. YouTube's links open in the browser.
 */
@Composable
private fun PageView(page: UiPage, modifier: Modifier, opensYouTube: Boolean, onHeight: ((Float) -> Unit)? = null) {
    AndroidView(
        factory = { context -> sandboxed(context, opensYouTube) },
        modifier = modifier,
        update = { view ->
            if (view.tag == page) return@AndroidView
            view.tag = page
            if (onHeight != null && WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
                WebViewCompat.removeWebMessageListener(view, page.handler)
                WebViewCompat.addWebMessageListener(view, page.handler, setOf("*")) { _, message, _, isMainFrame, _ ->
                    if (!isMainFrame) return@addWebMessageListener
                    message.data?.let(page::height)?.let(onHeight)
                }
            }
            view.loadDataWithBaseURL(page.base, page.html, "text/html", "utf-8", null)
        },
        onRelease = { view ->
            (view.webChromeClient as? FullScreenChrome)?.leave()
            view.destroy()
        },
    )
}

@SuppressLint("SetJavaScriptEnabled")
private fun sandboxed(context: Context, opensYouTube: Boolean): WebView = WebView(context).apply {
    setBackgroundColor(android.graphics.Color.TRANSPARENT)
    isVerticalScrollBarEnabled = false
    with(settings) {
        javaScriptEnabled = true
        allowFileAccess = false
        allowContentAccess = false
        domStorageEnabled = false
        cacheMode = WebSettings.LOAD_NO_CACHE
        javaScriptCanOpenWindowsAutomatically = false
        setSupportMultipleWindows(false)
        setGeolocationEnabled(false)
        mediaPlaybackRequiresUserGesture = true
    }
    webChromeClient = FullScreenChrome(context)
    webViewClient = object : WebViewClient() {
        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
            if (!request.isForMainFrame) return !opensYouTube
            val url = request.url
            if (opensYouTube && request.hasGesture() && url.scheme == "https" && url.host?.let(::isYouTube) == true) Platform.open(url.toString())
            return true
        }
    }
}

/**
 * Shows what the page puts full screen, the player's video, over the whole app with the system's
 * bars hidden, as a browser does, until the player leaves it or the back gesture does.
 */
private class FullScreenChrome(private val context: Context) : WebChromeClient() {
    private var shown: View? = null
    private var callback: CustomViewCallback? = null
    private var back: OnBackPressedCallback? = null

    override fun onShowCustomView(view: View, callback: CustomViewCallback) {
        val activity = context.activity()
        val decor = activity?.window?.decorView as? FrameLayout
        if (activity == null || decor == null || shown != null) return callback.onCustomViewHidden()
        view.setBackgroundColor(android.graphics.Color.BLACK)
        decor.addView(view, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        shown = view
        this.callback = callback
        WindowCompat.getInsetsController(activity.window, decor).apply {
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            hide(WindowInsetsCompat.Type.systemBars())
        }
        back = object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() = leave()
        }.also { activity.onBackPressedDispatcher.addCallback(it) }
    }

    override fun onHideCustomView() = close()

    /** Leaves full screen from outside the page: the page is told, then the view goes. */
    fun leave() {
        val callback = callback ?: return
        close()
        callback.onCustomViewHidden()
    }

    private fun close() {
        val view = shown ?: return
        shown = null
        callback = null
        back?.remove()
        back = null
        (view.parent as? ViewGroup)?.removeView(view)
        val window = context.activity()?.window ?: return
        WindowCompat.getInsetsController(window, window.decorView).show(WindowInsetsCompat.Type.systemBars())
    }
}

private tailrec fun Context.activity(): ComponentActivity? = when (this) {
    is ComponentActivity -> this
    is ContextWrapper -> baseContext.activity()
    else -> null
}

private fun isYouTube(host: String): Boolean = host == "youtu.be" || host == "youtube.com" || host.endsWith(".youtube.com")

/** A colour as CSS takes it, `#rrggbbaa`. */
private fun css(color: Color): String {
    val argb = color.toArgb()
    return "#%06x%02x".format(argb and 0xffffff, (argb ushr 24) and 0xff)
}

private const val START_HEIGHT = 360f
