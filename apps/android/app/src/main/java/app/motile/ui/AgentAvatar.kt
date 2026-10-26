package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import app.motile.core.Agent
import app.motile.core.AgentState
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.LocalPalette
import app.motile.theme.Palette
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.roundToInt

fun AgentState.color(palette: Palette): Color = when (this) {
    AgentState.Working -> palette.process
    AgentState.Waiting, AgentState.Limited -> palette.warning
    AgentState.Done -> palette.success
    AgentState.Failed -> palette.destructive
    AgentState.Stopped -> palette.mutedStrongerForeground
}

val AgentState.color: Color
    @Composable @ReadOnlyComposable get() = color(LocalPalette.current)

object AgentAvatar {
    val side = scaled(24f).dp
    /** The dot is as large on every system as the composer strips' are, and cut out of what it lies on. */
    val dot = 6.dp
    val cut = 2.dp
    /** The square of the agent's logo, or the bot when it has none, as a share of its circle. */
    const val MARK = 0.6f
    /** The ellipsis that stands for the agents there is no room for, as a share of its circle. */
    const val ELLIPSIS = 0.72f

    /** How far each avatar side by side stands from the one before it: they overlap by a quarter. */
    fun step(side: Dp): Dp = side * 3 / 4

    /** The ring around an avatar that is cut out of the one under it. */
    fun ring(side: Dp): Dp = max(1f, floor(side.value / 12)).dp
}

/** An agent's logo in a ring, with a dot on its corner in the colour of how it is doing, cut out of the ring. */
@Composable
fun AgentAvatar(agent: Agent?, state: AgentState?, modifier: Modifier = Modifier, side: Dp = AgentAvatar.side) {
    val palette = LocalPalette.current
    // The circle leaves the box's corner to the dot, which sits on its edge.
    val circle = if (state == null) side else side - AgentAvatar.cut
    val dotted = if (state == null) Modifier else Modifier.cutting {
        val dot = AgentAvatar.dot.toPx()
        val cut = AgentAvatar.cut.toPx()
        val center = Offset(size.width - cut - dot / 2, size.height - cut - dot / 2)
        drawCircle(Color.Black, radius = dot / 2 + cut, center = center, blendMode = BlendMode.Clear)
        drawCircle(state.color(palette), radius = dot / 2, center = center)
    }
    Box(modifier.size(side).then(dotted)) {
        AvatarCircle(circle) {
            Icon(agent?.symbol ?: Symbol.Bot, (circle * AgentAvatar.MARK).rounded(), color = agent?.color(palette) ?: palette.mutedForeground)
        }
    }
}

/**
 * Avatars side by side, each a quarter over the next, which has it cut out with a ring around it,
 * and an ellipsis for the agents there is no room for.
 */
@Composable
fun AgentAvatars(avatars: List<Pair<Agent?, AgentState?>>, more: Int, modifier: Modifier = Modifier, side: Dp = AgentAvatar.side) {
    val palette = LocalPalette.current
    val overlap = side - AgentAvatar.step(side)
    // Alone, it says how it is doing; side by side they leave that to what is written beside them.
    val alone = avatars.size == 1
    Row(modifier, verticalAlignment = Alignment.CenterVertically) {
        // The first lies over the second.
        avatars.forEachIndexed { index, (agent, state) ->
            val after = index > 0
            AgentAvatar(
                agent, if (alone) state else null,
                Modifier.overlapping(if (after) overlap else 0.dp).cutUnder(after, side).zIndex((avatars.size - index).toFloat()),
                side = side,
            )
        }
        if (more > 0) {
            val after = avatars.isNotEmpty()
            AvatarCircle(side, Modifier.overlapping(if (after) overlap else 0.dp).cutUnder(after, side)) {
                Icon(Symbol.Ellipsis, (side * AgentAvatar.ELLIPSIS).rounded(), color = palette.mutedForeground)
            }
        }
    }
}

@Composable
private fun AvatarCircle(side: Dp, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val palette = LocalPalette.current
    Box(
        modifier.size(side).background(palette.backgroundAccent, CircleShape).border(1.dp, palette.borderAccent, CircleShape),
        contentAlignment = Alignment.Center,
    ) { content() }
}

/** Draws `cut` over what it draws, in a layer of its own, so that a clear blend shows what lies under it. */
private fun Modifier.cutting(cut: DrawScope.() -> Unit): Modifier = this
    .graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
    .drawWithContent {
        drawContent()
        cut()
    }

/** Cuts the avatar before it out of it, with a ring around it, where that one lies over it. */
private fun Modifier.cutUnder(cut: Boolean, side: Dp): Modifier {
    if (!cut) return this
    return cutting {
        val radius = side.toPx() / 2
        val center = Offset(radius - AgentAvatar.step(side).toPx(), size.height / 2)
        drawCircle(Color.Black, radius = radius + AgentAvatar.ring(side).toPx(), center = center, blendMode = BlendMode.Clear)
    }
}

private fun Dp.rounded(): Dp = value.roundToInt().dp

/** Lies `overlap` over what is before it, which takes that much less room. */
private fun Modifier.overlapping(overlap: Dp): Modifier = layout { measurable, constraints ->
    val shift = overlap.roundToPx()
    val placeable = measurable.measure(constraints)
    layout((placeable.width - shift).coerceAtLeast(0), placeable.height) { placeable.place(-shift, 0) }
}
