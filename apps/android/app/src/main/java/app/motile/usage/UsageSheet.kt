package app.motile.usage

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.pulltorefresh.PullToRefreshDefaults
import androidx.compose.material3.pulltorefresh.rememberPullToRefreshState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.motile.LocalCloseSheet
import app.motile.LocalStore
import app.motile.platform.Symbol
import app.motile.theme.LocalPalette
import app.motile.ui.GlassCheckMenu
import app.motile.ui.SheetBar
import app.motile.ui.SheetCloseButton

private val margin = 16.dp

/**
 * What was done, what the agents spent and what is left of their plans, in a sheet: the tabs
 * pinned under its bar, the period at the top of the page, and the servers counted in the bar's
 * menu once there are several. Pulling the page down reads it again.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun UsageSheet() {
    val store = LocalStore.current
    val palette = LocalPalette.current
    val model = store.usage
    val close = LocalCloseSheet.current
    val servers = store.usageServers
    val picksServers = servers.size > 1
    var pulled by remember { mutableStateOf(false) }
    val refreshing = pulled && model.refreshing
    val pull = rememberPullToRefreshState()
    LaunchedEffect(model.refreshing) { if (!model.refreshing) pulled = false }
    Column(Modifier.fillMaxSize()) {
        SheetBar(
            "Usage",
            subtitle = if (picksServers) model.serversLabel(servers) else null,
            leading = { SheetCloseButton(onClose = close) },
            trailing = if (picksServers) {
                { GlassCheckMenu(Symbol.Server, "Servers counted", model.serverChecks(servers)) }
            } else {
                null
            },
        )
        UsageTabs(model, Modifier.padding(start = margin, end = margin, top = 4.dp, bottom = 8.dp), fills = true)
        PullToRefreshBox(
            refreshing,
            onRefresh = {
                model.refresh()
                pulled = model.refreshing
            },
            Modifier.weight(1f),
            state = pull,
            indicator = {
                PullToRefreshDefaults.Indicator(
                    pull, refreshing, Modifier.align(Alignment.TopCenter), containerColor = palette.popover, color = palette.foreground,
                )
            },
        ) {
            UsageContent(model, margin = margin, top = 4.dp, periodInPage = true)
        }
    }
}
