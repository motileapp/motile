package app.motile.transcript.map

import app.motile.core.UiKind
import app.motile.core.UiMapPin
import app.motile.core.UiRadius
import app.motile.core.UiRadiusKind
import app.motile.core.UiRouteKind
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MapGeoJsonTest {
    private val pins = listOf(
        UiMapPin("Ferry Building", "Ferry Building", 37.7955, -122.3937, "Food", "☕", 1, true, true),
        UiMapPin("Coit Tower", "Coit Tower", 37.8024, -122.4058, "Views", null, 2, false, true),
        UiMapPin("Hidden", "Hidden", 1.0, 2.0, "Views", "🌉", 3, false, false),
    )

    @Test
    fun pinsArePointsOfTheVisibleOnesWithAnIconOnlyForAnEmoji() {
        val json = MapGeoJson.pins(pins)
        assertEquals(
            Json.parseToJsonElement(
                """
                {"type":"FeatureCollection","features":[
                  {"type":"Feature","geometry":{"type":"Point","coordinates":[-122.3937,37.7955]},
                   "properties":{"key":"Ferry Building","label":"Ferry Building","number":"1","selected":true,"icon":"☕"}},
                  {"type":"Feature","geometry":{"type":"Point","coordinates":[-122.4058,37.8024]},
                   "properties":{"key":"Coit Tower","label":"Coit Tower","number":"2","selected":false}}
                ]}
                """,
            ),
            json,
        )
        assertEquals(setOf("☕"), MapGeoJson.emoji(pins))
    }

    @Test
    fun theRouteIsALineStringOfLngLatOrNothingUnderTwoPoints() {
        val line = MapGeoJson.line(listOf(listOf(1.0, 2.0), listOf(3.0, 4.0)))
        val feature = line["features"]!!.jsonArray.single().jsonObject
        assertEquals(Json.parseToJsonElement("""{"type":"LineString","coordinates":[[2.0,1.0],[4.0,3.0]]}"""), feature["geometry"])
        assertTrue(MapGeoJson.line(listOf(listOf(1.0, 2.0)))["features"]!!.jsonArray.isEmpty())
    }

    @Test
    fun aSnapshotsStyleCarriesThePinsAndTheRouteInItsSources() {
        val style = """{"version":8,"sources":{"openmaptiles":{"type":"vector"},"route":{"type":"geojson","data":{}},"pins":{"type":"geojson","data":{}}},"layers":[]}"""
        val styled = Json.parseToJsonElement(MapGeoJson.styled(style, pins, listOf(listOf(1.0, 2.0), listOf(3.0, 4.0)))).jsonObject
        val sources = styled["sources"]!!.jsonObject
        assertEquals(MapGeoJson.pins(pins), sources["pins"]!!.jsonObject["data"])
        assertEquals("geojson", (sources["route"] as JsonObject)["type"].toString().trim('"'))
        assertEquals(1, sources["route"]!!.jsonObject["data"]!!.jsonObject["features"]!!.jsonArray.size)
        assertEquals(Json.parseToJsonElement("""{"type":"vector"}"""), sources["openmaptiles"])
        assertFalse(styled["layers"] == null)
    }

    @Test
    fun aMapIsShorterOnANarrowWindow() {
        val node = UiKind.Map(emptyList(), UiRouteKind.Off, emptyList(), null, 400f, emptyList(), null, "", UiRadius(UiRadiusKind.Xl, 0f))
        assertEquals(400f, mapHeight(node, 1200f))
        assertEquals(319.8f, mapHeight(node, 390f), 0.01f)
        assertEquals(280f, mapHeight(node, 200f))
        assertEquals(200f, mapHeight(node.copy(height = 200f), 1200f))
    }
}
