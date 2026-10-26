package app.motile.panel

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.motile.core.LinearRow
import app.motile.core.LinearState
import app.motile.core.LinearUser
import app.motile.core.Time
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.iconSize
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.Radius
import app.motile.ui.ActionButton
import app.motile.ui.Chip
import app.motile.ui.Choice
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.MenuPopup
import app.motile.ui.Text
import app.motile.ui.pressPosition
import app.motile.ui.pressable
import app.motile.ui.rememberMenuState

/** A line of the Linear list: a status, or an issue under it with the statuses its team has. */
sealed interface LinearListEntry {
    val key: String

    data class Header(val state: LinearState, val count: Int, val folded: Boolean) : LinearListEntry {
        override val key: String get() = "state:${state.id}"
    }

    data class Issue(val row: LinearRow, val state: LinearState, val states: List<LinearState>) : LinearListEntry {
        override val key: String get() = "issue:${row.id}"
    }
}

/** The space between the rows' lights, and around each one: it looks empty but is the row's. */
private val rowGap = 2.dp

/**
 * The issues under their statuses, in a list that only builds the rows on screen. A tap on an issue
 * opens it, on its priority or status offers the others, and on a status folds its issues up and
 * out. Holding an issue offers what else there is to do with it.
 */
@Composable
fun LinearList(
    entries: List<LinearListEntry>,
    /** The issues whose status, priority or assignee is being changed. */
    pending: Set<String>,
    /** The panel is too narrow for the labels beside the titles. */
    narrow: Boolean,
    users: List<LinearUser>,
    offer: (List<Choice>) -> Unit,
    open: (LinearRow) -> Unit,
    fold: (String) -> Unit,
    change: (LinearRow, Map<String, Any?>) -> Unit,
) {
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(start = 8.dp, end = 8.dp, top = 8.dp, bottom = panelEndRoom)) {
        items(entries, key = { it.key }, contentType = { it::class }) { entry ->
            when (entry) {
                is LinearListEntry.Header -> HeaderRow(entry) { fold(entry.state.id) }
                is LinearListEntry.Issue -> IssueRow(entry, entry.row.id in pending, narrow, users, offer, open, change)
            }
        }
    }
}

/** A status over its issues, on the secondary background, which a tap folds up and out. */
@Composable
private fun HeaderRow(entry: LinearListEntry.Header, fold: () -> Unit) {
    val palette = LocalPalette.current
    val shape = RoundedCornerShape(Radius.md)
    Box(Modifier.padding(top = 6.dp + rowGap / 2, bottom = rowGap / 2)) {
        HighlightButton(
            onClick = fold,
            modifier = Modifier.fillMaxWidth().background(palette.backgroundSecondary, shape).semantics { contentDescription = "${entry.state.name}, ${entry.count}" },
            radius = Radius.md,
            fill = palette.backgroundSecondaryAccent,
        ) {
            Row(Modifier.height(pressable(30f)).padding(horizontal = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(entry.state.symbol, iconSize(text = 12f), color = entry.state.color)
                Spacer(Modifier.width(8.dp))
                Text(entry.state.name, size = 12f, weight = FontWeight.SemiBold, color = palette.foreground, maxLines = 1)
                Spacer(Modifier.width(8.dp))
                Text("${entry.count}", size = 12f, color = palette.mutedStrongerForeground, maxLines = 1)
                Spacer(Modifier.weight(1f))
                Icon(if (entry.folded) Symbol.ChevronRight else Symbol.ChevronDown, iconSize(text = 12f), color = palette.mutedStrongerForeground)
            }
        }
    }
}

/**
 * An issue on one line as Linear lists it: its priority and its status, which a tap changes, its
 * identifier and title, then its labels, who has it and when it last changed.
 */
@Composable
private fun IssueRow(
    entry: LinearListEntry.Issue,
    pending: Boolean,
    narrow: Boolean,
    users: List<LinearUser>,
    offer: (List<Choice>) -> Unit,
    open: (LinearRow) -> Unit,
    change: (LinearRow, Map<String, Any?>) -> Unit,
) {
    val palette = LocalPalette.current
    val row = entry.row
    val menu = rememberMenuState()
    Box(Modifier.padding(vertical = rowGap / 2)) {
        HighlightButton(
            onClick = {
                Platform.endEditing()
                open(row)
            },
            modifier = Modifier.fillMaxWidth().pressPosition(menu).semantics { contentDescription = "${row.identifier} ${row.title}" },
            radius = Radius.md,
            onLongClick = { menu.openAtPress() },
        ) {
            Row(
                Modifier.height(pressable(34f)).padding(start = 6.dp, end = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                ActionButton(
                    icon = row.prioritySymbol, help = row.priorityLabel, size = ControlSize.Small,
                    tint = if (row.priority == 1) palette.warning else null,
                ) {
                    offer(LinearRow.priorities.map { (value, name) -> Choice(name, LinearRow.symbol(value), value == row.priority) { change(row, mapOf("priority" to value)) } })
                }
                ActionButton(icon = entry.state.symbol, help = "Status: ${entry.state.name}", size = ControlSize.Small, tint = entry.state.color, pending = pending) {
                    if (pending) return@ActionButton
                    offer(entry.states.map { status -> Choice(status.name, status.symbol, status.id == entry.state.id) { change(row, mapOf("state" to status.id)) } })
                }
                Spacer(Modifier.width(4.dp))
                Text(row.identifier, size = 11.5f, color = palette.mutedStrongerForeground, maxLines = 1)
                Spacer(Modifier.width(4.dp))
                Text(row.title, Modifier.weight(1f), size = 13f, color = palette.foreground, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (!narrow) {
                    for (label in row.labels.take(2)) Chip(label.name, Modifier.padding(start = 4.dp), dot = label.color)
                }
                row.initials?.let { initials ->
                    Box(Modifier.padding(start = 4.dp)) { LinearInitials(initials, row.assignee ?: "") }
                }
                Text(Time.ago(row.updatedAt), Modifier.padding(start = 4.dp), size = 11.5f, color = palette.mutedStrongerForeground, maxLines = 1)
            }
        }
        MenuPopup(menu) {
            item("Assign To", Symbol.CircleUser) { offer(assignees(users, row, change)) }
            row.url?.let { url ->
                item("Open in Linear", Symbol.SquareArrowOutUpRight) { Platform.open(url) }
                item("Copy Link", Symbol.Link) { Platform.copy(url) }
            }
            item("Copy Identifier", Symbol.Copy) { Platform.copy(row.identifier) }
        }
    }
}

private fun assignees(users: List<LinearUser>, row: LinearRow, change: (LinearRow, Map<String, Any?>) -> Unit): List<Choice> {
    val nobody = Choice("Unassigned", chosen = row.assigneeID == null) { change(row, mapOf("assignee" to "")) }
    return listOf(nobody) + users.map { user ->
        Choice(if (user.me) "${user.name} (you)" else user.name, chosen = user.id == row.assigneeID) { change(row, mapOf("assignee" to user.id)) }
    }
}
