package app.motile.core

import app.motile.panel.PanelTab
import app.motile.platform.Symbol

/** An issue tracker whose tab the right panel offers once the settings turn it on. */
enum class Integration(val raw: String) {
    Linear("linear"), Ricrac("ricrac");

    val title: String
        get() = when (this) {
            Linear -> "Linear"
            Ricrac -> "Ricrac"
        }

    val summary: String
        get() = when (this) {
            Linear -> "Browse, file and change Linear issues, and hand them to an agent."
            Ricrac -> "Browse, file and change Ricrac issues, and hand them to an agent."
        }

    val symbol: Symbol
        get() = when (this) {
            Linear -> Symbol.Linear
            Ricrac -> Symbol.Ricrac
        }

    val preferencesKey: String get() = "integration.$raw"

    val onByDefault: Boolean get() = this == Linear

    companion object {
        /** The integrations the settings have on. */
        fun kept(): Set<Integration> = entries.filter { Preferences.optionalBool(it.preferencesKey) ?: it.onByDefault }.toSet()
    }
}

/** The integration the tab is of, which hides it while it is off. */
val PanelTab.integration: Integration?
    get() = when (this) {
        PanelTab.Linear, is PanelTab.LinearIssue -> Integration.Linear
        PanelTab.Ricrac, is PanelTab.RicracIssue -> Integration.Ricrac
        else -> null
    }
