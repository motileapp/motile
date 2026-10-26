package app.motile.composer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.motile.LocalStore
import app.motile.core.SkillMenu
import app.motile.core.SkillRow
import app.motile.core.pickSkill
import app.motile.platform.scaled
import app.motile.theme.iconSize
import app.motile.theme.Radius
import app.motile.theme.Theme
import app.motile.ui.Chip
import app.motile.ui.HighlightButton
import app.motile.ui.Icon
import app.motile.ui.Text
import kotlin.math.min

/**
 * The skills that fit the word being typed, on top of the composer as one of its strips: the best
 * first, each with what it does under its name and where it comes from. The arrows move the light,
 * Enter or Tab puts the lit one in, Esc puts the menu away, and a tap puts in the one tapped.
 */
object SkillMenuStrip {
    /** The description is under the name, where it has room. */
    val rowHeight = scaled(46f).dp
    val inset = 4.dp
    /** As many rows as show before the rest scroll, the last cut in half to say there are more. */
    const val shownRows = 7.5f
}

@Composable
fun SkillMenuStrip(menu: SkillMenu, modifier: Modifier = Modifier) {
    val store = LocalStore.current
    val scroll = rememberScrollState()
    val density = LocalDensity.current
    val inset = SkillMenuStrip.inset
    val height = SkillMenuStrip.rowHeight * min(menu.rows.size.toFloat(), SkillMenuStrip.shownRows) + inset * 2
    LaunchedEffect(store.skillHighlight, menu) {
        val index = store.skillHighlight
        if (index !in menu.rows.indices) return@LaunchedEffect
        with(density) {
            val top = (inset + SkillMenuStrip.rowHeight * index).roundToPx()
            val bottom = top + SkillMenuStrip.rowHeight.roundToPx()
            val shown = height.roundToPx()
            when {
                top - inset.roundToPx() < scroll.value -> scroll.scrollTo(top - inset.roundToPx())
                bottom + inset.roundToPx() > scroll.value + shown -> scroll.scrollTo(bottom + inset.roundToPx() - shown)
            }
        }
    }
    Column(
        modifier
            .composerStrip(ComposerStrip.Edge.Top, height = null)
            .height(height)
            .verticalScroll(scroll)
            .padding(vertical = inset)
            .semantics { contentDescription = "Skills" },
    ) {
        menu.rows.forEachIndexed { index, row ->
            HighlightButton(
                { store.pickSkill(row) }, Modifier.fillMaxWidth(), radius = Radius.md, lit = index == store.skillHighlight,
                inset = PaddingValues(horizontal = inset), fill = Theme.colors.borderComposer,
            ) { SkillMenuRow(row) }
        }
    }
}

@Composable
private fun SkillMenuRow(row: SkillRow) {
    val palette = Theme.colors
    Row(
        Modifier.padding(horizontal = SkillMenuStrip.inset).height(SkillMenuStrip.rowHeight).padding(horizontal = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(SkillChip.symbol, iconSize(text = 13f), color = palette.skill)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(row.label, size = 13f, weight = FontWeight.Medium, color = palette.foreground, maxLines = 1)
            Text(row.description, size = 12f, color = palette.mutedForeground, maxLines = 1)
        }
        Chip(row.source, icon = row.symbol)
    }
}
