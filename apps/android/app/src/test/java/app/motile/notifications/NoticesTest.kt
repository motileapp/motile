package app.motile.notifications

import app.motile.core.AgentsWork
import app.motile.core.AlertKind
import app.motile.core.PushedAlert
import app.motile.core.ServerWork
import app.motile.core.ThreadLink
import app.motile.core.WorkItem
import app.motile.core.WorkState
import org.junit.Assert.assertEquals
import org.junit.Test

class NoticesTest {
    @Test
    fun anAlertShowsItsThreadProjectAndBodyOnTheChannelOfItsKind() {
        val alert = PushedAlert("a1", "s1", "t1", AlertKind.Approval, "Fix the login", "motile", "Needs your approval · Run greet.py")
        assertEquals(
            AlertNotice(ThreadLink("s1", "t1"), "Fix the login", "motile", "Needs your approval · Run greet.py", AlertNotifications.NEEDS_YOU),
            AlertNotice.of(alert),
        )
        assertEquals(null, AlertNotice.of(alert.copy(project = "")).subtitle)
        assertEquals(AlertNotifications.UPDATES, AlertNotice.of(alert.copy(kind = AlertKind.Finished)).channel)
        assertEquals(AlertNotifications.NEEDS_YOU, AlertNotice.of(alert.copy(kind = AlertKind.Plan)).channel)
        assertEquals(AlertNotifications.UPDATES, AlertNotice.of(alert.copy(kind = AlertKind.Limit)).channel)
    }

    @Test
    fun anAlertThatCantBeOpenedStillOpensItsThread() {
        val notice = AlertNotice.placeholder("t1", AlertKind.Failed)
        assertEquals("t1", notice.link.threadID)
        assertEquals("Motile", notice.title)
        assertEquals("An agent failed", notice.body)
        assertEquals(AlertNotifications.UPDATES, notice.channel)
    }

    @Test
    fun theOngoingNotificationListsTheFirstThreeWhatWaitsFirst() {
        val items = listOf(
            WorkItem("w1", "Build", "motile", WorkState.Working, 4.0),
            WorkItem("f1", "Docs", "motile", WorkState.Failed, 5.0),
            WorkItem("q1", "Login", "motile", WorkState.Waiting, 1.0),
            WorkItem("w2", "Tests", "motile", WorkState.Working, 3.0),
            WorkItem("e1", "Old", "motile", WorkState.Finished, 0.0),
        )
        val work = AgentsWork.of(listOf(ServerWork("s", "studio", items, 2, 1, 10.0)))
        assertEquals(listOf("Login · needs you", "Build · working", "Tests · working"), OngoingWork.lines(work, 10.0))
        assertEquals(listOf("Login · needs you", "Build · working", "Tests · working"), OngoingWork.lines(work, 10_000.0))
        val ended = AgentsWork.of(listOf(ServerWork("s", "studio", items.filter { !it.state.isActive }, 0, 0, 10.0)))
        assertEquals(listOf("Docs · failed", "Old · finished"), OngoingWork.lines(ended, 10.0))
        assertEquals(emptyList<String>(), OngoingWork.lines(ended, 10_000.0))
    }
}
