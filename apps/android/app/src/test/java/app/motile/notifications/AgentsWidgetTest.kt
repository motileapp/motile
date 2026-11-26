package app.motile.notifications

import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Test

class AgentsWidgetTest {
    @Test
    fun listsOnlyTheThreadsItsHeightHolds() {
        assertEquals(0, rowsThatFit(60.dp))
        assertEquals(2, rowsThatFit(110.dp))
        assertEquals(5, rowsThatFit(190.dp))
    }
}
