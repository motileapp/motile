package app.motile.transcript.openui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.motile.core.UiKind
import app.motile.core.UiNode
import app.motile.core.UiSlot
import app.motile.core.UiTextStyle
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Surface
import app.motile.transcript.canvas.Celebrations
import app.motile.transcript.canvas.Confetti
import app.motile.transcript.canvas.Flip
import app.motile.transcript.canvas.rememberFlip
import app.motile.ui.Text
import kotlin.math.max

/** A flip that swaps the faces at once, until core's comes. */
private val instantFlip = Flip(0f, FloatArray(0), 0f, emptyList())

/**
 * A card that shows one face and flips to the other when pressed: the face narrows to nothing about
 * its centre and the other widens from it, as core's flip says. As tall as its taller face, 168 at
 * least.
 */
@Composable
fun UiFlashcard(node: UiNode, kind: UiKind.Flashcard) {
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    val host = LocalUiHost.current
    val scene = rememberScene(kind.canvas, LocalUiExtent.current.column, LocalUiTextSize.current)
    val look by rememberFlip(scene?.scene?.motion as? Flip ?: instantFlip, kind.flipped)
    val interaction = remember { MutableInteractionSource() }
    val hovered by interaction.collectIsHoveredAsState()
    val border = if (hovered) palette.borderAccent else surface.border(palette)
    val shown = if (kind.flipped) kind.back else kind.front
    val radius = UiStyle.radius(kind.radius)
    Layout(
        content = {
            FlashcardFace(kind.front, kind.hint, surface.color(palette), border, radius)
            FlashcardFace(kind.back, null, surface.color(Surface.Layer.Box, palette), border, radius)
        },
        modifier = Modifier
            .uiNode(node)
            .fillMaxWidth()
            .graphicsLayer { scaleX = look.scaleX }
            .hoverable(interaction)
            .clickable(interaction, indication = null, role = Role.Button) { host?.send(node.id, "flip") }
            .clearAndSetSemantics {
                role = Role.Button
                selected = kind.flipped
                shown.text?.text?.let { contentDescription = it }
            },
    ) { measurables, constraints ->
        val width = constraints.maxWidth
        val height = max(max(168.dp.roundToPx(), constraints.minHeight), measurables.maxOf { it.maxIntrinsicHeight(width) })
        val face = measurables[if (look.back) 1 else 0].measure(Constraints.fixed(width, height))
        layout(width, height) { face.place(0, 0) }
    }
}

/** A face: its words or components centred both ways, and the hint under them on the front. */
@Composable
private fun FlashcardFace(slot: UiSlot, hint: String?, fill: Color, border: Color, radius: Dp) {
    val palette = LocalPalette.current
    val shape = RoundedCornerShape(radius)
    Box(Modifier.background(fill, shape).border(1.dp, border, shape)) {
        CompositionLocalProvider(LocalUiTextAlign provides TextAlign.Center) {
            Column(
                Modifier.fillMaxSize().padding(start = 18.dp, end = 18.dp, top = 20.dp, bottom = 30.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterVertically),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                UiSlotView(slot)
            }
        }
        if (hint != null) {
            Text(
                hint, Modifier.align(Alignment.BottomCenter).padding(bottom = 10.dp),
                style = UiStyle.text(UiTextStyle.Tick, false), color = palette.mutedStrongerForeground, maxLines = 1,
            )
        }
    }
}

/** Draws nothing in its row: each new burst core counts fires core's confetti over the window. */
@Composable
fun UiCelebration(node: UiNode, kind: UiKind.Celebration) {
    val scene = rememberScene(kind.canvas, LocalUiExtent.current.column, LocalUiTextSize.current)
    val confetti = scene?.scene?.motion as? Confetti
    val seen = remember(node.id) { intArrayOf(kind.burst) }
    LaunchedEffect(kind.burst, confetti) {
        if (confetti == null || kind.burst <= seen[0]) return@LaunchedEffect
        seen[0] = kind.burst
        Celebrations.fire(confetti)
    }
}
