package app.motile.panel

import app.motile.panel.PanelTab.Agents
import app.motile.panel.PanelTab.Diff
import app.motile.panel.PanelTab.Files
import app.motile.panel.PanelTab.Linear
import app.motile.panel.PanelTab.PullRequest
import app.motile.panel.PanelTab.PullRequests
import app.motile.panel.PanelTab.Ricrac
import org.junit.Assert.assertEquals
import org.junit.Test

class RightPanelTest {
    private val a = PanelTab.File("a.txt")
    private val b = PanelTab.File("b.txt")
    private val c = PanelTab.File("c.txt")

    @Test
    fun aMovedTabTakesItsPlaceAmongTheOthersAndTheActiveOneStays() {
        val tabs = PanelTabs(tabs = listOf(a, b, c), active = b)
        assertEquals(PanelTabs(tabs = listOf(b, c, a), active = b), tabs.moving(a, 2))
        assertEquals(PanelTabs(tabs = listOf(c, a, b), active = b), tabs.moving(c, 0))
        assertEquals(listOf(a, c, b), tabs.moving(c, 1).tabs)
        assertEquals(listOf(b, c, a), tabs.moving(a, 9).tabs)
        assertEquals(tabs, tabs.moving(PanelTab.Diff, 0))
    }

    @Test
    fun theLauncherOrderDropsUnknownIdsAndPutsMissingOnesAfterTheOnesTheyFollow() {
        assertEquals(LauncherOrder.default, LauncherOrder.of(emptyList()))
        assertEquals(
            listOf(Ricrac, Diff, Agents, Files, PullRequest, PullRequests, Linear),
            LauncherOrder.of(listOf("ricrac", "gone", "diff", "agents", "pull_request", "diff")),
        )
        assertEquals(
            listOf(Agents, Ricrac, Linear, Files, Diff, PullRequest, PullRequests),
            LauncherOrder.of(listOf("ricrac", "linear", "files", "diff", "pull_request", "pull_requests")),
        )
    }

    @Test
    fun aLauncherRowMovesAmongTheShownOnesAndTheHiddenOnesKeepTheirPlaces() {
        val order = LauncherOrder.default
        val shown = listOf(Agents, Files, Diff, PullRequest, Ricrac)
        // Before the shown row it lands before.
        assertEquals(listOf(Files, Diff, Agents, PullRequest, PullRequests, Linear, Ricrac), LauncherOrder.moving(order, Agents, 2, shown))
        assertEquals(listOf(Agents, Files, Diff, Ricrac, PullRequest, PullRequests, Linear), LauncherOrder.moving(order, Ricrac, 3, shown))
        // After the last shown row: past the hidden rows before it, ahead of the ones after it.
        assertEquals(listOf(Agents, Files, PullRequest, PullRequests, Linear, Ricrac, Diff), LauncherOrder.moving(order, Diff, 4, shown))
        assertEquals(
            listOf(Files, Diff, PullRequest, Agents, PullRequests, Linear, Ricrac),
            LauncherOrder.moving(order, Agents, 3, listOf(Agents, Files, Diff, PullRequest)),
        )
        assertEquals(order, LauncherOrder.moving(order, Files, 1, shown))
    }
}
