package app.motile.core

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.MediaMetadataRetriever
import java.io.File
import java.util.UUID
import kotlin.math.max

/** Decodes pictures off the main thread: an image's, or a video's first frame. */
object Pictures {
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

    /** The image, no larger than `maxPixels` on its longer side. */
    fun decode(file: File, maxPixels: Int): Bitmap? = runCatching {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.path, bounds)
        var sample = 1
        while (max(bounds.outWidth, bounds.outHeight) / (sample * 2) >= maxPixels) sample *= 2
        val bitmap = BitmapFactory.decodeFile(file.path, BitmapFactory.Options().apply { inSampleSize = sample }) ?: return null
        fit(bitmap, maxPixels)
    }.getOrNull()

    private fun fit(bitmap: Bitmap, maxPixels: Int): Bitmap {
        val longer = max(bitmap.width, bitmap.height)
        if (longer <= maxPixels) return bitmap
        val scale = maxPixels.toFloat() / longer
        return Bitmap.createScaledBitmap(bitmap, (bitmap.width * scale).toInt(), (bitmap.height * scale).toInt(), true)
    }

    /** Writes the picture as a JPEG in the cache, for sending. */
    fun writeJPEG(bitmap: Bitmap, folder: File): File? = runCatching {
        folder.mkdirs()
        val file = File(folder, "${UUID.randomUUID()}.jpg")
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.JPEG, 85, it) }
        file
    }.getOrNull()
}
