package app.motile.gestures

import android.content.Intent
import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.BySelector
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.UiObject2
import androidx.test.uiautomator.Until
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.rules.TestWatcher
import org.junit.runner.Description
import org.junit.Test
import org.junit.runner.RunWith

/**
 * What fingers do, against the dev account (scripts/ui-test.sh starts it): the swipes that show
 * the sidebar and the panel, and the ones that must not. The tests of the iOS app's GestureTests.
 */
@RunWith(AndroidJUnit4::class)
class GestureTests {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val device = UiDevice.getInstance(instrumentation)
    private val packageName = "app.motile.android"

    /** What the screen showed when a test failed: a picture and its views, in /data/local/tmp/motile-failures. */
    @get:Rule
    val failures = object : TestWatcher() {
        override fun failed(error: Throwable?, description: Description) {
            val name = "/data/local/tmp/motile-failures/${description.methodName}"
            device.executeShellCommand("mkdir -p /data/local/tmp/motile-failures")
            device.executeShellCommand("screencap -p $name.png")
            device.executeShellCommand("uiautomator dump $name.xml")
        }
    }

    @Before
    fun launch() {
        val arguments = InstrumentationRegistry.getArguments()
        val intent = Intent(Intent.ACTION_MAIN)
            .setClassName(packageName, "app.motile.MainActivity")
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)
        for (key in listOf("MOTILE_AUTH_URL", "MOTILE_LOCAL", "MOTILE_SERVER_ADDR", "MOTILE_DEMO_SIGN_IN")) {
            arguments.getString(key)?.let { intent.putExtra(key, it) }
        }
        device.executeShellCommand("am force-stop $packageName")
        instrumentation.context.startActivity(intent)
        assertNotNull(device.wait(Until.findObject(By.desc("Threads")), 30_000))
    }

    private val newThread: BySelector get() = By.desc("New thread")

    private fun shown(selector: BySelector, timeout: Long = 3_000): Boolean = device.wait(Until.hasObject(selector), timeout)

    private fun gone(selector: BySelector, timeout: Long = 3_000): Boolean = device.wait(Until.gone(selector), timeout)

    /** The thread's row in the sidebar. */
    private fun row(title: String): UiObject2? {
        assertTrue(shown(newThread))
        return device.wait(Until.findObject(By.descContains(title).clickable(true)), 5_000)
            ?: device.wait(Until.findObject(By.textContains(title)), 5_000)
    }

    /** Where the swipes go across: the height of the top bars, where nothing scrolls sideways, as code and tables do and take the swipe first. */
    private val swipeY: Int get() = device.displayHeight * 9 / 100

    private fun swipeRight() = device.swipe(device.displayWidth / 10, swipeY, device.displayWidth * 9 / 10, swipeY, 20)

    private fun swipeLeft() = device.swipe(device.displayWidth * 9 / 10, swipeY, device.displayWidth / 10, swipeY, 20)

    /** Types in the field that has the keyboard, as its keys would. */
    private fun type(text: String) {
        device.executeShellCommand("input text $text")
    }

    private fun keyboardShown(): Boolean = device.executeShellCommand("dumpsys input_method").contains("mInputShown=true")

    private fun waitForKeyboard(shownNow: Boolean): Boolean {
        repeat(15) {
            if (keyboardShown() == shownNow) return true
            Thread.sleep(200)
        }
        return false
    }

    /** Swipes the sidebar in, again if the client was still loading its threads. */
    private fun openSidebar() {
        repeat(3) {
            swipeRight()
            if (shown(newThread)) return
        }
    }

    @Test
    fun aSwipeToTheRightShowsTheSidebarAndOneBackHidesIt() {
        assertFalse(device.hasObject(newThread))
        swipeRight()
        assertTrue(shown(newThread))
        swipeLeft()
        assertTrue(gone(newThread))
    }

    @Test
    fun aThreadOpensFromTheSidebarAndScrolls() {
        openSidebar()
        val thread = row("Add API Rate Limiting") ?: return fail("no row")
        thread.click()
        assertTrue(gone(newThread))
        // Up and down is the transcript's, and leaves the sidebar where it is.
        device.swipe(device.displayWidth / 2, device.displayHeight / 3, device.displayWidth / 2, device.displayHeight * 2 / 3, 20)
        device.swipe(device.displayWidth / 2, device.displayHeight * 2 / 3, device.displayWidth / 2, device.displayHeight / 3, 20)
        assertFalse(device.hasObject(newThread))
    }

    @Test
    fun aSwipeToTheLeftShowsThePanelAndOneBackHidesItWithoutTheSidebar() {
        swipeLeft()
        assertTrue(shown(By.desc("Back")))
        swipeRight()
        assertTrue(gone(By.desc("Back")))
        assertTrue(shown(By.desc("Threads")))
        assertFalse(device.hasObject(newThread))
    }

    @Test
    fun thePanelsBackButtonHidesIt() {
        device.findObject(By.desc("Files and changes")).click()
        assertTrue(shown(By.desc("Back")))
        device.findObject(By.desc("Back")).click()
        assertTrue(gone(By.desc("Back")))
        assertTrue(shown(By.desc("Threads")))
    }

    @Test
    fun theSystemBackHidesThePanel() {
        device.findObject(By.desc("Files and changes")).click()
        assertTrue(shown(By.desc("Back")))
        device.pressBack()
        assertTrue(gone(By.desc("Back")))
    }

    @Test
    fun aTapOnALinearIssueOpensIt() {
        openSidebar()
        (row("Add API Rate Limiting") ?: return fail("no row")).click()
        device.wait(Until.findObject(By.desc("Files and changes")), 3_000).click()
        assertTrue(shown(By.desc("Back")))
        val linear = device.wait(Until.findObject(By.desc("Linear")), 5_000) ?: device.wait(Until.findObject(By.text("Linear")), 5_000)
        assertNotNull(linear)
        linear.click()
        val issue = device.wait(Until.findObject(By.descContains("ENG-1 Greet by name")), 10_000)
            ?: device.wait(Until.findObject(By.textContains("Greet by name")), 5_000)
        assertNotNull(issue)
        (device.findObject(By.desc("Search")) ?: device.findObject(By.text("Search"))).click()
        assertTrue(waitForKeyboard(true))
        issue.click()
        assertTrue(waitForKeyboard(false))
        assertTrue(shown(By.text("Work on Issue"), 10_000) || shown(By.desc("Work on Issue")))
        device.findObject(By.desc("Back")).click()
    }

    @Test
    fun aSwipeToTheRightOnAThreadInTheSidebarMarksItDone() {
        openSidebar()
        val thread = row("Use an F-String in Greet") ?: return fail("no row")
        // Across the whole row, as a finger does, not only its title.
        val y = thread.visibleBounds.centerY()
        device.swipe(device.displayWidth / 10, y, device.displayWidth * 19 / 20, y, 15)
        val undo = By.descContains("Undo")
        assertTrue(shown(undo) || shown(By.textContains("Undo")))
        // The swipe was the row's, so the sidebar is where it was.
        assertTrue(device.hasObject(newThread))
        (device.findObject(undo) ?: device.findObject(By.textContains("Undo"))).click()
        assertTrue(gone(By.textContains("Undo")))
    }

    @Test
    fun aHoldAndAMoveOnAThreadDragsItAboveAnotherAndTheOrderStays() {
        openSidebar()
        val one = row("Use an F-String in Greet") ?: return fail("no row")
        val other = row("Add API Rate Limiting") ?: return fail("no row")
        val (upper, lower) = if (one.visibleBounds.top < other.visibleBounds.top) one to other else other to one
        val upperTitle = if (upper === one) "Use an F-String in Greet" else "Add API Rate Limiting"
        val lowerTitle = if (upper === one) "Add API Rate Limiting" else "Use an F-String in Greet"
        val from = lower.visibleBounds
        val to = upper.visibleBounds
        lower.drag(android.graphics.Point(to.centerX(), to.top + to.height() / 6), 400)
        assertTrue(ordered(lowerTitle, upperTitle))
        assertTrue(device.hasObject(newThread))
        launch()
        openSidebar()
        assertTrue(ordered(lowerTitle, upperTitle))
        assertTrue(from.height() > 0)
    }

    @Test
    fun aHoldOnAThreadBringsUpItsMenu() {
        openSidebar()
        val thread = row("Screenshot of the Landing Page") ?: return fail("no row")
        thread.longClick()
        val rename = By.text("Rename")
        assertTrue(shown(rename))
        device.findObject(rename).click()
        assertTrue(shown(By.text("Rename thread")))
        device.findObject(By.text("Cancel")).click()
        assertTrue(gone(By.text("Rename thread")))
    }

    /** Whether the sidebar lists the one thread's row over the other's, once both are drawn. */
    private fun ordered(title: String, other: String): Boolean {
        repeat(10) {
            val one = row(title)
            val two = row(other)
            if (one != null && two != null && one.visibleBounds.top < two.visibleBounds.top) return true
            Thread.sleep(1_000)
        }
        return false
    }

    @Test
    fun settingsOpenInASheetWhoseSectionsPushAndCloseTakesThemAway() {
        openSidebar()
        device.findObject(By.desc("Account")).click()
        device.wait(Until.findObject(By.text("Settings")), 3_000).click()
        assertTrue(shown(By.text("Servers")))
        device.findObject(By.text("Servers")).click()
        assertTrue(shown(By.desc("Back")) || shown(By.desc("Settings")))
        device.pressBack()
        assertTrue(shown(By.text("General")))
        device.wait(Until.findObject(By.desc("Close")), 3_000).click()
        assertTrue(gone(By.text("General")))
        assertTrue(shown(newThread))
    }

    @Test
    fun commandsOpenInASheetWhosePagesPushAndCloseTakesThemAway() {
        openSidebar()
        device.findObject(By.desc("Commands")).click()
        assertTrue(shown(By.text("Commands")))
        device.wait(Until.findObject(By.textStartsWith("Add a project")), 3_000).click()
        assertTrue(shown(By.text("Add a Project")))
        device.wait(Until.findObject(By.textStartsWith("New project")), 3_000).click()
        assertTrue(shown(By.text("New Project")))
        assertTrue(waitForKeyboard(true))
        // The sheet rises over the keyboard first; a tap while it still moves can miss the button.
        Thread.sleep(600)
        device.findObject(By.desc("Back")).click()
        if (!shown(By.text("Add a Project"), 1_500)) device.wait(Until.findObject(By.desc("Back")), 1_000)?.click()
        assertTrue(shown(By.text("Add a Project")))
        // The system's back, as iOS's swipe from the edge.
        device.pressBack()
        if (!shown(By.text("Commands"), 1_000)) device.pressBack()
        device.wait(Until.findObject(By.desc("Close")), 3_000).click()
        assertTrue(gone(By.text("Commands")))
        assertTrue(shown(newThread))
    }

    @Test
    fun aTapBesideTheSidebarsSearchPutsTheKeyboardAway() {
        openSidebar()
        searchField().click()
        assertTrue(waitForKeyboard(true))
        device.findObject(By.desc("Motile")).click()
        assertTrue(waitForKeyboard(false))
        assertTrue(device.hasObject(newThread))
    }

    @Test
    fun theSidebarsSearchTakesItsLineUntilItIsClosed() {
        openSidebar()
        searchField().click()
        type("api")
        val close = By.desc("Close search")
        assertTrue(shown(close))
        assertFalse(device.hasObject(newThread))
        device.findObject(close).click()
        assertTrue(waitForKeyboard(false))
        assertTrue(shown(newThread))
    }

    @Test
    fun theSidebarsSearchFindsAThreadByItsMessagesAndOpensIt() {
        openSidebar()
        searchField().click()
        type("handlers")
        val found = device.wait(Until.findObject(By.textContains("reach the handlers")), 5_000) ?: return fail("nothing found")
        found.click()
        assertTrue(gone(newThread))
        openSidebar()
        assertTrue(shown(newThread))
    }

    @Test
    fun aTapBesideTheComposersTextBringsTheKeyboard() {
        val composer = device.wait(Until.findObject(By.desc("Message")), 10_000) ?: device.wait(Until.findObject(By.clazz("android.widget.EditText")), 5_000)
        assertNotNull(composer)
        val bounds = composer.visibleBounds
        device.click(bounds.centerX(), bounds.top - 5)
        assertTrue(waitForKeyboard(true))
    }

    /** A finger dragged on a map pans it: the transcript stays where it is and the sidebar doesn't come in. */
    @Test
    fun aFingerDraggedOnAMapPansItAndLeavesTheTranscript() {
        try {
            openSidebar()
            device.findObject(newThread).click()
            assertTrue(gone(newThread))
            send("show the ui map")
            send("show the ui map")
            val map = lastShown(By.desc("Map")) ?: return fail("no map")
            val title = lastShown(By.text("San Francisco, your way")) ?: return fail("no title")
            val top = title.visibleBounds.top
            val before = lastShown(By.desc("Coit Tower"))?.visibleBounds?.centerY() ?: return fail("no pin")
            val bounds = map.visibleBounds
            val x = bounds.centerX()
            val y = bounds.top + bounds.height() * 2 / 5
            device.swipe(x, y, x, y + bounds.height() / 4, 40)
            Thread.sleep(2_000)
            assertEquals(top, title.visibleBounds.top)
            val after = lastShown(By.desc("Coit Tower"))?.visibleBounds?.centerY() ?: return fail("no pin")
            assertTrue(after > before + bounds.height() / 8)
            device.swipe(bounds.left + bounds.width() / 10, y, bounds.right - bounds.width() / 10, y, 40)
            assertFalse(shown(newThread, 1_000))
        } finally {
            deleteThread("San Francisco, Your Way")
        }
    }

    /**
     * A finger that lands on a program's canvas while the transcript still flings is the
     * transcript's: it stops the transcript there and drags it, and the page never sees it.
     */
    @Test
    fun aDragOnAProgramsCanvasWhileTheTranscriptFlingsScrollsTheTranscript() {
        try {
            openSidebar()
            device.findObject(newThread).click()
            assertTrue(gone(newThread))
            send("show the ui html")
            send("show the ui html")
            val page = lastShown(By.clazz("android.webkit.WebView"))?.visibleBounds ?: return fail("no page")
            val title = lastShown(By.text("Take a little break")) ?: return fail("no title")
            val dp = instrumentation.context.resources.displayMetrics.density
            val top = settled(title)
            val x = page.centerX()
            val start = title.visibleBounds.centerY()
            val fling = (0..10).map { Touch(x, start + (it * 6 * dp).toInt(), it * 15L) }
            drag(fling)
            assertTrue(settled(title) > top + 60 * dp)
            val besideX = page.left - (8 * dp).toInt()
            val besideY = page.bottom + (40 * dp).toInt()
            drag((0..50).map { Touch(besideX, besideY - (it * 10 * dp).toInt(), it * 20L) } + Touch(besideX, besideY - (500 * dp).toInt(), 1_200))
            assertEquals(top, settled(title))
            val canvasY = page.top + (250 * dp).toInt()
            drag(fling)
            drag(listOf(Touch(x, canvasY, 50)) + (1..30).map { Touch(x, canvasY - (it * 13 * dp).toInt(), 50 + it * 20L) } + Touch(x, canvasY - (390 * dp).toInt(), 850))
            assertEquals(top, settled(title))
        } finally {
            deleteThread("Catch the Dot")
        }
    }

    private data class Touch(val x: Int, val y: Int, val at: Long)

    /**
     * Moves a finger along the touches, each where it is and when in milliseconds, lifting it at the
     * last, as the screen reports a finger: UiDevice waits for the client to settle between gestures,
     * so a finger couldn't land while the transcript still flings.
     */
    private fun drag(touches: List<Touch>) {
        val down = SystemClock.uptimeMillis()
        touches.forEachIndexed { index, touch ->
            val action = when (index) {
                0 -> MotionEvent.ACTION_DOWN
                touches.lastIndex -> MotionEvent.ACTION_UP
                else -> MotionEvent.ACTION_MOVE
            }
            SystemClock.sleep((down + touch.at - SystemClock.uptimeMillis()).coerceAtLeast(0))
            val event = MotionEvent.obtain(down + touches.first().at, down + touch.at, action, touch.x.toFloat(), touch.y.toFloat(), 0)
            event.source = InputDevice.SOURCE_TOUCHSCREEN
            instrumentation.uiAutomation.injectInputEvent(event, false)
            event.recycle()
        }
    }

    /** Where the view's top is once the rows around it stop moving. */
    private fun settled(view: UiObject2): Int {
        var top = view.visibleBounds.top
        repeat(10) {
            Thread.sleep(1_000)
            val now = view.visibleBounds.top
            if (now == top) return top
            top = now
        }
        return top
    }

    /** Sends a message and waits for its reply. */
    private fun send(message: String) {
        val composer = device.wait(Until.findObject(By.clazz("android.widget.EditText")), 10_000) ?: return fail("no composer")
        composer.click()
        type(message.replace(" ", "%s"))
        device.findObject(By.desc("Send")).click()
        shown(By.desc("Stop"), 5_000)
        assertTrue(gone(By.desc("Stop"), 30_000))
        device.pressBack()
        waitForKeyboard(false)
        Thread.sleep(2_000)
    }

    /** The last of the views on the screen, once one is. */
    private fun lastShown(selector: BySelector): UiObject2? {
        repeat(30) {
            device.findObjects(selector).lastOrNull { it.visibleBounds.height() > 0 }?.let { return it }
            Thread.sleep(1_000)
        }
        return null
    }

    private fun deleteThread(title: String) {
        openSidebar()
        val thread = row(title) ?: return
        thread.longClick()
        repeat(2) {
            val delete = device.wait(Until.findObject(By.text("Delete")), 3_000) ?: return
            delete.click()
        }
        assertTrue(gone(By.textContains(title), 5_000))
    }

    private fun searchField(): UiObject2 =
        device.wait(Until.findObject(By.desc("Search")), 3_000) ?: device.wait(Until.findObject(By.text("Search")), 3_000)

    private fun fail(message: String) {
        assertEquals("", message)
    }
}
