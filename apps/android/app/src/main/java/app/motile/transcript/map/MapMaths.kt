package app.motile.transcript.map

import app.motile.core.Native
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** A point as core's map maths take it: `[lat, lng]`. */
typealias LatLngPoint = List<Double>

/** Where a map looks, in MapLibre's zoom levels, as core's `map::Camera`. */
data class MapCamera(val lat: Double, val lng: Double, val zoom: Double)

/** How `frame` fits the points: the first pin while streaming, every pin once it ends, or the recenter button's. */
enum class Framing(val raw: String) { First("first"), Fit("fit"), Recenter("recenter") }

/** Core's camera maths for the map (`crates/core/src/render/openui/map.rs`). */
interface MapMaths {
    fun frame(points: List<LatLngPoint>, width: Float, height: Float, framing: Framing): MapCamera?

    /** The line cut at the eased share `t` (0–1, linear) of its length. */
    fun reveal(line: List<LatLngPoint>, t: Double): List<LatLngPoint>
}

/** The maths through the core's JNI function, synchronous on any thread. */
object CoreMapMaths : MapMaths {
    override fun frame(points: List<LatLngPoint>, width: Float, height: Float, framing: Framing): MapCamera? {
        val answer = ask(
            buildJsonObject {
                put("op", "frame")
                put("points", line(points))
                put("width", width)
                put("height", height)
                put("framing", framing.raw)
            },
        ) as? JsonObject ?: return null
        val lat = answer["lat"]?.jsonPrimitive?.doubleOrNull ?: return null
        val lng = answer["lng"]?.jsonPrimitive?.doubleOrNull ?: return null
        val zoom = answer["zoom"]?.jsonPrimitive?.doubleOrNull ?: return null
        return MapCamera(lat, lng, zoom)
    }

    override fun reveal(line: List<LatLngPoint>, t: Double): List<LatLngPoint> {
        val answer = ask(
            buildJsonObject {
                put("op", "reveal")
                put("line", line(line))
                put("t", t)
            },
        ) as? JsonArray ?: return emptyList()
        return answer.mapNotNull { point -> (point as? JsonArray)?.mapNotNull { it.jsonPrimitive.doubleOrNull }?.takeIf { it.size == 2 } }
    }

    private fun line(points: List<LatLngPoint>): JsonArray = buildJsonArray {
        for (point in points) add(buildJsonArray { point.forEach { add(JsonPrimitive(it)) } })
    }

    private fun ask(input: JsonObject): JsonElement? =
        runCatching { Json.parseToJsonElement(Native.uiMap(input.toString())) }.getOrNull()
}
