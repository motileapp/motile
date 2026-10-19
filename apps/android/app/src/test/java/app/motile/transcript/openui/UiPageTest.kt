package app.motile.transcript.openui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class UiPageTest {
    private val page = UiPage(html = "", base = null, handler = "motile", start = 360f, min = 80f, max = 2400f)

    @Test
    fun aReportedHeightIsKeptWithinThePagesBounds() {
        assertEquals(512f, page.height("""{"type":"iui-html-view-height","height":512}"""))
        assertEquals(80f, page.height("""{"type":"iui-html-view-height","height":3}"""))
        assertEquals(2400f, page.height("""{"type":"iui-html-view-height","height":1e9}"""))
    }

    @Test
    fun anyOtherMessageGivesNoHeight() {
        assertNull(page.height("""{"type":"iui-html-view-close"}"""))
        assertNull(page.height("""{"type":"iui-html-view-height","height":"tall"}"""))
        assertNull(page.height("512"))
        assertNull(page.height("not json"))
    }
}
