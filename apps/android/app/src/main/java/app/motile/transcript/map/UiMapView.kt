package app.motile.transcript.map

import android.os.SystemClock
import android.view.MotionEvent
import android.view.View
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.PointerEvent
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.layout.onVisibilityChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.isTraversalGroup
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withLink
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import app.motile.core.UiKind
import app.motile.core.UiRouteKind
import app.motile.platform.Platform
import app.motile.platform.Symbol
import app.motile.theme.LocalDark
import app.motile.theme.LocalPalette
import app.motile.theme.LocalSurface
import app.motile.theme.Opacity
import app.motile.theme.Radius
import app.motile.theme.Theme
import app.motile.theme.at
import app.motile.ui.Choice
import app.motile.ui.Choices
import app.motile.ui.DropdownCard
import app.motile.ui.GlassButton
import app.motile.ui.GlassCapsule
import app.motile.ui.GlassSize
import app.motile.ui.Text
import app.motile.ui.lined
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlin.math.roundToInt

/**
 * A program's `Map`: MapLibre over OpenFreeMap's tiles, live while on screen and a snapshot
 * otherwise. One finger scrolls the transcript and two move the map. A pressed pin calls
 * `onSelect` with its label, a filter's choice `onFilter` with its category, or none for every place.
 */
@Composable
fun UiMapView(node: UiKind.Map, streaming: Boolean, onSelect: (String) -> Unit, onFilter: (String?) -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val dark = LocalDark.current
    val palette = LocalPalette.current
    val density = LocalDensity.current
    val windowWidth = with(density) { LocalWindowInfo.current.containerSize.width.toDp().value }
    val height = mapHeight(node, windowWidth)
    val director = rememberSaveable(saver = MapDirector.saver()) { MapDirector(CoreMapMaths, MainClock) }
    val reducedMotion = Platform.reducesMotion
    var expanded by rememberSaveable { mutableStateOf(false) }
    var width by remember { mutableFloatStateOf(0f) }
    val style by produceState(MapLibreSetup.cachedStyle(dark), dark) { value = MapLibreSetup.style(context, dark) }
    val id = remember { Any() }
    DisposableEffect(id) { onDispose { LivePool.maps.leave(id) } }
    LaunchedEffect(node, streaming, reducedMotion, width, height, expanded) {
        if (width <= 0f) return@LaunchedEffect
        if (!expanded) director.resize(width, height)
        director.update(node, streaming, reducedMotion)
    }
    val select = { label: String ->
        onSelect(label)
        expanded = false
    }
    val live = rememberLiveMap(LivePool.maps.isLive(id), style, director, attached = !expanded, onPin = select)
    Box(
        modifier
            .fillMaxWidth()
            .height(height.dp)
            .clip(RoundedCornerShape(Radius.xl))
            .background(palette.mapWater)
            .onSizeChanged { width = with(density) { it.width.toDp().value } }
            .onVisibilityChanged { shown -> if (shown) LivePool.maps.show(id) else LivePool.maps.hide(id) }
            .semantics {
                contentDescription = "Map"
                isTraversalGroup = true
            },
    ) {
        if (live != null) {
            key(live) { AndroidView({ live.view }, Modifier.fillMaxSize()) }
            PinButtons(live.spots, select)
            Box(Modifier.matchParentSize().cooperative(live.view) { at -> live.pinAt(at.x, at.y)?.let(select) })
        } else {
            MapSnapshot(director, node, dark, width, height)
            PinButtons(node.pins.filter { it.visible }.map { PinSpot(it, Offset.Zero) }, select)
        }
        MapChrome(node, director, expanded = false, tilesFailed = live?.tilesFailed == true, onExpand = { expanded = true }, onFilter = onFilter)
    }
    if (expanded) {
        Dialog(
            onDismissRequest = { expanded = false },
            properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
        ) {
            val cover = rememberLiveMap(true, style, director, attached = true, onPin = select)
            Box(
                Modifier
                    .fillMaxSize()
                    .background(palette.mapWater)
                    .onSizeChanged { with(density) { director.resize(it.width.toDp().value, it.height.toDp().value) } }
                    .semantics { contentDescription = "Map" },
            ) {
                if (cover != null) {
                    key(cover) { AndroidView({ cover.view }, Modifier.fillMaxSize()) }
                    PinButtons(cover.spots, select)
                }
                Box(Modifier.fillMaxSize().safeDrawingPadding()) {
                    MapChrome(node, director, expanded = true, tilesFailed = cover?.tilesFailed == true, onExpand = { expanded = false }, onFilter = onFilter)
                }
            }
        }
    }
}

/** A live map while `enabled` and the style has loaded, following the screen's lifecycle; `attached`, the director drives it. */
@Composable
private fun rememberLiveMap(enabled: Boolean, style: String?, director: MapDirector, attached: Boolean, onPin: (String) -> Unit): LiveMap? {
    val context = LocalContext.current
    val water = LocalPalette.current.mapWater.toArgb()
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val ready = enabled && style != null
    val map = remember(ready) { if (enabled && style != null) LiveMap(context, style, director.camera, water, director) else null } ?: return null
    map.onPin = onPin
    LaunchedEffect(map, style) { style?.let(map::restyle) }
    DisposableEffect(map, lifecycle) {
        val observer = LifecycleEventObserver { _, event -> map.follow(event) }
        lifecycle.addObserver(observer)
        onDispose {
            lifecycle.removeObserver(observer)
            map.follow(Lifecycle.Event.ON_DESTROY)
        }
    }
    DisposableEffect(map, attached) {
        if (attached) director.attach(map)
        onDispose { director.detach(map) }
    }
    return map
}

/** The picture of a map that isn't live, at the director's camera; the map's water until it comes. */
@Composable
private fun MapSnapshot(director: MapDirector, node: UiKind.Map, dark: Boolean, width: Float, height: Float) {
    val context = LocalContext.current
    val camera = director.camera
    val line = director.line
    var picture by remember { mutableStateOf<ImageBitmap?>(null) }
    LaunchedEffect(camera, node.pins, line, dark, width, height) {
        if (camera == null || width <= 0f) return@LaunchedEffect
        delay(150)
        val style = MapLibreSetup.style(context, dark)
        val styled = withContext(Dispatchers.Default) { MapGeoJson.styled(style, node.pins, line) }
        MapLibreSetup.snapshot(context, styled, node.pins, camera, width, height)?.let { picture = it.asImageBitmap() }
    }
    picture?.let { Image(it, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop) }
}

/** Each pin as a button for accessibility, over where the map draws it. */
@Composable
private fun PinButtons(spots: List<PinSpot>, onSelect: (String) -> Unit) {
    val half = with(LocalDensity.current) { 17.dp.toPx() }
    for (spot in spots) {
        Box(
            Modifier
                .offset { IntOffset((spot.at.x - half).roundToInt(), (spot.at.y - half).roundToInt()) }
                .size(34.dp)
                .semantics {
                    role = Role.Button
                    contentDescription = spot.pin.label
                    selected = spot.pin.selected
                    onClick {
                        onSelect(spot.pin.label)
                        true
                    }
                },
        )
    }
}

/** The controls over a map, the info and error boxes, "Route overview" and the attribution. */
@Composable
private fun BoxScope.MapChrome(node: UiKind.Map, director: MapDirector, expanded: Boolean, tilesFailed: Boolean, onExpand: () -> Unit, onFilter: (String?) -> Unit) {
    val palette = LocalPalette.current
    var infoOpen by remember { mutableStateOf(false) }
    var choosing by remember { mutableStateOf(emptyList<Choice>()) }
    Row(Modifier.align(Alignment.TopStart).padding(12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        if (node.categories.size > 1) {
            GlassCapsule(node.filter ?: "All places", Symbol.ChevronDown, onClick = {
                choosing = listOf(Choice("All places", chosen = node.filter == null) { onFilter(null) }) +
                    node.categories.map { category -> Choice(category, chosen = node.filter == category) { onFilter(category) } }
            }, help = "Filter places")
        }
        GlassButton(Symbol.LocateFixed, "Show every place", onClick = director::recenter, size = GlassSize.Inline)
        GlassButton(Symbol.Info, "About this map", onClick = { infoOpen = !infoOpen }, size = GlassSize.Inline, state = if (infoOpen) "Expanded" else "Collapsed")
    }
    GlassCapsule(
        if (expanded) "Collapse" else "Expand",
        if (expanded) Symbol.Minimize2 else Symbol.Maximize2,
        onClick = onExpand,
        modifier = Modifier.align(Alignment.TopEnd).padding(12.dp),
        size = GlassSize.Compact,
    )
    if (infoOpen) MapNote(node.note, Modifier.align(Alignment.TopStart).padding(start = 12.dp, top = 56.dp))
    if (tilesFailed) MapNote("Map tiles are temporarily unavailable. The places are still shown.", Modifier.align(Alignment.BottomStart).padding(start = 12.dp, bottom = 28.dp))
    val lit = LocalSurface.current.color.at(Opacity.Lit)
    if (node.route == UiRouteKind.Overview) {
        Text(
            "Route overview",
            Modifier.align(Alignment.BottomStart).padding(start = 12.dp, bottom = 22.dp).background(lit, RoundedCornerShape(Radius.sm)).padding(horizontal = 7.dp, vertical = 2.dp),
            style = Theme.ui(10f).lined(16f),
            color = palette.mutedForeground,
        )
    }
    Attribution(Modifier.align(Alignment.BottomStart).background(lit).padding(start = 8.dp, end = 4.dp, top = 1.dp, bottom = 1.dp))
    Choices(choosing) { choosing = emptyList() }
}

@Composable
private fun MapNote(text: String, modifier: Modifier) {
    DropdownCard(modifier.widthIn(max = 275.dp), radius = Radius.lg) {
        Text(text, Modifier.padding(12.dp), style = Theme.smallFont.lined(18f), color = LocalPalette.current.mutedForeground)
    }
}

/** The style's attribution, each name a link that opens in the browser. */
@Composable
private fun Attribution(modifier: Modifier) {
    val text = remember {
        buildAnnotatedString {
            fun link(name: String, url: String) = withLink(LinkAnnotation.Clickable(url) { Platform.open(url) }) { append(name) }
            link("OpenFreeMap", "https://openfreemap.org")
            append(" © ")
            link("OpenMapTiles", "https://openmaptiles.org")
            append(" · Data from ")
            link("OpenStreetMap", "https://www.openstreetmap.org/copyright")
        }
    }
    Text(text, modifier, style = Theme.ui(9f).lined(12f), color = LocalPalette.current.mutedForeground, maxLines = 1)
}

/**
 * Lets one finger scroll the transcript and hands two to the map, as MapLibre GL's cooperative
 * gestures do. A press that doesn't move asks for the pin under it.
 */
private fun Modifier.cooperative(map: View, onTap: (Offset) -> Unit): Modifier = pointerInput(map) {
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false)
        val forwarding = Forwarding(map)
        var scrolled = false
        try {
            while (true) {
                val event = awaitPointerEvent()
                when {
                    forwarding.active -> forwarding.pass(event)
                    scrolled -> Unit
                    event.changes.count { it.pressed } > 1 -> forwarding.begin(event)
                    event.changes.any { (it.position - down.position).getDistance() > viewConfiguration.touchSlop } -> scrolled = true
                }
                if (forwarding.active) event.changes.forEach { it.consume() }
                if (event.changes.none { it.pressed }) break
            }
        } finally {
            forwarding.end()
        }
        if (!forwarding.used && !scrolled) onTap(down.position)
    }
}

/** Hands a gesture's touches to the map's view, from where its second finger came down. */
private class Forwarding(private val view: View) {
    var used = false
        private set
    var active = false
        private set

    fun begin(event: PointerEvent) {
        val motion = event.motionEvent ?: return
        active = true
        used = true
        val local = local(motion)
        dispatch(only(local, 1, MotionEvent.ACTION_DOWN))
        for (count in 2..local.pointerCount) {
            dispatch(only(local, count, MotionEvent.ACTION_POINTER_DOWN or ((count - 1) shl MotionEvent.ACTION_POINTER_INDEX_SHIFT)))
        }
        if (local.actionMasked == MotionEvent.ACTION_MOVE) view.dispatchTouchEvent(local)
        local.recycle()
    }

    fun pass(event: PointerEvent) {
        val motion = event.motionEvent ?: return
        val local = local(motion)
        val action = local.actionMasked
        view.dispatchTouchEvent(local)
        local.recycle()
        if (action == MotionEvent.ACTION_UP || action == MotionEvent.ACTION_CANCEL) active = false
    }

    /** Cancels what the map still holds when the gesture ends without its last finger up. */
    fun end() {
        if (!active) return
        active = false
        val now = SystemClock.uptimeMillis()
        dispatch(MotionEvent.obtain(now, now, MotionEvent.ACTION_CANCEL, 0f, 0f, 0))
    }

    private fun dispatch(event: MotionEvent) {
        view.dispatchTouchEvent(event)
        event.recycle()
    }

    /** The event in the view's own coordinates. */
    private fun local(motion: MotionEvent): MotionEvent {
        val at = IntArray(2)
        view.getLocationOnScreen(at)
        return MotionEvent.obtain(motion).apply { offsetLocation(motion.rawX - motion.x - at[0], motion.rawY - motion.y - at[1]) }
    }

    /** The event's first `count` pointers, as `action`. */
    private fun only(motion: MotionEvent, count: Int, action: Int): MotionEvent {
        val properties = Array(count) { index -> MotionEvent.PointerProperties().also { motion.getPointerProperties(index, it) } }
        val coordinates = Array(count) { index -> MotionEvent.PointerCoords().also { motion.getPointerCoords(index, it) } }
        return MotionEvent.obtain(
            motion.downTime, motion.eventTime, action, count, properties, coordinates,
            motion.metaState, motion.buttonState, motion.xPrecision, motion.yPrecision, motion.deviceId, 0, motion.source, motion.flags,
        )
    }
}
