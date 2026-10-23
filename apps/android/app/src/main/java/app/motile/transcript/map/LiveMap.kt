package app.motile.transcript.map

import android.content.Context
import android.graphics.PointF
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.lifecycle.Lifecycle
import app.motile.core.UiMapPin
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapLibreMapOptions
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.android.tile.TileOperation

/** A pin where the live map draws it, in pixels from the map's top left, for accessibility. */
data class PinSpot(val pin: UiMapPin, val at: Offset)

/**
 * A MapLibre map view driven by a `MapDirector`: what the director asks before the style has
 * loaded waits for it, and a new style (the other appearance) gets the sources and images again.
 */
class LiveMap(context: Context, style: String, camera: MapCamera?, water: Int, private val director: MapDirector) : MapEngine {
    val view: MapView
    private var map: MapLibreMap? = null
    private var loaded: Style? = null
    private var styleJson = style
    private var pins: List<UiMapPin> = emptyList()
    private var line: List<LatLngPoint> = emptyList()
    private var pendingCamera: MapCamera? = null

    /** Tiles failed to load, until one loads again. */
    var tilesFailed by mutableStateOf(false)
        private set

    var spots by mutableStateOf<List<PinSpot>>(emptyList())
        private set

    /** Called with a pressed pin's label. */
    var onPin: (String) -> Unit = {}

    private var started = false
    private var resumed = false
    private var destroyed = false

    init {
        MapLibreSetup.start(context)
        val start = camera ?: MapCamera(20.0, 0.0, 1.0)
        val options = MapLibreMapOptions.createFromAttributes(context)
            .textureMode(true)
            .foregroundLoadColor(water)
            .camera(position(start))
            .attributionEnabled(false)
            .logoEnabled(false)
            .compassEnabled(false)
            .rotateGesturesEnabled(false)
            .tiltGesturesEnabled(false)
            .minZoomPreference(0.0)
        view = MapView(context, options)
        view.onCreate(null)
        view.addOnDidFailLoadingMapListener { tilesFailed = true }
        view.addOnTileActionListener { operation, _, _, _, _, _, source ->
            if (source != "openmaptiles") return@addOnTileActionListener
            when (operation) {
                TileOperation.Error -> if (!tilesFailed) tilesFailed = true
                TileOperation.EndParse -> if (tilesFailed) tilesFailed = false
                else -> Unit
            }
        }
        view.getMapAsync { ready ->
            map = ready
            ready.uiSettings.isRotateGesturesEnabled = false
            ready.uiSettings.isTiltGesturesEnabled = false
            ready.addOnCameraMoveStartedListener { reason ->
                if (reason == MapLibreMap.OnCameraMoveStartedListener.REASON_API_GESTURE) director.movedByUser(this)
            }
            ready.addOnMapClickListener { point ->
                val at = ready.projection.toScreenLocation(point)
                val label = pinAt(at.x, at.y) ?: return@addOnMapClickListener false
                onPin(label)
                true
            }
            ready.addOnCameraIdleListener {
                director.settled(this, cameraOf(ready.cameraPosition) ?: return@addOnCameraIdleListener)
                locate()
            }
            pendingCamera?.let { ready.moveCamera(CameraUpdateFactory.newCameraPosition(position(it))) }
            pendingCamera = null
            load()
        }
    }

    /** Follows the screen's lifecycle; destroyed, the view is done with. */
    fun follow(event: Lifecycle.Event) {
        if (destroyed) return
        val target = event.targetState
        if (resumed && target < Lifecycle.State.RESUMED) {
            view.onPause()
            resumed = false
        }
        if (started && target < Lifecycle.State.STARTED) {
            view.onStop()
            started = false
        }
        if (!started && target >= Lifecycle.State.STARTED) {
            view.onStart()
            started = true
        }
        if (!resumed && target >= Lifecycle.State.RESUMED) {
            view.onResume()
            resumed = true
        }
        if (target == Lifecycle.State.DESTROYED) {
            destroyed = true
            map = null
            loaded = null
            view.onDestroy()
        }
    }

    /** Swaps the style for the other appearance's, keeping the camera, the sources and the images. */
    fun restyle(style: String) {
        if (style == styleJson) return
        styleJson = style
        loaded = null
        load()
    }

    /** The label of the pin drawn at the point, in pixels from the map's top left. */
    private fun pinAt(x: Float, y: Float): String? {
        val map = map ?: return null
        if (loaded == null) return null
        return map.queryRenderedFeatures(PointF(x, y), "pin", "pin-mark").firstNotNullOfOrNull { feature ->
            feature.takeIf { it.hasProperty("label") }?.getStringProperty("label")
        }
    }

    override fun jump(camera: MapCamera) {
        val map = map ?: run { pendingCamera = camera; return }
        map.cancelTransitions()
        map.moveCamera(CameraUpdateFactory.newCameraPosition(position(camera)))
    }

    override fun fly(camera: MapCamera, ms: Int) {
        val map = map ?: run { pendingCamera = camera; return }
        map.animateCamera(CameraUpdateFactory.newCameraPosition(position(camera)), ms)
    }

    override fun setPins(pins: List<UiMapPin>) {
        if (pins == this.pins) return
        this.pins = pins
        val style = loaded ?: return
        fillPins(style)
        locate()
    }

    override fun setLine(line: List<LatLngPoint>) {
        this.line = line
        val style = loaded ?: return
        style.getSourceAs<GeoJsonSource>("route")?.setGeoJson(MapGeoJson.line(line).toString())
    }

    private fun load() {
        val map = map ?: return
        val requested = styleJson
        map.setStyle(Style.Builder().fromJson(requested)) { style ->
            if (requested != styleJson) return@setStyle
            loaded = style
            fillPins(style)
            setLine(line)
            locate()
        }
    }

    private fun fillPins(style: Style) {
        for (emoji in MapGeoJson.emoji(pins)) {
            if (style.getImage(emoji) == null) style.addImage(emoji, MapLibreSetup.emojiImage(view.context, emoji))
        }
        style.getSourceAs<GeoJsonSource>("pins")?.setGeoJson(MapGeoJson.pins(pins).toString())
    }

    private fun locate() {
        val map = map ?: return
        spots = pins.filter { it.visible }.map { pin ->
            val point = map.projection.toScreenLocation(LatLng(pin.lat, pin.lng))
            PinSpot(pin, Offset(point.x, point.y))
        }
    }

    private fun position(camera: MapCamera): CameraPosition =
        CameraPosition.Builder().target(LatLng(camera.lat, camera.lng)).zoom(camera.zoom).bearing(0.0).tilt(0.0).build()

    private fun cameraOf(position: CameraPosition): MapCamera? {
        val target = position.target ?: return null
        return MapCamera(target.latitude, target.longitude, position.zoom)
    }
}
