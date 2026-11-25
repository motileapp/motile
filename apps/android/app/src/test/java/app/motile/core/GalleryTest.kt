package app.motile.core

import app.motile.panel.MediaGrid
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class GalleryTest {
    private fun shown(media: String, row: String, video: Boolean = false, poster: String? = null, width: Int? = null, height: Int? = null) = mapOf(
        "media" to media, "row" to row, "item" to "item-$row", "name" to "$media.png", "video" to video.takeIf { it },
        "poster" to poster, "width" to width, "height" to height, "size" to 1024, "at" to 1700000000.5,
    )

    @Test
    fun theGalleryEventReadsAsTheThreadsImagesAndVideosWithWhereEachIsShown() {
        val event = json(
            "thread_id" to "t1",
            "media" to listOf(
                shown("clip", "r2", video = true, poster = "clip-poster", width = 1280, height = 720),
                shown("shot", "r1", width = 800, height = 600),
                shown("shot", "r1"),
            ),
        )
        val gallery = Gallery.of(event)
        assertEquals("t1", gallery.threadID)
        val (clip, shot, again) = gallery.media
        assertTrue(clip.video)
        assertEquals("clip-poster", clip.picture)
        assertEquals(1280f to 720f, clip.pixels)
        assertFalse(shot.video)
        assertEquals("shot", shot.picture)
        assertEquals(ShownIn("item-r1", "r1"), shot.shownIn)
        assertEquals(1700000000.5, shot.at, 0.0)
        assertNull(again.pixels)
        assertEquals(3, gallery.media.map { it.id }.toSet().size)
        val viewed = clip.viewed
        assertEquals(ViewedMedia.Source.Media("clip"), viewed.source)
        assertEquals(ShownIn("item-r2", "r2"), viewed.shownIn)
    }

    @Test
    fun aVideoWithoutAPosterHasNoPicture() {
        val gallery = Gallery.of(json("thread_id" to "t1", "media" to listOf(shown("clip", "r1", video = true))))
        assertNull(gallery.media.single().picture)
    }

    @Test
    fun theSummaryCountsImagesAndVideosLeavingOutWhatThereIsNoneOf() {
        fun summary(vararg videos: Boolean) =
            Gallery.of(json("thread_id" to "t", "media" to videos.mapIndexed { index, video -> shown("m$index", "r$index", video) })).summary
        assertEquals("1 image", summary(false))
        assertEquals("1 video", summary(true))
        assertEquals("2 images · 1 video", summary(false, true, false))
        assertEquals("3 videos", summary(true, true, true))
    }

    @Test
    fun theGridHasAColumnForEvery128PointsAndNeverFewerThanThree() {
        assertEquals(3, MediaGrid.columns(200f))
        assertEquals(3, MediaGrid.columns(511f))
        assertEquals(4, MediaGrid.columns(512f))
        assertEquals(7, MediaGrid.columns(900f))
    }

    @Test
    fun aPictureIsDecodedForItsShorterSideToFillTheSquareInStepsOf64Points() {
        assertEquals(256, MediaGrid.decodedPixels(100f, 2f, 800f to 800f))
        assertEquals(256, MediaGrid.decodedPixels(128f, 2f, 800f to 800f))
        assertEquals(512, MediaGrid.decodedPixels(100f, 2f, 1600f to 800f))
        assertEquals(1024, MediaGrid.decodedPixels(100f, 2f, 100f to 4000f))
        assertEquals(512, MediaGrid.decodedPixels(100f, 2f, null))
    }
}
