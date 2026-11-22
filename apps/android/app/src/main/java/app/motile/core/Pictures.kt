package app.motile.core

import androidx.core.graphics.scale
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.media.MediaMetadataRetriever
import androidx.compose.ui.graphics.toArgb
import app.motile.platform.Platform
import app.motile.theme.Theme
import coil3.ImageLoader
import coil3.executeBlocking
import coil3.request.CachePolicy
import coil3.request.ImageRequest
import coil3.request.SuccessResult
import coil3.request.allowHardware
import coil3.svg.SvgDecoder
import coil3.svg.css
import coil3.toBitmap
import java.io.File
import java.util.UUID
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow

/** Decodes pictures off the main thread: an image's, or a video's first frame. */
object Pictures {
    /** How large an SVG is drawn at most, on its longer side. */
    private const val SVG_MAX_PIXELS = 4096
    private val svgLoader by lazy {
        ImageLoader.Builder(Platform.context).components { add(SvgDecoder.Factory()) }.build()
    }

    /** The first frame of the video, no larger than `maxPixels` on its longer side. */
    fun firstFrame(file: File, maxPixels: Int): Bitmap? = runCatching {
        val retriever = MediaMetadataRetriever()
        try {
            retriever.setDataSource(file.path)
            val frame = retriever.getFrameAtTime(0, MediaMetadataRetriever.OPTION_CLOSEST_SYNC) ?: return null
            fit(frame, maxPixels)
        } finally {
            retriever.release()
        }
    }.getOrNull()

    /** The image, turned as its camera says, no larger than `maxPixels` on its longer side. */
    fun decode(file: File, maxPixels: Int): Bitmap? = runCatching {
        if (file.extension.equals("svg", ignoreCase = true)) return drawSVG(file, minOf(maxPixels, SVG_MAX_PIXELS))
        val bitmap = ImageDecoder.decodeBitmap(ImageDecoder.createSource(file)) { decoder, info, _ ->
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            var sample = 1
            while (max(info.size.width, info.size.height) / (sample * 2) >= maxPixels) sample *= 2
            decoder.setTargetSampleSize(sample)
        }
        fit(bitmap, maxPixels)
    }.getOrNull()

    /**
     * The SVG drawn `side` pixels on its longer side, in the shape it asks for, with its
     * `currentColor` in the text's colour, as a web page gives it, unless its root says a colour of its own.
     */
    private fun drawSVG(file: File, side: Int): Bitmap? {
        val request = ImageRequest.Builder(Platform.context)
            .data(file)
            .size(side)
            .allowHardware(false)
            .memoryCachePolicy(CachePolicy.DISABLED)
        if (!rootHasColor(file.bufferedReader().use { it.readText() })) {
            request.css("svg { color: ${hex(Theme.palette.foreground.toArgb())} }")
        }
        return (svgLoader.executeBlocking(request.build()) as? SuccessResult)?.image?.toBitmap()
    }

    /** Whether the SVG's root element says a `color` of its own. */
    fun rootHasColor(svg: String): Boolean {
        val root = svg.indexOf("<svg").takeIf { it >= 0 } ?: return false
        val tag = svg.substring(root + 4).substringBefore('>')
        return Regex("""\scolor\s*=|\bcolor\s*:""").containsMatchIn(tag)
    }

    private fun hex(argb: Int) = "#%06x".format(argb and 0xffffff)

    /**
     * How light what a picture draws is on average, weighed by how opaque it is, from its pixels as
     * ARGB: null when nothing in it is seen through.
     */
    fun inkLuminance(pixels: IntArray): Double? {
        var (sum, weight, seenThrough) = Triple(0.0, 0.0, false)
        for (pixel in pixels) {
            val alpha = (pixel ushr 24) / 255.0
            seenThrough = seenThrough || alpha < 0.98
            if (alpha <= 0.02) continue
            sum += relativeLuminance((pixel shr 16 and 0xff) / 255.0, (pixel shr 8 and 0xff) / 255.0, (pixel and 0xff) / 255.0) * alpha
            weight += alpha
        }
        return if (seenThrough && weight > 0) sum / weight else null
    }

    /** `inkLuminance` of the picture, drawn 32 pixels square. */
    fun inkLuminance(bitmap: Bitmap): Double? {
        if (!bitmap.hasAlpha()) return null
        val side = 32
        val small = bitmap.scale(side, side)
        val pixels = IntArray(side * side)
        small.getPixels(pixels, 0, side, 0, 0, side, side)
        return inkLuminance(pixels)
    }

    fun relativeLuminance(red: Double, green: Double, blue: Double): Double {
        fun linear(value: Double) = if (value <= 0.04045) value / 12.92 else ((value + 0.055) / 1.055).pow(2.4)
        return 0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
    }

    /** WCAG's contrast ratio between two relative luminances. */
    fun contrast(one: Double, other: Double): Double = (max(one, other) + 0.05) / (min(one, other) + 0.05)

    /**
     * Whether a picture whose see-through parts draw `ink` lies on the dark checkerboard rather than
     * the light one: when it shows more on the dark one, as what draws in light colours does.
     */
    fun onDark(ink: Double, dark: Double, light: Double): Boolean = contrast(ink, dark) > contrast(ink, light)

    private fun fit(bitmap: Bitmap, maxPixels: Int): Bitmap {
        val longer = max(bitmap.width, bitmap.height)
        if (longer <= maxPixels) return bitmap
        val scale = maxPixels.toFloat() / longer
        return bitmap.scale((bitmap.width * scale).toInt(), (bitmap.height * scale).toInt())
    }

    /** Writes the picture as a JPEG in the cache, for sending. */
    fun writeJPEG(bitmap: Bitmap, folder: File): File? = runCatching {
        folder.mkdirs()
        val file = File(folder, "${UUID.randomUUID()}.jpg")
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.JPEG, 85, it) }
        file
    }.getOrNull()
}
