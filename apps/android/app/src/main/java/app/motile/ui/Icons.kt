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
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.vector.PathParser
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
import app.motile.theme.LocalPalette
import app.motile.theme.Palette
import app.motile.theme.Theme
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

/**
 * The agents' logos and GitHub's, from the marks their makers publish (as collected by Simple
 * Icons), as paths in a square of 24. Compose reads an SVG path itself, so nothing here parses one.
 */
object AgentLogo {
    val claude: Path by lazy { logo("m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z") }
    val openAI: Path by lazy { logo("M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z") }
    val github: Path by lazy { logo("M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12") }

    private fun logo(data: String): Path = PathParser().parsePathString(data).toPath()
}

val Agent.logo: Path
    get() = when (this) {
        Agent.Claude -> AgentLogo.claude
        Agent.Codex -> AgentLogo.openAI
    }

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
fun AgentIcon(agent: Agent, modifier: Modifier = Modifier, size: Float = 14f) {
    LogoMark(agent.logo, size, agent.color, modifier.semantics { contentDescription = agent.title })
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

    /** The image as a square that its picture fills: drawn at one size and without the empty margin some icons come with, so that every icon is as large as the next. */
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
        val scale = min(side.toFloat() / bounds.width(), side.toFloat() / bounds.height())
        val targetWidth = bounds.width() * scale
        val targetHeight = bounds.height() * scale
        val result = createBitmap(side, side)
        val x = (side - targetWidth) / 2
        val y = (side - targetHeight) / 2
        Canvas(result).drawBitmap(canvas, bounds, RectF(x, y, x + targetWidth, y + targetHeight), paint)
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
                if ((pixels[row * drawn + column] ushr 24) <= 24) continue
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
fun ProjectIcon(project: Project?, modifier: Modifier = Modifier, size: Float = 16f) {
    val path = project?.iconPath
    // The icon read for a path, which a row that is reused for another project doesn't show.
    val loaded by produceState<Pair<String, ImageBitmap>?>(null, path) {
        value = path?.let { at -> ImageFiles.load(at)?.let { at to it } }
    }
    val image = loaded?.takeIf { it.first == path }?.second ?: ImageFiles.cached(path)
    Box(modifier.size(size.dp), contentAlignment = Alignment.Center) {
        if (image != null) {
            Image(
                image, project?.name, Modifier.fillMaxSize().clip(RoundedCornerShape((size * 0.22f).dp)),
                contentScale = ContentScale.Fit, filterQuality = FilterQuality.High,
            )
        } else {
            Icon(project.menuSymbol, size * 0.78f, Modifier.wrapContentSize(unbounded = true), color = LocalPalette.current.mutedForeground)
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
        // Lucide's server fills more of its square than the icons beside it.
        Icon(Symbol.Server, size - 1, color = (if (showsState) server.stateTint else null) ?: muted)
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
            horizontalArrangement = Arrangement.spacedBy(2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(pullRequest.state.symbol, 11f, color = color)
            Text("${pullRequest.number}", style = Theme.uiDigits(11f, FontWeight.Medium), color = color)
        }
    }
    if (onClick == null) {
        Box(modifier) { label() }
        return
    }
    DimButton(onClick, modifier) { label() }
}
