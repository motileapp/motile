package app.motile.usage

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.motile.LocalCloseSheet
import app.motile.LocalStore
import app.motile.core.UsageModel
import app.motile.platform.Symbol
import app.motile.ui.GlassButton
import app.motile.ui.SheetBar
import app.motile.ui.SheetCloseButton

private val margin = 16.dp

/**
 * What the agents spent and what is left of their plans, in a sheet: the tabs across it, under
 * them the servers counted and the period, and the way to read it all again in its bar.
 */
@Composable
fun UsageSheet() {
    val store = LocalStore.current
    val model = store.usage
    val close = LocalCloseSheet.current
    val picksServers = store.usageServers.size > 1
    Column(Modifier.fillMaxSize()) {
        SheetBar(
            "Usage",
            leading = { SheetCloseButton(onClose = close) },
            trailing = { GlassButton(Symbol.RefreshCw, "Refresh", onClick = { model.refresh() }, symbolSize = 16f, pending = model.refreshing) },
        )
        Column(Modifier.fillMaxWidth().padding(horizontal = margin, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            UsageTabs(model, fills = true)
            if (picksServers || model.tab != UsageModel.Tab.Limits) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (picksServers) UsageServersMenu(model)
                    if (model.tab != UsageModel.Tab.Limits) UsagePeriodMenu(model)
                }
            }
        }
        UsageContent(model, Modifier.weight(1f), margin = margin)
    }
}
