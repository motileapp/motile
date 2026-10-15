package app.motile.core

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** Whether the client follows the system's light or dark appearance, or keeps one of them. */
enum class Appearance(val raw: String) {
    System("system"), Light("light"), Dark("dark");

    val label: String
        get() = when (this) {
            System -> "System"
            Light -> "Light"
            Dark -> "Dark"
        }

    companion object {
        private const val KEY = "appearance"

        fun of(raw: String?): Appearance = entries.firstOrNull { it.raw == raw } ?: System

        /** The one chosen, which the root view follows. */
        var chosen by mutableStateOf(System)
            private set

        fun load() {
            chosen = of(Preferences.string(KEY))
        }

        fun choose(appearance: Appearance) {
            chosen = appearance
            Preferences.set(KEY, appearance.raw)
        }
    }
}
