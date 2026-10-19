package app.motile.transcript.openui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.snapping.SnapLayoutInfoProvider
import androidx.compose.foundation.gestures.snapping.rememberSnapFlingBehavior
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.layout.IntrinsicMeasurable
import androidx.compose.ui.layout.IntrinsicMeasureScope
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.Measurable
import androidx.compose.ui.layout.MeasurePolicy
import androidx.compose.ui.layout.MeasureResult
import androidx.compose.ui.layout.MeasureScope
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import app.motile.LocalStore
import app.motile.core.UiAlign
import app.motile.core.UiKind
import app.motile.core.UiNode
import app.motile.core.UiRadiusKind
import app.motile.core.UiRole
import app.motile.core.UiSlot
import app.motile.core.UiTextStyle
import app.motile.platform.scaled
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.OnSurface
import app.motile.theme.Radius
import app.motile.theme.ShadowSize
import app.motile.theme.Surface
import app.motile.theme.shadow
import app.motile.transcript.ErrorRow
import app.motile.transcript.LocalTranscript
import app.motile.transcript.RowModel
import app.motile.ui.lined
import androidx.compose.ui.unit.sp
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/** The column and the window lengths are measured in. */
val LocalUiExtent = staticCompositionLocalOf { UiExtent(0f, 0f) }

/**
 * A program's row: its views, from the tree core resolves, spaced from the paragraphs around it
 * as a prose table is, and what is wrong with it under them.
 */
@Composable
fun UiRow(row: RowModel, content: UiContent) {
    val store = LocalStore.current
    val threadID = LocalTranscript.current.model.threadID ?: return
    val host = remember(threadID, row.id, content.limited) { UiHost(store, threadID, row.id, content.limited) }
    val tree = content.tree
    val errors = content.errors
    BoxWithConstraints(Modifier.fillMaxWidth().padding(top = 6.dp)) {
        val width = maxWidth.value
        Column(Modifier.fillMaxWidth()) {
            if (tree != null) {
                CompositionLocalProvider(LocalUiHost provides host) { UiProgram(tree, width) }
            }
            if (errors.isNotEmpty()) ErrorRow(errors.joinToString("\n"), Modifier.padding(top = if (tree != null) 12.dp else 0.dp))
        }
    }
}

/** The program's tree laid out in a column `width` points wide. */
@Composable
fun UiProgram(tree: UiNode, width: Float) {
    val window = with(LocalDensity.current) { LocalWindowInfo.current.containerSize.height.toDp().value }
    val extent = remember(width, window) { UiExtent(width, window) }
    CompositionLocalProvider(
        LocalUiNarrow provides (width < UiStyle.NARROW_BELOW),
        LocalUiExtent provides extent,
        LocalUiTextAlign provides null,
    ) {
        UiColumnLayout(listOf(0f), UiAlign.Stretch, listOf(keepsWidth(tree)), Modifier.fillMaxWidth()) { UiNodeView(tree) }
    }
}

/** The view of one node, by its component. */
@Composable
fun UiNodeView(node: UiNode) {
    when (val kind = node.kind) {
        is UiKind.Col -> UiColumnLayout(kind.spacing, kind.align, kind.children.map(::keepsWidth), Modifier.uiNode(node)) {
            Aligned(kind.align) { kind.children.forEach { UiNodeView(it) } }
        }
        is UiKind.Row -> UiRowLayout(kind.gap, kind.align, kind.justify, kind.wrap, kind.sizing, LocalUiExtent.current, Modifier.uiNode(node)) {
            kind.children.forEach { UiNodeView(it) }
        }
        is UiKind.Box -> UiBox(node, kind)
        is UiKind.Grid -> UiGridLayout(kind.columns, kind.minChildWidth, kind.gap, Modifier.uiNode(node)) { kind.children.forEach { UiNodeView(it) } }
        is UiKind.Card -> UiCard(node, kind)
        is UiKind.Carousel -> UiCarousel(node, kind)
        is UiKind.Popover -> UiPopover(node, kind)
        UiKind.Divider -> Box(Modifier.uiNode(node).fillMaxWidth().height(1.dp).background(LocalSurface.current.border))
        UiKind.Spacer -> Box(Modifier.uiNode(node))
        is UiKind.Title -> UiTitle(node, kind)
        is UiKind.Text -> UiTextView(node, kind)
        is UiKind.Caption -> UiCaption(node, kind)
        is UiKind.Label -> UiLabel(node, kind)
        is UiKind.Badge -> UiBadge(node, kind)
        is UiKind.Link -> UiLink(node, kind)
        is UiKind.List -> UiList(node, kind)
        is UiKind.ListItem -> UiList(node, UiKind.List(listOf(node), app.motile.core.UiListMarker.Plain, timeline = false, wide = false))
        is UiKind.Markdown -> UiMarkdown(node, kind)
        is UiKind.Quote -> UiQuote(node, kind)
        is UiKind.Code -> UiCode(node, kind)
        is UiKind.Math -> UiCanvasView(node, kind.canvas)
        is UiKind.Button -> UiButton(node, kind)
        is UiKind.IconButton -> UiIconButton(node, kind)
        is UiKind.Pressable -> UiPressable(node, kind)
        is UiKind.Input -> UiInput(node, kind)
        is UiKind.TextArea -> UiTextArea(node, kind)
        is UiKind.Select -> UiSelect(node, kind)
        is UiKind.SegmentedControl -> UiSegmented(node, kind)
        is UiKind.Slider -> UiSlider(node, kind)
        is UiKind.Checkbox -> UiCheckbox(node, kind)
        is UiKind.RadioGroup -> UiRadioGroup(node, kind)
        is UiKind.DatePicker -> UiDatePicker(node, kind)
        is UiKind.Timer -> UiTimer(node, kind)
        is UiKind.Form -> UiColumnLayout(kind.spacing, UiAlign.Stretch, kind.children.map(::keepsWidth), Modifier.uiNode(node)) {
            kind.children.forEach { UiNodeView(it) }
        }
        is UiKind.SearchImage -> UiImage(node, kind.query, null, kind.aspectRatio, kind.radius, kind.fit, kind.photos)
        is UiKind.Image -> UiImage(node, kind.alt, kind.src, kind.aspectRatio, kind.radius, kind.fit, kind.photos)
        is UiKind.Icon -> UiIcon(node, kind)
        is UiKind.Table -> UiTable(node, kind)
        is UiKind.Chart -> UiCanvasView(node, kind.canvas)
        is UiKind.Map -> UiMapCard(node, kind)
        is UiKind.MapMarker -> Unit
        is UiKind.YouTube -> UiYouTube(node, kind)
        is UiKind.Flashcard -> UiFlashcard(node, kind)
        is UiKind.Celebration -> UiCelebration(node, kind)
        is UiKind.Svg -> UiCanvasView(node, kind.canvas)
        is UiKind.G, is UiKind.Circle, is UiKind.Ellipse, is UiKind.Rect, is UiKind.Line, is UiKind.Path,
        is UiKind.Polyline, is UiKind.Polygon, is UiKind.SvgText -> Unit
        is UiKind.HtmlView -> UiHtmlCard(node, kind)
    }
}

/** A child that stays at its own width at the column's start, even where the column stretches its children. */
fun keepsWidth(node: UiNode): Boolean = when (val kind = node.kind) {
    is UiKind.Button -> !kind.block
    is UiKind.SegmentedControl -> !kind.block
    is UiKind.IconButton, is UiKind.Badge, is UiKind.Icon, is UiKind.Link -> true
    is UiKind.Box -> kind.width != null
    else -> false
}

/** In a column that centres or ends its children, their text lines up the same way. */
@Composable
private fun Aligned(align: UiAlign, content: @Composable () -> Unit) {
    val text = when (align) {
        UiAlign.Center -> TextAlign.Center
        UiAlign.End -> TextAlign.End
        else -> null
    }
    if (text == null) return content()
    CompositionLocalProvider(LocalUiTextAlign provides text, content = content)
}

/** The node's frame for the dumps, and what accessibility is told of it. */
@Composable
fun Modifier.uiNode(node: UiNode): Modifier {
    val framed = uiFrame(node.id)
    val a11y = node.a11y ?: return framed
    return when (a11y.role) {
        UiRole.Hidden -> framed.clearAndSetSemantics {}
        UiRole.Heading -> framed.semantics { heading() }
        UiRole.Image -> framed.semantics {
            role = Role.Image
            a11y.label?.let { contentDescription = it }
        }
        UiRole.Region -> a11y.label?.let { label -> framed.semantics { contentDescription = label } } ?: framed
        else -> framed
    }
}

/** A shape of the radius core gives, half the shorter side for `full`. */
fun uiShape(radius: app.motile.core.UiRadius?): Shape = when {
    radius == null -> RectangleShape
    radius.kind == UiRadiusKind.Full -> RoundedCornerShape(50)
    else -> RoundedCornerShape(UiStyle.radius(radius))
}

@Composable
private fun UiBox(node: UiNode, kind: UiKind.Box) {
    val surface = LocalSurface.current
    val extent = LocalUiExtent.current
    val shape = uiShape(kind.radius)
    var box = Modifier.uiNode(node).uiSize(kind.width, kind.height, extent)
    if (kind.radius != null) box = box.clip(shape)
    kind.background?.let { box = box.background(it.color(), shape) }
    if (kind.border) box = box.border(1.dp, surface.border, shape)
    box = box.padding(kind.padding.dp)
    val inner = if (kind.innerSurface) surface.inBox else surface
    CompositionLocalProvider(LocalSurface provides inner) {
        if (kind.direction == app.motile.core.UiDirection.Row) {
            UiRowLayout(kind.gap, kind.align, kind.justify, kind.wrap, kind.sizing, extent, box) { kind.children.forEach { UiNodeView(it) } }
        } else {
            UiColumnLayout(kind.spacing, kind.align, kind.children.map(::keepsWidth), box, kind.sizing.map { it.grow }, kind.justify) {
                Aligned(kind.align) { kind.children.forEach { UiNodeView(it) } }
            }
        }
    }
}

/** Motile's card, or a soft box on the surface; a card in a card is smaller. */
@Composable
private fun UiCard(node: UiNode, kind: UiKind.Card) {
    val palette = LocalPalette.current
    val surface = LocalSurface.current
    val narrow = LocalUiNarrow.current
    val shape = RoundedCornerShape(if (kind.nested) Radius.md else Radius.lg)
    val padding = kind.padding ?: if (kind.nested) 16f else UiStyle.cardPadding(narrow)
    val frame = if (kind.soft) {
        Modifier.background(surface.color(Surface.Layer.Box), shape)
    } else {
        Modifier.background(palette.card, shape).border(1.dp, palette.borderCard, shape)
    }
    val inner = if (kind.soft) surface.inBox else Surface.Card
    CompositionLocalProvider(LocalSurface provides inner) {
        UiColumnLayout(kind.spacing, UiAlign.Stretch, kind.children.map(::keepsWidth), Modifier.uiNode(node).then(frame).padding(padding.dp)) {
            kind.children.forEach { UiNodeView(it) }
        }
    }
}

/**
 * Items side by side in a track that scrolls and snaps each item's leading edge to its own; the
 * items as tall as the tallest, a Card or a Pressable stretched to it. Its rings aren't cut: it
 * draws 4 past its top and bottom.
 */
@Composable
private fun UiCarousel(node: UiNode, kind: UiKind.Carousel) {
    val extent = LocalUiExtent.current
    val width = min(kind.itemWidth.coerceIn(80f, 640f), extent.column * 0.85f)
    val scroll = rememberScrollState()
    val bleed = 4.dp
    val density = LocalDensity.current
    val step = with(density) { (width + kind.gap).dp.toPx() }
    val bleedPx = with(density) { bleed.roundToPx() }
    val snapping = remember(scroll, step) {
        object : SnapLayoutInfoProvider {
            override fun calculateSnapOffset(velocity: Float): Float {
                val at = scroll.value / step
                val target = when {
                    velocity > 0 -> ceil(at)
                    velocity < 0 -> floor(at)
                    else -> at.roundToInt().toFloat()
                } * step
                return target.coerceIn(0f, scroll.maxValue.toFloat()) - scroll.value
            }
        }
    }
    val fling = rememberSnapFlingBehavior(snapping)
    Layout(
        content = { kind.children.forEach { UiNodeView(it) } },
        modifier = Modifier
            .uiNode(node)
            .fillMaxWidth()
            .layout { measurable, constraints ->
                val placeable = measurable.measure(constraints.copy(maxHeight = Constraints.Infinity, minHeight = 0))
                layout(placeable.width, max(0, placeable.height - bleedPx * 2)) { placeable.place(0, -bleedPx) }
            }
            .horizontalScroll(scroll, flingBehavior = fling)
            .padding(vertical = bleed),
        measurePolicy = CarouselPolicy(width, kind.gap, kind.fill),
    )
}

private class CarouselPolicy(val width: Float, val gap: Float, val fill: List<Boolean>) : MeasurePolicy {
    override fun MeasureScope.measure(measurables: List<Measurable>, constraints: Constraints): MeasureResult {
        val item = (width * density).roundToInt()
        val gapPx = (gap * density).roundToInt()
        val tallest = measurables.maxOfOrNull { it.maxIntrinsicHeight(item) } ?: 0
        val placeables = measurables.mapIndexed { index, measurable ->
            measurable.measure(if (fill.getOrNull(index) == true) Constraints.fixed(item, tallest) else Constraints(item, item, 0, tallest))
        }
        val total = placeables.size * item + max(0, placeables.size - 1) * gapPx
        return layout(total, tallest) {
            placeables.forEachIndexed { index, placeable -> placeable.place(index * (item + gapPx), 0) }
        }
    }

    override fun IntrinsicMeasureScope.maxIntrinsicHeight(measurables: List<IntrinsicMeasurable>, width: Int): Int =
        measurables.maxOfOrNull { it.maxIntrinsicHeight((this@CarouselPolicy.width * density).roundToInt()) } ?: 0
}

/**
 * A trigger laid inline that opens a popover under it: Motile's popover, as wide as what it holds
 * up to 280, above the trigger where there is no room under it. It closes on a press outside it,
 * on back, and when the transcript moves it.
 */
@Composable
private fun UiPopover(node: UiNode, kind: UiKind.Popover) {
    val host = LocalUiHost.current
    var open by remember { mutableStateOf(kind.open) }
    LaunchedEffect(kind.open) { open = kind.open }
    var placed by remember { mutableStateOf<Float?>(null) }
    fun show(shown: Boolean) {
        if (open == shown) return
        open = shown
        host?.send(node.id, "expand", "open" to shown)
    }
    Box(Modifier.uiNode(node)) {
        Row(
            Modifier
                .onGloballyPositioned { coordinates ->
                    val top = coordinates.localToRoot(androidx.compose.ui.geometry.Offset.Zero).y
                    if (open && placed != null && placed != top) show(false)
                    placed = top
                }
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, role = Role.Button) { show(!open) }
                .semantics { node.a11y?.label?.let { contentDescription = it } },
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            UiSlotView(kind.trigger)
        }
        if (open) PopoverPanel(280f, onDismiss = { show(false) }) { UiSlotView(kind.content, popover = true) }
    }
}

/** Motile's popover by what it lies in, placed by [PopoverPlacement], at most [most] wide, its content 8 apart. */
@Composable
internal fun PopoverPanel(most: Float, onDismiss: () -> Unit, content: @Composable () -> Unit) {
    val palette = LocalPalette.current
    val window = with(LocalDensity.current) { LocalWindowInfo.current.containerSize.width.toDp().value }
    Popup(
        popupPositionProvider = PopoverPlacement(with(LocalDensity.current) { 1.dp.toPx() }),
        onDismissRequest = onDismiss,
        properties = PopupProperties(focusable = true),
    ) {
        val shape = RoundedCornerShape(Radius.lg)
        OnSurface(Surface.Popover) {
            Column(
                Modifier
                    .padding(12.dp)
                    .widthIn(max = min(most, window - 24f).dp)
                    .shadow(ShadowSize.Md, shape)
                    .background(palette.popover, shape)
                    .border(1.dp, palette.borderPopover, shape)
                    .padding(horizontal = 14.dp, vertical = 12.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                content()
            }
        }
    }
}

/** Where a popover goes: 8 under its trigger at its leading edge, above it where there's no room under it, kept 12 in from the window's edges. */
private class PopoverPlacement(val point: Float) : PopupPositionProvider {
    override fun calculatePosition(anchorBounds: IntRect, windowSize: IntSize, layoutDirection: LayoutDirection, popupContentSize: IntSize): IntOffset {
        val margin = (12 * point).roundToInt()
        val gap = (8 * point).roundToInt()
        // The popup's content is inset by the margin on every side, for its shadow.
        val x = (anchorBounds.left - margin).coerceIn(0, max(0, windowSize.width - popupContentSize.width))
        val below = anchorBounds.bottom + gap - margin
        val room = windowSize.height - anchorBounds.bottom - gap
        val above = anchorBounds.top - gap - popupContentSize.height + margin
        val y = if (room < 220 * point && anchorBounds.top > 240 * point) above else below
        return IntOffset(x, y.coerceIn(0, max(0, windowSize.height - popupContentSize.height)))
    }
}

/** A slot's text, or its components side by side. */
@Composable
fun UiSlotView(slot: UiSlot, popover: Boolean = false) {
    val palette = LocalPalette.current
    val narrow = LocalUiNarrow.current
    slot.text?.let { text ->
        val style = UiStyle.text(slot.style, narrow).let { if (popover && slot.style == UiTextStyle.Body) it.copy(lineHeight = scaled(21f).sp) else it }
        UiRichText(text, style, palette.foreground)
    }
    slot.children.forEach { UiNodeView(it) }
}
