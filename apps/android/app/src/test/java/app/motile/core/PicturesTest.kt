package app.motile.core

import app.motile.transcript.MediaContent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PicturesTest {
    private val black = 0xff000000.toInt()
    private val white = 0xffffffff.toInt()
    private val clear = 0x00000000

    @Test
    fun onlyAPictureWithSeeThroughPartsHasInk() {
        assertNull(Pictures.inkLuminance(intArrayOf(black, white)))
        assertEquals(0.0, Pictures.inkLuminance(intArrayOf(black, clear))!!, 1e-9)
        assertEquals(1.0, Pictures.inkLuminance(intArrayOf(white, clear, clear))!!, 1e-9)
        assertNull(Pictures.inkLuminance(intArrayOf(clear, clear)))
    }

    @Test
    fun lightInkLiesOnTheDarkCheckerboardAndAnyOtherOnTheLightOne() {
        val (dark, light) = 0.012 to 0.89
        assertTrue(Pictures.onDark(1.0, dark, light))
        assertTrue(Pictures.onDark(0.63, dark, light))
        assertFalse(Pictures.onDark(0.0, dark, light))
        assertFalse(Pictures.onDark(0.1, dark, light))
    }

    @Test
    fun anSvgThatSaysItsOwnColourKeepsIt() {
        assertFalse(Pictures.rootHasColor("""<svg xmlns="http://www.w3.org/2000/svg" stroke="currentColor"><path fill-color="red"/></svg>"""))
        assertTrue(Pictures.rootHasColor("""<?xml version="1.0"?><svg viewBox="0 0 24 24" color="#fff">"""))
        assertTrue(Pictures.rootHasColor("""<svg style="color: red">"""))
    }

    @Test
    fun aSmallPictureLiesInTheMiddleOfABoxItsTileCanBeSeenIn() {
        val icon = MediaContent(json("media" to "icon.svg", "width" to 64, "height" to 64))
        assertEquals(96f to 96f, icon.box(600f))
        assertEquals(64f to 64f, icon.drawn(600f))
        val wide = MediaContent(json("media" to "wide.png", "width" to 1200, "height" to 40))
        assertEquals(600f to 96f, wide.box(600f))
        assertEquals(600f to 20f, wide.drawn(600f))
    }
}
