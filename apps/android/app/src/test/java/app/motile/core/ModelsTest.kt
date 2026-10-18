package app.motile.core

import app.motile.transcript.RemovedContent
import app.motile.transcript.RowModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ModelsTest {
    private fun thread(id: String, position: Double, doneAt: Double? = null) = ThreadInfo(
        json(
            "id" to id, "server_id" to "s", "title" to id, "project_id" to "p", "cwd" to "/", "agent" to "claude",
            "access" to "full", "created_at" to 1.0, "updated_at" to 1.0, "position" to position, "done_at" to doneAt,
        ),
    )

    @Test
    fun numbersAreAbbreviatedToThreeDigits() {
        assertEquals("820", 820L.abbreviated)
        assertEquals("1.5K", 1500L.abbreviated)
        assertEquals("45.7M", 45_700_000L.abbreviated)
        assertEquals("1M", 999_999L.abbreviated)
        assertEquals("-2K", (-2000L).abbreviated)
    }

    @Test
    fun durationsReadAsShortAsTheyCan() {
        assertEquals("850ms", Time.duration(850))
        assertEquals("12s", Time.duration(12_000))
        assertEquals("3m 5s", Time.duration(185_000))
        assertEquals("1h 2m", Time.duration(3_720_000))
        assertEquals("now", Time.ago(100.0, now = 130.0))
        assertEquals("5m", Time.ago(0.0, now = 300.0))
        assertEquals("2d", Time.ago(0.0, now = 2 * 86400.0 + 5))
    }

    @Test
    fun versionsCompareNumberByNumber() {
        assertTrue(Version.isOlder("0.1.9", "0.1.10"))
        assertFalse(Version.isOlder("0.2.0", "0.1.99"))
        assertFalse(Version.isOlder("1.0", "1.0.0"))
        assertFalse(Version.isOlder("1.0", null))
    }

    @Test
    fun threadsAreListedHighestFirstAndPlacedWhereTheyBelong() {
        val listed = listOf(thread("c", 3.0), thread("b", 2.0), thread("a", 1.0))
        assertEquals(listOf("c", "b", "a"), listed.shuffled().sortedWith(ThreadInfo.order).map { it.id })
        assertEquals(1, listed.place(thread("x", 2.5)))
        assertEquals(3, listed.place(thread("x", 0.5)))
        assertEquals(1, listed.indexOfListed(listed[1]))
        assertNull(listed.indexOfListed(thread("x", 2.5)))
    }

    @Test
    fun aDoneThreadIsListedByWhenItWasDone() {
        val done = thread("d", 1.0, doneAt = 50.0)
        assertTrue(done.isDone)
        assertEquals(50.0, done.listedAt, 0.0)
    }

    @Test
    fun aDraftPreviewIsItsFirstLineOrItsAttachments() {
        assertEquals("Fix the bug", AppStore.preview("\n Fix the bug\nand more", 0))
        assertEquals("2 attachments", AppStore.preview("  ", 2))
        assertEquals("1 attachment", AppStore.preview("", 1))
        assertNull(AppStore.preview("", 0))
    }

    @Test
    fun anAccountsFolderComesFromItsNameAndAvoidsTakenOnes() {
        val taken = AgentAccount.new(Agent.Claude).copy(folder = "~/.claude-work")
        val account = AgentAccount.new(Agent.Claude).copy(name = "Work")
        assertEquals("~/.claude-work-2", account.suggestedFolder(listOf(taken)))
        assertEquals("CLAUDE_CONFIG_DIR=~/.x claude auth login", account.copy(folder = "~/.x").signInCommand)
    }

    @Test
    fun reposRankByTheNameAfterTheOwnerFirst() {
        val repo = Repo("motileapp/motile", "The command center", false)
        assertEquals(0, repo.rank("mot"))
        assertEquals(1, repo.rank("motilea"))
        assertEquals(3, repo.rank("command"))
        assertNull(repo.rank("zzz"))
    }

    @Test
    fun aServerReadsItsInfoAndFallsBackToDefaultAccounts() {
        val server = Server(json("id" to "s", "name" to "studio", "state" to "connected", "info" to mapOf("version" to "0.1.2", "protocol" to 25)))
        assertEquals(Server.State.Connected, server.state)
        assertTrue(server.pullsDefaultBranch)
        assertFalse(server.signsInAccounts)
        assertEquals(listOf("claude", "codex"), server.agentAccounts.map { it.id })
        assertEquals("Default", server.account(null, Agent.Codex)?.name)
        assertFalse(server.removed)
    }

    @Test
    fun aRemovedServerKeepsItsLastNameWithoutInfo() {
        val server = Server(json("id" to "s", "name" to "build-box", "short_name" to "build-box", "removed" to true, "state" to "disconnected", "info" to null))
        assertTrue(server.removed)
        assertEquals("build-box", server.name)
        assertEquals(Server.State.Disconnected, server.state)
        assertFalse(server.known)
    }

    @Test
    fun theLastRowOfARemovedServersThreadSaysWhatTheCoreWrote() {
        val title = "“build-box” is unlinked"
        val message = "This thread is read-only. You can delete “build-box” in settings to remove its threads."
        val row = RowModel.parse(json("id" to "removed", "item" to "removed", "kind" to "removed", "server" to "build-box", "title" to title, "message" to message))
        assertEquals(RemovedContent("build-box", title, message), (row?.kind as? RowModel.Kind.Removed)?.content)
    }

    @Test
    fun aDraftKeepsTheThreadItContinuesAndOneKeptBeforeHasNone() {
        val kept = ThreadDraft.of(json("id" to "D", "projectID" to "p", "access" to "full"))
        assertNull(kept.from)
        val from = ContinuedThread(origin = "gone", threadID = "t", title = "Fix the build")
        val draft = kept.copy(from = from)
        assertEquals(from, ThreadDraft.of(jsonOf(draft.json) as JSON).from)
    }
}
