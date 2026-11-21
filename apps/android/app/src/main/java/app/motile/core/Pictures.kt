package app.motile.core

import androidx.core.graphics.scale
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.media.MediaMetadataRetriever
import app.motile.platform.Platform
import coil3.ImageLoader
import coil3.executeBlocking
import coil3.request.CachePolicy
import coil3.request.ImageRequest
import coil3.request.SuccessResult
import coil3.request.allowHardware
import coil3.svg.SvgDecoder
import coil3.toBitmap
import java.io.File
import java.util.UUID
import kotlin.math.max

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

    /** The SVG drawn `side` pixels on its longer side, in the shape it asks for. */
    private fun drawSVG(file: File, side: Int): Bitmap? {
        val request = ImageRequest.Builder(Platform.context)
            .data(file)
            .size(side)
            .allowHardware(false)
            .memoryCachePolicy(CachePolicy.DISABLED)
            .build()
        return (svgLoader.executeBlocking(request) as? SuccessResult)?.image?.toBitmap()
    }

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
