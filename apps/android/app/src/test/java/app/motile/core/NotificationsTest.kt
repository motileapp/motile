package app.motile.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class NotificationsTest {
    private fun item(id: String, state: WorkState, since: Double) = WorkItem(id, "Title $id", "motile", state, since)

    private fun server(id: String, at: Double, items: List<WorkItem>, working: Int = 0, waiting: Int = 0) =
        ServerWork(id, "server $id", items, working, waiting, at)

    @Test
    fun theSettingsReadAndWriteTheCoresJson() {
        val settings = NotificationSettings.of(
            json("mode" to "off", "toasts" to false, "choices" to json("finished" to false, "question" to false), "live" to false),
        )
        assertEquals(NotificationMode.Off, settings.mode)
        assertFalse(settings.notifies)
        assertEquals(AlertChoices(finished = false, question = false), settings.choices)
        assertEquals(settings, NotificationSettings.of(settings.json))
        assertEquals(NotificationSettings(), NotificationSettings.of(EmptyJSON))
    }

    @Test
    fun eachSwitchChangesItsOwnChoice() {
        val (_, approval) = AlertChoices.rows.first { it.first == "Needs approval" }
        val choices = approval.with(AlertChoices(), false)
        assertEquals(AlertChoices(approval = false), choices)
        assertFalse(approval.isOn(choices))
        assertEquals(5, AlertChoices.rows.size)
    }

    @Test
    fun eachServersLatestWorkIsKeptAndWhatWentQuietGoes() {
        val now = 100_000.0
        val kept = listOf(server("a", now - 60, emptyList()), server("b", now - 30, emptyList()), server("old", now - 3 * 60 * 60, emptyList()))
        val heard = listOf(server("a", now, listOf(item("t1", WorkState.Working, now))), server("b", now - 90, listOf(item("t2", WorkState.Working, now))))
        val merged = ServerWork.merge(kept, heard, now)
        assertEquals(listOf("a", "b"), merged.map { it.serverID })
        assertEquals(1, merged[0].items.size)
        assertEquals(now - 30, merged[1].at, 0.0)
    }

    @Test
    fun theAgentsOfEveryServerListWhatWaitsFirstAndCountBeyondTheirItems() {
        val a = server("a", 10.0, listOf(item("w1", WorkState.Working, 5.0), item("e1", WorkState.Finished, 9.0)), working = 7)
        val b = server("b", 20.0, listOf(item("q1", WorkState.Waiting, 1.0), item("w2", WorkState.Working, 8.0), item("w1", WorkState.Working, 8.0)), working = 1, waiting = 1)
        val work = AgentsWork.of(listOf(a, b))
        assertEquals(listOf("q1", "w2", "w1", "e1"), work.items.map { it.threadID })
        assertEquals("b", work.items[2].serverID)
        assertEquals(8, work.working)
        assertEquals(9, work.active)
        assertEquals("8 working · 1 needs you", work.summary)
        assertEquals("No agents working", AgentsWork.of(emptyList()).summary)
        assertEquals("2 need you", AgentsWork(emptyList(), 0, 2).summary)
        assertEquals(listOf("No agents working"), AgentsWork.of(emptyList()).summaryParts)
    }

    @Test
    fun whatWorksGoesStaleWithoutWordFromItsServer() {
        val work = AgentsWork.of(listOf(server("a", 1000.0, listOf(item("w", WorkState.Working, 0.0), item("e", WorkState.Finished, 0.0)))))
        val later = 1000.0 + AgentsWork.STALE_AFTER + 1
        assertTrue(work.items.first { it.threadID == "w" }.isStale(later))
        assertFalse(work.items.first { it.threadID == "w" }.isStale(1000.0))
        assertFalse(work.items.first { it.threadID == "e" }.isStale(later))
        assertEquals(listOf("w", "e"), work.shown(0.0 + AgentsWork.ENDED_STAYS - 1).map { it.threadID })
        assertEquals(listOf("w"), work.shown(AgentsWork.ENDED_STAYS + 1).map { it.threadID })
    }

    @Test
    fun aPushOpensIntoAnAlertOrWork() {
        val alert = Pushed.of(
            """{"type":"alert","id":"a1","server_id":"s1","thread_id":"t1","kind":"plan","thread_title":"Fix","project":"motile","body":"Has a plan for you","at":1.0}""",
        )
        assertEquals(Pushed.Alert(PushedAlert("a1", "s1", "t1", AlertKind.Plan, "Fix", "motile", "Has a plan for you")), alert)
        val work = Pushed.of("""{"type":"work","server_id":"s1","server_name":"studio","items":[],"at":2.0}""") as Pushed.Work
        assertEquals(0, work.work.working)
        assertNull(Pushed.of(""))
        assertNull(Pushed.of("""{"type":"other"}"""))
    }

    @Test
    fun aWorkPushCarriesItsSealedPartsAsText() {
        assertEquals(listOf("one", "two"), sealedParts("""["one","two"]"""))
        assertEquals(emptyList<String>(), sealedParts(null))
        assertEquals(emptyList<String>(), sealedParts("nope"))
    }

    @Test
    fun aThreadsLinkReadsBack() {
        val link = ThreadLink("server key", "thread/1")
        assertEquals(link, ThreadLink.of(link.url))
        assertNull(ThreadLink.of("motile://auth?code=1"))
        assertNull(ThreadLink.of("motile://thread?server=a"))
    }

    @Test
    fun anAlertThatCantBeOpenedSaysWhatKindItIs() {
        assertEquals("An agent needs you", AlertKind.of("question").placeholder)
        assertEquals("An agent reached its usage limit", AlertKind.of("limit").placeholder)
        assertEquals(AlertKind.Other, AlertKind.of("new"))
    }
}
