package app.motile.usage

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Loaded
import app.motile.core.Server
import app.motile.core.UsageModel
import app.motile.platform.Symbol
import app.motile.theme.LocalPalette
import app.motile.ui.ActionMenu
import app.motile.ui.ButtonVariant
import app.motile.ui.Check
import app.motile.ui.CheckMenu
import app.motile.ui.Disabled
import app.motile.ui.Segmented
import app.motile.ui.Spinner
import app.motile.ui.Text

/** The servers as the menu that picks them lists them, each checked when it is picked. */
fun UsageModel.serverChecks(all: List<Server>): List<Check> =
    all.map { server -> Check(server.id, server.name, server.id in servers) { toggle(server, all) } }

/** The servers counted, as a menu that picks them. */
@Composable
fun UsageServersMenu(model: UsageModel) {
    val servers = LocalStore.current.usageServers
    CheckMenu(
        model.serversLabel(servers), help = "The servers counted", checks = model.serverChecks(servers), icon = Symbol.Server,
        variant = ButtonVariant.Secondary,
    )
}

@Composable
fun UsageTabs(model: UsageModel, fills: Boolean = false) {
    Segmented(UsageModel.Tab.entries.map { it.label to it }, model.tab, { model.tab = it }, fills = fills)
}

@Composable
fun UsagePeriodMenu(model: UsageModel) {
    Disabled(model.tab == UsageModel.Tab.Limits) {
        ActionMenu(model.period.label, variant = ButtonVariant.Secondary) {
            for (period in UsageModel.Period.entries) item(period.label, checked = period == model.period) { model.period = period }
        }
    }
}

/** What the open tab shows, in a column that doesn't grow past a width. */
@Composable
fun UsageContent(model: UsageModel, modifier: Modifier = Modifier, margin: Dp = 24.dp, top: Dp = 20.dp) {
    Box(modifier.fillMaxWidth().verticalScroll(rememberScrollState()), contentAlignment = Alignment.TopCenter) {
        Column(Modifier.widthIn(max = 880.dp).fillMaxWidth().padding(start = margin, end = margin, top = top, bottom = 80.dp)) {
            when (model.tab) {
                UsageModel.Tab.Limits -> when (val limits = model.limits) {
                    Loaded.Loading -> UsageLoading()
                    is Loaded.Failed -> UsageNote(limits.message)
                    is Loaded.Ready -> LimitsView(limits.value)
                }
                UsageModel.Tab.Cost, UsageModel.Tab.Tokens -> when (val spending = model.spending) {
                    Loaded.Loading -> UsageLoading()
                    is Loaded.Failed -> UsageNote(spending.message)
                    is Loaded.Ready -> SpendingView(
                        spending.value, if (model.tab == UsageModel.Tab.Cost) Measure.Cost else Measure.Tokens, model.period,
                    )
                }
            }
        }
    }
}

@Composable
private fun UsageLoading() {
    Box(Modifier.fillMaxWidth().heightIn(min = 240.dp), contentAlignment = Alignment.Center) {
        Spinner(color = LocalPalette.current.mutedForeground)
    }
}

/** Why a tab has nothing to show. */
@Composable
fun UsageNote(text: String) {
    Box(Modifier.fillMaxWidth().heightIn(min = 240.dp), contentAlignment = Alignment.Center) {
        Text(text, size = 12f, color = LocalPalette.current.mutedForeground, align = TextAlign.Center)
    }
}
