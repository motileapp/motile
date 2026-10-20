package app.motile.transcript.map

import app.motile.core.UiMapPin
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** What the map's style's `pins` and `route` sources are filled with. */
object MapGeoJson {
    /** A Point (`[lng, lat]`) for each pin in the category shown; `icon`, the image's id, only for an emoji pin. */
    fun pins(pins: List<UiMapPin>): JsonObject = collection(
        pins.filter { it.visible }.map { pin ->
            feature(
                geometry = buildJsonObject {
                    put("type", "Point")
                    put("coordinates", position(pin.lat, pin.lng))
                },
                properties = buildJsonObject {
                    put("key", pin.key)
                    put("label", pin.label)
                    put("number", pin.number.toString())
                    put("selected", pin.selected)
                    pin.icon?.let { put("icon", it) }
                },
            )
        },
    )

    /** A LineString of the points (`[lat, lng]`), or nothing for fewer than two. */
    fun line(points: List<LatLngPoint>): JsonObject {
        if (points.size < 2) return collection(emptyList())
        val coordinates = buildJsonArray { points.forEach { add(position(it[0], it[1])) } }
        return collection(
            listOf(
                feature(
                    geometry = buildJsonObject {
                        put("type", "LineString")
                        put("coordinates", coordinates)
                    },
                    properties = JsonObject(emptyMap()),
                ),
            ),
        )
    }

    /** The emoji the pins shown draw, each an image the style needs before the source names it. */
    fun emoji(pins: List<UiMapPin>): Set<String> = pins.filter { it.visible }.mapNotNull { it.icon }.toSet()

    /** The style with its `pins` and `route` sources already filled, for a snapshot, which has no sources to set. */
    fun styled(style: String, pins: List<UiMapPin>, line: List<LatLngPoint>): String {
        val root = runCatching { Json.parseToJsonElement(style) as? JsonObject }.getOrNull() ?: return style
        val sources = root["sources"] as? JsonObject ?: return style
        val filled = sources.toMutableMap()
        filled["pins"] = withData(sources["pins"], pins(pins))
        filled["route"] = withData(sources["route"], line(line))
        return JsonObject(root + ("sources" to JsonObject(filled))).toString()
    }

    private fun withData(source: JsonElement?, data: JsonObject): JsonObject {
        val fields = (source as? JsonObject)?.toMutableMap() ?: mutableMapOf<String, JsonElement>("type" to JsonPrimitive("geojson"))
        fields["data"] = data
        return JsonObject(fields)
    }

    private fun position(lat: Double, lng: Double): JsonArray = buildJsonArray {
        add(JsonPrimitive(lng))
        add(JsonPrimitive(lat))
    }

    private fun feature(geometry: JsonObject, properties: JsonObject) = buildJsonObject {
        put("type", "Feature")
        put("geometry", geometry)
        put("properties", properties)
    }

    private fun collection(features: List<JsonObject>) = buildJsonObject {
        put("type", "FeatureCollection")
        put("features", JsonArray(features))
    }
}
