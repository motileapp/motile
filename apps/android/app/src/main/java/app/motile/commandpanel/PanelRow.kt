package app.motile.commandpanel

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.LocalDark
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.theme.Theme
import app.motile.theme.opacity
import app.motile.theme.value
import app.motile.theme.wash
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.LogoMark
import app.motile.ui.ProjectIcon
import app.motile.ui.Spinner
import app.motile.ui.Text

internal object PanelRow {
    val height = scaled(52f).dp
    val oneLineHeight = scaled(40f).dp
    val sideMargin = 8.dp
    val radius = Radius.md
    val titleHeight = scaled(17f).dp
    val detailHeight = scaled(15f).dp
    val dotGap = 6.dp
}

@Composable
internal fun PanelRow(item: PanelItem, lit: Boolean, onClick: () -> Unit) {
    val placeholder = item.placeholderLines > 0
    HighlightButton(
        onClick,
        Modifier.fillMaxWidth().height(if (item.oneLine) PanelRow.oneLineHeight else PanelRow.height).then(if (placeholder) Modifier.pulsing() else Modifier),
        enabled = item.selectable,
        radius = PanelRow.radius,
        lit = lit,
        inset = PaddingValues(horizontal = PanelRow.sideMargin),
    ) {
        Row(
            Modifier.fillMaxSize().padding(horizontal = PanelRow.sideMargin + 10.dp).opacity(Opacity.Disabled, on = item.off),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(scaled(22f).dp), contentAlignment = Alignment.Center) { RowIcon(item) }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                if (placeholder) PlaceholderLines(item) else Words(item)
            }
            if (item.busy || item.note != null || item.keys != null) {
                Spacer(Modifier.width(8.dp))
                Trailing(item)
            }
        }
    }
}

/** Fades in and out, as a row that is on its way. */
@Composable
private fun Modifier.pulsing(): Modifier {
    val faded = Opacity.Disabled.value(LocalDark.current)
    val alpha by rememberInfiniteTransition(label = "placeholder").animateFloat(
        initialValue = 1f,
        targetValue = faded,
        animationSpec = infiniteRepeatable(tween(800, easing = FastOutSlowInEasing), RepeatMode.Reverse),
        label = "fade",
    )
    return alpha(alpha)
}

@Composable
private fun Words(item: PanelItem) {
    val palette = LocalPalette.current
    if (item.oneLine) {
        Row(Modifier.height(PanelRow.titleHeight), horizontalArrangement = Arrangement.spacedBy(PanelRow.dotGap), verticalAlignment = Alignment.CenterVertically) {
            Text(item.title, size = 14f, maxLines = 1)
            if (item.detailParts.isNotEmpty()) {
                Text("·", size = 12f, color = palette.mutedStrongerForeground)
                DetailParts(item.detailParts)
            }
        }
        return
    }
    Box(Modifier.height(PanelRow.titleHeight), contentAlignment = Alignment.CenterStart) {
        Text(item.title, size = 14f, maxLines = 1)
    }
    when {
        item.detailParts.isNotEmpty() -> DetailParts(item.detailParts)
        item.detail.isNotEmpty() -> Box(Modifier.height(PanelRow.detailHeight), contentAlignment = Alignment.CenterStart) {
            Text(item.detail, size = 12f, color = palette.mutedForeground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
        }
    }
}

@Composable
private fun DetailParts(parts: List<Pair<Symbol, String>>) {
    val palette = LocalPalette.current
    Row(Modifier.height(PanelRow.detailHeight), horizontalArrangement = Arrangement.spacedBy(PanelRow.dotGap), verticalAlignment = Alignment.CenterVertically) {
        parts.forEachIndexed { position, (symbol, text) ->
            if (position > 0) Text("·", size = 12f, color = palette.mutedStrongerForeground)
            // The first part keeps its room; the ones after it are cut short first.
            val room = if (position == 0) Modifier else Modifier.weight(1f, fill = false)
            Row(room, horizontalArrangement = Arrangement.spacedBy(3.dp), verticalAlignment = Alignment.CenterVertically) {
                // Lucide's server fills more of its square than the icons beside it.
                Icon(symbol, if (symbol == Symbol.Server) 11f else 12f, color = palette.mutedForeground)
                Text(text, size = 12f, color = palette.mutedForeground, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
            }
        }
    }
}

/** Bars where the text will be, each in the room its line takes. */
@Composable
private fun PlaceholderLines(item: PanelItem) {
    val titles = listOf(150, 210, 120, 180, 240, 140, 200, 160)
    val details = listOf(280, 190, 320, 230, 150, 300, 210, 260)
    Box(Modifier.height(PanelRow.titleHeight), contentAlignment = Alignment.CenterStart) {
        Bar(titles[item.index % titles.size].dp, 10.dp)
    }
    if (item.placeholderLines > 1) {
        Box(Modifier.height(PanelRow.detailHeight), contentAlignment = Alignment.CenterStart) {
            Bar(details[item.index % details.size].dp, 8.dp)
        }
    }
}

@Composable
private fun Bar(width: Dp, height: Dp) {
    Box(Modifier.size(width, height).background(LocalSurface.current.color(Surface.Layer.Control), CircleShape))
}

@Composable
private fun Trailing(item: PanelItem) {
    val palette = LocalPalette.current
    val note = item.note
    val keys = item.keys
    when {
        item.busy -> Spinner(color = palette.mutedForeground)
        note != null && item.warns -> Text(
            note,
            Modifier.background(palette.warning.wash(), CircleShape).padding(horizontal = 8.dp, vertical = 3.dp),
            size = 12f,
            weight = FontWeight.Medium,
            color = palette.warning,
            maxLines = 1,
        )
        note != null -> Text(note, size = 12f, weight = FontWeight.Medium, color = palette.mutedStrongerForeground, maxLines = 1)
        keys != null -> Text(keys, style = Theme.uiDigits(12f, FontWeight.Medium), color = palette.mutedStrongerForeground, maxLines = 1)
    }
}

@Composable
private fun RowIcon(item: PanelItem) {
    val palette = LocalPalette.current
    when (val icon = item.icon) {
        is PanelIcon.Glyph -> if (item.placeholderLines > 0) {
            Box(Modifier.size(scaled(20f).dp).background(LocalSurface.current.color(Surface.Layer.Control), RoundedCornerShape(Radius.xs)))
        } else {
            Icon(icon.symbol, 15f, color = palette.mutedForeground)
        }
        is PanelIcon.Logo -> LogoMark(icon.path, scaled(16f), palette.mutedForeground)
        is PanelIcon.Project -> ProjectIcon(icon.project, size = scaled(20f))
    }
}
