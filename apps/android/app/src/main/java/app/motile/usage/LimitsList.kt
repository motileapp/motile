package app.motile.usage

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.motile.core.LimitResets
import app.motile.core.LimitsReport
import app.motile.core.Loaded
import app.motile.core.UsageModel
import app.motile.platform.Symbol
import app.motile.settings.NOTE_TEXT
import app.motile.settings.ROW_TEXT
import app.motile.settings.RowText
import app.motile.settings.SettingsList
import app.motile.settings.SettingsRow
import app.motile.settings.SettingsRows
import app.motile.theme.LocalPalette
import app.motile.theme.Opacity
import app.motile.theme.iconSize
import app.motile.theme.opacity
import app.motile.ui.AgentIcon
import app.motile.ui.Chip
import app.motile.ui.Text

/** The limits tab as the system's inset groups, as the gateway's accounts are in settings: a group for each account with what it has used and the limit resets it holds. */
@Composable
fun LimitsList(model: UsageModel, modifier: Modifier = Modifier) {
    WatchesGateway()
    val limits = model.limits
    if (limits == Loaded.Loading) return UsageLoading()
    SettingsList(modifier) {
        when (limits) {
            Loaded.Loading -> {}
            is Loaded.Failed -> note(limits.message)
            is Loaded.Ready -> {
                val report = limits.value
                if (report.sections.isEmpty() && report.notes.isEmpty()) note("No agent is installed on these servers.")
                report.sections.forEachIndexed { index, section ->
                    group("limits/$index") {
                        row { SectionRow(section) }
                        limitRows(section.windows, section.resets, section.note) { model.refresh() }
                    }
                }
                if (report.notes.isNotEmpty()) group(footer = report.notes.joinToString("\n")) {}
            }
        }
    }
}

/** An account's windows, the limit resets it holds and why it shows no windows, as rows of a group: the limits tab's and the gateway's accounts' in settings. */
fun SettingsRows.limitRows(
    windows: List<LimitsReport.Window>,
    resets: LimitResets?,
    note: String?,
    dimmed: Boolean = false,
    used: (() -> Unit)? = null,
) {
    for (window in windows) row { Box(Modifier.opacity(Opacity.Disabled, on = dimmed)) { LimitWindowRow(window) } }
    resets?.let { row { ResetsRow(it, used = used) } }
    note?.let { row { SettingsRow { RowText(it, color = LocalPalette.current.mutedForeground) } } }
}

/** An account's agent, plan and whether it is the gateway's, and who is signed in under them. */
@Composable
private fun SectionRow(section: LimitsReport.Section) {
    val palette = LocalPalette.current
    SettingsRow {
        AgentIcon(section.agent, size = iconSize(text = ROW_TEXT))
        Column(Modifier.weight(1f).padding(vertical = 6.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                RowText(section.agent.title, color = palette.foreground, maxLines = 1)
                section.plan?.let { Chip(it, compact = true) }
                if (section.gateway) Chip("Gateway", icon = Symbol.Waypoints, compact = true)
            }
            val account = listOfNotNull(section.name, section.account).joinToString(" · ")
            if (account.isNotEmpty()) {
                Text(account, size = NOTE_TEXT, color = palette.mutedForeground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
            }
        }
    }
}
