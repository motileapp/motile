package app.motile

import android.content.Intent
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

    private fun swipeRight() = device.swipe(device.displayWidth / 10, device.displayHeight / 2, device.displayWidth * 9 / 10, device.displayHeight / 2, 20)

    private fun swipeLeft() = device.swipe(device.displayWidth * 9 / 10, device.displayHeight / 2, device.displayWidth / 10, device.displayHeight / 2, 20)

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
        assertTrue(device.hasObject(By.desc("Threads")))
        assertFalse(device.hasObject(newThread))
    }

    @Test
    fun thePanelsBackButtonHidesIt() {
        device.findObject(By.desc("Files and changes")).click()
        assertTrue(shown(By.desc("Back")))
        device.findObject(By.desc("Back")).click()
        assertTrue(gone(By.desc("Back")))
        assertTrue(device.hasObject(By.desc("Threads")))
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
        device.findObject(By.desc("Files and changes")).click()
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
        val bounds = thread.visibleBounds
        device.swipe(bounds.left + bounds.width() / 10, bounds.centerY(), bounds.right - bounds.width() / 20, bounds.centerY(), 15)
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
        device.findObject(By.desc("Close")).click()
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
        device.pressBack()
        device.pressBack()
        assertTrue(shown(By.text("Add a Project")) || shown(By.text("Commands")))
        device.findObject(By.desc("Close")).click()
        assertTrue(gone(By.text("Commands")))
        assertTrue(shown(newThread))
    }

    @Test
    fun aTapBesideTheSidebarsSearchPutsTheKeyboardAway() {
        openSidebar()
        searchField().click()
        assertTrue(waitForKeyboard(true))
        device.findObject(By.text("Motile")).click()
        assertTrue(waitForKeyboard(false))
        assertTrue(device.hasObject(newThread))
    }

    @Test
    fun theSidebarsSearchTakesItsLineUntilItIsClosed() {
        openSidebar()
        searchField().click()
        searchField().text = "api"
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
        searchField().text = "handlers"
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

    private fun searchField(): UiObject2 =
        device.wait(Until.findObject(By.desc("Search")), 3_000) ?: device.wait(Until.findObject(By.text("Search")), 3_000)

    private fun fail(message: String) {
        assertEquals("", message)
    }
}
