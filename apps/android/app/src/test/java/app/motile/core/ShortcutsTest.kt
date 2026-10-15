package app.motile.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ShortcutsTest {
    private fun rule(command: String, key: String, control: Boolean = true, whenName: String? = null) = mapOf(
        "command" to command,
        "shortcut" to mapOf("key" to key, "command" to false, "control" to control, "option" to false, "shift" to false),
        "when" to whenName?.let { mapOf("type" to "name", "name" to it) },
        "caps" to listOf("Ctrl", key.uppercase()),
    )

    private val shortcuts = Shortcuts(
        json(
            "rules" to listOf(
                rule("commandPalette.toggle", "k"),
                rule("thread.stop", "k", whenName = "turnRunning"),
                rule("chat.new", "n"),
            ),
        ),
    )

    @Test
    fun theLastRuleWhoseConditionHoldsWins() {
        assertEquals("commandPalette.toggle", shortcuts.command(setOf("k"), false, true, false, false, emptySet()))
        assertEquals("thread.stop", shortcuts.command(setOf("k"), false, true, false, false, setOf("turnRunning")))
        assertNull(shortcuts.command(setOf("k"), false, false, false, false, emptySet()))
    }

    @Test
    fun aKeyTakenByALaterRuleIsNotTheEarliersWhereItHolds() {
        assertEquals(emptyList<ShortcutRule>(), shortcuts.effective("commandPalette.toggle", setOf("turnRunning")))
        assertEquals("Ctrl+K", shortcuts.label("commandPalette.toggle"))
        assertEquals("New thread (Ctrl+N)", shortcuts.help("New thread", "chat.new"))
    }
}
