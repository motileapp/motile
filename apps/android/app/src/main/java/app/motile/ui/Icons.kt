package app.motile.ui

import androidx.core.graphics.createBitmap
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import android.graphics.RectF
import android.util.LruCache
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.core.Agent
import app.motile.core.Project
import app.motile.core.PullRequest
import app.motile.core.Server
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.ControlSize
import app.motile.theme.IconSize
import app.motile.theme.iconSize
import app.motile.theme.LocalPalette
import app.motile.theme.Palette
import app.motile.theme.Theme
import app.motile.theme.symbolSide
import coil3.ImageLoader
import coil3.request.ImageRequest
import coil3.request.SuccessResult
import coil3.request.allowHardware
import coil3.svg.SvgDecoder
import coil3.toBitmap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlin.math.max
import kotlin.math.min

val Agent.symbol: Symbol get() = if (this == Agent.Claude) Symbol.Claude else Symbol.OpenAI

fun Agent.color(palette: Palette): Color = if (this == Agent.Claude) palette.claude else palette.openai

val Agent.color: Color
    @Composable @ReadOnlyComposable get() = color(LocalPalette.current)

/** A path in a square of 24, filled in one colour at the side given. */
@Composable
fun LogoMark(path: Path, side: Float, color: Color, modifier: Modifier = Modifier) {
    androidx.compose.foundation.Canvas(modifier.size(side.dp)) {
        scale(size.width / 24f, size.height / 24f, pivot = androidx.compose.ui.geometry.Offset.Zero) { drawPath(path, color) }
    }
}

@Composable
fun AgentIcon(agent: Agent, modifier: Modifier = Modifier, size: IconSize = ControlSize.Regular.symbol) {
    Icon(agent.symbol, size, modifier.semantics { contentDescription = agent.title }, color = agent.color)
}

/** Images read from files, each read once and off the main thread. SVGs are drawn by Coil's SVG decoder. */
object ImageFiles {
    private val cache = LruCache<String, ImageBitmap>(128)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val loader by lazy {
        ImageLoader.Builder(Platform.context).components { add(SvgDecoder.Factory()) }.build()
    }

    fun cached(path: String?): ImageBitmap? = path?.let { cache.get(it) }

    suspend fun load(path: String): ImageBitmap? {
        cached(path)?.let { return it }
        val request = ImageRequest.Builder(Platform.context).data(File(path)).size(drawn).allowHardware(false).build()
        val result = loader.execute(request) as? SuccessResult ?: return null
        val image = withContext(Dispatchers.Default) { filling(result.image.toBitmap()).asImageBitmap() }
        cache.put(path, image)
        return image
    }

    /** A picture at an address, like the account's, read once. */
    suspend fun load(url: URL): ImageBitmap? {
        val key = url.toString()
        cached(key)?.let { return it }
        val image = withContext(Dispatchers.IO) {
            runCatching {
                val connection = url.openConnection() as HttpURLConnection
                try {
                    connection.inputStream.use { BitmapFactory.decodeStream(it) }
                } finally {
                    connection.disconnect()
                }
            }.getOrNull()
        }?.asImageBitmap() ?: return null
        cache.put(key, image)
        return image
    }

    /** Reads the images now, so that menus find them. */
    fun warm(paths: List<String>) {
        for (path in paths) {
            if (cached(path) != null) continue
            scope.launch { load(path) }
        }
    }

    /** Writes a pasted or taken image to a file, which is what gets attached. */
    fun saveForAttaching(data: ByteArray, extension: String): File? = runCatching {
        val folder = File(Platform.context.cacheDir, "motile-attachments").apply { mkdirs() }
        val name = "Image ${SimpleDateFormat("yyyy-MM-dd 'at' HH.mm.ss.SSS", Locale.US).format(Date())}.$extension"
        File(folder, name).apply { writeBytes(data) }
    }.getOrNull()

    private const val drawn = 256
    private const val side = 128

    /**
     * The image as a square with its picture in the middle, trimmed of the empty margin some icons
     * come with and as wide as Lucide's icons draw, 22 of 24.
     */
    private fun filling(image: Bitmap): Bitmap {
        if (image.width <= 0 || image.height <= 0) return image
        val paint = Paint(Paint.FILTER_BITMAP_FLAG or Paint.ANTI_ALIAS_FLAG)
        val canvas = createBitmap(drawn, drawn)
        val fit = min(drawn.toFloat() / image.width, drawn.toFloat() / image.height)
        val width = image.width * fit
        val height = image.height * fit
        val left = (drawn - width) / 2
        val top = (drawn - height) / 2
        Canvas(canvas).drawBitmap(image, null, RectF(left, top, left + width, top + height), paint)
        val bounds = opaqueBounds(canvas) ?: return image
        val box = side * 22f / 24
        val scale = min(box / bounds.width(), box / bounds.height())
        val targetWidth = bounds.width() * scale
        val targetHeight = bounds.height() * scale
        val x = (side - targetWidth) / 2
        val y = (side - targetHeight) / 2
        val placed = RectF(x, y, x + targetWidth, y + targetHeight)
        val corner = min(targetWidth, targetHeight) * 0.22f
        val result = createBitmap(side, side)
        Canvas(result).apply {
            clipPath(android.graphics.Path().apply { addRoundRect(placed, corner, corner, android.graphics.Path.Direction.CW) })
            drawBitmap(canvas, bounds, placed, paint)
        }
        return result
    }

    /** The part of the bitmap that isn't transparent. */
    private fun opaqueBounds(bitmap: Bitmap): Rect? {
        val pixels = IntArray(drawn * drawn)
        bitmap.getPixels(pixels, 0, drawn, 0, 0, drawn, drawn)
        var left = drawn
        var right = -1
        var top = drawn
        var bottom = -1
        for (row in 0 until drawn) {
            for (column in 0 until drawn) {
                val alpha = pixels[row * drawn + column] ushr 24
                if (alpha <= 24) continue
                left = min(left, column)
                right = max(right, column)
                top = min(top, row)
                bottom = max(bottom, row)
            }
        }
        if (right < left || bottom < top) return null
        return Rect(left, top, right + 1, bottom + 1)
    }
}

/** A project's icon, or a folder when it has none. */
@Composable
fun ProjectIcon(project: Project?, modifier: Modifier = Modifier, size: IconSize = ControlSize.Regular.symbol) {
    val path = project?.iconPath
    // The icon read for a path, which a row that is reused for another project doesn't show.
    val loaded by produceState<Pair<String, ImageBitmap>?>(null, path) {
        value = path?.let { at -> ImageFiles.load(at)?.let { at to it } }
    }
    val image = loaded?.takeIf { it.first == path }?.second ?: ImageFiles.cached(path)
    Box(modifier.size(symbolSide(size)), contentAlignment = Alignment.Center) {
        if (image != null) {
            Image(image, project?.name, Modifier.fillMaxSize(), contentScale = ContentScale.Fit, filterQuality = FilterQuality.High)
        } else {
            Icon(project.menuSymbol, size, color = LocalPalette.current.mutedForeground)
        }
    }
}

/** The symbol a menu shows for a project, which has no icon there. */
val Project?.menuSymbol: Symbol get() = if (this?.noProject == true) Symbol.MessageSquareDashed else Symbol.Folder

/** The server a thread or project is on, for when there is more than one. */
@Composable
fun ServerLabel(server: Server, modifier: Modifier = Modifier, size: Float = 11f, weight: FontWeight = FontWeight.Normal, showsState: Boolean = false) {
    val muted = LocalPalette.current.mutedStrongerForeground
    Row(
        modifier.semantics { contentDescription = "On ${server.name}" },
        horizontalArrangement = Arrangement.spacedBy(3.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Symbol.Server, iconSize(text = size), color = (if (showsState) server.stateTint else null) ?: muted)
        Text(server.shortName, size = size, weight = weight, color = muted, maxLines = 1)
    }
}

/** The colour of the server's state while it isn't connected. */
val Server.stateTint: Color?
    @Composable @ReadOnlyComposable get() = when (state) {
        Server.State.Connected -> null
        Server.State.Connecting -> LocalPalette.current.pending
        Server.State.Disconnected, Server.State.Refused -> LocalPalette.current.destructive
    }

fun PullRequest.State.color(palette: Palette): Color = when (this) {
    PullRequest.State.Open -> palette.success
    PullRequest.State.Draft -> palette.mutedForeground
    PullRequest.State.Merged -> palette.merged
    PullRequest.State.Closed -> palette.destructive
}

val PullRequest.State.color: Color
    @Composable @ReadOnlyComposable get() = color(LocalPalette.current)

/** A thread's pull request: what became of it, and its number. With `onClick` it is a link. */
@Composable
fun PullRequestLabel(pullRequest: PullRequest, modifier: Modifier = Modifier, colored: Boolean = true, onClick: (() -> Unit)? = null) {
    val color = if (colored) pullRequest.state.color else LocalPalette.current.mutedStrongerForeground
    val label = @Composable {
        Row(
            Modifier.semantics { contentDescription = pullRequest.title },
            horizontalArrangement = Arrangement.spacedBy(3.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(pullRequest.state.symbol, IconSize.Sm, color = color)
            Text("${pullRequest.number}", style = Theme.uiDigits(11f, FontWeight.Medium), color = color)
        }
    }
    if (onClick == null) {
        Box(modifier) { label() }
        return
    }
    DimButton(onClick, modifier) { label() }
}
