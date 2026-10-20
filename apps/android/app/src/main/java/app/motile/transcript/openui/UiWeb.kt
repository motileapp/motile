package app.motile.transcript.openui

import android.annotation.SuppressLint
import android.content.Context
import android.content.ContextWrapper
import android.view.View
import android.view.ViewGroup
import android.webkit.JsPromptResult
import android.webkit.JsResult
import android.webkit.RenderProcessGoneDetail
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
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.text.style.TextAlign
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
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.LocalDark
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Palette
import app.motile.theme.Theme
import app.motile.ui.ActionButton
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
    val shape = RoundedCornerShape(UiStyle.radius(kind.radius))
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
    var gone by remember(key) { mutableStateOf(false) }
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
            gone -> Still(kind.title, "Interactive view") { gone = false }
            loaded != null -> PageView(loaded, Modifier.fillMaxSize(), opensYouTube = false, onGone = { gone = true }) { reportedHeight ->
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
    val shape = RoundedCornerShape(UiStyle.radius(kind.radius))
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
    var gone by remember(node.id, url) { mutableStateOf(false) }
    Box(Modifier.uiNode(node).fillMaxWidth().aspectRatio(16f / 9f).clip(shape).background(Color.Black, shape), contentAlignment = Alignment.Center) {
        val loaded = page
        when {
            gone -> CompositionLocalProvider(LocalPalette provides Palette.dark, LocalDark provides true) {
                Still(kind.title, "YouTube video") { gone = false }
            }
            loaded != null -> PageView(loaded, Modifier.fillMaxSize(), opensYouTube = true, onGone = { gone = true })
        }
    }
}

/** What stands in for a page whose renderer went: its title, and a button that builds it again. */
@Composable
private fun Still(title: String, kind: String, reload: () -> Unit) {
    val palette = LocalPalette.current
    Column(Modifier.padding(horizontal = 16.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (title.isNotEmpty()) Text(title, style = UiStyle.text(UiTextStyle.CardTitle, false), color = palette.foreground, textAlign = TextAlign.Center)
        Text(kind, style = UiStyle.text(UiTextStyle.Small, false), color = palette.mutedForeground)
        ActionButton("Reload", Symbol.RotateCw, onClick = reload)
    }
}

/**
 * A web view that loads the page once and goes nowhere else: no files, no storage, no other
 * windows, and one channel, for its height. YouTube's links open in the browser.
 */
@Composable
private fun PageView(page: UiPage, modifier: Modifier, opensYouTube: Boolean, onGone: () -> Unit, onHeight: ((Float) -> Unit)? = null) {
    val gone by rememberUpdatedState(onGone)
    AndroidView(
        factory = { context -> sandboxed(context, opensYouTube) { gone() } },
        modifier = modifier,
        update = { view ->
            if (view.tag == page || view.tag === Gone) return@AndroidView
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
            if (view.tag === Gone) return@AndroidView
            (view.webChromeClient as? FullScreenChrome)?.leave()
            view.destroy()
        },
    )
}

@SuppressLint("SetJavaScriptEnabled")
private fun sandboxed(context: Context, opensYouTube: Boolean, onGone: () -> Unit): WebView = WebView(context).apply {
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
    webChromeClient = if (opensYouTube) FullScreenChrome(context) else Chrome()
    webViewClient = object : WebViewClient() {
        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
            if (!request.isForMainFrame) return !opensYouTube
            val url = request.url
            if (opensYouTube && request.hasGesture() && url.scheme == "https" && url.host?.let(::isYouTube) == true) Platform.open(url.toString())
            return true
        }

        /** A page whose renderer crashed or was killed goes, instead of taking the app with it, and its still shows. */
        override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
            (view.webChromeClient as? FullScreenChrome)?.leave()
            view.tag = Gone
            (view.parent as? ViewGroup)?.removeView(view)
            view.destroy()
            onGone()
            return true
        }
    }
}

/** What a web view's tag is once its renderer is gone: the view is destroyed. */
private object Gone

/** A page shows no dialogs and doesn't go full screen. */
private open class Chrome : WebChromeClient() {
    override fun onJsAlert(view: WebView, url: String?, message: String?, result: JsResult): Boolean = refuse(result)

    override fun onJsConfirm(view: WebView, url: String?, message: String?, result: JsResult): Boolean = refuse(result)

    override fun onJsPrompt(view: WebView, url: String?, message: String?, defaultValue: String?, result: JsPromptResult): Boolean =
        refuse(result)

    override fun onJsBeforeUnload(view: WebView, url: String?, message: String?, result: JsResult): Boolean = refuse(result)

    override fun onShowCustomView(view: View, callback: CustomViewCallback) = callback.onCustomViewHidden()

    private fun refuse(result: JsResult): Boolean {
        result.cancel()
        return true
    }
}

/**
 * Shows what the page puts full screen, the player's video, over the whole app with the system's
 * bars hidden, as a browser does, until the player leaves it or the back gesture does.
 */
private class FullScreenChrome(private val context: Context) : Chrome() {
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
