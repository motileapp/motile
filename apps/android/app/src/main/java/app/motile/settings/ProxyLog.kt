package app.motile.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.Proxy
import app.motile.core.ProxyCaptured
import app.motile.core.ProxyLogRow
import app.motile.core.coreError
import app.motile.platform.Platform
import app.motile.theme.LocalPalette
import app.motile.theme.Theme
import app.motile.ui.Card
import app.motile.ui.Chip
import app.motile.ui.CopyButton
import app.motile.ui.HighlightButton
import app.motile.ui.Segmented
import app.motile.ui.Spinner
import app.motile.ui.Text
import app.motile.ui.ThemeDivider

/** Rows from the end of the log at which its next page is read. */
private const val PAGE_AHEAD = 5

/** The proxy's request log, newest first, read a page at a time as it scrolls. A captured request opens in full. */
@Composable
fun ProxyLogPage() {
    val proxy = LocalStore.current.proxy
    val log = proxy.log
    val navigator = LocalSettingsNavigator.current
    val palette = LocalPalette.current
    val state = rememberLazyListState()
    Watching(proxy)
    val nearEnd by remember {
        derivedStateOf {
            val last = state.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: return@derivedStateOf false
            last >= state.layoutInfo.totalItemsCount - PAGE_AHEAD
        }
    }
    LaunchedEffect(nearEnd, log.rows.size) { if (nearEnd) log.loadMore() }
    LaunchedEffect(proxy.view.logHead) { log.refresh() }
    val empty = when {
        log.rows.isNotEmpty() || log.loading -> null
        log.error != null -> log.error
        log.errorsOnly -> "No errors"
        else -> "No requests yet"
    }
    SettingsScreen("Request Log") {
        LazyColumn(
            Modifier.fillMaxSize(),
            state = state,
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 40.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            item(key = "filter") {
                Segmented(listOf("All" to false, "Errors" to true), log.errorsOnly, { log.errorsOnly = it }, Modifier.fillMaxWidth(), fills = true)
            }
            empty?.let { text ->
                item(key = "empty") { Text(text, Modifier.padding(horizontal = 16.dp, vertical = 8.dp), size = NOTE_TEXT, color = palette.mutedForeground) }
            }
            items(log.rows, key = { it.id }) { row ->
                LogRow(row, open = if (row.captured) ({ navigator.push(SettingsRoute.ProxyRequest(row.id)) }) else null)
            }
            if (log.loading) item(key = "loading") {
                Box(Modifier.fillMaxWidth().padding(12.dp), contentAlignment = Alignment.Center) { Spinner(color = palette.mutedForeground) }
            }
        }
    }
}

/** A captured request in full: its row, then the request's or the response's headers and body. */
@Composable
fun ProxyRequestPage(entry: Long) {
    val proxy = LocalStore.current.proxy
    val palette = LocalPalette.current
    var captured by remember(entry) { mutableStateOf<ProxyCaptured?>(null) }
    var failure by remember(entry) { mutableStateOf<String?>(null) }
    var loaded by remember(entry) { mutableStateOf(false) }
    var response by remember(entry) { mutableStateOf(false) }
    Watching(proxy)
    LaunchedEffect(entry) {
        proxy.captured(entry) { result ->
            loaded = true
            captured = result.getOrNull()
            failure = result.coreError?.message
        }
    }
    SettingsScreen("Captured Request") {
        val shown = captured
        if (shown == null) {
            Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
                if (!loaded) Spinner(color = palette.mutedForeground)
                else Text(failure ?: "Your proxy no longer has this request", size = NOTE_TEXT, color = palette.mutedForeground)
            }
            return@SettingsScreen
        }
        val headers = if (response) shown.responseHeaders else shown.requestHeaders
        val body = if (response) shown.responseBody else shown.requestBody
        val lines = if (response) shown.responseLines else shown.requestLines
        LazyColumn(
            Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 40.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            item(key = "row") { LogRow(shown.row, open = null) }
            item(key = "part") {
                Segmented(listOf("Request" to false, "Response" to true), response, { response = it }, Modifier.fillMaxWidth(), fills = true)
            }
            if (headers.isNotEmpty()) item(key = "headers") { Headers(headers) }
            item(key = "body") {
                Row(Modifier.fillMaxWidth().padding(start = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text("Body", Modifier.weight(1f), size = NOTE_TEXT, color = palette.mutedForeground)
                    if (body.isNotEmpty()) CopyButton("Copy the body") { Platform.copy(body) }
                }
            }
            if (lines.isEmpty()) item(key = "no-body") { Text("No body", Modifier.padding(horizontal = 16.dp), size = NOTE_TEXT, color = palette.mutedForeground) }
            items(lines.size) { index -> Text(lines[index], style = Theme.smallMono) }
        }
    }
}

/** Keeps the core connected to the proxy while the page shows, as its commands need. */
@Composable
private fun Watching(proxy: Proxy) {
    DisposableEffect(proxy) {
        proxy.watch(true)
        onDispose { proxy.watch(false) }
    }
}

/** A request as the log lists it: when, its status and where it went, then what it took. */
@Composable
private fun LogRow(row: ProxyLogRow, open: (() -> Unit)?) {
    val palette = LocalPalette.current
    val details = listOfNotNull(row.account, row.model, row.path, row.tokens, row.retries, row.size).joinToString(" · ")
    Card(Modifier.fillMaxWidth()) {
        HighlightButton(open ?: {}, Modifier.fillMaxWidth(), enabled = open != null, radius = 0.dp) {
            Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(row.time, style = Theme.uiDigits(12f), color = palette.mutedForeground, maxLines = 1)
                    Chip(row.status.toString(), tone = if (row.ok) null else palette.warning, monospaced = true)
                    Text(row.latency, Modifier.weight(1f), size = 12f, color = palette.mutedForeground, maxLines = 1)
                    if (row.captured) Chip("Captured", muted = true)
                }
                Text("${row.server} · ${row.thread}", maxLines = 1)
                Text(details, size = 11.5f, color = palette.mutedForeground, maxLines = 2)
                row.error?.let { Text(it, size = 11.5f, color = palette.warning, maxLines = 3) }
            }
        }
    }
}

@Composable
private fun Headers(headers: List<Pair<String, String>>) {
    val palette = LocalPalette.current
    Card(Modifier.fillMaxWidth()) {
        headers.forEachIndexed { index, (name, value) ->
            if (index > 0) ThemeDivider(Modifier.padding(start = 16.dp))
            Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(name, style = Theme.smallMono, color = palette.mutedForeground, maxLines = 1)
                Text(value, style = Theme.smallMono)
            }
        }
    }
}
