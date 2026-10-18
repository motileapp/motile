package app.motile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
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
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import app.motile.core.Agent
import app.motile.core.AgentState
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.platform.scaled
import app.motile.theme.LocalPalette
import app.motile.theme.Palette
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
}

/**
 * An agent's logo in a ring, with a dot on its corner in the colour of how it is doing, cut out of
 * the ring. One `ringed` lies over another, from which a ring of
 * `ground`, the colour of what it lies on, sets it apart.
 */
@Composable
fun AgentAvatar(
    agent: Agent?,
    state: AgentState?,
    modifier: Modifier = Modifier,
    side: Dp = AgentAvatar.side,
    ground: Color = LocalPalette.current.background,
    ringed: Boolean = false,
) {
    val palette = LocalPalette.current
    val inset = if (ringed) 2.dp else 0.dp
    // The circle leaves the box's corner to the dot, which sits on its edge.
    val circle = if (state == null) side else side - AgentAvatar.cut
    val mark = ((circle - inset * 2).value * 0.56f).roundToInt().toFloat()
    val dotted = if (state == null) Modifier else Modifier
        .graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
        .drawWithContent {
            drawContent()
            val dot = AgentAvatar.dot.toPx()
            val cut = AgentAvatar.cut.toPx()
            val center = Offset(size.width - cut - dot / 2, size.height - cut - dot / 2)
            drawCircle(Color.Black, radius = dot / 2 + cut, center = center, blendMode = BlendMode.Clear)
            drawCircle(state.color(palette), radius = dot / 2, center = center)
        }
    Box(modifier.size(side).then(dotted)) {
        if (ringed) Box(Modifier.size(circle).background(ground, CircleShape))
        Box(
            Modifier.size(circle).padding(inset).background(palette.backgroundAccent, CircleShape).border(1.dp, palette.borderAccent, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            if (agent != null) AgentIcon(agent, size = mark)
            else Icon(Symbol.Bot, mark * 0.85f / Platform.scale / 1.2f, color = palette.mutedForeground)
        }
    }
}

/** Avatars side by side, each a quarter over the next and ringed by the ground, without their dots, and how many more there are. */
@Composable
fun AgentAvatars(
    avatars: List<Pair<Agent?, AgentState>>,
    more: Int,
    modifier: Modifier = Modifier,
    side: Dp = AgentAvatar.side,
    ground: Color = LocalPalette.current.background,
) {
    val palette = LocalPalette.current
    val step = side * 3 / 4
    // Alone, it says how it is doing; side by side they leave that to what is written beside them.
    val alone = avatars.size == 1
    Box(modifier.size(width = step * (avatars.size + (if (more > 0) 1 else 0) - 1) + side, height = side)) {
        if (more > 0) {
            Box(
                Modifier
                    .offset(x = step * avatars.size)
                    .size(side)
                    .background(palette.backgroundAccent, CircleShape)
                    .border(1.dp, palette.borderAccent, CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                Text("+$more", size = 10f, weight = FontWeight.Medium, color = palette.mutedForeground, maxLines = 1)
            }
        }
        // The first lies over the second.
        avatars.forEachIndexed { index, (agent, state) ->
            AgentAvatar(
                agent, if (alone) state else null, Modifier.offset(x = step * index).zIndex((avatars.size - index).toFloat()),
                side = side, ground = ground, ringed = !alone,
            )
        }
    }
}
