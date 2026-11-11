package app.motile.usage

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Loaded
import app.motile.core.Server
import app.motile.core.UsageModel
import app.motile.theme.LocalPalette
import app.motile.theme.TextSize
import app.motile.ui.Check
import app.motile.ui.Segmented
import app.motile.ui.Spinner
import app.motile.ui.Text
import app.motile.ui.edgeFade

/** The servers as the menu that picks them lists them, each checked when it is picked. */
fun UsageModel.serverChecks(all: List<Server>): List<Check> =
    all.map { server -> Check(server.id, server.name, server.id in servers) { toggle(server, all) } }

/** What the open tab shows, in a column that doesn't grow past a width; `periodInPage` puts the period over it, where no bar has it. The page fades under what is pinned over it. */
@Composable
fun UsageContent(model: UsageModel, modifier: Modifier = Modifier, margin: Dp = 24.dp, top: Dp = 20.dp, periodInPage: Boolean = false) {
    val scroll = rememberScrollState()
    Box(modifier.fillMaxWidth().edgeFade(above = { scroll.value.toFloat() }, below = { 0f }).verticalScroll(scroll), contentAlignment = Alignment.TopCenter) {
        Column(
            Modifier.widthIn(max = 880.dp).fillMaxWidth().padding(start = margin, end = margin, top = top, bottom = 80.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            if (periodInPage && model.tab != UsageModel.Tab.Limits) {
                Segmented(UsageModel.Period.entries.map { it.label to it }, model.period, { model.period = it }, fills = true)
            }
            when (model.tab) {
                UsageModel.Tab.Limits -> {
                    WatchesGateway()
                    when (val limits = model.limits) {
                        Loaded.Loading -> UsageLoading()
                        is Loaded.Failed -> UsageNote(limits.message)
                        is Loaded.Ready -> LimitsView(limits.value)
                    }
                }
                UsageModel.Tab.Activity -> when (val activity = model.activity) {
                    Loaded.Loading -> UsageLoading()
                    is Loaded.Failed -> UsageNote(activity.message)
                    is Loaded.Ready -> ActivityView(activity.value, model.period)
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

/** Keeps the core following the account's gateway while the view shows, so that its accounts are known. */
@Composable
fun WatchesGateway() {
    val gateway = LocalStore.current.gateway
    DisposableEffect(gateway) {
        gateway.watch(true)
        onDispose { gateway.watch(false) }
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
        Text(text, size = TextSize.Xs, color = LocalPalette.current.mutedForeground, align = TextAlign.Center)
    }
}
