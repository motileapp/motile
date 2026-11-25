package app.motile.core

import android.os.Handler
import android.os.Looper
import app.motile.composer.PickedFile
import app.motile.platform.Platform
import kotlin.concurrent.thread

/** An image or a video of the open thread, as its Media tab shows it. */
data class GalleryMedia(
    /** Its place in the list as well: a message can show the same file twice. */
    val id: String,
    val media: String,
    val shownIn: ShownIn,
    val name: String,
    val video: Boolean,
    /** The image that stands for a video until it plays. */
    val poster: String?,
    val pixels: Pair<Float, Float>?,
    val at: Double,
) {
    /** The image its square shows: the image itself, or a video's poster. */
    val picture: String? get() = if (video) poster else media

    val viewed: ViewedMedia get() = ViewedMedia(name, video, ViewedMedia.Source.Media(media), pixels = pixels, shownIn = shownIn)

    companion object {
        fun of(json: JSON, index: Int): GalleryMedia {
            val shownIn = ShownIn(json.string("item"), json.string("row"))
            val media = json.string("media")
            val width = json.double("width").toFloat()
            val height = json.double("height").toFloat()
            return GalleryMedia(
                id = "${shownIn.row}/$media/$index",
                media = media,
                shownIn = shownIn,
                name = json.string("name"),
                video = json.bool("video"),
                poster = json.optionalString("poster"),
                pixels = if (width > 0 && height > 0) width to height else null,
                at = json.double("at"),
            )
        }
    }
}

/** The images and videos of one thread. */
data class Gallery(val threadID: String, val media: List<GalleryMedia>) {
    /** "3 images · 1 video", leaving out what there is none of. */
    val summary: String
        get() {
            val videos = media.count { it.video }
            return listOf(media.size - videos to "image", videos to "video")
                .filter { it.first > 0 }
                .joinToString(" · ") { (count, kind) -> "$count $kind${if (count == 1) "" else "s"}" }
        }

    companion object {
        fun of(event: JSON) = Gallery(event.string("thread_id"), event.objects("media").mapIndexed { index, json -> GalleryMedia.of(json, index) })
    }
}

internal fun AppStore.takeGallery(event: JSON): () -> Unit {
    val gallery = Gallery.of(event)
    return {
        if (transcript.threadID == gallery.threadID) this.gallery = gallery
    }
}

/** Has the core send the thread's images and videos, and again whenever one is added. */
fun AppStore.watchGallery(threadID: String, watch: Boolean) {
    if (!watch && gallery?.threadID == threadID) gallery = null
    core.send("watch_gallery", json("thread_id" to threadID, "watch" to watch))
}

/** Scrolls the open thread to where it shows the image or the video, out from behind the viewer and the panel. */
fun AppStore.showInChat(shownIn: ShownIn) {
    val threadID = transcript.threadID ?: return
    uncoverThread()
    core.send("reveal_row", json("thread_id" to threadID, "item_id" to shownIn.item, "row_id" to shownIn.row))
}

/** Puts the image or the video in the composer, under its name. */
fun AppStore.addToMessage(media: String, name: String) {
    fetchMedia(media) { fetched ->
        val file = fetched.getOrNull() ?: return@fetchMedia
        thread(isDaemon = true) {
            val copy = PickedFile.copy(file, name) ?: file
            Handler(Looper.getMainLooper()).post {
                uncoverThread()
                attach(listOf(copy))
            }
        }
    }
}

/** Closes the viewer, and the panel where it covers the thread. */
fun AppStore.uncoverThread() {
    if (viewing != null) closeViewer()
    if (rightPanel.isMaximized) rightPanel.toggleMaximized()
    if (Platform.panelCoversThread) rightPanel.isOpen = false
}
