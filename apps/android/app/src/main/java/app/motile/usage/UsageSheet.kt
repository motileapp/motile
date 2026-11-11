package app.motile.usage

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
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
import app.motile.core.UsageModel
import app.motile.platform.Symbol
import app.motile.theme.IconSize
import app.motile.theme.LocalPalette
import app.motile.theme.TextSize
import app.motile.ui.Check
import app.motile.ui.GlassCheckMenu
import app.motile.ui.GlassPickMenu
import app.motile.ui.glassButton
import app.motile.ui.Icon
import app.motile.ui.SheetBar
import app.motile.ui.SheetCloseButton
import app.motile.ui.Text

private val margin = 16.dp

/**
 * What was done, what the agents spent and what is left of their plans, in a sheet: the tabs
 * in the navigation bar under it, and the period and the servers counted, once there are several,
 * in the bar's menus. Pulling the page down reads it again.
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
    val picksPeriod = model.tab != UsageModel.Tab.Limits
    var pulled by remember { mutableStateOf(false) }
    val refreshing = pulled && model.refreshing
    val pull = rememberPullToRefreshState()
    LaunchedEffect(model.refreshing) { if (!model.refreshing) pulled = false }
    Column(Modifier.fillMaxSize()) {
        SheetBar(
            "Usage",
            subtitle = if (picksServers) model.serversLabel(servers) else null,
            leading = { SheetCloseButton(onClose = close) },
            trailing = if (picksPeriod || picksServers) {
                {
                    Row(Modifier.glassButton(CircleShape), verticalAlignment = Alignment.CenterVertically) {
                        if (picksPeriod) PeriodMenu(model)
                        if (picksServers) GlassCheckMenu(Symbol.Server, "Servers counted", model.serverChecks(servers), bare = true)
                    }
                }
            } else {
                null
            },
        )
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
            UsageContent(model, margin = margin, top = 4.dp)
        }
        UsageTabBar(model)
    }
}

@Composable
private fun PeriodMenu(model: UsageModel) {
    val choices = UsageModel.Period.entries.map { period ->
        Check(period.name, period.label, period == model.period) { model.period = period }
    }
    GlassPickMenu(Symbol.Calendar, "Period", choices, bare = true)
}

/** The tabs in the system's navigation bar, under the page. */
@Composable
private fun UsageTabBar(model: UsageModel) {
    val palette = LocalPalette.current
    val colors = NavigationBarItemDefaults.colors(
        selectedIconColor = palette.foreground,
        selectedTextColor = palette.foreground,
        indicatorColor = palette.segmentSelected,
        unselectedIconColor = palette.mutedForeground,
        unselectedTextColor = palette.mutedForeground,
    )
    NavigationBar(containerColor = palette.background, windowInsets = WindowInsets(0.dp)) {
        UsageModel.Tab.entries.forEach { tab ->
            val selected = model.tab == tab
            NavigationBarItem(
                selected,
                onClick = { model.tab = tab },
                icon = { Icon(tab.symbol, IconSize.Lg, color = LocalContentColor.current) },
                label = { Text(tab.label, size = TextSize.Xs, color = LocalContentColor.current) },
                colors = colors,
            )
        }
    }
}
