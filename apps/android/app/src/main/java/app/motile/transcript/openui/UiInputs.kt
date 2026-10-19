package app.motile.transcript.openui

import android.os.SystemClock
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.clickable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import app.motile.core.UiAlign
import app.motile.core.UiBasis
import app.motile.core.UiButtonVariant
import app.motile.core.UiControlSize
import app.motile.core.UiJustify
import app.motile.core.UiKind
import app.motile.core.UiNode
import app.motile.core.UiRadioVariant
import app.motile.core.UiSizing
import app.motile.core.UiTextStyle
import app.motile.theme.ControlSize
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.Surface
import app.motile.theme.opacity
import app.motile.theme.wash
import app.motile.ui.ActionButton
import app.motile.ui.ButtonVariant
import app.motile.ui.DateField
import app.motile.ui.Glyph
import app.motile.ui.InputField
import app.motile.ui.OptionCard
import app.motile.ui.PickMark
import app.motile.ui.PickRow
import app.motile.ui.Segmented
import app.motile.ui.Select
import app.motile.ui.Slider
import app.motile.ui.Text
import app.motile.ui.TextArea
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.time.LocalDate
import kotlin.math.floor
import kotlin.math.max

/** Sends what a control keeps changing at most every `interval`, and the last of it once it settles. */
private class Throttle(private val scope: CoroutineScope, private val interval: Long = 100) {
    private var sent = 0L
    private var waiting: Job? = null

    fun offer(send: () -> Unit) {
        waiting?.cancel()
        val now = SystemClock.uptimeMillis()
        if (now - sent >= interval) {
            sent = now
            send()
            return
        }
        waiting = scope.launch {
            delay(interval - (now - sent))
            sent = SystemClock.uptimeMillis()
            send()
        }
    }
}

@Composable
private fun rememberThrottle(): Throttle {
    val scope = rememberCoroutineScope()
    return remember(scope) { Throttle(scope) }
}

private fun UiControlSize.kit(): ControlSize = when (this) {
    UiControlSize.Small -> ControlSize.Small
    UiControlSize.Regular -> ControlSize.Regular
    UiControlSize.Large -> ControlSize.Large
}

private fun UiButtonVariant.kit(): ButtonVariant = when (this) {
    UiButtonVariant.Primary -> ButtonVariant.Primary
    UiButtonVariant.Danger -> ButtonVariant.Danger
    UiButtonVariant.Success -> ButtonVariant.Success
    UiButtonVariant.Outline -> ButtonVariant.Outline
    UiButtonVariant.Secondary -> ButtonVariant.Secondary
    UiButtonVariant.Ghost -> ButtonVariant.Ghost
}

@Composable
fun UiButton(node: UiNode, kind: UiKind.Button) {
    val palette = LocalPalette.current
    val host = LocalUiHost.current
    val size = kind.size.kit()
    ActionButton(
        title = kind.label,
        modifier = Modifier.uiNode(node),
        picture = kind.icon?.let { icon -> { Glyph(icon.glyph, size.symbolSide) } },
        variant = kind.variant.kit(),
        size = size,
        fills = kind.block,
        tint = kind.tint?.let { palette.token(it) },
        disabled = kind.disabled != null,
    ) { host?.send(node.id, "press") }
}

@Composable
fun UiIconButton(node: UiNode, kind: UiKind.IconButton) {
    val host = LocalUiHost.current
    val size = ControlSize.Regular
    ActionButton(
        modifier = Modifier.uiNode(node),
        picture = kind.icon?.let { icon -> { Glyph(icon.glyph, size.symbolSide) } },
        help = kind.label,
        variant = kind.variant.kit(),
        size = size,
        disabled = kind.disabled != null,
    ) { host?.send(node.id, "press") }
}

/**
 * Its children in a column that takes a press: Motile's row light behind it, or a wash over its
 * own background. Selected, the ring and the picked mark, unless it holds a Row that shows its own.
 */
@Composable
fun UiPressable(node: UiNode, kind: UiKind.Pressable) {
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    val host = LocalUiHost.current
    val interaction = remember { MutableInteractionSource() }
    val hovered by interaction.collectIsHoveredAsState()
    val pressed by interaction.collectIsPressedAsState()
    val enabled = kind.disabled == null
    val shape = RoundedCornerShape(UiStyle.radius(kind.radius))
    val background = kind.background?.color()
    val lit = enabled && (hovered || pressed)
    val light by animateColorAsState(
        when {
            background != null -> if (lit) palette.foreground.wash() else Color.Transparent
            pressed || kind.selected == true -> surface.color(Surface.Layer.RowSelected)
            lit -> surface.color(Surface.Layer.Row)
            else -> Color.Transparent
        },
        tween(120), label = "light",
    )
    val press = if (kind.selected != null) {
        Modifier.selectable(kind.selected, interaction, indication = null, enabled = enabled, role = Role.Button) { host?.send(node.id, "press") }
    } else {
        Modifier.clickable(interaction, indication = null, enabled = enabled, role = Role.Button) { host?.send(node.id, "press") }
    }
    val label = node.a11y?.label
    Box(
        Modifier
            .uiNode(node)
            .opacity(Opacity.Disabled, on = !enabled)
            .clip(shape)
            .then(if (background != null) Modifier.background(background, shape) else Modifier)
            .background(if (background == null) light else Color.Transparent, shape)
            .hoverable(interaction, enabled)
            .then(press)
            .semantics { if (label != null) contentDescription = label }
            .drawWithContent {
                drawContent()
                if (background != null) drawRect(light)
            },
    ) {
        CompositionLocalProvider(LocalSurface provides if (kind.innerSurface) surface.inBox else surface) {
            UiColumnLayout(kind.children.map { 0f }, UiAlign.Stretch, kind.children.map(::keepsWidth), Modifier.fillMaxWidth().padding(kind.padding.dp)) {
                kind.children.forEach { UiNodeView(it) }
            }
        }
        if (kind.ring) {
            Box(Modifier.matchParentSize().border(1.dp, palette.borderAccent, shape))
            PickMark(true, multiple = false, Modifier.align(Alignment.TopEnd).padding(top = 8.dp, end = 8.dp))
        }
    }
}

/** A control with its label 8 above it and what is wrong with it under it. */
@Composable
private fun Field(node: UiNode, label: String?, error: String?, content: @Composable () -> Unit) {
    val palette = LocalPalette.current
    Column(Modifier.uiNode(node).fillMaxWidth()) {
        if (label != null) Text(label, Modifier.padding(bottom = 8.dp), style = UiStyle.text(UiTextStyle.Label, false), color = palette.foreground)
        content()
        if (error != null) Text(error, Modifier.padding(top = 6.dp), style = UiStyle.text(UiTextStyle.Caption, false), color = palette.destructive)
    }
}

@Composable
fun UiInput(node: UiNode, kind: UiKind.Input) {
    val host = LocalUiHost.current
    val throttle = rememberThrottle()
    var typed by remember { mutableStateOf<String?>(null) }
    var focused by remember { mutableStateOf(false) }
    LaunchedEffect(kind.value, focused) { if (!focused) typed = null }
    Field(node, kind.label, kind.error) {
        app.motile.ui.Disabled(kind.disabled != null) {
            InputField(
                placeholder = kind.placeholder.orEmpty(),
                text = typed ?: kind.value,
                onTextChange = { text ->
                    typed = text
                    val number = text.replace(",", "").trim()
                    when {
                        !kind.number -> throttle.offer { host?.send(node.id, "value", "value" to text) }
                        number.isEmpty() -> throttle.offer { host?.send(node.id, "value", "value" to null) }
                        else -> number.toDoubleOrNull()?.let { value -> throttle.offer { host?.send(node.id, "value", "value" to value) } }
                    }
                },
                modifier = Modifier.fillMaxWidth(),
                keyboardOptions = if (kind.number) KeyboardOptions(keyboardType = KeyboardType.Decimal) else KeyboardOptions.Default,
                onFocusChange = { focused = it },
            )
        }
    }
}

@Composable
fun UiTextArea(node: UiNode, kind: UiKind.TextArea) {
    val host = LocalUiHost.current
    val throttle = rememberThrottle()
    var typed by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(kind.value) { if (typed == kind.value) typed = null }
    val lines = kind.rows.coerceIn(2, 20)
    Field(node, kind.label, kind.error) {
        app.motile.ui.Disabled(kind.disabled != null) {
            TextArea(
                placeholder = kind.placeholder.orEmpty(),
                text = typed ?: kind.value,
                onTextChange = { text ->
                    typed = text
                    throttle.offer { host?.send(node.id, "value", "value" to text) }
                },
                modifier = Modifier.fillMaxWidth(),
                lines = lines,
                maxLines = lines,
            )
        }
    }
}

@Composable
fun UiSelect(node: UiNode, kind: UiKind.Select) {
    val host = LocalUiHost.current
    Field(node, kind.label, kind.error) {
        Select(
            kind.options.map { it.label }, kind.selected,
            onSelect = { index -> kind.options.getOrNull(index)?.let { host?.send(node.id, "value", "value" to it.value) } },
            placeholder = kind.placeholder, disabled = kind.disabled != null,
        )
    }
}

@Composable
fun UiSegmented(node: UiNode, kind: UiKind.SegmentedControl) {
    val host = LocalUiHost.current
    val options = kind.options.mapIndexed { index, option -> option.label to index }
    val segmented = @Composable { modifier: Modifier ->
        app.motile.ui.Disabled(kind.disabled != null) {
            Segmented(options, kind.selected ?: -1, { index -> host?.send(node.id, "value", "value" to kind.options[index].value) }, modifier, fills = kind.block)
        }
    }
    if (kind.block) {
        Box(Modifier.uiNode(node).fillMaxWidth()) { segmented(Modifier) }
    } else {
        Box(Modifier.uiNode(node).horizontalScroll(rememberScrollState())) { segmented(Modifier) }
    }
}

/** The kit's slider: it keeps its own value while dragged and sends it every so often. */
@Composable
fun UiSlider(node: UiNode, kind: UiKind.Slider) {
    val host = LocalUiHost.current
    val throttle = rememberThrottle()
    Slider(
        value = kind.value,
        range = kind.min..max(kind.min, kind.max),
        onChange = { value -> throttle.offer { host?.send(node.id, "value", "value" to value) } },
        modifier = Modifier.uiNode(node),
        step = kind.step.takeIf { it > 0 },
        label = kind.label.takeIf { kind.head },
        readout = kind.readout.takeIf { kind.head },
        marks = kind.marks.map { (kind.min + it.at * (kind.max - kind.min)) to it.label },
        disabled = kind.disabled != null,
    )
}

@Composable
fun UiCheckbox(node: UiNode, kind: UiKind.Checkbox) {
    val host = LocalUiHost.current
    Field(node, null, kind.error) {
        PickRow(
            kind.label, kind.checked, multiple = true, onPress = { host?.send(node.id, "value", "value" to !kind.checked) },
            struck = kind.strike && kind.checked, disabled = kind.disabled != null,
        )
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun UiRadioGroup(node: UiNode, kind: UiKind.RadioGroup) {
    val palette = LocalPalette.current
    val host = LocalUiHost.current
    val disabled = kind.disabled != null
    val pick = { index: Int -> host?.send(node.id, "value", "value" to kind.options[index].value) }
    Column(Modifier.uiNode(node).fillMaxWidth()) {
        if (kind.label != null) Text(kind.label, Modifier.padding(bottom = 10.dp), style = UiStyle.text(UiTextStyle.Label, false), color = palette.foreground)
        when (kind.variant) {
            UiRadioVariant.List -> Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                kind.options.forEachIndexed { index, option ->
                    PickRow(option.label, index == kind.selected, multiple = false, onPress = { pick(index) }, detail = option.description, disabled = disabled)
                }
            }
            UiRadioVariant.Pills -> FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                kind.options.forEachIndexed { index, option ->
                    val chosen = index == kind.selected
                    ActionButton(
                        option.label, variant = if (chosen) ButtonVariant.Accent else ButtonVariant.Secondary, size = ControlSize.Regular,
                        disabled = disabled,
                    ) { pick(index) }
                }
            }
            UiRadioVariant.Cards -> UiGridLayout(12, 200f, 8f, Modifier.fillMaxWidth()) {
                kind.options.forEachIndexed { index, option ->
                    OptionCard(option.label, index == kind.selected, { pick(index) }, detail = option.description, disabled = disabled)
                }
            }
        }
        if (kind.error != null) Text(kind.error, Modifier.padding(top = 6.dp), style = UiStyle.text(UiTextStyle.Caption, false), color = palette.destructive)
    }
}

@Composable
fun UiDatePicker(node: UiNode, kind: UiKind.DatePicker) {
    val host = LocalUiHost.current
    fun day(text: String?) = text?.let { runCatching { LocalDate.parse(it) }.getOrNull() }
    Field(node, kind.label, kind.error) {
        DateField(
            text = kind.text.takeIf { kind.value != null },
            day = day(kind.value),
            onPick = { host?.send(node.id, "value", "value" to it.toString()) },
            label = kind.label,
            min = day(kind.min),
            max = day(kind.max),
            disabled = kind.disabled != null,
        )
    }
}

/** A countdown in a bordered box: its label over the time at one end, its buttons at the other. */
@Composable
fun UiTimer(node: UiNode, kind: UiKind.Timer) {
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    val host = LocalUiHost.current
    val shape = RoundedCornerShape(Radius.lg)
    val sizing = listOf(UiSizing(UiBasis.Content, null, 0f, true), UiSizing(UiBasis.Content, null, 0f, false))
    UiRowLayout(
        12f, UiAlign.Center, UiJustify.Between, wrap = true, sizing = sizing, extent = LocalUiExtent.current,
        modifier = Modifier.uiNode(node).fillMaxWidth().border(1.dp, surface.border, shape).padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            kind.label?.let { Text(it, style = UiStyle.text(UiTextStyle.Label, false), color = palette.foreground) }
            Text(kind.time, style = UiStyle.text(UiTextStyle.Timer, false), color = if (kind.done) palette.success else palette.foreground, maxLines = 1)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            val size = ControlSize.Small
            ActionButton(
                kind.toggle.label, picture = kind.toggle.icon?.let { icon -> { Glyph(icon.glyph, size.symbolSide) } },
                variant = ButtonVariant.Primary, size = size, disabled = kind.toggle.disabled != null,
            ) { host?.send(node.id, if (kind.running) "pause" else "start") }
            ActionButton(
                kind.reset.label, picture = kind.reset.icon?.let { icon -> { Glyph(icon.glyph, size.symbolSide) } },
                variant = ButtonVariant.Ghost, size = size, disabled = kind.reset.disabled != null,
            ) { host?.send(node.id, "reset") }
        }
    }
}
